use core::fmt;
use core::num::NonZeroU32;

use crate::handle::Handle;
use crate::mem::FrameInventory;
use crate::namespace::{MAX_PATH_COMPONENTS, NsEntry, NsEntryKind, Path};
use crate::rights::{ObjectKind, Right, Rights};
use prima_ostd::sync::StaticCell;

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
///
/// SPEC-094：boot 期峰值已达 **62**，`task_create` 前置（`live_count + 3 + initial_count
/// ≤ MAX`）在给 `ALPHA` 端点加一条独立栈段 cap 后需要 ≥ 68 的余量——原值 64 会令 boot 自测的
/// `task_create` 失败，调度器随后在 init 的 `SYS_YIELD` 上 `pick_next` 无 Ready 任务而 panic
/// （实测 `syscall.rs` `yield_call`）。按项目既有先例（SPEC-074 审计、SPEC-091 vspace：容量墙
/// → 扩展并**照实重测结构体守卫**）把上限提到 **72**。守卫（≤13 KiB）不变，见
/// `bootstrap_state_stays_within_the_stack_budget`。
pub const MAX_CAPABILITY_SLOTS: usize = 72;
/// 兼容别名：历史上该常量用于 positions 表容量，现等于槽位数。
const MAX_CAPABILITY_POSITIONS: usize = MAX_CAPABILITY_SLOTS;
/// 任务表容量（含已保留槽位与已销毁记录）。
///
/// SPEC-087：普通 id 空间 `2..8` 被保留的 Dead（3/4）与 Faulted（5/6/7）记录占满，
/// `next_free_id` 返回 `None`，故上调至 12 使存活的 Rust 客户端拿到 id 8。
/// SPEC-088：`user::USER_TASK_KSTACK_COUNT` 覆盖 id 3..9；若将来分配 id ≥ 10，
/// `register_created_task` 会确定性失败（`NoSpace`）而非带着空内核栈调度。
const MAX_TASKS: usize = 12;
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
/// SPEC-074：容量不再受 `BootstrapState` 守卫约束（存储已移出结构体），故提到 256 以留足余量。
/// 2026-10-13：256 在 boot 期已贴上限（实测补事件即 `AuditFull` 级联），提到 512 为
/// `ISSUE-EXC-011` 的 Task Capability 审计补事件留出余量（仍不占 `BootstrapState`）。
const MAX_AUDIT_EVENTS: usize = 512;

/// SPEC-074：审计 arena 按**实例**划分的区段数。
///
/// 内核只构造一个 `BootstrapState`，故只需 1 段；host 单测并行构造多个 state（每次 `empty()`
/// 领一段），需要足够多段才不至于耗尽。段数与容量都**不影响 `BootstrapState` 尺寸**。
#[cfg(not(test))]
const AUDIT_REGIONS: usize = 1;
#[cfg(test)]
const AUDIT_REGIONS: usize = 512;

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

/// SPEC-070：`Faulted` 见证 Task 的**保留槽位** id（普通 id 从 2 起分配）。
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
    ///
    /// SPEC-083：与 `ObjectCreated` 对称携带 `segment` 标志——`ObjectRef` 的 id 空间按类型
    /// 独立（image `Segment(1)` 与 init `Namespace(1)` 同 id），`audit(cap)` 按目标对象过滤
    /// 必须用「id + 类型」双条件，否则跨类型混计。
    ObjectDestroyed {
        object_id: u32,
        segment: bool,
    },
    /// 切片 P：Task 状态迁移（W13 要求每次迁移有事件）。
    TaskStateChanged {
        task_id: u32,
        status: TaskStatus,
    },
    /// SPEC-073 / §12.10 成功动作 4：一次成功的 `exc_resume`（§13.1 的唯一例外——
    /// `Faulted → Ready` 以本事件记录，而非 `TaskStateChanged`）。
    TaskResumed {
        task_id: u32,
    },
    /// S2-D4：`task_create` 成功（§8.1 步骤 8）。
    TaskCreated {
        task_id: u32,
    },
    /// SPEC-081 / §13.1 `CapMinted`：随对象创建铸造根 Capability（`parent = None`）。
    ///
    /// `bootstrap` 期的三条初始根仍由 [`InitialCapability`] 表示、**不**补发本事件（它们不是
    /// 「随对象创建铸造」的运行时路径）；本事件用于 `ns_create` 等运行时铸造。
    CapabilityMinted {
        cap: Handle,
        holder: u32,
    },
    /// SPEC-081 / §11.3：`ns_bind` 成功。
    NsBound {
        ns: u32,
        path: Path,
        cap: Handle,
    },
    /// SPEC-081 / §11.4：`ns_unbind` 成功。
    NsUnbound {
        ns: u32,
        path: Path,
        cap: Handle,
    },
    /// SPEC-081 / §11.5：`ns_mount` 成功（挂载子 Capability 是新派生的）。
    NsMounted {
        ns: u32,
        path: Path,
        source_ns: u32,
        cap: Handle,
    },
    /// SPEC-081 / §11.6：`ns_unmount` 成功（含 stale Mount 的清理）。
    NsUnmounted {
        ns: u32,
        path: Path,
        cap: Handle,
    },
    /// SPEC-081 / §11.7：`ns_resolve` 成功。
    ///
    /// §13.1 要求它记录：起始 Namespace Capability、规范化路径、依次经过的 Mount Capability、
    /// 最终父 Capability 与新 Capability——本事件的 `start_cap`/`path`/`mounts`/`parent`/`cap`
    /// 逐项对应。
    NsResolved {
        start_cap: Handle,
        path: Path,
        mount_count: u8,
        mounts: [Handle; MAX_PATH_COMPONENTS],
        parent: Handle,
        cap: Handle,
    },
    /// SPEC-082 / §10.2 成功动作 3：`timer_set`（无论随后是 `Pending` 还是立即 `Fired`）。
    TimerSet {
        timer_id: u32,
        creator: u32,
        deadline: Instant,
    },
    /// SPEC-082 / §10.2/§10.3/§10.6：Timer 进入 `Fired`（每次进入恰一条——exactly-once 的一半）。
    TimerFired {
        timer_id: u32,
        deadline: Instant,
        fired_at: Instant,
    },
    /// SPEC-082 / §10.4/§10.5：Timer 进入 `Cancelled`（`Pending` 的显式取消或 recipient 销毁）。
    TimerCancelled {
        timer_id: u32,
        reason: TimerCancelReason,
    },
}

#[derive(Clone, Copy)]
pub struct AuditLog {
    /// 本实例在 [`AUDIT_ARENA`] 中的区段起点（按段划分，实例之间不重叠）。
    base: usize,
    /// 已追加条数（区段内的偏移由 `base + count` 给出）。
    count: usize,
}

impl core::fmt::Debug for AuditLog {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AuditLog")
            .field("base", &self.base)
            .field("count", &self.count)
            .finish()
    }
}

impl PartialEq for AuditLog {
    fn eq(&self, other: &Self) -> bool {
        self.count == other.count
    }
}

impl Eq for AuditLog {}

/// SPEC-074：审计条目的**存储**——与 `BootstrapState` 解耦。
///
/// 移出结构体的理由是尺寸：`BootstrapState` 已贴到 16 KiB 守卫（余 72 B），而审计日志是其中
/// 最大的一块（170 条 × 24 B = 4080 B）。移到静态 arena 后，结构体降到约 **12.2 KiB**，
/// 且容量/段数都不再占结构体空间——后续切片可自由扩容。
///
/// 段由 `AUDIT_CURSOR` 原子分配，故 host 单测并行构造的多个 state **各有独立区段**，互不干扰。
static AUDIT_ARENA: StaticCell<[Option<AuditEvent>; MAX_AUDIT_EVENTS * AUDIT_REGIONS]> =
    StaticCell::new([None; MAX_AUDIT_EVENTS * AUDIT_REGIONS]);
static AUDIT_CURSOR: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

impl AuditLog {
    /// SPEC-074：在 arena 里为本实例领一段并返回句柄。
    ///
    /// 段耗尽即确定性 panic（不静默复用他人区段）——单测里段数足够，内核只用 1 段。
    fn claim() -> Self {
        use core::sync::atomic::Ordering;
        let base = AUDIT_CURSOR.fetch_add(MAX_AUDIT_EVENTS, Ordering::Relaxed);
        assert!(
            base + MAX_AUDIT_EVENTS <= MAX_AUDIT_EVENTS * AUDIT_REGIONS,
            "audit arena exhausted: 提高 AUDIT_REGIONS（它不占 BootstrapState 空间）"
        );
        Self { base, count: 0 }
    }

    fn can_append(&self, count: usize) -> bool {
        self.count.saturating_add(count) <= MAX_AUDIT_EVENTS
    }

    fn append(&mut self, event: AuditEvent) {
        let index = self.base + self.count;
        AUDIT_ARENA.with_mut(|arena| arena[index] = Some(event));
        self.count += 1;
    }

    /// 第 `index` 条的**副本**（存储不在结构体内，故不暴露内部引用）。
    fn entry(&self, index: usize) -> Option<AuditEvent> {
        if index >= self.count {
            return None;
        }
        AUDIT_ARENA.with(|arena| arena[self.base + index])
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.count
    }

    /// 是否存在满足谓词的条目（同样避免借出 arena 内部引用）。
    #[cfg(test)]
    fn any(&self, mut pred: impl FnMut(&AuditEvent) -> bool) -> bool {
        for index in 0..self.count {
            if self.entry(index).is_some_and(|event| pred(&event)) {
                return true;
            }
        }
        false
    }

    /// 按序遍历已追加条目（值拷贝，避免借出 arena 内部）。
    fn for_each(&self, mut visit: impl FnMut(&AuditEvent)) {
        for index in 0..self.count {
            if let Some(event) = self.entry(index) {
                visit(&event);
            }
        }
    }
}

/// §13.2：Capability 祖先链的最大长度（受 Capability 槽数上界约束，保证回溯必然终止）。
pub const MAX_TRACE_CHAIN: usize = MAX_CAPABILITY_SLOTS;

