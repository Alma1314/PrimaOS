//! 切片 H/I：syscall ABI —— CPL3 用户代码经 `int 0x80`（DPL3 门）陷入内核，内核按
//! 调用/返回协议分派服务并把返回值写回用户 `rax`，再 `iretq` 回 CPL3 继续执行。
//!
//! 这是 POSIR/COSIR 边界清单第 3 项「syscall 调用/返回协议」的 COSIR 细化实现（对齐
//! 阶段 S6-D2「OS 中立的 syscall / exception 递送机制」），冻结如下约定：
//! - **调用号**：`rax`；`0` 为保留号（确定性 `EINVAL`），未登记号确定性 `ENOSYS`；
//! - **参数寄存器**：`rdi, rsi, rdx, r10, r8, r9`（最多 6 个，SysV 序）；
//! - **返回寄存器**：`rax`；**错误码编码**：负 errno 二补码（`EINVAL=-22`、`EFAULT=-14`、
//!   `ENOSYS=-38`），`>=0` 为成功值；
//! - **进入来源**：硬件压栈的 `CS`（CPL3 应 [`USER_CS`]）逐次记录。
//!
//! 已登记服务：
//! - [`SYS_ECHO`]：返回 `arg0 ^ ECHO_MASK`；
//! - [`SYS_SUM4`]：返回 4 个参数之和（证明多寄存器参数传递）；
//! - [`SYS_COPY_IN`]：`(user_ptr, len)`，校验用户页 present+user 且不跨页后读入并组装
//!   为 u64；坏指针 → `EFAULT`，`len∉[1,8]` → `EINVAL`。
//!
//! **范围边界：** 单核、非抢占、单任务；这是 syscall **递送**，不是抽象模型里带
//! capability/订阅授权语义的 `syscall` 原语；不做 `syscall`/`sysret` MSR 快路径、
//! 不做多任务用户态协议、Linux trace/`COMPAT-TEST` 或 `KERNEL-TRACE`/`REFINEMENT-PROOF`。

use core::sync::atomic::{AtomicU64, Ordering};

use crate::bootstrap::{
    self, AuditError, AuditTrace, AuthError, DropError, ExcAuditCounts, ExcError, ExcKind,
    IMAGE_CAP_HANDLE, INIT_TASK_ID,
};
use crate::exc::SretFrame;
use crate::gdt::USER_CS;
use crate::handle::Handle;
use crate::mem::phys_to_virt;
use crate::namespace::Path;
use crate::rights::{Right, Rights};
use crate::sched;
use crate::user::{USER_BASE_VA, USER_STORE_VALUE, USER_X_VA, USER_Y_VA};
use crate::vspace::{self, PT_ADDR_MASK, PT_PRESENT, PT_USER, PT_WRITABLE};
use prima_ostd::entry::AbiFrame;
use prima_ostd::sync::StaticCell;

pub use prima_abi::syscall::{
    SYS_ABI_CHECK, SYS_AUDIT, SYS_AUTHORIZE, SYS_CAP_DERIVE, SYS_CAP_DROP, SYS_CLONE, SYS_COPY_IN,
    SYS_DEV_DESTROY, SYS_DEV_IOPORT_CREATE, SYS_DEV_IOPORT_IN, SYS_DEV_IOPORT_OUT,
    SYS_DEV_IRQ_CREATE, SYS_DEV_IRQ_MASK, SYS_ECHO, SYS_EXC_SUBSCRIBE, SYS_EXC_UNSUBSCRIBE,
    SYS_EXC_WAIT, SYS_EXEC_HANDOFF, SYS_EXEC_REPLACE, SYS_EXEC_REPLACE_ENV, SYS_FORK,
    SYS_FORK_TRANSACTION, SYS_GATE_ABORT, SYS_GATE_ATTACH, SYS_GATE_REPLY, SYS_GATE_WAIT,
    SYS_MEMSEG_CREATE, SYS_MEMSEG_DESTROY, SYS_MEMSEG_MAP, SYS_MEMSEG_PROTECT, SYS_MEMSEG_UNMAP,
    SYS_NS_BIND, SYS_NS_CREATE, SYS_NS_DESTROY, SYS_NS_MOUNT, SYS_NS_RESOLVE, SYS_NS_UNBIND,
    SYS_NS_UNMOUNT, SYS_PROCESS_EXIT, SYS_PROCESS_ID, SYS_PROCESS_TERMINATE, SYS_RECV,
    SYS_RECV_META, SYS_SEND, SYS_SIGACTION, SYS_SIGNAL, SYS_SIGRETURN, SYS_SUM4, SYS_TASK_CREATE,
    SYS_TASK_DESTROY, SYS_TASK_ID, SYS_TASK_INFO, SYS_TASK_SET_FS_BASE, SYS_TASK_WAIT_DEADLINE,
    SYS_THREAD_CLEAR_TID, SYS_THREAD_CREATE, SYS_THREAD_EXIT, SYS_THREAD_EXIT_UNMAP,
    SYS_TIME_ENABLE, SYS_TIME_NOW, SYS_TIME_TICKS, SYS_TIMER_CANCEL, SYS_TIMER_DESTROY,
    SYS_TIMER_SET, SYS_TRAP, SYS_UPCALL_CONTROL, SYS_VSPACE_ACCESS_ENABLE, SYS_WAIT_CREATE,
    SYS_WAIT_DESTROY, SYS_WAIT_OBJECTS, SYS_WAIT_OBSERVE, SYS_WAIT_SIGNAL, SYS_WAIT_WORD,
    SYS_WAIT4, SYS_YIELD,
};

/// SPEC-082：`timer_cancel` 的状态返回编码。
pub const TIMER_CANCEL_OUTCOME_CANCELLED: u64 = 0;
pub const TIMER_CANCEL_OUTCOME_ALREADY_FIRED: u64 = 1;
pub const TIMER_CANCEL_OUTCOME_ALREADY_CANCELLED: u64 = 2;

/// SPEC-082：Y 回执页中 Timer 各步的字节偏移（紧接 SPEC-081 的 656）。
pub const TIMER_RECEIPT_NOW: u64 = 664;
pub const TIMER_RECEIPT_T1: u64 = 672;
pub const TIMER_RECEIPT_T2: u64 = 680;
pub const TIMER_RECEIPT_T3: u64 = 688;
pub const TIMER_RECEIPT_CANCEL: u64 = 696;
pub const TIMER_RECEIPT_CANCEL_AGAIN: u64 = 704;
pub const TIMER_RECEIPT_DESTROY: u64 = 712;
/// SPEC-082：ring3 设定的第一个 Timer 的 deadline 相对偏移（未来 ⇒ `Pending`）。
pub const TIMER_DEADLINE_DELTA: u64 = 2;
/// SPEC-082：ring3 立即触发用的 deadline（`deadline ≤ now` ⇒ 同操作内 `Fired`，S3-D4）。
pub const TIMER_DEADLINE_IMMEDIATE: u64 = 0;

/// SPEC-083：Y 回执页中 `audit(cap)` 两步的字节偏移（紧接 SPEC-082 的 712）。
pub const AUDIT_RECEIPT_LIVE: u64 = 720;
pub const AUDIT_RECEIPT_STALE: u64 = 728;

/// SPEC-081：路径文本的最大字节数（细化层上限，见设计文档 §0.1）。
pub const MAX_NS_PATH_BYTES: u64 = 64;
/// SPEC-081：路径文本在用户 Y 页内的数据偏移，避开回执与接收缓冲区。
pub const NS_PATH_DATA_OFFSET: usize = 1024;
/// SPEC-081：路径文本数据区的 VA。
pub const NS_PATH_DATA_VA: u64 = crate::user::USER_Y_VA + NS_PATH_DATA_OFFSET as u64;
/// SPEC-081：各路径文本在数据区内的偏移（NUL 分隔的紧凑排布）。
pub const NS_PATH_LIB_OFF: u64 = 0;
pub const NS_PATH_SUB_OFF: u64 = 4;
pub const NS_PATH_MNT_OFF: u64 = 8;
pub const NS_PATH_MNTSUB_OFF: u64 = 12;
pub const NS_PATH_ILLEGAL_OFF: u64 = 20;
pub const NS_PATH_X_OFF: u64 = 25;
pub const NS_PATH_Y_OFF: u64 = 27;
/// SPEC-081：路径文本数据区的字节长度（含各串的 NUL 终止符与尾部对齐）。
pub const NS_PATH_DATA_LEN: usize = 32;
/// SPEC-081：`{Derive, SegMapR}` 位集——ring3 见证里被绑进 Namespace 的可绑定 Capability 权限。
pub const NS_BIND_RIGHTS_BITS: u64 = Rights::from_rights(&[Right::Derive, Right::SegMapR]).bits();
/// SPEC-081：Namespace 根权限位集（`ns_create` 铸出的根 cap，供派生副本用）。
pub const NS_ROOT_RIGHTS_BITS: u64 = crate::bootstrap::NAMESPACE_RIGHTS.bits();

/// SPEC-081：Y 回执页中各步的字节偏移（紧接 SPEC-067 的 496）。
pub const NS_RECEIPT_CREATE_A: u64 = 512;
pub const NS_RECEIPT_CREATE_B: u64 = 520;
pub const NS_RECEIPT_CREATE_C: u64 = 528;
pub const NS_RECEIPT_DERIVE1: u64 = 536;
pub const NS_RECEIPT_DERIVE2: u64 = 544;
pub const NS_RECEIPT_COPY_B: u64 = 552;
pub const NS_RECEIPT_COPY_C: u64 = 560;
pub const NS_RECEIPT_MOUNT_A_B: u64 = 568;
pub const NS_RECEIPT_MOUNT_CYCLE: u64 = 576;
pub const NS_RECEIPT_BIND: u64 = 584;
pub const NS_RECEIPT_BIND_OVERLAP: u64 = 592;
pub const NS_RECEIPT_BIND_SUB: u64 = 600;
pub const NS_RECEIPT_RESOLVE_LIB: u64 = 608;
pub const NS_RECEIPT_MOUNT_INIT_C: u64 = 616;
pub const NS_RECEIPT_RESOLVE_SUB: u64 = 624;
pub const NS_RECEIPT_BAD_PATH: u64 = 632;
pub const NS_RECEIPT_UNBIND: u64 = 640;
pub const NS_RECEIPT_UNMOUNT: u64 = 648;
pub const NS_RECEIPT_DESTROY: u64 = 656;

/// S2-D4：`task_create` 请求块的 qword 个数与字节长度。SPEC-101 增第 11 个 qword（运行期指派的
/// 服务名）。
pub const TASK_CREATE_REQ_QWORDS: usize = 11;
/// S2-D4：`task_create` 请求块的字节长度。
pub const TASK_CREATE_REQ_LEN: u64 = (TASK_CREATE_REQ_QWORDS * 8) as u64;
/// S2-D4：请求块在 task 1 用户栈页内的 VA（可写、user）。
pub const TASK_REQ_VA: u64 = crate::user::USER_STACK_VA + 0x800;
/// S2-D4：接收子任务消息的 out 缓冲（Y 页内，24 字节）。
pub const TASK_RECV_BUF: u64 = USER_Y_VA + 0x300;

/// S2-D4：task 1 新块各步在 Y 回执页内的偏移。
pub const TASK_RECEIPT_STACK_SEG: u64 = 368;
pub const TASK_RECEIPT_CREATE: u64 = 376;
pub const TASK_RECEIPT_BAD_ENTRY: u64 = 384;
pub const TASK_RECEIPT_NO_X: u64 = 392;
pub const TASK_RECEIPT_STACK_MAP: u64 = 400;
pub const TASK_RECEIPT_RECV: u64 = 408;
pub const TASK_RECEIPT_DENY: u64 = 416;
pub const TASK_RECEIPT_DESTROY: u64 = 424;
pub const TASK_RECEIPT_AGAIN: u64 = 432;
pub const TASK_RECEIPT_MAP_DEAD: u64 = 440;

/// SPEC-066：task 1 用**自持**平台 cap 经 `SYS_MEMSEG_MAP` 建立的新映射 VA。
///
/// 与既有映射不相交，且与 `USER_SEG_VA`(+0x5000) 同处一个 2 MiB 区域——复用已存在的 PT 页，
/// 故不新增页表帧。
pub const USER_MAP_WITNESS_VA: u64 = USER_BASE_VA + 0x4000;
/// SPEC-066：非属主负例请求的目标 VA（被 `authorize` 拒绝，不会真的映射）。
pub const USER_MAP_WITNESS_VA2: u64 = USER_BASE_VA + 0x6000;
/// SPEC-066：自持 cap 映射见证的三条回执（Y 页 448/456/464，紧接 S2-D4 的 368..447）。
pub const TASK_RECEIPT_MAP_OWN: u64 = 448;
pub const TASK_RECEIPT_MAP_VALUE: u64 = 456;
pub const TASK_RECEIPT_MAP_OTHER: u64 = 464;

/// SPEC-067：task 4 自毁见证的四条回执（Y 页 472/480/488/496）。
pub const TASK_RECEIPT_TASK4_CAP: u64 = 472;
pub const TASK_RECEIPT_TASK4_STACK: u64 = 480;
pub const TASK_RECEIPT_TASK4_RECV: u64 = 488;
pub const TASK_RECEIPT_TASK4_MAP_DEAD: u64 = 496;

/// S2-D3：用户创建 Segment 的映射 VA（PML4 槽 20 内、与既有区域不相交）。
pub const USER_SEG_VA: u64 = USER_BASE_VA + 0x5000;
/// 访问位集：`R` / `W`（供 `memseg_create`/`map` 的 `access` 参数）。
pub const ACCESS_R_BITS: u64 = 1 << (Right::SegMapR as u32);
pub const ACCESS_W_BITS: u64 = 1 << (Right::SegMapW as u32);
/// `R|W` 段权限。
pub const MEMSEG_RW_BITS: u64 = ACCESS_R_BITS | ACCESS_W_BITS;
/// `Derive` 位（SPEC-092：`memseg_map` 前置要求段 cap 含 `Derive`，故共享端点 cap 须带上）。
pub const ACCESS_DERIVE_BITS: u64 = 1 << (Right::Derive as u32);
/// 用户代码写入新映射页的模式值（证明真实 PTE 可写）。
pub const MEMSEG_PATTERN: u64 = 0xDEC0_DE00_1234_5678;
/// 第一次 `memseg_create` 铸出的根 Capability（槽 6 的第 3 代——能力生命周期已释放两次）。
pub const MEMSEG_CAP1_HANDLE: u64 = 0x0000_0003_0000_0006;
/// 第二次 `memseg_create` 铸出的根 Capability（槽 7 的第 1 代）。
pub const MEMSEG_CAP2_HANDLE: u64 = 0x0000_0001_0000_0007;

/// S2-D3：Y 回执页中 memseg 各步的字节偏移。
pub const MEMSEG_RECEIPT_CREATE: u64 = 240;
pub const MEMSEG_RECEIPT_MAP: u64 = 248;
pub const MEMSEG_RECEIPT_ZERO: u64 = 256;
pub const MEMSEG_RECEIPT_WRITE: u64 = 264;
pub const MEMSEG_RECEIPT_UNMAP: u64 = 272;
pub const MEMSEG_RECEIPT_DESTROY: u64 = 280;
pub const MEMSEG_RECEIPT_STALE: u64 = 288;
pub const MEMSEG_RECEIPT_CREATE2: u64 = 296;
pub const MEMSEG_RECEIPT_MAP2: u64 = 304;
pub const MEMSEG_RECEIPT_ZERO2: u64 = 312;
pub const MEMSEG_RECEIPT_UNMAP2: u64 = 320;
pub const MEMSEG_RECEIPT_DESTROY2: u64 = 328;
/// 未登记号：必须得到确定性 `ENOSYS`。
pub const SYS_BAD: u64 = 0xDEAD;

/// 切片 S：task 1 里共享 Segment 的映射 VA（PML4 槽 20，与既有区两两不交）。
pub const USER_SEG_SHARED_VA: u64 = USER_BASE_VA + 0x6000;
/// 切片 S：task 2 里同一共享 Segment 的映射 VA（PML4 槽 21，task 2 只占 base..+0x2000）。
pub const USER2_SHARED_VA: u64 = crate::user::USER2_BASE_VA + 0x3000;
/// 切片 S：task 2 的接收缓冲（在已映射的可写 Y2 页内，24 字节：cap0/cap1/cap_count）。
pub const USER2_RECV_BUF: u64 = crate::user::USER2_Y_VA + 0x100;
/// 切片 S：task 2 第二次接收的缓冲（阻塞路径，被直投唤醒）。
pub const USER2_RECV_BUF2: u64 = crate::user::USER2_Y_VA + 0x140;
/// S2-D4：task 2 代码尾部的阻塞接收缓冲（S2-D4 块需要 task 2 处于 `BlockedRecv`）。
pub const USER2_RECV_BUF3: u64 = crate::user::USER2_Y_VA + 0x180;
/// S2-D4：任务块结束时唤醒 task 2 的 payload（收尾阶段需要它有 Ready 任务可轮转）。
pub const TASK2_WAKE_PAYLOAD: u64 = 0x5A3C_0000_0000_0002;
/// `SYS_RECV` 的 out 缓冲长度（cap0 + cap1 + cap_count）。
pub const RECV_BUF_LEN: u64 = 24;
/// 切片 S：task 1 写进共享段、task 2 读回的魔数（证明是**同一物理帧**）。
pub const IPC_SHARED_MAGIC: u64 = 0x51A5_0000_0000_0001;
/// 切片 S：直投（阻塞唤醒）路径的 payload。
pub const IPC_DIRECT_PAYLOAD: u64 = 0xD1EC_7000_0000_0002;
/// 切片 S：task 1 首个 `SYS_MEMSEG_CREATE` 的句柄（槽 8 gen1：槽 6/7 已被既有块占用且
/// `memseg_destroy` 不释放 cap 槽，故下一空闲槽为 8）。
pub const SHARED_SEG_CAP_HANDLE: u64 = 0x0000_0001_0000_0008;
/// 切片 S：task 1 的发送凭证（目标 `Task(2)` + `TaskSend`，槽 47）。
pub const TASK2_SEND_CAP: u64 = bootstrap::TASK2_SEND_CAP_HANDLE.encode();
/// 切片 S：task 2 的自管理凭证（目标 `Task(2)` + `TaskManageVSpace`，槽 48）。
pub const TASK2_SELF_CAP: u64 = bootstrap::TASK2_SELF_CAP_HANDLE.encode();
/// S2-D4：`task_create` 返回的 manager cap 句柄（槽 10 gen1：槽 9 已被新块首个
/// `SYS_MEMSEG_CREATE` 的栈段 cap 占用）。
pub const TASK_MANAGER_CAP: u64 = 0x0000_0001_0000_000A;
/// S2-D4：`task_create` 选定的新 Task id（init 与 task 2 之后的最小空闲 id）。
pub const TASK_CREATE_CHILD_ID: u32 = 3;
/// SPEC-109：新 Task 代码段 Capability 的句柄（bring-up 铸在预留槽 62，指向**单一服务映像段**）。
pub const TASK_CODE_CAP: u64 = bootstrap::SERVICE_IMAGE_CAP_HANDLE.encode();
/// S2-D4：作为 `initial_cap0` 移入新 Task 的发送凭证（预留槽 50）。
pub const TASK_SEND_CAP: u64 = bootstrap::TASK_CREATE_SEND_CAP_HANDLE.encode();

/// 切片 S：task 1 新块的 Y 回执偏移。
pub const IPC_RECEIPT_CREATE: u64 = 336;
pub const IPC_RECEIPT_MAP1: u64 = 344;
pub const IPC_RECEIPT_SEND1: u64 = 352;
pub const IPC_RECEIPT_SEND2: u64 = 360;
/// 切片 S：task 2 的 Y2 回执偏移。
pub const IPC2_RECEIPT_SEND_DENIED: u64 = 32;
pub const IPC2_RECEIPT_RECV1: u64 = 40;
pub const IPC2_RECEIPT_CAP0: u64 = 48;
pub const IPC2_RECEIPT_MAGIC: u64 = 56;
pub const IPC2_RECEIPT_MAP2: u64 = 64;
pub const IPC2_RECEIPT_CROSS: u64 = 72;
pub const IPC2_RECEIPT_RECV2: u64 = 80;
pub const IPC2_RECEIPT_CAPCOUNT: u64 = 88;

/// 切片 M：第二个 ring3 任务的 Task id（由 SPEC-050 `platform_init` 创建）。
pub const TASK2_ID: u32 = 2;

/// ABI 参数寄存器个数。
pub const MAX_ARGS: usize = 6;

/// 回显掩码：内核侧常量，用户要拿到正确回显值必须真的完成一次内核往返。
pub const ECHO_MASK: u64 = 0xFFFF_0000_FFFF_0000;
/// 用户传给 [`SYS_ECHO`] 的参数。
pub const SYS_ECHO_ARG: u64 = 0x0123_4567_89AB_CDEF;

/// [`SYS_SUM4`] 的四个参数（各半字节错开，便于逐项对账）。
pub const SUM_ARGS: [u64; 4] = [
    0x1111_1111_1111_1111,
    0x2222_2222_2222_2222,
    0x0404_0404_0404_0404,
    0x0808_0808_0808_0808,
];
/// [`SYS_SUM4`] 的期望返回值。
pub const SUM_EXPECTED: u64 = 0x3F3F_3F3F_3F3F_3F3F;

/// [`SYS_COPY_IN`] 的合法用户缓冲：只读+user 的 X 页，首 8 字节为 [`crate::cow::pattern`]`(0)`。
pub const COPY_PTR: u64 = USER_X_VA;
/// [`SYS_COPY_IN`] 的读入长度（一次 u64）。
pub const COPY_LEN: u64 = 8;
/// [`SYS_COPY_IN`] 读合法缓冲的期望返回值。
///
/// X 页在切片 G 已 COW 为私有帧且首 qword 被重试的 store 写成 [`USER_STORE_VALUE`]，
/// 故内核经用户映射读到的是该私有值（而非原共享 `pattern(0)`）——这本身证明内核读的是
/// **COW 后的用户私有页**。
pub const COPY_EXPECTED: u64 = USER_STORE_VALUE;
/// [`SYS_COPY_IN`] 的坏指针：与用户页同 PML4 槽但中间层缺失，应 `EFAULT`。
pub const BAD_PTR: u64 = USER_BASE_VA + 0x20_0000;

/// 授权查询使用的 Capability：Bootstrap 初始 image cap（持有者拥有 `SegMapR` 但无 `SegMapW`）。
pub const AUTHORIZE_CAP: u64 = IMAGE_CAP_HANDLE.encode();
/// 成功授予时返回的凭据（即被授权的 cap 句柄）。
pub const AUTHORIZE_GRANTED: u64 = IMAGE_CAP_HANDLE.encode();
/// 权限编码：`SegMapR`（init image cap 具备）。
pub const RIGHT_SEGMAPR: u64 = 1;
/// 权限编码：`SegMapW`（init image cap 不具备，用于见证拒绝）。
pub const RIGHT_SEGMAPW: u64 = 2;
/// 授权查询用的**格式合法但从未分配**的句柄（槽 60、generation 1）→ 见证 `ENOENT`。
///
/// S2-D1 后裸整数 `999` 不再是合法句柄（其 generation 为 0 → `EINVAL`），故改用未分配槽位。
pub const UNSET_CAP: u64 = (1u64 << 32) | 60;

/// 切片 L：初始化创建的 `page_fault_source` Capability 句柄（镜像常量，见 bootstrap.rs）。
pub const EXC_PAGE_FAULT_CAP: u64 = bootstrap::EXC_PAGE_FAULT_HANDLE.encode();
/// 切片 L：初始化创建的 `syscall_trap_source` Capability 句柄。
pub const EXC_SYSCALL_TRAP_CAP: u64 = bootstrap::EXC_SYSCALL_TRAP_HANDLE.encode();
/// 切片 L：异常种类编码（镜像 `bootstrap::kind_code`）。
pub const EXC_KIND_PAGEFAULT: u64 = 1;
/// 切片 L：异常种类编码（镜像 `bootstrap::kind_code`）。
pub const EXC_KIND_SYSCALLTRAP: u64 = 2;
/// 切片 L：预定义 `page_fault_source` 的 ID（订阅成功返回的凭据）。
pub const EXC_PAGE_FAULT_SOURCE: u64 = bootstrap::PAGE_FAULT_SOURCE_ID as u64;
/// 切片 L：预定义 `syscall_trap_source` 的 ID。
pub const EXC_SYSCALL_TRAP_SOURCE: u64 = bootstrap::SYSCALL_TRAP_SOURCE_ID as u64;
/// 切片 L：平台初始化那次 page_fault 通知的期望 payload。
pub const EXC_PAGE_FAULT_PAYLOAD: u64 = EXC_PAGE_FAULT_SOURCE | (EXC_KIND_PAGEFAULT << 16);
/// 切片 N：`syscall_trap_source` 的 `SyscallTrap` 通知期望 payload。
pub const EXC_SYSCALL_TRAP_PAYLOAD: u64 = EXC_SYSCALL_TRAP_SOURCE | (EXC_KIND_SYSCALLTRAP << 16);

/// errno 编码（Linux 兼容负值）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Errno {
    /// 找不到对象（不存在的 Capability）。
    /// POSIX `ENOEXEC`：不可执行的镜像格式。
    Exec = 8,
    Interrupted = 4,
    NoEnt = 2,
    /// POSIX `ECHILD`：没有可等待的子进程。
    Child = 10,
    /// 切片 V：暂时不可用——自毁要求存在其他 Ready 任务，当前没有。
    Again = 11,
    /// 权限不足（缺权限 / 非持有者 / 已失效）。
    Access = 13,
    /// 用户指针不可读（未映射或非 user 页）。
    Fault = 14,
    /// SPEC-081：路径冲突——`ns_bind`/`ns_mount` 的 `Overlap`（`EEXIST`）。
    Exist = 17,
    /// SPEC-081：路径中间分量不是目录（`ns_resolve` 的 `NotDirectory`）。
    NotDir = 20,
    /// 参数非法（保留号、长度越界等）。
    Invalid = 22,
    /// 空间不足（队列/Audit 容量已满）。
    NoSpace = 28,
    /// 未登记的服务号。
    NoSys = 38,
}

/// 把 errno 编码为写回 `rax` 的二补码负值。
pub const fn encode_errno(errno: Errno) -> u64 {
    (-(errno as i64)) as u64
}

/// SPEC-081：`EEXIST` 的负值编码（`namespace_ready` 见证与协议断言共用）。
pub const ERRNO_EXIST: u64 = encode_errno(Errno::Exist);
/// SPEC-081：`EINVAL` 的负值编码（同上）。
pub const ERRNO_INVALID: u64 = encode_errno(Errno::Invalid);

/// errno 的稳定名字（协议/日志用）。
pub const fn errno_name(errno: Errno) -> &'static str {
    match errno {
        Errno::Exec => "ENOEXEC",
        Errno::Interrupted => "EINTR",
        Errno::NoEnt => "ENOENT",
        Errno::Child => "ECHILD",
        Errno::Again => "EAGAIN",
        Errno::Access => "EACCES",
        Errno::Fault => "EFAULT",
        Errno::Exist => "EEXIST",
        Errno::NotDir => "ENOTDIR",
        Errno::Invalid => "EINVAL",
        Errno::NoSpace => "ENOSPC",
        Errno::NoSys => "ENOSYS",
    }
}

/// S2-D2：权限编码 `Derive`（`SYS_AUTHORIZE` 的 right code）。
pub const RIGHT_DERIVE: u64 = 0;
/// S2-D2：`cap_derive` 请求权限位集——仅 `Derive`（`Right::Derive` 是第 0 位）。
pub const RIGHTS_DERIVE_BITS: u64 = 1;
/// S2-D2：`cap_derive` 越权请求位集——`SegMapW`（不在 image cap 权限内）。
pub const RIGHTS_SEGMAPW_BITS: u64 = 1 << (Right::SegMapW as u32);

/// S2-D2：第一次派生得到的子句柄（槽 6、generation 1；槽 1..5 已被初始根与异常源占用）。
pub const CAP_CHILD1_HANDLE: u64 = 0x0000_0001_0000_0006;
/// S2-D2：槽 6 释放后复用得到的子句柄（generation 2）。
pub const CAP_CHILD2_HANDLE: u64 = 0x0000_0002_0000_0006;

/// S2-D2：Y 回执页中能力生命周期各步的字节偏移。
pub const CAP_RECEIPT_CHILD1: u64 = 168;
pub const CAP_RECEIPT_ESCALATE: u64 = 176;
pub const CAP_RECEIPT_DROP: u64 = 184;
pub const CAP_RECEIPT_DROP_AGAIN: u64 = 192;
pub const CAP_RECEIPT_CHILD2: u64 = 200;
pub const CAP_RECEIPT_STALE: u64 = 208;
pub const CAP_RECEIPT_GRANT: u64 = 216;
pub const CAP_RECEIPT_DROP2: u64 = 224;
pub const CAP_RECEIPT_STRUCTURAL: u64 = 232;

