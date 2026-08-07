// placeholders; ops 转移在 Task 4 实现
pub type TaskId = u32;
pub type ExcSourceId = u32;
pub type SegId = u32;
pub type VirtAddr = u64;
pub type PageMapping = (VirtAddr, u8); // (addr, access:bitfield R/W/X)

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

pub const SRET_FRAME_LEN: usize = core::mem::size_of::<SretFrame>();
