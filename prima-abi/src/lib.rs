#![no_std]
#![forbid(unsafe_code)]

//! Prima 内核与用户态运行时共享的实验性机器接口。
//!
//! 这些调用号用于当前单机原型，**不是稳定 ABI**。应用应依赖 `prima-runtime`，不应直接
//! 使用本 crate；编号变更必须同步调整内核分派和运行时封装。

/// EXO-1：内核与 xtask 共享的通用 boot image（ramdisk）格式。
///
/// 该格式只描述**opaque ELF 字节块**，内核不解释服务名或服务协议；服务名/注册表由后续
/// ring3 loader 读取自己的 manifest。当前过渡实现按固定顺序读取块，以兼容阶段 4 的
/// 既有服务映像布局。
pub mod boot_image {
    /// little-endian 的 `"PRIMARMD"`。
    pub const MAGIC: u64 = 0x444d_5241_4d49_5250;
    /// 格式版本。
    pub const VERSION: u64 = 1;
    /// EXO-2C：映像块数量：init / alpha / beta / servicemgr / log / ns / store / driver。
    pub const ENTRY_COUNT: u64 = 8;
    /// 头部 qword 数：magic / version / count。
    pub const HEADER_QWORDS: usize = 3;
    /// 每个条目的 qword 数：offset / len。
    pub const ENTRY_QWORDS: usize = 2;

    /// EXO-2B：服务映像段目录页的第一个 u64。
    pub const SERVICE_IMAGE_MAGIC: u64 = 0x5345_5256_494D_4701;
    /// 服务映像目录头部字节数（`magic | count`）。
    pub const SERVICE_IMAGE_DIR_HEADER: u64 = 16;
    /// 服务映像目录项字节数（`offset_pages | page_count`）。
    pub const SERVICE_IMAGE_ENTRY_LEN: u64 = 16;
    /// 目录中允许的服务条目数（不含 ring3 服务管理器自身）。
    pub const SERVICE_IMAGE_ENTRY_COUNT: u64 = 6;
}

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
    /// EXO-4A / §15.8：销毁 `Device` 对象 `dev_destroy(dev_cap)`。
    pub const SYS_DEV_DESTROY: u64 = 36;
}
