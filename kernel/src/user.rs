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
    IMAGE_CAP_HANDLE, INIT_TASK_ID, NAMESPACE_CAP_HANDLE, PLATFORM_TASK1_CAP_HANDLE,
    PLATFORM_TASK2_CAP_HANDLE, SegmentFrames, TASK_CAP_HANDLE,
};
use crate::exc::SretFrame;
use crate::frames;
use crate::gdt::{USER_CS, USER_DS};
use crate::handle::Handle;
use crate::mem::{FRAME_SIZE, phys_to_virt};
use crate::rights::{Right, Rights};
use crate::sched;
use crate::syscall::{
    ACCESS_R_BITS, AUTHORIZE_CAP, BAD_PTR, CAP_CHILD1_HANDLE, CAP_CHILD2_HANDLE, COPY_LEN,
    COPY_PTR, EXC_KIND_PAGEFAULT, EXC_KIND_SYSCALLTRAP, EXC_PAGE_FAULT_CAP, EXC_SYSCALL_TRAP_CAP,
    FAST_ARG, IPC_DIRECT_PAYLOAD, IPC_SHARED_MAGIC, MEMSEG_CAP1_HANDLE, MEMSEG_CAP2_HANDLE,
    MEMSEG_PATTERN, MEMSEG_RW_BITS, NS_BIND_RIGHTS_BITS, NS_PATH_DATA_OFFSET, NS_PATH_DATA_VA,
    NS_PATH_ILLEGAL_OFF, NS_PATH_LIB_OFF, NS_PATH_MNT_OFF, NS_PATH_MNTSUB_OFF, NS_PATH_SUB_OFF,
    NS_PATH_X_OFF, NS_PATH_Y_OFF, NS_RECEIPT_BAD_PATH, NS_RECEIPT_BIND, NS_RECEIPT_BIND_OVERLAP,
    NS_RECEIPT_BIND_SUB, NS_RECEIPT_COPY_B, NS_RECEIPT_COPY_C, NS_RECEIPT_CREATE_A,
    NS_RECEIPT_CREATE_B, NS_RECEIPT_CREATE_C, NS_RECEIPT_DERIVE1, NS_RECEIPT_DERIVE2,
    NS_RECEIPT_DESTROY, NS_RECEIPT_MOUNT_A_B, NS_RECEIPT_MOUNT_CYCLE, NS_RECEIPT_MOUNT_INIT_C,
    NS_RECEIPT_RESOLVE_LIB, NS_RECEIPT_RESOLVE_SUB, NS_RECEIPT_UNBIND, NS_RECEIPT_UNMOUNT,
    NS_ROOT_RIGHTS_BITS, RIGHT_DERIVE, RIGHT_SEGMAPR, RIGHT_SEGMAPW, RIGHTS_DERIVE_BITS,
    RIGHTS_SEGMAPW_BITS, SUM_ARGS, SYS_AUTHORIZE, SYS_BAD, SYS_CAP_DERIVE, SYS_CAP_DROP,
    SYS_COPY_IN, SYS_ECHO, SYS_ECHO_ARG, SYS_EXC_SUBSCRIBE, SYS_EXC_UNSUBSCRIBE, SYS_EXC_WAIT,
    SYS_MEMSEG_CREATE, SYS_MEMSEG_DESTROY, SYS_MEMSEG_MAP, SYS_MEMSEG_UNMAP, SYS_NS_BIND,
    SYS_NS_CREATE, SYS_NS_DESTROY, SYS_NS_MOUNT, SYS_NS_RESOLVE, SYS_NS_UNBIND, SYS_NS_UNMOUNT,
    SYS_RECV, SYS_SEND, SYS_SUM4, SYS_TASK_CREATE, SYS_TASK_DESTROY, SYS_TASK_ID, SYS_TRAP,
    SYS_YIELD, TASK_RECV_BUF, TASK_REQ_VA, TASK2_ID, TASK2_SELF_CAP, TASK2_SEND_CAP,
    TASK2_WAKE_PAYLOAD, UNSET_CAP, USER_MAP_WITNESS_VA, USER_MAP_WITNESS_VA2, USER_SEG_SHARED_VA,
    USER_SEG_VA, USER2_RECV_BUF, USER2_RECV_BUF2, USER2_RECV_BUF3, USER2_SHARED_VA,
};
use crate::vspace::{self, VspaceError};
use prima_ostd::sync::StaticCell;

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

/// S2-D4：`task_create` 创建的任务所用的区域基址（独立 PML4 槽 22）。
///
/// 与 task 1/2 一样独占一个 PML4 槽：新根由 `clone_root` 克隆 bootloader 根后清空槽
/// 20/21/22，故新 Task 的代码/栈不会与既有任务的用户区相互可见。
pub const USER3_BASE_VA: u64 = 0x0000_0B00_0000_0000;
/// `task_create` 创建的任务的代码页 VA（`load_base`/`entry`）。
pub const USER3_CODE_VA: u64 = USER3_BASE_VA;
/// `task_create` 创建的任务的栈页 VA（调用者经 manager cap 映射到此）。
pub const USER3_STACK_VA: u64 = USER3_BASE_VA + 0x1000;
/// 新任务用户区所在的 PML4 槽。
pub const USER3_SLOT: usize = 22;

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

pub static KSTACK_TASK1: StaticCell<KStack> = StaticCell::new(KStack([0; KSTACK_SIZE]));
pub static KSTACK_TASK2: StaticCell<KStack> = StaticCell::new(KStack([0; KSTACK_SIZE]));

/// S2-D4：`task_create` 创建的 Task 的每任务内核栈（索引 = task id）。
///
/// 只覆盖 id 3..[`USER_TASK_KSTACK_COUNT`]（task 1/2 仍用上面的静态栈，故其地址与
/// `preempt_ready` 的 `rsp*`/`kstack*_top` 不变）。新增 BSS 追加在既有静态之后，既有静态
/// 的相对位置不变。
pub const USER_TASK_KSTACK_COUNT: usize = 6;

pub static KSTACK_TASKS: StaticCell<[KStack; USER_TASK_KSTACK_COUNT]> =
    StaticCell::new([const { KStack([0; KSTACK_SIZE]) }; USER_TASK_KSTACK_COUNT]);

/// S2-D4：task id（≥3）的内核栈顶；越界或未分配返回 `None`。
pub fn task_kstack_top(task_id: u32) -> Option<u64> {
    let index = task_id.checked_sub(3)? as usize;
    if index >= USER_TASK_KSTACK_COUNT {
        return None;
    }
    let stack = KSTACK_TASKS.with(|stacks| &stacks[index] as *const KStack);
    Some(kstack_top(stack))
}

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
/// S2-D4：`task_create` 创建的任务的代码长度（`task3_code()`）。
pub const TASK3_CODE_LEN: usize = 134;
/// 切片 V：`task4_code()` 在**同一代码页内**的偏移。
///
/// 复用切片 T 的代码段（1 页 4 KiB，`task3_code()` 仅占 134 字节），故不新增代码段帧——
/// `frames_ready.segment` 不变。第二次 `SYS_TASK_CREATE` 的 `entry` 指向
/// `USER3_CODE_VA + TASK4_CODE_OFFSET`（§8.1 只要求 `entry ∈ [load_base, load_base + pages*4KiB)`）。
pub const TASK4_CODE_OFFSET: u64 = 256;
/// 切片 V：task 4 的代码长度（自报 id → 让出 → 自毁 → 不该到达）。
///
/// **必须先让出再自毁**：§8.2 的 `task_destroy` 会丢弃 sender 为该 Task 的消息，若在同一
/// 时间片内发送后立即自毁，刚排队的 id 会被自己的销毁清理掉（实测确认），父任务将取不到。
pub const TASK4_CODE_LEN: usize = 109;
/// S2-D4：子任务若被再次调度时写入的哨兵（正常路径下销毁后不再被调度，故该值不应出现）。
pub const TASK3_RESCHEDULED_MAGIC: u64 = 0xDEAD_0000_0000_0003;

/// 用户代码长度。SPEC-066 追加「自持平台 cap 映射」块（2458..2636），
/// SPEC-067 追加「task 4 自毁见证」块（2636..2907），SPEC-081 追加「Namespace 路径操作」
/// 块（2979..，含末尾 `int3` 与代码页高偏移的路径文本数据区）。
pub const USER_CODE_LEN: usize = 4056;

