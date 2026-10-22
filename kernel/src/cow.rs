//! 切片 F：真实 COW（写时复制）——写故障 → 帧字节复制 → 换映射 → 重试原指令。
//!
//! 这是冻结垂直切片 `COW/page fault → handler → exc_resume → TLB` 中 COW 本身的最小
//! 完整实现：自建一条只读映射，真实写触发 `present∧write` #PF，内核 #PF 分派器在故障
//! 现场**逐字节复制**原帧、把页表改为可写指向私有帧、`invlpg`，然后**重试原故障指令**，
//! 使其成功执行；共享帧保持未被修改。
//!
//! **范围边界：** ring0-only、单核、非抢占；仅单页/单任务；不是抽象模型的 `exc_resume`
//! 授权语义，也不是 ring3 用户态 COW 协议；COW 动作自身若故障会产生嵌套 #PF（已知边界）。
//!
//! **与 #PF handler 的接口：** `idt::page_fault_entry` 捕获完整现场后 `call` 本模块的
//! [`dispatch`]：命中 COW 时返回原 `rip` 重试；早期 ring0 探针沿用固定长度跳过；其余
//! ring3 页故障把当前 Task 标为 `Faulted` 并切走。

use crate::exc::SretFrame;
use crate::frames;
use crate::mem::{FRAME_SIZE, phys_to_virt};
use crate::vspace::{self, VspaceError};
use prima_ostd::sync::StaticCell;

/// COW 见证使用的虚拟地址：i4 槽 16。
///
/// 实测该 bootloader 的 PML4 占用槽 0..8（含内核镜像槽 2、物理直映射等）与切片 E 的槽 1，
/// 故取明显空闲且隔离的槽 16；`prepare` 仍会 `slot_free` 复核，避免盲占。
pub const COW_VA: u64 = 0x0000_0800_0000_0000;

/// 一页中的 qword 数（4096 / 8）。
pub const PATTERN_WORDS: usize = (FRAME_SIZE / 8) as usize;

/// 预填字节模式：第 `index` 个 qword（仅低位索引变化，便于逐项对账复制完整性）。
pub const fn pattern(index: u64) -> u64 {
    0xC0FF_EE00_0000_0000 | index
}

/// 一次 COW 见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CowWitness {
    pub va: u64,
    /// 只读共享帧物理地址。
    pub shared: u64,
    /// COW 后分配的私有帧物理地址。
    pub private: u64,
    /// 写故障的硬件 error code（应 0x3 = present|write）。
    pub err: u64,
    /// 写故障分类名（应 `write_protection`）。
    pub class: &'static str,
    /// 共享帧首 qword（应仍为 [`pattern`]`(0)`，证明写时复制未改原帧）。
    pub shared0: u64,
    /// 私有帧首 qword（重试成功的 store 写入值，应等于 `va`）。
    pub private0: u64,
    /// 私有帧第 1 个 qword（应等于 [`pattern`]`(1)`，证明字节已复制）。
    pub private1: u64,
    /// 私有帧末 qword（应等于 [`pattern`]`(511)`）。
    pub private_last: u64,
    /// 经 `va` 读回的值（重试后应等于 `va`）。
    pub read_back: u64,
}

impl CowWitness {
    /// 纯判定：一次 COW 见证是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.va == COW_VA
            && self.err == 0x3
            && self.class == "write_protection"
            && self.private != 0
            && self.private != self.shared
            && self.shared0 == pattern(0)
            && self.private0 == self.va
            && self.private1 == pattern(1)
            && self.private_last == pattern(PATTERN_WORDS as u64 - 1)
            && self.read_back == self.va
    }
}

/// 分派器在 COW 期间使用的上下文（单核 boot 早期，无并发）。
struct CowContext {
    root_phys: u64,
    offset: u64,
    shared: u64,
    private: u64,
    copied: bool,
    /// 可选注册的第二个 COW 区域（切片 G 的用户页 X）。`user=true` 时换映射需保留 U/S 位。
    user_va: u64,
    user_shared: u64,
    user_private: u64,
    user_copied: bool,
}

static COW_CTX: StaticCell<Option<CowContext>> = StaticCell::new(None);
static USER_COW_ERR: StaticCell<Option<u64>> = StaticCell::new(None);

