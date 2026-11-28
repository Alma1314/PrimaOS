//! P5-3b-3：每 CPU 可运行队列、创建时放置与唯一运行者的受控双核探针。
//!
//! 本模块只编入 `sched-percpu-probe`（隐含 `smp-ap`/`gs3-probe`/`entry-percpu-probe`），
//! 普通镜像不含它。BSP 与一个 AP 各发布一个**真实 Task**（task 1/2，来自真实
//! `BootstrapState`），进入每 CPU 调度循环：用户机器码先在内核屏障等待双方到齐，
//! 再由 CPU1 的真实 `SYS_RECV` 阻塞、CPU0 的真实 `SYS_SEND` 跨核唤醒；随后双方执行
//! 真实 `SYS_FORK`（子任务按父 CPU 入队），最后在用户态写只读页触发 #PF，处理程序在
//! 同一 CPU 嵌套触发一次 CPL0 #PF 后再换映射成功重试。所有用户现场由每 CPU 入口槽保存。
//!
//! P5-3b-4 追加：`SYS_TASK_START` 的 held-launch 创建时放置、prepared fork
//! （`SYS_FORK_TRANSACTION`）与 `SYS_THREAD_CREATE` 的随父/随调用者发布，以及双核同时
//! `SYS_EXEC_REPLACE` 换根后 4 轮跨核阻塞/唤醒（丢唤醒矩阵）。
//!
//! P5-3b-5 追加：反向 4 轮丢唤醒；Gate spawn/fork 的创建时放置（仅 host 单测）。
//! P5-3b-6 追加：pre-exec fork 子进程并发 exec 第二映像，父进程 exec 后核对换根、
//! 停放与 owner，再做 fork/exec 后的双向唤醒。
//!
//! **边界**：仅 2 个 CPU 承载负载（`-smp 4` 的其余 AP 在线但停放）；本命令使用 MTTCG
//! （每 vCPU 一个宿主线程）观察真实并行，单线程 TCG 会在自旋 vCPU 上饿死同一工作负载。
//! 仍无 IPI：跨核唤醒只按 owner 入队，由目标 CPU 自己的空闲循环轮询（P5-4 引入 IPI 前
//! 的探针取舍）。Gate 端到端双核行为与用户态服务管理器仍未接线。

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

use prima_ostd::entry::{self, AbiFrame, SretFrame};

use crate::bootstrap;
use crate::frames;
use crate::gdt::{USER_CS, USER_DS};
use crate::handle::Handle;
use crate::mem::{FRAME_SIZE, phys_to_virt};
use crate::rights::{Right, Rights};
use crate::sched;
use crate::vspace;

const MAX_CPUS: usize = crate::cpu::local::MAX_CPUS;
const MAX_TASKS: usize = sched::MAX_TASKS;

/// 探针用户区（PML4 槽 24；每任务根各自映射，同 VA 不同物理帧）。
const U_SLOT: usize = 24;
const U_CODE_VA: u64 = (U_SLOT as u64) << 39;
const U_STACK_VA: u64 = U_CODE_VA + FRAME_SIZE;
const U_DATA_VA: u64 = U_CODE_VA + 2 * FRAME_SIZE;
const U_RO_VA: u64 = U_CODE_VA + 3 * FRAME_SIZE;
/// 嵌套 CPL0 故障地址（PML4 槽 25，未映射）。
const NESTED_VA: u64 = (25u64) << 39;
/// held-launch 任务自己的代码/栈 VA（PML4 槽 26；父根该槽为空，避免与探针页重叠）。
const HELD_CODE_VA: u64 = (26u64) << 39;
const HELD_STACK_VA: u64 = HELD_CODE_VA + FRAME_SIZE;

/// 探针 syscall 魔数（真实调用号之外；处理程序只识别这些，其余转交真实分派）。
const MAGIC_BARRIER: u64 = 0x5C0D_0100;
const MAGIC_WAIT_BLOCKED: u64 = 0x5C0D_0200;
const MAGIC_WAKE_SEEN: u64 = 0x5C0D_0300;
const MAGIC_CHILD_RUN: u64 = 0x5C0D_0400;
const MAGIC_CHILD_DONE: u64 = 0x5C0D_0500;
const MAGIC_FORK_DONE: u64 = 0x5C0D_0600;
const MAGIC_NESTED_DONE: u64 = 0x5C0D_0700;
const MAGIC_CPU_DONE: u64 = 0x5C0D_0800;
const MAGIC_HELD_RUN: u64 = 0x5C0D_1100;
const MAGIC_HELD_WAIT: u64 = 0x5C0D_1200;
const MAGIC_PREP_CHILD_RUN: u64 = 0x5C0D_1300;
const MAGIC_PREP_CHILD_DONE: u64 = 0x5C0D_1400;
const MAGIC_PREP_DONE: u64 = 0x5C0D_1500;
const MAGIC_THREAD_RUN: u64 = 0x5C0D_1600;
const MAGIC_THREAD_DONE: u64 = 0x5C0D_1700;
const MAGIC_EXEC_BARRIER: u64 = 0x5C0D_1800;
const MAGIC_EXEC_RUN: u64 = 0x5C0D_1900;
const MAGIC_EXEC_FAIL: u64 = 0x5C0D_1A00;
const MAGIC_LOST_ACK: u64 = 0x5C0D_1B00;
const MAGIC_LOST_WAIT: u64 = 0x5C0D_1C00;
const MAGIC_LOST_DONE: u64 = 0x5C0D_1D00;
const MAGIC_REV_GATE: u64 = 0x5C0D_1E00;
const MAGIC_REV_ACK: u64 = 0x5C0D_1F00;
const MAGIC_REV_WAIT: u64 = 0x5C0D_2000;
const MAGIC_REV_DONE: u64 = 0x5C0D_2100;
const MAGIC_FORK_EXEC_CHILD_RUN: u64 = 0x5C0D_2300;
const MAGIC_FORK_EXEC_DONE: u64 = 0x5C0D_2400;
const MAGIC_FE_FAIL: u64 = 0x5C0D_2500;
const MAGIC_POST_ACK: u64 = 0x5C0D_2600;
const MAGIC_POST_WAIT: u64 = 0x5C0D_2700;
const MAGIC_POST_DONE: u64 = 0x5C0D_2800;

/// 换根后跨核阻塞/唤醒矩阵的轮数。
const LOST_ROUNDS: u64 = 4;

/// 探针数据页内的布局（避开 `SYS_RECV` 输出指针与 ELF 映像）。
const RECV_OUT_OFF: u64 = 0x100;
// `SYS_RECV` 的输出区从 0x100 起最多写 40 字节，元数据与 ELF 映像必须避开。
const HELD_CAP_OFF: u64 = 0x7e0;
const IMAGE_LEN_OFF: u64 = 0x7f0;
const ELF_IMAGE_OFF: u64 = 0x800;
/// exec 后新映像的装载 VA 与入口（PML4 槽 1；槽 0 被内核低半区映像占用）。
const EXEC_IMAGE_VA: u64 = 1u64 << 39;
/// 子进程 exec 的第二映像根槽（与父映像槽 1 区分，便于核对旧映射消失）。
///
/// 选用任务根已清零的 `U_SLOT`：bootloader 低半区其余槽可能含 supervisor 项，
/// exec 会拒绝在已存在的 supervisor PML4 项上建立用户映像。
const EXEC_CHILD_VA: u64 = 24u64 << 39;
/// 子进程 ELF 在 fork 前数据页中的偏移（父 ELF 从 0x800 起，约 1 KiB）。
const CHILD_ELF_DATA_OFF: u32 = 0xC00;

/// 跨核 SEND/RECV 的载荷；双方都核对。
const PAYLOAD: u64 = 0x5C0D_9001;

/// 超时（TSC 周期）。
const BARRIER_LIMIT: u64 = 1_000_000_000;
const BLOCKED_LIMIT: u64 = 2_000_000_000;
const WATCHDOG_LIMIT: u64 = 2_000_000_000;

static ACTIVE: AtomicU32 = AtomicU32::new(1);
static APS: AtomicU32 = AtomicU32::new(0);
static GO: AtomicBool = AtomicBool::new(false);
static PHYS: AtomicU64 = AtomicU64::new(0);
static START: AtomicU64 = AtomicU64::new(0);
static SETUP_STEP: AtomicU32 = AtomicU32::new(0);

static ARRIVED: AtomicU32 = AtomicU32::new(0);
static BARRIER_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static WAKE_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static FORK_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static CHILD_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static NESTED_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static PREP_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static THREAD_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static EXEC_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static LOST_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static LOST_ACK: [AtomicU32; LOST_ROUNDS as usize] =
    [const { AtomicU32::new(0) }; LOST_ROUNDS as usize];
static REV_ACK: [AtomicU32; LOST_ROUNDS as usize] =
    [const { AtomicU32::new(0) }; LOST_ROUNDS as usize];
static REV_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static REV_FAIL: AtomicBool = AtomicBool::new(false);
static FORK_EXEC_RAN: [AtomicU32; MAX_TASKS] = [const { AtomicU32::new(0) }; MAX_TASKS];
static FORK_EXEC_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static FE_CODE: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
/// fork/exec 后双向矩阵的 2 轮确认（0 = CPU0→CPU1，1 = CPU1→CPU0）。
static POST_ACK: [AtomicU32; 2] = [const { AtomicU32::new(0) }; 2];
static POST_OK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static POST_FAIL: AtomicBool = AtomicBool::new(false);
static EXEC_ARRIVED: AtomicU32 = AtomicU32::new(0);
static LAUNCH_OK: AtomicU32 = AtomicU32::new(0);
static HELD_RAN: AtomicBool = AtomicBool::new(false);
static HELD_TASK: AtomicU32 = AtomicU32::new(0);
static HELD_OWNER: AtomicU32 = AtomicU32::new(u32::MAX);
static THREAD_RAN: [AtomicU32; MAX_TASKS] = [const { AtomicU32::new(0) }; MAX_TASKS];
static PREP_CHILD_RAN: [AtomicU32; MAX_TASKS] = [const { AtomicU32::new(0) }; MAX_TASKS];
static PRE_EXEC_ROOT: [AtomicU64; MAX_TASKS] = [const { AtomicU64::new(0) }; MAX_TASKS];
static DATA_FRAME: [AtomicU64; MAX_TASKS] = [const { AtomicU64::new(0) }; MAX_TASKS];
static DONE: AtomicU32 = AtomicU32::new(0);
static PARK_FAILED: AtomicU32 = AtomicU32::new(0);
static TIMED_OUT: AtomicBool = AtomicBool::new(false);
/// 诊断：每 CPU 空闲循环阶段（1 = ap_enter，2 = 见 GO，3 = 见候选，4 = 取出，5 = 调度成功）。
static STAGE: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static STAGE_TASK: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static SCHED_ERR: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static EXC: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static EXC_RIP: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static AP_ENTRIES: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static AP_LAST_INDEX: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static SPIN: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static FORK_RAW: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
static RESUMES: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

