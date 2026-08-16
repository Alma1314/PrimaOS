use crate::cow_refine::{
    AccessOutcome, CHILD, CHILD_SELF_TASK_CAP, CHILD_TASK_CAP, CODE_SEG, CODE_VA, COW_VA, CowError,
    CowSliceState, DATA_SEG_CAP, FRAME_CAP, PARENT, authorize_mapping, resume_frame,
};
use crate::ops;
use crate::state::{
    ACCESS_R, ACCESS_W, ACCESS_X, CAP_EXC_SUBSCRIBE, CAP_SEG_DESTROY, CAP_SEG_MAP_R, CAP_SEG_MAP_W,
    CAP_SEG_MAP_X, CAP_TASK_MANAGE_VSPACE, CapObject, Capability, ExceptionKind,
    PageMappingBacking, PendingException, TaskStatus,
};
use crate::tlb_refine::{AccessKind, has_tlb, invalidate_va, translate as tlb_translate};

fn mapped_pte_mut(table: &mut crate::pagewalk::PageTable, va: u64) -> &mut crate::pagewalk::Pte {
    fn descend(node: &mut crate::pagewalk::PageTableNode, va: u64) -> &mut crate::pagewalk::Pte {
        match node {
            crate::pagewalk::PageTableNode::Leaf(slots) => slots[((va >> 12) & 0x1ff) as usize]
                .as_mut()
                .expect("mapped leaf"),
            crate::pagewalk::PageTableNode::NonLeaf { level, slots } => {
                let shift = match *level {
                    crate::pagewalk::NonLeafLevel::Pml4 => 39,
                    crate::pagewalk::NonLeafLevel::Pdpt => 30,
                    crate::pagewalk::NonLeafLevel::Pd => 21,
                };
                descend(
                    slots[((va >> shift) & 0x1ff) as usize]
                        .as_mut()
                        .expect("mapped branch"),
                    va,
                )
            }
        }
    }

    descend(&mut table.root, va)
}

#[test]
fn create_segment_mints_root_capability_with_requested_mapping_rights() {
    let mut slice = CowSliceState::new_cow_fixture();

    let cap = slice
        .create_segment(CHILD, 1, ACCESS_R | ACCESS_W)
        .expect("create private segment");

    let capability = &slice.ops.capabilities[&cap];
    assert_eq!(capability.owner, CHILD);
    assert!(matches!(capability.object, CapObject::Segment(_)));
    assert_eq!(
        capability.rights,
        CAP_SEG_MAP_R | CAP_SEG_MAP_W | CAP_SEG_DESTROY
    );
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn privatize_cow_uses_equal_page_count_and_disjoint_frames() {
    let mut slice = faulted_child_fixture();
    let old_frames = slice.cow_frames(CHILD, COW_VA).unwrap();
    let private_cap = slice
        .create_segment(CHILD, old_frames.len() as u64, ACCESS_R | ACCESS_W)
        .unwrap();
    let task_cap = CHILD_SELF_TASK_CAP;

    slice
        .privatize_cow(CHILD, CHILD, private_cap, task_cap, COW_VA)
        .unwrap();

    let new_frames = slice.cow_frames(CHILD, COW_VA).unwrap();
    assert_eq!(old_frames.len(), new_frames.len());
    assert!(old_frames.iter().all(|frame| !new_frames.contains(frame)));
    assert_eq!(
        slice.mapping(CHILD, COW_VA).unwrap().access,
        ACCESS_R | ACCESS_W
    );
    assert!(!has_tlb(&slice.tlb, CHILD, COW_VA));
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn privatize_cow_rejects_fault_va_mismatch_atomically() {
    const OTHER_COW_VA: u64 = 0x7000_0000;

    let mut slice = established_cow_fixture(CHILD);
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        }))
    ));
    let private_cap = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
    let before = slice.clone();

    assert_eq!(
        slice.privatize_cow(CHILD, CHILD, private_cap, CHILD_SELF_TASK_CAP, OTHER_COW_VA),
        Err(CowError::InvalidTaskState)
    );
    assert_eq!(slice, before);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn privatize_cow_rejects_missing_fault_preconditions_atomically() {
    let mut slice = CowSliceState::new_cow_fixture();
    let private_cap = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
    let task_cap = CHILD_SELF_TASK_CAP;
    let before = slice.clone();

    assert_eq!(
        slice.privatize_cow(CHILD, CHILD, private_cap, task_cap, COW_VA),
        Err(CowError::InvalidTaskState)
    );
    assert_eq!(slice, before);
}

