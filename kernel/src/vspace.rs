//! 自建 4 级页表映射的细化（refine）模块：在 bootloader CR3 之上新增一条隔离的
//! 低半区映射，并承载 `invlpg` 的 `asm!`。映射构建的**纯逻辑**（级别遍历、PTE
//! 装载、缺页表页的预分配）可 host 单测；真实 `invlpg` 仅内核/QEMU 内执行。
//!
//! **范围边界：** 本模块不替换 CR3、不重建整棵页表，只在空闲 PML4 槽（见
//! [`VSPACE_VA`]）下装一条 4KiB 映射，用于见证真实 PTE 翻译与 TLB 刷新。

use core::arch::asm;

/// PTE present 位（bit 0）。
pub const PT_PRESENT: u64 = 1;

/// PTE writable 位（bit 1）。
pub const PT_WRITABLE: u64 = 1 << 1;

/// PTE user 位（bit 2）：CPL3 可访问该页。
pub const PT_USER: u64 = 1 << 2;

/// PTE 物理地址掩码（bits 12..51）。
pub const PT_ADDR_MASK: u64 = 0x000f_ffff_ffff_f000;

/// 演示映射使用的虚拟地址：i4 槽 1（低半区 `0x80_0000_0000`）。
///
/// 内核镜像映射在 i4=256（高半区）、bootloader 低映射在 i4=0，故槽 1 空闲。
pub const VSPACE_VA: u64 = 0x0000_0080_0000_0000;

/// 数据帧 A/B 的写入魔数（xtask 侧有镜像常量，需人工同步）。
pub const MAGIC_A: u64 = 0xAAAA_0000_1111_2222;
pub const MAGIC_B: u64 = 0xBBBB_0000_3333_4444;

/// 映射构建失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VspaceError {
    /// 需要新建页表页但帧分配器已耗尽（不产生任何写入）。
    NoFrame,
    /// 目标映射的中间层级不存在（`set_leaf`/`read_leaf` 只走已建好的路径）。
    BadRoot,
    /// 演示用的 PML4 槽已被占用（拒绝覆盖 bootloader 已有映射）。
    SlotOccupied,
}

/// 从虚拟地址取出 4 级索引 `[i4, i3, i2, i1]`。
pub const fn indices(va: u64) -> [usize; 4] {
    [
        ((va >> 39) & 0x1ff) as usize,
        ((va >> 30) & 0x1ff) as usize,
        ((va >> 21) & 0x1ff) as usize,
        ((va >> 12) & 0x1ff) as usize,
    ]
}

/// 叶 PTE 标志（present + 可选 writable + 可选 user）。
pub const fn pte_flags(writable: bool, user: bool) -> u64 {
    PT_PRESENT | if writable { PT_WRITABLE } else { 0 } | if user { PT_USER } else { 0 }
}

/// 中间层页表项标志：present + writable（写只读叶的故障必须落在叶上）。
pub const fn table_flags() -> u64 {
    PT_PRESENT | PT_WRITABLE
}

fn read_raw(addr: u64) -> u64 {
    unsafe { core::ptr::read_volatile(addr as *const u64) }
}

fn write_raw(addr: u64, value: u64) {
    unsafe { core::ptr::write_volatile(addr as *mut u64, value) };
}

/// 把一页（512 项）清零。
fn zero_table(mapped_va: u64) {
    for index in 0..512 {
        write_raw(mapped_va + (index as u64) * 8, 0);
    }
}

/// `va` 路径上缺失的页表页数（叶子所在 PT 计入）。
fn count_missing(root_phys: u64, va: u64, map: impl Fn(u64) -> u64) -> usize {
    let idx = indices(va);
    let mut table = root_phys;
    for (k, &slot) in idx[..3].iter().enumerate() {
        let entry = read_raw(map(table) + (slot as u64) * 8);
        if entry & PT_PRESENT == 0 {
            // k 级缺失 ⇒ 需新建 (3-k) 个中间层 + 1 个叶 PT。
            return 4 - k;
        }
        table = entry & PT_ADDR_MASK;
    }
    0
}

