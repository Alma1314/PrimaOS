//! 穿透映射内存的 POD 访问原语（唯一允许 `unsafe` 的内存触面之一）。
//!
//! 内核里大量访问发生在"已由页表映射、但类型系统看不到"的地址上：页表项、用户帧、
//! IPC 缓冲、物理直映射区、静态帧缓冲。本模块把这些裸访问收敛为一组**安全** `fn`。
//!
//! **契约（所有访问器共同）**：`va` 必须已映射且对声明的长度可读/可写；由调用方保证
//! （内核侧由页表与帧归属不变量保证）。误用可产生 UB——本层**信任调用方**，不宣称 sound，
//! 与 `arch` 的取向一致。

use core::ptr;

/// 读 `va` 处的 u64。
///
/// 契约：`va`（8 字节）已映射且可读。
#[inline]
pub fn read_u64(va: u64) -> u64 {
    // SAFETY: 见模块与函数契约——调用方保证 va 已映射可读。
    unsafe { ptr::read_volatile(va as *const u64) }
}

/// 写 `va` 处的 u64。
///
/// 契约：`va`（8 字节）已映射且可写。
#[inline]
pub fn write_u64(va: u64, value: u64) {
    // SAFETY: 见模块与函数契约——调用方保证 va 已映射可写。
    unsafe { ptr::write_volatile(va as *mut u64, value) }
}

/// 读 `va` 处的 u8。
///
/// 契约：`va`（1 字节）已映射且可读。
#[inline]
pub fn read_u8(va: u64) -> u8 {
    // SAFETY: 见模块与函数契约——调用方保证 va 已映射可读。
    unsafe { ptr::read_volatile(va as *const u8) }
}

/// 写 `va` 处的 u8。
///
/// 契约：`va`（1 字节）已映射且可写。
#[inline]
pub fn write_u8(va: u64, value: u8) {
    // SAFETY: 见模块与函数契约——调用方保证 va 已映射可写。
    unsafe { ptr::write_volatile(va as *mut u8, value) }
}

/// 读 `va + index*8` 处的 u64（页表项等按索引寻址）。
#[inline]
pub fn read_u64_at(va: u64, index: u64) -> u64 {
    read_u64(va + index * 8)
}

/// 写 `va + index*8` 处的 u64。
#[inline]
pub fn write_u64_at(va: u64, index: u64, value: u64) {
    write_u64(va + index * 8, value)
}

/// 读 `va + index` 处的 u8。
#[inline]
pub fn read_u8_at(va: u64, index: u64) -> u8 {
    read_u8(va + index)
}

/// 写 `va + index` 处的 u8。
#[inline]
pub fn write_u8_at(va: u64, index: u64, value: u8) {
    write_u8(va + index, value)
}

/// 从 `va` 起按 qword 写零 `len` 字节（`len` 应为 8 的倍数，调用方保证）。
///
/// 契约：`[va, va+len)` 已映射且可写。语义与逐 qword `write_u64` 相同。
#[inline]
pub fn fill_zero(va: u64, len: u64) {
    let mut offset = 0;
    while offset < len {
        write_u64(va + offset, 0);
        offset += 8;
    }
}

/// 取 `value` 的字节视图。
///
/// 契约：`T` 无未初始化填充（本项目用于 `repr(C)` 的 TSS 等 POD）。
#[inline]
pub fn bytes_of<T>(value: &T) -> &[u8] {
    // SAFETY: 见函数契约——`T` 视为 POD，整个 size_of::<T>() 区域已初始化、可读。
    unsafe { core::slice::from_raw_parts((value as *const T).cast::<u8>(), size_of::<T>()) }
}

/// 把 `va` 起 `len` 字节当作可变切片。
///
/// 契约：`[va, va+len)` 已映射且可写；同一区段在借用期内不得被他处可变访问（单核顺序访问）。
#[inline]
pub fn bytes_at_mut(va: u64, len: usize) -> &'static mut [u8] {
    // SAFETY: 见函数契约——调用方保证映射范围与独占性。
    unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) }
}

/// 把 `va` 起 `len` 字节当作只读切片。
///
/// 契约：`[va, va+len)` 已映射且可读。
#[inline]
pub fn bytes_at(va: u64, len: usize) -> &'static [u8] {
    // SAFETY: 见函数契约——调用方保证映射范围与只读性。
    unsafe { core::slice::from_raw_parts(va as *const u8, len) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_zero_writes_qwords() {
        let mut buffer = [0xaau64; 4];
        let va = buffer.as_mut_ptr() as u64;
        fill_zero(va, 16);
        assert_eq!(buffer, [0, 0, 0xaa, 0xaa]);
    }

    #[test]
    fn read_write_u64_roundtrip() {
        let mut slot = 0u64;
        let va = (&mut slot) as *mut u64 as u64;
        write_u64(va, 0x1234_5678);
        assert_eq!(read_u64(va), 0x1234_5678);
    }

    #[test]
    fn indexed_access_uses_eight_byte_stride() {
        let mut table = [0u64; 3];
        let va = table.as_mut_ptr() as u64;
        write_u64_at(va, 2, 0x42);
        assert_eq!(read_u64_at(va, 2), 0x42);
        assert_eq!(table, [0, 0, 0x42]);
    }

    #[test]
    fn bytes_of_exposes_object_bytes() {
        let bytes = bytes_of(&0x1122_3344_5566_7788u64);
        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn u8_accessors_roundtrip() {
        let mut bytes = [0u8; 2];
        let va = bytes.as_mut_ptr() as u64;
        write_u8_at(va, 1, 0x5a);
        assert_eq!(read_u8_at(va, 1), 0x5a);
        assert_eq!(bytes, [0, 0x5a]);
    }
}
