//! 异常/中断入口与恢复跳板的细化层（唯一允许 `unsafe` 的入口面）。
//!
//! 本模块承载与硬件 ABI 逐字节耦合的部分：`SretFrame` 的 0xA0 布局、四条裸入口
//! （#PF / `int 0x80` / `syscall` / timer）、恢复跳板、`extern "x86-interrupt"` breakpoint
//! handler，以及它们直接触碰的全部静态。上层机制通过**安全** API 与**注册回调**使用它们，
//! 自身不含 `unsafe`。
//!
//! **契约**：入口与跳板假定单核、由 CPU/已校验帧进入；入口调用的是**注册**的函数指针
//! （`AtomicU64` 存 `fn` 地址），注册前被触发会跳到地址 0——调用次序由上层保证。所有
//! 帧指针以 `#[repr(transparent)]` newtype（[`AbiFrame`]/[`SaveBlock`]）**按值**传递，
//! 仍是 8 字节、仍走 `rdi`，ABI 与原裸指针参数逐字节一致。信任调用方，不宣称 sound。

use core::arch::asm;
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use crate::mm;
use crate::sync::StaticCell;

/// 用户态 syscall 递送的入口错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UserSyscallError {
    /// 当前 CS 不处于 CPL3，拒绝执行可能依赖用户态入口配置的 `syscall` 指令。
    NotUserMode,
}

/// 从 CPL3 发起一次 x86_64 `syscall`，返回原始 RAX 位模式。
///
/// 入口必须已由内核配置，且本任务需要位于有效的用户地址空间中。函数先读取 CS 并检查
/// CPL，避免内核态误走用户 syscall 入口。调用约定与 Prima 原型一致：号在 RAX，参数在
/// RDI/RSI/RDX/R10/R8/R9，返回值在 RAX；RCX/R11 按硬件约定被破坏。
#[inline]
pub fn syscall_from_user(number: u64, args: [u64; 6]) -> Result<u64, UserSyscallError> {
    let cs: u16;
    // SAFETY: 读取 CS 是非特权可执行的 segment-register read；不访问内存或修改状态。
    unsafe {
        asm!(
            "mov {cs:x}, cs",
            cs = out(reg) cs,
            options(nomem, nostack, preserves_flags)
        );
    }
    if cs & 3 != 3 {
        return Err(UserSyscallError::NotUserMode);
    }

    let result: u64;
    // SAFETY: CPL3 已检查；目标内核必须已安装匹配的 LSTAR 入口。输入寄存器与当前 ABI
    // 一致，RCX/R11 按 syscall 指令语义声明为 clobber。默认保留内存副作用声明。
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") args[0],
            in("rsi") args[1],
            in("rdx") args[2],
            in("r10") args[3],
            in("r8") args[4],
            in("r9") args[5],
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack)
        );
    }
    Ok(result)
}

/// `SretFrame` 的字节长度（20 × u64）。
pub const SRET_FRAME_LEN: usize = 0xA0;

/// `SretFrame` 的对齐要求。
pub const SRET_FRAME_ALIGN: usize = 16;

/// x86_64 异常返回帧：20 个 u64，字段顺序与 `cosir::state::SretFrame` 一致。
///
/// 字节布局（little-endian，8 字节槽）：
/// `0x00 rax, 0x08 rbx, 0x10 rcx, 0x18 rdx, 0x20 rsi, 0x28 rdi, 0x30 rbp,
/// 0x38 rsp, 0x40 r8, 0x48 r9, 0x50 r10, 0x58 r11, 0x60 r12, 0x68 r13,
/// 0x70 r14, 0x78 r15, 0x80 rflags, 0x88 rip, 0x90 cs, 0x98 ss`。
#[repr(C)]
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

// SAFETY: repr(C) with only u64 fields; the size equals their summed sizes,
// so there is no internal or trailing padding, and every bit pattern is valid.
unsafe impl crate::pod::Pod for SretFrame {}
const _: () = assert!(size_of::<SretFrame>() == SRET_FRAME_LEN);

impl SretFrame {
    /// 全零帧（供静态初始化与默认值）。
    pub const ZERO: Self = Self {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        rsp: 0,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rflags: 0,
        rip: 0,
        cs: 0,
        ss: 0,
    };

