//! P5-1：ACPI RSDP/RSDT/XSDT 与 MADT 的纯解析。
//!
//! 只解析调用方给出的字节切片，不做物理地址发现、不读写硬件、不接地任何启动路径；
//! 因此可在 host 单测里用合成表钉住边界。P5-2 才从 `BootInfo.rsdp_addr` 经物理直映射
//! 读取真实表，并用 [`CpuTopology`] 建立 CPU 索引。
//!
//! 边界：只识别 RSDP v0/v2、RSDT/XSDT 表头与 MADT 的 Local APIC（type 0）与
//! x2APIC（type 9）条目；不做 AML、MCFG、IOAPIC 或中断覆盖解析。

/// 平台 CPU 上限，与 CPU 本地槽一致。
pub const MAX_CPUS: usize = crate::cpu::local::MAX_CPUS;

/// ACPI 固定表头长度。
pub const TABLE_HEADER_LEN: usize = 36;
const RSDP_V0_LEN: usize = 20;
const RSDP_V2_LEN: usize = 36;
const MADT_FIXED_LEN: usize = TABLE_HEADER_LEN + 8;
const MADT_ENTRY_START: usize = MADT_FIXED_LEN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcpiError {
    /// 输入短于签名或声明表头。
    TooSmall,
    /// 签名不是期望值。
    BadSignature,
    /// 校验和不为 0。
    BadChecksum,
    /// 声明长度越出输入或小于固定头。
    BadLength,
    /// 可用 CPU 条目超过 [`MAX_CPUS`]。
    TooManyCpus,
    /// 没有启用的 Local APIC/x2APIC 条目。
    NoCpu,
}

/// RSDP 中的系统描述表指针；v0 只有 RSDT。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rsdp {
    pub rsdt_phys: u32,
    pub xsdt_phys: Option<u64>,
}

/// 校验固定表头并返回声明长度（已确认不超过输入）。
fn validate_table(bytes: &[u8], signature: &[u8; 4]) -> Result<usize, AcpiError> {
    if bytes.len() < TABLE_HEADER_LEN {
        return Err(AcpiError::TooSmall);
    }
    if &bytes[0..4] != signature {
        return Err(AcpiError::BadSignature);
    }
    let length = u32::from_le_bytes(bytes[4..8].try_into().expect("4 bytes")) as usize;
    if length < TABLE_HEADER_LEN || length > bytes.len() {
        return Err(AcpiError::BadLength);
    }
    if checksum(&bytes[..length]) != 0 {
        return Err(AcpiError::BadChecksum);
    }
    Ok(length)
}

fn checksum(bytes: &[u8]) -> u8 {
    bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte))
}

/// 解析 RSDP。v0 校验前 20 字节；v2 还要求扩展长度与扩展校验和。
pub fn parse_rsdp(bytes: &[u8]) -> Result<Rsdp, AcpiError> {
    if bytes.len() < RSDP_V0_LEN {
        return Err(AcpiError::TooSmall);
    }
    if &bytes[0..8] != b"RSD PTR " {
        return Err(AcpiError::BadSignature);
    }
    if checksum(&bytes[..RSDP_V0_LEN]) != 0 {
        return Err(AcpiError::BadChecksum);
    }
    let revision = bytes[15];
    if revision == 0 {
        return Ok(Rsdp {
            rsdt_phys: u32::from_le_bytes(bytes[16..20].try_into().expect("4 bytes")),
            xsdt_phys: None,
        });
    }
    if bytes.len() < RSDP_V2_LEN {
        return Err(AcpiError::TooSmall);
    }
    let length = u32::from_le_bytes(bytes[20..24].try_into().expect("4 bytes")) as usize;
    if length < RSDP_V2_LEN || length > bytes.len() {
        return Err(AcpiError::BadLength);
    }
    if checksum(&bytes[..length]) != 0 {
        return Err(AcpiError::BadChecksum);
    }
    let xsdt = u64::from_le_bytes(bytes[24..32].try_into().expect("8 bytes"));
    Ok(Rsdp {
        rsdt_phys: u32::from_le_bytes(bytes[16..20].try_into().expect("4 bytes")),
        xsdt_phys: (xsdt != 0).then_some(xsdt),
    })
}