/// 第二任务代码字节长度。
pub const TASK2_CODE_LEN: usize = 496;

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

/// `mov [rcx+disp32], rax`（6 字节）——把 syscall 返回值写入 Y 回执页。
const fn put_store_rax<const N: usize>(code: &mut [u8; N], at: usize, disp: u32) {
    put(code, at, &[0x48, 0x89, 0x81]);
    put(code, at + 3, &disp.to_le_bytes());
}

/// `mov <reg>, [rcx+disp32]`（6 字节）——从 Y 回执页读回句柄。
///
/// `rex`/`modrm` 由调用点给出：`rdi`=`(0x48, 0xB9)`、`rsi`=`(0x48, 0xB1)`、
/// `rdx`=`(0x48, 0x91)`、`r10`=`(0x4C, 0x91)`。
const fn put_load_reg<const N: usize>(
    code: &mut [u8; N],
    at: usize,
    rex: u8,
    modrm: u8,
    disp: u32,
) {
    put(code, at, &[rex, 0x8B, modrm]);
    put(code, at + 3, &disp.to_le_bytes());
}

/// `mov rdi, [rcx+disp32]` 的 `modrm`。
const NS_MODRM_RDI: u8 = 0xB9;
/// `mov r10, [rcx+disp32]` 的 `modrm`（配合 REX.WR = 0x4C）。
const NS_MODRM_R10: u8 = 0x91;

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
/// 让出 → 再经**直投路径**送一条无 cap 消息唤醒阻塞的 task 2 → 让出；
/// S2-D4：建栈段 → `SYS_TASK_CREATE`（完整 §8.1）→ 两个创建负例（入口越界 / 缺 `SegMapX`）→
/// 把栈映射进**子任务**根 → 让出 → `SYS_RECV` 取子任务回发的 id → 缺 `TaskDestroy` 的销毁负例 →
/// 销毁成功 → 重复销毁与向已销毁任务映射两个负例 → `int3`。
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
    // --- S2-D4：Task create/yield/destroy 用户原语 -------------------------------------------------
    // rcx = Y（回执），rdx = 请求块（用户栈页内，可写）。
    put_imm64(&mut code, 1794, 0x48, 0xB9, USER_Y_VA);
    put_imm64(&mut code, 1804, 0x48, 0xBA, TASK_REQ_VA);
    // 建请求块（10 个 qword）：code_seg_cap/code_offset/code_pages/load_base/entry/stack_top/
    // parent_ns_cap/root_rights/initial_cap0/initial_cap1。
    put_imm64(
        &mut code,
        1814,
        0x48,
        0xB8,
        crate::bootstrap::TASK_CREATE_CODE_CAP_HANDLE.encode(),
    );
    put(&mut code, 1824, &[0x48, 0x89, 0x02]);
    put_imm64(&mut code, 1827, 0x48, 0xB8, 0);
    put(&mut code, 1837, &[0x48, 0x89, 0x42, 0x08]);
    put_imm64(&mut code, 1841, 0x48, 0xB8, 1);
    put(&mut code, 1851, &[0x48, 0x89, 0x42, 0x10]);
    put_imm64(&mut code, 1855, 0x48, 0xB8, USER3_CODE_VA);
    put(&mut code, 1865, &[0x48, 0x89, 0x42, 0x18]);
    put_imm64(&mut code, 1869, 0x48, 0xB8, USER3_CODE_VA);
    put(&mut code, 1879, &[0x48, 0x89, 0x42, 0x20]);
    put_imm64(&mut code, 1883, 0x48, 0xB8, USER3_STACK_VA + FRAME_SIZE);
    put(&mut code, 1893, &[0x48, 0x89, 0x42, 0x28]);
    put_imm64(
        &mut code,
        1897,
        0x48,
        0xB8,
        crate::bootstrap::NAMESPACE_CAP_HANDLE.encode(),
    );
    put(&mut code, 1907, &[0x48, 0x89, 0x42, 0x30]);
    put_imm64(
        &mut code,
        1911,
        0x48,
        0xB8,
        crate::bootstrap::TASK_CREATE_ROOT_NS_RIGHTS.bits() as u64,
    );
    put(&mut code, 1921, &[0x48, 0x89, 0x42, 0x38]);
    put_imm64(
        &mut code,
        1925,
        0x48,
        0xB8,
        crate::bootstrap::TASK_CREATE_SEND_CAP_HANDLE.encode(),
    );
    put(&mut code, 1935, &[0x48, 0x89, 0x42, 0x40]);
    put_imm64(&mut code, 1939, 0x48, 0xB8, 0);
    put(&mut code, 1949, &[0x48, 0x89, 0x42, 0x48]);
    // 栈段 = SYS_MEMSEG_CREATE(1, R|W) → [Y+368]
    put_imm64(&mut code, 1953, 0x48, 0xBF, 1);
    put_imm64(&mut code, 1963, 0x48, 0xBE, MEMSEG_RW_BITS);
    put_imm64(&mut code, 1973, 0x48, 0xB8, SYS_MEMSEG_CREATE);
    put(&mut code, 1983, &[0xCD, 0x80]);
    put(&mut code, 1985, &[0x48, 0x89, 0x81, 0x70, 0x01, 0, 0]);
    // mgr = SYS_TASK_CREATE(req)
    put(&mut code, 1992, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 1995, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2005, &[0xCD, 0x80]);
    put(&mut code, 2007, &[0x48, 0x89, 0x81, 0x78, 0x01, 0, 0]);
    // 负例 1：entry 越出代码段 → EINVAL
    put_imm64(&mut code, 2014, 0x48, 0xB8, USER3_BASE_VA + 0x9000);
    put(&mut code, 2024, &[0x48, 0x89, 0x42, 0x20]);
    put(&mut code, 2028, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 2031, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2041, &[0xCD, 0x80]);
    put(&mut code, 2043, &[0x48, 0x89, 0x81, 0x80, 0x01, 0, 0]);
    put_imm64(&mut code, 2050, 0x48, 0xB8, USER3_CODE_VA);
    put(&mut code, 2060, &[0x48, 0x89, 0x42, 0x20]);
    // 负例 2：code_seg_cap 换成只有 R|W 的栈段 cap（缺 SegMapX）→ EACCES
    put(&mut code, 2064, &[0x48, 0x8B, 0x81, 0x70, 0x01, 0, 0]);
    put(&mut code, 2071, &[0x48, 0x89, 0x02]);
    put(&mut code, 2074, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 2077, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2087, &[0xCD, 0x80]);
    put(&mut code, 2089, &[0x48, 0x89, 0x81, 0x88, 0x01, 0, 0]);
    put_imm64(
        &mut code,
        2096,
        0x48,
        0xB8,
        crate::bootstrap::TASK_CREATE_CODE_CAP_HANDLE.encode(),
    );
    put(&mut code, 2106, &[0x48, 0x89, 0x02]);
    // 栈映射进**子任务**根：SYS_MEMSEG_MAP(栈段, mgr, 0, USER3_STACK_VA, 1, R|W)
    put(&mut code, 2109, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2116, &[0x48, 0x8B, 0xB1, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2123, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2133, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2143, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2153, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2163, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2173, &[0xCD, 0x80]);
    put(&mut code, 2175, &[0x48, 0x89, 0x81, 0x90, 0x01, 0, 0]);
    // 第一次让出：此时 task 2 仍 Ready（id 2 < 3），会被优先调度；它恢复后进入代码尾部的
    // 阻塞接收循环，从而把 CPU 交回本任务。第二次让出才会真正轮到子任务（id 3）。
    put_imm64(&mut code, 2182, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2192, &[0xCD, 0x80]);
    put_imm64(&mut code, 2194, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2204, &[0xCD, 0x80]);
    // recv = SYS_RECV(TASK_RECV_BUF)（子任务回发自己的 id）
    put_imm64(&mut code, 2206, 0x48, 0xBF, TASK_RECV_BUF);
    put_imm64(&mut code, 2216, 0x48, 0xB8, SYS_RECV);
    put(&mut code, 2226, &[0xCD, 0x80]);
    put(&mut code, 2228, &[0x48, 0x89, 0x81, 0x98, 0x01, 0, 0]);
    // 负例 3：用不含 TaskDestroy 的栈段 cap 销毁 → EACCES
    put(&mut code, 2235, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put_imm64(&mut code, 2242, 0x48, 0xB8, SYS_TASK_DESTROY);
    put(&mut code, 2252, &[0xCD, 0x80]);
    put(&mut code, 2254, &[0x48, 0x89, 0x81, 0xA0, 0x01, 0, 0]);
    // 销毁子任务：SYS_TASK_DESTROY(mgr)
    put(&mut code, 2261, &[0x48, 0x8B, 0xB9, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2268, 0x48, 0xB8, SYS_TASK_DESTROY);
    put(&mut code, 2278, &[0xCD, 0x80]);
    put(&mut code, 2280, &[0x48, 0x89, 0x81, 0xA8, 0x01, 0, 0]);
    // 负例 4：重复销毁（目标已销毁）→ 拒绝
    put(&mut code, 2287, &[0x48, 0x8B, 0xB9, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2294, 0x48, 0xB8, SYS_TASK_DESTROY);
    put(&mut code, 2304, &[0xCD, 0x80]);
    put(&mut code, 2306, &[0x48, 0x89, 0x81, 0xB0, 0x01, 0, 0]);
    // 负例 5：向已销毁的子任务地址空间映射 → 拒绝
    put(&mut code, 2313, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2320, &[0x48, 0x8B, 0xB1, 0x78, 0x01, 0, 0]);
    put_imm64(&mut code, 2327, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2337, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2347, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2357, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2367, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2377, &[0xCD, 0x80]);
    put(&mut code, 2379, &[0x48, 0x89, 0x81, 0xB8, 0x01, 0, 0]);
    // 唤醒阻塞在尾部循环里的 task 2：收尾的抢占阶段需要存在 Ready 任务可轮转。
    put_imm64(&mut code, 2386, 0x48, 0xBF, 2);
    put_imm64(&mut code, 2396, 0x48, 0xBE, TASK2_SEND_CAP);
    put_imm64(&mut code, 2406, 0x48, 0xBA, TASK2_WAKE_PAYLOAD);
    put_imm64(&mut code, 2416, 0x49, 0xBA, 0);
    put_imm64(&mut code, 2426, 0x49, 0xB8, 0);
    put_imm64(&mut code, 2436, 0x49, 0xB9, 0);
    put_imm64(&mut code, 2446, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 2456, &[0xCD, 0x80]);
    // SPEC-066：用**自持**的平台段 cap（切片 U 已按属主移交）经 `SYS_MEMSEG_MAP` 建立新映射。
    // 正例：X 段 cap（槽 36）+ Task(1) 管理 cap（槽 45）→ 应在**用户侧**通过 `authorize`。
    put_imm64(
        &mut code,
        2458,
        0x48,
        0xBF,
        crate::bootstrap::platform_segment_cap_handle(frames::SEG_TASK1_X).encode(),
    );
    put_imm64(
        &mut code,
        2468,
        0x48,
        0xBE,
        crate::bootstrap::PLATFORM_TASK1_CAP_HANDLE.encode(),
    );
    put_imm64(&mut code, 2478, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2488, 0x49, 0xBA, USER_MAP_WITNESS_VA);
    put_imm64(&mut code, 2498, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2508, 0x49, 0xB9, ACCESS_R_BITS);
    put_imm64(&mut code, 2518, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2528, &[0xCD, 0x80]);
    put(&mut code, 2530, &[0x48, 0x89, 0x81, 0xC0, 0x01, 0, 0]);
    // 读回该映射：应等于 X 段首帧的 COW 模式值——证明 PTE 真的指向该 Segment 的帧，
    // 而不是「映射调用返回 0」而已。
    put_imm64(&mut code, 2537, 0x48, 0xBA, USER_MAP_WITNESS_VA);
    put(&mut code, 2547, &[0x48, 0x8B, 0x02]);
    put(&mut code, 2550, &[0x48, 0x89, 0x81, 0xC8, 0x01, 0, 0]);
    // 负例：用 **task 2 所属**的段 cap（槽 38）以 task 1 为 caller 再映射 → `EACCES`
    // （`authorize` 第 3 步的 `holder_of(cap) == caller` 对非属主成立拒绝）。
    put_imm64(
        &mut code,
        2557,
        0x48,
        0xBF,
        crate::bootstrap::platform_segment_cap_handle(frames::SEG_TASK2_CODE).encode(),
    );
    put_imm64(
        &mut code,
        2567,
        0x48,
        0xBE,
        crate::bootstrap::PLATFORM_TASK1_CAP_HANDLE.encode(),
    );
    put_imm64(&mut code, 2577, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2587, 0x49, 0xBA, USER_MAP_WITNESS_VA2);
    put_imm64(&mut code, 2597, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2607, 0x49, 0xB9, ACCESS_R_BITS);
    put_imm64(&mut code, 2617, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2627, &[0xCD, 0x80]);
    put(&mut code, 2629, &[0x48, 0x89, 0x81, 0xD0, 0x01, 0, 0]);
    // SPEC-067（切片 V）：task 4 承载自毁见证。
    // rdx 在 SPEC-066 块里被用作映射目标 VA，这里重设为请求块基址。
    put_imm64(&mut code, 2636, 0x48, 0xBA, TASK_REQ_VA);
    // 改写请求块：entry = 代码段首 + TASK4_CODE_OFFSET（同页内偏移），initial_cap0 = task4 的发送凭据。
    put_imm64(
        &mut code,
        2646,
        0x48,
        0xB8,
        USER3_CODE_VA + TASK4_CODE_OFFSET,
    );
    put(&mut code, 2656, &[0x48, 0x89, 0x42, 0x20]);
    put_imm64(
        &mut code,
        2660,
        0x48,
        0xB8,
        crate::bootstrap::TASK4_SEND_CAP_HANDLE.encode(),
    );
    put(&mut code, 2670, &[0x48, 0x89, 0x42, 0x40]);
    // create task 4
    put(&mut code, 2674, &[0x48, 0x89, 0xD7]);
    put_imm64(&mut code, 2677, 0x48, 0xB8, SYS_TASK_CREATE);
    put(&mut code, 2687, &[0xCD, 0x80]);
    put(&mut code, 2689, &[0x48, 0x89, 0x81, 0xD8, 0x01, 0, 0]);
    // 复用 task 3 的栈段 cap（段对象不随 task 3 销毁），把栈映射进 **task 4** 的根。
    put(&mut code, 2696, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2703, &[0x48, 0x8B, 0xB1, 0xD8, 0x01, 0, 0]);
    put_imm64(&mut code, 2710, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2720, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2730, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2740, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2750, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2760, &[0xCD, 0x80]);
    put(&mut code, 2762, &[0x48, 0x89, 0x81, 0xE0, 0x01, 0, 0]);
    // 让出两次：第一次排空刚被唤醒的 task 2（它执行尾部阻塞接收后变 BlockedRecv），
    // 第二次才轮到 task 4（id 4）。
    put_imm64(&mut code, 2769, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2779, &[0xCD, 0x80]);
    put_imm64(&mut code, 2781, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2791, &[0xCD, 0x80]);
    // task 4 自报 id 后自毁并切回本任务；取消息拿它的 id。
    put_imm64(&mut code, 2793, 0x48, 0xBF, TASK_RECV_BUF);
    put_imm64(&mut code, 2803, 0x48, 0xB8, SYS_RECV);
    put(&mut code, 2813, &[0xCD, 0x80]);
    put(&mut code, 2815, &[0x48, 0x89, 0x81, 0xE8, 0x01, 0, 0]);
    // task 4 已把 id 排队；让出一次让它完成自毁，再回来验证。
    put_imm64(&mut code, 2822, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 2832, &[0xCD, 0x80]);
    // 向已自毁的 task 4 映射 → 应 ENOENT（其调度登记已清、地址空间已拆）。
    put(&mut code, 2834, &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
    put(&mut code, 2841, &[0x48, 0x8B, 0xB1, 0xD8, 0x01, 0, 0]);
    put_imm64(&mut code, 2848, 0x48, 0xBA, 0);
    put_imm64(&mut code, 2858, 0x49, 0xBA, USER3_STACK_VA);
    put_imm64(&mut code, 2868, 0x49, 0xB8, 1);
    put_imm64(&mut code, 2878, 0x49, 0xB9, MEMSEG_RW_BITS);
    put_imm64(&mut code, 2888, 0x48, 0xB8, SYS_MEMSEG_MAP);
    put(&mut code, 2898, &[0xCD, 0x80]);
    put(&mut code, 2900, &[0x48, 0x89, 0x81, 0xF0, 0x01, 0, 0]);
    // 收尾的抢占阶段需要 Ready 任务可轮转：切片 V 的第一次让出已把 task 2 排空成
    // `BlockedRecv`，故这里再直投唤醒它一次（同切片 T 末尾的做法，位置后移）。
    put_imm64(&mut code, 2907, 0x48, 0xBF, 2);
    put_imm64(&mut code, 2917, 0x48, 0xBE, TASK2_SEND_CAP);
    put_imm64(&mut code, 2927, 0x48, 0xBA, TASK2_WAKE_PAYLOAD);
    put_imm64(&mut code, 2937, 0x49, 0xBA, 0);
    put_imm64(&mut code, 2947, 0x49, 0xB8, 0);
    put_imm64(&mut code, 2957, 0x49, 0xB9, 0);
    put_imm64(&mut code, 2967, 0x48, 0xB8, SYS_SEND);
    put(&mut code, 2977, &[0xCD, 0x80]);

    // ---- SPEC-081：§11 Namespace 路径操作（切片 AJ）----
    // 布局：本块接在 SPEC-067 块之后；末尾 `int3` 收尾，路径文本数据放在代码页高偏移
    // （`int3` 之后不会被当作指令执行）。`rcx` 恒为回执页基址（syscall 返回后由内核
    // 保存/恢复全部用户寄存器，故一直有效）。
    let mut at = 2979usize;
    put_imm64(&mut code, at, 0x48, 0xB9, USER_Y_VA);
    at += 10;

    // child1/child2 = derive(image_cap, {Derive, SegMapR})——避免移动初始 image cap。
    put_imm64(&mut code, at, 0x48, 0xBF, IMAGE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_BIND_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_DERIVE1 as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBF, IMAGE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_BIND_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_DERIVE2 as u32);
    at += 7;

    // ns_create ×3（a / b / c），回执依次写 512/520/528。
    let mut created = 0u32;
    while created < 3 {
        put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_CREATE);
        at += 10;
        put(&mut code, at, &[0xCD, 0x80]);
        at += 2;
        put_store_rax(&mut code, at, NS_RECEIPT_CREATE_A as u32 + created * 8);
        at += 7;
        created += 1;
    }

    // b_copy / c_copy = derive(ns 根 cap, NAMESPACE_RIGHTS)——挂载会移走源 cap，副本留作
    // 环检测的 destination 与销毁见证。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_B as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_ROOT_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_COPY_B as u32);
    at += 7;
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_C as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_ROOT_RIGHTS_BITS);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_CAP_DERIVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_COPY_C as u32);
    at += 7;

    // mount(a, "x", b)：建立边 a → b（b 的 cap 移入 a）。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_A as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_X_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 1);
    at += 10;
    put_load_reg(
        &mut code,
        at,
        0x4C,
        NS_MODRM_R10,
        NS_RECEIPT_CREATE_B as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_MOUNT);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_MOUNT_A_B as u32);
    at += 7;

    // mount(b_copy, "y", a)：a 沿边可达 b ⇒ 成环 ⇒ EINVAL。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, NS_RECEIPT_COPY_B as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_Y_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 1);
    at += 10;
    put_load_reg(
        &mut code,
        at,
        0x4C,
        NS_MODRM_R10,
        NS_RECEIPT_CREATE_A as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_MOUNT);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_MOUNT_CYCLE as u32);
    at += 7;

    // bind(init_ns, "lib", child1) 成功；再 bind 同名 child2 ⇒ EEXIST。
    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_DERIVE1 as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_BIND);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BIND as u32);
    at += 7;

    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_DERIVE2 as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_BIND);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BIND_OVERLAP as u32);
    at += 7;

    // bind(c, "sub", child2)：为跨挂载解析准备子 Namespace 内的绑定项。
    put_load_reg(
        &mut code,
        at,
        0x48,
        NS_MODRM_RDI,
        NS_RECEIPT_CREATE_C as u32,
    );
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_SUB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_DERIVE2 as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_BIND);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BIND_SUB as u32);
    at += 7;

    // resolve(init_ns, "lib", {NsResolve})：同一 Namespace 内的末端绑定解析。
    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, 1 << 1); // r10 = {SegMapR}（父项 child1 的合法子集）
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_RESOLVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_RESOLVE_LIB as u32);
    at += 7;

    // mount(init_ns, "mnt", c_copy)：把子 Namespace 挂到 init 下。
    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_MNT_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_load_reg(&mut code, at, 0x4C, NS_MODRM_R10, NS_RECEIPT_COPY_C as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_MOUNT);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_MOUNT_INIT_C as u32);
    at += 7;

    // resolve(init_ns, "mnt/sub", {SegMapR})：跨挂载解析，父项是 c 内的 child2。
    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(
        &mut code,
        at,
        0x48,
        0xBE,
        NS_PATH_DATA_VA + NS_PATH_MNTSUB_OFF,
    );
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 7);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, 1 << 1); // r10 = {SegMapR}
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_RESOLVE);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_RESOLVE_SUB as u32);
    at += 7;

    // bind(init_ns, "/lib", task_cap)：前导 `/` 产生空分量 ⇒ Path::parse 拒绝 ⇒ EINVAL。
    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(
        &mut code,
        at,
        0x48,
        0xBE,
        NS_PATH_DATA_VA + NS_PATH_ILLEGAL_OFF,
    );
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 4);
    at += 10;
    put_imm64(&mut code, at, 0x49, 0xBA, TASK_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_BIND);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_BAD_PATH as u32);
    at += 7;

    // unbind(init_ns, "lib") → 取回 child1；unmount(init_ns, "mnt")。
    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_LIB_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_UNBIND);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_UNBIND as u32);
    at += 7;

    put_imm64(&mut code, at, 0x48, 0xBF, NAMESPACE_CAP_HANDLE.encode());
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBE, NS_PATH_DATA_VA + NS_PATH_MNT_OFF);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xBA, 3);
    at += 10;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_UNMOUNT);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_UNMOUNT as u32);
    at += 7;

    // destroy(c_copy)：清空 c 的条目、从活动表删除对象（c_copy 变 stale token）。
    put_load_reg(&mut code, at, 0x48, NS_MODRM_RDI, NS_RECEIPT_COPY_C as u32);
    at += 7;
    put_imm64(&mut code, at, 0x48, 0xB8, SYS_NS_DESTROY);
    at += 10;
    put(&mut code, at, &[0xCD, 0x80]);
    at += 2;
    put_store_rax(&mut code, at, NS_RECEIPT_DESTROY as u32);
    at += 7;

    // int3（收尾）：本块必须落在路径文本数据区之前。
    assert!(
        at < NS_PATH_DATA_OFFSET,
        "SPEC-081 用户代码块越过路径文本数据区"
    );
    code[at] = 0xCC;

    // 路径文本数据区（`int3` 之后，不会被当作指令执行；每个槽 8 字节）。
    put(&mut code, NS_PATH_DATA_OFFSET, b"lib");
    put(&mut code, NS_PATH_DATA_OFFSET + 8, b"sub");
    put(&mut code, NS_PATH_DATA_OFFSET + 16, b"mnt");
    put(&mut code, NS_PATH_DATA_OFFSET + 24, b"mnt/sub");
    put(&mut code, NS_PATH_DATA_OFFSET + 32, b"/lib");
    put(&mut code, NS_PATH_DATA_OFFSET + 40, b"x");
    put(&mut code, NS_PATH_DATA_OFFSET + 48, b"y");
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
    // S2-D4：尾部改为**阻塞接收循环**——S2-D4 的 task 1 块需要 task 2 不处于 Ready，
    // 否则 `pick_next` 会一直优先选中 id 更小的 task 2，新建的子任务（id 3）永不运行。
    // 被 task 1 末尾的直投唤醒后若再次被调度，也只是再阻塞一次。
    put_imm64(&mut code, 472, 0x48, 0xBF, USER2_RECV_BUF3);
    put_imm64(&mut code, 482, 0x48, 0xB8, SYS_RECV);
    put(&mut code, 492, &[0xCD, 0x80]);
    put(&mut code, 494, &[0xEB, 0xE8]); // jmp 472
    code
}