#[derive(Clone, Copy)]
pub struct UserFaultRecord {
    pub task: u32,
    pub addr: u64,
    pub err: u64,
}

static USER_FAULT: StaticCell<Option<UserFaultRecord>> = StaticCell::new(None);

pub fn user_cow_err() -> Option<u64> {
    USER_COW_ERR.with(|slot| *slot)
}

pub fn user_fault_record() -> Option<UserFaultRecord> {
    USER_FAULT.with(|slot| *slot)
}

/// COW 私有帧的取帧入口（切片 R：追加进 COW `SegmentObject` 的真实帧向量）。
///
/// 帧仍经唯一取帧入口 `frames::allocate_for_segment(SEG_COW)` 取得（W6 归属与守恒不变），
/// 同时登记进 COW 段的对象视图，使该区域不再只有归属标签。
fn cow_frame() -> Result<u64, VspaceError> {
    crate::bootstrap::platform_append_frame(frames::SEG_COW).map_err(|_| VspaceError::NoFrame)
}

/// 返回 COW 上下文的 `(根页表物理地址, 物理直映射偏移)`（尚未 `prepare` 时为 `None`）。
///
/// 供 syscall 层在同一用户地址空间（bootloader CR3）上校验用户缓冲指针。
pub fn root_and_offset() -> Option<(u64, u64)> {
    COW_CTX.with(|slot| slot.as_ref().map(|ctx| (ctx.root_phys, ctx.offset)))
}

/// 切片 X：把 COW 上下文的 `root_phys` 改指**自建内核根**。
///
/// 该字段是 [`root_and_offset`] 的来源，而后者被 `SYS_TASK_CREATE` 用作新地址空间的克隆源——
/// 内核侧 COW 见证（`cow_region`）在首次切 CR3 **之前**执行、必须仍指 bootloader 根，故本函数
/// 只能在它之后调用。
pub fn rebase_root(root_phys: u64) {
    COW_CTX.with_mut(|slot| {
        if let Some(ctx) = slot.as_mut() {
            ctx.root_phys = root_phys;
        }
    });
}

/// 注册第二个 COW 区域（用户页）：写它时同样执行复制 + 换映射 + 重试。
pub fn register_user_region(va: u64, shared: u64) {
    COW_CTX.with_mut(|slot| {
        if let Some(ctx) = slot.as_mut() {
            ctx.user_va = va;
            ctx.user_shared = shared;
            ctx.user_private = 0;
            ctx.user_copied = false;
        }
    });
}

fn read_qword(va: u64) -> u64 {
    prima_ostd::mm::read_u64(va)
}

fn write_qword(va: u64, value: u64) {
    prima_ostd::mm::write_u64(va, value);
}

/// 准备 COW 见证：分配共享帧、写入可识别模式、建立只读映射，并登记 COW 区域。
///
/// 在触发写故障**之前**调用。共享帧与后续私有帧都经全局帧注册表取用（归属
/// [`frames::SEG_COW`]），不再持有 `FreeFrameAllocator` 副本，故不存在两份状态分叉。
pub fn prepare(root_phys: u64, offset: u64) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    if !vspace::slot_free(root_phys, COW_VA, map) {
        return Err(VspaceError::SlotOccupied);
    }
    // 切片 R：COW 区域建为真实 `SegmentObject`（内核持有可授权根 Capability），共享帧取自
    // 其帧向量。本区域保留**内核直映射**：页是内核私有的（`allowed_access` 为空），且
    // fault-time 换映射超出 `memseg_map` 语义。
    let (_, fields) = crate::bootstrap::platform_create_segment(
        frames::SEG_COW,
        1,
        crate::rights::Rights::EMPTY,
        offset,
    )
    .map_err(|_| VspaceError::NoFrame)?;
    let shared = fields.frame_at(0).ok_or(VspaceError::NoFrame)?;
    let shared_va = map(shared);
    for index in 0..PATTERN_WORDS {
        write_qword(shared_va + (index as u64) * 8, pattern(index as u64));
    }
    // 只读映射：写它即产生 `present∧write` #PF。
    vspace::map_4k(
        root_phys,
        COW_VA,
        map,
        || frames::allocate_table_frame().ok(),
        shared,
        false,
        false,
    )?;
    prima_ostd::arch::invlpg(COW_VA);

    COW_CTX.with_mut(|slot| {
        *slot = Some(CowContext {
            root_phys,
            offset,
            shared,
            private: 0,
            copied: false,
            user_va: 0,
            user_shared: 0,
            user_private: 0,
            user_copied: false,
        });
    });
    Ok(())
}

