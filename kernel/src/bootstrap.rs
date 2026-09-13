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

/// 切片 L：内核 actor 的 Task ID。异常通知以此作为发送者，永不参与 `is_live` 校验。
pub const KERNEL_ACTOR: u32 = 0;
/// 切片 L：异常源数量上限（page_fault_source + syscall_trap_source + 余量）。
const MAX_EXC_SOURCES: usize = 4;
/// 切片 L：单个异常源的订阅者上限。
const MAX_SUBSCRIBERS: usize = 4;
/// 切片 L：异常源 Capability ID 起始值（避开初始 1~3 与测试派生 ID）。
pub const EXC_SOURCE_CAP_BASE: u32 = 64;
/// 切片 L：异常源根 Capability 的权限串（对齐 §12.6）。
pub const EXC_SOURCE_RIGHTS: &str = "Derive,ExcSubscribe,ExcSourceDestroy";
/// 切片 L：预定义 `page_fault_source` 的 ID。
pub const PAGE_FAULT_SOURCE_ID: u32 = 1;
/// 切片 L：预定义 `syscall_trap_source` 的 ID。
pub const SYSCALL_TRAP_SOURCE_ID: u32 = 2;

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
    CapabilityDerived {
        cap_id: u32,
        parent: u32,
        holder: u32,
    },
    CapabilityMoved {
        cap_id: u32,
        from: CapabilityLocation,
        to: CapabilityLocation,
    },
    CapabilityRevoked {
        cap_id: u32,
    },
    /// 切片 L：异常源创建（§12.6）。
    ExcSourceCreated {
        source_id: u32,
        cap_id: u32,
        kind: ExcKind,
    },
    /// 切片 L：订阅成功（§12.4）。
    ExcSubscribed {
        source_id: u32,
        task: u32,
        kind: ExcKind,
    },
    /// 切片 L：取消订阅（§12.4）。
    ExcUnsubscribed {
        source_id: u32,
        task: u32,
    },
    /// 切片 L：异常触发（§12.5）。
    ExceptionRaised {
        source_id: u32,
        kind: ExcKind,
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
    AuditFull,
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

    fn revoke_subtree(&mut self, root: u32) -> ([u32; MAX_DERIVED_CAPABILITIES], usize) {
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
        (revoked, revoked_count)
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

// ---------------------------------------------------------------------------
// 切片 L：异常源 / 订阅 / pending 递送（COSIR §12.4~12.6）
// ---------------------------------------------------------------------------

/// 切片 L：异常种类。当前只建模两个预定义单 kind 源。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExcKind {
    /// 页故障（对应 `page_fault_source`）。
    PageFault,
    /// syscall trap（对应 `syscall_trap_source`）。
    SyscallTrap,
}

/// 异常种类的稳定编码（协议/镜像常量与 syscall 共用）。
pub const fn kind_code(kind: ExcKind) -> u32 {
    match kind {
        ExcKind::PageFault => 1,
        ExcKind::SyscallTrap => 2,
    }
}

/// 异常种类的稳定名字（日志用）。
pub const fn kind_name(kind: ExcKind) -> &'static str {
    match kind {
        ExcKind::PageFault => "page_fault",
        ExcKind::SyscallTrap => "syscall_trap",
    }
}

/// 切片 L：订阅生命周期与递送的错误（对齐 §12.4/§12.5 错误名）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExcError {
    /// 授权表尚未安装。
    NotInstalled,
    /// 找不到源（cap 不指向任何异常源）。
    NoSuchSource,
    /// 源不支持的 kind（`InvalidKind`）。
    InvalidKind,
    /// kind 与源的 kind 不兼容（`KindMismatch`）。
    KindMismatch,
    /// 缺 `ExcSubscribe`（`MissingRight`）。
    MissingRight,
    /// cap 不在调用者手中。
    NotHolder,
    /// 已订阅同一源（`AlreadySubscribed`）。
    AlreadySubscribed,
    /// 未订阅（`NotSubscribed`）。
    NotSubscribed,
    /// 订阅者/源容量已满。
    Full,
    /// Audit 容量已满。
    AuditFull,
    /// 目标 Task 不存在或已死。
    NoSuchTask,
}

/// 切片 L：一个异常源。capability 元数据内联在源记录里，**不进入 positions 表**，
/// 以免破坏 `positions.count == 3 + derivations.count` 不变量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcSource {
    pub id: u32,
    pub kind: ExcKind,
    /// 指向本源的根 Capability ID（holder 为创建时的 init Task）。
    pub cap_id: u32,
    pub cap_holder: u32,
    pub cap_rights: &'static str,
    pub subscribers: [Option<u32>; MAX_SUBSCRIBERS],
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcSourceTable {
    pub entries: [Option<ExcSource>; MAX_EXC_SOURCES],
    pub count: usize,
    next_cap_id: u32,
}

impl ExcSourceTable {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_EXC_SOURCES],
            count: 0,
            next_cap_id: EXC_SOURCE_CAP_BASE,
        }
    }

    pub fn find(&self, source_id: u32) -> Option<&ExcSource> {
        self.entries[..self.count]
            .iter()
            .flatten()
            .find(|source| source.id == source_id)
    }

    pub fn find_by_cap(&self, cap_id: u32) -> Option<&ExcSource> {
        self.entries[..self.count]
            .iter()
            .flatten()
            .find(|source| source.cap_id == cap_id)
    }

    fn find_mut(&mut self, source_id: u32) -> Option<&mut ExcSource> {
        self.entries[..self.count]
            .iter_mut()
            .flatten()
            .find(|source| source.id == source_id)
    }
}