    /// 从固定长度字节流解码（little-endian）。
    pub fn decode(bytes: &[u8; SRET_FRAME_LEN]) -> Self {
        let word = |offset: usize| {
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&bytes[offset..offset + 8]);
            u64::from_le_bytes(raw)
        };
        Self {
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
        }
    }

    /// 编码为固定长度字节流（little-endian）。
    pub fn encode(&self) -> [u8; SRET_FRAME_LEN] {
        let mut bytes = [0u8; SRET_FRAME_LEN];
        let mut put = |offset: usize, value: u64| {
            bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
        };
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

// ---------------------------------------------------------------------------
// 帧 newtype：以值传递的指针，保持 ABI 逐字节不变
// ---------------------------------------------------------------------------

/// ABI 现场帧的长度（qword）：`[num, arg0..arg5, cs]`。
pub const ABI_FRAME_QWORDS: usize = 8;

/// `int 0x80` / `syscall` 入口在栈上组装的 8-qword ABI 现场帧。
///
/// `#[repr(transparent)]` 包裸指针：按值传递即指针传递（仍走 `rdi`），与裸入口原先把
/// `*const u64` 传入 `call` 的 ABI 完全一致。访问器在框架内实现（框架允许 `unsafe`），
/// 上层因此无需解引用裸指针。
///
/// 契约（信任调用方）：指针必须指向裸入口在本任务内核栈上建立的有效帧；生命期不超出
/// 该次 handler 调用。
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct AbiFrame(*const u64);

impl AbiFrame {
    /// 由裸指针构造（仅裸入口内部与 host 测试使用）。
    pub const fn new(ptr: *const u64) -> Self {
        Self(ptr)
    }

    /// 调用号（槽 0）。
    pub fn num(self) -> u64 {
        mm::read_u64_at(self.0 as u64, 0)
    }

    /// 第 `index` 个参数（槽 `1 + index`）。契约：`index < 6`。
    pub fn arg(self, index: usize) -> u64 {
        mm::read_u64_at(self.0 as u64, 1 + index as u64)
    }

    /// 进入时的 `CS`（槽 7；快路径由入口填用户 `CS` 常量）。
    pub fn cs(self) -> u64 {
        mm::read_u64_at(self.0 as u64, 1 + 6)
    }

    /// 转为绑定同一地址的 [`SaveBlock`]（供回调只持 `AbiFrame` 却需整块上下文时使用）。
    pub fn as_save_block(self) -> SaveBlock {
        SaveBlock(self.0)
    }
}

/// `syscall`/中断入口压栈后的 20-qword 保存块（15 GPR + rip/cs/rflags/rsp/ss）。
///
/// 与 [`SretFrame`] 的**槽位不同**，必须逐槽按名映射（见 [`SaveBlock::to_sret_frame`]）。
#[derive(Clone, Copy, Debug)]
#[repr(transparent)]
pub struct SaveBlock(*const u64);

impl SaveBlock {
    /// 由裸指针构造。
    pub const fn new(ptr: *const u64) -> Self {
        Self(ptr)
    }

    /// 保存块中的进入 `CS`（槽 16 = 偏移 0x80，硬件帧的第 2 项）。
    pub fn cs(self) -> u64 {
        mm::read_u64_at(self.0 as u64, 16)
    }

    /// 展开为完整 [`SretFrame`]。
    ///
    /// **不可按字段序复制**：`rax..rbp` 在 `0x00..0x30`；`r8..r15` 在 `0x38..0x70`
    /// （对应 `SretFrame` 的 `0x40..0x78`）；`rip=0x78`、`cs=0x80`、`rflags=0x88`、
    /// `rsp=0x90`、`ss=0x98`（`SretFrame` 的 `rsp` 在 `0x38`、`rflags/rip/cs/ss`
    /// 在 `0x80..0x98`）。
    pub fn to_sret_frame(self) -> SretFrame {
        let read = |offset: usize| mm::read_u64_at(self.0 as u64, (offset / 8) as u64);
        SretFrame {
            rax: read(0x00),
            rbx: read(0x08),
            rcx: read(0x10),
            rdx: read(0x18),
            rsi: read(0x20),
            rdi: read(0x28),
            rbp: read(0x30),
            r8: read(0x38),
            r9: read(0x40),
            r10: read(0x48),
            r11: read(0x50),
            r12: read(0x58),
            r13: read(0x60),
            r14: read(0x68),
            r15: read(0x70),
            rsp: read(0x90),
            rflags: read(0x88),
            rip: read(0x78),
            cs: read(0x80),
            ss: read(0x98),
        }
    }
}

/// CPL3 #PF 的返回现场（硬件特权切换时压入的 5 个 qword）。
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserFrame {
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

// SAFETY: repr(C) with only u64 fields; the size equals their summed sizes,
// so there is no internal or trailing padding, and every bit pattern is valid.
unsafe impl crate::pod::Pod for UserFrame {}
const _: () = assert!(size_of::<UserFrame>() == 5 * 8);

/// 最小 TSS：为 trap 提供内核栈切换（rsp0）。`#[repr(C, packed)]` 使其保持标准
/// x86-64 104 字节 byte-packed 布局（rsp0 位于偏移 4）。
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iopb: u16,
}

impl Tss {
    pub const fn new(rsp0: u64) -> Self {
        Tss {
            reserved0: 0,
            rsp0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist: [0; 7],
            reserved2: 0,
            reserved3: 0,
            iopb: 104,
        }
    }

    /// Updates the privilege-switch kernel stack without exposing packed fields.
    pub fn set_rsp0(&mut self, rsp0: u64) {
        self.rsp0 = rsp0;
    }
}

// SAFETY: repr(C, packed) contains only integer fields/arrays: no padding,
// references or invalid bit patterns. The architectural size is checked below.
unsafe impl crate::pod::Pod for Tss {}
const _: () = assert!(size_of::<Tss>() == 104);

// ---------------------------------------------------------------------------
// handler 注册（回调反转的核心）
// ---------------------------------------------------------------------------

/// #PF 分派回调：收完整现场帧，返回要 `iretq` 回去的 RIP。
pub type PageFaultHandler = extern "C" fn(&SretFrame) -> u64;

/// syscall 处理回调（`int 0x80` 与 `syscall` 快路径共用签名）。
pub type SyscallHandler = extern "C" fn(AbiFrame) -> u64;

/// 时钟 tick 回调：保存块 + 进入时内核栈 `rsp`。
pub type TimerHandler = extern "C" fn(SaveBlock, u64);

/// 不返回的收尾回调（用户 `int3` 收尾 / 恢复落点）。
pub type ExitHandler = extern "C" fn() -> !;

static PAGE_FAULT_HANDLER: AtomicU64 = AtomicU64::new(0);
static SYSCALL_HANDLER: AtomicU64 = AtomicU64::new(0);
static FAST_SYSCALL_HANDLER: AtomicU64 = AtomicU64::new(0);
static TIMER_HANDLER: AtomicU64 = AtomicU64::new(0);
static RESUME_HANDLER: AtomicU64 = AtomicU64::new(0);
static USER_TRAP_HANDLER: AtomicU64 = AtomicU64::new(0);

/// 注册 #PF 分派回调（在装载 IDT 前调用）。
pub fn set_page_fault_handler(handler: PageFaultHandler) {
    PAGE_FAULT_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

/// 注册 `int 0x80` 处理回调。
pub fn set_syscall_handler(handler: SyscallHandler) {
    SYSCALL_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

/// 注册 `syscall` 快路径处理回调。
pub fn set_fast_syscall_handler(handler: SyscallHandler) {
    FAST_SYSCALL_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

/// 注册时钟 tick 回调（在放行 IRQ0、允许 IF=1 之前调用）。
pub fn set_timer_handler(handler: TimerHandler) {
    TIMER_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

/// 注册恢复跳板的收尾回调（在 [`resume`] 进 `resume_landing` 之前调用）。
pub fn set_resume_handler(handler: ExitHandler) {
    RESUME_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

/// 注册用户 `int3` 收尾回调。回调不得返回。
pub fn set_user_trap_handler(handler: ExitHandler) {
    USER_TRAP_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

// ---------------------------------------------------------------------------
// asm 直接触碰的静态（必须与 asm 同 crate）
// ---------------------------------------------------------------------------

/// 最近一次 #PF 的 CR2 地址。
static FAULT_ADDR: AtomicU64 = AtomicU64::new(0);

/// 最近一次 #PF 的 error code。
static FAULT_ERROR: AtomicU64 = AtomicU64::new(0);

/// 最近一次 #PF 的完整现场帧（裸入口在弹出 GPR 前写入）。
static FAULT_FRAME: StaticCell<SretFrame> = StaticCell::new(SretFrame::ZERO);

/// 最近一次来自 CPL3 的 #PF 的返回现场。
static USER_FRAME: StaticCell<UserFrame> = StaticCell::new(UserFrame {
    rip: 0,
    cs: 0,
    rflags: 0,
    rsp: 0,
    ss: 0,
});

/// `syscall_entry` 写入的当前保存块基址（`rbp`）。
static CONTEXT_BASE: AtomicU64 = AtomicU64::new(0);

/// `syscall` 快路径的专用内核栈容量。
// The debug build's authorization/segment lookup path exceeds the old 16 KiB
// once closure accesses acquire locks. Match the existing per-task 32 KiB budget.
pub const SYSCALL_STACK_SIZE: usize = 32 * 1024;
const STACK_CANARY_BYTES: usize = 64;
const STACK_CANARY: [u8; 8] = 0x6b73_7461_636b_a55au64.to_le_bytes();

#[repr(align(16))]
struct SyscallStack(
    #[allow(dead_code)] // 生产代码经 asm/volatile 访问；host 测试直接修改哨兵。
    [u8; SYSCALL_STACK_SIZE],
);

/// `syscall` 快路径的专用内核栈。
static SYSCALL_STACK: StaticCell<SyscallStack> = StaticCell::new(SyscallStack::new());

impl SyscallStack {
    const fn new() -> Self {
        let mut bytes = [0; SYSCALL_STACK_SIZE];
        let mut index = 0;
        while index < STACK_CANARY_BYTES {
            bytes[index] = STACK_CANARY[index % STACK_CANARY.len()];
            index += 1;
        }
        Self(bytes)
    }
}

fn stack_canary_at(base: u64) -> bool {
    (0..STACK_CANARY_BYTES / 8)
        .all(|index| mm::read_u64_at(base, index as u64) == u64::from_le_bytes(STACK_CANARY))
}

/// Checks the bottom 64 bytes after execution. This is an evidence sentinel,
/// not a guard page or prevention of arbitrary stack overflow. Raw reads avoid
/// borrowing the entire buffer while it may contain a live Rust stack.
pub fn syscall_stack_canary_intact() -> bool {
    stack_canary_at(SYSCALL_STACK.as_ptr() as u64)
}

/// `syscall` 进入前的用户 `rsp`（`sysretq` 前恢复）。
static USER_RSP: AtomicU64 = AtomicU64::new(0);

/// 恢复落点实测的 `r12`/`r13`/`rsp`/`rflags`。
static SEEN_R12: AtomicU64 = AtomicU64::new(0);
static SEEN_R13: AtomicU64 = AtomicU64::new(0);
static SEEN_RSP: AtomicU64 = AtomicU64::new(0);
static SEEN_RFLAGS: AtomicU64 = AtomicU64::new(0);

/// 已触发的向量号（初始 0；int3 后为 3）。
static TRAP_VECTOR_SEEN: AtomicU8 = AtomicU8::new(0);

// ---------------------------------------------------------------------------
// 只读访问器（上层用，无 unsafe）
// ---------------------------------------------------------------------------

/// 读取最近一次 #PF 的 CR2 地址（初始 0）。
pub fn fault_addr() -> u64 {
    FAULT_ADDR.load(Ordering::Relaxed)
}

/// 读取最近一次 #PF 的 error code（初始 0；CPU 压入为 64 位零扩展）。
pub fn last_fault_err() -> u64 {
    FAULT_ERROR.load(Ordering::Relaxed)
}

/// 读取最近一次 #PF 捕获的完整现场帧（初始全零）。
pub fn fault_frame() -> SretFrame {
    FAULT_FRAME.with(|frame| *frame)
}

/// 读取最近一次 CPL3 #PF 的返回现场（初始全零）。
pub fn user_frame() -> UserFrame {
    USER_FRAME.with(|frame| *frame)
}

/// 读取当前 syscall 保存块基址；未处于 `int 0x80` 现场时为 `None`。
pub fn context_base() -> Option<SaveBlock> {
    let raw = CONTEXT_BASE.load(Ordering::Relaxed);
    if raw == 0 {
        None
    } else {
        Some(SaveBlock(raw as *const u64))
    }
}

/// 读取已触发的向量号（初始 0）。
pub fn trap_seen() -> u8 {
    TRAP_VECTOR_SEEN.load(Ordering::Relaxed)
}

/// 恢复落点实测的 `r12`。
pub fn seen_r12() -> u64 {
    SEEN_R12.load(Ordering::Relaxed)
}

/// 恢复落点实测的 `r13`。
pub fn seen_r13() -> u64 {
    SEEN_R13.load(Ordering::Relaxed)
}

/// 恢复落点实测的 `rsp`。
pub fn seen_rsp() -> u64 {
    SEEN_RSP.load(Ordering::Relaxed)
}

/// 恢复落点实测的 `rflags`。
pub fn seen_rflags() -> u64 {
    SEEN_RFLAGS.load(Ordering::Relaxed)
}

// ---------------------------------------------------------------------------
// 入口地址 getter（供 IDT/GDT/MSR 装载方把裸入口装进硬件）
// ---------------------------------------------------------------------------

/// #PF 裸入口地址（IDT vector 14）。
pub fn page_fault_entry_addr() -> u64 {
    page_fault_entry as *const () as u64
}

/// `int 0x80` 裸入口地址（IDT vector 0x80）。
pub fn syscall_entry_addr() -> u64 {
    syscall_entry as *const () as u64
}

/// `syscall` 快路径裸入口地址（IA32_LSTAR）。
pub fn syscall_fast_entry_addr() -> u64 {
    syscall_fast_entry as *const () as u64
}

/// 时钟裸入口地址（IDT vector 32）。
pub fn timer_entry_addr() -> u64 {
    timer_entry as *const () as u64
}

/// 恢复落点地址（`SretFrame.rip` 指向它）。
pub fn resume_landing_addr() -> u64 {
    resume_landing as *const () as u64
}

/// breakpoint handler 地址（IDT vector 3，DPL3 门）。
pub fn breakpoint_handler_addr() -> u64 {
    breakpoint_handler as *const () as u64
}

// ---------------------------------------------------------------------------
// 裸入口
// ---------------------------------------------------------------------------

/// vector 14（page fault）入口（裸函数）。
///
/// #PF 是带 error code 的 fault 类异常。naked 入口完成两件事：
/// 1. **完整现场捕获**：把全部 15 个 GPR、硬件压栈的 rip/cs/rflags、合成的原始
///    rsp 与实时 ss 写入 `FAULT_FRAME`；
/// 2. **最小继续**：把返回地址前移（跳过触发指令，避免 iretq 后重试同一故障指令而
///    死循环）、弹出 error code，再 iretq，恢复全部 GPR。
///
/// CPL3 路径额外保存用户返回现场并向注册的 [`PageFaultHandler`] 询问重试 RIP；CPL0
/// 路径沿用「前移跳过长」语义。handler 由 [`set_page_fault_handler`] 注册。
///
/// 栈布局假设（CPL0、ist=0，无特权级切换）：进入时 CPU 压入 `err, rip, cs, rflags`
/// 四个 qword，`rsp` 指向 error code。入口再压 15 个 GPR（0x78 字节）后：
/// `[rsp+0x00..0x70]` = 15 个 GPR（rax…r15），`[rsp+0x78]` err、`[rsp+0x80]` rip、
/// `[rsp+0x88]` cs、`[rsp+0x90]` rflags；合成的原始 rsp = `rsp + 0x98`。
///
/// # Safety
///
/// 只能由 CPU 在 #PF（vector 14）经 IDT 调用，不能当作普通函数从 Rust 侧调用。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn page_fault_entry() -> ! {
    core::arch::naked_asm!(
        // 保存全部 15 个 GPR：压栈顺序使压出的块与 SretFrame 字段序对齐
        // （rax 最后压、位于最低地址；rsp 槽由 lea 合成，不经压栈）。
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        // 把压出的 GPR 块逐字段搬入 FAULT_FRAME。r11 作帧基址、r10 作暂存：
        // 两者的原值都已在栈上（故可安全改写），且 rsp 从不作暂存。
        "lea r11, [rip + {frame}]",
        "mov r10, qword ptr [rsp + 0x00]",
        "mov qword ptr [r11 + 0x00], r10", // rax
        "mov r10, qword ptr [rsp + 0x08]",
        "mov qword ptr [r11 + 0x08], r10", // rbx
        "mov r10, qword ptr [rsp + 0x10]",
        "mov qword ptr [r11 + 0x10], r10", // rcx
        "mov r10, qword ptr [rsp + 0x18]",
        "mov qword ptr [r11 + 0x18], r10", // rdx
        "mov r10, qword ptr [rsp + 0x20]",
        "mov qword ptr [r11 + 0x20], r10", // rsi
        "mov r10, qword ptr [rsp + 0x28]",
        "mov qword ptr [r11 + 0x28], r10", // rdi
        "mov r10, qword ptr [rsp + 0x30]",
        "mov qword ptr [r11 + 0x30], r10", // rbp
        "lea r10, [rsp + 0x98]",
        "mov qword ptr [r11 + 0x38], r10", // 合成的原始 rsp
        "mov r10, qword ptr [rsp + 0x38]",
        "mov qword ptr [r11 + 0x40], r10", // r8
        "mov r10, qword ptr [rsp + 0x40]",
        "mov qword ptr [r11 + 0x48], r10", // r9
        "mov r10, qword ptr [rsp + 0x48]",
        "mov qword ptr [r11 + 0x50], r10", // r10
        "mov r10, qword ptr [rsp + 0x50]",
        "mov qword ptr [r11 + 0x58], r10", // r11
        "mov r10, qword ptr [rsp + 0x58]",
        "mov qword ptr [r11 + 0x60], r10", // r12
        "mov r10, qword ptr [rsp + 0x60]",
        "mov qword ptr [r11 + 0x68], r10", // r13
        "mov r10, qword ptr [rsp + 0x68]",
        "mov qword ptr [r11 + 0x70], r10", // r14
        "mov r10, qword ptr [rsp + 0x70]",
        "mov qword ptr [r11 + 0x78], r10", // r15
        "mov r10, qword ptr [rsp + 0x90]",
        "mov qword ptr [r11 + 0x80], r10", // rflags
        "mov r10, qword ptr [rsp + 0x80]",
        "mov qword ptr [r11 + 0x88], r10", // rip（故障指令地址，稍后再前移）
        "mov r10, qword ptr [rsp + 0x88]",
        "mov qword ptr [r11 + 0x90], r10", // cs
        // 实时 ss：`mov r16, Sreg` 高位不保证清零，故先 xor；64 位下 push ss 是 #UD。
        "xor eax, eax",
        "mov ax, ss",
        "mov qword ptr [r11 + 0x98], rax", // ss
        // 既有证据面：CR2 与 error code。
        "mov r10, cr2",
        "lea r11, [rip + {fault}]",
        "mov qword ptr [r11], r10",
        "mov r10, qword ptr [rsp + 0x78]",
        "lea r11, [rip + {err}]",
        "mov qword ptr [r11], r10",
        // 判定故障来源的 CPL：CS 位于 [rsp+0x88]（CPL0/CPL3 两条路径该偏移均成立）。
        "mov r10, qword ptr [rsp + 0x88]",
        "test r10, 3",
        "jz 2f",
        // ---- CPL3 路径：保存用户返回现场（含特权切换多压的 rsp/ss）----
        "lea r11, [rip + {uframe}]",
        "mov r10, qword ptr [rsp + 0x80]",
        "mov qword ptr [r11 + 0x00], r10", // rip
        "mov r10, qword ptr [rsp + 0x88]",
        "mov qword ptr [r11 + 0x08], r10", // cs
        "mov r10, qword ptr [rsp + 0x90]",
        "mov qword ptr [r11 + 0x10], r10", // rflags
        "mov r10, qword ptr [rsp + 0x98]",
        "mov qword ptr [r11 + 0x18], r10", // rsp
        "mov r10, qword ptr [rsp + 0xA0]",
        "mov qword ptr [r11 + 0x20], r10", // ss
        // 询问分派器返回的 RIP（默认跳过、COW 命中则重试）。
        "lea rdi, [rip + {frame}]",
        "mov rbp, rsp",
        "and rsp, -16",
        "call qword ptr [rip + {handler}]",
        "mov rsp, rbp",
        "lea r11, [rip + {uframe}]",
        "mov qword ptr [r11 + 0x00], rax",
        // 恢复全部 15 个 GPR（用户态现场），丢弃 6 qword 硬件帧，重建 iretq 帧。
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "add rsp, 48",
        "lea r11, [rip + {uframe}]",
        "push qword ptr [r11 + 0x20]", // ss
        "push qword ptr [r11 + 0x18]", // rsp
        "push qword ptr [r11 + 0x10]", // rflags
        "push qword ptr [r11 + 0x08]", // cs
        "push qword ptr [r11 + 0x00]", // rip
        "iretq",
        "2:",
        // ---- CPL0 路径（原逻辑）----
        "lea rdi, [rip + {frame}]",
        "mov rbp, rsp",
        "and rsp, -16",
        "call qword ptr [rip + {handler}]",
        "mov rsp, rbp",
        "mov qword ptr [rsp + 0x80], rax",
        // 恢复全部 15 个 GPR（逆序），不清空现场。
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        // 弹出硬件压栈的 error code，使 rsp 指向 rip。
        "add rsp, 8",
        "iretq",
        frame = sym FAULT_FRAME,
        fault = sym FAULT_ADDR,
        err = sym FAULT_ERROR,
        uframe = sym USER_FRAME,
        handler = sym PAGE_FAULT_HANDLER,
    )
}

/// vector 0x80（syscall）裸入口。
///
/// `int 0x80` 是软件中断、无 error code。CPL3→CPL0 时 CPU 经 TSS.rsp0 换到内核栈，
/// 压入 `ss, rsp, rflags, cs, rip`（5 个 qword）；入口再压 15 个 GPR 后：
/// `[rsp+0x00..0x70]` = 15 个 GPR（rax…r15）、`[rsp+0x78]` rip、`[rsp+0x80]` cs、
/// `[rsp+0x88]` rflags、`[rsp+0x90]` rsp、`[rsp+0x98]` ss。
///
/// 入口在栈上组装 8 个 qword 的 ABI 现场帧 `[num, arg0..arg5, cs]`，把帧以
/// [`AbiFrame`] 传给注册的 [`SyscallHandler`]，把返回值写回保存的 `rax` 槽，恢复全部
/// GPR 后 `iretq` 回 CPL3。同时把保存块基址（`rbp`）存入 [`CONTEXT_BASE`]。
///
/// # Safety
///
/// 只能由 CPU 在 `int 0x80`（vector 0x80）经 IDT 调用；不可从 Rust 侧调用。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn syscall_entry() -> ! {
    core::arch::naked_asm!(
        // 保存全部 15 个 GPR：rax 最后压、位于最低地址，与恢复时的弹出序对齐。
        // 保存块偏移：rax=0x00 rbx=0x08 rcx=0x10 rdx=0x18 rsi=0x20 rdi=0x28
        // rbp=0x30 r8=0x38 r9=0x40 r10=0x48 r11=0x50 r12=0x58 r13=0x60 r14=0x68 r15=0x70。
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        // 以 rbp 为保存块基址；rsp 对齐后留 64 字节 ABI 现场帧（r10 作暂存，其原值已入栈）。
        "mov rbp, rsp",
        // 切片 M：把保存块基址存入静态，供 SYS_YIELD 读出当前任务完整用户上下文。
        "mov qword ptr [rip + {ctx}], rbp",
        "and rsp, -16",
        "sub rsp, 64",
        "mov r10, qword ptr [rbp + 0x00]",
        "mov qword ptr [rsp + 0x00], r10", // 调用号 rax
        "mov r10, qword ptr [rbp + 0x28]",
        "mov qword ptr [rsp + 0x08], r10", // arg0 rdi
        "mov r10, qword ptr [rbp + 0x20]",
        "mov qword ptr [rsp + 0x10], r10", // arg1 rsi
        "mov r10, qword ptr [rbp + 0x18]",
        "mov qword ptr [rsp + 0x18], r10", // arg2 rdx
        "mov r10, qword ptr [rbp + 0x48]",
        "mov qword ptr [rsp + 0x20], r10", // arg3 r10
        "mov r10, qword ptr [rbp + 0x38]",
        "mov qword ptr [rsp + 0x28], r10", // arg4 r8
        "mov r10, qword ptr [rbp + 0x40]",
        "mov qword ptr [rsp + 0x30], r10", // arg5 r9
        "mov r10, qword ptr [rbp + 0x80]",
        "mov qword ptr [rsp + 0x38], r10", // 进入时 CS（硬件压栈）
        "mov rdi, rsp",
        "call qword ptr [rip + {handle}]",
        "mov rsp, rbp",
        // 返回值写回保存的 rax 槽，随后弹出即进入 rax。
        "mov qword ptr [rsp + 0x00], rax",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        // rsp 现指向硬件压栈的 rip；iretq 依目标 CS 决定弹 3（CPL0）或 5（CPL3）个 qword。
        "iretq",
        handle = sym SYSCALL_HANDLER,
        ctx = sym CONTEXT_BASE,
    )
}

/// `syscall` 快路径入口（裸函数，由 IA32_LSTAR 指向）。
///
/// 与 `int 0x80` 路径的关键差异：`syscall` **不换栈、不压帧**，只把返回 RIP 写入
/// `rcx`、RFLAGS 写入 `r11`，并清 FMASK 位。故入口自行：
/// 1. 把用户 `rsp` 存入 [`USER_RSP`]，切到专用 [`SYSCALL_STACK`]；
/// 2. 在 15 个 GPR 之下压入 `ss/rsp/rflags/cs/rip`，**模拟**中断路径的 5-qword 硬件帧，
///    使保存块与 `int 0x80` 逐槽一致（`SaveBlock::to_sret_frame` 因而在快路径上也有效）；
/// 3. 保存 `rcx`（用户返回 RIP）与 `r11`（用户 RFLAGS）到栈上，再保存 15 个 GPR，并把
///    保存块基址存入 [`CONTEXT_BASE`]（与 `int 0x80` 一致，供阻塞/让出读回上下文）；
/// 4. 组装 8-qword ABI 帧并交给注册的 [`SyscallHandler`]（`cs` 槽填用户 `CS`，由上层经
///    [`set_fast_syscall_cs`] 提供——`syscall` 不压 CS）；
/// 5. 返回值写回保存的 `rax` 槽，恢复 GPR、`r11`、`rcx`、用户 `rsp`，`sysretq` 回 CPL3。
///
/// # Safety
///
/// 只能由 CPU 在 EFER.SCE=1、STAR/LSTAR 已配置后经 `syscall` 指令进入；不可从 Rust
/// 侧调用。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn syscall_fast_entry() -> ! {
    core::arch::naked_asm!(
        // 1) 记下用户 rsp，切到专用内核栈（单核，无需 swapgs/per-CPU）。
        "mov qword ptr [rip + {user_rsp}], rsp",
        "lea rsp, [rip + {stack}]",
        "add rsp, {size}",
        // 1b) 模拟 `int 0x80` 的硬件帧（`syscall` 不压帧）：在 15 个 GPR 之下压入
        //     ss/rsp/rflags/cs/rip，使保存块的 0x78..0x98 与中断路径逐槽一致。否则
        //     `SaveBlock::to_sret_frame` 会对快路径读出垃圾 rip/rsp/cs/ss，任何经
        //     `current_context()` 的阻塞/让出都会把坏上下文存入调度帧。
        "push qword ptr [rip + {user_ss}]",
        "push qword ptr [rip + {user_rsp}]",
        "push r11",
        "push qword ptr [rip + {user_cs}]",
        "push rcx",
        // 2) 保存全部 15 个 GPR：rax 最后压、位于最低地址。注意 rcx（用户返回 RIP）与
        //    r11（用户 RFLAGS）由 syscall 写入，也在其中被保存，sysretq 前据以恢复。
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        // 3) 以 rbp 为保存块基址，组装 8-qword ABI 现场帧。
        //    与 `int 0x80` 入口一致：把保存块基址存入 `CONTEXT_BASE`，否则快路径上的
        //    阻塞/让出 syscall（`SYS_RECV`/`SYS_YIELD` 等）经 `current_context()` 读到的
        //    是**上一次 `int 0x80` 的陈旧现场**，会把外来上下文存入本任务的调度帧。
        "mov rbp, rsp",
        "mov qword ptr [rip + {ctx}], rbp",
        "and rsp, -16",
        "sub rsp, 64",
        "mov r10, qword ptr [rbp + 0x00]",
        "mov qword ptr [rsp + 0x00], r10", // 调用号 rax
        "mov r10, qword ptr [rbp + 0x28]",
        "mov qword ptr [rsp + 0x08], r10", // arg0 rdi
        "mov r10, qword ptr [rbp + 0x20]",
        "mov qword ptr [rsp + 0x10], r10", // arg1 rsi
        "mov r10, qword ptr [rbp + 0x18]",
        "mov qword ptr [rsp + 0x18], r10", // arg2 rdx
        "mov r10, qword ptr [rbp + 0x48]",
        "mov qword ptr [rsp + 0x20], r10", // arg3 r10
        "mov r10, qword ptr [rbp + 0x38]",
        "mov qword ptr [rsp + 0x28], r10", // arg4 r8
        "mov r10, qword ptr [rbp + 0x40]",
        "mov qword ptr [rsp + 0x30], r10", // arg5 r9
        "mov r10, qword ptr [rip + {user_cs}]",
        "mov qword ptr [rsp + 0x38], r10", // CS = 用户 CS（syscall 不压 CS）
        "mov rdi, rsp",
        "call qword ptr [rip + {handle}]",
        "mov rsp, rbp",
        "mov qword ptr [rsp + 0x00], rax",
        // 4) 恢复 GPR，再恢复用户 RFLAGS/RIP 与 rsp，sysretq 回 CPL3。
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "mov rsp, qword ptr [rip + {user_rsp}]",
        "sysretq",
        stack = sym SYSCALL_STACK,
        size = const SYSCALL_STACK_SIZE,
        user_rsp = sym USER_RSP,
        handle = sym FAST_SYSCALL_HANDLER,
        user_cs = sym FAST_SYSCALL_CS,
        user_ss = sym FAST_SYSCALL_SS,
        ctx = sym CONTEXT_BASE,
    )
}

/// 快路径 ABI 帧中填入的用户 `CS`（`syscall` 不压 CS）。
static FAST_SYSCALL_CS: AtomicU64 = AtomicU64::new(0);

/// 快路径保存块硬件帧中填入的用户 `SS`（`syscall` 不压 SS）。
static FAST_SYSCALL_SS: AtomicU64 = AtomicU64::new(0);

/// 设置快路径 ABI 帧的 `CS` 槽值（应为用户代码段选择子）。**必须在启用 `syscall` 前调用。**
pub fn set_fast_syscall_cs(cs: u64) {
    FAST_SYSCALL_CS.store(cs, Ordering::Relaxed);
}

/// 设置快路径保存块硬件帧的 `SS` 槽值（应为用户数据段选择子）。
pub fn set_fast_syscall_ss(ss: u64) {
    FAST_SYSCALL_SS.store(ss, Ordering::Relaxed);
}

/// IRQ0（vector 32）裸入口。
///
/// 布局与 `syscall_entry` 相同：入口先 `mov rsi, rsp` 记录进入时的内核栈 rsp，再压
/// 15 个 GPR（rax 最低地址），`rbp` 作保存块基址，把 [`SaveBlock`] 与进入 `rsp` 传给注册的
/// [`TimerHandler`]。handler 返回则弹栈 + 丢弃 5-qword 硬件帧 + `iretq`；若发生抢占切换
/// 则永不返回。
///
/// # Safety
///
/// 只能由 CPU 在 IRQ0 经 IDT 调用；不可从 Rust 侧调用。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn timer_entry() -> ! {
    core::arch::naked_asm!(
        "mov rsi, rsp",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        "mov rbp, rsp",
        "mov rdi, rbp",
        "and rsp, -16",
        "call qword ptr [rip + {tick}]",
        "mov rsp, rbp",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "add rsp, 40",
        "iretq",
        tick = sym TIMER_HANDLER,
    )
}

/// `exc_resume_entry` 读取的恢复帧指针（由 [`resume`] 在跳转前写入）。
static RESUME_FRAME_PTR: AtomicU64 = AtomicU64::new(0);

/// 经真实 `iretq` 恢复进一帧：压入完整 64 位 `iretq` 帧
/// （`ss, rsp, rflags, cs, rip`），加载除 `rsp` 外的全部 GPR，再 `iretq`。
///
/// 契约（信任调用方）：`frame` 必须已由内核校验器校验、在整个跳转期间有效；
/// 跳板在换栈前读完全部帧字段，故允许使用当前内核栈上的局部帧；不得在此处遗留锁 guard。
/// `frame.rsp` 指向可写的 canonical 栈；`frame.cs`/`frame.ss` 是当前 GDT 中合法的
/// 内核选择子。该函数不返回：控制权转移至 `frame.rip`。
pub fn resume(frame: &SretFrame) -> ! {
    RESUME_FRAME_PTR.store(frame as *const SretFrame as u64, Ordering::Relaxed);
    // SAFETY: 依据函数契约——帧已校验且有效；跳板读取上面刚写入的指针。
    unsafe { exc_resume_entry() }
}

/// 恢复跳板的裸入口：见 [`resume`] 的语义与约定。
///
/// 由于该 nightly 上裸函数的参数 ABI 不可靠，帧指针经 [`RESUME_FRAME_PTR`] 静态传递。
/// 寄存器纪律：`r10` 作帧基址，直到最后一步才加载自身；压栈只用当前栈，新 `rsp` 由
/// iret 帧内的值决定。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn exc_resume_entry() -> ! {
    core::arch::naked_asm!(
        "lea r10, [rip + {ptr}]",
        "mov r10, qword ptr [r10]",
        // 压入完整 64 位 iret 帧：ss 先压（最高地址）、rip 最后压（最先弹）。
        // 弹出顺序 RIP/CS/RFLAGS/RSP/SS —— 与 SretFrame 的 0x88/0x90/0x80/0x38/0x98 对应。
        "push qword ptr [r10 + 0x98]",
        "push qword ptr [r10 + 0x38]",
        "push qword ptr [r10 + 0x80]",
        "push qword ptr [r10 + 0x90]",
        "push qword ptr [r10 + 0x88]",
        // 加载 GPR（跳过 rsp 槽 0x38；r10 最后加载）。
        "mov rax, qword ptr [r10 + 0x00]",
        "mov rbx, qword ptr [r10 + 0x08]",
        "mov rcx, qword ptr [r10 + 0x10]",
        "mov rdx, qword ptr [r10 + 0x18]",
        "mov rsi, qword ptr [r10 + 0x20]",
        "mov rdi, qword ptr [r10 + 0x28]",
        "mov rbp, qword ptr [r10 + 0x30]",
        "mov r8, qword ptr [r10 + 0x40]",
        "mov r9, qword ptr [r10 + 0x48]",
        "mov r11, qword ptr [r10 + 0x58]",
        "mov r12, qword ptr [r10 + 0x60]",
        "mov r13, qword ptr [r10 + 0x68]",
        "mov r14, qword ptr [r10 + 0x70]",
        "mov r15, qword ptr [r10 + 0x78]",
        "mov r10, qword ptr [r10 + 0x50]",
        "iretq",
        ptr = sym RESUME_FRAME_PTR,
    )
}

/// 恢复落点：记录实测寄存器与栈指针，然后进入注册的 [`ExitHandler`]（不返回）。
///
/// 用 `call` 而不是 `jmp`：`frame.rsp` 16 对齐时，`call` 使 handler 以
/// `rsp % 16 == 8` 进入，符合 SysV 调用约定。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn resume_landing() -> ! {
    core::arch::naked_asm!(
        // 先捕获落点实测值，再做任何调用（调用会破坏 RFLAGS）。
        "pushfq",
        "pop r10",
        "lea r11, [rip + {sflags}]",
        "mov qword ptr [r11], r10",
        "lea r10, [rip + {s12}]",
        "mov qword ptr [r10], r12",
        "lea r10, [rip + {s13}]",
        "mov qword ptr [r10], r13",
        "lea r10, [rip + {srsp}]",
        "mov qword ptr [r10], rsp",
        "call qword ptr [rip + {after}]",
        s12 = sym SEEN_R12,
        s13 = sym SEEN_R13,
        srsp = sym SEEN_RSP,
        sflags = sym SEEN_RFLAGS,
        after = sym RESUME_HANDLER,
    )
}

// ---------------------------------------------------------------------------
// 断点与描述符表装载
// ---------------------------------------------------------------------------

/// CPU 压入的中断帧（同级 trap：int3 压 rip/cs/rflags）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
}

