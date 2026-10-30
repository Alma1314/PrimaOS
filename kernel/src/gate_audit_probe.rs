//! Opt-in bare-metal failure injection. No syscall, user flag or release-image
//! trigger can activate this probe. The existing stop check remains responsible
//! for halting; a missing check emits `escaped` and cannot pass the runner.
use super::*;
use crate::exc::SretFrame;
use gate_audit::Diagnostic;

const SITE: Option<&str> = option_env!("PRIMA_GATE_AUDIT_SITE");

#[derive(Clone, Copy)]
struct Snapshot {
    site: &'static str,
    task: TaskRecord,
    object: WaitObject,
    frame: SretFrame,
    diagnostic: Diagnostic,
}

/// Evidence is emitted only after the real tables and saved scheduler frame
/// have been compared with the pre-failure snapshot, outside the State lock.
#[derive(Clone, Copy)]
pub struct Evidence {
    pub site: &'static str,
    pub diagnostic: Diagnostic,
}

type Observer = fn(&str, Evidence);
static OBSERVER: StaticCell<Option<Observer>> = StaticCell::new(None);
static EXPECTED: StaticCell<Option<Snapshot>> = StaticCell::new(None);

pub fn set_observer(observer: Observer) {
    OBSERVER.with_mut(|slot| *slot = Some(observer));
}

fn emit(phase: &str, snapshot: Snapshot) {
    if let Some(observer) = OBSERVER.with(|slot| *slot) {
        observer(
            phase,
            Evidence {
                site: snapshot.site,
                diagnostic: snapshot.diagnostic,
            },
        );
    }
}

pub(super) fn before(site: &'static str) {
    if SITE != Some(site) || EXPECTED.with(|slot| slot.is_some()) {
        return;
    }
    let owner = crate::sched::current();
    let frame = crate::sched::frame(owner).expect("probe owner has saved frame");
    let snapshot = BOOTSTRAP_STATE.with_mut(|slot| {
        let state = slot.as_mut().expect("probe after bootstrap install");
        // Some scheduler entries follow a native blocking operation. Wait for
        // a Running owner rather than rewriting an unrelated native wait.
        if state.tasks.status_of(owner) != Some(TaskStatus::Running) {
            return None;
        }
        let wait = state.reserve_completion_wait(owner).expect("probe reserve");
        let ticket = state.block_completion_wait(wait).expect("probe block");
        let task = *state.tasks.find(owner).expect("probe owner");
        let cookie = state.task_cookie(owner).expect("probe cookie");
        let object = state
            .wait_objects
            .iter()
            .find(|object| object.completion_owner == Some(cookie))
            .expect("probe private wait");
        // Leave ONE unreserved event, while completion needs TWO. This uses
        // the real bounded log's reservation accounting, not a mocked error.
        let used = state.audit.count;
        let reserved = MAX_AUDIT_EVENTS - used - state.audit.reserved - 1;
        assert!(state.audit.reserve(reserved));
        assert_eq!(
            state.complete_completion_wait(ticket),
            Err(completion::Error::AuditFull)
        );
        let diagnostic = state.gate_audit_diagnostic().expect("first diagnostic");
        assert_eq!(diagnostic.operation, gate_audit::Operation::Complete);
        assert_eq!(diagnostic.owner, cookie);
        assert_eq!(diagnostic.required, 2);
        assert_eq!(diagnostic.used, used);
        assert_eq!(diagnostic.used + diagnostic.reserved, MAX_AUDIT_EVENTS - 1);
        assert_eq!(state.tasks.find(owner), Some(&task));
        assert!(state.completion_waits_well_formed());
        assert_eq!(state.wait_objects.find(object.id), Some(object));
        assert_eq!(state.audit.count, used);
        // Ordinary pressure relief cannot revive the old continuation. Keep
        // the sealed diagnostic, and reject release/schedule/yield as well.
        state.audit.release_reservation(reserved);
        assert_eq!(
            state.complete_completion_wait(ticket),
            Err(completion::Error::AuditFull)
        );
        assert_eq!(
            state.release_completion_wait(wait),
            Err(completion::Error::AuditFull)
        );
        assert_eq!(state.schedule_task(owner), Err(TaskError::AuditFull));
        assert_eq!(state.yield_task(owner), Err(TaskError::AuditFull));
        assert_eq!(state.gate_audit_diagnostic(), Some(diagnostic));
        Some(Snapshot {
            site,
            task,
            object,
            frame,
            diagnostic,
        })
    });
    if let Some(snapshot) = snapshot {
        EXPECTED.with_mut(|slot| *slot = Some(snapshot));
        assert!(unchanged(snapshot));
        emit("armed", snapshot);
    }
}

fn unchanged(snapshot: Snapshot) -> bool {
    let tables = BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref().is_some_and(|state| {
            state.gate_audit_diagnostic() == Some(snapshot.diagnostic)
                && state.audit.count == snapshot.diagnostic.used
                && state.tasks.find(snapshot.task.id) == Some(&snapshot.task)
                && state.wait_objects.find(snapshot.object.id) == Some(snapshot.object)
                && state.completion_waits_well_formed()
        })
    });
    tables && crate::sched::frame(snapshot.task.id) == Some(snapshot.frame)
}

pub(super) fn after(site: &'static str) {
    if SITE == Some(site)
        && let Some(snapshot) = EXPECTED.with_mut(Option::take)
    {
        emit("escaped", snapshot);
        panic!("audit-stop guard returned after exhaustion");
    }
}

/// Called by the existing terminal panic handler. An unrelated panic or an
/// escaped boundary has no valid stopped record, so exit=35 alone never passes.
pub fn panic_evidence() {
    if let Some(snapshot) = EXPECTED.with(|slot| *slot)
        && unchanged(snapshot)
    {
        emit("stopped", snapshot);
    }
}
