use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerCancelOutcome {
    Cancelled,
    AlreadyFired,
    AlreadyCancelled,
}

fn fresh_object(next: u64) -> Result<u32, AuthorityError> {
    u32::try_from(next.max(1)).map_err(|_| AuthorityError::IdExhausted)
}

fn mint(s: &mut State, caller: TaskId, cap: CapId, object: CapObject, rights: u32) {
    s.insert_capability(
        cap,
        Capability {
            owner: caller,
            object,
            rights,
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
}

pub fn time_now(s: &State) -> Instant {
    s.now
}

pub fn timer_set(
    s: &mut State,
    caller: TaskId,
    deadline: Instant,
) -> Result<CapId, AuthorityError> {
    assert!(s.tasks.contains_key(&caller) && !s.remote_tasks.contains_key(&caller));
    let id = fresh_object(s.next_timer)?;
    assert!(!s.timers.contains_key(&id));
    let cap = fresh_cap(s)?;
    if deadline <= s.now {
        check_message_ids(s, 1)?;
    }
    s.next_timer = u64::from(id) + 1;
    s.timers.insert(
        id,
        Timer {
            creator: caller,
            recipient: caller,
            deadline,
            state: TimerState::Pending,
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::TimerSet {
            timer: id,
            recipient: caller,
            deadline,
        },
    );
    mint(
        s,
        caller,
        cap,
        CapObject::Timer(id),
        CAP_DERIVE | CAP_TIMER_CANCEL | CAP_TIMER_DESTROY,
    );
    if deadline <= s.now {
        fire(s, id);
    }
    Ok(cap)
}

fn fire(s: &mut State, id: TimerId) {
    let timer = s.timers[&id];
    assert_eq!(timer.state, TimerState::Pending);
    s.push_audit(
        Actor::Kernel,
        AuditKind::TimerFired {
            timer: id,
            deadline: timer.deadline,
            fired_at: s.now,
        },
    );
    let notification = enqueue_or_deliver(
        s,
        MessageOrigin::Kernel(KernelEvent::TimerFired(id)),
        timer.recipient,
        &[],
        &[],
        Actor::Kernel,
        None,
    );
    s.timers.get_mut(&id).unwrap().state = TimerState::Fired {
        fired_at: s.now,
        notification,
    };
}

pub fn advance_time(s: &mut State, now: Instant) -> Result<(), AuthorityError> {
    if now < s.now {
        return Err(AuthorityError::InvalidTime);
    }
    let mut due: Vec<_> = s
        .timers
        .iter()
        .filter_map(|(&id, t)| {
            (t.state == TimerState::Pending && t.deadline <= now).then_some((t.deadline, id))
        })
        .collect();
    due.sort_unstable();
    check_message_ids(s, due.len() as u64)?;
    s.now = now;
    for (_, id) in due {
        fire(s, id);
    }
    expire_waiters(s);
    Ok(())
}

fn timer(s: &State, caller: TaskId, cap: CapId, right: u32) -> Result<TimerId, AuthorityError> {
    match authorize(s, caller, cap, right)?.object {
        CapObject::Timer(id) => Ok(id),
        _ => Err(AuthorityError::InvalidTimer),
    }
}

pub fn timer_cancel(
    s: &mut State,
    caller: TaskId,
    cap: CapId,
) -> Result<TimerCancelOutcome, AuthorityError> {
    let id = timer(s, caller, cap, CAP_TIMER_CANCEL)?;
    match s.timers[&id].state {
        TimerState::Fired { .. } => Ok(TimerCancelOutcome::AlreadyFired),
        TimerState::Cancelled(_) => Ok(TimerCancelOutcome::AlreadyCancelled),
        TimerState::Pending => {
            let reason = CancelReason::ByTask(caller);
            s.timers.get_mut(&id).unwrap().state = TimerState::Cancelled(reason);
            s.push_audit(
                Actor::Task(caller),
                AuditKind::TimerCancelled { timer: id, reason },
            );
            Ok(TimerCancelOutcome::Cancelled)
        }
    }
}

fn destroy_timer(s: &mut State, id: TimerId, actor: Actor, reason: CancelReason) {
    if s.timers[&id].state == TimerState::Pending {
        s.push_audit(actor, AuditKind::TimerCancelled { timer: id, reason });
    }
    s.timers.remove(&id);
    s.push_audit(actor, AuditKind::TimerDestroyed { timer: id });
}

pub fn timer_destroy(s: &mut State, caller: TaskId, cap: CapId) -> Result<(), AuthorityError> {
    let id = timer(s, caller, cap, CAP_TIMER_DESTROY)?;
    destroy_timer(s, id, Actor::Task(caller), CancelReason::ByTask(caller));
    Ok(())
}

pub(super) fn retire_timers(s: &mut State, task: TaskId, actor: Actor) {
    let mut timers: Vec<_> = s
        .timers
        .iter()
        .filter_map(|(&id, t)| (t.recipient == task).then_some(id))
        .collect();
    timers.sort_unstable();
    for id in timers {
        destroy_timer(s, id, actor, CancelReason::RecipientDestroyed);
    }
}

pub fn exc_source_create(
    s: &mut State,
    caller: TaskId,
    kind: ExceptionKind,
) -> Result<CapId, AuthorityError> {
    exc_source_create_kinds(s, caller, &[kind])
}

pub fn exc_source_create_kinds(
    s: &mut State,
    caller: TaskId,
    kinds: &[ExceptionKind],
) -> Result<CapId, AuthorityError> {
    assert!(s.tasks.contains_key(&caller) && !s.remote_tasks.contains_key(&caller));
    let Some(&kind) = kinds.first() else {
        return Err(AuthorityError::InvalidKind);
    };
    let general_fault = |kind: ExceptionKind| {
        matches!(
            kind,
            ExceptionKind::GeneralProtectionFault
                | ExceptionKind::DivideError
                | ExceptionKind::InvalidOpcode
                | ExceptionKind::DoubleFault
        )
    };
    if kinds.iter().any(|&other| {
        core::mem::discriminant(&other) != core::mem::discriminant(&kind)
            && !(general_fault(kind) && general_fault(other))
    }) {
        return Err(AuthorityError::KindConflict);
    }
    let mut seen = HashSet::from([core::mem::discriminant(&kind)]);
    let additional_kinds = kinds
        .iter()
        .copied()
        .skip(1)
        .filter(|other| seen.insert(core::mem::discriminant(other)))
        .collect();
    let next = s
        .exc_sources
        .keys()
        .map(|&id| u64::from(id) + 1)
        .max()
        .unwrap_or(1);
    let id = fresh_object(s.next_exc_source.max(next))?;
    let cap = fresh_cap(s)?;
    s.next_exc_source = u64::from(id) + 1;
    s.exc_sources.insert(
        id,
        ExceptionSource {
            supported_kind: kind,
            additional_kinds,
            active: true,
        },
    );
    s.push_audit(
        Actor::Task(caller),
        AuditKind::ExceptionSourceCreated { source: id },
    );
    mint(
        s,
        caller,
        cap,
        CapObject::ExceptionSource(id),
        CAP_DERIVE | CAP_EXC_SUBSCRIBE | CAP_EXC_SOURCE_DESTROY,
    );
    Ok(cap)
}

fn source(
    s: &State,
    caller: TaskId,
    cap: CapId,
    right: u32,
) -> Result<ExcSourceId, AuthorityError> {
    match authorize(s, caller, cap, right)?.object {
        CapObject::ExceptionSource(id) => Ok(id),
        _ => Err(AuthorityError::InvalidExcSource),
    }
}

pub fn exc_subscribe(s: &mut State, caller: TaskId, cap: CapId) -> Result<(), AuthorityError> {
    assert!(s.tasks.contains_key(&caller) && !s.remote_tasks.contains_key(&caller));
    let id = source(s, caller, cap, CAP_EXC_SUBSCRIBE)?;
    if s.subscriptions.contains(&(caller, id)) {
        return Err(AuthorityError::AlreadySubscribed);
    }
    s.subscriptions.insert((caller, id));
    s.push_audit(
        Actor::Task(caller),
        AuditKind::ExceptionSubscribed {
            task: caller,
            source: id,
        },
    );
    Ok(())
}

pub fn exc_unsubscribe(s: &mut State, caller: TaskId, cap: CapId) -> Result<(), AuthorityError> {
    let id = source(s, caller, cap, CAP_EXC_SUBSCRIBE)?;
    if !s.subscriptions.remove(&(caller, id)) {
        return Err(AuthorityError::NotSubscribed);
    }
    s.push_audit(
        Actor::Task(caller),
        AuditKind::ExceptionUnsubscribed {
            task: caller,
            source: id,
        },
    );
    Ok(())
}

pub fn exc_source_destroy(s: &mut State, caller: TaskId, cap: CapId) -> Result<(), AuthorityError> {
    let id = source(s, caller, cap, CAP_EXC_SOURCE_DESTROY)?;
    let mut subscribers: Vec<_> = s
        .subscriptions
        .iter()
        .filter_map(|&(task, src)| (src == id).then_some(task))
        .collect();
    subscribers.sort_unstable();
    for task in subscribers {
        s.subscriptions.remove(&(task, id));
        s.push_audit(
            Actor::Task(caller),
            AuditKind::ExceptionUnsubscribed { task, source: id },
        );
    }
    s.exc_sources.remove(&id);
    s.push_audit(
        Actor::Task(caller),
        AuditKind::ExceptionSourceDestroyed { source: id },
    );
    Ok(())
}

/// Internal trap/IRQ fan-out; source/type checks are kernel obligations.
pub(crate) fn notify_exception(
    s: &mut State,
    source: ExcSourceId,
    kind: ExceptionKind,
    fault_va: Option<u64>,
) {
    assert!(
        s.exc_sources
            .get(&source)
            .is_some_and(|src| src.active && src.supports_kind(kind))
    );
    let mut tasks: Vec<_> = s
        .subscriptions
        .iter()
        .filter_map(|&(task, src)| (src == source).then_some(task))
        .collect();
    tasks.sort_unstable();
    check_message_ids(s, tasks.len() as u64).expect("exception notification ID budget");
    for task in tasks {
        enqueue_or_deliver(
            s,
            MessageOrigin::Kernel(KernelEvent::ExceptionRaised {
                source,
                kind,
                fault_va,
            }),
            task,
            &[],
            &[],
            Actor::Kernel,
            None,
        );
    }
}

/// W11/W15 and exact notification provenance within the local event model.
pub fn events_well_formed(s: &State) -> bool {
    let timers = s.timers.iter().all(|(&id, timer)| {
        s.tasks.contains_key(&timer.recipient) && !s.remote_tasks.contains_key(&timer.recipient)
            && match timer.state {
                TimerState::Pending => timer.deadline > s.now,
                TimerState::Cancelled(_) => !s.audit.iter().any(|e| matches!(e.kind, AuditKind::TimerFired { timer, .. } if timer == id)),
                TimerState::Fired { fired_at, notification } => {
                    fired_at >= timer.deadline && fired_at <= s.now
                        && s.audit.iter().filter(|e| matches!(e.kind, AuditKind::TimerFired { timer, .. } if timer == id)).count() == 1
                        && s.audit.iter().filter(|e| matches!(&e.kind, AuditKind::MessageSent { message, origin: MessageOrigin::Kernel(KernelEvent::TimerFired(sent_timer)), receiver, caps, .. }
                            if *message == notification && *sent_timer == id && *receiver == timer.recipient && caps.is_empty())).count() == 1
                }
            }
    });
    timers
        && s.subscriptions.iter().all(|&(task, source)| {
            s.tasks.contains_key(&task)
                && !s.remote_tasks.contains_key(&task)
                && s.exc_sources.get(&source).is_some_and(|src| src.active)
                && s.capabilities.iter().any(|(&id, cap)| {
                    s.held(task, id)
                        && cap.active
                        && cap.object == CapObject::ExceptionSource(source)
                        && cap.rights & CAP_EXC_SUBSCRIBE != 0
                })
        })
}
