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

/// errno 编码（Linux 兼容负值）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Errno {
    /// 参数非法（保留号、长度越界等）。
    Invalid = 22,
    /// 用户指针不可读（未映射或非 user 页）。
    Fault = 14,
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
        Errno::Invalid => "EINVAL",
        Errno::Fault => "EFAULT",
        Errno::NoSys => "ENOSYS",
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
const MAX_CALLS: usize = 6;

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

    let ret = if num == SYS_COPY_IN {
        match copy_in(args[0], args[1]) {
            Ok(value) => value,
            Err(errno) => encode_errno(errno),
        }
    } else {
        dispatch_plain(num, &args)
    };

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

/// 一次 syscall ABI 见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyscallWitness {
    /// 进入的 syscall 次数（应为 6）。
    pub calls: u64,
    /// 第一次进入时的 CS（应 [`USER_CS`]，RPL=3）。
    pub cs: u64,
    /// 全部 6 次进入是否都来自 CPL3（CS 均为 [`USER_CS`]）。
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
        self.calls == 6
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

/// 收集 syscall ABI 见证（在用户 `int3` 收尾回调中调用）。
pub fn witness() -> SyscallWitness {
    let calls = COUNT.load(Ordering::Relaxed);
    let records = unsafe {
        let ptr = core::ptr::addr_of!(CALLS);
        *ptr
    };
    let cs_all = records.iter().all(|record| record.cs == USER_CS as u64) as u32;
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
        assert!(((BAD_PTR >> 21) & 0x1ff) != ((USER_X_VA >> 21) & 0x1ff));
    }

    fn valid() -> SyscallWitness {
        SyscallWitness {
            calls: 6,
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
}
