//! WIN-M2 前置：把 PE32+ 平铺为装载后的映像。
//!
//! 本模块只做纯内存变换：校验映射计划、复制头部与节、应用 DIR64/HIGHLOW 基址重定位、
//! 通过调用方提供的解析器填充 IAT。它不创建 Task、不映射页、不改变段权限，也不执行入口；
//! 调用方必须先建立可写缓冲区，随后按节计划设置权限并发布。

use core::cmp::min;

use crate::pe::{ExportTarget, ImportThunk, PeError, PeImage, RelocationEntry};

/// 平铺装载失败。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    Pe(PeError),
    OutputTooSmall {
        needed: usize,
        provided: usize,
    },
    /// 基址变化但没有任何可应用的重定位。
    BaseWithoutRelocations,
    /// 当前模型不应用该重定位类型。
    UnsupportedRelocation(u8),
    /// 重定位目标不在映像内。
    RelocationTargetOutOfImage,
    /// IAT 槽位不在映像内。
    ImportAddressOutOfImage,
    /// 解析器拒绝该导入。
    UnresolvedImport,
}

impl From<PeError> for LoadError {
    fn from(error: PeError) -> Self {
        Self::Pe(error)
    }
}

/// 平铺装载结果。地址均为装载基址下的事实地址。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadedImage {
    base: u64,
    entry_va: u64,
    image_size: u32,
    imports_resolved: u32,
    relocations_applied: u32,
}

impl LoadedImage {
    pub fn base(self) -> u64 {
        self.base
    }

    pub fn entry_va(self) -> u64 {
        self.entry_va
    }

    pub fn image_size(self) -> u32 {
        self.image_size
    }

    pub fn imports_resolved(self) -> u32 {
        self.imports_resolved
    }

    pub fn relocations_applied(self) -> u32 {
        self.relocations_applied
    }
}

impl<'a> PeImage<'a> {
    /// 在 `output[..SizeOfImage]` 中构造装载后的平铺映像。
    ///
    /// `output` 其余字节保持不变；映像范围先清零。`resolve` 返回替换导入的实际地址，
    /// 返回 `None` 即整体失败。成功前不保留部分写入的承诺：失败时输出可能已被修改，
    /// 调用方应丢弃缓冲区。
    pub fn load_into<R>(
        self,
        base: u64,
        output: &mut [u8],
        mut resolve: R,
    ) -> Result<LoadedImage, LoadError>
    where
        R: FnMut(&'a [u8], ImportThunk<'a>) -> Option<u64>,
    {
        self.validate_mapping()?;
        let image_size = self.size_of_image() as usize;
        if output.len() < image_size {
            return Err(LoadError::OutputTooSmall {
                needed: image_size,
                provided: output.len(),
            });
        }
        let out = &mut output[..image_size];
        out.fill(0);

        let bytes = self.raw_bytes();
        let header_len = min(self.size_of_headers() as usize, bytes.len());
        out[..header_len].copy_from_slice(&bytes[..header_len]);

        for section in self.sections() {
            let copy_len = min(section.raw_size(), section.mapped_size()) as usize;
            if copy_len == 0 {
                continue;
            }
            let source = section.raw_offset() as usize;
            let source_end = source
                .checked_add(copy_len)
                .filter(|&end| end <= bytes.len())
                .ok_or(PeError::FileRange)?;
            let target = section.virtual_address() as usize;
            let target_end = target
                .checked_add(copy_len)
                .filter(|&end| end <= out.len())
                .ok_or(PeError::ImageRange)?;
            out[target..target_end].copy_from_slice(&bytes[source..source_end]);
        }

        let delta = base.wrapping_sub(self.image_base());
        let mut relocations_applied = 0u32;
        for block in self.relocations()? {
            let block = block?;
            let page = block.page_rva();
            for entry in block.entries() {
                match entry {
                    RelocationEntry::Absolute => {}
                    RelocationEntry::Dir64 { offset } => {
                        let target = page
                            .checked_add(offset)
                            .ok_or(LoadError::RelocationTargetOutOfImage)?;
                        let value = read_u64(out, target)?;
                        write_u64(out, target, value.wrapping_add(delta))?;
                        relocations_applied += 1;
                    }
                    RelocationEntry::HighLow { offset } => {
                        let target = page
                            .checked_add(offset)
                            .ok_or(LoadError::RelocationTargetOutOfImage)?;
                        let value = read_u32(out, target)?;
                        write_u32(out, target, value.wrapping_add(delta as u32))?;
                        relocations_applied += 1;
                    }
                    RelocationEntry::Unsupported { kind, .. } => {
                        return Err(LoadError::UnsupportedRelocation(kind));
                    }
                }
            }
        }
        if delta != 0 && relocations_applied == 0 {
            return Err(LoadError::BaseWithoutRelocations);
        }

        let mut imports_resolved = 0u32;
        for descriptor in self.imports()? {
            let descriptor = descriptor?;
            let iat = descriptor.first_thunk();
            if iat == 0 {
                return Err(LoadError::ImportAddressOutOfImage);
            }
            for (slot, thunk) in descriptor.thunks()?.enumerate() {
                let thunk = thunk?;
                let resolved =
                    resolve(descriptor.name(), thunk).ok_or(LoadError::UnresolvedImport)?;
                let index = u32::try_from(slot)
                    .ok()
                    .and_then(|slot| slot.checked_mul(8))
                    .ok_or(LoadError::ImportAddressOutOfImage)?;
                let target = iat
                    .checked_add(index)
                    .ok_or(LoadError::ImportAddressOutOfImage)?;
                write_u64(out, target, resolved).map_err(|_| LoadError::ImportAddressOutOfImage)?;
                imports_resolved += 1;
            }
        }

        let entry_va = base
            .checked_add(self.entry_rva() as u64)
            .ok_or(PeError::RangeOverflow)?;
        Ok(LoadedImage {
            base,
            entry_va,
            image_size: self.size_of_image(),
            imports_resolved,
            relocations_applied,
        })
    }
}