/// 读取 RSDT（u32 项）或 XSDT（u64 项）的第 `index` 个表地址。
///
/// 只按签名区分项宽；不校验被指向的表，后者由 [`parse_madt`] 负责。
pub fn table_phys(table: &[u8], index: usize) -> Option<u64> {
    if table.len() < TABLE_HEADER_LEN {
        return None;
    }
    let length = u32::from_le_bytes(table[4..8].try_into().ok()?) as usize;
    if length < TABLE_HEADER_LEN || length > table.len() {
        return None;
    }
    if checksum(&table[..length]) != 0 {
        return None;
    }
    match &table[0..4] {
        b"RSDT" => {
            let offset = TABLE_HEADER_LEN.checked_add(index.checked_mul(4)?)?;
            let end = offset.checked_add(4)?;
            (end <= length).then(|| {
                u64::from(u32::from_le_bytes(
                    table[offset..end].try_into().expect("4 bytes"),
                ))
            })
        }
        b"XSDT" => {
            let offset = TABLE_HEADER_LEN.checked_add(index.checked_mul(8)?)?;
            let end = offset.checked_add(8)?;
            (end <= length)
                .then(|| u64::from_le_bytes(table[offset..end].try_into().expect("8 bytes")))
        }
        _ => None,
    }
}

/// MADT 发现的 CPU 拓扑。索引按表中启用条目次序，BSP 归零由 P5-2 负责。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CpuTopology {
    local_apic_phys: u32,
    apic_ids: [u32; MAX_CPUS],
    count: usize,
    x2apic: bool,
}

