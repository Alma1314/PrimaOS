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
    self, AuthError, ExcAuditCounts, ExcError, ExcKind, IMAGE_CAP_ID, INIT_TASK_ID,
};
use crate::gdt::USER_CS;
use crate::mem::phys_to_virt;
use crate::user::{USER_BASE_VA, USER_STORE_VALUE, USER_X_VA, USER_Y_VA};
use crate::vspace::{self, PT_PRESENT, PT_USER};

/// 合法 syscall 号：回显（`arg0 ^ ECHO_MASK`）。
pub const SYS_ECHO: u64 = 1;
/// 合法 syscall 号：4 参数求和（多寄存器参数见证）。
pub const SYS_SUM4: u64 = 2;
/// 合法 syscall 号：从用户缓冲读入最多 8 字节并组装为 u64。
pub const SYS_COPY_IN: u64 = 3;
/// 合法 syscall 号：能力授权查询（`args[0]=cap_id`、`args[1]=right code`）。
pub const SYS_AUTHORIZE: u64 = 4;
/// 切片 L：异常订阅（`args[0]=exc source cap_id`、`args[1]=kind code`）。
pub const SYS_EXC_SUBSCRIBE: u64 = 5;
/// 切片 L：取消异常订阅（`args[0]=exc source cap_id`）。
pub const SYS_EXC_UNSUBSCRIBE: u64 = 6;
/// 未登记号：必须得到确定性 `ENOSYS`。
pub const SYS_BAD: u64 = 0xDEAD;

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

/// 切片 K：授权查询的调用者 Task（当前唯一 ring3 任务，等同 Bootstrap init Task）。
pub const AUTHORIZE_TASK: u32 = INIT_TASK_ID;
/// 授权查询使用的 Capability：Bootstrap 初始 image cap（持有者拥有 `SegMapR` 但无 `SegMapW`）。
pub const AUTHORIZE_CAP: u64 = IMAGE_CAP_ID as u64;
/// 成功授予时返回的凭据（即被授权的 cap id）。
pub const AUTHORIZE_GRANTED: u64 = IMAGE_CAP_ID as u64;
/// 权限编码：`SegMapR`（init image cap 具备）。
pub const RIGHT_SEGMAPR: u64 = 1;
/// 权限编码：`SegMapW`（init image cap 不具备，用于见证拒绝）。
pub const RIGHT_SEGMAPW: u64 = 2;
/// 授权查询用的不存在 Capability ID（见证 `ENOENT`）。
pub const UNSET_CAP: u64 = 999;

/// 切片 L：初始化创建的 `page_fault_source` Capability ID（镜像常量，见 bootstrap.rs）。
pub const EXC_PAGE_FAULT_CAP: u64 = bootstrap::EXC_SOURCE_CAP_BASE as u64;
/// 切片 L：初始化创建的 `syscall_trap_source` Capability ID。
pub const EXC_SYSCALL_TRAP_CAP: u64 = bootstrap::EXC_SOURCE_CAP_BASE as u64 + 1;
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

/// errno 编码（Linux 兼容负值）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Errno {
    /// 找不到对象（不存在的 Capability）。
    NoEnt = 2,
    /// 权限不足（缺权限 / 非持有者 / 已失效）。
    Access = 13,
    /// 用户指针不可读（未映射或非 user 页）。
    Fault = 14,
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

/// errno 的稳定名字（协议/日志用）。
pub const fn errno_name(errno: Errno) -> &'static str {
    match errno {
        Errno::NoEnt => "ENOENT",
        Errno::Access => "EACCES",
        Errno::Fault => "EFAULT",
        Errno::Invalid => "EINVAL",
        Errno::NoSpace => "ENOSPC",
        Errno::NoSys => "ENOSYS",
    }
}

/// 权限编码到权限名的映射（纯函数，host 可测）。
pub const fn right_name(code: u64) -> Option<&'static str> {
    match code {
        RIGHT_SEGMAPR => Some("SegMapR"),
        RIGHT_SEGMAPW => Some("SegMapW"),
        _ => None,
    }
}

/// 授权失败原因到 errno 的映射（纯函数，host 可测）。
pub const fn auth_errno(error: AuthError) -> Errno {
    match error {
        AuthError::NoSuchCapability | AuthError::NotInstalled => Errno::NoEnt,
        AuthError::Inactive | AuthError::NotHolder | AuthError::MissingRight => Errno::Access,
    }
}

