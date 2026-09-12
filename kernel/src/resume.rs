//! 切片 D 的恢复落点与收尾（二进制 crate 模块）。
//!
//! `protocol`/`serial`/`qemu` 都属于二进制 crate，lib 无法访问，故"恢复后要做
//! 什么"必须放在这里。流程：`run_and_exit` 取 `idt` 捕获的 #PF 现场帧，改写成
//! 指向 `resume_landing` 的、指向专用恢复栈的、带两个哨兵寄存器的帧，经
//! `kernel::exc::resume` 真实 `iretq` 进去；`resume_landing` 把实测寄存器/栈/
//! RFLAGS 存入静态，再调用 `after_resume` 上报证据并退出 QEMU。
//!
//! **边界：** 这不是抽象模型的 `exc_resume`（无 capability/subscription/pending/
//! `TaskResumed` audit/`Faulted→Ready`），也不恢复被捕现场本身（RIP/RSP 被改写为
//! landing/恢复栈）。它是 ring0 低层跳板，用来实证"验证一帧后真实恢复"。

use core::sync::atomic::{AtomicU64, Ordering};

use crate::protocol;
use crate::qemu;
use crate::serial;

/// 恢复栈容量（16 KiB，`#[repr(align(16))]` 保证栈顶 16 对齐）。
const RESUME_STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
struct ResumeStack(
    #[allow(dead_code)] // 只经 `addr_of!` 取地址使用，从不按字段读取。
    [u8; RESUME_STACK_SIZE],
);

/// 专用恢复栈：切换 `rsp` 到这里，证明恢复真的换了栈（盲跳路径做不到）。
static mut RESUME_STACK: ResumeStack = ResumeStack([0; RESUME_STACK_SIZE]);

/// 落点实测的 `r12`/`r13`/`rsp`/`rflags`。
static SEEN_R12: AtomicU64 = AtomicU64::new(0);
static SEEN_R13: AtomicU64 = AtomicU64::new(0);
static SEEN_RSP: AtomicU64 = AtomicU64::new(0);
static SEEN_RFLAGS: AtomicU64 = AtomicU64::new(0);

/// 恢复栈的 `(下界, 上界)`；上界为 one-past-end、16 对齐。
fn stack_bounds() -> (u64, u64) {
    let base = core::ptr::addr_of!(RESUME_STACK) as u64;
    (base, base + RESUME_STACK_SIZE as u64)
}

/// `iretq` 的落点：记录被恢复的寄存器与栈指针，然后进入 `after_resume`。
///
/// 用 `call` 而不是 `jmp`：`frame.rsp` 16 对齐时，`call` 使 `after_resume` 以
/// `rsp % 16 == 8` 进入，符合 SysV 调用约定（`jmp` 会以 0 进入）。
///
/// # Safety
///
/// 只能由 `exc_resume_entry` 经 `iretq` 进入：进入时 `rsp` 必须是有效恢复栈、
/// `r12`/`r13` 是恢复帧携带的值。不可从 Rust 侧调用。
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn resume_landing() -> ! {
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
        "call {after}",
        s12 = sym SEEN_R12,
        s13 = sym SEEN_R13,
        srsp = sym SEEN_RSP,
        sflags = sym SEEN_RFLAGS,
        after = sym after_resume,
    )
}

/// 恢复后的收尾：核对四条车道、上报 `exc_resume_ready`、发出 `boot_complete`、退出。
extern "C" fn after_resume() -> ! {
    let r12 = SEEN_R12.load(Ordering::Relaxed);
    let r13 = SEEN_R13.load(Ordering::Relaxed);
    let rsp = SEEN_RSP.load(Ordering::Relaxed);
    let rflags = SEEN_RFLAGS.load(Ordering::Relaxed);
    let (_, top) = stack_bounds();

    let resumed = r12 == kernel::exc::RESUME_SENTINEL_A && r13 == kernel::exc::RESUME_SENTINEL_B;
    let rsp_ok = rsp == top;
    // 恢复帧把 ZF(bit 6) 置位；若 RFLAGS 未被 iretq 恢复则该位不会出现。
    let flags_ok = rflags & (1 << 6) != 0;

    protocol::exc_resume_ready(r12, r13, resumed as u32, rsp_ok as u32, flags_ok as u32);
    protocol::boot_complete();
    serial::flush();
    qemu::exit(qemu::SUCCESS)
}

/// 构造并校验一帧恢复帧，然后真实 `iretq` 进去（不返回）。
///
/// 帧的 RIP 指向 [`resume_landing`]、RSP 指向恢复栈顶、CS/SS 为内核选择子、
/// RFLAGS 仅置 FIXED+ZF、两个哨兵寄存器置为已知常量。若校验失败则按 panic 退出
/// （不盲目恢复）。
pub fn run_and_exit() -> ! {
    let mut frame = kernel::idt::fault_frame();
    let (bottom, top) = stack_bounds();
    frame.rip = resume_landing as *const () as u64;
    frame.rsp = top;
    frame.cs = kernel::exc::KERNEL_CS;
    frame.ss = kernel::exc::KERNEL_SS;
    frame.rflags = kernel::exc::KERNEL_RFLAGS_FIXED | (1 << 6);
    frame.r12 = kernel::exc::RESUME_SENTINEL_A;
    frame.r13 = kernel::exc::RESUME_SENTINEL_B;

    if kernel::exc::validate_for_kernel_resume(&frame, frame.rip, bottom, top).is_err() {
        protocol::panic_event();
        serial::flush();
        qemu::exit(qemu::PANIC)
    }

    // 帧活在本（boot）栈上；`resume` 不返回，故其地址在跳转期间始终有效。
    unsafe { kernel::exc::resume(&frame) }
}
