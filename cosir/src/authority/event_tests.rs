use super::*;

fn fixture() -> State {
    let mut s = State {
        local_node: 10,
        known_nodes: HashSet::from([10, 20]),
        ..State::default()
    };
    for id in 1..=3 {
        s.tasks.insert(
            id,
            Task {
                id,
                status: TaskStatus::Running,
                vspace: vec![],
                root_ns_cap: 0,
                kernel_resume_frame: None,
            },
        );
        s.push_audit(
            Actor::Kernel,
            AuditKind::TaskCreated {
                task: id,
                status: TaskStatus::Running,
            },
        );
    }
    s
}

#[test]
fn timers_fire_once_in_deadline_id_order_and_keep_stale_notifications() {
    let mut s = fixture();
    let first = timer_set(&mut s, 1, 2).unwrap();
    let second = timer_set(&mut s, 1, 1).unwrap();
    let third = timer_set(&mut s, 1, 2).unwrap();
    let cancelled = timer_set(&mut s, 1, 1).unwrap();
    assert_eq!(
        timer_cancel(&mut s, 1, cancelled),
        Ok(TimerCancelOutcome::Cancelled)
    );
    let before = s.clone();
    assert_eq!(
        timer_cancel(&mut s, 1, cancelled),
        Ok(TimerCancelOutcome::AlreadyCancelled)
    );
    assert_eq!(s, before);
    advance_time(&mut s, 2).unwrap();
    let kinds: Vec<_> = s.mailboxes[&1]
        .iter()
        .map(|id| s.messages[id].origin.clone())
        .collect();
    assert_eq!(
        kinds,
        [2, 1, 3].map(|id| MessageOrigin::Kernel(KernelEvent::TimerFired(id)))
    );
    let before = s.clone();
    assert_eq!(
        timer_cancel(&mut s, 1, first),
        Ok(TimerCancelOutcome::AlreadyFired)
    );
    assert_eq!(s, before);
    timer_destroy(&mut s, 1, second).unwrap();
    assert_eq!(recv(&mut s, 1).unwrap().origin, kinds[0]);
    assert_eq!(
        timer_cancel(&mut s, 1, second),
        Err(AuthorityError::TargetDead)
    );
    assert!(events_well_formed(&s) && locations_well_formed(&s) && provenance_well_formed(&s));
    assert_eq!(s.capabilities[&third].object, CapObject::Timer(3));
    let before = s.clone();
    assert_eq!(advance_time(&mut s, 1), Err(AuthorityError::InvalidTime));
    assert_eq!(s, before);
}

#[test]
fn immediate_timer_satisfies_blocked_recv_and_ids_fail_atomically() {
    let mut s = fixture();
    let cap = timer_set(&mut s, 1, 0).unwrap();
    assert!(matches!(s.timers[&1].state, TimerState::Fired { .. }));
    assert!(recv(&mut s, 1).is_some());
    assert!(recv(&mut s, 1).is_none());
    advance_time(&mut s, 1).unwrap();
    // timer_set is a user operation, so it cannot be called while blocked.
    // A previously set timer fires through the same direct-delivery path.
    s.tasks.get_mut(&1).unwrap().status = TaskStatus::Running;
    timer_set(&mut s, 1, 3).unwrap();
    assert!(recv(&mut s, 1).is_none());
    advance_time(&mut s, 3).unwrap();
    assert_eq!(s.tasks[&1].status, TaskStatus::Ready);
    assert_eq!(
        s.pending_returns[&1].origin,
        MessageOrigin::Kernel(KernelEvent::TimerFired(2))
    );
    assert!(events_well_formed(&s) && locations_well_formed(&s));
    assert_eq!(s.capabilities[&cap].object, CapObject::Timer(1));
    s.next_message = u64::MAX;
    let before = s.clone();
    assert_eq!(timer_set(&mut s, 2, 0), Err(AuthorityError::IdExhausted));
    assert_eq!(s, before);
}