fn assert_privatize_rejected_atomically(
    mut slice: CowSliceState,
    private_cap: u32,
    task_cap: u32,
    va: u64,
    expected: CowError,
) {
    assert_eq!(slice.check_invariants(), Ok(()));
    let before = slice.clone();
    assert_eq!(
        slice.privatize_cow(CHILD, CHILD, private_cap, task_cap, va),
        Err(expected)
    );
    assert_eq!(slice, before);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn privatize_cow_public_rejections_are_independently_atomic() {
    // Each case starts Faulted with a matching writable-present fault, so it reaches
    // the targeted public validation rather than being masked by an earlier guard.
    let valid = || {
        let mut slice = faulted_child_fixture();
        let private = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
        let task = CHILD_SELF_TASK_CAP;
        (slice, private, task)
    };

    let (slice, _private, task) = valid();
    assert_privatize_rejected_atomically(slice, 999, task, COW_VA, CowError::NoSuchCapability);

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&private).unwrap().object = CapObject::Task(CHILD);
    assert_privatize_rejected_atomically(
        slice,
        private,
        task,
        COW_VA,
        CowError::CapabilityObjectMismatch,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&private).unwrap().owner = PARENT;
    assert_privatize_rejected_atomically(
        slice,
        private,
        task,
        COW_VA,
        CowError::CapabilityHolderMismatch,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&private).unwrap().active = false;
    assert_privatize_rejected_atomically(
        slice,
        private,
        task,
        COW_VA,
        CowError::InactiveOrStaleCapability,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&private).unwrap().object = CapObject::Segment(999);
    assert_privatize_rejected_atomically(
        slice,
        private,
        task,
        COW_VA,
        CowError::InactiveOrStaleCapability,
    );

    let (slice, private, _task) = valid();
    assert_privatize_rejected_atomically(slice, private, 999, COW_VA, CowError::NoSuchCapability);

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&task).unwrap().object = CapObject::Task(PARENT);
    assert_privatize_rejected_atomically(
        slice,
        private,
        task,
        COW_VA,
        CowError::CapabilityObjectMismatch,
    );

    let (mut slice, private, task) = valid();
    slice.ops.pending_exc.get_mut(&CHILD).unwrap().source_id = 99;
    assert_privatize_rejected_atomically(slice, private, task, COW_VA, CowError::InvalidTaskState);

    let (mut slice, private, task) = valid();
    slice.ops.pending_exc.get_mut(&CHILD).unwrap().kind = ExceptionKind::PageFault {
        page_present: false,
        writable_fault: true,
    };
    assert_privatize_rejected_atomically(slice, private, task, COW_VA, CowError::InvalidTaskState);

    let (mut slice, private, task) = valid();
    slice.ops.pending_exc.get_mut(&CHILD).unwrap().kind = ExceptionKind::PageFault {
        page_present: true,
        writable_fault: false,
    };
    assert_privatize_rejected_atomically(slice, private, task, COW_VA, CowError::InvalidTaskState);

    let (mut slice, _private, task) = valid();
    let wrong_size = slice.create_segment(CHILD, 2, ACCESS_R | ACCESS_W).unwrap();
    assert_privatize_rejected_atomically(
        slice,
        wrong_size,
        task,
        COW_VA,
        CowError::MappingConflict,
    );
}

