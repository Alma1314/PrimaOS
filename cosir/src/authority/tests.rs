use super::*;

fn fixture() -> State {
    let mut s = State::default();
    for task in 1..=3 {
        s.tasks.insert(
            task,
            Task {
                id: task,
                status: TaskStatus::Running,
                vspace: vec![],
                root_ns_cap: 0,
                kernel_resume_frame: None,
            },
        );
    }
    s.segments.insert(
        1,
        Segment {
            bytes: vec![],
            kernel_readable: true,
        },
    );
    s.namespaces.insert(1, Namespace);
    for (id, object, rights) in [
        (
            1,
            CapObject::Task(2),
            CAP_DERIVE | CAP_TASK_SEND | CAP_TASK_DESTROY,
        ),
        (
            2,
            CapObject::Segment(1),
            CAP_DERIVE | CAP_SEG_MAP_R | CAP_SEG_MAP_W,
        ),
        (
            3,
            CapObject::Namespace(1),
            CAP_DERIVE
                | CAP_NS_BIND
                | CAP_NS_UNBIND
                | CAP_NS_RESOLVE
                | CAP_NS_DESTROY
                | CAP_NS_MOUNT
                | CAP_NS_UNMOUNT,
        ),
        (
            4,
            CapObject::Task(3),
            CAP_DERIVE | CAP_TASK_SEND | CAP_TASK_DESTROY,
        ),
    ] {
        s.insert_capability(
            id,
            Capability {
                owner: 1,
                object,
                rights,
                active: true,
                parent: None,
            },
        );
        s.push_audit(
            Actor::Kernel,
            AuditKind::CapabilityMinted { cap: id, holder: 1 },
        );
    }
    assert!(locations_well_formed(&s));
    s
}

fn unchanged<T: std::fmt::Debug + PartialEq>(
    s: &mut State,
    error: T,
    f: impl FnOnce(&mut State) -> T,
) {
    let before = s.clone();
    assert_eq!(f(s), error);
    assert_eq!(*s, before);
}

#[test]
fn derive_checks_authority_rights_and_id_exhaustion_atomically() {
    let mut s = fixture();
    unchanged(&mut s, Err(AuthorityError::NotHeld), |s| {
        cap_derive(s, 2, 2, CAP_SEG_MAP_R)
    });
    unchanged(&mut s, Err(AuthorityError::InvalidCap), |s| {
        cap_derive(s, 1, 99, 0)
    });
    unchanged(&mut s, Err(AuthorityError::RightsExceeded), |s| {
        cap_derive(s, 1, 2, CAP_SEG_MAP_X)
    });
    let cap = cap_derive(&mut s, 1, 2, CAP_SEG_MAP_R).unwrap();
    assert_eq!(s.capabilities[&cap].parent, Some(2));
    assert_eq!(s.capabilities[&cap].rights, CAP_SEG_MAP_R);
    unchanged(&mut s, Err(AuthorityError::MissingRight), |s| {
        cap_derive(s, 1, cap, 0)
    });
    let empty = cap_derive(&mut s, 1, 2, 0).unwrap();
    assert!(s.held(1, empty));
    assert!(locations_well_formed(&s));
    s.next_cap = u64::from(u32::MAX);
    assert_eq!(cap_derive(&mut s, 1, 2, 0), Ok(u32::MAX));
    unchanged(&mut s, Err(AuthorityError::IdExhausted), |s| {
        cap_derive(s, 1, 2, 0)
    });
}

#[test]
fn queued_send_revokes_old_holder_and_fifo_recv_preserves_record() {
    let mut s = fixture();
    let record = s.capabilities[&2].clone();
    let a = send(&mut s, 1, 1, b"a", &[2]).unwrap();
    let b = send(&mut s, 1, 1, b"b", &[]).unwrap();
    assert_eq!(s.locations[&2], CapLocation::InMessage(a));
    assert_eq!(s.capabilities[&2], record);
    unchanged(&mut s, Err(AuthorityError::NotHeld), |s| {
        cap_derive(s, 1, 2, 0)
    });
    unchanged(&mut s, Err(AuthorityError::NotHeld), |s| cap_drop(s, 1, 2));
    assert!(locations_well_formed(&s));
    assert_eq!(recv(&mut s, 2).unwrap().id, a);
    assert!(s.held(2, 2));
    assert_eq!(s.capabilities[&2], record);
    let child = cap_derive(&mut s, 2, 2, CAP_SEG_MAP_R).unwrap();
    assert!(s.held(2, child));
    assert_eq!(recv(&mut s, 2).unwrap().id, b);
    assert!(locations_well_formed(&s));
}