/// §13.2：一条 Capability 的不可变祖先链与相关事件摘要（`audit(cap)` 的返回值）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditTrace {
    pub chain: [Handle; MAX_TRACE_CHAIN],
    pub chain_len: usize,
    pub minted: u32,
    pub derived: u32,
    pub moves: u32,
    pub created: u32,
    pub destroyed: u32,
    pub target_live: bool,
    /// 单节点实现不表示跨节点代理根（`origin = None` 恒真）。
    pub origin_none: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditError {
    InvalidCaller,
    InvalidCap,
    NotHeld,
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
    /// SPEC-101：细化层字段——服务管理器在创建时**运行期指派**给新 Task 的服务名（0 = 不指派）。
    /// 抽象模型 §5.2 的 Task 无此字段；细化层把它按值写入新任务初始帧 `rcx`（第 4 个入口参数），
    /// 使端点的身份是运行期数据而非编译进二进制的常量。
    pub name: u64,
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

    /// §12.10 成功动作 2/5：清除 `pending_exc` 并把目标从 `Faulted` 置为 `Ready`。
    fn resume_faulted(&mut self, task_id: u32) -> Result<(), TaskError> {
        let task = self.find_mut(task_id).ok_or(TaskError::NoSuchTask)?;
        task.pending_exc = None;
        task.status = TaskStatus::Ready;
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
    /// SPEC-082 / §5.5：定时器对象。
    Timer(u32),
}

impl ObjectRef {
    pub const fn kind(self) -> ObjectKind {
        match self {
            ObjectRef::Segment(_) => ObjectKind::Segment,
            ObjectRef::Task(_) => ObjectKind::Task,
            ObjectRef::Namespace(_) => ObjectKind::Namespace,
            ObjectRef::ExcSource(_) => ObjectKind::ExcSource,
            ObjectRef::Timer(_) => ObjectKind::Timer,
        }
    }

    /// 目标对象的 id。各类型的 id 空间**独立**（如 image `Segment(1)` 与 init
    /// `Namespace(1)` 同为 1），故按目标过滤审计事件时必须与 [`Self::kind`] 合用。
    pub const fn id(self) -> u32 {
        match self {
            Self::Segment(id)
            | Self::Task(id)
            | Self::Namespace(id)
            | Self::ExcSource(id)
            | Self::Timer(id) => id,
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
    /// SPEC-081 / §4.3：位于 Namespace `ns` 的某条路径上。
    ///
    /// 只编码 `ns` id——具体路径由 [`NsEntry`] 反查、并由 `Ns*` 审计事件显式记录（见设计文档
    /// §0.1 第 2 条：把 69 B 的 `Path` 塞进每条位置会无谓放大按槽直索引的位置表）。
    InNamespace(u32),
}

impl CapabilityLocation {
    /// 位置对应的持有者 id（`InMessage` 与 `InNamespace` 都没有 Task 持有者）。
    ///
    /// `InNamespace` 返回 `None` 是**有意**的：§4.5/§4.6 要求「移动只在规范列出的原子操作内
    /// 发生」，于是 `authorize`/`derive`/`drop` 的「必须由 caller 持有」前置对它自然给出
    /// `NotHeld`/`NotHolder`，不需要各处再加特例。
    pub const fn holder_id(self) -> Option<u32> {
        match self {
            CapabilityLocation::HeldByTask(task_id) => Some(task_id),
            CapabilityLocation::HeldByKernel => Some(KERNEL_ACTOR),
            CapabilityLocation::InMessage(_) => None,
            CapabilityLocation::InNamespace(_) => None,
        }
    }

    /// 位置所在的 Namespace（仅 [`CapabilityLocation::InNamespace`] 返回 `Some`）。
    pub const fn namespace_id(self) -> Option<u32> {
        match self {
            CapabilityLocation::InNamespace(ns) => Some(ns),
            _ => None,
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

    /// 按槽索引查找。**越界索引返回 `None` 而非 panic**——`Handle` 只是编码，槽索引上界由本表
    /// 决定；对不可信 ring3 传入的畸形句柄（如 `0xffff_ffff_ffff_ffea`），此路径必须安全失败。
    pub fn find(&self, handle: Handle) -> Option<&CapabilityRecord> {
        self.entries
            .get(handle.index() as usize)
            .and_then(Option::as_ref)
    }

    pub fn insert(
        &mut self,
        handle: Handle,
        parent: Option<Handle>,
        holder: u32,
        rights: Rights,
        target: ObjectRef,
    ) -> Result<(), DeriveError> {
        let index = handle.index() as usize;
        if index >= MAX_CAPABILITY_SLOTS {
            return Err(DeriveError::MalformedHandle);
        }
        if self.entries[index].is_some() {
            return Err(DeriveError::CapacityExceeded);
        }
        self.entries[index] = Some(CapabilityRecord {
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
        let index = handle.index() as usize;
        if index >= MAX_CAPABILITY_SLOTS {
            return false;
        }
        let slot = &mut self.entries[index];
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

/// SPEC-073：§12.10 `exc_resume` 的失败原因（对齐规范错误名）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExcResumeError {
    NoSuchTask,
    /// 前置 5：调用者（细化口径：故障任务本身）必须处于 `Faulted` 且有 `pending_exc`。
    NotFaulted,
    /// 前置 6：`pending_exc.kind` 不可恢复（`DoubleFault` 等）——保持 `Faulted`。
    UnrecoverableFault,
    /// 前置 8：调用者的 `kernel_resume_frame` 必须为 `None`。
    FramePending,
    /// 帧未通过 `validate_for_kernel_resume`。
    InvalidFrame,
    /// 帧偏移越界（超出内核持有的恢复帧缓冲）。
    OutOfBounds,
    /// Audit 容量不足。
    AuditFull,
}

/// SPEC-073：§12.10 的 `sret_frame_seg_cap` 所指存储（**细化口径**）。
///
/// POSIR 侧该参数是「持有恢复现场的 Segment Capability」；本细化层由 **KernelActor 持有的
/// 静态缓冲**承担（与 SPEC-066 的 `HeldByKernel` 同一取向：内核私有、不铸用户可持 cap）。
/// §12.10 成功动作 1「清零 Segment 中 `[frame_offset, frame_offset + 0xA0)` 的已消费 frame
/// bytes」在此等价为清零该缓冲的对应字节。
pub const RESUME_FRAME_BUFFER_LEN: usize = 0x200;
static RESUME_FRAME_BUFFER: StaticCell<[u8; RESUME_FRAME_BUFFER_LEN]> =
    StaticCell::new([0; RESUME_FRAME_BUFFER_LEN]);

/// SPEC-073：把一帧的编码写进内核持有的恢复帧缓冲（见证/代调方在调用前使用）。
pub fn install_resume_frame(frame_offset: usize, bytes: &[u8]) -> Result<(), ExcResumeError> {
    if frame_offset + bytes.len() > RESUME_FRAME_BUFFER_LEN {
        return Err(ExcResumeError::OutOfBounds);
    }
    let base = RESUME_FRAME_BUFFER.as_ptr() as u64 + frame_offset as u64;
    // `frame_offset + bytes.len()` 已在上面校验过不超过缓冲长度。
    let target = prima_ostd::mm::bytes_at_mut(base, bytes.len());
    target.copy_from_slice(bytes);
    Ok(())
}

/// SPEC-073：读取内核持有的恢复帧缓冲（见证断言用）。
pub fn resume_frame_bytes(frame_offset: usize) -> Option<[u8; crate::exc::SRET_FRAME_LEN]> {
    if frame_offset + crate::exc::SRET_FRAME_LEN > RESUME_FRAME_BUFFER_LEN {
        return None;
    }
    let base = RESUME_FRAME_BUFFER.as_ptr() as u64 + frame_offset as u64;
    // 上面的边界检查保证该区间落在缓冲内。
    let source = prima_ostd::mm::bytes_at(base, crate::exc::SRET_FRAME_LEN);
    let mut bytes = [0u8; crate::exc::SRET_FRAME_LEN];
    bytes.copy_from_slice(source);
    Some(bytes)
}

/// SPEC-073：§12.10 `exc_resume` 的 QEMU 见证。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResumeWitness {
    pub task_id: u32,
    /// 正例（可恢复的 `Faulted`）恢复成功（1 = 成功）。
    pub resumed: u32,
    /// 成功动作 5 之后的状态编码（应为 `Ready` = 0）。
    pub status_after: u32,
    /// 成功动作 2：`pending_exc` 已清除（1 = 已清）。
    pub pending_cleared: u32,
    /// 成功动作 3：`kernel_resume_frame` 已写入（1 = 已写）。
    pub frame_saved: u32,
    /// 成功动作 1：缓冲中该帧的字节已清零（1 = 已清）。
    pub frame_bytes_zeroed: u32,
    /// §8.4 步骤 3：再次 `schedule` 进 `Running` 时 `kernel_resume_frame` 被消费清空（1 = 成立）。
    pub frame_consumed: u32,
    /// 前置 6：不可恢复种类再次 `exc_resume` 被拒且保持 `Faulted`（1 = 成立）。
    pub unrecoverable_rejected: u32,
    /// 生命周期收尾：该任务随后经 §8.2 内核路径回收为 `Dead`（1 = 成功）。
    pub reclaimed_after: u32,
    /// W9 双向不变量在全程后仍成立（1 = 成立）。
    pub invariant_ok: u32,
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

    /// 按槽索引查找。**越界索引返回 `None` 而非 panic**（见 [`CapabilityRecordTable::find`]）。
    pub fn find(&self, handle: Handle) -> Option<&CapabilityPosition> {
        self.entries
            .get(handle.index() as usize)
            .and_then(Option::as_ref)
    }

    fn find_mut(&mut self, handle: Handle) -> Option<&mut CapabilityPosition> {
        self.entries
            .get_mut(handle.index() as usize)
            .and_then(Option::as_mut)
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
        let index = handle.index() as usize;
        if index >= MAX_CAPABILITY_POSITIONS {
            return Err(CapabilityPositionError::MalformedHandle);
        }
        if self.entries[index].is_some() {
            return Err(CapabilityPositionError::DuplicateCapability);
        }
        if self.count == MAX_CAPABILITY_POSITIONS {
            return Err(CapabilityPositionError::CapacityExceeded);
        }
        self.entries[index] = Some(CapabilityPosition { handle, location });
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
        let index = handle.index() as usize;
        if index >= MAX_CAPABILITY_POSITIONS {
            return false;
        }
        let slot = &mut self.entries[index];
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

    /// SPEC-081 / §4.6：`HeldBy(holder) → InNamespace(ns)`（`ns_bind` 的位置变化）。
    pub fn move_to_namespace(
        &mut self,
        handle: Handle,
        holder: u32,
        ns: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self
            .find_mut(handle)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if !entry.location.is_held_by(holder) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::InNamespace(ns);
        Ok(())
    }

    /// SPEC-081 / §4.6：`InNamespace(ns) → HeldBy(holder)`（`ns_unbind` 的位置变化）。
    pub fn move_from_namespace(
        &mut self,
        handle: Handle,
        ns: u32,
        holder: u32,
    ) -> Result<(), CapabilityPositionError> {
        let entry = self
            .find_mut(handle)
            .ok_or(CapabilityPositionError::NoSuchCapability)?;
        if entry.location != CapabilityLocation::InNamespace(ns) {
            return Err(CapabilityPositionError::HolderMismatch);
        }
        entry.location = CapabilityLocation::HeldByTask(holder);
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
/// SPEC-087：第二个（**存活**）Rust 客户端的回发凭据槽。
///
/// task 3/task 4 的凭据都随其销毁被撤销，故存活客户端必须另铸一条。
pub const CLIENT_SEND_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 5;
/// SPEC-087：经第三次 create 的 `initial_cap0` 移入存活客户端的 `Task(1)`+`TaskSend`。
pub const CLIENT_SEND_CAP_HANDLE: Handle = cap_handle(CLIENT_SEND_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-088：Rust **服务端点**的回发凭据槽（客户端凭据随其自身持有，服务需另铸一条）。
pub const SERVICE_SEND_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 6;
/// SPEC-088：经第四次 create 的 `initial_cap0` 移入服务任务的 `Task(1)`+`TaskSend`。
pub const SERVICE_SEND_CAP_HANDLE: Handle =
    cap_handle(SERVICE_SEND_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-092：**第二个**独立服务端点的回发凭据槽（每个任务各需一条 `Task(1)`+`TaskSend`）。
pub const SERVICE2_SEND_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 7;
/// SPEC-092：经第五次 create 的 `initial_cap0` 移入第二个服务端点的凭据。
pub const SERVICE2_SEND_CAP_HANDLE: Handle =
    cap_handle(SERVICE2_SEND_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-093：**第二个**服务端点**独立代码段**的根 Capability 槽（承载独立的二进制映像）。
pub const SERVICE2_CODE_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 8;
/// SPEC-093：第二个端点代码段的根 Capability（持有者 task 1，`{Derive, SegMapR, SegMapX, SegDestroy}`）。
pub const SERVICE2_CODE_CAP_HANDLE: Handle =
    cap_handle(SERVICE2_CODE_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-093：**第二个**服务端点**独立栈段**的根 Capability 槽（独立**物理栈帧**，不再与 A 共享）。
pub const SERVICE2_STACK_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 9;
/// SPEC-093：第二个端点栈段的根 Capability（持有者 task 1，`{Derive, SegMapR, SegMapW, SegDestroy}`）。
pub const SERVICE2_STACK_CAP_HANDLE: Handle =
    cap_handle(SERVICE2_STACK_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-103：ring3 **服务管理器**独立代码段的根 Capability 槽（承载独立二进制 `prima-servicemgr`）。
pub const SERVICE3_CODE_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 10;
/// SPEC-103：服务管理器代码段的根 Capability（持有者 task 1，`{Derive, SegMapR, SegMapX, SegDestroy}`）。
pub const SERVICE3_CODE_CAP_HANDLE: Handle =
    cap_handle(SERVICE3_CODE_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-103：ring3 **服务管理器**独立栈段的根 Capability 槽（独立物理栈帧）。
pub const SERVICE3_STACK_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 11;
/// SPEC-103：服务管理器栈段的根 Capability（持有者 task 1，`{Derive, SegMapR, SegMapW, SegDestroy}`）。
pub const SERVICE3_STACK_CAP_HANDLE: Handle =
    cap_handle(SERVICE3_STACK_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-103：服务管理器的回发凭据槽（目标 `Task(1)` + `TaskSend`，经 `initial_cap0` 移入其自身）。
pub const SERVICEMGR_SEND_CAP_SLOT: u32 = USER_IPC_CAP_SLOT_BASE + 12;
/// SPEC-103：经第六次 create 的 `initial_cap0` 移入服务管理器的凭据。
pub const SERVICEMGR_SEND_CAP_HANDLE: Handle =
    cap_handle(SERVICEMGR_SEND_CAP_SLOT, INITIAL_CAP_GENERATION);
/// SPEC-094：**第一个**服务端点（`ALPHA`）的**独立栈段**根 Capability 槽。
///
/// SPEC-093 之前该端点沿用客户端（task 8）经 `SYS_MEMSEG_CREATE` 建的那条栈段 `Y[368]`——
/// 两个任务映射**同一物理栈帧**。本槽另建独立栈段，闭合该共享。
///
/// **为何是 61**：本槽须避开两侧的固定预留区间与哨兵。ISSUE-CAP-002 修复后，usercap 占
/// 47..56、Namespace 根 cap 占槽位顶端 64..71（见 [`NS_ROOT_CAP_SLOT_BASE`]），故 61
/// （`>` `UNSET_CAP`(60) 且 `<` 64）落在两段预留区间之间的空档，结构上不与任何固定区间重叠。
pub const SERVICE_STACK_CAP_SLOT: u32 = 61;
/// SPEC-094：第一个端点栈段的根 Capability（持有者 task 1，`{Derive, SegMapR, SegMapW, SegDestroy}`）。
pub const SERVICE_STACK_CAP_HANDLE: Handle =
    cap_handle(SERVICE_STACK_CAP_SLOT, INITIAL_CAP_GENERATION);
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

/// SPEC-093：`prepare_task_create_caps` 返回的两个代码段帧向量。
///
/// 第一段（`code`）给 `ALPHA` 端点（`prima-smoke`），第二段（`code2`）给 `BETA` 端点
/// （`prima-beta`）——两者是**不同 ELF**，各占一个 Segment（`MAX_SEGMENT_FRAMES=16` 容不下
/// 10+8 页，故不能合段）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskCreateImages {
    pub code: SegmentFrames,
    pub code2: SegmentFrames,
    /// SPEC-103：ring3 服务管理器（`prima-servicemgr`）的独立代码段。
    pub code3: SegmentFrames,
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
    /// SPEC-088：Task id 恒非零，用 `NonZeroU32` 为 `Option<VspaceEntry>` 提供 niche。
    pub task_id: NonZeroU32,
    pub va: u64,
    pub seg_id: u32,
    pub seg_offset: u64,
    pub access: Rights,
}

/// SPEC-091：逻辑 vspace 视图的容量。
///
/// 存储已移出 `BootstrapState`（见 [`VSPACE_ARENA`]），故该值**不占结构体空间**——可自由扩容而
/// 不破 13 KiB 软守卫。取值与 `MAX_CAPABILITY_SLOTS` 对齐；当前 boot 实测占用 41。
pub const MAX_VSPACE_ENTRIES: usize = 64;

/// SPEC-091：arena 区段数。内核只构造一个 `BootstrapState`（1 段），host 单测并行构造多个 state。
#[cfg(not(test))]
const VSPACE_REGIONS: usize = 1;
#[cfg(test)]
const VSPACE_REGIONS: usize = 512;

/// SPEC-091：vspace 条目存储——与 `BootstrapState` 解耦。
///
/// 移出理由同 SPEC-074（审计）/ SPEC-081（Namespace）：`BootstrapState` 已贴 13 KiB 软守卫
/// （余量仅 72 B），而内联的 `[Option<VspaceEntry>; 44]` = 1408 B 是其中较大一块。移入静态 arena
/// 后结构体缩小，且容量不再占结构体空间——多端点服务切片据此把上限提到 64。
static VSPACE_ARENA: crate::arena::Arena<VspaceEntry, MAX_VSPACE_ENTRIES, VSPACE_REGIONS> =
    crate::arena::Arena::new();

/// S2-D3：每 Task 的逻辑 vspace 视图。条目内容在 [`VSPACE_ARENA`] 里按实例分段，结构体内只留
/// 区段句柄与计数。
#[derive(Clone, Copy)]
pub struct TaskVspaceTable {
    slice: crate::arena::ArenaSlice,
    count: usize,
}

impl core::fmt::Debug for TaskVspaceTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TaskVspaceTable")
            .field("region", &self.slice.region())
            .field("count", &self.count)
            .finish()
    }
}

impl PartialEq for TaskVspaceTable {
    fn eq(&self, other: &Self) -> bool {
        self.slice == other.slice && self.count == other.count
    }
}

impl Eq for TaskVspaceTable {}

impl TaskVspaceTable {
    /// 在 arena 里为本实例领一段并返回空表（段耗尽即确定性 panic，见 [`crate::arena::Arena::claim`]）。
    fn empty() -> Self {
        Self {
            slice: VSPACE_ARENA.claim(),
            count: 0,
        }
    }

    /// 条目数（等价于旧的内联 `count` 字段）。
    pub fn count(&self) -> usize {
        self.count
    }

    /// 第 `index` 槽的副本；越界或空槽返回 `None`。
    fn entry(&self, index: usize) -> Option<VspaceEntry> {
        VSPACE_ARENA.get(self.slice, index)
    }

    fn find(&self, task_id: u32, va: u64) -> Option<VspaceEntry> {
        let mut found = None;
        let mut index = 0;
        while index < self.count {
            if let Some(entry) = self
                .entry(index)
                .filter(|entry| entry.task_id.get() == task_id && entry.va == va)
            {
                found = Some(entry);
                break;
            }
            index += 1;
        }
        found
    }

    /// 按序遍历条目（值拷贝，不借出 arena 内部）。
    fn for_each_entry(&self, mut visit: impl FnMut(&VspaceEntry)) {
        let mut index = 0;
        while index < self.count {
            if let Some(entry) = self.entry(index) {
                visit(&entry);
            }
            index += 1;
        }
    }

    fn insert(&mut self, entry: VspaceEntry) -> Result<(), MemsegError> {
        if self.count == MAX_VSPACE_ENTRIES {
            return Err(MemsegError::CapacityExceeded);
        }
        VSPACE_ARENA.set(self.slice, self.count, Some(entry));
        self.count += 1;
        Ok(())
    }

    fn remove(&mut self, task_id: u32, va: u64) -> Option<VspaceEntry> {
        let mut index = None;
        let mut cursor = 0;
        while cursor < self.count {
            if self
                .entry(cursor)
                .is_some_and(|entry| entry.task_id.get() == task_id && entry.va == va)
            {
                index = Some(cursor);
                break;
            }
            cursor += 1;
        }
        let index = index?;
        let entry = self.entry(index);
        for shift in index..self.count - 1 {
            VSPACE_ARENA.set(self.slice, shift, self.entry(shift + 1));
        }
        VSPACE_ARENA.set(self.slice, self.count - 1, None);
        self.count -= 1;
        entry
    }

    fn references_segment(&self, seg_id: u32) -> bool {
        let mut found = false;
        self.for_each_entry(|entry| {
            if entry.seg_id == seg_id {
                found = true;
            }
        });
        found
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

/// §5.4：Namespace 对象。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NamespaceObject {
    pub id: u32,
    pub creator: u32,
    /// 当前活动条目数（条目内容在 [`NsEntryTable`] 承载）。
    pub entries: usize,
}

/// SPEC-081：Namespace 条目存储。
///
/// 与 [`AuditLog`] 同构：内容在静态 arena（[`NS_ARENA`]）里按实例分段，结构体内只留一个
/// 区段句柄——`BootstrapState` 已贴 13 KiB 守卫，**不得内联**。
#[derive(Clone, Copy)]
pub struct NsEntryTable {
    slice: crate::arena::ArenaSlice,
}

impl core::fmt::Debug for NsEntryTable {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("NsEntryTable")
            .field("region", &self.slice.region())
            .field("count", &self.count())
            .finish()
    }
}

impl PartialEq for NsEntryTable {
    fn eq(&self, other: &Self) -> bool {
        self.slice == other.slice
    }
}

impl Eq for NsEntryTable {}

static NS_ARENA: crate::arena::Arena<NsEntry, MAX_NS_ENTRIES, NS_REGIONS> =
    crate::arena::Arena::new();

impl NsEntryTable {
    /// 在 arena 里为本实例领一段（段耗尽即确定性 panic，见 [`crate::arena::Arena::claim`]）。
    fn claim() -> Self {
        Self {
            slice: NS_ARENA.claim(),
        }
    }

    /// 当前条目数（线性扫描；条目上限 16，代价可忽略）。
    pub fn count(&self) -> usize {
        let mut count = 0;
        let mut index = 0;
        while index < MAX_NS_ENTRIES {
            if NS_ARENA.get(self.slice, index).is_some() {
                count += 1;
            }
            index += 1;
        }
        count
    }

    /// 是否存在条目（用于「条目全清空后才能销毁 Namespace」之类的判定）。
    pub fn is_empty(&self) -> bool {
        self.count() == 0
    }

    /// `ns` 内的条目数。
    pub fn count_in(&self, ns: u32) -> usize {
        let mut count = 0;
        self.for_each(|entry| {
            if entry.ns == ns {
                count += 1;
            }
        });
        count
    }

    /// 第一个空槽（满则 `None`）。
    fn free_slot(&self) -> Option<usize> {
        let mut index = 0;
        while index < MAX_NS_ENTRIES {
            if NS_ARENA.get(self.slice, index).is_none() {
                return Some(index);
            }
            index += 1;
        }
        None
    }

    fn store(&self, index: usize, entry: NsEntry) {
        NS_ARENA.set(self.slice, index, Some(entry));
    }

    fn clear(&self, index: usize) {
        NS_ARENA.set(self.slice, index, None);
    }

    /// 按序遍历条目（值拷贝，不借出 arena 内部）。
    pub fn for_each(&self, mut visit: impl FnMut(NsEntry)) {
        let mut index = 0;
        while index < MAX_NS_ENTRIES {
            if let Some(entry) = NS_ARENA.get(self.slice, index) {
                visit(entry);
            }
            index += 1;
        }
    }

    /// 在 `ns` 内精确查找路径为 `path` 的条目。
    pub fn find_in(&self, ns: u32, path: &Path) -> Option<(usize, NsEntry)> {
        let mut found = None;
        let mut index = 0;
        while index < MAX_NS_ENTRIES {
            if let Some(entry) = NS_ARENA
                .get(self.slice, index)
                .filter(|entry| entry.ns == ns && entry.path == *path)
            {
                found = Some((index, entry));
                break;
            }
            index += 1;
        }
        found
    }

    /// 按 Capability 反查条目（位置 `InNamespace(ns)` 的具体路径由它给出）。
    pub fn find_by_cap(&self, cap: Handle) -> Option<NsEntry> {
        let mut found = None;
        let mut index = 0;
        while index < MAX_NS_ENTRIES {
            if let Some(entry) = NS_ARENA
                .get(self.slice, index)
                .filter(|entry| entry.cap == cap)
            {
                found = Some(entry);
                break;
            }
            index += 1;
        }
        found
    }

    /// `ns` 内是否存在「`path` 的终端祖先」——即某条目的路径严格前缀于 `path`（§11.1 约束 3）。
    pub fn has_terminal_ancestor(&self, ns: u32, path: &Path) -> bool {
        let mut found = false;
        self.for_each(|entry| {
            if entry.ns == ns && entry.path.strict_prefix_of(path) {
                found = true;
            }
        });
        found
    }

    /// `ns` 内是否存在「`path` 的后代条目」——即某条目的路径以 `path` 为严格前缀（§11.1 约束 3）。
    pub fn has_descendant(&self, ns: u32, path: &Path) -> bool {
        let mut found = false;
        self.for_each(|entry| {
            if entry.ns == ns && path.strict_prefix_of(&entry.path) {
                found = true;
            }
        });
        found
    }

    /// `ns` 内沿 `Mount` 边可直达的 Namespace id（供 `ns_mount` 的环检测 DFS）。
    pub fn mount_targets(&self, ns: u32, mut visit: impl FnMut(u32)) {
        self.for_each(|entry| {
            if entry.ns == ns && entry.kind == NsEntryKind::Mount {
                visit(entry.target_ns);
            }
        });
    }

    /// 删除 `ns` 内的全部条目并返回被删除条目数（`ns_destroy` 与测试用）。
    pub fn clear_namespace(&self, ns: u32) -> usize {
        let mut removed = 0;
        let mut index = 0;
        while index < MAX_NS_ENTRIES {
            let matched = NS_ARENA
                .get(self.slice, index)
                .is_some_and(|entry| entry.ns == ns);
            if matched {
                self.clear(index);
                removed += 1;
            }
            index += 1;
        }
        removed
    }
}

/// SPEC-081 / §11：Namespace 路径操作的失败域。
///
/// 前 15 个与规范 §11.2–§11.8 列出的错误名一一对应；末两个是实现层的资源可用性失败
/// （§6.2 明确排除在抽象层失败之外）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NsError {
    /// §6.1 Actor 约束：调用者不是活动 Task。
    InvalidCaller,
    /// `cap` 不是有效 CapId（§11.3/§11.5 的 `InvalidCap`）。
    InvalidCap,
    /// 目标不是活动 Namespace（含目标已销毁）。
    InvalidNamespace,
    /// 缺少指定权限（§11.3–§11.8 的 `MissingRight`）。
    MissingRight(Right),
    /// 调用者未持有该 Capability（§11.3/§11.5/§11.8 的 `NotHeld`）。
    NotHeld,
    /// `ns_bind` 的 `cap` 目标对象已销毁（`usable` 失败）。
    TargetDead,
    /// `ns_resolve` 遇到已销毁的挂载目标（§11.7 的 `MountTargetDead`）。
    MountTargetDead,
    /// 是活动 Task 的结构性根 Namespace（§11.8 的 `StructuralCap`）。
    StructuralCap,
    /// 父项不可被派生（缺 `NsResolve`/`Derive`，§11.5/§11.7 的 `NotDelegable`）。
    NotDelegable,
    /// 新增 `destination → source` 边会形成 Mount 环（§11.5 的 `MountCycle`）。
    MountCycle,
    /// 路径未精确指向预期条目（§11.4/§11.6 的 `NotFound`）。
    NotFound,
    /// `Bound` 之后仍有剩余分量（§11.7 的 `NotDirectory`）。
    NotDirectory,
    /// 路径存在终端祖先、同名条目或后代条目（§11.1 约束 3 的 `Overlap`）。
    Overlap,
    /// 请求权限不是父权限的子集（§11.7 的 `RightsExceeded`）。
    RightsExceeded,
    /// 请求权限对目标对象类型非法。
    WrongRightType,
    /// 实现层：槽位/对象/条目容量耗尽（§6.2 排除项）。
    CapacityExceeded,
    /// 实现层：审计容量耗尽（§6.2 排除项）。
    AuditFull,
}

/// SPEC-081：Namespace 路径操作被拒绝的**实现层计数**（供 `namespace_ready` 见证）。
///
/// 失败不改变抽象状态（§6.2），故这些计数不进审计日志——它们只是本切片的观测记录。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NsRejects {
    pub cycles: u32,
    pub overlaps: u32,
    pub non_directory: u32,
    pub invalid_path: u32,
}

/// SPEC-082 / §5.5：Timer 的取消原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerCancelReason {
    ByTask(u32),
    RecipientDestroyed,
}

/// SPEC-082 / §5.5：`TimerState`。Timer 是一次性的——`Fired` 与 `Cancelled` 都不回到 `Pending`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerState {
    Pending {
        deadline: Instant,
        recipient: u32,
    },
    Fired {
        deadline: Instant,
        recipient: u32,
        fired_at: Instant,
        /// §10.6：本 Timer 对应的唯一通知 `MsgId`（`TimerFired` 与 `MessageSent` 的交汇点）。
        notification: u32,
    },
    Cancelled {
        deadline: Instant,
        recipient: u32,
        reason: TimerCancelReason,
    },
}

impl TimerState {
    pub const fn deadline(&self) -> Instant {
        match self {
            TimerState::Pending { deadline, .. }
            | TimerState::Fired { deadline, .. }
            | TimerState::Cancelled { deadline, .. } => *deadline,
        }
    }

    pub const fn recipient(&self) -> u32 {
        match self {
            TimerState::Pending { recipient, .. }
            | TimerState::Fired { recipient, .. }
            | TimerState::Cancelled { recipient, .. } => *recipient,
        }
    }

    pub const fn is_pending(&self) -> bool {
        matches!(self, TimerState::Pending { .. })
    }

    pub const fn is_fired(&self) -> bool {
        matches!(self, TimerState::Fired { .. })
    }

    pub const fn is_cancelled(&self) -> bool {
        matches!(self, TimerState::Cancelled { .. })
    }
}

/// SPEC-082 / §5.5：`Timer { creator, state }`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerObject {
    pub id: u32,
    pub creator: u32,
    pub state: TimerState,
}

/// SPEC-082：活动 Timer 表（固定容量）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerTable {
    pub entries: [Option<TimerObject>; MAX_TIMERS],
    pub count: usize,
}

impl TimerTable {
    pub const fn empty() -> Self {
        Self {
            entries: [None; MAX_TIMERS],
            count: 0,
        }
    }

    pub fn find(&self, id: u32) -> Option<&TimerObject> {
        self.entries.iter().flatten().find(|timer| timer.id == id)
    }

    fn find_mut(&mut self, id: u32) -> Option<&mut TimerObject> {
        self.entries
            .iter_mut()
            .flatten()
            .find(|timer| timer.id == id)
    }

    /// 最小的空闲 id（`ns_destroy` 式的复用语义：删除后 id 可被再次使用）。
    fn next_id(&self) -> u32 {
        let mut candidate = 1;
        loop {
            if self.find(candidate).is_none() {
                return candidate;
            }
            candidate += 1;
        }
    }

    fn insert(&mut self, timer: TimerObject) -> bool {
        let Some(slot) = self.entries.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };
        *slot = Some(timer);
        self.count += 1;
        true
    }

    /// 从活动表删除并返回被删除的对象（§8.2 步骤 5 / §10.5 用）。
    fn remove(&mut self, id: u32) -> Option<TimerObject> {
        let slot = self
            .entries
            .iter_mut()
            .find(|slot| slot.is_some_and(|timer| timer.id == id))?;
        let removed = slot.take();
        if removed.is_some() {
            self.count -= 1;
        }
        removed
    }

    /// 按 `(deadline, id)` 稳定序收集全部 `Pending` 且已到期的 Timer id。
    fn due_pending(&self, now: Instant) -> [Option<u32>; MAX_TIMERS] {
        let mut due = [None; MAX_TIMERS];
        let mut count = 0;
        // 选择排序式的稳定收集：反复取「最小 (deadline, id)」的未收集项。
        let mut taken = [false; MAX_TIMERS];
        loop {
            let mut best: Option<(usize, Instant, u32)> = None;
            for (index, timer) in self.entries.iter().enumerate() {
                let Some(timer) = timer else { continue };
                if taken[index] || !timer.state.is_pending() || timer.state.deadline() > now {
                    continue;
                }
                let key = (timer.state.deadline(), timer.id);
                let better = match best {
                    None => true,
                    Some((_, deadline, id)) => key < (deadline, id),
                };
                if better {
                    best = Some((index, key.0, key.1));
                }
            }
            let Some((index, _, id)) = best else { break };
            taken[index] = true;
            due[count] = Some(id);
            count += 1;
            if count == MAX_TIMERS {
                break;
            }
        }
        due
    }
}

/// SPEC-082：`timer_cancel` 的状态返回（§10.4 明示 `AlreadyFired`/`AlreadyCancelled` **不是**失败）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerCancelOutcome {
    Cancelled,
    AlreadyFired,
    AlreadyCancelled,
}

/// SPEC-082 / §10：Timer 操作的失败域。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerError {
    /// §6.1 Actor 约束：调用者不是活动 Task。
    InvalidCaller,
    /// `cap` 不是有效 CapId。
    InvalidCap,
    /// §10.4/§10.5 的 `InvalidTimer`：`cap` 不指向有效 Timer。
    InvalidTimer,
    /// 不在 `HeldBy(caller)`。
    NotHeld,
    /// 缺少指定权限。
    MissingRight(Right),
    /// §10.3 的 `InvalidTime`：`new_now < now`（违反时间单调性）。
    InvalidTime,
    /// 实现层：Timer 表或槽位容量耗尽（§6.2 排除项）。
    CapacityExceeded,
    /// 实现层：审计容量耗尽（§6.2 排除项）。
    AuditFull,
}

/// SPEC-082：Timer 的实现层计数（失败/状态返回不产生审计事件，故单列）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimerStats {
    /// `deadline ≤ now` 而在 `timer_set` 的同一原子操作内立即触发者（S3-D4）。
    pub immediate_fired: u32,
    /// `timer_cancel` 得到 `AlreadyCancelled` 的次数（§10.4 的状态返回）。
    pub already_cancelled: u32,
    /// `timer_cancel` 得到 `AlreadyFired` 的次数。
    pub already_fired: u32,
    /// 用户显式 `timer_destroy` 成功的次数（审计里的 `ObjectDestroyed` 不区分对象类型，
    /// 故本计数单列）。
    pub destroyed: u32,
}

/// SPEC-082：`timer_ready` 见证的计数。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimerAudit {
    pub now: u64,
    pub timers: u32,
    pub sets: u32,
    pub fired: u32,
    pub cancelled: u32,
    pub destroyed: u32,
    /// 其中 `deadline ≤ now` 而**在同一原子操作内**立即触发者（S3-D4）。
    pub immediate_fired: u32,
    /// `timer_cancel` 得到 `AlreadyCancelled` 的次数（状态返回，不产生新事件）。
    pub already_cancelled: u32,
    /// 每个 Fired Timer 都恰有一条 `TimerFired` 且恰有一条同 `MsgId` 的 `MessageSent`（S3-X2）。
    pub exactly_once_ok: u32,
}

/// SPEC-081：`namespace_ready` 见证的计数（活动对象 + 审计事件 + 拒绝计数）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NamespaceAudit {
    pub namespaces: u32,
    pub entries: u32,
    pub binds: u32,
    pub unbinds: u32,
    pub mounts: u32,
    pub unmounts: u32,
    pub resolves: u32,
    pub cycles_rejected: u32,
    pub overlaps_rejected: u32,
    pub non_directory_rejected: u32,
    pub invalid_path_rejected: u32,
    /// 成功执行过环检测并判定无环的挂载次数（每次成功 `ns_mount` 一次）。
    pub acyclic_ok: u32,
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
/// SPEC-081：Namespace 对象上限（init + `ns_create` 创建的）。
///
/// 原为 2（只物化 init）。切片 AJ 把它提到 8 以便 `ns_mount` 有环检测的真实对象可挂；
/// `NamespaceObject` 只有 16 B，内联增量约 96 B，远在 13 KiB 守卫余量（约 3.2 KiB）内。
pub const MAX_NAMESPACE_OBJECTS: usize = 8;

/// SPEC-081：`ns_create` 铸造的根 Capability 预留槽位基址（**槽位空间顶端**的
/// `MAX_NAMESPACE_OBJECTS` 个槽：`[MAX_CAPABILITY_SLOTS - MAX_NAMESPACE_OBJECTS, MAX_CAPABILITY_SLOTS)`）。
///
/// ring3 见证程序以 const fn 拼装机码，句柄必须编译期可知——故仿 SPEC-066 的
/// `PLATFORM_SEGMENT_CAP_SLOT_BASE` 用显式预留区间，而非「最低空闲槽」（后者会被
/// `cap_derive`/`memseg_create` 的动态分配抢占，且位置取决于此前执行过的全部 syscall）。
///
/// **ISSUE-CAP-002 修复（2026-10-12）**：原基址 52 使区间 52..59 与 usercap 47..56
/// **重叠**，`ns_create` 实际只剩 57/58/59 三槽（恰够 boot 的 3 次；任何落在该区的新 cap
/// 都会令第 3 次 `ns_create` `ENOSPC` 并级联）。改为**顶端对齐**后，区间 64..71 与
/// usercap(47..56)、`UNSET_CAP`(60)、`SERVICE_STACK_CAP_SLOT`(61) 结构上**不相交**；
/// 以 `MAX_CAPABILITY_SLOTS - MAX_NAMESPACE_OBJECTS` 表达，使 `MAX_CAPABILITY_SLOTS`
/// 扩容时自动保持顶端对齐且不重叠（无需再平移任何 usercap——它们内嵌在 ring3 机码里）。
pub const NS_ROOT_CAP_SLOT_BASE: u32 = (MAX_CAPABILITY_SLOTS - MAX_NAMESPACE_OBJECTS) as u32;
/// SPEC-081：单个 Namespace 的条目上限（§5.4 的细化层容量约束）。
pub const MAX_NS_ENTRIES: usize = 16;

/// SPEC-082 / §10.1：抽象绝对时间。单位由细化层定（本实现取 PIT tick，见设计文档 §0.1）。
pub type Instant = u64;
/// SPEC-082：活动 Timer 上限（§5.5；细化为固定容量表，约 8 × 32 B = 256 B 内联）。
pub const MAX_TIMERS: usize = 8;
/// SPEC-082 / §10.2 成功动作 2：Timer 根 Capability 的权限集。
pub const TIMER_RIGHTS: Rights =
    Rights::from_rights(&[Right::Derive, Right::TimerCancel, Right::TimerDestroy]);
/// SPEC-082 / §10.6：`FromKernel(TimerFired(id))` 通知 payload 的来源标签（不可伪造标记）。
pub const TIMER_FIRED_TAG: u64 = 0x71E4_0000_0000_0000;

/// SPEC-082：把 Timer id 编码为内核事件 payload。
pub const fn timer_fired_payload(timer_id: u32) -> u64 {
    TIMER_FIRED_TAG | timer_id as u64
}

/// SPEC-082：从 payload 解出 Timer id（非本类事件返回 `None`）。
pub const fn timer_fired_id(payload: u64) -> Option<u32> {
    if payload & TIMER_FIRED_TAG == TIMER_FIRED_TAG {
        Some((payload & 0xffff_ffff) as u32)
    } else {
        None
    }
}
/// SPEC-081：Namespace 条目 arena 的区段数（内核只构造一个 state，故 1 段；host 单测并行
/// 构造多个 state，各领一段互不干扰——理由与 [`AUDIT_REGIONS`] 完全相同）。
#[cfg(not(test))]
const NS_REGIONS: usize = 1;
#[cfg(test)]
const NS_REGIONS: usize = 512;

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
    /// SPEC-081：Namespace 条目存储（内容在静态 arena，本字段只是一个区段句柄）。
    pub ns_entries: NsEntryTable,
    /// SPEC-081：被拒绝的 Namespace 操作计数（实现层观测，不属抽象状态）。
    pub ns_rejects: NsRejects,
    /// SPEC-082 / §10.1：抽象绝对时间（单位 = PIT tick，见设计文档 §0.1）。
    pub now: Instant,
    /// SPEC-082 / §5.5：活动 Timer 表。
    pub timers: TimerTable,
    /// SPEC-082：Timer 的实现层计数（不属抽象状态）。
    pub timer_stats: TimerStats,
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
        // 的消息必须在队列中且含该句柄；InNamespace 的条目必须存在（SPEC-081）。
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
                CapabilityLocation::InNamespace(ns_id) => {
                    // 条目（而非仅 Namespace 对象）必须存在，且它的 cap 就是这条位置。
                    let recorded = self
                        .ns_entries
                        .find_by_cap(position.handle)
                        .is_some_and(|entry| entry.ns == ns_id);
                    if !recorded {
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

    /// §13.2 `audit(cap)`：纯查询，不改变任何状态。
    ///
    /// 前置只有两条：`caller` 是存活 Task，且 `cap` 当前由 `caller` 持有。**目标对象是否存活
    /// 不参与前置**——被销毁的对象只要 Capability 仍在 caller 手中就仍可查询（§4.3 的
    /// `live(target)` 以 `target_live` 字段表达，而非拒绝查询）。
    pub fn audit_trace(&self, caller: u32, cap: Handle) -> Result<AuditTrace, AuditError> {
        if !self.tasks.is_live(caller) {
            return Err(AuditError::InvalidCaller);
        }
        if self.slots.check(cap) != SlotState::Live {
            return Err(AuditError::InvalidCap);
        }
        let target = self.target_of(cap).ok_or(AuditError::InvalidCap)?;
        if self.holder_of(cap) != Some(caller) {
            return Err(AuditError::NotHeld);
        }
        let mut trace = AuditTrace {
            chain: [Handle::ZERO; MAX_TRACE_CHAIN],
            chain_len: 0,
            minted: 0,
            derived: 0,
            moves: 0,
            created: 0,
            destroyed: 0,
            target_live: self.target_live(target),
            origin_none: true,
        };
        let mut current = cap;
        loop {
            // 每个 parent 都先于其 child 存在；该上界同时兜住内部状态畸形的情形。
            if trace.chain_len == MAX_TRACE_CHAIN {
                return Err(AuditError::InvalidCap);
            }
            trace.chain[trace.chain_len] = current;
            trace.chain_len += 1;
            // parent token 可能已被丢弃并复用了槽位；不可变的派生事件仍带着旧 handle 与其
            // parent，故 records 查不到时回退到审计日志。
            let parent = self
                .records
                .find(current)
                .filter(|record| record.handle == current)
                .and_then(|record| record.parent)
                .or_else(|| {
                    let mut found = None;
                    self.audit.for_each(|event| {
                        if let AuditEvent::CapabilityDerived { cap, parent, .. } = *event
                            && cap == current
                        {
                            found = Some(parent);
                        }
                    });
                    found
                });
            match parent {
                Some(parent) => current = parent,
                None => break,
            }
        }
        trace.chain[..trace.chain_len].reverse();
        let is_segment = matches!(target, ObjectRef::Segment(_));
        self.audit.for_each(|event| match *event {
            AuditEvent::CapabilityMinted { cap: event_cap, .. }
                if trace.chain[..trace.chain_len].contains(&event_cap) =>
            {
                trace.minted += 1
            }
            AuditEvent::CapabilityDerived { cap: event_cap, .. }
                if trace.chain[..trace.chain_len].contains(&event_cap) =>
            {
                trace.derived += 1
            }
            AuditEvent::CapabilityMoved { cap: event_cap, .. }
            | AuditEvent::CapabilityDropped { cap: event_cap }
                if event_cap == cap =>
            {
                trace.moves += 1
            }
            AuditEvent::ObjectCreated { object_id, segment }
                if object_id == target.id() && segment == is_segment =>
            {
                trace.created += 1
            }
            AuditEvent::ObjectDestroyed { object_id, segment }
                if object_id == target.id() && segment == is_segment =>
            {
                trace.destroyed += 1
            }
            _ => {}
        });
        Ok(trace)
    }

    pub fn audit_len(&self) -> usize {
        self.audit.count
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
            ObjectRef::Timer(id) => self.timers.find(id).is_some(),
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
        let log = self.audit;
        log.for_each(|event| {
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
        });
        audit
    }

    /// SPEC-081：Namespace 路径操作的汇总（活动对象 + 审计事件计数 + 拒绝计数）。
    pub fn namespace_audit(&self) -> NamespaceAudit {
        let mut audit = NamespaceAudit {
            namespaces: self.namespace_count as u32,
            entries: self.ns_entries.count() as u32,
            binds: 0,
            unbinds: 0,
            mounts: 0,
            unmounts: 0,
            resolves: 0,
            cycles_rejected: self.ns_rejects.cycles,
            overlaps_rejected: self.ns_rejects.overlaps,
            non_directory_rejected: self.ns_rejects.non_directory,
            invalid_path_rejected: self.ns_rejects.invalid_path,
            acyclic_ok: 0,
        };
        let log = self.audit;
        log.for_each(|event| match event {
            AuditEvent::NsBound { .. } => audit.binds += 1,
            AuditEvent::NsUnbound { .. } => audit.unbinds += 1,
            AuditEvent::NsMounted { .. } => audit.mounts += 1,
            AuditEvent::NsUnmounted { .. } => audit.unmounts += 1,
            AuditEvent::NsResolved { .. } => audit.resolves += 1,
            _ => {}
        });
        // 每次成功的 `ns_mount` 都在提交前跑完了环检测 DFS 并判定无环。
        audit.acyclic_ok = audit.mounts;
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
    ///
    /// 除格式外还校验**槽索引上界**：`Handle` 只是编码，任何 `index ≥ MAX_CAPABILITY_SLOTS` 的
    /// 句柄都不可能是本表铸造的，在此即拒为 `MalformedHandle`，杜绝畸形句柄进入模型后触发越界。
    pub fn decode_handle(&self, raw: u64) -> Result<Handle, AuthError> {
        let handle = Handle::decode(raw).map_err(|_| AuthError::MalformedHandle)?;
        if handle.index() as usize >= MAX_CAPABILITY_SLOTS {
            return Err(AuthError::MalformedHandle);
        }
        Ok(handle)
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

    /// §12.6 `exc_source_destroy`：清空订阅者、从活动表删除源、追加销毁事件。
    ///
    /// 指向该源的 Capability 变为 stale token——细化层以 `target_live` 表达（源不在表中，
    /// 任何经它的授权都失败），与 `memseg_destroy`/`ns_destroy` 同口径：**不回收槽位**
    /// （异常源 cap 本就不入 `positions`，其槽由 `MAX_CAPABILITY_SLOTS` 兜住，登记为泄漏）。
    /// 成功动作 5 的「stale」在此不表现为 `SlotState::Stale`，而是目标不活。
    ///
    /// `ExcSourceId` **不复用**（`count` 保持，§3；`next_id` 因此仍单调）。
    /// 已 `Faulted` 且 `pending_exc.source_id` 指向本源的 Task **不受影响**（§12.6：源销毁
    /// 不改变其状态、不产生通知；它只能经 §8.2 的内核回收路径处置）。
    pub fn exc_source_destroy(&mut self, caller: u32, cap: Handle) -> Result<(), ExcError> {
        if !self.tasks.is_live(caller) {
            return Err(ExcError::NoSuchTask);
        }
        if self.slots.check(cap) != SlotState::Live {
            return Err(ExcError::NoSuchSource);
        }
        let source_id = self
            .exc_sources
            .find_by_cap(cap)
            .ok_or(ExcError::NoSuchSource)?
            .id;
        let record = self.records.find(cap).ok_or(ExcError::NoSuchSource)?;
        if record.holder != caller {
            return Err(ExcError::NotHolder);
        }
        if !record.rights.contains(Right::ExcSourceDestroy) {
            return Err(ExcError::MissingRight);
        }
        if !self.audit.can_append(1) {
            return Err(ExcError::AuditFull);
        }
        // 成功动作 1/2/3：订阅集合随源对象一同消失；源从活动表移除（`count` 不减，
        // 故 `next_id` 不回头、`ExcSourceId` 不复用）。
        if let Some(index) = self.exc_sources.entries[..self.exc_sources.count]
            .iter()
            .position(|entry| entry.is_some_and(|source| source.id == source_id))
        {
            self.exc_sources.entries[index] = None;
        }
        // 成功动作 4：追加销毁事件。与 Timer/Namespace 共用 `segment: false` 形态
        // （`ObjectRef` 的 id 空间按类型独立，见 SPEC-083）。
        self.audit.append(AuditEvent::ObjectDestroyed {
            object_id: source_id,
            segment: false,
        });
        Ok(())
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
        let log = self.audit;
        log.for_each(|event| match event {
            AuditEvent::ExcSourceCreated { .. } => counts.created += 1,
            AuditEvent::ExcSubscribed { .. } => counts.subscribed += 1,
            AuditEvent::ExcUnsubscribed { .. } => counts.unsubscribed += 1,
            AuditEvent::ExceptionRaised { .. } => counts.raised += 1,
            _ => {}
        });
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

    // -----------------------------------------------------------------------
    // SPEC-081：§11 Namespace 路径操作（切片 AJ）
    // -----------------------------------------------------------------------

    /// SPEC-081：`cap` 必须由 `caller` 持有、可用、指向活动 Namespace 且含 `right`；
    /// 成功返回该 Namespace 的 id。七个 `ns_*` 操作共用的前置入口。
    pub fn authorize_namespace(
        &self,
        caller: u32,
        cap: Handle,
        right: Right,
    ) -> Result<u32, NsError> {
        if !self.tasks.is_live(caller) {
            return Err(NsError::InvalidCaller);
        }
        match self.slots.check(cap) {
            SlotState::Free => return Err(NsError::InvalidCap),
            SlotState::Stale => return Err(NsError::InvalidNamespace),
            SlotState::Live => {}
        }
        let target = self.target_of(cap).ok_or(NsError::InvalidCap)?;
        let ObjectRef::Namespace(ns) = target else {
            return Err(NsError::InvalidNamespace);
        };
        if !self.target_live(target) {
            return Err(NsError::InvalidNamespace);
        }
        if self.holder_of(cap) != Some(caller) {
            return Err(NsError::NotHeld);
        }
        let rights = self.rights_of(cap).ok_or(NsError::InvalidCap)?;
        if !rights.contains(right) {
            return Err(NsError::MissingRight(right));
        }
        Ok(ns)
    }

    /// SPEC-081 / §11.8：`ns` 是否是任一**活动** Task 的结构性根 Namespace。
    ///
    /// 按 **Namespace** 而非 Capability 判定（规范原文是「该 Namespace 不是任何活动 Task 的
    /// 结构性根 Namespace」）：即使要销毁的是指向该 ns 的另一个 Capability，只要该 ns 仍是某
    /// 活动 Task 的根，销毁就必须被拒绝。
    pub fn is_structural_root_namespace(&self, ns: u32) -> bool {
        self.tasks.entries.iter().flatten().any(|task| {
            task.status != TaskStatus::Dead
                && task.root_ns_cap != Handle::ZERO
                && self.target_of(task.root_ns_cap) == Some(ObjectRef::Namespace(ns))
        })
    }

    /// SPEC-081 / §11.3 前置 4：`cap` 是否是任一活动 Task 的结构性根 Namespace Capability。
    fn is_structural_root_ns_cap(&self, cap: Handle) -> bool {
        self.tasks
            .entries
            .iter()
            .flatten()
            .any(|task| task.status != TaskStatus::Dead && task.root_ns_cap == cap)
    }

    /// SPEC-081：`cap` 是否「可用」（槽活动 + 目标存活）。§11.4/§11.6 的 stale token 清理
    /// 路径**不**要求这一点，故单列一个判定。
    fn is_usable_cap(&self, cap: Handle) -> bool {
        matches!(self.slots.check(cap), SlotState::Live)
            && self
                .target_of(cap)
                .is_some_and(|target| self.target_live(target))
    }

    /// SPEC-081：最小未用的 Namespace id（`ns_destroy` 释放的 id 可被复用）。
    fn next_namespace_id(&self) -> u32 {
        let mut candidate = 1;
        loop {
            let used = self
                .namespaces
                .iter()
                .flatten()
                .any(|namespace| namespace.id == candidate);
            if !used {
                return candidate;
            }
            candidate += 1;
        }
    }

    /// SPEC-081：更新某 Namespace 的条目计数（`NamespaceObject.entries` 只用于上报/见证，
    /// 真实内容在 [`NsEntryTable`]）。
    fn set_namespace_entry_count(&mut self, ns: u32, entries: usize) {
        if let Some(object) = self
            .namespaces
            .iter_mut()
            .flatten()
            .find(|object| object.id == ns)
        {
            object.entries = entries;
        }
    }

    /// SPEC-081 / §11.5、§11.7：从**非调用者持有**的父项派生（路径授权语义）。
    ///
    /// 与 [`Self::derive_capability`] 的差别：父项不要求由 `caller` 持有（`ns_resolve` 的父项
    /// 就在 Namespace 里），且子 Capability 的落点由 `location` 指定。`record.holder` 记为
    /// `caller`——位于 Namespace 中的 Capability 没有 Task 持有者（§4.6），`caller` 是唯一
    /// 可归属的 actor（登记见设计文档 §5 第 4 条）。
    fn derive_capability_at(
        &mut self,
        caller: u32,
        parent: Handle,
        rights: Rights,
        location: CapabilityLocation,
    ) -> Result<Handle, NsError> {
        match self.slots.check(parent) {
            SlotState::Free => return Err(NsError::InvalidCap),
            SlotState::Stale => return Err(NsError::MountTargetDead),
            SlotState::Live => {}
        }
        let target = self.target_of(parent).ok_or(NsError::InvalidCap)?;
        if !self.target_live(target) {
            return Err(NsError::MountTargetDead);
        }
        let parent_rights = self.rights_of(parent).ok_or(NsError::InvalidCap)?;
        if !parent_rights.contains(Right::Derive) {
            return Err(NsError::NotDelegable);
        }
        if !rights.subset_of(parent_rights) {
            return Err(NsError::RightsExceeded);
        }
        if !rights.well_typed(target.kind()) {
            return Err(NsError::WrongRightType);
        }
        let handle = self.slots.alloc().ok_or(NsError::CapacityExceeded)?;
        self.records
            .insert(handle, Some(parent), caller, rights, target)
            .map_err(|_| NsError::CapacityExceeded)?;
        self.positions
            .register_location(handle, location)
            .map_err(|_| NsError::CapacityExceeded)?;
        self.audit.append(AuditEvent::CapabilityDerived {
            cap: handle,
            parent,
            holder: caller,
        });
        Ok(handle)
    }

    /// SPEC-081 / §11.2 `ns_create`：建空 Namespace 并铸造调用者持有的完整根 Capability。
    ///
    /// §11.2 声明「无失败分支」；细化层的元数据空间耗尽（槽/对象/审计容量）映射为
    /// [`NsError::CapacityExceeded`]/[`NsError::AuditFull`]，属 §6.2 明确排除的资源可用性问题。
    pub fn ns_create(&mut self, caller: u32) -> Result<Handle, NsError> {
        if !self.tasks.is_live(caller) {
            return Err(NsError::InvalidCaller);
        }
        let object_slot = self
            .namespaces
            .iter()
            .position(|slot| slot.is_none())
            .ok_or(NsError::CapacityExceeded)?;
        // 根 Capability 落在预留区间的**第一个空闲槽**：`ns_destroy` 只销毁对象、不释放仍被
        // 调用者持有的根 Capability，故不能按对象槽索引直接映射到 cap 槽。
        let cap_slot = (NS_ROOT_CAP_SLOT_BASE
            ..NS_ROOT_CAP_SLOT_BASE + MAX_NAMESPACE_OBJECTS as u32)
            .find(|slot| self.slots.slot_free(*slot))
            .ok_or(NsError::CapacityExceeded)?;
        if !self.audit.can_append(2) {
            return Err(NsError::AuditFull);
        }
        let ns_id = self.next_namespace_id();
        let handle = self
            .mint_root_capability_at(
                CapabilityLocation::HeldByTask(caller),
                cap_slot,
                NAMESPACE_RIGHTS,
                ObjectRef::Namespace(ns_id),
            )
            .map_err(|_| NsError::CapacityExceeded)?;
        self.namespaces[object_slot] = Some(NamespaceObject {
            id: ns_id,
            creator: caller,
            entries: 0,
        });
        self.namespace_count += 1;
        self.audit.append(AuditEvent::ObjectCreated {
            object_id: ns_id,
            segment: false,
        });
        self.audit.append(AuditEvent::CapabilityMinted {
            cap: handle,
            holder: caller,
        });
        Ok(handle)
    }

    /// SPEC-081 / §11.3 `ns_bind`：把调用者持有的 `cap` 移到 `InNamespace(ns, path)`。
    pub fn ns_bind(
        &mut self,
        caller: u32,
        ns_cap: Handle,
        path: Path,
        cap: Handle,
    ) -> Result<(), NsError> {
        let ns = self.authorize_namespace(caller, ns_cap, Right::NsBind)?;
        match self.slots.check(cap) {
            SlotState::Free | SlotState::Stale => return Err(NsError::InvalidCap),
            SlotState::Live => {}
        }
        let target = self.target_of(cap).ok_or(NsError::InvalidCap)?;
        if !self.target_live(target) {
            return Err(NsError::TargetDead);
        }
        if self.holder_of(cap) != Some(caller) {
            return Err(NsError::NotHeld);
        }
        if self.is_structural_root_ns_cap(cap) {
            return Err(NsError::StructuralCap);
        }
        if self.ns_entries.find_in(ns, &path).is_some()
            || self.ns_entries.has_terminal_ancestor(ns, &path)
            || self.ns_entries.has_descendant(ns, &path)
        {
            self.ns_rejects.overlaps += 1;
            return Err(NsError::Overlap);
        }
        let slot = self
            .ns_entries
            .free_slot()
            .ok_or(NsError::CapacityExceeded)?;
        if !self.audit.can_append(2) {
            return Err(NsError::AuditFull);
        }
        let from = self
            .positions
            .find(cap)
            .map(|position| position.location)
            .ok_or(NsError::NotHeld)?;
        self.positions
            .move_to_namespace(cap, caller, ns)
            .map_err(|_| NsError::NotHeld)?;
        self.ns_entries.store(
            slot,
            NsEntry {
                ns,
                path,
                kind: NsEntryKind::Bound,
                cap,
                creator: caller,
                target_ns: 0,
            },
        );
        let entries = self.ns_entries.count();
        self.set_namespace_entry_count(ns, entries);
        self.audit.append(AuditEvent::CapabilityMoved {
            cap,
            from,
            to: CapabilityLocation::InNamespace(ns),
        });
        self.audit.append(AuditEvent::NsBound { ns, path, cap });
        Ok(())
    }

    /// SPEC-081 / §11.4 `ns_unbind`：删条目并把 `cap` 取回调用者手中。
    ///
    /// 被绑定 Capability 的**目标可以已经销毁**——stale token 仍可取回（§11.4 明示）。
    pub fn ns_unbind(
        &mut self,
        caller: u32,
        ns_cap: Handle,
        path: Path,
    ) -> Result<Handle, NsError> {
        let ns = self.authorize_namespace(caller, ns_cap, Right::NsUnbind)?;
        let (slot, entry) = self
            .ns_entries
            .find_in(ns, &path)
            .ok_or(NsError::NotFound)?;
        if entry.kind != NsEntryKind::Bound {
            return Err(NsError::NotFound);
        }
        if !self.audit.can_append(2) {
            return Err(NsError::AuditFull);
        }
        self.ns_entries.clear(slot);
        let entries = self.ns_entries.count();
        self.set_namespace_entry_count(ns, entries);
        self.positions
            .move_from_namespace(entry.cap, ns, caller)
            .map_err(|_| NsError::NotFound)?;
        self.audit.append(AuditEvent::CapabilityMoved {
            cap: entry.cap,
            from: CapabilityLocation::InNamespace(ns),
            to: CapabilityLocation::HeldByTask(caller),
        });
        self.audit.append(AuditEvent::NsUnbound {
            ns,
            path,
            cap: entry.cap,
        });
        Ok(entry.cap)
    }

    /// SPEC-081 / §11.5 `ns_mount`：把 `source` Namespace 挂到 `destination` 的 `path` 上。
    pub fn ns_mount(
        &mut self,
        caller: u32,
        destination_cap: Handle,
        path: Path,
        source_cap: Handle,
    ) -> Result<(), NsError> {
        let destination = self.authorize_namespace(caller, destination_cap, Right::NsMount)?;
        match self.slots.check(source_cap) {
            SlotState::Free => return Err(NsError::InvalidCap),
            SlotState::Stale => return Err(NsError::InvalidNamespace),
            SlotState::Live => {}
        }
        let target = self.target_of(source_cap).ok_or(NsError::InvalidCap)?;
        let ObjectRef::Namespace(source) = target else {
            return Err(NsError::InvalidNamespace);
        };
        if !self.target_live(target) {
            return Err(NsError::InvalidNamespace);
        }
        if self.holder_of(source_cap) != Some(caller) {
            return Err(NsError::NotHeld);
        }
        let source_rights = self.rights_of(source_cap).ok_or(NsError::InvalidCap)?;
        if !source_rights.contains(Right::NsResolve) || !source_rights.contains(Right::Derive) {
            return Err(NsError::NotDelegable);
        }
        // 前置 6「同一节点」：单机细化层恒满足（跨节点挂载不在本实现范围，登记）。
        if self.ns_entries.find_in(destination, &path).is_some()
            || self.ns_entries.has_terminal_ancestor(destination, &path)
            || self.ns_entries.has_descendant(destination, &path)
        {
            self.ns_rejects.overlaps += 1;
            return Err(NsError::Overlap);
        }
        if self.namespace_reaches(source, destination) {
            self.ns_rejects.cycles += 1;
            return Err(NsError::MountCycle);
        }
        let slot = self
            .ns_entries
            .free_slot()
            .ok_or(NsError::CapacityExceeded)?;
        if !self.audit.can_append(2) {
            return Err(NsError::AuditFull);
        }
        // 成功动作 1/2：派生权限恰为 `{NsResolve, Derive}` 的子 Capability，直接放入
        // `InNamespace(destination, path)`；`source_cap` 仍由调用者持有。
        let child_rights = Rights::from_rights(&[Right::NsResolve, Right::Derive]);
        let child = self.derive_capability_at(
            caller,
            source_cap,
            child_rights,
            CapabilityLocation::InNamespace(destination),
        )?;
        self.ns_entries.store(
            slot,
            NsEntry {
                ns: destination,
                path,
                kind: NsEntryKind::Mount,
                cap: child,
                creator: caller,
                target_ns: source,
            },
        );
        let entries = self.ns_entries.count();
        self.set_namespace_entry_count(destination, entries);
        self.audit.append(AuditEvent::NsMounted {
            ns: destination,
            path,
            source_ns: source,
            cap: child,
        });
        Ok(())
    }

    /// SPEC-081 / §11.6 `ns_unmount`：删挂载条目与挂载子 Capability 的位置。
    ///
    /// 源 Namespace 已销毁、挂载 Capability 已 stale 均**不**构成失败（§11.6 明示）。
    pub fn ns_unmount(&mut self, caller: u32, ns_cap: Handle, path: Path) -> Result<(), NsError> {
        let ns = self.authorize_namespace(caller, ns_cap, Right::NsUnmount)?;
        let (slot, entry) = self
            .ns_entries
            .find_in(ns, &path)
            .ok_or(NsError::NotFound)?;
        if entry.kind != NsEntryKind::Mount {
            return Err(NsError::NotFound);
        }
        if !self.audit.can_append(2) {
            return Err(NsError::AuditFull);
        }
        self.ns_entries.clear(slot);
        let entries = self.ns_entries.count();
        self.set_namespace_entry_count(ns, entries);
        // §11.6 / §4.6：**只删除位置**（`CapRecord` 不变）；已经解析出去的子 Capability 不受影响。
        self.positions.remove(entry.cap);
        self.audit
            .append(AuditEvent::CapabilityDropped { cap: entry.cap });
        self.audit.append(AuditEvent::NsUnmounted {
            ns,
            path,
            cap: entry.cap,
        });
        Ok(())
    }

    /// SPEC-081 / §11.7 `ns_resolve`：按路径解析并派生新 Capability 到调用者手中。
    pub fn ns_resolve(
        &mut self,
        caller: u32,
        ns_cap: Handle,
        path: Path,
        requested_rights: Rights,
    ) -> Result<Handle, NsError> {
        let start_ns = self.authorize_namespace(caller, ns_cap, Right::NsResolve)?;
        let mut current_ns = start_ns;
        let mut index = 0usize;
        // 起始父项是调用者持有的 `ns_cap`（路径为空时即它本身）；每遇到一个条目就前推。
        let mut parent = ns_cap;
        let mut mounts = [Handle::ZERO; MAX_PATH_COMPONENTS];
        let mut mount_count = 0usize;

        while index < path.len() {
            let (consumed, entry) = self
                .match_entry_prefix(current_ns, &path, index)
                .ok_or(NsError::NotFound)?;
            index += consumed;
            match entry.kind {
                NsEntryKind::Bound => {
                    if index != path.len() {
                        // §11.7 算法 3：`Bound` 后仍有剩余分量。
                        self.ns_rejects.non_directory += 1;
                        return Err(NsError::NotDirectory);
                    }
                    parent = entry.cap;
                }
                NsEntryKind::Mount => {
                    // §11.7 算法 4：挂载 Capability 必须可用（槽活动 + 目标存活，stale Mount
                    // 因此在此得 `MountTargetDead`），且目标是 Namespace 且有 `NsResolve`。
                    if !self.is_usable_cap(entry.cap) {
                        return Err(NsError::MountTargetDead);
                    }
                    let rights = self.rights_of(entry.cap).ok_or(NsError::MountTargetDead)?;
                    if !rights.contains(Right::NsResolve) {
                        return Err(NsError::NotDelegable);
                    }
                    if index == path.len() {
                        // 挂载点恰为路径末端：以挂载 Capability 为派生父项。
                        parent = entry.cap;
                    } else {
                        if mount_count == MAX_PATH_COMPONENTS {
                            return Err(NsError::NotFound);
                        }
                        mounts[mount_count] = entry.cap;
                        mount_count += 1;
                        current_ns = entry.target_ns;
                    }
                }
            }
        }

        // 算法 5/6：最终父项可用、含 `Derive`，且 `requested_rights` 是它的合法子集。
        let parent_target = self.target_of(parent).ok_or(NsError::TargetDead)?;
        if !self.target_live(parent_target) {
            return Err(NsError::TargetDead);
        }
        let parent_rights = self.rights_of(parent).ok_or(NsError::TargetDead)?;
        if !parent_rights.contains(Right::Derive) {
            return Err(NsError::NotDelegable);
        }
        if !requested_rights.subset_of(parent_rights) {
            return Err(NsError::RightsExceeded);
        }
        if !self.audit.can_append(2) {
            return Err(NsError::AuditFull);
        }
        // 算法 7：派生新 Capability 放到 `HeldBy(caller)`；父项保持在 Namespace 中不变。
        let handle = self.derive_capability_at(
            caller,
            parent,
            requested_rights,
            CapabilityLocation::HeldByTask(caller),
        )?;
        let mut mount_list = [Handle::ZERO; MAX_PATH_COMPONENTS];
        let mut i = 0;
        while i < mount_count {
            mount_list[i] = mounts[i];
            i += 1;
        }
        self.audit.append(AuditEvent::NsResolved {
            start_cap: ns_cap,
            path,
            mount_count: mount_count as u8,
            mounts: mount_list,
            parent,
            cap: handle,
        });
        Ok(handle)
    }

    /// SPEC-081 / §11.8 `ns_destroy`：删 Namespace 的全部条目与对象。
    pub fn ns_destroy(&mut self, caller: u32, ns_cap: Handle) -> Result<(), NsError> {
        let ns = self.authorize_namespace(caller, ns_cap, Right::NsDestroy)?;
        if self.is_structural_root_namespace(ns) {
            return Err(NsError::StructuralCap);
        }
        // 审计预算：每个条目一条 `CapabilityDropped`，加一条 `ObjectDestroyed`。
        let entry_count = self.ns_entries.count_in(ns);
        if !self.audit.can_append(entry_count + 1) {
            return Err(NsError::AuditFull);
        }
        // 收集待删条目（先取值再改，避免闭包借用冲突）。
        let mut doomed = [Handle::ZERO; MAX_NS_ENTRIES];
        let mut doomed_count = 0usize;
        self.ns_entries.for_each(|entry| {
            if entry.ns == ns && doomed_count < MAX_NS_ENTRIES {
                doomed[doomed_count] = entry.cap;
                doomed_count += 1;
            }
        });
        self.ns_entries.clear_namespace(ns);
        let mut i = 0;
        while i < doomed_count {
            let cap = doomed[i];
            // 成功动作 2 / §4.6：删除这些 Bound/Mount Capability 的**位置**（`CapRecord`
            // 不变）——细化层因此不回收其槽位（无位置 Capability 不可表示），登记见设计 §5。
            self.positions.remove(cap);
            self.audit.append(AuditEvent::CapabilityDropped { cap });
            i += 1;
        }
        // 成功动作 3：从活动 Namespace 表删除。
        self.remove_namespace_object(ns);
        self.audit.append(AuditEvent::ObjectDestroyed {
            object_id: ns,
            segment: false,
        });
        Ok(())
    }

    /// SPEC-081：从活动 Namespace 表删除对象（`ns_destroy` 用）。
    ///
    /// §11.8 明示**不**全局扫描其他 Namespace 中指向它的挂载项——那些条目留作 stale Mount，
    /// 解析时得 `MountTargetDead`，由管理者另行 `ns_unmount`。
    fn remove_namespace_object(&mut self, ns: u32) {
        if let Some(slot) = self
            .namespaces
            .iter()
            .position(|object| object.is_some_and(|object| object.id == ns))
        {
            self.namespaces[slot] = None;
            self.namespace_count = self.namespace_count.saturating_sub(1);
        }
    }

    /// SPEC-081 / §11.7：在 `ns` 内找「从分量 `from` 起、路径前缀恰好匹配」的条目。
    ///
    /// §11.1 约束 3 保证同一 Namespace 内不会有两个条目互为前缀，故至多一个匹配。
    fn match_entry_prefix(&self, ns: u32, path: &Path, from: usize) -> Option<(usize, NsEntry)> {
        let mut found = None;
        self.ns_entries.for_each(|entry| {
            if entry.ns != ns {
                return;
            }
            let len = entry.path.len();
            if from + len > path.len() {
                return;
            }
            let mut matches = true;
            let mut i = 0;
            while i < len {
                if entry.path.component(i) != path.component(from + i) {
                    matches = false;
                    break;
                }
                i += 1;
            }
            if matches {
                found = Some((len, entry));
            }
        });
        found
    }

    /// SPEC-081 / §11.1 约束 6：从 `from` 沿 Mount 边能否到达 `to`（环检测）。
    ///
    /// `visited` 位集保证 DFS 终止（节点数 ≤ [`MAX_NAMESPACE_OBJECTS`]）——这是确定性判据，
    /// 不是「限制深度」的近似。
    pub fn namespace_reaches(&self, from: u32, to: u32) -> bool {
        if from == to {
            return true;
        }
        let mut visited = [false; MAX_NAMESPACE_OBJECTS];
        let mut stack = [0u32; MAX_NAMESPACE_OBJECTS];
        let mut stack_len = 1usize;
        stack[0] = from;
        while stack_len > 0 {
            stack_len -= 1;
            let current = stack[stack_len];
            if let Some(mark) = self
                .namespaces
                .iter()
                .position(|object| object.is_some_and(|object| object.id == current))
            {
                if visited[mark] {
                    continue;
                }
                visited[mark] = true;
            }
            let mut reached = false;
            self.ns_entries.mount_targets(current, |target| {
                if target == to {
                    reached = true;
                }
                if stack_len < MAX_NAMESPACE_OBJECTS {
                    stack[stack_len] = target;
                    stack_len += 1;
                }
            });
            if reached {
                return true;
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // SPEC-082：§10 Timer（切片 AK）
    // -----------------------------------------------------------------------

    /// §10.1 `time_now`：纯查询，不改状态、无失败分支。
    pub const fn time_now(&self) -> Instant {
        self.now
    }

    /// SPEC-082：`cap` 必须由 `caller` 持有、可用、指向活动 Timer 且含 `right`。
    fn authorize_timer(&self, caller: u32, cap: Handle, right: Right) -> Result<u32, TimerError> {
        if !self.tasks.is_live(caller) {
            return Err(TimerError::InvalidCaller);
        }
        match self.slots.check(cap) {
            SlotState::Free => return Err(TimerError::InvalidCap),
            SlotState::Stale => return Err(TimerError::InvalidTimer),
            SlotState::Live => {}
        }
        let target = self.target_of(cap).ok_or(TimerError::InvalidCap)?;
        let ObjectRef::Timer(id) = target else {
            return Err(TimerError::InvalidTimer);
        };
        if self.timers.find(id).is_none() {
            return Err(TimerError::InvalidTimer);
        }
        if self.holder_of(cap) != Some(caller) {
            return Err(TimerError::NotHeld);
        }
        let rights = self.rights_of(cap).ok_or(TimerError::InvalidCap)?;
        if !rights.contains(right) {
            return Err(TimerError::MissingRight(right));
        }
        Ok(id)
    }

    /// SPEC-082 / §6.3：内核事件投递（`FromKernel`，无 sender Task、不携带 Capability）。
    ///
    /// **不复用** `send_message_with_capabilities`——它要求指向 receiver 的 `TaskSend`
    /// Capability，而内核事件没有发送方 Task；这里复用其投递主体（直投 / 排队两条路径）。
    fn enqueue_kernel_event(&mut self, receiver: u32, timer_id: u32) -> u32 {
        let message = IpcMessage {
            id: self.ipc.allocate_id(),
            sender: KERNEL_ACTOR,
            receiver,
            payload: timer_fired_payload(timer_id),
            capabilities: [Handle::ZERO; MAX_MESSAGE_CAPABILITIES],
            capability_count: 0,
        };
        let message_id = message.id;
        if self.tasks.is_blocked_recv(receiver) {
            // 直投：写 pending 并把接收者从 `BlockedRecv` 迁到 `Ready`（§6.3 步骤 3）。
            let _ = self.set_pending_task(receiver, message);
            self.audit.append(AuditEvent::MessageSent {
                message_id,
                sender: KERNEL_ACTOR,
                receiver,
            });
            self.audit.append(AuditEvent::MessageReceived {
                message_id,
                receiver,
            });
        } else {
            let _ = self.ipc.send(message);
            self.audit.append(AuditEvent::MessageSent {
                message_id,
                sender: KERNEL_ACTOR,
                receiver,
            });
        }
        message_id
    }

    /// §10.2 `timer_set`：只能为自己创建 Timer（接收者固定为调用者）。
    ///
    /// §10.2 声明「无失败分支」；细化层的表/槽/审计容量耗尽映射为
    /// [`TimerError::CapacityExceeded`]/[`TimerError::AuditFull`]（§6.2 排除项）。
    pub fn timer_set(&mut self, caller: u32, deadline: Instant) -> Result<Handle, TimerError> {
        if !self.tasks.is_live(caller) {
            return Err(TimerError::InvalidCaller);
        }
        if self.timers.count == MAX_TIMERS {
            return Err(TimerError::CapacityExceeded);
        }
        let immediate = deadline <= self.now;
        // 审计预算：TimerSet + CapabilityMinted（+ 立即触发时的 TimerFired 与通知最多 3 条）。
        let audit_needed = 2 + if immediate { 4 } else { 0 };
        if !self.audit.can_append(audit_needed) {
            return Err(TimerError::AuditFull);
        }
        if immediate && !self.tasks.is_blocked_recv(caller) && self.ipc.count == MAX_MESSAGES {
            return Err(TimerError::CapacityExceeded);
        }
        let id = self.timers.next_id();
        let handle = self
            .mint_root_capability(caller, TIMER_RIGHTS, ObjectRef::Timer(id))
            .map_err(|_| TimerError::CapacityExceeded)?;
        let state = if immediate {
            TimerState::Fired {
                deadline,
                recipient: caller,
                fired_at: self.now,
                notification: 0,
            }
        } else {
            TimerState::Pending {
                deadline,
                recipient: caller,
            }
        };
        if !self.timers.insert(TimerObject {
            id,
            creator: caller,
            state,
        }) {
            return Err(TimerError::CapacityExceeded);
        }
        // §10.2 成功动作 3：`TimerSet` 先于任何 `TimerFired`。
        self.audit.append(AuditEvent::TimerSet {
            timer_id: id,
            creator: caller,
            deadline,
        });
        self.audit.append(AuditEvent::CapabilityMinted {
            cap: handle,
            holder: caller,
        });
        if immediate {
            // §10.2 成功动作 5：同一原子操作内追 `TimerFired` → 投递通知 → 以该 `MsgId` 进入 `Fired`。
            self.timer_stats.immediate_fired += 1;
            self.audit.append(AuditEvent::TimerFired {
                timer_id: id,
                deadline,
                fired_at: self.now,
            });
            let notification = self.enqueue_kernel_event(caller, id);
            if let Some(timer) = self.timers.find_mut(id) {
                timer.state = TimerState::Fired {
                    deadline,
                    recipient: caller,
                    fired_at: self.now,
                    notification,
                };
            }
        }
        Ok(handle)
    }

    /// §10.4 `timer_cancel`。
    ///
    /// `Fired`/`Cancelled` 返回 [`TimerCancelOutcome`] 的状态返回（**不是**失败），且状态不变。
    pub fn timer_cancel(
        &mut self,
        caller: u32,
        cap: Handle,
    ) -> Result<TimerCancelOutcome, TimerError> {
        let id = self.authorize_timer(caller, cap, Right::TimerCancel)?;
        let Some(timer) = self.timers.find(id).copied() else {
            return Err(TimerError::InvalidTimer);
        };
        match timer.state {
            TimerState::Pending {
                deadline,
                recipient,
            } => {
                if !self.audit.can_append(1) {
                    return Err(TimerError::AuditFull);
                }
                let reason = TimerCancelReason::ByTask(caller);
                if let Some(timer) = self.timers.find_mut(id) {
                    timer.state = TimerState::Cancelled {
                        deadline,
                        recipient,
                        reason,
                    };
                }
                self.audit.append(AuditEvent::TimerCancelled {
                    timer_id: id,
                    reason,
                });
                Ok(TimerCancelOutcome::Cancelled)
            }
            TimerState::Fired { .. } => {
                self.timer_stats.already_fired += 1;
                Ok(TimerCancelOutcome::AlreadyFired)
            }
            TimerState::Cancelled { .. } => {
                self.timer_stats.already_cancelled += 1;
                Ok(TimerCancelOutcome::AlreadyCancelled)
            }
        }
    }

    /// §10.5 `timer_destroy`：从活动 Timer 表删除；已排队/已交付的通知**不撤回**。
    pub fn timer_destroy(&mut self, caller: u32, cap: Handle) -> Result<(), TimerError> {
        let id = self.authorize_timer(caller, cap, Right::TimerDestroy)?;
        let Some(timer) = self.timers.find(id).copied() else {
            return Err(TimerError::InvalidTimer);
        };
        let pending = timer.state.is_pending();
        if !self.audit.can_append(if pending { 2 } else { 1 }) {
            return Err(TimerError::AuditFull);
        }
        self.timers.remove(id);
        if pending {
            // §10.5：仍为 `Pending` 时先追 `TimerCancelled`，**不产生通知**。
            self.audit.append(AuditEvent::TimerCancelled {
                timer_id: id,
                reason: TimerCancelReason::ByTask(caller),
            });
        }
        self.audit.append(AuditEvent::ObjectDestroyed {
            object_id: id,
            segment: false,
        });
        self.timer_stats.destroyed += 1;
        Ok(())
    }

    /// §10.3 `advance_time`：KernelActor 内部迁移。
    ///
    /// 本实现的驱动点是**抢占收尾**（`resume::preempt_finish` 一次推进到累计 tick），
    /// **不是** `preempt_tick` 逐 tick 调用——后者会在中断上下文里做通知投递（唤醒任务、
    /// 改调度状态），并让中断处理时长进入用户任务的执行时序，扰动 `preempt_ready.mark*`
    /// 这类时序敏感见证（实测逐 tick 调用会让 `mark1` 由 `0x1` 变 `0x0`）。
    pub fn advance_time(&mut self, new_now: Instant) -> Result<(), TimerError> {
        if new_now < self.now {
            // §10.3：失败时 `now` 与全部 Timer 状态**不变**。
            return Err(TimerError::InvalidTime);
        }
        let due = self.timers.due_pending(new_now);
        let due_count = due.iter().flatten().count();
        if due_count == 0 {
            self.now = new_now;
            return Ok(());
        }
        // 保守预算：每个到期 Timer 最多 TimerFired + MessageSent + MessageReceived/TaskStateChanged。
        if !self.audit.can_append(due_count * 3) {
            return Err(TimerError::AuditFull);
        }
        // 排队路径需要队列余量（保守地按全部到期 Timer 都要排队计）。
        if self.ipc.count + due_count > MAX_MESSAGES {
            return Err(TimerError::CapacityExceeded);
        }
        self.now = new_now;
        let mut index = 0;
        while index < MAX_TIMERS {
            let Some(id) = due[index] else { break };
            index += 1;
            let Some(timer) = self.timers.find(id).copied() else {
                continue;
            };
            let deadline = timer.state.deadline();
            let recipient = timer.state.recipient();
            // §10.3 步骤 4/5：先追 `TimerFired`，再投递通知并取得 `MsgId`，Timer 以该 `MsgId`
            // 进入 `Fired`。（§6.3 保证同一消息的 `MessageSent` 先于 `MessageReceived`。）
            self.audit.append(AuditEvent::TimerFired {
                timer_id: id,
                deadline,
                fired_at: new_now,
            });
            let notification = self.enqueue_kernel_event(recipient, id);
            if let Some(timer) = self.timers.find_mut(id) {
                timer.state = TimerState::Fired {
                    deadline,
                    recipient,
                    fired_at: new_now,
                    notification,
                };
            }
        }
        Ok(())
    }

    /// SPEC-082 / §8.2 步骤 5：Task 销毁时清理以它为 recipient 的全部 Timer。
    ///
    /// `Pending` 先追加 `TimerCancelled(RecipientDestroyed)`（**不产生**通知）；`Fired`/`Cancelled`
    /// 只追加对象销毁事件；随后一律从活动表删除。返回处理的 Timer 数。
    fn destroy_timers_of_recipient(&mut self, task_id: u32) -> usize {
        let mut ids = [0u32; MAX_TIMERS];
        let mut count = 0;
        for timer in self.timers.entries.iter().flatten() {
            if timer.state.recipient() == task_id && count < MAX_TIMERS {
                ids[count] = timer.id;
                count += 1;
            }
        }
        let mut index = 0;
        while index < count {
            let id = ids[index];
            index += 1;
            if let Some(timer) = self.timers.find(id).copied() {
                self.timers.remove(id);
                if timer.state.is_pending() {
                    self.audit.append(AuditEvent::TimerCancelled {
                        timer_id: id,
                        reason: TimerCancelReason::RecipientDestroyed,
                    });
                }
                self.audit.append(AuditEvent::ObjectDestroyed {
                    object_id: id,
                    segment: false,
                });
            }
        }
        count
    }

    /// SPEC-082：§8.2 步骤 5 的审计预算（0 = 无需预算，多数销毁路径用它预检）。
    fn timers_of_recipient_audit_cost(&self, task_id: u32) -> usize {
        self.timers
            .entries
            .iter()
            .flatten()
            .filter(|timer| timer.state.recipient() == task_id)
            .map(|timer| if timer.state.is_pending() { 2 } else { 1 })
            .sum()
    }

    /// SPEC-082：`timer_ready` 见证的计数（含 exactly-once 判定，S3-X2）。
    pub fn timer_audit(&self) -> TimerAudit {
        let mut audit = TimerAudit {
            now: self.now,
            timers: self.timers.count as u32,
            immediate_fired: self.timer_stats.immediate_fired,
            already_cancelled: self.timer_stats.already_cancelled,
            destroyed: self.timer_stats.destroyed,
            ..TimerAudit::default()
        };
        let log = self.audit;
        log.for_each(|event| match event {
            AuditEvent::TimerSet { .. } => audit.sets += 1,
            AuditEvent::TimerCancelled { .. } => audit.cancelled += 1,
            AuditEvent::TimerFired { .. } => audit.fired += 1,
            _ => {}
        });
        // S3-X2（exactly-once）：每个**处于 `Fired` 的活动 Timer** 必须恰有一条 `TimerFired`
        // 事件、且其 `notification` 恰对应一条 `MessageSent`。
        let mut exactly_once = true;
        for timer in self.timers.entries.iter().flatten() {
            let TimerState::Fired { notification, .. } = timer.state else {
                continue;
            };
            let mut fired_events = 0;
            let mut sent_events = 0;
            log.for_each(|event| match event {
                AuditEvent::TimerFired { timer_id, .. } if *timer_id == timer.id => {
                    fired_events += 1;
                }
                AuditEvent::MessageSent { message_id, .. } if *message_id == notification => {
                    sent_events += 1;
                }
                _ => {}
            });
            if fired_events != 1 || sent_events != 1 {
                exactly_once = false;
            }
        }
        audit.exactly_once_ok = exactly_once as u32;
        audit
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
                task_id: NonZeroU32::new(new_id).expect("task id is nonzero"),
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
        // §8.1 动作 8 + §13.1：铸造即追加 `CapabilityMinted`（`mint_root_capability` 只
        // `can_append(1)` 预留额度、由调用者补发——与 `ns_create` 同模式）。
        self.audit.append(AuditEvent::CapabilityMinted {
            cap: manager_cap,
            holder: caller,
        });
        // §8.1 动作 4：self Cap 从 manager Cap **派生**（`parent = manager_cap`、权限相同），
        // 非独立铸造；`derive_capability_to_holder` 内部追加 `CapabilityDerived`（W2 溯源链）。
        let Ok(self_cap) =
            self.derive_capability_to_holder(caller, manager_cap, MANAGER_TASK_RIGHTS, new_id)
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
            self.vspace.for_each_entry(|entry| {
                if entry.task_id.get() == caller && count < MAX_VSPACE_ENTRIES {
                    entries[count] = entry.va;
                    count += 1;
                }
            });
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
        let mut dropped_caps = 0usize;
        for message in self.ipc.entries[..self.ipc.count].iter().flatten() {
            if message.sender == task_id || message.receiver == task_id {
                dropped += 1;
                dropped_caps += message.capability_count;
            }
        }
        dropped
            + dropped_caps
            + usize::from(pending)
            + 1
            + self.timers_of_recipient_audit_cost(task_id)
    }

    /// 既有模型层入口（无地址空间拆除）：`caller` 取一个不可能是任务 id 的值。
    pub fn destroy_task(&mut self, task_id: u32) -> Result<(), TaskError> {
        self.task_destroy(u32::MAX, task_id, None, 0)
    }

    /// §12.10 的 `exc_resume`——**细化口径 (A)：`KernelActor` 代故障任务调用**。
    ///
    /// POSIR 侧要求「调用者 Task 处于 `Faulted`」，而细化层中 `Faulted` 任务不在 `Running`、
    /// 无法发起 syscall，故由内核在故障路径上代调；`sret_frame_seg_cap` 由内核持有的
    /// [`RESUME_FRAME_BUFFER`] 承担（同 SPEC-066 `HeldByKernel` 的取向）。**该口径偏差登记在案。**
    ///
    /// 前置（全部先于任何写入）：5（`Faulted` 且有 `pending_exc`）、6（种类可恢复）、
    /// 8（`kernel_resume_frame == None`）。成功动作：2 清 `pending_exc`、3 存
    /// `kernel_resume_frame`、4 追加 `TaskResumed`、5 `Faulted → Ready`。
    /// （动作 1「清零已消费帧字节」由调用方在缓冲上完成，见 [`kernel_exc_resume`]。）
    pub fn exc_resume_faulted(
        &mut self,
        task_id: u32,
        frame: crate::exc::SretFrame,
    ) -> Result<(), ExcResumeError> {
        let Some(task) = self.tasks.find(task_id) else {
            return Err(ExcResumeError::NoSuchTask);
        };
        let Some(pending) = task.pending_exc else {
            return Err(ExcResumeError::NotFaulted);
        };
        if task.status != TaskStatus::Faulted {
            return Err(ExcResumeError::NotFaulted);
        }
        if !pending.kind.is_recoverable() {
            return Err(ExcResumeError::UnrecoverableFault);
        }
        if crate::sched::kernel_resume_frame(task_id).is_some() {
            return Err(ExcResumeError::FramePending);
        }
        if !self.audit.can_append(1) {
            return Err(ExcResumeError::AuditFull);
        }
        // 预检已全部通过，此后不再有失败点。
        self.tasks
            .resume_faulted(task_id)
            .map_err(|_| ExcResumeError::NoSuchTask)?;
        crate::sched::set_kernel_resume_frame(task_id, frame);
        self.audit.append(AuditEvent::TaskResumed { task_id });
        Ok(())
    }

    /// §8.2 L735：目标是否处于「**不可恢复终态**」——判词 = **`exc_resume` 必定失败**。
    ///
    /// 与 [`Self::exc_resume_faulted`] 的前置**逐条对应**，防止两者漂移：
    /// - 前置 5（`Faulted` 且有 `pending_exc`）是本判据的前提；
    /// - 前置 6（`pending_exc.kind` 不可恢复）是本细化层内**唯一**能成立的不成功条件。
    ///
    /// **SPEC-074 收紧（登记）**：SPEC-072 的过渡口径还含「异常源已不存在」一支（依 §12.6
    /// L1440）。但 §12.10 的 `exc_source_cap` 授权语义在本细化层尚未实现（`exc_resume` 不查源），
    /// 源销毁后我们**照样能恢复**该任务——故那一支与实现不符，**据此撤销**。收紧后的判词只认
    /// 「种类不可恢复」，与 `exc_resume` 的实际前置一致。
    pub fn is_unrecoverable_faulted(&self, task_id: u32) -> bool {
        self.tasks.find(task_id).is_some_and(|task| {
            task.status == TaskStatus::Faulted
                && task
                    .pending_exc
                    .is_some_and(|pending| !pending.kind.is_recoverable())
        })
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
        self.vspace.for_each_entry(|entry| {
            if entry.task_id.get() == task_id && count < MAX_VSPACE_ENTRIES {
                entries[count] = entry.va;
                count += 1;
            }
        });
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
        // §8.2 步骤 2/3：同时收集被丢弃的消息与其携带的 Capability（后者位置删除需丢弃事件）。
        let mut dropped_caps = [Handle::ZERO; MAX_MESSAGES * MAX_MESSAGE_CAPABILITIES];
        let mut dropped_cap_count = 0;
        for message in self.ipc.entries[..self.ipc.count].iter().flatten() {
            if message.sender != task_id && message.receiver != task_id {
                continue;
            }
            if dropped_count < MAX_MESSAGES {
                dropped_message_ids[dropped_count] = message.id;
                dropped_count += 1;
            }
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
        let audit_events = dropped_count
            + dropped_cap_count
            + if pending_message.is_some() { 1 } else { 0 }
            + 1
            + self.timers_of_recipient_audit_cost(task_id);
        if !self.audit.can_append(audit_events) {
            return Err(TaskError::AuditFull);
        }
        // 先清结构性根，否则它还挂在 positions 上会被 `revoke_capability` 漏掉。
        if let Some(task) = self.tasks.find_mut(task_id) {
            task.root_ns_cap = Handle::ZERO;
        }
        self.revoke_task_capabilities(task_id)
            .map_err(|_| TaskError::NoSuchTask)?;
        for cap in dropped_caps[..dropped_cap_count].iter().copied() {
            if !cap.is_zero() {
                self.retire_capability(cap);
                // §8.2 步骤 3/9：消息内 Capability 的位置被删除 → 追加丢弃事件。
                self.audit.append(AuditEvent::CapabilityDropped { cap });
            }
        }
        // §8.2 步骤 5：清理以该 Task 为 recipient 的全部 Timer（`Pending` 先记
        // `TimerCancelled(RecipientDestroyed)`，随后一律删除；已排队/已交付的通知不撤回）。
        self.destroy_timers_of_recipient(task_id);
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
        // §13.1：每个携带的 Capability 都发生一次位置变化（→ `InMessage` 或 `HeldBy(receiver)`），
        // 各追加一条 `CapabilityMoved`。
        let audit_events = (if direct_delivery { 2 } else { 1 }) + capability_ids.len();
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
                let from = self.positions.find(cap).map(|position| position.location);
                self.positions
                    .move_to_task(cap, sender, receiver)
                    .map_err(|_| IpcError::CapabilityHolderMismatch)?;
                // §13.1：位置变化必须追加 `CapabilityMoved`（直投：`HeldBy(sender)` → `HeldBy(receiver)`）。
                if let Some(from) = from {
                    self.audit.append(AuditEvent::CapabilityMoved {
                        cap,
                        from,
                        to: CapabilityLocation::HeldByTask(receiver),
                    });
                }
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
                let from = self.positions.find(cap).map(|position| position.location);
                self.positions
                    .move_to_message(cap, sender, message.id)
                    .map_err(|_| IpcError::CapabilityHolderMismatch)?;
                // §13.1：位置变化必须追加 `CapabilityMoved`（排队：`HeldBy(sender)` → `InMessage`）。
                if let Some(from) = from {
                    self.audit.append(AuditEvent::CapabilityMoved {
                        cap,
                        from,
                        to: CapabilityLocation::InMessage(message.id),
                    });
                }
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
        if !self.audit.can_append(1 + MAX_MESSAGE_CAPABILITIES) {
            return Err(IpcError::AuditFull);
        }
        match self.ipc.receive(receiver) {
            Ok(message) => {
                for cap in message.capabilities[..message.capability_count]
                    .iter()
                    .copied()
                {
                    let from = self.positions.find(cap).map(|position| position.location);
                    self.positions
                        .move_from_message(cap, message.id, receiver)
                        .map_err(|_| IpcError::InvalidCapability)?;
                    // §13.1：位置变化必须追加 `CapabilityMoved`（接收：`InMessage` → `HeldBy(receiver)`）。
                    if let Some(from) = from {
                        self.audit.append(AuditEvent::CapabilityMoved {
                            cap,
                            from,
                            to: CapabilityLocation::HeldByTask(receiver),
                        });
                    }
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

/// 单核无 PCID：只有当前 Task 的页表根可能在本核缓存了用户页翻译。
/// 非当前根在下一次 `sched::resume` 写 CR3 时自然失效。
fn needs_local_tlb_flush(target_root: u64, current_root: Option<u64>) -> bool {
    current_root == Some(target_root)
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
    /// 返回代码段帧向量，供内核装载独立用户 ELF。代码段用**用户区 id**而非平台区间，
    /// 故 `PLATFORM_SEGMENT_ID_LAST`/`PLATFORM_SEGMENT_COUNT` 与平台 cap 槽 32..44 不变。
    ///
    /// 必须在 `platform_prepare_task_caps` 之后调用（两条 cap 的目标 Task(1) 已存活），且在
    /// 既有 bring-up 取帧序列**之后**调用（避免平移切片 E/F/G/O 的物理帧序列）。
    pub fn prepare_task_create_caps(
        &mut self,
        code_pages: usize,
        code2_pages: usize,
        code3_pages: usize,
        phys_offset: u64,
    ) -> Result<TaskCreateImages, MemsegError> {
        if code_pages == 0
            || code_pages > MAX_SEGMENT_FRAMES
            || code2_pages == 0
            || code2_pages > MAX_SEGMENT_FRAMES
            || code3_pages == 0
            || code3_pages > MAX_SEGMENT_FRAMES
        {
            return Err(MemsegError::OutOfBounds);
        }
        let (_, frames) = self.create_segment_object(
            USER_SEGMENT_ID_FIRST,
            KERNEL_ACTOR,
            code_pages,
            TASK_CREATE_CODE_ACCESS,
            phys_offset,
            Some((
                CapabilityLocation::HeldByTask(INIT_TASK_ID),
                TASK_CREATE_CODE_CAP_SLOT,
            )),
        )?;
        // SPEC-093：第二个服务端点的**独立代码段**（承载不同的二进制映像）。
        let code2_id = self
            .alloc_segment_slot()
            .ok_or(MemsegError::CapacityExceeded)?;
        let (_, code2) = self.create_segment_object(
            code2_id,
            KERNEL_ACTOR,
            code2_pages,
            TASK_CREATE_CODE_ACCESS,
            phys_offset,
            Some((
                CapabilityLocation::HeldByTask(INIT_TASK_ID),
                SERVICE2_CODE_CAP_SLOT,
            )),
        )?;
        // SPEC-093：第二个服务端点的**独立栈段**（独立**物理栈帧**，不再与 A 共享）。
        let stack2_id = self
            .alloc_segment_slot()
            .ok_or(MemsegError::CapacityExceeded)?;
        let stack2_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
        self.create_segment_object(
            stack2_id,
            KERNEL_ACTOR,
            1,
            stack2_access,
            phys_offset,
            Some((
                CapabilityLocation::HeldByTask(INIT_TASK_ID),
                SERVICE2_STACK_CAP_SLOT,
            )),
        )?;
        // SPEC-094：**第一个**服务端点（`ALPHA`）的独立栈段——此前它与客户端共享 `Y[368]`。
        let stack1_id = self
            .alloc_segment_slot()
            .ok_or(MemsegError::CapacityExceeded)?;
        let stack1_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
        self.create_segment_object(
            stack1_id,
            KERNEL_ACTOR,
            1,
            stack1_access,
            phys_offset,
            Some((
                CapabilityLocation::HeldByTask(INIT_TASK_ID),
                SERVICE_STACK_CAP_SLOT,
            )),
        )?;
        // SPEC-103：ring3 服务管理器的**独立代码段**（承载 `prima-servicemgr` 映像）。
        let code3_id = self
            .alloc_segment_slot()
            .ok_or(MemsegError::CapacityExceeded)?;
        let (_, code3) = self.create_segment_object(
            code3_id,
            KERNEL_ACTOR,
            code3_pages,
            TASK_CREATE_CODE_ACCESS,
            phys_offset,
            Some((
                CapabilityLocation::HeldByTask(INIT_TASK_ID),
                SERVICE3_CODE_CAP_SLOT,
            )),
        )?;
        // SPEC-103：服务管理器的**独立栈段**（独立物理栈帧）。
        let stack3_id = self
            .alloc_segment_slot()
            .ok_or(MemsegError::CapacityExceeded)?;
        let stack3_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
        self.create_segment_object(
            stack3_id,
            KERNEL_ACTOR,
            1,
            stack3_access,
            phys_offset,
            Some((
                CapabilityLocation::HeldByTask(INIT_TASK_ID),
                SERVICE3_STACK_CAP_SLOT,
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
        // SPEC-087：存活客户端的回发凭据（task 3/4 的已随其销毁撤销）。
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(INIT_TASK_ID),
            CLIENT_SEND_CAP_SLOT,
            Rights::from_rights(&[Right::TaskSend]),
            ObjectRef::Task(INIT_TASK_ID),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        // SPEC-088：Rust 服务端点的回发凭据（服务与客户端是两个任务，各需一条）。
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(INIT_TASK_ID),
            SERVICE_SEND_CAP_SLOT,
            Rights::from_rights(&[Right::TaskSend]),
            ObjectRef::Task(INIT_TASK_ID),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        // SPEC-092：第二个独立服务端点的回发凭据（每个服务任务各持一条，凭据随任务销毁撤销）。
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(INIT_TASK_ID),
            SERVICE2_SEND_CAP_SLOT,
            Rights::from_rights(&[Right::TaskSend]),
            ObjectRef::Task(INIT_TASK_ID),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        // SPEC-103：ring3 服务管理器的回发凭据（目标 `Task(1)` + `TaskSend`）。
        self.mint_root_capability_at(
            CapabilityLocation::HeldByTask(INIT_TASK_ID),
            SERVICEMGR_SEND_CAP_SLOT,
            Rights::from_rights(&[Right::TaskSend]),
            ObjectRef::Task(INIT_TASK_ID),
        )
        .map_err(|_| MemsegError::CapacityExceeded)?;
        Ok(TaskCreateImages {
            code: frames,
            code2,
            code3,
        })
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
        let mut vspace_hits = 0u32;
        self.vspace.for_each_entry(|entry| {
            if platform_range.contains(&entry.seg_id) {
                vspace_hits += 1;
            }
        });
        record.vspace = vspace_hits;
        for id in platform_range {
            // 只统计真实存在的对象，与 `segments` 的口径一致。
            if self.segment(id).is_none() {
                continue;
            }
            if platform_segment_is_direct(id) {
                record.direct += 1;
            } else if self.vspace.references_segment(id) {
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
            if crate::vspace::leaf_phys_or_none(root_phys, va, map).is_some() {
                return Err(MemsegError::Overlap);
            }
        }
        if self.vspace.count() + page_count > MAX_VSPACE_ENTRIES {
            return Err(MemsegError::CapacityExceeded);
        }
        let flush_local = needs_local_tlb_flush(root_phys, crate::sched::current_root());
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
            if flush_local {
                prima_ostd::arch::invlpg(va);
            }
            self.vspace.insert(VspaceEntry {
                task_id: NonZeroU32::new(task_id).expect("task id is nonzero"),
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
        let flush_local = needs_local_tlb_flush(root_phys, crate::sched::current_root());
        for index in 0..page_count {
            let va = target_base + (index as u64) * crate::mem::FRAME_SIZE;
            crate::vspace::unmap_4k(root_phys, va, map, |frame| {
                crate::frames::release_table_frame(frame)
                    .map_err(|_| crate::vspace::VspaceError::NoFrame)
            })
            .map_err(|_| MemsegError::NotMapped)?;
            if flush_local {
                prima_ostd::arch::invlpg(va);
            }
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
        self.audit.append(AuditEvent::ObjectDestroyed {
            object_id: seg_id,
            segment: true,
        });
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
        audit: AuditLog::claim(),
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
        ns_entries: NsEntryTable::claim(),
        ns_rejects: NsRejects::default(),
        now: 0,
        timers: TimerTable::empty(),
        timer_stats: TimerStats::default(),
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
    // W13「每个活动对象都有创建事件」+ §13.1：init Task 是**创建**（非状态迁移），
    // 故与 `task_create`（成功动作 8）一致用 `TaskCreated`，不误用 `TaskStateChanged`。
    state.audit.append(AuditEvent::TaskCreated {
        task_id: INIT_TASK_ID,
    });
    state
        .validate_authority()
        .map_err(|_| BootstrapError::InvalidAuthority)?;
    Ok(state)
}

/// 是否已安装 Bootstrap 状态（W14 幂等判定）。
pub fn is_installed() -> bool {
    BOOTSTRAP_STATE.with(|slot| slot.is_some())
}

/// 切片 P：读取初始 Authority 见证记录（未安装或无记录时报错）。
pub fn authority_record() -> Result<BootstrapRecord, BootstrapError> {
    BOOTSTRAP_STATE.with(
        |slot| match slot.as_ref().and_then(|state| state.boot_record) {
            Some(record) => Ok(record),
            None => Err(BootstrapError::InvalidAuthority),
        },
    )
}

/// SPEC-083：用户态 `audit(cap)` syscall 的只读全局入口。
pub fn audit_trace(caller: u32, cap: Handle) -> Result<AuditTrace, AuditError> {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => state.audit_trace(caller, cap),
        None => Err(AuditError::InvalidCaller),
    })
}

pub fn audit_len() -> Option<usize> {
    BOOTSTRAP_STATE.with(|slot| slot.as_ref().map(BootstrapState::audit_len))
}

/// §14 步骤 8 + W13：首次调度 init——`Ready → Running` 并追加 `TaskStateChanged`。
pub fn activate_init() -> Result<(), BootstrapError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.activate_init(),
        None => Err(BootstrapError::InvalidAuthority),
    })
}

/// SPEC-071：取 Task 状态迁移审计汇总（授权表未安装时 `None`）。
pub fn task_state_audit() -> Option<TaskStateAudit> {
    BOOTSTRAP_STATE.with(|slot| slot.as_ref().map(|state| state.task_state_audit()))
}

/// SPEC-070：采集 `Faulted` 见证（必须在 `activate_init` **之前**调用）。
pub fn collect_fault_witness() -> Result<FaultWitness, TaskError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.fault_witness(),
        None => Err(TaskError::NoSuchTask),
    })
}

/// SPEC-070：已采集的 `Faulted` 见证（供收尾事件上报）。
static FAULT_WITNESS: StaticCell<Option<FaultWitness>> = StaticCell::new(None);

/// 采集并登记 `Faulted` 见证。
pub fn record_fault_witness() -> Result<(), TaskError> {
    let witness = collect_fault_witness()?;
    FAULT_WITNESS.with_mut(|slot| *slot = Some(witness));
    Ok(())
}

/// SPEC-073：§12.10 `exc_resume` 的 KernelActor 代调入口（细化口径 A）。
///
/// 帧从内核持有的 [`RESUME_FRAME_BUFFER`] 的 `frame_offset` 处读取并**先验证**；验证与全部
/// 前置通过后才落状态，成功后再执行动作 1（清零该段已消费字节）。任一失败即返回错误且
/// **State 与缓冲都不变**。
pub fn kernel_exc_resume(
    task_id: u32,
    frame_offset: usize,
    landing: u64,
    stack_bottom: u64,
    stack_top: u64,
) -> Result<(), ExcResumeError> {
    let Some(bytes) = resume_frame_bytes(frame_offset) else {
        return Err(ExcResumeError::OutOfBounds);
    };
    let frame = crate::exc::SretFrame::decode(&bytes);
    if crate::exc::validate_for_kernel_resume(&frame, landing, stack_bottom, stack_top).is_err() {
        return Err(ExcResumeError::InvalidFrame);
    }
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.exc_resume_faulted(task_id, frame),
        None => Err(ExcResumeError::NoSuchTask),
    })?;
    // 成功动作 1：清零该段已消费的帧字节（此刻已无失败点）。
    let base = RESUME_FRAME_BUFFER.as_ptr() as u64 + frame_offset as u64;
    // `resume_frame_bytes` 已对同一区间做过边界校验。
    let consumed = prima_ostd::mm::bytes_at_mut(base, crate::exc::SRET_FRAME_LEN);
    for byte in consumed.iter_mut() {
        *byte = 0;
    }
    Ok(())
}

/// SPEC-073：§12.10 的完整见证（`KernelActor` 代故障任务 `exc_resume`）。
///
/// 必须在 `activate_init` **之前**的同一窗口调用（此刻无 Task `Running`，`schedule` 合法）。
/// 全过程只用**保留槽位任务** [`RECLAIM_NEGATIVE_TASK_ID`]（§8.2 回收见证已证明它"可恢复
/// ⇒ 不得回收"）：把它从 `Faulted` 恢复 → 再次 `schedule` 见证帧被消费 → 置为不可恢复终态
/// 并见证前置 6 拒绝 → 最后由 §8.2 内核路径回收。既覆盖 §12.10 的全部成功动作，又把该任务
/// 收尾为 `Dead`，不再留下永久 `Faulted` 记录。
pub fn record_resume_witness(
    landing: u64,
    stack_bottom: u64,
    stack_top: u64,
) -> Result<(), ExcResumeError> {
    let task_id = RECLAIM_NEGATIVE_TASK_ID;
    let mut frame = crate::exc::SretFrame::ZERO;
    frame.rip = landing;
    frame.rsp = stack_top - 16;
    frame.cs = crate::exc::KERNEL_CS;
    frame.ss = crate::exc::KERNEL_SS;
    // 与切片 D 的恢复跳板同一约定：固定位 + ZF（落点据此证明 RFLAGS 真被 iretq 恢复）。
    frame.rflags = crate::exc::KERNEL_RFLAGS_FIXED | (1 << 6);
    frame.r12 = crate::exc::RESUME_SENTINEL_A;
    frame.r13 = crate::exc::RESUME_SENTINEL_B;
    if crate::exc::validate_for_kernel_resume(&frame, landing, stack_bottom, stack_top).is_err() {
        return Err(ExcResumeError::InvalidFrame);
    }
    install_resume_frame(0, &frame.encode())?;

    // 正例：代调 exc_resume。
    let resumed = kernel_exc_resume(task_id, 0, landing, stack_bottom, stack_top).is_ok();
    let status_after = match task_status(task_id) {
        Some(TaskStatus::Ready) => 0u32,
        Some(TaskStatus::Running) => 1,
        Some(TaskStatus::BlockedRecv) => 2,
        Some(TaskStatus::Faulted) => 3,
        _ => 4,
    };
    let pending_cleared = BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref()
            .and_then(|state| state.tasks.find(task_id))
            .is_some_and(|task| task.pending_exc.is_none())
    });
    let frame_saved = crate::sched::kernel_resume_frame(task_id).is_some();
    let frame_bytes_zeroed = resume_frame_bytes(0) == Some([0u8; crate::exc::SRET_FRAME_LEN]);

    // §8.4 步骤 3：再次调度进 Running 时 `kernel_resume_frame` 被消费清空。
    let frame_consumed = schedule_task_global(task_id).is_ok()
        && crate::sched::kernel_resume_frame(task_id).is_none();

    // 前置 6 反例：不可恢复种类再次代调必须被拒且保持 Faulted。
    install_resume_frame(0, &frame.encode())?;
    let unrecoverable_rejected = {
        let faulted = BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
            Some(state) => state
                .tasks
                .fault(task_id, PAGE_FAULT_SOURCE_ID, ExcKind::DoubleFault)
                .is_ok(),
            None => false,
        });
        if faulted {
            BOOTSTRAP_STATE.with_mut(|slot| {
                if let Some(state) = slot.as_mut() {
                    state.audit.append(AuditEvent::TaskStateChanged {
                        task_id,
                        status: TaskStatus::Faulted,
                    });
                }
            });
        }
        matches!(
            kernel_exc_resume(task_id, 0, landing, stack_bottom, stack_top),
            Err(ExcResumeError::UnrecoverableFault)
        ) && task_status(task_id) == Some(TaskStatus::Faulted)
    };

    // 收尾：不可恢复终态 → §8.2 内核回收。
    let reclaimed_after =
        kernel_reclaim_task(task_id).is_ok() && task_status(task_id) != Some(TaskStatus::Faulted);
    let invariant_ok = BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref()
            .is_some_and(|state| state.tasks.check_faulted_invariant().is_ok())
    });

    let witness = ResumeWitness {
        task_id,
        resumed: u32::from(resumed),
        status_after,
        pending_cleared: u32::from(pending_cleared),
        frame_saved: u32::from(frame_saved),
        frame_bytes_zeroed: u32::from(frame_bytes_zeroed),
        frame_consumed: u32::from(frame_consumed),
        unrecoverable_rejected: u32::from(unrecoverable_rejected),
        reclaimed_after: u32::from(reclaimed_after),
        invariant_ok: u32::from(invariant_ok),
    };
    RESUME_WITNESS.with_mut(|slot| *slot = Some(witness));
    Ok(())
}

/// SPEC-073：把 §8.2 内核回收路径包装成布尔（见证用）。
fn kernel_reclaim_task(task_id: u32) -> Result<(), TaskError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.kernel_reclaim_task(task_id, None, 0),
        None => Err(TaskError::NoSuchTask),
    })
}

static RESUME_WITNESS: StaticCell<Option<ResumeWitness>> = StaticCell::new(None);

/// SPEC-073：已登记的 `exc_resume` 见证（未采集时为 `None`）。
pub fn resume_witness_record() -> Option<ResumeWitness> {
    RESUME_WITNESS.with(|slot| *slot)
}

/// SPEC-072：采集并登记内核回收见证（必须在 `activate_init` 之前调用）。
pub fn record_reclaim_witness() -> Result<(), TaskError> {
    let witness = BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.reclaim_witness(),
        None => Err(TaskError::NoSuchTask),
    })?;
    RECLAIM_WITNESS.with_mut(|slot| *slot = Some(witness));
    Ok(())
}

/// SPEC-072：已登记的内核回收见证（未采集时为 `None`）。
pub fn reclaim_witness_record() -> Option<ReclaimWitness> {
    RECLAIM_WITNESS.with(|slot| *slot)
}

static RECLAIM_WITNESS: StaticCell<Option<ReclaimWitness>> = StaticCell::new(None);

/// 取已登记的 `Faulted` 见证（未采集时为 `None`）。
pub fn fault_witness_record() -> Option<FaultWitness> {
    FAULT_WITNESS.with(|slot| *slot)
}

/// 切片 K：全局 Bootstrap 授权表的存储（单核 boot 早期安装一次）。
static BOOTSTRAP_STATE: StaticCell<Option<BootstrapState>> = StaticCell::new(None);

/// 安装 Bootstrap 授权表，供 syscall 授权查询使用（在进入 CPL3 前调用一次）。
pub fn install(state: BootstrapState) {
    BOOTSTRAP_STATE.with_mut(|slot| {
        *slot = Some(state);
    })
}

/// 查询全局授权表：`caller` 是否持有 `cap` 且具备 `right`。
pub fn authorize(caller: u32, cap: Handle, right: Right) -> Result<Handle, AuthError> {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => state.authorize(caller, cap, right),
        None => Err(AuthError::NotInstalled),
    })
}

/// 解码 syscall 传入的裸句柄（未安装时也报 `NotInstalled`）。
pub fn decode_handle(raw: u64) -> Result<Handle, AuthError> {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => state.decode_handle(raw),
        None => Err(AuthError::NotInstalled),
    })
}