#[test]
fn privatize_cow_covers_remaining_reachable_public_rejections_atomically() {
    // Every authority case begins with the complete, invariant-valid Faulted state
    // produced by a real write fault.  Mutating an unused capability record does
    // not alter the composite state and lets the public entry reach its guard.
    let valid = || {
        let mut slice = faulted_child_fixture();
        let private = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
        let task = CHILD_SELF_TASK_CAP;
        (slice, private, task)
    };
    let assert_rejected = |mut slice: CowSliceState, actor, target, private, task, va, expected| {
        assert_eq!(slice.check_invariants(), Ok(()));
        let before = slice.clone();
        assert_eq!(
            slice.privatize_cow(actor, target, private, task, va),
            Err(expected)
        );
        assert_eq!(slice, before);
        assert_eq!(slice.check_invariants(), Ok(()));
    };

    let (slice, private, task) = valid();
    assert_rejected(
        slice,
        CHILD,
        999,
        private,
        task,
        COW_VA,
        CowError::NoSuchTask,
    );

    let (slice, private, task) = valid();
    assert_rejected(
        slice,
        PARENT,
        CHILD,
        private,
        task,
        COW_VA,
        CowError::InvalidTaskState,
    );

    let (slice, private, task) = valid();
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        COW_VA + 1,
        CowError::InvalidAddress,
    );

    let (slice, private, task) = valid();
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        0x0000_8000_0000_0000,
        CowError::InvalidAddress,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&private).unwrap().rights = CAP_SEG_MAP_R;
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        COW_VA,
        CowError::CapabilityRightDenied,
    );

    let (mut slice, private, task) = valid();
    let private_seg = match slice.ops.capabilities[&private].object {
        CapObject::Segment(id) => id,
        _ => unreachable!("private capability must target a segment"),
    };
    slice
        .memory
        .segments
        .get_mut(&private_seg)
        .unwrap()
        .allowed_access = ACCESS_R;
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        COW_VA,
        CowError::CapabilityRightDenied,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&task).unwrap().active = false;
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        COW_VA,
        CowError::InactiveOrStaleCapability,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&task).unwrap().owner = PARENT;
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        COW_VA,
        CowError::CapabilityHolderMismatch,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&task).unwrap().object = CapObject::Segment(1);
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        COW_VA,
        CowError::CapabilityObjectMismatch,
    );

    let (mut slice, private, task) = valid();
    slice.ops.capabilities.get_mut(&task).unwrap().rights = 0;
    assert_rejected(
        slice,
        CHILD,
        CHILD,
        private,
        task,
        COW_VA,
        CowError::CapabilityRightDenied,
    );
}

#[test]
fn privatize_cow_rejects_when_another_task_is_current_atomically() {
    // A public child write fault releases the CPU. Scheduling PARENT afterwards leaves
    // every child fault, authority, VA, and segment precondition valid; only current
    // being Some(PARENT) must reject privatization.
    let mut slice = faulted_child_fixture();
    let private = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
    let task = CHILD_SELF_TASK_CAP;
    assert_eq!(slice.current, None);
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Faulted);
    slice.schedule(PARENT).unwrap();
    assert_eq!(slice.current, Some(PARENT));
    assert_eq!(slice.check_invariants(), Ok(()));
    let before = slice.clone();

    assert_eq!(
        slice.privatize_cow(CHILD, CHILD, private, task, COW_VA),
        Err(CowError::InvalidTaskState)
    );
    assert_eq!(slice, before);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn privatize_cow_rejects_current_faulted_task_as_corrupt_atomically() {
    // A Faulted task cannot be current under the composite invariant. Therefore this
    // public entry observes corrupt composite state, not a separately reachable guard.
    let mut slice = faulted_child_fixture();
    let private = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
    let task = CHILD_SELF_TASK_CAP;
    slice.current = Some(CHILD);
    let before = slice.clone();

    assert_eq!(
        slice.privatize_cow(CHILD, CHILD, private, task, COW_VA),
        Err(CowError::CorruptCompositeState)
    );
    assert_eq!(slice, before);
}

#[test]
fn resume_requires_explicit_schedule_before_running_again() {
    let mut slice = faulted_child_fixture();
    let private_cap = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
    let task_cap = CHILD_SELF_TASK_CAP;
    slice
        .privatize_cow(CHILD, CHILD, private_cap, task_cap, COW_VA)
        .unwrap();

    slice.resume(CHILD, 10, FRAME_CAP, 0).unwrap();
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Ready);
    assert_eq!(slice.current, None);
    slice.schedule(CHILD).unwrap();
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Running);
    assert_eq!(slice.current, Some(CHILD));
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Allowed(_))
    ));
}

