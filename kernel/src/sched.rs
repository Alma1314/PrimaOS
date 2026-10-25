//! 切片 M：最小协作式调度器。
//!
//! 只负责「当前 Task id」与「每个 Task 的完整用户上下文（`SretFrame`）」。任务状态由真实
//! [`crate::bootstrap`] 的 `TaskTable` 管理；切换时先完成 TaskTable 的 Running/Ready 迁移，
//! 再更新 [`current`]，最后经 [`crate::exc::resume`] 直接 `iretq` 进下一任务的用户上下文。
//!
//! **范围边界：** 单核、非抢占、无时钟/中断；仅 `SYS_YIELD` 触发的协作切换；共享单一
//! `TSS.rsp0` 内核栈（每次 CPL3→CPL0 硬件重置，无需 per-task 内核栈或 `swapgs`）。

use core::sync::atomic::{AtomicU32, Ordering};

use crate::bootstrap;
use crate::exc::SretFrame;
use prima_ostd::sync::StaticCell;

/// 调度器与 Task 表共用同一上限，避免新增 Task 只有模型槽、没有上下文槽。
pub const MAX_TASKS: usize = bootstrap::MAX_TASKS;

/// 当前运行的 Task id（进入用户态前初始化；初值 1 与 Bootstrap init Task 一致）。
static CURRENT: AtomicU32 = AtomicU32::new(1);

/// 每个 Task 的用户上下文；索引即 Task id。
static FRAMES: StaticCell<[SretFrame; MAX_TASKS]> = StaticCell::new([SretFrame::ZERO; MAX_TASKS]);

/// A blocked SYS_RECV's validated user out pointer, indexed by Task id (0 = none).
static RECV_OUTS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// 每个 Task 的 PML4 物理地址（0 = 未设置，例如 boot 早期）。索引即 Task id。
static ROOTS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// 每个 Task 的内核栈顶虚拟地址（0 = 未设置）。索引即 Task id。
static KSTACK_TOPS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// M2/T-MUSL-02：每个 Task 的 `%fs` 基址（0 = 无 TLS 上下文）。索引即 Task id。
static FS_BASES: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

/// FXSAVE 区域大小（x87 + SSE + MXCSR）。
const FXSAVE_SIZE: usize = 512;

/// 一个 Task 的 x87/SSE 状态镜像；`FXSAVE` 要求 16 字节对齐。
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct FpuArea([u8; FXSAVE_SIZE]);

