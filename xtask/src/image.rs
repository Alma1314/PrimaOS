use bootloader::{BiosBoot, UefiBoot};
use std::{
    fs,
    path::{Path, PathBuf},
};

const KERNEL_BINARY: &str = env!("CARGO_BIN_FILE_KERNEL_prima-kernel");
const BIOS_IMAGE_NAME: &str = "prima-os-bios.img";
const UEFI_IMAGE_NAME: &str = "prima-os-uefi.img";
const RAMDISK_IMAGE_NAME: &str = "prima-os-ramdisk.img";

/// EXO-2C/2D：ramdisk 中的服务 ELF（init 映像由 `prima-initgen` 生成）。
///
/// 名称只出现在构建工具侧；内核按 opaque `kind`/manifest 顺序读取。这里保持
/// `alpha_index == 1`、`servicemgr_index == 3` 仅作为构建期 init 生成器的索引输入。
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

    // EXO-2D：manifest 条目带 opaque `kind`。内核只按 kind 找 init，其余条目按
    // manifest 顺序做通用页打包/目录；服务语义只在用户态解释。
    let alpha_index = 1;
    let servicemgr_index = 3;
    let init_elf = build_init_elf(alpha_index, servicemgr_index)?;
    let mut manifest: Vec<(u64, Vec<u8>)> = Vec::with_capacity(service_blobs.len() + 1);
    manifest.push((prima_abi::boot_image::KIND_INIT, init_elf));
    manifest.push((
        prima_runtime::protocol::SERVICE_BEHAVIOR_ALPHA,
        service_blobs[0].clone(),
    ));
    manifest.push((
        prima_runtime::protocol::SERVICE_BEHAVIOR_BETA,
        service_blobs[1].clone(),
    ));
    manifest.push((
        prima_runtime::protocol::SERVICE_IMAGE_KIND_SERVICEMGR,
        service_blobs[2].clone(),
    ));
    manifest.push((
        prima_runtime::protocol::SERVICE_BEHAVIOR_LOG,
        service_blobs[3].clone(),
    ));
    manifest.push((
        prima_runtime::protocol::SERVICE_BEHAVIOR_NS,
        service_blobs[4].clone(),
    ));
    manifest.push((
        prima_runtime::protocol::SERVICE_BEHAVIOR_STORE,
        service_blobs[5].clone(),
    ));
    manifest.push((
        prima_runtime::protocol::SERVICE_BEHAVIOR_DRIVER,
        service_blobs[6].clone(),
    ));

    let count = manifest.len() as u64;
    if count as usize > prima_abi::boot_image::MAX_ENTRIES {
        return Err(format!(
            "boot manifest has {count} entries, capacity is {}",
            prima_abi::boot_image::MAX_ENTRIES
        ));
    }

    let mut next_offset = (prima_abi::boot_image::HEADER_QWORDS
        + manifest.len() * prima_abi::boot_image::ENTRY_QWORDS)
        * 8;
    let mut entries = Vec::with_capacity(manifest.len());
    for (kind, blob) in &manifest {
        next_offset = align_up(next_offset, 8)?;
        entries.push((*kind, next_offset, blob.len() as u64));
        next_offset = next_offset
            .checked_add(blob.len())
            .ok_or_else(|| "boot image size overflow".to_owned())?;
    }

    let mut ramdisk = Vec::with_capacity(next_offset);
    push_u64(&mut ramdisk, prima_abi::boot_image::MAGIC);
    push_u64(&mut ramdisk, prima_abi::boot_image::VERSION);
    push_u64(&mut ramdisk, count);
    for (kind, offset, len) in &entries {
        push_u64(&mut ramdisk, *kind);
        push_u64(&mut ramdisk, *offset as u64);
        push_u64(&mut ramdisk, *len);
    }
    for ((_, offset, _), (_, blob)) in entries.into_iter().zip(manifest) {
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

/// EXO-2C/2D：init 代码由 host 生成器产出，并封装为两个连续 `PT_LOAD`
/// （vaddr 0 / 0x1000）的不透明 ELF。布局值改由内核写入的 boot-info 提供。
fn build_init_elf(alpha_index: usize, servicemgr_index: usize) -> Result<Vec<u8>, String> {
    let manifest = prima_initgen::Manifest {
        alpha_index,
        servicemgr_index,
    };
    let code = prima_initgen::generate(&manifest);
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
