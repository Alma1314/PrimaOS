//! 切片 G：最小 ring3 用户任务 —— 在 CPL3 执行用户代码并触发真实用户态 #PF。
//!
//! 用户代码（手写机器码）：
//! 1. 把 [`USER_X_VA`]（**只读+user**）的地址装入 `rdx`、把 [`USER_STORE_VALUE`] 装入
//!    `rax`，执行 3 字节 store → 真实 `present∧write∧user` #PF（`err=0x7`，US 位证明来自 CPL3）；
//! 2. 内核 COW 换映射后**重试**该 store，成功；
//! 3. 把回执值写入 [`USER_Y_VA`]（可写+user）；
//! 4. `int3` 从 CPL3 陷入内核，由注册的回调收尾。
//!
//! **范围边界：** 单核、非抢占、单任务最小见证；不实现完整 syscall ABI，不实现抽象模型的
//! `exc_resume` 授权语义，不做多页/多任务用户态 COW 协议，不做 Linux trace/`COMPAT-TEST`。

use crate::exc::SretFrame;
use crate::gdt::{USER_CS, USER_DS};
use crate::mem::{FRAME_SIZE, phys_to_virt};
use crate::vspace::{self, VspaceError};

/// 用户空间基址：PML4 槽 20（实测槽 0..8 已被 bootloader/内核占用，槽 16 归切片 F）。
pub const USER_BASE_VA: u64 = 0x0000_0A00_0000_0000;

/// 用户代码页（只读、可执行、user）。
pub const USER_CODE_VA: u64 = USER_BASE_VA;
/// 用户栈页（可写、user）。
pub const USER_STACK_VA: u64 = USER_BASE_VA + 0x1000;
/// COW 数据页 X（**只读**、user），写它触发用户态写保护 #PF。
pub const USER_X_VA: u64 = USER_BASE_VA + 0x2000;
/// 回执页 Y（可写、user），用户代码写入以证明 CPL3 执行与重试成功。
pub const USER_Y_VA: u64 = USER_BASE_VA + 0x3000;

/// 用户代码写入的值（亦为回执值）。
pub const USER_STORE_VALUE: u64 = 0x5A5A_1234_DEAD_BEEF;

/// 用户代码字节长度。
pub const USER_CODE_LEN: usize = 37;

/// 组装用户代码机器码：
/// `mov rdx, USER_X_VA; mov rax, USER_STORE_VALUE; mov [rdx], rax;`
/// `mov rcx, USER_Y_VA; mov [rcx], rax; int3`
pub const fn user_code() -> [u8; USER_CODE_LEN] {
    let x = USER_X_VA.to_le_bytes();
    let y = USER_Y_VA.to_le_bytes();
    let v = USER_STORE_VALUE.to_le_bytes();
    let mut code = [0u8; USER_CODE_LEN];
    // 0: 48 BA <imm64>  mov rdx, X
    code[0] = 0x48;
    code[1] = 0xBA;
    code[2] = x[0];
    code[3] = x[1];
    code[4] = x[2];
    code[5] = x[3];
    code[6] = x[4];
    code[7] = x[5];
    code[8] = x[6];
    code[9] = x[7];
    // 10: 48 B8 <imm64>  mov rax, VAL
    code[10] = 0x48;
    code[11] = 0xB8;
    code[12] = v[0];
    code[13] = v[1];
    code[14] = v[2];
    code[15] = v[3];
    code[16] = v[4];
    code[17] = v[5];
    code[18] = v[6];
    code[19] = v[7];
    // 20: 48 89 02  mov [rdx], rax  （3 字节，与 FAULT_SKIP_LEN 成对）
    code[20] = 0x48;
    code[21] = 0x89;
    code[22] = 0x02;
    // 23: 48 B9 <imm64>  mov rcx, Y
    code[23] = 0x48;
    code[24] = 0xB9;
    code[25] = y[0];
    code[26] = y[1];
    code[27] = y[2];
    code[28] = y[3];
    code[29] = y[4];
    code[30] = y[5];
    code[31] = y[6];
    code[32] = y[7];
    // 33: 48 89 01  mov [rcx], rax
    code[33] = 0x48;
    code[34] = 0x89;
    code[35] = 0x01;
    // 36: CC  int3
    code[36] = 0xCC;
    code
}