/// 权限编码到 [`Right`] 的映射（纯函数，host 可测）。
pub const fn right_of(code: u64) -> Option<Right> {
    match code {
        RIGHT_DERIVE => Some(Right::Derive),
        RIGHT_SEGMAPR => Some(Right::SegMapR),
        RIGHT_SEGMAPW => Some(Right::SegMapW),
        _ => None,
    }
}

/// S2-D2：能力生命周期的用户态回执（从 Y 回执页读回；单核 boot 早期无并发）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapWitness {
    pub child1: u64,
    pub escalate: u64,
    pub drop: u64,
    pub drop_again: u64,
    pub child2: u64,
    pub stale: u64,
    pub grant: u64,
    pub drop2: u64,
    pub structural: u64,
}

impl CapWitness {
    /// 纯判定：派生 → 越权拒绝 → drop → 重复 drop → 槽位复用 → stale 拒绝 → 授权 → 结构性根保护。
    pub fn evidence_ok(&self) -> bool {
        self.child1 == CAP_CHILD1_HANDLE
            && self.escalate == encode_errno(Errno::Access)
            && self.drop == 0
            && self.drop_again == encode_errno(Errno::NoEnt)
            && self.child2 == CAP_CHILD2_HANDLE
            && self.child2 != self.child1
            && self.stale == encode_errno(Errno::NoEnt)
            && self.grant == CAP_CHILD2_HANDLE
            && self.drop2 == 0
            && self.structural == encode_errno(Errno::Access)
    }
}

fn read_user_qword(va: u64) -> u64 {
    prima_ostd::mm::read_u64(va)
}

/// 读取能力生命周期回执（`user_trap_exit` 中在 task 1 的根下调用）。
pub fn cap_witness() -> CapWitness {
    let base = USER_Y_VA;
    CapWitness {
        child1: read_user_qword(base + CAP_RECEIPT_CHILD1),
        escalate: read_user_qword(base + CAP_RECEIPT_ESCALATE),
        drop: read_user_qword(base + CAP_RECEIPT_DROP),
        drop_again: read_user_qword(base + CAP_RECEIPT_DROP_AGAIN),
        child2: read_user_qword(base + CAP_RECEIPT_CHILD2),
        stale: read_user_qword(base + CAP_RECEIPT_STALE),
        grant: read_user_qword(base + CAP_RECEIPT_GRANT),
        drop2: read_user_qword(base + CAP_RECEIPT_DROP2),
        structural: read_user_qword(base + CAP_RECEIPT_STRUCTURAL),
    }
}

/// S2-D3：`memseg_*` 错误到 errno 的映射（纯函数，host 可测）。
pub const fn memseg_errno(error: crate::bootstrap::MemsegError) -> Errno {
    use crate::bootstrap::MemsegError as E;
    match error {
        E::ZeroSize | E::InvalidAccess | E::MalformedHandle | E::WrongRightType => Errno::Invalid,
        E::NoSuchCapability
        | E::StaleHandle
        | E::NotInstalled
        | E::CapacityExceeded
        | E::InvalidTask
        | E::NotMapped => Errno::NoEnt,
        E::NotHolder
        | E::MissingRight
        | E::MissingMapRight
        | E::RightsExceeded
        | E::TargetDead
        | E::Overlap
        | E::EntryMapping
        | E::StillMapped => Errno::Access,
        E::OutOfBounds => Errno::Fault,
        E::NoMemory | E::AuditFull => Errno::NoSpace,
    }
}

/// 切片 S：IPC 错误到 errno 的映射（纯函数，host 可测）。
pub const fn ipc_errno(error: bootstrap::IpcError) -> Errno {
    use bootstrap::IpcError as E;
    match error {
        E::NoSuchTask | E::InvalidCapability | E::QueueEmpty => Errno::NoEnt,
        E::NotAuthorized | E::CapabilityHolderMismatch | E::StructuralCap => Errno::Access,
        E::TooManyCapabilities => Errno::Invalid,
        E::AuditFull => Errno::NoSpace,
        E::QueueFull => Errno::Again,
    }
}

/// 授权失败原因到 errno 的映射（纯函数，host 可测）。
///
/// S2-D1：句柄编码非法 → `EINVAL`；槽位从未分配 / 已释放 / generation 落后（stale）→
/// `ENOENT`（对用户不可区分，避免泄露槽位复用信息）；非持有者 / 缺权限 / 目标已销毁 →
/// `EACCES`（目标已销毁即 stale token，不可授权）。
pub const fn auth_errno(error: AuthError) -> Errno {
    match error {
        AuthError::MalformedHandle => Errno::Invalid,
        AuthError::NoSuchCapability | AuthError::StaleHandle | AuthError::NotInstalled => {
            Errno::NoEnt
        }
        AuthError::NotHolder | AuthError::MissingRight | AuthError::TargetDead => Errno::Access,
    }
}

/// 切片 K：授权查询服务——查 Bootstrap 授权表并编码结果。
fn authorize_call(raw_cap: u64, right_code: u64) -> u64 {
    let Some(right) = right_of(right_code) else {
        return encode_errno(Errno::Invalid);
    };
    let cap = match decode_caller_handle(raw_cap) {
        Ok(cap) => cap,
        Err(error) => return encode_errno(auth_errno(error)),
    };
    match bootstrap::authorize(sched::current(), cap, right) {
        Ok(granted) => encode_caller_handle(granted),
        Err(error) => encode_errno(auth_errno(error)),
    }
}

/// 切片 L：异常种类编码到 [`ExcKind`] 的映射（纯函数，host 可测）。
pub const fn kind_of(code: u64) -> Option<ExcKind> {
    match code {
        EXC_KIND_PAGEFAULT => Some(ExcKind::PageFault),
        EXC_KIND_SYSCALLTRAP => Some(ExcKind::SyscallTrap),
        7 => Some(ExcKind::ExternalInterrupt),
        _ => None,
    }
}

/// 切片 L：订阅错误到 errno 的映射（纯函数，host 可测）。
pub const fn exc_errno(error: ExcError) -> Errno {
    match error {
        ExcError::NoSuchSource | ExcError::NoSuchTask | ExcError::NotInstalled => Errno::NoEnt,
        ExcError::NotHolder
        | ExcError::MissingRight
        | ExcError::AlreadySubscribed
        | ExcError::NotSubscribed => Errno::Access,
        ExcError::InvalidKind | ExcError::KindMismatch => Errno::Invalid,
        ExcError::Full | ExcError::AuditFull => Errno::NoSpace,
        // §12.5 前置 4：调用者不是 Running——对于用户 syscall 而言等价于"当前任务状态不允许"。
        ExcError::NotRunning => Errno::Again,
    }
}

/// 切片 L：订阅服务——经全局授权表登记订阅。
fn exc_subscribe_call(raw_cap: u64, kind_code: u64) -> u64 {
    let Some(kind) = kind_of(kind_code) else {
        return encode_errno(Errno::Invalid);
    };
    let cap = match decode_caller_handle(raw_cap) {
        Ok(cap) => cap,
        Err(_) => return encode_errno(Errno::Invalid),
    };
    match bootstrap::exc_subscribe(sched::current(), cap, kind) {
        Ok(source_id) => source_id as u64,
        Err(error) => encode_errno(exc_errno(error)),
    }
}

/// 切片 L：取消订阅服务——经全局授权表移除订阅。
fn exc_unsubscribe_call(raw_cap: u64) -> u64 {
    let cap = match decode_caller_handle(raw_cap) {
        Ok(cap) => cap,
        Err(_) => return encode_errno(Errno::Invalid),
    };
    match bootstrap::exc_unsubscribe(sched::current(), cap) {
        Ok(source_id) => source_id as u64,
        Err(error) => encode_errno(exc_errno(error)),
    }
}

/// S2-D2：`cap_derive` 的 errno 映射。
pub const fn derive_errno(error: bootstrap::DeriveError) -> Errno {
    match error {
        bootstrap::DeriveError::MalformedHandle | bootstrap::DeriveError::WrongRightType => {
            Errno::Invalid
        }
        bootstrap::DeriveError::NoSuchParent
        | bootstrap::DeriveError::StaleHandle
        | bootstrap::DeriveError::CapacityExceeded => Errno::NoEnt,
        bootstrap::DeriveError::ParentNotHeld
        | bootstrap::DeriveError::TargetDead
        | bootstrap::DeriveError::MissingDerive
        | bootstrap::DeriveError::RightsEscalation => Errno::Access,
        bootstrap::DeriveError::AuditFull => Errno::NoSpace,
    }
}

fn decode_caller_handle(raw: u64) -> Result<Handle, AuthError> {
    bootstrap::decode_handle(raw)
        .map(|local| bootstrap::resolve_user_handle(sched::current(), local))
}

fn encode_caller_handle(token: Handle) -> u64 {
    bootstrap::expose_user_handle(sched::current(), token).encode()
}

/// S2-D2：`cap_drop` 的 errno 映射。
pub const fn drop_errno(error: DropError) -> Errno {
    match error {
        DropError::MalformedHandle => Errno::Invalid,
        DropError::NoSuchCapability | DropError::StaleHandle | DropError::NotInstalled => {
            Errno::NoEnt
        }
        DropError::NotHolder | DropError::StructuralCap => Errno::Access,
        DropError::AuditFull => Errno::NoSpace,
    }
}

/// S2-D2：`cap_derive` 服务——`args[0]=parent handle`、`args[1]=rights bitset`。
fn cap_derive_call(raw_parent: u64, rights_bits: u64) -> u64 {
    let Ok(parent) = decode_caller_handle(raw_parent) else {
        return encode_errno(Errno::Invalid);
    };
    let rights = Rights::from_bits(rights_bits);
    match bootstrap::derive_capability(sched::current(), parent, rights) {
        Ok(child) => encode_caller_handle(child),
        Err(error) => encode_errno(derive_errno(error)),
    }
}

/// S2-D2：`cap_drop` 服务——`args[0]=handle`。
fn cap_drop_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::drop_capability(sched::current(), cap) {
        Ok(()) => 0,
        Err(error) => encode_errno(drop_errno(error)),
    }
}

// ---------------------------------------------------------------------------
// SPEC-081：§11 Namespace 路径操作的 syscall 面
// ---------------------------------------------------------------------------

/// SPEC-081：把 `NsError` 映射到 errno（与规范错误名的语义对应）。
fn ns_errno(error: bootstrap::NsError) -> Errno {
    use bootstrap::NsError;
    match error {
        NsError::InvalidCaller
        | NsError::MissingRight(_)
        | NsError::NotHeld
        | NsError::TargetDead
        | NsError::StructuralCap
        | NsError::NotDelegable
        | NsError::RightsExceeded => Errno::Access,
        NsError::InvalidCap
        | NsError::InvalidNamespace
        | NsError::MountTargetDead
        | NsError::NotFound => Errno::NoEnt,
        NsError::MountCycle | NsError::WrongRightType => Errno::Invalid,
        NsError::NotDirectory => Errno::NotDir,
        NsError::Overlap => Errno::Exist,
        NsError::CapacityExceeded | NsError::AuditFull => Errno::NoSpace,
    }
}

/// SPEC-081：从用户缓冲读入路径文本并按 §3.1 解析。
///
/// 复用 [`copy_in`] 的 present+user 与不跨页校验（故跨页路径文本得 `EFAULT`）；语法非法得
/// `EINVAL`，并在内核侧记一次 `invalid_path` 观测。
fn read_user_path(ptr: u64, len: u64) -> Result<Path, Errno> {
    if len == 0 || len > MAX_NS_PATH_BYTES {
        bootstrap::note_ns_invalid_path();
        return Err(Errno::Invalid);
    }
    let mut bytes = [0u8; MAX_NS_PATH_BYTES as usize];
    let mut offset = 0u64;
    while offset < len {
        let chunk = core::cmp::min(8, len - offset);
        let value = copy_in(ptr + offset, chunk)?;
        let mut index = 0u64;
        while index < chunk {
            bytes[(offset + index) as usize] = (value >> (8 * index)) as u8;
            index += 1;
        }
        offset += chunk;
    }
    match Path::parse(&bytes[..len as usize]) {
        Ok(path) => Ok(path),
        Err(_) => {
            bootstrap::note_ns_invalid_path();
            Err(Errno::Invalid)
        }
    }
}

/// SPEC-081：`ns_create` 服务。
fn ns_create_call() -> u64 {
    match bootstrap::ns_create(sched::current()) {
        Ok(handle) => encode_caller_handle(handle),
        Err(error) => encode_errno(ns_errno(error)),
    }
}

