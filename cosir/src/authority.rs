//! Capability token locations, bounded local carriers and append-only provenance.
//!
//! This executable host slice implements §4/§9 and local Namespace entries. It does
//! not model remote nodes, timers or kernel/hardware refinement.
use crate::state::*;

mod events;
mod remote;
mod wait;
pub use events::*;
pub use remote::*;
use std::collections::HashSet;
pub use wait::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityError {
    InvalidCap,
    NotHeld,
    TargetDead,
    MissingRight,
    RightsExceeded,
    WrongRightType,
    StructuralCap,
    InvalidTask,
    InvalidNamespace,
    InvalidPath,
    Overlap,
    NotFound,
    NotDirectory,
    NotDelegable,
    MountCycle,
    MountTargetDead,
    DuplicateCap,
    MsgTooLarge,
    TooManyCaps,
    IdExhausted,
    InvalidTimer,
    InvalidTime,
    InvalidExcSource,
    AlreadySubscribed,
    NotSubscribed,
    InvalidNode,
    AlreadyConnected,
    NodeUnreachable,
    NonPortableCap,
    InvalidWaitObject,
    EmptyWaitSet,
    LengthMismatch,
    DuplicateWaitObject,
    InvalidGeneration,
    InvalidKind,
    KindConflict,
}

pub const MAX_MSG_SIZE: usize = 64;
pub const MAX_CAPS_PER_MSG: usize = 4;
pub const MAX_PATH_COMPONENTS: usize = 16;

pub fn target_live(s: &State, object: &CapObject) -> bool {
    match object {
        CapObject::Task(t) => s.tasks.contains_key(t),
        CapObject::Segment(seg) => s.segments.contains_key(seg),
        CapObject::Namespace(ns) => s.namespaces.contains_key(ns),
        CapObject::ExceptionSource(source) => s.exc_sources.get(source).is_some_and(|v| v.active),
        CapObject::Timer(timer) => s.timers.contains_key(timer),
        CapObject::Node(node) => s.nodes.contains_key(node),
        CapObject::WaitObject(object) => s.wait_objects.contains_key(object),
    }
}

fn valid_rights(object: &CapObject) -> u32 {
    CAP_DERIVE
        | match object {
            CapObject::Task(_) => CAP_TASK_SEND | CAP_TASK_DESTROY | CAP_TASK_MANAGE_VSPACE,
            CapObject::Segment(_) => {
                CAP_SEG_MAP_R | CAP_SEG_MAP_W | CAP_SEG_MAP_X | CAP_SEG_DESTROY
            }
            CapObject::Namespace(_) => {
                CAP_NS_RESOLVE
                    | CAP_NS_BIND
                    | CAP_NS_UNBIND
                    | CAP_NS_MOUNT
                    | CAP_NS_UNMOUNT
                    | CAP_NS_DESTROY
            }
            CapObject::ExceptionSource(_) => CAP_EXC_SUBSCRIBE | CAP_EXC_SOURCE_DESTROY,
            CapObject::Timer(_) => CAP_TIMER_CANCEL | CAP_TIMER_DESTROY,
            CapObject::Node(_) => CAP_NODE_CONNECT | CAP_NODE_DESTROY,
            CapObject::WaitObject(_) => CAP_WAIT_WAIT | CAP_WAIT_SIGNAL | CAP_WAIT_DESTROY,
        }
}

fn held_cap(s: &State, caller: TaskId, id: CapId) -> Result<&Capability, AuthorityError> {
    let cap = s.capabilities.get(&id).ok_or(AuthorityError::InvalidCap)?;
    if !s.held(caller, id) {
        return Err(AuthorityError::NotHeld);
    }
    Ok(cap)
}

fn authorize(
    s: &State,
    caller: TaskId,
    id: CapId,
    right: u32,
) -> Result<&Capability, AuthorityError> {
    let cap = held_cap(s, caller, id)?;
    if !cap.active || !target_live(s, &cap.object) {
        return Err(AuthorityError::TargetDead);
    }
    if cap.rights & right != right {
        return Err(AuthorityError::MissingRight);
    }
    Ok(cap)
}

fn structural(s: &State, id: CapId) -> bool {
    s.tasks.values().any(|task| task.root_ns_cap == id)
}

fn fresh_cap(s: &State) -> Result<CapId, AuthorityError> {
    let id = u32::try_from(s.next_cap.max(1)).map_err(|_| AuthorityError::IdExhausted)?;
    if s.capabilities.contains_key(&id) {
        return Err(AuthorityError::IdExhausted);
    }
    Ok(id)
}

