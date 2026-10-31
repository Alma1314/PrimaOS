//! S6-M1 fail-stop controller. The first diagnostic has its own arena cell,
//! reserved at boot, independent of the append-only audit pool. There is no reset
//! or drain API: only a new boot may resume after this resource failure.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Reserve,
    Block,
    Complete,
    Release,
    Destroy,
    /// S6-M1/M1b：门控绑定事务。
    Attach,
    /// S6-M1/M1b：被门控任务 trap 入队/容量等待。
    Trap,
    /// S6-M1/M1b：请求出队并交付给服务者。
    Deliver,
    /// S6-M1/M1b：请求终结（答复、中止或孤儿化）。
    Terminate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub operation: Operation,
    pub owner: u64,
    pub required: usize,
    pub used: usize,
    pub reserved: usize,
}

// AuditLog's already unique region selects an independent diagnostic cell.
// Reuse that identity so BootstrapState does not grow beyond its 13 KiB guard.
static DIAGNOSTICS: StaticCell<[Option<Diagnostic>; AUDIT_REGIONS]> =
    StaticCell::new([None; AUDIT_REGIONS]);

impl BootstrapState {
    pub(super) fn can_audit_task_destroy(&mut self, task: u32) -> bool {
        if self.gate_audit_diagnostic().is_some() {
            return false;
        }
        let cost = self.destroy_audit_events(task);
        if self.completion_wait_count(task) != 0 {
            return self
                .preflight_gate_audit(
                    cost,
                    self.task_cookie(task).unwrap_or(0),
                    Operation::Destroy,
                )
                .is_ok();
        }
        self.audit.can_append(cost)
    }

    pub fn gate_audit_diagnostic(&self) -> Option<Diagnostic> {
        DIAGNOSTICS.with(|cells| cells[self.audit.base / MAX_AUDIT_EVENTS])
    }

    /// Gate adapters must call this BEFORE any frame, capability, queue, memory
    /// or wait mutation. It does not itself reserve ordinary event slots; callers
    /// hold exclusive State access through their entire preflight/commit.
    pub fn preflight_gate_audit(
        &mut self,
        required: usize,
        owner: u64,
        operation: Operation,
    ) -> Result<(), completion::Error> {
        if self.gate_audit_diagnostic().is_some() {
            return Err(completion::Error::AuditFull);
        }
        if self.audit.can_append(required) {
            return Ok(());
        }
        DIAGNOSTICS.with_mut(|cells| {
            cells[self.audit.base / MAX_AUDIT_EVENTS] = Some(Diagnostic {
                operation,
                owner,
                required,
                used: self.audit.count,
                reserved: self.audit.reserved,
            })
        });
        Err(completion::Error::AuditFull)
    }
}

/// Check outside the State lock, before dispatch, wake processing or scheduling.
/// Uses the existing terminal kernel panic path (QEMU failure exit, CPU halt on
/// hardware); the dedicated record exists before any diagnostic output begins.
pub fn enforce_stop(_boundary: &'static str) {
    #[cfg(feature = "gate-audit-probe")]
    super::gate_audit_probe::before(_boundary);
    let diagnostic = BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref()
            .and_then(BootstrapState::gate_audit_diagnostic)
    });
    enforce_diagnostic(diagnostic);
    #[cfg(feature = "gate-audit-probe")]
    super::gate_audit_probe::after(_boundary);
}

fn enforce_diagnostic(diagnostic: Option<Diagnostic>) {
    if let Some(diagnostic) = diagnostic {
        panic!("GATE_AUDIT_FAIL_STOP: {diagnostic:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_controller_takes_the_terminal_path_and_instances_are_isolated() {
        let mut stopped = super::super::completion::tests::state();
        let healthy = super::super::completion::tests::state();
        let reservation = MAX_AUDIT_EVENTS - stopped.audit.count;
        assert!(stopped.audit.reserve(reservation));
        assert_eq!(
            stopped.preflight_gate_audit(1, 0, Operation::Reserve),
            Err(completion::Error::AuditFull)
        );
        enforce_diagnostic(healthy.gate_audit_diagnostic());
        assert!(
            std::panic::catch_unwind(|| enforce_diagnostic(stopped.gate_audit_diagnostic()))
                .is_err()
        );
        assert!(healthy.gate_audit_diagnostic().is_none());
    }

    #[test]
    fn first_exhaustion_is_preserved_after_pressure_is_removed() {
        let mut state = super::super::completion::tests::state();
        let wait = state.reserve_completion_wait(2).unwrap();
        let ticket = state.block_completion_wait(wait).unwrap();
        state.schedule_task(INIT_TASK_ID).unwrap();
        let tasks = state.tasks;
        let count = state.audit.count;
        let reservation = MAX_AUDIT_EVENTS - count;
        assert!(state.audit.reserve(reservation));
        assert_eq!(
            state.complete_completion_wait(ticket),
            Err(completion::Error::AuditFull)
        );
        let first = state.gate_audit_diagnostic().unwrap();
        assert_eq!(first.operation, Operation::Complete);
        assert_eq!(first.required, 2);
        assert_eq!(first.used, count);
        assert_eq!(first.reserved, reservation);
        assert_eq!(state.tasks, tasks);
        assert!(state.completion_waits_well_formed());
        state.audit.release_reservation(reservation);
        assert_eq!(
            state.preflight_gate_audit(0, 0, Operation::Reserve),
            Err(completion::Error::AuditFull)
        );
        assert_eq!(
            state.release_completion_wait(wait),
            Err(completion::Error::AuditFull)
        );
        assert_eq!(
            state.complete_completion_wait(ticket),
            Err(completion::Error::AuditFull)
        );
        assert_eq!(state.schedule_task(INIT_TASK_ID), Err(TaskError::AuditFull));
        assert_eq!(state.yield_task(INIT_TASK_ID), Err(TaskError::AuditFull));
        assert_eq!(state.gate_audit_diagnostic(), Some(first));
        assert_eq!(state.audit.count, count);
        assert_eq!(state.tasks, tasks);
        assert!(state.completion_waits_well_formed());
    }
}
