#![no_std]
#![forbid(unsafe_code)]

//! Prima 内核与用户态运行时共享的实验性机器接口。
//!
//! 这些调用号用于当前单机原型，**不是稳定 ABI**。应用应依赖 `prima-runtime`，不应直接
//! 使用本 crate；编号变更必须同步调整内核分派和运行时封装。

/// EXO-1/2D：内核与 xtask 共享的通用 boot image（ramdisk）manifest 格式。
///
/// 每个条目是 **opaque `kind` + ELF 字节块**；内核不解释服务名/协议，只按
/// [`KIND_INIT`] 找启动镜像，其余条目按 manifest 顺序通用打包。服务语义由 ring3
/// 服务管理器按 `kind` 解释。
pub mod boot_image {
    /// little-endian 的 `"PRIMARMD"`。
    pub const MAGIC: u64 = 0x444d_5241_4d49_5250;
    /// EXO-2D：带 opaque `kind` 的 manifest 版本。
    pub const VERSION: u64 = 2;
    /// manifest 条目容量。内核只检验实际 `count <= MAX_ENTRIES`，不假定固定服务数量。
    pub const MAX_ENTRIES: usize = 16;
    /// `kind` 保留值：init 映像（内核据此定位启动镜像，不解释其余 kind）。
    pub const KIND_INIT: u64 = 0;
    /// 头部 qword 数：magic / version / count。
    pub const HEADER_QWORDS: usize = 3;
    /// 每个条目的 qword 数：kind / offset / len。
    pub const ENTRY_QWORDS: usize = 3;

    /// EXO-2B/2D：服务映像段目录页的第一个 u64。
    pub const SERVICE_IMAGE_MAGIC: u64 = 0x5345_5256_494D_4701;
    /// 服务映像目录头部字节数（`magic | count`）。
    pub const SERVICE_IMAGE_DIR_HEADER: u64 = 16;
    /// EXO-2D：目录项字节数（`kind | offset_pages | page_count`）。
    pub const SERVICE_IMAGE_ENTRY_LEN: u64 = 24;
    /// 目录条目容量（与 manifest 容量一致；ring3 服务管理器自行跳过自身 kind）。
    pub const SERVICE_IMAGE_ENTRY_COUNT: u64 = MAX_ENTRIES as u64;
}

/// EXO-2D：内核写入、ring3 init 读取的通用 boot entry ABI。
///
/// 该页放在 init 用户栈页的低地址区；`task1_frame.r15` 在进入 CPL3 前指向
/// [`BOOT_INFO_VA`]。生成器通过 `[r15+disp]` 读取 manifest 条目与 boot capability
/// 句柄，故 `prima-initgen` 不再镜像具体句柄值。
pub mod boot_abi {
    use super::boot_image;

    /// boot info 页 magic。
    pub const MAGIC: u64 = 0x5449_4E49_424F_4F42; // "BOOTINIT"
    /// boot info 布局版本。
    pub const VERSION: u64 = 1;
    /// init 用户栈页内的 boot info 起始 VA（`USER_STACK_VA + 0x100`）。
    pub const BOOT_INFO_VA: u64 = 0x0000_0A00_0000_1100;
    /// boot info 支持的最大 manifest 条目数。
    pub const MAX_ENTRIES: usize = boot_image::MAX_ENTRIES;
    /// boot info 区字节数（给内核清零用；保留余量）。
    pub const AREA_LEN: usize = 0x700;

    pub const OFF_MAGIC: u32 = 0;
    pub const OFF_VERSION: u32 = 8;
    pub const OFF_ENTRY_COUNT: u32 = 16;
    pub const OFF_ENTRIES: u32 = 32;
    pub const ENTRY_SIZE: u32 = 40;

    pub const ENTRY_OFF_KIND: u32 = 0;
    pub const ENTRY_OFF_OFFSET_PAGES: u32 = 8;
    pub const ENTRY_OFF_PAGES: u32 = 16;
    pub const ENTRY_OFF_ENTRY_VA: u32 = 24;
    pub const ENTRY_OFF_CODE_END_VA: u32 = 32;

