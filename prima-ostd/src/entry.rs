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
#[cfg(feature = "entry-percpu-probe")]
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, AtomicU32, AtomicU64, Ordering};

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

    /// 保存块基址（只读；供每 CPU 记录与栈范围核对）。
    pub fn addr(self) -> u64 {
        self.0 as u64
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
        self.decode_shift(0)
    }

    pub fn cs_with_error(self) -> u64 {
        mm::read_u64_at(self.0 as u64, 17)
    }
    pub fn to_sret_frame_with_error(self) -> SretFrame {
        self.decode_shift(8)
    }
    fn decode_shift(self, shift: usize) -> SretFrame {
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
            rsp: read(0x90 + shift),
            rflags: read(0x88 + shift),
            rip: read(0x78 + shift),
            cs: read(0x80 + shift),
            ss: read(0x98 + shift),
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

    /// Sets IST entry `index` (1..=7; IDT gate encodes the same 1-based index) to `top`.
    /// `index == 0` is a no-op (0 means "no IST" in a gate).
    pub fn set_ist(&mut self, index: u8, top: u64) {
        if (1..=7).contains(&index) {
            // `Tss` is `#[repr(C, packed)]`, so the field may be unaligned; write through
            // a raw field pointer instead of forming a reference (E0793).
            let slot = core::ptr::addr_of_mut!(self.ist[(index - 1) as usize]);
            // SAFETY: `slot` points into `self`; unaligned store is valid for `packed`.
            unsafe {
                slot.write_unaligned(top);
            }
        }
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

/// P5-3b-2：每 CPU 用户探针的收尾回调（参数为本 CPU 索引；不得返回）。
pub type FinishHandler = extern "C" fn(usize) -> !;
static FINISH_HANDLER: AtomicU64 = AtomicU64::new(0);

/// 注册用户探针收尾回调（在进入 CPL3 前调用）。
pub fn set_finish_handler(handler: FinishHandler) {
    FINISH_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

/// 调用已注册的用户探针收尾回调；未注册时停放（`hlt` 循环）。
///
/// 契约：仅在受控探针的 `int 0x80` 回调中调用，且当前 CPU 的入口现场已记录。
pub fn finish_now(cpu: usize) -> ! {
    let raw = FINISH_HANDLER.load(Ordering::Relaxed);
    if raw != 0 {
        // SAFETY: 由 `set_finish_handler` 保证该槽登记的是同类型 `fn` 地址。
        let handler: FinishHandler = unsafe { core::mem::transmute(raw as usize) };
        handler(cpu);
    }
    loop {
        crate::arch::hlt();
    }
}

pub type CpuExceptionHandler = extern "C" fn(u64, SaveBlock, u64) -> !;
static CPU_EXCEPTION_HANDLER: AtomicU64 = AtomicU64::new(0);
pub fn set_cpu_exception_handler(handler: CpuExceptionHandler) {
    CPU_EXCEPTION_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}

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
pub const SYSCALL_GUARD_SIZE: usize = 4096;
const STACK_CANARY_BYTES: usize = 64;
const STACK_CANARY: [u8; 8] = 0x6b73_7461_636b_a55au64.to_le_bytes();

#[repr(align(4096))]
struct SyscallStack(
    #[allow(dead_code)] // 生产代码经 asm/volatile 访问；host 测试直接修改哨兵。
    [u8; SYSCALL_GUARD_SIZE + SYSCALL_STACK_SIZE],
);

/// `syscall` 快路径的专用内核栈。
static SYSCALL_STACK: StaticCell<SyscallStack> = StaticCell::new(SyscallStack::new());

impl SyscallStack {
    const fn new() -> Self {
        let mut bytes = [0; SYSCALL_GUARD_SIZE + SYSCALL_STACK_SIZE];
        let mut index = 0;
        while index < STACK_CANARY_BYTES {
            bytes[SYSCALL_GUARD_SIZE + index] = STACK_CANARY[index % STACK_CANARY.len()];
            index += 1;
        }
        Self(bytes)
    }
}

fn stack_canary_at(base: u64) -> bool {
    (0..STACK_CANARY_BYTES / 8)
        .all(|index| mm::read_u64_at(base, index as u64) == u64::from_le_bytes(STACK_CANARY))
}

/// Virtual page to omit from the kernel image mapping, immediately below the
/// usable syscall stack. The physical direct-map alias remains kernel-only.
pub fn syscall_stack_guard_page() -> u64 {
    SYSCALL_STACK.as_ptr() as u64
}

/// Checks the bottom 64 usable bytes after execution. Raw reads avoid borrowing
/// the unmapped guard or the entire buffer while it contains a live Rust stack.
pub fn syscall_stack_canary_intact() -> bool {
    stack_canary_at(syscall_stack_guard_page() + SYSCALL_GUARD_SIZE as u64)
}

/// `syscall` 进入前的用户 `rsp`（`sysretq` 前恢复）。
static USER_RSP: AtomicU64 = AtomicU64::new(0);

/// GS-1：`syscall` 入口/出口是否执行 `swapgs`。默认 0——普通镜像完全不触碰 GS 基址；
/// 仅 GS 受控探针经 [`enable_gs_swap`] 置 1。运行期只置一次、之后只读，故入口与出口看到
/// 同一值，交换必然配对（阻塞切换不回到出口，改由 `sched::resume` 强制重设）。
static GS_SWAP: AtomicU8 = AtomicU8::new(0);

/// GS 探针开关是否开启。
pub fn gs_swap_enabled() -> bool {
    GS_SWAP.load(Ordering::Relaxed) != 0
}

/// 开启 `syscall` 快路径的 `swapgs`（GS 受控探针专用）。
///
/// 开启前调用方必须把 `IA32_GS_BASE` 与 `IA32_KERNEL_GS_BASE` 初始化为一致状态，且
/// 内核在 GS-3 之前不得读取 `%gs`。
pub fn enable_gs_swap() {
    GS_SWAP.store(1, Ordering::Relaxed);
}

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
///
/// P5-3b-3：每 CPU 入口把基址写入本 CPU 槽；此时按 `%gs:0` 索引读取，避免 AP 的真实
/// 系统调用读到 BSP 的旧现场。普通镜像仍读全局单槽。
#[cfg(all(feature = "entry-percpu-probe", not(test)))]
pub fn context_base() -> Option<SaveBlock> {
    percpu_context_base(crate::arch::read_gs_u32::<0>() as usize)
}

/// 读取当前 syscall 保存块基址；未处于 `int 0x80` 现场时为 `None`。
#[cfg(any(not(feature = "entry-percpu-probe"), test))]
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

pub type ExternalIrqHandler = extern "C" fn(u64);
static EXTERNAL_IRQ_HANDLER: AtomicU64 = AtomicU64::new(0);
pub fn set_external_irq_handler(handler: ExternalIrqHandler) {
    EXTERNAL_IRQ_HANDLER.store(handler as usize as u64, Ordering::Relaxed);
}
macro_rules! external_irq_entry {
    ($name:ident, $line:expr) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                // GS-2：外部 IRQ 来源为 CPL3 时入口交换，出口换回；内核来源不交换。
                "cmp byte ptr [rip + {gs}], 0", "je 8f",
                "test byte ptr [rsp + 8], 3", "jz 8f", "swapgs", "8:",
                "push r15", "push r14", "push r13", "push r12", "push r11", "push r10",
                "push r9", "push r8", "push rbp", "push rdi", "push rsi", "push rdx",
                "push rcx", "push rbx", "push rax", "cld", "mov rbp, rsp", "and rsp, -16",
                "mov rdi, {line}", "call qword ptr [rip + {handler}]", "mov rsp, rbp",
                "pop rax", "pop rbx", "pop rcx", "pop rdx", "pop rsi", "pop rdi", "pop rbp",
                "pop r8", "pop r9", "pop r10", "pop r11", "pop r12", "pop r13", "pop r14", "pop r15",
                "cmp byte ptr [rip + {gs}], 0", "je 9f",
                "test byte ptr [rsp + 8], 3", "jz 9f", "swapgs", "9:", "iretq",
                line = const $line, handler = sym EXTERNAL_IRQ_HANDLER, gs = sym GS_SWAP,
            )
        }
    }
}
external_irq_entry!(irq1_entry, 1);
external_irq_entry!(irq3_entry, 3);
external_irq_entry!(irq4_entry, 4);
external_irq_entry!(irq5_entry, 5);
external_irq_entry!(irq6_entry, 6);
external_irq_entry!(irq7_entry, 7);
external_irq_entry!(irq8_entry, 8);
external_irq_entry!(irq9_entry, 9);
external_irq_entry!(irq10_entry, 10);
external_irq_entry!(irq11_entry, 11);
external_irq_entry!(irq12_entry, 12);
external_irq_entry!(irq13_entry, 13);
external_irq_entry!(irq14_entry, 14);
external_irq_entry!(irq15_entry, 15);
pub fn external_irq_entry_addr(line: u8) -> Option<u64> {
    Some(match line {
        1 => irq1_entry as *const () as u64,
        3 => irq3_entry as *const () as u64,
        4 => irq4_entry as *const () as u64,
        5 => irq5_entry as *const () as u64,
        6 => irq6_entry as *const () as u64,
        7 => irq7_entry as *const () as u64,
        8 => irq8_entry as *const () as u64,
        9 => irq9_entry as *const () as u64,
        10 => irq10_entry as *const () as u64,
        11 => irq11_entry as *const () as u64,
        12 => irq12_entry as *const () as u64,
        13 => irq13_entry as *const () as u64,
        14 => irq14_entry as *const () as u64,
        15 => irq15_entry as *const () as u64,
        _ => return None,
    })
}

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
        "cld",
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
        // GS-2：#PF 来自 CPL3 时入口交换 GS（已判定为用户来源，无需再测 CS）。
        "cmp byte ptr [rip + {gs}], 0",
        "je 6f",
        "swapgs",
        "6:",
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
        // The dispatcher may resume through an upcall instead of returning
        // here. Its full frame must contain the user stack and selector too.
        "lea rax, [rip + {frame}]",
        "mov qword ptr [rax + 0x38], r10",
        "mov r10, qword ptr [rsp + 0xA0]",
        "mov qword ptr [r11 + 0x20], r10", // ss
        "mov qword ptr [rax + 0x98], r10",
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
        // Every GPR is already restored: RIP-relative operands preserve r11
        // across a transparent COW retry as well as all other user registers.
        "push qword ptr [rip + {uframe} + 0x20]", // ss
        "push qword ptr [rip + {uframe} + 0x18]", // rsp
        "push qword ptr [rip + {uframe} + 0x10]", // rflags
        "push qword ptr [rip + {uframe} + 0x08]", // cs
        "push qword ptr [rip + {uframe} + 0x00]", // rip
        // GS-2：CPL3 出口与入口配对换回。
        "cmp byte ptr [rip + {gs}], 0",
        "je 7f",
        "swapgs",
        "7:",
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
        gs = sym GS_SWAP,
    )
}

