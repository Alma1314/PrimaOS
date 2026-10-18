//! 切片 J：x86-64 `syscall`/`sysret` 快路径所需的 MSR 细化模块。
//!
//! 承载 `rdmsr`/`wrmsr` 的 `asm!`；STAR 的段选择子编码与 EFER.SCE 位操作抽为纯函数，
//! 可 host 单测。`no_std` 由 crate 根 (`lib.rs`) 统一设置。
//!
//! **范围边界：** 单核、非抢占；不做 `swapgs`/per-CPU 区、不做 PCID/STAR 之外的
//! 兼容性配置；FMASK 仅屏蔽 IF/TF/DF。

/// IA32_EFER：扩展功能使能寄存器。
pub const IA32_EFER: u32 = 0xC000_0080;
/// IA32_STAR：`syscall`/`sysret` 的段选择子。
pub const IA32_STAR: u32 = 0xC000_0081;
/// IA32_LSTAR：`syscall` 的 64 位入口地址。
pub const IA32_LSTAR: u32 = 0xC000_0082;
/// IA32_FMASK：`syscall` 进入时在 RFLAGS 中屏蔽的位。
pub const IA32_FMASK: u32 = 0xC000_0084;
/// IA32_FS_BASE：`%fs` 段基址（M2/T-MUSL-02）。
pub const IA32_FS_BASE: u32 = 0xC000_0100;

/// EFER.SCE（System Call Extensions，bit 0）：置位后 `syscall`/`sysret` 可用。
pub const EFER_SCE: u64 = 1;

/// `syscall` 进入时屏蔽 IF/TF/DF（bit 9/8/10）。
pub const SYSCALL_FMASK: u64 = 0x700;

/// 读 MSR（特权指令，仅 CPL0）。
///
/// 契约随 [`prima_ostd::arch::rdmsr`]：只能在 CPL0 调用；host 用户态单测不得调用。
pub fn rdmsr(msr: u32) -> u64 {
    prima_ostd::arch::rdmsr(msr)
}

/// 写 MSR（特权指令，仅 CPL0）。
///
/// 契约随 [`prima_ostd::arch::wrmsr`]：只能在 CPL0 调用；写入错误值会使后续
/// `syscall`/`sysret` 行为未定义。
pub fn wrmsr(msr: u32, value: u64) {
    prima_ostd::arch::wrmsr(msr, value);
}

/// 在 EFER 值上置位 SCE（纯函数，供 host 单测）。
pub const fn efer_with_sce(efer: u64) -> u64 {
    efer | EFER_SCE
}

/// 64 位 STAR 编码：`[47:32]` 为内核 `CS`（`SS = CS + 8`），`[63:48]` 为用户基址
/// （`sysret` 取 `CS = base + 16 | 3`、`SS = base + 8 | 3`）。
pub const fn star_value(kernel_cs: u16, user_base: u16) -> u64 {
    ((user_base as u64) << 48) | ((kernel_cs as u64) << 32)
}

/// 本内核的 STAR 值：内核 `CS=0x8`（`SS=0x10`），用户基址 `0x20`
/// （`sysret` → `CS=0x33`、`SS=0x2b`，与 [`crate::gdt::USER_CS`]/[`crate::gdt::USER_DS`] 一致）。
pub const STAR_VALUE: u64 = star_value(0x8, 0x20);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdt::{USER_CS, USER_DS};

    #[test]
    fn star_encodes_kernel_and_user_bases() {
        assert_eq!(STAR_VALUE, 0x0020_0008_0000_0000);
        assert_eq!((STAR_VALUE >> 32) as u16, 0x8);
        assert_eq!((STAR_VALUE >> 48) as u16, 0x20);
    }

    #[test]
    fn star_user_base_yields_expected_sysret_selectors() {
        let base = (STAR_VALUE >> 48) as u16;
        let sysret_cs = (base + 16) | 3;
        let sysret_ss = (base + 8) | 3;
        assert_eq!(sysret_cs, USER_CS);
        assert_eq!(sysret_ss, USER_DS);
    }

    #[test]
    fn efer_sce_sets_bit_zero_without_clearing_others() {
        assert_eq!(efer_with_sce(0), 1);
        assert_eq!(efer_with_sce(0x500), 0x501);
    }

    #[test]
    fn fmask_covers_if_tf_df() {
        assert_eq!(SYSCALL_FMASK & (1 << 9), 1 << 9); // IF
        assert_eq!(SYSCALL_FMASK & (1 << 8), 1 << 8); // TF
        assert_eq!(SYSCALL_FMASK & (1 << 10), 1 << 10); // DF
    }
}