    pub const OFF_HANDLES: u32 = OFF_ENTRIES + (MAX_ENTRIES as u32) * ENTRY_SIZE;

    pub const OFF_IMAGE_CAP: u32 = OFF_HANDLES;
    pub const OFF_NAMESPACE_CAP: u32 = OFF_IMAGE_CAP + 8;
    pub const OFF_TASK_CAP: u32 = OFF_NAMESPACE_CAP + 8;
    pub const OFF_PLATFORM_TASK1_CAP: u32 = OFF_TASK_CAP + 8;
    pub const OFF_TASK2_SEND_CAP: u32 = OFF_PLATFORM_TASK1_CAP + 8;
    pub const OFF_SERVICE_IMAGE_CAP: u32 = OFF_TASK2_SEND_CAP + 8;
    pub const OFF_SERVICE_IMAGE_MGR_CAP: u32 = OFF_SERVICE_IMAGE_CAP + 8;
    pub const OFF_SERVICEMGR_SEND_CAP: u32 = OFF_SERVICE_IMAGE_MGR_CAP + 8;
    pub const OFF_SERVICE3_STACK_CAP: u32 = OFF_SERVICEMGR_SEND_CAP + 8;
    pub const OFF_TASK_CREATE_SEND_CAP: u32 = OFF_SERVICE3_STACK_CAP + 8;
    pub const OFF_TASK4_SEND_CAP: u32 = OFF_TASK_CREATE_SEND_CAP + 8;
    pub const OFF_EXC_PAGE_FAULT_CAP: u32 = OFF_TASK4_SEND_CAP + 8;
    pub const OFF_EXC_SYSCALL_TRAP_CAP: u32 = OFF_EXC_PAGE_FAULT_CAP + 8;
    pub const OFF_PLATFORM_TASK1_X_CAP: u32 = OFF_EXC_SYSCALL_TRAP_CAP + 8;
    pub const OFF_PLATFORM_TASK2_CODE_CAP: u32 = OFF_PLATFORM_TASK1_X_CAP + 8;
    pub const OFF_TASK_CREATE_ROOT_NS_RIGHTS: u32 = OFF_PLATFORM_TASK2_CODE_CAP + 8;

    pub const fn entry_field(index: usize, field: u32) -> u32 {
        OFF_ENTRIES + (index as u32) * ENTRY_SIZE + field
    }
}

/// EXO-3D：内核只透传、不解释的通用用户态 trace 记录 ABI。
pub mod trace {
    /// 一条 trace 记录最多携带 5 个 u64 原始参数。
    pub const ARGS: usize = 5;
    /// 客户端共享页中的 trace 区；前 512 字节留给既有协议字段。
    pub const SHARED_OFFSET: u64 = 512;
    /// 8 字节记录数，后跟至多 16 条小端 `Record`（M12b 增加清理见证）。
    pub const SHARED_RECORDS: usize = 16;
    pub const RECORD_BYTES: usize = 8 * (1 + ARGS);
    const _: () = assert!(SHARED_OFFSET as usize + 8 + SHARED_RECORDS * RECORD_BYTES <= 4096);
    /// 用户态/xtask 约定的 runtime-hello trace 事件 ID（内核不分支解释）。
    pub const EVENT_RUNTIME_HELLO: u64 = 1;
    /// 用户态/xtask 约定的 service-reply trace 事件 ID（内核不分支解释）。
    pub const EVENT_SERVICE_REPLY: u64 = 2;
    /// 用户态/xtask 约定的 service-negotiated trace 事件 ID（内核不分支解释）。
    pub const EVENT_SERVICE_NEGOTIATED: u64 = 3;
    /// 用户态/xtask 约定的 service-selected trace 事件 ID（内核不分支解释）。
    pub const EVENT_SERVICE_SELECTED: u64 = 4;
    /// 用户态/xtask 约定的 servicemgr trace 事件 ID（内核不分支解释）。
    pub const EVENT_SERVICEMGR: u64 = 5;
    /// 原生应用共享段数据往返、取消与错误恢复见证。
    pub const EVENT_NATIVE_APP: u64 = 6;
    /// 阶段 5 首个服务后 Task 的 ring3 容量见证。
    pub const EVENT_STAGE5_CAPACITY: u64 = 7;
    /// M12b：退出清理见证。`args = [task_id, open_handles, 0, 0, 0]`，`open_handles`
    /// 由存储服务的只读状态查询返回；0 表示退出后服务端句柄已全部释放。
    pub const EVENT_STAGE5_CLEANUP: u64 = 8;