/// SPEC-081：`ns_bind` 服务——`args[0]=ns cap`、`args[1]=path ptr`、`args[2]=path len`、
/// `args[3]=cap`。
fn ns_bind_call(raw_ns_cap: u64, path_ptr: u64, path_len: u64, raw_cap: u64) -> u64 {
    let Ok(ns_cap) = decode_caller_handle(raw_ns_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let path = match read_user_path(path_ptr, path_len) {
        Ok(path) => path,
        Err(errno) => return encode_errno(errno),
    };
    match bootstrap::ns_bind(sched::current(), ns_cap, path, cap) {
        Ok(()) => 0,
        Err(error) => encode_errno(ns_errno(error)),
    }
}

/// SPEC-081：`ns_unbind` 服务——`args[0]=ns cap`、`args[1]=path ptr`、`args[2]=path len`。
fn ns_unbind_call(raw_ns_cap: u64, path_ptr: u64, path_len: u64) -> u64 {
    let Ok(ns_cap) = decode_caller_handle(raw_ns_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let path = match read_user_path(path_ptr, path_len) {
        Ok(path) => path,
        Err(errno) => return encode_errno(errno),
    };
    match bootstrap::ns_unbind(sched::current(), ns_cap, path) {
        Ok(handle) => encode_caller_handle(handle),
        Err(error) => encode_errno(ns_errno(error)),
    }
}

/// SPEC-081：`ns_mount` 服务——`args[0]=destination cap`、`args[1]=path ptr`、
/// `args[2]=path len`、`args[3]=source cap`。
fn ns_mount_call(raw_dst_cap: u64, path_ptr: u64, path_len: u64, raw_src_cap: u64) -> u64 {
    let Ok(destination_cap) = decode_caller_handle(raw_dst_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let Ok(source_cap) = decode_caller_handle(raw_src_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let path = match read_user_path(path_ptr, path_len) {
        Ok(path) => path,
        Err(errno) => return encode_errno(errno),
    };
    match bootstrap::ns_mount(sched::current(), destination_cap, path, source_cap) {
        Ok(()) => 0,
        Err(error) => encode_errno(ns_errno(error)),
    }
}

/// SPEC-081：`ns_unmount` 服务——`args[0]=ns cap`、`args[1]=path ptr`、`args[2]=path len`。
fn ns_unmount_call(raw_ns_cap: u64, path_ptr: u64, path_len: u64) -> u64 {
    let Ok(ns_cap) = decode_caller_handle(raw_ns_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let path = match read_user_path(path_ptr, path_len) {
        Ok(path) => path,
        Err(errno) => return encode_errno(errno),
    };
    match bootstrap::ns_unmount(sched::current(), ns_cap, path) {
        Ok(()) => 0,
        Err(error) => encode_errno(ns_errno(error)),
    }
}

/// SPEC-081：`ns_resolve` 服务——`args[0]=ns cap`、`args[1]=path ptr`、`args[2]=path len`、
/// `args[3]=requested rights bits`。
fn ns_resolve_call(raw_ns_cap: u64, path_ptr: u64, path_len: u64, rights_bits: u64) -> u64 {
    let Ok(ns_cap) = decode_caller_handle(raw_ns_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let path = match read_user_path(path_ptr, path_len) {
        Ok(path) => path,
        Err(errno) => return encode_errno(errno),
    };
    let rights = Rights::from_bits(rights_bits);
    match bootstrap::ns_resolve(sched::current(), ns_cap, path, rights) {
        Ok(handle) => encode_caller_handle(handle),
        Err(error) => encode_errno(ns_errno(error)),
    }
}

/// SPEC-081：ring3 Namespace 路径操作见证（回执 + 内核侧审计计数）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NsWitness {
    pub create_a: u64,
    pub create_b: u64,
    pub create_c: u64,
    pub derive1: u64,
    pub derive2: u64,
    pub bind: u64,
    pub bind_overlap: u64,
    pub bind_sub: u64,
    pub resolve_lib: u64,
    pub mount_a_b: u64,
    pub mount_cycle: u64,
    pub mount_init_c: u64,
    pub resolve_sub: u64,
    pub bad_path: u64,
    pub unbind: u64,
    pub unmount: u64,
    pub destroy: u64,
    pub audit: bootstrap::NamespaceAudit,
}

impl NsWitness {
    /// 纯判定：一次完整的 Namespace 见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.create_a != 0
            && self.create_b != 0
            && self.create_c != 0
            && self.derive1 != 0
            && self.derive2 != 0
            && self.bind == 0
            && self.bind_overlap == encode_errno(Errno::Exist)
            && self.bind_sub == 0
            && self.resolve_lib != 0
            && self.mount_a_b == 0
            && self.mount_cycle == encode_errno(Errno::Invalid)
            && self.mount_init_c == 0
            && self.resolve_sub != 0
            && self.bad_path == encode_errno(Errno::Invalid)
            && self.unbind == self.derive1
            && self.unmount == 0
            && self.destroy == 0
            // 审计侧：每类成功操作各留一条事件，三类拒绝各计一次。
            // SPEC-111：`binds` 2→3、`resolves` 2→3——服务管理器把共享段副本绑到 `svc`（+1 bind），
            // Namespace 服务据此代理解析一次（+1 resolve），两者都是 ring3 真实触发的内核操作。
            // M4：管理器把文件访问令牌绑到隔离域的 `f`（+1 bind）。
            // M8：绑定改名为 rootfs 授权入口 `fs`（bind 数不变），授权改为 C 创建后**一次**
            // `SYS_NS_RESOLVE`（原逐 `open` 两次解析被收敛为一次），故 resolves 5→4。
            && self.audit.binds == 4
            && self.audit.unbinds == 2
            && self.audit.mounts == 2
            && self.audit.unmounts == 1
            && self.audit.resolves == 4
            && self.audit.cycles_rejected == 1
            && self.audit.overlaps_rejected == 1
            && self.audit.invalid_path_rejected == 1
            && self.audit.acyclic_ok == 2
    }
}

/// 读取 Namespace 见证（回执页 + 内核审计计数）。
pub fn ns_witness() -> NsWitness {
    let base = USER_Y_VA;
    NsWitness {
        create_a: read_user_qword(base + NS_RECEIPT_CREATE_A),
        create_b: read_user_qword(base + NS_RECEIPT_CREATE_B),
        create_c: read_user_qword(base + NS_RECEIPT_CREATE_C),
        derive1: read_user_qword(base + NS_RECEIPT_DERIVE1),
        derive2: read_user_qword(base + NS_RECEIPT_DERIVE2),
        bind: read_user_qword(base + NS_RECEIPT_BIND),
        bind_overlap: read_user_qword(base + NS_RECEIPT_BIND_OVERLAP),
        bind_sub: read_user_qword(base + NS_RECEIPT_BIND_SUB),
        resolve_lib: read_user_qword(base + NS_RECEIPT_RESOLVE_LIB),
        mount_a_b: read_user_qword(base + NS_RECEIPT_MOUNT_A_B),
        mount_cycle: read_user_qword(base + NS_RECEIPT_MOUNT_CYCLE),
        mount_init_c: read_user_qword(base + NS_RECEIPT_MOUNT_INIT_C),
        resolve_sub: read_user_qword(base + NS_RECEIPT_RESOLVE_SUB),
        bad_path: read_user_qword(base + NS_RECEIPT_BAD_PATH),
        unbind: read_user_qword(base + NS_RECEIPT_UNBIND),
        unmount: read_user_qword(base + NS_RECEIPT_UNMOUNT),
        destroy: read_user_qword(base + NS_RECEIPT_DESTROY),
        audit: bootstrap::namespace_audit().unwrap_or_default(),
    }
}

/// SPEC-081：`ns_destroy` 服务——`args[0]=ns cap`。
fn ns_destroy_call(raw_ns_cap: u64) -> u64 {
    let Ok(ns_cap) = decode_caller_handle(raw_ns_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::ns_destroy(sched::current(), ns_cap) {
        Ok(()) => 0,
        Err(error) => encode_errno(ns_errno(error)),
    }
}

// ---------------------------------------------------------------------------
// SPEC-082：§10 Timer 的 syscall 面
// ---------------------------------------------------------------------------

/// SPEC-082：把 `TimerError` 映射到 errno（与规范错误名的语义对应）。
fn timer_errno(error: bootstrap::TimerError) -> Errno {
    use bootstrap::TimerError;
    match error {
        TimerError::InvalidCaller | TimerError::NotHeld | TimerError::MissingRight(_) => {
            Errno::Access
        }
        TimerError::InvalidCap | TimerError::InvalidTimer => Errno::NoEnt,
        TimerError::InvalidTime => Errno::Invalid,
        TimerError::CapacityExceeded | TimerError::AuditFull => Errno::NoSpace,
    }
}

/// SPEC-113 / §15：`DeviceError` → errno（受控 I/O 路径的失败编码）。
fn device_errno(error: bootstrap::DeviceError) -> Errno {
    use bootstrap::DeviceError;
    match error {
        DeviceError::InvalidCaller | DeviceError::NotHeld | DeviceError::MissingRight(_) => {
            Errno::Access
        }
        DeviceError::InvalidCap | DeviceError::NotADevice | DeviceError::DeadDevice => Errno::NoEnt,
        DeviceError::PortOutOfRange | DeviceError::EmptyRange => Errno::Invalid,
        DeviceError::Overlap => Errno::Exist,
        DeviceError::CapacityExceeded | DeviceError::AuditFull => Errno::NoSpace,
    }
}

fn dev_irq_create_call(line: u64, pointer: u64) -> u64 {
    if !(1..=15).contains(&line) || line == 2 {
        return encode_errno(Errno::Invalid);
    }
    let mut old = [0; 16];
    if let Err(error) =
        copy_user_bytes(pointer, &mut old).and_then(|()| copy_out_user(pointer, &old))
    {
        return encode_errno(error);
    }
    let caller = sched::current();
    match bootstrap::dev_irq_create(caller, line as u8) {
        Ok((dev, source)) => {
            let mut bytes = [0; 16];
            bytes[..8].copy_from_slice(&encode_caller_handle(dev).to_le_bytes());
            bytes[8..].copy_from_slice(&encode_caller_handle(source).to_le_bytes());
            copy_out_user(pointer, &bytes).expect("IRQ output preflight");
            crate::irq::disable(line as u8);
            0
        }
        Err(error) => encode_errno(device_errno(error)),
    }
}
fn dev_irq_mask_call(raw_cap: u64, masked: u64) -> u64 {
    if masked > 1 {
        return encode_errno(Errno::Invalid);
    }
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let line = match bootstrap::irq_line(sched::current(), cap) {
        Ok(line) => line,
        Err(error) => return encode_errno(device_errno(error)),
    };
    crate::irq::set_mask(line, masked != 0);
    if masked == 0 {
        let task = sched::current();
        let mut frame = current_context();
        frame.rax = 0;
        frame.rflags |= 1 << 9;
        sched::set_frame(task, frame);
        sched::resume(task)
    }
    0
}

/// SPEC-113 / §15.5：`dev_ioport_create` 服务——`args[0]=base_port`、`args[1]=port_count`。
fn dev_ioport_create_call(base_port: u64, port_count: u64) -> u64 {
    match bootstrap::dev_ioport_create(sched::current(), base_port as u16, port_count as u16) {
        Ok(handle) => encode_caller_handle(handle),
        Err(error) => encode_errno(device_errno(error)),
    }
}

/// SPEC-113 / §15.5：受控端口写服务——`args[0]=dev cap`、`args[1]=port`、`args[2]=value`。
fn dev_ioport_out_call(raw_cap: u64, port: u64, value: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::dev_ioport_out(sched::current(), cap, port as u16, value as u8) {
        Ok(()) => 0,
        Err(error) => encode_errno(device_errno(error)),
    }
}

/// SPEC-113 / §15.5：受控端口读服务——`args[0]=dev cap`、`args[1]=port`；成功返回读回的字节。
fn dev_ioport_in_call(raw_cap: u64, port: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::dev_ioport_in(sched::current(), cap, port as u16) {
        Ok(value) => value as u64,
        Err(error) => encode_errno(device_errno(error)),
    }
}

/// EXO-4A / §15.8：销毁设备对象服务——`args[0]=dev cap`。
fn dev_destroy_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let irq = bootstrap::irq_for_cap(cap);
    match bootstrap::dev_destroy(sched::current(), cap) {
        Ok(()) => {
            if let Some(line) = irq {
                crate::irq::disable(line);
            }
            0
        }
        Err(error) => encode_errno(device_errno(error)),
    }
}

/// SPEC-082：`time_now` 服务（§10.1，纯查询）。
fn time_now_call() -> u64 {
    bootstrap::time_now().unwrap_or(0)
}

/// 启用单 CPU 的物理单调时钟与 IRQ0 唤醒；抽象 Timer 时钟保持独立。
fn time_enable_call() -> u64 {
    if !crate::preempt::start_time_counter() {
        return encode_errno(Errno::Invalid);
    }
    let mut frame = current_context();
    frame.rflags |= 1 << 9;
    frame.rax = 0;
    sched::set_frame(sched::current(), frame);
    sched::resume(sched::current())
}

/// SPEC-082：`timer_set` 服务——`args[0]=deadline`。
fn timer_set_call(deadline: u64) -> u64 {
    match bootstrap::timer_set(sched::current(), deadline) {
        Ok(handle) => encode_caller_handle(handle),
        Err(error) => encode_errno(timer_errno(error)),
    }
}

/// SPEC-082：`timer_cancel` 服务——`args[0]=timer cap`。
fn timer_cancel_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::timer_cancel(sched::current(), cap) {
        Ok(bootstrap::TimerCancelOutcome::Cancelled) => TIMER_CANCEL_OUTCOME_CANCELLED,
        Ok(bootstrap::TimerCancelOutcome::AlreadyFired) => TIMER_CANCEL_OUTCOME_ALREADY_FIRED,
        Ok(bootstrap::TimerCancelOutcome::AlreadyCancelled) => {
            TIMER_CANCEL_OUTCOME_ALREADY_CANCELLED
        }
        Err(error) => encode_errno(timer_errno(error)),
    }
}

/// SPEC-082：`timer_destroy` 服务——`args[0]=timer cap`。
fn timer_destroy_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::timer_destroy(sched::current(), cap) {
        Ok(()) => 0,
        Err(error) => encode_errno(timer_errno(error)),
    }
}

/// M15：`WaitError` → errno。
pub const fn wait_errno(error: bootstrap::WaitError) -> Errno {
    use bootstrap::WaitError;
    match error {
        WaitError::InvalidCaller | WaitError::NotHeld | WaitError::MissingRight(_) => Errno::Access,
        WaitError::InvalidCap
        | WaitError::InvalidWaitObject
        | WaitError::DuplicateWaitObject
        | WaitError::InvalidGeneration => Errno::Invalid,
        WaitError::TargetDead => Errno::NoEnt,
        WaitError::CapacityExceeded | WaitError::AuditFull => Errno::NoSpace,
        WaitError::WouldBlock => Errno::Again,
    }
}

pub(crate) fn inject_wait_wakes(wakes: &bootstrap::WaitWake) {
    for (task, result) in wakes.tasks[..wakes.count].iter().flatten() {
        sched::set_frame_rax(*task, result.code());
    }
}

static BLOCKED_CALL: StaticCell<[Option<u64>; sched::MAX_TASKS]> =
    StaticCell::new([None; sched::MAX_TASKS]);
static UPCALL_INTERRUPTED: StaticCell<[bool; sched::MAX_TASKS]> =
    StaticCell::new([false; sched::MAX_TASKS]);

pub(crate) fn has_pending_native_call(task: u32) -> bool {
    BLOCKED_CALL.with(|calls| calls.get(task as usize).is_some_and(Option::is_some))
        || UPCALL_INTERRUPTED.with(|flags| flags.get(task as usize).copied().unwrap_or(false))
}

pub fn clear_blocked_call(task: u32) {
    if (task as usize) < sched::MAX_TASKS {
        BLOCKED_CALL.with_mut(|calls| calls[task as usize] = None);
        UPCALL_INTERRUPTED.with_mut(|calls| calls[task as usize] = false);
    }
}
fn mark_blocked_call(task: u32, number: u64) {
    BLOCKED_CALL.with_mut(|calls| calls[task as usize] = Some(number));
    UPCALL_INTERRUPTED.with_mut(|calls| calls[task as usize] = false);
}

#[cfg(test)]
#[test]
fn gate_admission_detects_native_restart_and_interruption_without_consuming_them() {
    let task = 17;
    clear_blocked_call(task);
    assert!(!has_pending_native_call(task));
    mark_blocked_call(task, SYS_RECV);
    assert!(has_pending_native_call(task));
    assert_eq!(
        BLOCKED_CALL.with(|calls| calls[task as usize]),
        Some(SYS_RECV)
    );
    clear_blocked_call(task);
    UPCALL_INTERRUPTED.with_mut(|flags| flags[task as usize] = true);
    assert!(has_pending_native_call(task));
    assert!(UPCALL_INTERRUPTED.with(|flags| flags[task as usize]));
    clear_blocked_call(task);
    assert!(!has_pending_native_call(task));
}

/// M17b：fork（COW 地址空间；子进程独立用户栈）。
fn fork_call(prepared: bool) -> u64 {
    let caller = sched::current();
    let check = if prepared {
        bootstrap::check_prepared_fork(caller)
    } else {
        bootstrap::check_fork(caller)
    };
    if let Err(error) = check {
        return encode_errno(match error {
            bootstrap::ThreadError::Busy => Errno::Again,
            bootstrap::ThreadError::InvalidCaller => Errno::Invalid,
            _ => Errno::NoSpace,
        });
    }
    let Some(parent_root) = sched::root(caller) else {
        return encode_errno(Errno::Fault);
    };
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return encode_errno(Errno::Fault);
    };
    if crate::fork_tx::capture(caller, parent_root, offset).is_err() {
        return encode_errno(Errno::NoSpace);
    }
    let Ok(new_root) = crate::frames::allocate_table_frame() else {
        crate::fork_tx::forget(caller);
        return encode_errno(Errno::NoSpace);
    };
    let map = |phys| crate::mem::phys_to_virt(phys, offset);
    for index in 0..512 {
        prima_ostd::mm::write_u64_at(map(new_root), index, 0);
    }
    if crate::vspace::clone_root_cow(
        parent_root,
        new_root,
        map,
        || crate::frames::allocate_table_frame().ok(),
        |frame| {
            let _ = crate::frames::release_table_frame(frame);
        },
        || crate::cpu::write_cr3(crate::cpu::cr3().0),
    )
    .is_err()
    {
        let _ = crate::frames::release_table_frame(new_root);
        crate::fork_tx::forget(caller);
        return encode_errno(Errno::NoSpace);
    }
    let frame = current_context();
    let outcome = if prepared {
        bootstrap::prepare_fork(caller, frame.rip)
    } else {
        bootstrap::fork_create(caller, frame.rip).map(|child| (child, Handle::ZERO))
    };
    let (child, ticket) = match outcome {
        Ok(outcome) => outcome,
        Err(error) => {
            crate::vspace::discard_cow_root(new_root, map, |frame| {
                let _ = crate::frames::release_table_frame(frame);
            });
            let _ = crate::frames::release_table_frame(new_root);
            crate::fork_tx::restore(caller, offset);
            return encode_errno(if error == bootstrap::ThreadError::Busy {
                Errno::Again
            } else {
                Errno::NoSpace
            });
        }
    };
    let fs_base = sched::fs_base(caller).unwrap_or(0);
    if crate::user::register_clone_task(child, new_root, frame, frame.rsp, fs_base).is_err() {
        if prepared {
            bootstrap::abort_prepared_fork(caller, ticket, offset)
                .expect("prepared cancellation reserved");
        } else {
            bootstrap::fork_abort(child);
        }
        {
            sched::clear_task(child);
            crate::vspace::discard_cow_root(new_root, map, |frame| {
                let _ = crate::frames::release_table_frame(frame);
            });
            let _ = crate::frames::release_table_frame(new_root);
        }
        crate::fork_tx::restore(caller, offset);
        return encode_errno(Errno::Invalid);
    }
    crate::signal::inherit_process(caller, child);
    if prepared {
        bootstrap::set_prepared_fork_root(caller, ticket, new_root)
            .expect("registered prepared root");
        encode_caller_handle(ticket)
    } else {
        crate::fork_tx::forget(caller);
        child as u64
    }
}

fn fork_transaction_call(selector: u64, raw_ticket: u64) -> u64 {
    if selector == 0 {
        return fork_call(true);
    }
    let Ok(ticket) = decode_caller_handle(raw_ticket) else {
        return encode_errno(Errno::Invalid);
    };
    let parent = sched::current();
    match selector {
        1 => bootstrap::publish_fork(parent, ticket)
            .map_or_else(|error| encode_errno(task_destroy_errno(error)), u64::from),
        2 => {
            let Some((_, offset)) = crate::cow::root_and_offset() else {
                return encode_errno(Errno::Fault);
            };
            bootstrap::abort_prepared_fork(parent, ticket, offset)
                .map_or_else(|error| encode_errno(task_destroy_errno(error)), |_| 0)
        }
        _ => encode_errno(Errno::Invalid),
    }
}

fn wait4_call(pid: u64, status_ptr: u64, options: u64) -> u64 {
    let cur = sched::current();
    if options & !1 != 0 || (pid as i64 != -1 && pid as i64 <= 0) {
        return encode_errno(Errno::Invalid);
    }
    if let Err(error) = sched::prepare_wait_status(cur, status_ptr) {
        return encode_errno(error);
    }
    let no_hang = options & 1 != 0;
    match bootstrap::wait4(cur, pid as i64, status_ptr, no_hang) {
        Ok(bootstrap::Wait4Outcome::Exited(child, status)) => {
            match sched::write_wait_status(cur, status_ptr, status) {
                Ok(()) => child as u64,
                Err(error) => encode_errno(error),
            }
        }
        Ok(bootstrap::Wait4Outcome::None) => 0,
        Ok(bootstrap::Wait4Outcome::Blocked) => {
            let mut frame = current_context();
            frame.rax = 0;
            sched::set_frame(cur, frame);
            mark_blocked_call(cur, SYS_WAIT4);
            sched::block_current()
        }
        Err(bootstrap::Wait4Error::EChild) => encode_errno(Errno::Child),
    }
}

/// M17c：exec 的初始栈固定 VA（PML4 槽 254，单页）。
const EXEC_STACK_VA: u64 = 0x0000_7f00_0000_0000;

/// Page-checked copy-in with an explicit root at the mechanism boundary.
fn copy_user_bytes(ptr: u64, output: &mut [u8]) -> Result<(), Errno> {
    if ptr == 0 {
        return Err(Errno::Fault);
    }
    let root = sched::current_root().ok_or(Errno::Fault)?;
    let (_, offset) = crate::cow::root_and_offset().ok_or(Errno::Fault)?;
    vspace::read_user_bytes(root, ptr, output, |phys| phys_to_virt(phys, offset))
        .map_err(user_copy_errno)
}

fn user_copy_errno(error: vspace::VspaceError) -> Errno {
    if error == vspace::VspaceError::NoFrame {
        Errno::NoSpace
    } else {
        Errno::Fault
    }
}

/// Shared by native output, IPC direct delivery and wait status. Single CPU,
/// kernel non-preemptible: the plan must be consumed before scheduling or VM
/// mutation. Gate must additionally validate its request/Task/root generations
/// and reject shared roots before using this mechanism.
pub(crate) fn prepare_task_output(
    task: u32,
    ptr: u64,
    len: usize,
) -> Result<vspace::PreparedUserWrite, Errno> {
    let root = sched::root(task).ok_or(Errno::Fault)?;
    let (_, offset) = crate::cow::root_and_offset().ok_or(Errno::Fault)?;
    let map = |phys| phys_to_virt(phys, offset);
    // Keep missing Segment errors distinct from allocation failure. Validate
    // every COW owner before the first reservation; writable platform mappings
    // retain their existing behavior.
    let end = ptr.checked_add(len as u64).ok_or(Errno::Fault)?;
    if len > vspace::USER_COPY_LIMIT || len != 0 && (ptr == 0 || end > 1 << 47) {
        return Err(Errno::Fault);
    }
    if len != 0 {
        let first = ptr & !4095;
        let last = (end - 1) & !4095;
        for page in [first, last] {
            let leaf = vspace::read_user_leaf(root, page, map).map_err(user_copy_errno)?;
            if leaf & PT_WRITABLE == 0 && bootstrap::task_segment_at(task, page).is_none() {
                return Err(Errno::Fault);
            }
        }
    }
    let active = sched::current_root() == Some(root);
    vspace::prepare_user_write(
        root,
        ptr,
        len,
        map,
        |page| crate::frames::allocate_for_segment(bootstrap::task_segment_at(task, page)?).ok(),
        |page, frame| {
            let segment = bootstrap::task_segment_at(task, page).expect("exclusive output mapping");
            crate::frames::release_frame(segment, frame).expect("uncommitted COW reservation");
        },
        |page| {
            if active {
                prima_ostd::arch::invlpg(page);
            }
        },
    )
    .map_err(user_copy_errno)
}

fn copy_out_user(ptr: u64, bytes: &[u8]) -> Result<(), Errno> {
    if bytes.is_empty() {
        return Ok(());
    }
    let output = prepare_task_output(sched::current(), ptr, bytes.len())?;
    let (_, offset) = crate::cow::root_and_offset().ok_or(Errno::Fault)?;
    output.write(bytes, |phys| phys_to_virt(phys, offset));
    Ok(())
}

fn copy_exec_strings(
    ptr: u64,
    count: usize,
    buffers: &mut [[u8; 128]; 16],
    lengths: &mut [usize; 16],
) -> Result<(), Errno> {
    for index in 0..count {
        let mut word = [0u8; 8];
        copy_user_bytes(
            ptr.checked_add(8 * index as u64).ok_or(Errno::Fault)?,
            &mut word,
        )?;
        let string = u64::from_le_bytes(word);
        let mut terminated = false;
        for byte in 0..128 {
            copy_user_bytes(
                string.checked_add(byte as u64).ok_or(Errno::Fault)?,
                &mut buffers[index][byte..byte + 1],
            )?;
            if buffers[index][byte] == 0 {
                lengths[index] = byte;
                terminated = true;
                break;
            }
        }
        if !terminated {
            return Err(Errno::NoSpace);
        }
    }
    Ok(())
}

const EXEC_STACK_PAGES: usize = 8;
static EXEC_RETAINED: StaticCell<
    [Option<crate::exec::RetainedPage>; crate::exec::MAX_RETAINED_PAGES],
> = StaticCell::new([None; crate::exec::MAX_RETAINED_PAGES]);
#[derive(Clone, Copy)]
struct ExecTransfer {
    regions: [(u64, usize); 4],
    count: usize,
    registers: [u64; 6],
    adopt: Option<Handle>,
    redirect: u64,
}
fn exec_handoff_call(pointer: u64) -> u64 {
    let load = (|| -> Result<([u64; 6], ExecTransfer), Errno> {
        let mut bytes = [0; 88];
        copy_user_bytes(pointer, &mut bytes)?;
        let word = |i| u64::from_le_bytes(bytes[8 * i..8 * i + 8].try_into().expect("exec word"));
        let mut args = [0; 6];
        for (i, arg) in args.iter_mut().enumerate() {
            *arg = word(i);
        }
        let count = usize::try_from(word(7)).map_err(|_| Errno::Invalid)?;
        if count > 4 {
            return Err(Errno::Invalid);
        }
        let mut transfer = ExecTransfer {
            regions: [(0, 0); 4],
            count,
            registers: [0; 6],
            redirect: word(10),
            adopt: if word(9) == 0 {
                None
            } else {
                Some(decode_caller_handle(word(9)).map_err(|_| Errno::Access)?)
            },
        };
        for index in 0..count {
            let mut region = [0; 16];
            copy_user_bytes(
                word(6).checked_add(index as u64 * 16).ok_or(Errno::Fault)?,
                &mut region,
            )?;
            transfer.regions[index] = (
                u64::from_le_bytes(region[..8].try_into().expect("region base")),
                u64::from_le_bytes(region[8..].try_into().expect("region pages")) as usize,
            );
        }
        let mut registers = [0; 48];
        copy_user_bytes(word(8), &mut registers)?;
        for (i, reg) in transfer.registers.iter_mut().enumerate() {
            *reg = u64::from_le_bytes(
                registers[8 * i..8 * i + 8]
                    .try_into()
                    .expect("entry register"),
            );
        }
        Ok((args, transfer))
    })();
    match load {
        Ok((a, t)) => exec_call(a[0], a[1], a[2], a[3], a[4], a[5], Some(&t)),
        Err(error) => encode_errno(error),
    }
}

// Retire parser scratch before building the replacement image.
#[inline(never)]
fn load_exec_plan(image: u64, length: u64) -> Result<crate::exec::ExecPlan, Errno> {
    image.checked_add(length).ok_or(Errno::Fault)?;
    crate::exec::plan_read(length, |offset, output| {
        copy_user_bytes(image.checked_add(offset).ok_or(Errno::Fault)?, output)
    })
    .map_err(|error| match error {
        crate::exec::PlanReadError::Image(_) => Errno::Exec,
        crate::exec::PlanReadError::Read(error) => error,
    })
}

#[inline(never)]
fn copy_exec_page(source: u64, target: u64, count: usize) -> Result<(), Errno> {
    let mut bytes = [0; 4096];
    copy_user_bytes(source, &mut bytes[..count])?;
    for (index, &byte) in bytes[..count].iter().enumerate() {
        prima_ostd::mm::write_u8_at(target, index as u64, byte);
    }
    Ok(())
}

#[inline(never)]
fn prepare_exec_stack(
    plan: &crate::exec::ExecPlan,
    argv_ptr: u64,
    argc: usize,
    env_ptr: u64,
    envc: usize,
    stack: &mut [u8; 4096],
) -> Result<crate::exec::StackLayout, Errno> {
    let mut args = [[0u8; 128]; 16];
    let mut envs = [[0u8; 128]; 16];
    let mut arg_lens = [0usize; 16];
    let mut env_lens = [0usize; 16];
    copy_exec_strings(argv_ptr, argc, &mut args, &mut arg_lens)
        .and_then(|()| copy_exec_strings(env_ptr, envc, &mut envs, &mut env_lens))?;
    let mut arg_slices = [&[][..]; 16];
    let mut env_slices = [&[][..]; 16];
    for i in 0..argc {
        arg_slices[i] = &args[i][..arg_lens[i]];
    }
    for i in 0..envc {
        env_slices[i] = &envs[i][..env_lens[i]];
    }
    crate::exec::build_initial_stack(
        EXEC_STACK_VA + EXEC_STACK_PAGES as u64 * 4096,
        plan,
        &arg_slices[..argc],
        &env_slices[..envc],
        stack,
    )
    .map_err(|_| Errno::NoSpace)
}

// Retire construction scratch before committing/reclaiming the old image.
// The latter may destroy several private Segments during consecutive execs.
#[inline(never)]
fn prepare_exec_image(
    root: u64,
    offset: u64,
    plan: &crate::exec::ExecPlan,
    input: [u64; 5],
    pages: usize,
    retained_count: usize,
) -> Result<(u64, u32, u64), (Errno, bool)> {
    let [image, argv_ptr, argv_count, env_ptr, env_count] = input;
    let mut stack = [0u8; 4096];
    let layout = prepare_exec_stack(
        plan,
        argv_ptr,
        argv_count as usize,
        env_ptr,
        env_count as usize,
        &mut stack,
    )
    .map_err(|error| (error, false))?;
    let Ok(new_root) = crate::frames::allocate_table_frame() else {
        return Err((Errno::NoSpace, false));
    };
    let map = |phys| phys_to_virt(phys, offset);
    vspace::clone_supervisor_root(root, new_root, map);
    let (segment, frames) = match bootstrap::allocate_exec_segment(pages, offset) {
        Ok(value) => value,
        Err(error) => {
            let _ = crate::frames::release_table_frame(new_root);
            return Err((memseg_errno(error), false));
        }
    };
    // Track every table reservation, including map_4k's unlinked partial batch.
    let mut tables = [0u64; crate::frames::MAX_TABLE_FRAMES];
    let mut table_count = 0usize;
    let result = (|| -> Result<(), Errno> {
        let mut page_index = 0;
        for part in plan.segments.iter().flatten() {
            if part.map_start < 4096 || part.map_end() > EXEC_STACK_VA {
                return Err(Errno::Exec);
            }
            for page in 0..part.pages() {
                let va = part.map_start + 4096 * page;
                // Never extend a shared supervisor subtree with user access.
                let pml4 =
                    prima_ostd::mm::read_u64_at(map(new_root), vspace::indices(va)[0] as u64);
                if pml4 & PT_PRESENT != 0 && pml4 & PT_USER == 0 {
                    return Err(Errno::Exec);
                }
                let frame = frames
                    .frame_at(page_index)
                    .expect("exec Segment frame count");
                vspace::map_4k(
                    new_root,
                    va,
                    map,
                    || {
                        if table_count == tables.len() {
                            return None;
                        }
                        let frame = crate::frames::allocate_table_frame().ok()?;
                        tables[table_count] = frame;
                        table_count += 1;
                        Some(frame)
                    },
                    frame,
                    part.writable,
                    true,
                    |_| {},
                )
                .map_err(|_| Errno::NoSpace)?;
                // 文件数据从 `file_base` 起覆盖 `file_bytes()` 字节：首页可能包含段首
                // 之前的同页字节（Linux 也映射整页），其余页按 4096 切分。
                let begin = 4096 * page;
                let count = core::cmp::min(4096, part.file_bytes().saturating_sub(begin)) as usize;
                if count > 0 {
                    copy_exec_page(
                        image
                            .checked_add(part.file_base)
                            .and_then(|v| v.checked_add(begin))
                            .ok_or(Errno::Fault)?,
                        map(frame),
                        count,
                    )?;
                }
                page_index += 1;
            }
        }
        for index in 0..EXEC_STACK_PAGES {
            let stack_frame = frames
                .frame_at(page_index + index)
                .expect("exec stack frame");
            vspace::map_4k(
                new_root,
                EXEC_STACK_VA + index as u64 * 4096,
                map,
                || {
                    if table_count == tables.len() {
                        return None;
                    }
                    let frame = crate::frames::allocate_table_frame().ok()?;
                    tables[table_count] = frame;
                    table_count += 1;
                    Some(frame)
                },
                stack_frame,
                true,
                true,
                |_| {},
            )
            .map_err(|_| Errno::NoSpace)?;
            for (byte, &stack_value) in stack.iter().enumerate() {
                let value = if index + 1 == EXEC_STACK_PAGES {
                    stack_value
                } else {
                    0
                };
                prima_ostd::mm::write_u8_at(map(stack_frame), byte as u64, value);
            }
        }
        for index in 0..retained_count {
            let page = EXEC_RETAINED.with(|rows| rows[index].expect("retained page"));
            vspace::map_4k(
                new_root,
                page.entry.va,
                map,
                || {
                    if table_count == tables.len() {
                        return None;
                    }
                    let frame = crate::frames::allocate_table_frame().ok()?;
                    tables[table_count] = frame;
                    table_count += 1;
                    Some(frame)
                },
                page.pte & PT_ADDR_MASK,
                page.pte & PT_WRITABLE != 0,
                true,
                |_| {},
            )
            .map_err(|_| Errno::NoSpace)?;
            vspace::write_leaf_raw(new_root, page.entry.va, page.pte, map)
                .map_err(|_| Errno::Fault)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        for frame in &tables[..table_count] {
            let _ = crate::frames::release_table_frame(*frame);
        }
        let _ = crate::frames::release_table_frame(new_root);
        // Reclaim the staged Segment only after this large construction frame
        // has unwound; nested memseg_destroy otherwise crosses the stack guard.
        return Err((error, true));
    }
    Ok((new_root, segment, layout.rsp))
}

/// Construct a complete private image before retiring any old mapping.
#[allow(clippy::too_many_arguments)]
fn exec_call(
    image: u64,
    image_len: u64,
    argv_ptr: u64,
    argv_count: u64,
    env_ptr: u64,
    env_count: u64,
    transfer: Option<&ExecTransfer>,
) -> u64 {
    let cur = sched::current();
    let Some(root) = sched::root(cur) else {
        return encode_errno(Errno::Fault);
    };
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return encode_errno(Errno::Fault);
    };
    if argv_count > 16 || env_count > 16 {
        return encode_errno(Errno::NoSpace);
    }
    let plan = match load_exec_plan(image, image_len) {
        Ok(plan) => plan,
        Err(error) => return encode_errno(error),
    };
    let pages = EXEC_STACK_PAGES
        + plan
            .segments
            .iter()
            .flatten()
            .map(|s| s.pages() as usize)
            .sum::<usize>();
    let mut retained_count = 0;
    let mut retained_requested = 0;
    EXEC_RETAINED.with_mut(|rows| rows.fill(None));
    if let Some(transfer) = transfer {
        if let Some(cap) = transfer.adopt
            && let Err(error) = bootstrap::check_exec_adopt(cur, cap)
        {
            return encode_errno(memseg_errno(error));
        }
        let result = (|| -> Result<(), Errno> {
            for &(base, count) in &transfer.regions[..transfer.count] {
                let end = base
                    .checked_add((count as u64).checked_mul(4096).ok_or(Errno::Invalid)?)
                    .ok_or(Errno::Invalid)?;
                if base < 4096
                    || base & 4095 != 0
                    || end > EXEC_STACK_VA
                    || count == 0
                    || count > crate::exec::MAX_RETAINED_PAGES - retained_requested
                    || plan
                        .segments
                        .iter()
                        .flatten()
                        .any(|s| base < s.map_end() && s.map_start < end)
                {
                    return Err(Errno::Invalid);
                }
                retained_requested += count;
                for index in 0..count {
                    let va = base + index as u64 * 4096;
                    if EXEC_RETAINED.with(|rows| {
                        rows[..retained_count]
                            .iter()
                            .flatten()
                            .any(|p| p.entry.va == va)
                    }) {
                        return Err(Errno::Invalid);
                    }
                    let Some(entry) = bootstrap::task_mapping(cur, va) else {
                        continue;
                    };
                    let pte =
                        vspace::read_owned_user_leaf(root, va, |p| phys_to_virt(p, offset), true)
                            .map_err(|_| Errno::Fault)?;
                    EXEC_RETAINED.with_mut(|rows| {
                        rows[retained_count] = Some(crate::exec::RetainedPage { entry, pte })
                    });
                    retained_count += 1;
                }
            }
            if transfer.redirect != 0
                && !EXEC_RETAINED.with(|rows| {
                    rows[..retained_count].iter().flatten().any(|page| {
                        page.entry.va == transfer.redirect & !4095
                            && page.entry.access.contains(Right::SegMapX)
                            && page.pte & vspace::PT_USER != 0
                    })
                })
            {
                return Err(Errno::Access);
            }
            Ok(())
        })();
        if let Err(error) = result {
            return encode_errno(error);
        }
    }
    if let Err(error) = bootstrap::check_exec(cur, pages + retained_requested) {
        return encode_errno(task_destroy_errno(error));
    }
    let (new_root, segment, rsp) = match prepare_exec_image(
        root,
        offset,
        &plan,
        [image, argv_ptr, argv_count, env_ptr, env_count],
        pages,
        retained_count,
    ) {
        Ok(image) => image,
        Err((error, reclaim)) => {
            if reclaim {
                bootstrap::reclaim_exec_segments().expect("exec rollback audit reserved");
            }
            return encode_errno(error);
        }
    };
    if let Some(cap) = transfer.and_then(|t| t.adopt) {
        bootstrap::adopt_exec_segment(cur, cap);
    }
    EXEC_RETAINED
        .with(|rows| {
            bootstrap::commit_exec(
                cur,
                root,
                offset,
                segment,
                &plan,
                EXEC_STACK_VA,
                EXEC_STACK_PAGES,
                &rows[..retained_count],
            )
        })
        .expect("validated exec commit");
    sched::set_root(cur, new_root);
    let mut frame = current_context();
    frame.rip = plan.entry;
    frame.rsp = rsp;
    frame.rax = 0;
    frame.rdi = 0;
    if let Some(t) = transfer {
        [
            frame.rdi, frame.rsi, frame.rdx, frame.rcx, frame.r8, frame.r15,
        ] = t.registers;
        frame.r12 = plan.entry;
        if t.redirect != 0 {
            frame.rip = t.redirect;
        }
    }
    sched::set_frame(cur, frame);
    sched::set_fs_base(cur, 0);
    crate::signal::exec_reset(cur);
    sched::reset_fpu(cur);
    sched::resume(cur)
}

/// M18a：默认信号终止，保留与普通 exit 不同的 wait 原因。
fn task_info_call(selector: u64, argument: u64) -> u64 {
    let caller = sched::current();
    if selector == 7 {
        return bootstrap::process_of(caller).map_or(1, bootstrap::process_threads) as u64;
    }
    if selector == 5 {
        return match bootstrap::task_self_capability(caller) {
            Ok(cap) => encode_caller_handle(cap),
            Err(_) => encode_errno(Errno::Access),
        };
    }
    let target = match selector {
        0 => caller,
        1..=3 | 6 => match u32::try_from(argument) {
            Ok(task) => task,
            Err(_) => return encode_errno(Errno::Invalid),
        },
        4 => {
            let Ok(cap) = decode_caller_handle(argument) else {
                return encode_errno(Errno::Invalid);
            };
            if bootstrap::authorize(caller, cap, Right::TaskSend).is_err() {
                return encode_errno(Errno::Access);
            }
            let Some(task) = bootstrap::task_of_cap(cap) else {
                return encode_errno(Errno::Invalid);
            };
            task
        }
        _ => return encode_errno(Errno::Invalid),
    };
    if bootstrap::task_cookie(target).is_none() {
        return encode_errno(Errno::NoEnt);
    }
    if selector == 6 {
        return u64::from(bootstrap::task_status(target) == Some(bootstrap::TaskStatus::Prepared));
    }
    let task = match selector {
        2 => bootstrap::process_identity(target, 0).unwrap_or(target),
        3 => bootstrap::process_identity(target, 1).unwrap_or(0),
        _ => target,
    };
    if task == 0 {
        0
    } else {
        bootstrap::task_cookie(task).unwrap_or(0)
    }
}

fn signal_call(pid: u64, sig: u64) -> u64 {
    if !(1..=31).contains(&sig) {
        return encode_errno(Errno::Invalid);
    }
    let Ok(target) = u32::try_from(pid) else {
        return encode_errno(Errno::Invalid);
    };
    if !bootstrap::can_notify(sched::current(), target) {
        return encode_errno(Errno::Access);
    }
    if bootstrap::task_status(target) == Some(bootstrap::TaskStatus::Prepared) {
        return encode_errno(Errno::NoEnt);
    }
    let Some(process) = bootstrap::process_of(target) else {
        return encode_errno(Errno::NoEnt);
    };
    if sig == 9 {
        let leader = bootstrap::process_leader(process).unwrap_or(target);
        return finish_process(leader, bootstrap::ProcessExit::Signaled(9));
    }
    if crate::signal::handler_of(target, sig as u32) == 1 {
        return 0;
    }
    if crate::signal::handler_of(target, sig as u32) != 0 {
        crate::signal::set_pending(target, sig as u32);
        return 0;
    }
    let leader = bootstrap::process_leader(process).unwrap_or(target);
    finish_process(leader, bootstrap::ProcessExit::Signaled(sig as u8))
}

/// M18b：登记信号 handler。
fn sigaction_call(signal: u64, handler: u64) -> u64 {
    if !(1..=31).contains(&signal) {
        return encode_errno(Errno::Invalid);
    }
    let cur = sched::current();
    if handler > 1 && handler >> 47 != 0 {
        return encode_errno(Errno::Invalid);
    }
    crate::signal::set_handler(cur, signal as u32, handler);
    0
}

/// M18b：从 handler 返回，恢复被中断的现场。
fn sigreturn_call() -> u64 {
    let cur = sched::current();
    match crate::signal::take_saved(cur) {
        Some(frame) => {
            sched::set_frame(cur, frame);
            sched::resume(cur)
        }
        None => encode_errno(Errno::Invalid),
    }
}

/// Called on every scheduler return after selecting root, TLS and FPU state.
/// A wait interrupted by a restartable upcall replays the original native trap.
pub fn prepare_signal_resume(task: u32, mut frame: SretFrame) -> Option<SretFrame> {
    // Gate v1 retains signals without delivering native frames or replaying
    // opaque client numbers, including after reply and on orphan bindings.
    if bootstrap::gate::task_is_gated(task) {
        return None;
    }
    let blocked = BLOCKED_CALL.with_mut(|calls| calls[task as usize].take());
    let interrupted =
        UPCALL_INTERRUPTED.with_mut(|calls| core::mem::take(&mut calls[task as usize]));
    let pending = crate::signal::pending(task);
    if pending == 0 {
        return None;
    }
    let slot = pending.trailing_zeros() + 1;
    let action = crate::signal::action(task, slot)?;
    if action.handler == 1
        || action.handler == 0 && action.flags & crate::signal::IGNORE_DEFAULT != 0
    {
        crate::signal::clear_pending(task, slot);
        return None;
    }
    if action.handler == 0 {
        terminate_current(bootstrap::ProcessExit::Signaled(slot as u8));
    }
    if interrupted
        && action.flags & crate::signal::RESTART != 0
        && let Some(number) = blocked
        && number != SYS_TASK_WAIT_DEADLINE
        && !(matches!(number, 18 | 60) && action.flags & crate::signal::IPC_RESTART_IN_USER != 0)
        && frame.rip >= 2
    {
        let mut opcode = [0; 2];
        if copy_user_bytes(frame.rip - 2, &mut opcode).is_ok()
            && matches!(opcode, [0x0f, 0x05] | [0xcd, 0x80])
        {
            frame.rip -= 2;
            frame.rax = number;
        }
    }
    crate::signal::deliver(task, slot, frame)
}
fn maybe_deliver_signals(result: u64) {
    let task = sched::current();
    if crate::signal::pending(task) == 0 {
        return;
    }
    let mut frame = current_context();
    frame.rax = result;
    if let Some(handler) = prepare_signal_resume(task, frame) {
        sched::set_frame(task, handler);
        sched::resume(task);
    }
}

fn upcall_control(args: &[u64; MAX_ARGS]) -> u64 {
    let task = sched::current();
    let result = (|| -> Result<u64, Errno> {
        match args[0] {
            0 => {
                let slot = u32::try_from(args[1]).map_err(|_| Errno::Invalid)?;
                let action = crate::signal::Action {
                    handler: args[2],
                    mask: args[3],
                    flags: args[4],
                    context: args[5],
                };
                if action.handler > 1 && action.handler >> 47 != 0
                    || action.flags
                        & !(crate::signal::NO_DEFER
                            | crate::signal::RESET_ACTION
                            | crate::signal::RESTART
                            | crate::signal::IGNORE_DEFAULT
                            | crate::signal::WITH_INFO
                            | crate::signal::IPC_RESTART_IN_USER)
                        != 0
                {
                    return Err(Errno::Invalid);
                }
                crate::signal::set_action(task, slot, action).ok_or(Errno::Invalid)?;
                Ok(0)
            }
            1 => {
                let old = crate::signal::mask(task);
                let new = if args[2] == 0 {
                    None
                } else {
                    let mut bytes = [0; 8];
                    copy_user_bytes(args[2], &mut bytes)?;
                    let mask = u64::from_le_bytes(bytes);
                    Some(match args[1] {
                        0 => old | mask,
                        1 => old & !mask,
                        2 => mask,
                        _ => return Err(Errno::Invalid),
                    })
                };
                if args[3] != 0 {
                    copy_out_user(args[3], &old.to_le_bytes())?;
                }
                if let Some(new) = new {
                    crate::signal::set_mask(task, new).ok_or(Errno::Invalid)?;
                }
                Ok(0)
            }
            2 => {
                copy_out_user(args[1], &crate::signal::all_pending(task).to_le_bytes())?;
                Ok(0)
            }
            3 => {
                let target = u32::try_from(args[1]).map_err(|_| Errno::Invalid)?;
                let slot = u32::try_from(args[2]).map_err(|_| Errno::Invalid)?;
                if bootstrap::task_cookie(target).is_none()
                    || bootstrap::task_status(target) == Some(bootstrap::TaskStatus::Prepared)
                {
                    return Err(Errno::NoEnt);
                }
                if !bootstrap::can_notify(task, target) {
                    return Err(Errno::Access);
                }
                if slot == 0 {
                    return Ok(0);
                }
                let action = crate::signal::action(target, slot).ok_or(Errno::Invalid)?;
                if args[3] > 1 {
                    return Err(Errno::Invalid);
                }
                if action.handler == 1
                    || action.handler == 0 && action.flags & crate::signal::IGNORE_DEFAULT != 0
                {
                    return Ok(0);
                }
                let masked = crate::signal::mask(target) & (1u64 << (slot - 1)) != 0;
                if !masked && action.handler == 0 && bootstrap::upcall_interruptible(target) {
                    let leader = bootstrap::process_of(target)
                        .and_then(bootstrap::process_leader)
                        .unwrap_or(target);
                    return Ok(finish_process(
                        leader,
                        bootstrap::ProcessExit::Signaled(slot as u8),
                    ));
                }
                if !masked && bootstrap::interrupt_for_upcall(target).map_err(task_destroy_errno)? {
                    end_pending_recv(target, RecvEnd::Interrupted);
                    sched::set_frame_rax(target, encode_errno(Errno::Interrupted));
                    UPCALL_INTERRUPTED.with_mut(|flags| flags[target as usize] = true);
                }
                let source = bootstrap::process_identity(task, 0).unwrap_or(task);
                crate::signal::queue_info(
                    target,
                    slot,
                    crate::signal::Info {
                        slot: slot as u64,
                        sender: bootstrap::task_cookie(source).unwrap_or(0),
                        kind: args[3],
                        value: args[4],
                    },
                );
                Ok(0)
            }
            4 => {
                let bytes = crate::signal::export_context(task, args[1]).ok_or(Errno::Invalid)?;
                copy_out_user(args[2], &bytes)?;
                Ok(0)
            }
            5 => {
                let mut bytes = [0; crate::signal::CONTEXT_BYTES];
                copy_user_bytes(args[2], &mut bytes)?;
                let frame =
                    crate::signal::restore_context(task, args[1], &bytes).ok_or(Errno::Invalid)?;
                sched::set_frame(task, frame);
                sched::resume(task)
            }
            6 => {
                // Atomic temporary mask + wait. SA_RESTART never replays this call.
                let old = crate::signal::mask(task);
                crate::signal::set_mask(task, args[1]).ok_or(Errno::Invalid)?;
                if crate::signal::pending(task) == 0 {
                    if let Err(error) = bootstrap::block_upcall(task) {
                        crate::signal::set_mask(task, old);
                        return Err(task_destroy_errno(error));
                    }
                    crate::signal::suspend_mask(task, old);
                    let mut frame = current_context();
                    frame.rax = encode_errno(Errno::Interrupted);
                    sched::set_frame(task, frame);
                    sched::block_current()
                }
                crate::signal::suspend_mask(task, old);
                Err(Errno::Interrupted)
            }
            7 => {
                let slot = u32::try_from(args[1]).map_err(|_| Errno::Invalid)?;
                let action = crate::signal::action(task, slot).ok_or(Errno::Invalid)?;
                let mut bytes = [0; 32];
                for (i, value) in [action.handler, action.mask, action.flags, action.context]
                    .iter()
                    .enumerate()
                {
                    bytes[i * 8..i * 8 + 8].copy_from_slice(&value.to_le_bytes());
                }
                copy_out_user(args[2], &bytes)?;
                Ok(0)
            }
            8 => {
                if !crate::signal::set_route(
                    task,
                    args[1] as usize,
                    u32::try_from(args[2]).map_err(|_| Errno::Invalid)?,
                ) {
                    return Err(Errno::Invalid);
                }
                Ok(0)
            }
            _ => Err(Errno::Invalid),
        }
    })();
    result.unwrap_or_else(encode_errno)
}

/// Returns only when terminating a different Process or when preflight fails.
fn notify_child_exit(parent: u32, child_cookie: u64, status: bootstrap::ProcessExit) {
    let slot = crate::signal::route(parent, 0);
    if !crate::signal::action(parent, slot).is_some_and(|a| a.handler > 1) {
        return;
    }
    if crate::signal::mask(parent) & (1 << (slot - 1)) == 0
        && bootstrap::interrupt_for_upcall(parent).expect("exit callback audit preflight")
    {
        end_pending_recv(parent, RecvEnd::Interrupted);
        sched::set_frame_rax(parent, encode_errno(Errno::Interrupted));
        UPCALL_INTERRUPTED.with_mut(|flags| flags[parent as usize] = true);
    }
    let value = match status {
        bootstrap::ProcessExit::Exited(code) => u64::from(code),
        bootstrap::ProcessExit::Signaled(slot) => (1 << 63) | u64::from(slot),
    };
    crate::signal::queue_info(
        parent,
        slot,
        crate::signal::Info {
            slot: slot as u64,
            sender: child_cookie,
            kind: 2,
            value,
        },
    );
}

pub fn deliver_fault(frame: SretFrame, event: usize, value: u64, permission: bool) -> bool {
    deliver_fault_kind(frame, event, value, if permission { 4 } else { 3 })
}
pub fn deliver_cpu_fault(frame: SretFrame, vector: u64, _error: u64) {
    let (event, kind) = cpu_fault_route(vector);
    deliver_fault_kind(frame, event, frame.rip, kind);
}
fn cpu_fault_route(vector: u64) -> (usize, u64) {
    match vector {
        6 => (2, 5),
        0 => (3, 6),
        16 | 19 => (3, 7),
        17 => (4, 8), // #AC -> SIGBUS/BUS_ADRALN through the personality route.
        _ => (1, 4),
    }
}
fn deliver_fault_kind(frame: SretFrame, event: usize, value: u64, kind: u64) -> bool {
    let task = sched::current();
    let slot = crate::signal::route(task, event);
    if slot == 0 {
        return false;
    }
    let handler = crate::signal::handler_of(task, slot);
    if handler <= 1 || crate::signal::mask(task) & (1 << (slot - 1)) != 0 {
        terminate_current(bootstrap::ProcessExit::Signaled(slot as u8));
    }
    crate::signal::queue_info(
        task,
        slot,
        crate::signal::Info {
            slot: slot as u64,
            sender: 0,
            kind,
            value,
        },
    );
    sched::set_frame(task, frame);
    sched::resume(task)
}

fn finish_process(leader: u32, status: bootstrap::ProcessExit) -> u64 {
    let cur = sched::current();
    let active = bootstrap::process_of(cur)
        .is_some_and(|p| bootstrap::process_leader(p) == Some(leader))
        || cur == leader;
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return encode_errno(Errno::Fault);
    };
    let outcome =
        match bootstrap::terminate_process(leader, status, sched::root(leader), offset, active) {
            Ok(outcome) => outcome,
            Err(error) => return encode_errno(task_destroy_errno(error)),
        };
    for (task, _) in outcome.wakes.tasks[..outcome.wakes.count].iter().flatten() {
        let ptr = bootstrap::wait_status_ptr(*task);
        sched::set_pending_status(*task, ptr, status);
        sched::set_frame_rax(*task, leader as u64);
    }
    for id in 1..sched::MAX_TASKS as u32 {
        if outcome.members & (1 << id) != 0 {
            sched::clear_task(id);
        }
    }
    if let Some(parent) = outcome.parent {
        notify_child_exit(parent, outcome.cookie, status);
    }
    if active {
        sched::exit_current();
    }
    0
}

fn terminate_current(status: bootstrap::ProcessExit) -> ! {
    let cur = sched::current();
    let leader = bootstrap::process_of(cur)
        .and_then(bootstrap::process_leader)
        .unwrap_or(cur);
    let error = finish_process(leader, status);
    // A fatal delivery cannot resume user execution after a failed teardown.
    panic!("fatal Process termination failed: {error:#x}")
}

fn process_exit_call(status: u64) -> u64 {
    let cur = sched::current();
    let leader = bootstrap::process_of(cur)
        .and_then(bootstrap::process_leader)
        .unwrap_or(cur);
    finish_process(leader, bootstrap::ProcessExit::Exited(status as u8))
}

fn process_terminate_call(raw_cap: u64, status: u64) -> u64 {
    let caller = sched::current();
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    if bootstrap::authorize(caller, cap, Right::TaskDestroy).is_err() {
        return encode_errno(Errno::Access);
    }
    let Some(target) = bootstrap::task_of_cap(cap) else {
        return encode_errno(Errno::NoEnt);
    };
    if bootstrap::task_is_thread(target) {
        return encode_errno(Errno::Invalid);
    }
    finish_process(target, bootstrap::ProcessExit::Exited(status as u8))
}

/// M16b：Linux `clone` 子集；复制调用者帧（rax=0、rsp=stack），
/// 参数顺序与 Linux 一致：`flags, stack, parent_tid, child_tid, tls`。
fn clone_call(flags: u64, stack_top: u64, parent_tid: u64, child_tid: u64, tls: u64) -> u64 {
    const CLONE_VM: u64 = 0x100;
    const CLONE_FS: u64 = 0x200;
    const CLONE_FILES: u64 = 0x400;
    const CLONE_SIGHAND: u64 = 0x800;
    const CLONE_THREAD: u64 = 0x1_0000;
    const CLONE_SYSVSEM: u64 = 0x4_0000;
    const CLONE_SETTLS: u64 = 0x8_0000;
    const CLONE_PARENT_SETTID: u64 = 0x10_0000;
    const CLONE_CHILD_CLEARTID: u64 = 0x20_0000;
    const CLONE_CHILD_SETTID: u64 = 0x100_0000;
    // musl 设置已废弃的 CLONE_DETACHED（Linux 忽略）。
    const CLONE_DETACHED: u64 = 0x40_0000;
    const ALLOWED: u64 = CLONE_VM
        | CLONE_FS
        | CLONE_FILES
        | CLONE_SIGHAND
        | CLONE_THREAD
        | CLONE_SYSVSEM
        | CLONE_SETTLS
        | CLONE_PARENT_SETTID
        | CLONE_CHILD_CLEARTID
        | CLONE_CHILD_SETTID
        | CLONE_DETACHED;
    if flags & !ALLOWED != 0 || flags & (CLONE_VM | CLONE_THREAD) != (CLONE_VM | CLONE_THREAD) {
        return encode_errno(Errno::Invalid);
    }
    let caller = sched::current();
    let Some(root) = sched::root(caller) else {
        return encode_errno(Errno::Fault);
    };
    if flags & CLONE_SETTLS != 0 && validate_fs_base(tls).is_err() {
        return encode_errno(Errno::Invalid);
    }
    if flags & CLONE_PARENT_SETTID != 0
        && let Err(error) = sched::prepare_wait_status(caller, parent_tid)
    {
        return encode_errno(error);
    }
    if flags & (CLONE_CHILD_SETTID | CLONE_CHILD_CLEARTID) != 0
        && let Err(error) = sched::prepare_wait_status(caller, child_tid)
    {
        return encode_errno(error);
    }
    let frame = current_context();
    let thread = match bootstrap::thread_create(caller, frame.rip, stack_top) {
        Ok(thread) => thread,
        Err(_) => {
            return encode_errno(Errno::NoSpace);
        }
    };
    let fs_base = if flags & CLONE_SETTLS != 0 {
        tls
    } else {
        sched::fs_base(caller).unwrap_or(0)
    };
    if crate::user::register_clone_task(thread, root, frame, stack_top, fs_base).is_err() {
        bootstrap::thread_abort(thread);
        sched::clear_task(thread);
        return encode_errno(Errno::Invalid);
    }
    crate::signal::bind_thread(caller, thread);
    if flags & CLONE_CHILD_CLEARTID != 0 && child_tid != 0 {
        bootstrap::set_clear_child_tid(thread, child_tid);
    }
    if flags & CLONE_CHILD_SETTID != 0 && child_tid != 0 {
        sched::write_user_word(caller, child_tid, thread.to_le_bytes())
            .expect("clone output preflight");
    }
    if flags & CLONE_PARENT_SETTID != 0 && parent_tid != 0 {
        sched::write_user_word(caller, parent_tid, thread.to_le_bytes())
            .expect("clone output preflight");
    }
    thread as u64
}

/// M16b：退出当前线程（不拆共享 root、不报告进程状态）。
fn thread_exit_call() -> u64 {
    let cur = sched::current();
    if !bootstrap::task_is_thread(cur) {
        return encode_errno(Errno::Invalid);
    }
    if let Err(error) = bootstrap::task_destroy(cur, cur, None, 0) {
        return encode_errno(task_destroy_errno(error));
    }
    sched::clear_task(cur);
    sched::exit_current()
}

fn thread_exit_unmap_call(raw_cap: u64, base: u64, pages: u64) -> u64 {
    let cur = sched::current();
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let Some(root) = sched::root(cur) else {
        return encode_errno(Errno::Fault);
    };
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return encode_errno(Errno::Fault);
    };
    if let Err(error) = bootstrap::thread_exit_unmap(cur, cap, base, pages as usize, root, offset) {
        return encode_errno(task_destroy_errno(error));
    }
    sched::clear_task(cur);
    sched::exit_current()
}

