use std::collections::HashMap;

pub type TaskId = u32;
pub type ExcSourceId = u32;
pub type SegId = u32;
pub type VirtAddr = u64;

// access 位域：bit0=R, bit1=W, bit2=X
pub type PageMapping = (VirtAddr, u8);
pub const ACCESS_R: u8 = 0b001;
pub const ACCESS_W: u8 = 0b010;
pub const ACCESS_X: u8 = 0b100;

#[repr(C, align(16))]
#[derive(Clone, Debug, PartialEq, Eq)]
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
    pub flags: u64,
}

/// SretFrame 周期（帧长 0x88）；`exc_resume` 用它做 frame_offset 整数倍对齐与越界校验（前置 #5）。
pub const SRET_FRAME_LEN: usize = core::mem::size_of::<SretFrame>();

// 以下模型类型（TaskStatus 的 BlockedRecv、ExceptionKind 的若干可恢复变体、
// PendingException.save_frame、Task.id）在本探针中不被构造/读取，但它们是
// §12.10/§12.9 细化域的完整模型表面，保留以维护规范保真度。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // BlockedRecv
pub enum TaskStatus {
    Ready,
    Running,
    BlockedRecv,
    Faulted, // 细化层可见状态
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegRange {
    pub len: usize,
}

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
    // SyscallTrap / ExternalInterrupt / TaskExit 本探针不建模恢复，仅枚举保留未来扩展
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
    pub supported_kind: ExceptionKind, // 简化：单一可支持类型锚
}

#[derive(Debug)]
#[allow(dead_code)] // save_frame 暂不被读取（现场帧由细化层在恢复时交付）
pub struct PendingException {
    pub source_id: ExcSourceId,
    pub kind: ExceptionKind,
    pub save_frame: SretFrame,
}

#[derive(Clone, Debug)]
#[allow(dead_code)] // id 仍为 Task 模型字段，本探针暂不读取；vspace 已被 exc_resume 读取
pub struct Task {
    pub id: TaskId,
    pub status: TaskStatus,
    pub vspace: Vec<PageMapping>, // 简化：直接列映射，(addr, access)；exc_resume 用其可执行映射（前置 #6）
}

#[derive(Default)]
pub struct State {
    pub tasks: HashMap<TaskId, Task>,
    pub exc_sources: HashMap<ExcSourceId, ExceptionSource>,
    pub pending_exc: HashMap<TaskId, PendingException>,
    pub segments: HashMap<SegId, SegRange>,
}