/// S2-D4：全局 bring-up——建新 Task 的代码段并铸两条 Capability，返回代码段帧向量。
///
/// SPEC-093：额外建第二个端点（`BETA`）的**独立代码段**与**独立栈段**，返回两个代码段向量。
pub fn prepare_task_create_caps(
    code_pages: usize,
    code2_pages: usize,
    code3_pages: usize,
    phys_offset: u64,
) -> Result<TaskCreateImages, MemsegError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => {
            state.prepare_task_create_caps(code_pages, code2_pages, code3_pages, phys_offset)
        }
        None => Err(MemsegError::TargetDead),
    })
}

/// SPEC-093：查询 `task_id` 在虚拟地址 `va` 处映射所属的 Segment id（无映射返回 `None`）。
///
/// 用于见证两个端点的**栈段相异**（独立物理栈帧），而非仅凭「共享同一段的句柄」推断。
pub fn mapped_segment_at(task_id: u32, va: u64) -> Option<u32> {
    BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref()
            .and_then(|state| state.vspace.find(task_id, va).map(|entry| entry.seg_id))
    })
}

/// S2-D4：全局 `task_create`（§8.1）。
pub fn task_create(
    caller: u32,
    req: &TaskCreateRequest,
    boot_root: u64,
    phys_offset: u64,
) -> Result<TaskCreateOutcome, TaskCreateError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.task_create(caller, req, boot_root, phys_offset),
        None => Err(TaskCreateError::NoSuchCapability),
    })
}

