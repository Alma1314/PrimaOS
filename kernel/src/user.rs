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
    EXC_PAGE_FAULT_HANDLE, EXC_SYSCALL_TRAP_HANDLE, IMAGE_CAP_HANDLE, INIT_TASK_ID,
    NAMESPACE_CAP_HANDLE, PLATFORM_TASK1_CAP_HANDLE, PLATFORM_TASK2_CAP_HANDLE,
    SERVICE_IMAGE_CAP_HANDLE, SERVICE_IMAGE_MGR_CAP_HANDLE, SERVICE3_STACK_CAP_HANDLE,
    SERVICEMGR_SEND_CAP_HANDLE, SegmentFrames, TASK_CAP_HANDLE, TASK_CREATE_ROOT_NS_RIGHTS,
    TASK_CREATE_SEND_CAP_HANDLE, TASK2_SEND_CAP_HANDLE, TASK4_SEND_CAP_HANDLE,
};
use crate::exc::SretFrame;
use crate::frames;
use crate::gdt::{USER_CS, USER_DS};
use crate::handle::Handle;
use crate::mem::{FRAME_SIZE, phys_to_virt};
use crate::rights::{Right, Rights};
use crate::sched;
use crate::syscall::{
    AUTHORIZE_CAP, EXC_KIND_PAGEFAULT, EXC_PAGE_FAULT_CAP, MEMSEG_RW_BITS, NS_PATH_DATA_OFFSET,
    RIGHT_SEGMAPR, SYS_AUTHORIZE, SYS_EXC_SUBSCRIBE, SYS_MEMSEG_MAP, SYS_RECV, SYS_SEND,
    SYS_TASK_DESTROY, SYS_TASK_ID, SYS_TRAP, SYS_YIELD, TASK2_ID, TASK2_SELF_CAP, USER2_RECV_BUF,
    USER2_RECV_BUF2, USER2_RECV_BUF3, USER2_SHARED_VA,
};
use crate::vspace::{self, VspaceError};
use prima_ostd::sync::{Once, StaticCell};

/// EXO-1：一个 boot image 条目——只暴露 opaque ELF 字节与装载布局。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageEntry {
    /// EXO-2D：manifest 条目的 opaque kind；内核只解释 [`prima_abi::boot_image::KIND_INIT`]。
    pub kind: u64,
    pub bytes: &'static [u8],
    pub pages: usize,
    pub entry: u64,
}

impl ImageEntry {
    const EMPTY: Self = Self {
        kind: 0,
        bytes: &[],
        pages: 0,
        entry: 0,
    };
}

/// EXO-2D：从通用 ramdisk 解析出的**通用 manifest 布局**。
///
/// 内核不再假定固定 8 项或服务顺序：它只按 `kind == KIND_INIT` 找 init，其余条目按
/// manifest 顺序打包为服务映像段，并在 BootInfo 中按同一顺序暴露页偏移/入口。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootLayout {
    pub count: usize,
    pub entries: [ImageEntry; prima_abi::boot_image::MAX_ENTRIES],
    pub pages_off: [usize; prima_abi::boot_image::MAX_ENTRIES],
    pub total_pages: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootImageError {
    Missing,
    TooSmall,
    BadMagic,
    BadVersion,
    BadEntryCount,
    BadEntryRange,
    InvalidElf,
    ImageSegmentTooLarge,
    TooManyEntries,
    MissingInit,
}

static BOOT_LAYOUT: StaticCell<Option<BootLayout>> = StaticCell::new(None);

fn boot_layout() -> Option<BootLayout> {
    BOOT_LAYOUT.with(|slot| *slot)
}

/// 从 boot image 头部读取第 `index` 个 little-endian qword。
fn boot_u64(bytes: &[u8], index: usize) -> Option<u64> {
    let at = index.checked_mul(8)?;
    let raw = bytes.get(at..at.checked_add(8)?)?;
    Some(u64::from_le_bytes(raw.try_into().ok()?))
}

/// EXO-1：安装 bootloader 传入的通用 ramdisk，解析 opaque ELF 块并计算映像段布局。
///
/// 内核只解释格式头部（magic/version/count/offset/len）和 ELF program header；不读取
/// 服务名、服务注册表或服务协议。ramdisk 本身由 bootloader 映射、且不属于 `Usable` 帧。
pub fn install_boot_image(ramdisk_va: u64, ramdisk_len: u64) -> Result<(), BootImageError> {
    if ramdisk_len == 0 {
        return Err(BootImageError::Missing);
    }
    let len = usize::try_from(ramdisk_len).map_err(|_| BootImageError::TooSmall)?;
    let bytes = prima_ostd::mm::bytes_at(ramdisk_va, len);
    let header_qwords = prima_abi::boot_image::HEADER_QWORDS;
    if bytes.len() < header_qwords * 8 {
        return Err(BootImageError::TooSmall);
    }
    let magic = boot_u64(bytes, 0).ok_or(BootImageError::TooSmall)?;
    let version = boot_u64(bytes, 1).ok_or(BootImageError::TooSmall)?;
    let count = boot_u64(bytes, 2).ok_or(BootImageError::TooSmall)?;
    if magic != prima_abi::boot_image::MAGIC {
        return Err(BootImageError::BadMagic);
    }
    if version != prima_abi::boot_image::VERSION {
        return Err(BootImageError::BadVersion);
    }
    let count = usize::try_from(count).map_err(|_| BootImageError::BadEntryCount)?;
    if count == 0 {
        return Err(BootImageError::BadEntryCount);
    }
    if count > prima_abi::boot_image::MAX_ENTRIES {
        return Err(BootImageError::TooManyEntries);
    }

    let entry_qwords = prima_abi::boot_image::ENTRY_QWORDS;
    let table_qwords = header_qwords + count * entry_qwords;
    if bytes.len() < table_qwords * 8 {
        return Err(BootImageError::TooSmall);
    }

    let mut entries = [ImageEntry::EMPTY; prima_abi::boot_image::MAX_ENTRIES];
    for (index, entry) in entries.iter_mut().take(count).enumerate() {
        let base = header_qwords + index * entry_qwords;
        let kind = boot_u64(bytes, base).ok_or(BootImageError::TooSmall)?;
        let offset = boot_u64(bytes, base + 1).ok_or(BootImageError::TooSmall)?;
        let entry_len = boot_u64(bytes, base + 2).ok_or(BootImageError::TooSmall)?;
        let offset = usize::try_from(offset).map_err(|_| BootImageError::BadEntryRange)?;
        let entry_len = usize::try_from(entry_len).map_err(|_| BootImageError::BadEntryRange)?;
        let end = offset
            .checked_add(entry_len)
            .ok_or(BootImageError::BadEntryRange)?;
        let image = bytes
            .get(offset..end)
            .ok_or(BootImageError::BadEntryRange)?;
        let elf = elf_layout(image).map_err(|_| BootImageError::InvalidElf)?;
        *entry = ImageEntry {
            kind,
            bytes: image,
            pages: elf.pages,
            entry: elf.entry,
        };
    }

    let mut pages_off = [0usize; prima_abi::boot_image::MAX_ENTRIES];
    let mut total_pages = 1; // 服务映像段首页留给目录。
    let mut init_seen = false;
    for (index, entry) in entries.iter().take(count).enumerate() {
        if entry.kind == prima_abi::boot_image::KIND_INIT {
            init_seen = true;
            continue;
        }
        pages_off[index] = total_pages;
        total_pages = total_pages
            .checked_add(entry.pages)
            .ok_or(BootImageError::ImageSegmentTooLarge)?;
    }
    if !init_seen {
        return Err(BootImageError::MissingInit);
    }
    if total_pages > crate::bootstrap::MAX_BOOT_SERVICE_FRAMES {
        return Err(BootImageError::ImageSegmentTooLarge);
    }

    let layout = BootLayout {
        count,
        entries,
        pages_off,
        total_pages,
    };
    BOOT_LAYOUT.with_mut(|slot| *slot = Some(layout));
    Ok(())
}