/// 切片 L：一次 `raise_exception` 的实测结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RaiseOutcome {
    /// 投递到的订阅者数量。
    pub delivered: usize,
    /// 经 BlockedRecv 直投（写入 pending 并唤醒）的数量。
    pub pending: usize,
    /// 通知消息 ID。
    pub message_id: u32,
    /// 通知 payload（编码 source_id + kind）。
    pub payload: u64,
    /// 切片 N：被写入 pending 并唤醒的 BlockedRecv 订阅者（首个；无则 `None`）。
    pub woken: Option<u32>,
}

/// 切片 L：平台初始化期间那次 pending 递送的留证。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcBootRecord {
    pub delivered: usize,
    pub pending: usize,
    pub message_id: u32,
    pub payload: u64,
    /// 取回 pending 后目标 Task 的状态是否回到 Ready。
    pub task_ready_after: bool,
    /// 切片 N：平台初始化完成时的 `ExceptionRaised` Audit 计数（供 SPEC-050 见证快照，
    /// 使后续切片新增的 raise 不扰动其期望值）。
    pub audit_raised: usize,
}

/// 切片 L：四类异常相关 Audit 事件的计数（供见证对账）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcAuditCounts {
    pub created: usize,
    pub subscribed: usize,
    pub unsubscribed: usize,
    pub raised: usize,
}

