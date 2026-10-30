//! Kernel-private WaitObject continuations for S6-M1. No machine ABI or public
//! wait authority is exported. Callers must serialize these operations with
//! Gate/Request authorization, saved-frame updates and the encompassing audit
//! transaction; this layer owns only the wait registration and Task transition.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Reserved,
    Blocked,
    Completed,
    Released,
    OwnerDestroyed,
}

/// Attach-time reservation, bound to a full Task cookie, never a reusable id.
/// Only trusted kernel code can construct one; it is not a user capability.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionWait {
    object: u32,
    owner: u64,
}

/// Each blocking episode has a new generation. A delayed completion from a
/// previous episode must never wake a later request using the same reservation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompletionTicket {
    wait: CompletionWait,
    generation: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Stale,
    Busy,
    Capacity,
    AuditFull,
}

fn completion_request(object: u32, generation: u64) -> WaitRequest {
    WaitRequest {
        entries: [WaitEntry {
            object,
            observed: generation,
        }; 2],
        count: 1,
        mode: 0,
        deadline: None,
        physical: false,
        alertable: false,
    }
}

impl BootstrapState {
    pub(super) fn is_completion_target(&self, target: ObjectRef) -> bool {
        if let ObjectRef::WaitObject(id) = target {
            return self
                .wait_objects
                .find(id)
                .is_some_and(|object| object.completion_owner.is_some());
        }
        false
    }

    pub fn upcall_interruptible(&self, task: u32) -> bool {
        self.tasks
            .find(task)
            .and_then(|task| task.wait_request)
            .is_none_or(|request| request.alertable)
    }

    fn completion_object(&self, wait: CompletionWait) -> Result<WaitObject, Error> {
        if self.task_cookie(wait.owner as u32) != Some(wait.owner) {
            return Err(Error::Stale);
        }
        self.wait_objects
            .find(wait.object)
            .filter(|object| object.completion_owner == Some(wait.owner))
            .ok_or(Error::Stale)
    }

    fn audit_completion(&mut self, object: WaitObject, action: Action) {
        self.audit.append(AuditEvent::Completion {
            object: object.id,
            owner: object.completion_owner.expect("private completion"),
            generation: object.generation,
            action,
        });
    }

    /// Reserve before a future trap can block; never allocate at the blocking
    /// entry. A Gate attach must preflight ALL other resources before this step.
    pub fn reserve_completion_wait(&mut self, owner: u32) -> Result<CompletionWait, Error> {
        let cookie = self.task_cookie(owner).ok_or(Error::Stale)?;
        if !matches!(
            self.tasks.status_of(owner),
            Some(TaskStatus::Ready | TaskStatus::Running)
        ) {
            return Err(Error::Busy);
        }
        if self
            .wait_objects
            .iter()
            .filter(|object| object.completion_owner.is_some())
            .count()
            == MAX_COMPLETION_WAITS
        {
            return Err(Error::Capacity);
        }
        let id = self.wait_objects.next_id().ok_or(Error::Capacity)?;
        if !self.audit.can_append(1) {
            return Err(Error::AuditFull);
        }
        let object = WaitObject {
            id,
            creator: KERNEL_ACTOR,
            generation: 0,
            waiters: [None; MAX_WAITERS],
            waiter_count: 0,
            completion_owner: Some(cookie),
        };
        assert!(
            self.wait_objects.insert(object),
            "separate wait quotas preflighted"
        );
        self.audit_completion(object, Action::Reserved);
        Ok(CompletionWait {
            object: id,
            owner: cookie,
        })
    }

    /// Single-item Any, no deadline, nonalertable. Does not set a native blocked
    /// syscall number, generic WaitResult, or any saved user register.
    pub fn block_completion_wait(
        &mut self,
        wait: CompletionWait,
    ) -> Result<CompletionTicket, Error> {
        let mut object = self.completion_object(wait)?;
        let task_id = wait.owner as u32;
        let task = self.tasks.find(task_id).ok_or(Error::Stale)?;
        if task.status != TaskStatus::Running
            || task.wait_request.is_some()
            || task.pending_wait_result.is_some()
            || task.pending.is_some()
            || task.waiting_process.is_some()
            || task.pending_exc.is_some()
            || object.waiter_count != 0
        {
            return Err(Error::Busy);
        }
        let generation = object.generation.checked_add(1).ok_or(Error::Capacity)?;
        if !self.audit.can_append(2) {
            return Err(Error::AuditFull);
        }
        object.generation = generation;
        object.waiters[0] = Some(task_id);
        object.waiter_count = 1;
        self.wait_objects.write(object);
        let task = self.tasks.find_mut(task_id).expect("owner preflighted");
        task.wait_request = Some(completion_request(wait.object, generation));
        task.status = TaskStatus::BlockedWait;
        self.audit_completion(object, Action::Blocked);
        self.audit_transition(task_id, TaskStatus::BlockedWait);
        Ok(CompletionTicket { wait, generation })
    }

