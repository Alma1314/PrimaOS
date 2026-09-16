use core::fmt;

use crate::handle::Handle;
use crate::mem::FrameInventory;
use crate::rights::{ObjectKind, Right, Rights};

pub const INIT_TASK_ID: u32 = 1;
/// 初始根 Capability 的**物理槽索引**（S2-D1：句柄 = 槽索引 + generation）。
pub const IMAGE_CAP_SLOT: u32 = 1;
pub const NAMESPACE_CAP_SLOT: u32 = 2;
pub const TASK_CAP_SLOT: u32 = 3;
pub const INITIAL_CAPABILITY_COUNT: u32 = 3;
/// 初始根 Capability 的 generation（首次投入使用的槽）。
pub const INITIAL_CAP_GENERATION: u32 = 1;

/// const 上下文里构造句柄（非法组合返回 [`Handle::ZERO`]，由调用方常量保证合法）。
const fn cap_handle(slot: u32, generation: u32) -> Handle {
    match Handle::new(slot, generation) {
        Ok(handle) => handle,
        Err(_) => Handle::ZERO,
    }
}

/// 初始 image Segment Capability 的 ABI 句柄。
pub const IMAGE_CAP_HANDLE: Handle = cap_handle(IMAGE_CAP_SLOT, INITIAL_CAP_GENERATION);
/// 初始 Namespace Capability 的 ABI 句柄。
pub const NAMESPACE_CAP_HANDLE: Handle = cap_handle(NAMESPACE_CAP_SLOT, INITIAL_CAP_GENERATION);
/// 初始 Task Capability 的 ABI 句柄。
pub const TASK_CAP_HANDLE: Handle = cap_handle(TASK_CAP_SLOT, INITIAL_CAP_GENERATION);

/// §5.1：image Segment 的对象 ID（阶段 3 的 `SegmentTable` 槽位）。
pub const IMAGE_SEGMENT_ID: u32 = 1;
/// §5.4：init Namespace 的对象 ID。
pub const INIT_NAMESPACE_ID: u32 = 1;
/// §14 步骤 5：image Segment 根 Capability 的有限权限集。
pub const IMAGE_RIGHTS: Rights = Rights::from_rights(&[
    Right::Derive,
    Right::SegMapR,
    Right::SegMapX,
    Right::SegDestroy,
]);
/// §14 步骤 5：init Namespace 根 Capability 的**完整且有限**的 Namespace 权限集。
///
/// 先前实现用通配符 `"all"`，违反 §4.1 的 `FiniteSet<Right>` 与 `well_typed_rights`；
/// SPEC-061 改为规范列出的有限集（这会修正 `bootstrap_ready.namespace_rights` 的取值）。
pub const NAMESPACE_RIGHTS: Rights = Rights::from_rights(&[
    Right::Derive,
    Right::NsResolve,
    Right::NsBind,
    Right::NsUnbind,
    Right::NsMount,
    Right::NsUnmount,
    Right::NsDestroy,
]);
/// §14 步骤 5：init Task 根 Capability 的有限权限集。
pub const TASK_RIGHTS: Rights = Rights::from_rights(&[
    Right::Derive,
    Right::TaskSend,
    Right::TaskManageVSpace,
    Right::TaskDestroy,
]);
/// S2-D1：Capability 物理槽总数（初始 3 + 预定义异常源 + 平台 Segment cap + 派生 + 余量）。
///
/// 槽位会被复用（释放时 generation 递增），故该值是**同时存活**的 Capability 上限，
/// 不是历史累计数。
pub const MAX_CAPABILITY_SLOTS: usize = 64;
/// 兼容别名：历史上该常量用于 positions 表容量，现等于槽位数。
const MAX_CAPABILITY_POSITIONS: usize = MAX_CAPABILITY_SLOTS;
const MAX_TASKS: usize = 8;
/// Audit 容量（walk-in：平台 Segment 对象化会为每个区域追加创建/铸 cap 事件）。
const MAX_AUDIT_EVENTS: usize = 128;

/// 切片 L：内核 actor 的 Task ID。异常通知以此作为发送者，永不参与 `is_live` 校验。
pub const KERNEL_ACTOR: u32 = 0;
/// 切片 L：异常源数量上限（page_fault_source + syscall_trap_source + 余量）。
const MAX_EXC_SOURCES: usize = 4;
/// 切片 L：单个异常源的订阅者上限。
const MAX_SUBSCRIBERS: usize = 4;
/// 切片 L：预定义异常源 Capability 的槽位起始索引（初始 3 槽之后）。
///
/// 平台初始化按固定顺序创建 `page_fault_source`、`syscall_trap_source`，故它们拿到
/// 槽 4、5，句柄分别为 [`EXC_PAGE_FAULT_HANDLE`]、[`EXC_SYSCALL_TRAP_HANDLE`]。
pub const EXC_SOURCE_CAP_SLOT_BASE: u32 = 4;
/// `page_fault_source` 根 Capability 的 ABI 句柄。
pub const EXC_PAGE_FAULT_HANDLE: Handle =
    cap_handle(EXC_SOURCE_CAP_SLOT_BASE, INITIAL_CAP_GENERATION);
/// `syscall_trap_source` 根 Capability 的 ABI 句柄。
pub const EXC_SYSCALL_TRAP_HANDLE: Handle =
    cap_handle(EXC_SOURCE_CAP_SLOT_BASE + 1, INITIAL_CAP_GENERATION);
/// 切片 L：异常源根 Capability 的权限集（对齐 §12.6）。
pub const EXC_SOURCE_RIGHTS: Rights =
    Rights::from_rights(&[Right::Derive, Right::ExcSubscribe, Right::ExcSourceDestroy]);
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
        cap: Handle,
        parent: Handle,
        holder: u32,
    },
    CapabilityMoved {
        cap: Handle,
        from: CapabilityLocation,
        to: CapabilityLocation,
    },
    CapabilityRevoked {
        cap: Handle,
    },
    /// S2-D2：显式 `cap_drop`（§4.5）。撤销走 [`AuditEvent::CapabilityRevoked`]。
    CapabilityDropped {
        cap: Handle,
    },
    /// 切片 L：异常源创建（§12.6）。
    ExcSourceCreated {
        source_id: u32,
        cap: Handle,
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
    /// 切片 P：Bootstrap 铸造初始 Authority（§14 步骤 9 / §13.1 `Bootstrapped`）。
    Bootstrapped,
    /// 切片 P：对象创建（§13.1 要求活动对象有创建事件）。
    ObjectCreated {
        object_id: u32,
        segment: bool,
    },
    /// S2-D3：Segment 销毁（§13.1 对象生命周期；帧已归还来源池）。
    ObjectDestroyed {
        object_id: u32,
    },
    /// 切片 P：Task 状态迁移（W13 要求每次迁移有事件）。
    TaskStateChanged {
        task_id: u32,
        status: TaskStatus,
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
    /// §5.2/§14 步骤 7：Task 的结构性根 Namespace Capability（[`Handle::ZERO`] 表示无）。
    pub root_ns_cap: Handle,
    /// §5.2：Task 入口虚拟地址（`memseg_unmap` 的 `entry` 页保护）。
    pub entry: u64,
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
                    // §14 步骤 8：init 初始状态必须为 Ready，首次调度时迁 Running。
                    status: TaskStatus::Ready,
                    root_ns_cap: NAMESPACE_CAP_HANDLE,
                    entry: 0,
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
            // 非 init 任务由创建者显式设置结构根；本切片不创建此类任务的结构根。
            root_ns_cap: Handle::ZERO,
            entry: 0,
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
    pub capabilities: [Handle; MAX_MESSAGE_CAPABILITIES],
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
    pub image_cap: Handle,
    pub namespace_cap: Handle,
    pub task_cap: Handle,
    pub image_base: u64,
    pub image_len: u64,
    pub image_frames: u64,
    pub free_frames: u64,
    pub image_rights: Rights,
    pub namespace_rights: Rights,
    pub task_rights: Rights,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialObject {
    ImageSegment,
    Namespace,
    InitTask,
}

impl InitialObject {
    /// 初始对象在 `ObjectRef` 中的引用。
    pub const fn target(self) -> ObjectRef {
        match self {
            InitialObject::ImageSegment => ObjectRef::Segment(IMAGE_SEGMENT_ID),
            InitialObject::Namespace => ObjectRef::Namespace(INIT_NAMESPACE_ID),
            InitialObject::InitTask => ObjectRef::Task(INIT_TASK_ID),
        }
    }

    /// §4.1 类型匹配用的对象类型。
    pub const fn kind(self) -> ObjectKind {
        self.target().kind()
    }
}

/// Capability 目标对象的类型化引用（§4.1 的 `ObjectRef` 子集）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectRef {
    Segment(u32),
    Task(u32),
    Namespace(u32),
    ExcSource(u32),
}

impl ObjectRef {
    pub const fn kind(self) -> ObjectKind {
        match self {
            ObjectRef::Segment(_) => ObjectKind::Segment,
            ObjectRef::Task(_) => ObjectKind::Task,
            ObjectRef::Namespace(_) => ObjectKind::Namespace,
            ObjectRef::ExcSource(_) => ObjectKind::ExcSource,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitialCapability {
    pub handle: Handle,
    pub object: InitialObject,
    pub holder: u32,
    pub rights: Rights,
}

impl InitialCapability {
    pub const fn rights_for(&self, right: Right) -> bool {
        self.rights.contains(right)
    }
}

/// Capability 位置（§4.3）。丢弃即位置条目消失，不再需要 `Dropped` 变体。
///
/// 切片 R：平台 bring-up 的 Segment 根 Capability 由**内核平台 actor**持有，而不是任何用户
/// Task——故位置模型为内核持有单列一类（[`CapabilityLocation::HeldByKernel`]），既不复用
/// `HeldByTask(KERNEL_ACTOR)` 假造一个不存在的 Task，也不需要弱化持有者存活校验。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityLocation {
    HeldByTask(u32),
    /// 内核平台 actor 持有（[`KERNEL_ACTOR`]）；不是 TaskTable 条目。
    HeldByKernel,
    InMessage(u32),
}

impl CapabilityLocation {
    /// 位置对应的持有者 id（`InMessage` 无持有者）。
    pub const fn holder_id(self) -> Option<u32> {
        match self {
            CapabilityLocation::HeldByTask(task_id) => Some(task_id),
            CapabilityLocation::HeldByKernel => Some(KERNEL_ACTOR),
            CapabilityLocation::InMessage(_) => None,
        }
    }

    /// `holder` 是否就是该位置的持有者（`InMessage` 恒否）。
    pub const fn is_held_by(self, holder: u32) -> bool {
        match self.holder_id() {
            Some(id) => id == holder,
            None => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityPosition {
    pub handle: Handle,
    pub location: CapabilityLocation,
}

/// 位置表：**按槽索引直索引**（`entries[slot]`），槽释放即条目消失。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityPositionTable {
    pub entries: [Option<CapabilityPosition>; MAX_CAPABILITY_POSITIONS],
    /// 当前存在的条目数（初始根 + 存活派生）。
    pub count: usize,
}

/// 槽位状态（S2-X4 判定）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlotState {
    /// 槽位空闲（从未分配，或已释放且未复用）。
    Free,
    /// 槽位已被复用：句柄的 generation 落后于当前槽位。
    Stale,
    /// 槽位处于活动状态且 generation 匹配。
    Live,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlotEntry {
    pub generation: u32,
    pub in_use: bool,
}

/// Capability 槽位与 generation 表（S2-D1 的分配权威）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilitySlotTable {
    pub entries: [SlotEntry; MAX_CAPABILITY_SLOTS],
}

impl CapabilitySlotTable {
    const fn empty() -> Self {
        Self {
            entries: [SlotEntry {
                generation: Handle::FIRST_GENERATION,
                in_use: false,
            }; MAX_CAPABILITY_SLOTS],
        }
    }

    /// 保留一个固定槽（初始根、预定义异常源、平台根），返回其当前 generation 的句柄。
    ///
    /// **不重置 generation**：槽位一旦被释放过，其 generation 已递增，重置回 1 会让旧句柄
    /// 复活（违反 S2-X4）。调用方须先以 [`Self::slot_free`] 确认槽可用。
    fn reserve(&mut self, slot: u32) -> Handle {
        self.entries[slot as usize].in_use = true;
        cap_handle(slot, self.entries[slot as usize].generation)
    }

    /// 该槽位是否可被保留（合法索引、未被占用）。
    pub fn slot_free(&self, slot: u32) -> bool {
        slot != 0 && (slot as usize) < MAX_CAPABILITY_SLOTS && !self.entries[slot as usize].in_use
    }

    /// 分配最小空闲槽；槽位复用时不改变 generation（generation 只在释放时递增）。
    pub fn alloc(&mut self) -> Option<Handle> {
        let mut index = 1;
        while index < MAX_CAPABILITY_SLOTS {
            if !self.entries[index].in_use {
                self.entries[index].in_use = true;
                return Handle::new(index as u32, self.entries[index].generation).ok();
            }
            index += 1;
        }
        None
    }

    /// 释放槽位：`in_use=false` 且 generation 递增，使旧句柄永久不可用（S2-X4）。
    pub fn free(&mut self, handle: Handle) -> bool {
        let index = handle.index() as usize;
        if index == 0 || index >= MAX_CAPABILITY_SLOTS {
            return false;
        }
        if !self.entries[index].in_use || self.entries[index].generation != handle.generation() {
            return false;
        }
        let next = handle.next_generation();
        self.entries[index] = SlotEntry {
            generation: next,
            in_use: false,
        };
        true
    }

    pub fn check(&self, handle: Handle) -> SlotState {
        let index = handle.index() as usize;
        if index == 0 || index >= MAX_CAPABILITY_SLOTS {
            return SlotState::Free;
        }
        let entry = self.entries[index];
        if !entry.in_use {
            return SlotState::Free;
        }
        if entry.generation != handle.generation() {
            return SlotState::Stale;
        }
        SlotState::Live
    }

    pub fn live_count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.in_use).count()
    }
}

/// §4.2 的一条 `CapRecord`（初始 3 条根由 [`InitialCapability`] 表示，其余都在本表）。
///
/// `parent = None`：随对象创建铸造的根 Capability；`Some(p)`：由 `p` 派生。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityRecord {
    pub handle: Handle,
    pub parent: Option<Handle>,
    pub holder: u32,
    pub rights: Rights,
    /// 目标对象（派生时必须与父相同）。
    pub target: ObjectRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeriveError {
    MalformedHandle,
    /// 槽位从未分配。
    NoSuchParent,
    /// 槽位已被复用（旧句柄）。
    StaleHandle,
    /// 父不在调用者手中。
    ParentNotHeld,
    /// 父目标对象已销毁。
    TargetDead,
    /// 父缺少 `Derive`。
    MissingDerive,
    /// `requested ⊄ parent.rights`。
    RightsEscalation,
    /// 请求权限与目标对象类型不符（§4.1 `well_typed`）。
    WrongRightType,
    CapacityExceeded,
    AuditFull,
}

/// CapRecord 表：按槽索引直索引（`entries[slot]`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityRecordTable {
    pub entries: [Option<CapabilityRecord>; MAX_CAPABILITY_SLOTS],
    pub count: usize,
}

impl CapabilityRecordTable {
    pub const fn empty() -> Self {
        Self {
            entries: [None; MAX_CAPABILITY_SLOTS],
            count: 0,
        }
    }

    pub fn find(&self, handle: Handle) -> Option<&CapabilityRecord> {
        self.entries[handle.index() as usize].as_ref()
    }

    pub fn insert(
        &mut self,
        handle: Handle,
        parent: Option<Handle>,
        holder: u32,
        rights: Rights,
        target: ObjectRef,
    ) -> Result<(), DeriveError> {
        if self.entries[handle.index() as usize].is_some() {
            return Err(DeriveError::CapacityExceeded);
        }
        self.entries[handle.index() as usize] = Some(CapabilityRecord {
            handle,
            parent,
            holder,
            rights,
            target,
        });
        self.count += 1;
        Ok(())
    }

    fn remove(&mut self, handle: Handle) -> bool {
        let slot = &mut self.entries[handle.index() as usize];
        if slot.is_some() {
            *slot = None;
            self.count -= 1;
            true
        } else {
            false
        }
    }

    /// 撤销以 `root` 为根的派生子树，返回被撤销的句柄（含 root）。
    ///
    /// 只沿 `parent = Some` 的链传播：`parent = None` 的铸造根不会被「父链」误伤。
    fn revoke_subtree(&mut self, root: Handle) -> ([Handle; MAX_CAPABILITY_SLOTS], usize) {
        let mut revoked = [Handle::ZERO; MAX_CAPABILITY_SLOTS];
        if self.find(root).is_none() {
            // root 不是本表中的 Capability（初始根），仅返回自身。
            revoked[0] = root;
            return (revoked, 1);
        }
        revoked[0] = root;
        let mut revoked_count = 1;
        self.remove(root);
        loop {
            let mut changed = false;
            let mut index = 1;
            while index < MAX_CAPABILITY_SLOTS {
                let matches = self.entries[index].as_ref().is_some_and(|entry| {
                    entry
                        .parent
                        .is_some_and(|parent| revoked[..revoked_count].contains(&parent))
                });
                if matches {
                    let handle = self.entries[index]
                        .as_ref()
                        .map(|entry| entry.handle)
                        .unwrap_or(Handle::ZERO);
                    self.entries[index] = None;
                    self.count -= 1;
                    if revoked_count < MAX_CAPABILITY_SLOTS {
                        revoked[revoked_count] = handle;
                        revoked_count += 1;
                    }
                    changed = true;
                }
                index += 1;
            }
            if !changed {
                break;
            }
        }
        (revoked, revoked_count)
    }
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

/// 切片 L：一个异常源。capability 元数据内联在源记录里，**不进入 positions 表**
/// （它没有位置迁移语义），但其句柄占用 [`CapabilitySlotTable`] 的槽位。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcSource {
    pub id: u32,
    pub kind: ExcKind,
    /// 指向本源的根 Capability 句柄；holder/rights 存于 [`CapabilityRecordTable`]。
    pub cap: Handle,
    pub subscribers: [Option<u32>; MAX_SUBSCRIBERS],
    pub count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcSourceTable {
    pub entries: [Option<ExcSource>; MAX_EXC_SOURCES],
    pub count: usize,
}

impl ExcSourceTable {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_EXC_SOURCES],
            count: 0,
        }
    }

    pub fn find(&self, source_id: u32) -> Option<&ExcSource> {
        self.entries[..self.count]
            .iter()
            .flatten()
            .find(|source| source.id == source_id)
    }

    pub fn find_by_cap(&self, cap: Handle) -> Option<&ExcSource> {
        self.entries[..self.count]
            .iter()
            .flatten()
            .find(|source| source.cap == cap)
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
    MalformedHandle,
    NoSuchCapability,
    StaleHandle,
    NotHeld,
    HolderMismatch,
    DuplicateCapability,
    CapacityExceeded,
    ProtectedTask,
    /// §4.5：活动 Task 的结构性根 Namespace Capability 不可丢弃。
    StructuralCap,
    AuditFull,
}

