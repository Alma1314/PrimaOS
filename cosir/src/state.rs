use std::collections::{HashMap, HashSet};

pub type TaskId = u32;
pub type ExcSourceId = u32;
pub type SegId = u32;
pub type NsId = u32;
pub type CapId = u32;
pub type VirtAddr = u64;
/// §4 类型：Audit 事件序号与逻辑时间（均 `Nat`）。
pub type EventId = u64;
pub type Instant = u64;

// access 位域：bit0=R, bit1=W, bit2=X
pub const ACCESS_R: u8 = 0b001;
pub const ACCESS_W: u8 = 0b010;
pub const ACCESS_X: u8 = 0b100;

// Capability 权限位域（§4.1 `Right`）。位序与既有探针兼容（bit0..bit5 不变），
// 为 §14 初始 Authority 扩到 bit14。`Right` 全集中 Timer*/Node*/ExcSourceDestroy
// 等本探针无消费者的权限**未列**（待有消费者再加）。
pub const CAP_EXC_SUBSCRIBE: u16 = 0b0000_0000_0000_0001;
pub const CAP_SEG_MAP_R: u16 = 0b0000_0000_0000_0010;
pub const CAP_SEG_MAP_W: u16 = 0b0000_0000_0000_0100;
pub const CAP_SEG_MAP_X: u16 = 0b0000_0000_0000_1000;
#[allow(dead_code)] // Task 6 consumes segment-destroy authority; Task 2 tests its disjoint bit.
pub const CAP_SEG_DESTROY: u16 = 0b0000_0000_0001_0000;
pub const CAP_TASK_MANAGE_VSPACE: u16 = 0b0000_0000_0010_0000;
pub const CAP_DERIVE: u16 = 0b0000_0000_0100_0000;
pub const CAP_TASK_SEND: u16 = 0b0000_0000_1000_0000;
pub const CAP_TASK_DESTROY: u16 = 0b0000_0001_0000_0000;
pub const CAP_NS_RESOLVE: u16 = 0b0000_0010_0000_0000;
pub const CAP_NS_BIND: u16 = 0b0000_0100_0000_0000;
pub const CAP_NS_UNBIND: u16 = 0b0000_1000_0000_0000;
pub const CAP_NS_MOUNT: u16 = 0b0001_0000_0000_0000;
pub const CAP_NS_UNMOUNT: u16 = 0b0010_0000_0000_0000;
pub const CAP_NS_DESTROY: u16 = 0b0100_0000_0000_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageMappingBacking {
    pub segment: SegId,
    pub offset: u64,
    pub frame_phys: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PageMapping {
    pub base: VirtAddr,
    pub len: u64,
    pub access: u8,
    pub user: bool,
    /// Complete cross-model identity for mappings owned by a composite slice.
    /// Standalone ops fixtures may leave this absent; CowSliceState rejects that
    /// representation because access bits alone cannot establish refinement.
    pub backing: Option<PageMappingBacking>,
}

impl PageMapping {
    pub fn contains(self, addr: u64) -> bool {
        self.base
            .checked_add(self.len)
            .is_some_and(|end| self.base <= addr && addr < end)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapObject {
    ExceptionSource(ExcSourceId),
    Segment(SegId),
    Task(TaskId),
    Namespace(NsId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Capability {
    pub owner: TaskId,
    pub object: CapObject,
    pub rights: u16,
    pub active: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SretFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rflags: u64,
    pub rip: u64,
    pub cs: u64,
    pub ss: u64,
}

pub const SRET_FRAME_LEN: usize = 0xA0;
#[allow(dead_code)] // ABI constant is consumed by the kernel integration layer.
pub const SRET_FRAME_ALIGN: usize = 16;
pub const USER_CS: u64 = 0x1b;
pub const USER_SS: u64 = 0x23;

#[allow(dead_code)] // Wire codec is consumed by the kernel integration layer.
impl SretFrame {
    pub fn decode(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != SRET_FRAME_LEN {
            return None;
        }

        let word = |offset| {
            let mut word = [0; 8];
            word.copy_from_slice(&bytes[offset..offset + 8]);
            u64::from_le_bytes(word)
        };
        Some(Self {
            rax: word(0x00),
            rbx: word(0x08),
            rcx: word(0x10),
            rdx: word(0x18),
            rsi: word(0x20),
            rdi: word(0x28),
            rbp: word(0x30),
            rsp: word(0x38),
            r8: word(0x40),
            r9: word(0x48),
            r10: word(0x50),
            r11: word(0x58),
            r12: word(0x60),
            r13: word(0x68),
            r14: word(0x70),
            r15: word(0x78),
            rflags: word(0x80),
            rip: word(0x88),
            cs: word(0x90),
            ss: word(0x98),
        })
    }

    pub fn encode(self) -> [u8; SRET_FRAME_LEN] {
        let mut bytes = [0; SRET_FRAME_LEN];
        let mut put =
            |offset, value: u64| bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        put(0x00, self.rax);
        put(0x08, self.rbx);
        put(0x10, self.rcx);
        put(0x18, self.rdx);
        put(0x20, self.rsi);
        put(0x28, self.rdi);
        put(0x30, self.rbp);
        put(0x38, self.rsp);
        put(0x40, self.r8);
        put(0x48, self.r9);
        put(0x50, self.r10);
        put(0x58, self.r11);
        put(0x60, self.r12);
        put(0x68, self.r13);
        put(0x70, self.r14);
        put(0x78, self.r15);
        put(0x80, self.rflags);
        put(0x88, self.rip);
        put(0x90, self.cs);
        put(0x98, self.ss);
        bytes
    }
}

// 以下模型类型（TaskStatus::BlockedRecv、ExceptionKind 的若干可恢复变体、
// PendingException.save_frame、Task.id）保留以维护 §12.10/§12.9 细化域的完整模型表面，
// 但它们不参与本探针的判别路径：save_frame 为**只写**字段（raise 写入、无人读取——
// 实际恢复帧取自用户 Segment，见 ops.rs），BlockedRecv 等为前向状态锚。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // BlockedRecv
pub enum TaskStatus {
    Ready,
    Running,
    BlockedRecv,
    Faulted, // 细化层可见状态
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub bytes: Vec<u8>,
    pub kernel_readable: bool,
}

/// §5.4 Namespace。本切片只物化 bootstrap 的**空** init Namespace 对象，
/// 使 §14 步骤 3/5 的 Namespace 根 Capability 有目标；`entries : Path ⇀ NsEntry`
/// 与 §11 的 `ns_*` 操作未建模（本探针无消费者）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Namespace;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // GPF/DivideError/InvalidOpcode 为可恢复类型锚，留待后续探针
pub enum ExceptionKind {
    PageFault {
        page_present: bool,
        writable_fault: bool,
    },
    GeneralProtectionFault,
    DivideError,
    InvalidOpcode,
    DoubleFault,
    // SyscallTrap / ExternalInterrupt / TaskExit 未建模：本探针只覆盖可恢复的硬件执行异常
}

impl ExceptionKind {
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            ExceptionKind::PageFault { .. }
                | ExceptionKind::GeneralProtectionFault
                | ExceptionKind::DivideError
                | ExceptionKind::InvalidOpcode
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExceptionSource {
    pub supported_kind: ExceptionKind, // 简化：§12.3「类型签名」以单一 ExceptionKind 值作锚
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // save_frame 暂不被读取（现场帧由细化层在恢复时交付）
pub struct PendingException {
    pub source_id: ExcSourceId,
    pub kind: ExceptionKind,
    /// Faulting virtual address for a PageFault; non-addressed exceptions record `None`.
    pub fault_va: Option<VirtAddr>,
    pub save_frame: SretFrame,
}

/// §6.1 `Actor`。用户可调用操作为 `Task(t)`；`bootstrap`/调度/时间推进/Timer 到期/
/// `raise_exception` 等内部迁移为 `Kernel`（用户输入不能构造 `Kernel`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Task(TaskId),
    Kernel,
}

/// §13.1：每条事件至少包含 `{id : EventId, at : Instant, actor : Actor, kind : AuditKind}`。
/// `kind` 由 `AuditKind` 承载（记录操作涉及的对象/Capability ID）；信封由 `State::push_audit`
/// 统一填入。
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // 字段仅由模型测试与完整状态比较读取
pub struct AuditEvent {
    pub id: EventId,
    pub at: Instant,
    pub actor: Actor,
    pub kind: AuditKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AuditKind {
    /// §12.5 成功动作 1：异常发生时由 `KernelActor` 追加，早于状态迁移。
    /// `fault_va` 为页故障的 `faulting_vaddr`，非寻址异常记 `None`（§12.2/§13.1）。
    ExceptionRaised {
        source_id: ExcSourceId,
        kind: ExceptionKind,
        fault_va: Option<VirtAddr>,
    },
    /// §13.1：每次 Task 状态迁移追加。本探针覆盖 `Running→Faulted`（§12.5 动作 3）；
    /// `Faulted→Ready` 按 §13.1 唯一例外以 `TaskResumed` 记录。
    TaskStateChanged {
        task: TaskId,
        from: TaskStatus,
        to: TaskStatus,
    },
    /// §12.10 成功动作 4。§12.10 要求的「记录 actor」由信封的 `actor` 承载（= `Task(调用者)`）。
    TaskResumed {
        source_id: ExcSourceId,
        source_cap: CapId,
        frame_cap: CapId,
        frame_seg_id: SegId,
        frame_offset: usize,
        rip: VirtAddr,
    },
    /// §14 步骤 9 / §13.1 `Bootstrapped`：bootstrap 成功（唯一允许无 Task Actor 的内部迁移，
    /// 故 `actor = Kernel`）。
    Bootstrapped,
    /// §14 步骤 2/3 + §13.1：对象创建（W13「每个活动对象都有创建事件」）。`segment` 标志区分
    /// `ObjectRef` 的 id 空间（image `Segment(1)` 与 init `Namespace(1)` 同 id），形态镜像真实
    /// 内核 `AuditEvent::ObjectCreated`（`kernel/src/bootstrap.rs`）。
    ObjectCreated { object_id: u32, segment: bool },
    /// §14 步骤 4/8 + §13.1：init Task 创建（初始状态 `Ready`）。§13.1 的**创建**事件是
    /// `TaskCreated`——`TaskStateChanged` 仅用于状态**迁移**（创建非迁移），故 bootstrap 的
    /// init Task 用本变体而非 `TaskStateChanged`。
    TaskCreated { task: TaskId, status: TaskStatus },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // id 仍为 Task 模型字段，本探针暂不读取；vspace 已被 exc_resume 读取
pub struct Task {
    pub id: TaskId,
    pub status: TaskStatus,
    pub vspace: Vec<PageMapping>,
    /// §5.2/§14 步骤 7：结构性根 Namespace Capability（W5：由 Task 持有、指向活动 Namespace、
    /// 至少 `NsResolve`）。本探针只在 bootstrap 的 init Task 上物化该字段；其余手工 fixture
    /// 以 `0` 表示"未建模"（这些 fixture 本就不满足 W5 全部门槛，如无 Namespace 对象）。
    pub root_ns_cap: CapId,
    pub kernel_resume_frame: Option<SretFrame>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct State {
    pub tasks: HashMap<TaskId, Task>,
    pub exc_sources: HashMap<ExcSourceId, ExceptionSource>,
    pub pending_exc: HashMap<TaskId, PendingException>,
    pub segments: HashMap<SegId, Segment>,
    pub namespaces: HashMap<NsId, Namespace>,
    pub capabilities: HashMap<CapId, Capability>,
    pub subscriptions: HashSet<(TaskId, ExcSourceId)>,
    pub audit: Vec<AuditEvent>,
    /// §4 `State.now`：逻辑当前时间。由 `time_advance`（KernelActor）推进；本探针
    /// 未建模 Timer/`time_advance`，故恒为初始 `0`（事件 `at` 因此恒为 0，见 §20.4）。
    pub now: Instant,
    /// §4 `State.next_event`：下一个 Audit `EventId`（W13：严格递增）。
    pub next_event: EventId,
    /// POSIR 状态的 bootstrap 标志（真实部分）；默认 false
    pub booted: bool,
}

impl State {
    /// §13.1：追加一条事件并填入信封（`id = next_event` 且自增，`at = now`）。
    /// W13：`EventId` 严格递增、日志只追加不改写。
    pub fn push_audit(&mut self, actor: Actor, kind: AuditKind) {
        let id = self.next_event;
        self.next_event += 1;
        self.audit.push(AuditEvent {
            id,
            at: self.now,
            actor,
            kind,
        });
    }
}
