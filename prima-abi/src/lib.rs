#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

//! Prima 内核与用户态运行时共享的实验性机器接口。
//!
//! 这些调用号用于当前单机原型，**不是稳定 ABI**。应用应依赖 `prima-runtime`，不应直接
//! 使用本 crate；编号变更必须同步调整内核分派和运行时封装。

pub mod program_contract;

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
///
/// **冻结台账（D17）**：号段 `1..=72` 已分配，**不重编号**；`52` 保留不可用；`0` 是保留号
/// （确定性 `EINVAL`）。后续追加只能从 `73` 起逐项分配，**未实现的号不预留**——即
/// [`NUMBERS`] 里只出现已经实现并接入分派的号。
///
/// 常量与 [`NUMBERS`] 由同一个宏 [`syscall_numbers!`] 生成，因此不存在"改了常量忘了改
/// 台账"的漂移；`lib.rs` 末尾的冻结测试再把这 71 项逐号钉死，任何重编号都会失败。
pub mod syscall {
    /// 声明一个调用号常量，同时生成 [`NUMBERS`] 台账。
    ///
    /// 接受 `NAME = NUMBER,` 序列（可带 `///` 文档），声明顺序必须按号升序；
    /// 升序由冻结测试检查，不靠宏强制。
    macro_rules! syscall_numbers {
        ($( $(#[$doc:meta])* $name:ident = $num:literal, )*) => {
            $(
                $(#[$doc])*
                pub const $name: u64 = $num;
            )*

            /// 全部已实现调用号的 `(号, 名称)` 台账，按号升序。
            ///
            /// 内核分派、QEMU 证据与差分脚手架都可用它把不透明调用号翻译成名字；
            /// 台账的完整性由本宏保证，号的稳定性由冻结测试保证。
            pub const NUMBERS: &[(u64, &str)] = &[ $( ($num, stringify!($name)) ),* ];
        };
    }

    /// 保留号：曾用于内部占位，不分配、不实现、`name()` 返回 `None`。
    pub const RESERVED: [u64; 1] = [52];

    syscall_numbers! {
        SYS_ECHO = 1,
        SYS_SUM4 = 2,
        SYS_COPY_IN = 3,
        SYS_AUTHORIZE = 4,
        SYS_EXC_SUBSCRIBE = 5,
        SYS_EXC_UNSUBSCRIBE = 6,
        SYS_TASK_ID = 7,
        SYS_YIELD = 8,
        SYS_EXC_WAIT = 9,
        SYS_TRAP = 10,
        SYS_CAP_DERIVE = 11,
        SYS_CAP_DROP = 12,
        SYS_MEMSEG_CREATE = 13,
        SYS_MEMSEG_MAP = 14,
        SYS_MEMSEG_UNMAP = 15,
        SYS_MEMSEG_DESTROY = 16,
        SYS_SEND = 17,
        SYS_RECV = 18,
        SYS_TASK_CREATE = 19,
        SYS_TASK_DESTROY = 20,
        SYS_NS_CREATE = 21,
        SYS_NS_BIND = 22,
        SYS_NS_UNBIND = 23,
        SYS_NS_MOUNT = 24,
        SYS_NS_UNMOUNT = 25,
        SYS_NS_RESOLVE = 26,
        SYS_NS_DESTROY = 27,
        SYS_TIME_NOW = 28,
        SYS_TIMER_SET = 29,
        SYS_TIMER_CANCEL = 30,
        SYS_TIMER_DESTROY = 31,
        SYS_AUDIT = 32,
        /// SPEC-113 / §15.5：`dev_ioport_create(base_port, port_count) -> dev_cap`。
        SYS_DEV_IOPORT_CREATE = 33,
        /// SPEC-113 / §15.5：受控端口写 `dev_ioport_out(dev_cap, port, value)`（内核代执行 `outb`）。
        SYS_DEV_IOPORT_OUT = 34,
        /// SPEC-113 / §15.5：受控端口读 `dev_ioport_in(dev_cap, port) -> u8`（内核代执行 `inb`）。
        SYS_DEV_IOPORT_IN = 35,
        /// EXO-4A / §15.8：销毁 `Device` 对象 `dev_destroy(dev_cap)`。
        SYS_DEV_DESTROY = 36,
        /// Stage 5: check the native Prima syscall ABI version before using it.
        SYS_ABI_CHECK = 37,
        /// Stage 5 / M2：设置当前 Task 的 FS base（`%fs` 基址，用于 musl TLS/errno）。
        ///
        /// 追加号；只影响调用者自己的保存帧，切换任务时由调度器恢复各自的基址。
        SYS_TASK_SET_FS_BASE = 38,
        /// 启用单 CPU 物理单调时钟（PIT 校准）和 IRQ0 唤醒；启用调用者用户 IF。
        /// 不推进 COSIR 抽象 Timer 时钟。
        SYS_TIME_ENABLE = 39,
        /// 单 CPU 物理单调 tick（PIT 校准，约100Hz）；不使用抽象 Timer 时钟。
        ///
        /// 与抽象 `SYS_TIME_NOW`（`advance_time` 时钟，仅用于 Timer）分离：本号直接折算
        /// 硬件倒计数，不推进 Timer 状态，故不会提前触发既有 Timer 见证。
        SYS_TIME_TICKS = 40,
        /// Stage 5 / M10：`memseg_protect(task_cap, target_base, page_count, new_access)`（COSIR §7.4）。
        ///
        /// 追加号；把区间内已映射页的逻辑访问集原子地改为 `new_access`，并同步叶 PTE 的
        /// 可写位。新增权限要求调用者持有对应 Segment 的映射授权；入口页不得失去 `X`。
        SYS_MEMSEG_PROTECT = 41,
        /// M15：`wait_object_create() -> wait_cap`（COSIR §21.1.1）。
        SYS_WAIT_CREATE = 42,
        /// M15：`wait_object_observe(cap) -> generation`（COSIR §21.1.2）。
        SYS_WAIT_OBSERVE = 43,
        /// M15：`wait_objects(cap0, gen0, cap1, gen1, mode, timeout)`（COSIR §21.2.1）。
        ///
        /// `cap1 == 0` 表示单一对象；`timeout == u64::MAX` 表示无限。M15a-1 只实现
        /// 立即满足/立即超时；需要阻塞时返回 `EAGAIN`（阻塞路径见后续切片）。
        SYS_WAIT_OBJECTS = 44,
        /// M15：`signal_object(cap) -> generation`（COSIR §21.2.2）。
        SYS_WAIT_SIGNAL = 45,
        /// M15：`wait_object_destroy(cap)`（COSIR §21.1.3）。
        SYS_WAIT_DESTROY = 46,
        /// M15b-2：`thread_create(entry, stack_top, tls) -> thread_task_id`。
        ///
        /// 共享调用者 Process 的地址空间与能力空间；`tls=0` 时继承调用者 FS base。
        SYS_THREAD_CREATE = 47,
        /// M16b：Linux `clone` 语义的共享线程创建（copy 调用者帧、rax=0、rsp=stack）。
        SYS_CLONE = 48,
        /// M16b：退出当前线程（不拆共享地址空间，不报告进程状态）。
        SYS_THREAD_EXIT = 49,
        /// M16b-3：返回当前线程的 `clear_child_tid` 地址（0 = 非线程）。
        SYS_THREAD_CLEAR_TID = 50,
        /// M17b：`fork() -> pid`（COW 地址空间）。
        SYS_FORK = 51,
        /// M17b：`wait4(pid, status_ptr, options, rusage)`。
        SYS_WAIT4 = 53,
        /// M17b：当前进程以 `status` 退出（仅 leader）。
        SYS_PROCESS_EXIT = 54,
        /// M17c：`exec_replace(image_ptr, image_len, argv_ptr, argv_count)`。
        SYS_EXEC_REPLACE = 55,
        /// M18a：`kill(pid, sig)` 最小信号（默认终止）。
        SYS_SIGNAL = 56,
        /// M18b：`sigaction(sig, handler)`（最小：仅登记地址）。
        SYS_SIGACTION = 57,
        /// M18b：从信号 handler 返回（恢复被中断现场）。
        SYS_SIGRETURN = 58,
        /// Current Process leader (selector0) / parent leader (selector1); Task ID stays7.
        SYS_PROCESS_ID = 59,
        /// Receive with authenticated sender/msg-id metadata (five u64 output words).
        SYS_RECV_META = 60,
        /// Destroy a complete Process using its leader TaskDestroy authority.
        SYS_PROCESS_TERMINATE = 61,
        /// Memory-image replacement with argv and envp counts.
        SYS_EXEC_REPLACE_ENV = 62,
        /// Incarnation identity and current self authority for service handshakes.
        SYS_TASK_INFO = 63,
        /// 0: prepare, 1: publish, 2: abort a fork image using its ticket.
        SYS_FORK_TRANSACTION = 64,
        /// Bounded generic user upcall control, mask, delivery and return context.
        SYS_UPCALL_CONTROL = 65,
        /// Atomically unmap a retiring shared thread stack and exit that Task.
        SYS_THREAD_EXIT_UNMAP = 66,
        /// Compare a user word and register a generation wait without a scheduling gap.
        SYS_WAIT_WORD = 67,
        /// Toggle user accessibility while retaining the mapping and its COW state.
        SYS_VSPACE_ACCESS_ENABLE = 68,
        /// Image replacement with explicit retained ranges and entry registers.
        SYS_EXEC_HANDOFF = 69,
        /// Create an IRQ device and a delegable subscription token into two output words.
        SYS_DEV_IRQ_CREATE = 70,
        /// Mask(1)/unmask(0) a held IRQ device; unmask enables caller user IF.
        SYS_DEV_IRQ_MASK = 71,
        /// Block the current Task until a physical monotonic tick deadline; 1=elapsed.
        SYS_TASK_WAIT_DEADLINE = 72,
    }

    /// 按号查名称；未实现、保留号或越界号返回 `None`。
    pub fn name(num: u64) -> Option<&'static str> {
        let mut index = 0;
        while index < NUMBERS.len() {
            if NUMBERS[index].0 == num {
                return Some(NUMBERS[index].1);
            }
            index += 1;
        }
        None
    }
}

/// Native syscall ABI version; independent of `prima-runtime`'s source API version.
pub mod native_abi {
    pub const MAJOR: u64 = 1;
    pub const MINOR: u64 = 0;
    pub const VERSION: u64 = (MAJOR << 32) | MINOR;
}

/// 初始栈 `auxv` 的键号、键组顺序与取值口径（S6-M2 ⑤）。
///
/// 本仓库有**两条**写 `auxv` 的路径：内核 `SYS_EXEC`/`SYS_EXEC_HANDOFF` 的初始栈，以及
/// `prima-servicemgr` 的 ring3 C 装载。它们此前各自写死键号，漂移只能靠人工核对。本模块
/// 只收敛**键号、键组与取值表**：两条路径仍然选择不同的键组（[`KERNEL_KEYS`] 与
/// [`CORE_KEYS`]），但键号不再是两份字面量。
///
/// 键号与 Linux x86_64 `asm/auxvec.h` 一致。这是 **ELF 装载 ABI 事实**，不是内核的 Linux
/// 语义（S6-X3）：内核按记录的常量写入键值，不解释它们的含义。
pub mod auxv {
    pub const PHDR: u64 = 3;
    pub const PHENT: u64 = 4;
    pub const PHNUM: u64 = 5;
    pub const PAGESZ: u64 = 6;
    pub const BASE: u64 = 7;
    pub const FLAGS: u64 = 8;
    pub const ENTRY: u64 = 9;
    pub const UID: u64 = 11;
    pub const EUID: u64 = 12;
    pub const GID: u64 = 13;
    pub const EGID: u64 = 14;
    pub const HWCAP: u64 = 16;
    pub const CLKTCK: u64 = 17;
    pub const SECURE: u64 = 23;
    pub const RANDOM: u64 = 25;
    pub const EXECFN: u64 = 31;

    /// `AT_RANDOM` 指向的 16 字节块。**公开常量，不是熵**：D13 裁决把它定为兼容字段，
    /// 不构成机密性、ASLR 或栈保护主张。
    pub const RANDOM_BYTES: [u8; 16] = *b"PRIMA-RND-16BYTE";

    /// 内核 exec 路径写入的固定键（`AT_HWCAP`/`AT_RANDOM`/`AT_EXECFN` 另行追加）。
    pub const STATIC_KEYS: [u64; 13] = [
        PAGESZ, PHDR, PHENT, PHNUM, BASE, ENTRY, FLAGS, SECURE, UID, EUID, GID, EGID, CLKTCK,
    ];

    /// 内核 exec 路径的完整键序（16 项；`AT_NULL` 由栈构造器自行追加）。
    pub const KERNEL_KEYS: [u64; 16] = [
        PAGESZ, PHDR, PHENT, PHNUM, BASE, ENTRY, FLAGS, SECURE, UID, EUID, GID, EGID, CLKTCK,
        HWCAP, RANDOM, EXECFN,
    ];

    /// `prima-servicemgr` C 装载路径的键组（M12 冻结子集，不含身份/时钟/HWCAP/RANDOM）。
    pub const CORE_KEYS: [u64; 7] = [PAGESZ, PHDR, PHENT, PHNUM, BASE, ENTRY, EXECFN];

    /// 装载后才知道的键值。字符串指针类（[`EXECFN`]）与 [`RANDOM`] 由调用方在栈布局
    /// 确定后填入；其余键取本表下方的固定口径。
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub struct Values {
        pub pagesz: u64,
        pub phdr: u64,
        pub phent: u64,
        pub phnum: u64,
        pub base: u64,
        pub entry: u64,
        pub execfn: u64,
        pub random: u64,
    }

    impl Values {
        /// 无栈依赖的初值（`AT_EXECFN`/`AT_RANDOM` 先置 0，由调用方补齐）。
        pub const fn new(pagesz: u64, phdr: u64, phent: u64, phnum: u64, entry: u64) -> Self {
            Self {
                pagesz,
                phdr,
                phent,
                phnum,
                base: 0,
                entry,
                execfn: 0,
                random: 0,
            }
        }
    }

    /// 某个键的值；`None` 表示本层不提供该键。
    ///
    /// 固定口径：`AT_FLAGS`/`AT_SECURE`/`AT_UID`/`AT_EUID`/`AT_GID`/`AT_EGID` 为 0
    /// （无特权、无 setuid），`AT_HWCAP` 为 0（不宣告任何可选 CPU 特性，libc 仍须用
    /// `cpuid` 判定，不得把 0 读成"特性缺失"的证明），`AT_CLKTCK` 为 100。
    ///
    /// 明确不提供：`AT_PLATFORM`（需要额外栈上字符串）、`AT_SYSINFO_EHDR`（无 vDSO）。
    pub const fn value(values: &Values, key: u64) -> Option<u64> {
        match key {
            PAGESZ => Some(values.pagesz),
            PHDR => Some(values.phdr),
            PHENT => Some(values.phent),
            PHNUM => Some(values.phnum),
            BASE => Some(values.base),
            ENTRY => Some(values.entry),
            EXECFN => Some(values.execfn),
            RANDOM => Some(values.random),
            FLAGS | SECURE | UID | EUID | GID | EGID | HWCAP => Some(0),
            CLKTCK => Some(100),
            _ => None,
        }
    }

    /// [`CORE_KEYS`] 的 `(键, 值)` 序列，供 ring3 装载路径逐对写入用户栈。
    pub const fn core_pairs(values: &Values) -> [(u64, u64); 7] {
        [
            (PAGESZ, values.pagesz),
            (PHDR, values.phdr),
            (PHENT, values.phent),
            (PHNUM, values.phnum),
            (BASE, values.base),
            (ENTRY, values.entry),
            (EXECFN, values.execfn),
        ]
    }
}
#[cfg(test)]
mod tests {
    use super::syscall;

    /// D17 冻结台账：`1..=72` 已实现号的逐号映射。
    ///
    /// 这张表**故意与实现分开写**：宏生成的 [`syscall::NUMBERS`] 若被重编号（例如把
    /// `SYS_WAIT4` 从 53 挪到 73），实现内部仍然自洽，只有本表会失败。改号必须同时改
    /// 这里，并按 D17 留下裁决与迁移记录。
    const FROZEN: [(u64, &str); 71] = [
        (1, "SYS_ECHO"),
        (2, "SYS_SUM4"),
        (3, "SYS_COPY_IN"),
        (4, "SYS_AUTHORIZE"),
        (5, "SYS_EXC_SUBSCRIBE"),
        (6, "SYS_EXC_UNSUBSCRIBE"),
        (7, "SYS_TASK_ID"),
        (8, "SYS_YIELD"),
        (9, "SYS_EXC_WAIT"),
        (10, "SYS_TRAP"),
        (11, "SYS_CAP_DERIVE"),
        (12, "SYS_CAP_DROP"),
        (13, "SYS_MEMSEG_CREATE"),
        (14, "SYS_MEMSEG_MAP"),
        (15, "SYS_MEMSEG_UNMAP"),
        (16, "SYS_MEMSEG_DESTROY"),
        (17, "SYS_SEND"),
        (18, "SYS_RECV"),
        (19, "SYS_TASK_CREATE"),
        (20, "SYS_TASK_DESTROY"),
        (21, "SYS_NS_CREATE"),
        (22, "SYS_NS_BIND"),
        (23, "SYS_NS_UNBIND"),
        (24, "SYS_NS_MOUNT"),
        (25, "SYS_NS_UNMOUNT"),
        (26, "SYS_NS_RESOLVE"),
        (27, "SYS_NS_DESTROY"),
        (28, "SYS_TIME_NOW"),
        (29, "SYS_TIMER_SET"),
        (30, "SYS_TIMER_CANCEL"),
        (31, "SYS_TIMER_DESTROY"),
        (32, "SYS_AUDIT"),
        (33, "SYS_DEV_IOPORT_CREATE"),
        (34, "SYS_DEV_IOPORT_OUT"),
        (35, "SYS_DEV_IOPORT_IN"),
        (36, "SYS_DEV_DESTROY"),
        (37, "SYS_ABI_CHECK"),
        (38, "SYS_TASK_SET_FS_BASE"),
        (39, "SYS_TIME_ENABLE"),
        (40, "SYS_TIME_TICKS"),
        (41, "SYS_MEMSEG_PROTECT"),
        (42, "SYS_WAIT_CREATE"),
        (43, "SYS_WAIT_OBSERVE"),
        (44, "SYS_WAIT_OBJECTS"),
        (45, "SYS_WAIT_SIGNAL"),
        (46, "SYS_WAIT_DESTROY"),
        (47, "SYS_THREAD_CREATE"),
        (48, "SYS_CLONE"),
        (49, "SYS_THREAD_EXIT"),
        (50, "SYS_THREAD_CLEAR_TID"),
        (51, "SYS_FORK"),
        (53, "SYS_WAIT4"),
        (54, "SYS_PROCESS_EXIT"),
        (55, "SYS_EXEC_REPLACE"),
        (56, "SYS_SIGNAL"),
        (57, "SYS_SIGACTION"),
        (58, "SYS_SIGRETURN"),
        (59, "SYS_PROCESS_ID"),
        (60, "SYS_RECV_META"),
        (61, "SYS_PROCESS_TERMINATE"),
        (62, "SYS_EXEC_REPLACE_ENV"),
        (63, "SYS_TASK_INFO"),
        (64, "SYS_FORK_TRANSACTION"),
        (65, "SYS_UPCALL_CONTROL"),
        (66, "SYS_THREAD_EXIT_UNMAP"),
        (67, "SYS_WAIT_WORD"),
        (68, "SYS_VSPACE_ACCESS_ENABLE"),
        (69, "SYS_EXEC_HANDOFF"),
        (70, "SYS_DEV_IRQ_CREATE"),
        (71, "SYS_DEV_IRQ_MASK"),
        (72, "SYS_TASK_WAIT_DEADLINE"),
    ];

    /// 台账本身必须是严格升序且无重复，否则 [`syscall::name`] 的线性查找与证据工具
    /// 的渲染顺序都不可信。
    #[test]
    fn numbers_are_unique_and_ascending() {
        let mut previous = 0;
        for &(number, name) in syscall::NUMBERS {
            assert!(number > previous, "{name} = {number} 不是升序");
            previous = number;
        }
    }

    /// 逐号钉死：任何重编号、漏登记或提前预留都会在这里失败。
    #[test]
    fn numbers_match_the_frozen_d17_ledger() {
        assert_eq!(syscall::NUMBERS, &FROZEN[..]);
    }

    /// `0` 是保留号（确定性 `EINVAL`），`52` 保留、`73..=78` 尚未实现（D17：不预留）。
    #[test]
    fn zero_reserved_and_unimplemented_numbers_are_absent() {
        assert!(syscall::name(0).is_none());
        assert!(syscall::NUMBERS.iter().all(|&(number, _)| number >= 1));
        for reserved in syscall::RESERVED {
            assert!(
                syscall::name(reserved).is_none(),
                "保留号 {reserved} 不得实现"
            );
        }
        for pending in 73..=78 {
            assert!(
                syscall::name(pending).is_none(),
                "追加号 {pending} 只能在实现时分配（D17）"
            );
        }
        assert!(syscall::name(u64::MAX).is_none());
    }

    /// 名称查询覆盖全部已实现号，且不把相邻号当成已实现。
    #[test]
    fn name_round_trips_over_the_ledger() {
        for &(number, name) in syscall::NUMBERS {
            assert_eq!(syscall::name(number), Some(name));
            assert_eq!(name, name.trim());
            assert!(name.starts_with("SYS_"), "{name} 必须以 SYS_ 开头");
        }
        assert_eq!(syscall::name(1), Some("SYS_ECHO"));
        assert_eq!(syscall::name(72), Some("SYS_TASK_WAIT_DEADLINE"));
        assert!(syscall::name(99).is_none());
    }
}

#[cfg(test)]
mod auxv_tests {
    use super::auxv;

    /// 键号与 Linux x86_64 `asm/auxvec.h` 一致；改号会让未修改的 libc 读到错误值。
    #[test]
    fn key_numbers_match_the_frozen_linux_values() {
        assert_eq!(auxv::PHDR, 3);
        assert_eq!(auxv::PHENT, 4);
        assert_eq!(auxv::PHNUM, 5);
        assert_eq!(auxv::PAGESZ, 6);
        assert_eq!(auxv::BASE, 7);
        assert_eq!(auxv::FLAGS, 8);
        assert_eq!(auxv::ENTRY, 9);
        assert_eq!(auxv::UID, 11);
        assert_eq!(auxv::EUID, 12);
        assert_eq!(auxv::GID, 13);
        assert_eq!(auxv::EGID, 14);
        assert_eq!(auxv::HWCAP, 16);
        assert_eq!(auxv::CLKTCK, 17);
        assert_eq!(auxv::SECURE, 23);
        assert_eq!(auxv::RANDOM, 25);
        assert_eq!(auxv::EXECFN, 31);
        assert_eq!(auxv::RANDOM_BYTES.len(), 16);
    }

    /// 内核键序必须是 `STATIC_KEYS` 加三项追加键；`CORE_KEYS` 是它的子集（不含
    /// 身份/时钟/HWCAP/RANDOM），两条装载路径因此不会各自漂移。
    #[test]
    fn key_groups_are_consistent() {
        assert_eq!(auxv::KERNEL_KEYS[..13], auxv::STATIC_KEYS);
        assert_eq!(
            auxv::KERNEL_KEYS[13..],
            [auxv::HWCAP, auxv::RANDOM, auxv::EXECFN]
        );
        for key in auxv::CORE_KEYS {
            assert!(
                auxv::KERNEL_KEYS.contains(&key),
                "CORE 键 {key} 不在内核键序里"
            );
        }
        for (index, key) in auxv::KERNEL_KEYS.iter().enumerate() {
            assert!(
                !auxv::KERNEL_KEYS[..index].contains(key),
                "内核键序里 {key} 重复"
            );
        }
    }

    /// 取值口径：恒定量固定，栈依赖项来自 `Values`，未提供的键返回 `None`。
    #[test]
    fn values_follow_the_frozen_conventions() {
        let mut values = auxv::Values::new(4096, 0x400040, 56, 9, 0x401000);
        for key in [
            auxv::FLAGS,
            auxv::SECURE,
            auxv::UID,
            auxv::EUID,
            auxv::GID,
            auxv::EGID,
            auxv::HWCAP,
        ] {
            assert_eq!(auxv::value(&values, key), Some(0));
        }
        assert_eq!(auxv::value(&values, auxv::CLKTCK), Some(100));
        assert_eq!(auxv::value(&values, auxv::PAGESZ), Some(4096));
        assert_eq!(auxv::value(&values, auxv::PHDR), Some(0x400040));
        assert_eq!(auxv::value(&values, auxv::PHENT), Some(56));
        assert_eq!(auxv::value(&values, auxv::PHNUM), Some(9));
        assert_eq!(auxv::value(&values, auxv::ENTRY), Some(0x401000));
        assert_eq!(auxv::value(&values, auxv::BASE), Some(0));
        // 栈依赖项在调用方补齐前是 0，不泄漏上一条映像的地址。
        assert_eq!(auxv::value(&values, auxv::EXECFN), Some(0));
        assert_eq!(auxv::value(&values, auxv::RANDOM), Some(0));
        values.execfn = 0x7fff_1000;
        values.random = 0x7fff_2000;
        assert_eq!(auxv::value(&values, auxv::EXECFN), Some(0x7fff_1000));
        assert_eq!(auxv::value(&values, auxv::RANDOM), Some(0x7fff_2000));
        // 未提供的键（AT_PLATFORM=15、AT_SYSINFO_EHDR=33）与越界键。
        assert_eq!(auxv::value(&values, 15), None);
        assert_eq!(auxv::value(&values, 33), None);
        assert_eq!(auxv::value(&values, 0), None);
    }

    /// `core_pairs` 的顺序必须与 `CORE_KEYS` 一致，否则 C 装载路径的键序会悄悄跑偏。
    #[test]
    fn core_pairs_follow_the_core_key_order() {
        let values = auxv::Values::new(4096, 0x400040, 56, 9, 0x401000);
        let pairs = auxv::core_pairs(&values);
        assert_eq!(pairs.len(), auxv::CORE_KEYS.len());
        for (index, (key, _)) in pairs.iter().enumerate() {
            assert_eq!(*key, auxv::CORE_KEYS[index]);
        }
    }
}