impl CapabilityPositionTable {
    fn from_capabilities(
        capabilities: &[InitialCapability; INITIAL_CAPABILITY_COUNT as usize],
    ) -> Self {
        let mut table = Self {
            entries: [None; MAX_CAPABILITY_POSITIONS],
            count: 0,
        };
        for capability in capabilities {
            table.entries[capability.handle.index() as usize] = Some(CapabilityPosition {
                handle: capability.handle,
                location: CapabilityLocation::HeldByTask(capability.holder),
            });
            table.count += 1;
        }
        table
    }

    pub fn find(&self, handle: Handle) -> Option<&CapabilityPosition> {
        self.entries[handle.index() as usize].as_ref()
    }

    fn find_mut(&mut self, handle: Handle) -> Option<&mut CapabilityPosition> {
        self.entries[handle.index() as usize].as_mut()
    }

    pub fn register(&mut self, handle: Handle, holder: u32) -> Result<(), CapabilityPositionError> {
        self.register_location(handle, CapabilityLocation::HeldByTask(holder))
    }

    /// 按显式位置登记（内核持有 vs Task 持有的区分点，切片 R）。
    pub fn register_location(
        &mut self,
        handle: Handle,
        location: CapabilityLocation,
    ) -> Result<(), CapabilityPositionError> {
        if self.entries[handle.index() as usize].is_some() {
            return Err(CapabilityPositionError::DuplicateCapability);
        }
        if self.count == MAX_CAPABILITY_POSITIONS {
            return Err(CapabilityPositionError::CapacityExceeded);
        }
        self.entries[handle.index() as usize] = Some(CapabilityPosition { handle, location });
        self.count += 1;
        Ok(())
    }

    /// 每个槽至多一个位置条目（按槽索引直索引，故唯一性由构造保证）。
    pub fn has_unique_active_locations(&self) -> bool {
        self.entries
            .iter()
            .flatten()
            .all(|entry| entry.handle.index() != 0)
    }

    pub fn move_to_task(
        &mut self,
        handle: Handle,
        from_task: u32,
        to_task: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self
            .find_mut(handle)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.location.is_held_by(from_task) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::HeldByTask(to_task);
        Ok(())
    }

    pub fn drop_from_task(
        &mut self,
        handle: Handle,
        holder: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self
            .find_mut(handle)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.location.is_held_by(holder) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        self.remove(handle);
        Ok(())
    }

    pub fn remove(&mut self, handle: Handle) -> bool {
        let slot = &mut self.entries[handle.index() as usize];
        if slot.is_some() {
            *slot = None;
            self.count -= 1;
            true
        } else {
            false
        }
    }

    fn move_to_message(
        &mut self,
        handle: Handle,
        holder: u32,
        message_id: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self
            .find_mut(handle)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.location.is_held_by(holder) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::InMessage(message_id);
        Ok(())
    }

    fn move_from_message(
        &mut self,
        handle: Handle,
        message_id: u32,
        receiver: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self
            .find_mut(handle)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if entry.location != CapabilityLocation::InMessage(message_id) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::HeldByTask(receiver);
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameOwnership {
    pub image_start: u64,
    pub image_end: u64,
    pub image_frames: u64,
    pub free_frames: u64,
}

/// 平台 bring-up Segment 的槽位 id 区间（切片 R：`frames::SEG_*` 即这些 id）。
///
/// id 1 保留给 image；2..14 是 13 个平台区域；用户经 `memseg_create` 创建的 Segment 从
/// [`USER_SEGMENT_ID_FIRST`] 起。**帧注册表的归属标签与 Segment id 恒等**——独立的
/// `100 + id` 标签命名空间在切片 R 取消。
pub const PLATFORM_SEGMENT_ID_FIRST: u32 = 2;
pub const PLATFORM_SEGMENT_ID_LAST: u32 = 14;
pub const PLATFORM_SEGMENT_COUNT: usize = 13;
/// 平台 Segment 根 Capability 的槽位基址（32..44）。
///
/// 必须避开槽 4/5（预定义异常源）与槽 6/7（ring3 见证程序内嵌的句柄，见
/// `syscall.rs` 的 `CAP_CHILD*`/`MEMSEG_CAP*`），故显式指定高位区间而不用最低空闲分配。
pub const PLATFORM_SEGMENT_CAP_SLOT_BASE: u32 = 32;
/// 平台 Task Capability 槽位（45 = task 1，46 = task 2）。
pub const PLATFORM_TASK_CAP_SLOT_BASE: u32 =
    PLATFORM_SEGMENT_CAP_SLOT_BASE + PLATFORM_SEGMENT_COUNT as u32;
pub const PLATFORM_TASK_CAP_COUNT: usize = 2;
pub const PLATFORM_TASK1_CAP_HANDLE: Handle =
    cap_handle(PLATFORM_TASK_CAP_SLOT_BASE, INITIAL_CAP_GENERATION);
pub const PLATFORM_TASK2_CAP_HANDLE: Handle =
    cap_handle(PLATFORM_TASK_CAP_SLOT_BASE + 1, INITIAL_CAP_GENERATION);
/// 平台 Task Capability 只用于把平台区域映射进对应任务根，故只给 vspace 管理权。
pub const PLATFORM_TASK_RIGHTS: Rights =
    Rights::from_rights(&[Right::Derive, Right::TaskManageVSpace]);
/// 用户创建 Segment 的槽位起点（平台区间之后）。
pub const USER_SEGMENT_ID_FIRST: u32 = PLATFORM_SEGMENT_ID_LAST + 1;

/// 平台 Segment id → 根 Capability 槽位。
pub const fn platform_segment_cap_slot(id: u32) -> u32 {
    PLATFORM_SEGMENT_CAP_SLOT_BASE + (id - PLATFORM_SEGMENT_ID_FIRST)
}

/// 平台 Segment id → 根 Capability 句柄（gen1）。
pub const fn platform_segment_cap_handle(id: u32) -> Handle {
    cap_handle(platform_segment_cap_slot(id), INITIAL_CAP_GENERATION)
}

/// 该平台区域是否保留**内核直映射**（不走 `memseg_map`）。
///
/// VSPACE 见证区需要 `set_leaf` 重映射见证、COW 需要 fault-time 换映射且页为内核私有
/// （`allowed_access` 为空），二者都超出 `memseg_map` 的语义。
pub const fn platform_segment_is_direct(id: u32) -> bool {
    id == crate::frames::SEG_VSPACE || id == crate::frames::SEG_COW
}

/// 切片 R：平台 bring-up Segment 对象化的阶段末见证（`platform_segments_ready` 事件）。
///
/// 按需计算、不落存储字段（`BootstrapState` 已贴近 16 KiB 结构体守卫）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformSegmentsRecord {
    /// 真实存在且活动的平台 Segment 对象数。
    pub segments: u32,
    pub first_id: u32,
    pub last_id: u32,
    /// 通过全部校验的平台根 Capability 数（槽 live、目标 `Segment(id)`、位置 `HeldByKernel`、
    /// 权限非空且类型正确）。
    pub roots: u32,
    /// 通过校验的平台 Task Capability 数。
    pub task_caps: u32,
    /// 经 `memseg_map` 授权中介映射的平台区域数。
    pub mapped: u32,
    /// 保留内核直映射的平台区域数。
    pub direct: u32,
    /// `TaskVspaceTable` 中 `seg_id` 落在平台区间的逻辑映射条目数。
    pub vspace: u32,
    /// 逐帧归属标签与 Segment id 恒等（`owner_of(frame) == Segment(id)`）且无缺帧。
    pub unified: bool,
    pub cap_handles: [Handle; PLATFORM_SEGMENT_COUNT],
    pub task_cap_handles: [Handle; PLATFORM_TASK_CAP_COUNT],
    pub frame_counts: [u32; PLATFORM_SEGMENT_COUNT],
    pub direct_ids: [u32; 2],
}

/// 单个 Segment 的帧向量容量（`memseg_create` 的页数上限）。
pub const MAX_SEGMENT_FRAMES: usize = 16;

/// §7.2：`access` 只允许 R/W/X 三个 Segment 映射权限。
pub const SEGMENT_ACCESS_RIGHTS: Rights =
    Rights::from_rights(&[Right::SegMapR, Right::SegMapW, Right::SegMapX]);

/// §7.1 步骤 4：根 Capability 权限 = `Derive | SegDestroy` + 按 `allowed_access` 加入 SegMap*。
pub const SEGMENT_ROOT_RIGHTS: Rights = SEGMENT_ACCESS_RIGHTS;

/// §5.1：Segment 的帧集合。
///
/// `Contiguous` 用于 bootloader 载入的 image（帧连续、数量随内核镜像变化）；
/// `Vector` 是 `memseg_create` 新建 Segment 的**真实帧向量**（ISSUE-FRAME-001）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentFrames {
    Contiguous {
        base: u64,
        count: usize,
    },
    Vector {
        frames: [u64; MAX_SEGMENT_FRAMES],
        count: usize,
    },
}

impl SegmentFrames {
    pub fn frame_count(&self) -> usize {
        match self {
            SegmentFrames::Contiguous { count, .. } => *count,
            SegmentFrames::Vector { count, .. } => *count,
        }
    }

    pub fn frame_at(&self, index: usize) -> Option<u64> {
        match self {
            SegmentFrames::Contiguous { base, count } => {
                if index < *count {
                    Some(base + (index as u64) * crate::mem::FRAME_SIZE)
                } else {
                    None
                }
            }
            SegmentFrames::Vector { frames, count } => {
                if index < *count {
                    Some(frames[index])
                } else {
                    None
                }
            }
        }
    }
}

/// §5.1：Segment 对象（阶段 2：image / 平台 bring-up / 用户创建三类共用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentObject {
    /// 槽位索引（即 §5.1 的 `SegId`；`IMAGE_SEGMENT_ID` 保留给 image）。
    pub id: u32,
    /// 槽位复用计数（`memseg_destroy` 后递增，使旧句柄不可用）。
    pub generation: u32,
    pub creator: u32,
    pub pool: crate::frames::Pool,
    pub frames: SegmentFrames,
    pub allowed_access: Rights,
    pub active: bool,
}

impl SegmentObject {
    pub fn frame_count(&self) -> usize {
        self.frames.frame_count()
    }
}

/// 每个 Task 的逻辑 vspace 视图（`memseg_map`/`unmap` 维护；`StillMapped` 与重叠判定的依据）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VspaceEntry {
    pub task_id: u32,
    pub va: u64,
    pub seg_id: u32,
    pub seg_offset: u64,
    pub access: Rights,
}

pub const MAX_VSPACE_ENTRIES: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskVspaceTable {
    pub entries: [Option<VspaceEntry>; MAX_VSPACE_ENTRIES],
    pub count: usize,
}

impl TaskVspaceTable {
    const fn empty() -> Self {
        Self {
            entries: [None; MAX_VSPACE_ENTRIES],
            count: 0,
        }
    }

    fn find(&self, task_id: u32, va: u64) -> Option<&VspaceEntry> {
        self.entries[..self.count]
            .iter()
            .flatten()
            .find(|entry| entry.task_id == task_id && entry.va == va)
    }

    fn insert(&mut self, entry: VspaceEntry) -> Result<(), MemsegError> {
        if self.count == MAX_VSPACE_ENTRIES {
            return Err(MemsegError::CapacityExceeded);
        }
        self.entries[self.count] = Some(entry);
        self.count += 1;
        Ok(())
    }

    fn remove(&mut self, task_id: u32, va: u64) -> Option<VspaceEntry> {
        let index = self.entries[..self.count].iter().position(|entry| {
            entry.is_some_and(|entry| entry.task_id == task_id && entry.va == va)
        })?;
        let entry = self.entries[index].take();
        for shift in index..self.count - 1 {
            self.entries[shift] = self.entries[shift + 1];
        }
        self.entries[self.count - 1] = None;
        self.count -= 1;
        entry
    }

