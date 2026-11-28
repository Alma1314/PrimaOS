//! 切片 M：最小协作式调度器。
//!
//! 只负责「当前 Task id」与「每个 Task 的完整用户上下文（`SretFrame`）」。任务状态由真实
//! [`crate::bootstrap`] 的 `TaskTable` 管理；切换时先完成 TaskTable 的 Running/Ready 迁移，
//! 再更新 [`current`]，最后经 [`crate::exc::resume`] 直接 `iretq` 进下一任务的用户上下文。
//!
//! **范围边界：** 单核、非抢占、无时钟/中断；仅 `SYS_YIELD` 触发的协作切换；共享单一
//! `TSS.rsp0` 内核栈（每次 CPL3→CPL0 硬件重置，无需 per-task 内核栈或 `swapgs`）。

use crate::bootstrap;
use crate::exc::SretFrame;
use prima_ostd::sync::StaticCell;

/// 调度器与 Task 表共用同一上限，避免新增 Task 只有模型槽、没有上下文槽。
pub const MAX_TASKS: usize = bootstrap::MAX_TASKS;

/// P5-3b-3：每 CPU 可运行队列、唯一所有者与单运行者守卫（仅 `sched-percpu-probe`）。
///
/// 状态全部在一个 IRQ-safe 自旋锁内维护；调用方**不得**在持有本锁时再取
/// `BOOTSTRAP_STATE`（锁序固定为 bootstrap → 本队列），故这里的操作都不查任务状态，
/// 由调用方在释放后经 `bootstrap` 校验。
#[cfg(feature = "sched-percpu-probe")]
mod queues {
    use super::MAX_TASKS;
    use crate::cpu::local::MAX_CPUS;
    use core::sync::atomic::{AtomicU32, Ordering};
    use prima_ostd::sync::StaticCell;

    /// 未指定所有者。
    pub const NO_CPU: u8 = 0xff;

    pub struct State {
        owner: [u8; MAX_TASKS],
        running: [bool; MAX_TASKS],
        queued: [bool; MAX_TASKS],
        len: [u8; MAX_CPUS],
        items: [[u32; MAX_TASKS]; MAX_CPUS],
    }

    impl State {
        const fn new() -> Self {
            Self {
                owner: [NO_CPU; MAX_TASKS],
                running: [false; MAX_TASKS],
                queued: [false; MAX_TASKS],
                len: [0; MAX_CPUS],
                items: [[0; MAX_TASKS]; MAX_CPUS],
            }
        }
    }

    static STATE: StaticCell<State> = StaticCell::new(State::new());
    /// 见证：每 CPU 入队/出队次数、放置数、单运行者违规数、状态不符丢弃数。
    static ENQ: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
    static DEQ: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
    static PLACED: AtomicU32 = AtomicU32::new(0);
    static VIOLATIONS: AtomicU32 = AtomicU32::new(0);
    static STALE: AtomicU32 = AtomicU32::new(0);

    fn valid(task: u32, cpu: usize) -> bool {
        (task as usize) < MAX_TASKS && task != 0 && cpu < MAX_CPUS
    }