/// S2-D4：全局 `task_destroy`（§8.2）。
pub fn task_destroy(
    caller: u32,
    task_id: u32,
    root_phys: Option<u64>,
    phys_offset: u64,
) -> Result<(), TaskError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.task_destroy(caller, task_id, root_phys, phys_offset),
        None => Err(TaskError::NoSuchTask),
    })
}

/// S2-D4：见证用——活动（非 `Dead`）Task 数。
pub fn live_task_count() -> usize {
    BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref()
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
    })
}

/// S2-D4：见证用——活动 Capability 槽位数。
pub fn live_cap_count() -> usize {
    BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref()
            .map(|state| state.slots.live_count())
            .unwrap_or(0)
    })
}

/// S2-D2：全局 `cap_derive`（§4.4）。
pub fn derive_capability(
    caller: u32,
    parent: Handle,
    rights: Rights,
) -> Result<Handle, DeriveError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.derive_capability(caller, parent, rights),
        None => Err(DeriveError::CapacityExceeded),
    })
}

/// SPEC-081：全局 `ns_create`（§11.2）。
pub fn ns_create(caller: u32) -> Result<Handle, NsError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.ns_create(caller),
        None => Err(NsError::InvalidCaller),
    })
}

/// SPEC-081：全局 `ns_bind`（§11.3）。
pub fn ns_bind(caller: u32, ns_cap: Handle, path: Path, cap: Handle) -> Result<(), NsError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.ns_bind(caller, ns_cap, path, cap),
        None => Err(NsError::InvalidCaller),
    })
}

