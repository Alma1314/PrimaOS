//! PE32+（AMD64）有界解析与映射计划。
//!
//! 只解析，不装载、不重定位、不执行。所有读取都经过 RVA 到文件偏移的映射和长度检查；
//! 目录项、段、导入、重定位与 TLS 超出文件或映像范围时返回错误而不是零填充或截断。

use core::cmp::min;

const DOS_HEADER_LEN: usize = 64;
const COFF_HEADER_LEN: usize = 20;
const OPTIONAL_HEADER_MIN: usize = 0x70;
const SECTION_HEADER_LEN: usize = 40;
const DATA_DIRECTORY_LEN: usize = 8;
const MAX_DATA_DIRECTORIES: usize = 16;
const MAX_SECTIONS: usize = 96;
const TLS_DIRECTORY_LEN: usize = 40;

const PE32_PLUS_MAGIC: u16 = 0x20b;
const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
const IMAGE_SCN_MEM_EXECUTE: u32 = 0x2000_0000;
const IMAGE_SCN_MEM_READ: u32 = 0x4000_0000;
const IMAGE_SCN_MEM_WRITE: u32 = 0x8000_0000;
const IMPORT_ORDINAL_FLAG: u64 = 0x8000_0000_0000_0000;

/// PE 解析错误。变体区分“结构不可读”和“结构可读但映射不一致”。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeError {
    /// DOS 头或 PE 头不足以读取。
    Truncated,
    /// 缺少 `MZ`。
    DosMagic,
    /// 缺少 `PE\0\0`。
    PeSignature,
    /// 不是 AMD64。
    Machine(u16),
    /// 不是 PE32+。
    OptionalMagic(u16),
    /// 可选头不足以容纳声明的数据目录。
    OptionalHeaderTooSmall,
    /// 节表越过文件末尾。
    SectionTableOutOfBounds,
    /// 节数量超过 PE 规范上限。
    TooManySections,
    /// 文件/映像对齐不是期望的 2 的幂或互相冲突。
    Alignment,
    /// RVA 不落在任何已映射段或头部。
    RvaNotMapped(u32),
    /// RVA 在段的虚拟范围但超出文件后备数据。
    RvaNotInFile(u32),
    /// 地址或长度加法溢出。
    RangeOverflow,
    /// 文件范围越界。
    FileRange,
    /// 映像范围越界。
    ImageRange,
    /// 节虚拟范围重叠或侵入头部。
    SectionOverlap,
    /// 同时可写可执行，拒绝发布。
    WritableExecutable,
    /// 导入目录或名称不可读。
    ImportOutOfBounds,
    /// 重定位块结构不可读。
    RelocationOutOfBounds,
    /// TLS 目录、回调数组或回调地址越界。
    TlsOutOfBounds,
}

/// 需要解析的 PE 数据目录。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataDirectoryIndex {
    Export = 0,
    Import = 1,
    Exception = 3,
    BaseReloc = 5,
    Tls = 9,
}

impl DataDirectoryIndex {
    fn index(self) -> usize {
        self as usize
    }
}

/// 数据目录表项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataDirectory {
    virtual_address: u32,
    size: u32,
}

impl DataDirectory {
    pub fn virtual_address(self) -> u32 {
        self.virtual_address
    }

    pub fn size(self) -> u32 {
        self.size
    }
}

/// 节的内存保护，由节特征推导。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    NoAccess,
    ReadOnly,
    ReadWrite,
    ReadExecute,
    ReadWriteExecute,
}

/// 已解析的节头。
#[derive(Clone, Copy, Debug)]
pub struct Section<'a> {
    name_bytes: &'a [u8],
    virtual_size: u32,
    virtual_address: u32,
    raw_size: u32,
    raw_offset: u32,
    characteristics: u32,
}