    /// 发布：一次置 owner（只允许一次）并在需要时入队。失败不留下半发布状态。
    pub fn place(task: u32, cpu: usize, enqueue_task: bool) -> Result<(), ()> {
        if !valid(task, cpu) {
            return Err(());
        }
        STATE.with_mut(|state| {
            let slot = task as usize;
            let current = state.owner[slot];
            if current != NO_CPU && current as usize != cpu {
                return Err(());
            }
            let will_enqueue = enqueue_task && !state.queued[slot];
            let len = state.len[cpu] as usize;
            // 所有检查先于任何写入：失败不得留下已置 owner 的半发布任务。
            if will_enqueue && len >= MAX_TASKS {
                return Err(());
            }
            state.owner[slot] = cpu as u8;
            if will_enqueue {
                state.items[cpu][len] = task;
                state.len[cpu] = (len + 1) as u8;
                state.queued[slot] = true;
                ENQ[cpu].fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        })
    }

    /// 把一个已 Ready 的任务追加到其 owner 队列（唤醒路由）。
    pub fn wake(task: u32) -> bool {
        STATE.with_mut(|state| {
            let slot = task as usize;
            if slot >= MAX_TASKS || state.owner[slot] == NO_CPU || state.queued[slot] {
                return false;
            }
            let cpu = state.owner[slot] as usize;
            let len = state.len[cpu] as usize;
            if len >= MAX_TASKS {
                return false;
            }
            state.items[cpu][len] = task;
            state.len[cpu] = (len + 1) as u8;
            state.queued[slot] = true;
            ENQ[cpu].fetch_add(1, Ordering::Relaxed);
            true
        })
    }

    /// 读取队首（不消费）。`skip` 用于排除正在运行的任务（通常不在队列里）。
    pub fn peek(cpu: usize, skip: u32) -> Option<u32> {
        STATE.with(|state| {
            if cpu >= MAX_CPUS {
                return None;
            }
            state.items[cpu][..state.len[cpu] as usize]
                .iter()
                .copied()
                .find(|task| *task != skip)
        })
    }

    /// 丢弃一个候选（状态已不是 Ready）。
    pub fn discard(task: u32) {
        STATE.with_mut(|state| {
            let slot = task as usize;
            if slot >= MAX_TASKS || !state.queued[slot] {
                return;
            }
            let cpu = state.owner[slot] as usize;
            remove_locked(state, cpu, task);
            STALE.fetch_add(1, Ordering::Relaxed);
        });
    }

    fn remove_locked(state: &mut State, cpu: usize, task: u32) -> bool {
        let len = state.len[cpu] as usize;
        let Some(at) = state.items[cpu][..len].iter().position(|id| *id == task) else {
            return false;
        };
        for index in at..len - 1 {
            state.items[cpu][index] = state.items[cpu][index + 1];
        }
        state.len[cpu] = (len - 1) as u8;
        state.queued[task as usize] = false;
        DEQ[cpu].fetch_add(1, Ordering::Relaxed);
        true
    }

    /// 从 `cpu` 的队列取出 `task` 并置运行时守卫；任何所有者/重复运行违例都返回 false。
    pub fn take(task: u32, cpu: usize) -> bool {
        STATE.with_mut(|state| {
            let slot = task as usize;
            if !valid(task, cpu) || state.owner[slot] as usize != cpu {
                VIOLATIONS.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            if state.running[slot] {
                VIOLATIONS.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            if !remove_locked(state, cpu, task) {
                return false;
            }
            state.running[slot] = true;
            true
        })
    }

    pub fn release(task: u32) {
        STATE.with_mut(|state| {
            if (task as usize) < MAX_TASKS {
                state.running[task as usize] = false;
            }
        });
    }

    pub fn owner(task: u32) -> Option<usize> {
        STATE.with(|state| {
            let value = state.owner.get(task as usize).copied().unwrap_or(NO_CPU);
            (value != NO_CPU).then_some(value as usize)
        })
    }

    /// 任务销毁/清理：撤销 owner、运行时与队列项。
    pub fn forget(task: u32) {
        STATE.with_mut(|state| {
            let slot = task as usize;
            if slot >= MAX_TASKS {
                return;
            }
            if state.queued[slot] {
                let cpu = state.owner[slot] as usize;
                remove_locked(state, cpu, task);
            }
            state.owner[slot] = NO_CPU;
            state.running[slot] = false;
        });
    }

    /// 选择放置 CPU：在线 CPU 中队列最短者（BSP 兜底）。
    pub fn placement_cpu() -> usize {
        let online = crate::smp::online_mask();
        let mut best = crate::cpu::local::BSP;
        let mut best_len = u8::MAX;
        STATE.with(|state| {
            for cpu in 0..MAX_CPUS {
                if online & (1 << cpu) == 0 {
                    continue;
                }
                if state.len[cpu] < best_len {
                    best = cpu;
                    best_len = state.len[cpu];
                }
            }
        });
        best
    }

    pub fn stats() -> (u32, u32, u32, u32, u32) {
        let mut enq = 0;
        let mut deq = 0;
        for cpu in 0..MAX_CPUS {
            enq += ENQ[cpu].load(Ordering::Relaxed);
            deq += DEQ[cpu].load(Ordering::Relaxed);
        }
        (
            enq,
            deq,
            PLACED.load(Ordering::Relaxed),
            VIOLATIONS.load(Ordering::Relaxed),
            STALE.load(Ordering::Relaxed),
        )
    }

    pub fn count_placed() {
        PLACED.fetch_add(1, Ordering::Relaxed);
    }

    pub fn queue_lens() -> [u8; MAX_CPUS] {
        STATE.with(|state| state.len)
    }

    pub fn cpu_stats(cpu: usize) -> (u32, u32) {
        if cpu >= MAX_CPUS {
            return (0, 0);
        }
        (
            ENQ[cpu].load(Ordering::Relaxed),
            DEQ[cpu].load(Ordering::Relaxed),
        )
    }
}

/// 每个 Task 的用户上下文；索引即 Task id。
static FRAMES: StaticCell<[SretFrame; MAX_TASKS]> = StaticCell::new([SretFrame::ZERO; MAX_TASKS]);

/// A blocked SYS_RECV's validated user out pointer, indexed by Task id (0 = none).
static RECV_OUTS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);
static RECV_EXTENDED: StaticCell<[bool; MAX_TASKS]> = StaticCell::new([false; MAX_TASKS]);

/// 每个 Task 的 PML4 物理地址（0 = 未设置，例如 boot 早期）。索引即 Task id。
static ROOTS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);
static ROOT_GENERATIONS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// 每个 Task 的内核栈顶虚拟地址（0 = 未设置）。索引即 Task id。
static KSTACK_TOPS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// M2/T-MUSL-02：每个 Task 的 `%fs` 基址（0 = 无 TLS 上下文）。索引即 Task id。
static FS_BASES: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// GS-1：每个 Task 的用户 `%gs` 基址（0 = 无用户 GS 上下文）。索引即 Task id。
///
/// 与 [`FS_BASES`] 对称：用户无法在 CPL3 直接写 MSR，故值只经
/// [`crate::syscall`] 的 `SYS_TASK_SET_GS_BASE` 写入槽；任务切换时由
/// [`activate_gs`] 装入硬件。任务间不继承彼此的值。
static GS_BASES: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// FXSAVE 区域大小（x87 + SSE + MXCSR）。
const FXSAVE_SIZE: usize = 512;

/// 一个 Task 的 x87/SSE 状态镜像；`FXSAVE` 要求 16 字节对齐。
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub(crate) struct FpuArea([u8; FXSAVE_SIZE]);

impl FpuArea {
    pub(crate) fn bytes(self) -> [u8; FXSAVE_SIZE] {
        self.0
    }
    pub(crate) fn from_user(bytes: [u8; FXSAVE_SIZE], original: Self) -> Option<Self> {
        let mut mask = u32::from_le_bytes(original.0[28..32].try_into().ok()?);
        if mask == 0 {
            mask = 0xffbf;
        }
        let mxcsr = u32::from_le_bytes(bytes[24..28].try_into().ok()?);
        if mxcsr & !mask != 0 {
            return None;
        }
        let mut area = Self(bytes);
        area.0[28..32].copy_from_slice(&original.0[28..32]);
        Some(area)
    }
    /// `FXRSTOR` 可接受的初始状态：x87 控制字 `0x037F`（扩展精度、异常屏蔽），
    /// MXCSR `0x1F80`（屏蔽全部 SSE 异常、就近舍入）。全零控制字会把 x87 精度
    /// 降为单精度，不能作为默认值。
    const DEFAULT: Self = {
        let mut bytes = [0u8; FXSAVE_SIZE];
        bytes[0] = 0x7F;
        bytes[1] = 0x03;
        bytes[24] = 0x80;
        bytes[25] = 0x1F;
        Self(bytes)
    };
}

/// M3：每个 Task 的 x87/SSE 状态。索引即 Task id。
static FPU_AREAS: StaticCell<[FpuArea; MAX_TASKS]> = StaticCell::new([FpuArea::DEFAULT; MAX_TASKS]);

/// SPEC-070 / §5.2：每个 Task 的 `kernel_resume_frame`（**细化层持有**的 `Option<Bytes>`）。
///
/// 放细化层而非 `BootstrapState`：`Option<SretFrame>` 是 168 B，×8 任务 ≈ 1344 B，单独即超出
/// 该结构体 16 KiB 守卫的余量（按项目纪律不得放宽守卫）。§5.2 本就写明该字段由细化层持有。
///
/// 仅由**成功的 `exc_resume`** 写入（该原语的用户化属后续切片）；本切片实现存储与两条清空路径
/// ——§8.4 步骤 3 `schedule` 进 `Running` 时消费清空、§8.2 步骤 6 `task_destroy` 时清空。
static KERNEL_RESUME_FRAMES: StaticCell<[Option<SretFrame>; MAX_TASKS]> =
    StaticCell::new([None; MAX_TASKS]);

/// §5.2：写入 `task_id` 的 `kernel_resume_frame`（仅成功 `exc_resume` 调用）。
pub fn set_kernel_resume_frame(task_id: u32, frame: SretFrame) {
    KERNEL_RESUME_FRAMES.with_mut(|frames| {
        if let Some(slot) = frames.get_mut(task_id as usize) {
            *slot = Some(frame);
        }
    });
}

/// §5.2：读取 `task_id` 的 `kernel_resume_frame`。
pub fn kernel_resume_frame(task_id: u32) -> Option<SretFrame> {
    KERNEL_RESUME_FRAMES.with(|frames| frames.get(task_id as usize).copied().flatten())
}

/// §5.2/§8.2 步骤 6/§8.4 步骤 3：清空 `task_id` 的 `kernel_resume_frame`。
pub fn clear_kernel_resume_frame(task_id: u32) {
    KERNEL_RESUME_FRAMES.with_mut(|frames| {
        if let Some(slot) = frames.get_mut(task_id as usize) {
            *slot = None;
        }
    });
}

/// 切片 V：自毁任务待归还的页表帧队列容量。
///
/// 自毁者正运行在自己的 CR3 上，其根帧与中间层页表帧必须等 `write_cr3` 切走之后才能归还。
/// 上界 = 根帧 1 + 用户映射拆除回收的空表帧（≤ `MAX_VSPACE_ENTRIES`）。
const MAX_DEFERRED_FRAMES: usize = 1 + 3 * bootstrap::MAX_VSPACE_ENTRIES;

/// 待归还的页表帧（自毁路径入队，[`flush_deferred_frames`] 清空）。
static DEFERRED_FRAMES: StaticCell<[u64; MAX_DEFERRED_FRAMES]> =
    StaticCell::new([0; MAX_DEFERRED_FRAMES]);
static DEFERRED_COUNT: StaticCell<usize> = StaticCell::new(0);

/// M17b：等待任务的 status 写回（在目标任务的地址空间里执行）。
static PENDING_STATUS: StaticCell<[Option<(u64, bootstrap::ProcessExit)>; MAX_TASKS]> =
    StaticCell::new([None; MAX_TASKS]);

pub fn set_pending_status(task_id: u32, ptr: u64, status: bootstrap::ProcessExit) {
    if (task_id as usize) < MAX_TASKS {
        PENDING_STATUS.with_mut(|slots| slots[task_id as usize] = Some((ptr, status)));
    }
}

/// Native continuations that must be consumed before a Ready Task is gated.
pub(crate) fn has_pending_return(task: u32) -> bool {
    KERNEL_RESUME_FRAMES.with(|frames| frames.get(task as usize).is_some_and(Option::is_some))
        || recv_out(task).is_some()
        || PENDING_STATUS.with(|slots| slots.get(task as usize).is_some_and(Option::is_some))
}

/// Linux wait status is an int (4 bytes), never an 8-byte syscall word.
pub const fn wait_status_bytes(status: bootstrap::ProcessExit) -> [u8; 4] {
    let word = match status {
        bootstrap::ProcessExit::Exited(code) => (code as u32) << 8,
        bootstrap::ProcessExit::Signaled(signal) => (signal as u32) & 0x7f,
    };
    word.to_le_bytes()
}

/// Resolve the output while its owner is current, before wait can block/reap.
pub fn prepare_wait_status(task: u32, ptr: u64) -> Result<(), crate::syscall::Errno> {
    if ptr == 0 {
        return Ok(());
    }
    crate::syscall::prepare_task_output(task, ptr, 4).map(|_| ())
}

pub fn write_wait_status(
    task: u32,
    ptr: u64,
    status: bootstrap::ProcessExit,
) -> Result<(), crate::syscall::Errno> {
    write_user_word(task, ptr, wait_status_bytes(status))
}

pub fn write_user_word(task: u32, ptr: u64, bytes: [u8; 4]) -> Result<(), crate::syscall::Errno> {
    if ptr == 0 {
        return Ok(());
    }
    prepare_wait_status(task, ptr)?;
    let root = root(task).ok_or(crate::syscall::Errno::Fault)?;
    let (_, offset) = crate::cow::root_and_offset().ok_or(crate::syscall::Errno::Fault)?;
    crate::vspace::write_user_u32(
        root,
        ptr,
        bytes,
        |phys| crate::mem::phys_to_virt(phys, offset),
        || None,
        |_| unreachable!("preflight leaves no COW allocation"),
        prima_ostd::arch::invlpg,
    )
    .map_err(|_| crate::syscall::Errno::Fault)
}

fn write_pending_status(task_id: u32) {
    let pending = PENDING_STATUS.with_mut(|slots| slots[task_id as usize].take());
    if let Some((ptr, status)) = pending
        && let Err(error) = write_wait_status(task_id, ptr, status)
    {
        set_frame_rax(task_id, crate::syscall::encode_errno(error));
    }
}

/// 当前 CPU 上运行的 Task id（D15：这是 CPU 本地状态，不是全局变量）。
pub fn current() -> u32 {
    crate::cpu::local::current_task()
}

/// 进入用户态前初始化调度器（当前为 init Task）。
pub fn init() {
    crate::cpu::local::set_current_task(crate::bootstrap::INIT_TASK_ID);
}

fn set_current(task_id: u32) {
    crate::cpu::local::set_current_task(task_id);
}

/// 记录某 Task 的用户上下文（初始进入或让出时保存）。
pub fn set_frame(task_id: u32, frame: SretFrame) {
    if (task_id as usize) < MAX_TASKS {
        FRAMES.with_mut(|frames| frames[task_id as usize] = frame);
    }
}

/// 读取某 Task 的用户上下文。
pub fn frame(task_id: u32) -> Option<SretFrame> {
    if (task_id as usize) < MAX_TASKS {
        Some(FRAMES.with(|frames| frames[task_id as usize]))
    } else {
        None
    }
}

/// Save the out pointer while SYS_RECV is blocked; clear it when that call completes.
pub fn set_recv_out(task_id: u32, ptr: u64) {
    RECV_OUTS.with_mut(|outs| {
        if let Some(out) = outs.get_mut(task_id as usize) {
            *out = ptr;
        }
    });
}

pub fn recv_out(task_id: u32) -> Option<u64> {
    RECV_OUTS.with(|outs| outs.get(task_id as usize).copied().filter(|ptr| *ptr != 0))
}

pub fn set_recv_extended(task_id: u32, extended: bool) {
    RECV_EXTENDED.with_mut(|flags| {
        if let Some(flag) = flags.get_mut(task_id as usize) {
            *flag = extended;
        }
    });
}

pub fn recv_extended(task_id: u32) -> bool {
    RECV_EXTENDED.with(|flags| flags.get(task_id as usize).copied().unwrap_or(false))
}

pub fn clear_recv_out(task_id: u32) {
    set_recv_out(task_id, 0);
    set_recv_extended(task_id, false);
}

/// 切片 N：把某 Task 保存帧的 `rax` 设为 `value`。
///
/// 阻塞在 `SYS_EXC_WAIT` 的任务被 raise 唤醒且写入 pending 时，调用者据此把通知
/// payload 注入其保存帧；该任务被重新调度时 `iretq` 直接带着 payload 返回用户态。
pub fn set_frame_rax(task_id: u32, value: u64) {
    if (task_id as usize) < MAX_TASKS {
        FRAMES.with_mut(|frames| frames[task_id as usize].rax = value);
    }
}

/// 记录某 Task 的 PML4 物理地址（切片 O：per-task 地址空间）。
pub fn set_root(task_id: u32, root_phys: u64) {
    if (task_id as usize) < MAX_TASKS {
        ROOT_GENERATIONS.with_mut(|generations| {
            generations[task_id as usize] = generations[task_id as usize]
                .checked_add(1)
                .expect("root incarnation exhausted");
        });
        ROOTS.with_mut(|roots| roots[task_id as usize] = root_phys);
    }
}

/// 读取某 Task 的 PML4 物理地址（未设置返回 `None`）。
pub fn root(task_id: u32) -> Option<u64> {
    if (task_id as usize) < MAX_TASKS {
        let value = ROOTS.with(|roots| roots[task_id as usize]);
        if value != 0 {
            return Some(value);
        }
    }
    None
}

/// Detect root replacement even when the same physical address is reused.
pub fn root_identity(task: u32) -> Option<(u64, u64)> {
    root(task).map(|root| (root, ROOT_GENERATIONS.with(|g| g[task as usize])))
}

/// 当前任务的 PML4 物理地址（未设置返回 `None`，例如 boot 早期尚未建立任务根）。
pub fn current_root() -> Option<u64> {
    root(current())
}

/// 记录某 Task 的内核栈顶虚拟地址（切片 O：per-task 内核栈）。
pub fn set_kstack_top(task_id: u32, top: u64) {
    if (task_id as usize) < MAX_TASKS {
        KSTACK_TOPS.with_mut(|tops| tops[task_id as usize] = top);
    }
}

/// 读取某 Task 的内核栈顶虚拟地址（未设置返回 `None`）。
pub fn kstack_top(task_id: u32) -> Option<u64> {
    if (task_id as usize) < MAX_TASKS {
        let value = KSTACK_TOPS.with(|tops| tops[task_id as usize]);
        if value != 0 {
            return Some(value);
        }
    }
    None
}

/// M2/T-MUSL-02：记录某 Task 的 `%fs` 基址。
///
/// 调用方必须先校验地址为低半区规范地址（[`crate::syscall::validate_fs_base`]）；本函数
/// 不做校验，因为任务回收等内部路径需要把任意已保存值清零。
pub fn set_fs_base(task_id: u32, base: u64) {
    if (task_id as usize) < MAX_TASKS {
        FS_BASES.with_mut(|bases| bases[task_id as usize] = base);
    }
}

/// 读取某 Task 的 `%fs` 基址；0 表示没有 TLS 上下文。
pub fn fs_base(task_id: u32) -> Option<u64> {
    if (task_id as usize) < MAX_TASKS {
        let value = FS_BASES.with(|bases| bases[task_id as usize]);
        if value != 0 {
            return Some(value);
        }
    }
    None
}

/// GS-1：记录某 Task 的用户 `%gs` 基址（0 = 无）。
///
/// 调用方须先经 `validate_gs_base` 拒绝非规范值；任务回收路径可直接清零。
pub fn set_gs_base(task_id: u32, base: u64) {
    if (task_id as usize) < MAX_TASKS {
        GS_BASES.with_mut(|bases| bases[task_id as usize] = base);
    }
}

/// GS-1：读取某 Task 的用户 `%gs` 基址；0 表示无用户 GS 上下文。
pub fn gs_base(task_id: u32) -> Option<u64> {
    if (task_id as usize) < MAX_TASKS {
        let value = GS_BASES.with(|bases| bases[task_id as usize]);
        if value != 0 {
            return Some(value);
        }
    }
    None
}

/// GS-1：把 `task_id` 的用户 GS 基址装入硬件（仅在 GS 探针开关开启时）。
///
/// 与 [`activate_fs`] 一样是“强制覆盖”：进入用户态前 `GS_BASE` 必须是该任务的值。
/// 同时把 `KERNEL_GS_BASE` 置 0，使其在 `syscall` 入口 `swapgs` 后落入内核可见侧；
/// GS-3 之前内核不读 `%gs`，0 即可。开关关闭时整个函数是 no-op（普通镜像不触碰 GS MSR）。
#[cfg(not(feature = "sched-percpu-probe"))]
fn activate_gs(task_id: u32) {
    if !prima_ostd::entry::gs_swap_enabled() {
        return;
    }
    let base = GS_BASES.with(|bases| bases.get(task_id as usize).copied().unwrap_or(0));
    crate::msr::wrmsr(crate::msr::IA32_GS_BASE, base);
    crate::msr::wrmsr(crate::msr::IA32_KERNEL_GS_BASE, 0);
}

/// 把当前硬件 x87/SSE 状态保存到 `task_id` 的槽位。
fn save_fpu(task_id: u32) {
    FPU_AREAS.with_mut(|areas| {
        if let Some(area) = areas.get_mut(task_id as usize) {
            prima_ostd::arch::fxsave(area.0.as_mut_ptr());
        }
    });
}

pub(crate) fn snapshot_current_fpu() -> FpuArea {
    let task = current();
    save_fpu(task);
    FPU_AREAS.with(|areas| areas[task as usize])
}
pub(crate) fn saved_fpu(task: u32) -> Option<FpuArea> {
    FPU_AREAS.with(|areas| areas.get(task as usize).copied())
}

pub(crate) fn set_saved_fpu(task: u32, area: FpuArea) {
    FPU_AREAS.with_mut(|areas| {
        if let Some(slot) = areas.get_mut(task as usize) {
            *slot = area;
        }
    });
}
pub fn reset_fpu(task: u32) {
    set_saved_fpu(task, FpuArea::DEFAULT);
}

/// Snapshot the running caller before publishing a fork/clone child. A parent's
/// saved slot may predate its last user-space floating-point instruction.
pub fn inherit_current_fpu(child: u32) {
    let parent = current();
    if child as usize >= MAX_TASKS || parent as usize >= MAX_TASKS {
        return;
    }
    save_fpu(parent);
    FPU_AREAS.with_mut(|areas| areas[child as usize] = areas[parent as usize]);
}

/// 用 `task_id` 的槽位覆盖硬件 x87/SSE 状态。
fn restore_fpu(task_id: u32) {
    FPU_AREAS.with_mut(|areas| {
        if let Some(area) = areas.get_mut(task_id as usize) {
            prima_ostd::arch::fxrstor(area.0.as_ptr());
        }
    });
}

/// 把 `task_id` 的 FS 选择子与基址装入硬件。
///
/// 顺序固定：先装用户数据选择子（长模式下会把 FS.base 归零），再写 `IA32_FS_BASE`。
/// 无 TLS 的任务写入 0，保证任务之间不继承对方的基址。
fn activate_fs(task_id: u32) {
    let base = FS_BASES.with(|bases| bases.get(task_id as usize).copied().unwrap_or(0));
    prima_ostd::arch::load_fs_selector(crate::gdt::USER_DS);
    crate::msr::wrmsr(crate::msr::IA32_FS_BASE, base);
}

/// S2-D4：`task_destroy` 拆除目标后清除其调度登记，使该 id 不再可被恢复。
///
/// 清理 root、内核栈、FS base、保存帧及阻塞接收缓冲指针。
pub fn clear_task(task_id: u32) {
    crate::signal::clear_task(task_id);
    crate::syscall::clear_blocked_call(task_id);
    if (task_id as usize) < MAX_TASKS {
        set_root(task_id, 0);
        KSTACK_TOPS.with_mut(|tops| tops[task_id as usize] = 0);
        FS_BASES.with_mut(|bases| bases[task_id as usize] = 0);
        GS_BASES.with_mut(|bases| bases[task_id as usize] = 0);
        FPU_AREAS.with_mut(|areas| areas[task_id as usize] = FpuArea::DEFAULT);
        FRAMES.with_mut(|frames| frames[task_id as usize] = SretFrame::ZERO);
        crate::syscall::end_pending_recv(task_id, crate::syscall::RecvEnd::Retired);
        PENDING_STATUS.with_mut(|slots| slots[task_id as usize] = None);
        #[cfg(feature = "sched-percpu-probe")]
        queues::forget(task_id);
    }
}

/// 选择下一个 Ready 任务（最小 id，排除 `cur`）。
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn pick_next(cur: u32) -> Option<u32> {
    bootstrap::gate_audit::enforce_stop("pick-next");
    (0..MAX_TASKS as u32)
        .filter(|task_id| *task_id != cur)
        .find(|task_id| bootstrap::task_status(*task_id) == Some(bootstrap::TaskStatus::Ready))
}

/// P5-3b-3：只从本 CPU 队列选择；不在本 CPU 所有者名下的任务永不被看到。
#[cfg(feature = "sched-percpu-probe")]
pub fn pick_next(cur: u32) -> Option<u32> {
    bootstrap::gate_audit::enforce_stop("pick-next");
    let cpu = crate::cpu::local::current_index();
    loop {
        let task = queues::peek(cpu, cur)?;
        if bootstrap::task_status(task) == Some(bootstrap::TaskStatus::Ready) {
            return Some(task);
        }
        queues::discard(task);
    }
}

/// P5-3b-3：发布失败（容量或 owner 冲突）；不留下半发布状态。
#[cfg(feature = "sched-percpu-probe")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaceError;

/// P5-3b-3：发布一个尚未运行的任务到指定 CPU（owner 与入队一次完成）。
#[cfg(feature = "sched-percpu-probe")]
pub fn publish_placed(task: u32, cpu: usize) -> Result<(), PlaceError> {
    queues::place(task, cpu, true).map_err(|_| PlaceError)?;
    queues::count_placed();
    Ok(())
}

/// P5-3b-3：fork 子任务随父 CPU 发布（不迁移、不 work stealing）。
#[cfg(feature = "sched-percpu-probe")]
pub fn publish_fork(child: u32, parent: u32) -> Result<(), PlaceError> {
    let cpu = queues::owner(parent).ok_or(PlaceError)?;
    publish_placed(child, cpu)
}

/// P5-3b-3：为尚未运行的独立任务选择在线 CPU（队列最短者）。
#[cfg(feature = "sched-percpu-probe")]
pub fn placement_cpu() -> usize {
    queues::placement_cpu()
}

/// P5-3b-3：把已 Ready 的目标按 owner 重新入队（阻塞唤醒路由）。
#[cfg(feature = "sched-percpu-probe")]
pub fn wake_enqueue(task: u32) {
    queues::wake(task);
}

/// P5-3b-3：任务的所有者（未发布返回 `None`）。
#[cfg(feature = "sched-percpu-probe")]
pub fn owner(task: u32) -> Option<usize> {
    queues::owner(task)
}

/// P5-3b-3：队列/单运行者见证计数。
#[cfg(feature = "sched-percpu-probe")]
pub fn queue_stats() -> (u32, u32, u32, u32, u32) {
    queues::stats()
}

/// P5-3b-3：每 CPU 当前队列长度（诊断）。
#[cfg(feature = "sched-percpu-probe")]
pub fn queue_lengths() -> [u8; crate::cpu::local::MAX_CPUS] {
    queues::queue_lens()
}

/// P5-3b-3：指定 CPU 的（入队, 出队）计数。
#[cfg(feature = "sched-percpu-probe")]
pub fn queue_cpu_stats(cpu: usize) -> (u32, u32) {
    queues::cpu_stats(cpu)
}

/// P5-3b-3：停放当前任务（调用方已把它置为阻塞态）并进入本 CPU 空闲循环。
#[cfg(feature = "sched-percpu-probe")]
pub fn park_current(cpu: usize) -> ! {
    queues::release(current());
    idle_for(cpu)
}

/// 协作切换到 `next`：先把 TaskTable 的 Running/Ready 原子迁移成功，再更新当前 id 并恢复。
///
/// 迁移失败即 `panic`——绝不带着不一致的身份继续 `resume`，否则 `authorize` 会用错调用者。
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn switch_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "switch_to must target a different task");
    bootstrap::yield_task_global(cur).expect("current task must be Running to yield");
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    save_fpu(cur);
    set_current(next);
    resume(next)
}