/// 一次用户态 COW 见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserWitness {
    /// 用户写故障的 CR2（应 [`USER_X_VA`]）。
    pub va: u64,
    /// 用户写故障的硬件 error code（应 0x7 = P|W|US，证明来自 CPL3）。
    pub err: u64,
    /// 分类名（应 `write_protection`）。
    pub class: &'static str,
    /// 原共享帧首 qword（应仍为 `cow::pattern(0)`，证明复制而非原地写）。
    pub shared0: u64,
    /// 私有帧首 qword（重试成功的 store，应 [`USER_STORE_VALUE`]）。
    pub private0: u64,
    /// 私有帧第 1 个 qword（应 `cow::pattern(1)`，证明整页已复制）。
    pub private1: u64,
    /// 回执页 Y 的值（应 [`USER_STORE_VALUE`]，证明 CPL3 继续执行到 int3）。
    pub receipt: u64,
}

impl UserWitness {
    /// 纯判定：一次用户态 COW 见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.va == USER_X_VA
            && self.err & 0x3 == 0x3
            && self.err & (1 << 2) != 0 // US 位置位 ⇒ 故障来自 CPL3
            && self.class == "write_protection"
            && self.shared0 == crate::cow::pattern(0)
            && self.private0 == USER_STORE_VALUE
            && self.private1 == crate::cow::pattern(1)
            && self.receipt == USER_STORE_VALUE
    }
}

struct UserCtx {
    offset: u64,
    orig_x_phys: u64,
}

static mut USER_CTX: Option<UserCtx> = None;

fn read_qword(va: u64) -> u64 {
    unsafe { core::ptr::read_volatile(va as *const u64) }
}

fn write_qword(va: u64, value: u64) {
    unsafe { core::ptr::write_volatile(va as *mut u64, value) };
}

/// 构造用户任务：分配并映射代码/栈/X/Y 四页，填代码与 X 的模式。
///
/// 必须在 [`crate::cow::prepare`] **之后**调用（帧与页表页都取自 COW 上下文的唯一分配器，
/// 以避免多份 `FreeFrameAllocator` 副本各自推进 `next` 而重复发放同一物理帧）。
pub fn prepare(root_phys: u64, offset: u64) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    for va in [USER_CODE_VA, USER_STACK_VA, USER_X_VA, USER_Y_VA] {
        if !vspace::slot_free(root_phys, va, map) {
            return Err(VspaceError::SlotOccupied);
        }
    }

    let code = crate::cow::alloc_frame().ok_or(VspaceError::NoFrame)?;
    let stack = crate::cow::alloc_frame().ok_or(VspaceError::NoFrame)?;
    let x = crate::cow::alloc_frame().ok_or(VspaceError::NoFrame)?;
    let y = crate::cow::alloc_frame().ok_or(VspaceError::NoFrame)?;

    let code_bytes = user_code();
    for (index, &byte) in code_bytes.iter().enumerate() {
        unsafe { core::ptr::write_volatile((map(code) + index as u64) as *mut u8, byte) };
    }
    for index in 0..(FRAME_SIZE / 8) {
        write_qword(map(x) + index * 8, crate::cow::pattern(index));
    }
    // 栈与 Y 置零（Y 由用户代码写入回执）。
    for index in 0..(FRAME_SIZE / 8) {
        write_qword(map(stack) + index * 8, 0);
        write_qword(map(y) + index * 8, 0);
    }

    vspace::map_4k(
        root_phys,
        USER_CODE_VA,
        map,
        crate::cow::alloc_frame,
        code,
        false,
        true,
    )?;
    vspace::map_4k(
        root_phys,
        USER_STACK_VA,
        map,
        crate::cow::alloc_frame,
        stack,
        true,
        true,
    )?;
    // X 只读+user：写它触发用户态写保护 #PF。
    vspace::map_4k(
        root_phys,
        USER_X_VA,
        map,
        crate::cow::alloc_frame,
        x,
        false,
        true,
    )?;
    vspace::map_4k(
        root_phys,
        USER_Y_VA,
        map,
        crate::cow::alloc_frame,
        y,
        true,
        true,
    )?;
    for va in [USER_CODE_VA, USER_STACK_VA, USER_X_VA, USER_Y_VA] {
        unsafe { vspace::invlpg(va) };
    }

    crate::cow::register_user_region(USER_X_VA, x);

    unsafe {
        let ctx = core::ptr::addr_of_mut!(USER_CTX);
        *ctx = Some(UserCtx {
            offset,
            orig_x_phys: x,
        });
    }
    Ok(())
}