/// User-facing §4.4. The explicit-ID CDT helper is deliberately separate.
pub fn cap_derive(
    s: &mut State,
    caller: TaskId,
    parent: CapId,
    rights: u32,
) -> Result<CapId, AuthorityError> {
    let cap = authorize(s, caller, parent, CAP_DERIVE)?;
    if rights & !cap.rights != 0 {
        return Err(AuthorityError::RightsExceeded);
    }
    if rights & !valid_rights(&cap.object) != 0 {
        return Err(AuthorityError::WrongRightType);
    }
    let id = fresh_cap(s)?;
    let child = Capability {
        owner: caller,
        object: cap.object.clone(),
        rights,
        active: true,
        parent: Some(parent),
    };
    s.insert_capability(id, child);
    s.push_audit(
        Actor::Task(caller),
        AuditKind::CapabilityDerived {
            cap: id,
            parent,
            location: CapLocation::HeldBy(caller),
        },
    );
    Ok(id)
}

/// §4.8: caller's held authority is copied only into a newly created, empty
/// child Task. Memory/COW and Process creation belong to their own model slice.
pub fn inherit_fork_caps(
    s: &mut State,
    caller: TaskId,
    child: TaskId,
    self_cap: CapId,
) -> Result<(), AuthorityError> {
    if self_cap != 0
        && !s
            .capabilities
            .get(&self_cap)
            .is_some_and(|cap| cap.object == CapObject::Task(caller) && s.held(caller, self_cap))
    {
        return Err(AuthorityError::InvalidCap);
    }
    if caller == child
        || !s.tasks.contains_key(&caller)
        || !s.tasks.get(&child).is_some_and(|t| t.root_ns_cap == 0)
        || s.locations
            .values()
            .any(|l| *l == CapLocation::HeldBy(child))
    {
        return Err(AuthorityError::InvalidTask);
    }
    let mut sources: Vec<_> = s
        .capabilities
        .keys()
        .copied()
        .filter(|id| s.held(caller, *id))
        .collect();
    sources.sort_unstable();
    let next = s.next_cap.max(1);
    if next
        .checked_add(sources.len() as u64)
        .is_none_or(|last| last > u64::from(u32::MAX))
    {
        return Err(AuthorityError::IdExhausted);
    }
    let root = fresh_cap(s)?;
    s.insert_capability(
        root,
        Capability {
            owner: child,
            object: CapObject::Task(child),
            rights: CAP_DERIVE | CAP_TASK_SEND | CAP_TASK_MANAGE_VSPACE | CAP_TASK_DESTROY,
            active: true,
            parent: None,
        },
    );
    s.push_audit(
        Actor::Kernel,
        AuditKind::CapabilityMinted {
            cap: root,
            holder: child,
        },
    );
    let root_ns = s.tasks[&caller].root_ns_cap;
    for source in sources {
        let original = s.capabilities[&source].clone();
        let rebound = source == self_cap;
        let id = fresh_cap(s).expect("fork ID preflight");
        s.insert_capability(
            id,
            Capability {
                owner: child,
                object: if rebound {
                    CapObject::Task(child)
                } else {
                    original.object
                },
                rights: original.rights,
                active: original.active,
                parent: Some(if rebound { root } else { source }),
            },
        );
        if rebound {
            s.push_audit(
                Actor::Kernel,
                AuditKind::CapabilityDerived {
                    cap: id,
                    parent: root,
                    location: CapLocation::HeldBy(child),
                },
            );
            s.push_audit(
                Actor::Kernel,
                AuditKind::CapabilityForkRebound {
                    cap: id,
                    source,
                    child,
                },
            );
        } else {
            s.push_audit(
                Actor::Kernel,
                AuditKind::CapabilityForkInherited {
                    cap: id,
                    source,
                    child,
                },
            );
        }
        if source == root_ns {
            s.tasks.get_mut(&child).unwrap().root_ns_cap = id;
        }
    }
    remove_location(s, root, Actor::Kernel);
    Ok(())
}

fn unsubscribe_if_last(s: &mut State, task: TaskId, cap: CapId, actor: Actor) {
    let Some(Capability {
        object: CapObject::ExceptionSource(source),
        rights,
        ..
    }) = s.capabilities.get(&cap)
    else {
        return;
    };
    let source = *source;
    if rights & CAP_EXC_SUBSCRIBE != 0
        && s.subscriptions.contains(&(task, source))
        && !s.capabilities.iter().any(|(&id, c)| {
            s.held(task, id)
                && c.active
                && c.object == CapObject::ExceptionSource(source)
                && c.rights & CAP_EXC_SUBSCRIBE != 0
        })
    {
        s.subscriptions.remove(&(task, source));
        s.push_audit(actor, AuditKind::ExceptionUnsubscribed { task, source });
    }
}

fn relocate(s: &mut State, cap: CapId, to: CapLocation, message: Option<u64>, actor: Actor) {
    let from = s
        .locations
        .insert(cap, to.clone())
        .expect("validated token location");
    s.push_audit(
        actor,
        AuditKind::CapabilityMoved {
            cap,
            from: from.clone(),
            to,
            message,
        },
    );
    if let CapLocation::HeldBy(task) = from {
        unsubscribe_if_last(s, task, cap, actor);
    }
}

