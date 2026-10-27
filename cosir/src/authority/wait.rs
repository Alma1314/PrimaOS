use super::*;

fn object(
    s: &State,
    caller: TaskId,
    cap: CapId,
    right: u32,
) -> Result<WaitObjectId, AuthorityError> {
    match authorize(s, caller, cap, right)?.object {
        CapObject::WaitObject(id) => Ok(id),
        _ => Err(AuthorityError::InvalidWaitObject),
    }
}

pub fn wait_object_create(s: &mut State, caller: TaskId) -> Result<CapId, AuthorityError> {
    assert!(s.tasks.contains_key(&caller) && !s.remote_tasks.contains_key(&caller));
    let id = u32::try_from(s.next_wait_object.max(1)).map_err(|_| AuthorityError::IdExhausted)?;
    let cap = fresh_cap(s)?;
    s.next_wait_object = u64::from(id) + 1;
    s.wait_objects.insert(
        id,
        WaitObject {
            creator: caller,
            generation: 0,
            waiters: HashSet::new(),
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::WaitObjectCreated { object: id },
    );
    s.insert_capability(
        cap,
        Capability {
            owner: caller,
            object: CapObject::WaitObject(id),
            rights: CAP_DERIVE | CAP_WAIT_WAIT | CAP_WAIT_SIGNAL | CAP_WAIT_DESTROY,
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

pub fn wait_object_observe(s: &State, caller: TaskId, cap: CapId) -> Result<u64, AuthorityError> {
    Ok(s.wait_objects[&object(s, caller, cap, CAP_WAIT_WAIT)?].generation)
}

fn satisfied(s: &State, request: &WaitRequest) -> Option<WaitResult> {
    let advanced =
        |&(id, observed): &(WaitObjectId, u64)| s.wait_objects[&id].generation > observed;
    match request.mode {
        WaitMode::Any => request
            .entries
            .iter()
            .find(|entry| advanced(entry))
            .map(|(id, _)| WaitResult::Satisfied(vec![*id])),
        WaitMode::All if request.entries.iter().all(advanced) => Some(WaitResult::Satisfied(
            request.entries.iter().map(|(id, _)| *id).collect(),
        )),
        WaitMode::All => None,
    }
}

/// None is a blocked call; the scheduler consumes pending_wait_results later.
pub fn wait_objects(
    s: &mut State,
    caller: TaskId,
    caps: &[CapId],
    observed: &[u64],
    mode: WaitMode,
    alertable: bool,
    deadline: Option<Instant>,
) -> Result<Option<WaitResult>, AuthorityError> {
    assert_eq!(s.tasks[&caller].status, TaskStatus::Running);
    assert!(
        !s.remote_tasks.contains_key(&caller)
            && !s.pending_returns.contains_key(&caller)
            && !s.pending_wait_results.contains_key(&caller)
    );
    if caps.is_empty() {
        return Err(AuthorityError::EmptyWaitSet);
    }
    if caps.len() != observed.len() {
        return Err(AuthorityError::LengthMismatch);
    }
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    for (&cap, &generation) in caps.iter().zip(observed) {
        let id = object(s, caller, cap, CAP_WAIT_WAIT)?;
        if !seen.insert(id) {
            return Err(AuthorityError::DuplicateWaitObject);
        }
        if generation > s.wait_objects[&id].generation {
            return Err(AuthorityError::InvalidGeneration);
        }
        entries.push((id, generation));
    }
    let request = WaitRequest {
        entries,
        mode,
        alertable,
        deadline,
    };
    if let Some(result) = satisfied(s, &request) {
        return Ok(Some(result));
    }
    if alertable
        && s.pending_upcalls
            .get(&caller)
            .is_some_and(|q| !q.is_empty())
    {
        return Ok(Some(WaitResult::Alerted));
    }
    if deadline.is_some_and(|d| d <= s.now) {
        return Ok(Some(WaitResult::TimedOut));
    }
    for &(id, _) in &request.entries {
        s.wait_objects.get_mut(&id).unwrap().waiters.insert(caller);
    }
    s.wait_requests.insert(caller, request.clone());
    s.tasks.get_mut(&caller).unwrap().status = TaskStatus::BlockedWait;
    s.push_audit(
        Actor::Task(caller),
        AuditKind::TaskStateChanged {
            task: caller,
            from: TaskStatus::Running,
            to: TaskStatus::BlockedWait,
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::WaitBlocked {
            task: caller,
            request,
        },
    );
    Ok(None)
}

fn unlink(s: &mut State, task: TaskId) {
    if let Some(request) = s.wait_requests.remove(&task) {
        for (id, _) in request.entries {
            if let Some(object) = s.wait_objects.get_mut(&id) {
                object.waiters.remove(&task);
            }
        }
    }
}

fn complete(s: &mut State, task: TaskId, result: WaitResult, actor: Actor) {
    unlink(s, task);
    s.pending_wait_results.insert(task, result.clone());
    s.tasks.get_mut(&task).unwrap().status = TaskStatus::Ready;
    s.push_audit(
        actor,
        AuditKind::TaskStateChanged {
            task,
            from: TaskStatus::BlockedWait,
            to: TaskStatus::Ready,
        },
    );
    s.push_audit(actor, AuditKind::WaitCompleted { task, result });
}

pub fn signal_object(s: &mut State, caller: TaskId, cap: CapId) -> Result<usize, AuthorityError> {
    let id = object(s, caller, cap, CAP_WAIT_SIGNAL)?;
    let generation = s.wait_objects[&id]
        .generation
        .checked_add(1)
        .ok_or(AuthorityError::IdExhausted)?;
    s.wait_objects.get_mut(&id).unwrap().generation = generation;
    let mut tasks: Vec<_> = s.wait_objects[&id].waiters.iter().copied().collect();
    tasks.sort_unstable();
    let mut woken = 0;
    for task in tasks {
        let Some(request) = s.wait_requests.get(&task) else {
            s.wait_objects.get_mut(&id).unwrap().waiters.remove(&task);
            continue;
        };
        if let Some(result) = satisfied(s, request) {
            complete(s, task, result, Actor::Task(caller));
            woken += 1;
        }
    }
    s.push_audit(
        Actor::Task(caller),
        AuditKind::WaitObjectSignaled {
            object: id,
            generation,
            woken,
        },
    );
    Ok(woken)
}

pub fn wait_object_destroy(
    s: &mut State,
    caller: TaskId,
    cap: CapId,
) -> Result<(), AuthorityError> {
    let id = object(s, caller, cap, CAP_WAIT_DESTROY)?;
    let mut tasks: Vec<_> = s.wait_objects[&id].waiters.iter().copied().collect();
    tasks.sort_unstable();
    for task in tasks {
        if s.wait_requests
            .get(&task)
            .is_some_and(|r| r.entries.iter().any(|(obj, _)| *obj == id))
        {
            complete(s, task, WaitResult::TargetDead(id), Actor::Task(caller));
        }
    }
    s.wait_objects.remove(&id);
    s.push_audit(
        Actor::Task(caller),
        AuditKind::WaitObjectDestroyed { object: id },
    );
    Ok(())
}

pub(super) fn expire_waiters(s: &mut State) {
    let mut due: Vec<_> = s
        .wait_requests
        .iter()
        .filter_map(|(&task, request)| request.deadline.filter(|d| *d <= s.now).map(|d| (d, task)))
        .collect();
    due.sort_unstable();
    for (_, task) in due {
        complete(s, task, WaitResult::TimedOut, Actor::Kernel);
    }
}

pub(super) fn retire_waiter(s: &mut State, task: TaskId) {
    unlink(s, task);
    s.pending_wait_results.remove(&task);
    s.pending_upcalls.remove(&task);
}

pub fn task_alert(
    s: &mut State,
    caller: TaskId,
    cap: CapId,
    data: &[u8],
) -> Result<(), AuthorityError> {
    let CapObject::Task(target) = authorize(s, caller, cap, CAP_TASK_SEND)?.object else {
        return Err(AuthorityError::InvalidTask);
    };
    if s.remote_tasks.contains_key(&target) {
        return Err(AuthorityError::InvalidTask);
    }
    if data.len() > MAX_MSG_SIZE {
        return Err(AuthorityError::MsgTooLarge);
    }
    s.pending_upcalls
        .entry(target)
        .or_default()
        .push_back(Upcall {
            source: caller,
            data: data.to_vec(),
        });
    if s.wait_requests.get(&target).is_some_and(|r| r.alertable) {
        complete(s, target, WaitResult::Alerted, Actor::Task(caller));
    }
    s.push_audit(
        Actor::Task(caller),
        AuditKind::UpcallQueued {
            source: caller,
            target,
            data: data.to_vec(),
        },
    );
    Ok(())
}

pub fn task_upcall_recv(s: &mut State, caller: TaskId) -> Vec<Upcall> {
    assert_eq!(s.tasks[&caller].status, TaskStatus::Running);
    assert!(!s.remote_tasks.contains_key(&caller));
    let calls = s
        .pending_upcalls
        .remove(&caller)
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    s.push_audit(
        Actor::Task(caller),
        AuditKind::UpcallReceived {
            task: caller,
            count: calls.len(),
        },
    );
    calls
}

pub fn waits_well_formed(s: &State) -> bool {
    s.wait_requests.iter().all(|(&task, r)| {
        !r.entries.is_empty()
            && r.entries
                .iter()
                .map(|(id, _)| id)
                .collect::<HashSet<_>>()
                .len()
                == r.entries.len()
            && r.deadline.is_none_or(|d| d > s.now)
            && !s.pending_returns.contains_key(&task)
            && !s.pending_wait_results.contains_key(&task)
            && !s.remote_tasks.contains_key(&task)
            && s.tasks
                .get(&task)
                .is_some_and(|t| t.status == TaskStatus::BlockedWait)
            && r.entries.iter().all(|(id, observed)| {
                s.wait_objects
                    .get(id)
                    .is_some_and(|o| o.waiters.contains(&task) && *observed <= o.generation)
            })
    }) && s.wait_objects.iter().all(|(&id, o)| {
        o.waiters.iter().all(|task| {
            s.wait_requests
                .get(task)
                .is_some_and(|r| r.entries.iter().any(|(obj, _)| *obj == id))
        })
    }) && s.tasks.iter().all(|(&task, t)| {
        (t.status == TaskStatus::BlockedWait) == s.wait_requests.contains_key(&task)
    }) && s.pending_wait_results.keys().all(|task| {
        s.tasks
            .get(task)
            .is_some_and(|t| t.status == TaskStatus::Ready)
            && !s.pending_returns.contains_key(task)
    }) && s
        .pending_upcalls
        .keys()
        .all(|task| s.tasks.contains_key(task) && !s.remote_tasks.contains_key(task))
}
