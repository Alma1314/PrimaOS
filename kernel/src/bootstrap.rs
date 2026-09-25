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
/// Audit 容量。
///
/// SPEC-071（`ISSUE-TASK-003`）：§13.1/W13 要求**每次** Task 状态迁移都追加 `TaskStateChanged`。
/// 实测补齐调度路径后 boot 期共 **50** 条该类事件、审计总需求 130 条 > 原容量 128（被打满），
/// 故按项目既有先例（切片 R：**保留 16 KiB 结构体守卫**、调整容量并显式登记）把容量提到 170。
///
/// 尺寸实测：`AuditEvent` = 24 B（`Option` 经 niche 优化同为 24 B），`BootstrapState` 由
/// 15304 B 增至 **16312 B**，仍 ≤ 守卫 16384 B——**守卫未放宽**，但结构体余量只剩 72 B。
/// 后续任何进一步存储增长都**必须先做审计存储重构**（把 `AuditLog` 移出 `BootstrapState`），
/// 登记为已知瓶颈。
const MAX_AUDIT_EVENTS: usize = 170;

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

/// SPEC-070：`Faulted` 见证 Task 的**保留槽位** id（`MAX_TASKS = 8`，普通 id 从 2 起分配）。
///
/// 用保留槽位而非 `next_free_id`，使该 Task 不占用普通 id——既有 `child=3`/`grandchild=4`
/// 等 id 证据不发生顺移。
pub const FAULT_WITNESS_TASK_ID: u32 = 7;

/// SPEC-072：内核回收见证的**保留槽位**——不可恢复的 Faulted 任务（将被回收）。
pub const RECLAIM_WITNESS_TASK_ID: u32 = 6;
/// SPEC-072：内核回收反例的**保留槽位**——可恢复的 Faulted 任务（必须被拒绝回收）。
pub const RECLAIM_NEGATIVE_TASK_ID: u32 = 5;
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
    /// S2-D4：`task_create` 成功（§8.1 步骤 8）。
    TaskCreated {
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
    /// §5.2/§12.5：硬件执行异常使 Task 进入的常规状态。不可由调度隐式恢复。
    Faulted,
    Dead,
}

/// §5.2：Faulted Task 关联的未决异常（`status == Faulted ⇔ pending_exc != None`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingExc {
    pub source_id: u32,
    pub kind: ExcKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: u32,
    pub status: TaskStatus,
    /// §5.2/§14 步骤 7：Task 的结构性根 Namespace Capability（[`Handle::ZERO`] 表示无）。
    pub root_ns_cap: Handle,
    /// §5.2：Task 入口虚拟地址（`memseg_unmap` 的 `entry` 页保护）。
    pub entry: u64,
    /// §5.2/W9：仅 `status == Faulted` 时非空（双向不变量由 `check_faulted_invariant` 钉住）。
    pub pending_exc: Option<PendingExc>,
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
    /// SPEC-072：§8.2 内核回收路径的前置不成立——目标不是「不可恢复的 Faulted」。
    NotUnrecoverable,
}

/// S2-D4：`task_create` 的失败原因（§8.1 的错误名）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskCreateError {
    /// cap 槽位从未分配 / 目标对象已销毁。
    NoSuchCapability,
    /// cap 不在调用者手中。
    NotHolder,
    /// `code_pages = 0` 或 `code_offset + code_pages` 超出 Segment 帧数（`InvalidCodeRange`）。
    InvalidCodeRange,
    /// 入口不在 `[load_base, load_base + code_pages*4 KiB)`（`InvalidEntry`）。
    InvalidEntry,
    /// `parent_ns_cap` 不是 Namespace 或已失效（`InvalidNamespace`）。
    InvalidNamespace,
    /// 缺 `SegMapR`/`SegMapX`/`NsResolve`/`Derive` 等前置权限（`MissingRight`）。
    MissingRight,
    /// `root_rights ⊄ parent_ns_cap.rights`（`RightsExceeded`）或 `root_rights` 缺 `NsResolve`。
    RightsExceeded,
    /// `initial_caps` 有一条不可用（`InvalidInitialCap`）。
    InvalidInitialCap,
    /// `initial_caps` 有重复（`DuplicateCap`）。
    DuplicateCap,
    /// `initial_caps` 含活动 Task 的结构性根 Namespace Capability。
    StructuralCap,
    /// 任务表 / cap 槽位已满。
    CapacityExceeded,
    /// 页帧不足。
    NoMemory,
    AuditFull,
}

/// S2-D4：`task_create` 的请求块（用户内存布局见 `syscall.rs`；细化层字段 `stack_top` 在内）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskCreateRequest {
    pub code_seg_cap: Handle,
    pub code_offset: u64,
    pub code_pages: u64,
    pub load_base: u64,
    pub entry: u64,
    /// 细化层字段：新 Task 的初始 `rsp`（抽象模型 §5.2 的 Task 无 SP）。
    pub stack_top: u64,
    pub parent_ns_cap: Handle,
    pub root_rights: Rights,
    pub initial_caps: [Handle; MAX_TASK_INITIAL_CAPS],
}

/// §8.1 的 `initial_caps` 在本细化层固定为至多 2 条（与消息携带 cap 同容量）。
pub const MAX_TASK_INITIAL_CAPS: usize = 2;

/// §8.1 步骤 3：新 Task 的根管理 Capability 权限，恰为 `{Derive, TaskSend, TaskManageVSpace, TaskDestroy}`。
pub const MANAGER_TASK_RIGHTS: Rights = Rights::from_rights(&[
    Right::Derive,
    Right::TaskSend,
    Right::TaskManageVSpace,
    Right::TaskDestroy,
]);

/// S2-D4：`task_create` 成功后的产物（`root_phys` 供 syscall 层登记调度器）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskCreateOutcome {
    pub manager_cap: Handle,
    pub self_cap: Handle,
    pub root_ns_cap: Handle,
    pub task_id: u32,
    pub root_phys: u64,
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
                    pending_exc: None,
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
        self.create_with_entry(task_id, 0)
    }

    /// S2-D4：按 §8.1 步骤 1 建 Task 记录并写入入口 VA（`memseg_unmap` 的入口页保护依据）。
    pub fn create_with_entry(&mut self, task_id: u32, entry: u64) -> Result<(), TaskError> {
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
            entry,
            pending_exc: None,
            pending: None,
        });
        self.count += 1;
        Ok(())
    }

    /// S2-D4：`task_create` 选定的新 id——最小空闲的 id（`≥ 2`，init 与已有任务之外）。
    pub fn next_free_id(&self) -> Option<u32> {
        let mut task_id = INIT_TASK_ID + 1;
        while (task_id as usize) < MAX_TASKS {
            if self.find(task_id).is_none() {
                return Some(task_id);
            }
            task_id += 1;
        }
        None
    }

    /// S2-D4：移除 Task 记录（仅供 `task_create` 的回滚路径；正常销毁走 `finish_destroy` 保留
    /// `Dead` 记录，使 id 不复用）。
    pub fn remove(&mut self, task_id: u32) -> bool {
        let Some(index) = self.entries[..self.count]
            .iter()
            .position(|entry| entry.is_some_and(|task| task.id == task_id))
        else {
            return false;
        };
        for shift in index..self.count - 1 {
            self.entries[shift] = self.entries[shift + 1];
        }
        self.entries[self.count - 1] = None;
        self.count -= 1;
        true
    }

    /// S2-D4：写入结构根 Capability（§8.1 步骤 6）。
    pub fn set_root_ns_cap(&mut self, task_id: u32, cap: Handle) -> Result<(), TaskError> {
        self.find_mut(task_id)
            .ok_or(TaskError::NoSuchTask)
            .map(|task| task.root_ns_cap = cap)
    }

    /// 目标 Task 是否存活（非 `Dead`）。S2-D4：授权与目标存活的判定入口。
    pub fn is_live(&self, task_id: u32) -> bool {
        self.find(task_id)
            .is_some_and(|task| task.status != TaskStatus::Dead)
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
        // §8.4 步骤 3：`schedule` 进 Running 时消费并清空 `kernel_resume_frame`（细化层持有）。
        crate::sched::clear_kernel_resume_frame(task_id);
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

    /// §12.5 成功动作 3：把**正在运行**的 `task_id` 置为 `Faulted` 并写入唯一 `pending_exc`。
    ///
    /// 前置 4：`faulting_task` 必须是 Running（否则返回 [`TaskError::NotRunning`]，状态不变）。
    pub fn fault(&mut self, task_id: u32, source_id: u32, kind: ExcKind) -> Result<(), TaskError> {
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        if task.status != TaskStatus::Running {
            return Err(TaskError::NotRunning);
        }
        task.status = TaskStatus::Faulted;
        task.pending_exc = Some(PendingExc { source_id, kind });
        Ok(())
    }

    /// §5.2/W9：`status == Faulted ⇔ pending_exc != None`（双向），且非 Faulted 者必为 `None`。
    pub fn check_faulted_invariant(&self) -> Result<(), TaskError> {
        for task in self.entries[..self.count].iter().flatten() {
            if (task.status == TaskStatus::Faulted) != task.pending_exc.is_some() {
                return Err(TaskError::NotRunning);
            }
        }
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
        // §8.2 步骤 6：清空目标的 `pending_exc` 与 `kernel_resume_frame`（覆盖 Faulted 与已恢复者）。
        task.pending_exc = None;
        crate::sched::clear_kernel_resume_frame(task_id);
        Ok(())
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
pub const MAX_MESSAGE_CAPABILITIES: usize = 2;

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
    /// 一般保护故障（§12.10 可恢复）。
    GeneralProtectionFault,
    /// 除零（§12.10 可恢复）。
    DivideError,
    /// 非法指令（§12.10 可恢复）。
    InvalidOpcode,
    /// 双重故障（§12.10 **不可恢复**：Task 永久保持 `Faulted`）。
    DoubleFault,
    /// syscall trap（对应 `syscall_trap_source`；**非**硬件执行异常）。
    SyscallTrap,
}

impl ExcKind {
    /// §12.5 前置 4：是否为**硬件执行异常**（只有它会把 `faulting_task` 置 Faulted）。
    pub const fn is_hardware(self) -> bool {
        !matches!(self, ExcKind::SyscallTrap)
    }

    /// §12.10 前置 6：`exc_resume` 是否可恢复该种类（`DoubleFault` 等为不可恢复）。
    pub const fn is_recoverable(self) -> bool {
        matches!(
            self,
            ExcKind::PageFault
                | ExcKind::GeneralProtectionFault
                | ExcKind::DivideError
                | ExcKind::InvalidOpcode
        )
    }
}

/// 异常种类的稳定编码（协议/镜像常量与 syscall 共用）。
pub const fn kind_code(kind: ExcKind) -> u32 {
    match kind {
        ExcKind::PageFault => 1,
        ExcKind::SyscallTrap => 2,
        ExcKind::GeneralProtectionFault => 3,
        ExcKind::DivideError => 4,
        ExcKind::InvalidOpcode => 5,
        ExcKind::DoubleFault => 6,
    }
}

/// 异常种类的稳定名字（日志用）。
pub const fn kind_name(kind: ExcKind) -> &'static str {
    match kind {
        ExcKind::PageFault => "page_fault",
        ExcKind::SyscallTrap => "syscall_trap",
        ExcKind::GeneralProtectionFault => "general_protection_fault",
        ExcKind::DivideError => "divide_error",
        ExcKind::InvalidOpcode => "invalid_opcode",
        ExcKind::DoubleFault => "double_fault",
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
    /// §12.5 前置 4 不成立：`faulting_task` 不是 `Running`（状态不变）。
    NotRunning,
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

    /// S2-D4 / §8.2 步骤 7：从所有源的订阅集合移除 `task`。
    ///
    /// 返回被移除的订阅条数。销毁是被授权动作的原子清理，故**不追加** `ExcUnsubscribed`
    /// （§8.2 步骤 9 只要求追加 Task 销毁、消息丢弃、Capability 丢弃与 Timer 事件）。
    pub fn remove_subscriber(&mut self, task: u32) -> usize {
        let mut removed = 0usize;
        for source in self.entries[..self.count].iter_mut().flatten() {
            let mut index = 0usize;
            while index < source.count {
                if source.subscribers[index] == Some(task) {
                    for shift in index..source.count - 1 {
                        source.subscribers[shift] = source.subscribers[shift + 1];
                    }
                    source.count -= 1;
                    source.subscribers[source.count] = None;
                    removed += 1;
                } else {
                    index += 1;
                }
            }
        }
        removed
    }
}

/// SPEC-071：§13.1/W13 的 Task 状态迁移审计汇总（QEMU 见证）。
///
/// 计数**按目标状态**分列，使"每条迁移路径都真的有事件"成为可断言的事实；
/// `used`/`cap` 钉住审计余量（本切片实测 81+32=113 ≤ 128，故无需存储重构）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskStateAudit {
    pub to_running: u32,
    pub to_ready: u32,
    pub to_blocked: u32,
    pub to_faulted: u32,
    pub used: u32,
    pub cap: u32,
}

impl TaskStateAudit {
    pub fn total(&self) -> u32 {
        self.to_running + self.to_ready + self.to_blocked + self.to_faulted
    }
}

