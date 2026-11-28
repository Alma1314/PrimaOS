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

/// GS-3：各 CPU 经 `%gs:0` 读到的索引（AP 在自己的启动入口测量）。
#[cfg(feature = "gs3-probe")]
static GS_IDX_SEEN: [AtomicU32; crate::cpu::local::MAX_CPUS] =
    [const { AtomicU32::new(u32::MAX) }; crate::cpu::local::MAX_CPUS];
/// GS-3：各 CPU 的 `IA32_GS_BASE`（应等于本 CPU 的 GS 区基址）。
#[cfg(feature = "gs3-probe")]
static GS_BASE_SEEN: [AtomicU64; crate::cpu::local::MAX_CPUS] =
    [const { AtomicU64::new(0) }; crate::cpu::local::MAX_CPUS];
/// GS-3：各 CPU 经 `%gs:4` 读到的自身 APIC ID。
#[cfg(feature = "gs3-probe")]
static GS_APIC_SEEN: [AtomicU32; crate::cpu::local::MAX_CPUS] =
    [const { AtomicU32::new(0) }; crate::cpu::local::MAX_CPUS];
/// GS-3：各 CPU 的 CPUID APIC ID（用于证明各索引确由不同 CPU 执行）。
#[cfg(feature = "gs3-probe")]
static GS_CPUID_SEEN: [AtomicU32; crate::cpu::local::MAX_CPUS] =
    [const { AtomicU32::new(0) }; crate::cpu::local::MAX_CPUS];
/// GS-3：MADT 期望的每 CPU APIC ID（P5-4 IPI 目标的同一来源）。
#[cfg(feature = "gs3-probe")]
static EXPECTED_APIC: [AtomicU32; crate::cpu::local::MAX_CPUS] =
    [const { AtomicU32::new(0) }; crate::cpu::local::MAX_CPUS];

/// GS-3：`-smp` 探针的每 CPU GS 区见证。四个 `*_ok` 计数都等于 `cpus` 才算通过：
/// 每个 CPU 经 `%gs:0` 读到自己的索引、GS 基址等于自己的区、`%gs:4` 与 CPUID
/// 均等于 MADT APIC ID。
#[cfg(feature = "gs3-probe")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GsWitness {
    pub cpus: u32,
    pub index_ok: u32,
    pub base_ok: u32,
    pub apic_ok: u32,
    pub cpuid_ok: u32,
}

#[cfg(feature = "gs3-probe")]
impl GsWitness {
    pub fn evidence_ok(&self) -> bool {
        self.index_ok == self.cpus
            && self.base_ok == self.cpus
            && self.apic_ok == self.cpus
            && self.cpuid_ok == self.cpus
    }
}

/// GS-3 诊断：已收集的四种测量值（按 CPU 索引），仅失败报告使用。
#[cfg(feature = "gs3-probe")]
pub fn gs_progress() -> (
    [u32; crate::cpu::local::MAX_CPUS],
    [u64; crate::cpu::local::MAX_CPUS],
    [u32; crate::cpu::local::MAX_CPUS],
    [u32; crate::cpu::local::MAX_CPUS],
) {
    let mut indexes = [0u32; crate::cpu::local::MAX_CPUS];
    let mut bases = [0u64; crate::cpu::local::MAX_CPUS];
    let mut apics = [0u32; crate::cpu::local::MAX_CPUS];
    let mut cpuids = [0u32; crate::cpu::local::MAX_CPUS];
    for cpu in 0..crate::cpu::local::MAX_CPUS {
        indexes[cpu] = GS_IDX_SEEN[cpu].load(Ordering::Relaxed);
        bases[cpu] = GS_BASE_SEEN[cpu].load(Ordering::Relaxed);
        apics[cpu] = GS_APIC_SEEN[cpu].load(Ordering::Relaxed);
        cpuids[cpu] = GS_CPUID_SEEN[cpu].load(Ordering::Relaxed);
    }
    (indexes, bases, apics, cpuids)
}

/// GS-3：从 CPUID leaf 1 EBX[31:24] 读本 CPU 的初始 APIC ID（AP 自识别）。
#[cfg(feature = "gs3-probe")]
fn cpuid_apic_id() -> u32 {
    (prima_ostd::arch::cpuid(1, 0)[1] >> 24) & 0xff
}

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

/// 指定 AP 的内核栈区间 `[base, top)`（入口探针核对保存块基址用）。
#[cfg(feature = "entry-percpu-probe")]
pub fn ap_stack_bounds(index: u32) -> (u64, u64) {
    let base = core::ptr::addr_of!(AP_STACKS[index as usize]) as u64;
    (base, base + AP_STACK_SIZE as u64)
}