// Safety: these entries are installed only for the matching CPU vectors. All
// user GPRs are saved before scratch registers are touched. Error-code entries
// retain that extra word and tag the callback so it decodes the shifted frame.
// The callback checks CS before reading user-only RSP/SS, and never returns.
macro_rules! cpu_exception_entry {
    ($name:ident, $vector:expr, $tag:expr, $error:literal) => {
        #[unsafe(naked)]
        unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                "push r15", "push r14", "push r13", "push r12", "push r11",
                "push r10", "push r9", "push r8", "push rbp", "push rdi",
                "push rsi", "push rdx", "push rcx", "push rbx", "push rax",
                "cld",
                "mov rsi, rsp",
                "mov rdi, {vector}",
                $error,
                "and rsp, -16",
                "call qword ptr [rip + {handler}]",
                "ud2",
                vector = const ($vector | $tag), handler = sym CPU_EXCEPTION_HANDLER,
            )
        }
    }
}
cpu_exception_entry!(divide_entry, 0, 0, "xor edx, edx");
cpu_exception_entry!(invalid_opcode_entry, 6, 0, "xor edx, edx");
cpu_exception_entry!(x87_entry, 16, 0, "xor edx, edx");
cpu_exception_entry!(simd_entry, 19, 0, "xor edx, edx");
cpu_exception_entry!(
    stack_fault_entry,
    12,
    0x100,
    "mov rdx, qword ptr [rsi + 0x78]"
);
cpu_exception_entry!(
    protection_entry,
    13,
    0x100,
    "mov rdx, qword ptr [rsi + 0x78]"
);
cpu_exception_entry!(
    alignment_entry,
    17,
    0x100,
    "mov rdx, qword ptr [rsi + 0x78]"
);