impl<'a> Section<'a> {
    pub fn name(self) -> &'a [u8] {
        let len = self
            .name_bytes
            .iter()
            .position(|&byte| byte == 0)
            .unwrap_or(self.name_bytes.len());
        &self.name_bytes[..len]
    }

    pub fn virtual_size(self) -> u32 {
        self.virtual_size
    }

    pub fn virtual_address(self) -> u32 {
        self.virtual_address
    }

    pub fn raw_size(self) -> u32 {
        self.raw_size
    }

    pub fn raw_offset(self) -> u32 {
        self.raw_offset
    }

    pub fn characteristics(self) -> u32 {
        self.characteristics
    }

    /// Windows 装载器使用的虚拟占用：`VirtualSize` 为零时退回 `SizeOfRawData`。
    pub fn mapped_size(self) -> u32 {
        if self.virtual_size != 0 {
            self.virtual_size
        } else {
            self.raw_size
        }
    }

    pub fn protection(self) -> Protection {
        let execute = self.characteristics & IMAGE_SCN_MEM_EXECUTE != 0;
        let read = self.characteristics & IMAGE_SCN_MEM_READ != 0;
        let write = self.characteristics & IMAGE_SCN_MEM_WRITE != 0;
        match (read, write, execute) {
            (_, true, true) => Protection::ReadWriteExecute,
            (true, true, false) => Protection::ReadWrite,
            (true, false, true) => Protection::ReadExecute,
            (true, false, false) => Protection::ReadOnly,
            (false, false, false) => Protection::NoAccess,
            (false, false, true) => Protection::ReadExecute,
            (false, true, false) => Protection::ReadWrite,
        }
    }
}

/// 解析后的 PE32+ 映像视图。
#[derive(Clone, Copy, Debug)]
pub struct PeImage<'a> {
    bytes: &'a [u8],
    machine: u16,
    image_base: u64,
    entry_rva: u32,
    size_of_image: u32,
    size_of_headers: u32,
    section_alignment: u32,
    file_alignment: u32,
    section_count: u16,
    section_table_offset: usize,
    data_directory_offset: usize,
    data_directory_count: usize,
}

impl<'a> PeImage<'a> {
    /// 解析 PE32+ 头、节表和所需数据目录的位置；不验证段映射一致性。
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PeError> {
        if bytes.len() < DOS_HEADER_LEN {
            return Err(PeError::Truncated);
        }
        if &bytes[0..2] != b"MZ" {
            return Err(PeError::DosMagic);
        }

        let pe_offset = read_u32(bytes, 0x3c)? as usize;
        let signature_end = pe_offset.checked_add(4).ok_or(PeError::RangeOverflow)?;
        if signature_end > bytes.len() {
            return Err(PeError::Truncated);
        }
        if &bytes[pe_offset..signature_end] != b"PE\0\0" {
            return Err(PeError::PeSignature);
        }

        let coff = signature_end;
        let coff_end = coff
            .checked_add(COFF_HEADER_LEN)
            .ok_or(PeError::RangeOverflow)?;
        if coff_end > bytes.len() {
            return Err(PeError::Truncated);
        }
        let machine = read_u16(bytes, coff)?;
        if machine != IMAGE_FILE_MACHINE_AMD64 {
            return Err(PeError::Machine(machine));
        }
        let section_count = read_u16(bytes, coff + 2)?;
        if section_count as usize > MAX_SECTIONS {
            return Err(PeError::TooManySections);
        }
        let optional_size = read_u16(bytes, coff + 16)? as usize;
        if optional_size < OPTIONAL_HEADER_MIN {
            return Err(PeError::OptionalHeaderTooSmall);
        }

        let optional = coff_end;
        let optional_end = optional
            .checked_add(optional_size)
            .ok_or(PeError::RangeOverflow)?;
        if optional_end > bytes.len() {
            return Err(PeError::Truncated);
        }
        let magic = read_u16(bytes, optional)?;
        if magic != PE32_PLUS_MAGIC {
            return Err(PeError::OptionalMagic(magic));
        }

        let declared_directories = read_u32(bytes, optional + 0x6c)? as usize;
        let data_directory_count = min(declared_directories, MAX_DATA_DIRECTORIES);
        let directory_bytes = data_directory_count
            .checked_mul(DATA_DIRECTORY_LEN)
            .ok_or(PeError::RangeOverflow)?;
        let directory_end = OPTIONAL_HEADER_MIN
            .checked_add(directory_bytes)
            .ok_or(PeError::RangeOverflow)?;
        if directory_end > optional_size {
            return Err(PeError::OptionalHeaderTooSmall);
        }

        let section_table_offset = optional_end;
        let table_bytes = (section_count as usize)
            .checked_mul(SECTION_HEADER_LEN)
            .ok_or(PeError::RangeOverflow)?;
        let table_end = section_table_offset
            .checked_add(table_bytes)
            .ok_or(PeError::RangeOverflow)?;
        if table_end > bytes.len() {
            return Err(PeError::SectionTableOutOfBounds);
        }

        Ok(Self {
            bytes,
            machine,
            image_base: read_u64(bytes, optional + 0x18)?,
            entry_rva: read_u32(bytes, optional + 0x10)?,
            size_of_image: read_u32(bytes, optional + 0x38)?,
            size_of_headers: read_u32(bytes, optional + 0x3c)?,
            section_alignment: read_u32(bytes, optional + 0x20)?,
            file_alignment: read_u32(bytes, optional + 0x24)?,
            section_count,
            section_table_offset,
            data_directory_offset: optional + OPTIONAL_HEADER_MIN,
            data_directory_count,
        })
    }