/// P5-3b-3：让出当前任务——先按 owner 入队，再由本 CPU 队列取出 `next`。
#[cfg(feature = "sched-percpu-probe")]
pub fn switch_to(next: u32) -> ! {
    let cur = current();
    let cpu = crate::cpu::local::current_index();
    assert_ne!(cur, next, "switch_to must target a different task");
    bootstrap::yield_task_global(cur).expect("current task must be Running to yield");
    queues::wake(cur);
    queues::release(cur);
    save_fpu(cur);
    if !queues::take(next, cpu) {
        idle_for(cpu)
    }
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    set_current(next);
    resume(next)
}

/// 切片 N：当前任务已由调用方置 `BlockedRecv`，调度 `next`（Ready→Running）并切走。
///
/// 与 [`switch_to`] 的区别：不把当前任务置回 Ready（它保持阻塞，等待 raise 唤醒）。
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn block_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "block_to must target a different task");
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    save_fpu(cur);
    set_current(next);
    resume(next)
}

/// P5-3b-3：阻塞切走——当前任务不回到队列，`next` 必须在本 CPU 队列中。
#[cfg(feature = "sched-percpu-probe")]
pub fn block_to(next: u32) -> ! {
    let cur = current();
    let cpu = crate::cpu::local::current_index();
    assert_ne!(cur, next, "block_to must target a different task");
    queues::release(cur);
    save_fpu(cur);
    if !queues::take(next, cpu) {
        idle_for(cpu)
    }
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    set_current(next);
    resume(next)
}