#[test]
fn direct_send_completes_blocked_recv_without_message_container() {
    let mut s = fixture();
    assert_eq!(recv(&mut s, 2), None);
    let id = send(&mut s, 1, 1, b"direct", &[2]).unwrap();
    assert_eq!(s.tasks[&2].status, TaskStatus::Ready);
    assert_eq!(s.pending_returns[&2].id, id);
    assert!(s.messages.is_empty());
    assert!(s.held(2, 2));
    assert!(s.audit.iter().any(|e| e.kind
        == AuditKind::CapabilityMoved {
            cap: 2,
            from: CapLocation::HeldBy(1),
            to: CapLocation::HeldBy(2),
            message: Some(id),
        }));
    let sent = s
        .audit
        .iter()
        .position(|e| matches!(e.kind, AuditKind::MessageSent { message, .. } if message == id))
        .unwrap();
    let received = s
        .audit
        .iter()
        .position(|e| matches!(e.kind, AuditKind::MessageReceived { message, .. } if message == id))
        .unwrap();
    assert!(sent < received);
    assert!(locations_well_formed(&s));
}

#[test]
fn send_rejects_duplicate_structural_stale_and_invalid_caps_without_partial_moves() {
    let mut s = fixture();
    unchanged(&mut s, Err(AuthorityError::DuplicateCap), |s| {
        send(s, 1, 1, &[], &[2, 2])
    });
    unchanged(&mut s, Err(AuthorityError::InvalidCap), |s| {
        send(s, 1, 1, &[], &[2, 99])
    });
    unchanged(&mut s, Err(AuthorityError::MsgTooLarge), |s| {
        send(s, 1, 1, &[0; MAX_MSG_SIZE + 1], &[])
    });
    unchanged(&mut s, Err(AuthorityError::TooManyCaps), |s| {
        send(s, 1, 1, &[], &[2; MAX_CAPS_PER_MSG + 1])
    });
    s.tasks.get_mut(&1).unwrap().root_ns_cap = 3;
    unchanged(&mut s, Err(AuthorityError::StructuralCap), |s| {
        send(s, 1, 1, &[], &[3])
    });
    unchanged(&mut s, Err(AuthorityError::StructuralCap), |s| {
        ns_bind(s, 1, 3, "root", 3)
    });
    unchanged(&mut s, Err(AuthorityError::StructuralCap), |s| {
        cap_drop(s, 1, 3)
    });
    unchanged(&mut s, Err(AuthorityError::StructuralCap), |s| {
        ns_destroy(s, 1, 3)
    });
    s.segments.remove(&1);
    unchanged(&mut s, Err(AuthorityError::TargetDead), |s| {
        send(s, 1, 1, &[], &[2])
    });
    s.next_message = u64::MAX;
    unchanged(&mut s, Err(AuthorityError::IdExhausted), |s| {
        send(s, 1, 1, &[], &[])
    });
}

#[test]
fn namespace_bound_carrier_resolves_derived_authority_and_recovers_stale_tokens() {
    let mut s = fixture();
    ns_bind(&mut s, 1, 3, "data/item", 2).unwrap();
    assert_eq!(
        s.locations[&2],
        CapLocation::InNamespace(1, "data/item".into())
    );
    unchanged(&mut s, Err(AuthorityError::NotHeld), |s| {
        cap_derive(s, 1, 2, 0)
    });
    unchanged(&mut s, Err(AuthorityError::NotDirectory), |s| {
        ns_resolve(s, 1, 3, "data/item/child", 0)
    });
    let child = ns_resolve(&mut s, 1, 3, "data/item", CAP_SEG_MAP_R).unwrap();
    assert_eq!(s.capabilities[&child].parent, Some(2));
    assert!(locations_well_formed(&s));
    s.segments.remove(&1);
    unchanged(&mut s, Err(AuthorityError::TargetDead), |s| {
        ns_resolve(s, 1, 3, "data/item", 0)
    });
    assert_eq!(ns_unbind(&mut s, 1, 3, "data/item"), Ok(2));
    cap_drop(&mut s, 1, 2).unwrap();
    assert!(!s.locations.contains_key(&2));
    let before = s.clone();
    let AuditTrace { chain, events } = audit(&s, 1, child).unwrap();
    assert_eq!(
        chain.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [2, child]
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e.kind, AuditKind::CapabilityDropped { cap: 2, .. }))
    );
    assert_eq!(s, before);
    assert!(locations_well_formed(&s));
}