pub fn note_stage(cpu: usize, value: u32) {
    if cpu < MAX_CPUS {
        STAGE[cpu].store(value, Ordering::SeqCst);
    }
}

pub fn note_task(cpu: usize, task: u32) {
    if cpu < MAX_CPUS {
        STAGE_TASK[cpu].store(task, Ordering::SeqCst);
    }
}

pub fn note_sched_err(cpu: usize, code: u32) {
    if cpu < MAX_CPUS {
        SCHED_ERR[cpu].store(code, Ordering::SeqCst);
    }
}

pub fn note_ap_entry(apic_slot: usize, index: u32) {
    if apic_slot < MAX_CPUS {
        AP_ENTRIES[apic_slot].fetch_add(1, Ordering::SeqCst);
        AP_LAST_INDEX[apic_slot].store(index, Ordering::SeqCst);
    }
}

pub fn note_resume(cpu: usize) {
    if cpu < MAX_CPUS {
        RESUMES[cpu].fetch_add(1, Ordering::SeqCst);
    }
}

/// 任务首次被观察到的执行 CPU（u32::MAX = 未观察）。
static EXEC_CPU: [AtomicU32; MAX_TASKS] = [const { AtomicU32::new(u32::MAX) }; MAX_TASKS];
/// 任务曾在两个 CPU 上被观察到的次数（唯一运行者反例计数）。
static EXEC_SPLIT: AtomicU32 = AtomicU32::new(0);
static RO_FRAME: [AtomicU64; MAX_TASKS] = [const { AtomicU64::new(0) }; MAX_TASKS];
static CHILD_PARENT: [AtomicU32; MAX_TASKS] = [const { AtomicU32::new(0) }; MAX_TASKS];

fn tsc() -> u64 {
    prima_ostd::arch::timestamp_counter()
}

fn cpu_now() -> usize {
    crate::cpu::local::index_from_gs_base()
}

fn elapsed(start: u64) -> u64 {
    tsc().wrapping_sub(start)
}

fn active() -> u32 {
    ACTIVE.load(Ordering::SeqCst)
}

/// 记录任务在 `cpu` 上执行；出现跨 CPU 观察即记一次唯一运行者反例。
fn observe(task: u32, cpu: usize) {
    if task as usize >= MAX_TASKS {
        return;
    }
    match EXEC_CPU[task as usize].compare_exchange(
        u32::MAX,
        cpu as u32,
        Ordering::SeqCst,
        Ordering::SeqCst,
    ) {
        Ok(_) => {}
        Err(seen) if seen == cpu as u32 => {}
        Err(_) => {
            EXEC_SPLIT.fetch_add(1, Ordering::SeqCst);
        }
    }
}

fn owned_by_current(task: u32, cpu: usize) -> bool {
    sched::owner(task) == Some(cpu)
}

/// 覆盖并返回探针 syscall 分派；真实调用号转交 [`crate::syscall::handle`]。
pub fn install_handlers() {
    entry::set_syscall_handler(probe_syscall);
    entry::set_page_fault_handler(probe_fault);
    entry::set_cpu_exception_handler(probe_cpu_exception);
}

/// 诊断：CPU 异常把向量/错误码/RIP 记入本 CPU 槽后停放（BSP 汇总打印）。
extern "C" fn probe_cpu_exception(tag: u64, block: prima_ostd::entry::SaveBlock, error: u64) -> ! {
    let cpu = crate::cpu::local::index_from_gs_base();
    let with_error = tag & 0x100 != 0;
    let cs = if with_error {
        block.cs_with_error()
    } else {
        block.cs()
    };
    let frame = if with_error {
        block.to_sret_frame_with_error()
    } else {
        block.to_sret_frame()
    };
    let packed =
        (tag & 0xff) | ((error & 0xff) << 8) | ((cs & 3) << 16) | (u64::from(with_error) << 20);
    EXC[cpu].store(packed, Ordering::SeqCst);
    EXC_RIP[cpu].store(frame.rip, Ordering::SeqCst);
    loop {
        prima_ostd::arch::hlt();
    }
}

extern "C" fn probe_syscall(frame: AbiFrame) -> u64 {
    let cpu = cpu_now();
    match frame.num() {
        MAGIC_BARRIER => barrier(cpu),
        MAGIC_WAIT_BLOCKED => wait_blocked(),
        MAGIC_WAKE_SEEN => wake_seen(cpu, frame.arg(1)),
        MAGIC_CHILD_RUN => child_run(cpu),
        MAGIC_CHILD_DONE => child_done(cpu),
        MAGIC_FORK_DONE => fork_done(cpu, frame.arg(1)),
        MAGIC_NESTED_DONE => u64::from(NESTED_OK[cpu].load(Ordering::SeqCst) != 0) ^ 1,
        MAGIC_CPU_DONE => cpu_done(cpu),
        MAGIC_HELD_RUN => held_run(cpu),
        MAGIC_HELD_WAIT => held_wait(),
        MAGIC_PREP_CHILD_RUN => prep_child_run(cpu),
        MAGIC_PREP_CHILD_DONE => child_done(cpu),
        MAGIC_PREP_DONE => prep_done(cpu, frame.arg(1)),
        MAGIC_THREAD_RUN => thread_run(cpu),
        MAGIC_THREAD_DONE => thread_done(cpu, frame.arg(1)),
        MAGIC_EXEC_BARRIER => exec_barrier(),
        MAGIC_EXEC_RUN => exec_run(cpu),
        MAGIC_EXEC_FAIL => exec_fail(cpu),
        MAGIC_LOST_ACK => lost_ack(frame.arg(0), frame.arg(1)),
        MAGIC_LOST_WAIT => lost_wait(frame.arg(0)),
        MAGIC_LOST_DONE => lost_done(cpu),
        MAGIC_REV_GATE => rev_gate(frame.arg(0)),
        MAGIC_REV_ACK => rev_ack(frame.arg(0), frame.arg(1)),
        MAGIC_REV_WAIT => rev_wait(frame.arg(0)),
        MAGIC_REV_DONE => rev_done(cpu),
        MAGIC_FORK_EXEC_CHILD_RUN => fork_exec_child_run(cpu),
        MAGIC_FORK_EXEC_DONE => fork_exec_done(cpu, frame.arg(0)),
        MAGIC_FE_FAIL => fe_fail(cpu, frame.arg(0)),
        MAGIC_POST_ACK => post_ack(frame.arg(0), frame.arg(1)),
        MAGIC_POST_WAIT => post_wait(frame.arg(0)),
        MAGIC_POST_DONE => post_done(cpu),
        _ => crate::syscall::handle(frame),
    }
}

fn barrier(cpu: usize) -> u64 {
    let task = sched::current();
    observe(task, cpu);
    let owned = owned_by_current(task, cpu);
    ARRIVED.fetch_add(1, Ordering::SeqCst);
    let start = tsc();
    while ARRIVED.load(Ordering::SeqCst) < active() {
        if elapsed(start) > BARRIER_LIMIT {
            return 1;
        }
        core::hint::spin_loop();
    }
    BARRIER_OK[cpu].store(u32::from(owned), Ordering::SeqCst);
    0
}

fn wait_blocked() -> u64 {
    let start = tsc();
    while !bootstrap::is_blocked_recv_global(2) {
        if elapsed(start) > BLOCKED_LIMIT {
            return 1;
        }
        core::hint::spin_loop();
    }
    0
}

