//! M17c：静态 ELF64 的加载计划与初始栈（纯逻辑，host 可测）。
//!
//! 只接受 `ET_EXEC`、`EM_X86_64`、无 `PT_INTERP` 的静态可执行文件；输出
//! `PT_LOAD` 段描述与入口，并可在给定栈区上构造 `argc/argv/envp/auxv` 布局。
//! 真正的地址空间替换与 syscall 接线在后续片段。
//!
//! 段的对齐按 ELF 规范处理：`p_vaddr` 只要求与 `p_offset` 同余（模页大小），
//! 不要求页对齐；映射从 `p_vaddr` 向下对齐的页开始，文件数据从 `p_offset`
//! 向下对齐的偏移开始（见 [`ExecSegment::map_start`]/[`ExecSegment::file_base`]）。
//! 未修改的第三方静态产物（例如 `busybox` 的 RW 段 `vaddr≡0x820`）依赖这一条；
//! 本仓库自建的映像是页对齐的，因此该扩展不放宽任何既有检查。

/// 单个可执行文件接受的 `PT_LOAD` 段上限。
///
/// 实测静态 `busybox` 只需 4 个 `PT_LOAD`；这里留一倍余量以容纳常见的
/// "代码/只读/可重定位只读/数据" 四段再加 TLS 与对齐填充段的变体。超过上限
/// 确定性失败（[`ExecError::TooManySegments`]），不做静默截断。
pub const MAX_EXEC_SEGMENTS: usize = 8;

/// ELF64 头长度。
pub const ELF_HEADER_LEN: usize = 64;
/// ELF64 程序头长度。
pub const PROGRAM_HEADER_LEN: usize = 56;

/// `auxv` 里本内核会提供的键（取值见 [`static_auxv`] 与 [`build_initial_stack`]）。
pub const AT_PHDR: u64 = 3;
pub const AT_PHENT: u64 = 4;
pub const AT_PHNUM: u64 = 5;
pub const AT_PAGESZ: u64 = 6;
pub const AT_BASE: u64 = 7;
pub const AT_FLAGS: u64 = 8;
pub const AT_ENTRY: u64 = 9;
pub const AT_UID: u64 = 11;
pub const AT_EUID: u64 = 12;
pub const AT_GID: u64 = 13;
pub const AT_EGID: u64 = 14;
pub const AT_HWCAP: u64 = 16;
pub const AT_CLKTCK: u64 = 17;
pub const AT_SECURE: u64 = 23;
pub const AT_EXECFN: u64 = 31;

const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const PT_GNU_STACK: u32 = 0x6474_e551;
const PT_GNU_RELRO: u32 = 0x6474_e552;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PF_X: u32 = 1;
const PF_W: u32 = 2;

/// 一个可加载段。
///
/// `memsz >= filesz`；`vaddr - map_start == offset - file_base` 恒成立（同余）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecSegment {
    /// `p_vaddr`：段首字节的 VA，只与 `offset` 同余，不要求页对齐。
    pub vaddr: u64,
    /// `p_vaddr` 向下对齐到页：映射的第一页 VA。
    pub map_start: u64,
    /// `p_offset`：段首字节在文件中的偏移。
    pub offset: u64,
    /// `p_offset` 向下对齐到页：映射首页对应的文件偏移。
    pub file_base: u64,
    pub memsz: u64,
    pub filesz: u64,
    pub writable: bool,
    pub executable: bool,
}

impl ExecSegment {
    /// `vaddr` 在映射首页内的偏移（`0..4096`）。
    pub fn lead(&self) -> u64 {
        self.vaddr - self.map_start
    }

    /// 从 `map_start` 起需要映射的页数（文件数据 + BSS 补齐）。
    pub fn pages(&self) -> u64 {
        (self.lead() + self.memsz).div_ceil(4096)
    }

    /// 映射首页内有文件数据覆盖的字节数（`lead` 与 `filesz` 之和）。
    pub fn file_bytes(&self) -> u64 {
        self.lead() + self.filesz
    }

    /// 段映射的结束 VA（包含最后一个补齐页）。
    pub fn map_end(&self) -> u64 {
        self.map_start + self.pages() * 4096
    }
}

/// 解析结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecPlan {
    pub entry: u64,
    pub segments: [Option<ExecSegment>; MAX_EXEC_SEGMENTS],
    pub count: usize,
    pub phdr: u64,
    pub phnum: u64,
}