/// SPEC-081：全局 `ns_unbind`（§11.4）。
pub fn ns_unbind(caller: u32, ns_cap: Handle, path: Path) -> Result<Handle, NsError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.ns_unbind(caller, ns_cap, path),
        None => Err(NsError::InvalidCaller),
    })
}

/// SPEC-081：全局 `ns_mount`（§11.5）。
pub fn ns_mount(
    caller: u32,
    destination_cap: Handle,
    path: Path,
    source_cap: Handle,
) -> Result<(), NsError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.ns_mount(caller, destination_cap, path, source_cap),
        None => Err(NsError::InvalidCaller),
    })
}

/// SPEC-081：全局 `ns_unmount`（§11.6）。
pub fn ns_unmount(caller: u32, ns_cap: Handle, path: Path) -> Result<(), NsError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.ns_unmount(caller, ns_cap, path),
        None => Err(NsError::InvalidCaller),
    })
}

/// SPEC-081：全局 `ns_resolve`（§11.7）。
pub fn ns_resolve(
    caller: u32,
    ns_cap: Handle,
    path: Path,
    requested_rights: Rights,
) -> Result<Handle, NsError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.ns_resolve(caller, ns_cap, path, requested_rights),
        None => Err(NsError::InvalidCaller),
    })
}

/// SPEC-081：全局 `ns_destroy`（§11.8）。
pub fn ns_destroy(caller: u32, ns_cap: Handle) -> Result<(), NsError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.ns_destroy(caller, ns_cap),
        None => Err(NsError::InvalidCaller),
    })
}