/// The caller is already blocked and its frame saved. Keep its kernel stack
/// while idle; IRQ callbacks only wake Tasks and return, never schedule here.
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn block_current() -> ! {
    if let Some(next) = pick_next(current()) {
        block_to(next);
    }
    save_fpu(current());
    idle_until_ready()
}

/// P5-3b-3：阻塞当前任务；本 CPU 无后继时进入本 CPU 空闲循环（轮询自有队列）。
#[cfg(feature = "sched-percpu-probe")]
pub fn block_current() -> ! {
    if let Some(next) = pick_next(current()) {
        block_to(next);
    }
    let cur = current();
    queues::release(cur);
    save_fpu(cur);
    idle_for(crate::cpu::local::current_index())
}

/// Retain the retiring Task's deferred root until another root is activated.
/// Its static kernel stack and shared supervisor mappings remain valid in idle.
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn exit_current() -> ! {
    if let Some(next) = pick_next(current()) {
        exit_to(next);
    }
    idle_until_ready()
}

/// P5-3b-3：退出当前任务；本 CPU 无后继时进入本 CPU 空闲循环。
#[cfg(feature = "sched-percpu-probe")]
pub fn exit_current() -> ! {
    if let Some(next) = pick_next(current()) {
        exit_to(next);
    }
    queues::release(current());
    idle_for(crate::cpu::local::current_index())
}

