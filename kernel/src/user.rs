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
//! 5. 6 次异常订阅（SPEC-050）：订阅 `page_fault`（授予）、重复订阅（`EACCES`）、
//!    kind 不符（`EINVAL`）、订阅 `syscall_trap`（授予）、取消订阅、再次取消（`EACCES`），
//!    依次写入 `Y+88..Y+128`；
//! 6. 一次 `syscall` 指令（x86-64 快路径，SPEC-048）：ECHO，把返回值写入 `Y+56`；
//! 7. `int3` 从 CPL3 陷入内核，由注册的回调收尾。
//!
//! **范围边界：** 单核、非抢占、单任务最小见证；syscall 部分是**递送**而非抽象模型的
//! capability/订阅授权语义，不做多页/多任务用户态 COW 协议，不做 Linux trace/`COMPAT-TEST`。

use core::sync::atomic::{AtomicU64, Ordering};

use crate::bootstrap::{
    INIT_TASK_ID, NAMESPACE_CAP_HANDLE, PLATFORM_TASK1_CAP_HANDLE, PLATFORM_TASK2_CAP_HANDLE,
    SegmentFrames, TASK_CAP_HANDLE,
};
use crate::exc::SretFrame;
use crate::frames;
use crate::gdt::{USER_CS, USER_DS};
use crate::handle::Handle;
use crate::mem::{FRAME_SIZE, phys_to_virt};
use crate::rights::{Right, Rights};
use crate::sched;
use crate::syscall::{
    AUTHORIZE_CAP, BAD_PTR, CAP_CHILD1_HANDLE, CAP_CHILD2_HANDLE, COPY_LEN, COPY_PTR,
    EXC_KIND_PAGEFAULT, EXC_KIND_SYSCALLTRAP, EXC_PAGE_FAULT_CAP, EXC_SYSCALL_TRAP_CAP, FAST_ARG,
    IPC_DIRECT_PAYLOAD, IPC_SHARED_MAGIC, MEMSEG_CAP1_HANDLE, MEMSEG_CAP2_HANDLE, MEMSEG_PATTERN,
    MEMSEG_RW_BITS, RIGHT_DERIVE, RIGHT_SEGMAPR, RIGHT_SEGMAPW, RIGHTS_DERIVE_BITS,
    RIGHTS_SEGMAPW_BITS, SUM_ARGS, SYS_AUTHORIZE, SYS_BAD, SYS_CAP_DERIVE, SYS_CAP_DROP,
    SYS_COPY_IN, SYS_ECHO, SYS_ECHO_ARG, SYS_EXC_SUBSCRIBE, SYS_EXC_UNSUBSCRIBE, SYS_EXC_WAIT,
    SYS_MEMSEG_CREATE, SYS_MEMSEG_DESTROY, SYS_MEMSEG_MAP, SYS_MEMSEG_UNMAP, SYS_RECV, SYS_SEND,
    SYS_SUM4, SYS_TASK_ID, SYS_TRAP, SYS_YIELD, TASK2_ID, TASK2_SELF_CAP, TASK2_SEND_CAP,
    UNSET_CAP, USER_SEG_SHARED_VA, USER_SEG_VA, USER2_RECV_BUF, USER2_RECV_BUF2, USER2_SHARED_VA,
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

/// 切片 M：第二个 ring3 任务的区域基址（独立 PML4 槽 21，避免与 task 1 的槽 20
/// 共享 `slot_free` 只按 PML4 present 判定的粒度）。
pub const USER2_BASE_VA: u64 = 0x0000_0A80_0000_0000;
/// 第二任务代码页（只读、可执行、user）。
pub const USER2_CODE_VA: u64 = USER2_BASE_VA;
/// 第二任务栈页（可写、user）。
pub const USER2_STACK_VA: u64 = USER2_BASE_VA + 0x1000;
/// 第二任务回执页（可写、user）。
pub const USER2_Y_VA: u64 = USER2_BASE_VA + 0x2000;

/// 任务 1 用户区所在的 PML4 槽。
pub const USER_SLOT: usize = 20;
/// 任务 2 用户区所在的 PML4 槽。
pub const USER2_SLOT: usize = 21;

/// 切片 O：抢占空转代码/数据页（两任务**同 VA**，各自根下映射到不同帧，用于实证隔离）。
///
/// 取槽 20 内更高的 2 MiB 区（i2=2/3），避开 [`crate::syscall::BAD_PTR`]（`+0x20_0000`，
/// i2=1）——后者必须始终未映射，否则 COPY_IN 的 `EFAULT` 证据会失效。
pub const SPIN_CODE_VA: u64 = USER_BASE_VA + 0x40_0000;
/// 抢占空转写标记的数据页 VA。
pub const SPIN_DATA_VA: u64 = USER_BASE_VA + 0x41_0000;

/// 抢占空转代码长度（`mov rax, imm64` + `mov [rax], imm32` + `jmp $`）。
pub const SPIN_CODE_LEN: usize = 19;

/// 每任务内核栈大小（叶子单元页，直接映射在各自 PML4 克隆出的内核高半区）。
pub const KSTACK_SIZE: usize = 16 * 1024;

/// 内核栈数组（只经地址使用，字段从不按元素读）。
#[repr(align(16))]
pub struct KStack(#[allow(dead_code)] [u8; KSTACK_SIZE]);

pub static mut KSTACK_TASK1: KStack = KStack([0; KSTACK_SIZE]);
pub static mut KSTACK_TASK2: KStack = KStack([0; KSTACK_SIZE]);

/// 内核栈顶（one-past-end，16 对齐）。
fn kstack_top(stack: *const KStack) -> u64 {
    stack as u64 + KSTACK_SIZE as u64
}

/// 任务 2 回执页 Y2 的物理帧（切片 O：task1 根下不可见 Y2 VA，witness 按物理读）。
static TASK2_Y_PHYS: AtomicU64 = AtomicU64::new(0);

/// 任务 2 回执页的物理帧（未准备时 `None`）。
pub fn task2_y_phys() -> Option<u64> {
    let value = TASK2_Y_PHYS.load(Ordering::Relaxed);
    if value != 0 { Some(value) } else { None }
}

/// 用户代码写入的值（亦为回执值）。
pub const USER_STORE_VALUE: u64 = 0x5A5A_1234_DEAD_BEEF;

/// 用户代码字节长度。
pub const USER_CODE_LEN: usize = 1795;

/// 第二任务代码字节长度。
pub const TASK2_CODE_LEN: usize = 472;

/// 把 `bytes` 写入 `code[at..]`（const 友好，避免 const 上下文里的迭代器）。
const fn put<const N: usize>(code: &mut [u8; N], at: usize, bytes: &[u8]) {
    let mut i = 0;
    while i < bytes.len() {
        code[at + i] = bytes[i];
        i += 1;
    }
}

/// 写入 `opcode0 opcode1 <imm64>` 形态的指令（`mov r64, imm64`）。
const fn put_imm64<const N: usize>(
    code: &mut [u8; N],
    at: usize,
    opcode0: u8,
    opcode1: u8,
    imm: u64,
) {
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
/// `mov rax, SYS_BAD; int 0x80; mov [rcx+48], rax;`（回执 6）
/// 6 次异常订阅（SPEC-050）：订阅 page_fault 成功、重复（`EACCES`）、kind 不符（`EINVAL`）、
/// 订阅 syscall_trap 成功、取消成功、再次取消（`EACCES`），写 `Y+88..Y+128`；
/// `mov rdi, FAST_ARG; mov rax, SYS_ECHO; syscall; mov rcx, Y; mov [rcx+56], rax`；
/// 切片 S：建共享 Segment → 映射进自己的根 → 写魔数 → 经**排队路径**把段 cap 送给 task 2 →
/// 让出 → 再经**直投路径**送一条无 cap 消息唤醒阻塞的 task 2 → 让出 → `int3`。
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
    // 320: mov rdi, EXC_PAGE_FAULT_CAP
    put_imm64(&mut code, 320, 0x48, 0xBF, EXC_PAGE_FAULT_CAP);
    // 330: mov rsi, EXC_KIND_PAGEFAULT
    put_imm64(&mut code, 330, 0x48, 0xBE, EXC_KIND_PAGEFAULT);
    // 340: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 340, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 350: int 0x80 → 订阅成功（返回 source_id 1）
    put(&mut code, 350, &[0xCD, 0x80]);
    // 352: mov [rcx+88], rax
    put(&mut code, 352, &[0x48, 0x89, 0x41, 0x58]);
    // 356: mov rsi, EXC_KIND_PAGEFAULT（rdi 仍是 cap）
    put_imm64(&mut code, 356, 0x48, 0xBE, EXC_KIND_PAGEFAULT);
    // 366: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 366, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 376: int 0x80 → 重复订阅（EACCES）
    put(&mut code, 376, &[0xCD, 0x80]);
    // 378: mov [rcx+96], rax
    put(&mut code, 378, &[0x48, 0x89, 0x41, 0x60]);
    // 382: mov rsi, EXC_KIND_SYSCALLTRAP
    put_imm64(&mut code, 382, 0x48, 0xBE, EXC_KIND_SYSCALLTRAP);
    // 392: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 392, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 402: int 0x80 → kind 不符（EINVAL）
    put(&mut code, 402, &[0xCD, 0x80]);
    // 404: mov [rcx+104], rax
    put(&mut code, 404, &[0x48, 0x89, 0x41, 0x68]);
    // 408: mov rdi, EXC_SYSCALL_TRAP_CAP
    put_imm64(&mut code, 408, 0x48, 0xBF, EXC_SYSCALL_TRAP_CAP);
    // 418: mov rsi, EXC_KIND_SYSCALLTRAP
    put_imm64(&mut code, 418, 0x48, 0xBE, EXC_KIND_SYSCALLTRAP);
    // 428: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 428, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 438: int 0x80 → 订阅成功（返回 source_id 2）
    put(&mut code, 438, &[0xCD, 0x80]);
    // 440: mov [rcx+112], rax
    put(&mut code, 440, &[0x48, 0x89, 0x41, 0x70]);
    // 444: mov rdi, EXC_PAGE_FAULT_CAP
    put_imm64(&mut code, 444, 0x48, 0xBF, EXC_PAGE_FAULT_CAP);
    // 454: mov rax, SYS_EXC_UNSUBSCRIBE
    put_imm64(&mut code, 454, 0x48, 0xB8, SYS_EXC_UNSUBSCRIBE);
    // 464: int 0x80 → 取消成功（返回 source_id 1）
    put(&mut code, 464, &[0xCD, 0x80]);
    // 466: mov [rcx+120], rax
    put(&mut code, 466, &[0x48, 0x89, 0x41, 0x78]);
    // 470: mov rax, SYS_EXC_UNSUBSCRIBE（rdi 仍是 page_fault cap）
    put_imm64(&mut code, 470, 0x48, 0xB8, SYS_EXC_UNSUBSCRIBE);
    // 480: int 0x80 → 未订阅（EACCES）
    put(&mut code, 480, &[0xCD, 0x80]);
    // 482: mov [rcx+128], rax（disp32：0x80 作 disp8 会被符号扩展成 -128）
    put(&mut code, 482, &[0x48, 0x89, 0x81, 0x80, 0x00, 0x00, 0x00]);
    // 489: mov rdi, FAST_ARG
    put_imm64(&mut code, 489, 0x48, 0xBF, FAST_ARG);
    // 499: mov rax, SYS_ECHO
    put_imm64(&mut code, 499, 0x48, 0xB8, SYS_ECHO);
    // 509: 0F 05  syscall（快路径；破坏 rcx/r11）
    put(&mut code, 509, &[0x0F, 0x05]);
    // 511: mov rcx, Y（syscall 破坏了 rcx，需重载回执基址）
    put_imm64(&mut code, 511, 0x48, 0xB9, USER_Y_VA);
    // 521: mov [rcx+56], rax
    put(&mut code, 521, &[0x48, 0x89, 0x41, 0x38]);
    // 525: mov rax, SYS_TASK_ID
    put_imm64(&mut code, 525, 0x48, 0xB8, SYS_TASK_ID);
    // 535: int 0x80 → 返回 1
    put(&mut code, 535, &[0xCD, 0x80]);
    // 537: mov [rcx+136], rax（disp32：0x88 作 disp8 会被符号扩展）
    put(&mut code, 537, &[0x48, 0x89, 0x81, 0x88, 0, 0, 0]);
    // 544: mov rax, SYS_YIELD
    put_imm64(&mut code, 544, 0x48, 0xB8, SYS_YIELD);
    // 554: int 0x80（切走；恢复时从 556 继续，rax=0）
    put(&mut code, 554, &[0xCD, 0x80]);
    // 556: mov [rcx+144], rax
    put(&mut code, 556, &[0x48, 0x89, 0x81, 0x90, 0, 0, 0]);
    // 563: mov rax, SYS_TASK_ID（恢复后仍应是 task 1）
    put_imm64(&mut code, 563, 0x48, 0xB8, SYS_TASK_ID);
    // 573: int 0x80 → 返回 1
    put(&mut code, 573, &[0xCD, 0x80]);
    // 575: mov [rcx+152], rax
    put(&mut code, 575, &[0x48, 0x89, 0x81, 0x98, 0, 0, 0]);
    // 582: mov rdi, EXC_SYSCALL_TRAP_CAP
    put_imm64(&mut code, 582, 0x48, 0xBF, EXC_SYSCALL_TRAP_CAP);
    // 592: mov rsi, EXC_KIND_SYSCALLTRAP
    put_imm64(&mut code, 592, 0x48, 0xBE, EXC_KIND_SYSCALLTRAP);
    // 602: mov rax, SYS_EXC_WAIT
    put_imm64(&mut code, 602, 0x48, 0xB8, SYS_EXC_WAIT);
    // 612: int 0x80（无通知 → 阻塞切走；被 raise 唤醒后返回 payload）
    put(&mut code, 612, &[0xCD, 0x80]);
    // 614: mov [rcx+160], rax（disp32）
    put(&mut code, 614, &[0x48, 0x89, 0x81, 0xa0, 0, 0, 0]);
    // 621: S2-D2 能力生命周期：派生 → 越权拒绝 → drop → 重复 drop → 槽位复用 →
    //      stale 拒绝 → 授权 → 结构性根保护。回执写 Y+168..Y+232（disp32，≥128 不可用 disp8）。
    put_imm64(&mut code, 621, 0x48, 0xBF, AUTHORIZE_CAP);
    put_imm64(&mut code, 631, 0x48, 0xBE, RIGHTS_DERIVE_BITS);
    put_imm64(&mut code, 641, 0x48, 0xB8, SYS_CAP_DERIVE);
    put(&mut code, 651, &[0xCD, 0x80]);
    put(&mut code, 653, &[0x48, 0x89, 0x81, 168, 0, 0, 0]);
    // 越权：请求 image cap 不拥有的 SegMapW。
    put_imm64(&mut code, 660, 0x48, 0xBF, AUTHORIZE_CAP);
    put_imm64(&mut code, 670, 0x48, 0xBE, RIGHTS_SEGMAPW_BITS);
    put_imm64(&mut code, 680, 0x48, 0xB8, SYS_CAP_DERIVE);
    put(&mut code, 690, &[0xCD, 0x80]);
    put(&mut code, 692, &[0x48, 0x89, 0x81, 176, 0, 0, 0]);
    // drop 子句柄（成功）→ 重复 drop（ENOENT）。
    put_imm64(&mut code, 699, 0x48, 0xBF, CAP_CHILD1_HANDLE);
    put_imm64(&mut code, 709, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 719, &[0xCD, 0x80]);
    put(&mut code, 721, &[0x48, 0x89, 0x81, 184, 0, 0, 0]);
    put_imm64(&mut code, 728, 0x48, 0xBF, CAP_CHILD1_HANDLE);
    put_imm64(&mut code, 738, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 748, &[0xCD, 0x80]);
    put(&mut code, 750, &[0x48, 0x89, 0x81, 192, 0, 0, 0]);
    // 槽位复用：再派生应拿到同槽更高 generation 的新句柄。
    put_imm64(&mut code, 757, 0x48, 0xBF, AUTHORIZE_CAP);
    put_imm64(&mut code, 767, 0x48, 0xBE, RIGHTS_DERIVE_BITS);
    put_imm64(&mut code, 777, 0x48, 0xB8, SYS_CAP_DERIVE);
    put(&mut code, 787, &[0xCD, 0x80]);
    put(&mut code, 789, &[0x48, 0x89, 0x81, 200, 0, 0, 0]);
    // 旧句柄授权应被拒绝（stale），新句柄应被授予。
    put_imm64(&mut code, 796, 0x48, 0xBF, CAP_CHILD1_HANDLE);
    put_imm64(&mut code, 806, 0x48, 0xBE, RIGHT_DERIVE);
    put_imm64(&mut code, 816, 0x48, 0xB8, SYS_AUTHORIZE);
    put(&mut code, 826, &[0xCD, 0x80]);
    put(&mut code, 828, &[0x48, 0x89, 0x81, 208, 0, 0, 0]);
    put_imm64(&mut code, 835, 0x48, 0xBF, CAP_CHILD2_HANDLE);
    put_imm64(&mut code, 845, 0x48, 0xBE, RIGHT_DERIVE);
    put_imm64(&mut code, 855, 0x48, 0xB8, SYS_AUTHORIZE);
    put(&mut code, 865, &[0xCD, 0x80]);
    put(&mut code, 867, &[0x48, 0x89, 0x81, 216, 0, 0, 0]);
    // drop 新句柄（清理）。
    put_imm64(&mut code, 874, 0x48, 0xBF, CAP_CHILD2_HANDLE);
    put_imm64(&mut code, 884, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 894, &[0xCD, 0x80]);
    put(&mut code, 896, &[0x48, 0x89, 0x81, 224, 0, 0, 0]);
    // 结构性根 Namespace cap 不可丢弃（§4.5 StructuralCap → EACCES）。
    put_imm64(&mut code, 903, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    put_imm64(&mut code, 913, 0x48, 0xB8, SYS_CAP_DROP);
    put(&mut code, 923, &[0xCD, 0x80]);
    put(&mut code, 925, &[0x48, 0x89, 0x81, 232, 0, 0, 0]);
    // 932: NOP（cap 生命周期块末字节落在 931，此处补 1 字节使下一块按计划从 933 开始；
    //        不留零字节，否则会被当作 `add [rax], al` 执行并触发 #PF 错位）。
    code[932] = 0x90;
    // 933: S2-D3 Segment 原语：create → map → 读零 → 写回 → unmap → destroy →
    //      销毁后 map 被拒（stale）→ 再次 create（复用回收帧）→ map → 读零（S2-X3）→ 清理。
    put_imm64(&mut code, 933, 0x48, 0xBF, 1); // rdi = page_count
    put_imm64(&mut code, 943, 0x48, 0xBE, MEMSEG_RW_BITS); // rsi = allowed_access
    put_imm64(&mut code, 953, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 963, &[0xCD, 0x80]);
    put(&mut code, 965, &[0x48, 0x89, 0x81, 240, 0, 0, 0]);
    // map(seg, task, 0, SEG_VA, 1, R|W)
    put_imm64(&mut code, 972, 0x48, 0xBF, MEMSEG_CAP1_HANDLE);
    put_imm64(&mut code, 982, 0x48, 0xBE, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 992, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1002, 0x49, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1012, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1022, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1032, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1042, &[0xCD, 0x80]);
    put(&mut code, 1044, &[0x48, 0x89, 0x81, 248, 0, 0, 0]);
    // 读新页首 qword：必须为 0（S2-X3 清零后才可见）。
    put_imm64(&mut code, 1051, 0x48, 0xBA, USER_SEG_VA);
    put(&mut code, 1061, &[0x48, 0x8B, 0x02]);
    put(&mut code, 1064, &[0x48, 0x89, 0x81, 0, 1, 0, 0]);
    // 写入模式并读回（证明真实可写 PTE）。
    put_imm64(&mut code, 1071, 0x48, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1081, 0x48, 0xB8, MEMSEG_PATTERN);
    put(&mut code, 1091, &[0x48, 0x89, 0x02]);
    put(&mut code, 1094, &[0x48, 0x8B, 0x02]);
    put(&mut code, 1097, &[0x48, 0x89, 0x81, 8, 1, 0, 0]);
    // unmap(task, SEG_VA, 1)
    put_imm64(&mut code, 1104, 0x48, 0xBF, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 1114, 0x48, 0xBE, USER_SEG_VA);
    put_imm64(&mut code, 1124, 0x48, 0xBA, 1);
    put_imm64(&mut code, 1134, 0x48, 0xB8, SYS_MEMSEG_UNMAP);
    put(&mut code, 1144, &[0xCD, 0x80]);
    put(&mut code, 1146, &[0x48, 0x89, 0x81, 16, 1, 0, 0]);
    // destroy(seg)
    put_imm64(&mut code, 1153, 0x48, 0xBF, MEMSEG_CAP1_HANDLE);
    put_imm64(&mut code, 1163, 0x48, 0xB8, SYS_MEMSEG_DESTROY);
    put(&mut code, 1173, &[0xCD, 0x80]);
    put(&mut code, 1175, &[0x48, 0x89, 0x81, 24, 1, 0, 0]);
    // 销毁后再 map：stale token 不可授权（EACCES）。
    put_imm64(&mut code, 1182, 0x48, 0xBF, MEMSEG_CAP1_HANDLE);
    put_imm64(&mut code, 1192, 0x48, 0xBE, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 1202, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1212, 0x49, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1222, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1232, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1242, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1252, &[0xCD, 0x80]);
    put(&mut code, 1254, &[0x48, 0x89, 0x81, 32, 1, 0, 0]);
    // 再次 create：应复用刚归还的帧（回收栈）。
    put_imm64(&mut code, 1261, 0x48, 0xBF, 1);
    put_imm64(&mut code, 1271, 0x48, 0xBE, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1281, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 1291, &[0xCD, 0x80]);
    put(&mut code, 1293, &[0x48, 0x89, 0x81, 40, 1, 0, 0]);
    // map2 + 读零（复用帧必须先清零才对用户可见 → S2-X3）。
    put_imm64(&mut code, 1300, 0x48, 0xBF, MEMSEG_CAP2_HANDLE);
    put_imm64(&mut code, 1310, 0x48, 0xBE, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 1320, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1330, 0x49, 0xBA, USER_SEG_VA);
    put_imm64(&mut code, 1340, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1350, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1360, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1370, &[0xCD, 0x80]);
    put(&mut code, 1372, &[0x48, 0x89, 0x81, 48, 1, 0, 0]);
    put_imm64(&mut code, 1379, 0x48, 0xBA, USER_SEG_VA);
    put(&mut code, 1389, &[0x48, 0x8B, 0x02]);
    put(&mut code, 1392, &[0x48, 0x89, 0x81, 56, 1, 0, 0]);
    // unmap2 + destroy2（清理）。
    put_imm64(&mut code, 1399, 0x48, 0xBF, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 1409, 0x48, 0xBE, USER_SEG_VA);
    put_imm64(&mut code, 1419, 0x48, 0xBA, 1);
    put_imm64(&mut code, 1429, 0x48, 0xB8, SYS_MEMSEG_UNMAP);
    put(&mut code, 1439, &[0xCD, 0x80]);
    put(&mut code, 1441, &[0x48, 0x89, 0x81, 64, 1, 0, 0]);
    put_imm64(&mut code, 1448, 0x48, 0xBF, MEMSEG_CAP2_HANDLE);
    put_imm64(&mut code, 1458, 0x48, 0xB8, SYS_MEMSEG_DESTROY);
    put(&mut code, 1468, &[0xCD, 0x80]);
    put(&mut code, 1470, &[0x48, 0x89, 0x81, 72, 1, 0, 0]);
    // 1477: 切片 S —— 共享 Segment：建段 → 映射进自己的根 → 写魔数 → 经授权消息把段 cap
    //       交给 task 2（此时 task 2 是 Ready，走**排队路径**）→ 让出 → 之后直投唤醒。
    put_imm64(&mut code, 1477, 0x48, 0xBF, 1); // rdi = page_count
    put_imm64(&mut code, 1487, 0x48, 0xBE, MEMSEG_RW_BITS); // rsi = allowed_access
    put_imm64(&mut code, 1497, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 1507, &[0xCD, 0x80]);
    put(&mut code, 1509, &[0x48, 0x89, 0x81, 0x50, 0x01, 0, 0]);
    // map(seg, task1_self_cap, 0, SHARED_VA, 1, R|W)
    put(&mut code, 1516, &[0x48, 0x8B, 0xB9, 0x50, 0x01, 0, 0]); // rdi = [Y+336]（新段 cap）
    put_imm64(&mut code, 1523, 0x48, 0xBE, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 1533, 0x48, 0xBA, 0);
    put_imm64(&mut code, 1543, 0x49, 0xBA, USER_SEG_SHARED_VA);
    put_imm64(&mut code, 1553, 0x49, 0xB8, 1);
    put_imm64(&mut code, 1563, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1573, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 1583, &[0xCD, 0x80]);
    put(&mut code, 1585, &[0x48, 0x89, 0x81, 0x58, 0x01, 0, 0]);
    // 写魔数（task 2 会在自己的根下读回同一物理帧）。
    put_imm64(&mut code, 1592, 0x48, 0xBA, USER_SEG_SHARED_VA);
    put_imm64(&mut code, 1602, 0x48, 0xB8, IPC_SHARED_MAGIC);
    put(&mut code, 1612, &[0x48, 0x89, 0x02]);
    // send(2, TASK2_SEND_CAP, MAGIC, cap_count=1, cap0=[Y+336])
    put_imm64(&mut code, 1615, 0x48, 0xBF, 2);
    put_imm64(&mut code, 1625, 0x48, 0xBE, TASK2_SEND_CAP);
    put_imm64(&mut code, 1635, 0x48, 0xBA, IPC_SHARED_MAGIC);
    put_imm64(&mut code, 1645, 0x49, 0xBA, 1);
    put(&mut code, 1655, &[0x4C, 0x8B, 0x81, 0x50, 0x01, 0, 0]); // r8 = [Y+336]
    put_imm64(&mut code, 1662, 0x49, 0xB9, 0);
    put_imm64(&mut code, 1672, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 1682, &[0xCD, 0x80]);
    put(&mut code, 1684, &[0x48, 0x89, 0x81, 0x60, 0x01, 0, 0]);
    // 让出给 task 2（它读走队列里的消息）。
    put_imm64(&mut code, 1691, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 1701, &[0xCD, 0x80]);
    // send(2, TASK2_SEND_CAP, DIRECT_PAYLOAD, cap_count=0) —— 此时 task 2 阻塞在 RECV，
    // 走**直投路径**并唤醒它。
    put_imm64(&mut code, 1703, 0x48, 0xBF, 2);
    put_imm64(&mut code, 1713, 0x48, 0xBE, TASK2_SEND_CAP);
    put_imm64(&mut code, 1723, 0x48, 0xBA, IPC_DIRECT_PAYLOAD);
    put_imm64(&mut code, 1733, 0x49, 0xBA, 0);
    put_imm64(&mut code, 1743, 0x49, 0xB8, 0);
    put_imm64(&mut code, 1753, 0x49, 0xB9, 0);
    put_imm64(&mut code, 1763, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 1773, &[0xCD, 0x80]);
    put(&mut code, 1775, &[0x48, 0x89, 0x81, 0x68, 0x01, 0, 0]);
    // 让出，等 task 2 记录直投回执后交回。
    put_imm64(&mut code, 1782, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 1792, &[0xCD, 0x80]);
    // 1794: int3
    code[1794] = 0xCC;
    code
}

/// 组装第二任务机器码：
/// `mov rcx, Y2; SYS_TASK_ID→[rcx]`（应 2）；
/// `mov rdi, IMAGE_CAP; mov rsi, SegMapR; SYS_AUTHORIZE→[rcx+8]`（应 `EACCES`，非持有者）；
/// `mov rdi, EXC_PAGE_FAULT_CAP; mov rsi, PageFault; SYS_EXC_SUBSCRIBE→[rcx+16]`（应 `EACCES`）；
/// `SYS_YIELD` → `SYS_TRAP`（唤醒阻塞的 task 1）→ `[rcx+24]` → `SYS_YIELD`；
/// 切片 S：`SYS_RECV`（排队消息 + 段 cap）→ 用**自己**的 `Task(2)` 管理权映射进自己的根 →
/// 读回魔数 → 两个负例（跨任务映射、未授权发送，均应 `EACCES`）→ `SYS_RECV` 阻塞 →
/// 被直投唤醒 → `SYS_YIELD`。
pub const fn task2_code() -> [u8; TASK2_CODE_LEN] {
    let mut code = [0u8; TASK2_CODE_LEN];
    // 0: mov rcx, Y2
    put_imm64(&mut code, 0, 0x48, 0xB9, USER2_Y_VA);
    // 10: mov rax, SYS_TASK_ID
    put_imm64(&mut code, 10, 0x48, 0xB8, SYS_TASK_ID);
    // 20: int 0x80 → 2
    put(&mut code, 20, &[0xCD, 0x80]);
    // 22: mov [rcx], rax
    put(&mut code, 22, &[0x48, 0x89, 0x01]);
    // 25: mov rdi, AUTHORIZE_CAP
    put_imm64(&mut code, 25, 0x48, 0xBF, AUTHORIZE_CAP);
    // 35: mov rsi, RIGHT_SEGMAPR
    put_imm64(&mut code, 35, 0x48, 0xBE, RIGHT_SEGMAPR);
    // 45: mov rax, SYS_AUTHORIZE
    put_imm64(&mut code, 45, 0x48, 0xB8, SYS_AUTHORIZE);
    // 55: int 0x80 → EACCES
    put(&mut code, 55, &[0xCD, 0x80]);
    // 57: mov [rcx+8], rax
    put(&mut code, 57, &[0x48, 0x89, 0x41, 0x08]);
    // 61: mov rdi, EXC_PAGE_FAULT_CAP
    put_imm64(&mut code, 61, 0x48, 0xBF, EXC_PAGE_FAULT_CAP);
    // 71: mov rsi, EXC_KIND_PAGEFAULT
    put_imm64(&mut code, 71, 0x48, 0xBE, EXC_KIND_PAGEFAULT);
    // 81: mov rax, SYS_EXC_SUBSCRIBE
    put_imm64(&mut code, 81, 0x48, 0xB8, SYS_EXC_SUBSCRIBE);
    // 91: int 0x80 → EACCES
    put(&mut code, 91, &[0xCD, 0x80]);
    // 93: mov [rcx+16], rax
    put(&mut code, 93, &[0x48, 0x89, 0x41, 0x10]);
    // 97: mov rax, SYS_YIELD（让回 task 1；待 task 1 阻塞在 WAIT 后从 109 恢复）
    put_imm64(&mut code, 97, 0x48, 0xB8, SYS_YIELD);
    // 107: int 0x80
    put(&mut code, 107, &[0xCD, 0x80]);
    // 109: mov rax, SYS_TRAP（内核 actor raise；唤醒阻塞的 task 1）
    put_imm64(&mut code, 109, 0x48, 0xB8, SYS_TRAP);
    // 119: int 0x80
    put(&mut code, 119, &[0xCD, 0x80]);
    // 121: mov [rcx+24], rax（应 delivered=1）
    put(&mut code, 121, &[0x48, 0x89, 0x41, 0x18]);
    // 125: mov rax, SYS_YIELD（切回已就绪的 task 1）
    put_imm64(&mut code, 125, 0x48, 0xB8, SYS_YIELD);
    // 135: int 0x80
    put(&mut code, 135, &[0xCD, 0x80]);
    // 137: 切片 S —— 接收 task 1 经授权消息送来的共享段 cap，映射进**自己的**根并读回魔数；
    //       随后是两个负例（跨任务映射、未授权发送），最后阻塞接收被直投唤醒。
    put_imm64(&mut code, 137, 0x48, 0xB9, USER2_Y_VA);
    put_imm64(&mut code, 147, 0x48, 0xBF, USER2_RECV_BUF);
    put_imm64(&mut code, 157, 0x48, 0xB8, SYS_RECV);
    put(&mut code, 167, &[0xCD, 0x80]);
    put(&mut code, 169, &[0x48, 0x89, 0x41, 40]); // [rcx+40] = payload
    put(&mut code, 173, &[0x48, 0x8B, 0x81, 0, 1, 0, 0]); // rax = [rcx+0x100] = cap0
    put(&mut code, 180, &[0x48, 0x89, 0x41, 48]);
    put(&mut code, 184, &[0x48, 0x8B, 0x81, 16, 1, 0, 0]); // rax = [rcx+0x110] = cap_count
    put(&mut code, 191, &[0x48, 0x89, 0x41, 88]);
    // map(cap0, TASK2_SELF_CAP, 0, USER2_SHARED_VA, 1, R|W)（caller == target == 2）
    put(&mut code, 195, &[0x48, 0x8B, 0xB9, 0, 1, 0, 0]); // rdi = [rcx+0x100]
    put_imm64(&mut code, 202, 0x48, 0xBE, TASK2_SELF_CAP);
    put_imm64(&mut code, 212, 0x48, 0xBA, 0);
    put_imm64(&mut code, 222, 0x49, 0xBA, USER2_SHARED_VA);
    put_imm64(&mut code, 232, 0x49, 0xB8, 1);
    put_imm64(&mut code, 242, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 252, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 262, &[0xCD, 0x80]);
    put(&mut code, 264, &[0x48, 0x89, 0x41, 64]);
    put_imm64(&mut code, 268, 0x48, 0xBA, USER2_SHARED_VA);
    put(&mut code, 278, &[0x48, 0x8B, 0x02]); // rax = [rdx]
    put(&mut code, 281, &[0x48, 0x89, 0x41, 56]); // [rcx+56] = 读回的魔数
    // 负例 1：用**不属于自己**的 Task(1) cap 做跨任务映射 → 必须 EACCES。
    put(&mut code, 285, &[0x48, 0x8B, 0xB9, 0, 1, 0, 0]);
    put_imm64(&mut code, 292, 0x48, 0xBE, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 302, 0x48, 0xBA, 0);
    put_imm64(&mut code, 312, 0x49, 0xBA, USER2_SHARED_VA + 0x1000);
    put_imm64(&mut code, 322, 0x49, 0xB8, 1);
    put_imm64(&mut code, 332, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 342, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 352, &[0xCD, 0x80]);
    put(&mut code, 354, &[0x48, 0x89, 0x41, 72]);
    // 负例 2：用不属于自己的 cap 给 task 1 发消息 → 必须 EACCES。
    put_imm64(&mut code, 358, 0x48, 0xBF, 1);
    put_imm64(&mut code, 368, 0x48, 0xBE, TASK_CAP_HANDLE.encode());
    put_imm64(&mut code, 378, 0x48, 0xBA, 0);
    put_imm64(&mut code, 388, 0x49, 0xBA, 0);
    put_imm64(&mut code, 398, 0x49, 0xB8, 0);
    put_imm64(&mut code, 408, 0x49, 0xB9, 0);
    put_imm64(&mut code, 418, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 428, &[0xCD, 0x80]);
    put(&mut code, 430, &[0x48, 0x89, 0x41, 32]);
    // 阻塞接收：空队列 → 置 BlockedRecv 并切走；被 task 1 直投唤醒后 rax = payload。
    put_imm64(&mut code, 434, 0x48, 0xBF, USER2_RECV_BUF2);
    put_imm64(&mut code, 444, 0x48, 0xB8, SYS_RECV);
    put(&mut code, 454, &[0xCD, 0x80]);
    put(&mut code, 456, &[0x48, 0x89, 0x41, 80]);
    // 交回 task 1（它已 Ready），让它执行 int3 收尾。
    put_imm64(&mut code, 460, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 470, &[0xCD, 0x80]);
    code
}

/// 组装抢占空转代码：把 `task_id` 写入 [`SPIN_DATA_VA`] 后无限循环。
///
/// `48 B8 <imm64>`（mov rax, DATA）、`48 C7 00 <imm32>`（mov qword [rax], id）、`EB FE`（jmp $）。
pub const fn spin_code(task_id: u64) -> [u8; SPIN_CODE_LEN] {
    let mut code = [0u8; SPIN_CODE_LEN];
    put_imm64(&mut code, 0, 0x48, 0xB8, SPIN_DATA_VA);
    code[10] = 0x48;
    code[11] = 0xC7;
    code[12] = 0x00;
    let imm = (task_id as u32).to_le_bytes();
    code[13] = imm[0];
    code[14] = imm[1];
    code[15] = imm[2];
    code[16] = imm[3];
    code[17] = 0xEB;
    code[18] = 0xFE;
    code
}

/// task 1 的初始用户上下文（进入 CPL3 的起点）。
pub fn task1_frame() -> SretFrame {
    let mut frame = SretFrame::ZERO;
    frame.rip = USER_CODE_VA;
    frame.rsp = USER_STACK_VA + FRAME_SIZE;
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    frame.rflags = 0x2; // bit1 固定；IF=0
    frame
}

/// task 2 的初始用户上下文。
pub fn task2_frame() -> SretFrame {
    let mut frame = SretFrame::ZERO;
    frame.rip = USER2_CODE_VA;
    frame.rsp = USER2_STACK_VA + FRAME_SIZE;
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    frame.rflags = 0x2;
    frame
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

/// 切片 R：把一个平台区域建为真实 Segment 对象，返回其根 Capability 与帧向量。
///
/// 只创建、不映射：映射必须等**全部**数据帧取完、内容写好之后再做，否则页表帧会插进数据帧
/// 之间，改变与切片 Q 实测一致的物理帧序列。
fn create_region(
    id: u32,
    access: Rights,
    offset: u64,
) -> Result<(Handle, SegmentFrames), VspaceError> {
    crate::bootstrap::platform_create_segment(id, 1, access, offset)
        .map_err(|_| VspaceError::NoFrame)
}

/// 切片 R：经 `memseg_map` 把平台区域的单页映射进目标任务根（授权中介；调用者为内核 actor）。
fn map_region(
    seg_cap: Handle,
    task_cap: Handle,
    target_base: u64,
    access: Rights,
    root_phys: u64,
    offset: u64,
) -> Result<(), VspaceError> {
    crate::bootstrap::memseg_map(
        crate::bootstrap::KERNEL_ACTOR,
        seg_cap,
        task_cap,
        0,
        target_base,
        1,
        access,
        root_phys,
        offset,
    )
    .map_err(|_| VspaceError::NoFrame)
}

/// 平台区域首帧的物理地址。
fn region_frame(frames: &SegmentFrames) -> Result<u64, VspaceError> {
    frames.frame_at(0).ok_or(VspaceError::NoFrame)
}

/// 切片 O：为一任务构造独立 PML4——克隆 bootloader 根（含全部内核高半区与低映射）后，
/// 清空两个用户槽，随后由调用方把自己的用户页映射进来。同 VA 在不同任务根下指向不同帧。
fn build_task_root(boot_root: u64, offset: u64) -> Result<u64, VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    // S1-X3：任务根页表帧是**保留页表帧**，不是任何 Segment 的数据帧。
    let task_root = frames::allocate_table_frame().map_err(|_| VspaceError::NoFrame)?;
    // clone_root 覆写全部 512 项，故无需先清零。
    vspace::clone_root(boot_root, task_root, map);
    vspace::zero_slot(task_root, USER_SLOT, map);
    vspace::zero_slot(task_root, USER2_SLOT, map);
    Ok(task_root)
}

/// 构造用户任务：分配并映射代码/栈/X/Y 四页，填代码与 X 的模式。
///
/// 必须在 [`crate::cow::prepare`] **之后**调用（帧与页表页都取自 COW 上下文的唯一分配器，
/// 以避免多份 `FreeFrameAllocator` 副本各自推进 `next` 而重复发放同一物理帧）。
///
/// 切片 O：用户页映射进**本任务独立根**（克隆 bootloader 根 + 隔离用户槽），并把 root 与
/// 内核栈登记到调度器，使每次 [`crate::sched::resume`] 写 CR3 与 `TSS.rsp0`。
pub fn prepare(root_phys: u64, offset: u64) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    let task_root = build_task_root(root_phys, offset)?;
    for va in [USER_CODE_VA, USER_STACK_VA, USER_X_VA, USER_Y_VA] {
        if !vspace::slot_free(task_root, va, map) {
            return Err(VspaceError::SlotOccupied);
        }
    }

    // S1-X3 / 切片 R：四个区域各自建为真实 Segment 对象（creator = 内核平台 actor，根
    // Capability 铸在预留槽位）。取帧顺序与数量与切片 Q **逐字节一致**：四个数据帧全部取完、
    // 内容写好之后，才由 `memseg_map` 取页表帧。
    let code_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapX]);
    let stack_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
    let x_access = Rights::from_rights(&[Right::SegMapR]);
    let y_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
    let (code_cap, code_frames) = create_region(frames::SEG_TASK1_CODE, code_access, offset)?;
    let (stack_cap, stack_frames) = create_region(frames::SEG_TASK1_STACK, stack_access, offset)?;
    let (x_cap, x_frames) = create_region(frames::SEG_TASK1_X, x_access, offset)?;
    let (y_cap, y_frames) = create_region(frames::SEG_TASK1_Y, y_access, offset)?;
    let code = region_frame(&code_frames)?;
    let stack = region_frame(&stack_frames)?;
    let x = region_frame(&x_frames)?;
    let y = region_frame(&y_frames)?;

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

    // 经授权中介映射进 task 1 根；`memseg_map` 逐页 `invlpg`，故不再显式刷新 TLB。
    // X 的访问集上不含 `SegMapW`，装入的就是只读页——用户态写保护 #PF 由此产生。
    let task_cap = PLATFORM_TASK1_CAP_HANDLE;
    map_region(
        code_cap,
        task_cap,
        USER_CODE_VA,
        code_access,
        task_root,
        offset,
    )?;
    map_region(
        stack_cap,
        task_cap,
        USER_STACK_VA,
        stack_access,
        task_root,
        offset,
    )?;
    map_region(x_cap, task_cap, USER_X_VA, x_access, task_root, offset)?;
    map_region(y_cap, task_cap, USER_Y_VA, y_access, task_root, offset)?;

    sched::set_root(INIT_TASK_ID, task_root);
    sched::set_kstack_top(INIT_TASK_ID, kstack_top(core::ptr::addr_of!(KSTACK_TASK1)));
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

/// 切片 M：构造第二任务（code/stack/Y 三页，无 COW X）。
///
/// 与 [`prepare`] 一样必须在 `cow::prepare` 之后，复用同一唯一分配器。切片 O：映射进
/// **task 2 独立根**，并登记 root/内核栈；task 2 的 Y 页物理帧另行持久化，供 task 1
/// 根下不可见 Y2 VA 时按物理读回执。
pub fn prepare_task2(root_phys: u64, offset: u64) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    let task_root = build_task_root(root_phys, offset)?;
    for va in [USER2_CODE_VA, USER2_STACK_VA, USER2_Y_VA] {
        if !vspace::slot_free(task_root, va, map) {
            return Err(VspaceError::SlotOccupied);
        }
    }

    // 切片 R：三个区域建为真实 Segment 对象，经 `memseg_map` 授权中介映射进 task 2 根
    // （task cap 指向 Task(2)，由内核平台 actor 持有）。帧顺序与切片 Q 一致。
    let code_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapX]);
    let stack_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
    let y_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
    let (code_cap, code_frames) = create_region(frames::SEG_TASK2_CODE, code_access, offset)?;
    let (stack_cap, stack_frames) = create_region(frames::SEG_TASK2_STACK, stack_access, offset)?;
    let (y_cap, y_frames) = create_region(frames::SEG_TASK2_Y, y_access, offset)?;
    let code = region_frame(&code_frames)?;
    let stack = region_frame(&stack_frames)?;
    let y = region_frame(&y_frames)?;

    let code_bytes = task2_code();
    for (index, &byte) in code_bytes.iter().enumerate() {
        unsafe { core::ptr::write_volatile((map(code) + index as u64) as *mut u8, byte) };
    }
    for index in 0..(FRAME_SIZE / 8) {
        write_qword(map(stack) + index * 8, 0);
        write_qword(map(y) + index * 8, 0);
    }

    let task_cap = PLATFORM_TASK2_CAP_HANDLE;
    map_region(
        code_cap,
        task_cap,
        USER2_CODE_VA,
        code_access,
        task_root,
        offset,
    )?;
    map_region(
        stack_cap,
        task_cap,
        USER2_STACK_VA,
        stack_access,
        task_root,
        offset,
    )?;
    map_region(y_cap, task_cap, USER2_Y_VA, y_access, task_root, offset)?;

    sched::set_root(TASK2_ID, task_root);
    sched::set_kstack_top(TASK2_ID, kstack_top(core::ptr::addr_of!(KSTACK_TASK2)));
    TASK2_Y_PHYS.store(y, Ordering::Relaxed);
    Ok(())
}