    pub fn machine(self) -> u16 {
        self.machine
    }

    pub fn image_base(self) -> u64 {
        self.image_base
    }

    pub fn entry_rva(self) -> u32 {
        self.entry_rva
    }

    pub fn size_of_image(self) -> u32 {
        self.size_of_image
    }

    pub fn size_of_headers(self) -> u32 {
        self.size_of_headers
    }

    pub fn section_alignment(self) -> u32 {
        self.section_alignment
    }

    pub fn file_alignment(self) -> u32 {
        self.file_alignment
    }

    pub fn sections(self) -> Sections<'a> {
        Sections {
            image: self,
            next_index: 0,
        }
    }

    /// 读取数据目录；不存在的目录或全零项返回 `None`。
    pub fn data_directory(self, index: DataDirectoryIndex) -> Option<DataDirectory> {
        let slot = index.index();
        if slot >= self.data_directory_count {
            return None;
        }
        let offset = self.data_directory_offset + slot * DATA_DIRECTORY_LEN;
        let virtual_address = read_u32(self.bytes, offset).ok()?;
        let size = read_u32(self.bytes, offset + 4).ok()?;
        if virtual_address == 0 && size == 0 {
            None
        } else {
            Some(DataDirectory {
                virtual_address,
                size,
            })
        }
    }

    /// 把 RVA 范围映射为文件偏移；范围必须完整落在头部或某个节的原始数据内。
    pub fn rva_to_offset(self, rva: u32, len: u32) -> Result<u32, PeError> {
        rva.checked_add(len).ok_or(PeError::RangeOverflow)?;
        let (offset, available) = self.window(rva)?;
        if len as usize > available {
            return Err(PeError::RvaNotInFile(rva));
        }
        u32::try_from(offset).map_err(|_| PeError::FileRange)
    }

    /// 读取 RVA 范围对应的文件字节。
    pub fn slice_rva(self, rva: u32, len: u32) -> Result<&'a [u8], PeError> {
        let offset = self.rva_to_offset(rva, len)? as usize;
        let end = offset
            .checked_add(len as usize)
            .ok_or(PeError::RangeOverflow)?;
        self.bytes.get(offset..end).ok_or(PeError::FileRange)
    }

    /// 校验节映射计划：对齐、范围、重叠和 W^X。
    pub fn validate_mapping(self) -> Result<(), PeError> {
        if !self.file_alignment.is_power_of_two()
            || !self.section_alignment.is_power_of_two()
            || self.file_alignment < 0x200
            || self.file_alignment > 0x10000
            || self.section_alignment < self.file_alignment
        {
            return Err(PeError::Alignment);
        }
        if self.size_of_headers > self.size_of_image {
            return Err(PeError::ImageRange);
        }
        if self.size_of_headers as usize > self.bytes.len() {
            return Err(PeError::FileRange);
        }
        let table_end = self
            .section_table_offset
            .checked_add(
                (self.section_count as usize)
                    .checked_mul(SECTION_HEADER_LEN)
                    .ok_or(PeError::RangeOverflow)?,
            )
            .ok_or(PeError::RangeOverflow)?;
        if table_end > self.size_of_headers as usize {
            return Err(PeError::ImageRange);
        }
        if self.entry_rva >= self.size_of_image {
            return Err(PeError::ImageRange);
        }

        let mut next_free = align_up(self.size_of_headers, self.section_alignment)?;
        for section in self.sections() {
            if section.virtual_address % self.section_alignment != 0 {
                return Err(PeError::Alignment);
            }
            if section.raw_size != 0 {
                if section.raw_offset % self.file_alignment != 0 {
                    return Err(PeError::Alignment);
                }
                let raw_end = section
                    .raw_offset
                    .checked_add(section.raw_size)
                    .ok_or(PeError::RangeOverflow)?;
                if raw_end as usize > self.bytes.len() {
                    return Err(PeError::FileRange);
                }
            }

            let mapped = section.mapped_size();
            if mapped != 0 {
                let start = section.virtual_address;
                if start < next_free {
                    return Err(PeError::SectionOverlap);
                }
                let end = start.checked_add(mapped).ok_or(PeError::RangeOverflow)?;
                let end = align_up(end, self.section_alignment)?;
                if end > self.size_of_image {
                    return Err(PeError::ImageRange);
                }
                next_free = end;
            }

            if section.protection() == Protection::ReadWriteExecute {
                return Err(PeError::WritableExecutable);
            }
        }
        Ok(())
    }

    /// 迭代导入描述符；目录缺失时返回空迭代器。
    pub fn imports(self) -> Result<Imports<'a>, PeError> {
        match self.data_directory(DataDirectoryIndex::Import) {
            Some(directory) => Ok(Imports {
                image: self,
                rva: directory.virtual_address,
                remaining: directory.size,
                done: false,
            }),
            None => Ok(Imports {
                image: self,
                rva: 0,
                remaining: 0,
                done: true,
            }),
        }
    }

    /// 迭代基址重定位块；目录缺失时返回空迭代器。
    pub fn relocations(self) -> Result<RelocationBlocks<'a>, PeError> {
        match self.data_directory(DataDirectoryIndex::BaseReloc) {
            Some(directory) => Ok(RelocationBlocks {
                image: self,
                rva: directory.virtual_address,
                remaining: directory.size,
                done: false,
            }),
            None => Ok(RelocationBlocks {
                image: self,
                rva: 0,
                remaining: 0,
                done: true,
            }),
        }
    }

    /// 解析 TLS 目录；不存在时返回 `None`。
    pub fn tls(self) -> Result<Option<TlsDirectory>, PeError> {
        let Some(directory) = self.data_directory(DataDirectoryIndex::Tls) else {
            return Ok(None);
        };
        if directory.size < TLS_DIRECTORY_LEN as u32 {
            return Err(PeError::TlsOutOfBounds);
        }
        let raw = self
            .slice_rva(directory.virtual_address, TLS_DIRECTORY_LEN as u32)
            .map_err(|_| PeError::TlsOutOfBounds)?;
        Ok(Some(TlsDirectory {
            raw_start: read_u64(raw, 0).map_err(|_| PeError::TlsOutOfBounds)?,
            raw_end: read_u64(raw, 8).map_err(|_| PeError::TlsOutOfBounds)?,
            index_address: read_u64(raw, 16).map_err(|_| PeError::TlsOutOfBounds)?,
            callbacks_address: read_u64(raw, 24).map_err(|_| PeError::TlsOutOfBounds)?,
            zero_fill: read_u32(raw, 32).map_err(|_| PeError::TlsOutOfBounds)?,
            characteristics: read_u32(raw, 36).map_err(|_| PeError::TlsOutOfBounds)?,
        }))
    }

    /// 迭代 TLS 回调 VA；不存在 TLS 目录或回调地址为零时返回空迭代器。
    pub fn tls_callbacks(self) -> Result<TlsCallbacks<'a>, PeError> {
        let Some(tls) = self.tls()? else {
            return Ok(TlsCallbacks {
                image: self,
                rva: 0,
                done: true,
            });
        };
        if tls.callbacks_address == 0 {
            return Ok(TlsCallbacks {
                image: self,
                rva: 0,
                done: true,
            });
        }
        let rva = tls
            .callbacks_address
            .checked_sub(self.image_base)
            .ok_or(PeError::TlsOutOfBounds)?;
        let rva = u32::try_from(rva).map_err(|_| PeError::TlsOutOfBounds)?;
        Ok(TlsCallbacks {
            image: self,
            rva,
            done: false,
        })
    }

    fn window(self, rva: u32) -> Result<(usize, usize), PeError> {
        if rva < self.size_of_headers {
            let start = rva as usize;
            let end = min(self.size_of_headers as usize, self.bytes.len());
            if start >= end {
                return Err(PeError::RvaNotInFile(rva));
            }
            return Ok((start, end - start));
        }

        for section in self.sections() {
            let mapped = section.mapped_size();
            let mapped_end = section
                .virtual_address
                .checked_add(mapped)
                .ok_or(PeError::RangeOverflow)?;
            if rva < section.virtual_address || rva >= mapped_end {
                continue;
            }
            let delta = rva - section.virtual_address;
            if delta >= section.raw_size {
                return Err(PeError::RvaNotInFile(rva));
            }
            let offset = section
                .raw_offset
                .checked_add(delta)
                .ok_or(PeError::RangeOverflow)? as usize;
            if offset >= self.bytes.len() {
                return Err(PeError::FileRange);
            }
            let available = min(
                (section.raw_size - delta) as usize,
                self.bytes.len() - offset,
            );
            return Ok((offset, available));
        }
        Err(PeError::RvaNotMapped(rva))
    }

    fn c_string_rva(self, rva: u32) -> Result<&'a [u8], PeError> {
        let (offset, available) = self.window(rva).map_err(|error| match error {
            PeError::RvaNotMapped(_) | PeError::RvaNotInFile(_) | PeError::FileRange => {
                PeError::ImportOutOfBounds
            }
            other => other,
        })?;
        let window = self
            .bytes
            .get(offset..offset + available)
            .ok_or(PeError::FileRange)?;
        let len = window
            .iter()
            .position(|&byte| byte == 0)
            .ok_or(PeError::ImportOutOfBounds)?;
        Ok(&window[..len])
    }
}