#[cfg(not(feature = "sched-percpu-probe"))]
fn idle_until_ready() -> ! {
    loop {
        crate::irq::retry_pending();
        if let Some(next) = (1..MAX_TASKS as u32)
            .find(|&id| bootstrap::task_status(id) == Some(bootstrap::TaskStatus::Ready))
        {
            bootstrap::schedule_task_global(next).expect("idle successor Ready");
            set_current(next);
            resume(next);
        }
        prima_ostd::arch::wait_for_interrupt();
    }
}

/// P5-3b-3：本 CPU 空闲循环。只消费本 CPU 队列；BSP 在全部探针阶段完成后收尾。
///
/// 单线程 TCG 下跨核唤醒无法靠 `hlt`/IPI（P5-4 才引入）送达，故以自旋轮询队列；
/// 这是探针范围的取舍，报告已登记。
#[cfg(feature = "sched-percpu-probe")]
pub fn idle_for(cpu: usize) -> ! {
    loop {
        if cpu == crate::cpu::local::BSP && crate::sched_probe::complete() {
            crate::sched_probe::finalize();
        }
        crate::sched_probe::note_stage(cpu, 20);
        if let Some(next) = queues::peek(cpu, 0) {
            crate::sched_probe::note_stage(cpu, 21);
            crate::sched_probe::note_task(cpu, next);
            let ready = bootstrap::task_status(next) == Some(bootstrap::TaskStatus::Ready);
            crate::sched_probe::note_stage(cpu, 22);
            if ready && queues::take(next, cpu) {
                crate::sched_probe::note_stage(cpu, 23);
                match bootstrap::schedule_task_global(next) {
                    Ok(()) => {
                        crate::sched_probe::note_stage(cpu, 24);
                        set_current(next);
                        resume(next);
                    }
                    Err(error) => {
                        let status = match bootstrap::task_status(next) {
                            Some(bootstrap::TaskStatus::Running) => 1,
                            Some(bootstrap::TaskStatus::Ready) => 2,
                            Some(bootstrap::TaskStatus::Faulted) => 3,
                            Some(bootstrap::TaskStatus::BlockedRecv) => 4,
                            Some(bootstrap::TaskStatus::BlockedWait) => 5,
                            Some(bootstrap::TaskStatus::Dead) => 6,
                            _ => 7,
                        };
                        crate::sched_probe::note_sched_err(
                            cpu,
                            match error {
                                bootstrap::TaskError::NoSuchTask => 7,
                                bootstrap::TaskError::AlreadyRunning => 10 + status,
                                bootstrap::TaskError::NotReady => 20 + status,
                                bootstrap::TaskError::AuditFull => 6,
                                _ => 90 + status,
                            },
                        );
                    }
                }
                queues::release(next);
            } else {
                crate::sched_probe::note_sched_err(cpu, if ready { 2 } else { 3 });
                queues::discard(next);
            }
        }
        crate::sched_probe::tick(cpu);
        core::hint::spin_loop();
    }
}

