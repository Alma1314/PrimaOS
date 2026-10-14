#![no_std]
#![forbid(unsafe_code)]

//! Prima 内核与用户态运行时共享的实验性机器接口。
//!
//! 这些调用号用于当前单机原型，**不是稳定 ABI**。应用应依赖 `prima-runtime`，不应直接
//! 使用本 crate；编号变更必须同步调整内核分派和运行时封装。

/// 当前 x86_64 syscall 递送层使用的实验性调用号。
pub mod syscall {
    pub const SYS_ECHO: u64 = 1;
    pub const SYS_SUM4: u64 = 2;
    pub const SYS_COPY_IN: u64 = 3;
    pub const SYS_AUTHORIZE: u64 = 4;
    pub const SYS_EXC_SUBSCRIBE: u64 = 5;
    pub const SYS_EXC_UNSUBSCRIBE: u64 = 6;
    pub const SYS_TASK_ID: u64 = 7;
    pub const SYS_YIELD: u64 = 8;
    pub const SYS_EXC_WAIT: u64 = 9;
    pub const SYS_TRAP: u64 = 10;
    pub const SYS_CAP_DERIVE: u64 = 11;
    pub const SYS_CAP_DROP: u64 = 12;
    pub const SYS_MEMSEG_CREATE: u64 = 13;
    pub const SYS_MEMSEG_MAP: u64 = 14;
    pub const SYS_MEMSEG_UNMAP: u64 = 15;
    pub const SYS_MEMSEG_DESTROY: u64 = 16;
    pub const SYS_SEND: u64 = 17;
    pub const SYS_RECV: u64 = 18;
    pub const SYS_TASK_CREATE: u64 = 19;
    pub const SYS_TASK_DESTROY: u64 = 20;
    pub const SYS_NS_CREATE: u64 = 21;
    pub const SYS_NS_BIND: u64 = 22;
    pub const SYS_NS_UNBIND: u64 = 23;
    pub const SYS_NS_MOUNT: u64 = 24;
    pub const SYS_NS_UNMOUNT: u64 = 25;
    pub const SYS_NS_RESOLVE: u64 = 26;
    pub const SYS_NS_DESTROY: u64 = 27;
    pub const SYS_TIME_NOW: u64 = 28;
    pub const SYS_TIMER_SET: u64 = 29;
    pub const SYS_TIMER_CANCEL: u64 = 30;
    pub const SYS_TIMER_DESTROY: u64 = 31;
    pub const SYS_AUDIT: u64 = 32;
    /// SPEC-113 / §15.5：`dev_ioport_create(base_port, port_count) -> dev_cap`。
    pub const SYS_DEV_IOPORT_CREATE: u64 = 33;
    /// SPEC-113 / §15.5：受控端口写 `dev_ioport_out(dev_cap, port, value)`（内核代执行 `outb`）。
    pub const SYS_DEV_IOPORT_OUT: u64 = 34;
    /// SPEC-113 / §15.5：受控端口读 `dev_ioport_in(dev_cap, port) -> u8`（内核代执行 `inb`）。
    pub const SYS_DEV_IOPORT_IN: u64 = 35;
}
