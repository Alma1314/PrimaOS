use super::*;

pub fn node_create(
    s: &mut State,
    caller: TaskId,
    peer: NodeIdentity,
    transport: NodeTransport,
) -> Result<CapId, AuthorityError> {
    assert!(s.tasks.contains_key(&caller) && !s.remote_tasks.contains_key(&caller));
    if peer == s.local_node || !s.known_nodes.contains(&peer) {
        return Err(AuthorityError::InvalidNode);
    }
    if s.nodes.values().any(|node| node.peer == peer) {
        return Err(AuthorityError::AlreadyConnected);
    }
    let id = u32::try_from(s.next_node.max(1)).map_err(|_| AuthorityError::IdExhausted)?;
    let cap = fresh_cap(s)?;
    s.next_node = u64::from(id) + 1;
    s.nodes.insert(
        id,
        Node {
            creator: caller,
            peer,
            transport,
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::NodeCreated { node: id, peer },
    );
    s.insert_capability(
        cap,
        Capability {
            owner: caller,
            object: CapObject::Node(id),
            rights: CAP_DERIVE | CAP_NODE_CONNECT | CAP_NODE_DESTROY,
            active: true,
            parent: None,
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::CapabilityMinted {
            cap,
            holder: caller,
        },
    );
    Ok(cap)
}

fn node(s: &State, caller: TaskId, cap: CapId, right: u32) -> Result<NodeId, AuthorityError> {
    match authorize(s, caller, cap, right)?.object {
        CapObject::Node(id) => Ok(id),
        _ => Err(AuthorityError::InvalidNode),
    }
}

fn existing_proxy(s: &State, remote: RemoteTask) -> Option<TaskId> {
    s.remote_tasks
        .iter()
        .find_map(|(&task, &value)| (value == remote).then_some(task))
}

fn proxy_budget(s: &State) -> Result<(TaskId, CapId), AuthorityError> {
    let next = s
        .tasks
        .keys()
        .map(|&id| u64::from(id) + 1)
        .max()
        .unwrap_or(1);
    let task = u32::try_from(s.next_task.max(next)).map_err(|_| AuthorityError::IdExhausted)?;
    Ok((task, fresh_cap(s)?))
}

/// Kernel-only attach. Caller preflights both IDs and uniqueness before commit.
fn attach(s: &mut State, remote: RemoteTask, holder: TaskId, task: TaskId, cap: CapId) {
    assert!(existing_proxy(s, remote).is_none());
    s.next_task = u64::from(task) + 1;
    s.tasks.insert(
        task,
        Task {
            id: task,
            status: TaskStatus::Ready,
            vspace: vec![],
            root_ns_cap: 0,
            kernel_resume_frame: None,
        },
    );
    s.remote_tasks.insert(task, remote);
    s.push_audit(
        Actor::Kernel,
        AuditKind::TaskCreated {
            task,
            status: TaskStatus::Ready,
        },
    );
    s.push_audit(
        Actor::Kernel,
        AuditKind::RemoteAttached {
            task,
            peer: remote.peer,
            remote_id: remote.remote_id,
        },
    );
    s.insert_capability(
        cap,
        Capability {
            owner: holder,
            object: CapObject::Task(task),
            rights: CAP_DERIVE | CAP_TASK_SEND,
            active: true,
            parent: None,
        },
    );
    s.push_audit(Actor::Kernel, AuditKind::CapabilityMinted { cap, holder });
}

pub fn node_attach(
    s: &mut State,
    caller: TaskId,
    node_cap: CapId,
    remote_id: TaskId,
) -> Result<CapId, AuthorityError> {
    let node = node(s, caller, node_cap, CAP_NODE_CONNECT)?;
    let remote = RemoteTask {
        peer: s.nodes[&node].peer,
        remote_id,
    };
    if let Some(task) = existing_proxy(s, remote) {
        return s
            .capabilities
            .iter()
            .filter_map(|(&id, cap)| {
                (s.held(caller, id)
                    && cap.active
                    && cap.object == CapObject::Task(task)
                    && cap.rights & CAP_TASK_SEND != 0)
                    .then_some(id)
            })
            .min()
            .ok_or(AuthorityError::NotHeld);
    }
    let (task, cap) = proxy_budget(s)?;
    attach(s, remote, caller, task, cap);
    Ok(cap)
}

pub(super) fn remote_send(
    s: &mut State,
    caller: TaskId,
    receiver: TaskId,
    remote: RemoteTask,
    data: &[u8],
) -> Result<u64, AuthorityError> {
    let node = s
        .nodes
        .iter()
        .find_map(|(&id, node)| (node.peer == remote.peer).then_some(id))
        .ok_or(AuthorityError::NodeUnreachable)?;
    check_message_ids(s, 1)?;
    let id = s.next_message.max(1);
    s.next_message = id + 1;
    s.push_audit(
        Actor::Task(caller),
        AuditKind::MessageSent {
            message: id,
            origin: MessageOrigin::Task(caller),
            receiver,
            data: data.to_vec(),
            caps: vec![],
            transport_message: Some(id),
        },
    );
    // This queue models completed hand-off to the transport, not local inbox
    // delivery. Kernel-origin events and capabilities cannot enter this path.
    s.outgoing.push((
        node,
        TransportMessage {
            source_node: s.local_node,
            sender: caller,
            receiver: remote.remote_id,
            transport_message: id,
            data: data.to_vec(),
        },
    ));
    Ok(id)
}

/// Transport identity is supplied by the trusted refinement, never user data.
/// Invalid destinations drop before any sender proxy or root is created.
pub fn deliver_remote(
    s: &mut State,
    packet: &TransportMessage,
) -> Result<Option<u64>, AuthorityError> {
    assert!(packet.source_node != s.local_node && s.known_nodes.contains(&packet.source_node));
    assert!(packet.data.len() <= MAX_MSG_SIZE);
    if !s.tasks.contains_key(&packet.receiver) || s.remote_tasks.contains_key(&packet.receiver) {
        s.push_audit(
            Actor::Kernel,
            AuditKind::MessageDropped {
                transport_message: packet.transport_message,
                receiver: packet.receiver,
            },
        );
        return Ok(None);
    }
    check_message_ids(s, 1)?;
    let remote = RemoteTask {
        peer: packet.source_node,
        remote_id: packet.sender,
    };
    let existing = existing_proxy(s, remote);
    let budget = if existing.is_none() {
        Some(proxy_budget(s)?)
    } else {
        None
    };
    let sender = if let Some(task) = existing {
        task
    } else {
        let (task, cap) = budget.unwrap();
        attach(s, remote, packet.receiver, task, cap);
        task
    };
    Ok(Some(enqueue_or_deliver(
        s,
        MessageOrigin::Task(sender),
        packet.receiver,
        &packet.data,
        &[],
        Actor::Kernel,
        Some(packet.transport_message),
    )))
}

pub fn node_destroy(s: &mut State, caller: TaskId, cap: CapId) -> Result<(), AuthorityError> {
    let id = node(s, caller, cap, CAP_NODE_DESTROY)?;
    let peer = s.nodes[&id].peer;
    let mut proxies: Vec<_> = s
        .remote_tasks
        .iter()
        .filter_map(|(&task, remote)| (remote.peer == peer).then_some(task))
        .collect();
    proxies.sort_unstable();
    for task in proxies {
        retire_task(s, task, Actor::Task(caller));
    }
    s.nodes.remove(&id);
    s.push_audit(Actor::Task(caller), AuditKind::NodeDestroyed { node: id });
    Ok(())
}

pub fn remote_well_formed(s: &State) -> bool {
    let mut peers = HashSet::new();
    let mut identities = HashSet::new();
    s.nodes.values().all(|node| {
        node.peer != s.local_node && s.known_nodes.contains(&node.peer) && peers.insert(node.peer)
    }) && s.remote_tasks.iter().all(|(&id, remote)| {
        remote.peer != s.local_node
            && s.known_nodes.contains(&remote.peer)
            && identities.insert((remote.peer, remote.remote_id))
            && s.tasks.get(&id).is_some_and(|t| {
                t.status == TaskStatus::Ready
                    && t.vspace.is_empty()
                    && t.root_ns_cap == 0
                    && t.kernel_resume_frame.is_none()
            })
            && !s.pending_returns.contains_key(&id)
            && !s.pending_exc.contains_key(&id)
            && s.mailboxes.get(&id).is_none_or(|q| q.is_empty())
    })
}