/// 进入 CPL3 执行用户代码（不返回；用户代码最终以 `int3` 陷入内核）。
pub fn enter() -> ! {
    let mut frame = SretFrame::ZERO;
    frame.rip = USER_CODE_VA;
    frame.rsp = USER_STACK_VA + FRAME_SIZE;
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    frame.rflags = 0x2; // bit1 固定；IF=0
    // 复用切片 D 验证过的 5 元 iretq 跳板（含特权级切换）。
    unsafe { crate::exc::resume(&frame) }
}

/// 收集用户态 COW 见证（在 `int3` 回调中调用）。
pub fn witness(err: u64) -> UserWitness {
    let (offset, orig_x_phys) = unsafe {
        let ctx = core::ptr::addr_of!(USER_CTX);
        match (*ctx).as_ref() {
            Some(ctx) => (ctx.offset, ctx.orig_x_phys),
            None => (0, 0),
        }
    };
    let shared0 = read_qword(phys_to_virt(orig_x_phys, offset));
    UserWitness {
        va: USER_X_VA,
        err,
        class: crate::fault::classify(err).as_str(),
        shared0,
        private0: read_qword(USER_X_VA),
        private1: read_qword(USER_X_VA + 8),
        receipt: read_qword(USER_Y_VA),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 便于构造成功证据。
    fn valid() -> UserWitness {
        UserWitness {
            va: USER_X_VA,
            err: 0x7,
            class: "write_protection",
            shared0: crate::cow::pattern(0),
            private0: USER_STORE_VALUE,
            private1: crate::cow::pattern(1),
            receipt: USER_STORE_VALUE,
        }
    }

    #[test]
    fn user_code_bytes_are_fixed() {
        let code = user_code();
        assert_eq!(&code[0..2], &[0x48, 0xBA]); // mov rdx, imm64
        assert_eq!(&code[10..12], &[0x48, 0xB8]); // mov rax, imm64
        assert_eq!(&code[20..23], &[0x48, 0x89, 0x02]); // mov [rdx], rax (3 bytes)
        assert_eq!(&code[23..25], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[33..36], &[0x48, 0x89, 0x01]); // mov [rcx], rax
        assert_eq!(code[36], 0xCC); // int3
        assert_eq!(&code[2..10], &USER_X_VA.to_le_bytes());
        assert_eq!(&code[12..20], &USER_STORE_VALUE.to_le_bytes());
        assert_eq!(&code[25..33], &USER_Y_VA.to_le_bytes());
    }

    #[test]
    fn user_vas_are_distinct_and_in_slot_twenty() {
        let vas = [USER_CODE_VA, USER_STACK_VA, USER_X_VA, USER_Y_VA];
        for (i, a) in vas.iter().enumerate() {
            assert_eq!((a >> 39) & 0x1ff, 20, "va {i} must be in PML4 slot 20");
        }
        for i in 0..vas.len() {
            for j in (i + 1)..vas.len() {
                assert_ne!(vas[i], vas[j]);
            }
        }
    }

    #[test]
    fn accepts_complete_user_cow_evidence() {
        assert!(valid().evidence_ok());
    }

    #[test]
    fn rejects_non_user_fault() {
        let mut w = valid();
        w.err = 0x3; // 缺 US 位 ⇒ 非 CPL3 来源
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_shared_page_modified() {
        let mut w = valid();
        w.shared0 = USER_STORE_VALUE;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_incomplete_copy() {
        let mut w = valid();
        w.private1 = 0;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_missing_receipt() {
        let mut w = valid();
        w.receipt = 0; // 用户代码未跑到 int3
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_failed_retry() {
        let mut w = valid();
        w.private0 = crate::cow::pattern(0);
        assert!(!w.evidence_ok());
    }
}
