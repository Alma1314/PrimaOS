//! D14 bounded, single-threaded feasibility experiment using existing COSIR
//! authority operations. This is not two real LibOS binaries or a QEMU witness.
use cosir::{authority::*, state::*};

fn mint(s: &mut State, owner: TaskId, object: CapObject, rights: u32) -> CapId {
    let id = s.next_cap.max(1) as CapId;
    s.insert_capability(
        id,
        Capability {
            owner,
            object,
            rights,
            active: true,
            parent: None,
        },
    );
    s.push_audit(
        Actor::Kernel,
        AuditKind::CapabilityMinted {
            cap: id,
            holder: owner,
        },
    );
    id
}
fn log(s: &mut State, task: TaskId, root: CapId, data: &[u8]) {
    let cap = ns_resolve(s, task, root, "log", CAP_TASK_SEND).unwrap();
    send(s, task, cap, data, &[]).unwrap();
    let received = recv(s, 3).unwrap();
    assert_eq!(received.data, data);
    assert_eq!(received.origin, MessageOrigin::Task(task));
    cap_drop(s, task, cap).unwrap();
    s.tasks.get_mut(&3).unwrap().status = TaskStatus::Running;
}
fn main() {
    let mut s = State::default();
    for id in 1..=4 {
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
    let mut roots = [0; 2];
    for task in 1..=2 {
        s.namespaces.insert(task, Namespace);
        let root = mint(
            &mut s,
            task,
            CapObject::Namespace(task),
            CAP_DERIVE | CAP_NS_RESOLVE | CAP_NS_BIND,
        );
        roots[(task - 1) as usize] = root;
        s.tasks.get_mut(&task).unwrap().root_ns_cap = root;
        s.segments.insert(
            task,
            Segment {
                bytes: vec![task as u8; 4096],
                kernel_readable: true,
            },
        );
        s.tasks.get_mut(&task).unwrap().vspace.push(PageMapping {
            base: 0x1000,
            len: 4096,
            access: ACCESS_R | ACCESS_W,
            user: true,
            backing: Some(PageMappingBacking {
                segment: task,
                offset: 0,
                frame_phys: u64::from(task) * 4096,
            }),
        });
        let private = mint(
            &mut s,
            task,
            CapObject::Segment(task),
            CAP_DERIVE | CAP_SEG_MAP_R,
        );
        ns_bind(&mut s, task, root, "private", private).unwrap();
        let logging = mint(&mut s, task, CapObject::Task(3), CAP_DERIVE | CAP_TASK_SEND);
        ns_bind(&mut s, task, root, "log", logging).unwrap();
    }
    let kill = mint(&mut s, 4, CapObject::Task(1), CAP_TASK_DESTROY);
    assert_ne!(s.tasks[&1].root_ns_cap, s.tasks[&2].root_ns_cap);
    assert_ne!(s.tasks[&1].vspace[0].backing, s.tasks[&2].vspace[0].backing);
    log(&mut s, 1, roots[0], b"domain-a");
    log(&mut s, 2, roots[1], b"domain-b");
    let before = s.clone();
    assert_eq!(
        ns_resolve(&mut s, 2, roots[0], "private", CAP_SEG_MAP_R),
        Err(AuthorityError::NotHeld)
    );
    assert_eq!(s, before);
    println!("DENIED actor=2 foreign_root={} result=NotHeld", roots[0]);
    // A service adapter maps NotHeld to EACCES; frozen COSIR has no Linux errno.
    println!("PASS roots=2 private_spaces=2 shared_log=2 cross_domain=NotHeld adapter=EACCES");
    task_destroy(&mut s, 4, kill).unwrap();
    assert!(!s.tasks.contains_key(&1));
    assert_eq!(s.tasks[&2], before.tasks[&2]);
    assert_eq!(s.segments[&2], before.segments[&2]);
    log(&mut s, 2, roots[1], b"domain-b-after-a-crash");
    assert!(locations_well_formed(&s));
    assert!(provenance_well_formed(&s));
    let sent: Vec<_> = s
        .audit
        .iter()
        .filter_map(|e| match &e.kind {
            AuditKind::MessageSent {
                receiver: 3, data, ..
            } => Some(data.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        sent,
        vec![
            b"domain-a".to_vec(),
            b"domain-b".to_vec(),
            b"domain-b-after-a-crash".to_vec()
        ]
    );
    println!("PASS one_side_destroyed survivor_log=1 audit_replay=3 existing_mechanisms_only");
}
