//! tlb_refine host MODEL-TEST：每 Task TLB + 非强制刷新 + 显式失效。spec 2026-08-15-tlb-refine-design.md。

use crate::memseg_refine::{
    RefineState, SEG_MAP_R, SEG_MAP_W, memseg_create, memseg_map, memseg_unmap, spawn_task,
};
use crate::state::{ACCESS_R, ACCESS_W};
use crate::tlb_refine::{
    TlbError, TlbState, access_allowed, flush_all, has_tlb, invalidate_va, translate,
};

const T1: u32 = 1;
const T2: u32 = 2;
const VA_A: u64 = 0x4000_0000;
const VA_B: u64 = 0x5000_0000;
const ENTRY: u64 = 0x6000_0000;

// 两个 Task，各自把自己独立的 data 段映射到唯一地址，避免共享（每 Task 独立）。
// universe 含足够帧：最重用例（stale_frame_survives_without_flush）需 5 帧。
fn two_task_state() -> (RefineState, TlbState) {
    let mut pf = RefineState::new(&[
        0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000, 0x8000,
    ])
    .expect("universe");
    spawn_task(&mut pf, T1, ENTRY).expect("spawn");
    spawn_task(&mut pf, T2, ENTRY).expect("spawn");
    let t1seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let t2seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(&mut pf, rights, true, t1seg, T1, 0, VA_A, 1, ACCESS_R).expect("t1 A R");
    memseg_map(&mut pf, rights, true, t1seg, T1, 0, VA_B, 1, ACCESS_R).expect("t1 B R");
    memseg_map(&mut pf, rights, true, t2seg, T2, 0, VA_A, 1, ACCESS_R).expect("t2 A R");
    (pf, TlbState::new())
}

// T1a — 首次访问未命中，走页表并填充
#[test]
fn miss_fills_and_permits() {
    let (pf, mut tlb) = two_task_state();
    let xl = translate(&pf, &mut tlb, T1, VA_A).expect("walk fills");
    assert!(access_allowed(&xl, false), "只读访问应被允许");
    assert!(!access_allowed(&xl, true), "只读 map 不应许可写访问");
    assert!(has_tlb(&tlb, T1, VA_A), "未命中后应填充 TLB");
}

// T1b — 换映射（改可写）后未失效 → 命中陈旧只读 TLB，写仍被驳回
#[test]
fn remap_without_flush_hits_stale() {
    let (mut pf, mut tlb) = two_task_state();
    // 填充 TLB：只读
    let _ = translate(&pf, &mut tlb, T1, VA_A).expect("fill");
    // 换映射为可写（unmap + map）
    memseg_unmap(&mut pf, true, T1, VA_A, 1).expect("unmap");
    let t1seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(&mut pf, rights, true, t1seg2, T1, 0, VA_A, 1, ACCESS_W).expect("remap W");
    // 未 flush：仍命中陈旧只读翻译
    let xl = translate(&pf, &mut tlb, T1, VA_A).expect("stale hit");
    assert!(!access_allowed(&xl, true), "未 flush 应命中陈旧只读 TLB");
}

// T1c — 换映射后 invalidate_va → 走新可写 PTE；失效作用域只限指定地址（T2）
#[test]
fn invalidation_then_walk_permits_write() {
    let (mut pf, mut tlb) = two_task_state();
    let _ = translate(&pf, &mut tlb, T1, VA_A).expect("fill");
    let _ = translate(&pf, &mut tlb, T1, VA_B).expect("fill B");
    memseg_unmap(&mut pf, true, T1, VA_A, 1).expect("unmap");
    let t1seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(&mut pf, rights, true, t1seg2, T1, 0, VA_A, 1, ACCESS_W).expect("remap W");
    invalidate_va(&mut tlb, T1, VA_A).expect("invalidate");
    assert!(!has_tlb(&tlb, T1, VA_A), "失效后不应命中");
    let xl = translate(&pf, &mut tlb, T1, VA_A).expect("walk new PTE");
    assert!(access_allowed(&xl, true), "失效后应走新可写 PTE");
    // 同 Task 其他地址 TLB 保留（T2 作用域）
    assert!(has_tlb(&tlb, T1, VA_B), "invalidate_va 只清指定地址");
}

// T3 — Task 隔离：A 失效不影响 B 的 TLB
#[test]
fn invalidation_is_per_task() {
    let (pf, mut tlb) = two_task_state();
    let _ = translate(&pf, &mut tlb, T1, VA_A).expect("fill t1");
    let _ = translate(&pf, &mut tlb, T2, VA_A).expect("fill t2");
    invalidate_va(&mut tlb, T1, VA_A).expect("invalidate t1");
    assert!(!has_tlb(&tlb, T1, VA_A), "T1 已失效");
    assert!(has_tlb(&tlb, T2, VA_A), "T2 不受影响");
}

// R1 — 未映射地址返回 NotMapped 且不填充
#[test]
fn not_mapped_does_not_fill() {
    let (pf, mut tlb) = two_task_state();
    let err = translate(&pf, &mut tlb, T1, ENTRY).expect_err("ENTRY 未映射");
    assert_eq!(err, TlbError::NotMapped);
    assert!(!has_tlb(&tlb, T1, ENTRY), "未映射不应填充 TLB");
}

// R2 — 换映射改变物理帧后，未失效的 TLB 仍返回旧 frame_phys。
#[test]
fn stale_frame_survives_without_flush() {
    let (mut pf, mut tlb) = two_task_state();
    // 先单独 map 一块可写地址 Y（本测试专用新地址）避免干扰
    let yseg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
    let rights = SEG_MAP_R | SEG_MAP_W;
    memseg_map(
        &mut pf,
        rights,
        true,
        yseg,
        T1,
        0,
        0x7000_0000,
        1,
        ACCESS_R | ACCESS_W,
    )
    .expect("map Y RW");
    let xl0 = translate(&pf, &mut tlb, T1, 0x7000_0000).expect("fill Y");
    let old = xl0.frame_phys;
    // 换映射到另一新段（新物理帧）
    memseg_unmap(&mut pf, true, T1, 0x7000_0000, 1).expect("unmap Y");
    let yseg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg2");
    memseg_map(
        &mut pf,
        rights,
        true,
        yseg2,
        T1,
        0,
        0x7000_0000,
        1,
        ACCESS_R | ACCESS_W,
    )
    .expect("remap Y");
    // 未失效：仍是旧帧
    let xl1 = translate(&pf, &mut tlb, T1, 0x7000_0000).expect("stale hit");
    assert_eq!(xl1.frame_phys, old, "未 flush 应命中旧物理帧");
    // 失效后走新帧
    invalidate_va(&mut tlb, T1, 0x7000_0000).expect("invalidate");
    let xl2 = translate(&pf, &mut tlb, T1, 0x7000_0000).expect("walk new");
    assert_ne!(xl2.frame_phys, old, "失效后应观察到新物理帧");
}

// 错误面：NoSuchTask（空 TlbState 即触发）
#[test]
fn missing_task_errors() {
    let mut tlb = TlbState::new();
    assert_eq!(
        flush_all(&mut tlb, 99).expect_err("no such task"),
        TlbError::NoSuchTask
    );
    assert_eq!(
        invalidate_va(&mut tlb, 99, VA_A).expect_err("no such task"),
        TlbError::NoSuchTask
    );
    // 空状态上 flush_all 同一不存在的 task 仍为 NoSuchTask（幂等错误，非重复清空）
    assert_eq!(
        flush_all(&mut tlb, 99).expect_err("still no such task"),
        TlbError::NoSuchTask
    );
}
