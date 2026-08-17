//! 细化（refine）不安全原语：本模块承载需要 `unsafe` 的底层访问。
//!
//! 项目纪律：`unsafe` 只允许出现在细化层模块，`mem` 等纯逻辑模块不落 unsafe。
//! 本模块的 `read_phys_byte` 是唯一真正触碰内存的原语，调用方（`mem::probe_physical_memory`）
//! 负责保证 `va` 处于 bootloader 已映射的物理内存直映射区。

/// 经物理内存直映射读取一个字节。`va` 必须指向 bootloader 可读的映射。
///
/// # Safety
///
/// `va` 必须处于 `physical_memory_offset` 直映射区内，且对应物理页可安全读取；
/// 违反此条件属于 UB（可能触发页错误或读到未安全映射的内存）。
pub unsafe fn read_phys_byte(va: u64) -> u8 {
    // `deny(unsafe_op_in_unsafe_fn)`：edition 2024 要求在 unsafe fn 体内为每个 unsafe 操作显式标注块。
    unsafe { core::ptr::read_volatile(va as *const u8) }
}