/// S2-D4：`task_create` 创建的任务的用户代码。
///
/// `mov rcx, USER3_STACK_VA; SYS_TASK_ID → [rcx]; SYS_SEND(1, TASK_CREATE_SEND_CAP_HANDLE,
/// payload = [rcx], 0,0,0) → [rcx+8]; SYS_YIELD;`（若被重新调度）`写哨兵 [rcx+16]，再 yield，死循环`。
///
/// payload 是子任务自己经 `SYS_TASK_ID` 拿到的 id——父任务据此证明「子任务真的跑过，
/// 且它自报的身份正确」。发送凭证由 `task_create` 的 `initial_cap0` 从 task 1 移入，句柄在
/// 移动中不变，故此处内嵌 [`crate::bootstrap::TASK_CREATE_SEND_CAP_HANDLE`]。
pub const fn task3_code() -> [u8; TASK3_CODE_LEN] {
    let mut code = [0u8; TASK3_CODE_LEN];
    // 0: mov rcx, USER3_STACK_VA（回执基址 = 自己的栈页）
    put_imm64(&mut code, 0, 0x48, 0xB9, USER3_STACK_VA);
    // 10: mov rax, SYS_TASK_ID
    put_imm64(&mut code, 10, 0x48, 0xB8, SYS_TASK_ID);
    // 20: int 0x80
    put(&mut code, 20, &[0xCD, 0x80]);
    // 22: mov [rcx], rax
    put(&mut code, 22, &[0x48, 0x89, 0x01]);
    // 25: mov rdi, 1（接收者 = task 1）
    put_imm64(&mut code, 25, 0x48, 0xBF, 1);
    // 35: mov rsi, TASK_CREATE_SEND_CAP_HANDLE（initial_cap0 移入的发送凭证）
    put_imm64(
        &mut code,
        35,
        0x48,
        0xBE,
        crate::bootstrap::TASK_CREATE_SEND_CAP_HANDLE.encode(),
    );
    // 45: mov rdx, [rcx]（payload = 自己的 task id）
    put(&mut code, 45, &[0x48, 0x8B, 0x11]);
    // 48: mov r10, 0（cap_count）
    put_imm64(&mut code, 48, 0x49, 0xBA, 0);
    // 58: mov r8, 0
    put_imm64(&mut code, 58, 0x49, 0xB8, 0);
    // 68: mov r9, 0
    put_imm64(&mut code, 68, 0x49, 0xB9, 0);
    // 78: mov rax, SYS_SEND
    put_imm64(&mut code, 78, 0x48, 0xB8, SYS_SEND);
    // 88: int 0x80
    put(&mut code, 88, &[0xCD, 0x80]);
    // 90: mov [rcx+8], rax
    put(&mut code, 90, &[0x48, 0x89, 0x41, 0x08]);
    // 94: mov rax, SYS_YIELD（让回父任务；此后应永不再被调度）
    put_imm64(&mut code, 94, 0x48, 0xB8, SYS_YIELD);
    // 104: int 0x80
    put(&mut code, 104, &[0xCD, 0x80]);
    // 106: mov rax, TASK3_RESCHEDULED_MAGIC（仅在「销毁后仍被调度」时才会执行）
    put_imm64(&mut code, 106, 0x48, 0xB8, TASK3_RESCHEDULED_MAGIC);
    // 116: mov [rcx+16], rax
    put(&mut code, 116, &[0x48, 0x89, 0x41, 0x10]);
    // 120: mov rax, SYS_YIELD
    put_imm64(&mut code, 120, 0x48, 0xB8, SYS_YIELD);
    // 130: int 0x80
    put(&mut code, 130, &[0xCD, 0x80]);
    // 132: jmp $
    put(&mut code, 132, &[0xEB, 0xFE]);
    code
}