/// 切片 V：把一个**已从任务表删除**的当前任务切走。
///
/// 与 [`block_to`]/[`switch_to`] 的区别：不触碰 `cur` 的 TaskTable 记录——自毁者已 Dead，
/// `yield_task_global` 会拒绝。调用方（syscall 层）必须先确认存在 Ready 后继。
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn exit_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "exit_to must target a different task");
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    set_current(next);
    resume(next)
}

/// P5-3b-3：退出切走——当前任务不回到队列。
#[cfg(feature = "sched-percpu-probe")]
pub fn exit_to(next: u32) -> ! {
    let cur = current();
    let cpu = crate::cpu::local::current_index();
    assert_ne!(cur, next, "exit_to must target a different task");
    queues::release(cur);
    if !queues::take(next, cpu) {
        idle_for(cpu)
    }
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    set_current(next);
    resume(next)
}

/// 硬件故障后的切换：当前任务已经是 Faulted，不可重新置 Ready。
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn fault_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "fault_to must target a different task");
    bootstrap::schedule_task_global(next).expect("fault successor must be Ready");
    save_fpu(cur);
    set_current(next);
    resume(next)
}

/// P5-3b-3：故障切走——当前任务保持 Faulted，`next` 必须在本 CPU 队列中。
#[cfg(feature = "sched-percpu-probe")]
pub fn fault_to(next: u32) -> ! {
    let cur = current();
    let cpu = crate::cpu::local::current_index();
    assert_ne!(cur, next, "fault_to must target a different task");
    queues::release(cur);
    save_fpu(cur);
    if !queues::take(next, cpu) {
        idle_for(cpu)
    }
    bootstrap::schedule_task_global(next).expect("fault successor must be Ready");
    set_current(next);
    resume(next)
}

/// 切片 V：延迟帧队列已满。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeferredQueueFull;

pub fn can_defer_frames(needed: usize) -> bool {
    DEFERRED_COUNT.with(|count| needed <= MAX_DEFERRED_FRAMES - *count)
}

/// 切片 V：登记一个待归还页表帧（自毁路径）。队列满返回 `Err`，不静默丢弃。
pub fn defer_table_frame(frame: u64) -> Result<(), DeferredQueueFull> {
    DEFERRED_COUNT.with_mut(|count| {
        if *count >= MAX_DEFERRED_FRAMES {
            return Err(DeferredQueueFull);
        }
        DEFERRED_FRAMES.with_mut(|frames| frames[*count] = frame);
        *count += 1;
        Ok(())
    })
}

