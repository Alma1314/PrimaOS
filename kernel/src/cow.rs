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
//! [`dispatch`]，由它返回"iretq 应回到的 RIP"——默认 `rip + FAULT_SKIP_LEN`（跳过，保持
//! 切片 C/E 的探测行为），命中 COW 条件时返回原 `rip` 以**重试**故障指令。

use crate::exc::SretFrame;
use crate::mem::{FRAME_SIZE, FreeFrameAllocator, phys_to_virt};
use crate::vspace::{self, VspaceError};

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
    alloc: FreeFrameAllocator,
    shared: u64,
    private: u64,
    copied: bool,
}

static mut COW_CTX: Option<CowContext> = None;

fn read_qword(va: u64) -> u64 {
    unsafe { core::ptr::read_volatile(va as *const u64) }
}

fn write_qword(va: u64, value: u64) {
    unsafe { core::ptr::write_volatile(va as *mut u64, value) };
}

/// 准备 COW 见证：分配共享帧、写入可识别模式、建立只读映射，并登记后续私有帧分配器。
///
/// 在触发写故障**之前**调用。`alloc` 被移动进上下文（`FreeFrameAllocator` 为 `Copy`），
/// 调用后不应再使用同一 allocator 做其它分配，以免两侧状态分叉。
pub fn prepare(
    alloc: &mut FreeFrameAllocator,
    root_phys: u64,
    offset: u64,
) -> Result<(), VspaceError> {
    let map = |phys: u64| phys_to_virt(phys, offset);
    if !vspace::slot_free(root_phys, COW_VA, map) {
        return Err(VspaceError::SlotOccupied);
    }

    let shared = alloc.allocate().map_err(|_| VspaceError::NoFrame)?;
    let shared_va = map(shared);
    for index in 0..PATTERN_WORDS {
        write_qword(shared_va + (index as u64) * 8, pattern(index as u64));
    }
    // 只读映射：写它即产生 `present∧write` #PF。
    vspace::map_4k(
        root_phys,
        COW_VA,
        map,
        || alloc.allocate().ok(),
        shared,
        false,
        false,
    )?;
    unsafe { vspace::invlpg(COW_VA) };

    unsafe {
        let ctx = core::ptr::addr_of_mut!(COW_CTX);
        *ctx = Some(CowContext {
            root_phys,
            offset,
            alloc: *alloc,
            shared,
            private: 0,
            copied: false,
        });
    }
    Ok(())
}

/// #PF 分派器：由 `page_fault_entry` 在捕获现场后调用，返回 iretq 应恢复到的 RIP。
///
/// 默认跳过故障指令（`rip + FAULT_SKIP_LEN`）。仅当这是首次对 [`COW_VA`] 的
/// `present∧write` 写故障时，执行真正的 COW（分配 + 逐字节复制 + 换映射 + `invlpg`）
/// 并返回原 `rip` 以重试。
///
/// # Safety
///
/// 只能由裸 #PF 入口在捕获现场后调用；`frame` 必须指向有效的现场帧。
pub extern "C" fn dispatch(frame: &SretFrame) -> u64 {
    unsafe {
        let ctx_ptr = core::ptr::addr_of_mut!(COW_CTX);
        let Some(ctx) = (*ctx_ptr).as_mut() else {
            return frame.rip + crate::idt::FAULT_SKIP_LEN;
        };
        let (root_phys, offset, shared, copied) =
            (ctx.root_phys, ctx.offset, ctx.shared, ctx.copied);
        let cr2 = crate::idt::fault_addr();
        let err = crate::idt::last_fault_err();
        if copied || cr2 != COW_VA || err != 0x3 {
            return frame.rip + crate::idt::FAULT_SKIP_LEN;
        }

        let Ok(private) = ctx.alloc.allocate() else {
            return frame.rip + crate::idt::FAULT_SKIP_LEN;
        };

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

        // 换映射为可写并刷新，然后重试原故障指令。
        if vspace::set_leaf(root_phys, COW_VA, map, private, true, false).is_err() {
            return frame.rip + crate::idt::FAULT_SKIP_LEN;
        }
        vspace::invlpg(COW_VA);

        ctx.private = private;
        ctx.copied = true;
        frame.rip
    }
}

/// 触发一次 COW 见证并收集证据（内核/QEMU 专用）。
pub fn run() -> Result<CowWitness, &'static str> {
    unsafe {
        let ctx_ptr = core::ptr::addr_of!(COW_CTX);
        if (*ctx_ptr).is_none() {
            return Err("cow context not prepared");
        }
    }

    // 真实写只读页 → #PF → 分派器执行 COW 并重试。
    crate::idt::trigger_page_fault_write(COW_VA);
    let err = crate::idt::last_fault_err();

    let (offset, shared, private, copied) = unsafe {
        let ctx_ptr = core::ptr::addr_of!(COW_CTX);
        match (*ctx_ptr).as_ref() {
            Some(ctx) => (ctx.offset, ctx.shared, ctx.private, ctx.copied),
            None => return Err("cow context vanished"),
        }
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