impl CpuTopology {
    pub fn local_apic_phys(&self) -> u32 {
        self.local_apic_phys
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn apic_id(&self, index: usize) -> Option<u32> {
        (index < self.count).then(|| self.apic_ids[index])
    }

    pub fn index_of(&self, apic_id: u32) -> Option<usize> {
        self.apic_ids[..self.count]
            .iter()
            .position(|id| *id == apic_id)
    }

    pub fn uses_x2apic(&self) -> bool {
        self.x2apic
    }
}

/// 解析 MADT。同时存在 type 0 与 type 9 时优先 x2APIC 条目（ACPI 6 的规则）。
pub fn parse_madt(bytes: &[u8]) -> Result<CpuTopology, AcpiError> {
    let length = validate_table(bytes, b"APIC")?;
    if length < MADT_FIXED_LEN {
        return Err(AcpiError::BadLength);
    }
    let local_apic_phys = u32::from_le_bytes(bytes[36..40].try_into().expect("4 bytes"));

    let mut x2_ids = [0u32; MAX_CPUS];
    let mut x2_count = 0usize;
    let mut x2_overflow = false;
    let mut legacy_ids = [0u32; MAX_CPUS];
    let mut legacy_count = 0usize;
    let mut legacy_overflow = false;

    let mut offset = MADT_ENTRY_START;
    while offset < length {
        if offset + 2 > length {
            return Err(AcpiError::BadLength);
        }
        let entry_type = bytes[offset];
        let entry_len = bytes[offset + 1] as usize;
        if entry_len < 2 || offset + entry_len > length {
            return Err(AcpiError::BadLength);
        }
        let entry = &bytes[offset..offset + entry_len];
        match entry_type {
            0 if entry_len >= 8 => {
                let apic_id = u32::from(entry[3]);
                let flags = u32::from_le_bytes(entry[4..8].try_into().expect("4 bytes"));
                if flags & 1 != 0 {
                    if legacy_count == MAX_CPUS {
                        legacy_overflow = true;
                    } else {
                        legacy_ids[legacy_count] = apic_id;
                        legacy_count += 1;
                    }
                }
            }
            9 if entry_len >= 16 => {
                let apic_id = u32::from_le_bytes(entry[4..8].try_into().expect("4 bytes"));
                let flags = u32::from_le_bytes(entry[8..12].try_into().expect("4 bytes"));
                if flags & 1 != 0 {
                    if x2_count == MAX_CPUS {
                        x2_overflow = true;
                    } else {
                        x2_ids[x2_count] = apic_id;
                        x2_count += 1;
                    }
                }
            }
            _ => {}
        }
        offset += entry_len;
    }

    if x2_count > 0 {
        if x2_overflow {
            return Err(AcpiError::TooManyCpus);
        }
        Ok(CpuTopology {
            local_apic_phys,
            apic_ids: x2_ids,
            count: x2_count,
            x2apic: true,
        })
    } else if legacy_count > 0 {
        if legacy_overflow {
            return Err(AcpiError::TooManyCpus);
        }
        Ok(CpuTopology {
            local_apic_phys,
            apic_ids: legacy_ids,
            count: legacy_count,
            x2apic: false,
        })
    } else {
        Err(AcpiError::NoCpu)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 把 `bytes` 中 `index` 处的校验和字节置为让整段求和为 0 的值。
    fn seal(bytes: &mut [u8], index: usize) {
        bytes[index] = 0;
        let sum = bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        bytes[index] = 0u8.wrapping_sub(sum);
    }

    fn table(signature: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let length = TABLE_HEADER_LEN + body.len();
        let mut bytes = vec![0u8; length];
        bytes[0..4].copy_from_slice(signature);
        bytes[4..8].copy_from_slice(&(length as u32).to_le_bytes());
        bytes[8] = 5; // revision
        bytes[36..].copy_from_slice(body);
        seal(&mut bytes, 9);
        bytes
    }

    fn madt_body(local_apic_phys: u32, entries: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(&local_apic_phys.to_le_bytes());
        body.extend_from_slice(&1u32.to_le_bytes()); // flags
        body.extend_from_slice(entries);
        body
    }

    fn local_apic(apic_id: u8, enabled: bool) -> Vec<u8> {
        let mut entry = vec![0u8; 8];
        entry[0] = 0;
        entry[1] = 8;
        entry[2] = apic_id; // ACPI processor id
        entry[3] = apic_id; // APIC id
        entry[4..8].copy_from_slice(&u32::from(enabled).to_le_bytes());
        entry
    }

    fn x2apic(apic_id: u32, enabled: bool) -> Vec<u8> {
        let mut entry = vec![0u8; 16];
        entry[0] = 9;
        entry[1] = 16;
        entry[4..8].copy_from_slice(&apic_id.to_le_bytes());
        entry[8..12].copy_from_slice(&u32::from(enabled).to_le_bytes());
        entry
    }

    fn rsdp_v2(rsdt: u32, xsdt: u64) -> Vec<u8> {
        let mut bytes = vec![0u8; RSDP_V2_LEN];
        bytes[0..8].copy_from_slice(b"RSD PTR ");
        bytes[15] = 2;
        bytes[16..20].copy_from_slice(&rsdt.to_le_bytes());
        bytes[20..24].copy_from_slice(&(RSDP_V2_LEN as u32).to_le_bytes());
        bytes[24..32].copy_from_slice(&xsdt.to_le_bytes());
        // 首段校验和覆盖前 20 字节；扩展校验和覆盖 36 字节且包含首段结果。
        seal(&mut bytes[..RSDP_V0_LEN], 8);
        seal(&mut bytes, 32);
        bytes
    }

    #[test]
    fn rsdp_v2_requires_both_checksums_and_reports_both_tables() {
        let rsdp = rsdp_v2(0x1000, 0x2000);
        assert_eq!(
            parse_rsdp(&rsdp),
            Ok(Rsdp {
                rsdt_phys: 0x1000,
                xsdt_phys: Some(0x2000),
            })
        );

        let mut bad = rsdp.clone();
        bad[32] = bad[32].wrapping_add(1);
        assert_eq!(parse_rsdp(&bad), Err(AcpiError::BadChecksum));
    }

    #[test]
    fn rsdp_v0_has_no_xsdt() {
        let mut bytes = vec![0u8; RSDP_V0_LEN];
        bytes[0..8].copy_from_slice(b"RSD PTR ");
        bytes[15] = 0;
        bytes[16..20].copy_from_slice(&0x3000u32.to_le_bytes());
        seal(&mut bytes, 8);
        assert_eq!(
            parse_rsdp(&bytes),
            Ok(Rsdp {
                rsdt_phys: 0x3000,
                xsdt_phys: None,
            })
        );
    }

    #[test]
    fn madt_lists_enabled_local_apics_in_order() {
        let mut entries = Vec::new();
        entries.extend(local_apic(0, true));
        entries.extend(local_apic(1, false));
        entries.extend(local_apic(2, true));
        let bytes = table(b"APIC", &madt_body(0xfee0_0000, &entries));
        let topology = parse_madt(&bytes).expect("valid MADT");
        assert_eq!(topology.local_apic_phys(), 0xfee0_0000);
        assert_eq!(topology.count(), 2, "禁用的 CPU 不计入");
        assert_eq!(topology.apic_id(0), Some(0));
        assert_eq!(topology.apic_id(1), Some(2));
        assert_eq!(topology.index_of(2), Some(1));
        assert_eq!(topology.index_of(1), None);
        assert!(!topology.uses_x2apic());
    }

    #[test]
    fn madt_prefers_x2apic_entries() {
        let mut entries = Vec::new();
        entries.extend(local_apic(0, true));
        entries.extend(x2apic(16, true));
        entries.extend(x2apic(17, true));
        let bytes = table(b"APIC", &madt_body(0xfee0_0000, &entries));
        let topology = parse_madt(&bytes).expect("valid MADT");
        assert!(topology.uses_x2apic());
        assert_eq!(topology.count(), 2);
        assert_eq!(topology.apic_id(0), Some(16));
        assert_eq!(topology.index_of(17), Some(1));
    }

    #[test]
    fn madt_rejects_truncation_and_bad_checksum_and_no_cpu() {
        let entries = local_apic(0, true);
        let bytes = table(b"APIC", &madt_body(0xfee0_0000, &entries));

        assert_eq!(
            parse_madt(&bytes[..bytes.len() - 1]),
            Err(AcpiError::BadLength)
        );

        let mut bad = bytes.clone();
        bad[8] = bad[8].wrapping_add(1);
        assert_eq!(parse_madt(&bad), Err(AcpiError::BadChecksum));

        let empty = table(b"APIC", &madt_body(0xfee0_0000, &[]));
        assert_eq!(parse_madt(&empty), Err(AcpiError::NoCpu));

        let mut truncated_entry = table(b"APIC", &madt_body(0xfee0_0000, &local_apic(0, true)));
        // 把首个条目的长度改成越界值，并重算整表校验和。
        truncated_entry[MADT_ENTRY_START + 1] = 64;
        seal(&mut truncated_entry, 9);
        assert_eq!(parse_madt(&truncated_entry), Err(AcpiError::BadLength));
    }

    #[test]
    fn madt_rejects_more_cpus_than_slots() {
        let mut entries = Vec::new();
        for id in 0..=MAX_CPUS {
            entries.extend(local_apic(id as u8, true));
        }
        let bytes = table(b"APIC", &madt_body(0xfee0_0000, &entries));
        assert_eq!(parse_madt(&bytes), Err(AcpiError::TooManyCpus));
    }

    #[test]
    fn legacy_overflow_does_not_block_x2apic_selection() {
        let mut entries = Vec::new();
        for id in 0..=MAX_CPUS {
            entries.extend(local_apic(id as u8, true));
        }
        entries.extend(x2apic(16, true));
        let bytes = table(b"APIC", &madt_body(0xfee0_0000, &entries));
        let topology = parse_madt(&bytes).expect("x2APIC 条目优先，legacy 溢出不影响");
        assert!(topology.uses_x2apic());
        assert_eq!(topology.count(), 1);
        assert_eq!(topology.apic_id(0), Some(16));
    }

    #[test]
    fn rsdt_and_xsdt_entries_use_their_own_width() {
        let mut rsdt_body = vec![0u8; 12];
        rsdt_body[0..4].copy_from_slice(&0x1000u32.to_le_bytes());
        rsdt_body[4..8].copy_from_slice(&0x2000u32.to_le_bytes());
        let rsdt = table(b"RSDT", &rsdt_body);
        assert_eq!(table_phys(&rsdt, 1), Some(0x2000));
        assert_eq!(table_phys(&rsdt, 3), None, "越过声明长度返回 None");

        let mut xsdt_body = vec![0u8; 8];
        xsdt_body[0..8].copy_from_slice(&0x1_0000_0000u64.to_le_bytes());
        let xsdt = table(b"XSDT", &xsdt_body);
        assert_eq!(table_phys(&xsdt, 0), Some(0x1_0000_0000));
        assert_eq!(table_phys(&xsdt, 1), None);
        assert_eq!(table_phys(&table(b"NOPE", &[0u8; 8]), 0), None);
    }
}