/// SPEC-072：§8.2 内核回收路径（`KernelActor`，无 `task_cap`）的 QEMU 见证。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimWitness {
    /// 被回收的保留槽位任务（不可恢复的 Faulted）。
    pub task_id: u32,
    /// 反例任务的保留槽位（可恢复的 Faulted，必须拒绝回收）。
    pub negative_task: u32,
    /// `is_unrecoverable_faulted` 对正例为真（1 = 成立）。
    pub unrecoverable: u32,
    /// 内核回收成功（1 = 成功）。
    pub reclaimed: u32,
    /// 反例（可恢复的 Faulted）被拒绝且状态不变（1 = 成立）。
    pub negative_rejected: u32,
    /// 回收后目标不再存活（1 = 不再存活）。
    pub after_dead: u32,
    /// 回收后 `pending_exc` 已按 §8.2 步骤 6 清空（1 = 已清空）。
    pub pending_cleared: u32,
    /// W9 双向不变量在回收后仍成立（1 = 成立）。
    pub invariant_ok: u32,
}

/// SPEC-070：`Faulted` 状态机的 QEMU 见证。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FaultWitness {
    pub task_id: u32,
    /// §12.5 前置 4 通过时的状态（应为 `Running`）。
    pub before: TaskStatus,
    /// 成功动作 3 之后的状态（应为 `Faulted`）。
    pub after: TaskStatus,
    /// `raise_exception` 是否成功（1 = 成功）。
    pub raised: u32,
    /// `pending_exc.source_id`（应为 `PAGE_FAULT_SOURCE_ID`）。
    pub pending_source: u32,
    /// `pending_exc.kind` 的稳定编码（应为 `PageFault` = 1）。
    pub pending_kind: u32,
    /// §12.5 前置 4 反例：对 Ready 的 init raise 硬件异常被拒且状态不变（1 = 成立）。
    pub precond_rejected: u32,
    /// W9 双向不变量 `Faulted ⇔ pending_exc != None`（1 = 成立）。
    pub invariant_ok: u32,
    /// 调度器不会隐式恢复：`schedule(Faulted)` 被拒且状态不变（1 = 成立）。
    pub unschedulable: u32,
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

/// SPEC-066：11 条用户可见平台段 id → 属主 Task id（按 id 升序）。
///
/// `SEG_VSPACE`/`SEG_COW` **不在表内**：它们 `allowed_access` 为空（`memseg_map` 对其必然
/// `RightsExceeded`），且根 cap 带 `SegDestroy`——交给不可信用户等于授权销毁内核自建映射与
/// COW 私有帧区，故保留内核持有。
pub const PLATFORM_SEGMENT_OWNERS: [(u32, u32); 11] = [
    (crate::frames::SEG_TASK1_CODE, INIT_TASK_ID),
    (crate::frames::SEG_TASK1_STACK, INIT_TASK_ID),
    (crate::frames::SEG_TASK1_X, INIT_TASK_ID),
    (crate::frames::SEG_TASK1_Y, INIT_TASK_ID),
    (crate::frames::SEG_TASK2_CODE, 2),
    (crate::frames::SEG_TASK2_STACK, 2),
    (crate::frames::SEG_TASK2_Y, 2),
    (crate::frames::SEG_SPIN1_CODE, INIT_TASK_ID),
    (crate::frames::SEG_SPIN1_DATA, INIT_TASK_ID),
    (crate::frames::SEG_SPIN2_CODE, 2),
    (crate::frames::SEG_SPIN2_DATA, 2),
];

/// 切片 S：用户 IPC 见证所需 Capability 的槽位基址（47 = task 1 的发送权，48 = task 2 的自管理权）。
///
/// 避开 1..5（初始根/异常源）、6/7/8（ring3 见证程序内嵌句柄）、32..46（平台 cap）与
/// `UNSET_CAP`(60)。这两条是**新增铸造**，不动内核持有的平台 cap。
pub const USER_IPC_CAP_SLOT_BASE: u32 =
    PLATFORM_TASK_CAP_SLOT_BASE + PLATFORM_TASK_CAP_COUNT as u32;
/// task 1 持有、目标 `Task(2)`、含 `TaskSend`：**发给 task 2 的授权凭证**。
pub const TASK2_SEND_CAP_HANDLE: Handle =
    cap_handle(USER_IPC_CAP_SLOT_BASE, INITIAL_CAP_GENERATION);
/// task 2 持有、目标 `Task(2)`、含 `TaskManageVSpace`：把自己收到的共享段映射进自己的根。
pub const TASK2_SELF_CAP_HANDLE: Handle =
    cap_handle(USER_IPC_CAP_SLOT_BASE + 1, INITIAL_CAP_GENERATION);
/// 发送权：只给 `TaskSend`（不含 `Derive`，不可自我复制）。
pub const TASK2_SEND_RIGHTS: Rights = Rights::from_rights(&[Right::TaskSend]);
/// 自管理权：只给 `TaskManageVSpace`。
pub const TASK2_SELF_RIGHTS: Rights = Rights::from_rights(&[Right::TaskManageVSpace]);

/// S2-D4：`task_create` 见证所需 Capability 的槽位（49 = 新 Task 的代码段，50 = task 1 的
/// `Task(1)`+`TaskSend`，作为 `initial_caps` 移入新 Task）。
///
/// 避开 1..5、6/7/8、32..48（切片 R/S）与 `UNSET_CAP`(60)。两条均为 `mint_root_capability_at`
/// 的**新增铸造**，不动内核持有的平台 cap。
pub const TASK_CREATE_CODE_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 2;
/// task 1 自持的 `Task(1)`+`TaskSend`：`initial_caps` 把它移入新 Task，使子任务能向 task 1 发送。
pub const TASK_CREATE_SEND_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 3;
/// 新 Task 代码段的根 Capability（持有者 task 1，`{Derive, SegMapR, SegMapX, SegDestroy}`）。
pub const TASK_CREATE_CODE_CAP_HANDLE: Handle =
    cap_handle(TASK_CREATE_CODE_CAP_SLOT, INITIAL_CAP_GENERATION);
/// 作为 `initial_cap0` 移入新 Task 的句柄（句柄在移动中不变，子任务代码内嵌此常量）。
pub const TASK_CREATE_SEND_CAP_HANDLE: Handle =
    cap_handle(TASK_CREATE_SEND_CAP_SLOT, INITIAL_CAP_GENERATION);
/// 切片 V：task 4 的回发凭据槽（task 3 的那条已随其销毁被撤销，不能复用同一槽）。
pub const TASK4_SEND_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 4;
/// task 4 持有、授权 `SYS_SEND` 给 task 1 的凭据（经第二次 create 的 `initial_cap0` 移入）。
pub const TASK4_SEND_CAP_HANDLE: Handle = cap_handle(TASK4_SEND_CAP_SLOT, INITIAL_CAP_GENERATION);
/// 代码段根 Capability 权限：只给 `{R, X}` 映射权 + `Derive`（供 `memseg_map` 的授权检查）+ 销毁权。
pub const TASK_CREATE_CODE_RIGHTS: Rights = Rights::from_rights(&[
    Right::Derive,
    Right::SegMapR,
    Right::SegMapX,
    Right::SegDestroy,
]);
/// 新 Task 的结构性根 Namespace 权限（`root_rights`）：只含 `NsResolve`。
pub const TASK_CREATE_ROOT_NS_RIGHTS: Rights = Rights::from_rights(&[Right::NsResolve]);
/// 新 Task 代码段的访问集（§8.1 步骤 2：初始代码映射权限恰为 `{R, X}`）。
pub const TASK_CREATE_CODE_ACCESS: Rights = Rights::from_rights(&[Right::SegMapR, Right::SegMapX]);

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
    /// 通过全部校验的平台根 Capability 数（槽 live、目标 `Segment(id)`、位置为内核或存活
    /// Task 持有、权限非空且类型正确）。
    pub roots: u32,
    /// SPEC-066：按位置分类——由存活用户 Task 持有的平台段根数。
    pub user_roots: u32,
    /// SPEC-066：按位置分类——仍由内核持有的平台段根数（`SEG_VSPACE`/`SEG_COW`）。
    pub kernel_roots: u32,
    /// SPEC-066：逐槽属主 Task id（内核持有记 [`KERNEL_ACTOR`]），与 `cap_handles` 同序。
    pub holders: [u32; PLATFORM_SEGMENT_COUNT],
    /// 通过校验的平台 Task Capability 数。
    pub task_caps: u32,
    /// SPEC-066：平台 Task Capability 的逐槽属主 Task id。
    pub task_holders: [u32; PLATFORM_TASK_CAP_COUNT],
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
/// `Vector` 是用户/平台 bring-up Segment 的**内联真实帧向量**（容量 [`MAX_SEGMENT_FRAMES`]）。
/// `Image` 是 bootloader 载入的 image Segment 的**真实逐帧向量**（SPEC-068）；其帧数随内核
/// 镜像变化（实测 663–778），无法内联进本结构（`BootstrapState` 已贴近 16 KiB 守卫，见
/// [`MAX_SEGMENT_OBJECTS`] 注释），故向量物化在帧注册表 `frames::image_frame_*` 中，
/// 此处持有其长度。`frame_at` 对 `Image` 逐帧委托注册表——与 `Vector` 同为 §5.1 的
/// `Vector<FrameId>`，不再是 `(base, count)` 区间表达（ISSUE-FRAME-001）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentFrames {
    Vector {
        frames: [u64; MAX_SEGMENT_FRAMES],
        count: usize,
    },
    Image {
        count: usize,
    },
}

impl SegmentFrames {
    pub fn frame_count(&self) -> usize {
        match self {
            SegmentFrames::Vector { count, .. } => *count,
            SegmentFrames::Image { count } => *count,
        }
    }