#[test]
fn namespace_paths_reject_empty_noncanonical_and_terminal_overlap_atomically() {
    let mut s = fixture();
    for path in ["", "/a", "a/", "a//b", ".", "a/..", "a\0b"] {
        unchanged(&mut s, Err(AuthorityError::InvalidPath), |s| {
            ns_bind(s, 1, 3, path, 2)
        });
    }
    ns_bind(&mut s, 1, 3, "a/b", 2).unwrap();
    let child = ns_resolve(&mut s, 1, 3, "a/b", 0).unwrap();
    for path in ["a", "a/b", "a/b/c"] {
        unchanged(&mut s, Err(AuthorityError::Overlap), |s| {
            ns_bind(s, 1, 3, path, child)
        });
    }
    ns_bind(&mut s, 1, 3, "a/bc", child).unwrap();
    assert!(locations_well_formed(&s));
    ns_destroy(&mut s, 1, 3).unwrap();
    assert!(!s.locations.contains_key(&2));
    assert!(!s.locations.contains_key(&child));
    assert!(s.capabilities.contains_key(&2));
    cap_drop(&mut s, 1, 3).unwrap();
    assert!(locations_well_formed(&s));
}

#[test]
fn last_subscription_authority_lost_by_move_or_drop_unsubscribes_once() {
    let mut s = fixture();
    s.exc_sources.insert(
        1,
        ExceptionSource {
            supported_kind: ExceptionKind::DivideError,
            additional_kinds: vec![],
            active: true,
        },
    );
    s.insert_capability(
        5,
        Capability {
            owner: 1,
            object: CapObject::ExceptionSource(1),
            rights: CAP_DERIVE | CAP_EXC_SUBSCRIBE,
            active: true,
            parent: None,
        },
    );
    s.subscriptions.insert((1, 1));
    let second = cap_derive(&mut s, 1, 5, CAP_EXC_SUBSCRIBE).unwrap();
    cap_drop(&mut s, 1, 5).unwrap();
    assert!(s.subscriptions.contains(&(1, 1)));
    send(&mut s, 1, 1, &[], &[second]).unwrap();
    assert!(!s.subscriptions.contains(&(1, 1)));
    assert_eq!(
        s.audit
            .iter()
            .filter(|e| matches!(
                e.kind,
                AuditKind::ExceptionUnsubscribed { task: 1, source: 1 }
            ))
            .count(),
        1
    );
    assert!(locations_well_formed(&s));
}

#[test]
fn sender_exit_preserves_queued_and_delivered_tokens_but_receiver_exit_drops_them() {
    let mut s = fixture();
    let a = cap_derive(&mut s, 1, 2, CAP_SEG_MAP_R).unwrap();
    let b = cap_derive(&mut s, 1, 2, CAP_SEG_MAP_R).unwrap();
    recv(&mut s, 2);
    send(&mut s, 1, 1, &[], &[a]).unwrap();
    send(&mut s, 1, 4, &[], &[b]).unwrap();
    retire_task(&mut s, 1, Actor::Kernel);
    assert!(s.held(2, a));
    assert!(matches!(s.locations[&b], CapLocation::InMessage(_)));
    assert!(s.capabilities.contains_key(&b));
    assert_eq!(s.messages.len(), 1);
    assert_eq!(recv(&mut s, 3).unwrap().caps, [b]);
    assert!(s.held(3, b));
    assert!(locations_well_formed(&s));
    // Recipient exit drops pending delivered capabilities, not unrelated holders.
    retire_task(&mut s, 2, Actor::Kernel);
    assert!(!s.locations.contains_key(&a));
    assert!(locations_well_formed(&s));
}