/// `arg`：CPU0 传 SEND 返回值（0 = 成功），CPU1 传 RECV 收到的载荷。
fn wake_seen(cpu: usize, arg: u64) -> u64 {
    let task = sched::current();
    observe(task, cpu);
    let expected = if cpu == 0 { 0 } else { PAYLOAD };
    let ok = arg == expected && owned_by_current(task, cpu);
    WAKE_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

fn child_run(cpu: usize) -> u64 {
    let task = sched::current();
    observe(task, cpu);
    let owned = owned_by_current(task, cpu);
    let parent = cpu as u32 + 1;
    if (task as usize) < MAX_TASKS {
        CHILD_PARENT[task as usize].store(parent, Ordering::SeqCst);
    }
    let parent_ok = sched::owner(parent) == Some(cpu);
    let ok = task > 2 && owned && parent_ok;
    CHILD_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

fn child_done(cpu: usize) -> ! {
    let task = sched::current();
    observe(task, cpu);
    // 子任务只记录自己的结束位，由父任务在 FORK_DONE 中核对。
    let _ = task;
    park(cpu)
}

fn fork_done(cpu: usize, child: u64) -> u64 {
    FORK_RAW[cpu].store(child, Ordering::SeqCst);
    let parent = sched::current();
    observe(parent, cpu);
    let Ok(child) = u32::try_from(child) else {
        return 1;
    };
    let child_owned = sched::owner(child) == Some(cpu);
    let child_seen = CHILD_OK[cpu].load(Ordering::SeqCst) != 0;
    let parked = bootstrap::task_status(child) == Some(bootstrap::TaskStatus::BlockedRecv);
    let parent_ok = sched::owner(parent) == Some(cpu);
    let ok = child > 2
        && child_owned
        && child_seen
        && parked
        && parent_ok
        && CHILD_PARENT[child as usize].load(Ordering::SeqCst) == parent;
    FORK_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

fn held_run(cpu: usize) -> ! {
    let task = sched::current();
    observe(task, cpu);
    HELD_TASK.store(task, Ordering::SeqCst);
    HELD_OWNER.store(
        sched::owner(task).map_or(u32::MAX, |owner| owner as u32),
        Ordering::SeqCst,
    );
    HELD_RAN.store(true, Ordering::SeqCst);
    park(cpu)
}

/// CPU0 等 held 任务在其 owner CPU 上真实执行并停放。
fn held_wait() -> u64 {
    let start = tsc();
    while !HELD_RAN.load(Ordering::SeqCst) {
        if elapsed(start) > BLOCKED_LIMIT {
            return 1;
        }
        core::hint::spin_loop();
    }
    let task = HELD_TASK.load(Ordering::SeqCst);
    let owner = HELD_OWNER.load(Ordering::SeqCst);
    let ok = task > 2
        && sched::owner(task) == Some(owner as usize)
        && EXEC_CPU[task as usize].load(Ordering::SeqCst) == owner
        && bootstrap::task_status(task) == Some(bootstrap::TaskStatus::BlockedRecv);
    LAUNCH_OK.store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

fn prep_child_run(cpu: usize) -> u64 {
    let task = sched::current();
    observe(task, cpu);
    if (task as usize) < MAX_TASKS {
        PREP_CHILD_RAN[task as usize].store(1, Ordering::SeqCst);
    }
    0
}

fn prep_done(cpu: usize, child: u64) -> u64 {
    let parent = sched::current();
    observe(parent, cpu);
    let Ok(child) = u32::try_from(child) else {
        return 1;
    };
    if child as usize >= MAX_TASKS {
        return 1;
    }
    let ran = PREP_CHILD_RAN[child as usize].load(Ordering::SeqCst) != 0;
    let parked = bootstrap::task_status(child) == Some(bootstrap::TaskStatus::BlockedRecv);
    let ok = child > 2
        && ran
        && parked
        && sched::owner(child) == Some(cpu)
        && sched::owner(parent) == Some(cpu)
        && EXEC_CPU[child as usize].load(Ordering::SeqCst) == cpu as u32;
    PREP_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

fn thread_run(cpu: usize) -> u64 {
    let task = sched::current();
    observe(task, cpu);
    if (task as usize) < MAX_TASKS {
        THREAD_RAN[task as usize].store(1, Ordering::SeqCst);
    }
    0
}

fn thread_done(cpu: usize, thread: u64) -> u64 {
    let parent = sched::current();
    observe(parent, cpu);
    let Ok(thread) = u32::try_from(thread) else {
        return 1;
    };
    if thread as usize >= MAX_TASKS {
        return 1;
    }
    let ran = THREAD_RAN[thread as usize].load(Ordering::SeqCst) != 0;
    let gone = bootstrap::task_status(thread).is_none_or(|s| s == bootstrap::TaskStatus::Dead);
    let single = bootstrap::process_of(parent)
        .is_none_or(|process| bootstrap::process_threads(process) == 1);
    let ok = thread > 2
        && ran
        && gone
        && single
        && sched::owner(thread).is_none()
        && sched::owner(parent) == Some(cpu);
    THREAD_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

/// 双核同时进入 exec 的屏障；返回后双方各自调用 `SYS_EXEC_REPLACE`。
fn exec_barrier() -> u64 {
    let task = sched::current();
    if (task as usize) < MAX_TASKS {
        PRE_EXEC_ROOT[task as usize].store(sched::current_root().unwrap_or(0), Ordering::SeqCst);
    }
    EXEC_ARRIVED.fetch_add(1, Ordering::SeqCst);
    let start = tsc();
    while EXEC_ARRIVED.load(Ordering::SeqCst) < active() {
        if elapsed(start) > BARRIER_LIMIT {
            return 1;
        }
        core::hint::spin_loop();
    }
    0
}

/// exec 后的新映像入口：确认已换根、owner 未变、旧用户映射已消失；返回本 CPU 供分支。
fn exec_run(cpu: usize) -> u64 {
    let task = sched::current();
    observe(task, cpu);
    let before = PRE_EXEC_ROOT[task as usize].load(Ordering::SeqCst);
    let after = sched::current_root().unwrap_or(0);
    let (_, offset) = crate::cow::root_and_offset().unwrap_or((0, PHYS.load(Ordering::SeqCst)));
    let old_code_gone =
        vspace::read_user_leaf(after, U_CODE_VA, |p| phys_to_virt(p, offset)).is_err();
    let ok = before != 0
        && after != 0
        && after != before
        && old_code_gone
        && sched::owner(task) == Some(cpu);
    EXEC_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    cpu as u64
}

fn exec_fail(cpu: usize) -> ! {
    EXEC_OK[cpu].store(0, Ordering::SeqCst);
    cpu_done(cpu)
}

fn lost_ack(round: u64, ok: u64) -> u64 {
    if let Some(slot) = LOST_ACK.get(round as usize) {
        // 用户侧 `sete` 置 1 表示载荷相符。
        slot.store(if ok != 0 { 1 } else { 2 }, Ordering::SeqCst);
    }
    0
}

/// CPU0 每轮发送前确认上一轮已被接收方确认。
fn lost_wait(round: u64) -> u64 {
    let start = tsc();
    loop {
        match LOST_ACK
            .get(round as usize)
            .map(|slot| slot.load(Ordering::SeqCst))
        {
            Some(1) => return 0,
            Some(2) => return 1,
            Some(_) => {
                if elapsed(start) > BLOCKED_LIMIT {
                    return 1;
                }
                core::hint::spin_loop();
            }
            None => return 1,
        }
    }
}

fn lost_done(cpu: usize) -> u64 {
    let ok = LOST_ACK.iter().all(|slot| slot.load(Ordering::SeqCst) == 1);
    LOST_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

/// 反向发送方每轮先等接收方真正阻塞，避免消息落 pending（那不是唤醒）。
fn rev_gate(task: u64) -> u64 {
    let start = tsc();
    while !bootstrap::is_blocked_recv_global(task as u32) {
        if elapsed(start) > BLOCKED_LIMIT {
            REV_FAIL.store(true, Ordering::SeqCst);
            return 1;
        }
        core::hint::spin_loop();
    }
    0
}

fn rev_ack(round: u64, ok: u64) -> u64 {
    if let Some(slot) = REV_ACK.get(round as usize) {
        slot.store(if ok != 0 { 1 } else { 2 }, Ordering::SeqCst);
    }
    0
}

fn rev_wait(round: u64) -> u64 {
    let start = tsc();
    loop {
        match REV_ACK
            .get(round as usize)
            .map(|slot| slot.load(Ordering::SeqCst))
        {
            Some(1) => return 0,
            Some(2) => return 1,
            Some(_) => {
                if elapsed(start) > BLOCKED_LIMIT {
                    REV_FAIL.store(true, Ordering::SeqCst);
                    return 1;
                }
                core::hint::spin_loop();
            }
            None => return 1,
        }
    }
}

fn rev_done(cpu: usize) -> u64 {
    let ok = !REV_FAIL.load(Ordering::SeqCst)
        && REV_ACK.iter().all(|slot| slot.load(Ordering::SeqCst) == 1);
    REV_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

/// 子进程 exec 后的第二映像入口：记录已运行并停放，供父进程核对。
fn fork_exec_child_run(cpu: usize) -> ! {
    let task = sched::current();
    observe(task, cpu);
    if (task as usize) < MAX_TASKS {
        FORK_EXEC_RAN[task as usize].store(1, Ordering::SeqCst);
    }
    park(cpu)
}

/// 父进程核对子进程已 exec 到第二映像：旧父映像消失、新映像存在、随父 CPU 停放。
fn fork_exec_done(cpu: usize, child: u64) -> u64 {
    let parent = sched::current();
    observe(parent, cpu);
    let Ok(child) = u32::try_from(child) else {
        FE_CODE[cpu].store(1, Ordering::SeqCst);
        return 1;
    };
    if child as usize >= MAX_TASKS {
        FE_CODE[cpu].store(1, Ordering::SeqCst);
        return 1;
    }
    let ran = FORK_EXEC_RAN[child as usize].load(Ordering::SeqCst) != 0;
    let parked = bootstrap::task_status(child) == Some(bootstrap::TaskStatus::BlockedRecv);
    let root = sched::root(child).unwrap_or(0);
    let (_, offset) = crate::cow::root_and_offset().unwrap_or((0, PHYS.load(Ordering::SeqCst)));
    let old_gone = root != 0
        && vspace::read_user_leaf(root, EXEC_IMAGE_VA, |p| phys_to_virt(p, offset)).is_err();
    let new_present = root != 0
        && vspace::read_user_leaf(root, EXEC_CHILD_VA, |p| phys_to_virt(p, offset)).is_ok();
    let mut code = 0u32;
    if !(child > 2) {
        code |= 1;
    }
    if !ran {
        code |= 2;
    }
    if !parked {
        code |= 4;
    }
    if !old_gone {
        code |= 8;
    }
    if !new_present {
        code |= 16;
    }
    if sched::owner(child) != Some(cpu) {
        code |= 32;
    }
    if sched::owner(parent) != Some(cpu) {
        code |= 64;
    }
    if FE_CODE[cpu].load(Ordering::SeqCst) & 0x8000_0000 == 0 {
        FE_CODE[cpu].store(code, Ordering::SeqCst);
    }
    let ok = code == 0;
    FORK_EXEC_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

/// 子进程 exec 失败：标记失败并停放（不提前置 CPU DONE）。
fn fe_fail(cpu: usize, errno: u64) -> ! {
    FE_CODE[cpu].store(0x8000_0000 | errno as u32, Ordering::SeqCst);
    FORK_EXEC_OK[cpu].store(0, Ordering::SeqCst);
    park(cpu)
}

fn post_ack(round: u64, ok: u64) -> u64 {
    if let Some(slot) = POST_ACK.get(round as usize) {
        slot.store(if ok != 0 { 1 } else { 2 }, Ordering::SeqCst);
    }
    0
}

fn post_wait(round: u64) -> u64 {
    let start = tsc();
    loop {
        match POST_ACK
            .get(round as usize)
            .map(|slot| slot.load(Ordering::SeqCst))
        {
            Some(1) => return 0,
            Some(2) => return 1,
            Some(_) => {
                if elapsed(start) > BLOCKED_LIMIT {
                    POST_FAIL.store(true, Ordering::SeqCst);
                    return 1;
                }
                core::hint::spin_loop();
            }
            None => return 1,
        }
    }
}

fn post_done(cpu: usize) -> u64 {
    let ok = !POST_FAIL.load(Ordering::SeqCst)
        && POST_ACK.iter().all(|slot| slot.load(Ordering::SeqCst) == 1);
    POST_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    u64::from(!ok)
}

fn cpu_done(cpu: usize) -> ! {
    let task = sched::current();
    observe(task, cpu);
    DONE.fetch_or(1u32 << cpu, Ordering::SeqCst);
    park(cpu)
}

fn park(cpu: usize) -> ! {
    let task = sched::current();
    if bootstrap::probe_block_recv_global(task).is_err() {
        PARK_FAILED.fetch_add(1, Ordering::SeqCst);
    }
    sched::park_current(cpu)
}

/// 用户 #PF 处理：本探针的只读页 → 先嵌套触发 CPL0 故障，再换私有可写映射并重试。
extern "C" fn probe_fault(frame: &SretFrame) -> u64 {
    let cpu = cpu_now();
    if frame.cs & 3 == 3 {
        let cr2 = entry::percpu_fault_addr(cpu);
        let err = entry::percpu_fault_error(cpu);
        if err & 0x2 != 0 && (cr2 & !0xfff) == U_RO_VA {
            let outer_user = entry::percpu_user_frame(cpu);
            let outer_frame = entry::percpu_fault_frame(cpu);
            let task = sched::current();
            let nested = NESTED_VA + cpu as u64 * FRAME_SIZE;
            entry::trigger_page_fault(nested);
            let inner_addr = entry::percpu_fault_addr(cpu);
            let inner_err = entry::percpu_fault_error(cpu);
            let after_user = entry::percpu_user_frame(cpu);
            let user_intact = after_user.rip == outer_user.rip
                && after_user.rsp == outer_user.rsp
                && after_user.cs == outer_user.cs
                && after_user.ss == outer_user.ss;
            let mapped = map_private_ro(task);
            let ok = inner_addr == nested && inner_err & 0x1 == 0 && user_intact && mapped;
            NESTED_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
            if ok {
                return outer_user.rip;
            }
            return crate::cow::dispatch(&outer_frame);
        }
    }
    crate::cow::dispatch(frame)
}

/// 把本任务只读页复制到新帧并以可写映射替换（仅当前任务根）。
fn map_private_ro(task: u32) -> bool {
    let Some(root) = sched::current_root() else {
        return false;
    };
    let offset = PHYS.load(Ordering::SeqCst);
    let source = RO_FRAME[task as usize].load(Ordering::SeqCst);
    if source == 0 {
        return false;
    }
    let Ok(copy) = frames::allocate_table_frame() else {
        return false;
    };
    crate::mem::zero_frame(copy, offset);
    for index in 0..(FRAME_SIZE / 8) {
        let word = prima_ostd::mm::read_u64(phys_to_virt(source, offset) + index * 8);
        prima_ostd::mm::write_u64(phys_to_virt(copy, offset) + index * 8, word);
    }
    let map = |phys: u64| phys_to_virt(phys, offset);
    if vspace::map_4k(
        root,
        U_RO_VA,
        map,
        || None,
        copy,
        true,
        true,
        |frame| {
            let _ = frames::release_table_frame(frame);
        },
    )
    .is_err()
    {
        let _ = frames::release_table_frame(copy);
        return false;
    }
    if vspace::set_executable(root, U_RO_VA, false, map).is_err() {
        return false;
    }
    prima_ostd::arch::invlpg(U_RO_VA);
    true
}

// ---------------------------------------------------------------------------
// 用户机器码
// ---------------------------------------------------------------------------

const BUF_LEN: usize = 1024;

struct Asm {
    buf: [u8; BUF_LEN],
    at: usize,
}

impl Asm {
    fn new() -> Self {
        Self {
            buf: [0; BUF_LEN],
            at: 0,
        }
    }

    fn emit(&mut self, bytes: &[u8]) {
        self.buf[self.at..self.at + bytes.len()].copy_from_slice(bytes);
        self.at += bytes.len();
    }

    fn imm64(&mut self, prefix: [u8; 2], value: u64) {
        self.emit(&prefix);
        self.emit(&value.to_le_bytes());
    }

    fn mov_rax(&mut self, value: u64) {
        self.imm64([0x48, 0xB8], value);
    }
    fn mov_rdi(&mut self, value: u64) -> usize {
        self.imm64([0x48, 0xBF], value);
        self.at - 8
    }
    fn mov_rsi(&mut self, value: u64) {
        self.imm64([0x48, 0xBE], value);
    }
    fn mov_rdx(&mut self, value: u64) {
        self.imm64([0x48, 0xBA], value);
    }
    fn mov_r12_rax(&mut self) {
        self.emit(&[0x49, 0x89, 0xC4]);
    }
    fn mov_rsi_rax(&mut self) {
        self.emit(&[0x48, 0x89, 0xC6]);
    }
    fn mov_rdi_r12(&mut self) {
        self.emit(&[0x4C, 0x89, 0xE7]);
    }
    fn mov_rsi_r12(&mut self) {
        self.emit(&[0x4C, 0x89, 0xE6]);
    }
    fn xor_rdi(&mut self) {
        self.emit(&[0x48, 0x31, 0xFF]);
    }
    fn xor_rsi(&mut self) {
        self.emit(&[0x48, 0x31, 0xF6]);
    }
    fn xor_rdx(&mut self) {
        self.emit(&[0x48, 0x31, 0xD2]);
    }
    fn xor_r10_r8_r9(&mut self) {
        self.emit(&[0x4D, 0x31, 0xD2, 0x4D, 0x31, 0xC0, 0x4D, 0x31, 0xC9]);
    }
    fn xor_r10(&mut self) {
        self.emit(&[0x4D, 0x31, 0xD2]);
    }
    fn int80(&mut self) {
        self.emit(&[0xCD, 0x80]);
    }
    fn ud2(&mut self) {
        self.emit(&[0x0F, 0x0B]);
    }
    /// `rax = base`，再把 `[rax+disp]` 读入 `rdi`。
    fn load_rdi_at(&mut self, base: u64, disp: u32) {
        self.mov_rax(base);
        self.emit(&[0x48, 0x8B, 0xB8]);
        self.emit(&disp.to_le_bytes());
    }
    /// `rax = base`，再把 `[rax+disp]` 读入 `rsi`。
    fn load_rsi_at(&mut self, base: u64, disp: u32) {
        self.mov_rax(base);
        self.emit(&[0x48, 0x8B, 0xB0]);
        self.emit(&disp.to_le_bytes());
    }
    fn jmp(&mut self) -> usize {
        self.emit(&[0xE9, 0, 0, 0, 0]);
        self.at - 4
    }
    fn jz(&mut self) -> usize {
        self.emit(&[0x0F, 0x84, 0, 0, 0, 0]);
        self.at - 4
    }
    fn jne(&mut self) -> usize {
        self.emit(&[0x0F, 0x85, 0, 0, 0, 0]);
        self.at - 4
    }
    fn jb(&mut self) -> usize {
        self.emit(&[0x0F, 0x82, 0, 0, 0, 0]);
        self.at - 4
    }
    fn patch(&mut self, at: usize, target: usize) {
        let rel = (target as i64 - (at + 4) as i64) as i32;
        self.buf[at..at + 4].copy_from_slice(&rel.to_le_bytes());
    }
    fn patch_imm(&mut self, at: usize, value: u64) {
        self.buf[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
}

/// 用户程序（两任务同一份；`rbx` 保存初始 CPU 编码，exec 改写后仍保留）。
fn user_code() -> [u8; BUF_LEN] {
    let mut a = Asm::new();
    // rbx = cpu
    a.emit(&[0x48, 0x89, 0xFB]);
    a.mov_rax(MAGIC_BARRIER);
    a.emit(&[0x48, 0x89, 0xDF]); // mov rdi, rbx
    a.int80();
    a.emit(&[0x48, 0x83, 0xFB, 0x00]); // cmp rbx, 0
    let to_blocker = a.jne();

    // ---- CPU0：等待对端阻塞后发送 ----
    a.mov_rax(MAGIC_WAIT_BLOCKED);
    a.mov_rdi(2);
    a.int80();
    a.mov_rax(crate::syscall::SYS_SEND);
    a.mov_rdi(2);
    a.mov_rsi(bootstrap::TASK2_SEND_CAP_HANDLE.encode());
    a.mov_rdx(PAYLOAD);
    a.xor_r10_r8_r9();
    a.int80();
    a.mov_r12_rax();
    a.mov_rax(MAGIC_WAKE_SEEN);
    a.mov_rdi(0);
    a.mov_rsi_r12();
    a.int80();
    let to_fork = a.jmp();

    // ---- CPU1：阻塞接收 ----
    let blocker = a.at;
    a.mov_rax(crate::syscall::SYS_RECV);
    a.mov_rdi(U_DATA_VA + RECV_OUT_OFF);
    a.xor_rsi();
    a.int80();
    a.mov_rsi_rax(); // 载荷
    a.mov_rax(MAGIC_WAKE_SEEN);
    a.mov_rdi(1);
    a.int80();

    // ---- 双方：真实 fork（r13=0 标记普通子进程） ----
    let after_wake = a.at;
    a.patch(to_fork, after_wake);
    a.emit(&[0x4D, 0x31, 0xED]); // xor r13, r13
    a.mov_rax(crate::syscall::SYS_FORK);
    a.xor_rdi();
    a.int80();
    a.emit(&[0x48, 0x85, 0xC0]); // test rax, rax
    let to_child = a.jz();
    a.mov_r12_rax(); // 子 id
    a.emit(&[0x49, 0x89, 0xC7]); // mov r15, rax：保留到 exec 后核对子进程
    a.mov_rax(crate::syscall::SYS_YIELD);
    a.int80();
    a.mov_rax(MAGIC_FORK_DONE);
    a.mov_rdi(0);
    a.mov_rsi_r12();
    a.int80();

    // ---- 父：写只读页 → 嵌套故障 → 重试成功后继续 ----
    a.mov_rax(U_RO_VA);
    a.mov_rdx(0x5A5A_5A5A_5A5A_5A5A);
    a.emit(&[0x48, 0x89, 0x10]); // mov [rax], rdx
    a.mov_rax(MAGIC_NESTED_DONE);
    a.mov_rdi(0);
    a.int80();

    // ---- held-launch：只有 CPU0 持有管理器凭据 ----
    a.emit(&[0x48, 0x83, 0xFB, 0x00]); // cmp rbx, 0
    let to_skip_held = a.jne();
    a.load_rdi_at(U_DATA_VA, HELD_CAP_OFF as u32);
    a.xor_rsi();
    a.xor_rdx();
    a.xor_r10();
    a.mov_rax(crate::syscall::SYS_TASK_START);
    a.int80();
    a.mov_rax(crate::syscall::SYS_YIELD);
    a.int80();
    a.mov_rax(MAGIC_HELD_WAIT);
    a.xor_rdi();
    a.int80();
    let after_held = a.at;
    a.patch(to_skip_held, after_held);

    // ---- prepared fork：r13=1 标记（寄存器随保存帧复制，不触发 COW 写） ----
    a.emit(&[0x49, 0xC7, 0xC5, 0x01, 0x00, 0x00, 0x00]); // mov r13, 1
    a.mov_rax(crate::syscall::SYS_FORK_TRANSACTION);
    a.xor_rdi();
    a.xor_rsi();
    a.int80();
    a.emit(&[0x48, 0x85, 0xC0]); // test rax, rax
    let to_child_prep = a.jz();
    a.mov_r12_rax(); // ticket
    a.mov_rax(crate::syscall::SYS_FORK_TRANSACTION);
    a.mov_rdi(1);
    a.mov_rsi_r12();
    a.int80();
    a.mov_r12_rax(); // 子 id
    a.mov_rax(crate::syscall::SYS_YIELD);
    a.int80();
    a.mov_rax(MAGIC_PREP_DONE);
    a.mov_rdi(0);
    a.mov_rsi_r12();
    a.int80();

    // ---- 线程创建：随调用者 CPU 发布，让出给线程 ----
    let thread_entry_imm = a.mov_rdi(0); // 生成末尾回填线程入口 VA
    a.mov_rsi(U_DATA_VA + 0xF00);
    a.xor_rdx();
    a.mov_rax(crate::syscall::SYS_THREAD_CREATE);
    a.int80();
    a.mov_r12_rax();
    a.mov_rax(crate::syscall::SYS_YIELD);
    a.int80();
    a.mov_rax(MAGIC_THREAD_DONE);
    a.mov_rdi(0);
    a.mov_rsi_r12();
    a.int80();

    // ---- 双核同时 exec：屏障后各自换根 ----
    a.mov_rax(MAGIC_EXEC_BARRIER);
    a.xor_rdi();
    a.int80();
    a.load_rsi_at(U_DATA_VA, IMAGE_LEN_OFF as u32); // rsi = 映像长度
    a.mov_rdi(U_DATA_VA + ELF_IMAGE_OFF);
    a.mov_rax(crate::syscall::SYS_EXEC_REPLACE);
    a.xor_rdx();
    a.xor_r10_r8_r9();
    a.int80();
    // exec 失败时仍上报并收尾（见证不通过）。
    a.mov_rax(MAGIC_EXEC_FAIL);
    a.xor_rdi();
    a.int80();
    a.ud2();

    // ---- 普通 fork 子进程：报告运行后 exec 数据页中的第二映像并停放 ----
    let child = a.at;
    a.patch(to_child, child);
    a.patch(to_child_prep, child);
    a.emit(&[0x4D, 0x85, 0xED]); // test r13, r13
    let to_prep_child = a.jne();
    a.mov_rax(MAGIC_CHILD_RUN);
    a.xor_rdi();
    a.int80();
    a.load_rsi_at(U_DATA_VA, CHILD_ELF_DATA_OFF + 96); // 子 ELF p_filesz
    a.mov_rdi(U_DATA_VA + CHILD_ELF_DATA_OFF as u64);
    a.mov_rax(crate::syscall::SYS_EXEC_REPLACE);
    a.xor_rdx();
    a.xor_r10_r8_r9();
    a.int80();
    a.emit(&[0x48, 0x89, 0xC7]); // mov rdi, rax（保留 exec 错误码）
    a.mov_rax(MAGIC_FE_FAIL);
    a.int80();
    a.ud2();

    // ---- prepared fork 子进程：报告后停放 ----
    let prep_child = a.at;
    a.patch(to_prep_child, prep_child);
    a.mov_rax(MAGIC_PREP_CHILD_RUN);
    a.xor_rdi();
    a.int80();
    a.mov_rax(MAGIC_PREP_CHILD_DONE);
    a.xor_rdi();
    a.int80();
    a.ud2();

    // ---- 线程入口：报告后自毁（父任务此前已让出） ----
    let thread = a.at;
    a.patch_imm(thread_entry_imm, U_CODE_VA + thread as u64);
    a.mov_rax(MAGIC_THREAD_RUN);
    a.xor_rdi();
    a.int80();
    a.mov_rax(crate::syscall::SYS_THREAD_EXIT);
    a.xor_rdi();
    a.int80();
    a.ud2();

    a.patch(to_blocker, blocker);
    a.buf
}

/// held-launch 任务的最小用户程序：报告后停放。
fn held_code() -> [u8; 32] {
    let mut a = Asm::new();
    a.mov_rax(MAGIC_HELD_RUN);
    a.xor_rdi();
    a.int80();
    a.ud2();
    let mut out = [0u8; 32];
    out.copy_from_slice(&a.buf[..32]);
    out
}

/// exec 后新映像的机器码：正向/反向丢唤醒、核对 pre-exec fork 子进程的第二映像 exec，
/// 最后做 fork/exec 后的双向唤醒。
fn exec_code() -> ([u8; BUF_LEN], usize) {
    let mut a = Asm::new();
    a.mov_rax(MAGIC_EXEC_RUN);
    a.xor_rdi();
    a.int80();
    a.emit(&[0x48, 0x83, 0xF8, 0x00]); // cmp rax, 0
    let to_recv = a.jne();

    // CPU0：每轮等确认后再发送，共 LOST_ROUNDS 轮。
    a.emit(&[0x4D, 0x31, 0xE4]); // xor r12, r12
    let send_loop = a.at;
    a.mov_rax(crate::syscall::SYS_SEND);
    a.mov_rdi(2);
    a.mov_rsi(bootstrap::TASK2_SEND_CAP_HANDLE.encode());
    a.mov_rdx(PAYLOAD);
    a.emit(&[0x4C, 0x01, 0xE2]); // add rdx, r12
    a.xor_r10_r8_r9();
    a.int80();
    // 发送后等本轮确认，再进入下一轮（轮 0 之前无需等待）。
    a.mov_rax(MAGIC_LOST_WAIT);
    a.mov_rdi_r12();
    a.int80();
    a.emit(&[0x49, 0xFF, 0xC4]); // inc r12
    a.emit(&[0x49, 0x83, 0xFC, LOST_ROUNDS as u8]); // cmp r12, LOST_ROUNDS
    let send_back = a.jb();
    let to_done = a.jmp();

    // CPU1：每轮阻塞接收、核对载荷并确认。
    let recv = a.at;
    a.patch(to_recv, recv);
    a.emit(&[0x4D, 0x31, 0xE4]); // xor r12, r12
    let recv_loop = a.at;
    a.mov_rax(crate::syscall::SYS_RECV);
    a.emit(&[0x48, 0x8D, 0x7C, 0x24, 0xF0]); // lea rdi, [rsp-16]
    a.xor_rsi();
    a.int80();
    a.mov_rdx(PAYLOAD);
    a.emit(&[0x4C, 0x01, 0xE2]); // add rdx, r12（期望载荷）
    a.emit(&[0x31, 0xF6]); // xor esi, esi
    a.emit(&[0x48, 0x39, 0xD0]); // cmp rax, rdx
    a.emit(&[0x40, 0x0F, 0x94, 0xC6]); // sete sil
    a.mov_rax(MAGIC_LOST_ACK);
    a.mov_rdi_r12();
    a.int80();
    a.emit(&[0x49, 0xFF, 0xC4]); // inc r12
    a.emit(&[0x49, 0x83, 0xFC, LOST_ROUNDS as u8]);
    let recv_back = a.jb();

    let done = a.at;
    a.patch(to_done, done);
    a.patch(send_back, send_loop);
    a.patch(recv_back, recv_loop);
    a.mov_rax(MAGIC_LOST_DONE);
    a.xor_rdi();
    a.int80();

    // ---- 反向矩阵：换 CPU1 发送、CPU0 阻塞接收（仍共 LOST_ROUNDS 轮） ----
    // rbx 在 exec 前后由保存帧保留（本任务初始 CPU 编码）。
    a.emit(&[0x48, 0x83, 0xFB, 0x00]); // cmp rbx, 0
    let to_rev_send = a.jne();

    // CPU0：每轮阻塞接收、核对载荷并确认。
    a.emit(&[0x4D, 0x31, 0xE4]); // xor r12, r12
    let rev_recv_loop = a.at;
    a.mov_rax(crate::syscall::SYS_RECV);
    a.emit(&[0x48, 0x8D, 0x7C, 0x24, 0xF0]); // lea rdi, [rsp-16]
    a.xor_rsi();
    a.int80();
    a.mov_rdx(PAYLOAD);
    a.emit(&[0x4C, 0x01, 0xE2]); // add rdx, r12
    a.emit(&[0x31, 0xF6]); // xor esi, esi
    a.emit(&[0x48, 0x39, 0xD0]); // cmp rax, rdx
    a.emit(&[0x40, 0x0F, 0x94, 0xC6]); // sete sil
    a.mov_rax(MAGIC_REV_ACK);
    a.mov_rdi_r12();
    a.int80();
    a.emit(&[0x49, 0xFF, 0xC4]); // inc r12
    a.emit(&[0x49, 0x83, 0xFC, LOST_ROUNDS as u8]);
    let rev_recv_back = a.jb();
    let to_rev_done = a.jmp();

    // CPU1：每轮等 CPU0 真正阻塞后发送，再等确认。
    let rev_send = a.at;
    a.patch(to_rev_send, rev_send);
    a.emit(&[0x4D, 0x31, 0xE4]); // xor r12, r12
    let rev_send_loop = a.at;
    a.mov_rax(MAGIC_REV_GATE);
    a.mov_rdi(1);
    a.int80();
    a.mov_rax(crate::syscall::SYS_SEND);
    a.mov_rdi(1);
    a.mov_rsi(bootstrap::PROBE_TASK1_SEND_CAP_HANDLE.encode());
    a.mov_rdx(PAYLOAD);
    a.emit(&[0x4C, 0x01, 0xE2]); // add rdx, r12
    a.xor_r10_r8_r9();
    a.int80();
    a.mov_rax(MAGIC_REV_WAIT);
    a.mov_rdi_r12();
    a.int80();
    a.emit(&[0x49, 0xFF, 0xC4]); // inc r12
    a.emit(&[0x49, 0x83, 0xFC, LOST_ROUNDS as u8]);
    let rev_send_back = a.jb();

    let rev_done = a.at;
    a.patch(to_rev_done, rev_done);
    a.patch(rev_recv_back, rev_recv_loop);
    a.patch(rev_send_back, rev_send_loop);
    a.mov_rax(MAGIC_REV_DONE);
    a.xor_rdi();
    a.int80();

    // ---- 核对 pre-exec fork 子进程已 exec 第二映像并停放 ----
    // 子 id 在 fork 后存入 r15，exec 只重置 rax/rdi，故此处仍有效。
    a.mov_rax(MAGIC_FORK_EXEC_DONE);
    a.emit(&[0x4C, 0x89, 0xFF]); // mov rdi, r15
    a.int80();

    // ---- fork/exec 后双向丢唤醒：CPU0→CPU1，再 CPU1→CPU0 ----
    a.emit(&[0x48, 0x83, 0xFB, 0x00]); // cmp rbx, 0
    let to_post_cpu1 = a.jne();

    // CPU0：先发送，再阻塞接收反向。
    a.mov_rax(MAGIC_WAIT_BLOCKED);
    a.mov_rdi(2);
    a.int80();
    a.mov_rax(crate::syscall::SYS_SEND);
    a.mov_rdi(2);
    a.mov_rsi(bootstrap::TASK2_SEND_CAP_HANDLE.encode());
    a.mov_rdx(PAYLOAD + 0x10);
    a.xor_r10_r8_r9();
    a.int80();
    a.mov_rax(MAGIC_POST_WAIT);
    a.xor_rdi();
    a.int80();
    a.mov_rax(crate::syscall::SYS_RECV);
    a.emit(&[0x48, 0x8D, 0x7C, 0x24, 0xF0]); // lea rdi, [rsp-16]
    a.xor_rsi();
    a.int80();
    a.mov_rdx(PAYLOAD + 0x11);
    a.emit(&[0x31, 0xF6]); // xor esi, esi
    a.emit(&[0x48, 0x39, 0xD0]); // cmp rax, rdx
    a.emit(&[0x40, 0x0F, 0x94, 0xC6]); // sete sil
    a.mov_rax(MAGIC_POST_ACK);
    a.mov_rdi(1);
    a.int80();
    let to_post_done = a.jmp();

    // CPU1：先阻塞接收，再发送反向。
    let post_cpu1 = a.at;
    a.patch(to_post_cpu1, post_cpu1);
    a.mov_rax(crate::syscall::SYS_RECV);
    a.emit(&[0x48, 0x8D, 0x7C, 0x24, 0xF0]); // lea rdi, [rsp-16]
    a.xor_rsi();
    a.int80();
    a.mov_rdx(PAYLOAD + 0x10);
    a.emit(&[0x31, 0xF6]); // xor esi, esi
    a.emit(&[0x48, 0x39, 0xD0]); // cmp rax, rdx
    a.emit(&[0x40, 0x0F, 0x94, 0xC6]); // sete sil
    a.mov_rax(MAGIC_POST_ACK);
    a.xor_rdi();
    a.int80();
    a.mov_rax(MAGIC_REV_GATE);
    a.mov_rdi(1);
    a.int80();
    a.mov_rax(crate::syscall::SYS_SEND);
    a.mov_rdi(1);
    a.mov_rsi(bootstrap::PROBE_TASK1_SEND_CAP_HANDLE.encode());
    a.mov_rdx(PAYLOAD + 0x11);
    a.xor_r10_r8_r9();
    a.int80();
    a.mov_rax(MAGIC_POST_WAIT);
    a.mov_rdi(1);
    a.int80();

    let post_done = a.at;
    a.patch(to_post_done, post_done);
    a.mov_rax(MAGIC_POST_DONE);
    a.xor_rdi();
    a.int80();

    a.mov_rax(MAGIC_CPU_DONE);
    a.xor_rdi();
    a.int80();
    a.ud2();
    let len = a.at;
    (a.buf, len)
}

/// 子进程 exec 的第二映像机器码：报告已换根并停放。
fn child_exec_code() -> ([u8; BUF_LEN], usize) {
    let mut a = Asm::new();
    a.mov_rax(MAGIC_FORK_EXEC_CHILD_RUN);
    a.xor_rdi();
    a.int80();
    a.ud2();
    let len = a.at;
    (a.buf, len)
}

/// 第二映像：单段 RX ELF，映射到与父映像不同的根槽（`EXEC_CHILD_VA`）。
fn child_exec_elf() -> ([u8; 512], usize) {
    const OFF: usize = 64 + 56;
    let mut image = [0u8; 512];
    let (code, code_len) = child_exec_code();
    let len = OFF + code_len;
    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2;
    image[5] = 1;
    image[6] = 1;
    image[16..18].copy_from_slice(&2u16.to_le_bytes());
    image[18..20].copy_from_slice(&62u16.to_le_bytes());
    image[20..24].copy_from_slice(&1u32.to_le_bytes());
    image[24..32].copy_from_slice(&(EXEC_CHILD_VA + OFF as u64).to_le_bytes());
    image[32..40].copy_from_slice(&64u64.to_le_bytes());
    image[52..54].copy_from_slice(&64u16.to_le_bytes());
    image[54..56].copy_from_slice(&56u16.to_le_bytes());
    image[56..58].copy_from_slice(&1u16.to_le_bytes());
    image[64..68].copy_from_slice(&1u32.to_le_bytes());
    image[68..72].copy_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
    image[72..80].copy_from_slice(&0u64.to_le_bytes());
    image[80..88].copy_from_slice(&EXEC_CHILD_VA.to_le_bytes());
    image[88..96].copy_from_slice(&EXEC_CHILD_VA.to_le_bytes());
    image[96..104].copy_from_slice(&(len as u64).to_le_bytes());
    image[104..112].copy_from_slice(&(len as u64).to_le_bytes());
    image[112..120].copy_from_slice(&4096u64.to_le_bytes());
    image[OFF..len].copy_from_slice(&code[..code_len]);
    (image, len)
}

/// 单段静态 ELF64：ELF 头/程序头/机器码同处一个 RX `PT_LOAD`（offset 0）。
fn exec_elf() -> ([u8; 2048], usize) {
    const OFF: usize = 64 + 56;
    let mut image = [0u8; 2048];
    let (code, code_len) = exec_code();
    let len = OFF + code_len;
    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2; // ELFCLASS64
    image[5] = 1; // ELFDATA2LSB
    image[6] = 1; // EV_CURRENT
    image[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    image[18..20].copy_from_slice(&62u16.to_le_bytes()); // EM_X86_64
    image[20..24].copy_from_slice(&1u32.to_le_bytes()); // e_version
    image[24..32].copy_from_slice(&(EXEC_IMAGE_VA + OFF as u64).to_le_bytes()); // e_entry
    image[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    image[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    image[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    image[56..58].copy_from_slice(&1u16.to_le_bytes()); // e_phnum
    // phdr
    image[64..68].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    image[68..72].copy_from_slice(&5u32.to_le_bytes()); // PF_R | PF_X
    image[72..80].copy_from_slice(&0u64.to_le_bytes()); // p_offset
    image[80..88].copy_from_slice(&EXEC_IMAGE_VA.to_le_bytes()); // p_vaddr
    image[88..96].copy_from_slice(&EXEC_IMAGE_VA.to_le_bytes()); // p_paddr
    image[96..104].copy_from_slice(&(len as u64).to_le_bytes()); // p_filesz
    image[104..112].copy_from_slice(&(len as u64).to_le_bytes()); // p_memsz
    image[112..120].copy_from_slice(&4096u64.to_le_bytes()); // p_align
    image[OFF..OFF + code_len].copy_from_slice(&code[..code_len]);
    (image, len)
}

// ---------------------------------------------------------------------------
// 任务/地址空间构建
// ---------------------------------------------------------------------------

fn alloc_frame() -> Option<u64> {
    frames::allocate_table_frame().ok()
}

fn build_task(task: u32, cpu: usize, offset: u64) -> Result<(), u32> {
    let source = crate::user::kernel_root().ok_or(1u32)?;
    let root = frames::allocate_table_frame().map_err(|_| 2u32)?;
    let map = |phys: u64| phys_to_virt(phys, offset);
    vspace::clone_root(source, root, map);
    vspace::zero_slot(root, U_SLOT, map);

    let code = frames::allocate_table_frame().map_err(|_| 3u32)?;
    crate::mem::zero_frame(code, offset);
    let bytes = user_code();
    prima_ostd::mm::write_bytes(phys_to_virt(code, offset), &bytes);
    vspace::map_4k(root, U_CODE_VA, map, alloc_frame, code, false, true, |_| {})
        .map_err(|_| 4u32)?;

    for (va, step) in [(U_STACK_VA, 5u32), (U_DATA_VA, 6)] {
        let frame = frames::allocate_table_frame().map_err(|_| step)?;
        crate::mem::zero_frame(frame, offset);
        vspace::map_4k(root, va, map, alloc_frame, frame, true, true, |_| {}).map_err(|_| step)?;
        vspace::set_executable(root, va, false, map).map_err(|_| step)?;
        if va == U_DATA_VA {
            DATA_FRAME[task as usize].store(frame, Ordering::SeqCst);
            let (image, len) = exec_elf();
            assert!(
                ELF_IMAGE_OFF + len as u64 <= CHILD_ELF_DATA_OFF as u64,
                "parent image overlaps child ELF"
            );
            prima_ostd::mm::write_bytes(phys_to_virt(frame, offset) + ELF_IMAGE_OFF, &image[..len]);
            prima_ostd::mm::write_u64(phys_to_virt(frame, offset) + IMAGE_LEN_OFF, len as u64);
            let (child, child_len) = child_exec_elf();
            prima_ostd::mm::write_bytes(
                phys_to_virt(frame, offset) + CHILD_ELF_DATA_OFF as u64,
                &child[..child_len],
            );
        }
    }

    let ro = frames::allocate_table_frame().map_err(|_| 7u32)?;
    crate::mem::zero_frame(ro, offset);
    vspace::map_4k(root, U_RO_VA, map, alloc_frame, ro, false, true, |_| {}).map_err(|_| 8u32)?;
    vspace::set_executable(root, U_RO_VA, false, map).map_err(|_| 9u32)?;
    RO_FRAME[task as usize].store(ro, Ordering::SeqCst);

    let kstack = crate::user::task_kstack_top(task).ok_or(10u32)?;
    sched::set_root(task, root);
    sched::set_kstack_top(task, kstack);
    let mut frame = SretFrame::ZERO;
    frame.rip = U_CODE_VA;
    frame.rsp = U_STACK_VA + FRAME_SIZE - 8;
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    frame.rflags = 0x2; // IF=0：探针不用时钟中断
    frame.rdi = cpu as u64;
    sched::set_frame(task, frame);
    Ok(())
}

fn setup_roots(offset: u64) -> Result<(), u32> {
    build_task(1, 0, offset)?;
    bootstrap::probe_ensure_process(1).map_err(|_| 11u32)?;
    if active() >= 2 {
        build_task(2, 1, offset)?;
        bootstrap::probe_ensure_process(2).map_err(|_| 12u32)?;
        // P5-3b-5：反向丢唤醒矩阵需要 task 2 持有目标 `Task(1)` 的发送凭据。
        bootstrap::probe_grant_reverse_send().map_err(|_| 22u32)?;
    }
    create_held(offset)
}

/// 用真实 `task_create_held` + `memseg_map` 建一个 Prepared 任务，管理器凭据交给 task 1；
/// 用户程序随后经 `SYS_TASK_START` 让内核做创建时放置。
fn create_held(offset: u64) -> Result<(), u32> {
    let rx = Rights::from_rights(&[Right::SegMapR, Right::SegMapX]);
    let rw = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
    let code = bootstrap::memseg_create(1, 1, rx, offset).map_err(|_| 13u32)?;
    let req = bootstrap::TaskCreateRequest {
        code_seg_cap: code,
        code_offset: 0,
        code_pages: 1,
        load_base: HELD_CODE_VA,
        entry: HELD_CODE_VA,
        stack_top: HELD_STACK_VA + FRAME_SIZE,
        parent_ns_cap: bootstrap::NAMESPACE_CAP_HANDLE,
        root_rights: Rights::from_rights(&[Right::NsResolve]),
        initial_caps: [Handle::ZERO; 2],
        name: 0,
    };
    let parent_root = sched::root(1).ok_or(14u32)?;
    let made = bootstrap::task_create_held(1, &req, parent_root, offset).map_err(|_| 15u32)?;
    let stack = bootstrap::memseg_create(1, 1, rw, offset).map_err(|_| 16u32)?;
    bootstrap::memseg_map(
        1,
        stack,
        made.manager_cap,
        0,
        HELD_STACK_VA,
        1,
        rw,
        made.root_phys,
        offset,
    )
    .map_err(|_| 17u32)?;
    let map = |phys: u64| phys_to_virt(phys, offset);
    let pte = vspace::read_user_leaf(made.root_phys, HELD_CODE_VA, map).map_err(|_| 18u32)?;
    let frame = pte & 0x000f_ffff_ffff_f000;
    prima_ostd::mm::write_bytes(phys_to_virt(frame, offset), &held_code());
    crate::user::register_created_task(
        made.task_id,
        made.root_phys,
        HELD_CODE_VA,
        HELD_STACK_VA + FRAME_SIZE,
        made.self_cap.encode(),
        [0; 2],
        0,
    )
    .map_err(|_| 19u32)?;
    let data = DATA_FRAME[1].load(Ordering::SeqCst);
    if data == 0 {
        return Err(20u32);
    }
    prima_ostd::mm::write_u64(
        phys_to_virt(data, offset) + HELD_CAP_OFF,
        made.manager_cap.encode(),
    );
    Ok(())
}

/// 记录已上线的 AP 数（main 在 `start_aps` 成功后调用）。
pub fn note_aps(aps: u32) {
    APS.store(aps, Ordering::SeqCst);
}

/// BSP 的启动入口：发布两个真实任务后进入本 CPU 调度循环（不返回）。
pub fn setup_and_enter(offset: u64) -> ! {
    PHYS.store(offset, Ordering::SeqCst);
    let online = APS.load(Ordering::SeqCst) + 1;
    ACTIVE.store(if online >= 2 { 2 } else { 1 }, Ordering::SeqCst);
    entry::set_fast_syscall_handler(crate::syscall::handle_fast);
    install_handlers();
    if let Err(step) = setup_roots(offset) {
        SETUP_STEP.store(step, Ordering::SeqCst);
        finalize()
    }
    if sched::publish_placed(1, 0).is_err() {
        SETUP_STEP.store(20, Ordering::SeqCst);
        finalize()
    }
    if active() >= 2 && sched::publish_placed(2, 1).is_err() {
        SETUP_STEP.store(21, Ordering::SeqCst);
        finalize()
    }
    entry::enable_gs_swap();
    START.store(tsc(), Ordering::SeqCst);
    GO.store(true, Ordering::SeqCst);
    sched::idle_for(crate::cpu::local::BSP)
}

/// AP 的调度入口：等待 BSP 发布后只消费本 CPU 队列（不返回）。
pub fn ap_enter(cpu: usize) -> ! {
    note_stage(cpu, 1);
    // 本片只有 CPU0/CPU1 承载负载；其余 AP 在线后立即停放，不参与调度。
    if cpu >= 2 {
        loop {
            prima_ostd::arch::hlt();
        }
    }
    while !GO.load(Ordering::SeqCst) {
        SPIN[cpu].fetch_add(1, Ordering::SeqCst);
        core::hint::spin_loop();
    }
    note_stage(cpu, 2);
    sched::idle_for(cpu)
}

/// 全部活动 CPU 的根任务都已完成（由 `DONE` 位图判定）。
pub fn complete() -> bool {
    let mask = (1u32 << active()) - 1;
    DONE.load(Ordering::SeqCst) & mask == mask
}

/// BSP 空闲循环的看门狗。
pub fn tick(cpu: usize) {
    if cpu != crate::cpu::local::BSP {
        return;
    }
    let start = START.load(Ordering::SeqCst);
    if start != 0 && elapsed(start) > WATCHDOG_LIMIT && !complete() {
        TIMED_OUT.store(true, Ordering::SeqCst);
        finalize()
    }
}

/// 收尾是否由看门狗触发（供 BSP 输出失败原因）。
pub fn timed_out() -> bool {
    TIMED_OUT.load(Ordering::SeqCst)
}

/// 构建失败步骤码（0 = 未失败）。
pub fn setup_step() -> u32 {
    SETUP_STEP.load(Ordering::SeqCst)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchedWitness {
    pub cpus: u32,
    pub online: u32,
    pub place_ok: u32,
    pub single_ok: u32,
    pub queues_ok: u32,
    pub barrier_ok: u32,
    pub wake_ok: u32,
    pub fork_ok: u32,
    pub child_ok: u32,
    pub nested_ok: u32,
    pub launch_ok: u32,
    pub prepared_ok: u32,
    pub thread_ok: u32,
    pub exec_ok: u32,
    pub lost_ok: u32,
    pub reverse_ok: u32,
    pub forkexec_ok: u32,
    pub post_ok: u32,
}

impl SchedWitness {
    pub fn evidence_ok(&self) -> bool {
        self.cpus >= 1
            && self.online >= self.cpus
            && self.place_ok == self.cpus
            && self.single_ok == self.cpus
            && self.queues_ok == self.cpus
            && self.barrier_ok == self.cpus
            && self.wake_ok == self.cpus
            && self.fork_ok == self.cpus
            && self.child_ok == self.cpus
            && self.nested_ok == self.cpus
            && self.launch_ok == 1
            && self.prepared_ok == self.cpus
            && self.thread_ok == self.cpus
            && self.exec_ok == self.cpus
            && self.lost_ok == self.cpus
            && self.reverse_ok == self.cpus
            && self.forkexec_ok == self.cpus
            && self.post_ok == self.cpus
    }
}

pub fn witness() -> SchedWitness {
    let cpus = active();
    let (_, _, _, violations, _) = sched::queue_stats();
    let mut witness = SchedWitness {
        cpus,
        online: crate::smp::online_mask().count_ones(),
        place_ok: 0,
        single_ok: cpus.saturating_sub(violations + EXEC_SPLIT.load(Ordering::SeqCst)),
        queues_ok: 0,
        barrier_ok: 0,
        wake_ok: 0,
        fork_ok: 0,
        child_ok: 0,
        nested_ok: 0,
        launch_ok: LAUNCH_OK.load(Ordering::SeqCst),
        prepared_ok: 0,
        thread_ok: 0,
        exec_ok: 0,
        lost_ok: 0,
        reverse_ok: 0,
        forkexec_ok: 0,
        post_ok: 0,
    };
    for cpu in 0..cpus as usize {
        let task = cpu as u32 + 1;
        let placed = sched::owner(task) == Some(cpu)
            && EXEC_CPU[task as usize].load(Ordering::SeqCst) == cpu as u32;
        witness.place_ok += u32::from(placed);
        let (enq, deq) = sched::queue_cpu_stats(cpu);
        witness.queues_ok += u32::from(enq > 0 && deq > 0);
        witness.barrier_ok += BARRIER_OK[cpu].load(Ordering::SeqCst);
        witness.wake_ok += WAKE_OK[cpu].load(Ordering::SeqCst);
        witness.fork_ok += FORK_OK[cpu].load(Ordering::SeqCst);
        witness.child_ok += CHILD_OK[cpu].load(Ordering::SeqCst);
        witness.nested_ok += NESTED_OK[cpu].load(Ordering::SeqCst);
        witness.prepared_ok += PREP_OK[cpu].load(Ordering::SeqCst);
        witness.thread_ok += THREAD_OK[cpu].load(Ordering::SeqCst);
        witness.exec_ok += EXEC_OK[cpu].load(Ordering::SeqCst);
        witness.lost_ok += LOST_OK[cpu].load(Ordering::SeqCst);
        witness.reverse_ok += REV_OK[cpu].load(Ordering::SeqCst);
        witness.forkexec_ok += FORK_EXEC_OK[cpu].load(Ordering::SeqCst);
        witness.post_ok += POST_OK[cpu].load(Ordering::SeqCst);
    }
    witness
}

#[derive(Clone, Copy)]
pub struct Progress {
    pub done: u32,
    pub arrived: u32,
    pub barrier: [u32; MAX_CPUS],
    pub wake: [u32; MAX_CPUS],
    pub fork: [u32; MAX_CPUS],
    pub child: [u32; MAX_CPUS],
    pub nested: [u32; MAX_CPUS],
    pub prepared: [u32; MAX_CPUS],
    pub thread: [u32; MAX_CPUS],
    pub exec: [u32; MAX_CPUS],
    pub lost: [u32; MAX_CPUS],
    pub reverse: [u32; MAX_CPUS],
    pub forkexec: [u32; MAX_CPUS],
    pub fe_code: [u32; MAX_CPUS],
    pub post: [u32; MAX_CPUS],
    pub launch: u32,
    pub exec_arrived: u32,
    pub held_owner: u32,
    pub held_task: u32,
    pub queues: [u8; MAX_CPUS],
    pub park_failed: u32,
    pub setup_step: u32,
    pub stage: [u32; MAX_CPUS],
    pub stage_task: [u32; MAX_CPUS],
    pub sched_err: [u32; MAX_CPUS],
    pub resumes: [u32; MAX_CPUS],
    pub fork_raw: [u64; MAX_CPUS],
    pub spin: [u32; MAX_CPUS],
    pub exc: [u64; MAX_CPUS],
    pub exc_rip: [u64; MAX_CPUS],
    pub ap_entries: [u32; MAX_CPUS],
    pub ap_last_index: [u32; MAX_CPUS],
    pub go: bool,
}

pub fn progress() -> Progress {
    Progress {
        done: DONE.load(Ordering::SeqCst),
        arrived: ARRIVED.load(Ordering::SeqCst),
        barrier: core::array::from_fn(|cpu| BARRIER_OK[cpu].load(Ordering::SeqCst)),
        wake: core::array::from_fn(|cpu| WAKE_OK[cpu].load(Ordering::SeqCst)),
        fork: core::array::from_fn(|cpu| FORK_OK[cpu].load(Ordering::SeqCst)),
        child: core::array::from_fn(|cpu| CHILD_OK[cpu].load(Ordering::SeqCst)),
        nested: core::array::from_fn(|cpu| NESTED_OK[cpu].load(Ordering::SeqCst)),
        prepared: core::array::from_fn(|cpu| PREP_OK[cpu].load(Ordering::SeqCst)),
        thread: core::array::from_fn(|cpu| THREAD_OK[cpu].load(Ordering::SeqCst)),
        exec: core::array::from_fn(|cpu| EXEC_OK[cpu].load(Ordering::SeqCst)),
        lost: core::array::from_fn(|cpu| LOST_OK[cpu].load(Ordering::SeqCst)),
        reverse: core::array::from_fn(|cpu| REV_OK[cpu].load(Ordering::SeqCst)),
        forkexec: core::array::from_fn(|cpu| FORK_EXEC_OK[cpu].load(Ordering::SeqCst)),
        fe_code: core::array::from_fn(|cpu| FE_CODE[cpu].load(Ordering::SeqCst)),
        post: core::array::from_fn(|cpu| POST_OK[cpu].load(Ordering::SeqCst)),
        launch: LAUNCH_OK.load(Ordering::SeqCst),
        exec_arrived: EXEC_ARRIVED.load(Ordering::SeqCst),
        held_owner: HELD_OWNER.load(Ordering::SeqCst),
        held_task: HELD_TASK.load(Ordering::SeqCst),
        queues: sched::queue_lengths(),
        park_failed: PARK_FAILED.load(Ordering::SeqCst),
        setup_step: SETUP_STEP.load(Ordering::SeqCst),
        stage: core::array::from_fn(|cpu| STAGE[cpu].load(Ordering::SeqCst)),
        stage_task: core::array::from_fn(|cpu| STAGE_TASK[cpu].load(Ordering::SeqCst)),
        sched_err: core::array::from_fn(|cpu| SCHED_ERR[cpu].load(Ordering::SeqCst)),
        resumes: core::array::from_fn(|cpu| RESUMES[cpu].load(Ordering::SeqCst)),
        fork_raw: core::array::from_fn(|cpu| FORK_RAW[cpu].load(Ordering::SeqCst)),
        spin: core::array::from_fn(|cpu| SPIN[cpu].load(Ordering::SeqCst)),
        exc: core::array::from_fn(|cpu| EXC[cpu].load(Ordering::SeqCst)),
        exc_rip: core::array::from_fn(|cpu| EXC_RIP[cpu].load(Ordering::SeqCst)),
        ap_entries: core::array::from_fn(|cpu| AP_ENTRIES[cpu].load(Ordering::SeqCst)),
        ap_last_index: core::array::from_fn(|cpu| AP_LAST_INDEX[cpu].load(Ordering::SeqCst)),
        go: GO.load(Ordering::SeqCst),
    }
}

/// BSP 收尾：交给 `prima-ostd` 已登记的收尾回调（二进制侧负责打印与退出）。
pub fn finalize() -> ! {
    entry::finish_now(crate::cpu::local::BSP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_code_encodes_relative_branches_inside_the_buffer() {
        let code = user_code();
        assert!(code[..].iter().any(|byte| *byte == 0xCD));
        // 所有 rel32 目标都落在缓冲区内（越界即机器码生成错误）。
        let mut at = 0;
        while at + 5 < BUF_LEN {
            let rel = if code[at] == 0xE9 {
                Some(at + 1)
            } else if code[at] == 0x0F && matches!(code[at + 1], 0x84 | 0x85 | 0x82) {
                Some(at + 2)
            } else {
                None
            };
            if let Some(rel_at) = rel {
                let rel = i32::from_le_bytes(code[rel_at..rel_at + 4].try_into().unwrap());
                let target = (rel_at + 4) as i64 + rel as i64;
                assert!(
                    target >= 0 && target < BUF_LEN as i64,
                    "branch target {}",
                    target
                );
                at = rel_at + 4;
                continue;
            }
            at += 1;
        }
    }

    #[test]
    fn exec_image_is_a_single_loadable_segment_with_entry_inside() {
        let (image, len) = exec_elf();
        assert!(len < 2048);
        let plan = crate::exec::plan(&image[..len]).expect("probe ELF is loadable");
        assert_eq!(plan.count, 1);
        assert_eq!(plan.entry, EXEC_IMAGE_VA + 120);
        let part = plan.segments[0].unwrap();
        assert!(plan.entry >= part.vaddr && plan.entry < part.vaddr + part.memsz);
        assert!(part.executable && !part.writable);
    }

    #[test]
    fn child_exec_image_is_a_loadable_segment_at_its_own_root_slot() {
        let (image, len) = child_exec_elf();
        let plan = crate::exec::plan(&image[..len]).expect("child ELF is loadable");
        assert_eq!(plan.count, 1);
        assert_eq!(plan.entry, EXEC_CHILD_VA + 120);
        let part = plan.segments[0].unwrap();
        assert_eq!(part.vaddr, EXEC_CHILD_VA);
        assert!(part.executable && !part.writable);
    }

    /// 父 ELF 与子 ELF 必须落在同一数据页的不同区间。
    #[test]
    fn parent_and_child_images_fit_the_probe_data_page() {
        let (_, len) = exec_elf();
        let (_, child_len) = child_exec_elf();
        assert!(ELF_IMAGE_OFF + len as u64 <= CHILD_ELF_DATA_OFF as u64);
        assert!(CHILD_ELF_DATA_OFF as u64 + child_len as u64 <= FRAME_SIZE);
    }

    #[test]
    fn witness_requires_every_count_to_match_active_cpus() {
        let base = SchedWitness {
            cpus: 2,
            online: 2,
            place_ok: 2,
            single_ok: 2,
            queues_ok: 2,
            barrier_ok: 2,
            wake_ok: 2,
            fork_ok: 2,
            child_ok: 2,
            nested_ok: 2,
            launch_ok: 1,
            prepared_ok: 2,
            thread_ok: 2,
            exec_ok: 2,
            lost_ok: 2,
            reverse_ok: 2,
            forkexec_ok: 2,
            post_ok: 2,
        };
        assert!(base.evidence_ok());
        assert!(
            !SchedWitness {
                nested_ok: 1,
                ..base
            }
            .evidence_ok()
        );
        assert!(!SchedWitness { exec_ok: 1, ..base }.evidence_ok());
        assert!(
            !SchedWitness {
                reverse_ok: 1,
                ..base
            }
            .evidence_ok()
        );
        assert!(
            !SchedWitness {
                forkexec_ok: 1,
                ..base
            }
            .evidence_ok()
        );
        assert!(!SchedWitness { post_ok: 1, ..base }.evidence_ok());
        assert!(
            !SchedWitness {
                launch_ok: 0,
                ..base
            }
            .evidence_ok()
        );
        assert!(
            !SchedWitness {
                single_ok: 1,
                ..base
            }
            .evidence_ok()
        );
    }
}