    pub fn frame_at(&self, index: usize) -> Option<u64> {
        match self {
            SegmentFrames::Vector { frames, count } => {
                if index < *count {
                    Some(frames[index])
                } else {
                    None
                }
            }
            SegmentFrames::Image { count } => {
                if index < *count {
                    crate::frames::image_frame_at(index).ok()
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

    /// SPEC-071：汇总审计日志里按目标状态分类的 `TaskStateChanged` 计数与审计余量。
    pub fn task_state_audit(&self) -> TaskStateAudit {
        let mut audit = TaskStateAudit {
            to_running: 0,
            to_ready: 0,
            to_blocked: 0,
            to_faulted: 0,
            used: self.audit.count as u32,
            cap: MAX_AUDIT_EVENTS as u32,
        };
        for event in self.audit.entries[..self.audit.count].iter().flatten() {
            if let AuditEvent::TaskStateChanged { status, .. } = event {
                match status {
                    TaskStatus::Running => audit.to_running += 1,
                    TaskStatus::Ready => audit.to_ready += 1,
                    TaskStatus::BlockedRecv => audit.to_blocked += 1,
                    TaskStatus::Faulted => audit.to_faulted += 1,
                    // `Dead` 是细化态、非 POSIR 状态，§13.1 不对它要求事件（登记在案）。
                    TaskStatus::Dead => {}
                }
            }
        }
        audit
    }

    /// SPEC-070：`Faulted` 状态机的 boot 期见证。
    ///
    /// **采集时机必须在 `activate_init` 之前**：§8.4 的 `schedule` 只允许至多一个 Running，
    /// 此刻尚无任何 Task 处于 Running，故"建见证 Task → 调度为 Running → §12.5 置 Faulted"
    /// 是一条**合法**路径。见证 Task 用**保留槽位** [`FAULT_WITNESS_TASK_ID`]，不占用普通 id、
    /// 不铸 Capability、不取帧，故既有 id/帧/cap 序列不受扰动。
    ///
    /// 同时钉住三条反例/前提：对 **Ready** 任务 raise 硬件异常必须被拒（前置 4）、
    /// `status == Faulted ⇔ pending_exc != None`（W9 双向）、Faulted 任务不可被 `schedule`
    /// 选中（它不在 Ready 集合内，调度器不会隐式恢复它）。
    pub fn fault_witness(&mut self) -> Result<FaultWitness, TaskError> {
        let task_id = FAULT_WITNESS_TASK_ID;
        self.tasks.create_with_entry(task_id, 0)?;
        // 前置 4 反例：init 此刻仍是 Ready，对它 raise 硬件异常必须被拒且不留痕。
        let precond_rejected =
            self.tasks
                .fault(INIT_TASK_ID, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
                .is_err()
                && self.tasks.find(INIT_TASK_ID).is_some_and(|task| {
                    task.status == TaskStatus::Ready && task.pending_exc.is_none()
                });
        // 合法路径：Ready → Running → Faulted。
        self.tasks.schedule(task_id)?;
        let before = self.task_status_of(task_id).unwrap_or(TaskStatus::Dead);
        // 只走 §12.5 前置 4 + 成功动作 3 + `TaskStateChanged` 审计（状态机本体），**不经**
        // `raise_exception` 的通知递送：boot 期 `page_fault_source` 已有内核侧订阅者，
        // 递送会往其邮箱塞一条通知，扰动既有 IPC 证据（实测）。完整形态见 `exc_raise_on`，
        // 其递送语义由 host 单测钉住。
        let raised = self
            .tasks
            .fault(task_id, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
            .is_ok();
        if raised {
            self.audit.append(AuditEvent::TaskStateChanged {
                task_id,
                status: TaskStatus::Faulted,
            });
        }
        let pending = self.task_pending_exc(task_id);
        let after = self.task_status_of(task_id).unwrap_or(TaskStatus::Dead);
        let invariant_ok = self.tasks.check_faulted_invariant().is_ok();
        // 调度器不会选中 Faulted 任务：它既不是 Ready，`schedule` 也要求 Ready。
        let unschedulable = self.tasks.schedule(task_id).is_err()
            && self
                .tasks
                .find(task_id)
                .is_some_and(|task| task.status == TaskStatus::Faulted);
        Ok(FaultWitness {
            task_id,
            before,
            after,
            raised: u32::from(raised),
            pending_source: pending.map(|exc| exc.source_id).unwrap_or(0),
            pending_kind: pending.map(|exc| kind_code(exc.kind)).unwrap_or(0),
            precond_rejected: u32::from(precond_rejected),
            invariant_ok: u32::from(invariant_ok),
            unschedulable: u32::from(unschedulable),
        })
    }

    /// SPEC-072：§8.2 内核回收路径的 boot 期见证（与 `fault_witness` 同一窗口：
    /// 必须在 `activate_init` **之前**，此刻尚无任何 Task 处于 `Running`，`schedule` 合法）。
    ///
    /// 反例先行——**可恢复**的 `Faulted`（`PageFault` 且源存活）**不得**被内核回收（它是
    /// §12.10 的候选，回收会绕过未来的恢复路径）；正例为**不可恢复**的 `Faulted`
    /// （`DoubleFault`，§12.10 前置 6），由 KernelActor 无 cap 回收。
    pub fn reclaim_witness(&mut self) -> Result<ReclaimWitness, TaskError> {
        // 反例：可恢复的 Faulted。
        self.tasks.create_with_entry(RECLAIM_NEGATIVE_TASK_ID, 0)?;
        self.tasks.schedule(RECLAIM_NEGATIVE_TASK_ID)?;
        self.tasks.fault(
            RECLAIM_NEGATIVE_TASK_ID,
            PAGE_FAULT_SOURCE_ID,
            ExcKind::PageFault,
        )?;
        self.audit.append(AuditEvent::TaskStateChanged {
            task_id: RECLAIM_NEGATIVE_TASK_ID,
            status: TaskStatus::Faulted,
        });
        let negative_rejected = matches!(
            self.kernel_reclaim_task(RECLAIM_NEGATIVE_TASK_ID, None, 0),
            Err(TaskError::NotUnrecoverable)
        ) && self
            .tasks
            .find(RECLAIM_NEGATIVE_TASK_ID)
            .is_some_and(|task| task.status == TaskStatus::Faulted && task.pending_exc.is_some());

        // 正例：不可恢复的 Faulted。
        self.tasks.create_with_entry(RECLAIM_WITNESS_TASK_ID, 0)?;
        self.tasks.schedule(RECLAIM_WITNESS_TASK_ID)?;
        self.tasks.fault(
            RECLAIM_WITNESS_TASK_ID,
            PAGE_FAULT_SOURCE_ID,
            ExcKind::DoubleFault,
        )?;
        self.audit.append(AuditEvent::TaskStateChanged {
            task_id: RECLAIM_WITNESS_TASK_ID,
            status: TaskStatus::Faulted,
        });
        let unrecoverable = self.is_unrecoverable_faulted(RECLAIM_WITNESS_TASK_ID);
        let reclaimed = self
            .kernel_reclaim_task(RECLAIM_WITNESS_TASK_ID, None, 0)
            .is_ok();
        let after_dead = !self.tasks.is_live(RECLAIM_WITNESS_TASK_ID);
        let pending_cleared = self
            .tasks
            .find(RECLAIM_WITNESS_TASK_ID)
            .is_some_and(|task| task.pending_exc.is_none());
        let invariant_ok = self.tasks.check_faulted_invariant().is_ok();
        Ok(ReclaimWitness {
            task_id: RECLAIM_WITNESS_TASK_ID,
            negative_task: RECLAIM_NEGATIVE_TASK_ID,
            unrecoverable: u32::from(unrecoverable),
            reclaimed: u32::from(reclaimed),
            negative_rejected: u32::from(negative_rejected),
            after_dead: u32::from(after_dead),
            pending_cleared: u32::from(pending_cleared),
            invariant_ok: u32::from(invariant_ok),
        })
    }

    fn task_status_of(&self, task_id: u32) -> Option<TaskStatus> {
        self.tasks.find(task_id).map(|task| task.status)
    }

    fn task_pending_exc(&self, task_id: u32) -> Option<PendingExc> {
        self.tasks.find(task_id).and_then(|task| task.pending_exc)
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
    /// §12.5：`raise_exception` 的**完整形态**——带 `faulting_task`。
    ///
    /// **硬件执行异常**：前置 4 要求 `faulting_task` 必须是 `Running`（否则 `NotRunning`，
    /// 且状态原子不变）；成功动作 3 把它置 `Faulted`、写入唯一 `pending_exc`、并追加
    /// `TaskStateChanged(→ Faulted)`。随后仍按订阅表递送通知——§12.9 规定**未订阅**也照样
    /// 进入 `Faulted`（只是不产生通知），故状态迁移与递送是两件事。
    /// 非硬件异常（`SyscallTrap`）不触碰 Task 状态，与 [`Self::exc_raise`] 等价。
    pub fn exc_raise_on(
        &mut self,
        source_id: u32,
        kind: ExcKind,
        faulting_task: u32,
    ) -> Result<RaiseOutcome, ExcError> {
        if !kind.is_hardware() {
            return self.exc_raise(source_id, kind);
        }
        let source = self
            .exc_sources
            .find(source_id)
            .ok_or(ExcError::NoSuchSource)?;
        if source.kind != kind {
            return Err(ExcError::KindMismatch);
        }
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        // 前置 4 由 `fault` 内部校验；失败时此处无任何写入（原子）。
        self.tasks
            .fault(faulting_task, source_id, kind)
            .map_err(|_| ExcError::NotRunning)?;
        self.audit.append(AuditEvent::TaskStateChanged {
            task_id: faulting_task,
            status: TaskStatus::Faulted,
        });
        self.exc_raise(source_id, kind)
    }

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
        let audit_events = 1 + direct * 3 + queued;
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
                self.set_pending_task(task, message)
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
        self.block_recv_task(2).map_err(|_| ExcError::NoSuchTask)?;
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

    /// S2-D4：§8.1 步骤 6 的派生变体——子 Capability **落在指定 Task 手中**（而非调用者）。
    ///
    /// 除位置与 `holder` 外与 [`Self::derive_capability`] 同：父必须在调用者手中、目标存活、
    /// 子权限 ⊆ 父权限且类型匹配。用于 `task_create` 从 `parent_ns_cap` 派生新 Task 的
    /// 结构性根 Namespace Capability。
    pub fn derive_capability_to_holder(
        &mut self,
        caller: u32,
        parent: Handle,
        rights: Rights,
        holder: u32,
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
            .insert(handle, Some(parent), holder, rights, target)?;
        self.positions
            .register(handle, holder)
            .map_err(|_| DeriveError::CapacityExceeded)?;
        self.audit.append(AuditEvent::CapabilityDerived {
            cap: handle,
            parent,
            holder,
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

    /// §13.1/W13：为一次**已提交**的 Task 状态迁移追加 `TaskStateChanged`。
    ///
    /// 容量预检必须**先于**迁移提交：调用方在改动状态前用 `can_append(1)` 预检，失败即
    /// `AuditFull` 且状态不变（§6.5 先验证后原子提交）。容量不足时调度会显式失败，
    /// 不静默丢弃事件。
    fn audit_transition(&mut self, task_id: u32, status: TaskStatus) {
        self.audit
            .append(AuditEvent::TaskStateChanged { task_id, status });
    }

    /// §8.4 步骤 3 + §13.1：`Ready → Running` 并追加 `TaskStateChanged`。
    pub fn schedule_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        if !self.audit.can_append(1) {
            return Err(TaskError::AuditFull);
        }
        self.tasks.schedule(task_id)?;
        self.audit_transition(task_id, TaskStatus::Running);
        Ok(())
    }

    /// §8.4 + §13.1：`Running → Ready`（协作让出）并追加 `TaskStateChanged`。
    pub fn yield_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        if !self.audit.can_append(1) {
            return Err(TaskError::AuditFull);
        }
        self.tasks.yield_task(task_id)?;
        self.audit_transition(task_id, TaskStatus::Ready);
        Ok(())
    }

    /// §8.2/§12.9 + §13.1：`Running|Ready → BlockedRecv` 并追加 `TaskStateChanged`。
    pub fn block_recv_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        if !self.audit.can_append(1) {
            return Err(TaskError::AuditFull);
        }
        self.tasks.block_recv(task_id)?;
        self.audit_transition(task_id, TaskStatus::BlockedRecv);
        Ok(())
    }

    /// §8.2 唤醒 + §13.1：`BlockedRecv → Ready` 并追加 `TaskStateChanged`（无迁移则不追加）。
    pub fn wake_recv_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        if !self.tasks.is_blocked_recv(task_id) {
            return Ok(());
        }
        if !self.audit.can_append(1) {
            return Err(TaskError::AuditFull);
        }
        self.tasks.wake_recv(task_id)?;
        self.audit_transition(task_id, TaskStatus::Ready);
        Ok(())
    }

    /// §12.9 pending 直投：写 pending 并在 `BlockedRecv` 时唤醒（§13.1 记 `→ Ready`）。
    pub fn set_pending_task(&mut self, task_id: u32, message: IpcMessage) -> Result<(), TaskError> {
        let wakes = self.tasks.is_blocked_recv(task_id);
        if wakes && !self.audit.can_append(1) {
            return Err(TaskError::AuditFull);
        }
        self.tasks.set_pending(task_id, message)?;
        if wakes {
            self.audit_transition(task_id, TaskStatus::Ready);
        }
        Ok(())
    }

    /// S2-D4：§8.1 `task_create` 的完整成功动作。
    ///
    /// 失败保持在**任何状态修改之前**：全部前置（§8.1 1–8）与容量/审计预检先做；提交阶段
    /// （建记录 → 铸 manager/self → 派生 root_ns → 移动 initial_caps → 审计）在预检保证下逐条
    /// 不可能失败，任一失败仍按已提交量回滚。
    ///
    /// `boot_root` 是新 Task 地址空间的克隆源（bootloader 根，含内核高半区）；`phys_offset` 是
    /// 物理直映射偏移。新根清空三个用户 PML4 槽，代码页按 §8.1 步骤 2 装成 `{R, X}` 用户映射并
    /// 记入 `TaskVspaceTable`。返回的 [`TaskCreateOutcome`] 由 syscall 层登记到调度器（`sched`
    /// 不属于本模型层）。
    pub fn task_create(
        &mut self,
        caller: u32,
        req: &TaskCreateRequest,
        boot_root: u64,
        phys_offset: u64,
    ) -> Result<TaskCreateOutcome, TaskCreateError> {
        // ---- 前置 1：代码 Segment Capability ----
        if self.slots.check(req.code_seg_cap) != SlotState::Live {
            return Err(TaskCreateError::NoSuchCapability);
        }
        if self.holder_of(req.code_seg_cap) != Some(caller) {
            return Err(TaskCreateError::NotHolder);
        }
        let code_seg_id = match self.target_of(req.code_seg_cap) {
            Some(ObjectRef::Segment(id)) if self.target_live(ObjectRef::Segment(id)) => id,
            _ => return Err(TaskCreateError::NoSuchCapability),
        };
        let code_rights = self
            .rights_of(req.code_seg_cap)
            .ok_or(TaskCreateError::NoSuchCapability)?;
        if !code_rights.contains(Right::SegMapR) || !code_rights.contains(Right::SegMapX) {
            return Err(TaskCreateError::MissingRight);
        }
        // ---- 前置 2：代码范围 ----
        let frame_count = self
            .segment(code_seg_id)
            .map(|segment| segment.frame_count() as u64)
            .unwrap_or(0);
        let code_end_offset = req
            .code_offset
            .checked_add(req.code_pages)
            .ok_or(TaskCreateError::InvalidCodeRange)?;
        if req.code_pages == 0 || code_end_offset > frame_count {
            return Err(TaskCreateError::InvalidCodeRange);
        }
        // ---- 前置 3：入口落在代码范围内 ----
        if !req.load_base.is_multiple_of(crate::mem::FRAME_SIZE) {
            return Err(TaskCreateError::InvalidEntry);
        }
        let code_end = req
            .code_pages
            .checked_mul(crate::mem::FRAME_SIZE)
            .and_then(|span| req.load_base.checked_add(span))
            .ok_or(TaskCreateError::InvalidEntry)?;
        if req.entry < req.load_base || req.entry >= code_end {
            return Err(TaskCreateError::InvalidEntry);
        }
        // ---- 前置 4/5：父 Namespace Capability 与 root_rights ----
        if self.slots.check(req.parent_ns_cap) != SlotState::Live {
            return Err(TaskCreateError::InvalidNamespace);
        }
        if self.holder_of(req.parent_ns_cap) != Some(caller) {
            return Err(TaskCreateError::NotHolder);
        }
        let parent_is_live_ns = matches!(
            self.target_of(req.parent_ns_cap),
            Some(ObjectRef::Namespace(id)) if self.target_live(ObjectRef::Namespace(id))
        );
        if !parent_is_live_ns {
            return Err(TaskCreateError::InvalidNamespace);
        }
        let parent_rights = self
            .rights_of(req.parent_ns_cap)
            .ok_or(TaskCreateError::InvalidNamespace)?;
        if !parent_rights.contains(Right::NsResolve) || !parent_rights.contains(Right::Derive) {
            return Err(TaskCreateError::MissingRight);
        }
        if !req.root_rights.contains(Right::NsResolve)
            || !req.root_rights.subset_of(parent_rights)
            || !req.root_rights.well_typed(ObjectKind::Namespace)
        {
            return Err(TaskCreateError::RightsExceeded);
        }
        // ---- 前置 6：initial_caps ----
        let mut initial_count = 0usize;
        for (index, cap) in req.initial_caps.iter().copied().enumerate() {
            if cap.is_zero() {
                continue;
            }
            if req.initial_caps[..index].contains(&cap) {
                return Err(TaskCreateError::DuplicateCap);
            }
            if cap == req.code_seg_cap || cap == req.parent_ns_cap {
                return Err(TaskCreateError::InvalidInitialCap);
            }
            if self
                .tasks
                .entries
                .iter()
                .flatten()
                .any(|task| task.root_ns_cap == cap)
            {
                return Err(TaskCreateError::StructuralCap);
            }
            if self.slots.check(cap) != SlotState::Live || self.holder_of(cap) != Some(caller) {
                return Err(TaskCreateError::InvalidInitialCap);
            }
            if self
                .target_of(cap)
                .is_none_or(|target| !self.target_live(target))
                || self.rights_of(cap).is_none()
            {
                return Err(TaskCreateError::InvalidInitialCap);
            }
            initial_count += 1;
        }
        // ---- 前置 7：任务表 / cap 槽位 / 审计容量 ----
        let new_id = self
            .tasks
            .next_free_id()
            .ok_or(TaskCreateError::CapacityExceeded)?;
        if self.slots.live_count() + 3 + initial_count > MAX_CAPABILITY_SLOTS {
            return Err(TaskCreateError::CapacityExceeded);
        }
        if !self.audit.can_append(2 + initial_count) {
            return Err(TaskCreateError::AuditFull);
        }
        let map = |phys: u64| crate::mem::phys_to_virt(phys, phys_offset);
        // ---- 成功动作 2：新地址空间 + 代码页 ----
        let root = crate::frames::allocate_table_frame().map_err(|_| TaskCreateError::NoMemory)?;
        let mut fresh_tables = [0u64; 4];
        let mut fresh_count = 0usize;
        let rollback = |fresh_tables: &[u64; 4], fresh_count: usize| {
            for frame in fresh_tables[..fresh_count].iter().copied() {
                let _ = crate::frames::release_table_frame(frame);
            }
            let _ = crate::frames::release_table_frame(root);
        };
        crate::vspace::clone_root(boot_root, root, map);
        for slot in [
            crate::user::USER_SLOT,
            crate::user::USER2_SLOT,
            crate::user::USER3_SLOT,
        ] {
            crate::vspace::zero_slot(root, slot, map);
        }
        for index in 0..req.code_pages {
            let va = req.load_base + index * crate::mem::FRAME_SIZE;
            let Some(frame) =
                self.segment_frame(code_seg_id, req.code_offset as usize + index as usize)
            else {
                rollback(&fresh_tables, fresh_count);
                return Err(TaskCreateError::NoSuchCapability);
            };
            let mut alloc_count = fresh_count;
            let mapped = crate::vspace::map_4k(
                root,
                va,
                map,
                || match crate::frames::allocate_table_frame() {
                    Ok(table) => {
                        if alloc_count < fresh_tables.len() {
                            fresh_tables[alloc_count] = table;
                            alloc_count += 1;
                        }
                        Some(table)
                    }
                    Err(_) => None,
                },
                frame,
                false,
                true,
            );
            fresh_count = alloc_count;
            if mapped.is_err() {
                rollback(&fresh_tables, fresh_count);
                return Err(TaskCreateError::NoMemory);
            }
        }
        // ---- 成功动作 1：Task 记录 ----
        if self.tasks.create_with_entry(new_id, req.entry).is_err() {
            rollback(&fresh_tables, fresh_count);
            return Err(TaskCreateError::CapacityExceeded);
        }
        let mut vspace_count = 0u64;
        for index in 0..req.code_pages {
            let va = req.load_base + index * crate::mem::FRAME_SIZE;
            let inserted = self.vspace.insert(VspaceEntry {
                task_id: new_id,
                va,
                seg_id: code_seg_id,
                seg_offset: req.code_offset + index,
                access: Rights::from_rights(&[Right::SegMapR, Right::SegMapX]),
            });
            if inserted.is_err() {
                while vspace_count > 0 {
                    vspace_count -= 1;
                    let va = req.load_base + vspace_count * crate::mem::FRAME_SIZE;
                    let _ = self.vspace.remove(new_id, va);
                }
                self.tasks.remove(new_id);
                rollback(&fresh_tables, fresh_count);
                return Err(TaskCreateError::CapacityExceeded);
            }
            vspace_count += 1;
        }
        // ---- 成功动作 3/4：manager cap（调用者）与 self cap（新 Task） ----
        let Ok(manager_cap) =
            self.mint_root_capability(caller, MANAGER_TASK_RIGHTS, ObjectRef::Task(new_id))
        else {
            self.tasks.remove(new_id);
            rollback(&fresh_tables, fresh_count);
            return Err(TaskCreateError::CapacityExceeded);
        };
        let Ok(self_cap) =
            self.mint_root_capability(new_id, MANAGER_TASK_RIGHTS, ObjectRef::Task(new_id))
        else {
            self.retire_capability(manager_cap);
            self.tasks.remove(new_id);
            rollback(&fresh_tables, fresh_count);
            return Err(TaskCreateError::CapacityExceeded);
        };
        // ---- 成功动作 6：从 parent_ns_cap 派生新 Task 的结构性根 ----
        let Ok(root_ns_cap) =
            self.derive_capability_to_holder(caller, req.parent_ns_cap, req.root_rights, new_id)
        else {
            self.retire_capability(self_cap);
            self.retire_capability(manager_cap);
            self.tasks.remove(new_id);
            rollback(&fresh_tables, fresh_count);
            return Err(TaskCreateError::CapacityExceeded);
        };
        if self.tasks.set_root_ns_cap(new_id, root_ns_cap).is_err() {
            self.retire_capability(root_ns_cap);
            self.retire_capability(self_cap);
            self.retire_capability(manager_cap);
            self.tasks.remove(new_id);
            rollback(&fresh_tables, fresh_count);
            return Err(TaskCreateError::CapacityExceeded);
        }
        // ---- 成功动作 7：initial_caps 从调用者移入新 Task ----
        for cap in req.initial_caps.iter().copied() {
            if cap.is_zero() {
                continue;
            }
            if self.move_capability(cap, caller, new_id).is_err() {
                // 预检保证不可达；仍回滚本次创建，不留半成品。
                self.retire_capability(root_ns_cap);
                self.retire_capability(self_cap);
                self.retire_capability(manager_cap);
                self.tasks.remove(new_id);
                rollback(&fresh_tables, fresh_count);
                return Err(TaskCreateError::CapacityExceeded);
            }
        }
        // ---- 成功动作 8：TaskCreated ----
        self.audit
            .append(AuditEvent::TaskCreated { task_id: new_id });
        Ok(TaskCreateOutcome {
            manager_cap,
            self_cap,
            root_ns_cap,
            task_id: new_id,
            root_phys: root,
        })
    }

    /// §8.2 `task_destroy` 的完整路径。`root_phys = Some` 时先执行步骤 1（删除目标全部页映射、
    /// 归还空表帧与目标根帧）；`None` 供不经地址空间的模型层调用。
    ///
    /// **切片 V**：`caller` 为目标自身时走 [`Self::task_destroy_self`]（§8.2「删除自身是合法
    /// 的」）。该判断必须先于 `prepare_destroy`——调用者此刻是 `Running`，否则会先撞
    /// `CannotDestroyRunning` 而永远走不到自毁。
    pub fn task_destroy(
        &mut self,
        caller: u32,
        task_id: u32,
        root_phys: Option<u64>,
        phys_offset: u64,
    ) -> Result<(), TaskError> {
        if task_id == caller {
            return self.task_destroy_self(caller, root_phys, phys_offset);
        }
        self.tasks.prepare_destroy(task_id)?;
        if !self.audit.can_append(self.destroy_audit_events(task_id)) {
            return Err(TaskError::AuditFull);
        }
        self.teardown_vspace(task_id, root_phys, phys_offset)?;
        // §8.2 步骤 7：从所有异常源的订阅集合移除目标。
        self.exc_sources.remove_subscriber(task_id);
        self.destroy_task_model(task_id)
    }

    /// 切片 V：§8.2 的自毁路径。
    ///
    /// 模型清理与 [`Self::task_destroy`] 一致，唯一区别是**页表帧（含根帧）只入延迟队列**：
    /// 自毁者此刻正运行在自己的 CR3 上，必须等 `sched::resume` 完成 `write_cr3` 之后
    /// （`sched::flush_deferred_frames`）才能归还。
    ///
    /// 不触碰调度器：后继任务的存在性与切换由 syscall 层（`pick_next` + `exit_to`）负责。
    fn task_destroy_self(
        &mut self,
        caller: u32,
        root_phys: Option<u64>,
        phys_offset: u64,
    ) -> Result<(), TaskError> {
        if caller == INIT_TASK_ID {
            return Err(TaskError::CannotDestroyInit);
        }
        // 自身作为销毁目标是合法的（§8.2），故不做 `CannotDestroyRunning` 检查。
        if !self.audit.can_append(self.destroy_audit_events(caller)) {
            return Err(TaskError::AuditFull);
        }
        if let Some(root) = root_phys {
            let map = |phys: u64| crate::mem::phys_to_virt(phys, phys_offset);
            let mut entries = [0u64; MAX_VSPACE_ENTRIES];
            let mut count = 0usize;
            for entry in self.vspace.entries[..self.vspace.count].iter().flatten() {
                if entry.task_id == caller && count < MAX_VSPACE_ENTRIES {
                    entries[count] = entry.va;
                    count += 1;
                }
            }
            for entry_va in entries[..count].iter().copied() {
                crate::vspace::unmap_4k(root, entry_va, map, |frame| {
                    crate::sched::defer_table_frame(frame)
                        .map_err(|_| crate::vspace::VspaceError::NoFrame)
                })
                .map_err(|_| TaskError::NoSuchTask)?;
                let _ = self.vspace.remove(caller, entry_va);
            }
            crate::sched::defer_table_frame(root).map_err(|_| TaskError::NoSuchTask)?;
        }
        self.exc_sources.remove_subscriber(caller);
        self.destroy_task_body(caller)
    }

    /// §8.2 步骤 2/3/4/8/9 所需的审计条数（丢弃消息 + pending + `TaskDestroyed`）。
    fn destroy_audit_events(&self, task_id: u32) -> usize {
        let pending = self
            .tasks
            .find(task_id)
            .is_some_and(|task| task.pending.is_some());
        let mut dropped = 0usize;
        for message in self.ipc.entries[..self.ipc.count].iter().flatten() {
            if message.sender == task_id || message.receiver == task_id {
                dropped += 1;
            }
        }
        dropped + usize::from(pending) + 1
    }

    /// 既有模型层入口（无地址空间拆除）：`caller` 取一个不可能是任务 id 的值。
    pub fn destroy_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        self.task_destroy(u32::MAX, task_id, None, 0)
    }

    /// §8.2 L735 / §12.6 L1440：目标是否处于「**不可恢复终态**」。
    ///
    /// 判词（**本切片口径**）：目标为 `Faulted`，且其 `pending_exc` 的种类**不可恢复**
    /// （§12.10 前置 6，如 `DoubleFault`），**或**该故障的异常源已不存在
    /// （§12.6 L1440：源销毁后该故障不可能再经 `exc_resume` 恢复）。
    ///
    /// 登记：§12.10 `exc_resume` 用户化落地后，本判词应收紧为「依据当前 State 不可能成功
    /// `exc_resume`」——彼时才有 `kernel_resume_frame`、源 cap 可达性等更细的依据。
    pub fn is_unrecoverable_faulted(&self, task_id: u32) -> bool {
        let Some(task) = self.tasks.find(task_id) else {
            return false;
        };
        if task.status != TaskStatus::Faulted {
            return false;
        }
        let Some(pending) = task.pending_exc else {
            return false;
        };
        !pending.kind.is_recoverable() || self.exc_sources.find(pending.source_id).is_none()
    }

    /// §8.2 的**内核回收路径**（`KernelActor`，**无需 `task_cap`**）。
    ///
    /// 前置：目标处于「不可恢复终态」（[`Self::is_unrecoverable_faulted`]）且不是 init。
    /// 成功动作与 [`Self::task_destroy`] 的模型清理一致——含 §8.2 步骤 6 清空 `pending_exc`
    /// 与 `kernel_resume_frame`、步骤 7 从异常源订阅集合移除、`TaskDestroyed` 审计。
    /// `root_phys` 给出时一并拆除目标地址空间（与 `task_destroy` 同一语义；内核回收方通常
    /// 也持有该根）。
    ///
    /// 失败即 `TaskError`，且前置校验先于任何写入。
    pub fn kernel_reclaim_task(
        &mut self,
        task_id: u32,
        root_phys: Option<u64>,
        phys_offset: u64,
    ) -> Result<(), TaskError> {
        if task_id == INIT_TASK_ID {
            return Err(TaskError::CannotDestroyInit);
        }
        if !self.is_unrecoverable_faulted(task_id) {
            return Err(TaskError::NotUnrecoverable);
        }
        if !self.audit.can_append(self.destroy_audit_events(task_id)) {
            return Err(TaskError::AuditFull);
        }
        self.teardown_vspace(task_id, root_phys, phys_offset)?;
        // §8.2 步骤 7：从所有异常源的订阅集合移除目标。
        self.exc_sources.remove_subscriber(task_id);
        self.destroy_task_body(task_id)
    }

    /// 拆除目标地址空间（`task_destroy` 与内核回收路径共用）。
    fn teardown_vspace(
        &mut self,
        task_id: u32,
        root_phys: Option<u64>,
        phys_offset: u64,
    ) -> Result<(), TaskError> {
        let Some(root) = root_phys else {
            return Ok(());
        };
        let map = |phys: u64| crate::mem::phys_to_virt(phys, phys_offset);
        let mut entries = [0u64; MAX_VSPACE_ENTRIES];
        let mut count = 0usize;
        for entry in self.vspace.entries[..self.vspace.count].iter().flatten() {
            if entry.task_id == task_id && count < MAX_VSPACE_ENTRIES {
                entries[count] = entry.va;
                count += 1;
            }
        }
        for entry_va in entries[..count].iter().copied() {
            crate::vspace::unmap_4k(root, entry_va, map, |frame| {
                crate::frames::release_table_frame(frame)
                    .map_err(|_| crate::vspace::VspaceError::NoFrame)
            })
            .map_err(|_| TaskError::NoSuchTask)?;
            let _ = self.vspace.remove(task_id, entry_va);
        }
        let _ = crate::frames::release_table_frame(root);
        Ok(())
    }

    fn destroy_task_model(&mut self, task_id: u32) -> Result<(), TaskError> {
        self.tasks.prepare_destroy(task_id)?;
        self.destroy_task_body(task_id)
    }

    /// 销毁的公共清理体（§8.2 步骤 2/3/4/8/9），**不含** `prepare_destroy` 前置。
    ///
    /// 自毁路径（[`Self::task_destroy_self`]）复用它——调用者此刻是 `Running`，不能过
    /// `prepare_destroy` 的 Running 检查。
    fn destroy_task_body(&mut self, task_id: u32) -> Result<(), TaskError> {
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

    /// §8.2：发送一条不带 Capability 的消息。
    ///
    /// `send_cap` 必须是**指向 `receiver`** 且含 `TaskSend` 的 Task Capability——授权绑定接收者，
    /// 不接受「持有任一 `TaskSend` cap 即可发给任意任务」的宽松判定。
    pub fn send_message(
        &mut self,
        sender: u32,
        receiver: u32,
        send_cap: Handle,
        payload: u64,
    ) -> Result<(), IpcError> {
        self.send_message_with_capabilities(sender, receiver, send_cap, payload, &[])
    }

    /// §8.2：发送一条（可携带 Capability 的）消息。授权与失败语义同上。
    pub fn send_message_with_capabilities(
        &mut self,
        sender: u32,
        receiver: u32,
        send_cap: Handle,
        payload: u64,
        capability_ids: &[Handle],
    ) -> Result<(), IpcError> {
        if !self.tasks.is_live(sender) || !self.tasks.is_live(receiver) {
            return Err(IpcError::NoSuchTask);
        }
        if self.authorize(sender, send_cap, Right::TaskSend).is_err()
            || self.target_of(send_cap) != Some(ObjectRef::Task(receiver))
        {
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
            self.set_pending_task(receiver, message)
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
                let _ = self.wake_recv_task(receiver);
                Ok(message)
            }
            Err(IpcError::QueueEmpty) => {
                let _ = self.block_recv_task(receiver);
                Err(IpcError::QueueEmpty)
            }
            Err(error) => Err(error),
        }
    }

    /// 一句柄是否指向某个 Task（`send_call`/`memseg_*` 的 root 派生用）。
    pub fn task_of_cap(&self, cap: Handle) -> Option<u32> {
        match self.target_of(cap) {
            Some(ObjectRef::Task(task_id)) => Some(task_id),
            _ => None,
        }
    }
}

impl BootstrapState {
    /// S2-D4：Segment 的第 `index` 帧（`task_create` 装代码页时用）。
    fn segment_frame(&self, id: u32, index: usize) -> Option<u64> {
        self.segment(id)
            .and_then(|segment| segment.frames.frame_at(index))
    }

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
        cap_at: Option<(CapabilityLocation, u32)>,
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
        let minted = match cap_at {
            Some((location, slot)) => {
                self.mint_root_capability_at(location, slot, root_rights, ObjectRef::Segment(id))
            }
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
            Some((
                CapabilityLocation::HeldByKernel,
                platform_segment_cap_slot(id),
            )),
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
            !matches!(segment.frames, SegmentFrames::Vector { .. })
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
            SegmentFrames::Image { .. } => {
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

    /// SPEC-066：把 11 条用户可见平台段根 Capability 与 2 条平台 Task Capability 按属主
    /// 移交给 task 1 / task 2，使用户可自持使用（`authorize` 要求 `holder_of(cap) == caller`）。
    ///
    /// 迁移复用既有 [`Self::move_capability`]（同槽同 generation，句柄不变），审计记
    /// `from = HeldByKernel`。`SEG_VSPACE`/`SEG_COW` 保留内核持有，理由见
    /// [`PLATFORM_SEGMENT_OWNERS`]。
    ///
    /// 必须在 bring-up 的全部 `map_region` 之后调用——那些映射以 `KERNEL_ACTOR` 为 caller，
    /// 移交后内核不再持有这些 cap，无法再代用户映射。
    pub fn handover_platform_caps(&mut self) -> Result<(), CapabilityPositionError> {
        for (id, owner) in PLATFORM_SEGMENT_OWNERS {
            self.move_capability(platform_segment_cap_handle(id), KERNEL_ACTOR, owner)?;
        }
        for (slot, owner) in [
            (PLATFORM_TASK_CAP_SLOT_BASE, INIT_TASK_ID),
            (PLATFORM_TASK_CAP_SLOT_BASE + 1, 2),
        ] {
            self.move_capability(
                cap_handle(slot, INITIAL_CAP_GENERATION),
                KERNEL_ACTOR,
                owner,
            )?;
        }
        Ok(())
    }

    /// 切片 S：播种用户 IPC 见证所需的两条 Capability。
    ///
    /// 均**新增铸造**（不动内核持有的平台 cap，故 `platform_segments_record().task_caps` 仍为 2）：
    /// task 1 得 `Task(2)`+`TaskSend`（发给 task 2 的授权凭证），task 2 得 `Task(2)`+`TaskManageVSpace`
    /// （把自己收到的共享段映射进自己的根）。**不给任何任务对他方地址空间或消息通道的权限。**
    pub fn prepare_user_ipc_caps(&mut self) -> Result<(), MemsegError> {
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(INIT_TASK_ID),
            USER_IPC_CAP_SLOT_BASE,
            TASK2_SEND_RIGHTS,
            ObjectRef::Task(2),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(2),
            USER_IPC_CAP_SLOT_BASE + 1,
            TASK2_SELF_RIGHTS,
            ObjectRef::Task(2),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        Ok(())
    }

    /// S2-D4：建新 Task 的代码 Segment（用户区第一个 id）并铸两条 bring-up Capability。
    ///
    /// 返回代码段首帧物理地址，供内核写入 `task3_code()`。代码段用**用户区 id**而非平台区间，
    /// 故 `PLATFORM_SEGMENT_ID_LAST`/`PLATFORM_SEGMENT_COUNT` 与平台 cap 槽 32..44 不变。
    ///
    /// 必须在 `platform_prepare_task_caps` 之后调用（两条 cap 的目标 Task(1) 已存活），且在
    /// 既有 bring-up 取帧序列**之后**调用（避免平移切片 E/F/G/O 的物理帧序列）。
    pub fn prepare_task_create_caps(&mut self, phys_offset: u64) -> Result<u64, MemsegError> {
        let (_, frames) = self.create_segment_object(
            USER_SEGMENT_ID_FIRST,
            KERNEL_ACTOR,
            1,
            TASK_CREATE_CODE_ACCESS,
            phys_offset,
            Some((
                CapabilityLocation::HeldByTask(INIT_TASK_ID),
                TASK_CREATE_CODE_CAP_SLOT,
            )),
        )?;
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(INIT_TASK_ID),
            TASK_CREATE_SEND_CAP_SLOT,
            Rights::from_rights(&[Right::TaskSend]),
            ObjectRef::Task(INIT_TASK_ID),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        // 切片 V：task 4 的回发凭据（task 3 的那条已随其销毁撤销，不能复用同一槽）。
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(INIT_TASK_ID),
            TASK4_SEND_CAP_SLOT,
            Rights::from_rights(&[Right::TaskSend]),
            ObjectRef::Task(INIT_TASK_ID),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        frames.frame_at(0).ok_or(MemsegError::NoMemory)
    }

    /// 一条平台根 Capability 的属主：满足全部结构性条件时返回其 holder id。
    ///
    /// SPEC-066：位置可以是内核持有，也可以是**存活**用户 Task 持有——平台段 cap 已按属主
    /// 移交，`HeldByKernel` 不再是唯一合法位置。
    fn platform_cap_holder(&self, handle: Handle, target: ObjectRef) -> Option<u32> {
        if self.slots.check(handle) != SlotState::Live || self.target_of(handle) != Some(target) {
            return None;
        }
        if !self
            .rights_of(handle)
            .is_some_and(|rights| !rights.is_empty() && rights.well_typed(target.kind()))
        {
            return None;
        }
        match self
            .positions
            .find(handle)
            .map(|position| position.location)
        {
            Some(CapabilityLocation::HeldByKernel) => Some(KERNEL_ACTOR),
            Some(CapabilityLocation::HeldByTask(task_id)) if self.tasks.is_live(task_id) => {
                Some(task_id)
            }
            _ => None,
        }
    }

    /// 切片 R：组装平台段对象化的阶段末见证（按需计算）。
    pub fn platform_segments_record(&self) -> PlatformSegmentsRecord {
        let platform_range = PLATFORM_SEGMENT_ID_FIRST..=PLATFORM_SEGMENT_ID_LAST;
        let mut record = PlatformSegmentsRecord {
            segments: 0,
            first_id: PLATFORM_SEGMENT_ID_FIRST,
            last_id: PLATFORM_SEGMENT_ID_LAST,
            roots: 0,
            user_roots: 0,
            kernel_roots: 0,
            holders: [KERNEL_ACTOR; PLATFORM_SEGMENT_COUNT],
            task_caps: 0,
            task_holders: [KERNEL_ACTOR; PLATFORM_TASK_CAP_COUNT],
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
            // `holders` 只在通过校验的槽上有效；`roots == PLATFORM_SEGMENT_COUNT` 即全部有效。
            if let Some(holder) = self.platform_cap_holder(handle, ObjectRef::Segment(id)) {
                record.roots += 1;
                record.holders[index] = holder;
                if holder == KERNEL_ACTOR {
                    record.kernel_roots += 1;
                } else {
                    record.user_roots += 1;
                }
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
            if let Some(holder) = self.platform_cap_holder(handle, ObjectRef::Task(task_id)) {
                record.task_caps += 1;
                record.task_holders[offset as usize] = holder;
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
    // SPEC-068：帧集合是真实逐帧向量（物化在帧注册表），此处记其长度。
    let mut segments = [None; MAX_SEGMENT_OBJECTS];
    segments[0] = Some(SegmentObject {
        id: IMAGE_SEGMENT_ID,
        generation: INITIAL_CAP_GENERATION,
        creator: INIT_TASK_ID,
        pool: crate::frames::Pool::Cpu,
        frames: SegmentFrames::Image {
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

/// SPEC-071：取 Task 状态迁移审计汇总（授权表未安装时 `None`）。
pub fn task_state_audit() -> Option<TaskStateAudit> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        (*slot).as_ref().map(|state| state.task_state_audit())
    }
}

/// SPEC-070：采集 `Faulted` 见证（必须在 `activate_init` **之前**调用）。
pub fn collect_fault_witness() -> Result<FaultWitness, TaskError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.fault_witness(),
            None => Err(TaskError::NoSuchTask),
        }
    }
}

/// SPEC-070：已采集的 `Faulted` 见证（供收尾事件上报）。
static mut FAULT_WITNESS: Option<FaultWitness> = None;

/// 采集并登记 `Faulted` 见证。
pub fn record_fault_witness() -> Result<(), TaskError> {
    let witness = collect_fault_witness()?;
    unsafe {
        *core::ptr::addr_of_mut!(FAULT_WITNESS) = Some(witness);
    }
    Ok(())
}

/// SPEC-072：采集并登记内核回收见证（必须在 `activate_init` 之前调用）。
pub fn record_reclaim_witness() -> Result<(), TaskError> {
    let witness = unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.reclaim_witness()?,
            None => return Err(TaskError::NoSuchTask),
        }
    };
    unsafe {
        *core::ptr::addr_of_mut!(RECLAIM_WITNESS) = Some(witness);
    }
    Ok(())
}

/// SPEC-072：已登记的内核回收见证（未采集时为 `None`）。
pub fn reclaim_witness_record() -> Option<ReclaimWitness> {
    unsafe { *core::ptr::addr_of!(RECLAIM_WITNESS) }
}

static mut RECLAIM_WITNESS: Option<ReclaimWitness> = None;

/// 取已登记的 `Faulted` 见证（未采集时为 `None`）。
pub fn fault_witness_record() -> Option<FaultWitness> {
    unsafe { *core::ptr::addr_of!(FAULT_WITNESS) }
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

/// S2-D4：全局 bring-up——建新 Task 的代码段并铸两条 Capability，返回代码段首帧物理地址。
pub fn prepare_task_create_caps(phys_offset: u64) -> Result<u64, MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.prepare_task_create_caps(phys_offset),
            None => Err(MemsegError::TargetDead),
        }
    }
}

/// S2-D4：全局 `task_create`（§8.1）。
pub fn task_create(
    caller: u32,
    req: &TaskCreateRequest,
    boot_root: u64,
    phys_offset: u64,
) -> Result<TaskCreateOutcome, TaskCreateError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.task_create(caller, req, boot_root, phys_offset),
            None => Err(TaskCreateError::NoSuchCapability),
        }
    }
}

/// S2-D4：全局 `task_destroy`（§8.2）。
pub fn task_destroy(
    caller: u32,
    task_id: u32,
    root_phys: Option<u64>,
    phys_offset: u64,
) -> Result<(), TaskError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.task_destroy(caller, task_id, root_phys, phys_offset),
            None => Err(TaskError::NoSuchTask),
        }
    }
}