fn remove_location(s: &mut State, cap: CapId, actor: Actor) {
    if let Some(from) = s.locations.remove(&cap) {
        s.push_audit(
            actor,
            AuditKind::CapabilityDropped {
                cap,
                from: from.clone(),
            },
        );
        if let CapLocation::HeldBy(task) = from {
            unsubscribe_if_last(s, task, cap, actor);
        }
    }
}

pub fn cap_drop(s: &mut State, caller: TaskId, cap: CapId) -> Result<(), AuthorityError> {
    held_cap(s, caller, cap)?;
    if structural(s, cap) {
        return Err(AuthorityError::StructuralCap);
    }
    remove_location(s, cap, Actor::Task(caller));
    Ok(())
}

/// §9.1: preflight all inputs before allocating a message or moving any token.
pub fn send(
    s: &mut State,
    caller: TaskId,
    task_cap: CapId,
    data: &[u8],
    caps: &[CapId],
) -> Result<u64, AuthorityError> {
    let target = authorize(s, caller, task_cap, CAP_TASK_SEND)?;
    let CapObject::Task(receiver) = target.object else {
        return Err(AuthorityError::InvalidTask);
    };
    if data.len() > MAX_MSG_SIZE {
        return Err(AuthorityError::MsgTooLarge);
    }
    if let Some(remote) = s.remote_tasks.get(&receiver).copied() {
        if !caps.is_empty() {
            return Err(AuthorityError::NonPortableCap);
        }
        return remote_send(s, caller, receiver, remote, data);
    }
    if caps.len() > MAX_CAPS_PER_MSG {
        return Err(AuthorityError::TooManyCaps);
    }
    let mut seen = HashSet::new();
    for &cap in caps {
        if !seen.insert(cap) {
            return Err(AuthorityError::DuplicateCap);
        }
        authorize(s, caller, cap, 0)?;
        if structural(s, cap) {
            return Err(AuthorityError::StructuralCap);
        }
    }
    check_message_ids(s, 1)?;
    Ok(enqueue_or_deliver(
        s,
        MessageOrigin::Task(caller),
        receiver,
        data,
        caps,
        Actor::Task(caller),
        None,
    ))
}

fn check_message_ids(s: &State, count: u64) -> Result<(), AuthorityError> {
    s.next_message
        .max(1)
        .checked_add(count)
        .ok_or(AuthorityError::IdExhausted)?;
    Ok(())
}

/// Shared local delivery for user messages, Timer and exception notifications.
/// Caller preflights message IDs, target, payload, and all capability moves.
fn enqueue_or_deliver(
    s: &mut State,
    origin: MessageOrigin,
    receiver: TaskId,
    data: &[u8],
    caps: &[CapId],
    actor: Actor,
    transport_message: Option<u64>,
) -> u64 {
    assert!(s.tasks.contains_key(&receiver) && !s.remote_tasks.contains_key(&receiver));
    let id = s.next_message.max(1);
    let message = Message {
        id,
        origin: origin.clone(),
        receiver,
        data: data.to_vec(),
        caps: caps.to_vec(),
    };
    s.next_message = id + 1;
    s.push_audit(
        actor,
        AuditKind::MessageSent {
            message: id,
            origin,
            receiver,
            data: data.to_vec(),
            caps: caps.to_vec(),
            transport_message,
        },
    );
    let direct = s.tasks[&receiver].status == TaskStatus::BlockedRecv;
    let destination = if direct {
        CapLocation::HeldBy(receiver)
    } else {
        CapLocation::InMessage(id)
    };
    for &cap in caps {
        relocate(s, cap, destination.clone(), Some(id), actor);
    }
    if direct {
        s.pending_returns.insert(receiver, message);
        s.tasks.get_mut(&receiver).unwrap().status = TaskStatus::Ready;
        s.push_audit(
            Actor::Kernel,
            AuditKind::TaskStateChanged {
                task: receiver,
                from: TaskStatus::BlockedRecv,
                to: TaskStatus::Ready,
            },
        );
        s.push_audit(
            Actor::Task(receiver),
            AuditKind::MessageReceived {
                message: id,
                receiver,
            },
        );
    } else {
        s.messages.insert(id, message);
        s.mailboxes.entry(receiver).or_default().push_back(id);
    }
    id
}

/// §9.2: caller is a Running CPU Task, guaranteed by the model harness (§6.1).
/// None denotes a blocked call; pending_returns stores its eventual completion.
pub fn recv(s: &mut State, caller: TaskId) -> Option<Message> {
    assert!(!s.remote_tasks.contains_key(&caller));
    assert_eq!(s.tasks[&caller].status, TaskStatus::Running);
    assert!(!s.pending_returns.contains_key(&caller));
    if let Some(id) = s.mailboxes.get_mut(&caller).and_then(|q| q.pop_front()) {
        let message = s.messages.remove(&id).expect("mailbox message exists");
        for &cap in &message.caps {
            relocate(
                s,
                cap,
                CapLocation::HeldBy(caller),
                Some(id),
                Actor::Task(caller),
            );
        }
        s.push_audit(
            Actor::Task(caller),
            AuditKind::MessageReceived {
                message: id,
                receiver: caller,
            },
        );
        Some(message)
    } else {
        s.tasks.get_mut(&caller).unwrap().status = TaskStatus::BlockedRecv;
        s.push_audit(
            Actor::Task(caller),
            AuditKind::TaskStateChanged {
                task: caller,
                from: TaskStatus::Running,
                to: TaskStatus::BlockedRecv,
            },
        );
        None
    }
}