/// 待归还帧数（供 host 单测断言「切换前不释放」）。
pub fn deferred_frame_count() -> usize {
    DEFERRED_COUNT.with(|count| *count)
}

/// 归还全部待释放页表帧。
///
/// **只允许在 `write_cr3` 已切到别的根之后调用**——见 [`resume`] 中的释放点。
pub fn flush_deferred_frames() {
    DEFERRED_COUNT.with_mut(|count| {
        for index in 0..*count {
            let frame = DEFERRED_FRAMES.with(|frames| frames[index]);
            let _ = crate::frames::release_table_frame(frame);
        }
        *count = 0;
    });
}

/// 恢复 `task_id` 的用户上下文（不返回）。
///
/// 切片 O：恢复前先切换到该任务的地址空间与内核栈——`TSS.rsp0` 指到本任务内核栈顶
/// （CPL3→CPL0 硬件换栈用），再写 CR3。写 CR3 安全的前提是新根已由
/// [`crate::vspace::clone_root`] 复制了 bootloader 根的全部 PML4 项，故内核高半区
/// （本函数所在代码、当前栈、直映射）在新根下仍然可见。
#[cfg(not(feature = "sched-percpu-probe"))]
pub fn resume(task_id: u32) -> ! {
    bootstrap::gate_audit::enforce_stop("resume");
    // G5: entry timestamp for the service (5) and client (7) resumes; other
    // images compile this call out and the hook itself is a no-op.
    #[cfg(feature = "gate-probe")]
    bootstrap::gate_probe::bench::stamp(if task_id == 1 { 5 } else { 7 }, task_id);
    if let Some(top) = kstack_top(task_id) {
        crate::gdt::set_rsp0(top);
    }
    if let Some(root) = root(task_id) {
        crate::cpu::write_cr3(root);
        write_pending_status(task_id);
        crate::syscall::complete_pending_recv(task_id);
    }
    let mut frame = frame(task_id).expect("task frame must exist");
    // M2/T-MUSL-02：切回用户态前装入该任务自己的 FS 选择子与基址。
    activate_fs(task_id);
    // GS-1：装入该任务自己的用户 GS 基址（探针开关关闭时 no-op）。
    activate_gs(task_id);
    // M3：x87/SSE 状态按 Task 恢复（保存发生在各切换函数的 set_current 之前）。
    restore_fpu(task_id);
    if let Some(handler) = crate::syscall::prepare_signal_resume(task_id, frame) {
        frame = handler;
        set_frame(task_id, frame);
    }
    // 切片 V：CR3 已不再指向任何待释放的根帧，此刻归还是安全的。
    flush_deferred_frames();
    // 跳板在 iretq 换栈前读完所有帧字段，当前栈上的 frame 在此期间仍有效。
    // 不跨不返回的控制转移持有 StaticCell 锁，避免锁 guard 被永久遗留。
    #[cfg(feature = "gate-probe")]
    bootstrap::gate_probe::bench::stamp(4, task_id);
    crate::exc::resume(&frame)
}