    /// 内核保存的通用 trace 记录；内核只复制/透传，不解释 `id` 或 `args`。
    #[repr(C)]
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Record {
        pub id: u64,
        pub args: [u64; ARGS],
    }

    impl Record {
        pub const EMPTY: Self = Self {
            id: 0,
            args: [0; ARGS],
        };
    }
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
    /// Stage 5: check the native Prima syscall ABI version before using it.
    pub const SYS_ABI_CHECK: u64 = 37;
    /// Stage 5 / M2：设置当前 Task 的 FS base（`%fs` 基址，用于 musl TLS/errno）。
    ///
    /// 追加号；只影响调用者自己的保存帧，切换任务时由调度器恢复各自的基址。
    pub const SYS_TASK_SET_FS_BASE: u64 = 38;
    /// Stage 5 / M6：启动 PIT 通道 0 的自由运行计数（约 10 ms 周期）。
    ///
    /// 不配置 PIC、不打开 IF：既有协作阶段用户帧保持不可中断。追加号。
    pub const SYS_TIME_ENABLE: u64 = 39;
    /// Stage 5 / M6：轮询并返回自 `SYS_TIME_ENABLE` 起的 PIT tick 数（约 100 Hz）。
    ///
    /// 与抽象 `SYS_TIME_NOW`（`advance_time` 时钟，仅用于 Timer）分离：本号直接折算
    /// 硬件倒计数，不推进 Timer 状态，故不会提前触发既有 Timer 见证。
    pub const SYS_TIME_TICKS: u64 = 40;
    /// Stage 5 / M10：`memseg_protect(task_cap, target_base, page_count, new_access)`（COSIR §7.4）。
    ///
    /// 追加号；把区间内已映射页的逻辑访问集原子地改为 `new_access`，并同步叶 PTE 的
    /// 可写位。新增权限要求调用者持有对应 Segment 的映射授权；入口页不得失去 `X`。
    pub const SYS_MEMSEG_PROTECT: u64 = 41;
    /// M15：`wait_object_create() -> wait_cap`（COSIR §21.1.1）。
    pub const SYS_WAIT_CREATE: u64 = 42;
    /// M15：`wait_object_observe(cap) -> generation`（COSIR §21.1.2）。
    pub const SYS_WAIT_OBSERVE: u64 = 43;
    /// M15：`wait_objects(cap0, gen0, cap1, gen1, mode, timeout)`（COSIR §21.2.1）。
    ///
    /// `cap1 == 0` 表示单一对象；`timeout == u64::MAX` 表示无限。M15a-1 只实现
    /// 立即满足/立即超时；需要阻塞时返回 `EAGAIN`（阻塞路径见后续切片）。
    pub const SYS_WAIT_OBJECTS: u64 = 44;
    /// M15：`signal_object(cap) -> generation`（COSIR §21.2.2）。
    pub const SYS_WAIT_SIGNAL: u64 = 45;
    /// M15：`wait_object_destroy(cap)`（COSIR §21.1.3）。
    pub const SYS_WAIT_DESTROY: u64 = 46;
}

/// Native syscall ABI version; independent of `prima-runtime`'s source API version.
pub mod native_abi {
    pub const MAJOR: u64 = 1;
    pub const MINOR: u64 = 0;
    pub const VERSION: u64 = (MAJOR << 32) | MINOR;
}