pub fn cpu_exception_entry_addr(vector: u8) -> Option<u64> {
    Some(match vector {
        0 => divide_entry as *const () as u64,
        6 => invalid_opcode_entry as *const () as u64,
        12 => stack_fault_entry as *const () as u64,
        13 => protection_entry as *const () as u64,
        16 => x87_entry as *const () as u64,
        17 => alignment_entry as *const () as u64,
        19 => simd_entry as *const () as u64,
        _ => return None,
    })
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
        // GS-2：用户来源（CS.RPL==3）时入口交换 GS；内核来源（如嵌套）不交换。开关与
        // 快路径共用，默认关闭。只改 RFLAGS，不碰 GPR（用户 RFLAGS 仍在硬件帧里）。
        "cmp byte ptr [rip + {gs}], 0",
        "je 4f",
        "test byte ptr [rsp + 8], 3",
        "jz 4f",
        "swapgs",
        "4:",
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
        "cld",
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
        // GS-2：出口按同一 CS.RPL 判据换回（仅当入口来自用户且开启开关）。rsp 现指向
        // 硬件压栈的 rip，cs 在 [rsp+8]。
        "cmp byte ptr [rip + {gs}], 0",
        "je 5f",
        "test byte ptr [rsp + 8], 3",
        "jz 5f",
        "swapgs",
        "5:",
        // iretq 依目标 CS 决定弹 3（CPL0）或 5（CPL3）个 qword。
        "iretq",
        handle = sym SYSCALL_HANDLER,
        ctx = sym CONTEXT_BASE,
        gs = sym GS_SWAP,
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
        // GS-1：入口先交换 GS（安全不变量：先交换、再触碰任何状态）。开关关闭时整段跳过；
        // 只改 RFLAGS、不动 GPR，用户 RFLAGS 仍在 r11。
        "cmp byte ptr [rip + {gs}], 0",
        "je 1f",
        "swapgs",
        "1:",
        // 1) 记下用户 rsp，切到专用内核栈（无 per-CPU 区；GS 交换由上面的开关控制）。
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
        // GS-1：出口再交换，与入口配对（handler 内若阻塞切换则永不返回此处，改由调度器
        // 重设 GS）。只改 RFLAGS，用户 RFLAGS 仍在保存块/r11。
        "cmp byte ptr [rip + {gs}], 0",
        "je 3f",
        "swapgs",
        "3:",
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
        size = const SYSCALL_GUARD_SIZE + SYSCALL_STACK_SIZE,
        user_rsp = sym USER_RSP,
        handle = sym FAST_SYSCALL_HANDLER,
        user_cs = sym FAST_SYSCALL_CS,
        user_ss = sym FAST_SYSCALL_SS,
        ctx = sym CONTEXT_BASE,
        gs = sym GS_SWAP,
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
/// [`TimerHandler`]。handler 返回则弹 GPR，再由 `iretq` 消费硬件帧；若发生抢占切换
/// 则永不返回。
///
/// # Safety
///
/// 只能由 CPU 在 IRQ0 经 IDT 调用；不可从 Rust 侧调用。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn timer_entry() -> ! {
    core::arch::naked_asm!(
        // GS-2：IRQ0 来源为 CPL3 时入口交换；内核来源（嵌套/抢占内核）不交换。
        "cmp byte ptr [rip + {gs}], 0",
        "je 8f",
        "test byte ptr [rsp + 8], 3",
        "jz 8f",
        "swapgs",
        "8:",
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
        "cld",
        "mov rbp, rsp",
        "lea rsi, [rbp + 120]",
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
        // GS-2：出口按同一 CS.RPL 判据换回；rsp 现指向硬件帧，cs 在 [rsp+8]。
        "cmp byte ptr [rip + {gs}], 0",
        "je 9f",
        "test byte ptr [rsp + 8], 3",
        "jz 9f",
        "swapgs",
        "9:",
        "iretq",
        tick = sym TIMER_HANDLER,
        gs = sym GS_SWAP,
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

/// NMI（vector 2）计数（GS-2 探针）。只由 [`nmi_entry`] 增长。
static NMI_COUNT: AtomicU32 = AtomicU32::new(0);

/// 最近一次 NMI 交付时 handler 的栈指针（用于证明 NMI 确实运行在 IST 栈上）。
static NMI_RSP: AtomicU64 = AtomicU64::new(0);

/// 读取 NMI 计数。
pub fn nmi_count() -> u32 {
    NMI_COUNT.load(Ordering::Relaxed)
}

/// 读取最近一次 NMI 交付时的 handler 栈指针。
pub fn nmi_rsp() -> u64 {
    NMI_RSP.load(Ordering::Relaxed)
}

/// 清零 NMI 计数与记录的栈指针（探针开始前）。
pub fn reset_nmi_count() {
    NMI_COUNT.store(0, Ordering::Relaxed);
    NMI_RSP.store(0, Ordering::Relaxed);
}

/// NMI（vector 2）裸入口；由 IDT 在 `gs-probe` 下以 IST 门装载。
///
/// 只记录自身 `rsp`（证明运行在 IST 栈上）并 `lock inc` 计数器后 `iretq`。除被保存/
/// 恢复的 `rax` 外**不触碰任何 GPR，也不碰 GS 或调用者内核栈**。NMI 经独立 IST 栈交付，
/// 故无论命中用户态、内核半交换窗口或嵌套中断，本 handler 都不依赖 `%gs`；RFLAGS 由
/// `iretq` 从硬件帧恢复。这是 GS-2 安全不变量 4（NMI/IST 不依赖半交换状态）的实现。
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn nmi_entry() -> ! {
    core::arch::naked_asm!(
        "push rax",
        "mov rax, rsp",
        "mov qword ptr [rip + {recorded}], rax",
        "pop rax",
        "lock inc dword ptr [rip + {counter}]",
        "iretq",
        counter = sym NMI_COUNT,
        recorded = sym NMI_RSP,
    )
}

/// NMI 入口地址（IDT vector 2）。
pub fn nmi_entry_addr() -> u64 {
    nmi_entry as *const () as u64
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

// ---------------------------------------------------------------------------
// P5-3b-1：每 CPU 入口现场（CPL0 探针）
// ---------------------------------------------------------------------------
//
// 本片只覆盖 CPL0 来源的 `int 0x80` 与 #PF：`syscall` 快路径、`resume`、CPL3 用户入口
// 现场仍为全局单槽，且本 feature 不出现在默认镜像或任何用户态镜像。选择方式：入口从
// 本 CPU GS 区偏移 0 取索引（GS-3 契约），再按固定步长索引本模块的每 CPU 槽；调用方
// 必须已在每个参与 CPU 上安装 GS 区。

/// 每 CPU 入口现场槽上限；必须与内核 `cpu::local::MAX_CPUS` 一致。
#[cfg(feature = "entry-percpu-probe")]
pub const PERCPU_MAX: usize = 4;
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_STRIDE: usize = 0x100;
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_CTX_OFF: usize = 0x00;
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_FAULT_ADDR_OFF: usize = 0x08;
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_FAULT_ERROR_OFF: usize = 0x10;
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_FRAME_OFF: usize = 0x20;
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_RESUME_OFF: usize = 0xC0;
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_USER_FRAME_OFF: usize = 0xC8;
/// P5-3b-3：恢复跳板写入的用户 `%gs` 基址（`resume_percpu_user` 用）。
#[cfg(feature = "entry-percpu-probe")]
const PERCPU_USER_GS_OFF: usize = 0xF0;

/// 每 CPU GS 区中由入口汇编直接读写的字段偏移（内核 `cpu::local::CpuLocal` 必须按此布局）。
///
/// 快路径入口不得在保存用户 GPR 之前破坏任何寄存器，故 `USER_RSP` 与栈顶经 `%gs`
/// 直接寻址；槽基址同样放在 GS 区，供压栈后的 `ctx` 写入使用。
#[cfg(feature = "entry-percpu-probe")]
pub const GS_SYSCALL_STACK_TOP_OFF: usize = 0x10;
#[cfg(feature = "entry-percpu-probe")]
pub const GS_USER_RSP_OFF: usize = 0x18;
#[cfg(feature = "entry-percpu-probe")]
pub const GS_ENTRY_SLOT_OFF: usize = 0x20;

#[cfg(feature = "entry-percpu-probe")]
#[repr(C)]
struct PerCpuEntry {
    context_base: AtomicU64,
    fault_addr: AtomicU64,
    fault_error: AtomicU64,
    reserved: AtomicU64,
    fault_frame: UnsafeCell<SretFrame>,
    /// 恢复跳板的帧指针（由 [`resume_percpu`] 写入，裸入口消费后清零）。
    resume_frame: AtomicU64,
    /// CPL3 #PF 的返回现场（每 CPU）。
    user_frame: UnsafeCell<UserFrame>,
    /// P5-3b-3：即将恢复的用户 `%gs` 基址（0 = 无用户 GS）。
    user_gs: AtomicU64,
    pad: [u8; 0x08],
}

#[cfg(feature = "entry-percpu-probe")]
impl PerCpuEntry {
    const fn new() -> Self {
        Self {
            context_base: AtomicU64::new(0),
            fault_addr: AtomicU64::new(0),
            fault_error: AtomicU64::new(0),
            reserved: AtomicU64::new(0),
            fault_frame: UnsafeCell::new(SretFrame::ZERO),
            resume_frame: AtomicU64::new(0),
            user_frame: UnsafeCell::new(UserFrame {
                rip: 0,
                cs: 0,
                rflags: 0,
                rsp: 0,
                ss: 0,
            }),
            user_gs: AtomicU64::new(0),
            pad: [0; 0x08],
        }
    }
}

// 裸入口用 `shl rax, 8` 索引槽；步长必须与结构长度一致。
#[cfg(feature = "entry-percpu-probe")]
const _: () = assert!(size_of::<PerCpuEntry>() == PERCPU_STRIDE);

// SAFETY: 每槽只由本 CPU 在 CPL0 入口写入、由同一 CPU（或在其停放后由 BSP）读取；
// `UnsafeCell` 仅经 `get()` 裸访问，不形成跨 CPU 的 `&mut`。
#[cfg(feature = "entry-percpu-probe")]
unsafe impl Sync for PerCpuEntry {}

#[cfg(feature = "entry-percpu-probe")]
static PERCPU: [PerCpuEntry; PERCPU_MAX] = [const { PerCpuEntry::new() }; PERCPU_MAX];

#[cfg(feature = "entry-percpu-probe")]
fn percpu(cpu: usize) -> &'static PerCpuEntry {
    assert!(cpu < PERCPU_MAX, "cpu index out of range");
    &PERCPU[cpu]
}

/// 指定 CPU 的入口槽地址（供内核写入 GS 区，快路径 `ctx` 写入用）。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_slot_addr(cpu: usize) -> u64 {
    percpu(cpu) as *const PerCpuEntry as u64
}

/// 指定 CPU 最近一次 CPL3 #PF 的返回现场。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_user_frame(cpu: usize) -> UserFrame {
    // SAFETY: 见 [`percpu_fault_frame`] 的访问契约——每次读本 CPU 独立槽的 POD 值。
    unsafe { *percpu(cpu).user_frame.get() }
}

