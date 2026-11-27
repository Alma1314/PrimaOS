//! GS-2：真实 NMI 自注入矩阵（仅在 `gs-probe` feature 下编译）。
//!
//! 目的：证明「NMI 命中用户态 / 内核半交换窗口 / 嵌套」都不会破坏 GS 基址。做法：
//! 在 BSP 上用 LAPIC ICR 的 **NMI delivery + self shorthand** 向本 CPU 注入真实 NMI，
//! 该 NMI 经 IDT vector 2 的 **IST1 栈**交付到 [`prima_ostd::entry::nmi_entry`]——该 handler
//! 只 `lock inc` 一个计数器后 `iretq`，不触碰任何 GPR/GS。于是无论 GS 处于哪个半交换态，
//! 处理完都原样返回；测试在注入前后读 `IA32_GS_BASE`/`IA32_KERNEL_GS_BASE` 对账。
//!
//! 边界：单 CPU；AP 不使用 NMI；不验证 NMI 的嵌套重入（单次 NMI 不会重入），也不构成
//! 对 NMI 之外的 maskable 中断的证明（那由入口 `swapgs` 矩阵与 ring3 探针覆盖）。

use crate::mem::phys_to_virt;
use prima_ostd::mm::{read_u32, read_u64, write_u32};

const IA32_APIC_BASE: u32 = 0x1b;
const APIC_BASE_ENABLE: u64 = 1 << 11;

const LAPIC_WINDOW_VA: u64 = 0xffff_ff00_0000_0000;
const LAPIC_TPR: u32 = 0x80;
const LAPIC_SVR: u32 = 0xf0;
const LAPIC_ICR_LOW: u32 = 0x300;
const LAPIC_ICR_HIGH: u32 = 0x310;
/// ICR：delivery mode = NMI(100, bits 10:8)、dest shorthand = self(01, bits 19:18)。
const ICR_SELF_NMI: u32 = 0x0004_0400;

/// 一个非零的用户 GS 值，用于模拟用户态/内核半交换两态。
const USER_GS: u64 = 0x0000_0A00_0000_4000;
/// 半交换窗口注入轮数。
const WINDOW_ROUNDS: u32 = 8;

/// NMI 自注入矩阵的见证。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NmiWitness {
    /// 内核已交换态（GS=0、KERNEL_GS=user）注入一次后计数为 1 且 GS 不变。
    pub kernel_swapped_ok: bool,
    /// 用户态（GS=user、KERNEL_GS=0）注入一次后计数为 1 且 GS 不变。
    pub userlike_ok: bool,
    /// 半交换窗口注入 `WINDOW_ROUNDS` 次全部交付，且末态 GS 复原。
    pub window_ok: bool,
    /// 半交换窗口实际交付的 NMI 数（应等于 `WINDOW_ROUNDS`）。
    pub window_delivered: u32,
    /// 最近一次 NMI 的 handler 栈指针落在独立 IST 栈内（证明真正用了 IST）。
    pub ist_ok: bool,
    /// 内核半交换态下触发 CPL0 #PF（嵌套语义）后 GS 仍不为入口所动。
    pub nested_pf_ok: bool,
}

impl NmiWitness {
    /// 五项全部成立。
    pub fn evidence_ok(&self) -> bool {
        self.kernel_swapped_ok
            && self.userlike_ok
            && self.window_ok
            && self.ist_ok
            && self.nested_pf_ok
    }
}

/// 探测失败原因（打印为 `GS|nmi-fail|<reason>`）。
pub type NmiError = &'static str;

fn set_gs(gs_base: u64, kernel_gs: u64) {
    crate::msr::wrmsr(crate::msr::IA32_GS_BASE, gs_base);
    crate::msr::wrmsr(crate::msr::IA32_KERNEL_GS_BASE, kernel_gs);
}

fn gs_state() -> (u64, u64) {
    (
        crate::msr::rdmsr(crate::msr::IA32_GS_BASE),
        crate::msr::rdmsr(crate::msr::IA32_KERNEL_GS_BASE),
    )
}

/// 在 BSP 当前根上为 LAPIC 物理页建立可写内核窗口；槽必须为空。
fn map_lapic_window(phys_offset: u64, lapic_phys: u64) -> Result<u64, NmiError> {
    let root = crate::cpu::cr3_phys();
    let slot = (LAPIC_WINDOW_VA >> 39) & 0x1ff;
    if read_u64(phys_to_virt(root + slot * 8, phys_offset)) & 1 != 0 {
        return Err("lapic-slot-occupied");
    }
    crate::vspace::map_4k(
        root,
        LAPIC_WINDOW_VA,
        |frame| phys_to_virt(frame, phys_offset),
        || crate::frames::allocate_table_frame().ok(),
        lapic_phys,
        true,
        false,
        |frame| {
            let _ = crate::frames::release_table_frame(frame);
        },
    )
    .map_err(|_| "lapic-mapping")?;
    Ok(LAPIC_WINDOW_VA)
}

