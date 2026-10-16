//! 切片 D 的恢复落点与收尾（二进制 crate 模块）。
//!
//! `protocol`/`serial`/`qemu` 都属于二进制 crate，lib 无法访问，故"恢复后要做
//! 什么"必须放在这里。流程：`run_and_exit` 取 `idt` 捕获的 #PF 现场帧，改写成
//! 指向框架恢复落点、指向专用恢复栈、带两个哨兵寄存器的帧，经
//! `kernel::exc::resume` 真实 `iretq` 进去；框架落点把实测寄存器/栈/RFLAGS 记入静态，
//! 再经注册的 `after_resume` 上报证据并退出 QEMU。
//!
//! **边界：** 这不是抽象模型的 `exc_resume`（无 capability/subscription/pending/
//! `TaskResumed` audit/`Faulted→Ready`），也不恢复被捕现场本身（RIP/RSP 被改写为
//! landing/恢复栈）。它是 ring0 低层跳板，用来实证"验证一帧后真实恢复"。
//!
//! 切片 AH 后，裸落点与它触碰的静态已下沉 [`prima_ostd::entry`]；本模块只保留恢复栈、
//! 落点注册与收尾逻辑，不含任何 `unsafe`。

use crate::protocol;
use crate::qemu;
use crate::serial;
use prima_ostd::sync::StaticCell;

/// 恢复栈容量（16 KiB，`#[repr(align(16))]` 保证栈顶 16 对齐）。
const RESUME_STACK_SIZE: usize = 16 * 1024;

#[repr(align(16))]
struct ResumeStack(
    #[allow(dead_code)] // 只经 `as_ptr()` 取地址使用，从不按字段读取。
    [u8; RESUME_STACK_SIZE],
);

/// 专用恢复栈：切换 `rsp` 到这里，证明恢复真的换了栈（盲跳路径做不到）。
static RESUME_STACK: StaticCell<ResumeStack> = StaticCell::new(ResumeStack([0; RESUME_STACK_SIZE]));

/// SPEC-073：§12.10 帧验证所需的 `(landing, stack_bottom, stack_top)`。
pub fn resume_target() -> (u64, u64, u64) {
    let (bottom, top) = stack_bounds();
    (prima_ostd::entry::resume_landing_addr(), bottom, top)
}

/// 恢复栈的 `(下界, 上界)`；上界为 one-past-end、16 对齐。
fn stack_bounds() -> (u64, u64) {
    let base = RESUME_STACK.as_ptr() as u64;
    (base, base + RESUME_STACK_SIZE as u64)
}

/// 恢复后的收尾：核对四条车道、上报 `exc_resume_ready`、发出 `boot_complete`、退出。
///
/// 由框架裸落点经注册回调进入（不返回）；实测寄存器/栈/RFLAGS 从框架静态读取。
extern "C" fn after_resume() -> ! {
    let r12 = prima_ostd::entry::seen_r12();
    let r13 = prima_ostd::entry::seen_r13();
    let rsp = prima_ostd::entry::seen_rsp();
    let rflags = prima_ostd::entry::seen_rflags();
    let (_, top) = stack_bounds();

    let resumed = r12 == kernel::exc::RESUME_SENTINEL_A && r13 == kernel::exc::RESUME_SENTINEL_B;
    let rsp_ok = rsp == top;
    // 恢复帧把 ZF(bit 6) 置位；若 RFLAGS 未被 iretq 恢复则该位不会出现。
    let flags_ok = rflags & (1 << 6) != 0;

    // SPEC-082：Timer 见证——采集点必须在抢占阶段**之后**（`t1` 依赖 PIT tick 推进到期）。
    protocol::timer_ready(&kernel::syscall::timer_witness());
    // SPEC-083：§13.2 `audit(cap)` 纯查询见证（缓存回执 + 内核侧重算比对）。
    protocol::audit_ready(&kernel::syscall::audit_witness());
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
    // SPEC-073：§12.10 `exc_resume` 的 KernelActor 代调见证。
    match kernel::bootstrap::resume_witness_record() {
        Some(witness) => protocol::resume_ready(&witness),
        None => protocol::panic_event(),
    }
    protocol::boot_complete();
    // EXO-3I：原始 trace 调试通道（非 PRIMA 行，不影响既有事件序列校验）。
    protocol::trace_dump();
    protocol::user_fault_dump();
    serial::flush();
    qemu::exit(qemu::SUCCESS)
}