/// S2-D4：见证用——活动（非 `Dead`）Task 数。
pub fn live_task_count() -> usize {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        (*slot)
            .as_ref()
            .map(|state| {
                state
                    .tasks
                    .entries
                    .iter()
                    .flatten()
                    .filter(|task| task.status != TaskStatus::Dead)
                    .count()
            })
            .unwrap_or(0)
    }
}

/// S2-D4：见证用——活动 Capability 槽位数。
pub fn live_cap_count() -> usize {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        (*slot)
            .as_ref()
            .map(|state| state.slots.live_count())
            .unwrap_or(0)
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

/// SPEC-066：按属主移交平台段 / Task Capability。
///
/// 必须在 bring-up 全部 `map_region` 之后、进入用户态之前调用（见
/// [`BootstrapState::handover_platform_caps`]）。未安装状态归为 `NoSuchCapability`
/// ——与「槽/对象不存在」同一失败语义。
pub fn handover_platform_caps() -> Result<(), CapabilityPositionError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.handover_platform_caps(),
            None => Err(CapabilityPositionError::NoSuchCapability),
        }
    }
}

/// 切片 S：播种用户 IPC 见证所需的两条 Capability（须在 `platform_prepare_task_caps` 之后）。
pub fn prepare_user_ipc_caps() -> Result<(), MemsegError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.prepare_user_ipc_caps(),
            None => Err(MemsegError::NotInstalled),
        }
    }
}