/// 指定 CPU 已由恢复跳板消费的帧指针（未消费或已清为 0 返回 `None`）。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_resume_frame(cpu: usize) -> Option<u64> {
    let raw = percpu(cpu).resume_frame.load(Ordering::Relaxed);
    if raw == 0 { None } else { Some(raw) }
}

/// 指定 CPU 最近一次 `int 0x80` 保存块基址（未进入过为 `None`）。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_context_base(cpu: usize) -> Option<SaveBlock> {
    let raw = percpu(cpu).context_base.load(Ordering::Relaxed);
    if raw == 0 {
        None
    } else {
        Some(SaveBlock(raw as *const u64))
    }
}

/// 指定 CPU 最近一次 #PF 的 CR2。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_fault_addr(cpu: usize) -> u64 {
    percpu(cpu).fault_addr.load(Ordering::Relaxed)
}

/// 指定 CPU 最近一次 #PF 的 error code。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_fault_error(cpu: usize) -> u64 {
    percpu(cpu).fault_error.load(Ordering::Relaxed)
}

/// 指定 CPU 最近一次 #PF 捕获的完整现场帧。
///
/// 契约：仅在该 CPU 已停放（或本 CPU 自身）时读取；入口写入期间不得并发读取同一槽。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_fault_frame(cpu: usize) -> SretFrame {
    // SAFETY: 见函数契约——每次按 CPU 访问独立槽，读取 0xA0 字节的 POD 值。
    unsafe { *percpu(cpu).fault_frame.get() }
}