// SAFETY: repr(C) with only u64 fields; the size equals their summed sizes,
// so there is no internal or trailing padding, and every bit pattern is valid.
unsafe impl crate::pod::Pod for InterruptStackFrame {}
const _: () = assert!(size_of::<InterruptStackFrame>() == 3 * 8);

/// vector 3（breakpoint）handler。
///
/// 若已注册用户收尾回调（进入用户态前注册），则调用它（不返回）；否则按普通 int3
/// 记录触发，返回后 iretq 回到 int3 之后。
extern "x86-interrupt" fn breakpoint_handler(_frame: &mut InterruptStackFrame) {
    let raw = USER_TRAP_HANDLER.load(Ordering::Relaxed);
    if raw != 0 {
        // SAFETY: 由 `set_user_trap_handler` 保证该槽登记的是同类型 `fn` 地址。
        let handler: ExitHandler = unsafe { core::mem::transmute(raw as usize) };
        handler();
    }
    TRAP_VECTOR_SEEN.store(3, Ordering::Relaxed);
}

/// 装载 IDT（`lidt`）。`limit` 为 IDT 字节长 - 1。
///
/// 契约：特权指令，仅 CPL0；`base` 必须指向在 IDT 有效期内存活的内存。
pub fn load_idt(base: u64, limit: u16) {
    let mut idtr = [0u8; 10];
    idtr[0..2].copy_from_slice(&limit.to_le_bytes());
    idtr[2..10].copy_from_slice(&base.to_le_bytes());
    // SAFETY: 见函数契约——`idtr` 为局部量，在指令执行期间存活。
    unsafe {
        asm!("lidt [{}]", in(reg) idtr.as_ptr(), options(nostack, readonly, preserves_flags));
    }
}

