//! 页表访问与地址翻译的细化（refine）模块：承载读 cr3 的 `asm!` 与读页表项的
//! unsafe 原语；`translate` 的纯逻辑解析可 host 单测。

/// 物理地址 newtype。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslateError {
    PageNotMapped,
    InvalidLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableAuditError {
    TooManyTables,
    InvalidLevel,
    UnownedTable,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageTableEntry {
    pub flags: u64,
}

impl PageTableEntry {
    pub const fn new() -> Self {
        PageTableEntry { flags: 0 }
    }
    fn is_present(&self) -> bool {
        self.flags & 0b1 != 0
    }
    fn huge(&self) -> bool {
        self.flags & (1 << 7) != 0
    }
    fn next_phys(&self) -> u64 {
        self.flags & 0x000f_ffff_ffff_f000
    }
}

impl Default for PageTableEntry {
    fn default() -> Self {
        Self::new()
    }
}

/// 读取当前 CR3 寄存器（页表根物理地址）。
///
/// 读 cr3 无副作用，但需在内核已由 bootloader 装载 CR3 后调用。
pub fn cr3() -> PhysAddr {
    PhysAddr(prima_ostd::arch::read_cr3())
}

/// 当前 CR3 的页表根物理地址（清掉低 12 位的标志/PCID 位）。
pub fn cr3_phys() -> u64 {
    cr3().0 & 0x000f_ffff_ffff_f000
}

/// 写 CR3，切换到另一张 PML4（per-task 地址空间）。
///
/// 契约随 [`prima_ostd::arch::write_cr3`]：特权指令，仅 CPL0 可执行。调用瞬间的指令流与栈
/// 必须在新根下仍被映射（本项目通过 [`crate::vspace::clone_root`] 复制 bootloader 根的全部
/// PML4 项来保证），否则写 CR3 后立即取指失败。
pub fn write_cr3(phys: u64) {
    prima_ostd::arch::write_cr3(phys);
}

/// CR0 的 WP 位掩码（bit 16）。WP=1 ⇒ supervisor 写只读页产生 #PF；0 = 允许通过。
///
/// `read_cr0_wp` 是"本次是否可能造出 supervisor 写保护现场"的总闸门，不依赖页表形态。
/// 真实现场只能在内核层（CPL0）读 CR0；纯逻辑抽到 [`wp_of`] 供 host 单测。
pub const CR0_WP: u64 = 1 << 16;

/// 从一段已读出的 CR0 值里判断 WP 位是否置位（纯算术，无特权指令，可 host 单测钉住）。
pub fn wp_of(cr0: u64) -> bool {
    cr0 & CR0_WP != 0
}

/// 读 CR0 的 WP 位：1 = supervisor 写只读页产生 #PF；0 = 允许通过。
///
/// 读 cr0 无副作用，但属特权操作，仅在内核已运行于 CPL0（由 bootloader 进入）时可读，
/// 切不可在 host 用户态单测直接调用。
pub fn read_cr0_wp() -> bool {
    wp_of(prima_ostd::arch::read_cr0())
}

/// 读一个 `PageTableEntry` 数组的第 `index` 项。
/// `table_mapped_va` 是页表页已映射到虚拟地址空间中的地址（调用方负责转换）。
fn read_entry(table_mapped_va: u64, index: usize) -> Result<PageTableEntry, TranslateError> {
    let raw = prima_ostd::mm::read_u64_at(table_mapped_va, index as u64);
    let e = PageTableEntry { flags: raw };
    if e.is_present() {
        Ok(e)
    } else {
        Err(TranslateError::PageNotMapped)
    }
}

/// 纯页表解析：从已映射的 PML4 虚拟地址出发，把 vaddr 换成 4KiB 页物理地址。
///
/// 中间层级地址由页表项给出的是物理地址，调用方需先经 phys_to_virt 转成虚拟再递归读。
/// 因此此函数接受一个映射函数 `map: fn(u64) -> u64`（物理→虚拟）。
pub fn translate(
    pml4_mapped_va: u64,
    vaddr: u64,
    map: impl Fn(u64) -> u64,
) -> Result<u64, TranslateError> {
    let i4 = ((vaddr >> 39) & 0x1ff) as usize;
    let i3 = ((vaddr >> 30) & 0x1ff) as usize;
    let i2 = ((vaddr >> 21) & 0x1ff) as usize;
    let i1 = ((vaddr >> 12) & 0x1ff) as usize;
    let offset = vaddr & 0xfff;

    let pml4 = read_entry(pml4_mapped_va, i4)?;
    let pdpt_phys = pml4.next_phys();
    let pdpt_mapped = map(pdpt_phys);
    let pdpt = read_entry(pdpt_mapped, i3)?;
    if pdpt.huge() {
        return Err(TranslateError::PageNotMapped);
    }
    let pd_phys = pdpt.next_phys();
    let pd_mapped = map(pd_phys);
    let pd = read_entry(pd_mapped, i2)?;
    if pd.huge() {
        return Err(TranslateError::PageNotMapped);
    }
    let pt_phys = pd.next_phys();
    let pt_mapped = map(pt_phys);
    let pt = read_entry(pt_mapped, i1)?;
    Ok(pt.next_phys() | offset)
}

pub fn audit_reachable_page_tables(
    root_phys: u64,
    map: impl Fn(u64) -> u64 + Copy,
    mut validate: impl FnMut(u64) -> Result<(), ()>,
) -> Result<usize, PageTableAuditError> {
    const MAX_TABLES: usize = 1024;

    fn walk(
        table_phys: u64,
        level: u8,
        map: impl Fn(u64) -> u64 + Copy,
        validate: &mut impl FnMut(u64) -> Result<(), ()>,
        seen: &mut [u64; 1024],
        count: &mut usize,
    ) -> Result<(), PageTableAuditError> {
        if seen[..*count].contains(&table_phys) {
            return Ok(());
        }
        if *count == seen.len() {
            return Err(PageTableAuditError::TooManyTables);
        }
        validate(table_phys).map_err(|_| PageTableAuditError::UnownedTable)?;
        seen[*count] = table_phys;
        *count += 1;

        let table_va = map(table_phys);
        for index in 0..512 {
            let raw = prima_ostd::mm::read_u64_at(table_va, index as u64);
            let entry = PageTableEntry { flags: raw };
            if !entry.is_present() {
                continue;
            }
            if level == 4 && entry.huge() {
                return Err(PageTableAuditError::InvalidLevel);
            }
            if level == 1 || (entry.huge() && (level == 2 || level == 3)) {
                continue;
            }
            walk(entry.next_phys(), level - 1, map, validate, seen, count)?;
        }
        Ok(())
    }

    let mut seen = [0; MAX_TABLES];
    let mut count = 0;
    walk(root_phys, 4, map, &mut validate, &mut seen, &mut count)?;
    Ok(count)
}

/// 在 host 单测中使用的恒等映射（物理==虚拟），纯逻辑测试用。
#[cfg(test)]
fn identity_map(phys: u64) -> u64 {
    phys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::PagePool;
    use core::mem::size_of;
    use prima_ostd::mm::write_u64_at;

    /// 页表页由 [`PagePool`] 在堆上按 4 KiB 对齐分配、全零且地址稳定；读写走 `mm` 安全访问器。
    /// `translate` 经 `next_phys()` 会清除地址低 12 位，故分配必须按页对齐（否则页表项里存的
    /// 指针被掩码后会指向错误位置，误判 `PageNotMapped`）。
    #[test]
    fn audit_reachable_page_tables_visits_all_levels() {
        let mut pool = PagePool::new();
        let pml4 = pool.alloc().unwrap();
        let pdpt = pool.alloc().unwrap();
        let pd = pool.alloc().unwrap();
        let pt = pool.alloc().unwrap();
        write_u64_at(pml4, 0, pdpt | 1);
        write_u64_at(pdpt, 0, pd | 1);
        write_u64_at(pd, 0, pt | 1);
        write_u64_at(pt, 0, 0x2000 | 1);
        let mut visited = Vec::new();
        let count = audit_reachable_page_tables(
            pml4,
            |phys| phys,
            |phys| {
                visited.push(phys);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count, 4);
        assert_eq!(visited.len(), 4);
    }

    #[test]
    fn page_table_entry_field_size() {
        assert_eq!(size_of::<PageTableEntry>(), 8);
    }

    #[test]
    fn page_table_entry_is_present_uses_lsb() {
        assert!(PageTableEntry { flags: 0b1 }.is_present());
        assert!(!PageTableEntry { flags: 0 }.is_present());
    }

    #[test]
    fn page_table_entry_addr_bits_mask_offsets() {
        // 低 12 位（0x123）被掩码清除，返回帧地址 0x1234_5678_9000。
        let e = PageTableEntry {
            flags: 0x1234_5678_9123,
        };
        assert_eq!(e.next_phys(), 0x1234_5678_9000);
    }

    #[test]
    fn page_table_entry_huge_bit() {
        assert!(PageTableEntry { flags: 1 << 7 }.huge());
        assert!(!PageTableEntry { flags: 0 }.huge());
    }

    #[test]
    fn page_table_entry_new_zero_flags() {
        assert_eq!(PageTableEntry::new().flags, 0);
    }

    #[test]
    fn phys_addr_newtype_is_u64() {
        let p = PhysAddr(0x1000);
        assert_eq!(p.0, 0x1000);
    }

    /// 装配 PML4→PDPT→PD→PT 四级链，叶项指向 `frame`；返回根页地址。
    fn build_four_levels(pool: &mut PagePool, frame_phys: u64) -> u64 {
        let pt = pool.alloc().unwrap();
        write_u64_at(pt, 0, frame_phys | 0b1);
        let pd = pool.alloc().unwrap();
        write_u64_at(pd, 0, pt | 0b1);
        let pdpt = pool.alloc().unwrap();
        write_u64_at(pdpt, 0, pd | 0b1);
        let pml4 = pool.alloc().unwrap();
        write_u64_at(pml4, 0, pdpt | 0b1);
        pml4
    }

    #[test]
    fn translate_single_page_4k() {
        let mut pool = PagePool::new();
        let pml4 = build_four_levels(&mut pool, 0xf000_0000);
        // vaddr 落在 i4=i3=i2=i1=0, offset=0x123
        let vaddr = 0x123;
        let phys = translate(pml4, vaddr, identity_map).unwrap();
        assert_eq!(phys, 0xf000_0000 | 0x123);
    }

    #[test]
    fn translate_not_mapped_returns_error() {
        let mut pool = PagePool::new();
        let pml4 = pool.alloc().unwrap();
        let vaddr = 0x1_0000_0000; // i4=0, i3=0, i2=0, i1 非零
        assert_eq!(
            translate(pml4, vaddr, identity_map),
            Err(TranslateError::PageNotMapped)
        );
    }

    #[test]
    fn translate_offsets_into_last_level() {
        let mut pool = PagePool::new();
        let pml4 = build_four_levels(&mut pool, 0x1000_0000);
        let vaddr = 0x123;
        let phys = translate(pml4, vaddr, identity_map).unwrap();
        assert_eq!(phys, 0x1000_0000 | 0x123);
    }

    #[test]
    fn read_cr0_wp_wp_bit_decision_is_legal_and_stable() {
        // `mov reg, cr0` 是特权指令（仅 CPL0 可读）。host `cargo test` 把本 crate 以 std
        // 用户态进程编译运行，直接执行会触发 STATUS_PRIVILEGED_INSTRUCTION。因此真实现场
        // 性验证归 QEMU/kernel（本 slice Task 6）；此处钉住纯决策逻辑 `wp_of`：
        // CR0 现场 = 0/1 皆可能（平台相关），故不断言常量真值，只断言合法 bool +
        // 掩码取中幂等稳定。这镜像了 fault.rs"无 asm、纯逻辑 host 可测"的既有纪律。
        assert_eq!(CR0_WP, 1 << 16);
        assert!(!wp_of(0));
        assert!(wp_of(CR0_WP));
        assert!(!wp_of(CR0_WP >> 1)); // bit15 不清 WP
        assert!(wp_of(u64::MAX));
        // 幂等（稳定）：同一致现场多次求值一致，且是合法 bool。
        let a = wp_of(0x112_0000u64);
        let b = wp_of(0x112_0000u64);
        assert_eq!(a, b);
        assert!(!a); // 0x0000_0011_2000_0000 未含 bit16
    }
}