/// 切片 L：异常通知 payload——低位 16 位 source_id，高 16 位 kind 编码。
pub const fn exc_notification_payload(source_id: u32, kind: ExcKind) -> u64 {
    (source_id as u64 & 0xFFFF) | ((kind_code(kind) as u64) << 16)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityPositionError {
    NoSuchCapability,
    DuplicateCapability,
    CapacityExceeded,
    ProtectedTask,
    HolderMismatch,
    Inactive,
    AuditFull,
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
    /// 切片 L：异常源表（capability 元数据内联，不入 positions）。
    pub exc_sources: ExcSourceTable,
    /// 切片 L：平台初始化期间 pending 递送的留证。
    pub exc_boot: Option<ExcBootRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapError {
    EmptyImage,
    ImageRangeOverflow,
    InvalidFrameOwnership,
    InvalidAuthority,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorityError {
    InvalidCount,
    DuplicateCapability,
    MissingPosition,
    PositionMismatch,
    InvalidHolder,
    InvalidParent,
    RightsEscalation,
    InvalidInactiveState,
}

/// 切片 K：授权查询失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// 授权表尚未安装（`install` 未调用）。
    NotInstalled,
    /// 找不到该 Capability ID。
    NoSuchCapability,
    /// Capability 已失效（位置 inactive 或派生 inactive）。
    Inactive,
    /// Capability 不在 `caller` 手中。
    NotHolder,
    /// 持有者缺少所请求的权限。
    MissingRight,
}

impl BootstrapState {
    pub fn validate_authority(&self) -> Result<(), AuthorityError> {
        if self.positions.count != INITIAL_CAPABILITY_COUNT as usize + self.derivations.count
            || self.positions.count > MAX_CAPABILITY_POSITIONS
            || self.derivations.count > MAX_DERIVED_CAPABILITIES
            || self.audit.count > MAX_AUDIT_EVENTS
        {
            return Err(AuthorityError::InvalidCount);
        }

        let mut ids = [0u32; MAX_CAPABILITY_POSITIONS];
        let mut id_count = 0;
        for capability in &self.capabilities {
            if id_count == ids.len() || ids[..id_count].contains(&capability.id) {
                return Err(AuthorityError::DuplicateCapability);
            }
            ids[id_count] = capability.id;
            id_count += 1;
            if !self.tasks.is_live(capability.holder) || capability.rights.is_empty() {
                return Err(AuthorityError::InvalidHolder);
            }
        }
        for derived in self.derivations.entries[..self.derivations.count]
            .iter()
            .flatten()
        {
            if id_count == ids.len() || ids[..id_count].contains(&derived.id) {
                return Err(AuthorityError::DuplicateCapability);
            }
            ids[id_count] = derived.id;
            id_count += 1;
            let parent_rights = self
                .capabilities
                .iter()
                .find(|capability| capability.id == derived.parent)
                .map(|capability| capability.rights)
                .or_else(|| {
                    self.derivations.entries[..self.derivations.count]
                        .iter()
                        .flatten()
                        .find(|capability| capability.id == derived.parent)
                        .map(|capability| capability.rights)
                })
                .ok_or(AuthorityError::InvalidParent)?;
            if !rights_subset(parent_rights, derived.rights) {
                return Err(AuthorityError::RightsEscalation);
            }
            if derived.active && !self.tasks.is_live(derived.holder) {
                return Err(AuthorityError::InvalidHolder);
            }
        }

        for &cap_id in ids.iter().take(id_count) {
            let position = self.positions.entries[..self.positions.count]
                .iter()
                .flatten()
                .find(|entry| entry.cap_id == cap_id)
                .ok_or(AuthorityError::MissingPosition)?;
            let active = self.derivations.entries[..self.derivations.count]
                .iter()
                .flatten()
                .find(|entry| entry.id == cap_id)
                .map(|entry| entry.active)
                .unwrap_or(true);
            if position.active != active {
                return Err(AuthorityError::PositionMismatch);
            }
            match position.location {
                CapabilityLocation::HeldByTask(task_id) => {
                    if !self.tasks.is_live(task_id) {
                        return Err(AuthorityError::InvalidHolder);
                    }
                }
                CapabilityLocation::InMessage(message_id) => {
                    let queued =
                        self.ipc.entries[..self.ipc.count]
                            .iter()
                            .flatten()
                            .any(|message| {
                                message.id == message_id
                                    && message.capabilities[..message.capability_count]
                                        .contains(&cap_id)
                            });
                    if !queued {
                        return Err(AuthorityError::PositionMismatch);
                    }
                }
                CapabilityLocation::Dropped if position.active => {
                    return Err(AuthorityError::InvalidInactiveState);
                }
                CapabilityLocation::Dropped => {}
            }
        }

        // 切片 L：异常源 Capability 的独立校验。它们不进入 positions，故不能沿用
        // `ids` 数组（容量只按 positions 计）；改为与初始/派生 Capability 交叉查重。
        for source in self.exc_sources.entries[..self.exc_sources.count]
            .iter()
            .flatten()
        {
            if !self.tasks.is_live(source.cap_holder) || source.cap_rights.is_empty() {
                return Err(AuthorityError::InvalidHolder);
            }
            if self
                .capabilities
                .iter()
                .any(|capability| capability.id == source.cap_id)
                || self.derivations.entries[..self.derivations.count]
                    .iter()
                    .flatten()
                    .any(|capability| capability.id == source.cap_id)
            {
                return Err(AuthorityError::DuplicateCapability);
            }
            let duplicate = self.exc_sources.entries[..self.exc_sources.count]
                .iter()
                .flatten()
                .filter(|candidate| candidate.cap_id == source.cap_id || candidate.id == source.id)
                .count();
            if duplicate > 1 || source.count > MAX_SUBSCRIBERS {
                return Err(AuthorityError::DuplicateCapability);
            }
            for subscriber in source.subscribers[..source.count].iter().flatten() {
                if !self.tasks.is_live(*subscriber) {
                    return Err(AuthorityError::InvalidHolder);
                }
            }
        }
        Ok(())
    }

    /// 切片 K：授权查询——`caller` 是否持有 `cap_id` 且该 Capability 具备 `right`。
    ///
    /// 成功返回 `cap_id` 作为授予凭据；失败区分“无此 Capability / 已失效 / 非持有者 /
    /// 缺权限”。逐操作权限与位置语义仍以 [[COSIR 原语规范]] 为准。
    pub fn authorize(&self, caller: u32, cap_id: u32, right: &str) -> Result<u32, AuthError> {
        // 切片 L：异常源 Capability 不进入 positions，单独查 holder/rights。
        if let Some(source) = self.exc_sources.find_by_cap(cap_id) {
            if source.cap_holder != caller {
                return Err(AuthError::NotHolder);
            }
            if !has_right(source.cap_rights, right) {
                return Err(AuthError::MissingRight);
            }
            return Ok(cap_id);
        }
        let position = self.positions.entries[..self.positions.count]
            .iter()
            .flatten()
            .find(|entry| entry.cap_id == cap_id)
            .ok_or(AuthError::NoSuchCapability)?;
        if !position.active {
            return Err(AuthError::Inactive);
        }
        if position.location != CapabilityLocation::HeldByTask(caller) {
            return Err(AuthError::NotHolder);
        }
        let rights = self
            .capabilities
            .iter()
            .find(|capability| capability.id == cap_id)
            .map(|capability| capability.rights)
            .or_else(|| {
                self.derivations.entries[..self.derivations.count]
                    .iter()
                    .flatten()
                    .find(|capability| capability.id == cap_id)
                    .map(|capability| capability.rights)
            })
            .ok_or(AuthError::NoSuchCapability)?;
        if !has_right(rights, right) {
            return Err(AuthError::MissingRight);
        }
        Ok(cap_id)
    }

    /// 切片 L：`cap_id` 是否已被任何既有 Capability（初始/派生/异常源）占用。
    fn capability_id_taken(&self, cap_id: u32) -> bool {
        self.capabilities
            .iter()
            .any(|capability| capability.id == cap_id)
            || self.derivations.entries[..self.derivations.count]
                .iter()
                .flatten()
                .any(|capability| capability.id == cap_id)
            || self.exc_sources.entries[..self.exc_sources.count]
                .iter()
                .flatten()
                .any(|source| source.cap_id == cap_id)
    }

    /// 切片 L：创建一个异常源并铸造其根 Capability（holder 为 init Task）。
    ///
    /// 返回 `(source_id, cap_id)`。追加 `ExcSourceCreated` Audit（§12.6）。
    pub fn exc_source_create(&mut self, kind: ExcKind) -> Result<(u32, u32), ExcError> {
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        if self.exc_sources.count == MAX_EXC_SOURCES {
            return Err(ExcError::Full);
        }
        let source_id = self.exc_sources.count as u32 + 1;
        let mut cap_id = self.exc_sources.next_cap_id;
        while self.capability_id_taken(cap_id) {
            cap_id += 1;
        }
        self.exc_sources.next_cap_id = cap_id + 1;
        self.exc_sources.entries[self.exc_sources.count] = Some(ExcSource {
            id: source_id,
            kind,
            cap_id,
            cap_holder: INIT_TASK_ID,
            cap_rights: EXC_SOURCE_RIGHTS,
            subscribers: [None; MAX_SUBSCRIBERS],
            count: 0,
        });
        self.exc_sources.count += 1;
        self.audit.append(AuditEvent::ExcSourceCreated {
            source_id,
            cap_id,
            kind,
        });
        Ok((source_id, cap_id))
    }

    /// 切片 L：`caller` 用持有的 `cap_id` 订阅 `kind`（§12.4）。
    ///
    /// 成功返回 source_id；失败不修改任何状态、不追加 Audit。
    pub fn exc_subscribe(
        &mut self,
        caller: u32,
        cap_id: u32,
        kind: ExcKind,
    ) -> Result<u32, ExcError> {
        if !self.tasks.is_live(caller) {
            return Err(ExcError::NoSuchTask);
        }
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        let source = self
            .exc_sources
            .find_by_cap(cap_id)
            .ok_or(ExcError::NoSuchSource)?;
        let source_id = source.id;
        if source.cap_holder != caller {
            return Err(ExcError::NotHolder);
        }
        if !has_right(source.cap_rights, "ExcSubscribe") {
            return Err(ExcError::MissingRight);
        }
        if source.kind != kind {
            return Err(ExcError::KindMismatch);
        }
        let source = self
            .exc_sources
            .find_mut(source_id)
            .ok_or(ExcError::NoSuchSource)?;
        if source.subscribers[..source.count]
            .iter()
            .flatten()
            .any(|task| *task == caller)
        {
            return Err(ExcError::AlreadySubscribed);
        }
        if source.count == MAX_SUBSCRIBERS {
            return Err(ExcError::Full);
        }
        source.subscribers[source.count] = Some(caller);
        source.count += 1;
        self.audit.append(AuditEvent::ExcSubscribed {
            source_id,
            task: caller,
            kind,
        });
        Ok(source_id)
    }

    /// 切片 L：`caller` 取消对 `cap_id` 所指源的订阅（§12.4）。
    pub fn exc_unsubscribe(&mut self, caller: u32, cap_id: u32) -> Result<u32, ExcError> {
        if !self.tasks.is_live(caller) {
            return Err(ExcError::NoSuchTask);
        }
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        let source = self
            .exc_sources
            .find_by_cap(cap_id)
            .ok_or(ExcError::NoSuchSource)?;
        let source_id = source.id;
        if source.cap_holder != caller {
            return Err(ExcError::NotHolder);
        }
        if !has_right(source.cap_rights, "ExcSubscribe") {
            return Err(ExcError::MissingRight);
        }
        let source = self
            .exc_sources
            .find_mut(source_id)
            .ok_or(ExcError::NoSuchSource)?;
        let Some(position) = source.subscribers[..source.count]
            .iter()
            .position(|task| task.is_some_and(|task| task == caller))
        else {
            return Err(ExcError::NotSubscribed);
        };
        for index in position..source.count - 1 {
            source.subscribers[index] = source.subscribers[index + 1];
        }
        source.subscribers[source.count - 1] = None;
        source.count -= 1;
        self.audit.append(AuditEvent::ExcUnsubscribed {
            source_id,
            task: caller,
        });
        Ok(source_id)
    }

    /// 切片 N：查询 `caller` 是否已订阅 `cap_id` 所指源的 `kind`；成功返回 source_id。
    ///
    /// 纯查询，不修改状态、不追加 Audit（§13.1）。
    pub fn exc_subscribed(&self, caller: u32, cap_id: u32, kind: ExcKind) -> Result<u32, ExcError> {
        if !self.tasks.is_live(caller) {
            return Err(ExcError::NoSuchTask);
        }
        let source = self
            .exc_sources
            .find_by_cap(cap_id)
            .ok_or(ExcError::NoSuchSource)?;
        if source.kind != kind {
            return Err(ExcError::KindMismatch);
        }
        if !source.subscribers[..source.count]
            .iter()
            .flatten()
            .any(|task| *task == caller)
        {
            return Err(ExcError::NotSubscribed);
        }
        Ok(source.id)
    }

    /// 切片 L：内核侧登记订阅（不经理 Capability 门控），供平台初始化使用。
    fn exc_source_add_subscriber(&mut self, source_id: u32, task: u32) -> Result<(), ExcError> {
        if !self.tasks.is_live(task) {
            return Err(ExcError::NoSuchTask);
        }
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        let kind = self
            .exc_sources
            .find(source_id)
            .ok_or(ExcError::NoSuchSource)?
            .kind;
        let source = self
            .exc_sources
            .find_mut(source_id)
            .ok_or(ExcError::NoSuchSource)?;
        if source.subscribers[..source.count]
            .iter()
            .flatten()
            .any(|subscriber| *subscriber == task)
        {
            return Err(ExcError::AlreadySubscribed);
        }
        if source.count == MAX_SUBSCRIBERS {
            return Err(ExcError::Full);
        }
        source.subscribers[source.count] = Some(task);
        source.count += 1;
        self.audit.append(AuditEvent::ExcSubscribed {
            source_id,
            task,
            kind,
        });
        Ok(())
    }

    /// 切片 L：`raise_exception`——追加 `ExceptionRaised` 并向每个订阅者递送通知（§12.5）。
    ///
    /// BlockedRecv 的订阅者走直投：`set_pending` 写入 pending 并置 Ready，追加
    /// `MessageSent`+`MessageReceived`；其余订阅者入 IPC 队列，追加 `MessageSent`。
    /// 容量在修改任何状态前预检，保证原子性。
    pub fn exc_raise(&mut self, source_id: u32, kind: ExcKind) -> Result<RaiseOutcome, ExcError> {
        let source = self
            .exc_sources
            .find(source_id)
            .ok_or(ExcError::NoSuchSource)?;
        if source.kind != kind {
            return Err(ExcError::KindMismatch);
        }
        let mut subscribers = [None; MAX_SUBSCRIBERS];
        let mut count = 0;
        for task in source.subscribers[..source.count].iter().flatten() {
            subscribers[count] = Some(*task);
            count += 1;
        }
        let direct = subscribers[..count]
            .iter()
            .flatten()
            .filter(|task| self.tasks.is_blocked_recv(**task))
            .count();
        let queued = count - direct;
        let audit_events = 1 + direct * 2 + queued;
        if !self.audit.can_append(audit_events) {
            return Err(ExcError::AuditFull);
        }
        if queued > 0 && self.ipc.count + queued > MAX_MESSAGES {
            return Err(ExcError::Full);
        }
        let payload = exc_notification_payload(source_id, kind);
        self.audit
            .append(AuditEvent::ExceptionRaised { source_id, kind });
        let mut delivered = 0;
        let mut pending = 0;
        let mut first_message_id = 0;
        let mut woken = None;
        for task in subscribers[..count].iter().flatten().copied() {
            let message_id = self.ipc.allocate_id();
            if first_message_id == 0 {
                first_message_id = message_id;
            }
            let message = IpcMessage {
                id: message_id,
                sender: KERNEL_ACTOR,
                receiver: task,
                payload,
                capabilities: [0; MAX_MESSAGE_CAPABILITIES],
                capability_count: 0,
            };
            if self.tasks.is_blocked_recv(task) {
                self.tasks
                    .set_pending(task, message)
                    .map_err(|_| ExcError::NoSuchTask)?;
                self.audit.append(AuditEvent::MessageSent {
                    message_id,
                    sender: KERNEL_ACTOR,
                    receiver: task,
                });
                self.audit.append(AuditEvent::MessageReceived {
                    message_id,
                    receiver: task,
                });
                pending += 1;
                if woken.is_none() {
                    woken = Some(task);
                }
            } else {
                self.ipc.send(message).map_err(|_| ExcError::Full)?;
                self.audit.append(AuditEvent::MessageSent {
                    message_id,
                    sender: KERNEL_ACTOR,
                    receiver: task,
                });
            }
            delivered += 1;
        }
        Ok(RaiseOutcome {
            delivered,
            pending,
            message_id: first_message_id,
            payload,
            woken,
        })
    }

    /// 切片 L：平台初始化——显式创建两个预定义异常源，并实证一次 pending 直投。
    ///
    /// task 2 订阅 page_fault_source 后进入 BlockedRecv；`exc_raise` 直投写入其
    /// pending 并置 Ready，随后取回消息，留证于 `exc_boot`。
    pub fn platform_init(&mut self) -> Result<(), ExcError> {
        let (page_fault_source, _) = self.exc_source_create(ExcKind::PageFault)?;
        let _ = self.exc_source_create(ExcKind::SyscallTrap)?;
        self.create_task(2).map_err(|_| ExcError::Full)?;
        self.exc_source_add_subscriber(page_fault_source, 2)?;
        self.tasks.block_recv(2).map_err(|_| ExcError::NoSuchTask)?;
        let outcome = self.exc_raise(page_fault_source, ExcKind::PageFault)?;
        let message = self.tasks.take_pending(2);
        let task_ready_after = self
            .tasks
            .find(2)
            .is_some_and(|task| task.status == TaskStatus::Ready);
        self.exc_boot = Some(ExcBootRecord {
            delivered: outcome.delivered,
            pending: outcome.pending,
            message_id: message.map(|message| message.id).unwrap_or(0),
            payload: message.map(|message| message.payload).unwrap_or(0),
            task_ready_after,
            audit_raised: self.exc_audit_counts().raised,
        });
        Ok(())
    }

    /// 切片 L：四类异常相关 Audit 事件的计数。
    pub fn exc_audit_counts(&self) -> ExcAuditCounts {
        let mut counts = ExcAuditCounts {
            created: 0,
            subscribed: 0,
            unsubscribed: 0,
            raised: 0,
        };
        for event in self.audit.entries[..self.audit.count].iter().flatten() {
            match event {
                AuditEvent::ExcSourceCreated { .. } => counts.created += 1,
                AuditEvent::ExcSubscribed { .. } => counts.subscribed += 1,
                AuditEvent::ExcUnsubscribed { .. } => counts.unsubscribed += 1,
                AuditEvent::ExceptionRaised { .. } => counts.raised += 1,
                _ => {}
            }
        }
        counts
    }

    pub fn derive_capability(
        &mut self,
        parent: u32,
        child: u32,
        holder: u32,
        rights: &'static str,
    ) -> Result<(), DeriveError> {
        if !self.audit.can_append(1) {
            return Err(DeriveError::AuditFull);
        }
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
                | CapabilityPositionError::ProtectedTask
                | CapabilityPositionError::AuditFull => DeriveError::CapacityExceeded,
            })?;
        self.audit.append(AuditEvent::CapabilityDerived {
            cap_id: child,
            parent,
            holder,
        });
        Ok(())
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
        let mut preview = self.derivations;
        let (_, revoked_count) = preview.revoke_subtree(cap_id);
        if !self.audit.can_append(revoked_count) {
            return Err(CapabilityPositionError::AuditFull);
        }
        let (revoked_ids, revoked_count) = self.derivations.revoke_subtree(cap_id);
        self.positions.revoke(cap_id);
        for capability in self.derivations.entries[..self.derivations.count]
            .iter()
            .flatten()
        {
            if !capability.active {
                self.positions.revoke(capability.id);
            }
        }
        for revoked_id in revoked_ids[..revoked_count].iter().copied() {
            self.audit
                .append(AuditEvent::CapabilityRevoked { cap_id: revoked_id });
        }
        Ok(())
    }

    pub fn move_capability(
        &mut self,
        cap_id: u32,
        from_task: u32,
        to_task: u32,
    ) -> Result<(), CapabilityPositionError> {
        if !self.audit.can_append(1) {
            return Err(CapabilityPositionError::AuditFull);
        }
        self.positions.move_to_task(cap_id, from_task, to_task)?;
        self.audit.append(AuditEvent::CapabilityMoved {
            cap_id,
            from: CapabilityLocation::HeldByTask(from_task),
            to: CapabilityLocation::HeldByTask(to_task),
        });
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
                        | CapabilityPositionError::ProtectedTask
                        | CapabilityPositionError::AuditFull => IpcError::InvalidCapability,
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
                        | CapabilityPositionError::ProtectedTask
                        | CapabilityPositionError::AuditFull => IpcError::InvalidCapability,
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
            Self::InvalidAuthority => formatter.write_str("invalid authority"),
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
    let state = BootstrapState {
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
        exc_sources: ExcSourceTable::empty(),
        exc_boot: None,
    };
    state
        .validate_authority()
        .map_err(|_| BootstrapError::InvalidAuthority)?;
    Ok(state)
}

/// 切片 K：全局 Bootstrap 授权表的存储（单核 boot 早期安装一次）。
static mut BOOTSTRAP_STATE: Option<BootstrapState> = None;

/// 安装 Bootstrap 授权表，供 syscall 授权查询使用（在进入 CPL3 前调用一次）。
pub fn install(state: BootstrapState) {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        *slot = Some(state);
    }
}