/// 切片 V：task 4 的机器码（自毁见证载体）。
///
/// `mov rbx, rdi`（初始帧 `rdi` 携带的 self Capability 句柄）→ `SYS_TASK_ID` →
/// `SYS_SEND(1, TASK4_SEND_CAP, id, 0, 0, 0)` 自报 id → `SYS_TASK_DESTROY(rbx)` **自毁**
/// → `jmp $`（自毁成功则永不执行）。
pub const fn task4_code() -> [u8; TASK4_CODE_LEN] {
    let mut code = [0u8; TASK4_CODE_LEN];
    // 0: mov rbx, rdi（self cap）
    put(&mut code, 0, &[0x48, 0x89, 0xFB]);
    // 3: mov rax, SYS_TASK_ID
    put_imm64(&mut code, 3, 0x48, 0xB8, SYS_TASK_ID);
    // 13: int 0x80
    put(&mut code, 13, &[0xCD, 0x80]);
    // 15: mov rdx, rax（payload = 自己的 task id）
    put(&mut code, 15, &[0x48, 0x89, 0xC2]);
    // 18: mov rdi, 1（接收者 = task 1）
    put_imm64(&mut code, 18, 0x48, 0xBF, 1);
    // 28: mov rsi, TASK4_SEND_CAP_HANDLE（第二次 create 的 initial_cap0 移入）
    put_imm64(
        &mut code,
        28,
        0x48,
        0xBE,
        crate::bootstrap::TASK4_SEND_CAP_HANDLE.encode(),
    );
    // 38: mov r10, 0（cap_count）
    put_imm64(&mut code, 38, 0x49, 0xBA, 0);
    // 48: mov r8, 0
    put_imm64(&mut code, 48, 0x49, 0xB8, 0);
    // 58: mov r9, 0
    put_imm64(&mut code, 58, 0x49, 0xB9, 0);
    // 68: mov rax, SYS_SEND
    put_imm64(&mut code, 68, 0x48, 0xB8, SYS_SEND);
    // 78: int 0x80
    put(&mut code, 78, &[0xCD, 0x80]);
    // 80: 让出，等父任务取走刚排队的 id——否则自毁会把这条消息一起丢弃（§8.2）。
    put_imm64(&mut code, 80, 0x48, 0xB8, SYS_YIELD);
    put(&mut code, 90, &[0xCD, 0x80]);
    // 92: mov rdi, rbx（self cap）
    put(&mut code, 92, &[0x48, 0x89, 0xDF]);
    // 95: mov rax, SYS_TASK_DESTROY
    put_imm64(&mut code, 95, 0x48, 0xB8, SYS_TASK_DESTROY);
    // 105: int 0x80 —— 自毁成功则不再返回本任务
    put(&mut code, 105, &[0xCD, 0x80]);
    // 107: jmp $（自毁失败时停在此处，而非继续跑）
    put(&mut code, 107, &[0xEB, 0xFE]);
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

static USER_CTX: StaticCell<Option<UserCtx>> = StaticCell::new(None);

fn read_qword(va: u64) -> u64 {
    prima_ostd::mm::read_u64(va)
}

fn write_qword(va: u64, value: u64) {
    prima_ostd::mm::write_u64(va, value);
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

/// 切片 X：自建内核根（boot 早期构造一次）。
///
/// 任务根由它克隆而来，故内核高半区（镜像、直映射、启动栈）在本内核下**全部是自有页表**，
/// 不再是 bootloader 页表的共享上级。
static KERNEL_ROOT: StaticCell<Option<u64>> = StaticCell::new(None);

/// 镜像所在的 PML4 槽（`kernel_image_offset = 0x10000000000`）——自建根的镜像链入口。
pub const IMAGE_SLOT: usize = 2;

/// SPEC-069：自建内核根对应的镜像 VA 基址与页数（见证用）。
static KERNEL_IMAGE: StaticCell<Option<(u64, usize)>> = StaticCell::new(None);

/// SPEC-069：自建 CR3 的判决性见证（必须在内核已运行于自建根**之后**采集）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfRootWitness {
    /// 当前 CR3（运行中的地址空间根）。
    pub root: u64,
    /// 登记的自建内核根帧。
    pub kernel_root: u64,
    /// 当前根 PML4[2] 指向的中间表帧（镜像链的第一级）。
    pub image_table: u64,
    /// 该表帧在帧注册表中的归属是否为「保留页表帧」（W6 第 3 类）。
    pub owned: bool,
    /// 当前根下已映射的镜像 VA 页数。
    pub pages: u64,
    /// 当前根解析 `kernel_image_offset` 得到的物理地址（应等于内核镜像物理基址）。
    pub translated: u64,
}

/// 采集自建 CR3 见证；自建根或 COW 上下文尚未建立时返回 `None`。
pub fn self_root_witness() -> Option<SelfRootWitness> {
    let kernel_root = kernel_root()?;
    let (image_va, image_pages) = KERNEL_IMAGE.with(|slot| *slot)?;
    let (_, offset) = crate::cow::root_and_offset()?;
    let map = |phys: u64| phys_to_virt(phys, offset);
    let root = crate::sched::current_root().unwrap_or(kernel_root);
    let image_table = prima_ostd::mm::read_u64_at(map(root), IMAGE_SLOT as u64);
    let image_table = image_table & 0x000f_ffff_ffff_f000;
    let owned = matches!(
        crate::frames::owner_of(image_table),
        Ok(crate::frames::FrameOwner::Table)
    );
    let mut pages = 0u64;
    for index in 0..image_pages {
        if vspace::leaf_phys_or_none(root, image_va + (index as u64) * FRAME_SIZE, map).is_some() {
            pages += 1;
        }
    }
    let translated = crate::cpu::translate(map(root), image_va, map).unwrap_or(0);
    Some(SelfRootWitness {
        root,
        kernel_root,
        image_table,
        owned,
        pages,
        translated,
    })
}

/// 已构造的自建内核根（未构造时为 `None`）。
pub fn kernel_root() -> Option<u64> {
    KERNEL_ROOT.with(|slot| *slot)
}

/// 切片 X：构造自建内核根并登记，供随后所有任务根克隆。
///
/// 复刻三段：内核镜像（`image_va`/`image_phys`/`image_pages`）、物理直映射
/// （`offset` → `[0, max_phys)`）、启动栈（`stack_va`/`stack_pages`）。启动栈的**物理地址
/// 逐页从 bootloader 根解析**——它自己的映射不在直映射区内，且可能是大页，故不能用
/// [`crate::cpu::translate`]（只认 4 KiB 叶）。
///
/// 用户槽不在本根中建立：任务根克隆后再各自加载自己的用户页（同 VA 不同帧的隔离不变）。
pub fn build_kernel_root(
    boot_root: u64,
    offset: u64,
    image_va: u64,
    image_pages: usize,
    max_phys: u64,
    stack_va: u64,
    stack_pages: usize,
) -> Result<u64, VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    let stack_phys = |index: usize| {
        vspace::leaf_phys_or_none(boot_root, stack_va + (index as u64) * FRAME_SIZE, map)
    };
    let direct_pages = max_phys.div_ceil(FRAME_SIZE) as usize;
    let image_phys = |index: usize| {
        vspace::leaf_phys_or_none(boot_root, image_va + (index as u64) * FRAME_SIZE, map)
    };
    let root = vspace::build_self_root(
        map,
        || frames::allocate_table_frame().ok(),
        vspace::SelfRootSpec {
            direct: vspace::SelfMap {
                va: offset,
                phys: 0,
                pages: direct_pages,
                writable: true,
            },
            image_va,
            image_pages,
            stack_va,
            stack_pages,
        },
        image_phys,
        stack_phys,
    )?;
    KERNEL_ROOT.with_mut(|slot| *slot = Some(root));
    KERNEL_IMAGE.with_mut(|slot| *slot = Some((image_va, image_pages)));
    Ok(root)
}

