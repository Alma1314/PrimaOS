//! Managed initial launch. Prepared images cannot be scheduled or released by
//! an upcall. Gate installation and publication use one kernel transaction.
use super::gate::Error;
use super::*;

impl BootstrapState {
    pub fn task_launch_held(&self, task: u32) -> bool {
        self.tasks
            .find(task)
            .is_some_and(|t| t.launch_held && t.status == TaskStatus::Prepared)
    }

    pub fn task_create_held(
        &mut self,
        caller: u32,
        req: &TaskCreateRequest,
        root: u64,
        offset: u64,
    ) -> Result<TaskCreateOutcome, TaskCreateError> {
        if req.initial_caps != [Handle::ZERO; 2] {
            return Err(TaskCreateError::InvalidInitialCap);
        }
        self.preflight_gate_audit(
            8,
            self.task_cookie(caller).unwrap_or(0),
            gate_audit::Operation::Control,
        )
        .map_err(|_| TaskCreateError::AuditFull)?;
        let made = self.task_create(caller, req, root, offset)?;
        let task = self
            .tasks
            .find_mut(made.task_id)
            .expect("created held Task");
        task.launch_held = true;
        task.status = TaskStatus::Prepared;
        self.audit_transition(made.task_id, TaskStatus::Prepared);
        Ok(made)
    }

    pub fn managed_task(&self, caller: u32, cap: Handle) -> Result<u32, Error> {
        self.authorize(caller, cap, Right::TaskManageVSpace)
            .map_err(|_| Error::Denied)?;
        match self.target_of(cap) {
            Some(ObjectRef::Task(task)) if self.tasks.is_live(task) => Ok(task),
            _ => Err(Error::Stale),
        }
    }

    pub fn held_task(&self, caller: u32, cap: Handle) -> Result<u32, Error> {
        let task = self.managed_task(caller, cap)?;
        if !self.task_launch_held(task) || self.task_is_gated(task) {
            return Err(Error::Busy);
        }
        Ok(task)
    }

    pub fn preflight_held_image(
        &mut self,
        caller: u32,
        cap: Handle,
        pages: usize,
    ) -> Result<u32, Error> {
        let task = self.held_task(caller, cap)?;
        self.check_exec(task, pages).map_err(|_| Error::Capacity)?;
        self.preflight_gate_audit(
            8 + 3 * self.private_exec_segments.count_ones() as usize,
            self.task_cookie(task).unwrap(),
            gate_audit::Operation::Control,
        )
        .map_err(|_| Error::AuditFull)?;
        Ok(task)
    }

    pub fn start_held_task(
        &mut self,
        caller: u32,
        cap: Handle,
        gate: Handle,
        orphan: u64,
        gated: bool,
    ) -> Result<Handle, Error> {
        let task = self.held_task(caller, cap)?;
        self.preflight_gate_audit(
            if gated {
                if gate.is_zero() { 6 } else { 3 }
            } else {
                1
            },
            self.task_cookie(task).unwrap(),
            gate_audit::Operation::Attach,
        )
        .map_err(|_| Error::AuditFull)?;
        // Only Ready is admitted by v1 attach. This transient status is never
        // visible to the scheduler, and is rolled back on every attach error.
        self.tasks.find_mut(task).unwrap().status = TaskStatus::Ready;
        let result = if gated {
            self.gate_attach(caller, cap, gate, orphan)
                .map(|a| a.gate_cap)
        } else {
            Ok(Handle::ZERO)
        };
        if result.is_err() {
            self.tasks.find_mut(task).unwrap().status = TaskStatus::Prepared;
            return result;
        }
        self.tasks.find_mut(task).unwrap().launch_held = false;
        self.audit_transition(task, TaskStatus::Ready);
        result
    }

    pub fn set_managed_tls(&mut self, caller: u32, cap: Handle, base: u64) -> Result<(), Error> {
        let task = self.managed_task(caller, cap)?;
        if crate::syscall::validate_fs_base(base).is_err()
            || !self.gate_vm_mutation_permitted(caller, task)
            || !(self.task_launch_held(task)
                || self.tasks.status_of(task) == Some(TaskStatus::Ready)
                || self.task_is_gated(task))
        {
            return Err(Error::Busy);
        }
        self.preflight_gate_audit(
            1,
            self.task_cookie(task).unwrap(),
            gate_audit::Operation::Control,
        )
        .map_err(|_| Error::AuditFull)?;
        crate::sched::set_fs_base(task, base);
        self.audit.append(AuditEvent::TaskTlsSet {
            caller,
            task_cookie: self.task_cookie(task).unwrap(),
            base,
        });
        Ok(())
    }

    pub fn current_namespace(&self, caller: u32) -> Option<Handle> {
        self.tasks
            .find(caller)
            .map(|t| t.root_ns_cap)
            .filter(|c| !c.is_zero())
    }

    pub(crate) fn record_held_image(&mut self, caller: u32, task: u32, entry: u64) {
        self.audit.append(AuditEvent::TaskImageLoaded {
            caller,
            task_cookie: self.task_cookie(task).unwrap(),
            entry,
        });
    }
}