/// SPEC-081：记录一次「用户传入的路径文本非法」（`Path::parse` 在 syscall 层失败）。
///
/// 抽象状态不变（§6.2），只计一次实现层观测。
pub fn note_ns_invalid_path() {
    BOOTSTRAP_STATE.with_mut(|slot| {
        if let Some(state) = slot.as_mut() {
            state.ns_rejects.invalid_path += 1;
        }
    });
}

/// SPEC-082：全局 `time_now`（§10.1；未安装时 `None`）。
pub fn time_now() -> Option<Instant> {
    BOOTSTRAP_STATE.with(|slot| slot.as_ref().map(|state| state.time_now()))
}

/// SPEC-082：全局 `timer_set`（§10.2）。
pub fn timer_set(caller: u32, deadline: Instant) -> Result<Handle, TimerError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.timer_set(caller, deadline),
        None => Err(TimerError::InvalidCaller),
    })
}

/// SPEC-082：全局 `timer_cancel`（§10.4）。
pub fn timer_cancel(caller: u32, cap: Handle) -> Result<TimerCancelOutcome, TimerError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.timer_cancel(caller, cap),
        None => Err(TimerError::InvalidCaller),
    })
}

/// SPEC-082：全局 `timer_destroy`（§10.5）。
pub fn timer_destroy(caller: u32, cap: Handle) -> Result<(), TimerError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.timer_destroy(caller, cap),
        None => Err(TimerError::InvalidCaller),
    })
}

/// SPEC-082：全局 `advance_time`（§10.3，`KernelActor` 内部迁移）。
pub fn advance_time(new_now: Instant) -> Result<(), TimerError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.advance_time(new_now),
        None => Err(TimerError::InvalidTime),
    })
}

/// SPEC-082：Timer 的审计汇总（供 `timer_ready` 见证）。
pub fn timer_audit() -> Option<TimerAudit> {
    BOOTSTRAP_STATE.with(|slot| slot.as_ref().map(|state| state.timer_audit()))
}

/// SPEC-081：Namespace 路径操作的审计汇总（供 `namespace_ready` 见证）。
///
/// 供 `namespace_ready` 见证使用——计数直接从审计日志推导，不额外占状态。
pub fn namespace_audit() -> Option<NamespaceAudit> {
    BOOTSTRAP_STATE.with(|slot| slot.as_ref().map(|state| state.namespace_audit()))
}

/// S2-D3：全局 `memseg_create`（§7.1）。
pub fn memseg_create(
    caller: u32,
    page_count: usize,
    allowed_access: Rights,
    phys_offset: u64,
) -> Result<Handle, MemsegError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.memseg_create(caller, page_count, allowed_access, phys_offset),
        None => Err(MemsegError::NotInstalled),
    })
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
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
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
    })
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
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.memseg_unmap(
            caller,
            task_cap,
            target_base,
            page_count,
            root_phys,
            phys_offset,
        ),
        None => Err(MemsegError::NotInstalled),
    })
}

/// S2-D3：全局 `memseg_destroy`（§7.4）。
pub fn memseg_destroy(caller: u32, seg_cap: Handle) -> Result<(), MemsegError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.memseg_destroy(caller, seg_cap),
        None => Err(MemsegError::NotInstalled),
    })
}

/// S2-D2：全局 `cap_drop`（§4.5）。
pub fn drop_capability(caller: u32, cap: Handle) -> Result<(), DropError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.drop_capability(caller, cap),
        None => Err(DropError::NotInstalled),
    })
}

/// 切片 L：平台初始化——创建预定义异常源并实证一次 pending 直投（进入 CPL3 前调用）。
pub fn platform_init_exception_sources() -> Result<(), ExcError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.platform_init(),
        None => Err(ExcError::NotInstalled),
    })
}

/// 切片 R：全局平台 Segment 创建（真实 Segment 对象 + 内核持有的可授权根 Capability）。
pub fn platform_create_segment(
    id: u32,
    page_count: usize,
    allowed_access: Rights,
    phys_offset: u64,
) -> Result<(Handle, SegmentFrames), MemsegError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.platform_create_segment(id, page_count, allowed_access, phys_offset),
        None => Err(MemsegError::NotInstalled),
    })
}

/// 切片 R：平台 Segment 的第 `index` 帧。
pub fn platform_segment_frame(id: u32, index: usize) -> Result<u64, MemsegError> {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => state.platform_segment_frame(id, index),
        None => Err(MemsegError::NotInstalled),
    })
}

/// 切片 R：为内核私有平台 Segment 追加一帧（COW 段 fault-time 私有页）。
pub fn platform_append_frame(id: u32) -> Result<u64, MemsegError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.platform_append_frame(id),
        None => Err(MemsegError::NotInstalled),
    })
}

/// 切片 R：铸 task 1 / task 2 的平台 Task Capability（须在 `platform_init` 之后）。
pub fn platform_prepare_task_caps() -> Result<(), MemsegError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.platform_prepare_task_caps(),
        None => Err(MemsegError::NotInstalled),
    })
}

/// SPEC-066：按属主移交平台段 / Task Capability。
///
/// 必须在 bring-up 全部 `map_region` 之后、进入用户态之前调用（见
/// [`BootstrapState::handover_platform_caps`]）。未安装状态归为 `NoSuchCapability`
/// ——与「槽/对象不存在」同一失败语义。
pub fn handover_platform_caps() -> Result<(), CapabilityPositionError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.handover_platform_caps(),
        None => Err(CapabilityPositionError::NoSuchCapability),
    })
}

/// 切片 S：播种用户 IPC 见证所需的两条 Capability（须在 `platform_prepare_task_caps` 之后）。
pub fn prepare_user_ipc_caps() -> Result<(), MemsegError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.prepare_user_ipc_caps(),
        None => Err(MemsegError::NotInstalled),
    })
}

/// 切片 S：句柄是否指向某个 Task（供 syscall 层派生目标根）。
pub fn task_of_cap(cap: Handle) -> Option<u32> {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => state.task_of_cap(cap),
        None => None,
    })
}

/// 切片 S：接收者是否处于 `BlockedRecv`（供 `SYS_SEND` 判定投递路径）。
pub fn is_blocked_recv_global(task_id: u32) -> bool {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => state.tasks.is_blocked_recv(task_id),
        None => false,
    })
}