/// 建立 `va → frame_phys` 的 4KiB 映射，缺页表页时经 `alloc` 取新帧。
///
/// 两阶段：先统计缺失页表页并**一次性**分配；任一分配失败则返回 [`VspaceError::NoFrame`]
/// 且不产生任何写入（对分配失败保持原子）。随后沿路径装载，叶 PTE 按 `writable` 设位。
pub fn map_4k(
    root_phys: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
    mut alloc: impl FnMut() -> Option<u64>,
    frame_phys: u64,
    writable: bool,
    user: bool,
) -> Result<(), VspaceError> {
    let needed = count_missing(root_phys, va, &map);
    let mut fresh = [0u64; 4];
    for slot in fresh[..needed].iter_mut() {
        *slot = alloc().ok_or(VspaceError::NoFrame)?;
    }

    let idx = indices(va);
    let mut table = root_phys;
    let mut fresh_iter = fresh[..needed].iter();
    for &slot in &idx[..3] {
        let entry_addr = map(table) + (slot as u64) * 8;
        let entry = read_raw(entry_addr);
        if entry & PT_PRESENT == 0 {
            let frame = *fresh_iter.next().expect("count_missing matched alloc");
            zero_table(map(frame));
            write_raw(entry_addr, frame | table_flags());
            table = frame;
        } else {
            table = entry & PT_ADDR_MASK;
        }
    }
    write_raw(
        map(table) + (idx[3] as u64) * 8,
        frame_phys | pte_flags(writable, user),
    );
    Ok(())
}

/// 改写已有映射的叶 PTE（中间层级必须已存在，否则 [`VspaceError::BadRoot`]）。
pub fn set_leaf(
    root_phys: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
    frame_phys: u64,
    writable: bool,
    user: bool,
) -> Result<(), VspaceError> {
    let table = walk_to_leaf_table(root_phys, va, &map)?;
    let idx = indices(va);
    write_raw(
        map(table) + (idx[3] as u64) * 8,
        frame_phys | pte_flags(writable, user),
    );
    Ok(())
}

/// 读取已有映射的叶 PTE 原始值（含标志位）。
pub fn read_leaf(root_phys: u64, va: u64, map: impl Fn(u64) -> u64) -> Result<u64, VspaceError> {
    let table = walk_to_leaf_table(root_phys, va, &map)?;
    let idx = indices(va);
    Ok(read_raw(map(table) + (idx[3] as u64) * 8))
}

/// 沿已存在的中间层走到叶所在 PT 的物理帧；缺层返回 `BadRoot`。
fn walk_to_leaf_table(
    root_phys: u64,
    va: u64,
    map: impl Fn(u64) -> u64,
) -> Result<u64, VspaceError> {
    let idx = indices(va);
    let mut table = root_phys;
    for &slot in &idx[..3] {
        let entry = read_raw(map(table) + (slot as u64) * 8);
        if entry & PT_PRESENT == 0 {
            return Err(VspaceError::BadRoot);
        }
        table = entry & PT_ADDR_MASK;
    }
    Ok(table)
}

/// 使 `va` 的 TLB 项失效。
///
/// # Safety
///
/// `invlpg` 是特权指令，仅可在 CPL0 内核内执行；host 用户态单测不得调用。
pub unsafe fn invlpg(va: u64) {
    unsafe {
        asm!("invlpg [{}]", in(reg) va, options(nostack, preserves_flags));
    }
}

/// 检查 `va` 所在的 PML4 槽是否空闲（未被 bootloader 占用）。
pub fn slot_free(root_phys: u64, va: u64, map: impl Fn(u64) -> u64) -> bool {
    let idx = indices(va);
    read_raw(map(root_phys) + (idx[0] as u64) * 8) & PT_PRESENT == 0
}