#[test]
fn destroy_ready_task_removes_all_task_state_and_tlb() {
    let mut slice = CowSliceState::new_cow_fixture();
    tlb_translate(
        &slice.memory,
        &slice.ops.tasks,
        &mut slice.tlb,
        CHILD,
        CODE_VA,
    )
    .unwrap();
    assert!(has_tlb(&slice.tlb, CHILD, CODE_VA));

    slice.destroy_task(PARENT, CHILD, CHILD_TASK_CAP).unwrap();

    assert!(!slice.memory.tasks.contains_key(&CHILD));
    assert!(!slice.ops.tasks.contains_key(&CHILD));
    assert!(!slice.tlb_has_task(CHILD));
    assert!(slice.task_capabilities(CHILD).all(|cap| !cap.active));
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn destroy_task_rejects_current_parent_atomically() {
    let mut slice = CowSliceState::new_cow_fixture();
    let before = slice.clone();

    assert_eq!(
        slice.destroy_task(PARENT, PARENT, crate::cow_refine::PARENT_SELF_TASK_CAP),
        Err(CowError::InvalidTaskState)
    );
    assert_eq!(slice, before);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn destroy_task_rejects_faulted_task_atomically() {
    let mut slice = faulted_child_fixture();
    let before = slice.clone();

    assert_eq!(
        slice.destroy_task(PARENT, CHILD, CHILD_TASK_CAP),
        Err(CowError::InvalidTaskState)
    );
    assert_eq!(slice, before);
}

#[test]
fn fresh_slice_has_one_current_running_parent() {
    let slice = CowSliceState::new_cow_fixture();
    assert_eq!(slice.current, Some(PARENT));
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Running);
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Ready);
    assert!(!slice.memory.tasks[&CHILD].vspace.contains_key(&COW_VA));
    assert!(
        !slice.ops.tasks[&CHILD]
            .vspace
            .iter()
            .any(|mapping| mapping.contains(COW_VA))
    );
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn schedule_rejects_busy_cpu() {
    let mut slice = CowSliceState::new_cow_fixture();
    let before = slice.clone();
    assert_eq!(slice.schedule(CHILD), Err(CowError::CpuBusy));
    assert_eq!(slice, before);
}

#[test]
fn schedule_rejects_unknown_task() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.yield_current(PARENT).unwrap();
    let before = slice.clone();
    assert_eq!(slice.schedule(999), Err(CowError::NoSuchTask));
    assert_eq!(slice, before);
}

#[test]
fn schedule_rejects_non_ready_task() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.yield_current(PARENT).unwrap();
    slice.ops.tasks.get_mut(&CHILD).unwrap().status = TaskStatus::BlockedRecv;
    let before = slice.clone();
    assert_eq!(slice.schedule(CHILD), Err(CowError::InvalidTaskState));
    assert_eq!(slice, before);
}

#[test]
fn schedule_ready_task_sets_the_single_current() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.yield_current(PARENT).unwrap();
    slice.schedule(CHILD).unwrap();
    assert_eq!(slice.current, Some(CHILD));
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Ready);
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Running);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn yield_current_clears_current_and_readies_task() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.yield_current(PARENT).unwrap();
    assert_eq!(slice.current, None);
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Ready);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn yield_current_rejects_non_current_task() {
    let mut slice = CowSliceState::new_cow_fixture();
    let before = slice.clone();
    assert_eq!(slice.yield_current(CHILD), Err(CowError::InvalidTaskState));
    assert_eq!(slice, before);
}

fn established_cow_fixture(running: crate::state::TaskId) -> CowSliceState {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();
    if running == CHILD {
        slice.yield_current(PARENT).unwrap();
        slice.schedule(CHILD).unwrap();
    }
    slice
}

fn faulted_child_fixture() -> CowSliceState {
    let mut slice = established_cow_fixture(CHILD);
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        }))
    ));
    slice
}

#[test]
fn establish_cow_makes_parent_and_child_readonly_on_same_frame() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();
    let parent = slice.ops.tasks[&PARENT]
        .vspace
        .iter()
        .find(|mapping| mapping.contains(COW_VA))
        .unwrap();
    let child = slice.ops.tasks[&CHILD]
        .vspace
        .iter()
        .find(|mapping| mapping.contains(COW_VA))
        .unwrap();
    assert_eq!(parent.access, ACCESS_R);
    assert_eq!(child.access, ACCESS_R);
    assert_eq!(parent.backing, child.backing);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn establish_cow_rejects_invalid_address_atomically() {
    let mut slice = CowSliceState::new_cow_fixture();
    let before = slice.clone();
    assert_eq!(
        slice.establish_cow(
            PARENT,
            PARENT,
            CHILD,
            DATA_SEG_CAP,
            CHILD_TASK_CAP,
            COW_VA + 1
        ),
        Err(CowError::InvalidAddress)
    );
    assert_eq!(slice, before);
}