/// 按 `kind` 查找 manifest 条目；返回 `(index, entry)`。
fn boot_entry_by_kind(layout: &BootLayout, kind: u64) -> Option<(usize, ImageEntry)> {
    layout
        .entries
        .iter()
        .take(layout.count)
        .enumerate()
        .find(|(_, entry)| entry.kind == kind)
        .map(|(index, entry)| (index, *entry))
}

/// 用户空间基址：PML4 槽 20（实测槽 0..8 已被 bootloader/内核占用，槽 16 归切片 F）。
pub const USER_BASE_VA: u64 = 0x0000_0A00_0000_0000;

/// 用户代码页（只读、可执行、user）。
pub const USER_CODE_VA: u64 = USER_BASE_VA;
/// SPEC-087：task 1 的第二张代码页（用户代码页 4096→8192 B）。
///
/// `USER_STACK_VA` 紧贴 `USER_CODE_VA`（+0x1000），故第二页不连续，落在空闲槽 +0x5000；
/// 尾部追加的代码经 `jmp rel32` 从 `USER_CODE_VA + 4030` 跳到这里执行。
pub const USER_CODE2_VA: u64 = USER_BASE_VA + 0x9000;
/// 用户栈页（可写、user）。
pub const USER_STACK_VA: u64 = USER_BASE_VA + 0x1000;
/// COW 数据页 X（**只读**、user），写它触发用户态写保护 #PF。
pub const USER_X_VA: u64 = USER_BASE_VA + 0x2000;
/// 回执页 Y（可写、user），用户代码写入以证明 CPL3 执行与重试成功。
pub const USER_Y_VA: u64 = USER_BASE_VA + 0x3000;
/// Stage 4：Rust 用户客户端将 PRIM Hello 放入共享 Segment；init 经 IPC 收到 cap 后映射到此页。
pub const USER_HELLO_VA: u64 = USER_BASE_VA + 0x7000;
/// SPEC-087：第二个（存活的）Rust 客户端递交给 init 的共享 Segment 在 **init 根**内的映射 VA。
///
/// 与 `USER_HELLO_VA`（SPEC-085 首个客户端的段）分开，避免第二次 `memseg_map` 撞 `Overlap`。
pub const USER_HELLO2_VA: u64 = USER_BASE_VA + 0x8000;

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
// Rust ELF 占用 USER3_BASE_VA 起始 4 页；Task 3/4 共用的可写栈放到独立 2 MiB 区域，
// 防止入口代码 Segment 与栈映射互相覆盖。
pub const USER3_STACK_VA: u64 = USER3_BASE_VA + 0x10_0000;
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
/// SPEC-106：每任务内核栈大小。从 16 KiB 提到 **32 KiB**——S4-D3 让 `SYS_TASK_DESTROY` 的栈帧
/// 增大（其 `[0u64; MAX_VSPACE_ENTRIES]` 快照数组随 `MAX_VSPACE_ENTRIES` 扩容），16 KiB 栈**向下
/// 溢出**并覆写了相邻 BSS（实测把 `KERNEL_ROOT`/`KERNEL_IMAGE` 清零，使 `self_root_ready` 报
/// `None` → `panic` 事件）。与 `config.rs` 里启动栈 80→256 KiB 的既有取向一致。
pub const KSTACK_SIZE: usize = 32 * 1024;

/// 下界保护页；自建根不映射，不能借用或读取整个 KStack。
pub const KSTACK_GUARD_SIZE: usize = 4096;
const KSTACK_STORAGE_SIZE: usize = KSTACK_GUARD_SIZE + KSTACK_SIZE;