/// P5-3b-3：每 CPU 恢复——本 CPU 的 `TSS.rsp0`、本 CPU 的恢复跳板与本任务用户 GS。
///
/// 与普通路径的差别：(1) 只写本 CPU 的 `rsp0`；(2) 不经 `activate_gs`（那会清掉
/// `KERNEL_GS_BASE`，而下一条 iretq 跳板要保存内核 GS 到该 MSR）；(3) 入口基址由
/// `%gs:0` 索引本 CPU 槽。
#[cfg(feature = "sched-percpu-probe")]
pub fn resume(task_id: u32) -> ! {
    bootstrap::gate_audit::enforce_stop("resume");
    let cpu = crate::cpu::local::current_index();
    crate::sched_probe::note_resume(cpu);
    crate::sched_probe::note_stage(cpu, 6);
    assert_eq!(current(), task_id, "resume target must be the current task");
    if let Some(top) = kstack_top(task_id) {
        crate::gdt::set_rsp0_for(cpu, top);
    }
    if let Some(root) = root(task_id) {
        crate::cpu::write_cr3(root);
        write_pending_status(task_id);
        crate::syscall::complete_pending_recv(task_id);
    }
    let mut frame = frame(task_id).expect("task frame must exist");
    activate_fs(task_id);
    restore_fpu(task_id);
    if let Some(handler) = crate::syscall::prepare_signal_resume(task_id, frame) {
        frame = handler;
        set_frame(task_id, frame);
    }
    flush_deferred_frames();
    let user_gs = gs_base(task_id).unwrap_or(0);
    prima_ostd::entry::resume_percpu_user(cpu, &frame, user_gs)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Both tests mutate the same global deferred-frame queue.
    static DEFERRED_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn wait_status_has_exact_int_width_and_clear_task_discards_pending_output() {
        assert_eq!(
            wait_status_bytes(bootstrap::ProcessExit::Exited(7)),
            [0, 7, 0, 0]
        );
        assert_eq!(
            wait_status_bytes(bootstrap::ProcessExit::Exited(255)),
            [0, 255, 0, 0]
        );
        assert_eq!(
            wait_status_bytes(bootstrap::ProcessExit::Signaled(15)),
            [15, 0, 0, 0]
        );
        assert_ne!(
            wait_status_bytes(bootstrap::ProcessExit::Exited(143)),
            wait_status_bytes(bootstrap::ProcessExit::Signaled(15))
        );
        set_pending_status(15, 0x1000, bootstrap::ProcessExit::Exited(7));
        clear_task(15);
        assert_eq!(PENDING_STATUS.with(|slots| slots[15]), None);
    }

    /// SPEC-070：§5.2 的 `kernel_resume_frame` 由**细化层**持有——存储契约（设置/读取/清空，
    /// 越界 id 为无操作）。两条清空路径（§8.4 步骤 3 `schedule`、§8.2 步骤 6 `task_destroy`）
    /// 的接线由 `bootstrap` 侧的单测驱动（本模块的全局静态在两个 crate 测试间共享，故此处只钉
    /// 本层契约）。
    #[test]
    fn kernel_resume_frame_storage_contract() {
        init();
        assert_eq!(kernel_resume_frame(2), None);
        set_kernel_resume_frame(2, SretFrame::ZERO);
        assert_eq!(kernel_resume_frame(2), Some(SretFrame::ZERO));
        clear_kernel_resume_frame(2);
        assert_eq!(kernel_resume_frame(2), None);
        // 越界 id 不得 panic（容量外的任务 id 是调用方的错误，但不该越界写）。
        set_kernel_resume_frame(u32::MAX, SretFrame::ZERO);
        clear_kernel_resume_frame(u32::MAX);
        assert_eq!(kernel_resume_frame(u32::MAX), None);
    }

    #[test]
    fn frames_round_trip_by_task_id() {
        init();
        assert_eq!(current(), 1);
        let mut ctx = SretFrame::ZERO;
        ctx.rip = 0x1234;
        set_frame(2, ctx);
        assert_eq!(frame(2).unwrap().rip, 0x1234);
        assert!(frame(MAX_TASKS as u32).is_none());
    }

    /// S2-D4：`task_destroy` 拆除后清除调度登记，使该 id 不可再被恢复。
    #[test]
    fn clear_task_resets_root_kstack_and_frame() {
        init();
        let mut ctx = SretFrame::ZERO;
        ctx.rip = 0xABCD;
        set_root(3, 0x3000);
        set_kstack_top(3, 0xFFFF_8000_0003_0000);
        set_frame(3, ctx);
        set_recv_out(3, 0x4000);
        assert_eq!(root(3), Some(0x3000));
        assert_eq!(frame(3).unwrap().rip, 0xABCD);
        assert_eq!(recv_out(3), Some(0x4000));

        clear_task(3);
        assert_eq!(root(3), None);
        assert_eq!(kstack_top(3), None);
        assert_eq!(frame(3).unwrap().rip, 0);
        assert_eq!(recv_out(3), None);

        // 越界 id 安全忽略。
        clear_task(MAX_TASKS as u32);
    }

    /// M2/T-MUSL-02：FS base 按 Task id 独立保存；`task_destroy` 清零，避免 id 复用后
    /// 继承上一个任务的 TLS 上下文。
    #[test]
    fn fs_bases_are_per_task_and_cleared_with_task() {
        init();
        set_fs_base(2, 0x0000_0A00_0000_2000);
        set_fs_base(3, 0x0000_0A00_0000_3000);
        assert_eq!(fs_base(2), Some(0x0000_0A00_0000_2000));
        assert_eq!(fs_base(3), Some(0x0000_0A00_0000_3000));
        // 0 表示无 TLS 上下文，不返回 Some(0)。
        set_fs_base(3, 0);
        assert_eq!(fs_base(3), None);
        assert_eq!(fs_base(2), Some(0x0000_0A00_0000_2000));

        clear_task(2);
        assert_eq!(fs_base(2), None);
        // 越界 id 安全忽略。
        set_fs_base(MAX_TASKS as u32, 0xDEAD);
        assert_eq!(fs_base(MAX_TASKS as u32), None);
        // 清理测试留下的状态，避免影响其他用例（全局静态在测试间共享）。
        clear_task(3);
    }

    /// GS-1：用户 GS base 按 Task id 独立保存；`task_destroy` 清零，避免 id 复用后继承
    /// 上一个任务的 GS 上下文（与 FS 对称）。
    #[test]
    fn gs_bases_are_per_task_and_cleared_with_task() {
        init();
        set_gs_base(2, 0x0000_0A00_0000_4000);
        set_gs_base(3, 0x0000_0A00_0000_5000);
        assert_eq!(gs_base(2), Some(0x0000_0A00_0000_4000));
        assert_eq!(gs_base(3), Some(0x0000_0A00_0000_5000));
        // 0 表示无用户 GS 上下文。
        set_gs_base(3, 0);
        assert_eq!(gs_base(3), None);
        assert_eq!(gs_base(2), Some(0x0000_0A00_0000_4000));

        clear_task(2);
        assert_eq!(gs_base(2), None);
        // 越界 id 安全忽略。
        set_gs_base(MAX_TASKS as u32, 0xDEAD);
        assert_eq!(gs_base(MAX_TASKS as u32), None);
        clear_task(3);
    }

    /// M3：初始 FPU 状态必须是可用的 x87/SSE 默认值；`clear_task` 复位 id 复用的槽位。
    #[test]
    fn fpu_default_state_and_task_clear() {
        let default = FpuArea::DEFAULT.0;
        assert_eq!(&default[0..2], &[0x7F, 0x03], "x87 FCW 扩展精度");
        assert_eq!(&default[24..26], &[0x80, 0x1F], "MXCSR 默认掩码");
        assert!(default[2..24].iter().all(|byte| *byte == 0));

        FPU_AREAS.with_mut(|areas| areas[4].0[100] = 0xAB);
        clear_task(4);
        FPU_AREAS.with(|areas| assert_eq!(areas[4].0, default));
        // 越界 id 不 panic、不越界写。
        clear_task(MAX_TASKS as u32);
    }

    /// 切片 V：页表帧的归还被推迟到 `flush_deferred_frames`——入队期间 `count > 0`
    /// 即「尚未归还」的直接证据。
    #[test]
    fn deferred_frames_stay_pending_until_flush() {
        let _guard = DEFERRED_TEST_LOCK.lock().unwrap();
        flush_deferred_frames();
        assert_eq!(deferred_frame_count(), 0);
        defer_table_frame(0x1000).unwrap();
        defer_table_frame(0x2000).unwrap();
        assert_eq!(deferred_frame_count(), 2, "入队后仍未归还");
        flush_deferred_frames();
        assert_eq!(deferred_frame_count(), 0, "flush 后才归还");
        // 幂等：再次 flush 不改变任何东西。
        flush_deferred_frames();
        assert_eq!(deferred_frame_count(), 0);
    }

    /// 切片 V：队列满时确定性失败，不静默丢弃已入队的帧。
    #[test]
    fn deferred_queue_rejects_overflow() {
        let _guard = DEFERRED_TEST_LOCK.lock().unwrap();
        flush_deferred_frames();
        for index in 0..MAX_DEFERRED_FRAMES {
            assert!(defer_table_frame(0x1000 + index as u64 * 0x1000).is_ok());
        }
        assert!(
            defer_table_frame(0xDEAD_0000).is_err(),
            "满队列必须确定性失败"
        );
        assert_eq!(deferred_frame_count(), MAX_DEFERRED_FRAMES);
        flush_deferred_frames();
    }

    #[test]
    fn roots_and_kstacks_round_trip_and_default_to_none() {
        init();
        // 未设置：返回 None（不把 0 当作合法根）。
        assert_eq!(root(1), None);
        assert_eq!(kstack_top(1), None);
        assert_eq!(current_root(), None);

        set_root(1, 0x1234_0000);
        set_kstack_top(1, 0xFFFF_8000_0001_0000);
        assert_eq!(root(1), Some(0x1234_0000));
        assert_eq!(kstack_top(1), Some(0xFFFF_8000_0001_0000));
        // 当前任务为 1，故 current_root 随之可见。
        assert_eq!(current_root(), Some(0x1234_0000));

        // 越界任务号安全忽略。
        set_root(MAX_TASKS as u32, 0xDEAD_0000);
        assert!(root(MAX_TASKS as u32).is_none());
        set_kstack_top(MAX_TASKS as u32, 0xDEAD);
        assert!(kstack_top(MAX_TASKS as u32).is_none());
    }
}