/// 查询全局授权表：`caller` 是否持有 `cap_id` 且具备 `right`。
pub fn authorize(caller: u32, cap_id: u32, right: &str) -> Result<u32, AuthError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.authorize(caller, cap_id, right),
            None => Err(AuthError::NotInstalled),
        }
    }
}

/// 切片 L：平台初始化——创建预定义异常源并实证一次 pending 直投（进入 CPL3 前调用）。
pub fn platform_init_exception_sources() -> Result<(), ExcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.platform_init(),
            None => Err(ExcError::NotInstalled),
        }
    }
}

/// 切片 L：全局订阅查询（供 syscall 层使用）。
pub fn exc_subscribe(caller: u32, cap_id: u32, kind: ExcKind) -> Result<u32, ExcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.exc_subscribe(caller, cap_id, kind),
            None => Err(ExcError::NotInstalled),
        }
    }
}

/// 切片 L：全局取消订阅（供 syscall 层使用）。
pub fn exc_unsubscribe(caller: u32, cap_id: u32) -> Result<u32, ExcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.exc_unsubscribe(caller, cap_id),
            None => Err(ExcError::NotInstalled),
        }
    }
}

/// 切片 L：读取异常相关 Audit 计数（授权表未安装时返回 `None`）。
pub fn exc_audit_counts() -> Option<ExcAuditCounts> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        (*slot).as_ref().map(|state| state.exc_audit_counts())
    }
}

