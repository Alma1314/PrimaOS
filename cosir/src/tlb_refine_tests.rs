//! tlb_refine host MODEL-TEST：每 Task TLB + 非强制刷新 + 显式失效。spec 2026-08-15-tlb-refine-design.md。

use crate::memseg_refine::{RefineState, map_verified, memseg_create, spawn_task, unmap_verified};
use crate::state::{ACCESS_R, ACCESS_W, ACCESS_X, State, Task, TaskStatus};
use crate::tlb_refine::{
    AccessKind, CachedXlat, TlbError, TlbState, access_allowed, flush_all, has_tlb, invalidate_va,
    remove_task, translate,
};

const T1: u32 = 1;
const T2: u32 = 2;
const VA_A: u64 = 0x4000_0000;
const VA_B: u64 = 0x5000_0000;
const ENTRY: u64 = 0x6000_0000;

// 两个 Task，各自把自己独立的 data 段映射到唯一地址，避免共享（每 Task 独立）。
// universe 含足够帧：最重用例（stale_frame_survives_without_flush）需 5 帧。
fn two_task_state() -> (RefineState, State, TlbState) {
    let mut memory = RefineState::new(&(0u64..16).map(|i| 0x1000 + i * 0x1000).collect::<Vec<_>>())
        .expect("universe");
    spawn_task(&mut memory, T1, ENTRY).expect("spawn");
    spawn_task(&mut memory, T2, ENTRY).expect("spawn");
    let t1seg = memseg_create(&mut memory, 1, ACCESS_R | ACCESS_W).expect("seg");
    let t2seg = memseg_create(&mut memory, 1, ACCESS_R | ACCESS_W).expect("seg");
    map_verified(&mut memory, t1seg, T1, 0, VA_A, 1, ACCESS_R).expect("t1 A R");
    map_verified(&mut memory, t1seg, T1, 0, VA_B, 1, ACCESS_R).expect("t1 B R");
    map_verified(&mut memory, t2seg, T2, 0, VA_A, 1, ACCESS_R).expect("t2 A R");
    let mut ops = State::default();
    for id in [T1, T2] {
        ops.tasks.insert(
            id,
            Task {
                id,
                status: TaskStatus::Ready,
                vspace: Vec::new(),
                kernel_resume_frame: None,
            },
        );
    }
    (memory, ops, TlbState::new())
}

fn live_task_fixture() -> (RefineState, State) {
    let mut memory = RefineState::new(&[0x1000]).expect("universe");
    spawn_task(&mut memory, T1, ENTRY).expect("spawn");
    let mut ops = State::default();
    ops.tasks.insert(
        T1,
        Task {
            id: T1,
            status: TaskStatus::Ready,
            vspace: Vec::new(),
            kernel_resume_frame: None,
        },
    );
    (memory, ops)
}

// T1a — 首次访问未命中，走页表并填充
#[test]
fn miss_fills_and_permits() {
    let (pf, ops, mut tlb) = two_task_state();
    let xl = translate(&pf, &ops.tasks, &mut tlb, T1, VA_A).expect("walk fills");
    assert!(access_allowed(&xl, AccessKind::Read), "只读访问应被允许");
    assert!(
        !access_allowed(&xl, AccessKind::Write),
        "只读 map 不应许可写访问"
    );
    assert!(has_tlb(&tlb, T1, VA_A), "未命中后应填充 TLB");
}

// T1b — 换映射（改可写）后未失效 → 命中陈旧只读 TLB，写仍被驳回
#[test]
fn remap_without_flush_hits_stale() {
    let (mut pf, ops, mut tlb) = two_task_state();
    // 填充 TLB：只读
    let _ = translate(&pf, &ops.tasks, &mut tlb, T1, VA_A).expect("fill");
    // 换映射为可写（unmap + map）
    unmap_verified(&mut pf, T1, VA_A, 1).expect("unmap");
    let t1seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    map_verified(&mut pf, t1seg2, T1, 0, VA_A, 1, ACCESS_W).expect("remap W");
    // 未 flush：仍命中陈旧只读翻译
    let xl = translate(&pf, &ops.tasks, &mut tlb, T1, VA_A).expect("stale hit");
    assert!(
        !access_allowed(&xl, AccessKind::Write),
        "未 flush 应命中陈旧只读 TLB"
    );
}

