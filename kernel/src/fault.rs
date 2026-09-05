//! 纯 #PF 语义分类模块：无 asm、无真实现场依赖，唯一分类语义宿主，供 host 单测钉住。
//! 与 `idt.rs`（error-code 捕获）分离：本模块只负责把已捕获的 error code 译成互斥类别。

/// 一次 #PF 的语义分类（互斥）。由 x86_64 #PF error code 译码。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultClass {
    /// P=0：页不存在 → 真缺页（demand-paging 候选，非 COW 写保护）。
    NotPresent,
    /// P=1 且写意图：写保护 / 保护违规 → COW 处置候选。
    WriteProtection,
    /// P=1 且非写访问、或含 RSVD/PK/取指等其它置位；本 slice 不展开处置语义。
    Other,
}

impl FaultClass {
    /// wire 上的小写类别名。
    pub const fn as_str(self) -> &'static str {
        match self {
            FaultClass::NotPresent => "not_present",
            FaultClass::WriteProtection => "write_protection",
            FaultClass::Other => "other",
        }
    }
}

/// #PF error code 的关切位（Intel SDM §4.7 / 表 4-27）。
pub const PF_P: u64 = 1 << 0; // present; 0 => page not present; 1 => protection/permission
pub const PF_W: u64 = 1 << 1; // write access
pub const PF_US: u64 = 1 << 2; // user/supervisor (CPL3 source)
pub const PF_RSVD: u64 = 1 << 3; // reserved-bit violation in a paging-structure entry
pub const PF_ID: u64 = 1 << 4; // instruction fetch
pub const PF_PK: u64 = 1 << 5; // protection-key violation

/// 把 #PF error code（CPU 零扩展压栈 64 位）译成互斥分类。
///
/// 优先级：RSVD/PK/取指 等"页表/机器格式"异常 → `Other`；
/// 之后 `P=0` → `NotPresent`（即便 W/R=1，写 absent 页仍是缺页，不是写保护）；
/// 再 `P=1 && W/R=1` → `WriteProtection`；余下（present 读保护等）→ `Other`。
pub fn classify(err: u64) -> FaultClass {
    let present = err & PF_P != 0;
    let write = err & PF_W != 0;
    let format_err = err & (PF_RSVD | PF_PK | PF_ID) != 0;
    if format_err {
        FaultClass::Other
    } else if !present {
        FaultClass::NotPresent
    } else if write {
        FaultClass::WriteProtection
    } else {
        FaultClass::Other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_read_is_not_present() {
        // ring0 supervisor read of an absent page: P=0, W/R=0, U/S=0 -> 0x0
        assert_eq!(classify(0x0), FaultClass::NotPresent);
    }

    #[test]
    fn write_to_absent_page_is_not_present_not_write_protection() {
        // 反混淆关键：写一个 absent 页 -> P=0, W/R=1 -> 依然是缺页，不是写保护。
        assert_eq!(classify(PF_W), FaultClass::NotPresent);
    }

    #[test]
    fn write_protection_is_present_plus_write() {
        // a present(read-only) page written: P=1, W/R=1
        let err = PF_P | PF_W;
        assert_eq!(classify(err), FaultClass::WriteProtection);
    }

    #[test]
    fn present_read_protection_is_other() {
        // present but read fault (not write): P=1, W/R=0 -> not write-protection, Other.
        assert_eq!(classify(PF_P), FaultClass::Other);
    }

    #[test]
    fn rsvd_wins_over_present_write() {
        assert_eq!(classify(PF_P | PF_W | PF_RSVD), FaultClass::Other);
    }

    #[test]
    fn pk_wins_over_present_write() {
        assert_eq!(classify(PF_P | PF_W | PF_PK), FaultClass::Other);
    }

    #[test]
    fn user_write_protection_still_write_protection() {
        // U/S=1 does not change the P+W test.
        assert_eq!(classify(PF_P | PF_W | PF_US), FaultClass::WriteProtection);
    }

    #[test]
    fn instruction_fetch_format_error_is_other() {
        assert_eq!(classify(PF_P | PF_ID), FaultClass::Other);
    }

    #[test]
    fn as_str_names_wire_lowercase() {
        assert_eq!(FaultClass::NotPresent.as_str(), "not_present");
        assert_eq!(FaultClass::WriteProtection.as_str(), "write_protection");
        assert_eq!(FaultClass::Other.as_str(), "other");
    }
}
