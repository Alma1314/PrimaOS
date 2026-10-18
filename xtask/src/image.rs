use bootloader::{BiosBoot, UefiBoot};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const KERNEL_BINARY: &str = env!("CARGO_BIN_FILE_KERNEL_prima-kernel");
const BIOS_IMAGE_NAME: &str = "prima-os-bios.img";
const UEFI_IMAGE_NAME: &str = "prima-os-uefi.img";
const RAMDISK_IMAGE_NAME: &str = "prima-os-ramdisk.img";

/// 用户态服务映像及其 opaque manifest kind；内核只复制 kind，不解释服务角色。
const SERVICE_IMAGES: [(&str, u64); 8] = [
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-smoke"),
        prima_runtime::protocol::SERVICE_BEHAVIOR_ALPHA,
    ),
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-beta"),
        prima_runtime::protocol::SERVICE_BEHAVIOR_BETA,
    ),
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-servicemgr"),
        prima_runtime::protocol::SERVICE_IMAGE_KIND_SERVICEMGR,
    ),
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-log"),
        prima_runtime::protocol::SERVICE_BEHAVIOR_LOG,
    ),
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-ns"),
        prima_runtime::protocol::SERVICE_BEHAVIOR_NS,
    ),
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-store"),
        prima_runtime::protocol::SERVICE_BEHAVIOR_STORE,
    ),
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-driver"),
        prima_runtime::protocol::SERVICE_BEHAVIOR_DRIVER,
    ),
    (
        env!("CARGO_BIN_FILE_PRIMA_USER_prima-tty"),
        prima_runtime::protocol::SERVICE_IMAGE_KIND_TTY,
    ),
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
    let mut manifest = Vec::with_capacity(SERVICE_IMAGES.len() + 1);
    manifest.push((prima_abi::boot_image::KIND_INIT, Vec::new()));
    for (binary, kind) in SERVICE_IMAGES {
        let source = PathBuf::from(binary);
        ensure_non_empty(&source, "boot image binary")?;
        manifest.push((
            kind,
            fs::read(&source)
                .map_err(|error| format!("failed to read {}: {error}", source.display()))?,
        ));
    }
    let source = stage5_probe_source();
    let start = source.with_file_name("stage5_start.S");
    let binary = path.with_file_name("stage5-probe.elf");
    let compiled = Command::new("clang")
        .args([
            "-target",
            "x86_64-unknown-none",
            "-O2",
            "-std=c11",
            "-Werror",
            "-ffreestanding",
            "-fno-stack-protector",
            "-fPIE",
            "-nostdlib",
            "-fuse-ld=lld",
            "-static-pie",
            "-Wl,-e,_start",
            "-Wl,--build-id=none",
        ])
        .arg(&source)
        .arg(&start)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run clang for {}: {error}", source.display()))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build {}: {}",
            source.display(),
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    ensure_non_empty(&binary, "stage5 C probe ELF")?;
    manifest.push((
        prima_runtime::protocol::SERVICE_IMAGE_KIND_C_PROBE,
        fs::read(&binary)
            .map_err(|error| format!("failed to read {}: {error}", binary.display()))?,
    ));

    // EXO-2D：manifest 条目带 opaque `kind`。内核只按 kind 找 init，其余条目按
    // manifest 顺序做通用页打包/目录；服务语义只在用户态解释。
    let alpha_index = manifest_index(&manifest, prima_runtime::protocol::SERVICE_BEHAVIOR_ALPHA)?;
    let servicemgr_index = manifest_index(
        &manifest,
        prima_runtime::protocol::SERVICE_IMAGE_KIND_SERVICEMGR,
    )?;
    manifest[0].1 = build_init_elf(alpha_index, servicemgr_index)?;

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

fn stage5_probe_source() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .join("prima-user/c/stage5_probe.c")
}

/// 同一 C 源码在 Linux 宿主上的 main 返回码参考：成功及两个输入拒绝。
///
/// 片 C：另断言同源探针在 Linux 上 `write(1)`/`write(2)` 各输出一个字节——与 Prima QEMU
/// 侧 `CIO|` 见证（同一 `main` 路径写的 'h'/'i'）构成同源差分。
pub(crate) fn check_stage5_linux_probe() -> Result<(), String> {
    let source = stage5_probe_source();
    let binary = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .join("target/prima-os/stage5-probe-linux");
    let compiled = Command::new("clang")
        .args(["-O2", "-std=c11", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux clang: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux C probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    for (argument, environment, expected) in [(true, true, 37), (false, true, 2), (true, false, 3)]
    {
        let mut command = Command::new(&binary);
        command.env_clear();
        if argument {
            command.arg("stack-ready");
        }
        if environment {
            command.env("PRIMA_ABI", "1");
        }
        let output = command
            .output()
            .map_err(|error| format!("failed to run Linux C probe: {error}"))?;
        if output.status.code() != Some(expected) {
            return Err(format!(
                "Linux C probe exit mismatch: expected {expected}, got {:?}",
                output.status.code()
            ));
        }
        // 成功路径与 Prima 同源：fd 1 写 'h'、fd 2 写 'i'。
        if expected == 37 && (output.stdout != b"h" || output.stderr != b"i") {
            return Err(format!(
                "Linux C probe write mismatch: stdout {:?}, stderr {:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    println!("PASS linux stage5 C main exits 37/2/3 and writes h>i via write(1)/write(2)");
    Ok(())
}

fn manifest_index(manifest: &[(u64, Vec<u8>)], kind: u64) -> Result<usize, String> {
    manifest
        .iter()
        .position(|entry| entry.0 == kind)
        .ok_or_else(|| format!("boot manifest is missing kind {kind:#x}"))
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

#[cfg(test)]
mod tests {
    use super::manifest_index;

    #[test]
    fn manifest_index_follows_entry_order_and_rejects_missing_kind() {
        let entries = vec![(0x100, Vec::new()), (1, Vec::new()), (2, Vec::new())];
        assert_eq!(manifest_index(&entries, 1), Ok(1));
        assert_eq!(manifest_index(&entries, 0x100), Ok(0));
        assert!(manifest_index(&entries, 3).is_err());
    }
}