/// 内核栈数组（只经裸地址使用，字段从不按元素读）。
#[repr(align(4096))]
pub struct KStack(#[allow(dead_code)] [u8; KSTACK_STORAGE_SIZE]);

pub static KSTACK_TASK1: StaticCell<KStack> = StaticCell::new(KStack([0; KSTACK_STORAGE_SIZE]));
pub static KSTACK_TASK2: StaticCell<KStack> = StaticCell::new(KStack([0; KSTACK_STORAGE_SIZE]));

/// S2-D4：`task_create` 创建的 Task 的每任务内核栈（索引 = task id）。
///
/// 只覆盖 id 3..[`USER_TASK_KSTACK_COUNT`]（task 1/2 仍用上面的静态栈，故其地址与
/// `preempt_ready` 的 `rsp*`/`kstack*_top` 不变）。新增 BSS 追加在既有静态之后，既有静态
/// 的相对位置不变。
///
/// SPEC-088：SPEC-087 的存活客户端已占 id 8，本片第 4 次 create 的 Rust 服务端点拿到 id 9，
/// 故 6→7 把覆盖范围扩到 id 3..9（BSS +16 KiB，布局字段随之变化，逐条登记）。
/// SPEC-092：第 5 次 create 的**第二个**服务端点拿到 id 10，故 7→8 扩到 id 3..10。
/// SPEC-103：第 6 次 create 的 ring3 服务管理器拿到 id 11，故 8→9 扩到 id 3..11。
/// SPEC-106：S4-D3 的五个服务端点占 id 10..14，故 9→**16** 扩到 id 3..18（留 4 个余量）。
pub const USER_TASK_KSTACK_COUNT: usize = 16;

pub static KSTACK_TASKS: StaticCell<[KStack; USER_TASK_KSTACK_COUNT]> =
    StaticCell::new([const { KStack([0; KSTACK_STORAGE_SIZE]) }; USER_TASK_KSTACK_COUNT]);

/// S2-D4：task id（≥3）的内核栈顶；越界或未分配返回 `None`。
pub fn task_kstack_top(task_id: u32) -> Option<u64> {
    if task_id == 1 {
        return Some(kstack_top(KSTACK_TASK1.as_ptr()));
    }
    if task_id == 2 {
        return Some(kstack_top(KSTACK_TASK2.as_ptr()));
    }
    let index = task_id.checked_sub(3)? as usize;
    if index >= USER_TASK_KSTACK_COUNT {
        return None;
    }
    // 地址计算避免借用整个含未映射页的数组或单个栈。
    let stack = KSTACK_TASKS.as_ptr() as u64 + (index * KSTACK_STORAGE_SIZE) as u64;
    Some(stack + KSTACK_STORAGE_SIZE as u64)
}

/// 内核栈顶（one-past-end，16 对齐）。
fn kstack_top(stack: *const KStack) -> u64 {
    stack as u64 + KSTACK_STORAGE_SIZE as u64
}

/// 每个静态 Task 栈的下界页；只返回地址，不读取其内容。
pub fn task_stack_guard_page(index: usize) -> Option<u64> {
    match index {
        0 => Some(KSTACK_TASK1.as_ptr() as u64),
        1 => Some(KSTACK_TASK2.as_ptr() as u64),
        i if i < USER_TASK_KSTACK_COUNT + 2 => {
            Some(KSTACK_TASKS.as_ptr() as u64 + ((i - 2) * KSTACK_STORAGE_SIZE) as u64)
        }
        _ => None,
    }
}

pub fn is_stack_guard_page(va: u64) -> bool {
    va == prima_ostd::entry::syscall_stack_guard_page()
        || (0..USER_TASK_KSTACK_COUNT + 2).any(|i| task_stack_guard_page(i) == Some(va))
}

/// 任务 2 回执页 Y2 的物理帧（切片 O：task1 根下不可见 Y2 VA，witness 按物理读）。
static TASK1_STACK_PHYS: AtomicU64 = AtomicU64::new(0);
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
/// 阶段 5 PT_LOAD 装载后改到页内 0x800：原 0x100 落在 ELF program header 区，
/// 覆盖 `p_paddr/p_filesz/p_memsz`，使 ring3 的 PT_LOAD 解析在首个服务（alpha）上失败。
/// 0x800 位于 `.rela.dyn`，内核的相对重定位在写入本覆盖层**之前**已消费完毕。
pub const TASK4_CODE_OFFSET: u64 = 0x800;
/// 切片 V：task 4 的代码长度（自报 id → 让出 → 自毁 → 不该到达）。
///
/// **必须先让出再自毁**：§8.2 的 `task_destroy` 会丢弃 sender 为该 Task 的消息，若在同一
/// 时间片内发送后立即自毁，刚排队的 id 会被自己的销毁清理掉（实测确认），父任务将取不到。
pub const TASK4_CODE_LEN: usize = 109;
/// S2-D4：子任务若被再次调度时写入的哨兵（正常路径下销毁后不再被调度，故该值不应出现）。
pub const TASK3_RESCHEDULED_MAGIC: u64 = 0xDEAD_0000_0000_0003;

pub const SERVICE_IMAGE_MAGIC: u64 = 0x5345_5256_494D_4701;

/// 用户代码占据一个 4 KiB 页；路径文本放在独立的 Y 页。
pub const USER_CODE_LEN: usize = 8192;
/// SPEC-087：task 1 代码占用的页数（页 0 在 `USER_CODE_VA`、页 1 在 `USER_CODE2_VA`）。
pub const USER_CODE_PAGES: usize = USER_CODE_LEN / FRAME_SIZE as usize;

const NS_PATH_BYTES: &[u8] = b"lib\0sub\0mnt\0mnt/sub\0/lib\0x\0y\0";

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
    frame.r15 = prima_abi::boot_abi::BOOT_INFO_VA;
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
    create_region_pages(id, 1, access, offset)
}

/// SPEC-087：多页平台段（task 1 的代码段为 2 页）。
fn create_region_pages(
    id: u32,
    pages: usize,
    access: Rights,
    offset: u64,
) -> Result<(Handle, SegmentFrames), VspaceError> {
    crate::bootstrap::platform_create_segment(id, pages, access, offset)
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
    map_region_pages(
        seg_cap,
        task_cap,
        0,
        target_base,
        1,
        access,
        root_phys,
        offset,
    )
}