/// 节头迭代器。
pub struct Sections<'a> {
    image: PeImage<'a>,
    next_index: u16,
}

impl<'a> Iterator for Sections<'a> {
    type Item = Section<'a>;

    fn next(&mut self) -> Option<Section<'a>> {
        if self.next_index >= self.image.section_count {
            return None;
        }
        let offset =
            self.image.section_table_offset + self.next_index as usize * SECTION_HEADER_LEN;
        self.next_index += 1;
        let section = Section {
            name_bytes: &self.image.bytes[offset..offset + 8],
            virtual_size: read_u32(self.image.bytes, offset + 8).ok()?,
            virtual_address: read_u32(self.image.bytes, offset + 12).ok()?,
            raw_size: read_u32(self.image.bytes, offset + 16).ok()?,
            raw_offset: read_u32(self.image.bytes, offset + 20).ok()?,
            characteristics: read_u32(self.image.bytes, offset + 36).ok()?,
        };
        Some(section)
    }
}

/// TLS 目录（PE32+）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsDirectory {
    raw_start: u64,
    raw_end: u64,
    index_address: u64,
    callbacks_address: u64,
    zero_fill: u32,
    characteristics: u32,
}

impl TlsDirectory {
    pub fn raw_start(self) -> u64 {
        self.raw_start
    }