#[test]
fn invariant_rejects_dangling_duplicate_and_mismatched_carriers() {
    let s = fixture();
    let mut bad = s.clone();
    bad.locations.insert(99, CapLocation::HeldBy(1));
    assert!(!locations_well_formed(&bad));
    let mut queued = s.clone();
    let id = send(&mut queued, 1, 1, &[], &[2]).unwrap();
    let mut bad = queued.clone();
    bad.messages.get_mut(&id).unwrap().caps.push(2);
    assert!(!locations_well_formed(&bad));
    let mut bad = queued.clone();
    bad.mailboxes.get_mut(&2).unwrap().push_back(id);
    assert!(!locations_well_formed(&bad));
    queued.locations.insert(2, CapLocation::HeldBy(1));
    assert!(!locations_well_formed(&queued));
    let mut bad = s.clone();
    bad.bindings.insert((1, "a".into()), NsEntry::Bound(2));
    assert!(!locations_well_formed(&bad));
    let mut bad = s;
    let child = cap_derive(&mut bad, 1, 2, 0).unwrap();
    bad.capabilities.get_mut(&child).unwrap().rights = CAP_SEG_MAP_X;
    assert!(!locations_well_formed(&bad));
}

#[test]
fn provenance_replay_rejects_missing_events_or_forged_source_locations() {
    let mut s = fixture();
    let child = cap_derive(&mut s, 1, 2, CAP_SEG_MAP_R).unwrap();
    cap_drop(&mut s, 1, 2).unwrap();
    send(&mut s, 1, 1, &[], &[child]).unwrap();
    recv(&mut s, 2);
    assert!(provenance_well_formed(&s));
    let mut bad = s.clone();
    bad.audit
        .retain(|e| !matches!(e.kind, AuditKind::CapabilityMoved { cap, .. } if cap == child));
    assert!(!provenance_well_formed(&bad));
    let mut bad = s;
    let event = bad
        .audit
        .iter_mut()
        .find(|e| matches!(e.kind, AuditKind::CapabilityMoved { .. }))
        .unwrap();
    if let AuditKind::CapabilityMoved { from, .. } = &mut event.kind {
        *from = CapLocation::HeldBy(3);
    }
    assert!(!provenance_well_formed(&bad));
}

fn add_namespace(s: &mut State, ns: NsId) -> CapId {
    s.namespaces.insert(ns, Namespace);
    let id = s.next_cap as CapId;
    s.insert_capability(
        id,
        Capability {
            owner: 1,
            object: CapObject::Namespace(ns),
            rights: s.capabilities[&3].rights,
            active: true,
            parent: None,
        },
    );
    s.push_audit(
        Actor::Kernel,
        AuditKind::CapabilityMinted { cap: id, holder: 1 },
    );
    id
}

#[test]
fn mount_resolution_records_path_and_unmount_preserves_derived_children() {
    let mut s = fixture();
    let source = add_namespace(&mut s, 2);
    ns_bind(&mut s, 1, source, "data", 2).unwrap();
    ns_mount(&mut s, 1, 3, "mnt", source).unwrap();
    let mount = s.bindings[&(1, "mnt".into())].cap();
    let child = ns_resolve(&mut s, 1, 3, "mnt/data", CAP_SEG_MAP_R).unwrap();
    assert_eq!(s.capabilities[&child].parent, Some(2));
    let root = ns_resolve(&mut s, 1, 3, "mnt", CAP_NS_RESOLVE).unwrap();
    assert_eq!(s.capabilities[&root].parent, Some(mount));
    assert!(s.audit.iter().any(|e| matches!(&e.kind, AuditKind::NamespaceResolved { namespace_cap: 3, mounts, cap, .. } if mounts == &[mount] && *cap == child)));
    unchanged(&mut s, Err(AuthorityError::NotFound), |s| {
        ns_unbind(s, 1, 3, "mnt")
    });
    assert!(provenance_well_formed(&s));
    ns_unmount(&mut s, 1, 3, "mnt").unwrap();
    assert!(!s.locations.contains_key(&mount));
    assert!(s.held(1, child) && s.held(1, root));
    assert_eq!(
        audit(&s, 1, root)
            .unwrap()
            .chain
            .iter()
            .map(|(id, _)| *id)
            .collect::<Vec<_>>(),
        [source, mount, root]
    );
    assert!(provenance_well_formed(&s));
}