    /// Controlled completion only clears the private wait. The Gate caller must
    /// preflight its own audit and perform the rax-only frame commit in the SAME
    /// non-scheduling critical section. Never feed this through inject_wait_wakes.
    pub fn complete_completion_wait(&mut self, ticket: CompletionTicket) -> Result<(), Error> {
        let mut object = self.completion_object(ticket.wait)?;
        let task_id = ticket.wait.owner as u32;
        let task = self.tasks.find(task_id).ok_or(Error::Stale)?;
        let expected = completion_request(object.id, ticket.generation);
        if object.generation != ticket.generation
            || object.waiter_count != 1
            || object.waiters[0] != Some(task_id)
            || task.status != TaskStatus::BlockedWait
            || task.wait_request != Some(expected)
            || task.pending_wait_result.is_some()
        {
            return Err(Error::Stale);
        }
        if !self.audit.can_append(2) {
            return Err(Error::AuditFull);
        }
        object.waiters[0] = None;
        object.waiter_count = 0;
        self.wait_objects.write(object);
        let task = self.tasks.find_mut(task_id).expect("owner preflighted");
        task.wait_request = None;
        task.status = TaskStatus::Ready;
        self.audit_completion(object, Action::Completed);
        self.audit_transition(task_id, TaskStatus::Ready);
        Ok(())
    }

    /// Release only an idle reservation. Destruction of an owner uses the common
    /// Task lifecycle hook instead, without making a dead continuation Ready.
    pub fn release_completion_wait(&mut self, wait: CompletionWait) -> Result<(), Error> {
        let object = self.completion_object(wait)?;
        if object.waiter_count != 0 {
            return Err(Error::Busy);
        }
        if !self.audit.can_append(1) {
            return Err(Error::AuditFull);
        }
        self.wait_objects
            .remove(wait.object)
            .expect("reservation preflighted");
        self.audit_completion(object, Action::Released);
        Ok(())
    }

    /// Bounded W17 projection for private completions, checked in host tests.
    /// Native wait4/upcall suspension can use BlockedWait without a WaitRequest;
    /// those existing mechanisms are deliberately outside this projection.
    pub fn completion_waits_well_formed(&self) -> bool {
        let objects_ok = self.wait_objects.iter().all(|object| {
            let Some(owner) = object.completion_owner else {
                return true;
            };
            let Some(task) = self.tasks.find(owner as u32) else {
                return false;
            };
            if self.task_cookie(task.id) != Some(owner) {
                return false;
            }
            match object.waiter_count {
                0 => object.waiters.iter().all(Option::is_none),
                1 => {
                    object.generation != 0
                        && object.waiters[0] == Some(task.id)
                        && object.waiters[1..].iter().all(Option::is_none)
                        && task.status == TaskStatus::BlockedWait
                        && task.pending_wait_result.is_none()
                        && task.wait_request
                            == Some(completion_request(object.id, object.generation))
                }
                _ => false,
            }
        });
        objects_ok
            && self.tasks.entries[..self.tasks.count]
                .iter()
                .flatten()
                .all(|task| {
                    let Some(request) = task.wait_request.filter(|request| !request.alertable)
                    else {
                        return true;
                    };
                    let Some(object) = self.wait_objects.find(request.entries[0].object) else {
                        return false;
                    };
                    task.status == TaskStatus::BlockedWait
                        && task.pending_wait_result.is_none()
                        && object.completion_owner == self.task_cookie(task.id)
                        && object.completion_owner.is_some()
                        && object.waiter_count == 1
                        && object.waiters[0] == Some(task.id)
                        && request == completion_request(object.id, object.generation)
                })
    }

    pub(super) fn completion_wait_count(&self, task: u32) -> usize {
        let Some(cookie) = self.task_cookie(task) else {
            return 0;
        };
        self.wait_objects
            .iter()
            .filter(|object| object.completion_owner == Some(cookie))
            .count()
    }