/// 切片 O：为两任务准备抢占空转页（同 VA、各自根下不同帧）。
///
/// 必须在 [`prepare`] 与 [`prepare_task2`] 之后调用（两任务根已建立）。把带任务 id 的空转
/// 代码与数据页分别映射进对应任务根；相同 VA [`SPIN_CODE_VA`]/[`SPIN_DATA_VA`] 指向不同
/// 物理帧，是地址空间隔离的可证现场。
pub fn prepare_spin(offset: u64) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    let root1 = sched::root(INIT_TASK_ID).ok_or(VspaceError::BadRoot)?;
    let root2 = sched::root(TASK2_ID).ok_or(VspaceError::BadRoot)?;

    let code_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapX]);
    let data_access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
    for (task_id, root, code_seg, data_seg, task_cap) in [
        (
            INIT_TASK_ID,
            root1,
            frames::SEG_SPIN1_CODE,
            frames::SEG_SPIN1_DATA,
            PLATFORM_TASK1_CAP_HANDLE,
        ),
        (
            TASK2_ID,
            root2,
            frames::SEG_SPIN2_CODE,
            frames::SEG_SPIN2_DATA,
            PLATFORM_TASK2_CAP_HANDLE,
        ),
    ] {
        let (code_cap, code_frames) = create_region(code_seg, code_access, offset)?;
        let (data_cap, data_frames) = create_region(data_seg, data_access, offset)?;
        let code = region_frame(&code_frames)?;
        let data = region_frame(&data_frames)?;

        let bytes = spin_code(task_id as u64);
        for (index, &byte) in bytes.iter().enumerate() {
            unsafe { core::ptr::write_volatile((map(code) + index as u64) as *mut u8, byte) };
        }
        for index in 0..(FRAME_SIZE / 8) {
            write_qword(map(data) + index * 8, 0);
        }

        map_region(code_cap, task_cap, SPIN_CODE_VA, code_access, root, offset)?;
        map_region(data_cap, task_cap, SPIN_DATA_VA, data_access, root, offset)?;
    }
    Ok(())
}

