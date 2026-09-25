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
    // 切片 R：平台 bring-up 区域对象化见证（真实 Segment 对象 + 内核持有的可授权根 Capability）。
    match kernel::bootstrap::platform_segments_record() {
        Ok(record) => protocol::platform_segments_ready(&record),
        Err(_) => protocol::panic_event(),
    }
    // 切片 P：阶段末见证——初始 Authority 与帧唯一归属守恒（既有 seq 1..21 不变）。
    match kernel::bootstrap::authority_record() {
        Ok(record) => protocol::authority_ready(&record),
        Err(_) => protocol::panic_event(),
    }
    match kernel::frames::witness() {
        Ok(witness) => protocol::frames_ready(&witness),
        Err(_) => protocol::panic_event(),
    }
    // SPEC-069：自建 CR3 见证——此刻内核正运行在**自建根**（或其克隆）上。
    match kernel::user::self_root_witness() {
        Some(witness) => protocol::self_root_ready(&witness),
        None => protocol::panic_event(),
    }
    // SPEC-070：Faulted 状态机见证（boot 期采集，此刻上报）。
    match kernel::bootstrap::fault_witness_record() {
        Some(witness) => protocol::fault_ready(&witness),
        None => protocol::panic_event(),
    }
    // SPEC-071：§13.1/W13 Task 状态迁移审计汇总。
    match kernel::bootstrap::task_state_audit() {
        Some(audit) => protocol::state_audit_ready(&audit),
        None => protocol::panic_event(),
    }
    // SPEC-072：§8.2 内核回收路径见证。
    match kernel::bootstrap::reclaim_witness_record() {
        Some(witness) => protocol::reclaim_ready(&witness),
        None => protocol::panic_event(),
    }
    protocol::boot_complete();
    serial::flush();
    qemu::exit(qemu::SUCCESS)
}

/// 切片 G：用户任务以 `int3` 从 CPL3 陷入内核时的收尾回调（由 `idt` 调用，不返回）。
///
/// 此时最近一次 #PF 的 error code 仍是用户写只读页的 `0x7`（P|W|US），据此构造
/// [`kernel::user::UserWitness`]、发出 `user_cow_ready`，随后继续切片 D 的帧恢复见证。
pub extern "C" fn user_trap_exit() -> ! {
    let err = kernel::idt::last_fault_err();
    let witness = kernel::user::witness(err);
    protocol::user_cow_ready(&witness);
    // 切片 H/I：用户代码在 COW 之后完成 6 次 `int 0x80`（ABI v1 电池）。
    let syscall = kernel::syscall::witness();
    protocol::syscall_ready(&syscall);
    // 切片 K：随后 3 次 `int 0x80` 能力授权查询。
    let auth = kernel::syscall::auth_witness();
    protocol::syscall_auth_ready(&auth);
    // 切片 L：随后 6 次 `int 0x80` 异常订阅/取消订阅，以及平台初始化那次 pending 直投。
    let exc = kernel::syscall::exc_witness();
    protocol::exc_ready(&exc);
    let pending = kernel::syscall::pending_witness();
    protocol::exc_pending_ready(&pending);
    // 切片 J：此后还有一次 `syscall` 指令快路径往返。
    let fast = kernel::syscall::fast_witness();
    protocol::syscall_fast_ready(&fast);
    // 切片 M：快路径之后是双任务协作往返（task1→task2→task1）。
    let multitask = kernel::syscall::mt_witness();
    protocol::multitask_ready(&multitask);
    // 切片 N：最后是跨任务阻塞 raise/wait（task1 阻塞在 WAIT，task2 TRAP 唤醒）。
    let exc_wait = kernel::syscall::exc_wait_witness();
    protocol::exc_wait_ready(&exc_wait);
    // S2-D1/D2：句柄编码与能力生命周期（用户态在唤醒后追加执行，回执读自 Y 页）。
    protocol::handle_ready();
    let caps = kernel::syscall::cap_witness();
    protocol::cap_lifecycle_ready(&caps);
    // S2-D3：Segment 原语（create/map/unmap/destroy + 清零 + stale）。
    let memseg = kernel::syscall::memseg_witness();
    protocol::memseg_ready(&memseg);
    // 切片 S：IPC 用户化 + 两任务共享 Segment（S2-D5/S2-X2 主见证）。
    let ipc = kernel::syscall::ipc_witness();
    protocol::ipc_ready(&ipc);
    // S2-D4：Task create/yield/destroy 用户原语 + 单核调度（S2-D4 主见证）。
    let task = kernel::syscall::task_witness();
    protocol::task_ready(&task);
    // 切片 O：进入 PIT 时钟抢占阶段（两任务被时钟轮转，永不由用户 `yield` 结束）。
    kernel::preempt::set_finish_handler(preempt_finish);
    kernel::preempt::run()
}

/// 切片 O 完成回调：上报时钟抢占见证，再走既有 `exc_resume` 收尾（不返回）。
fn preempt_finish() -> ! {
    let witness = kernel::preempt::witness();
    protocol::preempt_ready(&witness);
    run_and_exit()
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