    pub fn raw_end(self) -> u64 {
        self.raw_end
    }

    pub fn index_address(self) -> u64 {
        self.index_address
    }

    pub fn callbacks_address(self) -> u64 {
        self.callbacks_address
    }

    pub fn zero_fill(self) -> u32 {
        self.zero_fill
    }

    pub fn characteristics(self) -> u32 {
        self.characteristics
    }
}

/// TLS 回调 VA 迭代器。
pub struct TlsCallbacks<'a> {
    image: PeImage<'a>,
    rva: u32,
    done: bool,
}

impl<'a> Iterator for TlsCallbacks<'a> {
    type Item = Result<u64, PeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let entry = match self.image.slice_rva(self.rva, 8) {
            Ok(entry) => entry,
            Err(_) => {
                self.done = true;
                return Some(Err(PeError::TlsOutOfBounds));
            }
        };
        let value = match read_u64(entry, 0) {
            Ok(value) => value,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        if value == 0 {
            self.done = true;
            return None;
        }
        if value
            .checked_sub(self.image.image_base)
            .is_none_or(|rva| rva >= self.image.size_of_image as u64)
        {
            self.done = true;
            return Some(Err(PeError::TlsOutOfBounds));
        }
        self.rva = match self.rva.checked_add(8) {
            Some(next) => next,
            None => {
                self.done = true;
                return Some(Err(PeError::TlsOutOfBounds));
            }
        };
        Some(Ok(value))
    }
}

