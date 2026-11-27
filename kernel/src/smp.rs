//! P5-2/P5-3a：AP 启动与每 CPU 特权状态（仅在 `smp-ap` feature 下编译）。
//!
//! 流程：为 AP 建立只共享内核高半区、并把跳板页标识映射到低地址的页表根；复制
//! [`prima_ostd::smp`] 跳板模板到 1 MiB 以下的可执行帧；经 LAPIC INIT-SIPI-SIPI 唤醒；
//! AP 在长模式下跳到 [`ap_entry`]，装载本 CPU 的 GDT/TSS 与 BSP 的只读 IDT，置在线位并
//! `hlt`。
//!
//! **边界**：AP 不参与调度、不接收中断（`hlt` 且 IF=0），不处理 x2APIC MSR 发送路径；
//! 这不是 SMP 验收。页帧经注册表归属（保留标签 `SMP_FRAME_SEGMENT`），AP 根共享 BSP
//! 高半区页表，低半区只映射跳板页。

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::acpi::CpuTopology;
use crate::mem::phys_to_virt;
use crate::smp_probe::Report;
use prima_ostd::mm::{read_u32, read_u64, write_u32, write_u64};

/// 帧注册表里给 AP 跳板使用的保留归属标签（不暴露为 Capability）。
const SMP_FRAME_SEGMENT: u32 = u32::MAX;
const AP_STACK_SIZE: usize = 16 * 1024;
/// SIPI 向量只能寻址 1 MiB 以下；避开 0 与 EBDA/显存区。
const LOW_TRAMP_MIN: u64 = 0x1000;
const LOW_TRAMP_MAX: u64 = 0x9f000;
const LOW_FRAME_TRIES: usize = 32;

/// LAPIC MMIO 与跳板页的内核窗口 VA（PML4 槽 510/509）；使用前要求该槽为空。
const LAPIC_WINDOW_VA: u64 = 0xffff_ff00_0000_0000;
const TRAMP_WINDOW_VA: u64 = 0xffff_fe80_0000_0000;
const LAPIC_TPR: u32 = 0x80;
const LAPIC_SVR: u32 = 0xf0;
const LAPIC_ICR_LOW: u32 = 0x300;
const LAPIC_ICR_HIGH: u32 = 0x310;
const ICR_DELIVERY_STATUS: u32 = 1 << 12;
/// INIT：delivery mode 5、level assert、level trigger。
const ICR_INIT: u32 = 0x000c_4500;
/// SIPI：delivery mode 6（vector 在低 8 位）。
const ICR_SIPI: u32 = 0x0000_0600;
const IA32_APIC_BASE: u32 = 0x1b;
const APIC_BASE_ENABLE: u64 = 1 << 11;

/// 在线位图：bit 0 = BSP（固定），bit n = AP 索引 n。
static ONLINE: AtomicU32 = AtomicU32::new(0);
/// 诊断：最近一次触碰的地址（panic 时由 panic handler 打印）。
static LAST_ADDR: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

/// 每个 AP 从自身 GDTR/TR 解出的 TSS 描述符基址；0 = 未装载。
static AP_TSS_BASE: [AtomicU64; crate::cpu::local::MAX_CPUS] =
    [const { AtomicU64::new(0) }; crate::cpu::local::MAX_CPUS];

/// 诊断用：最近触碰的地址。
pub fn last_addr() -> u64 {
    LAST_ADDR.load(Ordering::Relaxed)
}