#[test]
fn establish_cow_rejects_existing_child_mapping_atomically() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();
    let before = slice.clone();
    assert_eq!(
        slice.establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA),
        Err(CowError::MappingConflict)
    );
    assert_eq!(slice, before);
}

#[test]
fn establish_cow_rejects_bad_authority_atomically() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .active = false;
    let before = slice.clone();
    assert_eq!(
        slice.establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA),
        Err(CowError::InactiveOrStaleCapability)
    );
    assert_eq!(slice, before);
}

#[test]
fn parent_first_write_after_cow_faults_despite_preexisting_rw_tlb_entry() {
    let mut slice = CowSliceState::new_cow_fixture();
    let stale = tlb_translate(
        &slice.memory,
        &slice.ops.tasks,
        &mut slice.tlb,
        PARENT,
        COW_VA,
    )
    .unwrap();
    assert_eq!(stale.access, ACCESS_R | ACCESS_W);
    assert!(has_tlb(&slice.tlb, PARENT, COW_VA));

    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();
    assert!(!has_tlb(&slice.tlb, PARENT, COW_VA));

    assert_eq!(
        slice.access(PARENT, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        }))
    );
    assert_eq!(slice.current, None);
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Faulted);
    assert!(slice.ops.pending_exc.contains_key(&PARENT));
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn establish_cow_leaves_child_tlb_invalidation_idempotent() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();

    assert!(!has_tlb(&slice.tlb, CHILD, COW_VA));
    assert_eq!(
        invalidate_va(&slice.ops.tasks, &mut slice.tlb, CHILD, COW_VA),
        Ok(())
    );
    assert!(!has_tlb(&slice.tlb, CHILD, COW_VA));
}

#[test]
fn child_first_write_after_cow_faults_and_releases_cpu() {
    let mut slice = established_cow_fixture(CHILD);
    assert_eq!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        }))
    );
    assert_eq!(slice.current, None);
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Faulted);
    assert!(slice.ops.pending_exc.contains_key(&CHILD));
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Ready);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn access_rejects_non_current_task_atomically() {
    let mut slice = established_cow_fixture(PARENT);
    let before = slice.clone();
    assert_eq!(
        slice.access(CHILD, COW_VA, AccessKind::Read),
        Err(CowError::InvalidTaskState)
    );
    assert_eq!(slice, before);
}

#[test]
fn access_allows_read_without_scheduling() {
    let mut slice = established_cow_fixture(PARENT);
    assert!(matches!(
        slice.access(PARENT, COW_VA, AccessKind::Read),
        Ok(AccessOutcome::Allowed(_))
    ));
    assert_eq!(slice.current, Some(PARENT));
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Running);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn access_unmapped_faults_without_implicit_schedule() {
    let mut slice = established_cow_fixture(PARENT);
    let unmapped_va = 0x7000_0000;

    assert_eq!(
        slice.access(PARENT, unmapped_va, AccessKind::Read),
        Ok(AccessOutcome::Faulted(ExceptionKind::PageFault {
            page_present: false,
            writable_fault: false,
        }))
    );
    assert_eq!(slice.current, None);
    assert_eq!(slice.ops.tasks[&PARENT].status, TaskStatus::Faulted);
    assert_eq!(slice.ops.tasks[&CHILD].status, TaskStatus::Ready);
    assert_eq!(
        slice.ops.pending_exc[&PARENT].kind,
        ExceptionKind::PageFault {
            page_present: false,
            writable_fault: false,
        }
    );
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn check_invariants_rejects_second_running_task() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&CHILD).unwrap().status = TaskStatus::Running;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_current_pointing_to_ready_task() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&PARENT).unwrap().status = TaskStatus::Ready;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_memory_ops_task_set_mismatch() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.memory.tasks.remove(&CHILD);
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_ops_task_key_id_mismatch() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&PARENT).unwrap().id = CHILD;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_mapping_access_mismatch() {
    let mut slice = CowSliceState::new_cow_fixture();
    let mapping = slice.ops.tasks.get_mut(&PARENT).unwrap().vspace[0];
    slice.ops.tasks.get_mut(&PARENT).unwrap().vspace[0].access = mapping.access & !ACCESS_W;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_mapping_segment_mismatch() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&PARENT).unwrap().vspace[0].backing = Some(PageMappingBacking {
        segment: CODE_SEG,
        offset: 0,
        frame_phys: slice.memory.segments[&CODE_SEG].frames[0],
    });
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_mapping_frame_mismatch() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&PARENT).unwrap().vspace[0]
        .backing
        .as_mut()
        .unwrap()
        .frame_phys ^= crate::pagewalk::PAGE_SIZE;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_dead_task_tlb() {
    let mut slice = CowSliceState::new_cow_fixture();
    crate::tlb_refine::translate(
        &slice.memory,
        &slice.ops.tasks,
        &mut slice.tlb,
        CHILD,
        CODE_VA,
    )
    .unwrap();
    slice.memory.tasks.remove(&CHILD);
    slice.ops.tasks.remove(&CHILD);
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::TlbLifecycleInconsistent)
    );
}