/// 切片 G：用户任务以 `int3` 从 CPL3 陷入内核时的收尾回调（由 `idt` 调用，不返回）。
///
/// 用户 COW 的 error code 在故障时已单独保存（后续驱动 #PF 可覆盖硬件槽）；据此构造
/// [`kernel::user::UserWitness`]、发出 `user_cow_ready`，随后继续切片 D 的帧恢复见证。
pub extern "C" fn user_trap_exit() -> ! {
    let err = kernel::cow::user_cow_err().unwrap_or_else(kernel::idt::last_fault_err);
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
    // EXO-3D：`runtime_hello_ready` 不再由内核读共享段固定偏移；改为透传 ring3
    // 客户端通过 `SYS_TRACE_EMIT` 写入的通用 trace 记录。
    match kernel::syscall::trace_find(prima_abi::trace::EVENT_RUNTIME_HELLO, 0) {
        Some(record) => protocol::runtime_hello_trace(&record),
        None => {
            protocol::panic_event();
            serial::flush();
            qemu::exit(qemu::PANIC)
        }
    }
    // SPEC-087：init 与**存活**客户端的双向握手（请求 → 应答 → 客户端写回执）。
    protocol::service_reply_ready(&kernel::user::service_reply_witness());
    // SPEC-088：init（broker）把客户端 Hello 转发给 ring3 Rust 服务端点，服务解析+协商。
    protocol::service_negotiated_ready(&kernel::user::service_negotiated_witness());
    // SPEC-090：init（服务管理器）按客户端写入共享段的服务名解析端点并二次路由 + 未知名拒绝。
    protocol::service_selected_ready(&kernel::user::service_selected_witness());
    // SPEC-103：ring3 服务管理器任务被 init 建起并自报存活（独立二进制映像、id 非 init）。
    protocol::servicemgr_ready(&kernel::user::servicemgr_witness());
    // SPEC-081：§11 Namespace 路径操作（七个原语的 ring3 端到端见证 + 环检测）。
    let namespace = kernel::syscall::ns_witness();
    protocol::namespace_ready(&namespace);
    // SPEC-082：Timer 的 ring3 回执在此**缓存**——`after_resume` 已切回内核根，`USER_Y_VA`
    // 不再映射，故回执必须在 task 1 的地址空间下读出。
    kernel::syscall::cache_timer_receipts();
    // SPEC-083：`audit(cap)` 回执同样必须在 task 1 的地址空间下缓存（上面两条查询）。
    kernel::syscall::cache_audit_receipts();
    // 切片 O：进入 PIT 时钟抢占阶段（两任务被时钟轮转，永不由用户 `yield` 结束）。
    kernel::preempt::set_finish_handler(preempt_finish);
    kernel::preempt::run()
}

/// 切片 O 完成回调：上报时钟抢占见证，再走既有 `exc_resume` 收尾（不返回）。
fn preempt_finish() -> ! {
    // SPEC-082 / §10.3：`advance_time` 是 `KernelActor` 的内部迁移。本实现把它放在**抢占收尾点**
    // 一次推进到已累计的 tick——单调性与「迁移后无 `deadline ≤ now` 的 Pending」都成立。
    //
    // **不在 `preempt_tick` 里逐 tick 调用**：那会在中断上下文里做通知投递（投递会唤醒任务、
    // 改调度状态，而此刻正在保存/切换上下文），且中断处理时长会进入用户任务的执行时序、扰动
    // `preempt_ready.mark*` 这类时序敏感见证（实测：逐 tick 调用会让 `mark1` 由 0x1 变 0x0）。
    let _ = kernel::bootstrap::advance_time(kernel::preempt::ticks());
    let witness = kernel::preempt::witness();
    protocol::preempt_ready(&witness);
    run_and_exit()
}

/// 构造并校验一帧恢复帧，然后真实 `iretq` 进去（不返回）。
///
/// 帧的 RIP 指向框架恢复落点、RSP 指向恢复栈顶、CS/SS 为内核选择子、RFLAGS 仅置
/// FIXED+ZF、两个哨兵寄存器置为已知常量。若校验失败则按 panic 退出（不盲目恢复）。
pub fn run_and_exit() -> ! {
    // 落点回调须在跳转前注册（框架落点经该函数指针进入 after_resume）。
    prima_ostd::entry::set_resume_handler(after_resume);

    let mut frame = kernel::idt::fault_frame();
    let (bottom, top) = stack_bounds();
    frame.rip = prima_ostd::entry::resume_landing_addr();
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
    kernel::exc::resume(&frame)
}
