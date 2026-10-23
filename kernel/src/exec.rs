//! M17c：静态 ELF64 的加载计划与初始栈（纯逻辑，host 可测）。
//!
//! 只接受 `ET_EXEC`、`EM_X86_64`、无 `PT_INTERP` 的静态可执行文件；输出
//! `PT_LOAD` 段描述与入口，并可在给定栈区上构造 `argc/argv/envp/auxv` 布局。
//! 真正的地址空间替换与 syscall 接线在后续片段。

/// 单个可执行文件接受的 `PT_LOAD` 段上限。
pub const MAX_EXEC_SEGMENTS: usize = 4;

/// ELF64 头长度。
pub const ELF_HEADER_LEN: usize = 64;
/// ELF64 程序头长度。
pub const PROGRAM_HEADER_LEN: usize = 56;

const PT_LOAD: u32 = 1;
const PT_INTERP: u32 = 3;
const ET_EXEC: u16 = 2;
const EM_X86_64: u16 = 62;
const PF_X: u32 = 1;
const PF_W: u32 = 2;

/// 一个可加载段（页对齐范围由调用方补齐；`memsz >= filesz`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecSegment {
    pub vaddr: u64,
    pub memsz: u64,
    pub filesz: u64,
    pub offset: u64,
    pub writable: bool,
    pub executable: bool,
}

/// 解析结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecPlan {
    pub entry: u64,
    pub segments: [Option<ExecSegment>; MAX_EXEC_SEGMENTS],
    pub count: usize,
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
    SegmentOutsideImage,
    StackTooSmall,
    StackEmpty,
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
        if p_type == PT_INTERP {
            return Err(ExecError::Dynamic);
        }
        if p_type != PT_LOAD {
            continue;
        }
        if memsz < filesz {
            return Err(ExecError::BadProgramHeaders);
        }
        if filesz > 0 {
            let end = offset.checked_add(filesz).ok_or(ExecError::Truncated)?;
            if end > image_len {
                return Err(ExecError::SegmentOutsideImage);
            }
        }
        if vaddr & 0xfff != 0 || memsz == 0 {
            return Err(ExecError::Misaligned);
        }
        if count == MAX_EXEC_SEGMENTS {
            return Err(ExecError::TooManySegments);
        }
        segments[count] = Some(ExecSegment {
            vaddr,
            memsz,
            filesz,
            offset,
            writable: flags & PF_W != 0,
            executable: flags & PF_X != 0,
        });
        count += 1;
    }
    if count == 0 {
        return Err(ExecError::NotExecutable);
    }
    Ok(ExecPlan {
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
    if buffer.len() < 64 || top & 7 != 0 {
        return Err(ExecError::Misaligned);
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
    push(0);
    push(0);
    let bytes = word_count * 8;
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

    fn synthetic_elf(segments: &[(u64, u64, u64, u32)]) -> Vec<u8> {
        let mut image = vec![0u8; 64];
        image[0..4].copy_from_slice(b"\x7fELF");
        image[4] = 2;
        image[5] = 1;
        image[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        image[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        image[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
        image[32..40].copy_from_slice(&64u64.to_le_bytes());
        image[54..56].copy_from_slice(&(PROGRAM_HEADER_LEN as u16).to_le_bytes());
        image[56..58].copy_from_slice(&(segments.len() as u16).to_le_bytes());
        for &(vaddr, filesz, memsz, flags) in segments {
            let mut header = [0u8; PROGRAM_HEADER_LEN];
            header[0..4].copy_from_slice(&PT_LOAD.to_le_bytes());
            header[4..8].copy_from_slice(&flags.to_le_bytes());
            header[8..16].copy_from_slice(&0u64.to_le_bytes());
            header[16..24].copy_from_slice(&vaddr.to_le_bytes());
            header[32..40].copy_from_slice(&filesz.to_le_bytes());
            header[40..48].copy_from_slice(&memsz.to_le_bytes());
            image.extend_from_slice(&header);
        }
        image
    }

    #[test]
    fn plan_len_accepts_prefix_with_segment_bound_checked_against_length() {
        let mut image = synthetic_elf(&[(0x400000, 200, 200, PF_X)]);
        image.resize(320, 0);
        let prefix = &image[..64 + PROGRAM_HEADER_LEN];
        assert_eq!(plan(prefix), Err(ExecError::SegmentOutsideImage));
        let plan = plan_len(prefix, image.len() as u64).expect("plan_len");
        assert_eq!(plan.entry, 0x401000);
        assert_eq!(plan.count, 1);
    }

    #[test]
    fn plan_accepts_static_elf_with_two_load_segments() {
        let image = synthetic_elf(&[(0x400000, 16, 16, PF_X), (0x600000, 8, 24, PF_W)]);
        let plan = plan(&image).expect("plan");
        assert_eq!(plan.entry, 0x401000);
        assert_eq!(plan.count, 2);
        let first = plan.segments[0].expect("first");
        assert_eq!(first.vaddr, 0x400000);
        assert!(first.executable);
        assert!(!first.writable);
        let second = plan.segments[1].expect("second");
        assert_eq!(second.memsz, 24);
        assert!(second.writable);
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

        assert_eq!(plan(&[]), Err(ExecError::Truncated));
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
}