    fn references_segment(&self, seg_id: u32) -> bool {
        self.entries[..self.count]
            .iter()
            .flatten()
            .any(|entry| entry.seg_id == seg_id)
    }
}

/// S2-D3：`memseg_*` 的失败原因（对齐 §7.1~§7.4 的错误名）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemsegError {
    ZeroSize,
    InvalidAccess,
    NoMemory,
    MalformedHandle,
    NoSuchCapability,
    StaleHandle,
    NotInstalled,
    NotHolder,
    MissingRight,
    MissingMapRight,
    RightsExceeded,
    WrongRightType,
    TargetDead,
    OutOfBounds,
    Overlap,
    EntryMapping,
    NotMapped,
    StillMapped,
    CapacityExceeded,
    AuditFull,
}

/// §5.4：Namespace 对象（本切片只物化空 init Namespace）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamespaceObject {
    pub id: u32,
    pub creator: u32,
    pub entries: usize,
}

/// 切片 P：初始 Authority 的阶段末见证（供 `authority_ready` 事件使用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapRecord {
    pub caps: u32,
    pub segments: u32,
    pub namespaces: u32,
    pub booted: bool,
    /// §14 步骤 8：bootstrap 完成时 init 状态为 `Ready`。
    pub initial_status_ready: bool,
    /// §14 前置 3：入口落在 image 范围内（已在 `create_state` 校验）。
    pub entry_in_image: bool,
    pub root_ns_cap: Handle,
    /// W13：首次调度后 init 已迁到 `Running`。
    pub init_running: bool,
}

/// §5.1/§5.4 对象表容量：image(1) + 平台 bring-up 13 个(2..14) + 用户创建 Segment(15..)。
///
/// 取 32 是因为 `memseg_destroy` **不回收槽位**（只置 `active = false`），故每次
/// `memseg_create` 都消耗一个新槽。上限受 `bootstrap_state_stays_within_the_stack_budget`
/// 的 16 KiB 结构体守卫约束：32 槽时约 15 KiB，**不得再向 `BootstrapState` 增加存储字段**。
pub const MAX_SEGMENT_OBJECTS: usize = 32;
pub const MAX_NAMESPACE_OBJECTS: usize = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootstrapState {
    pub snapshot: BootstrapSnapshot,
    /// S2-D1：Capability 槽位与 generation（分配权威）。
    pub slots: CapabilitySlotTable,
    pub capabilities: [InitialCapability; INITIAL_CAPABILITY_COUNT as usize],
    pub positions: CapabilityPositionTable,
    /// §4.2：除初始 3 条根之外的全部 `CapRecord`（铸造根 + 派生）。
    pub records: CapabilityRecordTable,
    pub tasks: TaskTable,
    pub ipc: IpcQueue,
    pub audit: AuditLog,
    pub frame_ownership: FrameOwnership,
    /// 切片 L：异常源表（capability 元数据内联，不入 positions）。
    pub exc_sources: ExcSourceTable,
    /// 切片 L：平台初始化期间 pending 递送的留证。
    pub exc_boot: Option<ExcBootRecord>,
    /// §5.1：Segment 对象表（image + 平台 bring-up + 用户创建）。
    pub segments: [Option<SegmentObject>; MAX_SEGMENT_OBJECTS],
    pub segment_count: usize,
    /// 切片 P：§14 步骤 3 物化的 init Namespace。
    pub namespaces: [Option<NamespaceObject>; MAX_NAMESPACE_OBJECTS],
    pub namespace_count: usize,
    /// S2-D3：每 Task 的逻辑 vspace 视图（`memseg_map`/`unmap` 维护）。
    pub vspace: TaskVspaceTable,
    /// 切片 P：W14——`bootstrap` 成功后置位；重复调用返回 `AlreadyBooted`。
    pub booted: bool,
    /// 切片 P：初始 Authority 的阶段末见证记录。
    pub boot_record: Option<BootstrapRecord>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapError {
    EmptyImage,
    ImageRangeOverflow,
    InvalidFrameOwnership,
    InvalidAuthority,
    /// §14 前置 3：入口必须落在 image 范围内。
    EntryOutsideImage,
    /// W14：`booted=true` 后不得再次成功执行 `bootstrap`。
    AlreadyBooted,
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

/// 切片 K / S2-D1：授权查询失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthError {
    /// 授权表尚未安装（`install` 未调用）。
    NotInstalled,
    /// 句柄编码非法（index 或 generation 为 0）。
    MalformedHandle,
    /// 槽位从未分配（或已释放且未复用）。
    NoSuchCapability,
    /// 槽位已被复用，句柄的 generation 落后（S2-X4：stale handle 不可授权）。
    StaleHandle,
    /// Capability 存在但当前位置不在调用者手中。
    NotHolder,
    /// 持有者缺少所请求的权限。
    MissingRight,
    /// 目标对象已销毁：stale token 不可用于授权（但可 `cap_drop`）。
    TargetDead,
}

/// S2-D2：`cap_drop` 失败原因（§4.5）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropError {
    NotInstalled,
    MalformedHandle,
    NoSuchCapability,
    StaleHandle,
    NotHolder,
    /// 活动 Task 的结构性根 Namespace Capability。
    StructuralCap,
    AuditFull,
}

