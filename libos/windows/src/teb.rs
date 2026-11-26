//! WIN-M3 宿主模型：TEB/PEB 最小子集与静态 TLS 布局。
//!
//! 只构造字节布局与模板复制，不写硬件、不建立用户 GS，也不执行任何 Windows 代码。
//! 冻结的偏移是 x64 Windows 的公开子集（见各常量注释），TEB/PEB 的完整布局随 Windows
//! 版本变化，本模型只承诺这里列出的字段。ring3 启用静态 TLS 需要 WIN-D3（用户 GS 与
//! 停车/发布）评审，当前证据范围仅是宿主模型。

use crate::pe::PeImage;

/// TEB 冻结子集的最小长度（覆盖到 `TEB_LAST_ERROR` 之后）。
pub const TEB_SIZE: usize = 0x70;
/// `NT_TIB.StackBase`。
pub const TEB_STACK_BASE: usize = 0x08;
/// `NT_TIB.StackLimit`。
pub const TEB_STACK_LIMIT: usize = 0x10;
/// `NT_TIB.Self`：TEB 自身地址。
pub const TEB_SELF: usize = 0x30;
/// `ClientId.UniqueProcess`。
pub const TEB_PROCESS_ID: usize = 0x40;
/// `ClientId.UniqueThread`。
pub const TEB_THREAD_ID: usize = 0x48;
/// `ThreadLocalStoragePointer`：按 `_tls_index` 索引的模块 TLS 指针数组。
pub const TEB_TLS_POINTER: usize = 0x58;
/// `ProcessEnvironmentBlock`。
pub const TEB_PEB: usize = 0x60;
/// `LastErrorValue`（u32）。
pub const TEB_LAST_ERROR: usize = 0x68;

/// PEB 冻结子集的最小长度。
pub const PEB_SIZE: usize = 0x38;
/// `ImageBaseAddress`。
pub const PEB_IMAGE_BASE: usize = 0x10;
/// `ProcessParameters`。
pub const PEB_PROCESS_PARAMETERS: usize = 0x20;
/// `ProcessHeap`。
pub const PEB_PROCESS_HEAP: usize = 0x30;

/// TLS 模板在目录未给出对齐时使用的默认对齐。
pub const TLS_DEFAULT_ALIGNMENT: usize = 8;

/// TEB/PEB/TLS 布局构造失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// 输出缓冲小于冻结子集所需长度。
    TooSmall { needed: usize, provided: usize },
    /// TLS 目录的原始模板范围越界或长度不可表示。
    TlsOutOfBounds,
    /// TLS 模板对齐不是二的幂。
    TlsAlignment(u32),
}

/// TEB 冻结子集的写入参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TebSpec {
    pub self_va: u64,
    pub stack_base: u64,
    pub stack_limit: u64,
    pub process_id: u64,
    pub thread_id: u64,
    /// 模块 TLS 指针数组地址；0 表示尚未建立。
    pub tls_pointer_va: u64,
    pub peb_va: u64,
    pub last_error: u32,
}

/// PEB 冻结子集的写入参数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PebSpec {
    pub image_base: u64,
    pub process_parameters_va: u64,
    pub process_heap_va: u64,
}

/// 静态 TLS 模板：原始映像字节加装载期清零字节。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsTemplate<'a> {
    bytes: &'a [u8],
    zero_fill: usize,
    alignment: usize,
}

impl<'a> TlsTemplate<'a> {
    /// 单个线程的 TLS 块大小（模板字节加清零区）。
    pub fn size(self) -> usize {
        self.bytes.len() + self.zero_fill
    }

    pub fn alignment(self) -> usize {
        self.alignment
    }

    /// 把模板复制到 `dest` 并清零剩余部分；`dest` 必须至少 `size()` 字节。
    pub fn write_into(self, dest: &mut [u8]) -> Result<(), LayoutError> {
        if dest.len() < self.size() {
            return Err(LayoutError::TooSmall {
                needed: self.size(),
                provided: dest.len(),
            });
        }
        dest[..self.bytes.len()].copy_from_slice(self.bytes);
        dest[self.bytes.len()..self.size()].fill(0);
        Ok(())
    }
}

/// 从 PE 的 TLS 目录提取静态模板；没有 TLS 目录返回 `Ok(None)`。
///
/// 模板范围按 VA 给出（与 PE TLS 目录一致），转换为 RVA 后必须完整落在文件数据内；
/// 目录 `Characteristics` 为 0 时退回 [`TLS_DEFAULT_ALIGNMENT`]。
pub fn tls_template<'a>(image: PeImage<'a>) -> Result<Option<TlsTemplate<'a>>, LayoutError> {
    let Some(tls) = image.tls().map_err(|_| LayoutError::TlsOutOfBounds)? else {
        return Ok(None);
    };
    let size = tls
        .raw_end()
        .checked_sub(tls.raw_start())
        .ok_or(LayoutError::TlsOutOfBounds)?;
    let size = u32::try_from(size).map_err(|_| LayoutError::TlsOutOfBounds)?;
    let rva = tls
        .raw_start()
        .checked_sub(image.image_base())
        .ok_or(LayoutError::TlsOutOfBounds)?;
    let rva = u32::try_from(rva).map_err(|_| LayoutError::TlsOutOfBounds)?;
    let bytes = image
        .slice_rva(rva, size)
        .map_err(|_| LayoutError::TlsOutOfBounds)?;
    let alignment = match tls.characteristics() {
        0 => TLS_DEFAULT_ALIGNMENT,
        value if (value as usize).is_power_of_two() => value as usize,
        value => return Err(LayoutError::TlsAlignment(value)),
    };
    Ok(Some(TlsTemplate {
        bytes,
        zero_fill: tls.zero_fill() as usize,
        alignment,
    }))
}