/// 切片 S：取走某个 Task 的 pending 消息（直投后由 `SYS_SEND` 消费，避免被后续 `SYS_RECV` 重复投递）。
pub fn take_pending_global(task_id: u32) -> Option<IpcMessage> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.tasks.take_pending(task_id),
        None => None,
    })
}

/// 切片 S：全局发送（接收者绑定的授权在 `BootstrapState` 内完成）。
pub fn send_message_with_capabilities(
    sender: u32,
    receiver: u32,
    send_cap: Handle,
    payload: u64,
    capability_ids: &[Handle],
) -> Result<(), IpcError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.send_message_with_capabilities(
            sender,
            receiver,
            send_cap,
            payload,
            capability_ids,
        ),
        None => Err(IpcError::NoSuchTask),
    })
}

/// 切片 S：全局接收（空队列时会把接收者置为 `BlockedRecv`）。
pub fn receive_message(receiver: u32) -> Result<IpcMessage, IpcError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.receive_message(receiver),
        None => Err(IpcError::NoSuchTask),
    })
}

/// 切片 R：读取平台段对象化见证记录（供 `platform_segments_ready` 事件）。
pub fn platform_segments_record() -> Result<PlatformSegmentsRecord, BootstrapError> {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => Ok(state.platform_segments_record()),
        None => Err(BootstrapError::InvalidAuthority),
    })
}

/// 切片 L：全局订阅查询（供 syscall 层使用）。
pub fn exc_subscribe(caller: u32, cap: Handle, kind: ExcKind) -> Result<u32, ExcError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.exc_subscribe(caller, cap, kind),
        None => Err(ExcError::NotInstalled),
    })
}

/// 切片 L：全局取消订阅（供 syscall 层使用）。
pub fn exc_unsubscribe(caller: u32, cap: Handle) -> Result<u32, ExcError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.exc_unsubscribe(caller, cap),
        None => Err(ExcError::NotInstalled),
    })
}

/// 切片 L：读取异常相关 Audit 计数（授权表未安装时返回 `None`）。
pub fn exc_audit_counts() -> Option<ExcAuditCounts> {
    BOOTSTRAP_STATE.with(|slot| slot.as_ref().map(|state| state.exc_audit_counts()))
}

/// 切片 L：读取平台初始化期间的 pending 递送留证。
pub fn exc_boot_record() -> Option<ExcBootRecord> {
    BOOTSTRAP_STATE.with(|slot| slot.as_ref().and_then(|state| state.exc_boot))
}

/// 切片 M：把 `task_id` 从 Running 让出为 Ready（协作式调度用）。
pub fn yield_task_global(task_id: u32) -> Result<(), TaskError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.yield_task(task_id),
        None => Err(TaskError::NoSuchTask),
    })
}

/// 切片 M：把 Ready 的 `task_id` 调度为 Running（协作式调度用）。
pub fn schedule_task_global(task_id: u32) -> Result<(), TaskError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.schedule_task(task_id),
        None => Err(TaskError::NoSuchTask),
    })
}

/// 切片 M：读取任务状态（授权表未安装或任务不存在时返回 `None`）。
pub fn task_status(task_id: u32) -> Option<TaskStatus> {
    BOOTSTRAP_STATE.with(|slot| {
        slot.as_ref()
            .and_then(|state| state.tasks.find(task_id))
            .map(|task| task.status)
    })
}

/// 切片 N：把 `task_id` 置为 BlockedRecv（`SYS_EXC_WAIT` 阻塞调用者用）。
pub fn block_recv_global(task_id: u32) -> Result<(), TaskError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.block_recv_task(task_id),
        None => Err(TaskError::NoSuchTask),
    })
}

/// 切片 N：内核 actor 触发一次 `raise_exception`（供 `SYS_TRAP` 使用）。
pub fn exc_raise(source_id: u32, kind: ExcKind) -> Result<RaiseOutcome, ExcError> {
    BOOTSTRAP_STATE.with_mut(|slot| match slot.as_mut() {
        Some(state) => state.exc_raise(source_id, kind),
        None => Err(ExcError::NotInstalled),
    })
}