/// SPEC-087：从段内 `seg_offset` 起映射 `pages` 页到 `target_base`（可指定非零段偏移与非连续 VA）。
#[allow(clippy::too_many_arguments)]
fn map_region_pages(
    seg_cap: Handle,
    task_cap: Handle,
    seg_offset: u64,
    target_base: u64,
    pages: usize,
    access: Rights,
    root_phys: u64,
    offset: u64,
) -> Result<(), VspaceError> {
    crate::bootstrap::memseg_map(
        crate::bootstrap::KERNEL_ACTOR,
        seg_cap,
        task_cap,
        seg_offset,
        target_base,
        pages,
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
static KERNEL_ROOT: Once<KernelRootInfo> = Once::new();

/// 小型描述整体发布；页表本体仍通过既有映射/帧归属入口管理。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KernelRootInfo {
    root: u64,
    image_va: u64,
    image_pages: usize,
}

/// 镜像所在的 PML4 槽（`kernel_image_offset = 0x10000000000`）——自建根的镜像链入口。
pub const IMAGE_SLOT: usize = 2;

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
    /// 再次构建返回同一根/描述，且空闲帧计数不变。
    pub published_once: bool,
}

/// 采集自建 CR3 见证；自建根或 COW 上下文尚未建立时返回 `None`。
pub fn self_root_witness() -> Option<SelfRootWitness> {
    let info = *KERNEL_ROOT.get()?;
    let KernelRootInfo {
        root: kernel_root,
        image_va,
        image_pages,
    } = info;
    // 全量 W6 审计已由 frames_ready 执行，此处只需常数时间的分配计数。
    let before = crate::frames::free_frames().ok()?;
    // 已发布才执行：全零参数若误入构造路径，本见证应失败。
    let repeated = build_kernel_root(0, 0, 0, 0, 0, 0, 0);
    let published_once = repeated == Ok(kernel_root)
        && KERNEL_ROOT.get() == Some(&info)
        && crate::frames::free_frames().ok() == Some(before);
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
        published_once,
    })
}

/// 已构造的自建内核根（未构造时为 `None`）。
pub fn kernel_root() -> Option<u64> {
    KERNEL_ROOT.get().map(|info| info.root)
}

