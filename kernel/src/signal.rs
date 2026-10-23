//! M18b：最小 POSIX 信号 handler（sigaction + 递送 + sigreturn）。
//!
//! 表在静态区（不占 `BootstrapState`）：每个 Task 32 个 handler 槽、32 位
//! pending 位图、一份被中断的 `SretFrame`。只支持 1..=31、无掩码、无实时信号。

use prima_ostd::entry::SretFrame;
use prima_ostd::sync::StaticCell;

use crate::sched::MAX_TASKS;

/// 信号槽上限（含 0 位，1..=31 有效）。
pub const MAX_SIGNALS: usize = 32;

static HANDLERS: StaticCell<[[u64; MAX_SIGNALS]; MAX_TASKS]> =
    StaticCell::new([[0; MAX_SIGNALS]; MAX_TASKS]);
static PENDING: StaticCell<[u32; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);
static SAVED: StaticCell<[Option<SretFrame>; MAX_TASKS]> = StaticCell::new([None; MAX_TASKS]);

pub fn set_handler(task_id: u32, signal: u32, handler: u64) {
    if (task_id as usize) < MAX_TASKS && (signal as usize) < MAX_SIGNALS {
        HANDLERS.with_mut(|handlers| handlers[task_id as usize][signal as usize] = handler);
    }
}

pub fn handler_of(task_id: u32, signal: u32) -> u64 {
    if (task_id as usize) < MAX_TASKS && (signal as usize) < MAX_SIGNALS {
        HANDLERS.with(|handlers| handlers[task_id as usize][signal as usize])
    } else {
        0
    }
}

pub fn set_pending(task_id: u32, signal: u32) {
    if (task_id as usize) < MAX_TASKS && (signal as usize) < MAX_SIGNALS {
        PENDING.with_mut(|pending| pending[task_id as usize] |= 1 << signal);
    }
}

pub fn pending(task_id: u32) -> u32 {
    if (task_id as usize) < MAX_TASKS {
        PENDING.with(|pending| pending[task_id as usize])
    } else {
        0
    }
}

pub fn clear_pending(task_id: u32, signal: u32) {
    if (task_id as usize) < MAX_TASKS && (signal as usize) < MAX_SIGNALS {
        PENDING.with_mut(|pending| pending[task_id as usize] &= !(1 << signal));
    }
}

/// 把 `saved` 交给 handler：保存现场并返回 handler 的初始帧；无 handler 返回 None。
pub fn deliver(task_id: u32, signal: u32, saved: SretFrame) -> Option<SretFrame> {
    let handler = handler_of(task_id, signal);
    if handler == 0 {
        return None;
    }
    clear_pending(task_id, signal);
    SAVED.with_mut(|slots| slots[task_id as usize] = Some(saved));
    let mut frame = saved;
    frame.rax = 0;
    frame.rdi = signal as u64;
    frame.rip = handler;
    // 函数入口按 ABI 期望 rsp % 16 == 8；留出 128 字节红区。
    frame.rsp = (saved.rsp & !0xf) - 8;
    Some(frame)
}

/// 取出被中断的现场（`SYS_SIGRETURN`）。
pub fn take_saved(task_id: u32) -> Option<SretFrame> {
    if (task_id as usize) < MAX_TASKS {
        SAVED.with_mut(|slots| slots[task_id as usize].take())
    } else {
        None
    }
}
