//! P5-1 读取探测（仅在 `smp-probe` feature 下编译）。
//!
//! 从 `BootInfo.rsdp_addr` 经物理直映射读 ACPI 表，解析 MADT，并用 CPUID 与 LAPIC
//! 读取本 CPU 的 APIC ID 做自识别。**只读**：不改页表、不启动 AP、不发送 IPI，
//! 不写任何全局状态；调用方在探针结束后直接退出（专用 QEMU 命令）。
//!
//! 边界：物理直映射是否覆盖固件保留区由 bootloader 决定；本模块按“调用方给出可读
//! 物理地址”的契约访问，读失败会以确定性错误结束，不伪造结果。

use crate::acpi::{self, CpuTopology};
use crate::mem::phys_to_virt;

/// 单张 ACPI 表的长度上限，防止用坏表头读出巨量内存。
const MAX_TABLE_LEN: usize = 1 << 20;
/// IA32_X2APIC_APICID：x2APIC 模式下读取本 CPU APIC ID 的 MSR。
const IA32_X2APIC_APICID: u32 = 0x802;
/// LAPIC ID 寄存器偏移（xAPIC MMIO，bit 24..31 为 ID）。
const LAPIC_ID_OFFSET: u32 = 0x20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub rsdp_revision: u8,
    pub used_xsdt: bool,
    pub cpus: usize,
    pub bsp_apic_id: u32,
    pub bsp_index: usize,
    pub lapic_phys: u32,
    pub lapic_id: u32,
    pub x2apic: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProbeError {
    Rsdp(acpi::AcpiError),
    BadRootTable,
    BadTableLength,
    MissingMadt,
    Madt(acpi::AcpiError),
    BspNotListed,
    LapicIdMismatch,
}

impl ProbeError {
    pub fn as_str(self) -> &'static str {
        match self {
            ProbeError::Rsdp(_) => "rsdp",
            ProbeError::BadRootTable => "root-table",
            ProbeError::BadTableLength => "table-length",
            ProbeError::MissingMadt => "missing-madt",
            ProbeError::Madt(_) => "madt",
            ProbeError::BspNotListed => "bsp-not-listed",
            ProbeError::LapicIdMismatch => "lapic-id-mismatch",
        }
    }
}

/// 读一张表的完整字节；长度取自表头且必须在 `[36, 1 MiB]`。
fn table(phys: u64, phys_offset: u64) -> Result<&'static [u8], ProbeError> {
    let va = phys_to_virt(phys, phys_offset);
    let header = prima_ostd::mm::bytes_at(va, acpi::TABLE_HEADER_LEN);
    let length = u32::from_le_bytes(header[4..8].try_into().expect("4 bytes")) as usize;
    if !(acpi::TABLE_HEADER_LEN..=MAX_TABLE_LEN).contains(&length) {
        return Err(ProbeError::BadTableLength);
    }
    Ok(prima_ostd::mm::bytes_at(va, length))
}

fn find_madt(root: &[u8], root_is_xsdt: bool, phys_offset: u64) -> Result<u64, ProbeError> {
    let width = if root_is_xsdt { 8 } else { 4 };
    let Some(count) = root
        .len()
        .checked_sub(acpi::TABLE_HEADER_LEN)
        .map(|n| n / width)
    else {
        return Err(ProbeError::BadRootTable);
    };
    for index in 0..count {
        let Some(phys) = acpi::table_phys(root, index) else {
            continue;
        };
        let signature = prima_ostd::mm::bytes_at(phys_to_virt(phys, phys_offset), 4);
        if signature == b"APIC" {
            return Ok(phys);
        }
    }
    Err(ProbeError::MissingMadt)
}

fn read_bsp_apic_id() -> u32 {
    // CPUID leaf 1 EBX[31:24] = 初始 APIC ID。
    (prima_ostd::arch::cpuid(1, 0)[1] >> 24) & 0xff
}

fn read_lapic_id(topology: &CpuTopology, phys_offset: u64) -> u32 {
    if topology.uses_x2apic() {
        crate::msr::rdmsr(IA32_X2APIC_APICID) as u32
    } else {
        let va = phys_to_virt(
            u64::from(topology.local_apic_phys()) + u64::from(LAPIC_ID_OFFSET),
            phys_offset,
        );
        (prima_ostd::mm::read_u32(va) >> 24) & 0xff
    }
}

/// 执行一次只读探测。`rsdp_phys` 来自 `BootInfo.rsdp_addr`（物理地址）。
pub fn run(rsdp_phys: u64, phys_offset: u64) -> Result<Report, ProbeError> {
    let rsdp_va = phys_to_virt(rsdp_phys, phys_offset);
    let rsdp_bytes = prima_ostd::mm::bytes_at(rsdp_va, acpi::TABLE_HEADER_LEN);
    let rsdp = acpi::parse_rsdp(rsdp_bytes).map_err(ProbeError::Rsdp)?;
    let revision = rsdp_bytes[15];

    let (root_phys, used_xsdt) = match rsdp.xsdt_phys {
        Some(xsdt) => (xsdt, true),
        None => (u64::from(rsdp.rsdt_phys), false),
    };
    let root = table(root_phys, phys_offset)?;
    let madt_phys = find_madt(root, used_xsdt, phys_offset)?;
    let madt = table(madt_phys, phys_offset)?;
    let topology = acpi::parse_madt(madt).map_err(ProbeError::Madt)?;

    let bsp_apic_id = read_bsp_apic_id();
    let bsp_index = topology
        .index_of(bsp_apic_id)
        .ok_or(ProbeError::BspNotListed)?;
    let lapic_id = read_lapic_id(&topology, phys_offset);
    if lapic_id != bsp_apic_id {
        return Err(ProbeError::LapicIdMismatch);
    }

    Ok(Report {
        rsdp_revision: revision,
        used_xsdt,
        cpus: topology.count(),
        bsp_apic_id,
        bsp_index,
        lapic_phys: topology.local_apic_phys(),
        lapic_id,
        x2apic: topology.uses_x2apic(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 长度字段是坏的（为 0）时不得按 0 读取，必须确定性拒绝。
    #[test]
    fn table_rejects_bad_declared_length() {
        let mut bytes = [0u8; acpi::TABLE_HEADER_LEN];
        bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
        // 直接把同一虚拟地址当作“物理地址 + 0 偏移”读取。
        let base = bytes.as_ptr() as u64;
        assert_eq!(table(base, 0), Err(ProbeError::BadTableLength));

        bytes[4..8].copy_from_slice(&(MAX_TABLE_LEN as u32 + 1).to_le_bytes());
        assert_eq!(table(base, 0), Err(ProbeError::BadTableLength));
    }
}