#[test]
fn exception_fanout_uses_kernel_origin_and_last_token_loss_unsubscribes() {
    let mut s = fixture();
    let kind = ExceptionKind::ExternalInterrupt { irq: 4 };
    let source = exc_source_create(&mut s, 1, kind).unwrap();
    exc_subscribe(&mut s, 1, source).unwrap();
    let receiver = cap_derive(&mut s, 1, source, CAP_EXC_SUBSCRIBE).unwrap();
    // Transfer the subscription-only token via an actual local TaskSend cap.
    let send_cap = s.next_cap as u32;
    s.insert_capability(
        send_cap,
        Capability {
            owner: 1,
            object: CapObject::Task(2),
            rights: CAP_TASK_SEND,
            active: true,
            parent: None,
        },
    );
    s.push_audit(
        Actor::Kernel,
        AuditKind::CapabilityMinted {
            cap: send_cap,
            holder: 1,
        },
    );
    send(&mut s, 1, send_cap, b"cap", &[receiver]).unwrap();
    recv(&mut s, 2).unwrap();
    exc_subscribe(&mut s, 2, receiver).unwrap();
    assert!(recv(&mut s, 2).is_none());
    crate::ops::raise_exception(
        &mut s,
        1,
        1,
        kind,
        None,
        SretFrame::decode(&[0; SRET_FRAME_LEN]).unwrap(),
    );
    assert_eq!(s.tasks[&1].status, TaskStatus::Running);
    assert_eq!(s.tasks[&2].status, TaskStatus::Ready);
    assert!(matches!(
        s.pending_returns[&2].origin,
        MessageOrigin::Kernel(KernelEvent::ExceptionRaised { source: 1, .. })
    ));
    assert_eq!(s.mailboxes[&1].len(), 1);
    cap_drop(&mut s, 2, receiver).unwrap();
    assert!(!s.subscriptions.contains(&(2, 1)));
    exc_source_destroy(&mut s, 1, source).unwrap();
    assert!(s.subscriptions.is_empty());
    assert!(events_well_formed(&s) && locations_well_formed(&s));
    assert!(recv(&mut s, 1).is_some()); // Historical notification survives source destroy.
}

#[test]
fn exception_signatures_allow_general_fault_sets_and_reject_incompatible_kinds_atomically() {
    let mut s = fixture();
    let before = s.clone();
    assert_eq!(
        exc_source_create_kinds(&mut s, 1, &[]),
        Err(AuthorityError::InvalidKind)
    );
    assert_eq!(s, before);
    assert_eq!(
        exc_source_create_kinds(
            &mut s,
            1,
            &[
                ExceptionKind::DivideError,
                ExceptionKind::ExternalInterrupt { irq: 4 }
            ]
        ),
        Err(AuthorityError::KindConflict)
    );
    assert_eq!(s, before);
    let cap = exc_source_create_kinds(
        &mut s,
        1,
        &[
            ExceptionKind::GeneralProtectionFault,
            ExceptionKind::DivideError,
            ExceptionKind::InvalidOpcode,
            ExceptionKind::DoubleFault,
        ],
    )
    .unwrap();
    exc_subscribe(&mut s, 1, cap).unwrap();
    let destroy = cap_derive(&mut s, 1, cap, CAP_EXC_SOURCE_DESTROY).unwrap();
    relocate(
        &mut s,
        destroy,
        CapLocation::HeldBy(2),
        None,
        Actor::Task(1),
    );
    crate::ops::raise_exception(
        &mut s,
        1,
        1,
        ExceptionKind::DivideError,
        None,
        SretFrame::decode(&[0; SRET_FRAME_LEN]).unwrap(),
    );
    assert_eq!(s.tasks[&1].status, TaskStatus::Faulted);
    exc_source_destroy(&mut s, 2, destroy).unwrap();
    assert!(s.pending_exc.contains_key(&1));
    assert!(mechanisms_well_formed(&s));
}

#[test]
fn remote_data_has_no_local_queue_or_cap_copy_and_bad_destination_creates_no_proxy() {
    let mut a = fixture();
    let mut b = fixture();
    b.local_node = 20;
    let node = node_create(&mut a, 1, 20, NodeTransport::Direct(20)).unwrap();
    let target = node_attach(&mut a, 1, node, 2).unwrap();
    let before = a.clone();
    assert_eq!(
        send(&mut a, 1, target, b"no", &[node]),
        Err(AuthorityError::NonPortableCap)
    );
    assert_eq!(a, before);
    send(&mut a, 1, target, b"hello", &[]).unwrap();
    assert!(a.messages.is_empty());
    let packet = a.outgoing[0].1.clone();
    assert!(recv(&mut b, 2).is_none());
    deliver_remote(&mut b, &packet).unwrap().unwrap();
    assert_eq!(b.pending_returns[&2].data, b"hello");
    let proxy = match b.pending_returns[&2].origin {
        MessageOrigin::Task(task) => task,
        _ => panic!(),
    };
    assert_eq!(
        b.remote_tasks[&proxy],
        RemoteTask {
            peer: 10,
            remote_id: 1
        }
    );
    let roots = b.capabilities.len();
    let mut bad = packet;
    bad.receiver = 99;
    assert_eq!(deliver_remote(&mut b, &bad), Ok(None));
    assert_eq!(b.capabilities.len(), roots);
    node_destroy(&mut a, 1, node).unwrap();
    assert_eq!(
        send(&mut a, 1, target, b"stale", &[]),
        Err(AuthorityError::TargetDead)
    );
    assert!(remote_well_formed(&a) && remote_well_formed(&b));
    assert!(locations_well_formed(&a) && locations_well_formed(&b));
}