/// 切片 X：构造自建内核根并登记，供随后所有任务根克隆。
///
/// 复刻三段：内核镜像（`image_va`/`image_phys`/`image_pages`）、物理直映射
/// （`offset` → `[0, max_phys)`）、启动栈（`stack_va`/`stack_pages`）。启动栈的**物理地址
/// 逐页从 bootloader 根解析**——它自己的映射不在直映射区内，且可能是大页，故不能用
/// [`crate::cpu::translate`]（只认 4 KiB 叶）。
///
/// 用户槽不在本根中建立：任务根克隆后再各自加载自己的用户页（同 VA 不同帧的隔离不变）。
/// 首次成功整体发布根/镜像描述；之后返回原根，忽略后续构造参数且不分配帧。
/// 首次失败不发布，可重试；普通 Err 沿用底层页表回滚，Once 不回滚闭包外部副作用。
pub fn build_kernel_root(
    boot_root: u64,
    offset: u64,
    image_va: u64,
    image_pages: usize,
    max_phys: u64,
    stack_va: u64,
    stack_pages: usize,
) -> Result<u64, VspaceError> {
    KERNEL_ROOT
        .get_or_try_init(|| {
            let map = |phys: u64| phys_to_virt(phys, offset);
            let stack_phys = |index: usize| {
                vspace::leaf_phys_or_none(boot_root, stack_va + (index as u64) * FRAME_SIZE, map)
            };
            let direct_pages = max_phys.div_ceil(FRAME_SIZE) as usize;
            let image_phys = |index: usize| {
                let va = image_va + (index as u64) * FRAME_SIZE;
                if is_stack_guard_page(va) {
                    None
                } else {
                    vspace::leaf_phys_or_none(boot_root, va, map)
                }
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
            Ok(KernelRootInfo {
                root,
                image_va,
                image_pages,
            })
        })
        .map(|info| info.root)
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
    let layout = boot_layout().ok_or(VspaceError::NoBootImage)?;
    let map = |phys: u64| phys_to_virt(phys, offset);
    let task_root = build_task_root(offset)?;
    for va in [
        USER_CODE_VA,
        USER_CODE2_VA,
        USER_STACK_VA,
        USER_X_VA,
        USER_Y_VA,
        USER_HELLO2_VA,
    ] {
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
    let (code_cap, code_frames) =
        create_region_pages(frames::SEG_TASK1_CODE, USER_CODE_PAGES, code_access, offset)?;
    let (stack_cap, stack_frames) = create_region(frames::SEG_TASK1_STACK, stack_access, offset)?;
    let (x_cap, x_frames) = create_region(frames::SEG_TASK1_X, x_access, offset)?;
    let (y_cap, y_frames) = create_region(frames::SEG_TASK1_Y, y_access, offset)?;
    let stack = region_frame(&stack_frames)?;
    let x = region_frame(&x_frames)?;
    let y = region_frame(&y_frames)?;
    TASK1_STACK_PHYS.store(stack, Ordering::Relaxed);

    // EXO-2D：init 代码按 `kind == KIND_INIT` 从 manifest 中定位；其余服务条目
    // 不进入 task 1 代码段。
    let (_, init) = boot_entry_by_kind(&layout, prima_abi::boot_image::KIND_INIT)
        .ok_or(VspaceError::NoBootImage)?;
    load_runtime_elf(
        init.bytes,
        &code_frames,
        0,
        offset,
        USER_CODE_VA,
        USER_CODE_PAGES,
        init.entry,
    )?;
    for index in 0..(FRAME_SIZE / 8) {
        write_qword(map(x) + index * 8, crate::cow::pattern(index));
    }
    // 栈与 Y 置零（Y 由用户代码写入回执）。
    for index in 0..(FRAME_SIZE / 8) {
        write_qword(map(stack) + index * 8, 0);
        write_qword(map(y) + index * 8, 0);
    }
    for (index, &byte) in NS_PATH_BYTES.iter().enumerate() {
        prima_ostd::mm::write_u8_at(map(y), (NS_PATH_DATA_OFFSET + index) as u64, byte);
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
    // SPEC-087：代码段页 1 → `USER_CODE2_VA`（不连续，避开紧邻 `USER_CODE_VA` 的用户栈）。
    map_region_pages(
        code_cap,
        task_cap,
        1,
        USER_CODE2_VA,
        1,
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
    let layout = boot_layout().ok_or(VspaceError::NoBootImage)?;
    let images = crate::bootstrap::prepare_task_create_caps(layout.total_pages, offset)
        .map_err(|_| VspaceError::NoFrame)?;
    // EXO-2D：所有非 init 条目按 manifest 顺序通用打包；内核不解释服务 kind/顺序。
    let mut scratch_pages_off = None;
    for (index, entry) in layout.entries.iter().take(layout.count).enumerate() {
        if entry.kind == prima_abi::boot_image::KIND_INIT {
            continue;
        }
        let page_base = layout.pages_off[index];
        scratch_pages_off.get_or_insert(page_base);
        load_runtime_elf(
            entry.bytes,
            &images.image,
            page_base,
            offset,
            USER3_CODE_VA,
            entry.pages,
            entry.entry,
        )?;
    }
    // 段首页写入通用目录（kind / page offset / page count）。
    write_image_directory(&layout, &images.image, offset)?;
    let frame = images
        .image
        .frame_at(scratch_pages_off.ok_or(VspaceError::NoFrame)?)
        .ok_or(VspaceError::NoFrame)?;
    // 切片 V：task 4 的旧机器码见证复用首个服务 ELF 首页的空闲数据区，不改变阶段 2 的动态 task 流程。
    write_code(frame, offset, TASK4_CODE_OFFSET, &task4_code());
    Ok(())
}

/// EXO-2D：把通用 boot entry ABI 写入 init 用户栈页的低地址区。
///
/// 内核只写入 manifest 顺序、页偏移、入口和 boot capability 句柄；服务语义不在内核。
/// `task1_frame.r15` 指向该块，`prima-initgen` 生成的 init 代码通过 `[r15+disp]` 读取。
pub fn install_boot_info(offset: u64) -> Result<(), VspaceError> {
    let layout = boot_layout().ok_or(VspaceError::NoBootImage)?;
    let stack_phys = TASK1_STACK_PHYS.load(Ordering::Relaxed);
    if stack_phys == 0 {
        return Err(VspaceError::NoFrame);
    }
    let base =
        phys_to_virt(stack_phys, offset) + (prima_abi::boot_abi::BOOT_INFO_VA - USER_STACK_VA);
    for index in 0..prima_abi::boot_abi::AREA_LEN {
        prima_ostd::mm::write_u8_at(base, index as u64, 0);
    }
    let write_u64 = |at: u32, value: u64| {
        for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
            prima_ostd::mm::write_u8_at(base, (at as usize + index) as u64, byte);
        }
    };

    write_u64(prima_abi::boot_abi::OFF_MAGIC, prima_abi::boot_abi::MAGIC);
    write_u64(
        prima_abi::boot_abi::OFF_VERSION,
        prima_abi::boot_abi::VERSION,
    );
    write_u64(prima_abi::boot_abi::OFF_ENTRY_COUNT, layout.count as u64);

    for (index, entry) in layout.entries.iter().take(layout.count).enumerate() {
        let entry_va = if entry.kind == prima_abi::boot_image::KIND_INIT {
            USER_CODE_VA + entry.entry
        } else {
            USER3_CODE_VA + entry.entry
        };
        let code_end_va = if entry.kind == prima_abi::boot_image::KIND_INIT {
            USER_CODE_VA + (entry.pages as u64) * FRAME_SIZE
        } else {
            USER3_CODE_VA + (entry.pages as u64) * FRAME_SIZE
        };
        let field = |field: u32| prima_abi::boot_abi::entry_field(index, field);
        write_u64(field(prima_abi::boot_abi::ENTRY_OFF_KIND), entry.kind);
        write_u64(
            field(prima_abi::boot_abi::ENTRY_OFF_OFFSET_PAGES),
            layout.pages_off[index] as u64,
        );
        write_u64(
            field(prima_abi::boot_abi::ENTRY_OFF_PAGES),
            entry.pages as u64,
        );
        write_u64(field(prima_abi::boot_abi::ENTRY_OFF_ENTRY_VA), entry_va);
        write_u64(
            field(prima_abi::boot_abi::ENTRY_OFF_CODE_END_VA),
            code_end_va,
        );
    }

    write_u64(
        prima_abi::boot_abi::OFF_IMAGE_CAP,
        IMAGE_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_NAMESPACE_CAP,
        NAMESPACE_CAP_HANDLE.encode(),
    );
    write_u64(prima_abi::boot_abi::OFF_TASK_CAP, TASK_CAP_HANDLE.encode());
    write_u64(
        prima_abi::boot_abi::OFF_PLATFORM_TASK1_CAP,
        PLATFORM_TASK1_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_TASK2_SEND_CAP,
        TASK2_SEND_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_SERVICE_IMAGE_CAP,
        SERVICE_IMAGE_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_SERVICE_IMAGE_MGR_CAP,
        SERVICE_IMAGE_MGR_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_SERVICEMGR_SEND_CAP,
        SERVICEMGR_SEND_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_SERVICE3_STACK_CAP,
        SERVICE3_STACK_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_TASK_CREATE_SEND_CAP,
        TASK_CREATE_SEND_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_TASK4_SEND_CAP,
        TASK4_SEND_CAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_EXC_PAGE_FAULT_CAP,
        EXC_PAGE_FAULT_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_EXC_SYSCALL_TRAP_CAP,
        EXC_SYSCALL_TRAP_HANDLE.encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_PLATFORM_TASK1_X_CAP,
        crate::bootstrap::platform_segment_cap_handle(frames::SEG_TASK1_X).encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_PLATFORM_TASK2_CODE_CAP,
        crate::bootstrap::platform_segment_cap_handle(frames::SEG_TASK2_CODE).encode(),
    );
    write_u64(
        prima_abi::boot_abi::OFF_TASK_CREATE_ROOT_NS_RIGHTS,
        TASK_CREATE_ROOT_NS_RIGHTS.bits(),
    );
    Ok(())
}

const ELF_HEADER_SIZE: usize = 64;
const ELF_PHDR_SIZE: usize = 56;
const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const DT_RELA: i64 = 7;
const DT_RELASZ: i64 = 8;
const DT_RELAENT: i64 = 9;
const R_X86_64_RELATIVE: u32 = 8;

#[derive(Clone, Copy)]
struct ElfSegment {
    kind: u32,
    offset: u64,
    vaddr: u64,
    filesz: u64,
    memsz: u64,
}

#[derive(Clone, Copy)]
struct ElfLayout {
    entry: u64,
    pages: usize,
    phoff: usize,
    phnum: usize,
}

fn image_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn image_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
    ))
}