#[test]
fn mount_cycle_rejection_is_atomic_and_destroyed_target_can_be_unmounted() {
    let mut s = fixture();
    let second = add_namespace(&mut s, 2);
    let third = add_namespace(&mut s, 3);
    ns_mount(&mut s, 1, 3, "two", second).unwrap();
    ns_mount(&mut s, 1, second, "three", third).unwrap();
    unchanged(&mut s, Err(AuthorityError::MountCycle), |s| {
        ns_mount(s, 1, third, "one", 3)
    });
    unchanged(&mut s, Err(AuthorityError::MountCycle), |s| {
        ns_mount(s, 1, 3, "self", 3)
    });
    ns_mount(&mut s, 1, 3, "also-three", third).unwrap();
    assert!(provenance_well_formed(&s));
    ns_destroy(&mut s, 1, third).unwrap();
    unchanged(&mut s, Err(AuthorityError::MountTargetDead), |s| {
        ns_resolve(s, 1, 3, "two/three", 0)
    });
    ns_unmount(&mut s, 1, second, "three").unwrap();
    ns_unmount(&mut s, 1, 3, "also-three").unwrap();
    assert!(provenance_well_formed(&s));
}

#[test]
fn namespace_delegation_and_component_limits_are_enforced() {
    let mut s = fixture();
    let ns = add_namespace(&mut s, 2);
    let read_only = cap_derive(&mut s, 1, ns, CAP_NS_RESOLVE).unwrap();
    unchanged(&mut s, Err(AuthorityError::NotDelegable), |s| {
        ns_mount(s, 1, 3, "mnt", read_only)
    });
    let data = cap_derive(&mut s, 1, 2, CAP_SEG_MAP_R).unwrap();
    ns_bind(&mut s, 1, 3, "nondelegable", data).unwrap();
    unchanged(&mut s, Err(AuthorityError::NotDelegable), |s| {
        ns_resolve(s, 1, 3, "nondelegable", 0)
    });
    for path in [
        "x".repeat(256),
        "非ASCII".into(),
        ["x"; MAX_PATH_COMPONENTS + 1].join("/"),
    ] {
        unchanged(&mut s, Err(AuthorityError::InvalidPath), |s| {
            ns_bind(s, 1, 3, &path, 2)
        });
    }
    ns_bind(&mut s, 1, 3, &"x".repeat(255), 2).unwrap();
    assert!(provenance_well_formed(&s));
}

#[test]
fn controlled_fork_inherits_non_derivable_tokens_and_rebinds_self_without_cdt_target_mismatch() {
    let mut s = fixture();
    let self_cap = s.next_cap as CapId;
    s.insert_capability(
        self_cap,
        Capability {
            owner: 1,
            object: CapObject::Task(1),
            rights: CAP_TASK_SEND,
            active: true,
            parent: None,
        },
    );
    s.push_audit(
        Actor::Kernel,
        AuditKind::CapabilityMinted {
            cap: self_cap,
            holder: 1,
        },
    );
    s.tasks.get_mut(&1).unwrap().root_ns_cap = 3;
    s.tasks.insert(
        4,
        Task {
            id: 4,
            status: TaskStatus::Ready,
            vspace: vec![],
            root_ns_cap: 0,
            kernel_resume_frame: None,
        },
    );
    inherit_fork_caps(&mut s, 1, 4, self_cap).unwrap();
    let copied = s
        .audit
        .iter()
        .find_map(|e| match e.kind {
            AuditKind::CapabilityForkRebound {
                cap,
                source,
                child: 4,
            } if source == self_cap => Some(cap),
            _ => None,
        })
        .unwrap();
    assert_eq!(s.capabilities[&copied].object, CapObject::Task(4));
    assert_eq!(s.capabilities[&copied].rights, CAP_TASK_SEND);
    assert_ne!(s.capabilities[&copied].parent, Some(self_cap));
    assert!(s.held(4, s.tasks[&4].root_ns_cap));
    cap_drop(&mut s, 4, copied).unwrap();
    assert!(s.held(1, self_cap));
    assert!(provenance_well_formed(&s));
}