fn namespace(s: &State, caller: TaskId, cap: CapId, right: u32) -> Result<NsId, AuthorityError> {
    let cap = authorize(s, caller, cap, right)?;
    match cap.object {
        CapObject::Namespace(ns) => Ok(ns),
        _ => Err(AuthorityError::InvalidNamespace),
    }
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\0')
        && path.split('/').count() <= MAX_PATH_COMPONENTS
        && path
            .split('/')
            .all(|p| !p.is_empty() && p.len() <= 255 && p.is_ascii() && p != "." && p != "..")
}

fn path_prefix(a: &str, b: &str) -> bool {
    b.strip_prefix(a)
        .is_some_and(|suffix| suffix.starts_with('/'))
}

pub fn ns_bind(
    s: &mut State,
    caller: TaskId,
    ns_cap: CapId,
    path: &str,
    cap: CapId,
) -> Result<(), AuthorityError> {
    let ns = namespace(s, caller, ns_cap, CAP_NS_BIND)?;
    if !valid_path(path) {
        return Err(AuthorityError::InvalidPath);
    }
    authorize(s, caller, cap, 0)?;
    if structural(s, cap) {
        return Err(AuthorityError::StructuralCap);
    }
    if s.bindings
        .keys()
        .any(|(n, p)| *n == ns && (p == path || path_prefix(p, path) || path_prefix(path, p)))
    {
        return Err(AuthorityError::Overlap);
    }
    s.bindings
        .insert((ns, path.to_owned()), NsEntry::Bound(cap));
    relocate(
        s,
        cap,
        CapLocation::InNamespace(ns, path.to_owned()),
        None,
        Actor::Task(caller),
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::NamespaceBound {
            namespace: ns,
            path: path.to_owned(),
            cap,
        },
    );
    Ok(())
}

pub fn ns_unbind(
    s: &mut State,
    caller: TaskId,
    ns_cap: CapId,
    path: &str,
) -> Result<CapId, AuthorityError> {
    let ns = namespace(s, caller, ns_cap, CAP_NS_UNBIND)?;
    if !valid_path(path) {
        return Err(AuthorityError::InvalidPath);
    }
    let key = (ns, path.to_owned());
    let Some(NsEntry::Bound(cap)) = s.bindings.get(&key).copied() else {
        return Err(AuthorityError::NotFound);
    };
    s.bindings.remove(&key);
    relocate(
        s,
        cap,
        CapLocation::HeldBy(caller),
        None,
        Actor::Task(caller),
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::NamespaceUnbound {
            namespace: ns,
            path: path.to_owned(),
            cap,
        },
    );
    Ok(cap)
}

fn path_overlap(s: &State, ns: NsId, path: &str) -> bool {
    s.bindings
        .keys()
        .any(|(n, p)| *n == ns && (p == path || path_prefix(p, path) || path_prefix(path, p)))
}

fn mount_target(s: &State, entry: NsEntry) -> Option<NsId> {
    let NsEntry::Mount(id) = entry else {
        return None;
    };
    let cap = s.capabilities.get(&id)?;
    let CapObject::Namespace(target) = cap.object else {
        return None;
    };
    (cap.active && s.namespaces.contains_key(&target)).then_some(target)
}

fn mount_reaches(s: &State, from: NsId, to: NsId) -> bool {
    let mut pending = vec![from];
    let mut seen = HashSet::new();
    while let Some(ns) = pending.pop() {
        if ns == to {
            return true;
        }
        if !seen.insert(ns) {
            continue;
        }
        for ((source, _), entry) in &s.bindings {
            if *source == ns
                && let Some(target) = mount_target(s, *entry)
            {
                pending.push(target);
            }
        }
    }
    false
}