// T1c — 换映射后 invalidate_va → 走新可写 PTE；失效作用域只限指定地址（T2）
#[test]
fn invalidation_then_walk_permits_write() {
    let (mut pf, ops, mut tlb) = two_task_state();
    let _ = translate(&pf, &ops.tasks, &mut tlb, T1, VA_A).expect("fill");
    let _ = translate(&pf, &ops.tasks, &mut tlb, T1, VA_B).expect("fill B");
    unmap_verified(&mut pf, T1, VA_A, 1).expect("unmap");
    let t1seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    map_verified(&mut pf, t1seg2, T1, 0, VA_A, 1, ACCESS_W).expect("remap W");
    invalidate_va(&ops.tasks, &mut tlb, T1, VA_A).expect("invalidate");
    assert!(!has_tlb(&tlb, T1, VA_A), "失效后不应命中");
    let xl = translate(&pf, &ops.tasks, &mut tlb, T1, VA_A).expect("walk new PTE");
    assert!(
        access_allowed(&xl, AccessKind::Write),
        "失效后应走新可写 PTE"
    );
    // 同 Task 其他地址 TLB 保留（T2 作用域）
    assert!(has_tlb(&tlb, T1, VA_B), "invalidate_va 只清指定地址");
}

// T3 — Task 隔离：A 失效不影响 B 的 TLB
#[test]
fn invalidation_is_per_task() {
    let (pf, ops, mut tlb) = two_task_state();
    let _ = translate(&pf, &ops.tasks, &mut tlb, T1, VA_A).expect("fill t1");
    let _ = translate(&pf, &ops.tasks, &mut tlb, T2, VA_A).expect("fill t2");
    invalidate_va(&ops.tasks, &mut tlb, T1, VA_A).expect("invalidate t1");
    assert!(!has_tlb(&tlb, T1, VA_A), "T1 已失效");
    assert!(has_tlb(&tlb, T2, VA_A), "T2 不受影响");
}

// R1 — 未映射地址返回 NotMapped，且失败不改变完整 TLB 状态。
#[test]
fn not_mapped_preserves_full_tlb_state_with_unrelated_cache() {
    let (pf, ops, mut tlb) = two_task_state();
    let _ = translate(&pf, &ops.tasks, &mut tlb, T2, VA_A).expect("fill unrelated cache");
    let before = tlb.clone();

    let err = translate(&pf, &ops.tasks, &mut tlb, T1, ENTRY).expect_err("ENTRY 未映射");

    assert_eq!(err, TlbError::NotMapped);
    assert_eq!(tlb, before, "NotMapped 不能改变任何 Task 的 TLB 状态");
}

// R2 — 换映射改变物理帧后，未失效的 TLB 仍返回旧 frame_phys。
#[test]
fn stale_frame_survives_without_flush() {
    let (mut pf, ops, mut tlb) = two_task_state();
    // 先单独 map 一块可写地址 Y（本测试专用新地址）避免干扰
    let yseg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    map_verified(&mut pf, yseg, T1, 0, 0x7000_0000, 1, ACCESS_R | ACCESS_W).expect("map Y RW");
    let xl0 = translate(&pf, &ops.tasks, &mut tlb, T1, 0x7000_0000).expect("fill Y");
    let old = xl0.frame_phys;
    // 换映射到另一新段（新物理帧）
    unmap_verified(&mut pf, T1, 0x7000_0000, 1).expect("unmap Y");
    let yseg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg2");
    map_verified(&mut pf, yseg2, T1, 0, 0x7000_0000, 1, ACCESS_R | ACCESS_W).expect("remap Y");
    // 未失效：仍是旧帧
    let xl1 = translate(&pf, &ops.tasks, &mut tlb, T1, 0x7000_0000).expect("stale hit");
    assert_eq!(xl1.frame_phys, old, "未 flush 应命中旧物理帧");
    // 失效后走新帧
    invalidate_va(&ops.tasks, &mut tlb, T1, 0x7000_0000).expect("invalidate");
    let xl2 = translate(&pf, &ops.tasks, &mut tlb, T1, 0x7000_0000).expect("walk new");
    assert_ne!(xl2.frame_phys, old, "失效后应观察到新物理帧");
}

#[test]
fn access_kind_matrix_enforces_each_bit_independently() {
    let cases = [
        (ACCESS_R, [true, false, false]),
        (ACCESS_W, [false, true, false]),
        (ACCESS_X, [false, false, true]),
        (ACCESS_R | ACCESS_X, [true, false, true]),
    ];
    for (access, expected) in cases {
        let xl = CachedXlat {
            frame_phys: 0x1000,
            access,
        };
        assert_eq!(access_allowed(&xl, AccessKind::Read), expected[0]);
        assert_eq!(access_allowed(&xl, AccessKind::Write), expected[1]);
        assert_eq!(access_allowed(&xl, AccessKind::Execute), expected[2]);
    }
}

