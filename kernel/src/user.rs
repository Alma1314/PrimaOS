//! 切片 G：最小 ring3 用户任务 —— 在 CPL3 执行用户代码并触发真实用户态 #PF。
//!
//! 用户代码（手写机器码）：
//! 1. 把 [`USER_X_VA`]（**只读+user**）的地址装入 `rdx`、把 [`USER_STORE_VALUE`] 装入
//!    `rax`，执行 3 字节 store → 真实 `present∧write∧user` #PF（`err=0x7`，US 位证明来自 CPL3）；
//! 2. 内核 COW 换映射后**重试**该 store，成功，并把回执值写入 [`USER_Y_VA`]；
//! 3. 连续 6 次 `int 0x80`（syscall ABI v1）：ECHO、SUM4（多寄存器）、COPY_IN 合法指针、
//!    COPY_IN 坏指针（`EFAULT`）、保留号 `0`（`EINVAL`）、未登记号（`ENOSYS`），依次把
//!    返回值写入 `Y+8..Y+48`；
//! 4. 3 次能力授权查询（SPEC-049）：持有 `SegMapR`（授予）、缺 `SegMapW`（`EACCES`）、
//!    不存在的 cap（`ENOENT`），依次写入 `Y+64..Y+80`；
//! 5. 一次 `syscall` 指令（x86-64 快路径，SPEC-048）：ECHO，把返回值写入 `Y+56`；
//! 6. `int3` 从 CPL3 陷入内核，由注册的回调收尾。
//!
//! **范围边界：** 单核、非抢占、单任务最小见证；syscall 部分是**递送**而非抽象模型的
//! capability/订阅授权语义，不做多页/多任务用户态 COW 协议，不做 Linux trace/`COMPAT-TEST`。

use crate::exc::SretFrame;
use crate::gdt::{USER_CS, USER_DS};
use crate::mem::{FRAME_SIZE, phys_to_virt};
use crate::syscall::{
    AUTHORIZE_CAP, BAD_PTR, COPY_LEN, COPY_PTR, FAST_ARG, RIGHT_SEGMAPR, RIGHT_SEGMAPW, SUM_ARGS,
    SYS_AUTHORIZE, SYS_BAD, SYS_COPY_IN, SYS_ECHO, SYS_ECHO_ARG, SYS_SUM4, UNSET_CAP,
};
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
pub const USER_CODE_LEN: usize = 357;

/// 把 `bytes` 写入 `code[at..]`（const 友好，避免 const 上下文里的迭代器）。
const fn put(code: &mut [u8; USER_CODE_LEN], at: usize, bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() {
        code[at + i] = bytes[i];
        i += 1;
    }
}

/// 写入 `opcode0 opcode1 <imm64>` 形态的指令（`mov r64, imm64`）。
const fn put_imm64(code: &mut [u8; USER_CODE_LEN], at: usize, opcode0: u8, opcode1: u8, imm: u64) {
    code[at] = opcode0;
    code[at + 1] = opcode1;
    put(code, at + 2, &imm.to_le_bytes());
}