impl BootstrapState {
    pub fn validate_authority(&self) -> Result<(), AuthorityError> {
        // 计数：初始 3 条根 + 记录表 = 有位置的 Capability；且与活动槽数一致。
        if self.positions.count != INITIAL_CAPABILITY_COUNT as usize + self.records.count
            || self.slots.live_count() != self.positions.count
            || self.records.count > MAX_CAPABILITY_SLOTS
            || self.audit.count > MAX_AUDIT_EVENTS
        {
            return Err(AuthorityError::InvalidCount);
        }

        // 初始根：槽 live、holder 活跃、权限非空且与目标类型匹配（§4.1）。
        for capability in &self.capabilities {
            if self.slots.check(capability.handle) != SlotState::Live {
                return Err(AuthorityError::MissingPosition);
            }
            if !self.tasks.is_live(capability.holder) || capability.rights.is_empty() {
                return Err(AuthorityError::InvalidHolder);
            }
            if !capability.rights.well_typed(capability.object.kind()) {
                return Err(AuthorityError::RightsEscalation);
            }
        }

        // 每条 CapRecord：槽 live、位置存在、父链权限子集、同目标、类型匹配。
        for record in self.records.entries.iter().flatten() {
            if self.slots.check(record.handle) != SlotState::Live {
                return Err(AuthorityError::InvalidCount);
            }
            if self.positions.find(record.handle).is_none() {
                return Err(AuthorityError::MissingPosition);
            }
            if let Some(parent) = record.parent {
                let parent_rights = self
                    .rights_of(parent)
                    .ok_or(AuthorityError::InvalidParent)?;
                if !record.rights.subset_of(parent_rights) {
                    return Err(AuthorityError::RightsEscalation);
                }
                if self.target_of(parent) != Some(record.target) {
                    return Err(AuthorityError::InvalidParent);
                }
            }
            if !record.rights.well_typed(record.target.kind()) {
                return Err(AuthorityError::RightsEscalation);
            }
        }

        // 位置：HeldByTask 的任务必须活跃；HeldByKernel 的持有者不是 Task、恒在；InMessage
        // 的消息必须在队列中且含该句柄。
        for position in self.positions.entries.iter().flatten() {
            match position.location {
                CapabilityLocation::HeldByTask(task_id) => {
                    if !self.tasks.is_live(task_id) {
                        return Err(AuthorityError::InvalidHolder);
                    }
                }
                CapabilityLocation::HeldByKernel => {}
                CapabilityLocation::InMessage(message_id) => {
                    let queued =
                        self.ipc.entries[..self.ipc.count]
                            .iter()
                            .flatten()
                            .any(|message| {
                                message.id == message_id
                                    && message.capabilities[..message.capability_count]
                                        .contains(&position.handle)
                            });
                    if !queued {
                        return Err(AuthorityError::PositionMismatch);
                    }
                }
            }
        }

        // §14 步骤 2/3：image Segment 与 init Namespace 必须物化。
        if self
            .segments
            .iter()
            .flatten()
            .all(|segment| segment.id != IMAGE_SEGMENT_ID || !segment.active)
            || self.namespaces[..self.namespace_count]
                .iter()
                .flatten()
                .all(|namespace| namespace.id != INIT_NAMESPACE_ID)
        {
            return Err(AuthorityError::InvalidHolder);
        }

        // W5：init 的结构性根 Namespace Capability 由 init 持有且含 `NsResolve`。
        let init = self
            .tasks
            .find(INIT_TASK_ID)
            .ok_or(AuthorityError::InvalidHolder)?;
        let root_ns_cap = init.root_ns_cap;
        let root_position = self
            .positions
            .find(root_ns_cap)
            .ok_or(AuthorityError::InvalidHolder)?;
        if root_position.location != CapabilityLocation::HeldByTask(INIT_TASK_ID) {
            return Err(AuthorityError::InvalidHolder);
        }
        let root_rights = self
            .rights_of(root_ns_cap)
            .ok_or(AuthorityError::InvalidHolder)?;
        if !root_rights.contains(Right::NsResolve) {
            return Err(AuthorityError::RightsEscalation);
        }

        // 异常源：cap 必须是一条目标为 `ExcSource(id)` 的 live 记录；订阅者活跃；id 唯一。
        for source in self.exc_sources.entries[..self.exc_sources.count]
            .iter()
            .flatten()
        {
            let record = self
                .records
                .find(source.cap)
                .ok_or(AuthorityError::MissingPosition)?;
            if record.target != ObjectRef::ExcSource(source.id) || record.rights.is_empty() {
                return Err(AuthorityError::InvalidHolder);
            }
            if !self.tasks.is_live(record.holder) {
                return Err(AuthorityError::InvalidHolder);
            }
            let duplicate = self.exc_sources.entries[..self.exc_sources.count]
                .iter()
                .flatten()
                .filter(|candidate| candidate.cap == source.cap || candidate.id == source.id)
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

    /// 句柄 → 权限集合（初始根或 CapRecord）。
    pub fn rights_of(&self, handle: Handle) -> Option<Rights> {
        self.capabilities
            .iter()
            .find(|capability| capability.handle == handle)
            .map(|capability| capability.rights)
            .or_else(|| self.records.find(handle).map(|record| record.rights))
    }

    /// 句柄 → 目标对象（初始根或 CapRecord）。
    pub fn target_of(&self, handle: Handle) -> Option<ObjectRef> {
        self.capabilities
            .iter()
            .find(|capability| capability.handle == handle)
            .map(|capability| capability.object.target())
            .or_else(|| self.records.find(handle).map(|record| record.target))
    }

    /// `caller` 是否持有句柄（位置为 `HeldBy(caller)`）。
    pub fn holder_of(&self, handle: Handle) -> Option<u32> {
        self.positions
            .find(handle)
            .and_then(|position| position.location.holder_id())
    }

    /// 目标对象是否存活（§4.3 `usable` 的 `live(target)` 分量）。
    pub fn target_live(&self, target: ObjectRef) -> bool {
        match target {
            ObjectRef::Segment(id) => self
                .segments
                .iter()
                .flatten()
                .any(|segment| segment.id == id && segment.active),
            ObjectRef::Task(id) => self.tasks.is_live(id),
            ObjectRef::Namespace(id) => self.namespaces[..self.namespace_count]
                .iter()
                .flatten()
                .any(|namespace| namespace.id == id),
            ObjectRef::ExcSource(id) => self.exc_sources.find(id).is_some(),
        }
    }

    /// §14 步骤 8 + W13：首次调度 init——`Ready → Running` 并追加 `TaskStateChanged`。
    pub fn activate_init(&mut self) -> Result<(), BootstrapError> {
        let Some(task) = self.tasks.find_mut(INIT_TASK_ID) else {
            return Err(BootstrapError::InvalidAuthority);
        };
        if task.status != TaskStatus::Ready {
            return Err(BootstrapError::InvalidAuthority);
        }
        task.status = TaskStatus::Running;
        self.audit.append(AuditEvent::TaskStateChanged {
            task_id: INIT_TASK_ID,
            status: TaskStatus::Running,
        });
        if let Some(record) = self.boot_record.as_mut() {
            record.init_running = true;
        }
        Ok(())
    }

    /// 切片 K / S2-D1：授权查询——`caller` 是否持有 `cap` 且该 Capability 具备 `right`。
    ///
    /// 检查顺序（S2-X4）：槽位状态 → 目标存活 → 持有者 → 权限。stale 句柄在槽位阶段即被拒，
    /// 不会因槽位复用而重获 Authority。失败不修改状态。
    pub fn authorize(&self, caller: u32, cap: Handle, right: Right) -> Result<Handle, AuthError> {
        match self.slots.check(cap) {
            SlotState::Free => return Err(AuthError::NoSuchCapability),
            SlotState::Stale => return Err(AuthError::StaleHandle),
            SlotState::Live => {}
        }
        let target = self.target_of(cap).ok_or(AuthError::NoSuchCapability)?;
        if !self.target_live(target) {
            return Err(AuthError::TargetDead);
        }
        if self.holder_of(cap) != Some(caller) {
            return Err(AuthError::NotHolder);
        }
        let rights = self.rights_of(cap).ok_or(AuthError::NoSuchCapability)?;
        if !rights.contains(right) {
            return Err(AuthError::MissingRight);
        }
        Ok(cap)
    }

    /// 解码 syscall/协议传入的裸 `u64` 句柄（`Malformed` → [`AuthError::MalformedHandle`]）。
    pub fn decode_handle(&self, raw: u64) -> Result<Handle, AuthError> {
        Handle::decode(raw).map_err(|_| AuthError::MalformedHandle)
    }

    /// S2-D2：`cap_drop`（§4.5）。目标对象已销毁不构成失败；结构性根被保护。
    pub fn drop_capability(&mut self, caller: u32, cap: Handle) -> Result<(), DropError> {
        match self.slots.check(cap) {
            SlotState::Free => return Err(DropError::NoSuchCapability),
            SlotState::Stale => return Err(DropError::StaleHandle),
            SlotState::Live => {}
        }
        if self.holder_of(cap) != Some(caller) {
            return Err(DropError::NotHolder);
        }
        let structural = self
            .tasks
            .find(caller)
            .is_some_and(|task| task.status != TaskStatus::Dead && task.root_ns_cap == cap);
        if structural {
            return Err(DropError::StructuralCap);
        }
        if !self.audit.can_append(1) {
            return Err(DropError::AuditFull);
        }
        self.positions.remove(cap);
        self.records.remove(cap);
        self.slots.free(cap);
        self.audit.append(AuditEvent::CapabilityDropped { cap });
        Ok(())
    }

    /// 释放一个 Capability 的槽位（内部用：派生撤销/任务销毁）。返回是否成功。
    fn retire_capability(&mut self, cap: Handle) -> bool {
        self.positions.remove(cap);
        self.records.remove(cap);
        self.slots.free(cap)
    }

    /// 铸造一条根 Capability（随对象创建，`parent = None`，§4.2）。
    pub fn mint_root_capability(
        &mut self,
        holder: u32,
        rights: Rights,
        target: ObjectRef,
    ) -> Result<Handle, DeriveError> {
        if !self.audit.can_append(1) {
            return Err(DeriveError::AuditFull);
        }
        if !rights.well_typed(target.kind()) {
            return Err(DeriveError::WrongRightType);
        }
        let handle = self.slots.alloc().ok_or(DeriveError::CapacityExceeded)?;
        self.records.insert(handle, None, holder, rights, target)?;
        self.positions
            .register(handle, holder)
            .map_err(|_| DeriveError::CapacityExceeded)?;
        Ok(handle)
    }

    /// 在**显式槽位**铸造一条根 Capability（切片 R：平台 actor 的固定槽区间）。
    ///
    /// 与 [`Self::mint_root_capability`] 的差别只有分配策略：平台 cap 必须落在预留区间内，
    /// 以免占用 ring3 见证程序内嵌句柄所用的最低空闲槽。`slot` 为 0、越界或已占用即失败。
    pub fn mint_root_capability_at(
        &mut self,
        holder: CapabilityLocation,
        slot: u32,
        rights: Rights,
        target: ObjectRef,
    ) -> Result<Handle, DeriveError> {
        if !self.slots.slot_free(slot) {
            return Err(DeriveError::CapacityExceeded);
        }
        if !self.audit.can_append(1) {
            return Err(DeriveError::AuditFull);
        }
        if !rights.well_typed(target.kind()) {
            return Err(DeriveError::WrongRightType);
        }
        let record_holder = holder.holder_id().unwrap_or(KERNEL_ACTOR);
        let handle = self.slots.reserve(slot);
        self.records
            .insert(handle, None, record_holder, rights, target)?;
        self.positions
            .register_location(handle, holder)
            .map_err(|_| DeriveError::CapacityExceeded)?;
        Ok(handle)
    }

    /// 切片 L：创建一个异常源并铸造其根 Capability（holder 为 init Task）。
    ///
    /// 返回 `(source_id, cap_handle)`。追加 `ExcSourceCreated` Audit（§12.6）。
    pub fn exc_source_create(&mut self, kind: ExcKind) -> Result<(u32, Handle), ExcError> {
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        if self.exc_sources.count == MAX_EXC_SOURCES {
            return Err(ExcError::Full);
        }
        let source_id = self.exc_sources.count as u32 + 1;
        let cap = self
            .mint_root_capability(
                INIT_TASK_ID,
                EXC_SOURCE_RIGHTS,
                ObjectRef::ExcSource(source_id),
            )
            .map_err(|_| ExcError::Full)?;
        self.exc_sources.entries[self.exc_sources.count] = Some(ExcSource {
            id: source_id,
            kind,
            cap,
            subscribers: [None; MAX_SUBSCRIBERS],
            count: 0,
        });
        self.exc_sources.count += 1;
        self.audit.append(AuditEvent::ExcSourceCreated {
            source_id,
            cap,
            kind,
        });
        Ok((source_id, cap))
    }

    /// 切片 L：`caller` 用持有的 `cap_id` 订阅 `kind`（§12.4）。
    ///
    /// 成功返回 source_id；失败不修改任何状态、不追加 Audit。
    pub fn exc_subscribe(
        &mut self,
        caller: u32,
        cap: Handle,
        kind: ExcKind,
    ) -> Result<u32, ExcError> {
        if !self.tasks.is_live(caller) {
            return Err(ExcError::NoSuchTask);
        }
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        if self.slots.check(cap) != SlotState::Live {
            return Err(ExcError::NoSuchSource);
        }
        let source = self
            .exc_sources
            .find_by_cap(cap)
            .ok_or(ExcError::NoSuchSource)?;
        let source_id = source.id;
        let record = self.records.find(cap).ok_or(ExcError::NoSuchSource)?;
        if record.holder != caller {
            return Err(ExcError::NotHolder);
        }
        if !record.rights.contains(Right::ExcSubscribe) {
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
    pub fn exc_unsubscribe(&mut self, caller: u32, cap: Handle) -> Result<u32, ExcError> {
        if !self.tasks.is_live(caller) {
            return Err(ExcError::NoSuchTask);
        }
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        if self.slots.check(cap) != SlotState::Live {
            return Err(ExcError::NoSuchSource);
        }
        let source = self
            .exc_sources
            .find_by_cap(cap)
            .ok_or(ExcError::NoSuchSource)?;
        let source_id = source.id;
        let record = self.records.find(cap).ok_or(ExcError::NoSuchSource)?;
        if record.holder != caller {
            return Err(ExcError::NotHolder);
        }
        if !record.rights.contains(Right::ExcSubscribe) {
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
    pub fn exc_subscribed(&self, caller: u32, cap: Handle, kind: ExcKind) -> Result<u32, ExcError> {
        if !self.tasks.is_live(caller) {
            return Err(ExcError::NoSuchTask);
        }
        if self.slots.check(cap) != SlotState::Live {
            return Err(ExcError::NoSuchSource);
        }
        let source = self
            .exc_sources
            .find_by_cap(cap)
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
                capabilities: [Handle::ZERO; MAX_MESSAGE_CAPABILITIES],
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

    /// §4.4 `cap_derive`：`caller` 用持有的 `parent` 派生一个子 Capability。
    ///
    /// 失败不修改状态。检查顺序：槽位 → 父持有 → 目标存活 → `Derive` 权限 →
    /// 权限子集（`RightsExceeded`）→ 类型匹配（`WrongRightType`）。
    pub fn derive_capability(
        &mut self,
        caller: u32,
        parent: Handle,
        rights: Rights,
    ) -> Result<Handle, DeriveError> {
        match self.slots.check(parent) {
            SlotState::Free => return Err(DeriveError::NoSuchParent),
            SlotState::Stale => return Err(DeriveError::StaleHandle),
            SlotState::Live => {}
        }
        if self.holder_of(parent) != Some(caller) {
            return Err(DeriveError::ParentNotHeld);
        }
        let target = self.target_of(parent).ok_or(DeriveError::NoSuchParent)?;
        if !self.target_live(target) {
            return Err(DeriveError::TargetDead);
        }
        let parent_rights = self.rights_of(parent).ok_or(DeriveError::NoSuchParent)?;
        if !parent_rights.contains(Right::Derive) {
            return Err(DeriveError::MissingDerive);
        }
        if !rights.subset_of(parent_rights) {
            return Err(DeriveError::RightsEscalation);
        }
        if !rights.well_typed(target.kind()) {
            return Err(DeriveError::WrongRightType);
        }
        if !self.audit.can_append(1) {
            return Err(DeriveError::AuditFull);
        }
        let handle = self.slots.alloc().ok_or(DeriveError::CapacityExceeded)?;
        self.records
            .insert(handle, Some(parent), caller, rights, target)?;
        self.positions
            .register(handle, caller)
            .map_err(|_| DeriveError::CapacityExceeded)?;
        self.audit.append(AuditEvent::CapabilityDerived {
            cap: handle,
            parent,
            holder: caller,
        });
        Ok(handle)
    }

    /// 撤销以 `cap` 为根的派生子树（内部模型能力；§4.7 第一版不提供用户可调 `cap_revoke`）。
    pub fn revoke_capability(
        &mut self,
        cap: Handle,
        holder: u32,
    ) -> Result<(), CapabilityPositionError> {
        match self.slots.check(cap) {
            SlotState::Free => return Err(CapabilityPositionError::NoSuchCapability),
            SlotState::Stale => return Err(CapabilityPositionError::StaleHandle),
            SlotState::Live => {}
        }
        if self.holder_of(cap) != Some(holder) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        let mut preview = self.records;
        let (_, revoked_count) = preview.revoke_subtree(cap);
        if !self.audit.can_append(revoked_count) {
            return Err(CapabilityPositionError::AuditFull);
        }
        let (revoked, revoked_count) = self.records.revoke_subtree(cap);
        for revoked_cap in revoked[..revoked_count].iter().copied() {
            self.retire_capability(revoked_cap);
            self.audit
                .append(AuditEvent::CapabilityRevoked { cap: revoked_cap });
        }
        Ok(())
    }

    /// §4.6：位置迁移（同一槽、同一 generation，句柄不变）。无用户可调 move 原语。
    pub fn move_capability(
        &mut self,
        cap: Handle,
        from_task: u32,
        to_task: u32,
    ) -> Result<(), CapabilityPositionError> {
        match self.slots.check(cap) {
            SlotState::Free => return Err(CapabilityPositionError::NoSuchCapability),
            SlotState::Stale => return Err(CapabilityPositionError::StaleHandle),
            SlotState::Live => {}
        }
        if !self.audit.can_append(1) {
            return Err(CapabilityPositionError::AuditFull);
        }
        // 审计记录**实际**的源位置（内核持有的 cap 移交时 `from` 是 `HeldByKernel`）。
        let from = self
            .positions
            .find(cap)
            .map(|position| position.location)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        self.positions.move_to_task(cap, from_task, to_task)?;
        self.audit.append(AuditEvent::CapabilityMoved {
            cap,
            from,
            to: CapabilityLocation::HeldByTask(to_task),
        });
        Ok(())
    }

    /// §8.2 步骤 7：`task_destroy` 撤销该 Task 持有的全部 Capability（释放槽位、gen 递增）。
    pub fn revoke_task_capabilities(
        &mut self,
        task_id: u32,
    ) -> Result<usize, CapabilityPositionError> {
        if task_id == INIT_TASK_ID {
            return Err(CapabilityPositionError::ProtectedTask);
        }
        let mut handles = [Handle::ZERO; MAX_CAPABILITY_POSITIONS];
        let mut count = 0;
        for position in self.positions.entries.iter().flatten() {
            if position.location == CapabilityLocation::HeldByTask(task_id)
                && count < MAX_CAPABILITY_POSITIONS
            {
                handles[count] = position.handle;
                count += 1;
            }
        }
        let mut revoked_roots = 0;
        for cap in handles[..count].iter().copied() {
            if self.positions.find(cap).is_some() {
                self.revoke_capability(cap, task_id)?;
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
        // 先清结构性根，否则它还挂在 positions 上会被 `revoke_capability` 漏掉。
        if let Some(task) = self.tasks.find_mut(task_id) {
            task.root_ns_cap = Handle::ZERO;
        }
        self.revoke_task_capabilities(task_id)
            .map_err(|_| TaskError::NoSuchTask)?;
        let mut dropped_caps = [Handle::ZERO; MAX_MESSAGES * MAX_MESSAGE_CAPABILITIES];
        let mut dropped_cap_count = 0;
        for message in self.ipc.entries[..self.ipc.count].iter().flatten() {
            if message.sender == task_id || message.receiver == task_id {
                for cap in message.capabilities[..message.capability_count]
                    .iter()
                    .copied()
                {
                    if dropped_cap_count < dropped_caps.len() {
                        dropped_caps[dropped_cap_count] = cap;
                        dropped_cap_count += 1;
                    }
                }
            }
        }
        for cap in dropped_caps[..dropped_cap_count].iter().copied() {
            if !cap.is_zero() {
                self.retire_capability(cap);
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
        capability_ids: &[Handle],
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
        let mut capabilities = [Handle::ZERO; MAX_MESSAGE_CAPABILITIES];
        for (index, cap) in capability_ids.iter().copied().enumerate() {
            if capabilities[..index].contains(&cap) {
                return Err(IpcError::InvalidCapability);
            }
            match self.slots.check(cap) {
                SlotState::Live => {}
                SlotState::Free | SlotState::Stale => return Err(IpcError::InvalidCapability),
            }
            if self.holder_of(cap) != Some(sender) {
                return Err(IpcError::CapabilityHolderMismatch);
            }
            capabilities[index] = cap;
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
            for cap in capabilities[..capability_ids.len()].iter().copied() {
                self.positions
                    .move_to_task(cap, sender, receiver)
                    .map_err(|_| IpcError::CapabilityHolderMismatch)?;
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
            for cap in capabilities[..capability_ids.len()].iter().copied() {
                self.positions
                    .move_to_message(cap, sender, message.id)
                    .map_err(|_| IpcError::CapabilityHolderMismatch)?;
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
                for cap in message.capabilities[..message.capability_count]
                    .iter()
                    .copied()
                {
                    self.positions
                        .move_from_message(cap, message.id, receiver)
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
        self.positions
            .entries
            .iter()
            .flatten()
            .filter(|position| position.location == CapabilityLocation::HeldByTask(task_id))
            .any(|position| {
                self.rights_of(position.handle)
                    .is_some_and(|rights| rights.contains(Right::TaskSend))
            })
    }
}

impl BootstrapState {
    fn segment(&self, id: u32) -> Option<&SegmentObject> {
        self.segments
            .iter()
            .flatten()
            .find(|segment| segment.id == id && segment.active)
    }

    fn segment_slot_free(&self, id: u32) -> bool {
        !self
            .segments
            .iter()
            .flatten()
            .any(|segment| segment.id == id)
    }

    /// 选一个空闲 Segment 槽（id 1 = image，2..14 = 平台 bring-up 区域，用户从 15 起）。
    fn alloc_segment_slot(&self) -> Option<u32> {
        let mut id = USER_SEGMENT_ID_FIRST;
        while id < MAX_SEGMENT_OBJECTS as u32 {
            if self.segment_slot_free(id) {
                return Some(id);
            }
            id += 1;
        }
        None
    }

    /// 取帧 → 逐帧清零 → 建 `SegmentObject` → 铸根 Capability。
    ///
    /// 用户路径（`memseg_create`）与平台 bring-up 路径（`platform_create_segment`）的公共内核。
    /// `cap_slot = Some` 表示在预留槽位铸**内核持有**的根 Capability；`None` 表示按最低空闲槽
    /// 铸给 `creator`。失败保持原子：已取帧全部归还、归属标签关闭。
    #[allow(clippy::too_many_arguments)]
    fn create_segment_object(
        &mut self,
        id: u32,
        creator: u32,
        page_count: usize,
        allowed_access: Rights,
        phys_offset: u64,
        cap_slot: Option<u32>,
    ) -> Result<(Handle, SegmentFrames), MemsegError> {
        if !self.segment_slot_free(id) || self.segment_count == MAX_SEGMENT_OBJECTS {
            return Err(MemsegError::CapacityExceeded);
        }
        if !self.audit.can_append(1) {
            return Err(MemsegError::AuditFull);
        }
        if crate::frames::open_segment(id, crate::frames::Pool::Cpu).is_err() {
            return Err(MemsegError::NoMemory);
        }
        let mut frames = [0u64; MAX_SEGMENT_FRAMES];
        let mut count = 0;
        while count < page_count {
            let frame = match crate::frames::allocate_for_segment(id) {
                Ok(frame) => frame,
                Err(_) => {
                    // 回滚已取帧，保持失败原子。
                    let mut rollback = 0;
                    while rollback < count {
                        let _ = crate::frames::release_frame(id, frames[rollback]);
                        rollback += 1;
                    }
                    let _ = crate::frames::close_segment(id);
                    return Err(MemsegError::NoMemory);
                }
            };
            // S2-X3：帧在首次可见前清零（归还帧残留的旧字节不得泄漏）。
            crate::mem::zero_frame(frame, phys_offset);
            frames[count] = frame;
            count += 1;
        }
        let root_rights = SEGMENT_ROOT_RIGHTS
            .intersection(allowed_access)
            .union(Rights::from_rights(&[Right::Derive, Right::SegDestroy]));
        let minted = match cap_slot {
            Some(slot) => self.mint_root_capability_at(
                CapabilityLocation::HeldByKernel,
                slot,
                root_rights,
                ObjectRef::Segment(id),
            ),
            None => self.mint_root_capability(creator, root_rights, ObjectRef::Segment(id)),
        };
        let handle = match minted {
            Ok(handle) => handle,
            Err(_) => {
                let mut rollback = 0;
                while rollback < count {
                    let _ = crate::frames::release_frame(id, frames[rollback]);
                    rollback += 1;
                }
                let _ = crate::frames::close_segment(id);
                return Err(MemsegError::CapacityExceeded);
            }
        };
        let segment_frames = SegmentFrames::Vector { frames, count };
        self.segments[self.segment_count] = Some(SegmentObject {
            id,
            generation: INITIAL_CAP_GENERATION,
            creator,
            pool: crate::frames::Pool::Cpu,
            frames: segment_frames,
            allowed_access,
            active: true,
        });
        self.segment_count += 1;
        self.audit.append(AuditEvent::ObjectCreated {
            object_id: id,
            segment: true,
        });
        Ok((handle, segment_frames))
    }

    /// §7.1 `memseg_create`：用户可调——校验（含空/越界权限、目标访问集）后取槽并建对象。
    pub fn memseg_create(
        &mut self,
        caller: u32,
        page_count: usize,
        allowed_access: Rights,
        phys_offset: u64,
    ) -> Result<Handle, MemsegError> {
        if page_count == 0 {
            return Err(MemsegError::ZeroSize);
        }
        if page_count > MAX_SEGMENT_FRAMES {
            return Err(MemsegError::CapacityExceeded);
        }
        if allowed_access.is_empty() || !allowed_access.well_typed(ObjectKind::Segment) {
            return Err(MemsegError::InvalidAccess);
        }
        if allowed_access
            .intersection(SEGMENT_ACCESS_RIGHTS)
            .is_empty()
        {
            return Err(MemsegError::InvalidAccess);
        }
        if !crate::frames::free_frames().is_ok_and(|free| free >= page_count) {
            return Err(MemsegError::NoMemory);
        }
        if !self.audit.can_append(2) {
            return Err(MemsegError::AuditFull);
        }
        let slot = self
            .alloc_segment_slot()
            .ok_or(MemsegError::CapacityExceeded)?;
        self.create_segment_object(slot, caller, page_count, allowed_access, phys_offset, None)
            .map(|(handle, _)| handle)
    }

    /// 切片 R：把一个平台 bring-up 区域建为真实 Segment 对象并铸内核持有的根 Capability。
    ///
    /// 与 `memseg_create` 的差别：id 固定为 `frames::SEG_*`（2..14）、允许**空** `allowed_access`
    /// （VSPACE/COW 是内核私有页，不授予任何 SegMap 权限）、根 cap 铸在预留槽位。
    pub fn platform_create_segment(
        &mut self,
        id: u32,
        page_count: usize,
        allowed_access: Rights,
        phys_offset: u64,
    ) -> Result<(Handle, SegmentFrames), MemsegError> {
        if !(PLATFORM_SEGMENT_ID_FIRST..=PLATFORM_SEGMENT_ID_LAST).contains(&id) {
            return Err(MemsegError::CapacityExceeded);
        }
        if page_count == 0 {
            return Err(MemsegError::ZeroSize);
        }
        if page_count > MAX_SEGMENT_FRAMES {
            return Err(MemsegError::CapacityExceeded);
        }
        if !allowed_access.is_empty()
            && (!allowed_access.well_typed(ObjectKind::Segment)
                || !allowed_access.subset_of(SEGMENT_ACCESS_RIGHTS))
        {
            return Err(MemsegError::InvalidAccess);
        }
        if !crate::frames::free_frames().is_ok_and(|free| free >= page_count) {
            return Err(MemsegError::NoMemory);
        }
        self.create_segment_object(
            id,
            KERNEL_ACTOR,
            page_count,
            allowed_access,
            phys_offset,
            Some(platform_segment_cap_slot(id)),
        )
    }

    /// 平台 Segment 的第 `index` 帧（用于内核直映射与内容初始化）。
    pub fn platform_segment_frame(&self, id: u32, index: usize) -> Result<u64, MemsegError> {
        self.segment(id)
            .and_then(|segment| segment.frames.frame_at(index))
            .ok_or(MemsegError::TargetDead)
    }

    /// 为一个**内核私有**平台 Segment 追加一帧（COW 段的 fault-time 私有页）。
    ///
    /// 帧仍经唯一取帧入口 `frames::allocate_for_segment(id)` 取得，故 W6 归属与守恒不变；
    /// 追加失败时释放刚取的帧，不留无归属帧。
    pub fn platform_append_frame(&mut self, id: u32) -> Result<u64, MemsegError> {
        let Some(index) = self
            .segments
            .iter()
            .position(|entry| entry.is_some_and(|segment| segment.id == id && segment.active))
        else {
            return Err(MemsegError::TargetDead);
        };
        if self.segments[index].is_some_and(|segment| {
            matches!(segment.frames, SegmentFrames::Contiguous { .. })
                || segment.frame_count() >= MAX_SEGMENT_FRAMES
        }) {
            return Err(MemsegError::CapacityExceeded);
        }
        let frame = crate::frames::allocate_for_segment(id).map_err(|_| MemsegError::NoMemory)?;
        let Some(segment) = self.segments[index].as_mut() else {
            let _ = crate::frames::release_frame(id, frame);
            return Err(MemsegError::TargetDead);
        };
        match &mut segment.frames {
            SegmentFrames::Vector { frames, count } => {
                frames[*count] = frame;
                *count += 1;
            }
            SegmentFrames::Contiguous { .. } => {
                let _ = crate::frames::release_frame(id, frame);
                return Err(MemsegError::CapacityExceeded);
            }
        }
        Ok(frame)
    }

    /// 切片 R：为 task 1 / task 2 铸平台 Task Capability（内核持有，含 `TaskManageVSpace`）。
    ///
    /// 必须在 `platform_init`（已 `create_task(2)`）之后调用，否则 task 2 目标不存活。
    pub fn platform_prepare_task_caps(&mut self) -> Result<(), MemsegError> {
        for (slot, task_id) in [
            (PLATFORM_TASK_CAP_SLOT_BASE, INIT_TASK_ID),
            (PLATFORM_TASK_CAP_SLOT_BASE + 1, 2),
        ] {
            self.mint_root_capability_at(
                CapabilityLocation::HeldByKernel,
                slot,
                PLATFORM_TASK_RIGHTS,
                ObjectRef::Task(task_id),
            )
            .map_err(|_| MemsegError::CapacityExceeded)?;
        }
        Ok(())
    }

    /// 一条平台根 Capability 是否满足全部结构性条件。
    fn platform_cap_ok(&self, handle: Handle, target: ObjectRef) -> bool {
        if self.slots.check(handle) != SlotState::Live || self.target_of(handle) != Some(target) {
            return false;
        }
        if !matches!(
            self.positions
                .find(handle)
                .map(|position| position.location),
            Some(CapabilityLocation::HeldByKernel)
        ) {
            return false;
        }
        self.rights_of(handle)
            .is_some_and(|rights| !rights.is_empty() && rights.well_typed(target.kind()))
    }

    /// 切片 R：组装平台段对象化的阶段末见证（按需计算）。
    pub fn platform_segments_record(&self) -> PlatformSegmentsRecord {
        let platform_range = PLATFORM_SEGMENT_ID_FIRST..=PLATFORM_SEGMENT_ID_LAST;
        let mut record = PlatformSegmentsRecord {
            segments: 0,
            first_id: PLATFORM_SEGMENT_ID_FIRST,
            last_id: PLATFORM_SEGMENT_ID_LAST,
            roots: 0,
            task_caps: 0,
            mapped: 0,
            direct: 0,
            vspace: 0,
            unified: true,
            cap_handles: [Handle::ZERO; PLATFORM_SEGMENT_COUNT],
            task_cap_handles: [Handle::ZERO; PLATFORM_TASK_CAP_COUNT],
            frame_counts: [0; PLATFORM_SEGMENT_COUNT],
            direct_ids: [crate::frames::SEG_VSPACE, crate::frames::SEG_COW],
        };
        for id in platform_range.clone() {
            let index = (id - PLATFORM_SEGMENT_ID_FIRST) as usize;
            let handle = platform_segment_cap_handle(id);
            record.cap_handles[index] = handle;
            if self.platform_cap_ok(handle, ObjectRef::Segment(id)) {
                record.roots += 1;
            }
            let Some(segment) = self.segment(id) else {
                continue;
            };
            record.segments += 1;
            record.frame_counts[index] = segment.frame_count() as u32;
            // 帧归属标签必须与 Segment id 恒等（迁移前是独立的 100+id 命名空间）。
            for frame_index in 0..segment.frame_count() {
                let owned = segment.frames.frame_at(frame_index).is_some_and(|frame| {
                    crate::frames::owner_of(frame)
                        .is_ok_and(|owner| owner == crate::frames::FrameOwner::Segment(id))
                });
                if !owned {
                    record.unified = false;
                }
            }
        }
        record.vspace = self.vspace.entries[..self.vspace.count]
            .iter()
            .flatten()
            .filter(|entry| platform_range.contains(&entry.seg_id))
            .count() as u32;
        for id in platform_range {
            // 只统计真实存在的对象，与 `segments` 的口径一致。
            if self.segment(id).is_none() {
                continue;
            }
            if platform_segment_is_direct(id) {
                record.direct += 1;
            } else if self.vspace.entries[..self.vspace.count]
                .iter()
                .flatten()
                .any(|entry| entry.seg_id == id)
            {
                record.mapped += 1;
            }
        }
        for (offset, task_id) in [(0u32, INIT_TASK_ID), (1, 2)] {
            let handle = cap_handle(PLATFORM_TASK_CAP_SLOT_BASE + offset, INITIAL_CAP_GENERATION);
            record.task_cap_handles[offset as usize] = handle;
            if self.platform_cap_ok(handle, ObjectRef::Task(task_id)) {
                record.task_caps += 1;
            }
        }
        record
    }

    /// §7.2 `memseg_map`：把 Segment 的页真实映射进目标 Task 的 vspace。
    #[allow(clippy::too_many_arguments)]
    pub fn memseg_map(
        &mut self,
        caller: u32,
        seg_cap: Handle,
        task_cap: Handle,
        seg_offset: u64,
        target_base: u64,
        page_count: usize,
        access: Rights,
        root_phys: u64,
        phys_offset: u64,
    ) -> Result<(), MemsegError> {
        if page_count == 0 {
            return Err(MemsegError::ZeroSize);
        }
        if access.is_empty() || !access.well_typed(ObjectKind::Segment) {
            return Err(MemsegError::InvalidAccess);
        }
        let seg_id = match self.authorize(caller, seg_cap, Right::Derive) {
            Ok(_) => match self.target_of(seg_cap) {
                Some(ObjectRef::Segment(id)) => id,
                _ => return Err(MemsegError::NoSuchCapability),
            },
            Err(error) => return Err(map_auth_error(error)),
        };
        let segment_frames = match self.segment(seg_id) {
            Some(segment) => segment.frames,
            None => return Err(MemsegError::TargetDead),
        };
        let segment_allowed = match self.segment(seg_id) {
            Some(segment) => segment.allowed_access,
            None => return Err(MemsegError::TargetDead),
        };
        if !access.subset_of(segment_allowed) {
            return Err(MemsegError::RightsExceeded);
        }
        // 每项请求访问都需要对应的 SegMap 授权。
        for right in [Right::SegMapR, Right::SegMapW, Right::SegMapX] {
            if access.contains(right) && !self.rights_of(seg_cap).is_some_and(|r| r.contains(right))
            {
                return Err(MemsegError::MissingMapRight);
            }
        }
        let task_id = match self.authorize(caller, task_cap, Right::TaskManageVSpace) {
            Ok(_) => match self.target_of(task_cap) {
                Some(ObjectRef::Task(id)) => id,
                _ => return Err(MemsegError::NoSuchCapability),
            },
            Err(error) => return Err(map_auth_error(error)),
        };
        let frame_count = segment_frames.frame_count();
        let end = seg_offset
            .checked_add(page_count as u64)
            .ok_or(MemsegError::OutOfBounds)?;
        if end > frame_count as u64 {
            return Err(MemsegError::OutOfBounds);
        }
        if !target_base.is_multiple_of(crate::mem::FRAME_SIZE) {
            return Err(MemsegError::OutOfBounds);
        }
        // 重叠：逻辑视图与目标根的真实 PTE 都要为空（W7）。
        let map = |phys: u64| crate::mem::phys_to_virt(phys, phys_offset);
        for index in 0..page_count {
            let va = target_base + (index as u64) * crate::mem::FRAME_SIZE;
            if self.vspace.find(task_id, va).is_some() {
                return Err(MemsegError::Overlap);
            }
            if crate::vspace::read_leaf(root_phys, va, map)
                .is_ok_and(|pte| pte & crate::vspace::PT_PRESENT != 0)
            {
                return Err(MemsegError::Overlap);
            }
        }
        if self.vspace.count + page_count > MAX_VSPACE_ENTRIES {
            return Err(MemsegError::CapacityExceeded);
        }
        let writable = access.contains(Right::SegMapW);
        for index in 0..page_count {
            let va = target_base + (index as u64) * crate::mem::FRAME_SIZE;
            let frame = segment_frames
                .frame_at(seg_offset as usize + index)
                .ok_or(MemsegError::OutOfBounds)?;
            if crate::vspace::map_4k(
                root_phys,
                va,
                map,
                || crate::frames::allocate_table_frame().ok(),
                frame,
                writable,
                true,
            )
            .is_err()
            {
                return Err(MemsegError::NoMemory);
            }
            unsafe { crate::vspace::invlpg(va) };
            self.vspace.insert(VspaceEntry {
                task_id,
                va,
                seg_id,
                seg_offset: seg_offset + index as u64,
                access,
            })?;
        }
        Ok(())
    }

    /// §7.3 `memseg_unmap`：清除区间内的真实 PTE 与逻辑视图，回收变空的表结点。
    pub fn memseg_unmap(
        &mut self,
        caller: u32,
        task_cap: Handle,
        target_base: u64,
        page_count: usize,
        root_phys: u64,
        phys_offset: u64,
    ) -> Result<(), MemsegError> {
        if page_count == 0 {
            return Err(MemsegError::ZeroSize);
        }
        let task_id = match self.authorize(caller, task_cap, Right::TaskManageVSpace) {
            Ok(_) => match self.target_of(task_cap) {
                Some(ObjectRef::Task(id)) => id,
                _ => return Err(MemsegError::NoSuchCapability),
            },
            Err(error) => return Err(map_auth_error(error)),
        };
        let map = |phys: u64| crate::mem::phys_to_virt(phys, phys_offset);
        for index in 0..page_count {
            let va = target_base + (index as u64) * crate::mem::FRAME_SIZE;
            if self.vspace.find(task_id, va).is_none() {
                return Err(MemsegError::NotMapped);
            }
            // 入口页不可卸载：任务入口在整个生命周期内必须可执行（§7.3）。
            if self.tasks.find(task_id).is_some_and(|task| {
                task.entry != 0 && va == task.entry & !(crate::mem::FRAME_SIZE - 1)
            }) {
                return Err(MemsegError::EntryMapping);
            }
        }
        for index in 0..page_count {
            let va = target_base + (index as u64) * crate::mem::FRAME_SIZE;
            crate::vspace::unmap_4k(root_phys, va, map, |frame| {
                crate::frames::release_table_frame(frame)
                    .map_err(|_| crate::vspace::VspaceError::NoFrame)
            })
            .map_err(|_| MemsegError::NotMapped)?;
            unsafe { crate::vspace::invlpg(va) };
            let _ = self.vspace.remove(task_id, va);
        }
        Ok(())
    }

    /// §7.4 `memseg_destroy`：无活动映射时删除 Segment 并把全部帧归还来源池。
    pub fn memseg_destroy(&mut self, caller: u32, seg_cap: Handle) -> Result<(), MemsegError> {
        let seg_id = match self.authorize(caller, seg_cap, Right::SegDestroy) {
            Ok(_) => match self.target_of(seg_cap) {
                Some(ObjectRef::Segment(id)) => id,
                _ => return Err(MemsegError::NoSuchCapability),
            },
            Err(error) => return Err(map_auth_error(error)),
        };
        if self.vspace.references_segment(seg_id) {
            return Err(MemsegError::StillMapped);
        }
        if !self.audit.can_append(1) {
            return Err(MemsegError::AuditFull);
        }
        let Some(index) = self
            .segments
            .iter()
            .position(|entry| entry.is_some_and(|segment| segment.id == seg_id && segment.active))
        else {
            return Err(MemsegError::TargetDead);
        };
        let segment = self.segments[index].ok_or(MemsegError::TargetDead)?;
        let count = segment.frame_count();
        for page in 0..count {
            if let Some(frame) = segment.frames.frame_at(page) {
                let _ = crate::frames::release_frame(seg_id, frame);
            }
        }
        let _ = crate::frames::close_segment(seg_id);
        self.segments[index] = Some(SegmentObject {
            active: false,
            generation: segment.generation.wrapping_add(1).max(1),
            ..segment
        });
        self.audit
            .append(AuditEvent::ObjectDestroyed { object_id: seg_id });
        Ok(())
    }
}

/// `MAP`/`UNMAP`/`DESTROY` 的授权错误到 `memseg` 错误的映射。
fn map_auth_error(error: AuthError) -> MemsegError {
    match error {
        AuthError::MalformedHandle => MemsegError::MalformedHandle,
        AuthError::NoSuchCapability => MemsegError::NoSuchCapability,
        AuthError::StaleHandle => MemsegError::StaleHandle,
        AuthError::NotInstalled => MemsegError::NotInstalled,
        AuthError::NotHolder => MemsegError::NotHolder,
        AuthError::MissingRight => MemsegError::MissingRight,
        AuthError::TargetDead => MemsegError::TargetDead,
    }
}

impl fmt::Display for BootstrapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyImage => formatter.write_str("empty image"),
            Self::ImageRangeOverflow => formatter.write_str("image range overflow"),
            Self::InvalidFrameOwnership => formatter.write_str("invalid frame ownership"),
            Self::InvalidAuthority => formatter.write_str("invalid authority"),
            Self::EntryOutsideImage => formatter.write_str("entry outside image range"),
            Self::AlreadyBooted => formatter.write_str("already booted"),
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
        image_cap: IMAGE_CAP_HANDLE,
        namespace_cap: NAMESPACE_CAP_HANDLE,
        task_cap: TASK_CAP_HANDLE,
        image_base,
        image_len,
        image_frames: image_frames as u64,
        free_frames: free_frames as u64,
        image_rights: IMAGE_RIGHTS,
        namespace_rights: NAMESPACE_RIGHTS,
        task_rights: TASK_RIGHTS,
    })
}

/// §14 前置 3：入口必须落在 image 范围内。
pub const fn entry_in_image(entry: u64, image_base: u64, image_len: u64) -> bool {
    entry >= image_base && entry < image_base.saturating_add(image_len)
}

pub fn create_state(
    image_base: u64,
    image_len: u64,
    entry: u64,
    inventory: &FrameInventory,
) -> Result<BootstrapState, BootstrapError> {
    if is_installed() {
        return Err(BootstrapError::AlreadyBooted);
    }
    if inventory.kernel_phys_start >= inventory.kernel_phys_end
        || inventory.image_frames == 0
        || inventory.free_frames == 0
        || inventory.free_frames != inventory.usable_frames
    {
        return Err(BootstrapError::InvalidFrameOwnership);
    }
    if !entry_in_image(entry, image_base, image_len) {
        return Err(BootstrapError::EntryOutsideImage);
    }
    let snapshot = create_snapshot(
        image_base,
        image_len,
        inventory.image_frames,
        inventory.free_frames,
    )?;
    let capabilities = [
        InitialCapability {
            handle: IMAGE_CAP_HANDLE,
            object: InitialObject::ImageSegment,
            holder: INIT_TASK_ID,
            rights: IMAGE_RIGHTS,
        },
        InitialCapability {
            handle: NAMESPACE_CAP_HANDLE,
            object: InitialObject::Namespace,
            holder: INIT_TASK_ID,
            rights: NAMESPACE_RIGHTS,
        },
        InitialCapability {
            handle: TASK_CAP_HANDLE,
            object: InitialObject::InitTask,
            holder: INIT_TASK_ID,
            rights: TASK_RIGHTS,
        },
    ];
    let positions = CapabilityPositionTable::from_capabilities(&capabilities);

    // §14 步骤 2：以 image_frames 建只允许 {R,X} 的 image Segment（pool=CPU）。
    let mut segments = [None; MAX_SEGMENT_OBJECTS];
    segments[0] = Some(SegmentObject {
        id: IMAGE_SEGMENT_ID,
        generation: INITIAL_CAP_GENERATION,
        creator: INIT_TASK_ID,
        pool: crate::frames::Pool::Cpu,
        frames: SegmentFrames::Contiguous {
            base: inventory.kernel_phys_start,
            count: inventory.image_frames,
        },
        allowed_access: Rights::from_rights(&[Right::SegMapR, Right::SegMapX]),
        active: true,
    });
    // §14 步骤 3：建空 init Namespace。
    let mut namespaces = [None; MAX_NAMESPACE_OBJECTS];
    namespaces[0] = Some(NamespaceObject {
        id: INIT_NAMESPACE_ID,
        creator: INIT_TASK_ID,
        entries: 0,
    });

    let mut slots = CapabilitySlotTable::empty();
    for slot in [IMAGE_CAP_SLOT, NAMESPACE_CAP_SLOT, TASK_CAP_SLOT] {
        slots.reserve(slot);
    }

    let mut state = BootstrapState {
        snapshot,
        slots,
        capabilities,
        positions,
        records: CapabilityRecordTable::empty(),
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
        segments,
        segment_count: 1,
        namespaces,
        namespace_count: 1,
        vspace: TaskVspaceTable::empty(),
        // §14 步骤 9 / W14：bootstrap 成功即置位。
        booted: true,
        boot_record: Some(BootstrapRecord {
            caps: INITIAL_CAPABILITY_COUNT,
            segments: 1,
            namespaces: 1,
            booted: true,
            initial_status_ready: true,
            entry_in_image: true,
            root_ns_cap: NAMESPACE_CAP_HANDLE,
            init_running: false,
        }),
    };
    if let Some(task) = state.tasks.find_mut(INIT_TASK_ID) {
        task.entry = entry;
    }
    // §14 步骤 9 / §13.1：写第一组 Audit 事件。
    state.audit.append(AuditEvent::Bootstrapped);
    state.audit.append(AuditEvent::ObjectCreated {
        object_id: IMAGE_SEGMENT_ID,
        segment: true,
    });
    state.audit.append(AuditEvent::ObjectCreated {
        object_id: INIT_NAMESPACE_ID,
        segment: false,
    });
    state.audit.append(AuditEvent::TaskStateChanged {
        task_id: INIT_TASK_ID,
        status: TaskStatus::Ready,
    });
    state
        .validate_authority()
        .map_err(|_| BootstrapError::InvalidAuthority)?;
    Ok(state)
}

/// 是否已安装 Bootstrap 状态（W14 幂等判定）。
pub fn is_installed() -> bool {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        (*slot).is_some()
    }
}

/// 切片 P：读取初始 Authority 见证记录（未安装或无记录时报错）。
pub fn authority_record() -> Result<BootstrapRecord, BootstrapError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref().and_then(|state| state.boot_record) {
            Some(record) => Ok(record),
            None => Err(BootstrapError::InvalidAuthority),
        }
    }
}

/// §14 步骤 8 + W13：首次调度 init——`Ready → Running` 并追加 `TaskStateChanged`。
pub fn activate_init() -> Result<(), BootstrapError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.activate_init(),
            None => Err(BootstrapError::InvalidAuthority),
        }
    }
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

/// 查询全局授权表：`caller` 是否持有 `cap` 且具备 `right`。
pub fn authorize(caller: u32, cap: Handle, right: Right) -> Result<Handle, AuthError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.authorize(caller, cap, right),
            None => Err(AuthError::NotInstalled),
        }
    }
}

/// 解码 syscall 传入的裸句柄（未安装时也报 `NotInstalled`）。
pub fn decode_handle(raw: u64) -> Result<Handle, AuthError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.decode_handle(raw),
            None => Err(AuthError::NotInstalled),
        }
    }
}

/// S2-D2：全局 `cap_derive`（§4.4）。
pub fn derive_capability(
    caller: u32,
    parent: Handle,
    rights: Rights,
) -> Result<Handle, DeriveError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.derive_capability(caller, parent, rights),
            None => Err(DeriveError::CapacityExceeded),
        }
    }
}

