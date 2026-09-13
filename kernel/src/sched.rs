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

/// `exc::resume` 需要帧指针；复制到静态以避免指向将废弃的内核栈局部量。
static mut SCRATCH: SretFrame = SretFrame::ZERO;

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

/// 恢复 `task_id` 的用户上下文（不返回）。
pub fn resume(task_id: u32) -> ! {
    let frame = frame(task_id).expect("task frame must exist");
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
}