/// 从 CPL0 执行一次 `int 0x80`（探针专用；CPL3 来源被每 CPU 入口以 #UD 拒绝）。
///
/// 契约：本 CPU 已安装 GS 区，vector 0x80 门指向 [`syscall_entry_percpu_addr`]，且入口
/// 回调不阻塞、不切换任务。
#[cfg(feature = "entry-percpu-probe")]
pub fn int80_probe(number: u64, args: [u64; 6]) -> u64 {
    let result: u64;
    // SAFETY: 见函数契约；入口恢复全部 GPR 与 RFLAGS，只有 RAX 作为返回值有意不同。
    unsafe {
        asm!(
            "int 0x80",
            inlateout("rax") number => result,
            in("rdi") args[0],
            in("rsi") args[1],
            in("rdx") args[2],
            in("r10") args[3],
            in("r8") args[4],
            in("r9") args[5],
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
    result
}

/// 每 CPU `int 0x80` 入口地址（CPL0 探针）。
#[cfg(feature = "entry-percpu-probe")]
pub fn syscall_entry_percpu_addr() -> u64 {
    syscall_entry_percpu as *const () as u64
}

/// 每 CPU #PF 入口地址（CPL0 探针）。
#[cfg(feature = "entry-percpu-probe")]
pub fn page_fault_entry_percpu_addr() -> u64 {
    page_fault_entry_percpu as *const () as u64
}

/// `int 0x80` 的每 CPU 现场副本（CPL0 与 CPL3）。
///
/// 与 [`syscall_entry`] 的差别只有保存块基址写入的位置：`r12` 从 `%gs:0` 索引本模块的
/// 每 CPU 槽。GS 交换沿用 GS-2 的开关与 `CS.RPL` 判据，仅在受控镜像启用；CPL0 探针
/// 不交换（此时 `%gs` 已是内核 GS 区）。
#[cfg(feature = "entry-percpu-probe")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn syscall_entry_percpu() -> ! {
    core::arch::naked_asm!(
        "cmp byte ptr [rip + {gs}], 0",
        "je 4f",
        "test byte ptr [rsp + 8], 3",
        "jz 4f",
        "swapgs",
        "4:",
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
        "cld",
        "mov rbp, rsp",
        // r12 = &PERCPU + (%gs:0) * 0x100。
        "mov eax, dword ptr gs:[0]",
        "lea r12, [rip + {percpu}]",
        "shl rax, 8",
        "add r12, rax",
        "mov qword ptr [r12 + {ctx}], rbp",
        "and rsp, -16",
        "sub rsp, 64",
        "mov r10, qword ptr [rbp + 0x00]",
        "mov qword ptr [rsp + 0x00], r10",
        "mov r10, qword ptr [rbp + 0x28]",
        "mov qword ptr [rsp + 0x08], r10",
        "mov r10, qword ptr [rbp + 0x20]",
        "mov qword ptr [rsp + 0x10], r10",
        "mov r10, qword ptr [rbp + 0x18]",
        "mov qword ptr [rsp + 0x18], r10",
        "mov r10, qword ptr [rbp + 0x48]",
        "mov qword ptr [rsp + 0x20], r10",
        "mov r10, qword ptr [rbp + 0x38]",
        "mov qword ptr [rsp + 0x28], r10",
        "mov r10, qword ptr [rbp + 0x40]",
        "mov qword ptr [rsp + 0x30], r10",
        "mov r10, qword ptr [rbp + 0x80]",
        "mov qword ptr [rsp + 0x38], r10",
        "mov rdi, rsp",
        "call qword ptr [rip + {handle}]",
        "mov rsp, rbp",
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
        "cmp byte ptr [rip + {gs}], 0",
        "je 5f",
        "test byte ptr [rsp + 8], 3",
        "jz 5f",
        "swapgs",
        "5:",
        "iretq",
        percpu = sym PERCPU,
        ctx = const PERCPU_CTX_OFF,
        handle = sym SYSCALL_HANDLER,
        gs = sym GS_SWAP,
    )
}

/// #PF 的每 CPU 现场副本（CPL0 与 CPL3）。
///
/// CPL0 路径与 [`page_fault_entry`] 的 CPL0 分支逐槽相同；CPL3 路径按 GS-2 判据交换 GS，
/// 并把返回现场写入本 CPU 的 `USER_FRAME`。两条路径的现场基址都由 `r12`（`%gs:0`
/// 索引）给出，不再引用全局单槽。
#[cfg(feature = "entry-percpu-probe")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn page_fault_entry_percpu() -> ! {
    core::arch::naked_asm!(
        // CS 位于 [rsp+0x10]（err 在 0、rip 在 8），CPL3 走 3f。
        "test byte ptr [rsp + 0x10], 3",
        "jnz 3f",
        // ---- CPL0 ----
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
        "cld",
        "mov eax, dword ptr gs:[0]",
        "lea r12, [rip + {percpu}]",
        "shl rax, 8",
        "add r12, rax",
        "mov r10, qword ptr [rsp + 0x00]",
        "mov qword ptr [r12 + {frame} + 0x00], r10",
        "mov r10, qword ptr [rsp + 0x08]",
        "mov qword ptr [r12 + {frame} + 0x08], r10",
        "mov r10, qword ptr [rsp + 0x10]",
        "mov qword ptr [r12 + {frame} + 0x10], r10",
        "mov r10, qword ptr [rsp + 0x18]",
        "mov qword ptr [r12 + {frame} + 0x18], r10",
        "mov r10, qword ptr [rsp + 0x20]",
        "mov qword ptr [r12 + {frame} + 0x20], r10",
        "mov r10, qword ptr [rsp + 0x28]",
        "mov qword ptr [r12 + {frame} + 0x28], r10",
        "mov r10, qword ptr [rsp + 0x30]",
        "mov qword ptr [r12 + {frame} + 0x30], r10",
        "lea r10, [rsp + 0x98]",
        "mov qword ptr [r12 + {frame} + 0x38], r10",
        "mov r10, qword ptr [rsp + 0x38]",
        "mov qword ptr [r12 + {frame} + 0x40], r10",
        "mov r10, qword ptr [rsp + 0x40]",
        "mov qword ptr [r12 + {frame} + 0x48], r10",
        "mov r10, qword ptr [rsp + 0x48]",
        "mov qword ptr [r12 + {frame} + 0x50], r10",
        "mov r10, qword ptr [rsp + 0x50]",
        "mov qword ptr [r12 + {frame} + 0x58], r10",
        "mov r10, qword ptr [rsp + 0x58]",
        "mov qword ptr [r12 + {frame} + 0x60], r10",
        "mov r10, qword ptr [rsp + 0x60]",
        "mov qword ptr [r12 + {frame} + 0x68], r10",
        "mov r10, qword ptr [rsp + 0x68]",
        "mov qword ptr [r12 + {frame} + 0x70], r10",
        "mov r10, qword ptr [rsp + 0x70]",
        "mov qword ptr [r12 + {frame} + 0x78], r10",
        "mov r10, qword ptr [rsp + 0x90]",
        "mov qword ptr [r12 + {frame} + 0x80], r10",
        "mov r10, qword ptr [rsp + 0x80]",
        "mov qword ptr [r12 + {frame} + 0x88], r10",
        "mov r10, qword ptr [rsp + 0x88]",
        "mov qword ptr [r12 + {frame} + 0x90], r10",
        "xor eax, eax",
        "mov ax, ss",
        "mov qword ptr [r12 + {frame} + 0x98], rax",
        "mov r10, cr2",
        "mov qword ptr [r12 + {addr}], r10",
        "mov r10, qword ptr [rsp + 0x78]",
        "mov qword ptr [r12 + {error}], r10",
        "lea rdi, [r12 + {frame}]",
        "mov rbp, rsp",
        "and rsp, -16",
        "call qword ptr [rip + {handler}]",
        "mov rsp, rbp",
        "mov qword ptr [rsp + 0x80], rax",
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
        "add rsp, 8",
        "iretq",
        // ---- CPL3 ----
        "3:",
        "cmp byte ptr [rip + {gs}], 0",
        "je 4f",
        "swapgs",
        "4:",
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
        "cld",
        "mov eax, dword ptr gs:[0]",
        "lea r12, [rip + {percpu}]",
        "shl rax, 8",
        "add r12, rax",
        // 完整现场帧（rsp 槽合成后由用户 rsp 覆盖）。
        "mov r10, qword ptr [rsp + 0x00]",
        "mov qword ptr [r12 + {frame} + 0x00], r10",
        "mov r10, qword ptr [rsp + 0x08]",
        "mov qword ptr [r12 + {frame} + 0x08], r10",
        "mov r10, qword ptr [rsp + 0x10]",
        "mov qword ptr [r12 + {frame} + 0x10], r10",
        "mov r10, qword ptr [rsp + 0x18]",
        "mov qword ptr [r12 + {frame} + 0x18], r10",
        "mov r10, qword ptr [rsp + 0x20]",
        "mov qword ptr [r12 + {frame} + 0x20], r10",
        "mov r10, qword ptr [rsp + 0x28]",
        "mov qword ptr [r12 + {frame} + 0x28], r10",
        "mov r10, qword ptr [rsp + 0x30]",
        "mov qword ptr [r12 + {frame} + 0x30], r10",
        "mov r10, qword ptr [rsp + 0x98]",
        "mov qword ptr [r12 + {frame} + 0x38], r10",
        "mov r10, qword ptr [rsp + 0x38]",
        "mov qword ptr [r12 + {frame} + 0x40], r10",
        "mov r10, qword ptr [rsp + 0x40]",
        "mov qword ptr [r12 + {frame} + 0x48], r10",
        "mov r10, qword ptr [rsp + 0x48]",
        "mov qword ptr [r12 + {frame} + 0x50], r10",
        "mov r10, qword ptr [rsp + 0x50]",
        "mov qword ptr [r12 + {frame} + 0x58], r10",
        "mov r10, qword ptr [rsp + 0x58]",
        "mov qword ptr [r12 + {frame} + 0x60], r10",
        "mov r10, qword ptr [rsp + 0x60]",
        "mov qword ptr [r12 + {frame} + 0x68], r10",
        "mov r10, qword ptr [rsp + 0x68]",
        "mov qword ptr [r12 + {frame} + 0x70], r10",
        "mov r10, qword ptr [rsp + 0x70]",
        "mov qword ptr [r12 + {frame} + 0x78], r10",
        "mov r10, qword ptr [rsp + 0x90]",
        "mov qword ptr [r12 + {frame} + 0x80], r10",
        "mov r10, qword ptr [rsp + 0x80]",
        "mov qword ptr [r12 + {frame} + 0x88], r10",
        "mov r10, qword ptr [rsp + 0x88]",
        "mov qword ptr [r12 + {frame} + 0x90], r10",
        "mov r10, qword ptr [rsp + 0xA0]",
        "mov qword ptr [r12 + {frame} + 0x98], r10",
        "mov r10, cr2",
        "mov qword ptr [r12 + {addr}], r10",
        "mov r10, qword ptr [rsp + 0x78]",
        "mov qword ptr [r12 + {error}], r10",
        // 每 CPU 的 CPL3 返回现场。
        "lea r11, [r12 + {user_frame}]",
        "mov r10, qword ptr [rsp + 0x80]",
        "mov qword ptr [r11 + 0x00], r10",
        "mov r10, qword ptr [rsp + 0x88]",
        "mov qword ptr [r11 + 0x08], r10",
        "mov r10, qword ptr [rsp + 0x90]",
        "mov qword ptr [r11 + 0x10], r10",
        "mov r10, qword ptr [rsp + 0x98]",
        "mov qword ptr [r11 + 0x18], r10",
        "mov r10, qword ptr [rsp + 0xA0]",
        "mov qword ptr [r11 + 0x20], r10",
        "lea rdi, [r12 + {frame}]",
        "mov rbp, rsp",
        "and rsp, -16",
        "call qword ptr [rip + {handler}]",
        "mov rsp, rbp",
        // 询问分派器返回的 RIP，并写回硬件帧槽（弹 GPR 前完成，之后不再需要槽基址）。
        "lea r11, [r12 + {user_frame}]",
        "mov qword ptr [r11 + 0x00], rax",
        "mov r10, qword ptr [r11 + 0x00]",
        "mov qword ptr [rsp + 0x80], r10",
        "mov r10, qword ptr [r11 + 0x08]",
        "mov qword ptr [rsp + 0x88], r10",
        "mov r10, qword ptr [r11 + 0x10]",
        "mov qword ptr [rsp + 0x90], r10",
        "mov r10, qword ptr [r11 + 0x18]",
        "mov qword ptr [rsp + 0x98], r10",
        "mov r10, qword ptr [r11 + 0x20]",
        "mov qword ptr [rsp + 0xA0], r10",
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
        "add rsp, 8",
        "cmp byte ptr [rip + {gs}], 0",
        "je 5f",
        "swapgs",
        "5:",
        "iretq",
        percpu = sym PERCPU,
        frame = const PERCPU_FRAME_OFF,
        addr = const PERCPU_FAULT_ADDR_OFF,
        error = const PERCPU_FAULT_ERROR_OFF,
        user_frame = const PERCPU_USER_FRAME_OFF,
        handler = sym PAGE_FAULT_HANDLER,
        gs = sym GS_SWAP,
    )
}