fn thread_create_call(entry: u64, stack_top: u64, tls: u64) -> u64 {
    let caller = sched::current();
    let Some(root) = sched::root(caller) else {
        return encode_errno(Errno::Fault);
    };
    let thread = match bootstrap::thread_create(caller, entry, stack_top) {
        Ok(thread) => thread,
        Err(_) => return encode_errno(Errno::NoSpace),
    };
    let self_cap = match bootstrap::mint_thread_self_cap(thread) {
        Ok(cap) => cap,
        Err(_) => {
            bootstrap::thread_abort(thread);
            return encode_errno(Errno::NoSpace);
        }
    };
    let fs_base = if tls == 0 {
        sched::fs_base(caller).unwrap_or(0)
    } else {
        tls
    };
    if crate::user::register_thread_task(
        thread,
        root,
        entry,
        stack_top,
        bootstrap::expose_user_handle(thread, self_cap).encode(),
        fs_base,
    )
    .is_err()
    {
        bootstrap::thread_abort(thread);
        sched::clear_task(thread);
        return encode_errno(Errno::Invalid);
    }
    crate::signal::bind_thread(caller, thread);
    thread as u64
}

fn wait_create_call() -> u64 {
    match bootstrap::wait_object_create(sched::current()) {
        Ok(handle) => encode_caller_handle(handle),
        Err(error) => encode_errno(wait_errno(error)),
    }
}

fn wait_observe_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::wait_object_observe(sched::current(), cap) {
        Ok(generation) => generation,
        Err(error) => encode_errno(wait_errno(error)),
    }
}

fn wait_signal_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::wait_object_signal(sched::current(), cap) {
        Ok(outcome) => {
            inject_wait_wakes(&outcome.wakes);
            // M16a：低 32 位 generation、高 32 位被唤醒数（futex wake 需要计数）。
            outcome.generation | ((outcome.wakes.count as u64) << 32)
        }
        Err(error) => encode_errno(wait_errno(error)),
    }
}

fn wait_destroy_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::wait_object_destroy(sched::current(), cap) {
        Ok(wakes) => {
            inject_wait_wakes(&wakes);
            0
        }
        Err(error) => encode_errno(wait_errno(error)),
    }
}

/// M15a-2：立即满足/立即超时；否则登记 BlockedWait 并切走。
fn wait_objects_call(
    cap0: u64,
    observed0: u64,
    cap1: u64,
    observed1: u64,
    mode: u64,
    timeout: u64,
    physical: bool,
) -> u64 {
    let Ok(first) = decode_caller_handle(cap0) else {
        return encode_errno(Errno::Invalid);
    };
    let second = if cap1 == 0 {
        crate::handle::Handle::ZERO
    } else {
        match decode_caller_handle(cap1) {
            Ok(handle) => handle,
            Err(_) => return encode_errno(Errno::Invalid),
        }
    };
    if physical && timeout != u64::MAX && !crate::preempt::start_time_counter() {
        return encode_errno(Errno::Invalid);
    }
    let model_timeout = if !physical {
        timeout
    } else if timeout <= crate::preempt::time_ticks() {
        0
    } else {
        u64::MAX
    };
    match bootstrap::wait_objects(
        sched::current(),
        first,
        observed0,
        second,
        observed1,
        mode,
        model_timeout,
    ) {
        Ok(bootstrap::WaitOutcome::Satisfied) => 0,
        Ok(bootstrap::WaitOutcome::TimedOut) => 1,
        Ok(bootstrap::WaitOutcome::Blocked) => {
            let cur = sched::current();
            if physical && timeout != u64::MAX {
                bootstrap::set_wait_physical_deadline(cur, timeout);
            }
            let mut frame = current_context();
            frame.rax = 0;
            sched::set_frame(cur, frame);
            mark_blocked_call(cur, current_context().rax);
            sched::block_current()
        }
        Err(error) => encode_errno(wait_errno(error)),
    }
}

fn task_wait_deadline_call(deadline: u64) -> u64 {
    if !crate::preempt::start_time_counter() {
        return encode_errno(Errno::Invalid);
    }
    let task = sched::current();
    if crate::signal::pending(task) != 0 {
        return encode_errno(Errno::Interrupted);
    }
    match bootstrap::wait_physical_deadline(task, deadline, crate::preempt::time_ticks()) {
        Ok(bootstrap::WaitOutcome::TimedOut) => 1,
        Ok(bootstrap::WaitOutcome::Satisfied) => 0,
        Ok(bootstrap::WaitOutcome::Blocked) => {
            let mut frame = current_context();
            frame.rax = 0;
            sched::set_frame(task, frame);
            mark_blocked_call(task, SYS_TASK_WAIT_DEADLINE);
            sched::block_current()
        }
        Err(error) => encode_errno(wait_errno(error)),
    }
}

/// SPEC-083：把 `audit(cap)` 的摘要打包进单个机器字（回执与 xtask 断言用）。
///
/// 字段布局：`chain_len`(8) | `minted`(8…15) | `derived`(16…23) | `moves`(24…31) |
/// `created`(32…39) | `destroyed`(40…47)。
fn pack_audit(trace: &AuditTrace) -> u64 {
    (trace.chain_len as u64)
        | ((trace.minted as u64) << 8)
        | ((trace.derived as u64) << 16)
        | ((trace.moves as u64) << 24)
        | ((trace.created as u64) << 32)
        | ((trace.destroyed as u64) << 40)
}

/// SPEC-083 / §13.2：`audit(cap)` 只查询；目标已销毁但只要 cap 仍被持有就仍接受。
fn audit_call(raw_cap: u64) -> u64 {
    let Ok(cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::NoEnt);
    };
    match bootstrap::audit_trace(sched::current(), cap) {
        Ok(trace) => pack_audit(&trace),
        Err(AuditError::InvalidCap) => encode_errno(Errno::NoEnt),
        Err(AuditError::InvalidCaller | AuditError::NotHeld) => encode_errno(Errno::Access),
    }
}

/// SPEC-082：ring3 Timer 回执的缓存。
///
/// `user_trap_exit` 在 task 1 的根下读出回执并存入此静态；`after_resume`（已切回内核根、
/// `USER_Y_VA` 不再映射）只读缓存。
static TIMER_RECEIPTS: StaticCell<[u64; 7]> = StaticCell::new([0; 7]);

/// SPEC-082：在 `user_trap_exit` 里缓存 Timer 回执（必须在 task 1 的地址空间下调用）。
pub fn cache_timer_receipts() {
    let base = USER_Y_VA;
    TIMER_RECEIPTS.with_mut(|slots| {
        slots[0] = read_user_qword(base + TIMER_RECEIPT_NOW);
        slots[1] = read_user_qword(base + TIMER_RECEIPT_T1);
        slots[2] = read_user_qword(base + TIMER_RECEIPT_T2);
        slots[3] = read_user_qword(base + TIMER_RECEIPT_T3);
        slots[4] = read_user_qword(base + TIMER_RECEIPT_CANCEL);
        slots[5] = read_user_qword(base + TIMER_RECEIPT_CANCEL_AGAIN);
        slots[6] = read_user_qword(base + TIMER_RECEIPT_DESTROY);
    });
}

/// SPEC-083：`audit(cap)` 回执的缓存（同样必须在 task 1 的地址空间下读出）。
static AUDIT_RECEIPTS: StaticCell<[u64; 4]> = StaticCell::new([0; 4]);

/// SPEC-083：在 `user_trap_exit` 里缓存 `audit(cap)` 回执。
///
/// `live`/`stale` 两步由 ring3 写入 Y 页；后两项复用 SPEC-081 已写好的 `unbind`/`copy_c`
/// 句柄（这两个 Capability 一个仍被持有、一个位置已变），作为内核侧重算的输入。
pub fn cache_audit_receipts() {
    AUDIT_RECEIPTS.with_mut(|slots| {
        slots[0] = read_user_qword(USER_Y_VA + AUDIT_RECEIPT_LIVE);
        slots[1] = read_user_qword(USER_Y_VA + AUDIT_RECEIPT_STALE);
        slots[2] = read_user_qword(USER_Y_VA + NS_RECEIPT_UNBIND);
        slots[3] = read_user_qword(USER_Y_VA + NS_RECEIPT_COPY_C);
    });
}

/// SPEC-083：`audit(cap)` 见证（缓存回执 + 内核侧重算摘要比对）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditWitness {
    pub chain_len: usize,
    pub derived: u32,
    pub moves: u32,
    pub created: u32,
    pub destroyed: u32,
    pub stale_ok: bool,
    pub purity_ok: bool,
    pub evidence_ok: bool,
}

pub fn audit_witness() -> AuditWitness {
    let receipts = AUDIT_RECEIPTS.with(|slots| *slots);
    let before = bootstrap::audit_len();
    let live = Handle::decode(receipts[2])
        .ok()
        .and_then(|cap| bootstrap::audit_trace(INIT_TASK_ID, cap).ok());
    let stale = Handle::decode(receipts[3])
        .ok()
        .and_then(|cap| bootstrap::audit_trace(INIT_TASK_ID, cap).ok());
    let purity_ok = before == bootstrap::audit_len();
    let Some(live) = live else {
        return AuditWitness {
            chain_len: 0,
            derived: 0,
            moves: 0,
            created: 0,
            destroyed: 0,
            stale_ok: false,
            purity_ok,
            evidence_ok: false,
        };
    };
    let stale_ok = stale.is_some_and(|trace| {
        !trace.target_live
            && trace.created == 1
            && trace.destroyed == 1
            && receipts[1] == pack_audit(&trace)
    });
    let evidence_ok = live.chain_len == 2
        && live.derived == 1
        && live.moves == 2
        && live.created == 1
        && live.destroyed == 0
        && live.target_live
        && receipts[0] == pack_audit(&live)
        && stale_ok
        && purity_ok;
    AuditWitness {
        chain_len: live.chain_len,
        derived: live.derived,
        moves: live.moves,
        created: live.created,
        destroyed: live.destroyed,
        stale_ok,
        purity_ok,
        evidence_ok,
    }
}

/// SPEC-082：ring3 Timer 见证（缓存回执 + 内核侧审计计数）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerWitness {
    pub now: u64,
    pub t1: u64,
    pub t2: u64,
    pub t3: u64,
    pub cancel: u64,
    pub cancel_again: u64,
    pub destroy: u64,
    pub audit: bootstrap::TimerAudit,
}

impl TimerWitness {
    /// 纯判定：一次完整的 Timer 见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.t1 != 0
            && self.t2 != 0
            && self.t3 != 0
            && self.cancel == TIMER_CANCEL_OUTCOME_CANCELLED
            && self.cancel_again == TIMER_CANCEL_OUTCOME_ALREADY_CANCELLED
            && self.destroy == 0
            && self.audit.sets == 3
            && self.audit.fired == 2
            && self.audit.immediate_fired == 1
            && self.audit.cancelled == 1
            && self.audit.destroyed == 1
            && self.audit.timers == 2
            && self.audit.already_cancelled == 1
            && self.audit.exactly_once_ok == 1
    }
}

/// 读取 Timer 见证（缓存回执 + 内核审计计数）；须在 `cache_timer_receipts` 之后调用。
pub fn timer_witness() -> TimerWitness {
    let receipts = TIMER_RECEIPTS.with(|slots| *slots);
    TimerWitness {
        now: receipts[0],
        t1: receipts[1],
        t2: receipts[2],
        t3: receipts[3],
        cancel: receipts[4],
        cancel_again: receipts[5],
        destroy: receipts[6],
        audit: bootstrap::timer_audit().unwrap_or_default(),
    }
}

// ---------------------------------------------------------------------------
// 切片 M：协作式多任务切换
// ---------------------------------------------------------------------------

/// 当前 `int 0x80` 现场的完整用户上下文。
///
/// 裸入口把保存块基址写入 [`prima_ostd::entry`] 的 `CONTEXT_BASE`；本函数把它展开为
/// [`SretFrame`]。**保存块与 `SretFrame` 的槽位不同**，逐槽映射由
/// [`prima_ostd::entry::SaveBlock::to_sret_frame`] 负责（切片 AH 下沉）。
///
/// 契约：只可在 `syscall_entry` 递送的处理函数内部调用（此时保存块必已写入）。
fn current_context() -> SretFrame {
    prima_ostd::entry::context_base()
        .expect("syscall handler runs inside an int 0x80 frame")
        .to_sret_frame()
}