/// S2-D3：全局 `memseg_create`（§7.1）。
pub fn memseg_create(
    caller: u32,
    page_count: usize,
    allowed_access: Rights,
    phys_offset: u64,
) -> Result<Handle, MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.memseg_create(caller, page_count, allowed_access, phys_offset),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// S2-D3：全局 `memseg_map`（§7.2）。
#[allow(clippy::too_many_arguments)]
pub fn memseg_map(
    caller: u32,
    seg_cap: Handle,
    task_cap: Handle,
    seg_offset: u64,
    target_base: u64,
    page_count: usize,
    access: Rights,
    root_phys: u64,
    phys_offset: u64,
) -> Result<(), MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.memseg_map(
                caller,
                seg_cap,
                task_cap,
                seg_offset,
                target_base,
                page_count,
                access,
                root_phys,
                phys_offset,
            ),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// S2-D3：全局 `memseg_unmap`（§7.3）。
pub fn memseg_unmap(
    caller: u32,
    task_cap: Handle,
    target_base: u64,
    page_count: usize,
    root_phys: u64,
    phys_offset: u64,
) -> Result<(), MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.memseg_unmap(
                caller,
                task_cap,
                target_base,
                page_count,
                root_phys,
                phys_offset,
            ),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// S2-D3：全局 `memseg_destroy`（§7.4）。
