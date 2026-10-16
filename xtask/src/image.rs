use bootloader::{BiosBoot, UefiBoot};
use std::{
    fs,
    path::{Path, PathBuf},
};

const KERNEL_BINARY: &str = env!("CARGO_BIN_FILE_KERNEL_prima-kernel");
const BIOS_IMAGE_NAME: &str = "prima-os-bios.img";
const UEFI_IMAGE_NAME: &str = "prima-os-uefi.img";
const RAMDISK_IMAGE_NAME: &str = "prima-os-ramdisk.img";

/// EXO-2C：ramdisk 中的服务 ELF（init 映像由 `prima-initgen` 在第 0 项生成）。
///
/// 名称只出现在构建工具侧；内核按 opaque 序号读取。顺序必须与
/// `kernel/src/user.rs::install_boot_image` 的服务条目下标一致。
const SERVICE_IMAGE_BINARIES: [&str; 7] = [
    env!("CARGO_BIN_FILE_PRIMA_USER_prima-smoke"),
    env!("CARGO_BIN_FILE_PRIMA_USER_prima-beta"),
    env!("CARGO_BIN_FILE_PRIMA_USER_prima-servicemgr"),
    env!("CARGO_BIN_FILE_PRIMA_USER_prima-log"),
    env!("CARGO_BIN_FILE_PRIMA_USER_prima-ns"),
    env!("CARGO_BIN_FILE_PRIMA_USER_prima-store"),
    env!("CARGO_BIN_FILE_PRIMA_USER_prima-driver"),
];

#[derive(Debug)]
pub(crate) struct Images {
    pub(crate) bios: PathBuf,
}

pub(crate) fn build_images() -> Result<Images, String> {
    let kernel = PathBuf::from(KERNEL_BINARY);
    ensure_non_empty(&kernel, "kernel ELF")?;

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let output = workspace.join("target").join("prima-os");
    fs::create_dir_all(&output)
        .map_err(|error| format!("failed to create {}: {error}", output.display()))?;

    let ramdisk = output.join(RAMDISK_IMAGE_NAME);
    build_ramdisk(&ramdisk)?;

    let bios = output.join(BIOS_IMAGE_NAME);
    let uefi = output.join(UEFI_IMAGE_NAME);
    remove_stale_image(&bios)?;
    remove_stale_image(&uefi)?;

    BiosBoot::new(&kernel)
        .set_ramdisk(&ramdisk)
        .create_disk_image(&bios)
        .map_err(|error| format!("failed to create BIOS image: {error:#}"))?;
    UefiBoot::new(&kernel)
        .set_ramdisk(&ramdisk)
        .create_disk_image(&uefi)
        .map_err(|error| format!("failed to create UEFI image: {error:#}"))?;

    ensure_non_empty(&bios, "BIOS image")?;
    ensure_non_empty(&uefi, "UEFI image")?;

    Ok(Images { bios })
}

/// EXO-1：把用户态服务 ELF 打包成 bootloader `ramdisk` 文件。
///
/// 格式只含 magic/version/count + `(offset, len)` 表；内核不解释块名，块里的 ELF
/// program header 由内核通用解析器读取。
fn build_ramdisk(path: &Path) -> Result<(), String> {
    let mut service_blobs = Vec::with_capacity(SERVICE_IMAGE_BINARIES.len());
    for binary in SERVICE_IMAGE_BINARIES {
        let source = PathBuf::from(binary);
        ensure_non_empty(&source, "boot image binary")?;
        service_blobs.push(
            fs::read(&source)
                .map_err(|error| format!("failed to read {}: {error}", source.display()))?,
        );
    }

    // EXO-2C：init 机器码由 host 生成器产出，再封装成不透明 ELF 作为第 0 项。
    // 这样内核不再需要 `user_code()`，但仍在同一批 ramdisk 块中装载 init。
    let init_elf = build_init_elf(&service_blobs)?;
    let mut blobs = Vec::with_capacity(service_blobs.len() + 1);
    blobs.push(init_elf);
    blobs.extend(service_blobs);

    let count = blobs.len() as u64;
    let mut next_offset = (prima_abi::boot_image::HEADER_QWORDS
        + blobs.len() * prima_abi::boot_image::ENTRY_QWORDS)
        * 8;
    let mut entries = Vec::with_capacity(blobs.len());
    for blob in &blobs {
        next_offset = align_up(next_offset, 8)?;
        entries.push((next_offset, blob.len() as u64));
        next_offset = next_offset
            .checked_add(blob.len())
            .ok_or_else(|| "boot image size overflow".to_owned())?;
    }

    let mut ramdisk = Vec::with_capacity(next_offset);
    push_u64(&mut ramdisk, prima_abi::boot_image::MAGIC);
    push_u64(&mut ramdisk, prima_abi::boot_image::VERSION);
    push_u64(&mut ramdisk, count);
    for (offset, len) in &entries {
        push_u64(&mut ramdisk, *offset as u64);
        push_u64(&mut ramdisk, *len);
    }
    for ((offset, _), blob) in entries.into_iter().zip(blobs) {
        // 头部结束后先按 offset 补零；后续块之间也按 8 字节对齐。
        while ramdisk.len() < offset {
            ramdisk.push(0);
        }
        ramdisk.extend_from_slice(&blob);
    }

    ensure_ramdisk_layout(&ramdisk, next_offset)?;
    fs::write(path, &ramdisk)
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    Ok(())
}