/// 切片 M 的多任务实测记录（单核 boot 早期无并发）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MtState {
    switches: u64,
    last_from: u64,
    last_to: u64,
    task1_id_before: u64,
    task1_id_after: u64,
    task2_id: u64,
    task2_auth: u64,
    task2_sub: u64,
    task1_cs_before: u64,
    task1_cs_after: u64,
    task2_cs: u64,
}

static MT: StaticCell<MtState> = StaticCell::new(MtState {
    switches: 0,
    last_from: 0,
    last_to: 0,
    task1_id_before: 0,
    task1_id_after: 0,
    task2_id: 0,
    task2_auth: 0,
    task2_sub: 0,
    task1_cs_before: 0,
    task1_cs_after: 0,
    task2_cs: 0,
});

/// 切片 N：SPEC-051 多任务见证在 task 1 恢复后即固定下来，后续切片新增的切换不扰动它。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MtSnapshot {
    switches: u64,
    last_from: u64,
    last_to: u64,
}

static MT_SNAPSHOT: StaticCell<Option<MtSnapshot>> = StaticCell::new(None);

/// 记录一次非阶段一 syscall（task 1 阶段二或 task 2 的调用）。
fn mt_record(task: u32, num: u64, cs: u64, ret: u64) {
    MT.with_mut(|mt| {
        if task == INIT_TASK_ID {
            if num == SYS_TASK_ID {
                if mt.switches == 0 {
                    mt.task1_id_before = ret;
                    mt.task1_cs_before = cs;
                } else {
                    mt.task1_id_after = ret;
                    mt.task1_cs_after = cs;
                    // SPEC-051 证据在 task 1 恢复后封存。
                    MT_SNAPSHOT.with_mut(|snapshot| {
                        *snapshot = Some(MtSnapshot {
                            switches: mt.switches,
                            last_from: mt.last_from,
                            last_to: mt.last_to,
                        });
                    });
                }
            }
        } else if task == TASK2_ID {
            mt.task2_cs = cs;
            if num == SYS_TASK_ID {
                mt.task2_id = ret;
            } else if num == SYS_AUTHORIZE {
                mt.task2_auth = ret;
            } else if num == SYS_EXC_SUBSCRIBE {
                mt.task2_sub = ret;
            } else if num == SYS_TRAP {
                EXC_WAIT.with_mut(|wait| wait.cs_trap = cs);
            }
        }
    });
}

/// 记录一次协作切换（在状态迁移与恢复之前调用）。
fn mt_record_yield(from: u32, to: u32) {
    MT.with_mut(|mt| {
        mt.switches += 1;
        mt.last_from = from as u64;
        mt.last_to = to as u64;
    });
}

/// 读取多任务记录（单核，无并发）。
fn mt_state() -> MtState {
    MT.with(|mt| *mt)
}

/// 切片 N：raise/wait 的实测记录（单核，无并发）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ExcWaitState {
    /// `SYS_EXC_WAIT` 阻塞次数。
    blocks: u64,
    /// `SYS_EXC_WAIT` 进入时的 CS。
    cs_wait: u64,
    /// 触发 raise 的 `SYS_TRAP` 进入时的 CS。
    cs_trap: u64,
    source_id: u64,
    delivered: u64,
    pending: u64,
    woken: u64,
    payload: u64,
}

static EXC_WAIT: StaticCell<ExcWaitState> = StaticCell::new(ExcWaitState {
    blocks: 0,
    cs_wait: 0,
    cs_trap: 0,
    source_id: 0,
    delivered: 0,
    pending: 0,
    woken: 0,
    payload: 0,
});

/// 记录一次 `SYS_EXC_WAIT` 阻塞（在切走前调用）。
fn exc_wait_record(cs: u64, source_id: u32) {
    EXC_WAIT.with_mut(|wait| {
        wait.blocks += 1;
        wait.cs_wait = cs;
        wait.source_id = source_id as u64;
    });
}

/// 记录一次内核 actor raise 的结果。
fn exc_trap_record(outcome: &bootstrap::RaiseOutcome) {
    EXC_WAIT.with_mut(|wait| {
        wait.delivered = outcome.delivered as u64;
        wait.pending = outcome.pending as u64;
        wait.woken = outcome.woken.map(|task| task as u64).unwrap_or(0);
        wait.payload = outcome.payload;
    });
}

fn exc_wait_state() -> ExcWaitState {
    EXC_WAIT.with(|wait| *wait)
}

/// `SYS_TRAP`：内核 actor 在 syscall trap 线性化点 raise `SyscallTrap`（§12.5）。
///
/// 若某订阅者正处于 `BlockedRecv`，raise 会把通知写入其 pending 并把 payload 注入其
/// 保存帧的 `rax`，使其 `SYS_EXC_WAIT` 在恢复时直接返回 payload。
pub(crate) fn complete_irq_wakes(outcome: &bootstrap::RaiseOutcome) {
    for task in 1..sched::MAX_TASKS as u32 {
        if outcome.woken_tasks & (1 << task) != 0 && sched::recv_out(task).is_none() {
            let _ = bootstrap::take_pending_global(task);
            sched::set_frame_rax(task, outcome.payload);
        }
    }
}

fn trap_call() -> u64 {
    match bootstrap::exc_raise(bootstrap::SYSCALL_TRAP_SOURCE_ID, ExcKind::SyscallTrap) {
        Ok(outcome) => {
            complete_irq_wakes(&outcome);
            exc_trap_record(&outcome);
            outcome.delivered as u64
        }
        Err(error) => encode_errno(exc_errno(error)),
    }
}

/// `SYS_EXC_WAIT`：已订阅则阻塞等待通知；被 raise 唤醒时 payload 已在保存帧 `rax` 中。
fn wait_call(cap_id: u64, kind_code: u64) -> u64 {
    let Some(kind) = kind_of(kind_code) else {
        return encode_errno(Errno::Invalid);
    };
    let cur = sched::current();
    let cap = match decode_caller_handle(cap_id) {
        Ok(cap) => cap,
        Err(_) => return encode_errno(Errno::Invalid),
    };
    let source_id = match bootstrap::exc_subscribed(cur, cap, kind) {
        Ok(source_id) => source_id,
        Err(error) => return encode_errno(exc_errno(error)),
    };
    let mut frame = current_context();
    frame.rax = 0;
    sched::set_frame(cur, frame);
    exc_wait_record(frame.cs, source_id);
    if bootstrap::block_recv_global(cur).is_err() {
        return encode_errno(Errno::NoEnt);
    }
    sched::block_current()
}

/// `SYS_YIELD`：保存当前任务完整上下文，切到下一个 Ready 任务（不返回给当前调用者）。
///
/// 当前 `syscall_entry` 栈帧被有意放弃——下一次 CPL3→CPL0 时硬件按 `TSS.rsp0` 重置 `rsp`，
/// 不会泄漏；被让出的任务在其被重新调度时从保存的 `SretFrame` 直接恢复，`yield` 返回 0。
fn yield_call() -> u64 {
    let cur = sched::current();
    let mut frame = current_context();
    frame.rax = 0;
    sched::set_frame(cur, frame);
    let Some(next) = sched::pick_next(cur) else {
        return 0;
    };
    mt_record_yield(cur, next);
    sched::switch_to(next)
}

/// S2-D3：`memseg_create` 服务。
fn memseg_create_call(page_count: u64, access_bits: u64) -> u64 {
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return encode_errno(Errno::Fault);
    };
    let access = Rights::from_bits(access_bits);
    match bootstrap::memseg_create(sched::current(), page_count as usize, access, offset) {
        Ok(handle) => encode_caller_handle(handle),
        Err(error) => encode_errno(memseg_errno(error)),
    }
}

/// S2-D3：`memseg_map` 服务。
///
/// 切片 S：目标根由 **task cap 的目标**派生（`sched::root(task_id)`），不再假定等于当前根——
/// 否则「逻辑视图记在 Task X 名下、页表却改当前任务」的错配无任何检查拦截。今天所有用户
/// 可达的 `TaskManageVSpace` cap 都指向持有者自身，故行为不变。
fn memseg_map_call(
    raw_seg: u64,
    raw_task: u64,
    seg_offset: u64,
    target_base: u64,
    page_count: u64,
    access_bits: u64,
) -> u64 {
    let (Ok(seg_cap), Ok(task_cap)) = (
        decode_caller_handle(raw_seg),
        decode_caller_handle(raw_task),
    ) else {
        return encode_errno(Errno::Invalid);
    };
    let (Some(task_id), Some((_, offset))) = (
        bootstrap::task_of_cap(task_cap),
        crate::cow::root_and_offset(),
    ) else {
        return encode_errno(Errno::NoEnt);
    };
    let Some(root) = sched::root(task_id) else {
        return encode_errno(Errno::NoEnt);
    };
    let access = Rights::from_bits(access_bits);
    match bootstrap::memseg_map(
        sched::current(),
        seg_cap,
        task_cap,
        seg_offset,
        target_base,
        page_count as usize,
        access,
        root,
        offset,
    ) {
        Ok(()) => 0,
        Err(error) => encode_errno(memseg_errno(error)),
    }
}
fn memseg_unmap_call(raw_task: u64, target_base: u64, page_count: u64) -> u64 {
    let Ok(task_cap) = decode_caller_handle(raw_task) else {
        return encode_errno(Errno::Invalid);
    };
    let (Some(task_id), Some((_, offset))) = (
        bootstrap::task_of_cap(task_cap),
        crate::cow::root_and_offset(),
    ) else {
        return encode_errno(Errno::NoEnt);
    };
    let Some(root) = sched::root(task_id) else {
        return encode_errno(Errno::NoEnt);
    };
    match bootstrap::memseg_unmap(
        sched::current(),
        task_cap,
        target_base,
        page_count as usize,
        root,
        offset,
    ) {
        Ok(()) => 0,
        Err(error) => encode_errno(memseg_errno(error)),
    }
}

/// COSIR §7.4 / M10：`memseg_protect` 服务。
fn memseg_protect_call(raw_task: u64, target_base: u64, page_count: u64, access_bits: u64) -> u64 {
    let Ok(task_cap) = decode_caller_handle(raw_task) else {
        return encode_errno(Errno::Invalid);
    };
    let (Some(task_id), Some((_, offset))) = (
        bootstrap::task_of_cap(task_cap),
        crate::cow::root_and_offset(),
    ) else {
        return encode_errno(Errno::NoEnt);
    };
    let Some(root) = sched::root(task_id) else {
        return encode_errno(Errno::NoEnt);
    };
    match bootstrap::memseg_protect(
        sched::current(),
        task_cap,
        target_base,
        page_count as usize,
        Rights::from_bits(access_bits),
        root,
        offset,
    ) {
        Ok(()) => 0,
        Err(error) => encode_errno(memseg_errno(error)),
    }
}

/// S2-D3：`memseg_destroy` 服务。
fn memseg_destroy_call(raw_seg: u64) -> u64 {
    let Ok(seg_cap) = decode_caller_handle(raw_seg) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::memseg_destroy(sched::current(), seg_cap) {
        Ok(()) => 0,
        Err(error) => encode_errno(memseg_errno(error)),
    }
}

// ---------------------------------------------------------------------------
// 切片 S：IPC 用户化（S2-D5）与两任务共享 Segment
// ---------------------------------------------------------------------------

/// 切片 S：一次运行内的投递路径计数（证明排队与直投两条路径都被真实走到）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct IpcPathState {
    queued: u64,
    direct: u64,
    blocks: u64,
    kernel: u64,
    uart: u64,
    interrupted: u64,
    retired: u64,
    faults: u64,
}

static IPC_PATHS: StaticCell<IpcPathState> = StaticCell::new(IpcPathState {
    queued: 0,
    direct: 0,
    blocks: 0,
    kernel: 0,
    uart: 0,
    interrupted: 0,
    retired: 0,
    faults: 0,
});

/// Non-SEND endings of an outstanding SYS_RECV(_META), counted separately
/// from user message delivery. An IRQ can finish a receive without a SEND.
pub(crate) enum RecvEnd {
    Kernel,
    Uart,
    Interrupted,
    Retired,
    Fault,
}

pub(crate) fn end_pending_recv(task: u32, reason: RecvEnd) {
    if sched::recv_out(task).is_some() {
        IPC_PATHS.with_mut(|paths| match reason {
            RecvEnd::Kernel => paths.kernel += 1,
            RecvEnd::Uart => {
                paths.kernel += 1;
                paths.uart += 1;
            }
            RecvEnd::Interrupted => paths.interrupted += 1,
            RecvEnd::Retired => paths.retired += 1,
            RecvEnd::Fault => paths.faults += 1,
        });
    }
    sched::clear_recv_out(task);
}

/// Every blocked receive either returned through a classified path, was
/// retired with its task, or still has a saved output pointer at the snapshot.
pub fn ipc_receive_counts() -> [u64; 6] {
    let paths = ipc_paths();
    let live = (0..sched::MAX_TASKS as u32)
        .filter(|task| sched::recv_out(*task).is_some())
        .count() as u64;
    [
        paths.kernel,
        paths.uart,
        paths.interrupted,
        paths.retired,
        paths.faults,
        live,
    ]
}

fn ipc_paths() -> IpcPathState {
    IPC_PATHS.with(|paths| *paths)
}

fn ipc_path_record(direct: bool) {
    IPC_PATHS.with_mut(|paths| {
        if direct {
            paths.direct += 1;
        } else {
            paths.queued += 1;
        }
    });
}

/// `SYS_SEND`：经授权把一条（可携带 Capability 的）消息投递给 `receiver`。
///
/// **授权绑定接收者**：`send_cap` 必须是指向 `receiver` 且含 `TaskSend` 的 Task Capability。
/// 阻塞接收者的 out 缓冲由 SYS_RECV 保存；直投前验证其映射，成功后同时写回句柄。
fn send_call(
    raw_receiver: u64,
    raw_send_cap: u64,
    payload: u64,
    cap_count: u64,
    raw_cap0: u64,
    raw_cap1: u64,
) -> u64 {
    let sender = sched::current();
    let Ok(receiver) = u32::try_from(raw_receiver) else {
        return encode_errno(Errno::Invalid);
    };
    let Ok(send_cap) = decode_caller_handle(raw_send_cap) else {
        return encode_errno(Errno::Invalid);
    };
    if cap_count > bootstrap::MAX_MESSAGE_CAPABILITIES as u64 {
        return encode_errno(Errno::Invalid);
    }
    // 授权先行：未授权者不应从错误码差异推断接收者当前状态。
    if bootstrap::authorize(sender, send_cap, Right::TaskSend).is_err()
        || bootstrap::task_of_cap(send_cap) != Some(receiver)
    {
        return encode_errno(Errno::Access);
    }
    let was_blocked = bootstrap::is_blocked_recv_global(receiver);
    let direct_out = if was_blocked && (cap_count > 0 || sched::recv_extended(receiver)) {
        let Some(ptr) = sched::recv_out(receiver) else {
            return encode_errno(Errno::Invalid);
        };
        match recv_out_phys(receiver, ptr) {
            Ok(phys) => Some(phys),
            Err(errno) => return encode_errno(errno),
        }
    } else {
        None
    };
    let mut caps = [Handle::ZERO; bootstrap::MAX_MESSAGE_CAPABILITIES];
    for (index, raw) in [raw_cap0, raw_cap1]
        .into_iter()
        .take(cap_count as usize)
        .enumerate()
    {
        let Ok(cap) = decode_caller_handle(raw) else {
            return encode_errno(Errno::Invalid);
        };
        caps[index] = cap;
    }
    match bootstrap::send_message_with_capabilities(
        sender,
        receiver,
        send_cap,
        payload,
        &caps[..cap_count as usize],
    ) {
        Ok(()) => {
            if was_blocked {
                // 直投：吃掉 pending（否则后续 `SYS_RECV` 会重复投递同一消息），并把 payload
                // 注入被唤醒任务的保存帧 `rax`，使其阻塞中的 `SYS_RECV` 直接返回 payload。
                let message = bootstrap::take_pending_global(receiver)
                    .expect("successful direct send must leave a pending message");
                if let Some(out) = direct_out {
                    write_recv_out(receiver, out, Some(&message));
                }
                sched::clear_recv_out(receiver);
                sched::set_frame_rax(receiver, payload);
            }
            ipc_path_record(was_blocked);
            0
        }
        Err(error) => encode_errno(ipc_errno(error)),
    }
}

/// `SYS_RECV` 的 out 缓冲边界：用户低半区、24 字节、8 对齐、不得跨页。
pub const fn recv_out_bounds(ptr: u64) -> Result<(), Errno> {
    if ptr >= (1u64 << 47) || !ptr.is_multiple_of(8) || (ptr & 0xfff) + RECV_BUF_LEN > 0x1000 {
        return Err(Errno::Fault);
    }
    Ok(())
}

struct RecvOutPhys {
    output: vspace::PreparedUserWrite,
    offset: u64,
    len: usize,
}

pub const fn recv_extended_bounds(ptr: u64) -> Result<(), Errno> {
    if ptr == 0 || !ptr.is_multiple_of(8) || ptr > (1u64 << 47) - 40 {
        return Err(Errno::Fault);
    }
    Ok(())
}

/// Preflight the full output before consuming a message or moving any token.
fn prepare_recv_out(receiver: u32, ptr: u64, extended: bool) -> Result<RecvOutPhys, Errno> {
    if extended {
        recv_extended_bounds(ptr)?;
    } else {
        recv_out_bounds(ptr)?;
    }
    let len = if extended { 5 } else { 3 };
    let output = prepare_task_output(receiver, ptr, len * 8)?;
    let (_, offset) = crate::cow::root_and_offset().ok_or(Errno::Fault)?;
    Ok(RecvOutPhys {
        output,
        offset,
        len,
    })
}

fn recv_out_phys(receiver: u32, ptr: u64) -> Result<RecvOutPhys, Errno> {
    prepare_recv_out(receiver, ptr, sched::recv_extended(receiver))
}

#[cfg(test)]
fn recv_out_phys_from_pte(ptr: u64, pte: u64, offset: u64) -> Result<u64, Errno> {
    recv_out_bounds(ptr)?;
    if pte & (PT_PRESENT | PT_USER | PT_WRITABLE) != (PT_PRESENT | PT_USER | PT_WRITABLE) {
        return Err(Errno::Fault);
    }
    Ok(phys_to_virt((pte & PT_ADDR_MASK) + (ptr & 0xfff), offset))
}

fn write_recv_out(receiver: u32, out: RecvOutPhys, message: Option<&bootstrap::IpcMessage>) {
    let mut values = [0u64; 5];
    if let Some(message) = message {
        for (index, cap) in message.capabilities[..message.capability_count]
            .iter()
            .enumerate()
        {
            values[index] = bootstrap::expose_user_handle(receiver, *cap).encode();
        }
        values[2] = message.capability_count as u64;
        values[3] = ((message.sender_generation as u64) << 32) | message.sender as u64;
        values[4] = message.id as u64;
    }
    let mut bytes = [0; 40];
    for (index, value) in values[..out.len].iter().enumerate() {
        bytes[index * 8..index * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    out.output
        .write(&bytes[..out.len * 8], |phys| phys_to_virt(phys, out.offset));
}

/// Kernel-origin direct deliveries also complete a pending receive on resume.
/// User sends materialize immediately after their atomic send preflight.
pub fn complete_pending_recv(task: u32) {
    let Some(ptr) = sched::recv_out(task) else {
        return;
    };
    let out = match recv_out_phys(task, ptr) {
        Ok(out) => out,
        Err(error) => {
            end_pending_recv(task, RecvEnd::Fault);
            sched::set_frame_rax(task, encode_errno(error));
            return;
        }
    };
    if let Some(message) = bootstrap::take_pending_global(task) {
        write_recv_out(task, out, Some(&message));
        let uart = message.sender == bootstrap::KERNEL_ACTOR
            && bootstrap::irq_source(4).is_some_and(|source| {
                message.payload
                    == bootstrap::exc_notification_payload(source, ExcKind::ExternalInterrupt)
            });
        end_pending_recv(task, if uart { RecvEnd::Uart } else { RecvEnd::Kernel });
        sched::set_frame_rax(task, message.payload);
    }
}

fn recv_call(out_ptr: u64, format: u64) -> u64 {
    if format > 1 {
        return encode_errno(Errno::Invalid);
    }
    let cur = sched::current();
    let out = match prepare_recv_out(cur, out_ptr, format == 1) {
        Ok(out) => out,
        Err(error) => return encode_errno(error),
    };
    sched::set_recv_extended(cur, format == 1);
    match bootstrap::receive_message(cur) {
        Ok(message) => {
            write_recv_out(cur, out, Some(&message));
            sched::clear_recv_out(cur);
            message.payload
        }
        Err(bootstrap::IpcError::QueueEmpty) => {
            write_recv_out(cur, out, None);
            let mut frame = current_context();
            frame.rax = 0;
            sched::set_frame(cur, frame);
            mark_blocked_call(cur, if format == 1 { SYS_RECV_META } else { SYS_RECV });
            sched::set_recv_out(cur, out_ptr);
            IPC_PATHS.with_mut(|paths| paths.blocks += 1);
            sched::block_current()
        }
        Err(error) => encode_errno(ipc_errno(error)),
    }
}

/// 切片 S：IPC 用户化的用户态回执（task 1 读 Y，task 2 读 Y2）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpcWitness {
    pub create: u64,
    pub map1: u64,
    pub send1: u64,
    pub send2: u64,
    pub recv1: u64,
    pub cap0: u64,
    pub magic: u64,
    pub map2: u64,
    pub cross: u64,
    pub send_denied: u64,
    pub recv2: u64,
    pub cap_count: u64,
    pub direct_cap0: u64,
    pub direct_cap_count: u64,
    pub queued: u64,
    pub direct: u64,
    pub blocks: u64,
}

impl IpcWitness {
    /// 判定：排队路径转交共享段 cap，直投路径向阻塞接收者转交异常源 cap；
    /// task 2 读回共享帧魔数，跨任务映射与未授权发送均被 `EACCES` 拒绝。
    pub fn evidence_ok(&self) -> bool {
        let denied = encode_errno(Errno::Access);
        self.create == SHARED_SEG_CAP_HANDLE
            && self.map1 == 0
            && self.send1 == 0
            && self.send2 == 0
            && self.recv1 == IPC_SHARED_MAGIC
            && self.cap0 == SHARED_SEG_CAP_HANDLE
            && self.magic == IPC_SHARED_MAGIC
            && self.map2 == 0
            && self.cross == denied
            && self.send_denied == denied
            && self.recv2 == IPC_DIRECT_PAYLOAD
            && self.cap_count == 1
            && self.direct_cap0 == EXC_PAGE_FAULT_CAP
            && self.direct_cap_count == 1
            && self.queued == 1
            && self.direct == 1
            && self.blocks == 1
    }
}

/// 读取 IPC 见证（task 1 的回执在 Y 页；task 2 的在 Y2 页，需按物理读）。
pub fn ipc_witness() -> IpcWitness {
    let base = USER_Y_VA;
    let paths = ipc_paths();
    IpcWitness {
        create: read_qword(base + IPC_RECEIPT_CREATE),
        map1: read_qword(base + IPC_RECEIPT_MAP1),
        send1: read_qword(base + IPC_RECEIPT_SEND1),
        send2: read_qword(base + IPC_RECEIPT_SEND2),
        recv1: read_task2_qword(IPC2_RECEIPT_RECV1),
        cap0: read_task2_qword(IPC2_RECEIPT_CAP0),
        magic: read_task2_qword(IPC2_RECEIPT_MAGIC),
        map2: read_task2_qword(IPC2_RECEIPT_MAP2),
        cross: read_task2_qword(IPC2_RECEIPT_CROSS),
        send_denied: read_task2_qword(IPC2_RECEIPT_SEND_DENIED),
        recv2: read_task2_qword(IPC2_RECEIPT_RECV2),
        cap_count: read_task2_qword(IPC2_RECEIPT_CAPCOUNT),
        direct_cap0: read_task2_qword(USER2_RECV_BUF2 - crate::user::USER2_Y_VA),
        direct_cap_count: read_task2_qword(USER2_RECV_BUF2 - crate::user::USER2_Y_VA + 16),
        queued: paths.queued,
        direct: paths.direct,
        blocks: paths.blocks,
    }
}

// ---------------------------------------------------------------------------
// S2-D4：Task create/yield/destroy 用户原语（单核调度）
// ---------------------------------------------------------------------------

/// S2-D4：`task_create` 的请求块边界——8 对齐且不跨页（纯函数，host 可测）。
pub const fn task_req_bounds(ptr: u64) -> Result<(), Errno> {
    if !ptr.is_multiple_of(8) || (ptr & 0xfff) + TASK_CREATE_REQ_LEN > 0x1000 {
        return Err(Errno::Fault);
    }
    Ok(())
}

/// 请求块必须是当前任务根下 present|user 的页。
fn task_req_ok(ptr: u64) -> Result<(), Errno> {
    task_req_bounds(ptr)?;
    let Some((fallback_root, offset)) = crate::cow::root_and_offset() else {
        return Err(Errno::Fault);
    };
    let root = crate::sched::current_root().unwrap_or(fallback_root);
    let map = |phys: u64| phys_to_virt(phys, offset);
    let pte = vspace::read_leaf(root, ptr, map).map_err(|_| Errno::Fault)?;
    if pte & PT_PRESENT == 0 || pte & PT_USER == 0 {
        return Err(Errno::Fault);
    }
    Ok(())
}

/// 句柄字段解码：`0` 表示「未提供」（`initial_caps` 的缺省），其余必须是合法句柄。
fn decode_optional_handle(raw: u64) -> Result<Handle, Errno> {
    if raw == 0 {
        return Ok(Handle::ZERO);
    }
    decode_caller_handle(raw).map_err(|_| Errno::Invalid)
}

/// 读取并解码 `task_create` 请求块。
fn read_task_req(ptr: u64) -> Result<bootstrap::TaskCreateRequest, Errno> {
    task_req_ok(ptr)?;
    let mut words = [0u64; TASK_CREATE_REQ_QWORDS];
    for (index, word) in words.iter_mut().enumerate() {
        *word = prima_ostd::mm::read_u64_at(ptr, index as u64);
    }
    Ok(bootstrap::TaskCreateRequest {
        code_seg_cap: decode_optional_handle(words[0])?,
        code_offset: words[1],
        code_pages: words[2],
        load_base: words[3],
        entry: words[4],
        stack_top: words[5],
        parent_ns_cap: decode_optional_handle(words[6])?,
        root_rights: Rights::from_bits(words[7]),
        initial_caps: [
            decode_optional_handle(words[8])?,
            decode_optional_handle(words[9])?,
        ],
        // SPEC-101：运行期指派的服务名（0 = 不指派）。
        name: words[10],
    })
}

/// §8.1 错误名 → errno。
pub const fn task_create_errno(error: bootstrap::TaskCreateError) -> Errno {
    use bootstrap::TaskCreateError as E;
    match error {
        E::NoSuchCapability => Errno::NoEnt,
        E::NotHolder => Errno::Access,
        E::InvalidCodeRange => Errno::Invalid,
        E::InvalidEntry => Errno::Invalid,
        E::InvalidNamespace => Errno::Access,
        E::MissingRight => Errno::Access,
        E::RightsExceeded => Errno::Access,
        E::InvalidInitialCap => Errno::Access,
        E::DuplicateCap => Errno::Invalid,
        E::StructuralCap => Errno::Access,
        E::CapacityExceeded => Errno::NoSpace,
        E::NoMemory => Errno::NoSpace,
        E::AuditFull => Errno::NoSpace,
    }
}

/// §8.2 错误名 → errno。
pub const fn task_destroy_errno(error: bootstrap::TaskError) -> Errno {
    use bootstrap::TaskError as E;
    match error {
        E::NoSuchTask => Errno::NoEnt,
        E::HasLiveThreads => Errno::Again,
        E::CannotDestroyInit | E::CannotDestroyRunning => Errno::Access,
        E::DuplicateTask
        | E::AlreadyRunning
        | E::NotRunning
        | E::NotReady
        | E::CapacityExceeded
        // SPEC-072：内核回收路径只由 KernelActor 走，用户面不会产生该错误；此处映射仅为穷尽性。
        | E::NotUnrecoverable => Errno::Invalid,
        E::AuditFull => Errno::NoSpace,
    }
}

/// S2-D4：内核侧记录的新 Task 事实（供见证读取）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TaskState {
    /// 首次 `SYS_TASK_CREATE` 选定的 task id（切片 T 的子任务）。
    child: u32,
    stack_top: u64,
    /// 切片 V：第二次 `SYS_TASK_CREATE` 选定的 task id（自毁见证载体）。
    grandchild: u32,
    /// 本任务发起过的 `SYS_TASK_CREATE` 次数（区分 child / grandchild）。
    create_count: u32,
}

static TASK_STATE: StaticCell<TaskState> = StaticCell::new(TaskState {
    child: 0,
    stack_top: 0,
    grandchild: 0,
    create_count: 0,
});

fn task_state() -> TaskState {
    TASK_STATE.with(|state| *state)
}