pub fn memseg_destroy(caller: u32, seg_cap: Handle) -> Result<(), MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.memseg_destroy(caller, seg_cap),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// S2-D2：全局 `cap_drop`（§4.5）。
pub fn drop_capability(caller: u32, cap: Handle) -> Result<(), DropError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.drop_capability(caller, cap),
            None => Err(DropError::NotInstalled),
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

/// 切片 R：全局平台 Segment 创建（真实 Segment 对象 + 内核持有的可授权根 Capability）。
pub fn platform_create_segment(
    id: u32,
    page_count: usize,
    allowed_access: Rights,
    phys_offset: u64,
) -> Result<(Handle, SegmentFrames), MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => {
                state.platform_create_segment(id, page_count, allowed_access, phys_offset)
            }
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// 切片 R：平台 Segment 的第 `index` 帧。
pub fn platform_segment_frame(id: u32, index: usize) -> Result<u64, MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.platform_segment_frame(id, index),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// 切片 R：为内核私有平台 Segment 追加一帧（COW 段 fault-time 私有页）。
pub fn platform_append_frame(id: u32) -> Result<u64, MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.platform_append_frame(id),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// 切片 R：铸 task 1 / task 2 的平台 Task Capability（须在 `platform_init` 之后）。
pub fn platform_prepare_task_caps() -> Result<(), MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.platform_prepare_task_caps(),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// 切片 R：读取平台段对象化见证记录（供 `platform_segments_ready` 事件）。
pub fn platform_segments_record() -> Result<PlatformSegmentsRecord, BootstrapError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => Ok(state.platform_segments_record()),
            None => Err(BootstrapError::InvalidAuthority),
        }
    }
}

/// 切片 L：全局订阅查询（供 syscall 层使用）。
pub fn exc_subscribe(caller: u32, cap: Handle, kind: ExcKind) -> Result<u32, ExcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.exc_subscribe(caller, cap, kind),
            None => Err(ExcError::NotInstalled),
        }
    }
}

/// 切片 L：全局取消订阅（供 syscall 层使用）。
pub fn exc_unsubscribe(caller: u32, cap: Handle) -> Result<u32, ExcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.exc_unsubscribe(caller, cap),
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