fn image_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
    ))
}

fn elf_layout(bytes: &[u8]) -> Result<ElfLayout, VspaceError> {
    if bytes.len() < ELF_HEADER_SIZE
        || bytes.get(0..4) != Some(b"\x7fELF")
        || bytes[4] != 2
        || bytes[5] != 1
        || image_u16(bytes, 16) != Some(3)
        || image_u16(bytes, 18) != Some(62)
        || image_u32(bytes, 20) != Some(1)
        || image_u16(bytes, 54) != Some(ELF_PHDR_SIZE as u16)
    {
        return Err(VspaceError::InvalidImage);
    }
    let entry = image_u64(bytes, 24).ok_or(VspaceError::InvalidImage)?;
    let phoff = usize::try_from(image_u64(bytes, 32).ok_or(VspaceError::InvalidImage)?)
        .map_err(|_| VspaceError::InvalidImage)?;
    let phnum = image_u16(bytes, 56).ok_or(VspaceError::InvalidImage)? as usize;
    let phbytes = phnum
        .checked_mul(ELF_PHDR_SIZE)
        .and_then(|size| phoff.checked_add(size))
        .ok_or(VspaceError::InvalidImage)?;
    if phnum == 0 || phbytes > bytes.len() {
        return Err(VspaceError::InvalidImage);
    }

    let mut max_end = 0u64;
    let mut entry_loaded = false;
    for index in 0..phnum {
        let segment = elf_segment(bytes, phoff, index)?;
        if segment.kind != PT_LOAD {
            continue;
        }
        let end = segment
            .vaddr
            .checked_add(segment.memsz)
            .ok_or(VspaceError::InvalidImage)?;
        let file_end = segment
            .offset
            .checked_add(segment.filesz)
            .ok_or(VspaceError::InvalidImage)?;
        if segment.memsz < segment.filesz
            || usize::try_from(file_end).map_or(true, |end| end > bytes.len())
            || segment.memsz == 0
        {
            return Err(VspaceError::InvalidImage);
        }
        max_end = max_end.max(end);
        entry_loaded |= entry >= segment.vaddr && entry < end;
    }
    let pages =
        usize::try_from(max_end.div_ceil(FRAME_SIZE)).map_err(|_| VspaceError::InvalidImage)?;
    if !entry_loaded || pages == 0 {
        return Err(VspaceError::InvalidImage);
    }
    Ok(ElfLayout {
        entry,
        pages,
        phoff,
        phnum,
    })
}

fn elf_segment(bytes: &[u8], phoff: usize, index: usize) -> Result<ElfSegment, VspaceError> {
    let at = phoff
        .checked_add(
            index
                .checked_mul(ELF_PHDR_SIZE)
                .ok_or(VspaceError::InvalidImage)?,
        )
        .ok_or(VspaceError::InvalidImage)?;
    Ok(ElfSegment {
        kind: image_u32(bytes, at).ok_or(VspaceError::InvalidImage)?,
        offset: image_u64(bytes, at + 8).ok_or(VspaceError::InvalidImage)?,
        vaddr: image_u64(bytes, at + 16).ok_or(VspaceError::InvalidImage)?,
        filesz: image_u64(bytes, at + 32).ok_or(VspaceError::InvalidImage)?,
        memsz: image_u64(bytes, at + 40).ok_or(VspaceError::InvalidImage)?,
    })
}

fn load_runtime_elf(
    bytes: &[u8],
    frames: &SegmentFrames,
    page_base: usize,
    offset: u64,
    load_base: u64,
    expected_pages: usize,
    expected_entry: u64,
) -> Result<(), VspaceError> {
    let layout = elf_layout(bytes)?;
    if page_base + layout.pages > frames.frame_count()
        || layout.entry != expected_entry
        || layout.pages != expected_pages
    {
        return Err(VspaceError::InvalidImage);
    }
    let map = |phys| phys_to_virt(phys, offset);
    for index in 0..layout.pages {
        let frame = frames
            .frame_at(page_base + index)
            .ok_or(VspaceError::NoFrame)?;
        for byte in 0..FRAME_SIZE {
            prima_ostd::mm::write_u8_at(map(frame), byte, 0);
        }
    }

    let mut dynamic = None;
    for index in 0..layout.phnum {
        let segment = elf_segment(bytes, layout.phoff, index)?;
        if segment.kind == PT_LOAD {
            for index in 0..segment.filesz {
                let source = usize::try_from(segment.offset + index)
                    .map_err(|_| VspaceError::InvalidImage)?;
                let address = segment.vaddr + index;
                write_image_byte(
                    frames,
                    page_base,
                    map,
                    address,
                    *bytes.get(source).ok_or(VspaceError::InvalidImage)?,
                )?;
            }
        } else if segment.kind == PT_DYNAMIC {
            dynamic = Some(segment);
        }
    }
    apply_relative_relocations(bytes, frames, page_base, map, dynamic, layout, load_base)?;
    Ok(())
}

