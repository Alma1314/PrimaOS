use crate::cow_refine::{
    CHILD, CODE_SEG, COW_VA, CowProbe, FRAME_CAP, PARENT, WriteOutcome, base_exc,
    cow_state_consistent, new_refine, probe_write, resume_frame, write_fault,
};
use crate::memseg_refine::{
    SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_alpha, memseg_create, memseg_map, memseg_unmap,
};
use crate::ops;
use crate::state::{ACCESS_R, ACCESS_W, ExceptionKind, TaskStatus};

#[test]
fn cross_model_state_is_deeply_comparable() {
    let pf = new_refine();
    let exc = base_exc();
    assert_eq!(pf, pf.clone());
    assert_eq!(exc, exc.clone());
    assert!(crate::memseg_refine::memseg_alpha(&pf), "fixture alpha");
}

#[test]
fn parent_and_child_share_read_only_at_child() {
    let pf = new_refine();
    let child_ref = pf.tasks[&crate::cow_refine::CHILD].vspace[&crate::cow_refine::COW_VA];
    assert_eq!(child_ref.access, crate::state::ACCESS_R);
}

#[test]
fn probe_parent_readwrite_is_writable() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
}

#[test]
fn probe_child_readonly_is_readonly() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, CHILD, COW_VA), CowProbe::ReadOnly);
}

#[test]
fn probe_unmapped_is_not_mapped() {
    let pf = new_refine();
    assert_eq!(probe_write(&pf, CHILD, 0x0600_0000), CowProbe::NotMapped);
}

#[test]
fn probe_has_no_side_effects() {
    let pf = new_refine();
    let before = pf.clone();
    let _ = probe_write(&pf, CHILD, COW_VA);
    assert_eq!(pf, before);
}

#[test]
fn write_fault_on_readwrite_parent_writes_without_fault() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    assert_eq!(write_fault(&mut exc, &mut pf, PARENT, COW_VA), WriteOutcome::Wrote);
    // 父仍 Running，未产生 pending
    assert_eq!(exc.tasks[&PARENT].status, TaskStatus::Running);
    assert!(!exc.pending_exc.contains_key(&PARENT));
}

#[test]
fn write_fault_on_child_readonly_triggers_page_fault() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let before_pf = pf.clone();
    let out = write_fault(&mut exc, &mut pf, CHILD, COW_VA);
    assert!(matches!(out, WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Faulted);
    assert!(exc.pending_exc.contains_key(&CHILD));
    assert_eq!(pf, before_pf, "probe/写不修改页表");
}

#[test]
fn write_fault_on_unmapped_is_faulted() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let out = write_fault(&mut exc, &mut pf, CHILD, 0x0600_0000);
    assert!(matches!(out, WriteOutcome::Faulted(ExceptionKind::PageFault { page_present: false, .. })));
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Faulted);
}

#[test]
fn shared_readonly_state_is_consistent() {
    let pf = new_refine();
    let exc = base_exc();
    assert!(cow_state_consistent(&pf, &exc, PARENT));
    assert!(cow_state_consistent(&pf, &exc, CHILD));
}

#[test]
fn inconsistent_when_child_upgraded_in_one_model_only() {
    let mut exc = base_exc();
    // 只改 ops 侧为 RW，RefineState 仍 R → 不一致
    exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = crate::state::ACCESS_R | crate::state::ACCESS_W;
    let pf = new_refine();
    assert!(!cow_state_consistent(&pf, &exc, CHILD));
}

#[test]
fn missing_task_is_inconsistent() {
    let mut pf = new_refine();
    // 删掉 RefineState 的 child task → 不一致
    let gone = pf.tasks.remove(&CHILD).unwrap();
    let exc = base_exc();
    assert!(!cow_state_consistent(&pf, &exc, CHILD));
    pf.tasks.insert(CHILD, gone);
}

/// §12.8 COW 成功路径端到端：父私写，子只读共享；子写触发 fault 后，
/// 细化层为子分配私写副本段（与原段同帧卡片数、物理帧相离→写隔离成立）。
/// 全程未引入任何新内核机制，仅组合既有 memseg/ops 原语。
#[test]
fn cow_roundtrip_proves_no_new_kernel_mechanism() {
    let mut pf = new_refine();
    let mut exc = base_exc();
    let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;

    // 1. 父可写，直接写成功
    assert_eq!(write_fault(&mut exc, &mut pf, PARENT, COW_VA), WriteOutcome::Wrote);

    // 2. 子只读共享，一致
    assert!(memseg_alpha(&pf));
    assert!(cow_state_consistent(&pf, &exc, CHILD));

    // 3. 子写只读 → page fault → Faulted
    assert!(matches!(write_fault(&mut exc, &mut pf, CHILD, COW_VA),
                     WriteOutcome::Faulted(ExceptionKind::PageFault { .. })));
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Faulted);
    assert!(cow_state_consistent(&pf, &exc, CHILD));

    // 4. 父（LibOS 角色）分配副本段并复制原帧集合：
    //    memseg_create 分配新物理帧（host 不模拟内存内容），故副本与原段
    //    共享“相同帧卡片数”、物理帧相离——既满足 α 的 W6 划分，又证写隔离。
    let copy_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).unwrap();
    let parent_frames = pf.segments[&1].frames.clone();
    let copy_frames = pf.segments[&copy_seg].frames.clone();
    assert_eq!(parent_frames.len(), copy_frames.len(), "副本复制原段物理帧集合（同卡片数）");
    assert!(
        parent_frames.iter().all(|f| !copy_frames.contains(f)),
        "副本物理帧与原段相离，写隔离成立"
    );

    // 5. 子换为私写副本
    memseg_unmap(&mut pf, true, CHILD, COW_VA, 1).unwrap();
    memseg_map(&mut pf, rights, true, copy_seg, CHILD, 0, COW_VA, 1,
               ACCESS_R | ACCESS_W).unwrap();
    exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = ACCESS_R | ACCESS_W;
    assert!(cow_state_consistent(&pf, &exc, CHILD));

    // 6. exc_resume：frame Segment(CODE_SEG) 写合法用户帧，帧 cap=FRAME_CAP
    let frame = resume_frame();
    exc.segments.get_mut(&CODE_SEG).unwrap().bytes.copy_from_slice(&frame.encode());
    ops::exc_resume(&mut exc, CHILD, 10, FRAME_CAP, 0).expect("resume child");
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Ready);

    // 7. schedule 恢复执行
    ops::schedule(&mut exc, CHILD);
    assert_eq!(exc.tasks[&CHILD].status, TaskStatus::Running);

    // 8. 终态：子私写可写，父仍私写；α 全程成立
    assert_eq!(probe_write(&pf, CHILD, COW_VA), CowProbe::Writable);
    assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
    assert!(cow_state_consistent(&pf, &exc, CHILD));
    assert!(memseg_alpha(&pf));
    // 父子对原共享页的写隔离：父段帧集合未因子复制而丢失，子段副本帧集合保持
    assert_eq!(pf.segments[&1].frames, parent_frames);
    assert_eq!(pf.segments[&copy_seg].frames, copy_frames);
}