/// 切片 L：读取平台初始化期间的 pending 递送留证。
pub fn exc_boot_record() -> Option<ExcBootRecord> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        (*slot).as_ref().and_then(|state| state.exc_boot)
    }
}

/// 切片 M：把 `task_id` 从 Running 让出为 Ready（协作式调度用）。
pub fn yield_task_global(task_id: u32) -> Result<(), TaskError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.yield_task(task_id),
            None => Err(TaskError::NoSuchTask),
        }
    }
}

/// 切片 M：把 Ready 的 `task_id` 调度为 Running（协作式调度用）。
pub fn schedule_task_global(task_id: u32) -> Result<(), TaskError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.schedule_task(task_id),
            None => Err(TaskError::NoSuchTask),
        }
    }
}

/// 切片 M：读取任务状态（授权表未安装或任务不存在时返回 `None`）。
pub fn task_status(task_id: u32) -> Option<TaskStatus> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        (*slot)
            .as_ref()
            .and_then(|state| state.tasks.find(task_id))
            .map(|task| task.status)
    }
}

/// 切片 N：把 `task_id` 置为 BlockedRecv（`SYS_EXC_WAIT` 阻塞调用者用）。
pub fn block_recv_global(task_id: u32) -> Result<(), TaskError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.tasks.block_recv(task_id),
            None => Err(TaskError::NoSuchTask),
        }
    }
}