/// 写入 TEB 冻结子集；不触碰未列出的字段。
pub fn write_teb(buffer: &mut [u8], spec: &TebSpec) -> Result<(), LayoutError> {
    if buffer.len() < TEB_SIZE {
        return Err(LayoutError::TooSmall {
            needed: TEB_SIZE,
            provided: buffer.len(),
        });
    }
    buffer[..TEB_SIZE].fill(0);
    put_u64(buffer, TEB_STACK_BASE, spec.stack_base);
    put_u64(buffer, TEB_STACK_LIMIT, spec.stack_limit);
    put_u64(buffer, TEB_SELF, spec.self_va);
    put_u64(buffer, TEB_PROCESS_ID, spec.process_id);
    put_u64(buffer, TEB_THREAD_ID, spec.thread_id);
    put_u64(buffer, TEB_TLS_POINTER, spec.tls_pointer_va);
    put_u64(buffer, TEB_PEB, spec.peb_va);
    put_u32(buffer, TEB_LAST_ERROR, spec.last_error);
    Ok(())
}

/// 写入 PEB 冻结子集；不触碰未列出的字段。
pub fn write_peb(buffer: &mut [u8], spec: &PebSpec) -> Result<(), LayoutError> {
    if buffer.len() < PEB_SIZE {
        return Err(LayoutError::TooSmall {
            needed: PEB_SIZE,
            provided: buffer.len(),
        });
    }
    buffer[..PEB_SIZE].fill(0);
    put_u64(buffer, PEB_IMAGE_BASE, spec.image_base);
    put_u64(buffer, PEB_PROCESS_PARAMETERS, spec.process_parameters_va);
    put_u64(buffer, PEB_PROCESS_HEAP, spec.process_heap_va);
    Ok(())
}

fn put_u64(buffer: &mut [u8], offset: usize, value: u64) {
    buffer[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(buffer: &mut [u8], offset: usize, value: u32) {
    buffer[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_frozen_teb_subset() {
        let mut teb = [0u8; TEB_SIZE];
        write_teb(
            &mut teb,
            &TebSpec {
                self_va: 0x7000,
                stack_base: 0x9000,
                stack_limit: 0x8000,
                process_id: 7,
                thread_id: 9,
                tls_pointer_va: 0xa000,
                peb_va: 0xb000,
                last_error: 0xdead,
            },
        )
        .expect("teb fits");
        assert_eq!(
            u64::from_le_bytes(teb[0x08..0x10].try_into().unwrap()),
            0x9000
        );
        assert_eq!(
            u64::from_le_bytes(teb[0x30..0x38].try_into().unwrap()),
            0x7000
        );
        assert_eq!(u64::from_le_bytes(teb[0x40..0x48].try_into().unwrap()), 7);
        assert_eq!(
            u64::from_le_bytes(teb[0x58..0x60].try_into().unwrap()),
            0xa000
        );
        assert_eq!(
            u64::from_le_bytes(teb[0x60..0x68].try_into().unwrap()),
            0xb000
        );
        assert_eq!(
            u32::from_le_bytes(teb[0x68..0x6c].try_into().unwrap()),
            0xdead
        );
    }

    #[test]
    fn rejects_short_teb_and_peb_buffers_without_writing() {
        let mut short = [0xffu8; TEB_SIZE - 1];
        assert!(matches!(
            write_teb(
                &mut short,
                &TebSpec {
                    self_va: 1,
                    stack_base: 0,
                    stack_limit: 0,
                    process_id: 0,
                    thread_id: 0,
                    tls_pointer_va: 0,
                    peb_va: 0,
                    last_error: 0
                }
            ),
            Err(LayoutError::TooSmall {
                needed: TEB_SIZE,
                ..
            })
        ));
        assert!(short.iter().all(|&byte| byte == 0xff));

        let mut short = [0xffu8; PEB_SIZE - 1];
        assert!(matches!(
            write_peb(
                &mut short,
                &PebSpec {
                    image_base: 1,
                    process_parameters_va: 0,
                    process_heap_va: 0
                }
            ),
            Err(LayoutError::TooSmall {
                needed: PEB_SIZE,
                ..
            })
        ));
        assert!(short.iter().all(|&byte| byte == 0xff));
    }

    #[test]
    fn writes_the_frozen_peb_subset() {
        let mut peb = [0xffu8; PEB_SIZE];
        write_peb(
            &mut peb,
            &PebSpec {
                image_base: 0x1_4000_0000,
                process_parameters_va: 0xc000,
                process_heap_va: 0xd000,
            },
        )
        .expect("peb fits");
        assert_eq!(
            u64::from_le_bytes(peb[0x10..0x18].try_into().unwrap()),
            0x1_4000_0000
        );
        assert_eq!(
            u64::from_le_bytes(peb[0x20..0x28].try_into().unwrap()),
            0xc000
        );
        assert_eq!(
            u64::from_le_bytes(peb[0x30..0x38].try_into().unwrap()),
            0xd000
        );
    }

    #[test]
    fn tls_template_copies_bytes_and_zero_fills_the_rest() {
        let template = TlsTemplate {
            bytes: b"abcd",
            zero_fill: 4,
            alignment: 8,
        };
        assert_eq!(template.size(), 8);
        let mut block = [0xeeu8; 8];
        template.write_into(&mut block).expect("block fits");
        assert_eq!(&block, b"abcd\0\0\0\0");
        assert!(matches!(
            template.write_into(&mut [0u8; 7]),
            Err(LayoutError::TooSmall { .. })
        ));
    }
}