/// 切片 N：查询 `caller` 是否已订阅 `cap` 所指源的 `kind`。
pub fn exc_subscribed(caller: u32, cap: Handle, kind: ExcKind) -> Result<u32, ExcError> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.exc_subscribed(caller, cap, kind),
            None => Err(ExcError::NotInstalled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AuditEvent, AuthError, AuthorityError, BootstrapError, CapabilityLocation,
        CapabilityPositionError, DeriveError, DropError, EXC_PAGE_FAULT_HANDLE, EXC_SOURCE_RIGHTS,
        EXC_SYSCALL_TRAP_HANDLE, ExcError, ExcKind, Handle, IMAGE_CAP_HANDLE, IMAGE_CAP_SLOT,
        IMAGE_RIGHTS, IMAGE_SEGMENT_ID, INIT_TASK_ID, INITIAL_CAP_GENERATION,
        INITIAL_CAPABILITY_COUNT, InitialObject, IpcError, KERNEL_ACTOR, MAX_CAPABILITY_SLOTS,
        MAX_SEGMENT_FRAMES, MemsegError, NAMESPACE_CAP_HANDLE, NAMESPACE_RIGHTS, ObjectRef,
        PAGE_FAULT_SOURCE_ID, PLATFORM_SEGMENT_CAP_SLOT_BASE, PLATFORM_SEGMENT_COUNT,
        PLATFORM_SEGMENT_ID_FIRST, PLATFORM_SEGMENT_ID_LAST, PLATFORM_TASK_RIGHTS,
        PLATFORM_TASK1_CAP_HANDLE, PLATFORM_TASK2_CAP_HANDLE, SEGMENT_ROOT_RIGHTS,
        SYSCALL_TRAP_SOURCE_ID, SegmentFrames, SegmentObject, TASK_CAP_HANDLE, TASK_RIGHTS,
        TaskError, TaskStatus, USER_SEGMENT_ID_FIRST, VspaceEntry, create_snapshot, create_state,
        entry_in_image, platform_segment_cap_handle, platform_segment_cap_slot,
        platform_segment_is_direct,
    };
    use crate::mem::FrameInventory;
    use crate::rights::{ObjectKind, Right, Rights};

    /// 测试用权限解析（非法串直接 panic，暴露出测试自身的错误）。
    fn rights(text: &str) -> Rights {
        Rights::parse(text).expect("test rights text parses")
    }

    fn inventory() -> FrameInventory {
        FrameInventory {
            usable_frames: 16,
            image_frames: 3,
            free_frames: 16,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_3000,
        }
    }

    fn fresh_state() -> super::BootstrapState {
        create_state(0x4000_0000, 0x3000, 0x4000_0100, &inventory()).unwrap()
    }

    #[test]
    fn initial_authority_rights_are_well_typed_and_finite() {
        // SPEC-061：三组初始权限必须与目标对象类型匹配（§4.1），且不含通配符。
        assert!(IMAGE_RIGHTS.well_typed(ObjectKind::Segment));
        assert!(NAMESPACE_RIGHTS.well_typed(ObjectKind::Namespace));
        assert!(TASK_RIGHTS.well_typed(ObjectKind::Task));
        assert!(!NAMESPACE_RIGHTS.well_typed(ObjectKind::Segment));
        assert!(!NAMESPACE_RIGHTS.well_typed(ObjectKind::Task));
        assert_eq!(NAMESPACE_RIGHTS.count(), 7);
        assert_eq!(IMAGE_RIGHTS.count(), 4);
        assert_eq!(TASK_RIGHTS.count(), 4);
        assert!(Rights::parse("all").is_err());
    }

    #[test]
    fn initial_capability_handles_are_slot_and_generation_encoded() {
        // S2-D1：严格编码——gen1 的句柄不是裸槽索引。
        assert_eq!(IMAGE_CAP_HANDLE.index(), IMAGE_CAP_SLOT);
        assert_eq!(IMAGE_CAP_HANDLE.generation(), 1);
        assert_eq!(IMAGE_CAP_HANDLE.encode(), 0x0000_0001_0000_0001);
        assert_eq!(NAMESPACE_CAP_HANDLE.encode(), 0x0000_0001_0000_0002);
        assert_eq!(TASK_CAP_HANDLE.encode(), 0x0000_0001_0000_0003);
        assert_eq!(EXC_PAGE_FAULT_HANDLE.encode(), 0x0000_0001_0000_0004);
        assert_eq!(EXC_SYSCALL_TRAP_HANDLE.encode(), 0x0000_0001_0000_0005);
    }

    #[test]
    fn entry_must_lie_inside_the_image_range() {
        assert!(entry_in_image(0x4000_0000, 0x4000_0000, 0x3000));
        assert!(entry_in_image(0x4000_2fff, 0x4000_0000, 0x3000));
        assert!(!entry_in_image(0x4000_3000, 0x4000_0000, 0x3000));
        assert!(!entry_in_image(0x3fff_ffff, 0x4000_0000, 0x3000));
        assert_eq!(
            create_state(0x4000_0000, 0x3000, 0x5000_0000, &inventory()),
            Err(BootstrapError::EntryOutsideImage)
        );
    }

    #[test]
    fn init_starts_ready_and_activation_appends_a_state_change() {
        let mut state = fresh_state();
        assert_eq!(
            state.tasks.find(INIT_TASK_ID).unwrap().status,
            TaskStatus::Ready
        );
        assert!(!state.boot_record.unwrap().init_running);
        let before = state.audit.count;
        state.activate_init().unwrap();
        assert_eq!(
            state.tasks.find(INIT_TASK_ID).unwrap().status,
            TaskStatus::Running
        );
        assert_eq!(state.audit.count, before + 1);
        assert!(state.boot_record.unwrap().init_running);
        assert_eq!(state.activate_init(), Err(BootstrapError::InvalidAuthority));
    }

    #[test]
    fn snapshot_contains_exact_initial_authority() {
        let snapshot = create_snapshot(0x4000_0000, 0x3000, 3, 16).unwrap();
        assert_eq!(snapshot.task_id, 1);
        assert_eq!(INITIAL_CAPABILITY_COUNT, 3);
        assert_eq!(snapshot.image_cap, IMAGE_CAP_HANDLE);
        assert_eq!(snapshot.namespace_cap, NAMESPACE_CAP_HANDLE);
        assert_eq!(snapshot.task_cap, TASK_CAP_HANDLE);
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
        let state = fresh_state();
        assert_eq!(state.capabilities[0].object, InitialObject::ImageSegment);
        assert_eq!(state.capabilities[1].object, InitialObject::Namespace);
        assert_eq!(state.capabilities[2].object, InitialObject::InitTask);
        assert!(state.positions.has_unique_active_locations());
        assert_eq!(state.slots.live_count(), INITIAL_CAPABILITY_COUNT as usize);
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
        let mut state = fresh_state();
        state.positions.entries[IMAGE_CAP_SLOT as usize]
            .as_mut()
            .unwrap()
            .location = CapabilityLocation::HeldByTask(99);
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::InvalidHolder)
        );

        let mut state = fresh_state();
        let child = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("SegMapR"))
            .unwrap();
        // 越权：把子权限改成非父权限子集（TaskSend 不在 image cap 权限内）。
        state.records.entries[child.index() as usize]
            .as_mut()
            .unwrap()
            .rights = Rights::from_rights(&[Right::SegMapR, Right::TaskSend]);
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::RightsEscalation)
        );
    }

    #[test]
    fn state_rejects_inconsistent_frame_inventory() {
        let mut inventory = inventory();
        inventory.free_frames = 15;
        assert_eq!(
            create_state(0x4000_0000, 0x3000, 0x4000_0100, &inventory),
            Err(BootstrapError::InvalidFrameOwnership)
        );
    }

    #[test]
    fn capability_move_and_drop_preserve_unique_location() {
        let mut state = fresh_state();
        state
            .move_capability(IMAGE_CAP_HANDLE, INIT_TASK_ID, 2)
            .unwrap();
        assert!(state.audit.entries[..state.audit.count]
            .iter()
            .flatten()
            .any(|event| matches!(event, AuditEvent::CapabilityMoved { cap, .. } if *cap == IMAGE_CAP_HANDLE)));
        assert_eq!(
            state
                .positions
                .move_to_task(IMAGE_CAP_HANDLE, INIT_TASK_ID, 3),
            Err(CapabilityPositionError::HolderMismatch)
        );
        state.positions.drop_from_task(IMAGE_CAP_HANDLE, 2).unwrap();
        assert!(state.positions.has_unique_active_locations());
        assert_eq!(state.positions.count, INITIAL_CAPABILITY_COUNT as usize - 1);
    }

    #[test]
    fn capability_derivation_enforces_permission_monotonicity() {
        let mut state = fresh_state();
        let child = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive,SegMapR"))
            .unwrap();
        assert_eq!(child.index(), 4);
        assert_eq!(child.generation(), 1);
        assert_eq!(
            state.records.find(child).unwrap().parent,
            Some(IMAGE_CAP_HANDLE)
        );
        assert_eq!(state.positions.count, INITIAL_CAPABILITY_COUNT as usize + 1);
        assert!(state.positions.has_unique_active_locations());
        assert_eq!(
            state.derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("SegMapW")),
            Err(DeriveError::RightsEscalation)
        );
        // TaskSend 不在父权限内（同时也不是 Segment 的合法权限）→ 先触发越权拒绝。
        assert_eq!(
            state.derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("TaskSend")),
            Err(DeriveError::RightsEscalation)
        );
        // 非持有者不能派生。
        assert_eq!(
            state.derive_capability(2, IMAGE_CAP_HANDLE, rights("Derive")),
            Err(DeriveError::ParentNotHeld)
        );
        state
            .positions
            .move_to_task(child, INIT_TASK_ID, 2)
            .unwrap();
        state.positions.drop_from_task(child, 2).unwrap();
        assert!(state.positions.has_unique_active_locations());
    }

    #[test]
    fn capability_slot_capacity_is_enforced() {
        let mut state = fresh_state();
        let mut granted = 0;
        loop {
            match state.derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive")) {
                Ok(_) => granted += 1,
                Err(error) => {
                    assert_eq!(error, DeriveError::CapacityExceeded);
                    break;
                }
            }
        }
        // 槽 0 保留不用：可分配槽 = MAX-1，扣除 3 条初始根。
        assert_eq!(
            granted,
            MAX_CAPABILITY_SLOTS - 1 - INITIAL_CAPABILITY_COUNT as usize
        );
        assert_eq!(state.slots.live_count(), MAX_CAPABILITY_SLOTS - 1);
    }

    #[test]
    fn stale_handle_is_rejected_after_slot_reuse() {
        // S2-X4：槽位释放后复用会递增 generation，旧句柄永远不再匹配。
        let mut state = fresh_state();
        let first = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive"))
            .unwrap();
        assert_eq!(state.drop_capability(INIT_TASK_ID, first), Ok(()));
        let second = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive"))
            .unwrap();
        assert_eq!(second.index(), first.index(), "最低空闲槽被复用");
        assert_eq!(second.generation(), first.generation() + 1);
        assert_ne!(second, first);
        assert_eq!(
            state.authorize(INIT_TASK_ID, first, Right::Derive),
            Err(AuthError::StaleHandle)
        );
        assert_eq!(
            state.authorize(INIT_TASK_ID, second, Right::Derive),
            Ok(second)
        );
        assert_eq!(state.validate_authority(), Ok(()));
    }

    #[test]
    fn drop_capability_protects_the_live_structural_root() {
        let mut state = fresh_state();
        assert_eq!(
            state.drop_capability(INIT_TASK_ID, NAMESPACE_CAP_HANDLE),
            Err(DropError::StructuralCap)
        );
        assert_eq!(
            state.authorize(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, Right::NsResolve),
            Ok(NAMESPACE_CAP_HANDLE)
        );
    }

    #[test]
    fn drop_capability_rejects_non_holder_and_unknown_handle() {
        let mut state = fresh_state();
        assert_eq!(
            state.drop_capability(2, IMAGE_CAP_HANDLE),
            Err(DropError::NotHolder)
        );
        let unallocated = Handle::new(60, 1).unwrap();
        assert_eq!(
            state.drop_capability(INIT_TASK_ID, unallocated),
            Err(DropError::NoSuchCapability)
        );
    }

    #[test]
    fn dead_target_makes_the_token_stale_for_authorization_but_droppable() {
        let mut state = fresh_state();
        state.create_task(2).unwrap();
        let target_cap = state
            .mint_root_capability(INIT_TASK_ID, rights("TaskSend"), ObjectRef::Task(2))
            .unwrap();
        assert_eq!(
            state.authorize(INIT_TASK_ID, target_cap, Right::TaskSend),
            Ok(target_cap)
        );
        state.destroy_task(2).unwrap();
        // 目标已销毁：不可授权，但仍是可 `cap_drop` 的 stale token（§4.3/§4.5）。
        assert_eq!(
            state.authorize(INIT_TASK_ID, target_cap, Right::TaskSend),
            Err(AuthError::TargetDead)
        );
        assert_eq!(state.drop_capability(INIT_TASK_ID, target_cap), Ok(()));
    }

    #[test]
    fn revoking_parent_invalidates_derived_subtree_atomically() {
        let mut state = fresh_state();
        let parent = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive,SegMapR"))
            .unwrap();
        state.create_task(2).unwrap();
        state.move_capability(parent, INIT_TASK_ID, 2).unwrap();
        let child = state
            .derive_capability(2, parent, rights("Derive"))
            .unwrap();
        assert_eq!(
            state.revoke_capability(parent, INIT_TASK_ID),
            Err(CapabilityPositionError::HolderMismatch)
        );
        assert!(state.positions.has_unique_active_locations());
        state.revoke_capability(parent, 2).unwrap();
        assert!(state.records.find(parent).is_none());
        assert!(state.records.find(child).is_none());
        for cap in [parent, child] {
            assert!(state.audit.entries[..state.audit.count]
                .iter()
                .flatten()
                .any(|event| matches!(event, AuditEvent::CapabilityRevoked { cap: id } if *id == cap)));
        }
        // 撤销后句柄所在槽被释放并递增 generation → 旧句柄不再可用（对用户同为 ENOENT）。
        assert_eq!(
            state.authorize(2, child, Right::Derive),
            Err(AuthError::NoSuchCapability)
        );
        assert!(state.positions.has_unique_active_locations());
    }

    #[test]
    fn task_capability_cleanup_revokes_holder_subtrees_but_protects_init() {
        let mut state = fresh_state();
        state.create_task(2).unwrap();
        let parent = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive,SegMapR"))
            .unwrap();
        state.move_capability(parent, INIT_TASK_ID, 2).unwrap();
        let child = state
            .derive_capability(2, parent, rights("Derive"))
            .unwrap();
        assert_eq!(state.revoke_task_capabilities(2), Ok(1));
        assert!(state.records.find(parent).is_none());
        assert!(state.records.find(child).is_none());
        assert_eq!(state.revoke_task_capabilities(2), Ok(0));
        assert_eq!(
            state.revoke_task_capabilities(INIT_TASK_ID),
            Err(CapabilityPositionError::ProtectedTask)
        );
        assert!(state.positions.has_unique_active_locations());
    }

    #[test]
    fn task_lifecycle_enforces_single_running_and_destroy_cleanup() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        let cap = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive,SegMapR"))
            .unwrap();
        state.move_capability(cap, INIT_TASK_ID, 2).unwrap();
        assert_eq!(state.tasks.entries[0].unwrap().status, TaskStatus::Running);
        state.schedule_task(2).unwrap_err();
        state.yield_task(INIT_TASK_ID).unwrap();
        state.schedule_task(2).unwrap();
        assert_eq!(state.tasks.entries[1].unwrap().status, TaskStatus::Running);
        assert_eq!(state.destroy_task(2), Err(TaskError::CannotDestroyRunning));
        state.yield_task(2).unwrap();
        state.destroy_task(2).unwrap();
        assert_eq!(state.tasks.entries[1].unwrap().status, TaskStatus::Dead);
        assert!(state.records.find(cap).is_none());
        assert_eq!(
            state.destroy_task(INIT_TASK_ID),
            Err(TaskError::CannotDestroyInit)
        );
    }

    #[test]
    fn ipc_fifo_blocks_empty_receiver_wakes_and_cleans_on_destroy() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
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
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        assert_eq!(
            state.send_message(2, INIT_TASK_ID, 7),
            Err(IpcError::NotAuthorized)
        );
        assert_eq!(state.ipc.count, 0);
        let send_cap = state
            .derive_capability(INIT_TASK_ID, TASK_CAP_HANDLE, rights("TaskSend"))
            .unwrap();
        state.move_capability(send_cap, INIT_TASK_ID, 2).unwrap();
        state.send_message(2, INIT_TASK_ID, 9).unwrap();
        assert_eq!(state.receive_message(INIT_TASK_ID).unwrap().payload, 9);
    }

    #[test]
    fn ipc_transfers_capability_atomically_through_message() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        assert_eq!(state.receive_message(2), Err(IpcError::QueueEmpty));
        state
            .send_message_with_capabilities(INIT_TASK_ID, 2, 55, &[IMAGE_CAP_HANDLE])
            .unwrap();
        assert_eq!(state.ipc.count, 0);
        // 前 5 条是 SPEC-061 的初始 Authority 审计，此处只验证新增的最后两条。
        assert_eq!(state.audit.count, 7);
        assert!(matches!(
            state.audit.entries[state.audit.count - 2],
            Some(AuditEvent::MessageSent { message_id: 1, .. })
        ));
        assert!(matches!(
            state.audit.entries[state.audit.count - 1],
            Some(AuditEvent::MessageReceived { message_id: 1, .. })
        ));
        assert_eq!(
            state.positions.find(IMAGE_CAP_HANDLE).unwrap().location,
            CapabilityLocation::HeldByTask(2)
        );
        assert_eq!(state.receive_message(2).unwrap().payload, 55);
        assert_eq!(
            state.send_message_with_capabilities(INIT_TASK_ID, 2, 66, &[IMAGE_CAP_HANDLE]),
            Err(IpcError::CapabilityHolderMismatch)
        );
        assert_eq!(state.ipc.count, 0);
    }

    #[test]
    fn authorize_grants_held_capability_with_right() {
        let state = fresh_state();
        assert_eq!(
            state.authorize(INIT_TASK_ID, IMAGE_CAP_HANDLE, Right::SegMapR),
            Ok(IMAGE_CAP_HANDLE)
        );
        assert_eq!(
            state.authorize(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, Right::NsResolve),
            Ok(NAMESPACE_CAP_HANDLE)
        );
        assert_eq!(
            state.authorize(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, Right::SegMapW),
            Err(AuthError::MissingRight)
        );
    }

    #[test]
    fn authorize_denies_missing_right() {
        let state = fresh_state();
        // 初始 image cap 只有 SegMapR/MapX/Destroy，没有 SegMapW。
        assert_eq!(
            state.authorize(INIT_TASK_ID, IMAGE_CAP_HANDLE, Right::SegMapW),
            Err(AuthError::MissingRight)
        );
    }

    #[test]
    fn authorize_reports_unknown_and_stale_handles_distinctly() {
        let mut state = fresh_state();
        let unallocated = Handle::new(60, 1).unwrap();
        assert_eq!(
            state.authorize(INIT_TASK_ID, unallocated, Right::SegMapR),
            Err(AuthError::NoSuchCapability)
        );
        let cap = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive"))
            .unwrap();
        state.drop_capability(INIT_TASK_ID, cap).unwrap();
        state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive"))
            .unwrap();
        assert_eq!(
            state.authorize(INIT_TASK_ID, cap, Right::Derive),
            Err(AuthError::StaleHandle)
        );
    }

    #[test]
    fn authorize_rejects_non_holder() {
        let state = fresh_state();
        assert_eq!(
            state.authorize(2, IMAGE_CAP_HANDLE, Right::SegMapR),
            Err(AuthError::NotHolder)
        );
    }

    fn exc_state() -> super::BootstrapState {
        let mut state = fresh_state();
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
        assert_eq!(page_fault.cap, EXC_PAGE_FAULT_HANDLE);
        assert_eq!(
            state.records.find(EXC_PAGE_FAULT_HANDLE).unwrap().rights,
            EXC_SOURCE_RIGHTS
        );
        assert_eq!(
            state.exc_sources.find(SYSCALL_TRAP_SOURCE_ID).unwrap().cap,
            EXC_SYSCALL_TRAP_HANDLE
        );
        let counts = state.exc_audit_counts();
        assert_eq!(counts.created, 2);
        assert_eq!(counts.subscribed, 0);
        assert!(state.validate_authority().is_ok());
    }

    #[test]
    fn exc_subscribe_checks_holder_kind_and_duplicates() {
        let mut state = exc_state();
        let page_cap = EXC_PAGE_FAULT_HANDLE;
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
            state.exc_subscribe(
                INIT_TASK_ID,
                Handle::new(61, 1).unwrap(),
                ExcKind::PageFault
            ),
            Err(ExcError::NoSuchSource)
        );
        // 缺少 ExcSubscribe：把源 cap 权限改窄后触发 MissingRight。
        state.records.entries[page_cap.index() as usize]
            .as_mut()
            .unwrap()
            .rights = Rights::from_rights(&[Right::Derive]);
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault),
            Err(ExcError::MissingRight)
        );
        state.records.entries[page_cap.index() as usize]
            .as_mut()
            .unwrap()
            .rights = EXC_SOURCE_RIGHTS;
        state.records.entries[page_cap.index() as usize]
            .as_mut()
            .unwrap()
            .holder = 2;
        assert_eq!(
            state.exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault),
            Err(ExcError::NotHolder)
        );
    }

    #[test]
    fn exc_unsubscribe_removes_and_audits_only_when_subscribed() {
        let mut state = exc_state();
        let page_cap = EXC_PAGE_FAULT_HANDLE;
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
        let mut state = fresh_state();
        state.platform_init().unwrap();
        let boot = state.exc_boot.unwrap();
        assert_eq!(boot.delivered, 1);
        assert_eq!(boot.pending, 1);
        assert!(boot.task_ready_after);
        assert_eq!(boot.payload, (1 << 16) | 1);
        assert_eq!(boot.message_id, 1);
        assert_eq!(
            state.exc_sources.find(PAGE_FAULT_SOURCE_ID).unwrap().cap,
            EXC_PAGE_FAULT_HANDLE
        );
        let counts = state.exc_audit_counts();
        assert_eq!(counts.created, 2);
        assert_eq!(counts.subscribed, 1);
        assert_eq!(counts.raised, 1);
        assert!(state.validate_authority().is_ok());
    }

    #[test]
    fn platform_segment_ids_and_cap_slots_are_stable_and_disjoint() {
        // 切片 R：Segment 槽位 id 与帧归属标签**恒等**（`100 + id` 命名空间已取消）。
        assert_eq!(crate::frames::SEG_VSPACE, PLATFORM_SEGMENT_ID_FIRST);
        assert_eq!(crate::frames::SEG_COW, 3);
        assert_eq!(crate::frames::SEG_TASK1_CODE, 4);
        assert_eq!(crate::frames::SEG_SPIN2_DATA, PLATFORM_SEGMENT_ID_LAST);
        assert_eq!(PLATFORM_SEGMENT_COUNT, 13);
        assert_eq!(USER_SEGMENT_ID_FIRST, PLATFORM_SEGMENT_ID_LAST + 1);

        // 平台根 cap 的槽位区间既不碰初始根/异常源（1..5），也不碰 ring3 见证程序
        // 内嵌句柄所用的最低空闲槽（6/7）。
        assert_eq!(PLATFORM_SEGMENT_CAP_SLOT_BASE, 32);
        for id in PLATFORM_SEGMENT_ID_FIRST..=PLATFORM_SEGMENT_ID_LAST {
            let slot = platform_segment_cap_slot(id);
            assert!((32..45).contains(&slot), "平台槽 {slot} 越界");
        }
        assert_eq!(
            platform_segment_cap_handle(2).encode(),
            0x0000_0001_0000_0020
        );
        assert_eq!(
            platform_segment_cap_handle(14).encode(),
            0x0000_0001_0000_002c
        );
        assert_eq!(PLATFORM_TASK1_CAP_HANDLE.encode(), 0x0000_0001_0000_002d);
        assert_eq!(PLATFORM_TASK2_CAP_HANDLE.encode(), 0x0000_0001_0000_002e);
        assert!(PLATFORM_TASK_RIGHTS.contains(Right::TaskManageVSpace));

        // 只有 VSPACE 与 COW 保留内核直映射。
        assert!(platform_segment_is_direct(crate::frames::SEG_VSPACE));
        assert!(platform_segment_is_direct(crate::frames::SEG_COW));
        assert!(!platform_segment_is_direct(crate::frames::SEG_TASK1_CODE));
    }

    #[test]
    fn platform_root_capability_is_minted_at_an_explicit_kernel_held_slot() {
        let mut state = fresh_state();
        let mut frames = [0u64; MAX_SEGMENT_FRAMES];
        frames[0] = 0x20_0000;
        frames[1] = 0x21_0000;
        state.segments[state.segment_count] = Some(SegmentObject {
            id: crate::frames::SEG_VSPACE,
            generation: INITIAL_CAP_GENERATION,
            creator: KERNEL_ACTOR,
            pool: crate::frames::Pool::Cpu,
            frames: SegmentFrames::Vector { frames, count: 2 },
            allowed_access: Rights::EMPTY,
            active: true,
        });
        state.segment_count += 1;

        let handle = state
            .mint_root_capability_at(
                CapabilityLocation::HeldByKernel,
                platform_segment_cap_slot(crate::frames::SEG_VSPACE),
                SEGMENT_ROOT_RIGHTS,
                ObjectRef::Segment(crate::frames::SEG_VSPACE),
            )
            .unwrap();
        assert_eq!(
            handle,
            platform_segment_cap_handle(crate::frames::SEG_VSPACE)
        );
        assert_eq!(handle.encode(), 0x0000_0001_0000_0020);
        assert!(!state.slots.slot_free(handle.index()));
        assert_eq!(state.holder_of(handle), Some(KERNEL_ACTOR));
        assert_eq!(
            state.positions.find(handle).unwrap().location,
            CapabilityLocation::HeldByKernel
        );
        assert_eq!(
            state.authorize(KERNEL_ACTOR, handle, Right::SegMapR),
            Ok(handle)
        );
        // 内核持有的位置不是 Task，故不参与 Task 存活校验；init 的初始 Authority 仍为 3。
        assert_eq!(state.validate_authority(), Ok(()));
        assert_eq!(state.positions.count, INITIAL_CAPABILITY_COUNT as usize + 1);
        assert_eq!(state.records.find(handle).unwrap().holder, KERNEL_ACTOR);

        // 同一槽再铸、槽 0、越界槽都被拒（失败不改状态）。
        for slot in [32, 0, MAX_CAPABILITY_SLOTS as u32] {
            assert_eq!(
                state.mint_root_capability_at(
                    CapabilityLocation::HeldByKernel,
                    slot,
                    SEGMENT_ROOT_RIGHTS,
                    ObjectRef::Segment(crate::frames::SEG_VSPACE),
                ),
                Err(DeriveError::CapacityExceeded)
            );
        }
        assert_eq!(state.positions.count, INITIAL_CAPABILITY_COUNT as usize + 1);
    }

    #[test]
    fn platform_segment_record_witnesses_objects_caps_and_vspace() {
        let mut state = fresh_state();
        for (id, allowed) in [
            (crate::frames::SEG_VSPACE, Rights::EMPTY),
            (
                crate::frames::SEG_TASK1_CODE,
                Rights::from_rights(&[Right::SegMapR, Right::SegMapX]),
            ),
        ] {
            let mut frames = [0u64; MAX_SEGMENT_FRAMES];
            frames[0] = 0x20_0000 + id as u64 * 0x1000;
            frames[1] = 0x21_0000 + id as u64 * 0x1000;
            let count = if id == crate::frames::SEG_VSPACE {
                2
            } else {
                1
            };
            state.segments[state.segment_count] = Some(SegmentObject {
                id,
                generation: INITIAL_CAP_GENERATION,
                creator: KERNEL_ACTOR,
                pool: crate::frames::Pool::Cpu,
                frames: SegmentFrames::Vector { frames, count },
                allowed_access: allowed,
                active: true,
            });
            state.segment_count += 1;
            state
                .mint_root_capability_at(
                    CapabilityLocation::HeldByKernel,
                    platform_segment_cap_slot(id),
                    SEGMENT_ROOT_RIGHTS,
                    ObjectRef::Segment(id),
                )
                .unwrap();
        }
        // TASK1_CODE 经 `memseg_map` 留下逻辑映射；VSPACE 保留直映射，故无条目。
        state
            .vspace
            .insert(VspaceEntry {
                task_id: INIT_TASK_ID,
                va: 0x0000_0A00_0000_0000,
                seg_id: crate::frames::SEG_TASK1_CODE,
                seg_offset: 0,
                access: Rights::from_rights(&[Right::SegMapR, Right::SegMapX]),
            })
            .unwrap();

        let record = state.platform_segments_record();
        assert_eq!(record.segments, 2);
        assert_eq!(record.roots, 2);
        assert_eq!(record.first_id, PLATFORM_SEGMENT_ID_FIRST);
        assert_eq!(record.last_id, PLATFORM_SEGMENT_ID_LAST);
        assert_eq!(record.mapped, 1);
        assert_eq!(record.direct, 1);
        assert_eq!(
            record.direct_ids,
            [crate::frames::SEG_VSPACE, crate::frames::SEG_COW]
        );
        assert_eq!(record.vspace, 1);
        // `frame_counts` 按 `id - first_id` 直索引：0=VSPACE(2)、1=COW(3)、2=TASK1_CODE(4)。
        assert_eq!(record.frame_counts[0], 2);
        assert_eq!(record.frame_counts[1], 0, "COW 未物化");
        assert_eq!(record.frame_counts[2], 1);
        // 帧注册表在 host 上未安装，`unified` 无法为真——该字段的权威证据是 QEMU 的
        // `platform_segments_ready.unified=1`（见 xtask 校验器）。
        assert!(!record.unified);
    }

    #[test]
    fn user_segment_slots_start_after_the_platform_range() {
        let mut state = fresh_state();
        assert_eq!(state.alloc_segment_slot(), Some(USER_SEGMENT_ID_FIRST));
        for id in PLATFORM_SEGMENT_ID_FIRST..=PLATFORM_SEGMENT_ID_LAST {
            state.segments[state.segment_count] = Some(SegmentObject {
                id,
                generation: INITIAL_CAP_GENERATION,
                creator: KERNEL_ACTOR,
                pool: crate::frames::Pool::Cpu,
                frames: SegmentFrames::Vector {
                    frames: [0; MAX_SEGMENT_FRAMES],
                    count: 0,
                },
                allowed_access: Rights::EMPTY,
                active: true,
            });
            state.segment_count += 1;
        }
        assert_eq!(state.alloc_segment_slot(), Some(USER_SEGMENT_ID_FIRST));
    }

    #[test]
    fn platform_create_segment_validates_id_range_and_access_first() {
        let mut state = fresh_state();
        // id 必须在平台区间内：image 的 1 与用户区间的 15 都被拒。
        for id in [IMAGE_SEGMENT_ID, USER_SEGMENT_ID_FIRST] {
            assert_eq!(
                state.platform_create_segment(id, 1, Rights::EMPTY, 0),
                Err(MemsegError::CapacityExceeded)
            );
        }
        assert_eq!(
            state.platform_create_segment(crate::frames::SEG_VSPACE, 0, Rights::EMPTY, 0),
            Err(MemsegError::ZeroSize)
        );
        // 非空权限必须是 Segment 访问集的子集（`TaskSend` 不是 Segment 权限）。
        assert_eq!(
            state.platform_create_segment(crate::frames::SEG_TASK1_CODE, 1, rights("TaskSend"), 0),
            Err(MemsegError::InvalidAccess)
        );
        // 参数合法时进入取帧路径；host 未安装帧注册表，故以 `NoMemory` 结束
        // （真实取帧/清零/铸 cap 由 QEMU 的 `platform_segments_ready` 覆盖）。
        assert_eq!(
            state.platform_create_segment(
                crate::frames::SEG_TASK1_CODE,
                1,
                rights("SegMapR,SegMapX"),
                0
            ),
            Err(MemsegError::NoMemory)
        );
    }

    #[test]
    fn bootstrap_state_stays_within_the_stack_budget() {
        // `create_state` 按值返回该结构，内核启动栈有限：钉死上限防止后续扩容悄悄超栈。
        let size = core::mem::size_of::<super::BootstrapState>();
        assert!(size <= 16 * 1024, "BootstrapState size = {size} bytes");
    }

    #[test]
    fn validate_authority_rejects_source_cap_without_record() {
        let mut state = exc_state();
        // 源 cap 指向没有 CapRecord 的初始槽 → 违反「每个 live cap 有记录」。
        state.exc_sources.entries[0].as_mut().unwrap().cap = IMAGE_CAP_HANDLE;
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::MissingPosition)
        );
    }
}