/// 切片 N：内核 actor 触发一次 `raise_exception`（供 `SYS_TRAP` 使用）。
pub fn exc_raise(source_id: u32, kind: ExcKind) -> Result<RaiseOutcome, ExcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.exc_raise(source_id, kind),
            None => Err(ExcError::NotInstalled),
        }
    }
}

/// 切片 N：查询 `caller` 是否已订阅 `cap_id` 所指源的 `kind`。
pub fn exc_subscribed(caller: u32, cap_id: u32, kind: ExcKind) -> Result<u32, ExcError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.exc_subscribed(caller, cap_id, kind),
            None => Err(ExcError::NotInstalled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuditEvent, AuthError, AuthorityError, BootstrapError, CapabilityPositionError,
        DeriveError, EXC_SOURCE_RIGHTS, ExcError, ExcKind, IMAGE_CAP_ID, IMAGE_RIGHTS,
        INIT_TASK_ID, INITIAL_CAPABILITY_COUNT, InitialObject, IpcError, KERNEL_ACTOR,
        NAMESPACE_RIGHTS, PAGE_FAULT_SOURCE_ID, SYSCALL_TRAP_SOURCE_ID, TASK_RIGHTS, TaskError,
        TaskStatus, create_snapshot, create_state,
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
        assert_eq!(state.validate_authority(), Ok(()));
    }

    #[test]
    fn authority_validation_rejects_position_and_rights_corruption() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.positions.entries[0].as_mut().unwrap().active = false;
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::PositionMismatch)
        );

        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.positions.entries[0].as_mut().unwrap().location =
            super::CapabilityLocation::HeldByTask(99);
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::InvalidHolder)
        );

        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state
            .derive_capability(IMAGE_CAP_ID, 4, INIT_TASK_ID, "SegMapR")
            .unwrap();
        state.derivations.entries[0].as_mut().unwrap().rights = "all";
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::RightsEscalation)
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
        state.move_capability(1, INIT_TASK_ID, 2).unwrap();
        assert!(
            state.audit.entries[..state.audit.count]
                .iter()
                .flatten()
                .any(|event| matches!(event, AuditEvent::CapabilityMoved { cap_id: 1, .. }))
        );
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
        for cap_id in [1, 4, 5] {
            assert!(state.audit.entries[..state.audit.count].iter().flatten().any(
                |event| matches!(event, AuditEvent::CapabilityRevoked { cap_id: id } if *id == cap_id)
            ));
        }
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

    fn authorized_state() -> super::BootstrapState {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        create_state(0x4000_0000, 0x3000, &inventory).unwrap()
    }

    #[test]
    fn authorize_grants_held_capability_with_right() {
        let state = authorized_state();
        assert_eq!(
            state.authorize(INIT_TASK_ID, IMAGE_CAP_ID, "SegMapR"),
            Ok(1)
        );
        // namespace 的权限为 "all"，任意权限均可。
        assert_eq!(state.authorize(INIT_TASK_ID, 2, "SegMapW"), Ok(2));
    }

    #[test]
    fn authorize_denies_missing_right() {
        let state = authorized_state();
        // 初始 image cap 只有 SegMapR/MapX/Destroy，没有 SegMapW。
        assert_eq!(
            state.authorize(INIT_TASK_ID, IMAGE_CAP_ID, "SegMapW"),
            Err(AuthError::MissingRight)
        );
    }

    #[test]
    fn authorize_reports_unknown_capability() {
        let state = authorized_state();
        assert_eq!(
            state.authorize(INIT_TASK_ID, 999, "SegMapR"),
            Err(AuthError::NoSuchCapability)
        );
    }

    #[test]
    fn authorize_rejects_non_holder() {
        let state = authorized_state();
        assert_eq!(
            state.authorize(2, IMAGE_CAP_ID, "SegMapR"),
            Err(AuthError::NotHolder)
        );
    }

    #[test]
    fn authorize_rejects_inactive_capability() {
        let mut state = authorized_state();
        state
            .positions
            .drop_from_task(IMAGE_CAP_ID, INIT_TASK_ID)
            .unwrap();
        assert_eq!(
            state.authorize(INIT_TASK_ID, IMAGE_CAP_ID, "SegMapR"),
            Err(AuthError::Inactive)
        );
    }

    fn exc_state() -> super::BootstrapState {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.exc_source_create(ExcKind::PageFault).unwrap();
        state.exc_source_create(ExcKind::SyscallTrap).unwrap();
        state
    }

    #[test]
    fn exc_source_create_mints_distinct_caps_and_audits() {
        let state = exc_state();
        assert_eq!(state.exc_sources.count, 2);
        let page_fault = state.exc_sources.find(PAGE_FAULT_SOURCE_ID).unwrap();
        assert_eq!(page_fault.kind, ExcKind::PageFault);
        assert_eq!(page_fault.cap_id, 64);
        assert_eq!(page_fault.cap_holder, INIT_TASK_ID);
        assert_eq!(page_fault.cap_rights, EXC_SOURCE_RIGHTS);
        assert_eq!(
            state
                .exc_sources
                .find(SYSCALL_TRAP_SOURCE_ID)
                .unwrap()
                .cap_id,
            65
        );
        let counts = state.exc_audit_counts();
        assert_eq!(counts.created, 2);
        assert_eq!(counts.subscribed, 0);
        assert!(state.validate_authority().is_ok());
    }

    #[test]
    fn exc_subscribe_checks_holder_kind_and_duplicates() {
        let mut state = exc_state();
        let page_cap = state.exc_sources.find(PAGE_FAULT_SOURCE_ID).unwrap().cap_id;
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault),
            Ok(PAGE_FAULT_SOURCE_ID)
        );
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault),
            Err(ExcError::AlreadySubscribed)
        );
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::SyscallTrap),
            Err(ExcError::KindMismatch)
        );
        assert_eq!(
            state.exc_subscribe(2, page_cap, ExcKind::PageFault),
            Err(ExcError::NoSuchTask)
        );
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, 999, ExcKind::PageFault),
            Err(ExcError::NoSuchSource)
        );
        // 缺少 ExcSubscribe：把源 cap 权限改窄后触发 MissingRight。
        state.exc_sources.entries[0].as_mut().unwrap().cap_rights = "Derive";
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault),
            Err(ExcError::MissingRight)
        );
        // 非持有者：holder 仍是 init 时用别的 task id 会被 NoSuchTask 拦下，故直接改 holder。
        state.exc_sources.entries[0].as_mut().unwrap().cap_rights = EXC_SOURCE_RIGHTS;
        state.exc_sources.entries[0].as_mut().unwrap().cap_holder = 2;
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault),
            Err(ExcError::NotHolder)
        );
    }

    #[test]
    fn exc_unsubscribe_removes_and_audits_only_when_subscribed() {
        let mut state = exc_state();
        let page_cap = state.exc_sources.find(PAGE_FAULT_SOURCE_ID).unwrap().cap_id;
        state
            .exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault)
            .unwrap();
        assert_eq!(
            state.exc_unsubscribe(INIT_TASK_ID, page_cap),
            Ok(PAGE_FAULT_SOURCE_ID)
        );
        assert_eq!(
            state.exc_unsubscribe(INIT_TASK_ID, page_cap),
            Err(ExcError::NotSubscribed)
        );
        assert_eq!(
            state.exc_sources.find(PAGE_FAULT_SOURCE_ID).unwrap().count,
            0
        );
        assert_eq!(state.exc_audit_counts().unsubscribed, 1);
    }

    #[test]
    fn exc_raise_writes_pending_for_blocked_subscriber() {
        let mut state = exc_state();
        let page_source = PAGE_FAULT_SOURCE_ID;
        state.create_task(2).unwrap();
        state.exc_source_add_subscriber(page_source, 2).unwrap();
        state.tasks.block_recv(2).unwrap();

        let outcome = state.exc_raise(page_source, ExcKind::PageFault).unwrap();
        assert_eq!(outcome.delivered, 1);
        assert_eq!(outcome.pending, 1);
        assert_eq!(outcome.payload, (1 << 16) | 1);
        assert_eq!(
            state.tasks.find(2).unwrap().status,
            TaskStatus::Ready,
            "直投应把 BlockedRecv 唤醒为 Ready"
        );
        let message = state.tasks.take_pending(2).unwrap();
        assert_eq!(message.sender, KERNEL_ACTOR);
        assert_eq!(message.payload, outcome.payload);
        assert_eq!(message.id, outcome.message_id);
        assert_eq!(state.ipc.count, 0);
        let counts = state.exc_audit_counts();
        assert_eq!(counts.subscribed, 1);
        assert_eq!(counts.raised, 1);
        assert!(state.validate_authority().is_ok());
    }

    #[test]
    fn exc_raise_queues_notification_for_running_subscriber() {
        let mut state = exc_state();
        let page_source = PAGE_FAULT_SOURCE_ID;
        state.create_task(2).unwrap();
        state.exc_source_add_subscriber(page_source, 2).unwrap();

        let outcome = state.exc_raise(page_source, ExcKind::PageFault).unwrap();
        assert_eq!(outcome.delivered, 1);
        assert_eq!(outcome.pending, 0);
        assert_eq!(state.ipc.count, 1);
        let queued = state.ipc.entries[0].unwrap();
        assert_eq!(queued.sender, KERNEL_ACTOR);
        assert_eq!(queued.receiver, 2);
        assert_eq!(queued.payload, outcome.payload);
        assert!(state.validate_authority().is_ok());
    }

    #[test]
    fn exc_raise_rejects_unknown_source_and_kind_mismatch() {
        let mut state = exc_state();
        assert_eq!(
            state.exc_raise(99, ExcKind::PageFault),
            Err(ExcError::NoSuchSource)
        );
        assert_eq!(
            state.exc_raise(PAGE_FAULT_SOURCE_ID, ExcKind::SyscallTrap),
            Err(ExcError::KindMismatch)
        );
    }

    #[test]
    fn platform_init_witnesses_pending_delivery_and_audits() {
        let inventory = FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        };
        let mut state = create_state(0x4000_0000, 0x3000, &inventory).unwrap();
        state.platform_init().unwrap();
        let boot = state.exc_boot.unwrap();
        assert_eq!(boot.delivered, 1);
        assert_eq!(boot.pending, 1);
        assert!(boot.task_ready_after);
        assert_eq!(boot.payload, (1 << 16) | 1);
        assert_eq!(boot.message_id, 1);
        let counts = state.exc_audit_counts();
        assert_eq!(counts.created, 2);
        assert_eq!(counts.subscribed, 1);
        assert_eq!(counts.raised, 1);
        assert!(state.validate_authority().is_ok());
    }

    #[test]
    fn validate_authority_rejects_source_cap_collision() {
        let mut state = exc_state();
        state.exc_sources.entries[0].as_mut().unwrap().cap_id = IMAGE_CAP_ID;
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::DuplicateCapability)
        );
    }
}