/// 切片 O / X：为一任务构造独立 PML4——克隆**自建内核根**（本内核自有的内核高半区）后，
/// 清空两个用户槽，随后由调用方把自己的用户页映射进来。同 VA 在不同任务根下指向不同帧。
fn build_task_root(offset: u64) -> Result<u64, VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    let source = kernel_root().ok_or(VspaceError::BadRoot)?;
    // S1-X3：任务根页表帧是**保留页表帧**，不是任何 Segment 的数据帧。
    let task_root = frames::allocate_table_frame().map_err(|_| VspaceError::NoFrame)?;
    // clone_root 覆写全部 512 项，故无需先清零。
    vspace::clone_root(source, task_root, map);
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
pub fn prepare(offset: u64) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    let task_root = build_task_root(offset)?;
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
        prima_ostd::mm::write_u8_at(map(code), index as u64, byte);
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
    sched::set_kstack_top(INIT_TASK_ID, kstack_top(KSTACK_TASK1.as_ptr()));
    crate::cow::register_user_region(USER_X_VA, x);

    USER_CTX.with_mut(|slot| {
        *slot = Some(UserCtx {
            offset,
            orig_x_phys: x,
        });
    });
    Ok(())
}

/// 切片 M：构造第二任务（code/stack/Y 三页，无 COW X）。
///
/// 与 [`prepare`] 一样必须在 `cow::prepare` 之后，复用同一唯一分配器。切片 O：映射进
/// **task 2 独立根**，并登记 root/内核栈；task 2 的 Y 页物理帧另行持久化，供 task 1
/// 根下不可见 Y2 VA 时按物理读回执。
pub fn prepare_task2(offset: u64) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    let task_root = build_task_root(offset)?;
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
        prima_ostd::mm::write_u8_at(map(code), index as u64, byte);
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
    sched::set_kstack_top(TASK2_ID, kstack_top(KSTACK_TASK2.as_ptr()));
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
            prima_ostd::mm::write_u8_at(map(code), index as u64, byte);
        }
        for index in 0..(FRAME_SIZE / 8) {
            write_qword(map(data) + index * 8, 0);
        }

        map_region(code_cap, task_cap, SPIN_CODE_VA, code_access, root, offset)?;
        map_region(data_cap, task_cap, SPIN_DATA_VA, data_access, root, offset)?;
    }
    Ok(())
}

