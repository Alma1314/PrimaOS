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

/// CR0.EM：置位时 SSE 指令非法（#UD）。
pub const CR0_EM: u64 = 1 << 2;
/// CR0.TS：置位时首次 SSE 指令触发 #NM（本内核无 #NM 处理）。
pub const CR0_TS: u64 = 1 << 3;
/// CR4.OSFXSR：允许 FXSAVE/FXRSTOR 与 SSE 指令。
pub const CR4_OSFXSR: u64 = 1 << 9;
/// CR4.OSXMMEXCPT：SSE 异常经 #XF 交付（本内核未处理，但置位是常规契约）。
pub const CR4_OSXMMEXCPT: u64 = 1 << 10;

/// 纯算术：清掉阻止 SSE 的 CR0.EM/TS，保留其余位。
pub fn cr0_for_sse(cr0: u64) -> u64 {
    cr0 & !(CR0_EM | CR0_TS)
}

/// 纯算术：置 CR4.OSFXSR/OSXMMEXCPT，保留其余位（尤其 PAE）。
pub fn cr4_for_sse(cr4: u64) -> u64 {
    cr4 | CR4_OSFXSR | CR4_OSXMMEXCPT
}

/// 启用 x87/SSE 指令并允许 FXSAVE/FXRSTOR。
///
/// 在进入 CPL3 之前调用一次；per-task 状态切换见 [`crate::sched`]。
pub fn enable_sse() {
    let cr0 = prima_ostd::arch::read_cr0();
    prima_ostd::arch::write_cr0(cr0_for_sse(cr0));
    let cr4 = prima_ostd::arch::read_cr4();
    prima_ostd::arch::write_cr4(cr4_for_sse(cr4));
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

/// D15 / S7-B.1：CPU 本地状态的唯一边界，SMP 分区从「当前任务」开始。
///
/// 这里的每个字段**每 CPU 各自一份**：任何 CPU 只读写自己的槽位，不跨 CPU 共享。
/// 任务表、地址空间、能力等共享状态不在此处，仍由各自锁保护——分区不意味着复制
/// 共享状态。单核阶段只有 BSP(0) 在线，[`current_index`] 固定返回 BSP；AP 启动
/// 后改为由 LAPIC ID 映射到索引。设计与分期见
/// `docs/superpowers/specs/2026-11-27-stage7-smp-state-partition.md`。
pub mod local {
    use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    /// 平台上限：S7-SCOPE-13 冻结 x86_64 为 2/4 vCPU。
    pub const MAX_CPUS: usize = 4;
    /// BSP 的索引固定为 0。
    pub const BSP: usize = 0;

    /// 每 CPU 区（GS-3）。`%gs:0` = index、`%gs:4` = APIC ID；字段顺序即 GS 偏移，
    /// 改动必须同步 [`install_gs`]/[`local_apic_id`] 与 host 布局测试。
    #[repr(C)]
    struct CpuLocal {
        index: u32,
        apic_id: AtomicU32,
        /// 本 CPU 当前（或即将恢复）的 Task id；0 = 尚无任务（AP 握手前）。
        current_task: AtomicU32,
        /// P5-3b-2：本 CPU `syscall` 快路径栈可用顶（入口经 `%gs` 直接读取）。
        syscall_stack_top: AtomicU64,
        /// P5-3b-2：本 CPU 快路径 `USER_RSP` 槽（入口经 `%gs` 直接读写）。
        user_rsp: AtomicU64,
        /// P5-3b-2：本 CPU 入口槽地址（快路径 `ctx` 写入）。
        entry_slot: AtomicU64,
    }

    impl CpuLocal {
        const fn new(index: u32, current_task: u32) -> Self {
            Self {
                index,
                apic_id: AtomicU32::new(0),
                current_task: AtomicU32::new(current_task),
                syscall_stack_top: AtomicU64::new(0),
                user_rsp: AtomicU64::new(0),
                entry_slot: AtomicU64::new(0),
            }
        }
    }

    /// BSP 初值与迁移前 `sched::CURRENT` 相同（bootstrap init Task）。
    static LOCALS: [CpuLocal; MAX_CPUS] = [
        CpuLocal::new(0, crate::bootstrap::INIT_TASK_ID),
        CpuLocal::new(1, 0),
        CpuLocal::new(2, 0),
        CpuLocal::new(3, 0),
    ];

    /// 指定 CPU 的 GS 区基址；越界 panic（不返回错误槽位）。
    pub fn area_ptr(cpu: usize) -> u64 {
        assert!(cpu < MAX_CPUS, "cpu index out of range");
        core::ptr::addr_of!(LOCALS[cpu]) as u64
    }

    /// 指定 CPU 区记录的 APIC ID（未安装时为 0）。
    pub fn apic_id_on(cpu: usize) -> u32 {
        assert!(cpu < MAX_CPUS, "cpu index out of range");
        LOCALS[cpu].apic_id.load(Ordering::Relaxed)
    }

    /// P5-3b-2：指定 CPU 快路径入口最近写入的 `USER_RSP`（未进入过为 0）。
    ///
    /// 仅供受控探针见证；普通镜像不写该槽。
    #[cfg(feature = "entry-percpu-probe")]
    pub fn user_rsp_on(cpu: usize) -> u64 {
        assert!(cpu < MAX_CPUS, "cpu index out of range");
        LOCALS[cpu].user_rsp.load(Ordering::Relaxed)
    }

    /// P5-3b-2：指定 CPU GS 区记录的快路径栈顶（诊断/断言用）。
    #[cfg(feature = "entry-percpu-probe")]
    pub fn syscall_stack_top_on(cpu: usize) -> u64 {
        assert!(cpu < MAX_CPUS, "cpu index out of range");
        LOCALS[cpu].syscall_stack_top.load(Ordering::Relaxed)
    }

    /// P5-3b-2：把指定 CPU 的快路径栈顶改指向探针显式映射的内核栈。
    #[cfg(feature = "entry-percpu-probe")]
    pub fn set_syscall_stack_top(cpu: usize, top: u64) {
        assert!(cpu < MAX_CPUS, "cpu index out of range");
        LOCALS[cpu].syscall_stack_top.store(top, Ordering::Relaxed);
    }

    /// GS-3：把本 CPU 的 GS 基址指向 [`LOCALS`] 中自己的槽位。
    ///
    /// 契约：调用方正在 `cpu` 上执行且处于 CPL0；`KERNEL_GS_BASE` 一并清零，使
    /// `%gs` 在未交换态下就指向本区。P5-3b-2 的受控镜像会在此后进入 CPL3，届时由
    /// 恢复跳板把两根 MSR 切到用户侧，入口再按 GS-1 判据交换。
    #[cfg(all(feature = "gs3-probe", not(test)))]
    pub fn install_gs(cpu: usize, apic_id: u32) {
        assert!(cpu < MAX_CPUS, "cpu index out of range");
        LOCALS[cpu].apic_id.store(apic_id, Ordering::Relaxed);
        #[cfg(feature = "entry-percpu-probe")]
        {
            // 探针在当前根显式映射本 CPU 的快路径栈后再写入真实栈顶。
            LOCALS[cpu].syscall_stack_top.store(0, Ordering::Relaxed);
            LOCALS[cpu].user_rsp.store(0, Ordering::Relaxed);
            LOCALS[cpu]
                .entry_slot
                .store(prima_ostd::entry::percpu_slot_addr(cpu), Ordering::Relaxed);
        }
        crate::msr::wrmsr(crate::msr::IA32_GS_BASE, area_ptr(cpu));
        crate::msr::wrmsr(crate::msr::IA32_KERNEL_GS_BASE, 0);
    }

    /// GS-3：经 `%gs` 读本 CPU 区记录的 APIC ID。
    #[cfg(all(feature = "gs3-probe", not(test)))]
    pub fn local_apic_id() -> u32 {
        prima_ostd::arch::read_gs_u32::<4>()
    }

    /// 本 CPU 的索引。GS-3 探针镜像经每 CPU GS 区读取；普通镜像与 host 测试恒为 BSP
    /// （AP 上线前唯一在线 CPU，且内核未安装 GS 区）。
    #[cfg(all(feature = "gs3-probe", not(test)))]
    pub fn current_index() -> usize {
        let index = prima_ostd::arch::read_gs_u32::<0>() as usize;
        assert!(index < MAX_CPUS, "gs cpu index out of range");
        index
    }

    #[cfg(any(not(feature = "gs3-probe"), test))]
    pub fn current_index() -> usize {
        BSP
    }

    /// P5-3b-3：不改 `%gs`、只读 MSR 的 CPU 身份。
    ///
    /// 早期 boot（GS 区尚未安装、`IA32_GS_BASE` 为 0）必须能安全定位 #PF 现场；
    /// 此时返回 BSP。`%gs:0` 版本在这是不可读的（基址 0 可能未映射低页）。
    #[cfg(feature = "sched-percpu-probe")]
    pub fn index_from_gs_base() -> usize {
        let base = crate::msr::rdmsr(crate::msr::IA32_GS_BASE);
        (0..MAX_CPUS)
            .find(|&cpu| area_ptr(cpu) == base)
            .unwrap_or(BSP)
    }

    /// 本 CPU 的当前 Task id。
    pub fn current_task() -> u32 {
        task_on(current_index())
    }

    /// 记录本 CPU 的当前 Task id（由调度切换调用）。
    pub fn set_current_task(task_id: u32) {
        set_task_on(current_index(), task_id);
    }

    /// 读取指定 CPU 的当前 Task id；越界 panic，不静默落到错误槽位。
    pub fn task_on(cpu: usize) -> u32 {
        LOCALS[cpu].current_task.load(Ordering::Relaxed)
    }

    /// 记录指定 CPU 的当前 Task id（AP 握手与宿主测试的受控入口）。
    pub fn set_task_on(cpu: usize, task_id: u32) {
        assert!(cpu < MAX_CPUS, "cpu index out of range");
        LOCALS[cpu].current_task.store(task_id, Ordering::Relaxed);
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn cpu_slots_hold_independent_current_tasks() {
            set_task_on(1, 7);
            set_task_on(2, 9);
            assert_eq!((task_on(1), task_on(2)), (7, 9));
            set_task_on(1, 8);
            assert_eq!((task_on(1), task_on(2)), (8, 9), "写一个槽不影响另一个");
            assert_eq!(current_task(), task_on(current_index()));
            set_task_on(1, 0);
            set_task_on(2, 0);
        }

        #[test]
        #[should_panic(expected = "cpu index out of range")]
        fn out_of_range_cpu_index_panics() {
            set_task_on(MAX_CPUS, 1);
        }

        /// GS-3：`%gs:0`/`%gs:4` 的偏移由结构布局决定，必须钉住；各区互异。
        #[test]
        fn gs_area_layout_and_distinct_bases() {
            use core::mem::offset_of;
            assert_eq!(offset_of!(CpuLocal, index), 0);
            assert_eq!(offset_of!(CpuLocal, apic_id), 4);
            // P5-3b-2：入口汇编经 `%gs` 直接读写的字段必须与 prima-ostd 常量一致。
            #[cfg(feature = "entry-percpu-probe")]
            {
                use prima_ostd::entry::{
                    GS_ENTRY_SLOT_OFF, GS_SYSCALL_STACK_TOP_OFF, GS_USER_RSP_OFF,
                };
                assert_eq!(
                    offset_of!(CpuLocal, syscall_stack_top),
                    GS_SYSCALL_STACK_TOP_OFF
                );
                assert_eq!(offset_of!(CpuLocal, user_rsp), GS_USER_RSP_OFF);
                assert_eq!(offset_of!(CpuLocal, entry_slot), GS_ENTRY_SLOT_OFF);
            }
            let mut bases = [0u64; MAX_CPUS];
            for (cpu, slot) in bases.iter_mut().enumerate() {
                *slot = area_ptr(cpu);
                assert_eq!(*slot % 4, 0);
            }
            for i in 0..bases.len() {
                for j in 0..i {
                    assert_ne!(bases[i], bases[j], "CPU {i} 与 {j} 共享 GS 区");
                }
            }
            assert_eq!(bases[BSP], area_ptr(BSP));
            assert_eq!(apic_id_on(BSP), 0);
        }

        #[test]
        #[should_panic(expected = "cpu index out of range")]
        fn gs_area_ptr_rejects_out_of_range() {
            area_ptr(MAX_CPUS);
        }
    }
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

    /// M3：SSE 契约只清 EM/TS、只置 OSFXSR/OSXMMEXCPT；PAE 与 WP 等既有位必须保留
    /// （清掉 PAE 会立即改变分页模式，清掉 WP 会破坏 W^X 的 supervisor 写保护证据）。
    #[test]
    fn sse_bit_masks_preserve_unrelated_control_bits() {
        assert_eq!(CR0_EM, 1 << 2);
        assert_eq!(CR0_TS, 1 << 3);
        assert_eq!(CR4_OSFXSR, 1 << 9);
        assert_eq!(CR4_OSXMMEXCPT, 1 << 10);

        // bootloader 观察值：CR4 只有 PAE（0x20）。
        assert_eq!(cr4_for_sse(0x20), 0x20 | 0x200 | 0x400);
        assert!(cr4_for_sse(0x20) & 0x20 != 0);

        // CR0：保留 WP(16)、PE(0)、PG(31)，只清 EM/TS。
        let cr0 = 0x8000_0001 | CR0_WP | CR0_EM | CR0_TS;
        assert_eq!(cr0_for_sse(cr0), 0x8000_0001 | CR0_WP);
        assert_eq!(cr0_for_sse(0), 0);
        // 幂等。
        assert_eq!(cr0_for_sse(cr0_for_sse(cr0)), cr0_for_sse(cr0));
        assert_eq!(cr4_for_sse(cr4_for_sse(0x20)), cr4_for_sse(0x20));
    }
}
