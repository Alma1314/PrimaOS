use core::fmt;

use crate::mem::FrameInventory;

pub const INIT_TASK_ID: u32 = 1;
pub const IMAGE_CAP_ID: u32 = 1;
pub const NAMESPACE_CAP_ID: u32 = 2;
pub const TASK_CAP_ID: u32 = 3;
pub const INITIAL_CAPABILITY_COUNT: u32 = 3;
pub const IMAGE_RIGHTS: &str = "Derive,SegMapR,SegMapX,SegDestroy";
pub const NAMESPACE_RIGHTS: &str = "all";
pub const TASK_RIGHTS: &str = "Derive,TaskSend,TaskManageVSpace,TaskDestroy";
const MAX_DERIVED_CAPABILITIES: usize = 8;
const MAX_CAPABILITY_POSITIONS: usize =
    INITIAL_CAPABILITY_COUNT as usize + MAX_DERIVED_CAPABILITIES;
const MAX_TASKS: usize = 8;
const MAX_AUDIT_EVENTS: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditEvent {
    MessageSent {
        message_id: u32,
        sender: u32,
        receiver: u32,
    },
    MessageReceived {
        message_id: u32,
        receiver: u32,
    },
    MessageDropped {
        message_id: u32,
    },
    TaskDestroyed {
        task_id: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditLog {
    pub entries: [Option<AuditEvent>; MAX_AUDIT_EVENTS],
    pub count: usize,
}

impl AuditLog {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_AUDIT_EVENTS],
            count: 0,
        }
    }

    fn can_append(&self, count: usize) -> bool {
        self.count.saturating_add(count) <= MAX_AUDIT_EVENTS
    }

    fn append(&mut self, event: AuditEvent) {
        self.entries[self.count] = Some(event);
        self.count += 1;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Ready,
    Running,
    BlockedRecv,
    Dead,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: u32,
    pub status: TaskStatus,
    pending: Option<IpcMessage>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskError {
    NoSuchTask,
    DuplicateTask,
    CapacityExceeded,
    AlreadyRunning,
    NotRunning,
    NotReady,
    CannotDestroyInit,
    CannotDestroyRunning,
    AuditFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskTable {
    pub entries: [Option<TaskRecord>; MAX_TASKS],
    pub count: usize,
}

impl TaskTable {
    const fn with_init() -> Self {
        Self {
            entries: [
                Some(TaskRecord {
                    id: INIT_TASK_ID,
                    status: TaskStatus::Running,
                    pending: None,
                }),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            count: 1,
        }
    }

    fn find(&self, task_id: u32) -> Option<&TaskRecord> {
        self.entries[..self.count]
            .iter()
            .flatten()
            .find(|task| task.id == task_id)
    }

    fn find_mut(&mut self, task_id: u32) -> Option<&mut TaskRecord> {
        self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|task| task.id == task_id)
    }

    pub fn create(&mut self, task_id: u32) -> Result<(), TaskError> {
        if self.find(task_id).is_some() {
            return Err(TaskError::DuplicateTask);
        }
        if self.count == MAX_TASKS {
            return Err(TaskError::CapacityExceeded);
        }
        self.entries[self.count] = Some(TaskRecord {
            id: task_id,
            status: TaskStatus::Ready,
            pending: None,
        });
        self.count += 1;
        Ok(())
    }

    pub fn schedule(&mut self, task_id: u32) -> Result<(), TaskError> {
        if self.entries[..self.count]
            .iter()
            .flatten()
            .any(|task| task.status == TaskStatus::Running)
        {
            return Err(TaskError::AlreadyRunning);
        }
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.status != TaskStatus::Ready {
            return Err(TaskError::NotReady);
        }
        task.status = TaskStatus::Running;
        Ok(())
    }

    pub fn yield_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.status != TaskStatus::Running {
            return Err(TaskError::NotRunning);
        }
        task.status = TaskStatus::Ready;
        Ok(())
    }

    fn prepare_destroy(&self, task_id: u32) -> Result<(), TaskError> {
        if task_id == INIT_TASK_ID {
            return Err(TaskError::CannotDestroyInit);
        }
        let task = self.find(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.status == TaskStatus::Running {
            return Err(TaskError::CannotDestroyRunning);
        }
        Ok(())
    }

    fn finish_destroy(&mut self, task_id: u32) -> Result<(), TaskError> {
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        task.status = TaskStatus::Dead;
        Ok(())
    }

    fn is_live(&self, task_id: u32) -> bool {
        self.find(task_id)
            .is_some_and(|task| task.status != TaskStatus::Dead)
    }

    fn block_recv(&mut self, task_id: u32) -> Result<(), TaskError> {
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.status == TaskStatus::Running || task.status == TaskStatus::Ready {
            task.status = TaskStatus::BlockedRecv;
            return Ok(());
        }
        Err(TaskError::NotRunning)
    }

    fn wake_recv(&mut self, task_id: u32) -> Result<(), TaskError> {
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.status == TaskStatus::BlockedRecv {
            task.status = TaskStatus::Ready;
        }
        Ok(())
    }

    fn is_blocked_recv(&self, task_id: u32) -> bool {
        self.find(task_id)
            .is_some_and(|task| task.status == TaskStatus::BlockedRecv)
    }

    fn set_pending(&mut self, task_id: u32, message: IpcMessage) -> Result<(), TaskError> {
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        task.pending = Some(message);
        if task.status == TaskStatus::BlockedRecv {
            task.status = TaskStatus::Ready;
        }
        Ok(())
    }

    fn take_pending(&mut self, task_id: u32) -> Option<IpcMessage> {
        self.find_mut(task_id).and_then(|task| task.pending.take())
    }
}

const MAX_MESSAGES: usize = 8;
const MAX_MESSAGE_CAPABILITIES: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpcMessage {
    pub id: u32,
    pub sender: u32,
    pub receiver: u32,
    pub payload: u64,
    pub capabilities: [u32; MAX_MESSAGE_CAPABILITIES],
    pub capability_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcError {
    NoSuchTask,
    NotAuthorized,
    TooManyCapabilities,
    InvalidCapability,
    CapabilityHolderMismatch,
    AuditFull,
    QueueFull,
    QueueEmpty,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpcQueue {
    pub entries: [Option<IpcMessage>; MAX_MESSAGES],
    pub count: usize,
    next_id: u32,
}

impl IpcQueue {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_MESSAGES],
            count: 0,
            next_id: 1,
        }
    }

    fn allocate_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        id
    }

    fn send(&mut self, message: IpcMessage) -> Result<(), IpcError> {
        if self.count == MAX_MESSAGES {
            return Err(IpcError::QueueFull);
        }
        self.entries[self.count] = Some(message);
        self.count += 1;
        Ok(())
    }

    fn receive(&mut self, receiver: u32) -> Result<IpcMessage, IpcError> {
        let index = self.entries[..self.count]
            .iter()
            .position(|entry| entry.is_some_and(|message| message.receiver == receiver))
            .ok_or(IpcError::QueueEmpty)?;
        let message = self.entries[index].take().ok_or(IpcError::QueueEmpty)?;
        for position in index..self.count - 1 {
            self.entries[position] = self.entries[position + 1];
        }
        self.entries[self.count - 1] = None;
        self.count -= 1;
        Ok(message)
    }

    fn remove_task(&mut self, task_id: u32) {
        let mut position = 0;
        while position < self.count {
            let remove = self.entries[position]
                .is_some_and(|message| message.sender == task_id || message.receiver == task_id);
            if remove {
                for shift in position..self.count - 1 {
                    self.entries[shift] = self.entries[shift + 1];
                }
                self.entries[self.count - 1] = None;
                self.count -= 1;
            } else {
                position += 1;
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapSnapshot {
    pub task_id: u32,
    pub image_cap_id: u32,
    pub namespace_cap_id: u32,
    pub task_cap_id: u32,
    pub image_base: u64,
    pub image_len: u64,
    pub image_frames: u64,
    pub free_frames: u64,
    pub image_rights: &'static str,
    pub namespace_rights: &'static str,
    pub task_rights: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialObject {
    ImageSegment,
    Namespace,
    InitTask,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitialCapability {
    pub id: u32,
    pub object: InitialObject,
    pub holder: u32,
    pub rights: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityLocation {
    HeldByTask(u32),
    InMessage(u32),
    Dropped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityPosition {
    pub cap_id: u32,
    pub location: CapabilityLocation,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityPositionTable {
    pub entries: [Option<CapabilityPosition>; MAX_CAPABILITY_POSITIONS],
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DerivedCapability {
    pub id: u32,
    pub parent: u32,
    pub holder: u32,
    pub rights: &'static str,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeriveError {
    NoSuchParent,
    DuplicateCapability,
    RightsEscalation,
    CapacityExceeded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityDerivationTable {
    pub entries: [Option<DerivedCapability>; MAX_DERIVED_CAPABILITIES],
    pub count: usize,
}

impl CapabilityDerivationTable {
    pub const fn empty() -> Self {
        Self {
            entries: [None; MAX_DERIVED_CAPABILITIES],
            count: 0,
        }
    }

    pub fn derive_from_initial(
        &mut self,
        initial: &[InitialCapability; INITIAL_CAPABILITY_COUNT as usize],
        parent: u32,
        child: u32,
        holder: u32,
        rights: &'static str,
    ) -> Result<(), DeriveError> {
        let parent_rights = initial
            .iter()
            .find(|capability| capability.id == parent)
            .map(|capability| capability.rights)
            .or_else(|| {
                self.entries[..self.count]
                    .iter()
                    .flatten()
                    .find(|capability| capability.id == parent && capability.active)
                    .map(|capability| capability.rights)
            })
            .ok_or(DeriveError::NoSuchParent)?;
        if !rights_subset(parent_rights, rights) {
            return Err(DeriveError::RightsEscalation);
        }
        if initial.iter().any(|capability| capability.id == child)
            || self.entries[..self.count]
                .iter()
                .flatten()
                .any(|capability| capability.id == child)
        {
            return Err(DeriveError::DuplicateCapability);
        }
        if self.count == MAX_DERIVED_CAPABILITIES {
            return Err(DeriveError::CapacityExceeded);
        }
        self.entries[self.count] = Some(DerivedCapability {
            id: child,
            parent,
            holder,
            rights,
            active: true,
        });
        self.count += 1;
        Ok(())
    }

    fn revoke_subtree(&mut self, root: u32) {
        let mut revoked = [0u32; MAX_DERIVED_CAPABILITIES];
        let mut revoked_count = 1;
        revoked[0] = root;
        if let Some(entry) = self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|capability| capability.id == root)
        {
            entry.active = false;
        }
        loop {
            let mut changed = false;
            for entry in self.entries[..self.count].iter_mut().flatten() {
                if !entry.active || !revoked[..revoked_count].contains(&entry.parent) {
                    continue;
                }
                entry.active = false;
                if revoked_count < MAX_DERIVED_CAPABILITIES {
                    revoked[revoked_count] = entry.id;
                    revoked_count += 1;
                }
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }
}

fn rights_subset(parent: &str, child: &str) -> bool {
    if parent == "all" {
        return child != "all";
    }
    if child == "all" {
        return false;
    }
    child
        .split(',')
        .all(|right| parent.split(',').any(|parent_right| parent_right == right))
}

fn has_right(rights: &str, wanted: &str) -> bool {
    rights == "all" || rights.split(',').any(|right| right == wanted)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityPositionError {
    NoSuchCapability,
    DuplicateCapability,
    CapacityExceeded,
    ProtectedTask,
    HolderMismatch,
    Inactive,
}

impl CapabilityPositionTable {
    fn from_capabilities(
        capabilities: &[InitialCapability; INITIAL_CAPABILITY_COUNT as usize],
    ) -> Self {
        Self {
            entries: [
                Some(CapabilityPosition {
                    cap_id: capabilities[0].id,
                    location: CapabilityLocation::HeldByTask(capabilities[0].holder),
                    active: true,
                }),
                Some(CapabilityPosition {
                    cap_id: capabilities[1].id,
                    location: CapabilityLocation::HeldByTask(capabilities[1].holder),
                    active: true,
                }),
                Some(CapabilityPosition {
                    cap_id: capabilities[2].id,
                    location: CapabilityLocation::HeldByTask(capabilities[2].holder),
                    active: true,
                }),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
            count: INITIAL_CAPABILITY_COUNT as usize,
        }
    }

    pub fn register_derived(
        &mut self,
        cap_id: u32,
        holder: u32,
    ) -> Result<(), CapabilityPositionError> {
        if self.entries[..self.count]
            .iter()
            .flatten()
            .any(|entry| entry.cap_id == cap_id)
        {
            return Err(CapabilityPositionError::DuplicateCapability);
        }
        if self.count == MAX_CAPABILITY_POSITIONS {
            return Err(CapabilityPositionError::CapacityExceeded);
        }
        self.entries[self.count] = Some(CapabilityPosition {
            cap_id,
            location: CapabilityLocation::HeldByTask(holder),
            active: true,
        });
        self.count += 1;
        Ok(())
    }

    pub fn has_unique_active_locations(&self) -> bool {
        self.entries[..self.count]
            .iter()
            .flatten()
            .enumerate()
            .all(|(index, entry)| {
                !entry.active
                    || self.entries[..index]
                        .iter()
                        .flatten()
                        .all(|previous| !previous.active || previous.cap_id != entry.cap_id)
            })
            && self.entries[..self.count]
                .iter()
                .flatten()
                .all(|entry| entry.active || entry.location == CapabilityLocation::Dropped)
    }

    pub fn move_to_task(
        &mut self,
        cap_id: u32,
        from_task: u32,
        to_task: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|entry| entry.cap_id == cap_id)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.active {
            return Err(CapabilityPositionError::Inactive);
        }
        if entry.location != CapabilityLocation::HeldByTask(from_task) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::HeldByTask(to_task);
        Ok(())
    }

    pub fn drop_from_task(
        &mut self,
        cap_id: u32,
        holder: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|entry| entry.cap_id == cap_id)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.active {
            return Err(CapabilityPositionError::Inactive);
        }
        if entry.location != CapabilityLocation::HeldByTask(holder) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::Dropped;
        entry.active = false;
        Ok(())
    }

    fn move_to_message(
        &mut self,
        cap_id: u32,
        holder: u32,
        message_id: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|entry| entry.cap_id == cap_id)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.active {
            return Err(CapabilityPositionError::Inactive);
        }
        if entry.location != CapabilityLocation::HeldByTask(holder) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::InMessage(message_id);
        Ok(())
    }

    fn move_from_message(
        &mut self,
        cap_id: u32,
        message_id: u32,
        receiver: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|entry| entry.cap_id == cap_id)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.active {
            return Err(CapabilityPositionError::Inactive);
        }
        if entry.location != CapabilityLocation::InMessage(message_id) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::HeldByTask(receiver);
        Ok(())
    }

    fn revoke(&mut self, cap_id: u32) {
        if let Some(entry) = self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|entry| entry.cap_id == cap_id)
        {
            entry.location = CapabilityLocation::Dropped;
            entry.active = false;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameOwnership {
    pub image_start: u64,
    pub image_end: u64,
    pub image_frames: u64,
    pub free_frames: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapState {
    pub snapshot: BootstrapSnapshot,
    pub capabilities: [InitialCapability; INITIAL_CAPABILITY_COUNT as usize],
    pub positions: CapabilityPositionTable,
    pub derivations: CapabilityDerivationTable,
    pub tasks: TaskTable,
    pub ipc: IpcQueue,
    pub audit: AuditLog,
    pub frame_ownership: FrameOwnership,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapError {
    EmptyImage,
    ImageRangeOverflow,
    InvalidFrameOwnership,
}

impl BootstrapState {
    pub fn derive_capability(
        &mut self,
        parent: u32,
        child: u32,
        holder: u32,
        rights: &'static str,
    ) -> Result<(), DeriveError> {
        self.derivations
            .derive_from_initial(&self.capabilities, parent, child, holder, rights)?;
        self.positions
            .register_derived(child, holder)
            .map_err(|error| match error {
                CapabilityPositionError::DuplicateCapability => DeriveError::DuplicateCapability,
                CapabilityPositionError::CapacityExceeded => DeriveError::CapacityExceeded,
                CapabilityPositionError::NoSuchCapability
                | CapabilityPositionError::HolderMismatch
                | CapabilityPositionError::Inactive
                | CapabilityPositionError::ProtectedTask => DeriveError::CapacityExceeded,
            })
    }

    pub fn revoke_capability(
        &mut self,
        cap_id: u32,
        holder: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self.positions.entries[..self.positions.count]
            .iter()
            .flatten()
            .find(|entry| entry.cap_id == cap_id)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.active {
            return Err(CapabilityPositionError::Inactive);
        }
        if entry.location != CapabilityLocation::HeldByTask(holder) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        self.positions.revoke(cap_id);
        self.derivations.revoke_subtree(cap_id);
        for capability in self.derivations.entries[..self.derivations.count]
            .iter()
            .flatten()
        {
            if !capability.active {
                self.positions.revoke(capability.id);
            }
        }
        Ok(())
    }

    pub fn revoke_task_capabilities(
        &mut self,
        task_id: u32,
    ) -> Result<usize, CapabilityPositionError> {
        if task_id == INIT_TASK_ID {
            return Err(CapabilityPositionError::ProtectedTask);
        }
        let mut capability_ids = [0u32; MAX_CAPABILITY_POSITIONS];
        let mut capability_count = 0;
        for entry in self.positions.entries[..self.positions.count]
            .iter()
            .flatten()
        {
            if entry.active
                && entry.location == CapabilityLocation::HeldByTask(task_id)
                && capability_count < MAX_CAPABILITY_POSITIONS
            {
                capability_ids[capability_count] = entry.cap_id;
                capability_count += 1;
            }
        }
        let mut revoked_roots = 0;
        for cap_id in capability_ids[..capability_count].iter().copied() {
            if self.positions.entries[..self.positions.count]
                .iter()
                .flatten()
                .any(|entry| entry.active && entry.cap_id == cap_id)
            {
                self.revoke_capability(cap_id, task_id)?;
                revoked_roots += 1;
            }
        }
        Ok(revoked_roots)
    }

    pub fn create_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        self.tasks.create(task_id)
    }

    pub fn schedule_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        self.tasks.schedule(task_id)
    }

    pub fn yield_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        self.tasks.yield_task(task_id)
    }

    pub fn destroy_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        self.tasks.prepare_destroy(task_id)?;
        let pending_message = self.tasks.find(task_id).and_then(|task| task.pending);
        let mut dropped_message_ids = [0u32; MAX_MESSAGES];
        let mut dropped_count = 0;
        for message in self.ipc.entries[..self.ipc.count].iter().flatten() {
            if (message.sender == task_id || message.receiver == task_id)
                && dropped_count < MAX_MESSAGES
            {
                dropped_message_ids[dropped_count] = message.id;
                dropped_count += 1;
            }
        }
        let audit_events = dropped_count + if pending_message.is_some() { 1 } else { 0 } + 1;
        if !self.audit.can_append(audit_events) {
            return Err(TaskError::AuditFull);
        }
        self.revoke_task_capabilities(task_id)
            .map_err(|_| TaskError::NoSuchTask)?;
        for message in self.ipc.entries[..self.ipc.count].iter().flatten() {
            if message.sender == task_id || message.receiver == task_id {
                for cap_id in message.capabilities[..message.capability_count]
                    .iter()
                    .copied()
                {
                    self.positions.revoke(cap_id);
                }
            }
        }
        let _ = self.tasks.take_pending(task_id);
        self.ipc.remove_task(task_id);
        self.tasks.finish_destroy(task_id)?;
        for message_id in dropped_message_ids[..dropped_count].iter().copied() {
            self.audit.append(AuditEvent::MessageDropped { message_id });
        }
        if let Some(message) = pending_message {
            self.audit.append(AuditEvent::MessageDropped {
                message_id: message.id,
            });
        }
        self.audit.append(AuditEvent::TaskDestroyed { task_id });
        Ok(())
    }

    pub fn send_message(
        &mut self,
        sender: u32,
        receiver: u32,
        payload: u64,
    ) -> Result<(), IpcError> {
        self.send_message_with_capabilities(sender, receiver, payload, &[])
    }

    pub fn send_message_with_capabilities(
        &mut self,
        sender: u32,
        receiver: u32,
        payload: u64,
        capability_ids: &[u32],
    ) -> Result<(), IpcError> {
        if !self.tasks.is_live(sender) || !self.tasks.is_live(receiver) {
            return Err(IpcError::NoSuchTask);
        }
        if !self.has_task_send_authority(sender) {
            return Err(IpcError::NotAuthorized);
        }
        if capability_ids.len() > MAX_MESSAGE_CAPABILITIES {
            return Err(IpcError::TooManyCapabilities);
        }
        let mut capabilities = [0u32; MAX_MESSAGE_CAPABILITIES];
        for (index, cap_id) in capability_ids.iter().copied().enumerate() {
            if capabilities[..index].contains(&cap_id) {
                return Err(IpcError::InvalidCapability);
            }
            let position = self.positions.entries[..self.positions.count]
                .iter()
                .flatten()
                .find(|entry| entry.cap_id == cap_id)
                .ok_or(IpcError::InvalidCapability)?;
            if !position.active {
                return Err(IpcError::InvalidCapability);
            }
            if position.location != CapabilityLocation::HeldByTask(sender) {
                return Err(IpcError::CapabilityHolderMismatch);
            }
            capabilities[index] = cap_id;
        }
        let direct_delivery = self.tasks.is_blocked_recv(receiver);
        if !direct_delivery && self.ipc.count == MAX_MESSAGES {
            return Err(IpcError::QueueFull);
        }
        let audit_events = if direct_delivery { 2 } else { 1 };
        if !self.audit.can_append(audit_events) {
            return Err(IpcError::AuditFull);
        }
        let message = IpcMessage {
            id: self.ipc.allocate_id(),
            sender,
            receiver,
            payload,
            capabilities,
            capability_count: capability_ids.len(),
        };
        if direct_delivery {
            for cap_id in capabilities[..capability_ids.len()].iter().copied() {
                self.positions
                    .move_to_task(cap_id, sender, receiver)
                    .map_err(|error| match error {
                        CapabilityPositionError::HolderMismatch => {
                            IpcError::CapabilityHolderMismatch
                        }
                        CapabilityPositionError::NoSuchCapability
                        | CapabilityPositionError::Inactive
                        | CapabilityPositionError::DuplicateCapability
                        | CapabilityPositionError::CapacityExceeded
                        | CapabilityPositionError::ProtectedTask => IpcError::InvalidCapability,
                    })?;
            }
            self.tasks
                .set_pending(receiver, message)
                .map_err(|_| IpcError::NoSuchTask)?;
            self.audit.append(AuditEvent::MessageSent {
                message_id: message.id,
                sender,
                receiver,
            });
            self.audit.append(AuditEvent::MessageReceived {
                message_id: message.id,
                receiver,
            });
        } else {
            for cap_id in capabilities[..capability_ids.len()].iter().copied() {
                self.positions
                    .move_to_message(cap_id, sender, message.id)
                    .map_err(|error| match error {
                        CapabilityPositionError::HolderMismatch => {
                            IpcError::CapabilityHolderMismatch
                        }
                        CapabilityPositionError::NoSuchCapability
                        | CapabilityPositionError::Inactive
                        | CapabilityPositionError::DuplicateCapability
                        | CapabilityPositionError::CapacityExceeded
                        | CapabilityPositionError::ProtectedTask => IpcError::InvalidCapability,
                    })?;
            }
            self.ipc.send(message)?;
            self.audit.append(AuditEvent::MessageSent {
                message_id: message.id,
                sender,
                receiver,
            });
        }
        Ok(())
    }

    pub fn receive_message(&mut self, receiver: u32) -> Result<IpcMessage, IpcError> {
        if !self.tasks.is_live(receiver) {
            return Err(IpcError::NoSuchTask);
        }
        if let Some(message) = self.tasks.take_pending(receiver) {
            return Ok(message);
        }
        if !self.audit.can_append(1) {
            return Err(IpcError::AuditFull);
        }
        match self.ipc.receive(receiver) {
            Ok(message) => {
                for cap_id in message.capabilities[..message.capability_count]
                    .iter()
                    .copied()
                {
                    self.positions
                        .move_from_message(cap_id, message.id, receiver)
                        .map_err(|_| IpcError::InvalidCapability)?;
                }
                self.audit.append(AuditEvent::MessageReceived {
                    message_id: message.id,
                    receiver,
                });
                let _ = self.tasks.wake_recv(receiver);
                Ok(message)
            }
            Err(IpcError::QueueEmpty) => {
                let _ = self.tasks.block_recv(receiver);
                Err(IpcError::QueueEmpty)
            }
            Err(error) => Err(error),
        }
    }

    fn has_task_send_authority(&self, task_id: u32) -> bool {
        self.positions.entries[..self.positions.count]
            .iter()
            .flatten()
            .filter(|entry| {
                entry.active && entry.location == CapabilityLocation::HeldByTask(task_id)
            })
            .any(|entry| {
                self.capabilities
                    .iter()
                    .find(|capability| capability.id == entry.cap_id)
                    .map(|capability| has_right(capability.rights, "TaskSend"))
                    .or_else(|| {
                        self.derivations.entries[..self.derivations.count]
                            .iter()
                            .flatten()
                            .find(|capability| capability.id == entry.cap_id)
                            .map(|capability| {
                                capability.active && has_right(capability.rights, "TaskSend")
                            })
                    })
                    .unwrap_or(false)
            })
    }
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImage => formatter.write_str("empty image"),
            Self::ImageRangeOverflow => formatter.write_str("image range overflow"),
            Self::InvalidFrameOwnership => formatter.write_str("invalid frame ownership"),
        }
    }
}

pub fn create_snapshot(
    image_base: u64,
    image_len: u64,
    image_frames: usize,
    free_frames: usize,
) -> Result<BootstrapSnapshot, BootstrapError> {
    if image_len == 0 || image_frames == 0 || free_frames == 0 {
        return Err(BootstrapError::EmptyImage);
    }
    image_base
        .checked_add(image_len)
        .ok_or(BootstrapError::ImageRangeOverflow)?;

    Ok(BootstrapSnapshot {
        task_id: INIT_TASK_ID,
        image_cap_id: IMAGE_CAP_ID,
        namespace_cap_id: NAMESPACE_CAP_ID,
        task_cap_id: TASK_CAP_ID,
        image_base,
        image_len,
        image_frames: image_frames as u64,
        free_frames: free_frames as u64,
        image_rights: IMAGE_RIGHTS,
        namespace_rights: NAMESPACE_RIGHTS,
        task_rights: TASK_RIGHTS,
    })
}

pub fn create_state(
    image_base: u64,
    image_len: u64,
    inventory: &FrameInventory,
) -> Result<BootstrapState, BootstrapError> {
    if inventory.kernel_phys_start >= inventory.kernel_phys_end
        || inventory.image_frames == 0
        || inventory.free_frames == 0
        || inventory.free_frames != inventory.usable_frames
    {
        return Err(BootstrapError::InvalidFrameOwnership);
    }
    let snapshot = create_snapshot(
        image_base,
        image_len,
        inventory.image_frames,
        inventory.free_frames,
    )?;
    let capabilities = [
        InitialCapability {
            id: IMAGE_CAP_ID,
            object: InitialObject::ImageSegment,
            holder: INIT_TASK_ID,
            rights: IMAGE_RIGHTS,
        },
        InitialCapability {
            id: NAMESPACE_CAP_ID,
            object: InitialObject::Namespace,
            holder: INIT_TASK_ID,
            rights: NAMESPACE_RIGHTS,
        },
        InitialCapability {
            id: TASK_CAP_ID,
            object: InitialObject::InitTask,
            holder: INIT_TASK_ID,
            rights: TASK_RIGHTS,
        },
    ];
    let positions = CapabilityPositionTable::from_capabilities(&capabilities);
    Ok(BootstrapState {
        snapshot,
        capabilities,
        positions,
        derivations: CapabilityDerivationTable::empty(),
        tasks: TaskTable::with_init(),
        ipc: IpcQueue::empty(),
        audit: AuditLog::empty(),
        frame_ownership: FrameOwnership {
            image_start: inventory.kernel_phys_start,
            image_end: inventory.kernel_phys_end,
            image_frames: inventory.image_frames as u64,
            free_frames: inventory.free_frames as u64,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AuditEvent, BootstrapError, CapabilityPositionError, DeriveError, IMAGE_RIGHTS,
        INIT_TASK_ID, INITIAL_CAPABILITY_COUNT, InitialObject, IpcError, NAMESPACE_RIGHTS,
        TASK_RIGHTS, TaskError, TaskStatus, create_snapshot, create_state,
    };
    use crate::mem::FrameInventory;

    #[test]
    fn snapshot_contains_exact_initial_authority() {
        let snapshot = create_snapshot(0x4000_0000, 0x3000, 3, 16).unwrap();
        assert_eq!(snapshot.task_id, 1);
        assert_eq!(INITIAL_CAPABILITY_COUNT, 3);
        assert_eq!(snapshot.image_cap_id, 1);
        assert_eq!(snapshot.namespace_cap_id, 2);
        assert_eq!(snapshot.task_cap_id, 3);
        assert_eq!(snapshot.image_base, 0x4000_0000);
        assert_eq!(snapshot.image_len, 0x3000);
        assert_eq!(snapshot.image_frames, 3);
        assert_eq!(snapshot.free_frames, 16);
        assert_eq!(snapshot.image_rights, IMAGE_RIGHTS);
        assert_eq!(snapshot.namespace_rights, NAMESPACE_RIGHTS);
        assert_eq!(snapshot.task_rights, TASK_RIGHTS);
    }

    #[test]
    fn snapshot_rejects_invalid_image() {
        assert_eq!(
            create_snapshot(0x4000_0000, 0, 1, 1),
            Err(BootstrapError::EmptyImage)
        );
        assert_eq!(
            create_snapshot(u64::MAX, 1, 1, 1),
            Err(BootstrapError::ImageRangeOverflow)
        );
    }

    #[test]
    fn state_has_unique_capability_records_and_disjoint_frame_classes() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        assert_eq!(state.capabilities[0].object, InitialObject::ImageSegment);
        assert_eq!(state.capabilities[1].object, InitialObject::Namespace);
        assert_eq!(state.capabilities[2].object, InitialObject::InitTask);
        assert!(state.positions.has_unique_active_locations());
        assert_eq!(state.frame_ownership.image_frames, 3);
        assert_eq!(state.frame_ownership.free_frames, 16);
        assert_ne!(
            state.frame_ownership.image_start,
            state.frame_ownership.image_end
        );
    }

    #[test]
    fn state_rejects_inconsistent_frame_inventory() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 15,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        assert_eq!(
            create_state(0x4000_0000, 0x3000, &inventory),
            Err(BootstrapError::InvalidFrameOwnership)
        );
    }

    #[test]
    fn capability_move_and_drop_preserve_unique_location() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.positions.move_to_task(1, INIT_TASK_ID, 2).unwrap();
        assert_eq!(
            state.positions.move_to_task(1, INIT_TASK_ID, 3),
            Err(CapabilityPositionError::HolderMismatch)
        );
        state.positions.drop_from_task(1, 2).unwrap();
        assert!(state.positions.has_unique_active_locations());
        assert_eq!(state.positions.count, INITIAL_CAPABILITY_COUNT as usize);
    }

    #[test]
    fn capability_derivation_enforces_permission_monotonicity() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        let mut state = state;
        state
            .derive_capability(1, 4, INIT_TASK_ID, "Derive,SegMapR")
            .unwrap();
        assert_eq!(state.derivations.entries[0].unwrap().parent, 1);
        assert_eq!(state.positions.count, INITIAL_CAPABILITY_COUNT as usize + 1);
        assert!(state.positions.has_unique_active_locations());
        assert_eq!(
            state.derive_capability(1, 5, INIT_TASK_ID, "SegMapW"),
            Err(DeriveError::RightsEscalation)
        );
        assert_eq!(
            state.derive_capability(1, 4, INIT_TASK_ID, "Derive"),
            Err(DeriveError::DuplicateCapability)
        );
        state.positions.move_to_task(4, INIT_TASK_ID, 2).unwrap();
        state.positions.drop_from_task(4, 2).unwrap();
        assert!(state.positions.has_unique_active_locations());

        for child in 10..17 {
            state
                .derive_capability(1, child, INIT_TASK_ID, "Derive")
                .unwrap();
        }
        assert_eq!(state.derivations.count, 8);
        assert_eq!(state.positions.count, 11);
        assert_eq!(
            state.derive_capability(1, 17, INIT_TASK_ID, "Derive"),
            Err(DeriveError::CapacityExceeded)
        );
    }

    #[test]
    fn revoking_parent_invalidates_derived_subtree_atomically() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state
            .derive_capability(1, 4, INIT_TASK_ID, "Derive,SegMapR")
            .unwrap();
        state.derive_capability(4, 5, 2, "Derive").unwrap();
        assert_eq!(
            state.revoke_capability(1, 2),
            Err(CapabilityPositionError::HolderMismatch)
        );
        assert!(state.positions.has_unique_active_locations());
        state.revoke_capability(1, INIT_TASK_ID).unwrap();
        assert!(!state.derivations.entries[0].unwrap().active);
        assert!(!state.derivations.entries[1].unwrap().active);
        assert_eq!(
            state.positions.move_to_task(5, 2, 3),
            Err(CapabilityPositionError::Inactive)
        );
        assert!(state.positions.has_unique_active_locations());
    }

    #[test]
    fn task_capability_cleanup_revokes_holder_subtrees_but_protects_init() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.derive_capability(1, 4, 2, "Derive,SegMapR").unwrap();
        state.derive_capability(4, 5, 2, "Derive").unwrap();
        assert_eq!(state.revoke_task_capabilities(2), Ok(1));
        assert!(!state.derivations.entries[0].unwrap().active);
        assert!(!state.derivations.entries[1].unwrap().active);
        assert_eq!(state.revoke_task_capabilities(2), Ok(0));
        assert_eq!(
            state.revoke_task_capabilities(INIT_TASK_ID),
            Err(CapabilityPositionError::ProtectedTask)
        );
        assert!(state.positions.has_unique_active_locations());
    }

    #[test]
    fn task_lifecycle_enforces_single_running_and_destroy_cleanup() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.create_task(2).unwrap();
        state.derive_capability(1, 4, 2, "Derive,SegMapR").unwrap();
        assert_eq!(state.tasks.entries[0].unwrap().status, TaskStatus::Running);
        state.schedule_task(2).unwrap_err();
        state.yield_task(INIT_TASK_ID).unwrap();
        state.schedule_task(2).unwrap();
        assert_eq!(state.tasks.entries[1].unwrap().status, TaskStatus::Running);
        assert_eq!(state.destroy_task(2), Err(TaskError::CannotDestroyRunning));
        state.yield_task(2).unwrap();
        state.destroy_task(2).unwrap();
        assert_eq!(state.tasks.entries[1].unwrap().status, TaskStatus::Dead);
        assert!(!state.derivations.entries[0].unwrap().active);
        assert_eq!(
            state.destroy_task(INIT_TASK_ID),
            Err(TaskError::CannotDestroyInit)
        );
    }

    #[test]
    fn ipc_fifo_blocks_empty_receiver_wakes_and_cleans_on_destroy() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.create_task(2).unwrap();
        assert_eq!(state.receive_message(2), Err(IpcError::QueueEmpty));
        assert_eq!(
            state.tasks.entries[1].unwrap().status,
            TaskStatus::BlockedRecv
        );
        state.send_message(INIT_TASK_ID, 2, 11).unwrap();
        assert_eq!(state.tasks.entries[1].unwrap().status, TaskStatus::Ready);
        state.send_message(INIT_TASK_ID, 2, 22).unwrap();
        assert_eq!(state.receive_message(2).unwrap().payload, 11);
        assert_eq!(state.receive_message(2).unwrap().payload, 22);
        assert_eq!(state.receive_message(2), Err(IpcError::QueueEmpty));
        state.send_message(INIT_TASK_ID, 2, 33).unwrap();
        state.yield_task(INIT_TASK_ID).unwrap();
        state.destroy_task(2).unwrap();
        assert_eq!(state.ipc.count, 0);
        assert!(
            state.audit.entries[..state.audit.count]
                .iter()
                .flatten()
                .any(|event| matches!(event, AuditEvent::MessageDropped { message_id: 3 }))
        );
        assert!(
            state.audit.entries[..state.audit.count]
                .iter()
                .flatten()
                .any(|event| matches!(event, AuditEvent::TaskDestroyed { task_id: 2 }))
        );
    }

    #[test]
    fn ipc_send_requires_task_send_capability_and_preserves_queue_on_denial() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.create_task(2).unwrap();
        assert_eq!(
            state.send_message(2, INIT_TASK_ID, 7),
            Err(IpcError::NotAuthorized)
        );
        assert_eq!(state.ipc.count, 0);
        state
            .derive_capability(super::TASK_CAP_ID, 4, 2, "TaskSend")
            .unwrap();
        state.send_message(2, INIT_TASK_ID, 9).unwrap();
        assert_eq!(state.receive_message(INIT_TASK_ID).unwrap().payload, 9);
    }

    #[test]
    fn ipc_transfers_capability_atomically_through_message() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.create_task(2).unwrap();
        assert_eq!(state.receive_message(2), Err(IpcError::QueueEmpty));
        state
            .send_message_with_capabilities(INIT_TASK_ID, 2, 55, &[1])
            .unwrap();
        assert_eq!(state.ipc.count, 0);
        assert_eq!(state.audit.count, 2);
        assert!(matches!(
            state.audit.entries[0],
            Some(AuditEvent::MessageSent { message_id: 1, .. })
        ));
        assert!(matches!(
            state.audit.entries[1],
            Some(AuditEvent::MessageReceived { message_id: 1, .. })
        ));
        assert_eq!(
            state.positions.entries[0].unwrap().location,
            super::CapabilityLocation::HeldByTask(2)
        );
        assert_eq!(state.receive_message(2).unwrap().payload, 55);
        assert_eq!(
            state.send_message_with_capabilities(INIT_TASK_ID, 2, 66, &[1]),
            Err(IpcError::CapabilityHolderMismatch)
        );
        assert_eq!(state.ipc.count, 0);
    }
}