/// 每 CPU `syscall` 快路径入口地址（IA32_LSTAR，受控探针）。
#[cfg(feature = "entry-percpu-probe")]
pub fn syscall_fast_entry_percpu_addr() -> u64 {
    syscall_fast_entry_percpu as *const () as u64
}

/// `syscall` 快路径的每 CPU 版本（仅受控探针；GS-1 交换 + 每 CPU 栈/`USER_RSP`/`ctx`）。
///
/// 快路径在保存用户 GPR 之前不得破坏任何寄存器，故 `USER_RSP` 与栈顶经 `%gs` 直接读写
/// （GS 区布局见 [`GS_USER_RSP_OFF`]/[`GS_SYSCALL_STACK_TOP_OFF`]）。出口用户 `rsp`
/// 暂存到模拟帧的 `ss` 槽，弹完 GPR 后从栈上取回，避免依赖已被恢复的 `r12`。
#[cfg(feature = "entry-percpu-probe")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn syscall_fast_entry_percpu() -> ! {
    core::arch::naked_asm!(
        "cmp byte ptr [rip + {gs}], 0",
        "je 1f",
        "swapgs",
        "1:",
        // 1) 保存用户 rsp、切到本 CPU 专用栈（均经 GS 区，不碰 GPR）。
        "mov qword ptr gs:[{user_rsp}], rsp",
        "mov rsp, qword ptr gs:[{stack_top}]",
        // 2) 模拟 `int 0x80` 硬件帧。
        "push qword ptr [rip + {user_ss}]",
        "push qword ptr gs:[{user_rsp}]",
        "push r11",
        "push qword ptr [rip + {user_cs}]",
        "push rcx",
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
        // 3) 保存块基址写入本 CPU 槽（压栈后 r11 可作临时）。
        "mov r11, qword ptr gs:[{slot}]",
        "mov qword ptr [r11 + {ctx}], rbp",
        "and rsp, -16",
        "sub rsp, 64",
        "mov r10, qword ptr [rbp + 0x00]",
        "mov qword ptr [rsp + 0x00], r10",
        "mov r10, qword ptr [rbp + 0x28]",
        "mov qword ptr [rsp + 0x08], r10",
        "mov r10, qword ptr [rbp + 0x20]",
        "mov qword ptr [rsp + 0x10], r10",
        "mov r10, qword ptr [rbp + 0x18]",
        "mov qword ptr [rsp + 0x18], r10",
        "mov r10, qword ptr [rbp + 0x48]",
        "mov qword ptr [rsp + 0x20], r10",
        "mov r10, qword ptr [rbp + 0x38]",
        "mov qword ptr [rsp + 0x28], r10",
        "mov r10, qword ptr [rbp + 0x40]",
        "mov qword ptr [rsp + 0x30], r10",
        "mov r10, qword ptr [rip + {user_cs}]",
        "mov qword ptr [rsp + 0x38], r10",
        "mov rdi, rsp",
        "call qword ptr [rip + {handle}]",
        "mov rsp, rbp",
        "mov qword ptr [rsp + 0x00], rax",
        // 4) 用户 rsp 暂存到模拟帧的 ss 槽（弹 GPR 后从这里取回）。
        "mov r10, qword ptr gs:[{user_rsp}]",
        "mov qword ptr [rbp + 0x98], r10",
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
        "cmp byte ptr [rip + {gs}], 0",
        "je 3f",
        "swapgs",
        "3:",
        "mov rsp, qword ptr [rsp + 0x20]",
        "sysretq",
        user_rsp = const GS_USER_RSP_OFF,
        stack_top = const GS_SYSCALL_STACK_TOP_OFF,
        slot = const GS_ENTRY_SLOT_OFF,
        ctx = const PERCPU_CTX_OFF,
        handle = sym FAST_SYSCALL_HANDLER,
        user_cs = sym FAST_SYSCALL_CS,
        user_ss = sym FAST_SYSCALL_SS,
        gs = sym GS_SWAP,
    )
}

