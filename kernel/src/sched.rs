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

/// 调度器支持的任务数上限（与 `bootstrap::MAX_TASKS` 一致）。
const MAX_TASKS: usize = 8;

/// 当前运行的 Task id（进入用户态前初始化；初值 1 与 Bootstrap init Task 一致）。
static CURRENT: AtomicU32 = AtomicU32::new(1);

/// 每个 Task 的用户上下文；索引即 Task id。
static mut FRAMES: [SretFrame; MAX_TASKS] = [SretFrame::ZERO; MAX_TASKS];

/// 每个 Task 的 PML4 物理地址（0 = 未设置，例如 boot 早期）。索引即 Task id。
static mut ROOTS: [u64; MAX_TASKS] = [0; MAX_TASKS];

/// 每个 Task 的内核栈顶虚拟地址（0 = 未设置）。索引即 Task id。
static mut KSTACK_TOPS: [u64; MAX_TASKS] = [0; MAX_TASKS];

/// `exc::resume` 需要帧指针；复制到静态以避免指向将废弃的内核栈局部量。
static mut SCRATCH: SretFrame = SretFrame::ZERO;

/// 切片 V：自毁任务待归还的页表帧队列容量。
///
/// 自毁者正运行在自己的 CR3 上，其根帧与中间层页表帧必须等 `write_cr3` 切走之后才能归还。
/// 上界 = 根帧 1 + 用户映射拆除回收的空表帧（≤ `MAX_VSPACE_ENTRIES`）。
const MAX_DEFERRED_FRAMES: usize = 40;

/// 待归还的页表帧（自毁路径入队，[`flush_deferred_frames`] 清空）。
static mut DEFERRED_FRAMES: [u64; MAX_DEFERRED_FRAMES] = [0; MAX_DEFERRED_FRAMES];
static mut DEFERRED_COUNT: usize = 0;

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
        unsafe {
            let frames = core::ptr::addr_of_mut!(FRAMES);
            (*frames)[task_id as usize] = frame;
        }
    }
}

/// 读取某 Task 的用户上下文。
pub fn frame(task_id: u32) -> Option<SretFrame> {
    if (task_id as usize) < MAX_TASKS {
        unsafe {
            let frames = core::ptr::addr_of!(FRAMES);
            Some((*frames)[task_id as usize])
        }
    } else {
        None
    }
}

/// 切片 N：把某 Task 保存帧的 `rax` 设为 `value`。
///
/// 阻塞在 `SYS_EXC_WAIT` 的任务被 raise 唤醒且写入 pending 时，调用者据此把通知
/// payload 注入其保存帧；该任务被重新调度时 `iretq` 直接带着 payload 返回用户态。
pub fn set_frame_rax(task_id: u32, value: u64) {
    if (task_id as usize) < MAX_TASKS {
        unsafe {
            let frames = core::ptr::addr_of_mut!(FRAMES);
            (*frames)[task_id as usize].rax = value;
        }
    }
}

/// 记录某 Task 的 PML4 物理地址（切片 O：per-task 地址空间）。
pub fn set_root(task_id: u32, root_phys: u64) {
    if (task_id as usize) < MAX_TASKS {
        unsafe {
            let roots = core::ptr::addr_of_mut!(ROOTS);
            (*roots)[task_id as usize] = root_phys;
        }
    }
}

/// 读取某 Task 的 PML4 物理地址（未设置返回 `None`）。
pub fn root(task_id: u32) -> Option<u64> {
    if (task_id as usize) < MAX_TASKS {
        let value = unsafe {
            let roots = core::ptr::addr_of!(ROOTS);
            (*roots)[task_id as usize]
        };
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
        unsafe {
            let tops = core::ptr::addr_of_mut!(KSTACK_TOPS);
            (*tops)[task_id as usize] = top;
        }
    }
}