/// EXO-2A：把服务映像目录写进映像段首页。布局（little-endian u64）：
/// `magic(0..8) | count(8..16) | [offset_pages(0..8) | page_count(8..16)] × count`。
///
/// 目录不再携带服务名；内核只按 boot image 的固定顺序写 opaque 条目，名字由 ring3
/// `prima-servicemgr` 的 `SERVICE_TABLE` 持有。这样“服务叫什么”不再进入内核的目录生成路径。
fn write_image_directory(
    layout: &BootLayout,
    frames: &SegmentFrames,
    offset: u64,
) -> Result<(), VspaceError> {
    let map = |phys| phys_to_virt(phys, offset);
    let dir_frame = frames.frame_at(0).ok_or(VspaceError::NoFrame)?;
    let base = map(dir_frame);
    for byte in 0..FRAME_SIZE {
        prima_ostd::mm::write_u8_at(base, byte, 0);
    }
    // EXO-2D：目录项为 `kind | offset_pages | page_count`；内核只按 manifest 顺序透传。
    let entry_count = layout
        .entries
        .iter()
        .take(layout.count)
        .filter(|entry| entry.kind != prima_abi::boot_image::KIND_INIT)
        .count();
    let mut at = 0usize;
    write_dir_u64(base, &mut at, prima_abi::boot_image::SERVICE_IMAGE_MAGIC);
    write_dir_u64(base, &mut at, entry_count as u64);
    for (index, entry) in layout.entries.iter().take(layout.count).enumerate() {
        if entry.kind == prima_abi::boot_image::KIND_INIT {
            continue;
        }
        write_dir_u64(base, &mut at, entry.kind);
        write_dir_u64(base, &mut at, layout.pages_off[index] as u64);
        write_dir_u64(base, &mut at, entry.pages as u64);
    }
    Ok(())
}

/// SPEC-109：目录页内写一个 little-endian u64 并推进游标。
fn write_dir_u64(base: u64, at: &mut usize, value: u64) {
    for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
        prima_ostd::mm::write_u8_at(base, (*at + index) as u64, byte);
    }
    *at += 8;
}

fn write_image_byte(
    frames: &SegmentFrames,
    page_base: usize,
    map: impl Fn(u64) -> u64,
    address: u64,
    value: u8,
) -> Result<(), VspaceError> {
    let page =
        page_base + usize::try_from(address / FRAME_SIZE).map_err(|_| VspaceError::InvalidImage)?;
    let frame = frames.frame_at(page).ok_or(VspaceError::InvalidImage)?;
    prima_ostd::mm::write_u8_at(map(frame), address % FRAME_SIZE, value);
    Ok(())
}

fn write_image_u64(
    frames: &SegmentFrames,
    page_base: usize,
    map: impl Fn(u64) -> u64,
    address: u64,
    value: u64,
) -> Result<(), VspaceError> {
    if !address.is_multiple_of(8) || address % FRAME_SIZE > FRAME_SIZE - 8 {
        return Err(VspaceError::InvalidImage);
    }
    for (index, byte) in value.to_le_bytes().into_iter().enumerate() {
        write_image_byte(frames, page_base, &map, address + index as u64, byte)?;
    }
    Ok(())
}

fn apply_relative_relocations(
    bytes: &[u8],
    frames: &SegmentFrames,
    page_base: usize,
    map: impl Fn(u64) -> u64,
    dynamic: Option<ElfSegment>,
    layout: ElfLayout,
    load_base: u64,
) -> Result<(), VspaceError> {
    let Some(dynamic) = dynamic else {
        return Ok(());
    };
    let mut rela_vaddr = None;
    let mut rela_size = None;
    let mut rela_entry_size = None;
    let dynamic_size = usize::try_from(dynamic.filesz).map_err(|_| VspaceError::InvalidImage)?;
    let dynamic_offset = usize::try_from(dynamic.offset).map_err(|_| VspaceError::InvalidImage)?;
    if !dynamic_size.is_multiple_of(16) {
        return Err(VspaceError::InvalidImage);
    }
    for entry in (0..dynamic_size).step_by(16) {
        let at = dynamic_offset + entry;
        let tag = image_u64(bytes, at).ok_or(VspaceError::InvalidImage)? as i64;
        let value = image_u64(bytes, at + 8).ok_or(VspaceError::InvalidImage)?;
        match tag {
            DT_RELA => rela_vaddr = Some(value),
            DT_RELASZ => rela_size = Some(value),
            DT_RELAENT => rela_entry_size = Some(value),
            0 => break,
            _ => {}
        }
    }
    let (Some(rela_vaddr), Some(rela_size), Some(entry_size)) =
        (rela_vaddr, rela_size, rela_entry_size)
    else {
        return Ok(());
    };
    if entry_size != 24 || !rela_size.is_multiple_of(entry_size) {
        return Err(VspaceError::InvalidImage);
    }
    let mut rela_offset = None;
    for index in 0..layout.phnum {
        let segment = elf_segment(bytes, layout.phoff, index)?;
        if segment.kind == PT_LOAD
            && rela_vaddr >= segment.vaddr
            && rela_vaddr
                .checked_add(rela_size)
                .is_some_and(|end| end <= segment.vaddr + segment.filesz)
        {
            rela_offset = Some(segment.offset + (rela_vaddr - segment.vaddr));
            break;
        }
    }
    let rela_offset = usize::try_from(rela_offset.ok_or(VspaceError::InvalidImage)?)
        .map_err(|_| VspaceError::InvalidImage)?;
    let count = usize::try_from(rela_size / entry_size).map_err(|_| VspaceError::InvalidImage)?;
    for index in 0..count {
        let at = rela_offset + index * 24;
        let target = image_u64(bytes, at).ok_or(VspaceError::InvalidImage)?;
        let info = image_u64(bytes, at + 8).ok_or(VspaceError::InvalidImage)?;
        let addend = i64::from_le_bytes(
            bytes
                .get(at + 16..at + 24)
                .ok_or(VspaceError::InvalidImage)?
                .try_into()
                .map_err(|_| VspaceError::InvalidImage)?,
        );
        if info as u32 != R_X86_64_RELATIVE || info >> 32 != 0 || addend < 0 {
            return Err(VspaceError::InvalidImage);
        }
        let value = load_base
            .checked_add(addend as u64)
            .ok_or(VspaceError::InvalidImage)?;
        if target >= layout.pages as u64 * FRAME_SIZE {
            return Err(VspaceError::InvalidImage);
        }
        write_image_u64(frames, page_base, &map, target, value)?;
    }
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
    initial_caps: [u64; 2],
    name: u64,
) -> Result<(), VspaceError> {
    let kstack = task_kstack_top(task_id).ok_or(VspaceError::BadRoot)?;
    let mut frame = SretFrame::ZERO;
    frame.rip = entry;
    frame.rsp = stack_top;
    // 切片 V：self Capability 的句柄值写入初始帧 `rdi`——子任务据此才能调用
    // `SYS_TASK_DESTROY` 自毁。这是细化层自由度（抽象模型不规定寄存器传参），
    // 不新增槽位、不改句柄分配。
    frame.rdi = self_cap;
    frame.rsi = initial_caps[0];
    frame.rdx = initial_caps[1];
    // SPEC-101：服务名的运行期指派——写入初始帧 `rcx`（第 4 个入口参数）。端点在编译期不含
    // 任何服务名字面量，名字由服务管理器在 `task_create` 时给出，故同一映像可按名重用/改名。
    frame.rcx = name;
    // SPEC-104：**对端 id 的运行期指派**——写入初始帧 `r8`（第 5 个入口参数）：`initial_caps[0]`
    // 所指向的 task id（若它不是 Task cap 则为 0）。`send` 的接收者须等于发送凭据的目标
    //（`syscall::send_call`），故持有发送凭据的子任务据此才能**寻址**其设计对端——`init` 建
    // 任务时凭据指向谁，子任务的入口 `r8` 就是谁。SPEC-104 前 `initial_caps[0]` 皆指向 task 1，
    // 故本片不改变任何行为（事件逐字节冻结）。
    frame.r8 = crate::bootstrap::decode_handle(initial_caps[0])
        .ok()
        .and_then(crate::bootstrap::task_of_cap)
        .map_or(0, |id| id as u64);
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    frame.rflags = 0x2 | (1 << 9); // bit1 固定 + IF=1
    crate::sched::set_root(task_id, root_phys);
    crate::sched::set_kstack_top(task_id, kstack);
    crate::sched::set_frame(task_id, frame);
    Ok(())
}