/// 经每 CPU 恢复槽进入用户态：帧指针写入本 CPU 槽后由 [`exc_resume_entry_percpu`]
/// 消费，并在 `iretq` 前把 GS 切到"已交换"侧（`GS_BASE=0`、`KERNEL_GS_BASE=旧 GS`）。
///
/// 契约：仅在 `cpu` 上、CPL0、本 CPU GS 区已安装且即将 iret 到 CPL3 时调用。
#[cfg(feature = "entry-percpu-probe")]
pub fn resume_percpu(cpu: usize, frame: &SretFrame) -> ! {
    percpu(cpu)
        .resume_frame
        .store(frame as *const SretFrame as u64, Ordering::Relaxed);
    // SAFETY: 见函数契约——帧已校验且有效；GS 区已安装，入口读取本 CPU 槽。
    unsafe { exc_resume_entry_percpu() }
}

/// P5-3b-3：带用户 `%gs` 基址的每 CPU 恢复（真实任务用）。
///
/// 与 [`resume_percpu`] 的唯一差别：`iretq` 前把 `GS_BASE` 置为 `user_gs` 而不是 0，
/// 内核侧原 GS 基址仍保存到 `KERNEL_GS_BASE`。`user_gs` 为 0 时与旧跳板等价。
///
/// 契约同 [`resume_percpu`]；`user_gs` 必须是已校验的低半区规范地址（调用方负责）。
#[cfg(feature = "entry-percpu-probe")]
pub fn resume_percpu_user(cpu: usize, frame: &SretFrame, user_gs: u64) -> ! {
    let slot = percpu(cpu);
    slot.resume_frame
        .store(frame as *const SretFrame as u64, Ordering::Relaxed);
    slot.user_gs.store(user_gs, Ordering::Relaxed);
    // SAFETY: 见函数契约——帧与 GS 基址已校验；入口读取本 CPU 槽。
    unsafe { exc_resume_entry_percpu_user() }
}

/// 指定 CPU 最近一次恢复跳板写入的用户 GS 基址（测试/诊断用）。
#[cfg(feature = "entry-percpu-probe")]
pub fn percpu_user_gs(cpu: usize) -> u64 {
    percpu(cpu).user_gs.load(Ordering::Relaxed)
}