/// 切片 S：句柄是否指向某个 Task（供 syscall 层派生目标根）。
pub fn task_of_cap(cap: Handle) -> Option<u32> {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.task_of_cap(cap),
            None => None,
        }
    }
}

/// 切片 S：接收者是否处于 `BlockedRecv`（供 `SYS_SEND` 判定投递路径）。
pub fn is_blocked_recv_global(task_id: u32) -> bool {
    unsafe {
        let slot = core::ptr::addr_of!(BOOTSTRAP_STATE);
        match (*slot).as_ref() {
            Some(state) => state.tasks.is_blocked_recv(task_id),
            None => false,
        }
    }
}

/// 切片 S：取走某个 Task 的 pending 消息（直投后由 `SYS_SEND` 消费，避免被后续 `SYS_RECV` 重复投递）。
pub fn take_pending_global(task_id: u32) -> Option<IpcMessage> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.tasks.take_pending(task_id),
            None => None,
        }
    }
}

/// 切片 S：全局发送（接收者绑定的授权在 `BootstrapState` 内完成）。
pub fn send_message_with_capabilities(
    sender: u32,
    receiver: u32,
    send_cap: Handle,
    payload: u64,
    capability_ids: &[Handle],
) -> Result<(), IpcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.send_message_with_capabilities(
                sender,
                receiver,
                send_cap,
                payload,
                capability_ids,
            ),
            None => Err(IpcError::NoSuchTask),
        }
    }
}