fn enable_lapic(lapic: u64) {
    let base = crate::msr::rdmsr(IA32_APIC_BASE);
    if base & APIC_BASE_ENABLE == 0 {
        crate::msr::wrmsr(IA32_APIC_BASE, base | APIC_BASE_ENABLE);
    }
    let svr = read_u32(lapic + u64::from(LAPIC_SVR));
    if svr & 0x100 == 0 {
        write_u32(lapic + u64::from(LAPIC_SVR), 0x1ff);
    }
    write_u32(lapic + u64::from(LAPIC_TPR), 0);
}

fn send_self_nmi(lapic: u64) {
    write_u32(lapic + u64::from(LAPIC_ICR_HIGH), 0);
    // self+shorthand 的 NMI 在本次 MMIO 写完成后的下一指令边界被交付。
    write_u32(lapic + u64::from(LAPIC_ICR_LOW), ICR_SELF_NMI);
}

/// 自旋等待 NMI 计数达到 `target`；超时返回错误（说明 QEMU 未交付自注入 NMI）。
fn wait_count(target: u32) -> Result<(), NmiError> {
    let start = prima_ostd::arch::timestamp_counter();
    while prima_ostd::arch::timestamp_counter().wrapping_sub(start) < 2_000_000_000 {
        if prima_ostd::entry::nmi_count() >= target {
            return Ok(());
        }
        core::hint::spin_loop();
    }
    Err("nmi-timeout")
}

/// 运行 NMI 自注入矩阵。`phys_offset` 为物理直映射偏移。
pub fn run(phys_offset: u64) -> Result<NmiWitness, NmiError> {
    let lapic_phys = crate::msr::rdmsr(IA32_APIC_BASE) & 0x000f_ffff_ffff_f000;
    let lapic = map_lapic_window(phys_offset, lapic_phys)?;
    enable_lapic(lapic);

    // T1：内核已交换态（GS=0、KERNEL_GS=user），不 swapgs 直接注入。
    set_gs(0, USER_GS);
    prima_ostd::entry::reset_nmi_count();
    send_self_nmi(lapic);
    wait_count(1)?;
    let kernel_swapped_ok = prima_ostd::entry::nmi_count() == 1 && gs_state() == (0, USER_GS);

    // T2：用户态（GS=user、KERNEL_GS=0），不 swapgs 直接注入。
    set_gs(USER_GS, 0);
    prima_ostd::entry::reset_nmi_count();
    send_self_nmi(lapic);
    wait_count(1)?;
    let userlike_ok = prima_ostd::entry::nmi_count() == 1 && gs_state() == (USER_GS, 0);

    // T3：半交换窗口——`swapgs; 注入; 等交付; swapgs` 每轮都在“已交换”的奇数态命中 NMI。
    set_gs(USER_GS, 0);
    prima_ostd::entry::reset_nmi_count();
    let mut delivered = 0u32;
    for round in 0..WINDOW_ROUNDS {
        prima_ostd::arch::swapgs(); // GS=0、KERNEL_GS=user（半交换态）
        send_self_nmi(lapic);
        wait_count(round + 1)?;
        delivered += 1;
        prima_ostd::arch::swapgs(); // 复原 GS=user、KERNEL_GS=0
    }
    let window_ok = delivered == WINDOW_ROUNDS && gs_state() == (USER_GS, 0);

    // NMI handler 记录的栈指针必须落在独立 IST 栈区间内。
    let (lo, hi) = crate::gdt::nmi_stack_bounds();
    let recorded = prima_ostd::entry::nmi_rsp();
    let ist_ok = recorded >= lo && recorded < hi;

    // T4：内核半交换态（GS=0、KERNEL_GS=user）下触发一次 **CPL0 #PF**（嵌套语义）。#PF 的
    // CPL0 路径不做 swapgs，故处理完 GS 应原样不变。用已知未映射地址（boot 亦用此地址）。
    set_gs(0, USER_GS);
    crate::idt::trigger_page_fault(0x0000_0000_0004_0000);
    let nested_pf_ok = gs_state() == (0, USER_GS);

    // 复原两 MSR，交给后续 gs-probe 流程重新设定。
    set_gs(0, 0);

    Ok(NmiWitness {
        kernel_swapped_ok,
        userlike_ok,
        window_ok,
        window_delivered: delivered,
        ist_ok,
        nested_pf_ok,
    })
}