/// 每 CPU 恢复跳板：从 `%gs:0` 索引的槽读取帧指针，加载完整 `iretq` 帧与 GPR，
/// 并把两根 GS MSR 切到用户态（`GS_BASE=0`、`KERNEL_GS_BASE=原 GS`）后 `iretq`。
#[cfg(feature = "entry-percpu-probe")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn exc_resume_entry_percpu() -> ! {
    core::arch::naked_asm!(
        "mov eax, dword ptr gs:[0]",
        "lea r10, [rip + {percpu}]",
        "shl rax, 8",
        "add r10, rax",
        "mov r11, r10",
        "mov r10, qword ptr [r10 + {resume}]",
        // 清零本 CPU 槽，证明帧指针被本槽消费。
        "mov qword ptr [r11 + {resume}], 0",
        // 切 GS 到用户侧：先保存原 GS（rdmsr），再 GS=0、KERNEL_GS=原 GS。
        "mov ecx, 0xC0000101",
        "rdmsr",
        "mov r8d, eax",
        "mov r9d, edx",
        "xor eax, eax",
        "xor edx, edx",
        "wrmsr",
        "mov ecx, 0xC0000102",
        "mov eax, r8d",
        "mov edx, r9d",
        "wrmsr",
        // 压完整 iret 帧。
        "push qword ptr [r10 + 0x98]",
        "push qword ptr [r10 + 0x38]",
        "push qword ptr [r10 + 0x80]",
        "push qword ptr [r10 + 0x90]",
        "push qword ptr [r10 + 0x88]",
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
        percpu = sym PERCPU,
        resume = const PERCPU_RESUME_OFF,
    )
}

/// 带用户 `%gs` 基址的每 CPU 恢复跳板（P5-3b-3）。
///
/// 与 [`exc_resume_entry_percpu`] 相同，只把 `GS_BASE` 从 0 改为本槽的 `user_gs`；
/// `KERNEL_GS_BASE` 仍保存内核侧原 GS，供入口 `swapgs` 配对。
#[cfg(feature = "entry-percpu-probe")]
#[unsafe(naked)]
#[unsafe(no_mangle)]
unsafe extern "C" fn exc_resume_entry_percpu_user() -> ! {
    core::arch::naked_asm!(
        "mov eax, dword ptr gs:[0]",
        "lea r10, [rip + {percpu}]",
        "shl rax, 8",
        "add r10, rax",
        "mov r11, r10",
        "mov r10, qword ptr [r10 + {resume}]",
        "mov qword ptr [r11 + {resume}], 0",
        // 保存内核 GS，然后写入本任务用户 GS（可能为 0），最后把内核 GS 存入 KERNEL_GS。
        "mov ecx, 0xC0000101",
        "rdmsr",
        "mov r8d, eax",
        "mov r9d, edx",
        "mov rax, qword ptr [r11 + {user_gs}]",
        "mov rdx, rax",
        "shr rdx, 32",
        "wrmsr",
        "mov ecx, 0xC0000102",
        "mov eax, r8d",
        "mov edx, r9d",
        "wrmsr",
        "push qword ptr [r10 + 0x98]",
        "push qword ptr [r10 + 0x38]",
        "push qword ptr [r10 + 0x80]",
        "push qword ptr [r10 + 0x90]",
        "push qword ptr [r10 + 0x88]",
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
        percpu = sym PERCPU,
        resume = const PERCPU_RESUME_OFF,
        user_gs = const PERCPU_USER_GS_OFF,
    )
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
    fn error_code_exception_shifts_only_hardware_return_words() {
        let mut block = [0u64; 21];
        for (index, slot) in block.iter_mut().enumerate() {
            *slot = index as u64 + 100;
        }
        let frame = SaveBlock::new(block.as_ptr()).to_sret_frame_with_error();
        assert_eq!(frame.rax, 100);
        assert_eq!(frame.r15, 114);
        assert_eq!(frame.rip, 116);
        assert_eq!(frame.cs, 117);
        assert_eq!(frame.rflags, 118);
        assert_eq!(frame.rsp, 119);
        assert_eq!(frame.ss, 120);
        assert_eq!(SaveBlock::new(block.as_ptr()).cs_with_error(), 117);
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
        let base = &stack as *const SyscallStack as u64 + SYSCALL_GUARD_SIZE as u64;
        assert_eq!(SYSCALL_STACK_SIZE, 32 * 1024);
        assert!(stack_canary_at(base));
        for index in 0..STACK_CANARY_BYTES {
            stack.0[SYSCALL_GUARD_SIZE + index] ^= 1;
            assert!(!stack_canary_at(base), "byte {index}");
            stack.0[SYSCALL_GUARD_SIZE + index] ^= 1;
        }
        stack.0[SYSCALL_GUARD_SIZE + STACK_CANARY_BYTES] = 1;
        assert_eq!(stack.0[SYSCALL_GUARD_SIZE + STACK_CANARY_BYTES], 1);
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
        assert!((SYSCALL_STACK.as_ptr() as usize).is_multiple_of(SYSCALL_GUARD_SIZE));
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

    /// P5-3b-1/2：每 CPU 槽的步长/偏移是裸入口与访问器的共同契约，必须钉住。
    #[cfg(feature = "entry-percpu-probe")]
    #[test]
    fn percpu_entry_layout_is_pinned_and_slots_are_distinct() {
        use core::mem::offset_of;
        assert_eq!(size_of::<PerCpuEntry>(), PERCPU_STRIDE);
        assert_eq!(offset_of!(PerCpuEntry, context_base), 0x00);
        assert_eq!(offset_of!(PerCpuEntry, fault_addr), 0x08);
        assert_eq!(offset_of!(PerCpuEntry, fault_error), 0x10);
        assert_eq!(offset_of!(PerCpuEntry, fault_frame), 0x20);
        assert_eq!(offset_of!(PerCpuEntry, resume_frame), 0xC0);
        assert_eq!(offset_of!(PerCpuEntry, user_frame), 0xC8);
        assert_eq!(PERCPU.len(), PERCPU_MAX);
        let mut bases = [0u64; PERCPU_MAX];
        for (cpu, slot) in bases.iter_mut().enumerate() {
            *slot = percpu(cpu) as *const PerCpuEntry as u64;
            assert_eq!(percpu_fault_addr(cpu), 0);
            assert_eq!(percpu_fault_error(cpu), 0);
            assert_eq!(percpu_fault_frame(cpu), SretFrame::ZERO);
            assert!(percpu_context_base(cpu).is_none());
            assert!(percpu_resume_frame(cpu).is_none());
            assert_eq!(percpu_user_frame(cpu), UserFrame::default());
        }
        for i in 0..bases.len() {
            for j in 0..i {
                assert_eq!(bases[i] - bases[j], (i - j) as u64 * PERCPU_STRIDE as u64);
            }
        }
    }

    #[cfg(feature = "entry-percpu-probe")]
    #[test]
    #[should_panic(expected = "cpu index out of range")]
    fn percpu_accessor_rejects_out_of_range_cpu() {
        let _ = percpu_fault_addr(PERCPU_MAX);
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