/// `SYS_TASK_CREATE`：§8.1 完整成功动作 + 调度登记。
fn task_create_call(req_ptr: u64) -> u64 {
    let caller = sched::current();
    let req = match read_task_req(req_ptr) {
        Ok(req) => req,
        Err(errno) => return encode_errno(errno),
    };
    let Some((boot_root, offset)) = crate::cow::root_and_offset() else {
        return encode_errno(Errno::Fault);
    };
    match bootstrap::task_create(caller, &req, boot_root, offset) {
        Ok(outcome) => {
            // 防御分支：`next_free_id` 返回 `2..MAX_TASKS(=12)`，而内核栈数组只覆盖 id 3..8
            // （`user::USER_TASK_KSTACK_COUNT`）。保留为防御分支，不静默吞掉——若将来分配
            // id ≥ 9，这里报错而不是让新任务带着空内核栈被调度。
            if crate::user::register_created_task(
                outcome.task_id,
                outcome.root_phys,
                req.entry,
                req.stack_top,
                outcome.self_cap.encode(),
                req.initial_caps.map(Handle::encode),
                req.name,
            )
            .is_err()
            {
                return encode_errno(Errno::NoSpace);
            }
            TASK_STATE.with_mut(|state| {
                // 只记前两次 create：`child`（task 3）与 `grandchild`（task 4，自毁载体）。
                // SPEC-087 之后 init 还会 create 一个存活的 Rust 客户端（第 3 次），若继续
                // 覆盖 `grandchild`，`task4_recv`/`root4_cleared` 会去核验错误的 task。
                if state.create_count == 0 {
                    state.child = outcome.task_id;
                } else if state.create_count == 1 {
                    state.grandchild = outcome.task_id;
                }
                state.create_count += 1;
                state.stack_top = req.stack_top;
            });
            encode_caller_handle(outcome.manager_cap)
        }
        Err(error) => encode_errno(task_create_errno(error)),
    }
}

/// `SYS_TASK_DESTROY`：§8.2 完整清理（含地址空间拆除），成功后清调度登记。
///
/// 切片 V：目标为调用者自身时走自毁——**先确认存在 Ready 后继**（细化层限制：自毁要求系统
/// 在自毁者之外至少有一个可运行任务），否则确定性 `EAGAIN` 且不改状态；随后清理、清调度登记，
/// 最后 `exit_to` 切走（`-> !`）。自毁者的页表帧由 `sched::flush_deferred_frames` 在
/// `write_cr3` 之后归还。
fn task_destroy_call(raw_cap: u64) -> u64 {
    let caller = sched::current();
    let Ok(task_cap) = decode_caller_handle(raw_cap) else {
        return encode_errno(Errno::Invalid);
    };
    let task_id = match bootstrap::authorize(caller, task_cap, Right::TaskDestroy) {
        Ok(_) => match bootstrap::task_of_cap(task_cap) {
            Some(task_id) => task_id,
            None => return encode_errno(Errno::NoEnt),
        },
        Err(_) => return encode_errno(Errno::Access),
    };
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return encode_errno(Errno::Fault);
    };
    let self_destroy = task_id == caller;
    // M16b：非 leader 线程退出不拆共享地址空间（root 归 leader/Process）。
    let root = if bootstrap::task_is_thread(task_id) {
        None
    } else {
        crate::sched::root(task_id)
    };
    match bootstrap::task_destroy(caller, task_id, root, offset) {
        Ok(()) if self_destroy => {
            crate::sched::clear_task(caller);
            crate::sched::exit_current()
        }
        Ok(()) => {
            crate::sched::clear_task(task_id);
            0
        }
        Err(error) => encode_errno(task_destroy_errno(error)),
    }
}

/// S2-D4：S2-D4 的用户态回执 + 内核侧事实。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskWitness {
    pub stack_seg: u64,
    pub create: u64,
    pub bad_entry: u64,
    pub no_x: u64,
    pub stack_map: u64,
    pub recv: u64,
    pub deny: u64,
    pub destroy: u64,
    pub again: u64,
    pub map_dead: u64,
    /// SPEC-066：自持平台段 cap 经 `SYS_MEMSEG_MAP` 建立新映射的返回值（应为 0）。
    pub map_own: u64,
    /// SPEC-066：该映射首 qword 的读回值（应为 X 段共享帧的 COW 模式值）。
    pub map_value: u64,
    /// SPEC-066：用非属主（task 2 所属）段 cap 映射的返回值（应为 `EACCES`）。
    pub map_other: u64,
    /// SPEC-067：第二次 `task_create` 返回的 task4 manager cap。
    pub task4_cap: u64,
    /// SPEC-067：task4 的栈映射回执（应为 0）。
    pub task4_stack: u64,
    /// SPEC-067：task4 自报的 id（应为 4——id 不复用已销毁的 task 3）。
    pub task4_recv: u64,
    /// SPEC-067：向已自毁的 task4 映射的返回值（应为 `ENOENT`）。
    pub task4_map_dead: u64,
    /// 内核侧：`task_create` 实际选定的子任务 id。
    pub child: u32,
    /// 内核侧：事件发射时的活动任务数（应为 init + task 2）。
    pub tasks: u64,
    /// 内核侧：事件发射时的活动 cap 槽位数。
    pub caps: u64,
    /// 内核侧：子任务被销毁后其调度登记（root）确已清除。
    pub root_cleared: u64,
    /// 内核侧：自毁的 task4 被 `task_create` 选定的 id（应为 4）。
    pub grandchild: u32,
    /// 内核侧：task4 自毁后的活动任务数（应为 init + task 2 = 2）。
    pub tasks_after: u64,
    /// 内核侧：task4 自毁后的活动 cap 槽位数。
    pub caps_after: u64,
    /// 内核侧：task4 自毁后其调度登记确已清除。
    pub root4_cleared: u64,
}

impl TaskWitness {
    /// 判定：完整 create（manager cap + 栈映射进子任务根）→ 子任务自报 id 并回发消息 →
    /// 两个创建负例与一个销毁负例被拒 → 销毁成功 → 重复销毁与向已销毁任务映射均被拒 →
    /// SPEC-066：自持平台段 cap 的映射成功且读回该段帧内容，非属主同请求被 `EACCES` 拒绝 →
    /// 内核侧确认目标已从任务表与调度登记移除。
    pub fn evidence_ok(&self) -> bool {
        self.create == TASK_MANAGER_CAP
            && self.bad_entry == encode_errno(Errno::Invalid)
            && self.no_x == encode_errno(Errno::Access)
            && self.stack_map == 0
            && self.recv == self.child as u64
            && self.deny == encode_errno(Errno::Access)
            && self.destroy == 0
            && self.again != 0
            && self.map_dead != 0
            && self.map_own == 0
            && self.map_value == crate::cow::pattern(0)
            && self.map_other == encode_errno(Errno::Access)
            && self.child == TASK_CREATE_CHILD_ID
            && self.tasks == 2
            && self.root_cleared == 1
            && self.stack_seg != 0
            // SPEC-067：task4 承载自毁——栈映射成功、自报 id（id 不复用）、自毁后不可映射。
            && self.task4_cap != 0
            && self.task4_stack == 0
            && self.task4_recv == self.grandchild as u64
            && self.task4_map_dead == encode_errno(Errno::NoEnt)
            && self.grandchild == TASK_CREATE_CHILD_ID + 1
            && self.tasks_after == 2
            && self.root4_cleared == 1
    }
}

/// S2-D4：读取 Task 原语见证（回执读自 Y 页；内核侧事实读自内核状态）。
pub fn task_witness() -> TaskWitness {
    let base = USER_Y_VA;
    let state = task_state();
    let child = state.child;
    TaskWitness {
        stack_seg: read_qword(base + TASK_RECEIPT_STACK_SEG),
        create: read_qword(base + TASK_RECEIPT_CREATE),
        bad_entry: read_qword(base + TASK_RECEIPT_BAD_ENTRY),
        no_x: read_qword(base + TASK_RECEIPT_NO_X),
        stack_map: read_qword(base + TASK_RECEIPT_STACK_MAP),
        recv: read_qword(base + TASK_RECEIPT_RECV),
        deny: read_qword(base + TASK_RECEIPT_DENY),
        destroy: read_qword(base + TASK_RECEIPT_DESTROY),
        again: read_qword(base + TASK_RECEIPT_AGAIN),
        map_dead: read_qword(base + TASK_RECEIPT_MAP_DEAD),
        map_own: read_qword(base + TASK_RECEIPT_MAP_OWN),
        map_value: read_qword(base + TASK_RECEIPT_MAP_VALUE),
        map_other: read_qword(base + TASK_RECEIPT_MAP_OTHER),
        task4_cap: read_qword(base + TASK_RECEIPT_TASK4_CAP),
        task4_stack: read_qword(base + TASK_RECEIPT_TASK4_STACK),
        task4_recv: read_qword(base + TASK_RECEIPT_TASK4_RECV),
        task4_map_dead: read_qword(base + TASK_RECEIPT_TASK4_MAP_DEAD),
        child,
        tasks: bootstrap::live_task_count() as u64,
        caps: bootstrap::live_cap_count() as u64,
        root_cleared: if child != 0 && crate::sched::root(child).is_none() {
            1
        } else {
            0
        },
        grandchild: state.grandchild,
        tasks_after: bootstrap::live_task_count() as u64,
        caps_after: bootstrap::live_cap_count() as u64,
        root4_cleared: if state.grandchild != 0 && crate::sched::root(state.grandchild).is_none() {
            1
        } else {
            0
        },
    }
}

/// S2-D3：Segment 原语的用户态回执。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemsegWitness {
    pub create: u64,
    pub map: u64,
    pub zero: u64,
    pub write: u64,
    pub unmap: u64,
    pub destroy: u64,
    pub stale: u64,
    pub create2: u64,
    pub map2: u64,
    pub zero2: u64,
    pub unmap2: u64,
    pub destroy2: u64,
}

impl MemsegWitness {
    pub fn evidence_ok(&self) -> bool {
        self.create == MEMSEG_CAP1_HANDLE
            && self.map == 0
            && self.zero == 0
            && self.write == MEMSEG_PATTERN
            && self.unmap == 0
            && self.destroy == 0
            && self.stale == encode_errno(Errno::Access)
            && self.create2 == MEMSEG_CAP2_HANDLE
            && self.map2 == 0
            && self.zero2 == 0
            && self.unmap2 == 0
            && self.destroy2 == 0
    }
}

/// 读取 Segment 原语回执。
pub fn memseg_witness() -> MemsegWitness {
    let base = USER_Y_VA;
    MemsegWitness {
        create: read_user_qword(base + MEMSEG_RECEIPT_CREATE),
        map: read_user_qword(base + MEMSEG_RECEIPT_MAP),
        zero: read_user_qword(base + MEMSEG_RECEIPT_ZERO),
        write: read_user_qword(base + MEMSEG_RECEIPT_WRITE),
        unmap: read_user_qword(base + MEMSEG_RECEIPT_UNMAP),
        destroy: read_user_qword(base + MEMSEG_RECEIPT_DESTROY),
        stale: read_user_qword(base + MEMSEG_RECEIPT_STALE),
        create2: read_user_qword(base + MEMSEG_RECEIPT_CREATE2),
        map2: read_user_qword(base + MEMSEG_RECEIPT_MAP2),
        zero2: read_user_qword(base + MEMSEG_RECEIPT_ZERO2),
        unmap2: read_user_qword(base + MEMSEG_RECEIPT_UNMAP2),
        destroy2: read_user_qword(base + MEMSEG_RECEIPT_DESTROY2),
    }
}
/// [`SYS_ECHO`] 的期望返回值（用户与内核共享的纯函数，供两侧对账）。
pub const fn echo_expected(arg: u64) -> u64 {
    arg ^ ECHO_MASK
}

/// 纯分派器（不触碰硬件）：处理不需要地址空间的服务与全部错误路径。
///
/// [`SYS_COPY_IN`] 需要校验页表，由 [`handle`] 先行处理，不进入本函数。
pub const fn dispatch_plain(num: u64, args: &[u64; MAX_ARGS]) -> u64 {
    match num {
        SYS_ABI_CHECK => {
            if args[0] != prima_abi::native_abi::MAJOR
                || args[1] > prima_abi::native_abi::MINOR
                || args[2] != 0
                || args[3] != 0
                || args[4] != 0
                || args[5] != 0
            {
                encode_errno(Errno::Invalid)
            } else {
                prima_abi::native_abi::VERSION
            }
        }
        SYS_ECHO => echo_expected(args[0]),
        SYS_SUM4 => args[0]
            .wrapping_add(args[1])
            .wrapping_add(args[2])
            .wrapping_add(args[3]),
        0 => encode_errno(Errno::Invalid),
        _ => encode_errno(Errno::NoSys),
    }
}

/// [`SYS_COPY_IN`] 的纯边界检查：长度必须为 `1..=8`（否则 `EINVAL`），且不得跨页
/// （否则 `EFAULT`）。页表 present/user 校验在 [`copy_in`] 中做。
pub const fn copy_in_bounds(ptr: u64, len: u64) -> Result<(), Errno> {
    if len == 0 || len > 8 {
        return Err(Errno::Invalid);
    }
    if (ptr & 0xfff) + len > 0x1000 {
        return Err(Errno::Fault);
    }
    Ok(())
}

/// M2/T-MUSL-02：FS base 必须是低半区规范地址。
///
/// 非规范值写入 `IA32_FS_BASE` 会触发 #GP；与用户指针一样，只接受 `[0, 2^47)`。
pub const fn validate_fs_base(base: u64) -> Result<(), Errno> {
    if base >= (1u64 << 47) {
        return Err(Errno::Invalid);
    }
    Ok(())
}

/// `SYS_TASK_SET_FS_BASE`：保存到当前 Task 的调度槽并立即写入硬件（syscall 不经过 resume）。
fn set_fs_base_call(base: u64) -> u64 {
    if let Err(errno) = validate_fs_base(base) {
        return encode_errno(errno);
    }
    crate::sched::set_fs_base(crate::sched::current(), base);
    crate::msr::wrmsr(crate::msr::IA32_FS_BASE, base);
    0
}

/// 从用户缓冲读入 `len` 字节并组装为 little-endian u64（内核/QEMU 专用）。
fn copy_in(ptr: u64, len: u64) -> Result<u64, Errno> {
    copy_in_bounds(ptr, len)?;
    let Some((fallback_root, offset)) = crate::cow::root_and_offset() else {
        return Err(Errno::Fault);
    };
    // 切片 O：用户缓冲指针必须按**当前任务**的根校验（per-task CR3）。
    let root = crate::sched::current_root().unwrap_or(fallback_root);
    let map = |phys: u64| phys_to_virt(phys, offset);
    let pte = vspace::read_leaf(root, ptr, map).map_err(|_| Errno::Fault)?;
    if pte & PT_PRESENT == 0 || pte & PT_USER == 0 {
        return Err(Errno::Fault);
    }
    let mut value = 0u64;
    let mut i = 0;
    while i < len {
        // 单核 boot 早期；指针已由上面的页表检查证明可达。
        let byte = prima_ostd::mm::read_u8(ptr + i);
        value |= (byte as u64) << (8 * i);
        i += 1;
    }
    Ok(value)
}

/// 单次 syscall 进入的实测记录。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CallRecord {
    /// 进入时的代码段（CPL3 应为 [`USER_CS`]）。
    cs: u64,
    /// 内核写回用户 `rax` 的返回值（成功值或 errno 编码）。
    ret: u64,
}

/// 记录前 [`MAX_CALLS`] 次 syscall 进入（单核 boot 早期，无并发）。
///
/// 顺序：ABI 电池 6 次 + 授权查询 3 次 + 订阅/取消订阅 6 次（快路径单独记录，不占此处）。
const MAX_CALLS: usize = 15;

static CALLS: StaticCell<[CallRecord; MAX_CALLS]> =
    StaticCell::new([CallRecord { cs: 0, ret: 0 }; MAX_CALLS]);

/// 已进入的 syscall 次数。
static COUNT: AtomicU64 = AtomicU64::new(0);

/// 由框架裸入口 `syscall_entry` 调用的内核侧处理。
///
/// `frame` 是栈上 8 个 qword 的 ABI 现场帧的**类型化视图**：`[0]=调用号(rax)`、
/// `[1..7]=参数(rdi,rsi,rdx,r10,r8,r9)`、`[7]=进入时 CS`。解引用由框架的
/// [`AbiFrame`] 访问器完成，本函数因此是 safe `extern "C" fn`（ABI 不变）。
pub extern "C" fn handle(frame: AbiFrame) -> u64 {
    bootstrap::gate_audit::enforce_stop("int80-entry");
    let num = frame.num();
    let mut args = [0u64; MAX_ARGS];
    let mut i = 0;
    while i < MAX_ARGS {
        args[i] = frame.arg(i);
        i += 1;
    }
    let cs = frame.cs();
    let current = sched::current();
    // M15a-2：syscall 入口推进超时等待，并消费已注入的等待结果。
    inject_wait_wakes(&bootstrap::expire_waits_global());
    inject_wait_wakes(&bootstrap::expire_physical_waits(
        crate::preempt::time_ticks(),
    ));
    let _ = bootstrap::take_wait_result(current);

    let ret = serve(num, &args, 1);
    bootstrap::gate_audit::enforce_stop("int80-return");
    maybe_deliver_signals(ret);

    // 切片 M：只把 task 1 阶段一的前 MAX_CALLS 次进入记入 CALLS（既有证据固定下标）；
    // 其余（task 1 阶段二、task 2）记入 mt，避免扰动 COUNT 与 CALLS[0..15]。
    let index = if current == INIT_TASK_ID && COUNT.load(Ordering::Relaxed) < MAX_CALLS as u64 {
        COUNT.fetch_add(1, Ordering::Relaxed) as usize
    } else {
        usize::MAX
    };
    if index < MAX_CALLS {
        // 单核 boot 早期：写入与读取之间无并发。
        CALLS.with_mut(|calls| calls[index] = CallRecord { cs, ret });
    } else {
        mt_record(current, num, cs, ret);
    }
    ret
}

/// Native Gate controls. Every unused argument is required to be zero.
#[inline(never)]
fn gate_control_call(num: u64, args: &[u64; MAX_ARGS]) -> u64 {
    use bootstrap::gate::{self, WaitOutcome};
    let task = sched::current();
    let budget = gate::with_state(|state| state.preflight_gate_control(task));
    bootstrap::gate_audit::enforce_stop("gate-control-entry");
    budget.expect("native Gate control has an active bootstrap state");
    let result = (|| -> Result<u64, Errno> {
        let used = match num {
            SYS_GATE_ATTACH | SYS_GATE_WAIT => 3,
            _ => 2,
        };
        if args[used..].iter().any(|&arg| arg != 0) {
            return Err(Errno::Invalid);
        }
        let cap = decode_caller_handle(args[0]).map_err(auth_errno)?;
        match num {
            SYS_GATE_ATTACH => {
                let other = if args[1] == 0 {
                    Handle::ZERO
                } else {
                    decode_caller_handle(args[1]).map_err(auth_errno)?
                };
                // Check the actual hardware root as well as Process membership.
                let client = bootstrap::task_of_cap(cap).ok_or(Errno::Access)?;
                let root = sched::root(client).ok_or(Errno::Fault)?;
                if (1..sched::MAX_TASKS as u32).any(|id| {
                    id != client
                        && bootstrap::task_status(id).is_some()
                        && sched::root(id) == Some(root)
                }) {
                    return Err(Errno::Again);
                }
                gate::with_state(|state| state.gate_attach(task, cap, other, args[2]))
                    .map(|outcome| {
                        let local = encode_caller_handle(outcome.gate_cap);
                        assert_ne!(local, 0, "preflighted local Gate handle capacity");
                        local
                    })
                    .map_err(gate_errno)
            }
            SYS_GATE_WAIT => {
                if args[1] & 7 != 0 || args[2] != gate::OUTPUT_BYTES {
                    return Err(Errno::Invalid);
                }
                gate::with_state(|state| state.gate_wait_preflight(task, cap))
                    .map_err(gate_errno)?;
                // Do not retain this physical plan across a blocked wait.
                let output = prepare_task_output(task, args[1], gate::OUTPUT_BYTES as usize)?;
                match gate::with_state(|state| state.gate_wait(task, cap)).map_err(gate_errno)? {
                    delivered @ WaitOutcome::Delivered { .. } => {
                        let mut bytes = [0; gate::OUTPUT_BYTES as usize];
                        let mut words = delivered.words().unwrap();
                        words[2] =
                            encode_caller_handle(bootstrap::decode_handle(words[2]).unwrap());
                        assert_ne!(words[2], 0, "preflighted local handle capacity");
                        for (chunk, word) in bytes.as_chunks_mut::<8>().0.iter_mut().zip(words) {
                            chunk.copy_from_slice(&word.to_le_bytes());
                        }
                        let (_, offset) =
                            crate::cow::root_and_offset().expect("prepared output root");
                        output.write(&bytes, |phys| phys_to_virt(phys, offset));
                        Ok(0)
                    }
                    WaitOutcome::Blocked => {
                        let mut frame = current_context();
                        // Both native entry instructions are two bytes. Retry
                        // validates the current root/output after service wake.
                        frame.rip = frame.rip.checked_sub(2).expect("native wait instruction");
                        frame.rax = SYS_GATE_WAIT;
                        sched::set_frame(task, frame);
                        sched::block_current()
                    }
                }
            }
            _ => gate::with_state(|state| {
                state.gate_reply(task, cap, args[1], num == SYS_GATE_ABORT)
            })
            .map(|_| 0)
            .map_err(gate_errno),
        }
    })();
    // AuditFull is terminal, never an errno returned to either participant.
    bootstrap::gate_audit::enforce_stop("gate-control");
    if let Err(error) = result {
        let recorded = gate::with_state(|state| {
            state.gate_control_rejected(task, num, *args, encode_errno(error))
        });
        bootstrap::gate_audit::enforce_stop("gate-control-rejected");
        recorded.expect("preflighted native Gate rejection audit");
    }
    result.unwrap_or_else(encode_errno)
}

const fn gate_errno(error: bootstrap::gate::Error) -> Errno {
    use bootstrap::gate::Error;
    match error {
        Error::Denied => Errno::Access,
        Error::Stale => Errno::NoEnt,
        Error::Busy => Errno::Again,
        Error::Capacity => Errno::NoSpace,
        Error::AuditFull => Errno::NoSpace, // consumed by enforce_stop above
    }
}

// Keep the frozen frame off native dispatch's stack: exec has a deep loader
// path on the same guarded 32 KiB stack. Only Gate clients need this snapshot.
#[inline(never)]
fn gate_client_call(num: u64, args: &[u64; MAX_ARGS], flags: u64) -> u64 {
    let task = sched::current();
    let frame = current_context();
    let outcome = bootstrap::gate::with_state(|state| state.gate_trap(task, num, *args, flags));
    bootstrap::gate_audit::enforce_stop("gate-trap");
    match outcome {
        Ok(bootstrap::gate::TrapOutcome::Queued { .. }) => {
            sched::set_frame(task, frame);
            sched::block_current()
        }
        Ok(bootstrap::gate::TrapOutcome::Orphaned { value }) => value,
        other => panic!("bound gate trap could not commit: {other:?}"),
    }
}

/// 服务分派（`int 0x80` 与 `syscall` 快路径共用），不含进入记录。
///
/// [`SYS_YIELD`] 会切走当前任务、不返回给调用者，故必须在 `dispatch_plain` 之前分支。
fn serve(num: u64, args: &[u64; MAX_ARGS], flags: u64) -> u64 {
    bootstrap::gate_audit::enforce_stop("dispatch");
    // Intercept before every native number, including zero and Gate controls.
    if bootstrap::gate::task_is_gated(sched::current()) {
        return gate_client_call(num, args, flags);
    }
    crate::irq::retry_pending();
    if matches!(
        num,
        SYS_GATE_ATTACH | SYS_GATE_WAIT | SYS_GATE_REPLY | SYS_GATE_ABORT
    ) {
        gate_control_call(num, args)
    } else if num == SYS_COPY_IN {
        match copy_in(args[0], args[1]) {
            Ok(value) => value,
            Err(errno) => encode_errno(errno),
        }
    } else if num == SYS_AUTHORIZE {
        authorize_call(args[0], args[1])
    } else if num == SYS_EXC_SUBSCRIBE {
        exc_subscribe_call(args[0], args[1])
    } else if num == SYS_EXC_UNSUBSCRIBE {
        exc_unsubscribe_call(args[0])
    } else if num == SYS_UPCALL_CONTROL {
        upcall_control(args)
    } else if num == SYS_TASK_INFO {
        task_info_call(args[0], args[1])
    } else if num == SYS_TASK_ID {
        sched::current() as u64
    } else if num == SYS_PROCESS_ID {
        bootstrap::process_identity(sched::current(), args[0])
            .map_or_else(|| encode_errno(Errno::Invalid), u64::from)
    } else if num == SYS_TASK_SET_FS_BASE {
        set_fs_base_call(args[0])
    } else if num == SYS_YIELD {
        yield_call()
    } else if num == SYS_EXC_WAIT {
        wait_call(args[0], args[1])
    } else if num == SYS_TRAP {
        trap_call()
    } else if num == SYS_CAP_DERIVE {
        cap_derive_call(args[0], args[1])
    } else if num == SYS_CAP_DROP {
        cap_drop_call(args[0])
    } else if num == SYS_MEMSEG_CREATE {
        memseg_create_call(args[0], args[1])
    } else if num == SYS_MEMSEG_MAP {
        memseg_map_call(args[0], args[1], args[2], args[3], args[4], args[5])
    } else if num == SYS_VSPACE_ACCESS_ENABLE {
        match (decode_caller_handle(args[0]), crate::cow::root_and_offset()) {
            (Ok(cap), Some((_, offset))) if args[3] <= 1 => {
                let root = bootstrap::task_of_cap(cap).and_then(sched::root);
                match root {
                    Some(root) => bootstrap::vspace_access_enable(
                        sched::current(),
                        cap,
                        args[1],
                        args[2] as usize,
                        args[3] != 0,
                        root,
                        offset,
                    )
                    .map_or_else(|e| encode_errno(memseg_errno(e)), |_| 0),
                    None => encode_errno(Errno::NoEnt),
                }
            }
            _ => encode_errno(Errno::Invalid),
        }
    } else if num == SYS_MEMSEG_UNMAP {
        memseg_unmap_call(args[0], args[1], args[2])
    } else if num == SYS_MEMSEG_PROTECT {
        memseg_protect_call(args[0], args[1], args[2], args[3])
    } else if num == SYS_MEMSEG_DESTROY {
        memseg_destroy_call(args[0])
    } else if num == SYS_SEND {
        send_call(args[0], args[1], args[2], args[3], args[4], args[5])
    } else if num == SYS_RECV {
        recv_call(args[0], 0)
    } else if num == SYS_RECV_META {
        recv_call(args[0], 1)
    } else if num == SYS_TASK_CREATE {
        task_create_call(args[0])
    } else if num == SYS_TASK_DESTROY {
        task_destroy_call(args[0])
    } else if num == SYS_NS_CREATE {
        ns_create_call()
    } else if num == SYS_NS_BIND {
        ns_bind_call(args[0], args[1], args[2], args[3])
    } else if num == SYS_NS_UNBIND {
        ns_unbind_call(args[0], args[1], args[2])
    } else if num == SYS_NS_MOUNT {
        ns_mount_call(args[0], args[1], args[2], args[3])
    } else if num == SYS_NS_UNMOUNT {
        ns_unmount_call(args[0], args[1], args[2])
    } else if num == SYS_NS_RESOLVE {
        ns_resolve_call(args[0], args[1], args[2], args[3])
    } else if num == SYS_NS_DESTROY {
        ns_destroy_call(args[0])
    } else if num == SYS_TIME_NOW {
        time_now_call()
    } else if num == SYS_TIME_ENABLE {
        time_enable_call()
    } else if num == SYS_TIME_TICKS {
        crate::preempt::time_ticks()
    } else if num == SYS_TIMER_SET {
        timer_set_call(args[0])
    } else if num == SYS_TIMER_CANCEL {
        timer_cancel_call(args[0])
    } else if num == SYS_TIMER_DESTROY {
        timer_destroy_call(args[0])
    } else if num == SYS_FORK_TRANSACTION {
        fork_transaction_call(args[0], args[1])
    } else if num == SYS_FORK {
        fork_call(false)
    } else if num == SYS_EXEC_HANDOFF {
        exec_handoff_call(args[0])
    } else if num == SYS_EXEC_REPLACE_ENV {
        exec_call(args[0], args[1], args[2], args[3], args[4], args[5], None)
    } else if num == SYS_EXEC_REPLACE {
        exec_call(args[0], args[1], args[2], args[3], 0, 0, None)
    } else if num == SYS_SIGACTION {
        sigaction_call(args[0], args[1])
    } else if num == SYS_SIGRETURN {
        sigreturn_call()
    } else if num == SYS_SIGNAL {
        signal_call(args[0], args[1])
    } else if num == SYS_WAIT4 {
        wait4_call(args[0], args[1], args[2])
    } else if num == SYS_PROCESS_TERMINATE {
        process_terminate_call(args[0], args[1])
    } else if num == SYS_PROCESS_EXIT {
        process_exit_call(args[0])
    } else if num == SYS_CLONE {
        clone_call(args[0], args[1], args[2], args[3], args[4])
    } else if num == SYS_THREAD_CREATE {
        thread_create_call(args[0], args[1], args[2])
    } else if num == SYS_THREAD_EXIT_UNMAP {
        thread_exit_unmap_call(args[0], args[1], args[2])
    } else if num == SYS_THREAD_EXIT {
        thread_exit_call()
    } else if num == SYS_THREAD_CLEAR_TID {
        bootstrap::clear_child_tid(sched::current())
    } else if num == SYS_WAIT_CREATE {
        wait_create_call()
    } else if num == SYS_WAIT_OBSERVE {
        wait_observe_call(args[0])
    } else if num == SYS_WAIT_WORD {
        let mut bytes = [0; 4];
        match copy_user_bytes(args[2], &mut bytes) {
            Err(error) => encode_errno(error),
            Ok(()) if u32::from_le_bytes(bytes) != args[3] as u32 => encode_errno(Errno::Again),
            Ok(()) => wait_objects_call(args[0], args[1], 0, 0, 0, args[4], true),
        }
    } else if num == SYS_WAIT_OBJECTS {
        wait_objects_call(args[0], args[1], args[2], args[3], args[4], args[5], false)
    } else if num == SYS_TASK_WAIT_DEADLINE {
        task_wait_deadline_call(args[0])
    } else if num == SYS_WAIT_SIGNAL {
        wait_signal_call(args[0])
    } else if num == SYS_WAIT_DESTROY {
        wait_destroy_call(args[0])
    } else if num == SYS_AUDIT {
        audit_call(args[0])
    } else if num == SYS_DEV_IRQ_CREATE {
        dev_irq_create_call(args[0], args[1])
    } else if num == SYS_DEV_IRQ_MASK {
        dev_irq_mask_call(args[0], args[1])
    } else if num == SYS_DEV_IOPORT_CREATE {
        dev_ioport_create_call(args[0], args[1])
    } else if num == SYS_DEV_IOPORT_OUT {
        dev_ioport_out_call(args[0], args[1], args[2])
    } else if num == SYS_DEV_IOPORT_IN {
        dev_ioport_in_call(args[0], args[1])
    } else if num == SYS_DEV_DESTROY {
        dev_destroy_call(args[0])
    } else {
        dispatch_plain(num, args)
    }
}