/// 已装载模块：解析视图、装载基址与导入表给出的名称。
#[derive(Clone, Copy, Debug)]
pub struct Module<'a> {
    name: &'a [u8],
    image: PeImage<'a>,
    base: u64,
}

impl<'a> Module<'a> {
    pub const fn new(name: &'a [u8], image: PeImage<'a>, base: u64) -> Self {
        Self { name, image, base }
    }

    pub fn name(self) -> &'a [u8] {
        self.name
    }

    pub fn image(self) -> PeImage<'a> {
        self.image
    }

    pub fn base(self) -> u64 {
        self.base
    }
}

/// 转发链的最大深度；超过即视为无法解析，防止循环转发。
pub const MAX_FORWARD_DEPTH: u8 = 8;

/// 有界模块集合上的导入解析器。
///
/// 只按显式模块名匹配（忽略 ASCII 大小写与可选 `.dll` 后缀），不猜测 API-set 路由；
/// API-set 白名单应由调用方展开为模块集合。按名与按序号导入都经同一转发解析，
/// 每次转发消耗一层深度，循环在深度耗尽时返回 `None`。
pub struct ModuleSet<'a> {
    modules: &'a [Module<'a>],
}

impl<'a> ModuleSet<'a> {
    pub const fn new(modules: &'a [Module<'a>]) -> Self {
        Self { modules }
    }

    /// 解析一个导入；任何模块/导出缺失或转发链过深都返回 `None`。
    pub fn resolve(&self, dll: &[u8], thunk: ImportThunk<'_>) -> Option<u64> {
        self.resolve_from(dll, thunk, MAX_FORWARD_DEPTH)
    }

    fn resolve_from(&self, dll: &[u8], thunk: ImportThunk<'_>, depth: u8) -> Option<u64> {
        let module = self.find(dll)?;
        let export = match thunk {
            ImportThunk::Name { name, .. } => module.image.export_by_name(name).ok()??,
            ImportThunk::Ordinal(ordinal) => {
                module.image.export_by_ordinal(u32::from(ordinal)).ok()??
            }
        };
        match export.target() {
            ExportTarget::Function { rva } => module.base.checked_add(u64::from(rva)),
            ExportTarget::Forwarder {
                module: target_module,
                symbol,
            } => {
                if depth == 0 {
                    return None;
                }
                self.resolve_from(
                    target_module,
                    ImportThunk::Name {
                        hint: 0,
                        name: symbol,
                    },
                    depth - 1,
                )
            }
        }
    }

    fn find(&self, dll: &[u8]) -> Option<&Module<'a>> {
        self.modules
            .iter()
            .find(|module| module_names_equal(module.name, dll))
    }
}

fn module_names_equal(left: &[u8], right: &[u8]) -> bool {
    strip_dll_suffix(left).eq_ignore_ascii_case(strip_dll_suffix(right))
}

fn strip_dll_suffix(name: &[u8]) -> &[u8] {
    if name.len() > 4 && name[name.len() - 4..].eq_ignore_ascii_case(b".dll") {
        &name[..name.len() - 4]
    } else {
        name
    }
}

fn read_u32(out: &[u8], rva: u32) -> Result<u32, LoadError> {
    let end = rva
        .checked_add(4)
        .ok_or(LoadError::RelocationTargetOutOfImage)? as usize;
    let raw = out
        .get(rva as usize..end)
        .ok_or(LoadError::RelocationTargetOutOfImage)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u64(out: &[u8], rva: u32) -> Result<u64, LoadError> {
    let end = rva
        .checked_add(8)
        .ok_or(LoadError::RelocationTargetOutOfImage)? as usize;
    let raw = out
        .get(rva as usize..end)
        .ok_or(LoadError::RelocationTargetOutOfImage)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}

fn write_u32(out: &mut [u8], rva: u32, value: u32) -> Result<(), LoadError> {
    let end = rva
        .checked_add(4)
        .ok_or(LoadError::RelocationTargetOutOfImage)? as usize;
    let raw = out
        .get_mut(rva as usize..end)
        .ok_or(LoadError::RelocationTargetOutOfImage)?;
    raw.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

fn write_u64(out: &mut [u8], rva: u32, value: u64) -> Result<(), LoadError> {
    let end = rva
        .checked_add(8)
        .ok_or(LoadError::RelocationTargetOutOfImage)? as usize;
    let raw = out
        .get_mut(rva as usize..end)
        .ok_or(LoadError::RelocationTargetOutOfImage)?;
    raw.copy_from_slice(&value.to_le_bytes());
    Ok(())
}