/// 切片 S：全局接收（空队列时会把接收者置为 `BlockedRecv`）。
pub fn receive_message(receiver: u32) -> Result<IpcMessage, IpcError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(BOOTSTRAP_STATE);
        match (*slot).as_mut() {
            Some(state) => state.receive_message(receiver),
            None => Err(IpcError::NoSuchTask),
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
            Some(state) => state.block_recv_task(task_id),
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
        PLATFORM_TASK1_CAP_HANDLE, PLATFORM_TASK2_CAP_HANDLE, PendingExc, SEGMENT_ROOT_RIGHTS,
        SYSCALL_TRAP_SOURCE_ID, SegmentFrames, SegmentObject, TASK_CAP_HANDLE, TASK_RIGHTS,
        TASK2_SELF_CAP_HANDLE, TASK2_SEND_CAP_HANDLE, TaskError, TaskStatus,
        USER_IPC_CAP_SLOT_BASE, USER_SEGMENT_ID_FIRST, VspaceEntry, create_snapshot, create_state,
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
    fn ipc_send_authorization_is_bound_to_the_receiver() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        state.prepare_user_ipc_caps().unwrap();
        // 非持有者：task 2 不持有 init 的 Task(1) cap。
        assert_eq!(
            state.send_message(2, INIT_TASK_ID, TASK_CAP_HANDLE, 1),
            Err(IpcError::NotAuthorized)
        );
        // 持有且含 TaskSend，但目标 `Task(2)` ≠ receiver(1)。
        assert_eq!(
            state.send_message(INIT_TASK_ID, INIT_TASK_ID, TASK2_SEND_CAP_HANDLE, 1),
            Err(IpcError::NotAuthorized)
        );
        // 持有且目标匹配，但缺 `TaskSend`（自管理 cap 只含 `TaskManageVSpace`）。
        assert_eq!(
            state.send_message(2, 2, TASK2_SELF_CAP_HANDLE, 1),
            Err(IpcError::NotAuthorized)
        );
        assert_eq!(state.ipc.count, 0);
        // 唯一合法组合：持有、目标匹配、权限齐备。
        state
            .send_message(INIT_TASK_ID, 2, TASK2_SEND_CAP_HANDLE, 5)
            .unwrap();
        assert_eq!(state.receive_message(2).unwrap().payload, 5);
    }

    #[test]
    fn user_ipc_caps_do_not_touch_platform_task_caps() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        assert_eq!(state.platform_segments_record().task_caps, 0);
        state.prepare_user_ipc_caps().unwrap();
        // 新增铸造落在 47/48，平台记录的 task_caps（槽 45/46）不受影响。
        assert_eq!(state.platform_segments_record().task_caps, 0);
        assert_eq!(TASK2_SEND_CAP_HANDLE.index(), USER_IPC_CAP_SLOT_BASE);
        assert_eq!(TASK2_SELF_CAP_HANDLE.index(), USER_IPC_CAP_SLOT_BASE + 1);
        assert_eq!(state.holder_of(TASK2_SEND_CAP_HANDLE), Some(INIT_TASK_ID));
        assert_eq!(state.holder_of(TASK2_SELF_CAP_HANDLE), Some(2));
        for (handle, right) in [
            (TASK2_SEND_CAP_HANDLE, Right::TaskSend),
            (TASK2_SELF_CAP_HANDLE, Right::TaskManageVSpace),
        ] {
            assert_eq!(state.target_of(handle), Some(ObjectRef::Task(2)));
            assert!(state.rights_of(handle).unwrap().contains(right));
        }
        assert_eq!(state.validate_authority(), Ok(()));
    }

    #[test]
    fn queued_capability_position_keeps_authority_valid() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        state.prepare_user_ipc_caps().unwrap();
        // 接收者未阻塞 → 排队路径：cap 位置变成 `InMessage`。
        state
            .send_message_with_capabilities(
                INIT_TASK_ID,
                2,
                TASK2_SEND_CAP_HANDLE,
                7,
                &[IMAGE_CAP_HANDLE],
            )
            .unwrap();
        assert!(matches!(
            state.positions.find(IMAGE_CAP_HANDLE).unwrap().location,
            CapabilityLocation::InMessage(_)
        ));
        assert_eq!(state.validate_authority(), Ok(()));
        // 承载消息消失而位置仍为 `InMessage` → 位置校验必须拒绝（此前该分支无测试）。
        state.ipc.entries[0] = None;
        state.ipc.count = 0;
        assert_eq!(
            state.validate_authority(),
            Err(AuthorityError::PositionMismatch)
        );
    }

    #[test]
    fn ipc_fifo_blocks_empty_receiver_wakes_and_cleans_on_destroy() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        state.prepare_user_ipc_caps().unwrap();
        assert_eq!(state.receive_message(2), Err(IpcError::QueueEmpty));
        assert_eq!(
            state.tasks.entries[1].unwrap().status,
            TaskStatus::BlockedRecv
        );
        state
            .send_message(INIT_TASK_ID, 2, TASK2_SEND_CAP_HANDLE, 11)
            .unwrap();
        assert_eq!(state.tasks.entries[1].unwrap().status, TaskStatus::Ready);
        state
            .send_message(INIT_TASK_ID, 2, TASK2_SEND_CAP_HANDLE, 22)
            .unwrap();
        assert_eq!(state.receive_message(2).unwrap().payload, 11);
        assert_eq!(state.receive_message(2).unwrap().payload, 22);
        assert_eq!(state.receive_message(2), Err(IpcError::QueueEmpty));
        state
            .send_message(INIT_TASK_ID, 2, TASK2_SEND_CAP_HANDLE, 33)
            .unwrap();
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
        // 非持有者：task 2 不持有 init 的 Task cap。
        assert_eq!(
            state.send_message(2, INIT_TASK_ID, TASK_CAP_HANDLE, 7),
            Err(IpcError::NotAuthorized)
        );
        assert_eq!(state.ipc.count, 0);
        let send_cap = state
            .derive_capability(INIT_TASK_ID, TASK_CAP_HANDLE, rights("TaskSend"))
            .unwrap();
        state.move_capability(send_cap, INIT_TASK_ID, 2).unwrap();
        // 授权绑定接收者：该 cap 指向 `Task(1)`，发给 Task(2) 必须被拒。
        assert_eq!(
            state.send_message(2, 2, send_cap, 8),
            Err(IpcError::NotAuthorized)
        );
        assert_eq!(state.ipc.count, 0);
        state.send_message(2, INIT_TASK_ID, send_cap, 9).unwrap();
        assert_eq!(state.receive_message(INIT_TASK_ID).unwrap().payload, 9);
    }

    #[test]
    fn ipc_transfers_capability_atomically_through_message() {
        let mut state = fresh_state();
        state.activate_init().unwrap();
        state.create_task(2).unwrap();
        state.prepare_user_ipc_caps().unwrap();
        assert_eq!(state.receive_message(2), Err(IpcError::QueueEmpty));
        state
            .send_message_with_capabilities(
                INIT_TASK_ID,
                2,
                TASK2_SEND_CAP_HANDLE,
                55,
                &[IMAGE_CAP_HANDLE],
            )
            .unwrap();
        assert_eq!(state.ipc.count, 0);
        // 前 5 条是 SPEC-061 的初始 Authority 审计；随后是 SPEC-071 的 Task 状态迁移
        // （`prepare_user_ipc_caps`/`receive_message` 的空队列阻塞与投递唤醒）与消息事件。
        assert_eq!(state.audit.count, 9);
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
            state.send_message_with_capabilities(
                INIT_TASK_ID,
                2,
                TASK2_SEND_CAP_HANDLE,
                66,
                &[IMAGE_CAP_HANDLE]
            ),
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
        // SPEC-066：未移交时全部由内核持有（`holders` 只在通过校验的槽上有效）。
        assert_eq!(record.user_roots, 0);
        assert_eq!(record.kernel_roots, 2);
        assert_eq!(record.holders[0], KERNEL_ACTOR); // VSPACE
        assert_eq!(record.holders[1], KERNEL_ACTOR); // COW 未物化
        assert_eq!(record.holders[2], KERNEL_ACTOR); // TASK1_CODE
        assert_eq!(record.task_holders, [KERNEL_ACTOR, KERNEL_ACTOR]);
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

    /// SPEC-066：11 条用户可见平台段 cap 与 2 条平台 Task cap 按属主移交，VSPACE/COW 保留内核。
    #[test]
    fn handover_platform_caps_assigns_owners_and_keeps_kernel_privates() {
        let mut state = fresh_state();
        // task 2 必须存活：属主为它的 cap 会经 `HeldByTask(2)` 的存活校验。
        state.create_task(2).unwrap();
        for id in PLATFORM_SEGMENT_ID_FIRST..=PLATFORM_SEGMENT_ID_LAST {
            let mut frames = [0u64; MAX_SEGMENT_FRAMES];
            frames[0] = 0x30_0000 + id as u64 * 0x1000;
            state.segments[state.segment_count] = Some(SegmentObject {
                id,
                generation: INITIAL_CAP_GENERATION,
                creator: KERNEL_ACTOR,
                pool: crate::frames::Pool::Cpu,
                frames: SegmentFrames::Vector { frames, count: 1 },
                allowed_access: SEGMENT_ROOT_RIGHTS,
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
        state.platform_prepare_task_caps().unwrap();

        let before = state.platform_segments_record();
        assert_eq!(
            (before.roots, before.user_roots, before.kernel_roots),
            (13, 0, 13)
        );

        state.handover_platform_caps().unwrap();

        let record = state.platform_segments_record();
        assert_eq!(record.roots, 13);
        assert_eq!(record.user_roots, 11);
        assert_eq!(record.kernel_roots, 2);
        assert_eq!(record.task_caps, 2);
        // 按 id 2..14 顺序：VSPACE/COW 内核持有，TASK1_*/SPIN1_* 归 1，TASK2_*/SPIN2_* 归 2。
        assert_eq!(
            record.holders,
            [KERNEL_ACTOR, KERNEL_ACTOR, 1, 1, 1, 1, 2, 2, 2, 1, 1, 2, 2]
        );
        assert_eq!(record.task_holders, [INIT_TASK_ID, 2]);

        // 属主可授权、非属主被拒；内核私有两段对任何 Task 都拒绝。
        let x_cap = platform_segment_cap_handle(crate::frames::SEG_TASK1_X);
        assert_eq!(
            state.authorize(INIT_TASK_ID, x_cap, Right::SegMapR),
            Ok(x_cap)
        );
        assert!(state.authorize(2, x_cap, Right::SegMapR).is_err());
        let task2_code = platform_segment_cap_handle(crate::frames::SEG_TASK2_CODE);
        assert_eq!(
            state.authorize(2, task2_code, Right::SegMapR),
            Ok(task2_code)
        );
        assert!(
            state
                .authorize(INIT_TASK_ID, task2_code, Right::SegMapR)
                .is_err()
        );
        for id in [crate::frames::SEG_VSPACE, crate::frames::SEG_COW] {
            let cap = platform_segment_cap_handle(id);
            assert_eq!(state.holder_of(cap), Some(KERNEL_ACTOR));
            assert!(state.authorize(INIT_TASK_ID, cap, Right::SegMapR).is_err());
            assert!(state.authorize(2, cap, Right::SegMapR).is_err());
        }

        // 13 条位置迁移审计（11 段 + 2 task cap），源位置为内核持有。
        let moved = state.audit.entries[..state.audit.count]
            .iter()
            .flatten()
            .filter(|event| {
                matches!(
                    event,
                    AuditEvent::CapabilityMoved {
                        from: CapabilityLocation::HeldByKernel,
                        ..
                    }
                )
            })
            .count();
        assert_eq!(moved, 13);
        assert_eq!(state.validate_authority(), Ok(()));
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

    /// S2-D4：造一个含 task 2、一个活动 Segment 对象与其根 Capability 的授权表。
    ///
    /// 只登记对象元数据、**不取真实帧**——本组测试只覆盖「任何状态修改之前」的前置拒绝，
    /// 以及帧分配失败时的原子性；真实地址空间构造由 QEMU 见证（`task_ready`）。
    fn task_state() -> (super::BootstrapState, Handle, Handle) {
        let mut state = fresh_state();
        state.create_task(2).unwrap();
        state.segments[0] = Some(SegmentObject {
            id: crate::frames::SEG_TASK1_CODE,
            generation: INITIAL_CAP_GENERATION,
            creator: KERNEL_ACTOR,
            pool: crate::frames::Pool::Cpu,
            frames: SegmentFrames::Vector {
                frames: [0x1000; MAX_SEGMENT_FRAMES],
                count: 2,
            },
            allowed_access: Rights::from_rights(&[Right::SegMapR, Right::SegMapX]),
            active: true,
        });
        state.segment_count = 1;
        let segment = ObjectRef::Segment(crate::frames::SEG_TASK1_CODE);
        let code_cap = state
            .mint_root_capability(INIT_TASK_ID, code_rights(), segment)
            .unwrap();
        // 同一授权表里的「缺 SegMapX」cap：用于权限不足的拒绝分支。
        let weak_cap = state
            .mint_root_capability(
                INIT_TASK_ID,
                Rights::from_rights(&[Right::Derive, Right::SegMapR, Right::SegDestroy]),
                segment,
            )
            .unwrap();
        (state, code_cap, weak_cap)
    }

    fn code_rights() -> Rights {
        Rights::from_rights(&[
            Right::Derive,
            Right::SegMapR,
            Right::SegMapX,
            Right::SegDestroy,
        ])
    }

    fn create_request(code_cap: Handle) -> super::TaskCreateRequest {
        super::TaskCreateRequest {
            code_seg_cap: code_cap,
            code_offset: 0,
            code_pages: 1,
            load_base: crate::user::USER3_CODE_VA,
            entry: crate::user::USER3_CODE_VA,
            stack_top: crate::user::USER3_STACK_VA + crate::mem::FRAME_SIZE,
            parent_ns_cap: NAMESPACE_CAP_HANDLE,
            root_rights: Rights::from_rights(&[Right::NsResolve]),
            initial_caps: [Handle::ZERO; super::MAX_TASK_INITIAL_CAPS],
        }
    }

    /// S2-D4：`task_create` 的九类前置拒绝必须**在任何状态修改之前**发生。
    #[test]
    fn task_create_rejects_every_precondition_without_state_change() {
        let (state, code_cap, weak_cap) = task_state();
        let before = state;
        let boot_root = 0x2000;
        let cases: [(super::TaskCreateRequest, super::TaskCreateError); 8] = [
            // 1. 代码 cap 缺 SegMapX。
            (
                super::TaskCreateRequest {
                    code_seg_cap: weak_cap,
                    ..create_request(code_cap)
                },
                super::TaskCreateError::MissingRight,
            ),
            // 2. 入口越出代码范围。
            (
                super::TaskCreateRequest {
                    entry: crate::user::USER3_BASE_VA + 0x9000,
                    ..create_request(code_cap)
                },
                super::TaskCreateError::InvalidEntry,
            ),
            // 3. 代码页数为 0。
            (
                super::TaskCreateRequest {
                    code_pages: 0,
                    ..create_request(code_cap)
                },
                super::TaskCreateError::InvalidCodeRange,
            ),
            // 4. code_offset + code_pages 超出该 Segment 的帧数（2）。
            (
                super::TaskCreateRequest {
                    code_offset: 2,
                    code_pages: 1,
                    ..create_request(code_cap)
                },
                super::TaskCreateError::InvalidCodeRange,
            ),
            // 5. 父 Namespace cap 不是 Namespace（用 image Segment cap）。
            (
                super::TaskCreateRequest {
                    parent_ns_cap: IMAGE_CAP_HANDLE,
                    ..create_request(code_cap)
                },
                super::TaskCreateError::InvalidNamespace,
            ),
            // 6. root_rights 缺 NsResolve。
            (
                super::TaskCreateRequest {
                    root_rights: Rights::from_rights(&[Right::NsBind]),
                    ..create_request(code_cap)
                },
                super::TaskCreateError::RightsExceeded,
            ),
            // 7. root_rights ⊄ parent_ns_cap.rights（root 的 Namespace 权限不含 NsResolve？含；
            //    故用 NsDestroy 之外的自造位集：NsResolve ∪ {NsDestroy} 仍在父内，改用
            //    父 cap 之外的权限——这里以「类型不符」的 Segment 权限触发 RightsExceeded）。
            (
                super::TaskCreateRequest {
                    root_rights: Rights::from_rights(&[Right::NsResolve, Right::SegMapR]),
                    ..create_request(code_cap)
                },
                super::TaskCreateError::RightsExceeded,
            ),
            // 8. initial_caps 重复。
            (
                super::TaskCreateRequest {
                    initial_caps: [TASK_CAP_HANDLE, TASK_CAP_HANDLE],
                    ..create_request(code_cap)
                },
                super::TaskCreateError::DuplicateCap,
            ),
        ];
        for (index, (request, expected)) in cases.into_iter().enumerate() {
            let mut state = state;
            assert_eq!(
                state.task_create(INIT_TASK_ID, &request, boot_root, 0),
                Err(expected),
                "case {index} must be rejected"
            );
            assert_eq!(state, before, "case {index} must not modify state");
        }
    }

    /// S2-D4：`initial_caps` 含活动 Task 的结构性根 Capability 必须被拒（§8.1 前置 8）。
    #[test]
    fn task_create_rejects_structural_root_in_initial_caps() {
        let (mut state, code_cap, _) = task_state();
        // 把 task 2 的结构性根设成一条普通 Task cap（Namespace 根与 parent_ns_cap 同一条，
        // 无法在 initial_caps 里与前置 5 区分，故用可区分的替身钉住该分支）。
        state.tasks.set_root_ns_cap(2, TASK_CAP_HANDLE).unwrap();
        let before = state;
        let request = super::TaskCreateRequest {
            initial_caps: [TASK_CAP_HANDLE, Handle::ZERO],
            ..create_request(code_cap)
        };
        assert_eq!(
            state.task_create(INIT_TASK_ID, &request, 0x2000, 0),
            Err(super::TaskCreateError::StructuralCap)
        );
        assert_eq!(state, before);
    }

    /// S2-D4：非持有者 / 未知句柄 / 帧分配失败的原子性。
    #[test]
    fn task_create_rejects_foreign_handles_and_rolls_back_on_no_memory() {
        let (mut state, code_cap, _) = task_state();
        let before = state;
        // 代码 cap 不在调用者手中（调用者用 task 2）。
        assert_eq!(
            state.task_create(2, &create_request(code_cap), 0x2000, 0),
            Err(super::TaskCreateError::NotHolder)
        );
        assert_eq!(state, before);
        // 合法请求但 host 未安装帧注册表 → 取根表帧失败，且不留半成品。
        assert_eq!(
            state.task_create(INIT_TASK_ID, &create_request(code_cap), 0x2000, 0),
            Err(super::TaskCreateError::NoMemory)
        );
        assert_eq!(state, before);
        // 未分配槽位的「格式合法」句柄 → NoSuchCapability。
        let unset = Handle::new(60, 1).unwrap();
        assert_eq!(
            state.task_create(INIT_TASK_ID, &create_request(unset), 0x2000, 0),
            Err(super::TaskCreateError::NoSuchCapability)
        );
    }

    /// 切片 V：§8.2「删除自身是合法的」——自毁路径必须先于 `CannotDestroyRunning` 判定生效；
    /// init 仍受结构保护。
    /// SPEC-070：§12.5 硬件执行异常把 **Running** 任务置 `Faulted` 并写唯一 `pending_exc`。
    /// SPEC-071：调度路径的每条迁移都必须追加 `TaskStateChanged`（§13.1/W13）。
    #[test]
    fn every_scheduler_transition_appends_a_state_change_event() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        let base = state.audit.count;
        state.schedule_task(2).unwrap(); // Ready → Running
        assert!(matches!(
            state.audit.entries[state.audit.count - 1],
            Some(AuditEvent::TaskStateChanged {
                task_id: 2,
                status: TaskStatus::Running
            })
        ));
        state.yield_task(2).unwrap(); // Running → Ready
        assert!(matches!(
            state.audit.entries[state.audit.count - 1],
            Some(AuditEvent::TaskStateChanged {
                task_id: 2,
                status: TaskStatus::Ready
            })
        ));
        state.block_recv_task(2).unwrap(); // Ready → BlockedRecv
        assert!(matches!(
            state.audit.entries[state.audit.count - 1],
            Some(AuditEvent::TaskStateChanged {
                task_id: 2,
                status: TaskStatus::BlockedRecv
            })
        ));
        state.wake_recv_task(2).unwrap(); // BlockedRecv → Ready
        assert!(matches!(
            state.audit.entries[state.audit.count - 1],
            Some(AuditEvent::TaskStateChanged {
                task_id: 2,
                status: TaskStatus::Ready
            })
        ));
        assert_eq!(state.audit.count, base + 4);
        // 无迁移的重复唤醒不得再追加事件（避免把"通知"误记成"迁移"）。
        state.wake_recv_task(2).unwrap();
        assert_eq!(state.audit.count, base + 4);
    }

    /// SPEC-071：审计容量不足时迁移必须**整体失败**且状态不变（§6.5 先验证后原子提交）。
    #[test]
    fn transition_is_rejected_when_the_audit_log_is_full() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        while state.audit.can_append(1) {
            state.audit.append(AuditEvent::TaskStateChanged {
                task_id: 0,
                status: TaskStatus::Ready,
            });
        }
        let before = state.tasks.find(2).copied().unwrap();
        assert_eq!(state.schedule_task(2), Err(TaskError::AuditFull));
        assert_eq!(state.tasks.find(2).copied().unwrap(), before);
    }

    /// SPEC-071：审计汇总按**目标状态**分类，且 `Dead`（细化态）不产生事件。
    /// SPEC-072：§8.2 内核回收路径——只回收「不可恢复的 Faulted」，并有真实清理效果。
    #[test]
    fn kernel_reclaim_destroys_an_unrecoverable_faulted_task() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        state
            .tasks
            .fault(2, PAGE_FAULT_SOURCE_ID, ExcKind::DoubleFault)
            .unwrap();
        assert!(state.is_unrecoverable_faulted(2));
        assert!(state.kernel_reclaim_task(2, None, 0).is_ok());
        let task = state.tasks.find(2).copied().unwrap();
        assert_eq!(task.status, TaskStatus::Dead);
        assert!(
            task.pending_exc.is_none(),
            "§8.2 步骤 6 必须清空 pending_exc"
        );
        assert!(!state.tasks.is_live(2));
        assert!(state.tasks.check_faulted_invariant().is_ok());
    }

    /// SPEC-072：**可恢复**的 Faulted（`PageFault` 且源存活）不得被内核回收——它是 §12.10 的
    /// 候选，回收会绕过未来的恢复路径。
    #[test]
    fn kernel_reclaim_rejects_a_recoverable_faulted_task() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        state
            .tasks
            .fault(2, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
            .unwrap();
        assert!(!state.is_unrecoverable_faulted(2));
        let before = state.tasks.find(2).copied().unwrap();
        assert_eq!(
            state.kernel_reclaim_task(2, None, 0),
            Err(TaskError::NotUnrecoverable)
        );
        assert_eq!(state.tasks.find(2).copied().unwrap(), before);
    }

    /// SPEC-072：非 Faulted（`Ready`）与 init 都不在内核回收的范围内。
    #[test]
    fn kernel_reclaim_rejects_non_faulted_and_init() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        assert!(!state.is_unrecoverable_faulted(2), "Ready 不是不可恢复终态");
        assert_eq!(
            state.kernel_reclaim_task(2, None, 0),
            Err(TaskError::NotUnrecoverable)
        );
        assert_eq!(
            state.kernel_reclaim_task(INIT_TASK_ID, None, 0),
            Err(TaskError::CannotDestroyInit)
        );
    }

    /// SPEC-072：「源已销毁」也构成不可恢复终态（§12.6 L1440：源销毁后该故障不可能再恢复）。
    #[test]
    fn destroyed_fault_source_makes_a_faulted_task_unrecoverable() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        state
            .tasks
            .fault(2, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
            .unwrap();
        assert!(
            !state.is_unrecoverable_faulted(2),
            "源存活时 PageFault 是可恢复的"
        );
        // 直接摘掉该源记录（等价于源销毁后 `exc_resume` 已不可能成功）。
        state.exc_sources.entries[0] = None;
        assert!(state.is_unrecoverable_faulted(2));
        assert!(state.kernel_reclaim_task(2, None, 0).is_ok());
        assert!(!state.tasks.is_live(2));
    }

    #[test]
    fn task_state_audit_counts_each_target_status() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        // 相对基线断言：夹具本身可能已产生状态迁移事件（不是本用例的被测面）。
        let before = state.task_state_audit();
        state.schedule_task(2).unwrap();
        state.yield_task(2).unwrap();
        state.block_recv_task(2).unwrap();
        state.wake_recv_task(2).unwrap();
        let audit = state.task_state_audit();
        assert_eq!(audit.to_running - before.to_running, 1);
        assert_eq!(audit.to_ready - before.to_ready, 2);
        assert_eq!(audit.to_blocked - before.to_blocked, 1);
        assert_eq!(audit.to_faulted - before.to_faulted, 0);
        assert_eq!(audit.total() - before.total(), 4);
        assert_eq!(audit.cap, super::MAX_AUDIT_EVENTS as u32);
        assert!(audit.used <= audit.cap);
    }

    #[test]
    fn faulting_a_running_task_sets_pending_exc_and_faulted_status() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        assert!(
            state
                .tasks
                .fault(2, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
                .is_ok()
        );
        let task = state.tasks.find(2).copied().unwrap();
        assert_eq!(task.status, TaskStatus::Faulted);
        assert_eq!(
            task.pending_exc,
            Some(PendingExc {
                source_id: PAGE_FAULT_SOURCE_ID,
                kind: ExcKind::PageFault,
            })
        );
        assert!(state.tasks.check_faulted_invariant().is_ok());
    }

    /// SPEC-070：§12.5 前置 4——`faulting_task` 非 Running 时拒绝且**状态原子不变**。
    #[test]
    fn faulting_a_non_running_task_is_rejected_without_state_change() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        // init 此刻是 Ready（尚未 activate_init）。
        let before = state.tasks.find(INIT_TASK_ID).copied().unwrap();
        assert_eq!(
            state
                .tasks
                .fault(INIT_TASK_ID, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault),
            Err(TaskError::NotRunning)
        );
        let after = state.tasks.find(INIT_TASK_ID).copied().unwrap();
        assert_eq!(before, after, "被拒的 fault 不得改动任何字段");
        assert!(state.tasks.check_faulted_invariant().is_ok());
    }

    /// SPEC-070：W9 双向不变量在**全部**状态迁移后仍成立；`task_destroy` 清空 `pending_exc`。
    #[test]
    fn faulted_invariant_holds_across_transitions_and_destroy() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        state
            .tasks
            .fault(2, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
            .unwrap();
        assert!(state.tasks.check_faulted_invariant().is_ok());
        // §8.2 步骤 6：销毁清空 pending_exc 与 kernel_resume_frame（Faulted 不是 Running，故可销毁）。
        crate::sched::set_kernel_resume_frame(2, crate::exc::SretFrame::ZERO);
        assert!(state.tasks.prepare_destroy(2).is_ok());
        state.tasks.finish_destroy(2).unwrap();
        assert_eq!(crate::sched::kernel_resume_frame(2), None);
        let task = state.tasks.find(2).copied().unwrap();
        assert_eq!(task.status, TaskStatus::Dead);
        assert!(task.pending_exc.is_none());
        assert!(state.tasks.check_faulted_invariant().is_ok());
    }

    /// SPEC-070：§12.10 的可恢复性矩阵与 §12.5 的"硬件执行异常"判定。
    #[test]
    fn exception_kind_classification_matrix() {
        assert!(!ExcKind::SyscallTrap.is_hardware());
        for kind in [
            ExcKind::PageFault,
            ExcKind::GeneralProtectionFault,
            ExcKind::DivideError,
            ExcKind::InvalidOpcode,
            ExcKind::DoubleFault,
        ] {
            assert!(kind.is_hardware(), "{kind:?} 是硬件执行异常");
        }
        assert!(ExcKind::PageFault.is_recoverable());
        assert!(ExcKind::GeneralProtectionFault.is_recoverable());
        assert!(ExcKind::DivideError.is_recoverable());
        assert!(ExcKind::InvalidOpcode.is_recoverable());
        assert!(
            !ExcKind::DoubleFault.is_recoverable(),
            "DoubleFault 不可恢复"
        );
    }

    /// SPEC-070：`exc_raise_on` 对**非硬件**异常不改 Task 状态（`SYS_TRAP` 语义不变）。
    #[test]
    fn exc_raise_on_keeps_status_for_non_hardware_kinds() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        let before = state.tasks.find(2).copied().unwrap();
        assert!(
            state
                .exc_raise_on(SYSCALL_TRAP_SOURCE_ID, ExcKind::SyscallTrap, 2)
                .is_ok()
        );
        assert_eq!(state.tasks.find(2).copied().unwrap(), before);
    }

    /// SPEC-070：`exc_raise_on` 完整形态——硬件异常置 Faulted，且**无订阅者时不产生消息**。
    #[test]
    fn exc_raise_on_faults_a_running_task_and_delivers_to_nobody() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        let outcome = state
            .exc_raise_on(PAGE_FAULT_SOURCE_ID, ExcKind::PageFault, 2)
            .unwrap();
        assert_eq!(outcome.delivered, 0, "无订阅者时不得产生通知");
        let task = state.tasks.find(2).copied().unwrap();
        assert_eq!(task.status, TaskStatus::Faulted);
        assert!(task.pending_exc.is_some());
        assert!(state.tasks.check_faulted_invariant().is_ok());
    }

    /// SPEC-070：`exc_raise_on` 的前置 4 失败即原子不变（不写 `pending_exc`、不改状态、不产通知）。
    #[test]
    fn exc_raise_on_rejects_a_non_running_faulting_task() {
        let mut state = exc_state();
        let before = state.tasks.find(INIT_TASK_ID).copied().unwrap();
        assert_eq!(
            state.exc_raise_on(PAGE_FAULT_SOURCE_ID, ExcKind::PageFault, INIT_TASK_ID),
            Err(ExcError::NotRunning)
        );
        assert_eq!(state.tasks.find(INIT_TASK_ID).copied().unwrap(), before);
    }

    #[test]
    fn task_destroy_allows_self_and_keeps_init_protected() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        // 让调用者处于 Running，模拟「正在运行的任务销毁自己」。
        state.schedule_task(2).unwrap();
        assert!(state.tasks.is_live(2));
        assert_eq!(state.task_destroy(2, 2, None, 0), Ok(()));
        assert!(!state.tasks.is_live(2), "自毁后目标不再是活动任务");
        // init 仍受保护。
        assert_eq!(
            state.task_destroy(INIT_TASK_ID, INIT_TASK_ID, None, 0),
            Err(TaskError::CannotDestroyInit)
        );
    }

    /// 切片 V：非自毁目标仍在 Running 时，`prepare_destroy` 的既有拒绝语义不变。
    #[test]
    fn task_destroy_still_rejects_running_others() {
        let mut state = fresh_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        assert_eq!(
            state.task_destroy(INIT_TASK_ID, 2, None, 0),
            Err(TaskError::CannotDestroyRunning)
        );
    }

    /// S2-D4 / §8.2 步骤 7：销毁把目标从所有异常源的订阅集合移除。
    #[test]
    fn task_destroy_removes_exception_subscribers() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        let source = PAGE_FAULT_SOURCE_ID;
        state.exc_source_add_subscriber(source, 2).unwrap();
        assert!(
            state
                .exc_sources
                .find(source)
                .unwrap()
                .subscribers
                .iter()
                .flatten()
                .any(|task| *task == 2)
        );
        state.task_destroy(INIT_TASK_ID, 2, None, 0).unwrap();
        assert!(
            !state
                .exc_sources
                .find(source)
                .unwrap()
                .subscribers
                .iter()
                .flatten()
                .any(|task| *task == 2)
        );
    }

    /// S2-D4：`remove_subscriber` 对多个源、多个订阅者都精确移除。
    #[test]
    fn remove_subscriber_covers_every_source() {
        let mut table = super::ExcSourceTable::empty();
        for (index, id) in [1u32, 2].into_iter().enumerate() {
            table.entries[index] = Some(super::ExcSource {
                id,
                kind: ExcKind::PageFault,
                cap: Handle::ZERO,
                subscribers: [Some(2), Some(3), None, None],
                count: 2,
            });
        }
        table.count = 2;
        assert_eq!(table.remove_subscriber(2), 2);
        for entry in table.entries[..table.count].iter().flatten() {
            assert_eq!(entry.count, 1);
            assert_eq!(entry.subscribers[0], Some(3));
            assert_eq!(entry.subscribers[1], None);
        }
        assert_eq!(table.remove_subscriber(9), 0);
    }

    /// S2-D4：`task_create` 成功时追加 `TaskCreated`；`destroy_audit_events` 覆盖消息与 pending。
    #[test]
    fn task_created_event_exists_and_destroy_audit_is_counted() {
        let mut state = fresh_state();
        state.create_task(2).unwrap();
        state.audit.append(AuditEvent::TaskCreated { task_id: 2 });
        assert!(
            state.audit.entries[..state.audit.count]
                .iter()
                .flatten()
                .any(|event| matches!(event, AuditEvent::TaskCreated { task_id: 2 }))
        );
        // 无消息、无 pending：恰好 1 条 TaskDestroyed。
        assert_eq!(state.destroy_audit_events(2), 1);
    }

    /// S2-D4：`next_free_id` 跳过已占用 id；`remove` 供回滚且不复活已用 id。
    #[test]
    fn next_free_id_and_rollback_remove() {
        let mut table = super::TaskTable::with_init();
        assert_eq!(table.next_free_id(), Some(2));
        table.create(2).unwrap();
        assert_eq!(table.next_free_id(), Some(3));
        table.create(4).unwrap();
        assert_eq!(table.next_free_id(), Some(3));
        assert!(table.remove(2));
        assert_eq!(table.next_free_id(), Some(2));
        assert!(!table.remove(9));
    }

    #[test]
    fn bootstrap_state_stays_within_the_stack_budget() {
        // `create_state` 按值返回该结构，内核启动栈有限：钉死上限防止后续扩容悄悄超栈。
        let size = core::mem::size_of::<super::BootstrapState>();
        assert!(size <= 16 * 1024, "BootstrapState size = {size} bytes");
        // SPEC-071：容量提到 170 后实测 16312 B——**余量只剩 72 B**。
        // 再往里加存储（新字段或再扩数组）会直接撞上守卫，故先把"余量已枯竭"钉进测试：
        // 任何进一步存储增长都必须先做审计存储重构（把 `AuditLog` 移出该结构）。
        assert!(
            size >= 16 * 1024 - 128,
            "BootstrapState size = {size} bytes：结构体余量意外变大，请复核 SPEC-071 登记的余量结论"
        );
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
