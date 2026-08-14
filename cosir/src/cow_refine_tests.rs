use crate::cow_refine::{
    CHILD, COW_VA, CowProbe, PARENT, WriteOutcome, base_exc, cow_state_consistent, new_refine,
    probe_write, write_fault,
};
use crate::state::{ExceptionKind, TaskStatus};

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