#[test]
fn check_invariants_rejects_mapping_offset_mismatch() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&PARENT).unwrap().vspace[0]
        .backing
        .as_mut()
        .unwrap()
        .offset = 1;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_non_user_mapping() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&PARENT).unwrap().vspace[0].user = false;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_nonpresent_memory_leaf() {
    let mut slice = CowSliceState::new_cow_fixture();
    let task = slice.memory.tasks.get_mut(&PARENT).unwrap();
    mapped_pte_mut(&mut task.table, COW_VA).present = false;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_ready_task_with_pending_exception() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.pending_exc.insert(
        CHILD,
        PendingException {
            source_id: crate::cow_refine::page_fault_source_id(),
            kind: ExceptionKind::PageFault {
                page_present: true,
                writable_fault: true,
            },
            fault_va: Some(COW_VA),
            save_frame: resume_frame(),
        },
    );
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_faulted_task_without_pending_exception() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.tasks.get_mut(&CHILD).unwrap().status = TaskStatus::Faulted;
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

#[test]
fn check_invariants_rejects_faulted_current_task() {
    let mut slice = CowSliceState::new_cow_fixture();
    ops::raise_exception(
        &mut slice.ops,
        PARENT,
        crate::cow_refine::page_fault_source_id(),
        ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        },
        Some(COW_VA),
        resume_frame(),
    );
    assert_eq!(
        slice.check_invariants(),
        Err(CowError::CorruptCompositeState)
    );
}

fn assert_authority_rejects(
    slice: &CowSliceState,
    seg_cap: u32,
    task_cap: u32,
    requested: u8,
    expected: CowError,
) {
    assert_eq!(
        authorize_mapping(
            &slice.ops,
            &slice.memory,
            PARENT,
            seg_cap,
            task_cap,
            requested,
        )
        .map(|_| ()),
        Err(expected)
    );
}

#[test]
fn authority_accepts_valid_segment_and_task_capabilities() {
    let slice = CowSliceState::new_cow_fixture();
    assert!(
        authorize_mapping(
            &slice.ops,
            &slice.memory,
            PARENT,
            DATA_SEG_CAP,
            CHILD_TASK_CAP,
            ACCESS_R,
        )
        .is_ok()
    );
}

#[test]
fn authority_rejects_invalid_requested_bits_before_missing_caps() {
    let slice = CowSliceState::new_cow_fixture();
    assert_authority_rejects(&slice, 999, 998, 0, CowError::InvalidAddress);
}

#[test]
fn authority_rejects_missing_segment_capability() {
    let slice = CowSliceState::new_cow_fixture();
    assert_authority_rejects(
        &slice,
        999,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::NoSuchCapability,
    );
}

#[test]
fn authority_rejects_foreign_segment_capability_holder() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.capabilities.get_mut(&DATA_SEG_CAP).unwrap().owner = CHILD;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::CapabilityHolderMismatch,
    );
}

#[test]
fn authority_rejects_wrong_segment_capability_object() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .object = CapObject::Task(CHILD);
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::CapabilityObjectMismatch,
    );
}

#[test]
fn authority_rejects_inactive_segment_capability() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .active = false;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::InactiveOrStaleCapability,
    );
}

#[test]
fn authority_rejects_stale_segment_target() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice.ops.capabilities.insert(
        99,
        Capability {
            owner: PARENT,
            object: CapObject::Segment(999),
            rights: CAP_SEG_MAP_R,
            active: true,
        },
    );
    assert_authority_rejects(
        &slice,
        99,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::InactiveOrStaleCapability,
    );
}

#[test]
fn authority_rejects_missing_segment_map_right() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights = CAP_SEG_MAP_W;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::CapabilityRightDenied,
    );
}