/// S2-D4：建新 Task 的代码段并写入 `task3_code()`。
///
/// 必须在既有 bring-up 取帧序列（`prepare`/`prepare_task2`/`prepare_spin`）**之后**调用，
/// 否则会平移切片 E/F/G/O 的物理帧序列与既有金标地址。
pub fn prepare_task_create(offset: u64) -> Result<(), VspaceError> {
    let frame =
        crate::bootstrap::prepare_task_create_caps(offset).map_err(|_| VspaceError::NoFrame)?;
    // task 3 的代码在帧首。
    write_code(frame, offset, 0, &task3_code());
    // 切片 V：task 4 的代码复用**同一页**、写在页内偏移处（不新增代码段帧）。
    write_code(frame, offset, TASK4_CODE_OFFSET, &task4_code());
    Ok(())
}

/// 把 `code` 写到代码段首帧的页内偏移 `at` 处。
fn write_code(frame: u64, offset: u64, at: u64, code: &[u8]) {
    let base = phys_to_virt(frame, offset) + at;
    for (index, &byte) in code.iter().enumerate() {
        prima_ostd::mm::write_u8_at(base, index as u64, byte);
    }
}

/// S2-D4：把 `task_create` 的新 Task 登记到调度器（root / 内核栈 / 初始用户上下文）。
///
/// 由 syscall 层在 [`crate::bootstrap::task_create`] 成功后调用——`sched` 不属于 bootstrap 模型层。
/// 入口 `rip = entry`，`rsp = stack_top`（细化层字段），CS/SS 为用户选择子，RFLAGS 置 FIXED+IF。
pub fn register_created_task(
    task_id: u32,
    root_phys: u64,
    entry: u64,
    stack_top: u64,
    self_cap: u64,
) -> Result<(), VspaceError> {
    let kstack = task_kstack_top(task_id).ok_or(VspaceError::BadRoot)?;
    let mut frame = SretFrame::ZERO;
    frame.rip = entry;
    frame.rsp = stack_top;
    // 切片 V：self Capability 的句柄值写入初始帧 `rdi`——子任务据此才能调用
    // `SYS_TASK_DESTROY` 自毁。这是细化层自由度（抽象模型不规定寄存器传参），
    // 不新增槽位、不改句柄分配。
    frame.rdi = self_cap;
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    frame.rflags = 0x2 | (1 << 9); // bit1 固定 + IF=1
    crate::sched::set_root(task_id, root_phys);
    crate::sched::set_kstack_top(task_id, kstack);
    crate::sched::set_frame(task_id, frame);
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
    let (offset, orig_x_phys) = USER_CTX.with(|slot| {
        slot.as_ref()
            .map_or((0, 0), |ctx| (ctx.offset, ctx.orig_x_phys))
    });
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
    use crate::bootstrap::{NAMESPACE_CAP_HANDLE, TASK_CREATE_ROOT_NS_RIGHTS};
    use crate::syscall::{TASK_CODE_CAP, TASK_REQ_VA, TASK_SEND_CAP};

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
        assert_eq!(USER_CODE_LEN, 4056);
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
        // S2-D4：请求块建在用户栈页 TASK_REQ_VA（rcx=Y 回执，rdx=请求块）。
        assert_eq!(&code[1794..1796], &[0x48, 0xB9]);
        assert_eq!(&code[1796..1804], &USER_Y_VA.to_le_bytes());
        assert_eq!(&code[1804..1806], &[0x48, 0xBA]);
        assert_eq!(&code[1806..1814], &TASK_REQ_VA.to_le_bytes());
        assert_eq!(&code[1816..1824], &TASK_CODE_CAP.to_le_bytes());
        assert_eq!(&code[1824..1827], &[0x48, 0x89, 0x02]); // [rdx+0] = code_seg_cap
        assert_eq!(&code[1837..1841], &[0x48, 0x89, 0x42, 0x08]); // code_offset = 0
        assert_eq!(&code[1851..1855], &[0x48, 0x89, 0x42, 0x10]); // code_pages = 1
        assert_eq!(&code[1857..1865], &USER3_CODE_VA.to_le_bytes());
        assert_eq!(&code[1865..1869], &[0x48, 0x89, 0x42, 0x18]); // load_base
        assert_eq!(&code[1879..1883], &[0x48, 0x89, 0x42, 0x20]); // entry
        assert_eq!(
            &code[1885..1893],
            &(USER3_STACK_VA + FRAME_SIZE).to_le_bytes()
        );
        assert_eq!(&code[1893..1897], &[0x48, 0x89, 0x42, 0x28]); // stack_top
        assert_eq!(
            &code[1899..1907],
            &NAMESPACE_CAP_HANDLE.encode().to_le_bytes()
        );
        assert_eq!(&code[1907..1911], &[0x48, 0x89, 0x42, 0x30]); // parent_ns_cap
        assert_eq!(
            &code[1913..1921],
            &(TASK_CREATE_ROOT_NS_RIGHTS.bits() as u64).to_le_bytes()
        );
        assert_eq!(&code[1921..1925], &[0x48, 0x89, 0x42, 0x38]); // root_rights
        assert_eq!(&code[1927..1935], &TASK_SEND_CAP.to_le_bytes());
        assert_eq!(&code[1935..1939], &[0x48, 0x89, 0x42, 0x40]); // initial_cap0
        assert_eq!(&code[1949..1953], &[0x48, 0x89, 0x42, 0x48]); // initial_cap1 = 0
        // 栈段 = SYS_MEMSEG_CREATE(1, R|W) → [Y+368]
        assert_eq!(&code[1963..1965], &[0x48, 0xBE]);
        assert_eq!(&code[1965..1973], &MEMSEG_RW_BITS.to_le_bytes());
        assert_eq!(&code[1975..1983], &SYS_MEMSEG_CREATE.to_le_bytes());
        assert_eq!(&code[1983..1985], &[0xCD, 0x80]);
        assert_eq!(&code[1985..1992], &[0x48, 0x89, 0x81, 0x70, 0x01, 0, 0]);
        // mgr = SYS_TASK_CREATE(req) → [Y+376]
        assert_eq!(&code[1992..1995], &[0x48, 0x89, 0xD7]); // mov rdi, rdx
        assert_eq!(&code[1997..2005], &SYS_TASK_CREATE.to_le_bytes());
        assert_eq!(&code[2005..2007], &[0xCD, 0x80]);
        assert_eq!(&code[2007..2014], &[0x48, 0x89, 0x81, 0x78, 0x01, 0, 0]);
        // 负例 1：entry 越界 → [Y+384]；随后还原 entry。
        assert_eq!(&code[2016..2024], &(USER3_BASE_VA + 0x9000).to_le_bytes());
        assert_eq!(&code[2024..2028], &[0x48, 0x89, 0x42, 0x20]);
        assert_eq!(&code[2043..2050], &[0x48, 0x89, 0x81, 0x80, 0x01, 0, 0]);
        assert_eq!(&code[2060..2064], &[0x48, 0x89, 0x42, 0x20]);
        // 负例 2：code_seg_cap 换成栈段 cap（缺 SegMapX）→ [Y+392]；随后还原。
        assert_eq!(&code[2064..2071], &[0x48, 0x8B, 0x81, 0x70, 0x01, 0, 0]);
        assert_eq!(&code[2071..2074], &[0x48, 0x89, 0x02]);
        assert_eq!(&code[2089..2096], &[0x48, 0x89, 0x81, 0x88, 0x01, 0, 0]);
        assert_eq!(&code[2098..2106], &TASK_CODE_CAP.to_le_bytes());
        // 栈映射进子任务根 → [Y+400]
        assert_eq!(&code[2109..2116], &[0x48, 0x8B, 0xB9, 0x70, 0x01, 0, 0]);
        assert_eq!(&code[2116..2123], &[0x48, 0x8B, 0xB1, 0x78, 0x01, 0, 0]);
        assert_eq!(&code[2135..2143], &USER3_STACK_VA.to_le_bytes());
        assert_eq!(&code[2165..2173], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2175..2182], &[0x48, 0x89, 0x81, 0x90, 0x01, 0, 0]);
        // 两次让出：先排空 Ready 的 task 2，再轮到子任务。
        assert_eq!(&code[2184..2192], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2196..2204], &SYS_YIELD.to_le_bytes());
        // recv → [Y+408]
        assert_eq!(&code[2208..2216], &TASK_RECV_BUF.to_le_bytes());
        assert_eq!(&code[2218..2226], &SYS_RECV.to_le_bytes());
        assert_eq!(&code[2228..2235], &[0x48, 0x89, 0x81, 0x98, 0x01, 0, 0]);
        // 缺 TaskDestroy 的销毁负例 → [Y+416]；销毁成功 → [Y+424]；重复销毁 → [Y+432]。
        assert_eq!(&code[2244..2252], &SYS_TASK_DESTROY.to_le_bytes());
        assert_eq!(&code[2254..2261], &[0x48, 0x89, 0x81, 0xA0, 0x01, 0, 0]);
        assert_eq!(&code[2270..2278], &SYS_TASK_DESTROY.to_le_bytes());
        assert_eq!(&code[2280..2287], &[0x48, 0x89, 0x81, 0xA8, 0x01, 0, 0]);
        assert_eq!(&code[2296..2304], &SYS_TASK_DESTROY.to_le_bytes());
        assert_eq!(&code[2306..2313], &[0x48, 0x89, 0x81, 0xB0, 0x01, 0, 0]);
        // 向已销毁任务映射 → [Y+440]
        assert_eq!(&code[2369..2377], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2379..2386], &[0x48, 0x89, 0x81, 0xB8, 0x01, 0, 0]);
        // 唤醒 task 2（收尾阶段需要 Ready 任务可轮转）。
        assert_eq!(&code[2398..2406], &TASK2_SEND_CAP.to_le_bytes());
        assert_eq!(&code[2408..2416], &TASK2_WAKE_PAYLOAD.to_le_bytes());
        assert_eq!(&code[2448..2456], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[2456..2458], &[0xCD, 0x80]);
        // SPEC-066：自持平台 cap 映射块。
        // 正例：rdi = X 段 cap（槽 36），rsi = Task(1) 管理 cap（槽 45）。
        assert_eq!(
            &code[2460..2468],
            &crate::bootstrap::platform_segment_cap_handle(frames::SEG_TASK1_X)
                .encode()
                .to_le_bytes()
        );
        assert_eq!(
            &code[2470..2478],
            &crate::bootstrap::PLATFORM_TASK1_CAP_HANDLE
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[2490..2498], &USER_MAP_WITNESS_VA.to_le_bytes());
        assert_eq!(&code[2520..2528], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2528..2530], &[0xCD, 0x80]);
        assert_eq!(&code[2530..2537], &[0x48, 0x89, 0x81, 0xC0, 0x01, 0, 0]);
        assert_eq!(&code[2539..2547], &USER_MAP_WITNESS_VA.to_le_bytes());
        assert_eq!(&code[2547..2550], &[0x48, 0x8B, 0x02]); // mov rax, [rdx]
        assert_eq!(&code[2550..2557], &[0x48, 0x89, 0x81, 0xC8, 0x01, 0, 0]);
        // 负例：rdi = task 2 所属的 TASK2_CODE 段 cap（槽 38）。
        assert_eq!(
            &code[2559..2567],
            &crate::bootstrap::platform_segment_cap_handle(frames::SEG_TASK2_CODE)
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[2589..2597], &USER_MAP_WITNESS_VA2.to_le_bytes());
        assert_eq!(&code[2619..2627], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2629..2636], &[0x48, 0x89, 0x81, 0xD0, 0x01, 0, 0]);
        // SPEC-067：task4 自毁见证块（rdx 重设为请求块，改写 entry 与 initial_cap0）。
        assert_eq!(&code[2638..2646], &TASK_REQ_VA.to_le_bytes());
        assert_eq!(
            &code[2648..2656],
            &(USER3_CODE_VA + TASK4_CODE_OFFSET).to_le_bytes()
        );
        assert_eq!(&code[2656..2660], &[0x48, 0x89, 0x42, 0x20]);
        assert_eq!(
            &code[2662..2670],
            &crate::bootstrap::TASK4_SEND_CAP_HANDLE
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[2670..2674], &[0x48, 0x89, 0x42, 0x40]);
        assert_eq!(&code[2679..2687], &SYS_TASK_CREATE.to_le_bytes());
        assert_eq!(&code[2689..2696], &[0x48, 0x89, 0x81, 0xD8, 0x01, 0, 0]);
        assert_eq!(&code[2722..2730], &USER3_STACK_VA.to_le_bytes());
        assert_eq!(&code[2752..2760], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2762..2769], &[0x48, 0x89, 0x81, 0xE0, 0x01, 0, 0]);
        assert_eq!(&code[2771..2779], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2783..2791], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2795..2803], &TASK_RECV_BUF.to_le_bytes());
        assert_eq!(&code[2805..2813], &SYS_RECV.to_le_bytes());
        assert_eq!(&code[2815..2822], &[0x48, 0x89, 0x81, 0xE8, 0x01, 0, 0]);
        // 让出一次，等 task 4 完成自毁。
        assert_eq!(&code[2824..2832], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[2832..2834], &[0xCD, 0x80]);
        assert_eq!(&code[2860..2868], &USER3_STACK_VA.to_le_bytes());
        assert_eq!(&code[2890..2898], &SYS_MEMSEG_MAP.to_le_bytes());
        assert_eq!(&code[2900..2907], &[0x48, 0x89, 0x81, 0xF0, 0x01, 0, 0]);
        // 抢占阶段前再唤醒一次 task 2（第一次让出已把它排空）。
        assert_eq!(&code[2909..2917], &2u64.to_le_bytes());
        // SPEC-081：Namespace 块从 2979 起（`mov rcx, Y`），路径文本数据落在代码页高偏移。
        assert_eq!(&code[2979..2981], &[0x48, 0xB9]);
        assert_eq!(
            &code[NS_PATH_DATA_OFFSET + 24..NS_PATH_DATA_OFFSET + 31],
            b"mnt/sub"
        );
        // 块内有且仅有一次 `int3` 收尾，之后到路径数据区之间是零填充（不会被当作指令执行）。
        let int3 = (2979..NS_PATH_DATA_OFFSET)
            .find(|index| code[*index] == 0xCC)
            .expect("SPEC-081 块必须以 int3 收尾");
        assert!((int3 + 1..NS_PATH_DATA_OFFSET).all(|index| code[index] == 0));
    }

    #[test]
    fn task4_code_bytes_are_fixed() {
        let code = task4_code();
        assert_eq!(TASK4_CODE_LEN, 109);
        // mov rbx, rdi（初始帧携带的 self Capability 句柄）。
        assert_eq!(&code[0..3], &[0x48, 0x89, 0xFB]);
        // SYS_TASK_ID → rdx（payload = 自己的 id）。
        assert_eq!(&code[5..13], &SYS_TASK_ID.to_le_bytes());
        assert_eq!(&code[13..15], &[0xCD, 0x80]);
        assert_eq!(&code[15..18], &[0x48, 0x89, 0xC2]);
        // 接收者 = task 1；发送凭据 = 第二次 create 移入的 TASK4_SEND_CAP。
        assert_eq!(&code[20..28], &1u64.to_le_bytes());
        assert_eq!(
            &code[30..38],
            &crate::bootstrap::TASK4_SEND_CAP_HANDLE
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[70..78], &SYS_SEND.to_le_bytes());
        // 让出一次：等父任务取走消息，否则自毁会丢弃它（§8.2）。
        assert_eq!(&code[82..90], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[90..92], &[0xCD, 0x80]);
        // mov rdi, rbx（self cap）→ SYS_TASK_DESTROY（自毁）。
        assert_eq!(&code[92..95], &[0x48, 0x89, 0xDF]);
        assert_eq!(&code[97..105], &SYS_TASK_DESTROY.to_le_bytes());
        assert_eq!(&code[105..107], &[0xCD, 0x80]);
        assert_eq!(&code[107..109], &[0xEB, 0xFE]); // jmp $
    }

    #[test]
    fn task3_code_bytes_are_fixed() {
        let code = task3_code();
        assert_eq!(TASK3_CODE_LEN, 134);
        assert_eq!(&code[0..2], &[0x48, 0xB9]); // mov rcx, imm64
        assert_eq!(&code[2..10], &USER3_STACK_VA.to_le_bytes());
        assert_eq!(&code[12..20], &SYS_TASK_ID.to_le_bytes());
        assert_eq!(&code[20..22], &[0xCD, 0x80]);
        assert_eq!(&code[22..25], &[0x48, 0x89, 0x01]); // [rcx] = 自己的 id
        assert_eq!(&code[27..35], &1u64.to_le_bytes()); // receiver = 1
        assert_eq!(
            &code[37..45],
            &crate::bootstrap::TASK_CREATE_SEND_CAP_HANDLE
                .encode()
                .to_le_bytes()
        );
        assert_eq!(&code[45..48], &[0x48, 0x8B, 0x11]); // rdx = [rcx]（payload = 自己的 id）
        assert_eq!(&code[80..88], &SYS_SEND.to_le_bytes());
        assert_eq!(&code[88..90], &[0xCD, 0x80]);
        assert_eq!(&code[90..94], &[0x48, 0x89, 0x41, 0x08]);
        assert_eq!(&code[96..104], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[104..106], &[0xCD, 0x80]);
        // 仅在「销毁后仍被调度」时才写下的哨兵。
        assert_eq!(&code[108..116], &TASK3_RESCHEDULED_MAGIC.to_le_bytes());
        assert_eq!(&code[116..120], &[0x48, 0x89, 0x41, 0x10]);
        assert_eq!(&code[122..130], &SYS_YIELD.to_le_bytes());
        assert_eq!(&code[132..134], &[0xEB, 0xFE]); // jmp $
    }

    #[test]
    fn task2_code_bytes_are_fixed() {
        let code = task2_code();
        assert_eq!(TASK2_CODE_LEN, 496);
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
        // S2-D4：尾部阻塞接收循环——S2-D4 块的两次让出需要 task 2 不处于 Ready。
        assert_eq!(&code[472..474], &[0x48, 0xBF]);
        assert_eq!(&code[474..482], &USER2_RECV_BUF3.to_le_bytes());
        assert_eq!(&code[482..484], &[0x48, 0xB8]);
        assert_eq!(&code[484..492], &SYS_RECV.to_le_bytes());
        assert_eq!(&code[492..494], &[0xCD, 0x80]);
        assert_eq!(&code[494..496], &[0xEB, 0xE8]); // jmp 472
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