#[test]
fn invalidate_live_task_without_cache_is_idempotent() {
    let (memory, ops) = live_task_fixture();
    let mut tlb = TlbState::new();
    assert_eq!(invalidate_va(&ops.tasks, &mut tlb, T1, VA_A), Ok(()));
    assert!(!has_tlb(&tlb, T1, VA_A));
    assert!(memory.tasks.contains_key(&T1));
}

#[test]
fn invalidate_rejects_unaligned_address_atomically() {
    let (_, ops) = live_task_fixture();
    let mut tlb = TlbState::new();
    let before = tlb.clone();
    assert_eq!(
        invalidate_va(&ops.tasks, &mut tlb, T1, VA_A + 1),
        Err(TlbError::UnalignedAddress)
    );
    assert_eq!(tlb, before);
}

#[test]
fn invalidate_rejects_noncanonical_address_atomically() {
    let (_, ops) = live_task_fixture();
    let mut tlb = TlbState::new();
    let before = tlb.clone();
    assert_eq!(
        invalidate_va(&ops.tasks, &mut tlb, T1, 0x0000_8000_0000_0000),
        Err(TlbError::NonCanonicalAddress)
    );
    assert_eq!(tlb, before);
}

#[test]
fn flush_live_task_without_cache_is_idempotent() {
    let (_, ops) = live_task_fixture();
    let mut tlb = TlbState::new();
    assert_eq!(flush_all(&ops.tasks, &mut tlb, T1), Ok(()));
    assert!(!has_tlb(&tlb, T1, VA_A));
}

#[test]
fn missing_task_errors_even_without_cache() {
    let (_, ops) = live_task_fixture();
    let mut tlb = TlbState::new();
    assert_eq!(
        invalidate_va(&ops.tasks, &mut tlb, 99, VA_A),
        Err(TlbError::NoSuchTask)
    );
    assert_eq!(
        flush_all(&ops.tasks, &mut tlb, 99),
        Err(TlbError::NoSuchTask)
    );
}

#[test]
fn translate_rejects_task_missing_from_state_atomically() {
    let (memory, mut ops, mut tlb) = two_task_state();
    let _ = translate(&memory, &ops.tasks, &mut tlb, T2, VA_A).expect("fill unrelated cache");
    ops.tasks.remove(&T1);
    let before = tlb.clone();

    assert_eq!(
        translate(&memory, &ops.tasks, &mut tlb, T1, VA_A),
        Err(TlbError::NoSuchTask)
    );
    assert_eq!(tlb, before);
}

#[test]
fn translate_rejects_task_missing_from_refine_state_atomically() {
    let (mut memory, ops, mut tlb) = two_task_state();
    let _ = translate(&memory, &ops.tasks, &mut tlb, T2, VA_A).expect("fill unrelated cache");
    memory.tasks.remove(&T1);
    let before = tlb.clone();

    assert_eq!(
        translate(&memory, &ops.tasks, &mut tlb, T1, VA_A),
        Err(TlbError::NoSuchTask)
    );
    assert_eq!(tlb, before);
}

#[test]
fn remove_task_discards_all_cached_translations_idempotently() {
    let (memory, ops, mut tlb) = two_task_state();
    let _ = translate(&memory, &ops.tasks, &mut tlb, T1, VA_A).expect("fill A");
    let _ = translate(&memory, &ops.tasks, &mut tlb, T1, VA_B).expect("fill B");
    let _ = translate(&memory, &ops.tasks, &mut tlb, T2, VA_A).expect("fill T2");
    remove_task(&mut tlb, T1);
    assert!(!has_tlb(&tlb, T1, VA_A));
    assert!(!has_tlb(&tlb, T1, VA_B));
    assert!(has_tlb(&tlb, T2, VA_A));
    remove_task(&mut tlb, T1);
    assert!(has_tlb(&tlb, T2, VA_A));
}

// 错误面：NoSuchTask（权威 Task 表为空时触发）
#[test]
fn missing_task_errors() {
    let ops = State::default();
    let mut tlb = TlbState::new();
    assert_eq!(
        flush_all(&ops.tasks, &mut tlb, 99).expect_err("no such task"),
        TlbError::NoSuchTask
    );
    assert_eq!(
        invalidate_va(&ops.tasks, &mut tlb, 99, VA_A).expect_err("no such task"),
        TlbError::NoSuchTask
    );
}