/// 一次自建映射见证的实测结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Witness {
    pub va: u64,
    pub frame_a: u64,
    pub frame_b: u64,
    /// 映射到 A 后经 VA 读回的值（应 [`MAGIC_A`]）：证明 PTE 驱动翻译。
    pub read_before: u64,
    /// 改叶指向 B、未刷新时经 VA 读回的值（信息：陈旧=真 TLB，亦可能 QEMU 已失效）。
    pub stale_read: u64,
    /// `invlpg` 后经 VA 读回的值（应 [`MAGIC_B`]）：证明真实页表更新 + TLB 刷新。
    pub read_after: u64,
    /// `cpu::translate` 对同一 VA 解析出的物理帧（应等于 `frame_b`）。
    pub translated: u64,
    /// 写只读页 #PF 的硬件 error code（应 0x3）。
    pub wp_err: u64,
    /// 写只读页 #PF 的 CR2（应等于 `va`）。
    pub wp_addr: u64,
}

/// 在 `root_phys` 下运行一次自建映射见证（内核/QEMU 专用，host 不可执行）。
///
/// 步骤：分配 A/B 两帧并写入魔数 → 建 `VA→A` 映射并读回 → 改叶 `VA→B` 观察刷新
/// 前后的读值 → `cpu::translate` 对账 → 改叶为只读并触发真实写保护 #PF。
pub fn run_witness(
    root_phys: u64,
    offset: u64,
    mut alloc: impl FnMut() -> Option<u64>,
) -> Result<Witness, VspaceError> {
    let map = |phys: u64| crate::mem::phys_to_virt(phys, offset);
    if !slot_free(root_phys, VSPACE_VA, map) {
        return Err(VspaceError::SlotOccupied);
    }

    let frame_a = alloc().ok_or(VspaceError::NoFrame)?;
    let frame_b = alloc().ok_or(VspaceError::NoFrame)?;
    zero_table(map(frame_a));
    zero_table(map(frame_b));
    write_raw(map(frame_a), MAGIC_A);
    write_raw(map(frame_b), MAGIC_B);

    map_4k(root_phys, VSPACE_VA, map, &mut alloc, frame_a, true, false)?;
    unsafe { invlpg(VSPACE_VA) };
    let read_before = read_raw(VSPACE_VA);

    set_leaf(root_phys, VSPACE_VA, map, frame_b, true, false)?;
    let stale_read = read_raw(VSPACE_VA); // 信息：刷新前
    unsafe { invlpg(VSPACE_VA) };
    let read_after = read_raw(VSPACE_VA);

    let translated = crate::cpu::translate(map(root_phys), VSPACE_VA, map).unwrap_or(0);

    // 只读叶 + 写 → 真实 `present∧write` #PF（classify → WriteProtection）。
    set_leaf(root_phys, VSPACE_VA, map, frame_a, false, false)?;
    unsafe { invlpg(VSPACE_VA) };
    crate::idt::trigger_page_fault_write(VSPACE_VA);
    let wp_err = crate::idt::last_fault_err();
    let wp_addr = crate::idt::fault_addr();

    Ok(Witness {
        va: VSPACE_VA,
        frame_a,
        frame_b,
        read_before,
        stale_read,
        read_after,
        translated,
        wp_err,
        wp_addr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::alloc::{Layout, alloc_zeroed, dealloc};

    struct PagePool {
        pages: Vec<*mut u64>,
    }

    impl PagePool {
        fn new() -> Self {
            PagePool { pages: Vec::new() }
        }
        fn alloc(&mut self) -> Option<u64> {
            let layout = Layout::from_size_align(4096, 4096).expect("4K layout");
            let ptr = unsafe { alloc_zeroed(layout) as *mut u64 };
            if ptr.is_null() {
                return None;
            }
            self.pages.push(ptr);
            Some(ptr as u64)
        }
    }

    impl Drop for PagePool {
        fn drop(&mut self) {
            let layout = Layout::from_size_align(4096, 4096).expect("4K layout");
            for &ptr in &self.pages {
                unsafe { dealloc(ptr as *mut u8, layout) };
            }
        }
    }

    fn identity(phys: u64) -> u64 {
        phys
    }

    #[test]
    fn pte_flags_sets_present_and_writable() {
        assert_eq!(pte_flags(true, false), PT_PRESENT | PT_WRITABLE);
        assert_eq!(pte_flags(false, false), PT_PRESENT);
        assert_eq!(pte_flags(true, true), PT_PRESENT | PT_WRITABLE | PT_USER);
        assert_eq!(pte_flags(false, true), PT_PRESENT | PT_USER);
        assert_eq!(table_flags(), PT_PRESENT | PT_WRITABLE);
    }

    #[test]
    fn indices_decode_vspace_va_into_slot_one() {
        assert_eq!(indices(VSPACE_VA), [1, 0, 0, 0]);
        // 叶索引测试：VA + 0x3000 → i1 = 3
        assert_eq!(indices(VSPACE_VA + 0x3000), [1, 0, 0, 3]);
    }

    #[test]
    fn map_4k_builds_all_levels_and_reads_leaf() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();

        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame,
            true,
            false,
        )
        .unwrap();

        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap(),
            frame | pte_flags(true, false)
        );
        // 逐级 present：PML4[1] -> PDPT -> PD -> PT
        let idx = indices(VSPACE_VA);
        let pml4e = read_raw(root + (idx[0] as u64) * 8);
        assert!(pml4e & PT_PRESENT != 0);
        let pdpt = pml4e & PT_ADDR_MASK;
        let pdpte = read_raw(pdpt + (idx[1] as u64) * 8);
        assert!(pdpte & PT_PRESENT != 0);
        let pd = pdpte & PT_ADDR_MASK;
        let pde = read_raw(pd + (idx[2] as u64) * 8);
        assert!(pde & PT_PRESENT != 0);
    }

    #[test]
    fn map_4k_reuses_existing_path() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame_a = pool.alloc().unwrap();
        let frame_b = pool.alloc().unwrap();

        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame_a,
            true,
            false,
        )
        .unwrap();
        let pages_after_first = pool.pages.len();
        // 同一路径的第二条映射只改叶，不新建页表页。
        map_4k(
            root,
            VSPACE_VA + 0x1000,
            identity,
            || pool.alloc(),
            frame_b,
            true,
            false,
        )
        .unwrap();
        assert_eq!(pool.pages.len(), pages_after_first);

        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap(),
            frame_a | pte_flags(true, false)
        );
        assert_eq!(
            read_leaf(root, VSPACE_VA + 0x1000, identity).unwrap(),
            frame_b | pte_flags(true, false)
        );
    }

    #[test]
    fn map_4k_no_frame_leaves_state_untouched() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();

        // 空分配器：需要 4 个页表页，必然失败。
        let result = map_4k(root, VSPACE_VA, identity, || None, frame, true, false);
        assert_eq!(result, Err(VspaceError::NoFrame));
        // 未产生任何写入：PML4[1] 仍为 0。
        assert_eq!(read_raw(root + 8), 0);
    }

    #[test]
    fn set_leaf_can_clear_writable_for_write_protect() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();

        map_4k(
            root,
            VSPACE_VA,
            identity,
            || pool.alloc(),
            frame,
            true,
            false,
        )
        .unwrap();
        set_leaf(root, VSPACE_VA, identity, frame, false, false).unwrap();
        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap(),
            frame | pte_flags(false, false)
        );
        assert_eq!(
            read_leaf(root, VSPACE_VA, identity).unwrap() & PT_WRITABLE,
            0
        );
    }

    #[test]
    fn set_leaf_without_path_is_bad_root() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap();
        let frame = pool.alloc().unwrap();
        assert_eq!(
            set_leaf(root, VSPACE_VA, identity, frame, true, false),
            Err(VspaceError::BadRoot)
        );
    }

    #[test]
    fn read_leaf_without_path_is_bad_root() {
        let mut pool = PagePool::new();
        let root = pool.alloc().unwrap(); // 全零根：i4=1 项缺失
        assert_eq!(
            read_leaf(root, VSPACE_VA, identity),
            Err(VspaceError::BadRoot)
        );
    }
}