/// 对某个已注册 COW 区域执行一次复制 + 换映射；成功返回 `true`。
///
/// `user` 决定换映射后是否保留 U/S 位（用户页需要）。
fn cow_region(root_phys: u64, offset: u64, va: u64, shared: u64, user: bool) -> Option<u64> {
    let private = cow_frame().ok()?;

    // 逐字节（qword）复制共享帧内容到私有帧（POD read/write 形态，不依赖 rep movs）。
    let map = |phys: u64| phys_to_virt(phys, offset);
    let src = map(shared);
    let dst = map(private);
    for index in 0..PATTERN_WORDS {
        write_qword(
            dst + (index as u64) * 8,
            read_qword(src + (index as u64) * 8),
        );
    }

    // 换映射为可写并刷新，使重试的原故障指令成功。
    vspace::set_leaf(root_phys, va, map, private, true, user).ok()?;
    prima_ostd::arch::invlpg(va);
    Some(private)
}

/// #PF 分派器：由 `page_fault_entry` 在捕获现场后调用，返回 iretq 应恢复到的 RIP。
///
/// 早期 ring0 探针跳过故障指令（`rip + FAULT_SKIP_LEN`）。命中已注册 COW 区域（内核 [`COW_VA`] 或
/// 用户页 [`crate::user::USER_X_VA`]）的首次写故障时，执行复制 + 换映射 + `invlpg` 并
/// 返回原 `rip` 以**重试**。其他 ring3 页故障进入 `Faulted` 并调度后继。
///
/// # Safety
///
/// 只能由裸 #PF 入口在捕获现场后调用；`frame` 必须指向有效的现场帧。
pub extern "C" fn dispatch(frame: &SretFrame) -> u64 {
    let handled = COW_CTX.with_mut(|slot| {
        let ctx = slot.as_mut()?;
        let cr2 = crate::idt::fault_addr();
        let err = crate::idt::last_fault_err();
        let write = err & 0x2 != 0;
        if !write {
            return None;
        }
        let (root_phys, offset) = (ctx.root_phys, ctx.offset);

        if !ctx.copied && cr2 == COW_VA {
            if let Some(private) = cow_region(root_phys, offset, COW_VA, ctx.shared, false) {
                ctx.private = private;
                ctx.copied = true;
                return Some(frame.rip);
            }
        } else if write
            && frame.cs & 3 != 0
            && let Some(task) = Some(crate::sched::current())
        {
            // M17b：general user COW——任意用户写故障上带 PT_COW 的叶。
            let va = cr2 & !0xfff;
            let user_root = crate::sched::current_root().unwrap_or(root_phys);
            let segment = crate::bootstrap::task_segment_id(task);
            if let Some(segment) = segment {
                let map = |phys: u64| crate::mem::phys_to_virt(phys, offset);
                let resolved = crate::vspace::resolve_cow(user_root, va, map, || {
                    crate::frames::allocate_for_segment(segment).ok()
                });
                if resolved.is_ok() {
                    prima_ostd::arch::invlpg(va);
                    return Some(frame.rip);
                }
            }
        }
        if !ctx.user_copied && ctx.user_va != 0 && cr2 == ctx.user_va {
            let shared = ctx.user_shared;
            let va = ctx.user_va;
            // 切片 O：用户页换映射必须写到**当前任务**的根（per-task CR3），而非 boot 根。
            let user_root = crate::sched::current_root().unwrap_or(root_phys);
            if let Some(private) = cow_region(user_root, offset, va, shared, true) {
                ctx.user_private = private;
                ctx.user_copied = true;
                USER_COW_ERR.with_mut(|slot| *slot = Some(err));
                return Some(frame.rip);
            }
        }
        None
    });
    if let Some(rip) = handled {
        return rip;
    }
    if frame.cs & 3 != 0 {
        let task = crate::sched::current();
        let record = UserFaultRecord {
            task,
            addr: crate::idt::fault_addr(),
            err: crate::idt::last_fault_err(),
        };
        crate::bootstrap::fault_running_global(task).expect("faulted task transition");
        USER_FAULT.with_mut(|slot| *slot = Some(record));
        let next = crate::sched::pick_next(task).expect("fault requires a Ready successor");
        crate::sched::fault_to(next)
    }
    // 早期 ring0 页表探针的固定三字节指令仍沿用原有跳过语义。
    frame.rip + crate::idt::FAULT_SKIP_LEN
}