/// 触发一次写只读页的 page fault：向 `addr` 写入，若该页 present 但只读则 #PF。
///
/// 故障指令固定为 `mov qword ptr [rax], rax`（`48 89 00`，3 字节），与上层的
/// `FAULT_SKIP_LEN` 成对。写故障的 error code 为 `P|W`（0x3）。
///
/// 契约：`addr` 必须可访问或按预期故障；本函数不捕获异常本身。
pub fn trigger_page_fault_write(addr: u64) {
    // SAFETY: 见函数契约——故意触发缺页，由已装载的 #PF 入口接管。
    unsafe {
        asm!(
            "mov qword ptr [rax], rax",
            inout("rax") addr => _,
            options(nostack, preserves_flags)
        );
    }
}

/// 触发一次读 `addr` 的 page fault：读该线性地址，若未映射则 #PF。
///
/// 故障指令固定为 `mov tmp, [addr]`（REX.W 8B，基址寻址，3 字节），与上层的
/// `FAULT_SKIP_LEN` 成对：handler 记录 CR2 并把返回地址前移 3 字节后 iretq。
///
/// 契约：同 [`trigger_page_fault_write`]。
pub fn trigger_page_fault(addr: u64) {
    // SAFETY: 见函数契约——故意触发缺页，由已装载的 #PF 入口接管。
    unsafe {
        asm!(
            "mov rax, qword ptr [rax]",
            inout("rax") addr => _,
            options(nostack, preserves_flags)
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::{align_of, size_of};

    #[test]
    fn audited_hardware_types_match_independent_byte_encodings() {
        use crate::pod::{Pod, bytes_of};
        let wire: [u8; SRET_FRAME_LEN] = core::array::from_fn(|index| index as u8);
        let frame = SretFrame::decode(&wire);
        assert_eq!(bytes_of(&frame), wire);
        assert_eq!(SretFrame::from_bytes(&wire), Some(frame));
        let mut tss = Tss::new(0x1234_5678_9abc_def0);
        tss.set_rsp0(0xfedc_ba98_7654_3210);
        let mut expected = [0u8; 104];
        expected[4..12].copy_from_slice(&0xfedc_ba98_7654_3210u64.to_le_bytes());
        expected[102..104].copy_from_slice(&104u16.to_le_bytes());
        assert_eq!(bytes_of(&tss), expected);
        // Rust representation validity does not imply CPU configuration validity.
        let all_bits = Tss::from_bytes(&[0xff; 104]).unwrap();
        assert_eq!(bytes_of(&all_bits), &[0xff; 104]);
    }

    #[test]
    fn sret_frame_layout_is_pinned() {
        assert_eq!(size_of::<SretFrame>(), SRET_FRAME_LEN);
        assert_eq!(align_of::<SretFrame>(), 8);
        // 关键槽位（与 cosir::state::SretFrame 逐字节一致；字段改动必须在此显式更新）。
        let f = SretFrame::ZERO;
        let base = &f as *const SretFrame as u64;
        let at = |field: *const u64| field as u64 - base;
        assert_eq!(at(&f.rax), 0x00);
        assert_eq!(at(&f.rsp), 0x38);
        assert_eq!(at(&f.rflags), 0x80);
        assert_eq!(at(&f.rip), 0x88);
        assert_eq!(at(&f.cs), 0x90);
        assert_eq!(at(&f.ss), 0x98);
    }

    #[test]
    fn decode_encode_round_trip() {
        let mut f = SretFrame::ZERO;
        f.rax = 0x1122;
        f.rip = 0xdead_beef;
        f.cs = 0x8;
        assert_eq!(SretFrame::decode(&f.encode()), f);
        assert_eq!(SretFrame::ZERO.encode(), [0u8; SRET_FRAME_LEN]);
    }

    #[test]
    fn abi_frame_newtype_is_pointer_sized() {
        // `#[repr(transparent)]`：值传递即指针传递，ABI 与原 `*const u64` 参数一致。
        assert_eq!(size_of::<AbiFrame>(), size_of::<*const u64>());
        assert_eq!(align_of::<AbiFrame>(), align_of::<*const u64>());
        assert_eq!(size_of::<SaveBlock>(), size_of::<*const u64>());
    }

    #[test]
    fn abi_frame_reads_number_args_and_cs() {
        let block = [0x77u64, 1, 2, 3, 4, 5, 6, 0x33];
        let frame = AbiFrame::new(block.as_ptr());
        assert_eq!(frame.num(), 0x77);
        for index in 0..6 {
            assert_eq!(frame.arg(index), 1 + index as u64);
        }
        assert_eq!(frame.cs(), 0x33);
    }

    #[test]
    fn save_block_maps_every_slot_by_name() {
        // 与内核 `frame_from_save_block` 相同的槽位钉死：槽 i 的值 = 0x1000 + i。
        let mut block = [0u64; 20];
        for (index, slot) in block.iter_mut().enumerate() {
            *slot = 0x1000 + index as u64;
        }
        let frame = SaveBlock::new(block.as_ptr()).to_sret_frame();
        assert_eq!(frame.rax, 0x1000);
        assert_eq!(frame.rbx, 0x1001);
        assert_eq!(frame.rcx, 0x1002);
        assert_eq!(frame.rdx, 0x1003);
        assert_eq!(frame.rsi, 0x1004);
        assert_eq!(frame.rdi, 0x1005);
        assert_eq!(frame.rbp, 0x1006);
        assert_eq!(frame.r8, 0x1007);
        assert_eq!(frame.r9, 0x1008);
        assert_eq!(frame.r10, 0x1009);
        assert_eq!(frame.r11, 0x100a);
        assert_eq!(frame.r12, 0x100b);
        assert_eq!(frame.r13, 0x100c);
        assert_eq!(frame.r14, 0x100d);
        assert_eq!(frame.r15, 0x100e);
        assert_eq!(frame.rip, 0x100f);
        assert_eq!(frame.cs, 0x1010);
        assert_eq!(frame.rflags, 0x1011);
        assert_eq!(frame.rsp, 0x1012);
        assert_eq!(frame.ss, 0x1013);
    }

    #[test]
    fn save_block_cs_is_hardware_frame_cs() {
        let mut block = [0u64; 20];
        block[16] = 0x33;
        assert_eq!(SaveBlock::new(block.as_ptr()).cs(), 0x33);
    }

    #[test]
    fn user_frame_layout_is_five_qwords() {
        assert_eq!(size_of::<UserFrame>(), 5 * 8);
        let f = UserFrame {
            rip: 1,
            cs: 2,
            rflags: 3,
            rsp: 4,
            ss: 5,
        };
        let bytes = crate::mm::bytes_of(&f);
        assert_eq!(&bytes[0..8], &1u64.to_le_bytes());
        assert_eq!(&bytes[32..40], &5u64.to_le_bytes());
    }

    #[test]
    fn fault_and_trap_accessors_default_to_zero() {
        assert_eq!(fault_addr(), 0);
        assert_eq!(last_fault_err(), 0);
        assert_eq!(fault_frame(), SretFrame::ZERO);
        assert_eq!(user_frame(), UserFrame::default());
        assert_eq!(trap_seen(), 0);
        assert!(context_base().is_none());
    }

    #[test]
    fn stack_canary_detects_each_boundary_byte_change() {
        let mut stack = SyscallStack::new();
        let base = &stack as *const SyscallStack as u64;
        assert_eq!(SYSCALL_STACK_SIZE, 32 * 1024);
        assert!(stack_canary_at(base));
        for index in 0..STACK_CANARY_BYTES {
            stack.0[index] ^= 1;
            assert!(!stack_canary_at(base), "byte {index}");
            stack.0[index] ^= 1;
        }
        stack.0[STACK_CANARY_BYTES] = 1;
        assert_eq!(stack.0[STACK_CANARY_BYTES], 1);
        assert!(stack_canary_at(base));
    }

    #[test]
    fn raw_entry_symbols_still_start_at_their_values() {
        assert_eq!(
            &FAULT_FRAME as *const _ as usize,
            FAULT_FRAME.as_ptr() as usize
        );
        assert_eq!(
            &USER_FRAME as *const _ as usize,
            USER_FRAME.as_ptr() as usize
        );
        assert_eq!(
            &SYSCALL_STACK as *const _ as usize,
            SYSCALL_STACK.as_ptr() as usize
        );
        assert!((SYSCALL_STACK.as_ptr() as usize).is_multiple_of(16));
    }

    #[test]
    fn entry_addresses_are_non_zero_and_distinct() {
        let addresses = [
            page_fault_entry_addr(),
            syscall_entry_addr(),
            syscall_fast_entry_addr(),
            timer_entry_addr(),
            resume_landing_addr(),
            breakpoint_handler_addr(),
        ];
        assert!(addresses.iter().all(|addr| *addr != 0));
        for (index, left) in addresses.iter().enumerate() {
            for right in &addresses[index + 1..] {
                assert_ne!(left, right);
            }
        }
    }

    #[test]
    fn interrupt_stack_frame_field_layout() {
        // 三条 u64：rip, cs, rflags —— 顺序与 CPU 压入一致。
        let f = InterruptStackFrame {
            instruction_pointer: 0xdead_beef,
            code_segment: 0x8,
            cpu_flags: 0x202,
        };
        let bytes = crate::mm::bytes_of(&f);
        assert_eq!(&bytes[0..8], &0xdead_beefu64.to_le_bytes());
        assert_eq!(&bytes[8..16], &0x8u64.to_le_bytes());
        assert_eq!(&bytes[16..24], &0x202u64.to_le_bytes());
    }
}
