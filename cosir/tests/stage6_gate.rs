use cosir::{authority::*, gate::*, gate_memory::*, state::*};
use std::{collections::BTreeMap, sync::Arc};

fn frame() -> SretFrame {
    SretFrame::decode(&[0x17; SRET_FRAME_LEN]).unwrap()
}
fn fixture(count: u32, limit: usize) -> Model {
    let mut state = State::default();
    let mut memory = Memory::default();
    for id in 1..=count + 1 {
        state.tasks.insert(
            id,
            Task {
                id,
                status: if id == 1 {
                    TaskStatus::Running
                } else {
                    TaskStatus::Ready
                },
                vspace: vec![],
                root_ns_cap: 0,
                kernel_resume_frame: None,
            },
        );
        memory.0.insert(
            id,
            Space {
                identity: u64::from(id) * 0x1000,
                pages: BTreeMap::from([(
                    0x1000,
                    Page {
                        readable: true,
                        writable: true,
                        user: true,
                        bytes: Arc::new([id as u8; 4096]),
                    },
                )]),
            },
        );
        if id > 1 {
            state.insert_capability(
                id,
                Capability {
                    owner: 1,
                    object: CapObject::Task(id),
                    rights: CAP_TASK_MANAGE_VSPACE | CAP_TASK_SEND,
                    active: true,
                    parent: None,
                },
            );
        }
    }
    Model::new(state, memory, limit)
}
fn unchanged<T: PartialEq + std::fmt::Debug>(
    m: &mut Model,
    expected: Result<T, Error>,
    f: impl FnOnce(&mut Model) -> Result<T, Error>,
) {
    let before = m.clone();
    assert_eq!(f(m), expected);
    assert_eq!(*m, before);
}

#[test]
fn reply_is_opaque_rax_only_once_and_private_wait_cannot_be_signalled() {
    let mut m = fixture(1, usize::MAX);
    let g = m.attach(1, 2, None, 0x112233).unwrap();
    m.trap(2, frame(), u64::MAX, [u64::MAX; 6]).unwrap();
    assert!(waits_well_formed(m.state()));
    unchanged(&mut m, Err(Error::Denied), |m| m.signal_private(1, 2));
    unchanged(&mut m, Err(Error::Fault), |m| m.wait(1, g, false, true));
    unchanged(&mut m, Err(Error::Capacity), |m| m.wait(1, g, true, false));
    let req = m.wait(1, g, true, true).unwrap();
    unchanged(&mut m, Err(Error::Denied), |m| m.reply(2, req, 42));
    let alias = req;
    m.reply(1, req, 0xfedcba).unwrap();
    let mut expected = frame();
    expected.rax = 0xfedcba;
    assert_eq!(m.frame(2), Some(expected));
    unchanged(&mut m, Err(Error::Stale), |m| m.reply(1, alias, 99));
    unchanged(&mut m, Err(Error::Stale), |m| {
        m.copy(1, alias, 0x1000, 0x1000, 1, true)
    });
    assert_eq!(
        m.events()
            .iter()
            .filter(|e| matches!(e, Event::Terminal(..)))
            .count(),
        1
    );
}

#[test]
fn signal_before_and_after_delivery_never_cancels_or_restarts() {
    let mut m = fixture(1, usize::MAX);
    let g = m.attach(1, 2, None, 123).unwrap();
    m.trap(2, frame(), 18, [0; 6]).unwrap(); // collision with native recv is irrelevant
    m.alert(1, 2).unwrap();
    assert_eq!(m.phase(2), Some(Phase::Queued));
    let req = m.wait(1, g, true, true).unwrap();
    m.copy(1, req, 0x1000, 0x1000, 1, true).unwrap(); // observable side effect
    m.alert(1, 2).unwrap();
    assert_eq!(m.phase(2), Some(Phase::Delivered));
    assert_eq!(m.state().pending_upcalls[&2].len(), 2);
    assert_eq!(
        m.events()
            .iter()
            .filter(|e| matches!(e, Event::Trapped(..)))
            .count(),
        1
    );
    m.reply(1, req, 0).unwrap();
    assert_eq!(m.state().tasks[&2].status, TaskStatus::Ready);
}

#[test]
fn full_queue_uses_reserved_slots_and_promotes_fifo_after_cancel() {
    let mut m = fixture(65, usize::MAX);
    let g = m.attach(1, 2, None, 7).unwrap();
    for task in 3..=65 {
        m.attach(1, task, Some(g), 7).unwrap();
    }
    unchanged(&mut m, Err(Error::Capacity), |m| {
        m.attach(1, 66, Some(g), 7)
    });
    for task in 2..=65 {
        m.trap(task, frame(), u64::from(task), [0; 6]).unwrap();
    }
    assert_eq!(m.phase(10), Some(Phase::WaitingCapacity));
    m.destroy_client(5).unwrap();
    assert_eq!(m.phase(10), Some(Phase::Queued));
    let mut order = Vec::new();
    for _ in 0..63 {
        let req = m.wait(1, g, true, true).unwrap();
        let task = m
            .events()
            .iter()
            .find_map(|e| match e {
                Event::Trapped(r, t, _, _) if *r == req => Some(*t),
                _ => None,
            })
            .unwrap();
        order.push(task);
        m.reply(1, req, 0).unwrap();
    }
    assert_eq!(order, (2..=65).filter(|t| *t != 5).collect::<Vec<_>>());
    assert!(m.well_formed());
}