/// 切片 N：查询 `caller` 是否已订阅 `cap` 所指源的 `kind`。
pub fn exc_subscribed(caller: u32, cap: Handle, kind: ExcKind) -> Result<u32, ExcError> {
    BOOTSTRAP_STATE.with(|slot| match slot.as_ref() {
        Some(state) => state.exc_subscribed(caller, cap, kind),
        None => Err(ExcError::NotInstalled),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AuditError, AuditEvent, AuthError, AuthorityError, BootstrapError, CapabilityLocation,
        CapabilityPositionError, DeriveError, DropError, EXC_PAGE_FAULT_HANDLE, EXC_SOURCE_RIGHTS,
        EXC_SYSCALL_TRAP_HANDLE, ExcError, ExcKind, ExcResumeError, Handle, IMAGE_CAP_HANDLE,
        IMAGE_CAP_SLOT, IMAGE_RIGHTS, IMAGE_SEGMENT_ID, INIT_TASK_ID, INITIAL_CAP_GENERATION,
        INITIAL_CAPABILITY_COUNT, InitialObject, IpcError, KERNEL_ACTOR, MAX_CAPABILITY_SLOTS,
        MAX_NAMESPACE_OBJECTS, MAX_SEGMENT_FRAMES, MemsegError, NAMESPACE_CAP_HANDLE,
        NAMESPACE_RIGHTS, NS_ROOT_CAP_SLOT_BASE, NsError, ObjectRef, PAGE_FAULT_SOURCE_ID,
        PLATFORM_SEGMENT_CAP_SLOT_BASE, PLATFORM_SEGMENT_COUNT, PLATFORM_SEGMENT_ID_FIRST,
        PLATFORM_SEGMENT_ID_LAST, PLATFORM_TASK_RIGHTS, PLATFORM_TASK1_CAP_HANDLE,
        PLATFORM_TASK2_CAP_HANDLE, Path, PendingExc, SEGMENT_ROOT_RIGHTS, SERVICE_STACK_CAP_SLOT,
        SYSCALL_TRAP_SOURCE_ID, SegmentFrames, SegmentObject, TASK_CAP_HANDLE, TASK_RIGHTS,
        TASK2_SELF_CAP_HANDLE, TASK2_SEND_CAP_HANDLE, TIMER_RIGHTS, TaskError, TaskStatus,
        TimerCancelOutcome, TimerCancelReason, TimerError, TimerState, USER_IPC_CAP_SLOT_BASE,
        USER_SEGMENT_ID_FIRST, VspaceEntry, create_snapshot, create_state, entry_in_image,
        platform_segment_cap_handle, platform_segment_cap_slot, platform_segment_is_direct,
    };
    use crate::mem::FrameInventory;
    use crate::rights::{ObjectKind, Right, Rights};

    #[test]
    fn tlb_flush_only_for_current_root() {
        assert!(super::needs_local_tlb_flush(0x1000, Some(0x1000)));
        assert!(!super::needs_local_tlb_flush(0x2000, Some(0x1000)));
        assert!(!super::needs_local_tlb_flush(0x1000, None));
    }

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

    /// ISSUE-CAP-001 反例：不可信 ring3 可构造**格式合法但槽索引越界**的句柄
    /// （`0xffff_ffff_ffff_ffea`：index `0xffffffea` ≥ `MAX_CAPABILITY_SLOTS`、generation 非零）。
    /// 此类句柄在解码边界即被拒，且三张按槽直索引的表一律安全失败（`None`/`false`），不得 panic。
    #[test]
    fn malformed_out_of_range_handles_fail_safety_not_panic() {
        let mut state = fresh_state();
        let bad = Handle::decode(0xffff_ffff_ffff_ffea).expect("format-valid, range-invalid");
        assert!(bad.index() as usize >= MAX_CAPABILITY_SLOTS);
        let before = state;

        // 解码边界（syscall/协议入口）拒绝越界索引。
        assert_eq!(
            state.decode_handle(0xffff_ffff_ffff_ffea),
            Err(AuthError::MalformedHandle)
        );
        // 记录 / 位置 / 槽三张表的按槽访问都安全失败。
        assert!(state.records.find(bad).is_none());
        assert!(state.positions.find(bad).is_none());
        assert!(state.holder_of(bad).is_none());
        assert!(state.rights_of(bad).is_none());
        assert!(state.target_of(bad).is_none());
        assert_eq!(
            state.authorize(INIT_TASK_ID, bad, Right::SegMapR),
            Err(AuthError::NoSuchCapability)
        );
        assert!(!state.positions.remove(bad));
        // 全部为纯查询/拒绝，状态逐字段不变。
        assert_eq!(state, before);
    }

    // -----------------------------------------------------------------------
    // SPEC-081：§11 Namespace 路径操作
    // -----------------------------------------------------------------------

    fn path(text: &str) -> Path {
        Path::parse(text.as_bytes()).expect("test path parses")
    }

    fn ns_of(state: &super::BootstrapState, cap: Handle) -> u32 {
        match state.target_of(cap) {
            Some(ObjectRef::Namespace(id)) => id,
            _ => panic!("cap does not target a namespace"),
        }
    }

    #[test]
    fn audit_trace_reconstructs_parent_and_position_without_mutating() {
        let mut state = fresh_state();
        let child = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("SegMapR"))
            .expect("derive");
        state
            .ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("x"), child)
            .expect("bind");
        assert_eq!(
            state.audit_trace(INIT_TASK_ID, child),
            Err(AuditError::NotHeld)
        );
        state
            .ns_unbind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("x"))
            .expect("unbind");
        let before = state.audit_len();
        let trace = state.audit_trace(INIT_TASK_ID, child).expect("query");
        assert_eq!(&trace.chain[..trace.chain_len], &[IMAGE_CAP_HANDLE, child]);
        assert_eq!((trace.minted, trace.derived, trace.moves), (0, 1, 2));
        assert_eq!((trace.created, trace.destroyed), (1, 0));
        assert!(trace.target_live && trace.origin_none);
        assert_eq!(state.audit_len(), before);
    }

    #[test]
    fn audit_trace_follows_a_dropped_and_reused_parent_slot() {
        let mut state = fresh_state();
        let parent = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("Derive,SegMapR"))
            .expect("parent");
        let child = state
            .derive_capability(INIT_TASK_ID, parent, rights("SegMapR"))
            .expect("child");
        state
            .drop_capability(INIT_TASK_ID, parent)
            .expect("drop parent");
        let replacement = state
            .derive_capability(INIT_TASK_ID, IMAGE_CAP_HANDLE, rights("SegMapR"))
            .expect("reuse slot");
        assert_eq!(parent.index(), replacement.index());
        assert_ne!(parent, replacement);
        let trace = state.audit_trace(INIT_TASK_ID, child).expect("query child");
        assert_eq!(
            &trace.chain[..trace.chain_len],
            &[IMAGE_CAP_HANDLE, parent, child]
        );
        assert_eq!(trace.derived, 2);
    }

    #[test]
    fn audit_trace_accepts_a_held_capability_with_dead_target() {
        let mut state = fresh_state();
        let ns = state.ns_create(INIT_TASK_ID).expect("create");
        state.ns_destroy(INIT_TASK_ID, ns).expect("destroy");
        let trace = state
            .audit_trace(INIT_TASK_ID, ns)
            .expect("dead target is queryable");
        assert_eq!(&trace.chain[..trace.chain_len], &[ns]);
        assert_eq!(trace.minted, 1);
        assert_eq!((trace.created, trace.destroyed), (1, 1));
        assert!(!trace.target_live);
    }

    #[test]
    fn audit_trace_rejects_invalid_caller_cap_and_foreign_holder() {
        let mut state = fresh_state();
        assert_eq!(
            state.audit_trace(99, IMAGE_CAP_HANDLE),
            Err(AuditError::InvalidCaller)
        );
        assert_eq!(
            state.audit_trace(INIT_TASK_ID, Handle::ZERO),
            Err(AuditError::InvalidCap)
        );
        state.create_task(2).expect("task");
        assert_eq!(
            state.audit_trace(2, IMAGE_CAP_HANDLE),
            Err(AuditError::NotHeld)
        );
    }

    #[test]
    fn ns_create_mints_a_root_capability_in_the_reserved_slot_range() {
        let mut state = fresh_state();
        assert_eq!(state.namespace_count, 1);
        let cap = state.ns_create(INIT_TASK_ID).expect("create succeeds");
        assert_eq!(cap.index(), NS_ROOT_CAP_SLOT_BASE);
        assert_eq!(cap.generation(), INITIAL_CAP_GENERATION);
        assert_eq!(state.namespace_count, 2);
        assert_eq!(ns_of(&state, cap), 2);
        assert_eq!(state.holder_of(cap), Some(INIT_TASK_ID));
        assert_eq!(state.rights_of(cap), Some(NAMESPACE_RIGHTS));
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::CapabilityMinted { cap: minted, holder }
                if *minted == cap && *holder == INIT_TASK_ID
        )));
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::ObjectCreated {
                object_id: 2,
                segment: false
            }
        )));
    }

    /// ISSUE-CAP-002：Namespace 根 cap 预留区间必须与 usercap 固定块、`UNSET_CAP` 哨兵
    /// 与服务栈槽**结构上不相交**——这是原 52..59 基址与 usercap 47..56 重叠致
    /// `ns_create` 只剩 3 槽的回归守卫。
    #[test]
    fn ns_reserved_range_is_disjoint_from_fixed_cap_slots() {
        const UNSET_CAP_SLOT: u32 = 60;
        let ns_start = NS_ROOT_CAP_SLOT_BASE;
        let ns_end = ns_start + MAX_NAMESPACE_OBJECTS as u32;
        // usercap 固定块 = [USER_IPC_CAP_SLOT_BASE, USER_IPC_CAP_SLOT_BASE + 13)（+0/+1 切片 S，
        // +2/+3 task_create，+4 task4，+5 client，+6 service，+7..+9 service2，+10..+12 servicemgr）。
        let usercap_end = USER_IPC_CAP_SLOT_BASE + 13;
        assert!(
            ns_end <= MAX_CAPABILITY_SLOTS as u32,
            "NS 预留区间必须落在槽位空间内"
        );
        assert!(
            usercap_end <= ns_start,
            "usercap 固定块必须整体位于 NS 预留区间之下（原重叠缺陷）"
        );
        assert!(
            SERVICE_STACK_CAP_SLOT < ns_start && SERVICE_STACK_CAP_SLOT > UNSET_CAP_SLOT,
            "服务栈槽须避开 UNSET_CAP 哨兵并落在 NS 区间之下"
        );
    }

    #[test]
    fn ns_create_rejects_a_non_live_caller() {
        let mut state = fresh_state();
        assert_eq!(state.ns_create(2), Err(NsError::InvalidCaller));
    }

    #[test]
    fn ns_bind_moves_the_capability_into_the_namespace() {
        let mut state = fresh_state();
        state
            .ns_bind(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("lib"),
                IMAGE_CAP_HANDLE,
            )
            .expect("bind succeeds");
        assert_eq!(
            state
                .positions
                .find(IMAGE_CAP_HANDLE)
                .map(|position| position.location),
            Some(CapabilityLocation::InNamespace(1))
        );
        // 位于命名空间中的 Capability 没有 Task 持有者——`derive`/`drop` 因此自然拒绝它。
        assert_eq!(state.holder_of(IMAGE_CAP_HANDLE), None);
        assert_eq!(state.ns_entries.count_in(1), 1);
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::NsBound { ns: 1, cap, .. } if *cap == IMAGE_CAP_HANDLE
        )));
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::CapabilityMoved {
                cap,
                from: CapabilityLocation::HeldByTask(INIT_TASK_ID),
                to: CapabilityLocation::InNamespace(1),
            } if *cap == IMAGE_CAP_HANDLE
        )));
    }

    #[test]
    fn ns_bind_rejects_overlap_including_ancestors_and_descendants() {
        let mut state = fresh_state();
        let first = state.ns_create(INIT_TASK_ID).expect("create");
        let second = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("a"), first)
            .expect("first bind");
        // 同名条目。
        assert_eq!(
            state.ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("a"), second),
            Err(NsError::Overlap)
        );
        // 终端祖先（`a` 是 `a/b` 的严格前缀）。
        assert_eq!(
            state.ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("a/b"), second),
            Err(NsError::Overlap)
        );
        // 后代条目（先绑 `c/d`，再绑 `c`）。
        let third = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("c/d"), second)
            .expect("bind descendant");
        assert_eq!(
            state.ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("c"), third),
            Err(NsError::Overlap)
        );
    }

    #[test]
    fn ns_bind_rejects_a_structural_root_namespace_capability() {
        let mut state = fresh_state();
        assert_eq!(
            state.ns_bind(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("root"),
                NAMESPACE_CAP_HANDLE
            ),
            Err(NsError::StructuralCap)
        );
    }

    #[test]
    fn ns_bind_rejects_missing_rights_and_foreign_namespaces() {
        let mut state = fresh_state();
        let cap = state.ns_create(INIT_TASK_ID).expect("create");
        // `cap` 只含 NsResolve/NsBind/... 但没有 Init 的根 cap 那种……用 Segment cap 当 ns_cap：
        assert_eq!(
            state.ns_bind(INIT_TASK_ID, IMAGE_CAP_HANDLE, path("x"), cap),
            Err(NsError::InvalidNamespace)
        );
        // ns_cap 是 TASK cap（也不是 Namespace）。
        assert_eq!(
            state.ns_bind(INIT_TASK_ID, TASK_CAP_HANDLE, path("x"), cap),
            Err(NsError::InvalidNamespace)
        );
    }

    #[test]
    fn ns_unbind_returns_the_capability_to_the_caller() {
        let mut state = fresh_state();
        let cap = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("lib"), cap)
            .expect("bind");
        let returned = state
            .ns_unbind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("lib"))
            .expect("unbind");
        assert_eq!(returned, cap);
        assert_eq!(state.holder_of(cap), Some(INIT_TASK_ID));
        assert_eq!(state.ns_entries.count_in(1), 0);
        assert_eq!(
            state.ns_unbind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("lib")),
            Err(NsError::NotFound)
        );
    }

    #[test]
    fn ns_unbind_rejects_a_mount_entry() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_mount(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("mnt"), a)
            .expect("mount");
        assert_eq!(
            state.ns_unbind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("mnt")),
            Err(NsError::NotFound)
        );
    }

    #[test]
    fn ns_mount_links_namespaces_and_rejects_cycles() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create a");
        let b = state.ns_create(INIT_TASK_ID).expect("create b");
        let a_ns = ns_of(&state, a);
        let b_ns = ns_of(&state, b);
        state
            .ns_mount(INIT_TASK_ID, a, path("x"), b)
            .expect("mount b under a");
        // 已有边 a → b；再加 b → a 会成环。
        assert_eq!(
            state.ns_mount(INIT_TASK_ID, b, path("y"), a),
            Err(NsError::MountCycle)
        );
        assert!(state.namespace_reaches(a_ns, b_ns));
        assert!(!state.namespace_reaches(b_ns, a_ns));
        // 自挂（destination == source）也成环。
        assert_eq!(
            state.ns_mount(INIT_TASK_ID, a, path("self"), a),
            Err(NsError::MountCycle)
        );
        assert_eq!(state.ns_entries.count_in(a_ns), 1);
    }

    #[test]
    fn ns_mount_requires_a_delegable_source() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create a");
        let b = state.ns_create(INIT_TASK_ID).expect("create b");
        let bare = state
            .derive_capability(INIT_TASK_ID, a, Rights::from_rights(&[Right::Derive]))
            .expect("derive");
        assert_eq!(
            state.ns_mount(INIT_TASK_ID, b, path("x"), bare),
            Err(NsError::NotDelegable)
        );
        // 非 Namespace 的 source 直接无效。
        assert_eq!(
            state.ns_mount(INIT_TASK_ID, b, path("x"), TASK_CAP_HANDLE),
            Err(NsError::InvalidNamespace)
        );
    }

    #[test]
    fn ns_resolve_walks_bound_and_mounted_entries() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_bind(INIT_TASK_ID, a, path("sub"), IMAGE_CAP_HANDLE)
            .expect("bind inside a");
        state
            .ns_mount(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("mnt"), a)
            .expect("mount a");
        let derived = state
            .ns_resolve(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("mnt/sub"),
                Rights::from_rights(&[Right::SegMapR]),
            )
            .expect("resolve through the mount");
        assert_eq!(state.holder_of(derived), Some(INIT_TASK_ID));
        assert_eq!(
            state.target_of(derived),
            Some(ObjectRef::Segment(IMAGE_SEGMENT_ID))
        );
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::NsResolved {
                start_cap,
                mount_count: 1,
                parent,
                cap,
                ..
            } if *start_cap == NAMESPACE_CAP_HANDLE
                && *parent == IMAGE_CAP_HANDLE
                && *cap == derived
        )));
    }

    #[test]
    fn ns_resolve_rejects_missing_and_non_directory_paths() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("lib"), a)
            .expect("bind");
        let ns_resolve_rights = Rights::from_rights(&[Right::NsResolve]);
        assert_eq!(
            state.ns_resolve(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("nope"),
                ns_resolve_rights
            ),
            Err(NsError::NotFound)
        );
        assert_eq!(
            state.ns_resolve(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("lib/x"),
                ns_resolve_rights
            ),
            Err(NsError::NotDirectory)
        );
    }

    #[test]
    fn ns_resolve_enforces_the_parent_rights_subset() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("lib"), a)
            .expect("bind");
        // 父项权限是 NAMESPACE_RIGHTS（不含 SegMapR）⇒ 请求 Segment 权限被拒。
        assert_eq!(
            state.ns_resolve(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("lib"),
                Rights::from_rights(&[Right::SegMapR])
            ),
            Err(NsError::RightsExceeded)
        );
    }

    #[test]
    fn ns_resolve_terminates_on_the_deepest_path() {
        // S3-X1：解析步数受路径分量数约束——最深的合法路径也必须终止并给出确定结果。
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        state
            .ns_mount(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("m"), a)
            .expect("mount");
        let result = state.ns_resolve(
            INIT_TASK_ID,
            NAMESPACE_CAP_HANDLE,
            path("m/a/b/c"),
            Rights::from_rights(&[Right::NsResolve]),
        );
        assert!(result.is_err());
    }

    #[test]
    fn ns_resolve_reports_mount_target_dead_for_stale_mounts() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        // 另派生一个持有 Capability，供挂载后销毁源 Namespace 使用。
        let destroy_cap = state
            .derive_capability(INIT_TASK_ID, a, NAMESPACE_RIGHTS)
            .expect("derive");
        state
            .ns_mount(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("mnt"), a)
            .expect("mount");
        state
            .ns_destroy(INIT_TASK_ID, destroy_cap)
            .expect("destroy source");
        // stale Mount 保留在原路径（§11.1 约束 5），解析得 `MountTargetDead`。
        assert_eq!(
            state.ns_resolve(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("mnt"),
                Rights::from_rights(&[Right::NsResolve])
            ),
            Err(NsError::MountTargetDead)
        );
        // 它只能被 `ns_unmount` 清理（§11.6：源已销毁不构成失败）。
        state
            .ns_unmount(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("mnt"))
            .expect("unmount stale mount");
        assert_eq!(state.ns_entries.count_in(1), 0);
    }

    #[test]
    fn ns_destroy_rejects_a_structural_root_namespace() {
        let mut state = fresh_state();
        assert_eq!(
            state.ns_destroy(INIT_TASK_ID, NAMESPACE_CAP_HANDLE),
            Err(NsError::StructuralCap)
        );
    }

    #[test]
    fn ns_destroy_clears_entries_and_removes_the_object() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        let keeper = state
            .derive_capability(INIT_TASK_ID, a, NAMESPACE_RIGHTS)
            .expect("derive keeper");
        state
            .ns_bind(INIT_TASK_ID, a, path("lib"), IMAGE_CAP_HANDLE)
            .expect("bind");
        let a_ns = ns_of(&state, a);
        assert_eq!(state.ns_entries.count_in(a_ns), 1);
        state.ns_destroy(INIT_TASK_ID, keeper).expect("destroy");
        assert_eq!(state.ns_entries.count_in(a_ns), 0);
        assert_eq!(state.namespace_count, 1);
        assert!(!state.target_live(ObjectRef::Namespace(a_ns)));
        // §11.8：指向已销毁 Namespace 的 Capability 仍在（成为 stale token）。
        assert_eq!(state.holder_of(keeper), Some(INIT_TASK_ID));
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::ObjectDestroyed { object_id, .. } if *object_id == a_ns
        )));
    }

    #[test]
    fn failed_ns_operations_leave_the_state_unchanged() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create");
        let audit_before = state.audit.len();
        let entries_before = state.ns_entries.count();
        // 环检测失败：不改状态、不追加事件。
        assert_eq!(
            state.ns_mount(INIT_TASK_ID, a, path("self"), a),
            Err(NsError::MountCycle)
        );
        // 非目录解析失败：同样不改状态。
        state
            .ns_bind(INIT_TASK_ID, NAMESPACE_CAP_HANDLE, path("lib"), a)
            .expect("bind");
        let audit_after_bind = state.audit.len();
        let entries_after_bind = state.ns_entries.count();
        assert_eq!(
            state.ns_resolve(
                INIT_TASK_ID,
                NAMESPACE_CAP_HANDLE,
                path("lib/x"),
                Rights::from_rights(&[Right::NsResolve])
            ),
            Err(NsError::NotDirectory)
        );
        assert_eq!(state.audit.len(), audit_after_bind);
        assert_eq!(state.ns_entries.count(), entries_after_bind);
        assert!(state.audit.len() > audit_before);
        assert_eq!(entries_before, 0);
    }

    #[test]
    fn namespace_reaches_is_a_bounded_dfs() {
        let mut state = fresh_state();
        let a = state.ns_create(INIT_TASK_ID).expect("create a");
        let b = state.ns_create(INIT_TASK_ID).expect("create b");
        let c = state.ns_create(INIT_TASK_ID).expect("create c");
        let (a_ns, c_ns) = (ns_of(&state, a), ns_of(&state, c));
        state.ns_mount(INIT_TASK_ID, a, path("b"), b).expect("a→b");
        state.ns_mount(INIT_TASK_ID, b, path("c"), c).expect("b→c");
        assert!(state.namespace_reaches(a_ns, c_ns));
        assert!(!state.namespace_reaches(c_ns, a_ns));
        // 自身恒可达（`ns_mount` 因此拒绝自挂）。
        assert!(state.namespace_reaches(c_ns, c_ns));
    }

    // -----------------------------------------------------------------------
    // SPEC-082：§10 Timer
    // -----------------------------------------------------------------------

    fn count_events(
        state: &super::BootstrapState,
        mut pred: impl FnMut(&AuditEvent) -> bool,
    ) -> usize {
        let mut count = 0;
        state.audit.for_each(|event| {
            if pred(event) {
                count += 1;
            }
        });
        count
    }

    #[test]
    fn timer_set_with_a_future_deadline_leaves_it_pending() {
        let mut state = fresh_state();
        assert_eq!(state.time_now(), 0);
        let cap = state.timer_set(INIT_TASK_ID, 2).expect("set");
        assert_eq!(state.rights_of(cap), Some(TIMER_RIGHTS));
        assert_eq!(state.holder_of(cap), Some(INIT_TASK_ID));
        let timer = state.timers.find(1).expect("timer 1 exists");
        assert_eq!(timer.creator, INIT_TASK_ID);
        assert!(timer.state.is_pending());
        assert_eq!(timer.state.deadline(), 2);
        assert_eq!(timer.state.recipient(), INIT_TASK_ID);
        assert_eq!(
            count_events(&state, |event| matches!(
                event,
                AuditEvent::TimerSet { timer_id: 1, .. }
            )),
            1
        );
    }

    #[test]
    fn timer_set_with_a_past_deadline_fires_in_the_same_operation() {
        // S3-D4：过去/当前的 deadline **不是错误**，且在同一原子操作内触发。
        let mut state = fresh_state();
        let cap = state.timer_set(INIT_TASK_ID, 0).expect("set");
        let timer = state.timers.find(1).expect("timer 1 exists");
        let TimerState::Fired {
            deadline,
            fired_at,
            notification,
            ..
        } = timer.state
        else {
            panic!("timer must be Fired immediately");
        };
        assert_eq!((deadline, fired_at), (0, 0));
        assert_eq!(state.timer_stats.immediate_fired, 1);
        assert_eq!(
            count_events(&state, |event| matches!(
                event,
                AuditEvent::TimerFired { timer_id: 1, .. }
            )),
            1
        );
        assert_eq!(
            count_events(&state, |event| matches!(
                event,
                AuditEvent::MessageSent { message_id, .. } if *message_id == notification
            )),
            1
        );
        assert!(!cap.is_zero());
    }

    #[test]
    fn advance_time_rejects_a_backwards_clock_without_mutating() {
        let mut state = fresh_state();
        state.advance_time(5).expect("advance forward");
        let _ = state.timer_set(INIT_TASK_ID, 9).expect("set");
        let events_before = state.audit.len();
        // §10.3：`new_now < now` ⇒ `InvalidTime`，`now` 与全部 Timer 状态不变。
        assert_eq!(state.advance_time(4), Err(TimerError::InvalidTime));
        assert_eq!(state.time_now(), 5);
        assert_eq!(
            state.timers.find(1).map(|timer| timer.state.deadline()),
            Some(9)
        );
        assert_eq!(state.audit.len(), events_before);
    }

    #[test]
    fn advance_time_fires_due_timers_in_stable_order() {
        let mut state = fresh_state();
        let _first = state.timer_set(INIT_TASK_ID, 3).expect("first");
        let _second = state.timer_set(INIT_TASK_ID, 3).expect("second");
        let _late = state.timer_set(INIT_TASK_ID, 9).expect("late");
        state.advance_time(3).expect("advance");
        // §10.3 步骤 3：按 `(deadline, TimerId)` 稳定序处理——同 deadline 时 id 升序。
        let mut fired = [0u32; 4];
        let mut count = 0;
        state.audit.for_each(|event| {
            if let AuditEvent::TimerFired { timer_id, .. } = event {
                fired[count] = *timer_id;
                count += 1;
            }
        });
        assert_eq!(&fired[..count], &[1, 2]);
        assert!(
            state
                .timers
                .find(1)
                .is_some_and(|timer| timer.state.is_fired())
        );
        assert!(
            state
                .timers
                .find(2)
                .is_some_and(|timer| timer.state.is_fired())
        );
        // 未到期者保持 `Pending`（§10.3 迁移后不变式：所有 Pending 的 deadline > now）。
        assert!(
            state
                .timers
                .find(3)
                .is_some_and(|timer| timer.state.is_pending())
        );
        assert_eq!(state.time_now(), 3);
    }

    #[test]
    fn timer_cancel_returns_state_outcomes_not_failures() {
        let mut state = fresh_state();
        let pending = state.timer_set(INIT_TASK_ID, 5).expect("pending");
        assert_eq!(
            state.timer_cancel(INIT_TASK_ID, pending),
            Ok(TimerCancelOutcome::Cancelled)
        );
        assert!(
            state
                .timers
                .find(1)
                .is_some_and(|timer| timer.state.is_cancelled())
        );
        // 再次取消：`AlreadyCancelled` 是**状态返回**，状态与审计都不变。
        let events_before = state.audit.len();
        assert_eq!(
            state.timer_cancel(INIT_TASK_ID, pending),
            Ok(TimerCancelOutcome::AlreadyCancelled)
        );
        assert_eq!(state.audit.len(), events_before);
        assert_eq!(state.timer_stats.already_cancelled, 1);
        // `Fired` 的取消：`AlreadyFired`，状态不变。
        let fired = state.timer_set(INIT_TASK_ID, 0).expect("immediate");
        assert_eq!(
            state.timer_cancel(INIT_TASK_ID, fired),
            Ok(TimerCancelOutcome::AlreadyFired)
        );
        assert_eq!(state.timer_stats.already_fired, 1);
        assert!(
            state
                .timers
                .find(2)
                .is_some_and(|timer| timer.state.is_fired())
        );
    }

    #[test]
    fn timer_destroy_cancels_pending_and_keeps_a_fired_notification() {
        let mut state = fresh_state();
        let pending = state.timer_set(INIT_TASK_ID, 7).expect("pending");
        let fired = state.timer_set(INIT_TASK_ID, 0).expect("immediate");
        let notification = state.timers.find(2).map(|timer| match timer.state {
            TimerState::Fired { notification, .. } => notification,
            _ => 0,
        });
        state
            .timer_destroy(INIT_TASK_ID, pending)
            .expect("destroy pending");
        state
            .timer_destroy(INIT_TASK_ID, fired)
            .expect("destroy fired");
        assert_eq!(state.timers.count, 0);
        // §10.5：`Pending` 的销毁先产生取消事件…
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::TimerCancelled {
                timer_id: 1,
                reason: TimerCancelReason::ByTask(_)
            }
        )));
        // …而 `Fired` 的已排队通知**不撤回**。
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::MessageSent { message_id, .. } if Some(*message_id) == notification
        )));
        // 指向已销毁 Timer 的 Capability 失效（§10.5 的 stale token）。
        assert_eq!(
            state.timer_cancel(INIT_TASK_ID, pending),
            Err(TimerError::InvalidTimer)
        );
    }

    #[test]
    fn destroying_a_recipient_cancels_its_timers() {
        // §8.2 步骤 5 + W11。
        let mut state = fresh_state();
        let _ = state.timer_set(INIT_TASK_ID, 3).expect("set");
        assert_eq!(state.timers.count, 1);
        assert_eq!(state.destroy_timers_of_recipient(INIT_TASK_ID), 1);
        assert_eq!(state.timers.count, 0);
        assert!(state.audit.any(|event| matches!(
            event,
            AuditEvent::TimerCancelled {
                timer_id: 1,
                reason: TimerCancelReason::RecipientDestroyed
            }
        )));
        // W11：删除后活动 Timer 表不再含以该 Task 为 recipient 的条目。
        assert!(
            !state
                .timers
                .entries
                .iter()
                .flatten()
                .any(|timer| timer.state.recipient() == INIT_TASK_ID)
        );
    }

    #[test]
    fn destroying_one_recipient_preserves_other_recipients_timers() {
        let mut state = fresh_state();
        state.create_task(2).expect("second recipient");
        let _first = state.timer_set(INIT_TASK_ID, 3).expect("first timer");
        let second = state.timer_set(2, 7).expect("second timer");
        assert_eq!(state.destroy_timers_of_recipient(INIT_TASK_ID), 1);
        assert_eq!(state.timers.count, 1);
        assert!(
            state
                .timers
                .find(2)
                .is_some_and(|timer| timer.state.recipient() == 2 && timer.state.is_pending())
        );
        assert_eq!(state.target_of(second), Some(ObjectRef::Timer(2)));
        assert!(
            !state
                .audit
                .any(|event| matches!(event, AuditEvent::ObjectDestroyed { object_id: 2, .. }))
        );
    }

    #[test]
    fn timer_audit_reports_exactly_once_for_fired_timers() {
        // S3-X2：每个 Fired Timer 恰一条 `TimerFired` 且恰一条同 `MsgId` 的 `MessageSent`。
        let mut state = fresh_state();
        let _ = state.timer_set(INIT_TASK_ID, 0).expect("immediate");
        let _ = state.timer_set(INIT_TASK_ID, 2).expect("future");
        state.advance_time(2).expect("advance");
        let audit = state.timer_audit();
        assert_eq!(audit.sets, 2);
        assert_eq!(audit.fired, 2);
        assert_eq!(audit.immediate_fired, 1);
        assert_eq!(audit.timers, 2);
        assert_eq!(audit.now, 2);
        assert_eq!(audit.exactly_once_ok, 1);
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
        assert!(state
            .audit
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
            assert!(state.audit.any(
                |event| matches!(event, AuditEvent::CapabilityRevoked { cap: id } if *id == cap)
            ));
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
            state
                .audit
                .any(|event| matches!(event, AuditEvent::MessageDropped { message_id: 3 }))
        );
        assert!(
            state
                .audit
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
        // （`prepare_user_ipc_caps`/`receive_message` 的空队列阻塞与投递唤醒）、§13.1 的
        // 携带 Capability 位置变化（直投：`HeldBy(init)` → `HeldBy(2)`）与消息事件。
        assert_eq!(state.audit.count, 10);
        assert_eq!(
            state.audit.entry(state.audit.len() - 4),
            Some(AuditEvent::CapabilityMoved {
                cap: IMAGE_CAP_HANDLE,
                from: CapabilityLocation::HeldByTask(INIT_TASK_ID),
                to: CapabilityLocation::HeldByTask(2),
            })
        );
        assert!(matches!(
            state.audit.entry(state.audit.len() - 2),
            Some(AuditEvent::MessageSent { message_id: 1, .. })
        ));
        assert!(matches!(
            state.audit.entry(state.audit.len() - 1),
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
    fn exc_source_destroy_removes_the_source_and_stales_its_cap() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        let page_cap = EXC_PAGE_FAULT_HANDLE;
        state
            .exc_subscribe(INIT_TASK_ID, page_cap, ExcKind::PageFault)
            .unwrap();
        assert_eq!(
            state.exc_sources.find(PAGE_FAULT_SOURCE_ID).unwrap().count,
            1
        );

        // 成功：源离开活动表、订阅集合随之消失、cap 的目标不再存活、其余源不受影响。
        assert_eq!(state.exc_source_destroy(INIT_TASK_ID, page_cap), Ok(()));
        assert!(state.exc_sources.find(PAGE_FAULT_SOURCE_ID).is_none());
        assert!(!state.target_live(ObjectRef::ExcSource(PAGE_FAULT_SOURCE_ID)));
        assert!(state.exc_sources.find(SYSCALL_TRAP_SOURCE_ID).is_some());
        assert!(state.validate_authority().is_ok());

        // 失败路径：同一 cap 再销毁（源已不在）、未知句柄、非持有者、缺 `ExcSourceDestroy`。
        assert_eq!(
            state.exc_source_destroy(INIT_TASK_ID, page_cap),
            Err(ExcError::NoSuchSource)
        );
        assert_eq!(
            state.exc_source_destroy(INIT_TASK_ID, Handle::new(61, 1).unwrap()),
            Err(ExcError::NoSuchSource)
        );
        assert_eq!(
            state.exc_source_destroy(2, EXC_SYSCALL_TRAP_HANDLE),
            Err(ExcError::NotHolder)
        );
        state.records.entries[EXC_SYSCALL_TRAP_HANDLE.index() as usize]
            .as_mut()
            .unwrap()
            .rights = Rights::from_rights(&[Right::ExcSubscribe]);
        assert_eq!(
            state.exc_source_destroy(INIT_TASK_ID, EXC_SYSCALL_TRAP_HANDLE),
            Err(ExcError::MissingRight)
        );
        // 失败路径不改变状态。
        assert!(state.exc_sources.find(SYSCALL_TRAP_SOURCE_ID).is_some());
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
                task_id: core::num::NonZeroU32::new(INIT_TASK_ID).expect("task id is nonzero"),
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
        let mut moved = 0usize;
        state.audit.for_each(|event| {
            if matches!(
                event,
                AuditEvent::CapabilityMoved {
                    from: CapabilityLocation::HeldByKernel,
                    ..
                }
            ) {
                moved += 1;
            }
        });
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
            name: 0,
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
            state.audit.entry(state.audit.len() - 1),
            Some(AuditEvent::TaskStateChanged {
                task_id: 2,
                status: TaskStatus::Running
            })
        ));
        state.yield_task(2).unwrap(); // Running → Ready
        assert!(matches!(
            state.audit.entry(state.audit.len() - 1),
            Some(AuditEvent::TaskStateChanged {
                task_id: 2,
                status: TaskStatus::Ready
            })
        ));
        state.block_recv_task(2).unwrap(); // Ready → BlockedRecv
        assert!(matches!(
            state.audit.entry(state.audit.len() - 1),
            Some(AuditEvent::TaskStateChanged {
                task_id: 2,
                status: TaskStatus::BlockedRecv
            })
        ));
        state.wake_recv_task(2).unwrap(); // BlockedRecv → Ready
        assert!(matches!(
            state.audit.entry(state.audit.len() - 1),
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
    /// SPEC-073：§12.10 成功动作 2/3/4/5——清 `pending_exc`、存 `kernel_resume_frame`、
    /// 追加 `TaskResumed`、`Faulted → Ready`。
    #[test]
    fn exc_resume_moves_a_recoverable_faulted_task_to_ready() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        state
            .tasks
            .fault(2, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
            .unwrap();
        crate::sched::clear_kernel_resume_frame(2);
        let before = state.audit.count;
        assert!(
            state
                .exc_resume_faulted(2, crate::exc::SretFrame::ZERO)
                .is_ok()
        );
        let task = state.tasks.find(2).copied().unwrap();
        assert_eq!(
            task.status,
            TaskStatus::Ready,
            "成功动作 5：Faulted → Ready"
        );
        assert!(task.pending_exc.is_none(), "成功动作 2：清 pending_exc");
        assert!(
            crate::sched::kernel_resume_frame(2).is_some(),
            "成功动作 3：存 kernel_resume_frame"
        );
        assert!(matches!(
            state.audit.entry(state.audit.len() - 1),
            Some(AuditEvent::TaskResumed { task_id: 2 })
        ));
        assert_eq!(
            state.audit.count,
            before + 1,
            "成功动作 4：恰追加一条 TaskResumed"
        );
        assert!(state.tasks.check_faulted_invariant().is_ok());
        crate::sched::clear_kernel_resume_frame(2);
    }

    /// SPEC-073：前置 5/6/8 的反例——非 `Faulted`、不可恢复种类、已有待消费帧都必须被拒
    /// 且**状态原子不变**。
    #[test]
    fn exc_resume_rejects_precondition_violations_atomically() {
        let mut state = exc_state();
        state.create_task(3).unwrap();
        crate::sched::clear_kernel_resume_frame(3);
        // 前置 5：Ready 不是 Faulted。
        let ready = state.tasks.find(3).copied().unwrap();
        assert_eq!(
            state.exc_resume_faulted(3, crate::exc::SretFrame::ZERO),
            Err(ExcResumeError::NotFaulted)
        );
        assert_eq!(state.tasks.find(3).copied().unwrap(), ready);
        // 前置 6：不可恢复种类。
        state.schedule_task(3).unwrap();
        state
            .tasks
            .fault(3, PAGE_FAULT_SOURCE_ID, ExcKind::DoubleFault)
            .unwrap();
        let faulted = state.tasks.find(3).copied().unwrap();
        assert_eq!(
            state.exc_resume_faulted(3, crate::exc::SretFrame::ZERO),
            Err(ExcResumeError::UnrecoverableFault)
        );
        assert_eq!(
            state.tasks.find(3).copied().unwrap(),
            faulted,
            "被拒的 exc_resume 不得改动任何字段"
        );
        // 前置 8：已有待消费的 kernel_resume_frame。
        state.tasks.find_mut(3).unwrap().pending_exc = Some(PendingExc {
            source_id: PAGE_FAULT_SOURCE_ID,
            kind: ExcKind::PageFault,
        });
        crate::sched::set_kernel_resume_frame(3, crate::exc::SretFrame::ZERO);
        assert_eq!(
            state.exc_resume_faulted(3, crate::exc::SretFrame::ZERO),
            Err(ExcResumeError::FramePending)
        );
        crate::sched::clear_kernel_resume_frame(3);
        state.tasks.find_mut(3).unwrap().pending_exc = None;
        state.tasks.find_mut(3).unwrap().status = TaskStatus::Dead;
    }

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

    /// SPEC-074：**源销毁不构成回收条件**——§12.10 的 `exc_source_cap` 授权语义未实现，
    /// `exc_resume` 不查源，故源销毁后该任务**仍可恢复**；判据必须与实现的前置一致，
    /// 不能沿用 SPEC-072 过渡口径里"源已销毁 ⇒ 不可恢复"那一支。
    #[test]
    fn destroyed_fault_source_does_not_make_a_faulted_task_reclaimable() {
        let mut state = exc_state();
        state.create_task(2).unwrap();
        state.schedule_task(2).unwrap();
        state
            .tasks
            .fault(2, PAGE_FAULT_SOURCE_ID, ExcKind::PageFault)
            .unwrap();
        // 摘掉该源记录。
        state.exc_sources.entries[0] = None;
        assert!(
            !state.is_unrecoverable_faulted(2),
            "源销毁不构成回收条件：exc_resume 本就不查源，该任务仍可恢复"
        );
        assert_eq!(
            state.kernel_reclaim_task(2, None, 0),
            Err(TaskError::NotUnrecoverable)
        );
        // 真正的不可恢复终态仍由「种类不可恢复」判定。
        state.tasks.find_mut(2).unwrap().pending_exc = Some(PendingExc {
            source_id: PAGE_FAULT_SOURCE_ID,
            kind: ExcKind::DoubleFault,
        });
        assert!(state.is_unrecoverable_faulted(2));
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
            state
                .audit
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
        // SPEC-074：审计日志移出结构体后实测 12240 B（原 16312 B，释放 4072 B）。
        // 这里把"余量充足"钉住，既防再次撞 16 KiB 守卫，也防悄悄回归到内联存储。
        assert!(
            size <= 13 * 1024,
            "BootstrapState size = {size} bytes：审计存储应已移出结构体（SPEC-074），余量应 ≥ 3 KiB"
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