#[test]
fn authority_checks_segment_stage_before_task_stage() {
    let slice = CowSliceState::new_cow_fixture();
    assert_authority_rejects(&slice, 999, 998, ACCESS_R, CowError::NoSuchCapability);
}

#[test]
fn authority_checks_inactive_task_cap_before_allowed_access() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights |= CAP_SEG_MAP_X;
    slice
        .ops
        .capabilities
        .get_mut(&CHILD_TASK_CAP)
        .unwrap()
        .active = false;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_X,
        CowError::InactiveOrStaleCapability,
    );
}

#[test]
fn authority_checks_foreign_task_holder_before_allowed_access() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights |= CAP_SEG_MAP_X;
    slice
        .ops
        .capabilities
        .get_mut(&CHILD_TASK_CAP)
        .unwrap()
        .owner = CHILD;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_X,
        CowError::CapabilityHolderMismatch,
    );
}

#[test]
fn authority_checks_stale_task_target_before_allowed_access() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights |= CAP_SEG_MAP_X;
    slice
        .ops
        .capabilities
        .get_mut(&CHILD_TASK_CAP)
        .unwrap()
        .object = CapObject::Task(999);
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_X,
        CowError::InactiveOrStaleCapability,
    );
}

#[test]
fn authority_checks_wrong_task_object_before_allowed_access() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights |= CAP_SEG_MAP_X;
    slice
        .ops
        .capabilities
        .get_mut(&CHILD_TASK_CAP)
        .unwrap()
        .object = CapObject::Segment(1);
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_X,
        CowError::CapabilityObjectMismatch,
    );
}

#[test]
fn authority_checks_missing_task_right_before_allowed_access() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights |= CAP_SEG_MAP_X;
    slice
        .ops
        .capabilities
        .get_mut(&CHILD_TASK_CAP)
        .unwrap()
        .rights = 0;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_X,
        CowError::CapabilityRightDenied,
    );
}

#[test]
fn authority_checks_allowed_access_after_both_capability_stages() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights |= CAP_SEG_MAP_X;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_X,
        CowError::CapabilityRightDenied,
    );
}

#[test]
fn authority_rejects_missing_task_capability() {
    let slice = CowSliceState::new_cow_fixture();
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        999,
        ACCESS_R,
        CowError::NoSuchCapability,
    );
}

#[test]
fn authority_rejects_wrong_task_capability_object() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&CHILD_TASK_CAP)
        .unwrap()
        .object = CapObject::Segment(1);
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::CapabilityObjectMismatch,
    );
}

#[test]
fn authority_rejects_missing_task_manage_right() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&CHILD_TASK_CAP)
        .unwrap()
        .rights = 0;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_R,
        CowError::CapabilityRightDenied,
    );
}

#[test]
fn authority_rejects_access_beyond_segment_allowed_access() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .ops
        .capabilities
        .get_mut(&DATA_SEG_CAP)
        .unwrap()
        .rights |= CAP_SEG_MAP_X;
    assert_authority_rejects(
        &slice,
        DATA_SEG_CAP,
        CHILD_TASK_CAP,
        ACCESS_X,
        CowError::CapabilityRightDenied,
    );
}

#[test]
fn capability_right_domains_do_not_overlap() {
    assert_eq!(CAP_EXC_SUBSCRIBE & CAP_SEG_MAP_R, 0);
    assert_eq!(CAP_SEG_MAP_R | CAP_SEG_MAP_W | CAP_SEG_MAP_X, 0b0000_1110);
    assert_eq!(CAP_SEG_DESTROY & CAP_TASK_MANAGE_VSPACE, 0);
}