/// 切片 J：`syscall`/`sysret` 快路径的进入记录（单核 boot 早期，无并发）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FastRecord {
    num: u64,
    arg: u64,
    cs: u64,
    ret: u64,
}

static FAST: StaticCell<FastRecord> = StaticCell::new(FastRecord {
    num: 0,
    arg: 0,
    cs: 0,
    ret: 0,
});

/// 由框架裸入口 `syscall_fast_entry` 调用的内核侧处理（与 [`handle`] 共用 [`serve`]）。
///
/// `frame` 布局与 `int 0x80` 路径相同：`[0]=号`、`[1..7]=参数`、`[7]=CS`（快路径由入口
/// 填 [`prima_ostd::entry::set_fast_syscall_cs`] 设定的用户 `CS`，因为 `syscall` 不压栈 CS）。
pub extern "C" fn handle_fast(frame: AbiFrame) -> u64 {
    let num = frame.num();
    let mut args = [0u64; MAX_ARGS];
    let mut i = 0;
    while i < MAX_ARGS {
        args[i] = frame.arg(i);
        i += 1;
    }
    let cs = frame.cs();
    let ret = serve(num, &args, 0);
    bootstrap::gate_audit::enforce_stop("fast-return");
    maybe_deliver_signals(ret);
    // 记录指定的 echo 快路径见证，而不让后来运行的服务 Runtime syscall 覆盖它。
    if num == SYS_ECHO && args[0] == FAST_ARG {
        FAST.with_mut(|fast| {
            if fast.num == 0 {
                *fast = FastRecord {
                    num,
                    arg: args[0],
                    cs,
                    ret,
                };
            }
        });
    }
    ret
}

/// 快路径调用使用的参数（可识别，便于与 `int 0x80` 路径对账）。
pub const FAST_ARG: u64 = 0x0FED_CBA9_8765_4321;

/// 配置 x86-64 `syscall`/`sysret` 快路径所需的 MSR。
///
/// 置 EFER.SCE、装 STAR（内核/用户选择子）、LSTAR（入口地址）、FMASK（屏蔽 IF/TF/DF）。
/// 必须在进入 CPL3 前、CPL0 下调用一次。
///
/// # Safety
///
/// 写 MSR 是特权操作；仅可在单核 boot 早期调用。
pub fn configure_fast_path() {
    let efer = crate::msr::rdmsr(crate::msr::IA32_EFER);
    crate::msr::wrmsr(crate::msr::IA32_EFER, crate::msr::efer_with_sce(efer));
    crate::msr::wrmsr(crate::msr::IA32_STAR, crate::msr::STAR_VALUE);
    // 快路径入口不压 CS/SS，故先把用户 CS/SS 告知框架（写入 ABI 帧的 CS 槽与保存块
    // 硬件帧的 SS 槽）。
    prima_ostd::entry::set_fast_syscall_cs(USER_CS as u64);
    prima_ostd::entry::set_fast_syscall_ss(crate::gdt::USER_DS as u64);
    crate::msr::wrmsr(
        crate::msr::IA32_LSTAR,
        prima_ostd::entry::syscall_fast_entry_addr(),
    );
    crate::msr::wrmsr(crate::msr::IA32_FMASK, crate::msr::SYSCALL_FMASK);
}

/// 一次 `syscall`/`sysret` 快路径见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FastWitness {
    /// 进入时的 CS（快路径入口填 [`USER_CS`]，应 0x33）。
    pub cs: u64,
    /// 调用号（应 [`SYS_ECHO`]）。
    pub num: u64,
    /// 参数（应 [`FAST_ARG`]）。
    pub arg: u64,
    /// 内核返回并写入 `rax` 的值。
    pub ret: u64,
    /// 用户写回 `Y+56` 的值（应等于 [`FastWitness::ret`]）。
    pub receipt: u64,
    /// EFER.SCE 是否置位（实测 `rdmsr`）。
    pub efer_sce: u32,
    /// 实测 IA32_STAR（应 [`crate::msr::STAR_VALUE`]）。
    pub star: u64,
    /// 实测 IA32_FMASK（应 [`crate::msr::SYSCALL_FMASK`]）。
    pub fmask: u64,
    /// 实测 IA32_LSTAR 是否等于快路径入口地址。
    pub lstar_ok: u32,
    /// 快路径栈底 64 字节哨兵在完整执行后仍保持初值。
    pub stack_canary: u32,
    /// The current root leaves syscall and every Task lower guard page unmapped.
    pub stack_guard: u32,
}

impl FastWitness {
    /// 纯判定：一次快路径见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.cs == USER_CS as u64
            && self.cs & 3 == 3
            && self.num == SYS_ECHO
            && self.arg == FAST_ARG
            && self.ret == echo_expected(FAST_ARG)
            && self.receipt == self.ret
            && self.efer_sce == 1
            && self.star == crate::msr::STAR_VALUE
            && self.fmask == crate::msr::SYSCALL_FMASK
            && self.lstar_ok == 1
            && self.stack_canary == 1
            && self.stack_guard == 1
    }
}

/// 收集 `syscall`/`sysret` 快路径见证（在用户 `int3` 收尾回调中调用）。
pub fn fast_witness() -> FastWitness {
    let record = FAST.with(|fast| *fast);
    let efer = crate::msr::rdmsr(crate::msr::IA32_EFER);
    let star = crate::msr::rdmsr(crate::msr::IA32_STAR);
    let lstar = crate::msr::rdmsr(crate::msr::IA32_LSTAR);
    let fmask = crate::msr::rdmsr(crate::msr::IA32_FMASK);
    FastWitness {
        cs: record.cs,
        num: record.num,
        arg: record.arg,
        ret: record.ret,
        receipt: read_qword(USER_Y_VA + 56),
        efer_sce: (efer & crate::msr::EFER_SCE != 0) as u32,
        star,
        fmask,
        lstar_ok: (lstar == prima_ostd::entry::syscall_fast_entry_addr()) as u32,
        stack_canary: prima_ostd::entry::syscall_stack_canary_intact() as u32,
        stack_guard: syscall_guard_unmapped() as u32,
    }
}

fn syscall_guard_unmapped() -> bool {
    let Some(root) = sched::current_root() else {
        return false;
    };
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return false;
    };
    let unmapped = |va| {
        crate::vspace::leaf_phys_or_none(root, va, |phys| crate::mem::phys_to_virt(phys, offset))
            .is_none()
    };
    unmapped(prima_ostd::entry::syscall_stack_guard_page())
        && (0..crate::user::USER_TASK_KSTACK_COUNT + 2)
            .all(|i| crate::user::task_stack_guard_page(i).is_some_and(unmapped))
}

/// 一次 syscall ABI 见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyscallWitness {
    /// 进入的 syscall 次数（应为 [`MAX_CALLS`]）。
    pub calls: u64,
    /// 第一次进入时的 CS（应 [`USER_CS`]，RPL=3）。
    pub cs: u64,
    /// 全部进入是否都来自 CPL3（CS 均为 [`USER_CS`]）。
    pub cs_all: u32,
    /// [`SYS_ECHO`] 的返回值。
    pub echo_ret: u64,
    /// [`SYS_SUM4`] 的返回值。
    pub sum_ret: u64,
    /// [`SYS_COPY_IN`] 读合法缓冲的返回值。
    pub copy_ret: u64,
    /// [`SYS_COPY_IN`] 坏指针的返回值（应 `EFAULT`）。
    pub fault_ret: u64,
    /// 保留号 `0` 的返回值（应 `EINVAL`）。
    pub invalid_ret: u64,
    /// 未登记号 [`SYS_BAD`] 的返回值（应 `ENOSYS`）。
    pub bad_ret: u64,
    /// 回执页 Y 首 qword（COW 重试的 store 值）。
    pub receipt0: u64,
    /// Y+8：ECHO 返回值。
    pub receipt1: u64,
    /// Y+16：SUM4 返回值。
    pub receipt2: u64,
    /// Y+24：COPY_IN 合法返回值。
    pub receipt3: u64,
    /// Y+32：COPY_IN 坏指针返回值。
    pub receipt4: u64,
    /// Y+40：保留号返回值。
    pub receipt5: u64,
    /// Y+48：未登记号返回值。
    pub receipt6: u64,
}

impl SyscallWitness {
    /// 纯判定：一次 syscall ABI v1 见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.calls == MAX_CALLS as u64
            && self.cs == USER_CS as u64
            && self.cs_all == 1
            && self.echo_ret == echo_expected(SYS_ECHO_ARG)
            && self.sum_ret == SUM_EXPECTED
            && self.copy_ret == COPY_EXPECTED
            && self.fault_ret == encode_errno(Errno::Fault)
            && self.invalid_ret == encode_errno(Errno::Invalid)
            && self.bad_ret == encode_errno(Errno::NoSys)
            && self.receipt0 == USER_STORE_VALUE
            && self.receipt1 == self.echo_ret
            && self.receipt2 == self.sum_ret
            && self.receipt3 == self.copy_ret
            && self.receipt4 == self.fault_ret
            && self.receipt5 == self.invalid_ret
            && self.receipt6 == self.bad_ret
    }
}

fn read_qword(va: u64) -> u64 {
    prima_ostd::mm::read_u64(va)
}

/// 切片 O：从 task 2 回执页 Y2 的**物理帧**读一个 qword（经直映射）。
///
/// per-task CR3 后 task 1 根下不再映射 Y2 的 VA，故按物理读；读不到（未准备/无直映射
/// 偏移）返回 0，与既有 witness 的缺省 0 语义一致。
fn read_task2_qword(byte_offset: u64) -> u64 {
    let Some(phys) = crate::user::task2_y_phys() else {
        return 0;
    };
    let Some((_, offset)) = crate::cow::root_and_offset() else {
        return 0;
    };
    read_qword(phys_to_virt(phys, offset) + byte_offset)
}

/// 读取前 [`MAX_CALLS`] 条进入记录（单核 boot 早期，无并发）。
fn call_records() -> [CallRecord; MAX_CALLS] {
    CALLS.with(|calls| *calls)
}

/// 全部进入记录是否都来自 CPL3。
fn records_all_cpl3(records: &[CallRecord; MAX_CALLS]) -> u32 {
    records.iter().all(|record| record.cs == USER_CS as u64) as u32
}

/// 收集 syscall ABI 见证（在用户 `int3` 收尾回调中调用）。
pub fn witness() -> SyscallWitness {
    let calls = COUNT.load(Ordering::Relaxed);
    let records = call_records();
    let cs_all = records_all_cpl3(&records);
    SyscallWitness {
        calls,
        cs: records[0].cs,
        cs_all,
        echo_ret: records[0].ret,
        sum_ret: records[1].ret,
        copy_ret: records[2].ret,
        fault_ret: records[3].ret,
        invalid_ret: records[4].ret,
        bad_ret: records[5].ret,
        receipt0: read_qword(USER_Y_VA),
        receipt1: read_qword(USER_Y_VA + 8),
        receipt2: read_qword(USER_Y_VA + 16),
        receipt3: read_qword(USER_Y_VA + 24),
        receipt4: read_qword(USER_Y_VA + 32),
        receipt5: read_qword(USER_Y_VA + 40),
        receipt6: read_qword(USER_Y_VA + 48),
    }
}

/// 切片 K：一次能力授权查询见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuthWitness {
    /// 全部进入是否都来自 CPL3。
    pub cs_all: u32,
    /// 持有 `SegMapR` 的查询返回值（应 [`AUTHORIZE_GRANTED`]）。
    pub granted_ret: u64,
    /// 缺少 `SegMapW` 的查询返回值（应 `EACCES`）。
    pub denied_ret: u64,
    /// 不存在 Capability 的查询返回值（应 `ENOENT`）。
    pub unknown_ret: u64,
    /// 回执页 Y+64：授予结果。
    pub receipt_granted: u64,
    /// Y+72：拒绝结果。
    pub receipt_denied: u64,
    /// Y+80：`ENOENT` 结果。
    pub receipt_unknown: u64,
}

impl AuthWitness {
    /// 纯判定：一次授权查询见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.cs_all == 1
            && self.granted_ret == AUTHORIZE_GRANTED
            && self.denied_ret == encode_errno(Errno::Access)
            && self.unknown_ret == encode_errno(Errno::NoEnt)
            && self.receipt_granted == self.granted_ret
            && self.receipt_denied == self.denied_ret
            && self.receipt_unknown == self.unknown_ret
    }
}

/// 收集能力授权查询见证（在用户 `int3` 收尾回调中调用）。
pub fn auth_witness() -> AuthWitness {
    let records = call_records();
    AuthWitness {
        cs_all: records_all_cpl3(&records),
        granted_ret: records[6].ret,
        denied_ret: records[7].ret,
        unknown_ret: records[8].ret,
        receipt_granted: read_qword(USER_Y_VA + 64),
        receipt_denied: read_qword(USER_Y_VA + 72),
        receipt_unknown: read_qword(USER_Y_VA + 80),
    }
}

/// 切片 L：读取全局异常 Audit 计数；授权表未安装（host 单测）时全 0。
fn exc_audit_snapshot() -> ExcAuditCounts {
    bootstrap::exc_audit_counts().unwrap_or(ExcAuditCounts {
        created: 0,
        subscribed: 0,
        unsubscribed: 0,
        raised: 0,
    })
}

/// 切片 L：一次异常订阅/取消订阅见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcWitness {
    /// 全部进入是否都来自 CPL3。
    pub cs_all: u32,
    /// `page_fault_source` 订阅成功返回值（应 1）。
    pub sub_ret: u64,
    /// 重复订阅（应 `EACCES`）。
    pub dup_ret: u64,
    /// kind 不符（应 `EINVAL`）。
    pub kind_ret: u64,
    /// `syscall_trap_source` 订阅成功返回值（应 2）。
    pub sub2_ret: u64,
    /// 取消订阅成功返回值（应 1）。
    pub unsub_ret: u64,
    /// 再次取消（应 `EACCES`）。
    pub unsub2_ret: u64,
    pub receipt_sub: u64,
    pub receipt_dup: u64,
    pub receipt_kind: u64,
    pub receipt_sub2: u64,
    pub receipt_unsub: u64,
    pub receipt_unsub2: u64,
    /// `ExcSourceCreated` Audit 计数（应 2）。
    pub audit_created: usize,
    /// `ExcSubscribed` Audit 计数（应 3：boot task2 + init 两次）。
    pub audit_subscribed: usize,
    /// `ExcUnsubscribed` Audit 计数（应 1）。
    pub audit_unsubscribed: usize,
}

fn exc_denied_ret() -> u64 {
    encode_errno(Errno::Access)
}

fn exc_invalid_ret() -> u64 {
    encode_errno(Errno::Invalid)
}

impl ExcWitness {
    /// 纯判定：一次异常订阅见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.cs_all == 1
            && self.sub_ret == EXC_PAGE_FAULT_SOURCE
            && self.dup_ret == exc_denied_ret()
            && self.kind_ret == exc_invalid_ret()
            && self.sub2_ret == EXC_SYSCALL_TRAP_SOURCE
            && self.unsub_ret == EXC_PAGE_FAULT_SOURCE
            && self.unsub2_ret == exc_denied_ret()
            && self.receipt_sub == self.sub_ret
            && self.receipt_dup == self.dup_ret
            && self.receipt_kind == self.kind_ret
            && self.receipt_sub2 == self.sub2_ret
            && self.receipt_unsub == self.unsub_ret
            && self.receipt_unsub2 == self.unsub2_ret
            && self.audit_created == 2
            && self.audit_subscribed == 3
            && self.audit_unsubscribed == 1
    }
}

/// 收集异常订阅见证（在用户 `int3` 收尾回调中调用）。
pub fn exc_witness() -> ExcWitness {
    let records = call_records();
    let audit = exc_audit_snapshot();
    ExcWitness {
        cs_all: records_all_cpl3(&records),
        sub_ret: records[9].ret,
        dup_ret: records[10].ret,
        kind_ret: records[11].ret,
        sub2_ret: records[12].ret,
        unsub_ret: records[13].ret,
        unsub2_ret: records[14].ret,
        receipt_sub: read_qword(USER_Y_VA + 88),
        receipt_dup: read_qword(USER_Y_VA + 96),
        receipt_kind: read_qword(USER_Y_VA + 104),
        receipt_sub2: read_qword(USER_Y_VA + 112),
        receipt_unsub: read_qword(USER_Y_VA + 120),
        receipt_unsub2: read_qword(USER_Y_VA + 128),
        audit_created: audit.created,
        audit_subscribed: audit.subscribed,
        audit_unsubscribed: audit.unsubscribed,
    }
}

/// 切片 L：平台初始化那次 pending 直投的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingWitness {
    pub delivered: usize,
    pub pending: usize,
    pub message_id: u32,
    pub payload: u64,
    pub task_ready_after: bool,
    /// `ExceptionRaised` Audit 计数（应 1）。
    pub audit_raised: usize,
}

impl PendingWitness {
    /// 纯判定：pending 直投见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.delivered == 1
            && self.pending == 1
            && self.message_id != 0
            && self.payload == EXC_PAGE_FAULT_PAYLOAD
            && self.task_ready_after
            && self.audit_raised == 1
    }
}

/// 收集 pending 直投见证（在用户 `int3` 收尾回调中调用）。
///
/// `audit_raised` 取平台初始化时的快照（`ExcBootRecord::audit_raised`），使后续切片
/// 新增的 raise 不扰动 SPEC-050 的期望值。
pub fn pending_witness() -> PendingWitness {
    match bootstrap::exc_boot_record() {
        Some(boot) => PendingWitness {
            delivered: boot.delivered,
            pending: boot.pending,
            message_id: boot.message_id,
            payload: boot.payload,
            task_ready_after: boot.task_ready_after,
            audit_raised: boot.audit_raised,
        },
        None => PendingWitness {
            delivered: 0,
            pending: 0,
            message_id: 0,
            payload: 0,
            task_ready_after: false,
            audit_raised: 0,
        },
    }
}

/// 切片 M：一次双任务协作切换见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MtWitness {
    /// 协作切换次数（应 2：task1→task2、task2→task1）。
    pub switches: u64,
    /// 最后一次切换的来源/去向（应 2→1，证明切回 task 1）。
    pub last_from: u64,
    pub last_to: u64,
    /// task 1 让出前 / 恢复后的 `SYS_TASK_ID`（应均为 1）。
    pub task1_id_before: u64,
    pub task1_id_after: u64,
    /// task 2 的 `SYS_TASK_ID`（应 2）。
    pub task2_id: u64,
    /// task 2 的授权查询（应 `EACCES`，image cap 属 task 1）。
    pub task2_auth: u64,
    /// task 2 的异常订阅（应 `EACCES`，源 cap 属 task 1）。
    pub task2_sub: u64,
    pub task1_cs_before: u64,
    pub task1_cs_after: u64,
    pub task2_cs: u64,
    pub receipt_task1_before: u64,
    pub receipt_task1_yield: u64,
    pub receipt_task1_after: u64,
    pub receipt_task2_id: u64,
    pub receipt_task2_auth: u64,
    pub receipt_task2_sub: u64,
}

impl MtWitness {
    /// 纯判定：双任务协作切换见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        let denied = encode_errno(Errno::Access);
        self.switches == 2
            && self.last_from == TASK2_ID as u64
            && self.last_to == INIT_TASK_ID as u64
            && self.task1_id_before == INIT_TASK_ID as u64
            && self.task1_id_after == INIT_TASK_ID as u64
            && self.task2_id == TASK2_ID as u64
            && self.task2_auth == denied
            && self.task2_sub == denied
            && self.task1_cs_before == USER_CS as u64
            && self.task1_cs_after == USER_CS as u64
            && self.task2_cs == USER_CS as u64
            && self.receipt_task1_before == self.task1_id_before
            && self.receipt_task1_yield == 0
            && self.receipt_task1_after == self.task1_id_after
            && self.receipt_task2_id == self.task2_id
            && self.receipt_task2_auth == self.task2_auth
            && self.receipt_task2_sub == self.task2_sub
    }
}

/// 收集双任务协作切换见证（在 task 1 的 `int3` 收尾回调中调用）。
pub fn mt_witness() -> MtWitness {
    let mt = mt_state();
    // 切片 N：SPEC-051 的切换计数在 task 1 恢复后即封存，后续 raise/wait 的切换不扰动。
    let (switches, last_from, last_to) = MT_SNAPSHOT.with(|snapshot| match *snapshot {
        Some(snapshot) => (snapshot.switches, snapshot.last_from, snapshot.last_to),
        None => (mt.switches, mt.last_from, mt.last_to),
    });
    MtWitness {
        switches,
        last_from,
        last_to,
        task1_id_before: mt.task1_id_before,
        task1_id_after: mt.task1_id_after,
        task2_id: mt.task2_id,
        task2_auth: mt.task2_auth,
        task2_sub: mt.task2_sub,
        task1_cs_before: mt.task1_cs_before,
        task1_cs_after: mt.task1_cs_after,
        task2_cs: mt.task2_cs,
        receipt_task1_before: read_qword(USER_Y_VA + 136),
        receipt_task1_yield: read_qword(USER_Y_VA + 144),
        receipt_task1_after: read_qword(USER_Y_VA + 152),
        receipt_task2_id: read_task2_qword(0),
        receipt_task2_auth: read_task2_qword(8),
        receipt_task2_sub: read_task2_qword(16),
    }
}

/// 切片 N：一次跨任务阻塞 raise/wait 见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExcWaitWitness {
    /// `SYS_EXC_WAIT`（task 1）与 `SYS_TRAP`（task 2）进入时的 CS。
    pub cs_wait: u64,
    pub cs_trap: u64,
    /// raise 投递数 / 直投 pending 数 / 被唤醒的任务 id。
    pub delivered: u64,
    pub pending: u64,
    pub woken: u64,
    /// 通知 payload（应 [`EXC_SYSCALL_TRAP_PAYLOAD`]）。
    pub payload: u64,
    /// `SYS_EXC_WAIT` 阻塞次数（应 1）。
    pub blocks: u64,
    /// task 1 恢复后写入 `Y+160` 的返回值（应 payload）。
    pub wait_receipt: u64,
    /// task 2 写入 `Y2+24` 的 `SYS_TRAP` 返回值（应 delivered）。
    pub trap_receipt: u64,
}

impl ExcWaitWitness {
    /// 纯判定：跨任务阻塞 raise/wait 见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.cs_wait == USER_CS as u64
            && self.cs_trap == USER_CS as u64
            && self.delivered == 1
            && self.pending == 1
            && self.woken == INIT_TASK_ID as u64
            && self.payload == EXC_SYSCALL_TRAP_PAYLOAD
            && self.blocks == 1
            && self.wait_receipt == self.payload
            && self.trap_receipt == self.delivered
    }
}