pub fn ns_mount(
    s: &mut State,
    caller: TaskId,
    destination: CapId,
    path: &str,
    source: CapId,
) -> Result<(), AuthorityError> {
    let ns = namespace(s, caller, destination, CAP_NS_MOUNT)?;
    if !valid_path(path) {
        return Err(AuthorityError::InvalidPath);
    }
    let parent = authorize(s, caller, source, 0)?;
    let CapObject::Namespace(target) = parent.object else {
        return Err(AuthorityError::InvalidNamespace);
    };
    let rights = CAP_DERIVE | CAP_NS_RESOLVE;
    if parent.rights & rights != rights {
        return Err(AuthorityError::NotDelegable);
    }
    if path_overlap(s, ns, path) {
        return Err(AuthorityError::Overlap);
    }
    if mount_reaches(s, target, ns) {
        return Err(AuthorityError::MountCycle);
    }
    let cap = fresh_cap(s)?;
    let location = CapLocation::InNamespace(ns, path.to_owned());
    s.insert_capability(
        cap,
        Capability {
            owner: caller,
            object: CapObject::Namespace(target),
            rights,
            active: true,
            parent: Some(source),
        },
    );
    s.locations.insert(cap, location.clone());
    s.bindings
        .insert((ns, path.to_owned()), NsEntry::Mount(cap));
    s.push_audit(
        Actor::Task(caller),
        AuditKind::CapabilityDerived {
            cap,
            parent: source,
            location,
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::NamespaceMounted {
            namespace: ns,
            path: path.to_owned(),
            cap,
        },
    );
    Ok(())
}

pub fn ns_unmount(
    s: &mut State,
    caller: TaskId,
    ns_cap: CapId,
    path: &str,
) -> Result<(), AuthorityError> {
    let ns = namespace(s, caller, ns_cap, CAP_NS_UNMOUNT)?;
    if !valid_path(path) {
        return Err(AuthorityError::InvalidPath);
    }
    let key = (ns, path.to_owned());
    let Some(NsEntry::Mount(cap)) = s.bindings.get(&key).copied() else {
        return Err(AuthorityError::NotFound);
    };
    s.bindings.remove(&key);
    remove_location(s, cap, Actor::Task(caller));
    s.push_audit(
        Actor::Task(caller),
        AuditKind::NamespaceUnmounted {
            namespace: ns,
            path: path.to_owned(),
            cap,
        },
    );
    Ok(())
}

/// §11.7 consumes at least one path component per traversal step.
pub fn ns_resolve(
    s: &mut State,
    caller: TaskId,
    ns_cap: CapId,
    path: &str,
    rights: u32,
) -> Result<CapId, AuthorityError> {
    let start = namespace(s, caller, ns_cap, CAP_NS_RESOLVE)?;
    if !valid_path(path) {
        return Err(AuthorityError::InvalidPath);
    }
    let mut ns = start;
    let mut remaining = path;
    let mut mounts = Vec::new();
    let parent = loop {
        let terminal = s
            .bindings
            .iter()
            .find(|((n, p), _)| *n == ns && (p == remaining || path_prefix(p, remaining)));
        let Some(((.., prefix), &entry)) = terminal else {
            return Err(AuthorityError::NotFound);
        };
        let id = entry.cap();
        match entry {
            NsEntry::Bound(_) if prefix == remaining => break id,
            NsEntry::Bound(_) => return Err(AuthorityError::NotDirectory),
            NsEntry::Mount(_) => {
                let cap = &s.capabilities[&id];
                let Some(target) = mount_target(s, entry) else {
                    return Err(AuthorityError::MountTargetDead);
                };
                if cap.rights & (CAP_NS_RESOLVE | CAP_DERIVE) != CAP_NS_RESOLVE | CAP_DERIVE {
                    return Err(AuthorityError::NotDelegable);
                }
                mounts.push(id);
                if prefix == remaining {
                    break id;
                }
                remaining = &remaining[prefix.len() + 1..];
                ns = target;
            }
        }
    };
    let p = &s.capabilities[&parent];
    if !p.active || !target_live(s, &p.object) {
        return Err(AuthorityError::TargetDead);
    }
    if p.rights & CAP_DERIVE == 0 {
        return Err(AuthorityError::NotDelegable);
    }
    if rights & !p.rights != 0 {
        return Err(AuthorityError::RightsExceeded);
    }
    if rights & !valid_rights(&p.object) != 0 {
        return Err(AuthorityError::WrongRightType);
    }
    let id = fresh_cap(s)?;
    s.insert_capability(
        id,
        Capability {
            owner: caller,
            object: p.object.clone(),
            rights,
            active: true,
            parent: Some(parent),
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::CapabilityDerived {
            cap: id,
            parent,
            location: CapLocation::HeldBy(caller),
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::NamespaceResolved {
            namespace: start,
            namespace_cap: ns_cap,
            mounts,
            path: path.to_owned(),
            parent,
            cap: id,
        },
    );
    Ok(id)
}

pub fn task_destroy(s: &mut State, caller: TaskId, task_cap: CapId) -> Result<(), AuthorityError> {
    let cap = authorize(s, caller, task_cap, CAP_TASK_DESTROY)?;
    let CapObject::Task(task) = cap.object else {
        return Err(AuthorityError::InvalidTask);
    };
    retire_task(s, task, Actor::Task(caller));
    Ok(())
}

pub fn ns_destroy(s: &mut State, caller: TaskId, ns_cap: CapId) -> Result<(), AuthorityError> {
    let ns = namespace(s, caller, ns_cap, CAP_NS_DESTROY)?;
    if s.tasks.values().any(|t| {
        s.capabilities
            .get(&t.root_ns_cap)
            .is_some_and(|c| c.object == CapObject::Namespace(ns))
    }) {
        return Err(AuthorityError::StructuralCap);
    }
    let mut entries: Vec<_> = s
        .bindings
        .keys()
        .filter(|(n, _)| *n == ns)
        .cloned()
        .collect();
    entries.sort();
    for key in entries {
        let entry = s.bindings.remove(&key).unwrap();
        let cap = entry.cap();
        remove_location(s, cap, Actor::Task(caller));
        let event = match entry {
            NsEntry::Bound(_) => AuditKind::NamespaceUnbound {
                namespace: ns,
                path: key.1,
                cap,
            },
            NsEntry::Mount(_) => AuditKind::NamespaceUnmounted {
                namespace: ns,
                path: key.1,
                cap,
            },
        };
        s.push_audit(Actor::Task(caller), event);
    }
    s.namespaces.remove(&ns);
    s.push_audit(
        Actor::Task(caller),
        AuditKind::NamespaceDestroyed { namespace: ns },
    );
    Ok(())
}

/// Container cleanup shared by the composite memory model and local authority model.
/// Caller has already validated task-destruction authority and scheduling constraints.
pub(crate) fn retire_task(s: &mut State, task: TaskId, actor: Actor) {
    retire_timers(s, task, actor);
    retire_waiter(s, task);
    let mut discarded: Vec<_> = s
        .messages
        .iter()
        .filter_map(|(&id, m)| (m.receiver == task).then_some(id))
        .collect();
    discarded.sort_unstable();
    for id in discarded {
        let message = s.messages.remove(&id).unwrap();
        for cap in message.caps {
            remove_location(s, cap, actor);
        }
        if let Some(q) = s.mailboxes.get_mut(&message.receiver) {
            q.retain(|m| *m != id);
        }
    }
    let mut sources: Vec<_> = s
        .subscriptions
        .iter()
        .filter_map(|&(t, source)| (t == task).then_some(source))
        .collect();
    sources.sort_unstable();
    for source in sources {
        s.subscriptions.remove(&(task, source));
        s.push_audit(actor, AuditKind::ExceptionUnsubscribed { task, source });
    }
    let mut held: Vec<_> = s
        .locations
        .iter()
        .filter_map(|(&id, l)| (*l == CapLocation::HeldBy(task)).then_some(id))
        .collect();
    held.sort_unstable();
    for cap in held {
        remove_location(s, cap, actor);
    }
    s.pending_returns.remove(&task);
    s.mailboxes.remove(&task);
    s.pending_exc.remove(&task);
    s.subscriptions.retain(|(t, _)| *t != task);
    s.tasks.remove(&task);
    s.remote_tasks.remove(&task);
    s.push_audit(actor, AuditKind::TaskDestroyed { task });
}

/// W2–W4, registered structural roots, and local message-carrier constraints.
/// Fixtures with root_ns_cap=0 do not claim W5; full COSIR WellFormed is broader.
pub fn locations_well_formed(s: &State) -> bool {
    let caps_ok = s.capabilities.iter().all(|(&id, c)| {
        id != 0
            && u64::from(id) < s.next_cap
            && c.rights & !valid_rights(&c.object) == 0
            && c.parent.is_none_or(|p| {
                p < id
                    && s.capabilities.get(&p).is_some_and(|parent| {
                        parent.object == c.object && c.rights & !parent.rights == 0
                    })
            })
    });
    let locations_ok = s.locations.iter().all(|(&id, l)| {
        s.capabilities.contains_key(&id)
            && match l {
                CapLocation::HeldBy(t) => s.tasks.contains_key(t),
                CapLocation::InMessage(m) => s
                    .messages
                    .get(m)
                    .is_some_and(|m| m.caps.iter().filter(|&&c| c == id).count() == 1),
                CapLocation::InNamespace(ns, p) => {
                    s.namespaces.contains_key(ns)
                        && s.bindings
                            .get(&(*ns, p.clone()))
                            .is_some_and(|entry| entry.cap() == id)
                }
            }
    });
    let messages_ok = s.messages.iter().all(|(&id, m)| {
        m.id == id
            && id < s.next_message
            && s.tasks.contains_key(&m.receiver)
            && m.data.len() <= MAX_MSG_SIZE
            && m.caps.len() <= MAX_CAPS_PER_MSG
            && m.caps.iter().collect::<HashSet<_>>().len() == m.caps.len()
            && m.caps
                .iter()
                .all(|c| s.locations.get(c) == Some(&CapLocation::InMessage(id)))
            && s.mailboxes
                .values()
                .flat_map(|q| q.iter())
                .filter(|&&v| v == id)
                .count()
                == 1
            && s.mailboxes
                .get(&m.receiver)
                .is_some_and(|q| q.contains(&id))
    });
    let bindings_ok = s.bindings.iter().all(|((ns, p), &entry)| {
        let cap = entry.cap();
        valid_path(p)
            && s.namespaces.contains_key(ns)
            && s.locations.get(&cap) == Some(&CapLocation::InNamespace(*ns, p.clone()))
            && match entry {
                NsEntry::Bound(_) => true,
                NsEntry::Mount(_) => s.capabilities.get(&cap).is_some_and(|c| {
                    matches!(c.object, CapObject::Namespace(_))
                        && c.rights & (CAP_NS_RESOLVE | CAP_DERIVE) == (CAP_NS_RESOLVE | CAP_DERIVE)
                        && mount_target(s, entry)
                            .is_none_or(|target| !mount_reaches(s, target, *ns))
                }),
            }
            && !s
                .bindings
                .keys()
                .any(|(n, other)| n == ns && other != p && path_prefix(p, other))
    });
    let tasks_ok = s.tasks.iter().all(|(&id, t)| {
        let root_ok = t.root_ns_cap == 0
            || s.held(id, t.root_ns_cap) && s.capabilities.get(&t.root_ns_cap).is_some_and(|c| {
                c.active
                    && c.rights & CAP_NS_RESOLVE != 0
                    && matches!(c.object, CapObject::Namespace(n) if s.namespaces.contains_key(&n))
            });
        root_ok
            && (t.status != TaskStatus::BlockedRecv
                || !s.pending_returns.contains_key(&id)
                    && s.mailboxes.get(&id).is_none_or(|q| q.is_empty()))
    });
    let queues_ok = s.mailboxes.iter().all(|(task, q)| {
        s.tasks.contains_key(task)
            && q.iter()
                .all(|id| s.messages.get(id).is_some_and(|m| m.receiver == *task))
    });
    let returns_ok = s.pending_returns.iter().all(|(task, m)| {
        s.tasks
            .get(task)
            .is_some_and(|t| t.status == TaskStatus::Ready)
            && m.receiver == *task
            && !s.messages.contains_key(&m.id)
            && m.caps.iter().collect::<HashSet<_>>().len() == m.caps.len()
            && m.caps.iter().all(|c| s.held(*task, *c))
    });
    caps_ok && locations_ok && messages_ok && bindings_ok && tasks_ok && queues_ok && returns_ok
}

/// Composed local carriers, event/wait state and data-only remote transport.
/// Memory page tables/device resources and the scheduling core-count bound
/// remain separate refinement models, not consequences of this predicate.
pub fn mechanisms_well_formed(s: &State) -> bool {
    let message_valid = |message: &Message| {
        let sent = s
            .audit
            .iter()
            .filter(|event| {
                matches!(&event.kind,
            AuditKind::MessageSent { message: id, origin, receiver, data, caps, .. }
                if *id == message.id && *origin == message.origin && *receiver == message.receiver
                    && *data == message.data && *caps == message.caps)
            })
            .count()
            == 1;
        sent && match &message.origin {
            MessageOrigin::Task(task) => s.audit.iter().any(|event|
                matches!(event.kind, AuditKind::TaskCreated { task: created, .. } if created == *task)),
            MessageOrigin::Kernel(event) => message.caps.is_empty() && match event {
                KernelEvent::TimerFired(timer) => s.audit.iter().any(|event|
                    matches!(event.kind, AuditKind::TimerFired { timer: fired, .. } if fired == *timer)),
                KernelEvent::ExceptionRaised { source, kind, fault_va } => s.audit.iter().any(|event|
                    matches!(&event.kind, AuditKind::ExceptionRaised { source_id, kind: raised, fault_va: address }
                        if source_id == source && raised == kind && address == fault_va)),
            },
        }
    };
    provenance_well_formed(s)
        && locations_well_formed(s)
        && events_well_formed(s)
        && remote_well_formed(s)
        && waits_well_formed(s)
        && s.tasks.iter().all(|(&task, t)| {
            (t.status == TaskStatus::Faulted) == s.pending_exc.contains_key(&task)
        })
        && s.messages
            .values()
            .chain(s.pending_returns.values())
            .all(message_valid)
}

/// W13 for this slice: replay token creation and every location event. Legacy
/// CDT-only fixtures deliberately do not claim this stronger audit invariant.
pub fn provenance_well_formed(s: &State) -> bool {
    let mut locations = std::collections::HashMap::new();
    let mut created = HashSet::new();
    let mut previous = None;
    for event in &s.audit {
        if event.id >= s.next_event || previous.is_some_and(|id| id >= event.id) {
            return false;
        }
        previous = Some(event.id);
        match &event.kind {
            AuditKind::CapabilityMinted { cap, holder } => {
                if !created.insert(*cap)
                    || !s
                        .capabilities
                        .get(cap)
                        .is_some_and(|c| c.parent.is_none() && c.owner == *holder)
                {
                    return false;
                }
                locations.insert(*cap, CapLocation::HeldBy(*holder));
            }
            AuditKind::CapabilityDerived {
                cap,
                parent,
                location,
            } => {
                let holder = match location {
                    CapLocation::HeldBy(task) => *task,
                    CapLocation::InNamespace(_, _) => match event.actor {
                        Actor::Task(task) => task,
                        Actor::Kernel => return false,
                    },
                    CapLocation::InMessage(_) => return false,
                };
                if !created.contains(parent)
                    || !created.insert(*cap)
                    || !s
                        .capabilities
                        .get(cap)
                        .is_some_and(|c| c.parent == Some(*parent) && c.owner == holder)
                {
                    return false;
                }
                locations.insert(*cap, location.clone());
            }
            AuditKind::CapabilityForkInherited { cap, source, child } => {
                if !created.contains(source)
                    || !created.insert(*cap)
                    || !s
                        .capabilities
                        .get(cap)
                        .is_some_and(|c| c.parent == Some(*source) && c.owner == *child)
                {
                    return false;
                }
                locations.insert(*cap, CapLocation::HeldBy(*child));
            }
            AuditKind::CapabilityMoved { cap, from, to, .. } => {
                if locations.get(cap) != Some(from) {
                    return false;
                }
                locations.insert(*cap, to.clone());
            }
            AuditKind::CapabilityDropped { cap, from }
                if locations.remove(cap).as_ref() != Some(from) =>
            {
                return false;
            }
            _ => {}
        }
    }
    locations == s.locations && created.len() == s.capabilities.len() && locations_well_formed(s)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuditTrace {
    pub chain: Vec<(CapId, Capability)>,
    pub events: Vec<AuditEvent>,
}

/// §13.2 read-only provenance; dropped ancestors remain queryable via a held child.
/// Returns ancestor records root-first plus creation/move/drop events for that chain.
pub fn audit(s: &State, caller: TaskId, cap: CapId) -> Result<AuditTrace, AuthorityError> {
    held_cap(s, caller, cap)?;
    let mut chain = Vec::new();
    let mut current = Some(cap);
    while let Some(id) = current {
        let record = s.capabilities.get(&id).ok_or(AuthorityError::InvalidCap)?;
        if record.parent.is_some_and(|p| p >= id) {
            return Err(AuthorityError::InvalidCap);
        }
        current = record.parent;
        chain.push((id, record.clone()));
    }
    chain.reverse();
    let events = s
        .audit
        .iter()
        .filter(|event| match &event.kind {
            AuditKind::CapabilityMinted { cap, .. }
            | AuditKind::CapabilityDerived { cap, .. }
            | AuditKind::CapabilityForkInherited { cap, .. }
            | AuditKind::CapabilityForkRebound { cap, .. }
            | AuditKind::CapabilityMoved { cap, .. }
            | AuditKind::CapabilityDropped { cap, .. } => chain.iter().any(|(id, _)| id == cap),
            AuditKind::ObjectCreated { object_id, segment } => {
                chain.last().is_some_and(|(_, c)| {
                    if *segment {
                        c.object == CapObject::Segment(*object_id)
                    } else {
                        c.object == CapObject::Namespace(*object_id)
                    }
                })
            }
            AuditKind::TaskCreated { task, .. } | AuditKind::TaskDestroyed { task } => chain
                .last()
                .is_some_and(|(_, c)| c.object == CapObject::Task(*task)),
            AuditKind::NamespaceDestroyed { namespace } => chain
                .last()
                .is_some_and(|(_, c)| c.object == CapObject::Namespace(*namespace)),
            AuditKind::TimerSet { timer, .. }
            | AuditKind::TimerFired { timer, .. }
            | AuditKind::TimerCancelled { timer, .. }
            | AuditKind::TimerDestroyed { timer } => chain
                .last()
                .is_some_and(|(_, c)| c.object == CapObject::Timer(*timer)),
            AuditKind::NodeCreated { node, .. } | AuditKind::NodeDestroyed { node } => chain
                .last()
                .is_some_and(|(_, c)| c.object == CapObject::Node(*node)),
            AuditKind::WaitObjectCreated { object }
            | AuditKind::WaitObjectDestroyed { object }
            | AuditKind::WaitObjectSignaled { object, .. } => chain
                .last()
                .is_some_and(|(_, c)| c.object == CapObject::WaitObject(*object)),
            AuditKind::ExceptionSourceCreated { source }
            | AuditKind::ExceptionSourceDestroyed { source }
            | AuditKind::ExceptionSubscribed { source, .. }
            | AuditKind::ExceptionUnsubscribed { source, .. } => chain
                .last()
                .is_some_and(|(_, c)| c.object == CapObject::ExceptionSource(*source)),
            _ => false,
        })
        .cloned()
        .collect();
    Ok(AuditTrace { chain, events })
}

#[cfg(test)]
mod event_tests;
#[cfg(test)]
mod tests;