#[test]
fn recipient_death_cancels_timers_but_does_not_reclaim_external_tokens() {
    let mut s = fixture();
    let timer = timer_set(&mut s, 1, 10).unwrap();
    let inherited = cap_derive(&mut s, 1, timer, CAP_TIMER_CANCEL).unwrap();
    // Exercise ordinary move/drop cleanup with a bound external token.
    relocate(
        &mut s,
        inherited,
        CapLocation::HeldBy(2),
        None,
        Actor::Task(1),
    );
    retire_task(&mut s, 1, Actor::Kernel);
    assert!(s.timers.is_empty());
    assert!(s.held(2, inherited));
    assert_eq!(
        timer_cancel(&mut s, 2, inherited),
        Err(AuthorityError::TargetDead)
    );
    advance_time(&mut s, 20).unwrap();
    assert!(s.messages.is_empty());
    assert!(events_well_formed(&s) && locations_well_formed(&s));
}

#[test]
fn wait_all_alert_timeout_and_target_death_keep_registrations_consistent() {
    let mut s = fixture();
    let a = wait_object_create(&mut s, 1).unwrap();
    let b = wait_object_create(&mut s, 1).unwrap();
    let before = s.clone();
    assert_eq!(
        wait_objects(&mut s, 1, &[a, a], &[0, 0], WaitMode::All, false, None),
        Err(AuthorityError::DuplicateWaitObject)
    );
    assert_eq!(s, before);
    assert_eq!(
        wait_objects(&mut s, 1, &[a, b], &[0, 0], WaitMode::All, false, Some(10)),
        Ok(None)
    );
    assert!(waits_well_formed(&s));
    let signal_a = cap_derive(&mut s, 1, a, CAP_WAIT_SIGNAL).unwrap();
    let signal_b = cap_derive(&mut s, 1, b, CAP_WAIT_SIGNAL | CAP_WAIT_DESTROY).unwrap();
    relocate(
        &mut s,
        signal_a,
        CapLocation::HeldBy(2),
        None,
        Actor::Task(1),
    );
    relocate(
        &mut s,
        signal_b,
        CapLocation::HeldBy(2),
        None,
        Actor::Task(1),
    );
    assert_eq!(signal_object(&mut s, 2, signal_a), Ok(0));
    assert_eq!(signal_object(&mut s, 2, signal_b), Ok(1));
    assert_eq!(
        s.pending_wait_results.remove(&1),
        Some(WaitResult::Satisfied(vec![1, 2]))
    );
    s.tasks.get_mut(&1).unwrap().status = TaskStatus::Running;
    assert_eq!(
        wait_objects(&mut s, 1, &[a], &[1], WaitMode::Any, true, Some(10)),
        Ok(None)
    );
    let task_cap = fresh_cap(&s).unwrap();
    s.insert_capability(
        task_cap,
        Capability {
            owner: 2,
            object: CapObject::Task(1),
            rights: CAP_TASK_SEND,
            active: true,
            parent: None,
        },
    );
    task_alert(&mut s, 2, task_cap, b"signal").unwrap();
    assert_eq!(s.pending_wait_results.remove(&1), Some(WaitResult::Alerted));
    s.tasks.get_mut(&1).unwrap().status = TaskStatus::Running;
    assert_eq!(
        wait_objects(&mut s, 1, &[a], &[1], WaitMode::Any, true, Some(0)),
        Ok(Some(WaitResult::Alerted))
    );
    assert_eq!(task_upcall_recv(&mut s, 1)[0].data, b"signal");
    assert_eq!(
        wait_objects(&mut s, 1, &[a], &[1], WaitMode::Any, false, Some(10)),
        Ok(None)
    );
    advance_time(&mut s, 10).unwrap();
    assert_eq!(
        s.pending_wait_results.remove(&1),
        Some(WaitResult::TimedOut)
    );
    s.tasks.get_mut(&1).unwrap().status = TaskStatus::Running;
    assert_eq!(
        wait_objects(&mut s, 1, &[a, b], &[1, 1], WaitMode::All, false, None),
        Ok(None)
    );
    wait_object_destroy(&mut s, 2, signal_b).unwrap();
    assert_eq!(s.pending_wait_results[&1], WaitResult::TargetDead(2));
    assert!(waits_well_formed(&s) && locations_well_formed(&s));
}
