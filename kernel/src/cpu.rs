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
    let mut value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    PhysAddr(value)
}

/// 读一个 `PageTableEntry` 数组的第 `index` 项。
/// `table_mapped_va` 是页表页已映射到虚拟地址空间中的地址（调用方负责转换）。
fn read_entry(table_mapped_va: u64, index: usize) -> Result<PageTableEntry, TranslateError> {
    let raw =
        unsafe { core::ptr::read_volatile((table_mapped_va + (index as u64) * 8) as *const u64) };
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
            let raw =
                unsafe { core::ptr::read_volatile((table_va + (index as u64) * 8) as *const u64) };
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
    use core::mem::size_of;
    use std::alloc::{Layout, alloc_zeroed, dealloc};
    use std::ptr;

    /// 分配一个 4096 对齐、4096 字节的页表页（与真实页表页一致），返回裸指针。
    ///
    /// 必须 4K 对齐：`translate` 经 `next_phys()` 会清除地址低 12 位，若分配不按页对齐，
    /// 页表项里存的指针被掩码后会指向错误位置，导致误判 PageNotMapped。
    unsafe fn alloc_page() -> *mut u64 {
        let layout = Layout::from_size_align(4096, 4096).expect("4K page layout");
        unsafe { alloc_zeroed(layout) as *mut u64 }
    }

    /// 释放一个由 `alloc_page` 分配的页。
    unsafe fn free_page(ptr: *mut u64) {
        let layout = Layout::from_size_align(4096, 4096).expect("4K page layout");
        unsafe {
            dealloc(ptr as *mut u8, layout);
        }
    }

    #[test]
    fn audit_reachable_page_tables_visits_all_levels() {
        let pml4 = unsafe { alloc_page() };
        let pdpt = unsafe { alloc_page() };
        let pd = unsafe { alloc_page() };
        let pt = unsafe { alloc_page() };
        unsafe {
            *pml4.add(0) = pdpt as u64 | 1;
            *pdpt.add(0) = pd as u64 | 1;
            *pd.add(0) = pt as u64 | 1;
            *pt.add(0) = 0x2000 | 1;
        }
        let mut visited = Vec::new();
        let count = audit_reachable_page_tables(
            pml4 as u64,
            |phys| phys,
            |phys| {
                visited.push(phys);
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count, 4);
        assert_eq!(visited.len(), 4);
        unsafe {
            free_page(pml4);
            free_page(pdpt);
            free_page(pd);
            free_page(pt);
        }
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

    #[test]
    fn translate_single_page_4k() {
        unsafe {
            let pt = alloc_page();
            ptr::write(pt.add(0), 0xf000_0000 | 0b1); // PT[0] -> 帧 0xf0000000, present
            let pt_ptr = pt as u64;

            let pd = alloc_page();
            ptr::write(pd.add(0), pt_ptr | 0b1); // PD[0] -> PT 表
            let pd_ptr = pd as u64;

            let pdpt = alloc_page();
            ptr::write(pdpt.add(0), pd_ptr | 0b1);
            let pdpt_ptr = pdpt as u64;

            let pml4 = alloc_page();
            ptr::write(pml4.add(0), pdpt_ptr | 0b1);
            let pml4_ptr = pml4 as u64;

            // vaddr 落在 i4=i3=i2=i1=0, offset=0x123
            let vaddr = 0x123;
            let phys = translate(pml4_ptr, vaddr, identity_map).unwrap();
            assert_eq!(phys, 0xf000_0000 | 0x123);

            free_page(pt);
            free_page(pd);
            free_page(pdpt);
            free_page(pml4);
        }
    }

    #[test]
    fn translate_not_mapped_returns_error() {
        unsafe {
            let pml4 = alloc_page();
            let pml4_ptr = pml4 as u64;
            let vaddr = 0x1_0000_0000; // i4=0, i3=0, i2=0, i1 非零
            assert_eq!(
                translate(pml4_ptr, vaddr, identity_map),
                Err(TranslateError::PageNotMapped)
            );
            free_page(pml4);
        }
    }

    #[test]
    fn translate_offsets_into_last_level() {
        unsafe {
            let pt = alloc_page();
            ptr::write(pt.add(0), 0x1000_0000 | 0b1);
            let pt_ptr = pt as u64;

            let pd = alloc_page();
            ptr::write(pd.add(0), pt_ptr | 0b1);
            let pd_ptr = pd as u64;

            let pdpt = alloc_page();
            ptr::write(pdpt.add(0), pd_ptr | 0b1);
            let pdpt_ptr = pdpt as u64;

            let pml4 = alloc_page();
            ptr::write(pml4.add(0), pdpt_ptr | 0b1);
            let pml4_ptr = pml4 as u64;

            let vaddr = 0x123;
            let phys = translate(pml4_ptr, vaddr, identity_map).unwrap();
            assert_eq!(phys, 0x1000_0000 | 0x123);

            free_page(pt);
            free_page(pd);
            free_page(pdpt);
            free_page(pml4);
        }
    }
}