    /// Called before finish_destroy advances the Task generation. The enclosing
    /// destroy preflight accounts for each event. A fault alone retains the
    /// reservation; Gate fault/service-death policy belongs to the future adapter.
    pub(super) fn destroy_completion_waits(&mut self, task: u32) {
        let Some(cookie) = self.task_cookie(task) else {
            return;
        };
        // Do not hold an arena iterator borrow across removal.
        loop {
            let object = self
                .wait_objects
                .iter()
                .find(|object| object.completion_owner == Some(cookie));
            let Some(object) = object else {
                break;
            };
            self.wait_objects
                .remove(object.id)
                .expect("owned reservation");
            self.audit_completion(object, Action::OwnerDestroyed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> BootstrapState {
        let inventory = crate::mem::FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, 0x4000_0100, &inventory).unwrap();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        state
    }

    // Arena handles compare only their identity; snapshot the actual records too.
    fn snapshot(state: &BootstrapState) -> (TaskTable, Vec<WaitObject>, usize) {
        (
            state.tasks,
            state.wait_objects.iter().collect(),
            state.audit.count,
        )
    }

    #[test]
    fn private_wait_defers_alerts_and_native_wakes_then_completes_once() {
        let mut state = state();
        let slots = state.slots;
        let positions = state.positions;
        let wait = state.reserve_completion_wait(2).unwrap();
        assert_eq!(state.slots, slots, "no user token minted");
        assert_eq!(state.positions, positions);
        let before = state.audit.count;
        let ticket = state.block_completion_wait(wait).unwrap();
        assert_eq!(state.audit.count, before + 2);
        let blocked = snapshot(&state);
        assert!(state.completion_waits_well_formed());
        assert!(!state.upcall_interruptible(2));
        // Both custom handlers and default-action upcalls must stay queued.
        let mut signals = crate::signal::SignalState::new();
        for handler in [0, 0x400100] {
            signals
                .set_action(
                    2,
                    10,
                    crate::signal::Action {
                        handler,
                        mask: 0,
                        flags: crate::signal::RESTART,
                        context: 0,
                    },
                )
                .unwrap();
            assert!(signals.queue(2, 10, crate::signal::Info::ZERO));
            assert!(!state.interrupt_for_upcall(2).unwrap());
            assert_ne!(signals.pending(2), 0);
            assert_eq!(snapshot(&state), blocked);
        }
        for result in [
            WaitResultCode::Satisfied,
            WaitResultCode::TimedOut,
            WaitResultCode::TargetDead,
        ] {
            assert!(!state.wake_waiter(2, result));
        }
        assert_eq!(state.expire_waits(u64::MAX).count, 0);
        assert_eq!(state.expire_waits_on(u64::MAX, true).count, 0);
        assert_eq!(state.release_completion_wait(wait), Err(Error::Busy));
        assert_eq!(state.block_completion_wait(wait), Err(Error::Busy));
        let public = state.wait_object_create(2).unwrap();
        let with_public = snapshot(&state);
        assert_eq!(
            state.wait_objects(2, public, 0, Handle::ZERO, 0, 0, u64::MAX),
            Err(WaitError::InvalidCaller)
        );
        assert_eq!(snapshot(&state), with_public);
        state.wait_object_destroy(2, public).unwrap();
        state.complete_completion_wait(ticket).unwrap();
        let ready = snapshot(&state);
        assert_eq!(state.tasks.status_of(2), Some(TaskStatus::Ready));
        assert_eq!(state.tasks.find(2).unwrap().pending_wait_result, None);
        assert!(state.completion_waits_well_formed());
        assert!(state.upcall_interruptible(2));
        assert_ne!(
            signals.pending(2),
            0,
            "completion does not consume queued upcalls"
        );
        assert_eq!(state.complete_completion_wait(ticket), Err(Error::Stale));
        assert_eq!(snapshot(&state), ready);
        state.schedule_task(2).unwrap();
        let next = state.block_completion_wait(wait).unwrap();
        let blocked_again = snapshot(&state);
        assert_ne!(next, ticket);
        assert_eq!(state.complete_completion_wait(ticket), Err(Error::Stale));
        assert_eq!(
            snapshot(&state),
            blocked_again,
            "late completion cannot target next request"
        );
        state.complete_completion_wait(next).unwrap();
        state.release_completion_wait(wait).unwrap();
        assert_eq!(state.wait_objects.count, 0);
        assert!(state.completion_waits_well_formed());
    }

    #[test]
    fn private_waits_reject_public_authority_even_if_a_token_is_misinstalled() {
        let mut state = state();
        let wait = state.reserve_completion_wait(2).unwrap();
        let before = snapshot(&state);
        let slots = state.slots;
        assert_eq!(
            state.mint_root_capability(2, WAIT_RIGHTS, ObjectRef::WaitObject(wait.object)),
            Err(DeriveError::WrongRightType)
        );
        assert_eq!(
            state.mint_root_capability_at(
                CapabilityLocation::HeldByTask(2),
                8,
                WAIT_RIGHTS,
                ObjectRef::WaitObject(wait.object)
            ),
            Err(DeriveError::WrongRightType)
        );
        assert_eq!(snapshot(&state), before);
        assert_eq!(state.slots, slots);
        // Simulate a trusted-adapter bug: even a real live token with all public
        // rights must not expose a private object's signal/destroy/observe path.
        let cap = state.wait_object_create(2).unwrap();
        let ObjectRef::WaitObject(id) = state.target_of(cap).unwrap() else {
            panic!()
        };
        let mut object = state.wait_objects.find(id).unwrap();
        object.completion_owner = Some(state.task_cookie(2).unwrap());
        state.wait_objects.write(object);
        let before = snapshot(&state);
        assert_eq!(
            state.wait_object_observe(2, cap),
            Err(WaitError::InvalidWaitObject)
        );
        assert_eq!(
            state.wait_object_signal(2, cap),
            Err(WaitError::InvalidWaitObject)
        );
        assert_eq!(
            state.wait_object_destroy(2, cap),
            Err(WaitError::InvalidWaitObject)
        );
        assert_eq!(
            state.wait_objects(2, cap, 0, Handle::ZERO, 0, 0, u64::MAX),
            Err(WaitError::InvalidWaitObject)
        );
        assert_eq!(
            state.derive_capability(2, cap, WAIT_RIGHTS),
            Err(DeriveError::WrongRightType)
        );
        assert_eq!(snapshot(&state), before);
    }

    #[test]
    fn audit_and_generation_exhaustion_leave_all_wait_state_unchanged() {
        let mut state = state();
        let wait = state.reserve_completion_wait(2).unwrap();
        // One slot is insufficient: lifecycle and Ready/BlockedWait migration
        // must either both be logged or neither be committed.
        let reservation = MAX_AUDIT_EVENTS - state.audit.count - 1;
        assert!(state.audit.reserve(reservation));
        let before = snapshot(&state);
        assert_eq!(state.block_completion_wait(wait), Err(Error::AuditFull));
        assert_eq!(snapshot(&state), before);
        state.audit.release_reservation(reservation);
        let ticket = state.block_completion_wait(wait).unwrap();
        let reservation = MAX_AUDIT_EVENTS - state.audit.count - 1;
        assert!(state.audit.reserve(reservation));
        let blocked = snapshot(&state);
        assert_eq!(
            state.complete_completion_wait(ticket),
            Err(Error::AuditFull)
        );
        assert_eq!(snapshot(&state), blocked);
        // Deferring an upcall needs no audit space and cannot silently unregister.
        assert!(!state.interrupt_for_upcall(2).unwrap());
        state.audit.release_reservation(reservation);
        state.complete_completion_wait(ticket).unwrap();
        state.schedule_task(2).unwrap();
        let mut object = state.wait_objects.find(wait.object).unwrap();
        object.generation = u64::MAX;
        state.wait_objects.write(object);
        let before = snapshot(&state);
        assert_eq!(state.block_completion_wait(wait), Err(Error::Capacity));
        assert_eq!(snapshot(&state), before);
        assert!(state.completion_waits_well_formed());
    }

    #[test]
    fn owner_destruction_releases_reservations_without_late_wake_or_id_reuse() {
        let mut state = state();
        let wait = state.reserve_completion_wait(2).unwrap();
        let idle = state.reserve_completion_wait(2).unwrap();
        let ticket = state.block_completion_wait(wait).unwrap();
        state.destroy_task(2).unwrap();
        assert_eq!(state.wait_objects.count, 0);
        assert!(state.completion_waits_well_formed());
        let dead = snapshot(&state);
        assert_eq!(state.complete_completion_wait(ticket), Err(Error::Stale));
        assert_eq!(state.release_completion_wait(idle), Err(Error::Stale));
        assert_eq!(snapshot(&state), dead);
        // TaskTable::create intentionally forbids reusable IDs; the real task
        // allocator may reuse a Dead slot with a fresh cookie (create_task_record).
        assert!(state.tasks.remove(2));
        state.create_task_record(2, 0x400100).unwrap();
        let fresh = state.reserve_completion_wait(2).unwrap();
        assert_ne!(fresh.owner, wait.owner);
        assert_ne!(fresh.object, wait.object);
        state.schedule_task(2).unwrap();
        let current = state.block_completion_wait(fresh).unwrap();
        let before = snapshot(&state);
        assert_eq!(state.complete_completion_wait(ticket), Err(Error::Stale));
        assert_eq!(snapshot(&state), before);
        state.complete_completion_wait(current).unwrap();
        assert!(state.completion_waits_well_formed());
    }

    #[test]
    fn reservation_release_and_owner_destroy_preflight_audit_before_any_mutation() {
        let mut state = state();
        let reservation = MAX_AUDIT_EVENTS - state.audit.count;
        assert!(state.audit.reserve(reservation));
        let before = snapshot(&state);
        assert_eq!(state.reserve_completion_wait(2), Err(Error::AuditFull));
        assert_eq!(snapshot(&state), before);
        state.audit.release_reservation(reservation);
        let wait = state.reserve_completion_wait(2).unwrap();
        let reservation = MAX_AUDIT_EVENTS - state.audit.count;
        assert!(state.audit.reserve(reservation));
        let before = snapshot(&state);
        assert_eq!(state.release_completion_wait(wait), Err(Error::AuditFull));
        assert_eq!(snapshot(&state), before);
        state.audit.release_reservation(reservation);
        let ticket = state.block_completion_wait(wait).unwrap();
        let reservation = MAX_AUDIT_EVENTS - state.audit.count - state.destroy_audit_events(2) + 1;
        assert!(state.audit.reserve(reservation));
        let before = snapshot(&state);
        assert_eq!(state.destroy_task(2), Err(TaskError::AuditFull));
        assert_eq!(snapshot(&state), before);
        state.audit.release_reservation(reservation);
        state.destroy_task(2).unwrap();
        assert_eq!(state.complete_completion_wait(ticket), Err(Error::Stale));
        assert!(state.completion_waits_well_formed());
        assert!(state.audit.any(|event| matches!(event, AuditEvent::Completion { owner, action: Action::OwnerDestroyed, .. } if *owner == wait.owner)));
    }

    #[test]
    fn private_wait_projection_detects_a_missing_reverse_registration() {
        let mut state = state();
        let wait = state.reserve_completion_wait(2).unwrap();
        let ticket = state.block_completion_wait(wait).unwrap();
        assert!(state.completion_waits_well_formed());
        let task = *state.tasks.find(2).unwrap();
        state.tasks.find_mut(2).unwrap().wait_request = None;
        assert!(!state.completion_waits_well_formed());
        *state.tasks.find_mut(2).unwrap() = task;
        let object = state.wait_objects.find(wait.object).unwrap();
        let mut broken = object;
        broken.waiter_count = 0;
        broken.waiters[0] = None;
        state.wait_objects.write(broken);
        assert!(!state.completion_waits_well_formed());
        state.wait_objects.write(object);
        state.complete_completion_wait(ticket).unwrap();
        assert!(state.completion_waits_well_formed());
    }

    #[test]
    fn public_and_private_quotas_do_not_steal_each_others_capacity() {
        let mut state = state();
        let public: Vec<_> = (0..MAX_PUBLIC_WAIT_OBJECTS)
            .map(|_| state.wait_object_create(2).unwrap())
            .collect();
        assert_eq!(
            state.wait_object_create(2),
            Err(WaitError::CapacityExceeded)
        );
        let private: Vec<_> = (0..MAX_COMPLETION_WAITS)
            .map(|_| state.reserve_completion_wait(2).unwrap())
            .collect();
        assert_eq!(state.reserve_completion_wait(2), Err(Error::Capacity));
        assert_eq!(state.wait_objects.count, MAX_WAIT_OBJECTS);
        assert!(state.completion_waits_well_formed());
        // Public destruction cannot free a private quota slot, and vice versa.
        state.wait_object_destroy(2, public[0]).unwrap();
        assert_eq!(state.reserve_completion_wait(2), Err(Error::Capacity));
        state.wait_object_create(2).unwrap();
        state.release_completion_wait(private[0]).unwrap();
        assert_eq!(
            state.wait_object_create(2),
            Err(WaitError::CapacityExceeded)
        );
        let replacement = state.reserve_completion_wait(2).unwrap();
        assert_ne!(replacement.object, private[0].object);
        assert_eq!(state.wait_objects.count, MAX_WAIT_OBJECTS);
    }
}