/// 导入描述符。
#[derive(Clone, Copy, Debug)]
pub struct ImportDescriptor<'a> {
    image: PeImage<'a>,
    name: &'a [u8],
    original_first_thunk: u32,
    first_thunk: u32,
}

impl<'a> ImportDescriptor<'a> {
    pub fn name(self) -> &'a [u8] {
        self.name
    }

    pub fn original_first_thunk(self) -> u32 {
        self.original_first_thunk
    }

    pub fn first_thunk(self) -> u32 {
        self.first_thunk
    }

    /// 迭代导入 thunk；优先使用 ILT，缺失时退回 IAT。
    pub fn thunks(self) -> Result<ImportThunks<'a>, PeError> {
        let rva = if self.original_first_thunk != 0 {
            self.original_first_thunk
        } else {
            self.first_thunk
        };
        Ok(ImportThunks {
            image: self.image,
            rva,
            done: rva == 0,
        })
    }
}

/// 导入描述符迭代器。
pub struct Imports<'a> {
    image: PeImage<'a>,
    rva: u32,
    remaining: u32,
    done: bool,
}

impl<'a> Iterator for Imports<'a> {
    type Item = Result<ImportDescriptor<'a>, PeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        const DESCRIPTOR_LEN: u32 = 20;
        if self.remaining < DESCRIPTOR_LEN {
            self.done = true;
            return None;
        }
        let raw = match self.image.slice_rva(self.rva, DESCRIPTOR_LEN) {
            Ok(raw) => raw,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        let original_first_thunk = read_u32(raw, 0).ok()?;
        let name_rva = read_u32(raw, 12).ok()?;
        let first_thunk = read_u32(raw, 16).ok()?;
        if original_first_thunk == 0 && name_rva == 0 && first_thunk == 0 {
            self.done = true;
            return None;
        }
        let name = match self.image.c_string_rva(name_rva) {
            Ok(name) => name,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        self.rva = match self.rva.checked_add(DESCRIPTOR_LEN) {
            Some(next) => next,
            None => {
                self.done = true;
                return Some(Err(PeError::RangeOverflow));
            }
        };
        self.remaining -= DESCRIPTOR_LEN;
        Some(Ok(ImportDescriptor {
            image: self.image,
            name,
            original_first_thunk,
            first_thunk,
        }))
    }
}

/// 单个导入 thunk。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportThunk<'a> {
    Ordinal(u16),
    Name { hint: u16, name: &'a [u8] },
}

/// 导入 thunk 迭代器。
pub struct ImportThunks<'a> {
    image: PeImage<'a>,
    rva: u32,
    done: bool,
}

impl<'a> Iterator for ImportThunks<'a> {
    type Item = Result<ImportThunk<'a>, PeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let raw = match self.image.slice_rva(self.rva, 8) {
            Ok(raw) => raw,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        let value = match read_u64(raw, 0) {
            Ok(value) => value,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        if value == 0 {
            self.done = true;
            return None;
        }
        let next_rva = match self.rva.checked_add(8) {
            Some(next) => next,
            None => {
                self.done = true;
                return Some(Err(PeError::RangeOverflow));
            }
        };
        self.rva = next_rva;

        if value & IMPORT_ORDINAL_FLAG != 0 {
            return Some(Ok(ImportThunk::Ordinal(value as u16)));
        }
        let name_rva = value as u32;
        let hint = match self.image.slice_rva(name_rva, 2) {
            Ok(raw) => read_u16(raw, 0).unwrap_or(0),
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        let name_offset = match name_rva.checked_add(2) {
            Some(offset) => offset,
            None => {
                self.done = true;
                return Some(Err(PeError::RangeOverflow));
            }
        };
        let name = match self.image.c_string_rva(name_offset) {
            Ok(name) => name,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        Some(Ok(ImportThunk::Name { hint, name }))
    }
}

/// 基址重定位块。
#[derive(Clone, Copy, Debug)]
pub struct RelocationBlock<'a> {
    page_rva: u32,
    entries: &'a [u8],
}

impl<'a> RelocationBlock<'a> {
    pub fn page_rva(self) -> u32 {
        self.page_rva
    }