/// AP 在长模式下的入口：装载本 CPU 的 GDT/TSS 与共享 IDT，置在线位后停放。
///
/// IF 仍为 0：IDT 已有门，但 AP 不参与中断/调度；装载失败不置在线位，由 BSP 超时判失败。
extern "C" fn ap_entry(index: u32) -> ! {
    // GS-3：先安装本 CPU 的 GS 区，再经 `%gs` 自测。测量放在启动入口而非等 BSP
    // 会合：单线程 TCG 下忙等的 vCPU 不保证被再次调度，入口执行则是确定的。
    #[cfg(feature = "gs3-probe")]
    {
        let cpu = index as usize;
        let cpuid_apic = cpuid_apic_id();
        crate::cpu::local::install_gs(cpu, cpuid_apic);
        GS_IDX_SEEN[cpu].store(crate::cpu::local::current_index() as u32, Ordering::Relaxed);
        GS_BASE_SEEN[cpu].store(
            crate::msr::rdmsr(crate::msr::IA32_GS_BASE),
            Ordering::Relaxed,
        );
        GS_APIC_SEEN[cpu].store(crate::cpu::local::local_apic_id(), Ordering::Relaxed);
        GS_CPUID_SEEN[cpu].store(cpuid_apic, Ordering::Relaxed);
    }
    crate::cpu::enable_sse();
    let mut ready = false;
    if crate::gdt::init_cpu_gdt(index as usize, stack_top(index)).is_ok() {
        // 装载本 CPU GDT 后，旧的 CS 选择子可能在新表中不再指向代码段；任何中断/异常
        // 返回都会按新表重新校验它。用远返回把 CS 切到本表的内核代码段。
        prima_ostd::arch::reload_code_segment(crate::gdt::KERNEL_CODE_SELECTOR);
        crate::idt::load_shared();
        AP_TSS_BASE[index as usize]
            .store(crate::gdt::loaded_tss_base().unwrap_or(0), Ordering::SeqCst);
        // P5-3b-1：本 CPU 执行一次 CPL0 `int 0x80` 与 #PF，写本 CPU 的入口现场槽。
        #[cfg(feature = "entry-percpu-probe")]
        {
            let (stack_lo, stack_hi) = ap_stack_bounds(index);
            crate::entry_probe::run_local(index as usize, stack_lo, stack_hi);
        }
        ready = true;
    }
    // P5-3b-2：本 CPU 从 CPL3 执行用户探针；其收尾回调置在线位并停放（不返回）。
    #[cfg(feature = "entry-percpu-probe")]
    if ready {
        let offset = AP_PHYS_OFFSET.load(Ordering::SeqCst);
        crate::entry_probe::run_user_local(index as usize, offset, 0);
    }
    ONLINE.fetch_or(1u32 << index, Ordering::SeqCst);
    loop {
        prima_ostd::arch::hlt();
    }
}

/// P5-3b-2：AP 用户探针收尾时置在线位（由 `entry_probe::finish` 调用）。
#[cfg(feature = "entry-percpu-probe")]
pub fn mark_online(index: u32) {
    ONLINE.fetch_or(1u32 << index, Ordering::SeqCst);
}

/// P5-3b-2：AP 用户页构建用的物理直映射偏移（由 [`start_aps`] 写入）。
#[cfg(feature = "entry-percpu-probe")]
static AP_PHYS_OFFSET: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);

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
    #[cfg(feature = "entry-percpu-probe")]
    AP_PHYS_OFFSET.store(phys_offset, Ordering::SeqCst);
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
        #[cfg(feature = "gs3-probe")]
        EXPECTED_APIC[index as usize].store(apic_id, Ordering::Relaxed);
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

/// GS-3：汇总每 CPU 的 `%gs` 自识别见证。各 AP 在启动入口测量（已由
/// [`start_aps`] 逐个等待在线），BSP 此刻补测自身；每项失败只降低对应计数，
/// 由调用方（与 xtask 校验器）要求四个计数全等于 CPU 数。
#[cfg(feature = "gs3-probe")]
pub fn gs_witness(report: &Report, aps: u32) -> GsWitness {
    let cpus = aps + 1;
    EXPECTED_APIC[crate::cpu::local::BSP].store(report.bsp_apic_id, Ordering::Relaxed);
    let bsp = (
        crate::cpu::local::current_index() as u32,
        crate::msr::rdmsr(crate::msr::IA32_GS_BASE),
        crate::cpu::local::local_apic_id(),
        cpuid_apic_id(),
    );
    let mut witness = GsWitness {
        cpus,
        index_ok: 0,
        base_ok: 0,
        apic_ok: 0,
        cpuid_ok: 0,
    };
    for cpu in 0..cpus as usize {
        let (index, base, apic, cpuid) = if cpu == crate::cpu::local::BSP {
            bsp
        } else {
            (
                GS_IDX_SEEN[cpu].load(Ordering::Relaxed),
                GS_BASE_SEEN[cpu].load(Ordering::Relaxed),
                GS_APIC_SEEN[cpu].load(Ordering::Relaxed),
                GS_CPUID_SEEN[cpu].load(Ordering::Relaxed),
            )
        };
        let expected = EXPECTED_APIC[cpu].load(Ordering::Relaxed);
        if index == cpu as u32 {
            witness.index_ok += 1;
        }
        if base == crate::cpu::local::area_ptr(cpu) {
            witness.base_ok += 1;
        }
        if apic == expected {
            witness.apic_ok += 1;
        }
        if cpuid == expected {
            witness.cpuid_ok += 1;
        }
    }
    witness
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