#[test]
fn nonreply_is_unbounded_until_supervisor_orphans_then_binding_fails_closed() {
    let mut m = fixture(2, usize::MAX);
    let g = m.attach(1, 2, None, 0xdeadcafe).unwrap();
    m.attach(1, 3, Some(g), 0xaabb).unwrap();
    m.trap(2, frame(), 1, [0; 6]).unwrap();
    let req = m.wait(1, g, true, true).unwrap();
    m.trap(3, frame(), 1, [0; 6]).unwrap();
    for _ in 0..100 {
        assert_eq!(m.phase(2), Some(Phase::Delivered));
    }
    m.orphan(g).unwrap();
    assert_eq!(m.frame(2).unwrap().rax, 0xdeadcafe);
    assert_eq!(m.frame(3).unwrap().rax, 0xaabb);
    unchanged(&mut m, Err(Error::Stale), |m| m.reply(1, req, 0));
    m.trap(2, frame(), 1, [0; 6]).unwrap();
    assert_eq!(m.frame(2).unwrap().rax, 0xdeadcafe); // never native syscall 1
    assert!(m.well_formed());
}

#[test]
fn proxy_copy_selects_target_root_and_preflights_both_ranges() {
    let mut m = fixture(1, usize::MAX);
    let g = m.attach(1, 2, None, 0).unwrap();
    m.trap(2, frame(), 0, [0; 6]).unwrap();
    let r = m.wait(1, g, true, true).unwrap();
    m.copy(1, r, 0x1000, 0x1000, 64, false).unwrap();
    assert_eq!(&m.memory().0[&1].pages[&0x1000].bytes[..64], &[2; 64]);
    unchanged(&mut m, Err(Error::Fault), |m| {
        m.copy(1, r, 0x1000, 0x1fff, 2, false)
    });
    unchanged(&mut m, Err(Error::Fault), |m| {
        m.copy(1, r, 0x1fff, 0x1000, 2, true)
    });
    unchanged(&mut m, Err(Error::Overflow), |m| {
        m.copy(1, r, u64::MAX, 0x1000, 2, true)
    });
    unchanged(&mut m, Err(Error::Capacity), |m| {
        m.copy(1, r, 0x1000, 0x1000, 4097, true)
    });
    m.copy(1, r, 0x1000, 0x1000, 0, false).unwrap();
    m.copy(1, r, 0x1000, 0x1000, 4096, false).unwrap();
}

#[test]
fn cow_cross_page_copy_preserves_sibling_and_rejects_stale_space_or_readonly_page() {
    let mut memory = Memory::default();
    let shared = Arc::new([9; 4096]);
    for task in 1..=3 {
        memory.0.insert(
            task,
            Space {
                identity: task as u64,
                pages: BTreeMap::from([
                    (
                        0x1000,
                        Page {
                            readable: true,
                            writable: true,
                            user: true,
                            bytes: shared.clone(),
                        },
                    ),
                    (
                        0x2000,
                        Page {
                            readable: true,
                            writable: task != 3,
                            user: true,
                            bytes: Arc::new([task as u8; 4096]),
                        },
                    ),
                ]),
            },
        );
    }
    let before = memory.clone();
    assert_eq!(
        memory.copy(1, 1, 0x1000, 3, 3, 0x1fff, 2),
        Err(Error::Fault)
    );
    assert_eq!(memory, before);
    assert_eq!(
        memory.copy(1, 99, 0x1000, 2, 2, 0x1000, 1),
        Err(Error::Stale)
    );
    memory.copy(1, 1, 0x1fff, 2, 2, 0x1fff, 2).unwrap();
    assert_eq!(memory.0[&2].pages[&0x2000].bytes[0], 1);
    assert_eq!(memory.0[&1], before.0[&1]);
    assert_eq!(memory.0[&3], before.0[&3]);
    assert!(!Arc::ptr_eq(&memory.0[&2].pages[&0x1000].bytes, &shared));
}

#[test]
fn audit_exhaustion_rolls_back_delivery_reply_and_orphan_atomically() {
    // Four events: attach, trap, deliver, reply. An orphan adds a second event.
    let mut m = fixture(1, 3);
    let g = m.attach(1, 2, None, 0).unwrap();
    m.trap(2, frame(), 0, [0; 6]).unwrap();
    let r = m.wait(1, g, true, true).unwrap();
    unchanged(&mut m, Err(Error::AuditFull), |m| m.reply(1, r, 123));
    unchanged(&mut m, Err(Error::AuditFull), |m| m.orphan(g));
    assert_eq!(m.state().tasks[&2].status, TaskStatus::BlockedWait);
}

#[test]
fn gate_service_cannot_be_gated_and_foreign_gate_cannot_be_shared() {
    let mut m = fixture(2, usize::MAX);
    let g = m.attach(1, 2, None, 0).unwrap();
    unchanged(&mut m, Err(Error::Denied), |m| m.attach(2, 3, Some(g), 0));
    unchanged(&mut m, Err(Error::Busy), |m| m.attach(1, 2, Some(g), 0));
    let mut state = m.state().clone();
    state.insert_capability(
        20,
        Capability {
            owner: 2,
            object: CapObject::Task(3),
            rights: CAP_TASK_MANAGE_VSPACE,
            active: true,
            parent: None,
        },
    );
    // Build another experiment with a bound service to exercise the cycle guard.
    let mut n = Model::new(state, m.memory().clone(), usize::MAX);
    let _ = n.attach(1, 2, None, 0).unwrap();
    unchanged(&mut n, Err(Error::Busy), |n| n.attach(2, 20, None, 0));
    let mut memory = fixture(2, usize::MAX).memory().clone();
    memory.0.get_mut(&3).unwrap().identity = memory.0[&2].identity;
    let mut shared = Model::new(fixture(2, usize::MAX).state().clone(), memory, usize::MAX);
    unchanged(&mut shared, Err(Error::Busy), |m| m.attach(1, 2, None, 0));
}