    pub fn entries(self) -> RelocationEntries<'a> {
        RelocationEntries {
            entries: self.entries,
        }
    }
}

/// 重定位块迭代器。
pub struct RelocationBlocks<'a> {
    image: PeImage<'a>,
    rva: u32,
    remaining: u32,
    done: bool,
}

impl<'a> Iterator for RelocationBlocks<'a> {
    type Item = Result<RelocationBlock<'a>, PeError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        if self.remaining < 8 {
            self.done = true;
            return None;
        }
        let header = match self.image.slice_rva(self.rva, 8) {
            Ok(header) => header,
            Err(error) => {
                self.done = true;
                return Some(Err(error));
            }
        };
        let page_rva = read_u32(header, 0).ok()?;
        let block_size = read_u32(header, 4).ok()?;
        if page_rva == 0 && block_size == 0 {
            self.done = true;
            return None;
        }
        if block_size < 8 || block_size % 2 != 0 || block_size > self.remaining {
            self.done = true;
            return Some(Err(PeError::RelocationOutOfBounds));
        }
        let entries_rva = match self.rva.checked_add(8) {
            Some(rva) => rva,
            None => {
                self.done = true;
                return Some(Err(PeError::RangeOverflow));
            }
        };
        let entries = match self.image.slice_rva(entries_rva, block_size - 8) {
            Ok(entries) => entries,
            Err(_) => {
                self.done = true;
                return Some(Err(PeError::RelocationOutOfBounds));
            }
        };
        self.rva = match self.rva.checked_add(block_size) {
            Some(rva) => rva,
            None => {
                self.done = true;
                return Some(Err(PeError::RangeOverflow));
            }
        };
        self.remaining -= block_size;
        Some(Ok(RelocationBlock { page_rva, entries }))
    }
}

/// 单个重定位项。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RelocationEntry {
    Absolute,
    Dir64 { offset: u32 },
    HighLow { offset: u32 },
    Unsupported { kind: u8, offset: u32 },
}

/// 重定位项迭代器。
pub struct RelocationEntries<'a> {
    entries: &'a [u8],
}

impl Iterator for RelocationEntries<'_> {
    type Item = RelocationEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let first = *self.entries.first()?;
        let second = *self.entries.get(1)?;
        self.entries = &self.entries[2..];
        let raw = u16::from_le_bytes([first, second]);
        let kind = (raw >> 12) as u8;
        let offset = (raw & 0x0fff) as u32;
        Some(match kind {
            0 => RelocationEntry::Absolute,
            3 => RelocationEntry::HighLow { offset },
            10 => RelocationEntry::Dir64 { offset },
            _ => RelocationEntry::Unsupported { kind, offset },
        })
    }
}

fn align_up(value: u32, alignment: u32) -> Result<u32, PeError> {
    if alignment == 0 || !alignment.is_power_of_two() {
        return Err(PeError::Alignment);
    }
    let sum = value
        .checked_add(alignment - 1)
        .ok_or(PeError::RangeOverflow)?;
    Ok(sum & !(alignment - 1))
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, PeError> {
    let raw = bytes
        .get(offset..offset.checked_add(2).ok_or(PeError::RangeOverflow)?)
        .ok_or(PeError::Truncated)?;
    Ok(u16::from_le_bytes([raw[0], raw[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, PeError> {
    let raw = bytes
        .get(offset..offset.checked_add(4).ok_or(PeError::RangeOverflow)?)
        .ok_or(PeError::Truncated)?;
    Ok(u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, PeError> {
    let raw = bytes
        .get(offset..offset.checked_add(8).ok_or(PeError::RangeOverflow)?)
        .ok_or(PeError::Truncated)?;
    Ok(u64::from_le_bytes([
        raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
    ]))
}