/// 解析/栈构造失败原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecError {
    Truncated,
    BadMagic,
    NotExecutable,
    NotX86_64,
    BadProgramHeaders,
    Dynamic,
    TooManySegments,
    Misaligned,
    /// 一个 `PT_LOAD` 同时要求可写与可执行（W^X 违规），确定性拒绝。
    WritableExecutable,
    /// `PT_GNU_STACK` 显式要求可执行栈（`PF_X`），确定性拒绝。
    ExecutableStack,
    SegmentOutsideImage,
    StackTooSmall,
    StackEmpty,
    InvalidAddress,
    OverlappingSegments,
    InvalidEntry,
    InvalidString,
}

fn u16_at(data: &[u8], offset: usize) -> Result<u16, ExecError> {
    let bytes = data.get(offset..offset + 2).ok_or(ExecError::Truncated)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn u32_at(data: &[u8], offset: usize) -> Result<u32, ExecError> {
    let bytes = data.get(offset..offset + 4).ok_or(ExecError::Truncated)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn u64_at(data: &[u8], offset: usize) -> Result<u64, ExecError> {
    let bytes = data.get(offset..offset + 8).ok_or(ExecError::Truncated)?;
    let mut value = [0u8; 8];
    value.copy_from_slice(bytes);
    Ok(u64::from_le_bytes(value))
}

/// 解析静态 ELF64（`image_len` 用于校验段边界；`image` 可只含头部）。
pub fn plan_len(image: &[u8], image_len: u64) -> Result<ExecPlan, ExecError> {
    if image.len() < ELF_HEADER_LEN {
        return Err(ExecError::Truncated);
    }
    if image.get(0..4) != Some(b"\x7fELF") || image.get(4) != Some(&2) || image.get(5) != Some(&1) {
        return Err(ExecError::BadMagic);
    }
    if u16_at(image, 16)? != ET_EXEC {
        return Err(ExecError::NotExecutable);
    }
    if u16_at(image, 18)? != EM_X86_64 {
        return Err(ExecError::NotX86_64);
    }
    let entry = u64_at(image, 24)?;
    let phoff = u64_at(image, 32)? as usize;
    let phentsize = u16_at(image, 54)? as usize;
    let phnum = u16_at(image, 56)? as usize;
    if phentsize != PROGRAM_HEADER_LEN || phnum == 0 {
        return Err(ExecError::BadProgramHeaders);
    }
    let mut segments = [None; MAX_EXEC_SEGMENTS];
    let mut count = 0usize;
    for index in 0..phnum {
        let header = phoff
            .checked_add(index * PROGRAM_HEADER_LEN)
            .ok_or(ExecError::Truncated)?;
        let p_type = u32_at(image, header)?;
        let offset = u64_at(image, header + 8)?;
        let vaddr = u64_at(image, header + 16)?;
        let filesz = u64_at(image, header + 32)?;
        let memsz = u64_at(image, header + 40)?;
        let flags = u32_at(image, header + 4)?;
        match p_type {
            PT_INTERP => return Err(ExecError::Dynamic),
            // 可执行栈是显式的危险声明：确定性拒绝，而不是静默降级成可执行数据页。
            // 缺少 `PT_GNU_STACK` 的旧映像按"数据栈"处理，不拒绝。
            PT_GNU_STACK => {
                if flags & PF_X != 0 {
                    return Err(ExecError::ExecutableStack);
                }
                continue;
            }
            // `PT_GNU_RELRO` 只覆盖静态自举重定位目标，装载时仍须可写；只读化属于
            // 装载后的 mprotect 语义，本切片（内核装载且尚无 LibOS）不做，明确记录为缺口。
            PT_GNU_RELRO => continue,
            PT_LOAD => {}
            _ => continue,
        }
        if memsz < filesz || memsz == 0 {
            return Err(ExecError::BadProgramHeaders);
        }
        if filesz > 0 {
            let end = offset.checked_add(filesz).ok_or(ExecError::Truncated)?;
            if end > image_len {
                return Err(ExecError::SegmentOutsideImage);
            }
        }
        // 文件字节与 VA 必须同余（模页大小）：这是 Linux 装载器的唯一对齐前提。
        if vaddr & 0xfff != offset & 0xfff {
            return Err(ExecError::Misaligned);
        }
        if flags & PF_W != 0 && flags & PF_X != 0 {
            return Err(ExecError::WritableExecutable);
        }
        let map_start = vaddr & !0xfff;
        let end = vaddr.checked_add(memsz).ok_or(ExecError::InvalidAddress)?;
        if map_start < 4096 || end > 0x0000_7f00_0000_0000 {
            return Err(ExecError::InvalidAddress);
        }
        let page_end = end.checked_add(4095).ok_or(ExecError::InvalidAddress)? & !4095;
        if segments[..count]
            .iter()
            .flatten()
            .any(|part: &ExecSegment| map_start < part.map_end() && part.map_start < page_end)
        {
            return Err(ExecError::OverlappingSegments);
        }
        if count == MAX_EXEC_SEGMENTS {
            return Err(ExecError::TooManySegments);
        }
        segments[count] = Some(ExecSegment {
            vaddr,
            map_start,
            offset,
            file_base: offset & !0xfff,
            memsz,
            filesz,
            writable: flags & PF_W != 0,
            executable: flags & PF_X != 0,
        });
        count += 1;
    }
    if count == 0 {
        return Err(ExecError::NotExecutable);
    }
    if !segments
        .iter()
        .flatten()
        .any(|part| part.executable && entry >= part.vaddr && entry < part.vaddr + part.memsz)
    {
        return Err(ExecError::InvalidEntry);
    }
    let header_end = (phoff as u64)
        .checked_add((phnum * PROGRAM_HEADER_LEN) as u64)
        .ok_or(ExecError::BadProgramHeaders)?;
    let phdr = segments
        .iter()
        .flatten()
        .find_map(|part| {
            (part.offset <= phoff as u64 && header_end <= part.offset + part.filesz)
                .then(|| part.vaddr + phoff as u64 - part.offset)
        })
        .unwrap_or(0);
    Ok(ExecPlan {
        phdr,
        phnum: phnum as u64,
        entry,
        segments,
        count,
    })
}

/// 解析完整的 ELF64 镜像。
pub fn plan(image: &[u8]) -> Result<ExecPlan, ExecError> {
    plan_len(image, image.len() as u64)
}

/// 初始栈布局：`rsp` 指向 `argc`，其后是 argv/envp 指针数组与 auxv 对。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackLayout {
    pub rsp: u64,
    pub argc: u64,
}

/// exec 时提供的静态 `auxv` 条目（`AT_EXECFN` 之外的全部键）。
///
/// 与[静态程序契约](../../docs/superpowers/specs/2026-10-26-static-program-contract-design.md)
/// 及 M12 冻结集合一致：`AT_PAGESZ`/`AT_PHDR`/`AT_PHENT`/`AT_PHNUM`/`AT_ENTRY`/
/// `AT_BASE`/`AT_EXECFN`，另加取值恒定的身份与时钟项，使未修改的 libc 启动路径
/// （`__init_libc` 一类的读取）不需要特例分支。
///
/// 明确不提供：`AT_RANDOM`（项目无安全熵，D13 未裁决）、`AT_PLATFORM`（需要额外
/// 栈上字符串）、`AT_SYSINFO_EHDR`（无 vDSO）。`AT_HWCAP` 取 0，即"不宣告任何
/// 可选 CPU 特性"；libc 仍须用 `cpuid` 判定，不得把 0 读成"特性缺失"的证明。
pub fn static_auxv(plan: &ExecPlan) -> [(u64, u64); 13] {
    [
        (AT_PAGESZ, 4096),
        (AT_PHDR, plan.phdr),
        (AT_PHENT, PROGRAM_HEADER_LEN as u64),
        (AT_PHNUM, plan.phnum),
        (AT_BASE, 0),
        (AT_ENTRY, plan.entry),
        (AT_FLAGS, 0),
        (AT_SECURE, 0),
        (AT_UID, 0),
        (AT_EUID, 0),
        (AT_GID, 0),
        (AT_EGID, 0),
        (AT_CLKTCK, 100),
    ]
}

/// 构造 exec 的完整初始栈：`static_auxv` 加 `AT_HWCAP` 与 `AT_EXECFN`
/// （后者指向栈上的 `argv[0]`）。
pub fn build_initial_stack(
    top: u64,
    plan: &ExecPlan,
    argv: &[&[u8]],
    envp: &[&[u8]],
    buffer: &mut [u8],
) -> Result<StackLayout, ExecError> {
    let mut pairs = [(0u64, 0u64); 14];
    for (slot, (key, value)) in pairs.iter_mut().zip(static_auxv(plan)) {
        *slot = (key, value);
    }
    pairs[13] = (AT_HWCAP, 0);
    build_stack_inner(top, argv, envp, &pairs, true, buffer)
}

/// 在 `buffer`（地址区间 `[top - len, top)`）上自顶向下构造初始栈。
///
/// 布局（低 → 高）：`argc`、`argv[0..n]`、`NULL`、`envp[0..m]`、`NULL`、
/// `auxv` 键值对、`AT_NULL(0,0)`；字符串在指针数组之上（高地址）。
/// 返回 `rsp`（8 字节对齐、指向 `argc`）与 `argc`。
pub fn build_stack(
    top: u64,
    argv: &[&[u8]],
    envp: &[&[u8]],
    auxv: &[(u64, u64)],
    buffer: &mut [u8],
) -> Result<StackLayout, ExecError> {
    build_stack_inner(top, argv, envp, auxv, false, buffer)
}

fn build_stack_inner(
    top: u64,
    argv: &[&[u8]],
    envp: &[&[u8]],
    auxv: &[(u64, u64)],
    execfn: bool,
    buffer: &mut [u8],
) -> Result<StackLayout, ExecError> {
    if buffer.len() < 64 || top & 7 != 0 {
        return Err(ExecError::Misaligned);
    }
    if top < buffer.len() as u64 {
        return Err(ExecError::InvalidAddress);
    }
    if argv.iter().chain(envp).any(|s| s.contains(&0)) {
        return Err(ExecError::InvalidString);
    }
    let tail = if execfn { 2 } else { 0 };
    let word_count_needed = 1 + argv.len() + 1 + envp.len() + 1 + 2 * auxv.len() + 2 + tail;
    if word_count_needed > 64 {
        return Err(ExecError::StackTooSmall);
    }
    let mut pos = 0usize; // 距栈顶的字节数（向下增长）
    let mut argv_ptrs = [0u64; 16];
    let mut envp_ptrs = [0u64; 16];
    if argv.len() > 16 || envp.len() > 16 {
        return Err(ExecError::StackTooSmall);
    }
    let mut put_string = |pos: &mut usize, data: &[u8]| -> Result<u64, ExecError> {
        // C 字符串以 NUL 结尾：数据在高地址、NUL 紧随其下。
        *pos = pos
            .checked_add(data.len() + 1)
            .ok_or(ExecError::StackTooSmall)?;
        if *pos > buffer.len() {
            return Err(ExecError::StackTooSmall);
        }
        let start = buffer.len() - *pos;
        buffer[start..start + data.len()].copy_from_slice(data);
        buffer[start + data.len()] = 0;
        Ok(top - *pos as u64)
    };
    for (index, arg) in argv.iter().enumerate() {
        argv_ptrs[index] = put_string(&mut pos, arg)?;
    }
    for (index, env) in envp.iter().enumerate() {
        envp_ptrs[index] = put_string(&mut pos, env)?;
    }
    pos = (pos + 7) & !7;
    let mut words = [0u64; 64];
    let mut word_count = 0usize;
    let mut push = |value: u64| {
        words[word_count] = value;
        word_count += 1;
    };
    push(argv.len() as u64);
    for pointer in argv_ptrs.iter().take(argv.len()) {
        push(*pointer);
    }
    push(0);
    for pointer in envp_ptrs.iter().take(envp.len()) {
        push(*pointer);
    }
    push(0);
    for &(key, value) in auxv {
        push(key);
        push(value);
    }
    if execfn {
        // `AT_EXECFN` 必须指向本映像的 `argv[0]`（Linux 语义），因此只能在字符串
        // 地址确定之后追加；没有 argv 时取 0（无自证可执行名）。
        push(AT_EXECFN);
        push(argv_ptrs[0]);
    }
    push(0);
    push(0);
    let bytes = word_count * 8;
    if pos + bytes > buffer.len() {
        return Err(ExecError::StackTooSmall);
    }
    // Linux x86_64 starts a new image with a 16-byte-aligned stack pointer.
    let padding = (top - (pos + bytes) as u64) as usize & 15;
    pos += padding;
    if pos + bytes > buffer.len() {
        return Err(ExecError::StackTooSmall);
    }
    let start = buffer.len() - pos - bytes;
    for (index, word) in words.iter().take(word_count).enumerate() {
        buffer[start + index * 8..start + index * 8 + 8].copy_from_slice(&word.to_le_bytes());
    }
    Ok(StackLayout {
        rsp: top - (pos + bytes) as u64,
        argc: argv.len() as u64,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PF_R: u32 = 4;

    /// 合成 ELF64：`segments` 是 `PT_LOAD` 的
    /// `(p_offset, p_vaddr, p_filesz, p_memsz, p_flags)`；`extra` 是附加程序头
    /// `(p_type, p_flags)`（如 `PT_GNU_STACK`），文件体补零。
    fn synthetic_elf_with(segments: &[(u64, u64, u64, u64, u32)], extra: &[(u32, u32)]) -> Vec<u8> {
        let phnum = segments.len() + extra.len();
        let mut image = vec![0u8; ELF_HEADER_LEN];
        image[0..4].copy_from_slice(b"\x7fELF");
        image[4] = 2;
        image[5] = 1;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        image[24..32].copy_from_slice(&0x400000u64.to_le_bytes());
        image[32..40].copy_from_slice(&(ELF_HEADER_LEN as u64).to_le_bytes());
        image[54..56].copy_from_slice(&(PROGRAM_HEADER_LEN as u16).to_le_bytes());
        image[56..58].copy_from_slice(&(phnum as u16).to_le_bytes());
        let mut write_header =
            |p_type: u32, offset: u64, vaddr: u64, filesz: u64, memsz: u64, flags: u32| {
                let mut header = [0u8; PROGRAM_HEADER_LEN];
                header[0..4].copy_from_slice(&p_type.to_le_bytes());
                header[4..8].copy_from_slice(&flags.to_le_bytes());
                header[8..16].copy_from_slice(&offset.to_le_bytes());
                header[16..24].copy_from_slice(&vaddr.to_le_bytes());
                header[32..40].copy_from_slice(&filesz.to_le_bytes());
                header[40..48].copy_from_slice(&memsz.to_le_bytes());
                header[48..56].copy_from_slice(&4096u64.to_le_bytes());
                image.extend_from_slice(&header);
            };
        for &(offset, vaddr, filesz, memsz, flags) in segments {
            write_header(PT_LOAD, offset, vaddr, filesz, memsz, flags);
        }
        for &(p_type, flags) in extra {
            write_header(p_type, 0, 0, 0, 0, flags);
        }
        image
    }

    fn synthetic_elf(segments: &[(u64, u64, u64, u32)]) -> Vec<u8> {
        let rows: Vec<(u64, u64, u64, u64, u32)> = segments
            .iter()
            .map(|&(vaddr, filesz, memsz, flags)| (0, vaddr, filesz, memsz, flags))
            .collect();
        synthetic_elf_with(&rows, &[])
    }

    #[test]
    fn plan_len_accepts_prefix_with_segment_bound_checked_against_length() {
        let mut image = synthetic_elf(&[(0x400000, 200, 200, PF_X)]);
        image.resize(320, 0);
        let prefix = &image[..64 + PROGRAM_HEADER_LEN];
        assert_eq!(plan(prefix), Err(ExecError::SegmentOutsideImage));
        let plan = plan_len(prefix, image.len() as u64).expect("plan_len");
        assert_eq!(plan.entry, 0x400000);
        assert_eq!(plan.count, 1);
    }

    #[test]
    fn plan_accepts_static_elf_with_two_load_segments() {
        let image = synthetic_elf(&[(0x400000, 16, 16, PF_X), (0x600000, 8, 24, PF_W)]);
        let plan = plan(&image).expect("plan");
        assert_eq!(plan.entry, 0x400000);
        assert_eq!(plan.count, 2);
        let first = plan.segments[0].expect("first");
        assert_eq!(first.vaddr, 0x400000);
        assert_eq!(first.map_start, 0x400000);
        assert_eq!(first.lead(), 0);
        assert!(first.executable);
        assert!(!first.writable);
        let second = plan.segments[1].expect("second");
        assert_eq!(second.memsz, 24);
        assert_eq!(second.pages(), 1);
        assert!(second.writable);
    }

    #[test]
    fn plan_accepts_congruent_unaligned_load_segments() {
        // 与静态 busybox 同形：RW 段的 p_offset/p_vaddr 只同余到 0x820，并不页对齐。
        // 映射必须从 0x401000（向下对齐）开始，文件数据从 0x1000 起。
        let mut image = synthetic_elf_with(
            &[
                (0, 0x400000, 0x840, 0x840, PF_R | PF_X),
                (0x1820, 0x401820, 0x100, 0x200, PF_R | PF_W),
            ],
            &[],
        );
        image.resize(0x1920, 0);
        let plan = plan(&image).expect("plan");
        assert_eq!(plan.count, 2);
        let data = plan.segments[1].expect("data segment");
        assert_eq!(data.vaddr, 0x401820);
        assert_eq!(data.map_start, 0x401000);
        assert_eq!(data.offset, 0x1820);
        assert_eq!(data.file_base, 0x1000);
        assert_eq!(data.lead(), 0x820);
        assert_eq!(data.file_bytes(), 0x920);
        assert_eq!(data.pages(), 1);
        assert_eq!(data.map_end(), 0x402000);
        // 程序头必须仍能从第一段的文件数据里定位（AT_PHDR 的可读前提）。
        assert_eq!(plan.phdr, 0x400000 + ELF_HEADER_LEN as u64);
    }

    #[test]
    fn plan_rejects_dynamic_pie_and_bad_headers() {
        let mut image = synthetic_elf(&[(0x400000, 16, 16, PF_X)]);
        image[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN
        assert_eq!(plan(&image), Err(ExecError::NotExecutable));

        let mut image = synthetic_elf(&[(0x400000, 16, 16, PF_X)]);
        // 把第一个程序头改成 PT_INTERP。
        let phoff = 64usize;
        image[phoff..phoff + 4].copy_from_slice(&PT_INTERP.to_le_bytes());
        assert_eq!(plan(&image), Err(ExecError::Dynamic));

        let image = synthetic_elf(&[(0x400123, 16, 16, PF_X)]);
        assert_eq!(plan(&image), Err(ExecError::Misaligned));

        // 文件偏移 0x1000 配 VA 0x400123：不同余，映射会错位。
        let mut image = synthetic_elf_with(&[(0x1000, 0x400123, 16, 16, PF_X)], &[]);
        image.resize(0x1010, 0);
        assert_eq!(plan(&image), Err(ExecError::Misaligned));

        assert_eq!(plan(&[]), Err(ExecError::Truncated));
    }

    #[test]
    fn plan_rejects_writable_executable_and_executable_stack() {
        let image = synthetic_elf(&[(0x400000, 16, 16, PF_X | PF_W)]);
        assert_eq!(plan(&image), Err(ExecError::WritableExecutable));

        let image = synthetic_elf_with(&[(0, 0x400000, 16, 16, PF_X)], &[(PT_GNU_STACK, PF_X)]);
        assert_eq!(plan(&image), Err(ExecError::ExecutableStack));

        // 不可执行栈（含缺省/无 `PT_GNU_STACK`）必须照常接受。
        let image = synthetic_elf_with(
            &[(0, 0x400000, 16, 16, PF_X)],
            &[(PT_GNU_STACK, PF_W), (PT_GNU_RELRO, PF_R)],
        );
        assert_eq!(plan(&image).expect("plan").count, 1);
    }

    #[test]
    fn load_segment_cap_is_frozen_and_overflow_is_deterministic() {
        let rows: Vec<(u64, u64, u64, u64, u32)> = (0..MAX_EXEC_SEGMENTS)
            .map(|index| (0, 0x400000 + 0x1000 * index as u64, 0x10, 0x10, PF_R | PF_X))
            .collect();
        let image = synthetic_elf_with(&rows, &[]);
        let accepted = plan(&image).expect("plan at cap");
        assert_eq!(accepted.count, MAX_EXEC_SEGMENTS);

        let mut rows = rows;
        rows.push((
            0,
            0x400000 + 0x1000 * MAX_EXEC_SEGMENTS as u64,
            0x10,
            0x10,
            PF_R | PF_X,
        ));
        let image = synthetic_elf_with(&rows, &[]);
        assert_eq!(plan(&image), Err(ExecError::TooManySegments));
    }

    #[test]
    fn stack_layout_places_argc_argv_envp_auxv_and_strings() {
        let top = 0x0000_0000_8000_0000u64;
        let mut buffer = vec![0u8; 4096];
        let layout = build_stack(
            top,
            &[b"ab".as_slice(), b"c".as_slice()],
            &[b"X=1".as_slice()],
            &[(6, 0x1000)],
            &mut buffer,
        )
        .expect("stack");
        assert_eq!(layout.argc, 2);
        assert_eq!(layout.rsp & 7, 0);
        let base_index = |addr: u64| buffer.len() - (top - addr) as usize;
        let read_word = |addr: u64| {
            let index = base_index(addr);
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buffer[index..index + 8]);
            u64::from_le_bytes(bytes)
        };
        assert_eq!(read_word(layout.rsp), 2);
        let argv0 = read_word(layout.rsp + 8);
        let argv1 = read_word(layout.rsp + 16);
        let argv_null = read_word(layout.rsp + 24);
        assert_eq!(argv_null, 0);
        let string_at = |addr: u64| {
            let start = base_index(addr);
            let mut end = start;
            while buffer[end] != 0 {
                end += 1;
            }
            String::from_utf8(buffer[start..end].to_vec()).expect("utf8")
        };
        assert_eq!(string_at(argv0), "ab");
        assert_eq!(string_at(argv1), "c");
        let env0 = read_word(layout.rsp + 32);
        assert_eq!(string_at(env0), "X=1");
        assert_eq!(read_word(layout.rsp + 40), 0);
        let aux_key = read_word(layout.rsp + 48);
        assert_eq!(aux_key, 6);
        assert_eq!(read_word(layout.rsp + 56), 0x1000);
        assert_eq!(read_word(layout.rsp + 64), 0);
        assert_eq!(read_word(layout.rsp + 72), 0);
    }

    #[test]
    fn initial_stack_carries_frozen_auxv_and_execfn_pointer() {
        let mut image = synthetic_elf(&[(0x400000, 0x200, 0x200, PF_X)]);
        image.resize(0x200, 0);
        let plan = plan(&image).expect("plan");
        let top = 0x0000_0000_8000_0000u64;
        let mut buffer = vec![0u8; 4096];
        let layout = build_initial_stack(
            top,
            &plan,
            &[b"busybox".as_slice()],
            &[b"PATH=/".as_slice()],
            &mut buffer,
        )
        .expect("stack");
        assert_eq!(layout.rsp & 15, 0);
        let base_index = |addr: u64| buffer.len() - (top - addr) as usize;
        let read_word = |addr: u64| {
            let index = base_index(addr);
            let mut bytes = [0u8; 8];
            bytes.copy_from_slice(&buffer[index..index + 8]);
            u64::from_le_bytes(bytes)
        };
        let argc = read_word(layout.rsp);
        assert_eq!(argc, 1);
        let argv0 = read_word(layout.rsp + 8);
        assert_eq!(read_word(layout.rsp + 16), 0);
        let env0 = read_word(layout.rsp + 24);
        assert_ne!(env0, 0);
        assert_eq!(read_word(layout.rsp + 32), 0);
        let mut at = layout.rsp + 40;
        let mut pairs = Vec::new();
        while read_word(at) != 0 || read_word(at + 8) != 0 {
            pairs.push((read_word(at), read_word(at + 8)));
            at += 16;
        }
        let value = |key: u64| {
            pairs
                .iter()
                .find(|(pair_key, _)| *pair_key == key)
                .map(|(_, value)| *value)
        };
        assert_eq!(value(AT_PAGESZ), Some(4096));
        assert_eq!(value(AT_PHDR), Some(0x400000 + ELF_HEADER_LEN as u64));
        assert_eq!(value(AT_PHENT), Some(PROGRAM_HEADER_LEN as u64));
        assert_eq!(value(AT_PHNUM), Some(1));
        assert_eq!(value(AT_BASE), Some(0));
        assert_eq!(value(AT_ENTRY), Some(0x400000));
        assert_eq!(value(AT_FLAGS), Some(0));
        assert_eq!(value(AT_SECURE), Some(0));
        assert_eq!(value(AT_UID), Some(0));
        assert_eq!(value(AT_EUID), Some(0));
        assert_eq!(value(AT_GID), Some(0));
        assert_eq!(value(AT_EGID), Some(0));
        assert_eq!(value(AT_CLKTCK), Some(100));
        assert_eq!(value(AT_HWCAP), Some(0));
        // AT_EXECFN 必须指向栈上的 argv[0]，而不是一个字面量或空值。
        let (_, execfn) = pairs
            .iter()
            .find(|(key, _)| *key == AT_EXECFN)
            .copied()
            .expect("AT_EXECFN");
        assert_eq!(execfn, argv0);
        let start = base_index(execfn);
        assert_eq!(&buffer[start..start + 7], b"busybox");
        // 无安全熵：AT_RANDOM 必须缺省，不能伪造可预测的"随机"。
        assert_eq!(value(25), None);
        assert_eq!(pairs.len(), 15);
    }

    #[test]
    fn malformed_load_ranges_and_entry_are_rejected_before_loading() {
        let overlap = synthetic_elf(&[(0x400000, 8, 8192, PF_X), (0x401000, 8, 8, PF_W)]);
        assert_eq!(plan(&overlap), Err(ExecError::OverlappingSegments));
        // 页对齐后重叠同样拒绝：只相差页内偏移的两个段落在同一页。
        let mut overlap = synthetic_elf_with(
            &[
                (0, 0x400000, 0x840, 0x840, PF_X),
                (0x1820, 0x400820, 8, 8, PF_W),
            ],
            &[],
        );
        overlap.resize(0x1830, 0);
        assert_eq!(plan(&overlap), Err(ExecError::OverlappingSegments));
        let invalid = synthetic_elf(&[(0xffff_ffff_ffff_f000, 8, 8192, PF_X)]);
        assert_eq!(plan(&invalid), Err(ExecError::InvalidAddress));
        let non_exec = synthetic_elf(&[(0x400000, 8, 8, PF_W)]);
        assert_eq!(plan(&non_exec), Err(ExecError::InvalidEntry));
        // p_memsz < p_filesz 与空段都是坏头。
        let bad = synthetic_elf(&[(0x400000, 16, 8, PF_X)]);
        assert_eq!(plan(&bad), Err(ExecError::BadProgramHeaders));
        let empty = synthetic_elf(&[(0x400000, 0, 0, PF_X)]);
        assert_eq!(plan(&empty), Err(ExecError::BadProgramHeaders));
    }

    #[test]
    fn stack_rejects_oversized_auxv_and_aligns_rsp_to_sixteen_bytes() {
        let mut bytes = [0; 4096];
        assert_eq!(
            build_stack(0x8000, &[], &[], &[(1, 2); 32], &mut bytes),
            Err(ExecError::StackTooSmall)
        );
        let out = build_stack(0x8000, &[b"a", b"bb"], &[b"X=y"], &[(6, 4096)], &mut bytes).unwrap();
        assert_eq!(out.rsp & 15, 0);
    }

    #[test]
    fn stack_rejects_tiny_buffers() {
        let mut buffer = [0u8; 32];
        assert_eq!(
            build_stack(0x1000, &[], &[], &[], &mut buffer),
            Err(ExecError::Misaligned)
        );
        let mut buffer = [0u8; 64];
        assert!(
            build_stack(
                0x1000,
                &[b"0123456789012345678901234567890123456789".as_slice()],
                &[],
                &[],
                &mut buffer
            )
            .is_err()
        );
    }

    /// 运维提供的未修改静态 ELF 的装载演练（默认跳过，仓库不携带二进制）。
    ///
    /// 运行：`PRIMA_ELF_SAMPLE=/usr/bin/busybox cargo test -p kernel --lib -- --ignored`。
    /// 断言的是**解析结果**，不是运行结果：本切片没有 Linux LibOS，见
    /// `docs/superpowers/reports/2026-10-28-stage6-simple-slice.md`。
    #[test]
    #[ignore = "needs PRIMA_ELF_SAMPLE pointing at an unmodified static Linux ELF"]
    fn plan_accepts_operator_supplied_static_linux_elf() {
        let path = std::env::var("PRIMA_ELF_SAMPLE").expect("PRIMA_ELF_SAMPLE");
        let bytes = std::fs::read(&path).expect("read sample");
        let plan = plan(&bytes).expect("static ELF must plan");
        assert!(plan.count > 0);
        assert_ne!(plan.phdr, 0, "AT_PHDR must resolve into a mapped segment");
        let exec = plan
            .segments
            .iter()
            .flatten()
            .find(|part| part.executable)
            .expect("executable segment");
        assert!(plan.entry >= exec.vaddr && plan.entry < exec.vaddr + exec.memsz);
        for part in plan.segments.iter().flatten() {
            assert_eq!(part.vaddr & 0xfff, part.offset & 0xfff);
            assert!(!(part.writable && part.executable));
            assert!(part.map_start >= 4096);
        }
    }
}

/// Retained mapping snapshot for an explicit exec handoff, independent of libc.
#[derive(Clone, Copy)]
pub(crate) struct RetainedPage {
    pub entry: crate::bootstrap::VspaceEntry,
    pub pte: u64,
}
pub(crate) const MAX_RETAINED_PAGES: usize = 128;