#[test]
fn cow_slice_child_write_fault_private_resume_schedule_succeeds() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();
    slice.yield_current(PARENT).unwrap();
    slice.schedule(CHILD).unwrap();
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(_))
    ));
    let private = slice.create_segment(CHILD, 1, ACCESS_R | ACCESS_W).unwrap();
    slice
        .privatize_cow(CHILD, CHILD, private, CHILD_SELF_TASK_CAP, COW_VA)
        .unwrap();
    slice.write_valid_resume_frame_for_test(CHILD).unwrap();
    slice.resume(CHILD, 10, FRAME_CAP, 0).unwrap();
    slice.schedule(CHILD).unwrap();
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Allowed(_))
    ));
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn r1_fixed_two_task_order_writes_each_fault_on_shared_readonly() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();

    // 单一 fixture 的固定 2-Task 顺序（parent 写 fault → schedule child → child 写 fault），
    // 不含并发交错。仅为 SPEC-015 反例 R1 的顺序外推提供宿主证据，不能读作
    // "多核并发下各 writer 必 fault"的一般性主张。
    assert!(matches!(
        slice.access(PARENT, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(_))
    ));
    slice.schedule(CHILD).unwrap();
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(_))
    ));
    assert_eq!(slice.current, None);
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn r4_resume_without_privatization_faults_again_after_schedule() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();
    slice.yield_current(PARENT).unwrap();
    slice.schedule(CHILD).unwrap();
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(_))
    ));

    slice.write_valid_resume_frame_for_test(CHILD).unwrap();
    slice.resume(CHILD, 10, FRAME_CAP, 0).unwrap();
    slice.schedule(CHILD).unwrap();
    assert!(matches!(
        slice.access(CHILD, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(_))
    ));
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn parent_write_fault_private_resume_schedule_succeeds_via_public_api() {
    let mut slice = CowSliceState::new_cow_fixture();
    slice
        .establish_cow(PARENT, PARENT, CHILD, DATA_SEG_CAP, CHILD_TASK_CAP, COW_VA)
        .unwrap();
    assert!(matches!(
        slice.access(PARENT, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Faulted(_))
    ));
    let private = slice
        .create_segment(PARENT, 1, ACCESS_R | ACCESS_W)
        .unwrap();
    slice
        .privatize_cow(
            PARENT,
            PARENT,
            private,
            crate::cow_refine::PARENT_SELF_TASK_CAP,
            COW_VA,
        )
        .unwrap();
    slice.write_valid_resume_frame_for_test(PARENT).unwrap();
    slice
        .resume(
            PARENT,
            crate::cow_refine::PARENT_EXC_CAP,
            crate::cow_refine::PARENT_FRAME_CAP,
            0,
        )
        .unwrap();
    slice.schedule(PARENT).unwrap();
    assert!(matches!(
        slice.access(PARENT, COW_VA, AccessKind::Write),
        Ok(AccessOutcome::Allowed(_))
    ));
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn destroy_task_requires_actor_task_authority_and_cleans_holder_state() {
    let mut slice = CowSliceState::new_cow_fixture();
    let before = slice.clone();
    assert_eq!(
        slice.destroy_task(CHILD, CHILD, CHILD_TASK_CAP),
        Err(CowError::CapabilityHolderMismatch)
    );
    assert_eq!(slice, before);

    slice.destroy_task(PARENT, CHILD, CHILD_TASK_CAP).unwrap();
    assert!(!slice.task_exists(CHILD));
    assert!(!slice.has_active_capability_owner(CHILD));
    assert!(!slice.has_subscription(CHILD, crate::cow_refine::page_fault_source_id()));
    assert!(!slice.tlb_has_task(CHILD));
    assert_eq!(slice.check_invariants(), Ok(()));
}

#[test]
fn destroy_segment_rejects_missing_destroy_right_without_mutating_state() {
    let mut slice = CowSliceState::new_cow_fixture();
    let cap = slice.create_segment(PARENT, 1, ACCESS_R).unwrap();
    slice.ops.capabilities.get_mut(&cap).unwrap().rights &= !CAP_SEG_DESTROY;
    let before = slice.clone();

    assert_eq!(
        slice.destroy_segment(PARENT, cap),
        Err(CowError::CapabilityRightDenied)
    );
    assert_eq!(slice, before);
}

#[test]
fn destroy_segment_requires_authority_and_is_atomic_while_mapped() {
    let mut slice = CowSliceState::new_cow_fixture();
    let before = slice.clone();
    assert_eq!(
        slice.destroy_segment(CHILD, DATA_SEG_CAP),
        Err(CowError::CapabilityHolderMismatch)
    );
    assert_eq!(slice, before);

    let before = slice.clone();
    assert_eq!(
        slice.destroy_segment(PARENT, DATA_SEG_CAP),
        Err(CowError::MemSeg(
            crate::memseg_refine::MemSegError::StillMapped
        ))
    );
    assert_eq!(slice, before);

    let cap = slice.create_segment(PARENT, 1, ACCESS_R).unwrap();
    slice.destroy_segment(PARENT, cap).unwrap();
    assert_eq!(slice.check_invariants(), Ok(()));
}