/// 切片 O：两任务的抢占空转初始帧（同代码 VA，各自用户栈，RFLAGS.IF=1 以接收时钟）。
pub fn spin_frames() -> (SretFrame, SretFrame) {
    let mut f1 = SretFrame::ZERO;
    f1.rip = SPIN_CODE_VA;
    f1.rsp = USER_STACK_VA + FRAME_SIZE;
    f1.cs = USER_CS as u64;
    f1.ss = USER_DS as u64;
    f1.rflags = 0x2 | (1 << 9); // bit1 固定 + IF=1

    let mut f2 = SretFrame::ZERO;
    f2.rip = SPIN_CODE_VA;
    f2.rsp = USER2_STACK_VA + FRAME_SIZE;
    f2.cs = USER_CS as u64;
    f2.ss = USER_DS as u64;
    f2.rflags = 0x2 | (1 << 9);
    (f1, f2)
}

/// 进入 CPL3：初始化调度器，装入两个任务的初始上下文，恢复 task 1（不返回）。
///
/// task 1 最终以 `int3` 陷入内核收尾；task 2 以 `SYS_YIELD` 结束（不再被调度）。
pub fn enter() -> ! {
    crate::sched::init();
    crate::sched::set_frame(crate::bootstrap::INIT_TASK_ID, task1_frame());
    crate::sched::set_frame(TASK2_ID, task2_frame());
    crate::sched::resume(crate::bootstrap::INIT_TASK_ID)
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
        assert_eq!(USER_CODE_LEN, 1795);
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
        // 异常订阅六次（SPEC-050）。
        assert_eq!(&code[320..322], &[0x48, 0xBF]);
        assert_eq!(&code[322..330], &EXC_PAGE_FAULT_CAP.to_le_bytes());
        assert_eq!(&code[330..332], &[0x48, 0xBE]);
        assert_eq!(&code[332..340], &EXC_KIND_PAGEFAULT.to_le_bytes());
        assert_eq!(&code[340..342], &[0x48, 0xB8]);
        assert_eq!(&code[342..350], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[350..352], &[0xCD, 0x80]);
        assert_eq!(&code[352..356], &[0x48, 0x89, 0x41, 0x58]); // [rcx+88]
        assert_eq!(&code[356..358], &[0x48, 0xBE]);
        assert_eq!(&code[358..366], &EXC_KIND_PAGEFAULT.to_le_bytes());
        assert_eq!(&code[366..368], &[0x48, 0xB8]);
        assert_eq!(&code[368..376], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[376..378], &[0xCD, 0x80]);
        assert_eq!(&code[378..382], &[0x48, 0x89, 0x41, 0x60]); // [rcx+96]
        assert_eq!(&code[382..384], &[0x48, 0xBE]);
        assert_eq!(&code[384..392], &EXC_KIND_SYSCALLTRAP.to_le_bytes());
        assert_eq!(&code[392..394], &[0x48, 0xB8]);
        assert_eq!(&code[394..402], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[402..404], &[0xCD, 0x80]);
        assert_eq!(&code[404..408], &[0x48, 0x89, 0x41, 0x68]); // [rcx+104]
        assert_eq!(&code[408..410], &[0x48, 0xBF]);
        assert_eq!(&code[410..418], &EXC_SYSCALL_TRAP_CAP.to_le_bytes());
        assert_eq!(&code[418..420], &[0x48, 0xBE]);
        assert_eq!(&code[420..428], &EXC_KIND_SYSCALLTRAP.to_le_bytes());
        assert_eq!(&code[428..430], &[0x48, 0xB8]);
        assert_eq!(&code[430..438], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[438..440], &[0xCD, 0x80]);
        assert_eq!(&code[440..444], &[0x48, 0x89, 0x41, 0x70]); // [rcx+112]
        assert_eq!(&code[444..446], &[0x48, 0xBF]);
        assert_eq!(&code[446..454], &EXC_PAGE_FAULT_CAP.to_le_bytes());
        assert_eq!(&code[454..456], &[0x48, 0xB8]);
        assert_eq!(&code[456..464], &SYS_EXC_UNSUBSCRIBE.to_le_bytes());
        assert_eq!(&code[464..466], &[0xCD, 0x80]);
        assert_eq!(&code[466..470], &[0x48, 0x89, 0x41, 0x78]); // [rcx+120]
        assert_eq!(&code[470..472], &[0x48, 0xB8]);
        assert_eq!(&code[472..480], &SYS_EXC_UNSUBSCRIBE.to_le_bytes());
        assert_eq!(&code[480..482], &[0xCD, 0x80]);
        assert_eq!(&code[482..489], &[0x48, 0x89, 0x81, 0x80, 0, 0, 0]); // [rcx+128] disp32
        // 快路径：syscall 指令调用 ECHO，再重载 rcx 写 Y+56。
        assert_eq!(&code[489..491], &[0x48, 0xBF]);
        assert_eq!(&code[491..499], &FAST_ARG.to_le_bytes());
        assert_eq!(&code[499..501], &[0x48, 0xB8]);
        assert_eq!(&code[501..509], &SYS_ECHO.to_le_bytes());
        assert_eq!(&code[509..511], &[0x0F, 0x05]); // syscall
        assert_eq!(&code[511..513], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[513..521], &USER_Y_VA.to_le_bytes());
        assert_eq!(&code[521..525], &[0x48, 0x89, 0x41, 0x38]); // mov [rcx+56], rax
        // 阶段二：task id → yield → 恢复后再取 task id → int3。
        assert_eq!(&code[525..527], &[0x48, 0xB8]);
        assert_eq!(&code[527..535], &SYS_TASK_ID.to_le_bytes());
        assert_eq!(&code[535..537], &[0xCD, 0x80]);
        assert_eq!(&code[537..544], &[0x48, 0x89, 0x81, 0x88, 0, 0, 0]); // [rcx+136] disp32
        assert_eq!(&code[544..546], &[0x48, 0xB8]);
        assert_eq!(&code[546..554], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[554..556], &[0xCD, 0x80]);
        assert_eq!(&code[556..563], &[0x48, 0x89, 0x81, 0x90, 0, 0, 0]); // [rcx+144] disp32
        assert_eq!(&code[563..565], &[0x48, 0xB8]);
        assert_eq!(&code[565..573], &SYS_TASK_ID.to_le_bytes());
        assert_eq!(&code[573..575], &[0xCD, 0x80]);
        assert_eq!(&code[575..582], &[0x48, 0x89, 0x81, 0x98, 0, 0, 0]); // [rcx+152] disp32
        // 阶段三：阻塞等待 task 2 触发的通知。
        assert_eq!(&code[582..584], &[0x48, 0xBF]);
        assert_eq!(&code[584..592], &EXC_SYSCALL_TRAP_CAP.to_le_bytes());
        assert_eq!(&code[592..594], &[0x48, 0xBE]);
        assert_eq!(&code[594..602], &EXC_KIND_SYSCALLTRAP.to_le_bytes());
        assert_eq!(&code[602..604], &[0x48, 0xB8]);
        assert_eq!(&code[604..612], &SYS_EXC_WAIT.to_le_bytes());
        assert_eq!(&code[612..614], &[0xCD, 0x80]);
        assert_eq!(&code[614..621], &[0x48, 0x89, 0x81, 0xa0, 0, 0, 0]); // [rcx+160] disp32
        // S2-D2 能力生命周期（回执 Y+168..Y+232，全部 disp32）。
        assert_eq!(&code[621..623], &[0x48, 0xBF]);
        assert_eq!(&code[623..631], &AUTHORIZE_CAP.to_le_bytes());
        assert_eq!(&code[631..633], &[0x48, 0xBE]);
        assert_eq!(&code[633..641], &RIGHTS_DERIVE_BITS.to_le_bytes());
        assert_eq!(&code[641..643], &[0x48, 0xB8]);
        assert_eq!(&code[643..651], &SYS_CAP_DERIVE.to_le_bytes());
        assert_eq!(&code[651..653], &[0xCD, 0x80]);
        assert_eq!(&code[653..660], &[0x48, 0x89, 0x81, 168, 0, 0, 0]);
        assert_eq!(&code[670..672], &[0x48, 0xBE]);
        assert_eq!(&code[672..680], &RIGHTS_SEGMAPW_BITS.to_le_bytes());
        assert_eq!(&code[692..699], &[0x48, 0x89, 0x81, 176, 0, 0, 0]);
        assert_eq!(&code[699..701], &[0x48, 0xBF]);
        assert_eq!(&code[701..709], &CAP_CHILD1_HANDLE.to_le_bytes());
        assert_eq!(&code[709..711], &[0x48, 0xB8]);
        assert_eq!(&code[711..719], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[721..728], &[0x48, 0x89, 0x81, 184, 0, 0, 0]);
        assert_eq!(&code[738..740], &[0x48, 0xB8]);
        assert_eq!(&code[740..748], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[750..757], &[0x48, 0x89, 0x81, 192, 0, 0, 0]);
        assert_eq!(&code[777..779], &[0x48, 0xB8]);
        assert_eq!(&code[779..787], &SYS_CAP_DERIVE.to_le_bytes());
        assert_eq!(&code[789..796], &[0x48, 0x89, 0x81, 200, 0, 0, 0]);
        assert_eq!(&code[796..798], &[0x48, 0xBF]);
        assert_eq!(&code[798..806], &CAP_CHILD1_HANDLE.to_le_bytes());
        assert_eq!(&code[806..808], &[0x48, 0xBE]);
        assert_eq!(&code[808..816], &RIGHT_DERIVE.to_le_bytes());
        assert_eq!(&code[816..818], &[0x48, 0xB8]);
        assert_eq!(&code[818..826], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[828..835], &[0x48, 0x89, 0x81, 208, 0, 0, 0]);
        assert_eq!(&code[835..837], &[0x48, 0xBF]);
        assert_eq!(&code[837..845], &CAP_CHILD2_HANDLE.to_le_bytes());
        assert_eq!(&code[867..874], &[0x48, 0x89, 0x81, 216, 0, 0, 0]);
        assert_eq!(&code[874..876], &[0x48, 0xBF]);
        assert_eq!(&code[876..884], &CAP_CHILD2_HANDLE.to_le_bytes());
        assert_eq!(&code[884..886], &[0x48, 0xB8]);
        assert_eq!(&code[886..894], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[896..903], &[0x48, 0x89, 0x81, 224, 0, 0, 0]);
        assert_eq!(&code[903..905], &[0x48, 0xBF]);
        assert_eq!(
            &code[905..913],
            &NAMESPACE_CAP_HANDLE.encode().to_le_bytes()
        );
        assert_eq!(&code[913..915], &[0x48, 0xB8]);
        assert_eq!(&code[915..923], &SYS_CAP_DROP.to_le_bytes());
        assert_eq!(&code[925..932], &[0x48, 0x89, 0x81, 232, 0, 0, 0]);
        assert_eq!(code[932], 0x90); // NOP 对齐
        // S2-D3 Segment 原语（回执 Y+240..Y+328，全部 disp32）。
        assert_eq!(&code[933..943], &[0x48, 0xBF, 1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&code[953..955], &[0x48, 0xB8]);
        assert_eq!(&code[955..963], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[963..965], &[0xCD, 0x80]);
        assert_eq!(&code[965..972], &[0x48, 0x89, 0x81, 240, 0, 0, 0]);
        assert_eq!(&code[972..974], &[0x48, 0xBF]);
        assert_eq!(&code[974..982], &MEMSEG_CAP1_HANDLE.to_le_bytes());
        assert_eq!(&code[1032..1034], &[0x48, 0xB8]);
        assert_eq!(&code[1034..1042], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[1051..1053], &[0x48, 0xBA]);
        assert_eq!(&code[1053..1061], &USER_SEG_VA.to_le_bytes());
        assert_eq!(&code[1061..1064], &[0x48, 0x8B, 0x02]); // mov rax, [rdx]
        assert_eq!(&code[1064..1071], &[0x48, 0x89, 0x81, 0, 1, 0, 0]); // [rcx+256] disp32
        assert_eq!(&code[1091..1094], &[0x48, 0x89, 0x02]); // mov [rdx], rax
        assert_eq!(&code[1134..1136], &[0x48, 0xB8]);
        assert_eq!(&code[1136..1144], &SYS_MEMSEG_UNMAP.to_le_bytes());
        assert_eq!(&code[1163..1165], &[0x48, 0xB8]);
        assert_eq!(&code[1165..1173], &SYS_MEMSEG_DESTROY.to_le_bytes());
        assert_eq!(&code[1242..1244], &[0x48, 0xB8]);
        assert_eq!(&code[1244..1252], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[1254..1261], &[0x48, 0x89, 0x81, 32, 1, 0, 0]); // [rcx+288] disp32
        assert_eq!(&code[1281..1283], &[0x48, 0xB8]);
        assert_eq!(&code[1283..1291], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[1293..1300], &[0x48, 0x89, 0x81, 40, 1, 0, 0]); // [rcx+296] disp32
        assert_eq!(&code[1300..1302], &[0x48, 0xBF]);
        assert_eq!(&code[1302..1310], &MEMSEG_CAP2_HANDLE.to_le_bytes());
        assert_eq!(&code[1372..1379], &[0x48, 0x89, 0x81, 48, 1, 0, 0]); // [rcx+304] disp32
        assert_eq!(&code[1392..1399], &[0x48, 0x89, 0x81, 56, 1, 0, 0]); // [rcx+312] disp32
        assert_eq!(&code[1458..1460], &[0x48, 0xB8]);
        assert_eq!(&code[1460..1468], &SYS_MEMSEG_DESTROY.to_le_bytes());
        assert_eq!(&code[1470..1477], &[0x48, 0x89, 0x81, 72, 1, 0, 0]); // [rcx+328] disp32
        // 切片 S：共享 Segment —— create → map → 写魔数 → 排队路径送 cap → yield →
        //           直投路径送无 cap 消息 → yield → int3。
        assert_eq!(&code[1477..1479], &[0x48, 0xBF]);
        assert_eq!(&code[1479..1487], &1u64.to_le_bytes()); // page_count
        assert_eq!(&code[1497..1499], &[0x48, 0xB8]);
        assert_eq!(&code[1499..1507], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[1507..1509], &[0xCD, 0x80]);
        assert_eq!(&code[1509..1516], &[0x48, 0x89, 0x81, 0x50, 0x01, 0, 0]); // [Y+336]
        assert_eq!(&code[1516..1523], &[0x48, 0x8B, 0xB9, 0x50, 0x01, 0, 0]); // rdi = [Y+336]
        assert_eq!(&code[1525..1533], &TASK_CAP_HANDLE.encode().to_le_bytes());
        assert_eq!(&code[1545..1553], &USER_SEG_SHARED_VA.to_le_bytes());
        assert_eq!(&code[1583..1585], &[0xCD, 0x80]);
        assert_eq!(&code[1585..1592], &[0x48, 0x89, 0x81, 0x58, 0x01, 0, 0]); // [Y+344]
        assert_eq!(&code[1594..1602], &USER_SEG_SHARED_VA.to_le_bytes());
        assert_eq!(&code[1604..1612], &IPC_SHARED_MAGIC.to_le_bytes());
        assert_eq!(&code[1612..1615], &[0x48, 0x89, 0x02]); // mov [rdx], rax
        assert_eq!(&code[1617..1625], &2u64.to_le_bytes()); // receiver
        assert_eq!(&code[1627..1635], &TASK2_SEND_CAP.to_le_bytes());
        assert_eq!(&code[1637..1645], &IPC_SHARED_MAGIC.to_le_bytes()); // payload
        assert_eq!(&code[1655..1662], &[0x4C, 0x8B, 0x81, 0x50, 0x01, 0, 0]); // r8 = [Y+336]
        assert_eq!(&code[1674..1682], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[1684..1691], &[0x48, 0x89, 0x81, 0x60, 0x01, 0, 0]); // [Y+352]
        assert_eq!(&code[1693..1701], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[1701..1703], &[0xCD, 0x80]);
        assert_eq!(&code[1725..1733], &IPC_DIRECT_PAYLOAD.to_le_bytes());
        assert_eq!(&code[1765..1773], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[1775..1782], &[0x48, 0x89, 0x81, 0x68, 0x01, 0, 0]); // [Y+360]
        assert_eq!(&code[1784..1792], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[1792..1794], &[0xCD, 0x80]);
        assert_eq!(code[1794], 0xCC); // int3
    }

    #[test]
    fn task2_code_bytes_are_fixed() {
        let code = task2_code();
        assert_eq!(TASK2_CODE_LEN, 472);
        assert_eq!(&code[0..2], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[2..10], &USER2_Y_VA.to_le_bytes());
        assert_eq!(&code[10..12], &[0x48, 0xB8]);
        assert_eq!(&code[12..20], &SYS_TASK_ID.to_le_bytes());
        assert_eq!(&code[20..22], &[0xCD, 0x80]);
        assert_eq!(&code[22..25], &[0x48, 0x89, 0x01]); // [rcx]
        assert_eq!(&code[25..27], &[0x48, 0xBF]);
        assert_eq!(&code[27..35], &AUTHORIZE_CAP.to_le_bytes());
        assert_eq!(&code[45..47], &[0x48, 0xB8]);
        assert_eq!(&code[47..55], &SYS_AUTHORIZE.to_le_bytes());
        assert_eq!(&code[55..57], &[0xCD, 0x80]);
        assert_eq!(&code[57..61], &[0x48, 0x89, 0x41, 0x08]); // [rcx+8]
        assert_eq!(&code[61..63], &[0x48, 0xBF]);
        assert_eq!(&code[63..71], &EXC_PAGE_FAULT_CAP.to_le_bytes());
        assert_eq!(&code[81..83], &[0x48, 0xB8]);
        assert_eq!(&code[83..91], &SYS_EXC_SUBSCRIBE.to_le_bytes());
        assert_eq!(&code[91..93], &[0xCD, 0x80]);
        assert_eq!(&code[93..97], &[0x48, 0x89, 0x41, 0x10]); // [rcx+16]
        assert_eq!(&code[97..99], &[0x48, 0xB8]);
        assert_eq!(&code[99..107], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[107..109], &[0xCD, 0x80]);
        assert_eq!(&code[109..111], &[0x48, 0xB8]);
        assert_eq!(&code[111..119], &SYS_TRAP.to_le_bytes());
        assert_eq!(&code[119..121], &[0xCD, 0x80]);
        assert_eq!(&code[121..125], &[0x48, 0x89, 0x41, 0x18]); // [rcx+24]
        assert_eq!(&code[125..127], &[0x48, 0xB8]);
        assert_eq!(&code[127..135], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[135..137], &[0xCD, 0x80]);
        // 切片 S：收消息 → 自映射共享段 → 读魔数 → 两个负例 → 阻塞接收 → 交回。
        assert_eq!(&code[137..139], &[0x48, 0xB9]);
        assert_eq!(&code[139..147], &USER2_Y_VA.to_le_bytes());
        assert_eq!(&code[149..157], &USER2_RECV_BUF.to_le_bytes());
        assert_eq!(&code[159..167], &SYS_RECV.to_le_bytes());
        assert_eq!(&code[167..169], &[0xCD, 0x80]);
        assert_eq!(&code[169..173], &[0x48, 0x89, 0x41, 40]); // [rcx+40] = payload
        assert_eq!(&code[173..180], &[0x48, 0x8B, 0x81, 0x00, 0x01, 0, 0]); // rax = cap0
        assert_eq!(&code[180..184], &[0x48, 0x89, 0x41, 48]);
        assert_eq!(&code[184..191], &[0x48, 0x8B, 0x81, 0x10, 0x01, 0, 0]); // rax = cap_count
        assert_eq!(&code[191..195], &[0x48, 0x89, 0x41, 88]);
        assert_eq!(&code[195..202], &[0x48, 0x8B, 0xB9, 0x00, 0x01, 0, 0]); // rdi = cap0
        assert_eq!(&code[204..212], &TASK2_SELF_CAP.to_le_bytes());
        assert_eq!(&code[224..232], &USER2_SHARED_VA.to_le_bytes());
        assert_eq!(&code[254..262], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[262..264], &[0xCD, 0x80]);
        assert_eq!(&code[264..268], &[0x48, 0x89, 0x41, 64]);
        assert_eq!(&code[270..278], &USER2_SHARED_VA.to_le_bytes());
        assert_eq!(&code[278..281], &[0x48, 0x8B, 0x02]); // rax = [rdx]
        assert_eq!(&code[281..285], &[0x48, 0x89, 0x41, 56]); // [rcx+56] = 魔数
        assert_eq!(&code[294..302], &TASK_CAP_HANDLE.encode().to_le_bytes());
        assert_eq!(&code[314..322], &(USER2_SHARED_VA + 0x1000).to_le_bytes());
        assert_eq!(&code[344..352], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[352..354], &[0xCD, 0x80]);
        assert_eq!(&code[354..358], &[0x48, 0x89, 0x41, 72]);
        assert_eq!(&code[360..368], &1u64.to_le_bytes()); // receiver = 1
        assert_eq!(&code[370..378], &TASK_CAP_HANDLE.encode().to_le_bytes());
        assert_eq!(&code[420..428], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[430..434], &[0x48, 0x89, 0x41, 32]);
        assert_eq!(&code[436..444], &USER2_RECV_BUF2.to_le_bytes());
        assert_eq!(&code[446..454], &SYS_RECV.to_le_bytes());
        assert_eq!(&code[454..456], &[0xCD, 0x80]);
        assert_eq!(&code[456..460], &[0x48, 0x89, 0x41, 80]);
        assert_eq!(&code[462..470], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[470..472], &[0xCD, 0x80]);
    }

    #[test]
    fn user_vas_are_distinct_and_in_expected_slots() {
        let task1 = [USER_CODE_VA, USER_STACK_VA, USER_X_VA, USER_Y_VA];
        let task2 = [USER2_CODE_VA, USER2_STACK_VA, USER2_Y_VA];
        for (i, a) in task1.iter().enumerate() {
            assert_eq!(
                (a >> 39) & 0x1ff,
                20,
                "task1 va {i} must be in PML4 slot 20"
            );
        }
        for (i, a) in task2.iter().enumerate() {
            assert_eq!(
                (a >> 39) & 0x1ff,
                21,
                "task2 va {i} must be in PML4 slot 21"
            );
        }
        let vas = [task1.as_slice(), task2.as_slice()].concat();
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