/// 收集跨任务阻塞 raise/wait 见证（在 task 1 的 `int3` 收尾回调中调用）。
pub fn exc_wait_witness() -> ExcWaitWitness {
    let wait = exc_wait_state();
    ExcWaitWitness {
        cs_wait: wait.cs_wait,
        cs_trap: wait.cs_trap,
        delivered: wait.delivered,
        pending: wait.pending,
        woken: wait.woken,
        payload: wait.payload,
        blocks: wait.blocks,
        wait_receipt: read_qword(USER_Y_VA + 160),
        trap_receipt: read_task2_qword(24),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alignment_fault_uses_the_bus_route_instead_of_page_fault_route() {
        assert_eq!(cpu_fault_route(17), (4, 8));
        assert_eq!(cpu_fault_route(13), (1, 4));
        assert_eq!(cpu_fault_route(6), (2, 5));
        assert_eq!(cpu_fault_route(0), (3, 6));
    }

    #[test]
    fn echo_returns_masked_arg() {
        let mut args = [0u64; MAX_ARGS];
        args[0] = SYS_ECHO_ARG;
        assert_eq!(dispatch_plain(SYS_ECHO, &args), SYS_ECHO_ARG ^ ECHO_MASK);
        assert_eq!(echo_expected(SYS_ECHO_ARG), SYS_ECHO_ARG ^ ECHO_MASK);
    }

    #[test]
    fn sum4_adds_four_registers() {
        let mut args = [0u64; MAX_ARGS];
        args[..4].copy_from_slice(&SUM_ARGS);
        assert_eq!(dispatch_plain(SYS_SUM4, &args), SUM_EXPECTED);
    }

    #[test]
    fn reserved_number_is_einval() {
        let args = [0u64; MAX_ARGS];
        assert_eq!(dispatch_plain(0, &args), encode_errno(Errno::Invalid));
    }

    /// M2/T-MUSL-02：FS base 只接受低半区规范地址；非规范值必须确定性 EINVAL，
    /// 不能把会触发 #GP 的值写进 MSR。
    #[test]
    fn fs_base_validation_rejects_non_canonical_and_kernel_addresses() {
        assert_eq!(validate_fs_base(0), Ok(()));
        assert_eq!(validate_fs_base(0x0000_0A00_0000_2000), Ok(()));
        assert_eq!(validate_fs_base(0x0000_8000_0000_0000), Err(Errno::Invalid));
        assert_eq!(validate_fs_base(u64::MAX), Err(Errno::Invalid));
        assert_eq!(validate_fs_base(0xFFFF_8000_0000_0000), Err(Errno::Invalid));
    }

    #[test]
    fn unknown_number_is_enosys() {
        let args = [1u64; MAX_ARGS];
        assert_eq!(dispatch_plain(SYS_BAD, &args), encode_errno(Errno::NoSys));
        assert_eq!(dispatch_plain(u64::MAX, &args), encode_errno(Errno::NoSys));
    }

    #[test]
    fn abi_check_accepts_compatible_version_and_rejects_other_requests() {
        let mut args = [prima_abi::native_abi::MAJOR, 0, 0, 0, 0, 0];
        assert_eq!(
            dispatch_plain(SYS_ABI_CHECK, &args),
            prima_abi::native_abi::VERSION
        );
        args[0] += 1;
        assert_eq!(
            dispatch_plain(SYS_ABI_CHECK, &args),
            encode_errno(Errno::Invalid)
        );
        args[0] -= 1;
        args[1] = 1;
        assert_eq!(
            dispatch_plain(SYS_ABI_CHECK, &args),
            encode_errno(Errno::Invalid)
        );
        args[1] = 0;
        args[2] = 1;
        assert_eq!(
            dispatch_plain(SYS_ABI_CHECK, &args),
            encode_errno(Errno::Invalid)
        );
    }

    #[test]
    fn errno_encodings_are_negative_twos_complement() {
        assert_eq!(encode_errno(Errno::Fault), 0xFFFF_FFFF_FFFF_FFF2); // -14
        assert_eq!(encode_errno(Errno::Invalid), 0xFFFF_FFFF_FFFF_FFEA); // -22
        assert_eq!(encode_errno(Errno::NoSys), 0xFFFF_FFFF_FFFF_FFDA); // -38
        assert_eq!(errno_name(Errno::Fault), "EFAULT");
        assert_eq!(errno_name(Errno::Invalid), "EINVAL");
        assert_eq!(errno_name(Errno::NoSys), "ENOSYS");
    }

    #[test]
    fn copy_in_bounds_rejects_length_out_of_range() {
        assert!(copy_in_bounds(COPY_PTR, 1).is_ok());
        assert!(copy_in_bounds(COPY_PTR, 8).is_ok());
        assert_eq!(copy_in_bounds(COPY_PTR, 0), Err(Errno::Invalid));
        assert_eq!(copy_in_bounds(COPY_PTR, 9), Err(Errno::Invalid));
    }

    #[test]
    fn copy_in_bounds_rejects_cross_page() {
        assert!(copy_in_bounds(0x1000 - 8, 8).is_ok());
        assert_eq!(copy_in_bounds(0x1000 - 4, 8), Err(Errno::Fault));
    }

    #[test]
    fn bad_ptr_is_in_same_pml4_slot_but_distinct_page() {
        assert_eq!((BAD_PTR >> 39) & 0x1ff, (USER_X_VA >> 39) & 0x1ff);
        const { assert!(((BAD_PTR >> 21) & 0x1ff) != ((USER_X_VA >> 21) & 0x1ff)) };
    }

    fn valid() -> SyscallWitness {
        SyscallWitness {
            calls: MAX_CALLS as u64,
            cs: USER_CS as u64,
            cs_all: 1,
            echo_ret: echo_expected(SYS_ECHO_ARG),
            sum_ret: SUM_EXPECTED,
            copy_ret: COPY_EXPECTED,
            fault_ret: encode_errno(Errno::Fault),
            invalid_ret: encode_errno(Errno::Invalid),
            bad_ret: encode_errno(Errno::NoSys),
            receipt0: USER_STORE_VALUE,
            receipt1: echo_expected(SYS_ECHO_ARG),
            receipt2: SUM_EXPECTED,
            receipt3: COPY_EXPECTED,
            receipt4: encode_errno(Errno::Fault),
            receipt5: encode_errno(Errno::Invalid),
            receipt6: encode_errno(Errno::NoSys),
        }
    }

    #[test]
    fn accepts_complete_syscall_evidence() {
        assert!(valid().evidence_ok());
    }

    #[test]
    fn rejects_missing_call() {
        let mut w = valid();
        w.calls = 5;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_non_cpl3_origin() {
        let mut w = valid();
        w.cs = 0x8;
        assert!(!w.evidence_ok());
        let mut w = valid();
        w.cs_all = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_wrong_sum() {
        let mut w = valid();
        w.sum_ret = SUM_ARGS[0] + SUM_ARGS[1];
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_bad_ptr_accepted() {
        let mut w = valid();
        w.fault_ret = COPY_EXPECTED; // 坏指针本应 EFAULT
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_missing_errno_for_unknown_number() {
        let mut w = valid();
        w.bad_ret = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_receipt_mismatch() {
        let mut w = valid();
        w.receipt3 = 0xdead;
        assert!(!w.evidence_ok());
    }

    fn valid_fast() -> FastWitness {
        FastWitness {
            cs: USER_CS as u64,
            num: SYS_ECHO,
            arg: FAST_ARG,
            ret: echo_expected(FAST_ARG),
            receipt: echo_expected(FAST_ARG),
            efer_sce: 1,
            star: crate::msr::STAR_VALUE,
            fmask: crate::msr::SYSCALL_FMASK,
            lstar_ok: 1,
            stack_canary: 1,
            stack_guard: 1,
        }
    }

    #[test]
    fn rejects_fast_path_corrupted_stack_canary() {
        for value in [0, 2, u32::MAX] {
            let mut witness = valid_fast();
            witness.stack_canary = value;
            assert!(!witness.evidence_ok());
        }
    }

    #[test]
    fn rejects_fast_path_without_unmapped_guard() {
        for value in [0, 2, u32::MAX] {
            let mut witness = valid_fast();
            witness.stack_guard = value;
            assert!(!witness.evidence_ok());
        }
    }

    #[test]
    fn accepts_complete_fast_path_evidence() {
        assert!(valid_fast().evidence_ok());
    }

    #[test]
    fn rejects_fast_path_non_cpl3() {
        let mut w = valid_fast();
        w.cs = 0x8;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_fast_path_wrong_star() {
        let mut w = valid_fast();
        w.star = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_fast_path_lstar_mismatch() {
        let mut w = valid_fast();
        w.lstar_ok = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_fast_path_missing_sce() {
        let mut w = valid_fast();
        w.efer_sce = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_fast_path_receipt_mismatch() {
        let mut w = valid_fast();
        w.receipt = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn right_of_maps_known_codes() {
        assert_eq!(right_of(RIGHT_DERIVE), Some(Right::Derive));
        assert_eq!(right_of(RIGHT_SEGMAPR), Some(Right::SegMapR));
        assert_eq!(right_of(RIGHT_SEGMAPW), Some(Right::SegMapW));
        assert_eq!(right_of(0xDEAD), None);
        assert_eq!(right_of(3), None);
    }

    #[test]
    fn cap_witness_accepts_the_canonical_lifecycle() {
        let witness = CapWitness {
            child1: CAP_CHILD1_HANDLE,
            escalate: encode_errno(Errno::Access),
            drop: 0,
            drop_again: encode_errno(Errno::NoEnt),
            child2: CAP_CHILD2_HANDLE,
            stale: encode_errno(Errno::NoEnt),
            grant: CAP_CHILD2_HANDLE,
            drop2: 0,
            structural: encode_errno(Errno::Access),
        };
        assert!(witness.evidence_ok());
        // 反例：复用后 generation 未递增（旧、新句柄相同）必须被拒。
        let mut reused = witness;
        reused.child2 = CAP_CHILD1_HANDLE;
        assert!(!reused.evidence_ok());
        // 反例：stale 句柄被误授予。
        let mut granted = witness;
        granted.stale = CAP_CHILD1_HANDLE;
        assert!(!granted.evidence_ok());
        // 反例：结构性根未被保护。
        let mut unprotected = witness;
        unprotected.structural = 0;
        assert!(!unprotected.evidence_ok());
    }

    #[test]
    fn auth_errno_maps_failure_reasons() {
        assert_eq!(auth_errno(AuthError::NoSuchCapability), Errno::NoEnt);
        assert_eq!(auth_errno(AuthError::NotInstalled), Errno::NoEnt);
        assert_eq!(auth_errno(AuthError::MissingRight), Errno::Access);
        assert_eq!(auth_errno(AuthError::NotHolder), Errno::Access);
        assert_eq!(auth_errno(AuthError::StaleHandle), Errno::NoEnt);
        assert_eq!(auth_errno(AuthError::TargetDead), Errno::Access);
        assert_eq!(auth_errno(AuthError::MalformedHandle), Errno::Invalid);
        assert_eq!(encode_errno(Errno::Access), 0xFFFF_FFFF_FFFF_FFF3); // -13
        assert_eq!(encode_errno(Errno::NoEnt), 0xFFFF_FFFF_FFFF_FFFE); // -2
    }

    fn valid_auth() -> AuthWitness {
        AuthWitness {
            cs_all: 1,
            granted_ret: AUTHORIZE_GRANTED,
            denied_ret: encode_errno(Errno::Access),
            unknown_ret: encode_errno(Errno::NoEnt),
            receipt_granted: AUTHORIZE_GRANTED,
            receipt_denied: encode_errno(Errno::Access),
            receipt_unknown: encode_errno(Errno::NoEnt),
        }
    }

    #[test]
    fn accepts_complete_auth_evidence() {
        assert!(valid_auth().evidence_ok());
    }

    #[test]
    fn rejects_auth_denied_accepted() {
        let mut w = valid_auth();
        w.denied_ret = AUTHORIZE_GRANTED; // 缺权限却被授予
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_auth_unknown_cap_granted() {
        let mut w = valid_auth();
        w.unknown_ret = AUTHORIZE_GRANTED;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_auth_non_cpl3() {
        let mut w = valid_auth();
        w.cs_all = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_auth_receipt_mismatch() {
        let mut w = valid_auth();
        w.receipt_denied = 0;
        assert!(!w.evidence_ok());
    }

    fn valid_ipc() -> IpcWitness {
        IpcWitness {
            create: SHARED_SEG_CAP_HANDLE,
            map1: 0,
            send1: 0,
            send2: 0,
            recv1: IPC_SHARED_MAGIC,
            cap0: SHARED_SEG_CAP_HANDLE,
            magic: IPC_SHARED_MAGIC,
            map2: 0,
            cross: encode_errno(Errno::Access),
            send_denied: encode_errno(Errno::Access),
            recv2: IPC_DIRECT_PAYLOAD,
            cap_count: 1,
            direct_cap0: EXC_PAGE_FAULT_CAP,
            direct_cap_count: 1,
            queued: 1,
            direct: 1,
            blocks: 1,
        }
    }

    #[test]
    fn rejects_ipc_without_direct_capability_delivery() {
        let mut w = valid_ipc();
        w.direct_cap0 = 0;
        assert!(!w.evidence_ok());
        w.direct_cap0 = EXC_PAGE_FAULT_CAP;
        w.direct_cap_count = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn accepts_complete_ipc_evidence() {
        assert!(valid_ipc().evidence_ok());
    }

    #[test]
    fn rejects_ipc_cross_task_map_granted() {
        let mut w = valid_ipc();
        w.cross = 0; // 未持目标 Task 的 ManageVSpace 却映射成功
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_ipc_unauthorized_send_granted() {
        let mut w = valid_ipc();
        w.send_denied = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_ipc_without_capability_transfer() {
        let mut w = valid_ipc();
        w.cap_count = 0;
        assert!(!w.evidence_ok());
        let mut w = valid_ipc();
        w.cap0 = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_ipc_when_task2_reads_a_different_frame() {
        let mut w = valid_ipc();
        w.magic = IPC_SHARED_MAGIC + 1;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_ipc_without_both_delivery_paths() {
        let mut w = valid_ipc();
        w.queued = 0;
        assert!(!w.evidence_ok());
        let mut w = valid_ipc();
        w.direct = 0;
        assert!(!w.evidence_ok());
        let mut w = valid_ipc();
        w.blocks = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_ipc_direct_payload_not_delivered_on_wake() {
        let mut w = valid_ipc();
        w.recv2 = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn ipc_errno_maps_every_variant() {
        use crate::bootstrap::IpcError as E;
        assert_eq!(ipc_errno(E::NoSuchTask), Errno::NoEnt);
        assert_eq!(ipc_errno(E::InvalidCapability), Errno::NoEnt);
        assert_eq!(ipc_errno(E::QueueEmpty), Errno::NoEnt);
        assert_eq!(ipc_errno(E::NotAuthorized), Errno::Access);
        assert_eq!(ipc_errno(E::CapabilityHolderMismatch), Errno::Access);
        assert_eq!(ipc_errno(E::StructuralCap), Errno::Access);
        assert_eq!(ipc_errno(E::TooManyCapabilities), Errno::Invalid);
        assert_eq!(ipc_errno(E::AuditFull), Errno::NoSpace);
        assert_eq!(ipc_errno(E::QueueFull), Errno::Again);
    }

    #[test]
    fn task_req_bounds_rejects_misaligned_and_cross_page() {
        assert_eq!(task_req_bounds(TASK_REQ_VA), Ok(()));
        assert_eq!(task_req_bounds(0x1000), Ok(()));
        assert_eq!(task_req_bounds(0x1004), Err(Errno::Fault)); // 未 8 对齐
        // SPEC-101 起请求块 88 字节（11 qword）：0xfc0 + 88 > 0x1000 跨页；0xfa8 + 88 = 0x1000 恰好不跨页。
        assert_eq!(task_req_bounds(0xfc0), Err(Errno::Fault));
        assert_eq!(task_req_bounds(0xfb0), Err(Errno::Fault));
        assert_eq!(task_req_bounds(0xfa8), Ok(())); // 恰好不跨页
    }

    #[test]
    fn task_create_errno_maps_every_variant() {
        use crate::bootstrap::TaskCreateError as E;
        assert_eq!(task_create_errno(E::NoSuchCapability), Errno::NoEnt);
        assert_eq!(task_create_errno(E::NotHolder), Errno::Access);
        assert_eq!(task_create_errno(E::InvalidCodeRange), Errno::Invalid);
        assert_eq!(task_create_errno(E::InvalidEntry), Errno::Invalid);
        assert_eq!(task_create_errno(E::InvalidNamespace), Errno::Access);
        assert_eq!(task_create_errno(E::MissingRight), Errno::Access);
        assert_eq!(task_create_errno(E::RightsExceeded), Errno::Access);
        assert_eq!(task_create_errno(E::InvalidInitialCap), Errno::Access);
        assert_eq!(task_create_errno(E::DuplicateCap), Errno::Invalid);
        assert_eq!(task_create_errno(E::StructuralCap), Errno::Access);
        assert_eq!(task_create_errno(E::CapacityExceeded), Errno::NoSpace);
        assert_eq!(task_create_errno(E::NoMemory), Errno::NoSpace);
        assert_eq!(task_create_errno(E::AuditFull), Errno::NoSpace);
    }

    #[test]
    fn task_destroy_errno_maps_every_variant() {
        use crate::bootstrap::TaskError as E;
        assert_eq!(task_destroy_errno(E::NoSuchTask), Errno::NoEnt);
        assert_eq!(task_destroy_errno(E::CannotDestroyInit), Errno::Access);
        assert_eq!(task_destroy_errno(E::CannotDestroyRunning), Errno::Access);
        assert_eq!(task_destroy_errno(E::HasLiveThreads), Errno::Again);
        assert_eq!(task_destroy_errno(E::NotRunning), Errno::Invalid);
        assert_eq!(task_destroy_errno(E::NotReady), Errno::Invalid);
        assert_eq!(task_destroy_errno(E::AlreadyRunning), Errno::Invalid);
        assert_eq!(task_destroy_errno(E::DuplicateTask), Errno::Invalid);
        assert_eq!(task_destroy_errno(E::CapacityExceeded), Errno::Invalid);
        assert_eq!(task_destroy_errno(E::AuditFull), Errno::NoSpace);
    }

    #[test]
    fn decode_optional_handle_treats_zero_as_absent() {
        // `initial_caps` 的缺省值是 0：不报错，直接映射为 `Handle::ZERO`（不做 `decode_handle`，
        // 否则会与「裸整数不是合法句柄」的拒绝混淆）。
        assert_eq!(decode_optional_handle(0), Ok(Handle::ZERO));
        // 未安装授权表时非零句柄一律拒绝（host 测试不装全局状态，故这里只钉住拒绝性）。
        assert_eq!(decode_optional_handle(7), Err(Errno::Invalid));
    }

    fn valid_task() -> TaskWitness {
        TaskWitness {
            stack_seg: 0x0000_0001_0000_0009,
            create: TASK_MANAGER_CAP,
            bad_entry: encode_errno(Errno::Invalid),
            no_x: encode_errno(Errno::Access),
            stack_map: 0,
            recv: TASK_CREATE_CHILD_ID as u64,
            deny: encode_errno(Errno::Access),
            destroy: 0,
            again: encode_errno(Errno::Access),
            map_dead: encode_errno(Errno::NoEnt),
            map_own: 0,
            map_value: crate::cow::pattern(0),
            map_other: encode_errno(Errno::Access),
            task4_cap: 0x0000_0001_0000_000B,
            task4_stack: 0,
            task4_recv: (TASK_CREATE_CHILD_ID + 1) as u64,
            task4_map_dead: encode_errno(Errno::NoEnt),
            grandchild: TASK_CREATE_CHILD_ID + 1,
            tasks_after: 2,
            caps_after: 30,
            root4_cleared: 1,
            child: TASK_CREATE_CHILD_ID,
            tasks: 2,
            caps: 28,
            root_cleared: 1,
        }
    }

    #[test]
    fn accepts_complete_task_evidence() {
        assert!(valid_task().evidence_ok());
    }

    #[test]
    fn task_evidence_requires_all_six_denials_and_successes() {
        // manager cap 不是 create 返回值（例如 create 本身失败）。
        let mut w = valid_task();
        w.create = encode_errno(Errno::Access);
        assert!(!w.evidence_ok());

        // 入口越界被接受。
        let mut w = valid_task();
        w.bad_entry = 0;
        assert!(!w.evidence_ok());

        // 缺 SegMapX 被接受。
        let mut w = valid_task();
        w.no_x = 0;
        assert!(!w.evidence_ok());

        // 栈没有映射进子任务根。
        let mut w = valid_task();
        w.stack_map = 1;
        assert!(!w.evidence_ok());

        // 子任务自报的 id 不是它自己的。
        let mut w = valid_task();
        w.recv = 2;
        assert!(!w.evidence_ok());

        // 缺 TaskDestroy 的销毁被放行。
        let mut w = valid_task();
        w.deny = 0;
        assert!(!w.evidence_ok());

        // 销毁失败。
        let mut w = valid_task();
        w.destroy = encode_errno(Errno::Access);
        assert!(!w.evidence_ok());

        // 重复销毁被放行（stale token 仍可授权）。
        let mut w = valid_task();
        w.again = 0;
        assert!(!w.evidence_ok());

        // 向已销毁任务映射被放行。
        let mut w = valid_task();
        w.map_dead = 0;
        assert!(!w.evidence_ok());

        // 子任务仍在任务表中 / 调度登记未清。
        let mut w = valid_task();
        w.tasks = 3;
        assert!(!w.evidence_ok());
        let mut w = valid_task();
        w.root_cleared = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn blocked_recv_out_translation_requires_writable_user_page() {
        let ptr = 0x3140;
        let page = 0x2000;
        let flags = PT_PRESENT | PT_USER | PT_WRITABLE;
        assert_eq!(
            recv_out_phys_from_pte(ptr, page | flags, 0x1000),
            Ok(0x3140)
        );
        assert_eq!(
            recv_out_phys_from_pte(ptr, page | PT_PRESENT | PT_USER, 0),
            Err(Errno::Fault)
        );
        assert_eq!(
            recv_out_phys_from_pte(ptr, page | PT_PRESENT | PT_WRITABLE, 0),
            Err(Errno::Fault)
        );
        assert_eq!(
            recv_out_phys_from_pte(0xff0, page | flags, 0),
            Err(Errno::Fault)
        );
    }

    #[test]
    fn recv_out_bounds_rejects_misaligned_and_cross_page() {
        assert_eq!(recv_out_bounds(0x1000), Ok(()));
        assert_eq!(recv_out_bounds(0x1004), Err(Errno::Fault)); // 未 8 对齐
        // 24 字节跨页：0xff8 + 24 > 0x1000。
        assert_eq!(recv_out_bounds(0xff8), Err(Errno::Fault));
        assert_eq!(recv_out_bounds(0xfe8), Ok(())); // 恰好不跨页
        assert_eq!(recv_out_bounds(1u64 << 47), Err(Errno::Fault)); // 非用户 canonical 半区
        assert_eq!(recv_out_bounds(0xffff_8000_0000_1000), Err(Errno::Fault));
    }

    #[test]
    fn kind_of_maps_known_codes() {
        assert_eq!(kind_of(EXC_KIND_PAGEFAULT), Some(ExcKind::PageFault));
        assert_eq!(kind_of(EXC_KIND_SYSCALLTRAP), Some(ExcKind::SyscallTrap));
        assert_eq!(kind_of(0), None);
        assert_eq!(kind_of(99), None);
    }

    #[test]
    fn exc_errno_maps_failure_reasons() {
        assert_eq!(exc_errno(ExcError::NoSuchSource), Errno::NoEnt);
        assert_eq!(exc_errno(ExcError::NoSuchTask), Errno::NoEnt);
        assert_eq!(exc_errno(ExcError::NotInstalled), Errno::NoEnt);
        assert_eq!(exc_errno(ExcError::NotHolder), Errno::Access);
        assert_eq!(exc_errno(ExcError::MissingRight), Errno::Access);
        assert_eq!(exc_errno(ExcError::AlreadySubscribed), Errno::Access);
        assert_eq!(exc_errno(ExcError::NotSubscribed), Errno::Access);
        assert_eq!(exc_errno(ExcError::InvalidKind), Errno::Invalid);
        assert_eq!(exc_errno(ExcError::KindMismatch), Errno::Invalid);
        assert_eq!(exc_errno(ExcError::Full), Errno::NoSpace);
        assert_eq!(exc_errno(ExcError::AuditFull), Errno::NoSpace);
        assert_eq!(encode_errno(Errno::NoSpace), 0xFFFF_FFFF_FFFF_FFE4); // -28
    }

    fn valid_exc() -> ExcWitness {
        ExcWitness {
            cs_all: 1,
            sub_ret: EXC_PAGE_FAULT_SOURCE,
            dup_ret: exc_denied_ret(),
            kind_ret: exc_invalid_ret(),
            sub2_ret: EXC_SYSCALL_TRAP_SOURCE,
            unsub_ret: EXC_PAGE_FAULT_SOURCE,
            unsub2_ret: exc_denied_ret(),
            receipt_sub: EXC_PAGE_FAULT_SOURCE,
            receipt_dup: exc_denied_ret(),
            receipt_kind: exc_invalid_ret(),
            receipt_sub2: EXC_SYSCALL_TRAP_SOURCE,
            receipt_unsub: EXC_PAGE_FAULT_SOURCE,
            receipt_unsub2: exc_denied_ret(),
            audit_created: 2,
            audit_subscribed: 3,
            audit_unsubscribed: 1,
        }
    }

    #[test]
    fn accepts_complete_exc_evidence() {
        assert!(valid_exc().evidence_ok());
    }

    #[test]
    fn rejects_exc_duplicate_accepted() {
        let mut w = valid_exc();
        w.dup_ret = EXC_PAGE_FAULT_SOURCE;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_kind_mismatch_accepted() {
        let mut w = valid_exc();
        w.kind_ret = EXC_SYSCALL_TRAP_SOURCE;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_wrong_audit_counts() {
        let mut w = valid_exc();
        w.audit_subscribed = 2;
        assert!(!w.evidence_ok());
        let mut w = valid_exc();
        w.audit_created = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_receipt_mismatch() {
        let mut w = valid_exc();
        w.receipt_unsub2 = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_non_cpl3() {
        let mut w = valid_exc();
        w.cs_all = 0;
        assert!(!w.evidence_ok());
    }

    fn valid_pending() -> PendingWitness {
        PendingWitness {
            delivered: 1,
            pending: 1,
            message_id: 1,
            payload: EXC_PAGE_FAULT_PAYLOAD,
            task_ready_after: true,
            audit_raised: 1,
        }
    }

    #[test]
    fn accepts_complete_pending_evidence() {
        assert!(valid_pending().evidence_ok());
        assert_eq!(EXC_PAGE_FAULT_PAYLOAD, 0x1_0001);
    }

    #[test]
    fn rejects_pending_without_direct_delivery() {
        let mut w = valid_pending();
        w.pending = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_pending_wrong_payload() {
        let mut w = valid_pending();
        w.payload = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_pending_task_not_ready() {
        let mut w = valid_pending();
        w.task_ready_after = false;
        assert!(!w.evidence_ok());
    }

    /// 构造可区分的保存块：槽 i 的值为 `base + i`，用于逐槽钉死映射。
    ///
    /// 映射实现已下沉 [`prima_ostd::entry::SaveBlock::to_sret_frame`]（切片 AH），此处仍以
    /// 内核调用方视角对同一契约回归。
    #[test]
    fn save_block_maps_every_slot_by_name() {
        let mut block = [0u64; 20];
        for (index, slot) in block.iter_mut().enumerate() {
            *slot = 0x1000 + index as u64;
        }
        let frame = prima_ostd::entry::SaveBlock::new(block.as_ptr()).to_sret_frame();
        assert_eq!(frame.rax, 0x1000);
        assert_eq!(frame.rbx, 0x1001);
        assert_eq!(frame.rcx, 0x1002);
        assert_eq!(frame.rdx, 0x1003);
        assert_eq!(frame.rsi, 0x1004);
        assert_eq!(frame.rdi, 0x1005);
        assert_eq!(frame.rbp, 0x1006);
        assert_eq!(frame.r8, 0x1007);
        assert_eq!(frame.r9, 0x1008);
        assert_eq!(frame.r10, 0x1009);
        assert_eq!(frame.r11, 0x100a);
        assert_eq!(frame.r12, 0x100b);
        assert_eq!(frame.r13, 0x100c);
        assert_eq!(frame.r14, 0x100d);
        assert_eq!(frame.r15, 0x100e);
        // 硬件帧槽：rip=0x0f、cs=0x10、rflags=0x11、rsp=0x12、ss=0x13。
        assert_eq!(frame.rip, 0x100f);
        assert_eq!(frame.cs, 0x1010);
        assert_eq!(frame.rflags, 0x1011);
        assert_eq!(frame.rsp, 0x1012);
        assert_eq!(frame.ss, 0x1013);
    }

    fn valid_mt() -> MtWitness {
        let denied = encode_errno(Errno::Access);
        MtWitness {
            switches: 2,
            last_from: TASK2_ID as u64,
            last_to: INIT_TASK_ID as u64,
            task1_id_before: INIT_TASK_ID as u64,
            task1_id_after: INIT_TASK_ID as u64,
            task2_id: TASK2_ID as u64,
            task2_auth: denied,
            task2_sub: denied,
            task1_cs_before: USER_CS as u64,
            task1_cs_after: USER_CS as u64,
            task2_cs: USER_CS as u64,
            receipt_task1_before: INIT_TASK_ID as u64,
            receipt_task1_yield: 0,
            receipt_task1_after: INIT_TASK_ID as u64,
            receipt_task2_id: TASK2_ID as u64,
            receipt_task2_auth: denied,
            receipt_task2_sub: denied,
        }
    }

    #[test]
    fn accepts_complete_multitask_evidence() {
        assert!(valid_mt().evidence_ok());
    }

    #[test]
    fn rejects_multitask_task2_granted() {
        let mut w = valid_mt();
        w.task2_auth = 1; // task 2 不应持有 image cap
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_multitask_wrong_task_id() {
        let mut w = valid_mt();
        w.task2_id = 1;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_multitask_no_switch_back() {
        let mut w = valid_mt();
        w.switches = 1;
        assert!(!w.evidence_ok());
        let mut w = valid_mt();
        w.last_to = TASK2_ID as u64; // 最后一次切回 task 1 的证据缺失
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_multitask_non_cpl3() {
        let mut w = valid_mt();
        w.task2_cs = 0x8;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_multitask_task1_not_resumed() {
        let mut w = valid_mt();
        w.task1_id_after = 0;
        assert!(!w.evidence_ok());
        let mut w = valid_mt();
        w.receipt_task1_yield = 9; // yield 应返回 0
        assert!(!w.evidence_ok());
    }

    fn valid_exc_wait() -> ExcWaitWitness {
        ExcWaitWitness {
            cs_wait: USER_CS as u64,
            cs_trap: USER_CS as u64,
            delivered: 1,
            pending: 1,
            woken: INIT_TASK_ID as u64,
            payload: EXC_SYSCALL_TRAP_PAYLOAD,
            blocks: 1,
            wait_receipt: EXC_SYSCALL_TRAP_PAYLOAD,
            trap_receipt: 1,
        }
    }

    #[test]
    fn accepts_complete_exc_wait_evidence() {
        assert!(valid_exc_wait().evidence_ok());
        assert_eq!(EXC_SYSCALL_TRAP_PAYLOAD, 0x2_0002);
    }

    #[test]
    fn rejects_exc_wait_not_blocked() {
        let mut w = valid_exc_wait();
        w.blocks = 0; // 未经阻塞路径
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_wait_wrong_waker() {
        let mut w = valid_exc_wait();
        w.woken = TASK2_ID as u64;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_wait_not_direct_delivery() {
        let mut w = valid_exc_wait();
        w.pending = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_wait_non_cpl3() {
        let mut w = valid_exc_wait();
        w.cs_trap = 0x8;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_exc_wait_receipt_mismatch() {
        let mut w = valid_exc_wait();
        w.wait_receipt = 0;
        assert!(!w.evidence_ok());
    }
}