/// 触发一次 COW 见证并收集证据（内核/QEMU 专用）。
pub fn run() -> Result<CowWitness, &'static str> {
    if COW_CTX.with(|slot| slot.is_none()) {
        return Err("cow context not prepared");
    }

    // 真实写只读页 → #PF → 分派器执行 COW 并重试。
    crate::idt::trigger_page_fault_write(COW_VA);
    let err = crate::idt::last_fault_err();

    let (offset, shared, private, copied) = match COW_CTX.with(|slot| {
        slot.as_ref()
            .map(|ctx| (ctx.offset, ctx.shared, ctx.private, ctx.copied))
    }) {
        Some(values) => values,
        None => return Err("cow context vanished"),
    };
    if !copied || private == 0 {
        return Err("cow dispatch did not complete");
    }

    let shared_va = phys_to_virt(shared, offset);
    let private_va = phys_to_virt(private, offset);
    let last = PATTERN_WORDS - 1;
    let witness = CowWitness {
        va: COW_VA,
        shared,
        private,
        err,
        class: crate::fault::classify(err).as_str(),
        shared0: read_qword(shared_va),
        private0: read_qword(private_va),
        private1: read_qword(private_va + 8),
        private_last: read_qword(private_va + (last as u64) * 8),
        read_back: read_qword(COW_VA),
    };
    Ok(witness)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cow_va_is_slot_sixteen_low_half() {
        // 自建低半区选择子：i4 = 16，低 39 位为零，且不与切片 E 的槽 1 冲突。
        assert_eq!((COW_VA >> 39) & 0x1ff, 16);
        assert_eq!(COW_VA & ((1 << 39) - 1), 0);
        assert_ne!(COW_VA, crate::vspace::VSPACE_VA);
    }

    fn valid() -> CowWitness {
        let last = PATTERN_WORDS as u64 - 1;
        CowWitness {
            va: COW_VA,
            shared: 0x1000,
            private: 0x2000,
            err: 0x3,
            class: "write_protection",
            shared0: pattern(0),
            private0: COW_VA,
            private1: pattern(1),
            private_last: pattern(last),
            read_back: COW_VA,
        }
    }

    #[test]
    fn pattern_encodes_index_in_low_bits() {
        assert_eq!(pattern(0), 0xC0FF_EE00_0000_0000);
        assert_eq!(pattern(1), 0xC0FF_EE00_0000_0001);
        assert_eq!(pattern(511), 0xC0FF_EE00_0000_01FF);
    }

    #[test]
    fn accepts_complete_cow_evidence() {
        assert!(valid().evidence_ok());
    }

    #[test]
    fn rejects_alias_frame() {
        let mut w = valid();
        w.private = w.shared; // 未真正分配私有帧
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_shared_page_modified() {
        let mut w = valid();
        w.shared0 = COW_VA; // 直接改原帧而非复制 ⇒ 违反写时复制
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_incomplete_frame_copy() {
        let mut w = valid();
        w.private1 = 0; // 只写了首 qword，未复制其余
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_failed_retry() {
        let mut w = valid();
        w.read_back = pattern(0); // 重试未生效（仍是原内容）
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_wrong_fault_class() {
        let mut w = valid();
        w.err = 0x0; // 不是写保护故障
        w.class = "not_present";
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_wrong_va() {
        let mut w = valid();
        w.va = 0x40000;
        assert!(!w.evidence_ok());
    }
}