impl FpuArea {
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
const MAX_DEFERRED_FRAMES: usize = 40;

/// 待归还的页表帧（自毁路径入队，[`flush_deferred_frames`] 清空）。
static DEFERRED_FRAMES: StaticCell<[u64; MAX_DEFERRED_FRAMES]> =
    StaticCell::new([0; MAX_DEFERRED_FRAMES]);
static DEFERRED_COUNT: StaticCell<usize> = StaticCell::new(0);

/// M17b：等待任务的 status 写回（在目标任务的地址空间里执行）。
static PENDING_STATUS: StaticCell<[Option<(u64, i64)>; MAX_TASKS]> =
    StaticCell::new([None; MAX_TASKS]);

pub fn set_pending_status(task_id: u32, ptr: u64, status: i64) {
    if (task_id as usize) < MAX_TASKS {
        PENDING_STATUS.with_mut(|slots| slots[task_id as usize] = Some((ptr, status)));
    }
}

/// Linux wait status is an int (4 bytes), never an 8-byte syscall word.
pub const fn wait_status_bytes(status: i64) -> [u8; 4] {
    (((status as u32) & 0xff) << 8).to_le_bytes()
}

/// Resolve the output while its owner is current, before wait can block/reap.
pub fn prepare_wait_status(task: u32, ptr: u64) -> Result<(), crate::syscall::Errno> {
    if ptr == 0 {
        return Ok(());
    }
    let root = root(task).ok_or(crate::syscall::Errno::Fault)?;
    let (_, offset) = crate::cow::root_and_offset().ok_or(crate::syscall::Errno::Fault)?;
    let segment = bootstrap::task_segment_id(task).ok_or(crate::syscall::Errno::Fault)?;
    crate::vspace::prepare_user_u32_write(
        root,
        ptr,
        |phys| crate::mem::phys_to_virt(phys, offset),
        || crate::frames::allocate_for_segment(segment).ok(),
        prima_ostd::arch::invlpg,
    )
    .map_err(wait_copy_errno)
}

fn wait_copy_errno(error: crate::vspace::VspaceError) -> crate::syscall::Errno {
    match error {
        crate::vspace::VspaceError::NoFrame => crate::syscall::Errno::NoSpace,
        _ => crate::syscall::Errno::Fault,
    }
}

pub fn write_wait_status(task: u32, ptr: u64, status: i64) -> Result<(), crate::syscall::Errno> {
    if ptr == 0 {
        return Ok(());
    }
    let root = root(task).ok_or(crate::syscall::Errno::Fault)?;
    let (_, offset) = crate::cow::root_and_offset().ok_or(crate::syscall::Errno::Fault)?;
    let segment = bootstrap::task_segment_id(task).ok_or(crate::syscall::Errno::Fault)?;
    crate::vspace::write_user_u32(
        root,
        ptr,
        wait_status_bytes(status),
        |phys| crate::mem::phys_to_virt(phys, offset),
        || crate::frames::allocate_for_segment(segment).ok(),
        prima_ostd::arch::invlpg,
    )
    .map_err(wait_copy_errno)
}

fn write_pending_status(task_id: u32) {
    let pending = PENDING_STATUS.with_mut(|slots| slots[task_id as usize].take());
    if let Some((ptr, status)) = pending
        && let Err(error) = write_wait_status(task_id, ptr, status)
    {
        set_frame_rax(task_id, crate::syscall::encode_errno(error));
    }
}

/// 当前运行的 Task id。
pub fn current() -> u32 {
    CURRENT.load(Ordering::Relaxed)
}

/// 进入用户态前初始化调度器（当前为 init Task）。
pub fn init() {
    CURRENT.store(crate::bootstrap::INIT_TASK_ID, Ordering::Relaxed);
}

fn set_current(task_id: u32) {
    CURRENT.store(task_id, Ordering::Relaxed);
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

pub fn clear_recv_out(task_id: u32) {
    set_recv_out(task_id, 0);
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

/// 把当前硬件 x87/SSE 状态保存到 `task_id` 的槽位。
fn save_fpu(task_id: u32) {
    FPU_AREAS.with_mut(|areas| {
        if let Some(area) = areas.get_mut(task_id as usize) {
            prima_ostd::arch::fxsave(area.0.as_mut_ptr());
        }
    });
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
    if (task_id as usize) < MAX_TASKS {
        ROOTS.with_mut(|roots| roots[task_id as usize] = 0);
        KSTACK_TOPS.with_mut(|tops| tops[task_id as usize] = 0);
        FS_BASES.with_mut(|bases| bases[task_id as usize] = 0);
        FPU_AREAS.with_mut(|areas| areas[task_id as usize] = FpuArea::DEFAULT);
        FRAMES.with_mut(|frames| frames[task_id as usize] = SretFrame::ZERO);
        clear_recv_out(task_id);
        PENDING_STATUS.with_mut(|slots| slots[task_id as usize] = None);
    }
}

/// 选择下一个 Ready 任务（最小 id，排除 `cur`）。
pub fn pick_next(cur: u32) -> Option<u32> {
    (0..MAX_TASKS as u32)
        .filter(|task_id| *task_id != cur)
        .find(|task_id| bootstrap::task_status(*task_id) == Some(bootstrap::TaskStatus::Ready))
}

/// 协作切换到 `next`：先把 TaskTable 的 Running/Ready 原子迁移成功，再更新当前 id 并恢复。
///
/// 迁移失败即 `panic`——绝不带着不一致的身份继续 `resume`，否则 `authorize` 会用错调用者。
pub fn switch_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "switch_to must target a different task");
    bootstrap::yield_task_global(cur).expect("current task must be Running to yield");
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    save_fpu(cur);
    set_current(next);
    resume(next)
}

/// 切片 N：当前任务已由调用方置 `BlockedRecv`，调度 `next`（Ready→Running）并切走。
///
/// 与 [`switch_to`] 的区别：不把当前任务置回 Ready（它保持阻塞，等待 raise 唤醒）。
pub fn block_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "block_to must target a different task");
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    save_fpu(cur);
    set_current(next);
    resume(next)
}

/// 切片 V：把一个**已从任务表删除**的当前任务切走。
///
/// 与 [`block_to`]/[`switch_to`] 的区别：不触碰 `cur` 的 TaskTable 记录——自毁者已 Dead，
/// `yield_task_global` 会拒绝。调用方（syscall 层）必须先确认存在 Ready 后继。
pub fn exit_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "exit_to must target a different task");
    bootstrap::schedule_task_global(next).expect("next task must be Ready to schedule");
    save_fpu(cur);
    set_current(next);
    resume(next)
}

/// 硬件故障后的切换：当前任务已经是 Faulted，不可重新置 Ready。
pub fn fault_to(next: u32) -> ! {
    let cur = current();
    assert_ne!(cur, next, "fault_to must target a different task");
    bootstrap::schedule_task_global(next).expect("fault successor must be Ready");
    save_fpu(cur);
    set_current(next);
    resume(next)
}

/// 切片 V：延迟帧队列已满。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeferredQueueFull;

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
pub fn resume(task_id: u32) -> ! {
    if let Some(top) = kstack_top(task_id) {
        crate::gdt::set_rsp0(top);
    }
    if let Some(root) = root(task_id) {
        crate::cpu::write_cr3(root);
        write_pending_status(task_id);
    }
    let frame = frame(task_id).expect("task frame must exist");
    // M2/T-MUSL-02：切回用户态前装入该任务自己的 FS 选择子与基址。
    activate_fs(task_id);
    // M3：x87/SSE 状态按 Task 恢复（保存发生在各切换函数的 set_current 之前）。
    restore_fpu(task_id);
    // 切片 V：CR3 已不再指向任何待释放的根帧，此刻归还是安全的。
    flush_deferred_frames();
    // 跳板在 iretq 换栈前读完所有帧字段，当前栈上的 frame 在此期间仍有效。
    // 不跨不返回的控制转移持有 StaticCell 锁，避免锁 guard 被永久遗留。
    crate::exc::resume(&frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_status_has_exact_int_width_and_clear_task_discards_pending_output() {
        assert_eq!(wait_status_bytes(7), [0, 7, 0, 0]);
        assert_eq!(wait_status_bytes(-1), [0, 255, 0, 0]);
        set_pending_status(15, 0x1000, 7);
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