/// EXO-2C：用七块服务 ELF 的布局生成 init 机器码，并封装成两块 4 KiB
/// `PT_LOAD`（vaddr 0 与 0x1000）的不透明 ELF。
///
/// 内核的通用 ELF 解析器只读 program header 与字节；装载 init 代码时按这两个
/// 连续虚拟页选帧，然后映射到 `USER_CODE_VA` / `USER_CODE2_VA`。
fn build_init_elf(service_blobs: &[Vec<u8>]) -> Result<Vec<u8>, String> {
    let (alpha_pages, alpha_entry) = elf_layout(&service_blobs[0])?;
    let (beta_pages, _) = elf_layout(&service_blobs[1])?;
    let (servicemgr_pages, servicemgr_entry) = elf_layout(&service_blobs[2])?;
    let alpha_pages_off = 1;
    let beta_pages_off = alpha_pages_off + alpha_pages;
    let servicemgr_pages_off = beta_pages_off + beta_pages;
    let layout = prima_initgen::BootLayout {
        alpha: prima_initgen::ImageEntry {
            pages: alpha_pages,
            entry: alpha_entry,
        },
        servicemgr: prima_initgen::ImageEntry {
            pages: servicemgr_pages,
            entry: servicemgr_entry,
        },
        alpha_pages_off,
        servicemgr_pages_off,
    };
    let code = prima_initgen::generate(&layout);
    Ok(encode_init_elf(&code))
}

const ELF_HEADER_LEN: usize = 64;
const ELF_PHDR_LEN: usize = 56;
const INIT_CODE_OFFSET: usize = ELF_HEADER_LEN + 2 * ELF_PHDR_LEN;
const INIT_CODE_PAGE: usize = 0x1000;

/// 构造仅供内核通用 ELF 解析器读取的最小 ELF64：两个连续页的 `PT_LOAD`。
fn encode_init_elf(code: &[u8; prima_initgen::USER_CODE_LEN]) -> Vec<u8> {
    let mut elf = vec![0u8; INIT_CODE_OFFSET + code.len()];
    elf[0..4].copy_from_slice(b"\x7fELF");
    elf[4] = 2; // ELFCLASS64
    elf[5] = 1; // ELFDATA2LSB
    elf[6] = 1; // EV_CURRENT
    put_u16(&mut elf, 16, 3); // ET_DYN
    put_u16(&mut elf, 18, 62); // EM_X86_64
    put_u32(&mut elf, 20, 1); // EV_CURRENT
    put_u64(&mut elf, 24, 0); // e_entry: kernel starts at USER_CODE_VA directly
    put_u64(&mut elf, 32, ELF_HEADER_LEN as u64); // e_phoff
    put_u16(&mut elf, 52, ELF_HEADER_LEN as u16); // e_ehsize
    put_u16(&mut elf, 54, ELF_PHDR_LEN as u16); // e_phentsize
    put_u16(&mut elf, 56, 2); // e_phnum
    // phdr 0: page 0.
    write_phdr(&mut elf, ELF_HEADER_LEN, 0, 0, INIT_CODE_PAGE as u64);
    // phdr 1: page 1 (contiguous in the ELF; kernel maps it to USER_CODE2_VA).
    write_phdr(
        &mut elf,
        ELF_HEADER_LEN + ELF_PHDR_LEN,
        INIT_CODE_PAGE as u64,
        INIT_CODE_PAGE as u64,
        2 * INIT_CODE_PAGE as u64,
    );
    elf[INIT_CODE_OFFSET..].copy_from_slice(code);
    elf
}