#[repr(align(16))]
struct ApStack(#[allow(dead_code)] [u8; AP_STACK_SIZE]); // 只取地址作栈顶

impl ApStack {
    const fn new() -> Self {
        Self([0; AP_STACK_SIZE])
    }
}

static AP_STACKS: [ApStack; crate::cpu::local::MAX_CPUS] =
    [const { ApStack::new() }; crate::cpu::local::MAX_CPUS];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApError {
    SegmentLedger,
    Frame,
    PageTable,
    NoLowPage,
    LapicMapping,
    LapicTimeout,
    TooManyCpus,
    X2apicUnsupported,
    ApTimeout(u32),
    PerCpuInit(u32),
}

impl ApError {
    pub fn as_str(self) -> &'static str {
        match self {
            ApError::SegmentLedger => "ap-frame-ledger",
            ApError::Frame => "ap-frame",
            ApError::PageTable => "ap-page-table",
            ApError::NoLowPage => "ap-low-page",
            ApError::LapicMapping => "ap-lapic-mapping",
            ApError::LapicTimeout => "ap-lapic-timeout",
            ApError::TooManyCpus => "ap-too-many-cpus",
            ApError::X2apicUnsupported => "ap-x2apic-unsupported",
            ApError::ApTimeout(_) => "ap-online-timeout",
            ApError::PerCpuInit(_) => "ap-percpu-init",
        }
    }
}

fn cycles() -> u64 {
    prima_ostd::arch::timestamp_counter()
}

fn delay_cycles(limit: u64) {
    let start = cycles();
    while cycles().wrapping_sub(start) < limit {
        core::hint::spin_loop();
    }
}

fn stack_top(index: u32) -> u64 {
    let base = core::ptr::addr_of!(AP_STACKS[index as usize]) as u64;
    base + AP_STACK_SIZE as u64
}

/// AP 在长模式下的入口：装载本 CPU 的 GDT/TSS 与共享 IDT，置在线位后停放。
///
/// IF 仍为 0：IDT 已有门，但 AP 不参与中断/调度；装载失败不置在线位，由 BSP 超时判失败。
extern "C" fn ap_entry(index: u32) -> ! {
    crate::cpu::enable_sse();
    if crate::gdt::init_cpu_gdt(index as usize, stack_top(index)).is_ok() {
        crate::idt::load_shared();
        AP_TSS_BASE[index as usize]
            .store(crate::gdt::loaded_tss_base().unwrap_or(0), Ordering::SeqCst);
    }
    ONLINE.fetch_or(1u32 << index, Ordering::SeqCst);
    loop {
        prima_ostd::arch::hlt();
    }
}

/// 校验 BSP+已上线 AP 的 TSS 基址互异，且每个 AP 从自身 GDTR/TR 读回本槽描述符基址。
fn verify_per_cpu(aps: u32) -> Result<(), ApError> {
    let mut bases = [0u64; crate::cpu::local::MAX_CPUS];
    for cpu in 0..=aps as usize {
        let Some(base) = crate::gdt::tss_base(cpu) else {
            return Err(ApError::PerCpuInit(cpu as u32));
        };
        if base == 0 || bases[..cpu].contains(&base) {
            return Err(ApError::PerCpuInit(cpu as u32));
        }
        bases[cpu] = base;
    }
    if crate::gdt::loaded_tss_base() != Some(bases[0]) {
        return Err(ApError::PerCpuInit(0));
    }
    for index in 1..=aps as usize {
        if AP_TSS_BASE[index].load(Ordering::SeqCst) != bases[index] {
            return Err(ApError::PerCpuInit(index as u32));
        }
    }
    Ok(())
}

fn allocate_low_frame() -> Result<u64, ApError> {
    for _ in 0..LOW_FRAME_TRIES {
        let frame =
            crate::frames::allocate_for_segment(SMP_FRAME_SEGMENT).map_err(|_| ApError::Frame)?;
        if (LOW_TRAMP_MIN..LOW_TRAMP_MAX).contains(&frame) {
            return Ok(frame);
        }
    }
    Err(ApError::NoLowPage)
}

/// 复制 BSP 高半区 PML4 项，低半区留空，再把跳板页标识映射进去。
fn build_ap_root(tramp_phys: u64, phys_offset: u64) -> Result<u64, ApError> {
    let bsp_root = crate::cpu::cr3_phys();
    let ap_root = crate::frames::allocate_table_frame().map_err(|_| ApError::Frame)?;
    crate::mem::zero_frame(ap_root, phys_offset);
    for index in 1..512u64 {
        let entry = read_u64(phys_to_virt(bsp_root + index * 8, phys_offset));
        write_u64(phys_to_virt(ap_root + index * 8, phys_offset), entry);
    }
    crate::vspace::map_4k(
        ap_root,
        tramp_phys,
        |phys| phys_to_virt(phys, phys_offset),
        || crate::frames::allocate_table_frame().ok(),
        tramp_phys,
        true,
        false,
        |frame| {
            let _ = crate::frames::release_table_frame(frame);
        },
    )
    .map_err(|_| ApError::PageTable)?;
    Ok(ap_root)
}