/// M15b-2：登记用户态线程（共享 root；独立内核栈/FS base/初始帧）。
pub fn register_thread_task(
    task_id: u32,
    root_phys: u64,
    entry: u64,
    stack_top: u64,
    self_cap: u64,
    fs_base: u64,
) -> Result<(), VspaceError> {
    let kstack = task_kstack_top(task_id).ok_or(VspaceError::BadRoot)?;
    let mut frame = SretFrame::ZERO;
    frame.rip = entry;
    frame.rsp = stack_top;
    frame.rdi = self_cap;
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    frame.rflags = 0x2 | (1 << 9);
    crate::sched::set_root(task_id, root_phys);
    crate::sched::set_kstack_top(task_id, kstack);
    crate::sched::set_frame(task_id, frame);
    crate::sched::set_fs_base(task_id, fs_base);
    Ok(())
}

/// M16b：登记 clone 线程（复制父帧、rax=0、rsp=stack、可选 FS base）。
pub fn register_clone_task(
    task_id: u32,
    root_phys: u64,
    parent_frame: SretFrame,
    stack_top: u64,
    fs_base: u64,
) -> Result<(), VspaceError> {
    let kstack = task_kstack_top(task_id).ok_or(VspaceError::BadRoot)?;
    let mut frame = parent_frame;
    frame.rax = 0;
    frame.rsp = stack_top;
    frame.cs = USER_CS as u64;
    frame.ss = USER_DS as u64;
    crate::sched::set_root(task_id, root_phys);
    crate::sched::set_kstack_top(task_id, kstack);
    crate::sched::set_frame(task_id, frame);
    crate::sched::set_fs_base(task_id, fs_base);
    crate::sched::inherit_current_fpu(task_id);
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

    /// EXO-1：构造一个最小合法 ELF，验证 ramdisk 解析器只看 program header。
    fn minimal_elf(entry: u64, memsz: u64) -> Vec<u8> {
        let mut bytes = vec![0u8; 128];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[16..18].copy_from_slice(&3u16.to_le_bytes());
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[20..24].copy_from_slice(&1u32.to_le_bytes());
        bytes[24..32].copy_from_slice(&entry.to_le_bytes());
        bytes[32..40].copy_from_slice(&64u64.to_le_bytes());
        bytes[54..56].copy_from_slice(&(ELF_PHDR_SIZE as u16).to_le_bytes());
        bytes[56..58].copy_from_slice(&1u16.to_le_bytes());
        bytes[64..68].copy_from_slice(&PT_LOAD.to_le_bytes());
        bytes[72..80].copy_from_slice(&120u64.to_le_bytes());
        bytes[80..88].copy_from_slice(&0u64.to_le_bytes());
        bytes[96..104].copy_from_slice(&8u64.to_le_bytes());
        bytes[104..112].copy_from_slice(&memsz.to_le_bytes());
        bytes[120..128].copy_from_slice(&0x5A5A_5A5A_5A5A_5A5Au64.to_le_bytes());
        bytes
    }

    #[test]
    fn every_task_stack_has_a_distinct_page_aligned_lower_guard() {
        assert_eq!(core::mem::align_of::<KStack>(), 4096);
        assert_eq!(
            core::mem::size_of::<KStack>(),
            KSTACK_GUARD_SIZE + KSTACK_SIZE
        );
        for index in 0..USER_TASK_KSTACK_COUNT + 2 {
            let page = task_stack_guard_page(index).unwrap();
            assert_eq!(page & 0xfff, 0);
            assert!(is_stack_guard_page(page));
            assert!(!is_stack_guard_page(page + KSTACK_GUARD_SIZE as u64));
            for other in 0..index {
                assert_ne!(Some(page), task_stack_guard_page(other));
            }
            if index >= 2 {
                assert_eq!(
                    task_kstack_top(index as u32 + 1),
                    Some(page + KSTACK_STORAGE_SIZE as u64)
                );
            }
        }
        assert_eq!(task_stack_guard_page(USER_TASK_KSTACK_COUNT + 2), None);
    }

    #[test]
    fn runtime_elf_matches_loader_layout() {
        let bytes = minimal_elf(0x10, FRAME_SIZE * 3);
        let layout = elf_layout(&bytes).unwrap();
        assert_eq!(layout.entry, 0x10);
        assert_eq!(layout.pages, 3);
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