/// 读取某 Task 的内核栈顶虚拟地址（未设置返回 `None`）。
pub fn kstack_top(task_id: u32) -> Option<u64> {
    if (task_id as usize) < MAX_TASKS {
        let value = unsafe {
            let tops = core::ptr::addr_of!(KSTACK_TOPS);
            (*tops)[task_id as usize]
        };
        if value != 0 {
            return Some(value);
        }
    }
    None
}

/// S2-D4：`task_destroy` 拆除目标后清除其调度登记，使该 id 不再可被恢复。
///
/// 只清 root/kstack/frame 三个槽；调用者 id 不得被清除（自毁未实现，`task_destroy` 已拒绝）。
pub fn clear_task(task_id: u32) {
    if (task_id as usize) < MAX_TASKS {
        unsafe {
            let roots = core::ptr::addr_of_mut!(ROOTS);
            (*roots)[task_id as usize] = 0;
            let tops = core::ptr::addr_of_mut!(KSTACK_TOPS);
            (*tops)[task_id as usize] = 0;
            let frames = core::ptr::addr_of_mut!(FRAMES);
            (*frames)[task_id as usize] = SretFrame::ZERO;
        }
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
    set_current(next);
    resume(next)
}

/// 切片 V：延迟帧队列已满。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeferredQueueFull;

/// 切片 V：登记一个待归还页表帧（自毁路径）。队列满返回 `Err`，不静默丢弃。
pub fn defer_table_frame(frame: u64) -> Result<(), DeferredQueueFull> {
    unsafe {
        let count = core::ptr::addr_of_mut!(DEFERRED_COUNT);
        if *count >= MAX_DEFERRED_FRAMES {
            return Err(DeferredQueueFull);
        }
        let frames = core::ptr::addr_of_mut!(DEFERRED_FRAMES);
        (*frames)[*count] = frame;
        *count += 1;
    }
    Ok(())
}

/// 待归还帧数（供 host 单测断言「切换前不释放」）。
pub fn deferred_frame_count() -> usize {
    unsafe { *core::ptr::addr_of!(DEFERRED_COUNT) }
}

/// 归还全部待释放页表帧。
///
/// **只允许在 `write_cr3` 已切到别的根之后调用**——见 [`resume`] 中的释放点。
pub fn flush_deferred_frames() {
    unsafe {
        let count = core::ptr::addr_of_mut!(DEFERRED_COUNT);
        let frames = core::ptr::addr_of!(DEFERRED_FRAMES);
        for index in 0..*count {
            let _ = crate::frames::release_table_frame((*frames)[index]);
        }
        *count = 0;
    }
}

/// 恢复 `task_id` 的用户上下文（不返回）。
///
/// 切片 O：恢复前先切换到该任务的地址空间与内核栈——`TSS.rsp0` 指到本任务内核栈顶
/// （CPL3→CPL0 硬件换栈用），再写 CR3。写 CR3 安全的前提是新根已由
/// [`crate::vspace::clone_root`] 复制了 bootloader 根的全部 PML4 项，故内核高半区
/// （本函数所在代码、当前栈、直映射）在新根下仍然可见。
pub fn resume(task_id: u32) -> ! {
    let frame = frame(task_id).expect("task frame must exist");
    if let Some(top) = kstack_top(task_id) {
        crate::gdt::set_rsp0(top);
    }
    if let Some(root) = root(task_id) {
        unsafe { crate::cpu::write_cr3(root) };
    }
    // 切片 V：CR3 已不再指向任何待释放的根帧，此刻归还是安全的。
    flush_deferred_frames();
    unsafe {
        let scratch = core::ptr::addr_of_mut!(SCRATCH);
        *scratch = frame;
        crate::exc::resume(&*core::ptr::addr_of!(SCRATCH))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(root(3), Some(0x3000));
        assert_eq!(frame(3).unwrap().rip, 0xABCD);

        clear_task(3);
        assert_eq!(root(3), None);
        assert_eq!(kstack_top(3), None);
        assert_eq!(frame(3).unwrap().rip, 0);

        // 越界 id 安全忽略。
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