/// 模板里的 GDTR 基址与远跳偏移不保证 4 字节对齐，逐字节写避免对齐前置条件。
fn store_u32_le(va: u64, value: u32) {
    for (index, byte) in value.to_le_bytes().iter().enumerate() {
        prima_ostd::mm::write_u8(va + index as u64, *byte);
    }
}

fn write_trampoline(
    page: u64,
    base: u64,
    ap_root: u64,
    index: u32,
    entry: u64,
    note: &mut dyn FnMut(&'static str),
) -> Result<(), ApError> {
    let template = prima_ostd::smp::template();
    if template.len() != prima_ostd::smp::TEMPLATE_LEN {
        return Err(ApError::Frame);
    }
    LAST_ADDR.store(base, Ordering::Relaxed);
    note("tramp-copy");
    for (offset, byte) in template.iter().enumerate() {
        prima_ostd::mm::write_u8(base + offset as u64, *byte);
    }
    let jumps = prima_ostd::smp::far_jumps();
    LAST_ADDR.store(base + prima_ostd::smp::CR3_OFF as u64, Ordering::Relaxed);
    note("tramp-cr3");
    write_u32(base + prima_ostd::smp::CR3_OFF as u64, ap_root as u32);
    write_u32(base + prima_ostd::smp::INDEX_OFF as u64, index);
    write_u64(
        base + prima_ostd::smp::STACK_TOP_OFF as u64,
        stack_top(index),
    );
    write_u64(base + prima_ostd::smp::ENTRY_OFF as u64, entry);
    // GDTR 的 limit（0x1f）由模板提供，只需改写 32 位基址。
    store_u32_le(
        base + prima_ostd::smp::GDTR_OFF as u64 + 2,
        (page + prima_ostd::smp::GDT_OFF as u64) as u32,
    );
    store_u32_le(
        base + jumps.pm32 as u64,
        (page + jumps.pm32_target as u64) as u32,
    );
    store_u32_le(
        base + jumps.lm64 as u64,
        (page + jumps.lm64_target as u64) as u32,
    );
    Ok(())
}

/// 在 BSP 根上为一张物理页建立可写内核窗口；窗口槽必须为空。
fn map_window(window_va: u64, phys: u64, phys_offset: u64) -> Result<u64, ApError> {
    let bsp_root = crate::cpu::cr3_phys();
    let slot = (window_va >> 39) & 0x1ff;
    let entry = read_u64(phys_to_virt(bsp_root + slot * 8, phys_offset));
    if entry & 1 != 0 {
        return Err(ApError::LapicMapping);
    }
    crate::vspace::map_4k(
        bsp_root,
        window_va,
        |frame| phys_to_virt(frame, phys_offset),
        || crate::frames::allocate_table_frame().ok(),
        phys,
        true,
        false,
        |frame| {
            let _ = crate::frames::release_table_frame(frame);
        },
    )
    .map_err(|_| ApError::LapicMapping)?;
    Ok(window_va)
}

fn enable_lapic(lapic: u64) {
    let base_msr = crate::msr::rdmsr(IA32_APIC_BASE);
    if base_msr & APIC_BASE_ENABLE == 0 {
        crate::msr::wrmsr(IA32_APIC_BASE, base_msr | APIC_BASE_ENABLE);
    }
    let svr = read_u32(lapic + u64::from(LAPIC_SVR));
    if svr & 0x100 == 0 {
        write_u32(lapic + u64::from(LAPIC_SVR), 0x1ff);
    }
    write_u32(lapic + u64::from(LAPIC_TPR), 0);
}

fn wait_icr(lapic: u64) -> Result<(), ApError> {
    for _ in 0..10_000_000u32 {
        if read_u32(lapic + u64::from(LAPIC_ICR_LOW)) & ICR_DELIVERY_STATUS == 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(ApError::LapicTimeout)
}

fn send_ipi(lapic: u64, apic_id: u32, command: u32) -> Result<(), ApError> {
    wait_icr(lapic)?;
    write_u32(lapic + u64::from(LAPIC_ICR_HIGH), apic_id << 24);
    write_u32(lapic + u64::from(LAPIC_ICR_LOW), command);
    wait_icr(lapic)
}

fn wait_online(index: u32) -> Result<(), ApError> {
    let start = cycles();
    while cycles().wrapping_sub(start) < 2_000_000_000 {
        if ONLINE.load(Ordering::SeqCst) & (1 << index) != 0 {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err(ApError::ApTimeout(index))
}

/// 启动固件报告的所有非 BSP CPU，返回成功上线的 AP 数。
///
/// `report` 来自 [`crate::smp_probe::run`]；调用方须已在同一地址空间完成物理直映射。
pub fn start_aps(
    report: &Report,
    phys_offset: u64,
    note: &mut dyn FnMut(&'static str),
) -> Result<u32, ApError> {
    let topology: CpuTopology = report.topology;
    if topology.count() > crate::cpu::local::MAX_CPUS {
        return Err(ApError::TooManyCpus);
    }
    if topology.count() <= 1 {
        ONLINE.store(1, Ordering::SeqCst);
        return Ok(0);
    }
    if report.x2apic {
        return Err(ApError::X2apicUnsupported);
    }
    crate::frames::open_segment(SMP_FRAME_SEGMENT, crate::frames::Pool::Cpu)
        .map_err(|_| ApError::SegmentLedger)?;
    note("frame");
    let tramp_phys = allocate_low_frame()?;
    note("root");
    let ap_root = build_ap_root(tramp_phys, phys_offset)?;
    note("tramp-map");
    let tramp_va = map_window(TRAMP_WINDOW_VA, tramp_phys, phys_offset)?;
    note("lapic-map");
    let lapic = map_window(LAPIC_WINDOW_VA, u64::from(report.lapic_phys), phys_offset)?;
    note("lapic-enable");
    enable_lapic(lapic);
    note("lapic-enabled");
    ONLINE.store(1, Ordering::SeqCst);

    let vector = (tramp_phys >> 12) as u8;
    let mut aps = 0u32;
    for order in 0..topology.count() {
        let apic_id = topology.apic_id(order).expect("topology index in range");
        if apic_id == report.bsp_apic_id {
            continue;
        }
        let index = aps + 1;
        if index as usize >= crate::cpu::local::MAX_CPUS {
            return Err(ApError::TooManyCpus);
        }
        note("tramp");
        write_trampoline(
            tramp_phys,
            tramp_va,
            ap_root,
            index,
            ap_entry as *const () as usize as u64,
            note,
        )?;
        note("init");
        send_ipi(lapic, apic_id, ICR_INIT)?;
        delay_cycles(20_000_000);
        note("sipi");
        send_ipi(lapic, apic_id, ICR_SIPI | u32::from(vector))?;
        delay_cycles(2_000_000);
        send_ipi(lapic, apic_id, ICR_SIPI | u32::from(vector))?;
        note("online");
        wait_online(index)?;
        aps += 1;
    }
    verify_per_cpu(aps)?;
    Ok(aps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_tops_are_distinct_aligned_and_in_range() {
        let first = stack_top(1);
        let second = stack_top(2);
        assert_ne!(first, second);
        assert_eq!(first % 16, 0);
        assert_eq!(second % 16, 0);
        assert!(first > second || second > first);
    }

    /// P5-3：BSP 与每个 AP 槽都指向不同的 TSS。
    #[test]
    fn per_cpu_tss_bases_are_all_distinct() {
        let mut bases = [0u64; crate::cpu::local::MAX_CPUS];
        for (cpu, slot) in bases.iter_mut().enumerate() {
            *slot = crate::gdt::tss_base(cpu).expect("cpu slot exists");
            assert_ne!(*slot, 0);
        }
        for i in 0..bases.len() {
            for j in 0..i {
                assert_ne!(bases[i], bases[j]);
            }
        }
    }
}
