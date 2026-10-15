use bootloader::{BiosBoot, UefiBoot};
use std::{
    fs,
    path::{Path, PathBuf},
};

const KERNEL_BINARY: &str = env!("CARGO_BIN_FILE_KERNEL_prima-kernel");
const BIOS_IMAGE_NAME: &str = "prima-os-bios.img";
const UEFI_IMAGE_NAME: &str = "prima-os-uefi.img";
const RAMDISK_IMAGE_NAME: &str = "prima-os-ramdisk.img";

/// EXO-1：boot image 的 7 个 opaque ELF 块。顺序与内核过渡布局一致；名称只在 xtask/
/// 用户态 loader 侧出现，内核只按序号读取。
const BOOT_IMAGE_BINARIES: [&str; 7] = [
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
    let mut blobs = Vec::with_capacity(BOOT_IMAGE_BINARIES.len());
    for binary in BOOT_IMAGE_BINARIES {
        let source = PathBuf::from(binary);
        ensure_non_empty(&source, "boot image binary")?;
        blobs.push(
            fs::read(&source)
                .map_err(|error| format!("failed to read {}: {error}", source.display()))?,
        );
    }

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