/// 组装用户代码机器码：
/// `mov rdx, X; mov rax, VAL; mov [rdx], rax;`（COW 故障点）
/// `mov rcx, Y; mov [rcx], rax;`（回执 0）
/// `mov rdi, ARG; mov rax, SYS_ECHO; int 0x80; mov [rcx+8], rax;`（回执 1）
/// `mov rdi/rsi/rdx/r10, SUM_ARGS; mov rax, SYS_SUM4; int 0x80; mov [rcx+16], rax;`（回执 2）
/// `mov rdi, X; mov rsi, 8; mov rax, SYS_COPY_IN; int 0x80; mov [rcx+24], rax;`（回执 3）
/// `mov rdi, BAD_PTR; mov rsi, 8; mov rax, SYS_COPY_IN; int 0x80; mov [rcx+32], rax;`（回执 4）
/// `mov rax, 0; int 0x80; mov [rcx+40], rax;`（回执 5）
/// `mov rax, SYS_BAD; int 0x80; mov [rcx+48], rax; int3`（回执 6）
pub const fn user_code() -> [u8; USER_CODE_LEN] {
    let mut code = [0u8; USER_CODE_LEN];
    // 0: mov rdx, X
    put_imm64(&mut code, 0, 0x48, 0xBA, USER_X_VA);
    // 10: mov rax, VAL
    put_imm64(&mut code, 10, 0x48, 0xB8, USER_STORE_VALUE);
    // 20: mov [rdx], rax  （3 字节，与 FAULT_SKIP_LEN 成对）
    put(&mut code, 20, &[0x48, 0x89, 0x02]);
    // 23: mov rcx, Y
    put_imm64(&mut code, 23, 0x48, 0xB9, USER_Y_VA);
    // 33: mov [rcx], rax
    put(&mut code, 33, &[0x48, 0x89, 0x01]);
    // 36: mov rdi, SYS_ECHO_ARG
    put_imm64(&mut code, 36, 0x48, 0xBF, SYS_ECHO_ARG);
    // 46: mov rax, SYS_ECHO
    put_imm64(&mut code, 46, 0x48, 0xB8, SYS_ECHO);
    // 56: int 0x80
    put(&mut code, 56, &[0xCD, 0x80]);
    // 58: mov [rcx+8], rax
    put(&mut code, 58, &[0x48, 0x89, 0x41, 0x08]);
    // 62: mov rdi, SUM_ARGS[0]
    put_imm64(&mut code, 62, 0x48, 0xBF, SUM_ARGS[0]);
    // 72: mov rsi, SUM_ARGS[1]
    put_imm64(&mut code, 72, 0x48, 0xBE, SUM_ARGS[1]);
    // 82: mov rdx, SUM_ARGS[2]
    put_imm64(&mut code, 82, 0x48, 0xBA, SUM_ARGS[2]);
    // 92: mov r10, SUM_ARGS[3]
    put_imm64(&mut code, 92, 0x49, 0xBA, SUM_ARGS[3]);
    // 102: mov rax, SYS_SUM4
    put_imm64(&mut code, 102, 0x48, 0xB8, SYS_SUM4);
    // 112: int 0x80
    put(&mut code, 112, &[0xCD, 0x80]);
    // 114: mov [rcx+16], rax
    put(&mut code, 114, &[0x48, 0x89, 0x41, 0x10]);
    // 118: mov rdi, COPY_PTR
    put_imm64(&mut code, 118, 0x48, 0xBF, COPY_PTR);
    // 128: mov rsi, COPY_LEN
    put_imm64(&mut code, 128, 0x48, 0xBE, COPY_LEN);
    // 138: mov rax, SYS_COPY_IN
    put_imm64(&mut code, 138, 0x48, 0xB8, SYS_COPY_IN);
    // 148: int 0x80
    put(&mut code, 148, &[0xCD, 0x80]);
    // 150: mov [rcx+24], rax
    put(&mut code, 150, &[0x48, 0x89, 0x41, 0x18]);
    // 154: mov rdi, BAD_PTR
    put_imm64(&mut code, 154, 0x48, 0xBF, BAD_PTR);
    // 164: mov rsi, COPY_LEN
    put_imm64(&mut code, 164, 0x48, 0xBE, COPY_LEN);
    // 174: mov rax, SYS_COPY_IN
    put_imm64(&mut code, 174, 0x48, 0xB8, SYS_COPY_IN);
    // 184: int 0x80
    put(&mut code, 184, &[0xCD, 0x80]);
    // 186: mov [rcx+32], rax
    put(&mut code, 186, &[0x48, 0x89, 0x41, 0x20]);
    // 190: mov rax, 0（保留号）
    put_imm64(&mut code, 190, 0x48, 0xB8, 0);
    // 200: int 0x80
    put(&mut code, 200, &[0xCD, 0x80]);
    // 202: mov [rcx+40], rax
    put(&mut code, 202, &[0x48, 0x89, 0x41, 0x28]);
    // 206: mov rax, SYS_BAD
    put_imm64(&mut code, 206, 0x48, 0xB8, SYS_BAD);
    // 216: int 0x80
    put(&mut code, 216, &[0xCD, 0x80]);
    // 218: mov [rcx+48], rax
    put(&mut code, 218, &[0x48, 0x89, 0x41, 0x30]);
    // 222: mov rdi, AUTHORIZE_CAP
    put_imm64(&mut code, 222, 0x48, 0xBF, AUTHORIZE_CAP);
    // 232: mov rsi, RIGHT_SEGMAPR
    put_imm64(&mut code, 232, 0x48, 0xBE, RIGHT_SEGMAPR);
    // 242: mov rax, SYS_AUTHORIZE
    put_imm64(&mut code, 242, 0x48, 0xB8, SYS_AUTHORIZE);
    // 252: int 0x80 → 授予
    put(&mut code, 252, &[0xCD, 0x80]);
    // 254: mov [rcx+64], rax
    put(&mut code, 254, &[0x48, 0x89, 0x41, 0x40]);
    // 258: mov rsi, RIGHT_SEGMAPW（rdi 仍是 cap）
    put_imm64(&mut code, 258, 0x48, 0xBE, RIGHT_SEGMAPW);
    // 268: mov rax, SYS_AUTHORIZE
    put_imm64(&mut code, 268, 0x48, 0xB8, SYS_AUTHORIZE);
    // 278: int 0x80 → 拒绝
    put(&mut code, 278, &[0xCD, 0x80]);
    // 280: mov [rcx+72], rax
    put(&mut code, 280, &[0x48, 0x89, 0x41, 0x48]);
    // 284: mov rdi, UNSET_CAP
    put_imm64(&mut code, 284, 0x48, 0xBF, UNSET_CAP);
    // 294: mov rsi, RIGHT_SEGMAPR
    put_imm64(&mut code, 294, 0x48, 0xBE, RIGHT_SEGMAPR);
    // 304: mov rax, SYS_AUTHORIZE
    put_imm64(&mut code, 304, 0x48, 0xB8, SYS_AUTHORIZE);
    // 314: int 0x80 → ENOENT
    put(&mut code, 314, &[0xCD, 0x80]);
    // 316: mov [rcx+80], rax
    put(&mut code, 316, &[0x48, 0x89, 0x41, 0x50]);
    // 320: mov rdi, FAST_ARG
    put_imm64(&mut code, 320, 0x48, 0xBF, FAST_ARG);
    // 330: mov rax, SYS_ECHO
    put_imm64(&mut code, 330, 0x48, 0xB8, SYS_ECHO);
    // 340: 0F 05  syscall（快路径；破坏 rcx/r11）
    put(&mut code, 340, &[0x0F, 0x05]);
    // 342: mov rcx, Y（syscall 破坏了 rcx，需重载回执基址）
    put_imm64(&mut code, 342, 0x48, 0xB9, USER_Y_VA);
    // 352: mov [rcx+56], rax
    put(&mut code, 352, &[0x48, 0x89, 0x41, 0x38]);
    // 356: int3
    code[356] = 0xCC;
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
        assert_eq!(USER_CODE_LEN, 357);
        // COW 故障点与回执 0。
        assert_eq!(&code[0..2], &[0x48, 0xBA]); // mov rdx, imm64
        assert_eq!(&code[2..10], &USER_X_VA.to_le_bytes());
        assert_eq!(&code[10..12], &[0x48, 0xB8]); // mov rax, imm64
        assert_eq!(&code[12..20], &USER_STORE_VALUE.to_le_bytes());
        assert_eq!(&code[20..23], &[0x48, 0x89, 0x02]); // mov [rdx], rax (3 bytes)
        assert_eq!(&code[23..25], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[25..33], &USER_Y_VA.to_le_bytes());
        assert_eq!(&code[33..36], &[0x48, 0x89, 0x01]); // mov [rcx], rax
        // 回执 1：SYS_ECHO。
        assert_eq!(&code[36..38], &[0x48, 0xBF]); // mov rdi, imm64
        assert_eq!(&code[38..46], &SYS_ECHO_ARG.to_le_bytes());
        assert_eq!(&code[46..48], &[0x48, 0xB8]); // mov rax, imm64
        assert_eq!(&code[48..56], &SYS_ECHO.to_le_bytes());
        assert_eq!(&code[56..58], &[0xCD, 0x80]); // int 0x80
        assert_eq!(&code[58..62], &[0x48, 0x89, 0x41, 0x08]); // mov [rcx+8], rax
        // 回执 2：SYS_SUM4，参数经 rdi/rsi/rdx/r10。
        assert_eq!(&code[62..64], &[0x48, 0xBF]);
        assert_eq!(&code[64..72], &SUM_ARGS[0].to_le_bytes());
        assert_eq!(&code[72..74], &[0x48, 0xBE]);
        assert_eq!(&code[74..82], &SUM_ARGS[1].to_le_bytes());
        assert_eq!(&code[82..84], &[0x48, 0xBA]);
        assert_eq!(&code[84..92], &SUM_ARGS[2].to_le_bytes());
        assert_eq!(&code[92..94], &[0x49, 0xBA]); // mov r10, imm64
        assert_eq!(&code[94..102], &SUM_ARGS[3].to_le_bytes());
        assert_eq!(&code[102..104], &[0x48, 0xB8]);
        assert_eq!(&code[104..112], &SYS_SUM4.to_le_bytes());
        assert_eq!(&code[112..114], &[0xCD, 0x80]);
        assert_eq!(&code[114..118], &[0x48, 0x89, 0x41, 0x10]);
        // 回执 3/4：SYS_COPY_IN 合法与坏指针。
        assert_eq!(&code[118..120], &[0x48, 0xBF]);
        assert_eq!(&code[120..128], &COPY_PTR.to_le_bytes());
        assert_eq!(&code[128..130], &[0x48, 0xBE]);
        assert_eq!(&code[130..138], &COPY_LEN.to_le_bytes());
        assert_eq!(&code[138..140], &[0x48, 0xB8]);
        assert_eq!(&code[140..148], &SYS_COPY_IN.to_le_bytes());
        assert_eq!(&code[148..150], &[0xCD, 0x80]);
        assert_eq!(&code[150..154], &[0x48, 0x89, 0x41, 0x18]);
        assert_eq!(&code[154..156], &[0x48, 0xBF]);
        assert_eq!(&code[156..164], &BAD_PTR.to_le_bytes());
        assert_eq!(&code[164..166], &[0x48, 0xBE]);
        assert_eq!(&code[166..174], &COPY_LEN.to_le_bytes());
        assert_eq!(&code[174..176], &[0x48, 0xB8]);
        assert_eq!(&code[176..184], &SYS_COPY_IN.to_le_bytes());
        assert_eq!(&code[184..186], &[0xCD, 0x80]);
        assert_eq!(&code[186..190], &[0x48, 0x89, 0x41, 0x20]);
        // 回执 5/6：保留号 0 与未登记号。
        assert_eq!(&code[190..192], &[0x48, 0xB8]);
        assert_eq!(&code[192..200], &0u64.to_le_bytes());
        assert_eq!(&code[200..202], &[0xCD, 0x80]);
        assert_eq!(&code[202..206], &[0x48, 0x89, 0x41, 0x28]);
        assert_eq!(&code[206..208], &[0x48, 0xB8]);
        assert_eq!(&code[208..216], &SYS_BAD.to_le_bytes());
        assert_eq!(&code[216..218], &[0xCD, 0x80]);
        assert_eq!(&code[218..222], &[0x48, 0x89, 0x41, 0x30]);
        // 授权查询三次。
        assert_eq!(&code[222..224], &[0x48, 0xBF]);
        assert_eq!(&code[224..232], &AUTHORIZE_CAP.to_le_bytes());
        assert_eq!(&code[232..234], &[0x48, 0xBE]);
        assert_eq!(&code[234..242], &RIGHT_SEGMAPR.to_le_bytes());
        assert_eq!(&code[242..244], &[0x48, 0xB8]);
        assert_eq!(&code[244..252], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[252..254], &[0xCD, 0x80]);
        assert_eq!(&code[254..258], &[0x48, 0x89, 0x41, 0x40]); // [rcx+64]
        assert_eq!(&code[258..260], &[0x48, 0xBE]);
        assert_eq!(&code[260..268], &RIGHT_SEGMAPW.to_le_bytes());
        assert_eq!(&code[268..270], &[0x48, 0xB8]);
        assert_eq!(&code[270..278], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[278..280], &[0xCD, 0x80]);
        assert_eq!(&code[280..284], &[0x48, 0x89, 0x41, 0x48]); // [rcx+72]
        assert_eq!(&code[284..286], &[0x48, 0xBF]);
        assert_eq!(&code[286..294], &UNSET_CAP.to_le_bytes());
        assert_eq!(&code[294..296], &[0x48, 0xBE]);
        assert_eq!(&code[296..304], &RIGHT_SEGMAPR.to_le_bytes());
        assert_eq!(&code[304..306], &[0x48, 0xB8]);
        assert_eq!(&code[306..314], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[314..316], &[0xCD, 0x80]);
        assert_eq!(&code[316..320], &[0x48, 0x89, 0x41, 0x50]); // [rcx+80]
        // 快路径：syscall 指令调用 ECHO，再重载 rcx 写 Y+56。
        assert_eq!(&code[320..322], &[0x48, 0xBF]);
        assert_eq!(&code[322..330], &FAST_ARG.to_le_bytes());
        assert_eq!(&code[330..332], &[0x48, 0xB8]);
        assert_eq!(&code[332..340], &SYS_ECHO.to_le_bytes());
        assert_eq!(&code[340..342], &[0x0F, 0x05]); // syscall
        assert_eq!(&code[342..344], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[344..352], &USER_Y_VA.to_le_bytes());
        assert_eq!(&code[352..356], &[0x48, 0x89, 0x41, 0x38]); // mov [rcx+56], rax
        assert_eq!(code[356], 0xCC); // int3
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