fn write_phdr(elf: &mut [u8], at: usize, vaddr: u64, file_offset: u64, file_end: u64) {
    put_u32(elf, at, 1); // PT_LOAD
    put_u32(elf, at + 4, 5); // PF_R | PF_X
    put_u64(elf, at + 8, INIT_CODE_OFFSET as u64 + file_offset);
    put_u64(elf, at + 16, vaddr);
    put_u64(elf, at + 24, vaddr);
    put_u64(elf, at + 32, file_end - file_offset);
    put_u64(elf, at + 40, file_end - file_offset);
    put_u64(elf, at + 48, INIT_CODE_PAGE as u64);
}

fn elf_layout(bytes: &[u8]) -> Result<(usize, u64), String> {
    if bytes.len() < ELF_HEADER_LEN
        || bytes.get(0..4) != Some(b"\x7fELF")
        || bytes[4] != 2
        || bytes[5] != 1
        || read_u16(bytes, 16) != Some(3)
        || read_u16(bytes, 18) != Some(62)
        || read_u32(bytes, 20) != Some(1)
        || read_u16(bytes, 54) != Some(ELF_PHDR_LEN as u16)
    {
        return Err("service image is not a supported ELF64".to_owned());
    }
    let entry = read_u64(bytes, 24).ok_or_else(|| "ELF entry is truncated".to_owned())?;
    let phoff =
        usize::try_from(read_u64(bytes, 32).ok_or_else(|| "ELF phoff is truncated".to_owned())?)
            .map_err(|_| "ELF phoff does not fit usize".to_owned())?;
    let phnum = read_u16(bytes, 56).ok_or_else(|| "ELF phnum is truncated".to_owned())? as usize;
    let phbytes = phnum
        .checked_mul(ELF_PHDR_LEN)
        .and_then(|size| phoff.checked_add(size))
        .ok_or_else(|| "ELF program headers overflow".to_owned())?;
    if phnum == 0 || phbytes > bytes.len() {
        return Err("ELF program headers are out of range".to_owned());
    }
    let mut max_end = 0u64;
    let mut entry_loaded = false;
    for index in 0..phnum {
        let at = phoff + index * ELF_PHDR_LEN;
        let kind = read_u32(bytes, at).ok_or_else(|| "ELF phdr is truncated".to_owned())?;
        if kind != 1 {
            continue;
        }
        let vaddr = read_u64(bytes, at + 16).ok_or_else(|| "ELF vaddr is truncated".to_owned())?;
        let filesz =
            read_u64(bytes, at + 32).ok_or_else(|| "ELF filesz is truncated".to_owned())?;
        let memsz = read_u64(bytes, at + 40).ok_or_else(|| "ELF memsz is truncated".to_owned())?;
        if memsz < filesz || memsz == 0 {
            return Err("ELF PT_LOAD has an invalid size".to_owned());
        }
        let end = vaddr
            .checked_add(memsz)
            .ok_or_else(|| "ELF segment end overflows".to_owned())?;
        max_end = max_end.max(end);
        entry_loaded |= entry >= vaddr && entry < end;
    }
    if !entry_loaded || max_end == 0 {
        return Err("ELF entry is not covered by a PT_LOAD".to_owned());
    }
    let pages = usize::try_from(max_end.div_ceil(INIT_CODE_PAGE as u64))
        .map_err(|_| "ELF page count does not fit usize".to_owned())?;
    if pages == 0 {
        return Err("ELF page count is zero".to_owned());
    }
    Ok((pages, entry))
}

fn read_u16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn read_u64(bytes: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(bytes.get(at..at + 8)?.try_into().ok()?))
}

fn put_u16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn ensure_ramdisk_layout(ramdisk: &[u8], expected_len: usize) -> Result<(), String> {
    if ramdisk.len() != expected_len {
        return Err(format!(
            "boot image length mismatch: expected {expected_len}, got {}",
            ramdisk.len()
        ));
    }
    Ok(())
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn align_up(value: usize, align: usize) -> Result<usize, String> {
    let rem = value % align;
    if rem == 0 {
        Ok(value)
    } else {
        value
            .checked_add(align - rem)
            .ok_or_else(|| "boot image alignment overflow".to_owned())
    }
}

pub(crate) fn report_build_success() {
    println!("PASS build kernel");
    println!("PASS image bios");
    println!("PASS image uefi");
}

fn remove_stale_image(path: &PathBuf) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("failed to remove stale {}: {error}", path.display()))?;
    }
    Ok(())
}

fn ensure_non_empty(path: &PathBuf, label: &str) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("{label} missing at {}: {error}", path.display()))?;
    if metadata.len() == 0 {
        return Err(format!("{label} is empty at {}", path.display()));
    }
    Ok(())
}