/// 切片 K：授权查询服务——查 Bootstrap 授权表并编码结果。
fn authorize_call(cap_id: u64, right_code: u64) -> u64 {
    let Some(right) = right_name(right_code) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::authorize(AUTHORIZE_TASK, cap_id as u32, right) {
        Ok(granted) => granted as u64,
        Err(error) => encode_errno(auth_errno(error)),
    }
}

/// 切片 L：异常种类编码到 [`ExcKind`] 的映射（纯函数，host 可测）。
pub const fn kind_of(code: u64) -> Option<ExcKind> {
    match code {
        EXC_KIND_PAGEFAULT => Some(ExcKind::PageFault),
        EXC_KIND_SYSCALLTRAP => Some(ExcKind::SyscallTrap),
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
    }
}

/// 切片 L：订阅服务——经全局授权表登记订阅。
fn exc_subscribe_call(cap_id: u64, kind_code: u64) -> u64 {
    let Some(kind) = kind_of(kind_code) else {
        return encode_errno(Errno::Invalid);
    };
    match bootstrap::exc_subscribe(AUTHORIZE_TASK, cap_id as u32, kind) {
        Ok(source_id) => source_id as u64,
        Err(error) => encode_errno(exc_errno(error)),
    }
}

/// 切片 L：取消订阅服务——经全局授权表移除订阅。
fn exc_unsubscribe_call(cap_id: u64) -> u64 {
    match bootstrap::exc_unsubscribe(AUTHORIZE_TASK, cap_id as u32) {
        Ok(source_id) => source_id as u64,
        Err(error) => encode_errno(exc_errno(error)),
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

/// 从用户缓冲读入 `len` 字节并组装为 little-endian u64（内核/QEMU 专用）。
fn copy_in(ptr: u64, len: u64) -> Result<u64, Errno> {
    copy_in_bounds(ptr, len)?;
    let Some((root, offset)) = crate::cow::root_and_offset() else {
        return Err(Errno::Fault);
    };
    let map = |phys: u64| phys_to_virt(phys, offset);
    let pte = vspace::read_leaf(root, ptr, map).map_err(|_| Errno::Fault)?;
    if pte & PT_PRESENT == 0 || pte & PT_USER == 0 {
        return Err(Errno::Fault);
    }
    let mut value = 0u64;
    let mut i = 0;
    while i < len {
        // 单核 boot 早期；指针已由上面的页表检查证明可达。
        let byte = unsafe { core::ptr::read_volatile((ptr + i) as *const u8) };
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

static mut CALLS: [CallRecord; MAX_CALLS] = [CallRecord { cs: 0, ret: 0 }; MAX_CALLS];

/// 已进入的 syscall 次数。
static COUNT: AtomicU64 = AtomicU64::new(0);

/// 由裸入口 `idt::syscall_entry` 调用的内核侧处理。
///
/// `frame` 指向栈上 8 个 qword 的现场帧：`[0]=调用号(rax)`、`[1..7]=参数(rdi,rsi,rdx,
/// r10,r8,r9)`、`[7]=进入时 CS`。
///
/// # Safety
///
/// 仅由 `syscall_entry` 在 CPL3→CPL0 的 `int 0x80` 现场调用；`frame` 必须指向上述
/// 布局的有效只读数组。从 Rust 侧传入其它指针属未定义行为。
pub unsafe extern "C" fn handle(frame: *const u64) -> u64 {
    let num = unsafe { *frame };
    let mut args = [0u64; MAX_ARGS];
    let mut i = 0;
    while i < MAX_ARGS {
        args[i] = unsafe { *frame.add(1 + i) };
        i += 1;
    }
    let cs = unsafe { *frame.add(1 + MAX_ARGS) };

    let ret = serve(num, &args);

    let index = COUNT.fetch_add(1, Ordering::Relaxed) as usize;
    if index < MAX_CALLS {
        // 单核 boot 早期：写入与读取之间无并发，故用裸指针而非原子数组。
        unsafe {
            let calls = core::ptr::addr_of_mut!(CALLS);
            (*calls)[index] = CallRecord { cs, ret };
        }
    }
    ret
}

/// 服务分派（`int 0x80` 与 `syscall` 快路径共用），不含进入记录。
fn serve(num: u64, args: &[u64; MAX_ARGS]) -> u64 {
    if num == SYS_COPY_IN {
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

static mut FAST: FastRecord = FastRecord {
    num: 0,
    arg: 0,
    cs: 0,
    ret: 0,
};

/// 由裸入口 `idt::syscall_fast_entry` 调用的内核侧处理（与 [`handle`] 共用 [`serve`]）。
///
/// `frame` 布局与 `int 0x80` 路径相同：`[0]=号`、`[1..7]=参数`、`[7]=CS`（快路径由入口
/// 填 [`USER_CS`] 常量，因为 `syscall` 不压栈 CS）。
///
/// # Safety
///
/// 仅由 `syscall_fast_entry` 在 CPL3 `syscall` 现场调用；`frame` 必须指向上述布局的
/// 有效只读数组。
pub unsafe extern "C" fn handle_fast(frame: *const u64) -> u64 {
    let num = unsafe { *frame };
    let mut args = [0u64; MAX_ARGS];
    let mut i = 0;
    while i < MAX_ARGS {
        args[i] = unsafe { *frame.add(1 + i) };
        i += 1;
    }
    let cs = unsafe { *frame.add(1 + MAX_ARGS) };
    let ret = serve(num, &args);
    unsafe {
        let fast = core::ptr::addr_of_mut!(FAST);
        (*fast) = FastRecord {
            num,
            arg: args[0],
            cs,
            ret,
        };
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
    unsafe {
        let efer = crate::msr::rdmsr(crate::msr::IA32_EFER);
        crate::msr::wrmsr(crate::msr::IA32_EFER, crate::msr::efer_with_sce(efer));
        crate::msr::wrmsr(crate::msr::IA32_STAR, crate::msr::STAR_VALUE);
        crate::msr::wrmsr(
            crate::msr::IA32_LSTAR,
            crate::idt::syscall_fast_entry as *const () as u64,
        );
        crate::msr::wrmsr(crate::msr::IA32_FMASK, crate::msr::SYSCALL_FMASK);
    }
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
    }
}

/// 收集 `syscall`/`sysret` 快路径见证（在用户 `int3` 收尾回调中调用）。
pub fn fast_witness() -> FastWitness {
    let record = unsafe {
        let fast = core::ptr::addr_of!(FAST);
        *fast
    };
    let efer = unsafe { crate::msr::rdmsr(crate::msr::IA32_EFER) };
    let star = unsafe { crate::msr::rdmsr(crate::msr::IA32_STAR) };
    let lstar = unsafe { crate::msr::rdmsr(crate::msr::IA32_LSTAR) };
    let fmask = unsafe { crate::msr::rdmsr(crate::msr::IA32_FMASK) };
    FastWitness {
        cs: record.cs,
        num: record.num,
        arg: record.arg,
        ret: record.ret,
        receipt: read_qword(USER_Y_VA + 56),
        efer_sce: (efer & crate::msr::EFER_SCE != 0) as u32,
        star,
        fmask,
        lstar_ok: (lstar == crate::idt::syscall_fast_entry as *const () as u64) as u32,
    }
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
    unsafe { core::ptr::read_volatile(va as *const u64) }
}

/// 读取前 [`MAX_CALLS`] 条进入记录（单核 boot 早期，无并发）。
fn call_records() -> [CallRecord; MAX_CALLS] {
    unsafe {
        let ptr = core::ptr::addr_of!(CALLS);
        *ptr
    }
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
pub fn pending_witness() -> PendingWitness {
    let audit = exc_audit_snapshot();
    match bootstrap::exc_boot_record() {
        Some(boot) => PendingWitness {
            delivered: boot.delivered,
            pending: boot.pending,
            message_id: boot.message_id,
            payload: boot.payload,
            task_ready_after: boot.task_ready_after,
            audit_raised: audit.raised,
        },
        None => PendingWitness {
            delivered: 0,
            pending: 0,
            message_id: 0,
            payload: 0,
            task_ready_after: false,
            audit_raised: audit.raised,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn unknown_number_is_enosys() {
        let args = [1u64; MAX_ARGS];
        assert_eq!(dispatch_plain(SYS_BAD, &args), encode_errno(Errno::NoSys));
        assert_eq!(dispatch_plain(u64::MAX, &args), encode_errno(Errno::NoSys));
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
    fn right_name_maps_known_codes() {
        assert_eq!(right_name(RIGHT_SEGMAPR), Some("SegMapR"));
        assert_eq!(right_name(RIGHT_SEGMAPW), Some("SegMapW"));
        assert_eq!(right_name(0), None);
    }

    #[test]
    fn auth_errno_maps_failure_reasons() {
        assert_eq!(auth_errno(AuthError::NoSuchCapability), Errno::NoEnt);
        assert_eq!(auth_errno(AuthError::NotInstalled), Errno::NoEnt);
        assert_eq!(auth_errno(AuthError::MissingRight), Errno::Access);
        assert_eq!(auth_errno(AuthError::NotHolder), Errno::Access);
        assert_eq!(auth_errno(AuthError::Inactive), Errno::Access);
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
}
