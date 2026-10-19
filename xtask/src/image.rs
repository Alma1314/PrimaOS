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

/// M5 冻结用例（profile v0 §5）：argv 不含程序名以外的解释，stdout 逐字节冻结。
pub(crate) struct EchoCase {
    pub(crate) name: &'static str,
    pub(crate) args: &'static [&'static str],
    pub(crate) stdout: &'static [u8],
}

pub(crate) const ECHO_CASES: [EchoCase; 6] = [
    EchoCase {
        name: "E1",
        args: &["echo"],
        stdout: b"\n",
    },
    EchoCase {
        name: "E2",
        args: &["echo", "-n"],
        stdout: b"",
    },
    EchoCase {
        name: "E3",
        args: &["echo", "hello"],
        stdout: b"hello\n",
    },
    EchoCase {
        name: "E4",
        args: &["echo", "-n", "hello"],
        stdout: b"hello",
    },
    EchoCase {
        name: "E5",
        args: &["echo", "a", "b", "c"],
        stdout: b"a b c\n",
    },
    EchoCase {
        name: "E6",
        args: &["echo", "--", "-n"],
        stdout: b"-- -n\n",
    },
];

pub(crate) fn build_images() -> Result<Images, String> {
    build_images_with_echo(None)
}

/// M5：用 sbase `echo` 替换 C 探针映像，并打包本次用例的 argv。
pub(crate) fn build_images_for_echo(args: &[&str]) -> Result<Images, String> {
    build_images_with_echo(Some(args))
}

fn build_images_with_echo(echo_args: Option<&[&str]>) -> Result<Images, String> {
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
    build_ramdisk(&ramdisk, echo_args)?;

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
fn build_ramdisk(path: &Path, echo_args: Option<&[&str]>) -> Result<(), String> {
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
    // M3：从锁定快照构建静态 musl 基座（脚本按补丁/快照哈希缓存）。
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let musl_script = workspace.join("prima-user").join("musl").join("build.sh");
    let musl_status = Command::new("sh")
        .arg(&musl_script)
        .status()
        .map_err(|error| format!("failed to run {}: {error}", musl_script.display()))?;
    if !musl_status.success() {
        return Err(format!(
            "failed to build Prima musl sysroot: {}",
            musl_script.display()
        ));
    }
    let musl_prefix = workspace.join("target").join("musl-prefix");

    let c_dir = workspace.join("prima-user").join("c");
    let crt = c_dir.join("prima_crt.c");
    let start = c_dir.join("prima_crt_start.S");
    let adapter = c_dir.join("prima_syscall.c");
    let (binary, sources, label) = match echo_args {
        Some(_) => {
            let sbase = workspace.join("target").join("sbase");
            ensure_sbase(&sbase)?;
            (
                path.with_file_name("echo.elf"),
                vec![
                    sbase.join("echo.c"),
                    sbase.join("libutil").join("putword.c"),
                    sbase.join("libutil").join("fshut.c"),
                    sbase.join("libutil").join("eprintf.c"),
                    c_dir.join("echo_harness.c"),
                ],
                "sbase echo",
            )
        }
        None => (
            path.with_file_name("stage5-probe.elf"),
            vec![stage5_probe_source()],
            "stage5 C probe",
        ),
    };
    let mut clang = Command::new("clang");
    clang.args([
        "-target",
        "x86_64-unknown-none",
        "-O2",
        "-std=c11",
        "-Werror",
        "-ffreestanding",
        "-fno-stack-protector",
        // musl 构建：跳过会覆盖 musl TLS 基址的 M2 FS 实验。
        "-DPRIMA_WITH_LIBC",
        // 探针/适配层 TU 仍只用通用寄存器：SSE 路径由 libc.a 提供，这里保持
        // M2 的 XMM 往返断言不被编译器自身的 XMM 使用干扰。
        "-mgeneral-regs-only",
        "-fPIC",
        "-nostdlib",
        "-fuse-ld=lld",
        "-static-pie",
        "-Wl,-e,_start",
        "-Wl,--build-id=none",
        "-Wl,--gc-sections",
        "-nostdinc",
    ]);
    clang.arg("-isystem").arg(musl_prefix.join("include"));
    if echo_args.is_some() {
        clang
            .arg("-DPRIMA_ECHO_HARNESS")
            .arg("-I")
            .arg(workspace.join("target").join("sbase"))
            // `echo_harness.c` 引用 c/ 下的 `prima_abi.h`（同目录已可找到，显式补一次）。
            .arg("-I")
            .arg(&c_dir);
    }
    for source in &sources {
        clang.arg(source);
    }
    clang
        .arg(&crt)
        .arg(&start)
        .arg(&adapter)
        .arg("-L")
        .arg(musl_prefix.join("lib"))
        .arg("-lc")
        .arg("-lgcc")
        .arg("-o")
        .arg(&binary);
    let compiled = clang
        .output()
        .map_err(|error| format!("failed to run clang for {label}: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build {label}: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    ensure_non_empty(&binary, "stage5 C ELF")?;
    let c_image = fs::read(&binary)
        .map_err(|error| format!("failed to read {}: {error}", binary.display()))?;
    validate_static_pie(&c_image)?;
    manifest.push((prima_runtime::protocol::SERVICE_IMAGE_KIND_C_PROBE, c_image));
    if let Some(args) = echo_args {
        manifest.push((
            prima_runtime::protocol::SERVICE_IMAGE_KIND_ECHO_ARGS,
            pack_echo_args(args)?,
        ));
    }

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

/// sbase 快照只落在 git-ignored 的 `target/`；按 profile v0 冻结的 commit 校验身份。
fn ensure_sbase(sbase: &Path) -> Result<(), String> {
    if !sbase.join("echo.c").is_file() {
        return Err(format!(
            "sbase snapshot missing: {} (restore commit c546c3a5724c81cee9a11d816a38ccdf17472129)",
            sbase.display()
        ));
    }
    let output = Command::new("git")
        .args(["-C"])
        .arg(sbase)
        .args(["rev-parse", "HEAD"])
        .output()
        .map_err(|error| format!("failed to verify sbase snapshot: {error}"))?;
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if !output.status.success() || commit != "c546c3a5724c81cee9a11d816a38ccdf17472129" {
        return Err(format!(
            "sbase snapshot commit mismatch: expected c546c3a5724c81cee9a11d816a38ccdf17472129, got {commit:?}"
        ));
    }
    Ok(())
}

/// argv 参数块：`[count][len, bytes]...`。内核把每个 manifest 条目都当 ELF 解析，
/// 故参数块放进一个单页 ET_DYN（一个覆盖全页的 `PT_LOAD`）的固定偏移之后。
fn pack_echo_args(args: &[&str]) -> Result<Vec<u8>, String> {
    if args.is_empty() || args.len() > 8 {
        return Err(format!("echo argv count {} is outside 1..=8", args.len()));
    }
    let mut payload = Vec::new();
    payload.push(args.len() as u8);
    for arg in args {
        if arg.len() > 127 {
            return Err(format!("echo argument too long: {} bytes", arg.len()));
        }
        payload.push(arg.len() as u8);
        payload.extend_from_slice(arg.as_bytes());
    }
    let offset = prima_runtime::protocol::SERVICE_ECHO_ARGS_OFFSET as usize;
    let total = offset + payload.len();
    if total > 0x1000 {
        return Err("echo argv block exceeds one page".to_owned());
    }
    let mut image = vec![0u8; total];
    image[0..4].copy_from_slice(b"\x7fELF");
    image[4] = 2; // ELF64
    image[5] = 1; // little-endian
    image[6] = 1;
    image[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN
    image[18..20].copy_from_slice(&62u16.to_le_bytes()); // x86_64
    image[20..24].copy_from_slice(&1u32.to_le_bytes());
    image[32..40].copy_from_slice(&64u64.to_le_bytes()); // e_phoff
    image[52..54].copy_from_slice(&64u16.to_le_bytes()); // e_ehsize
    image[54..56].copy_from_slice(&56u16.to_le_bytes()); // e_phentsize
    image[56..58].copy_from_slice(&1u16.to_le_bytes()); // e_phnum
    let ph = 64;
    image[ph..ph + 4].copy_from_slice(&1u32.to_le_bytes()); // PT_LOAD
    image[ph + 4..ph + 8].copy_from_slice(&5u32.to_le_bytes()); // R|X
    image[ph + 32..ph + 40].copy_from_slice(&(total as u64).to_le_bytes()); // p_filesz
    image[ph + 40..ph + 48].copy_from_slice(&(total as u64).to_le_bytes()); // p_memsz
    image[ph + 48..ph + 56].copy_from_slice(&0x1000u64.to_le_bytes()); // p_align
    image[offset..].copy_from_slice(&payload);
    Ok(image)
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
    // M4：文件探针在 cwd 下创建 "f"。用一次性目录，避免污染仓库或并行运行互相干扰。
    let probe_cwd = binary
        .parent()
        .expect("probe binary has a parent directory")
        .join("linux-probe-cwd");
    fs::create_dir_all(&probe_cwd)
        .map_err(|error| format!("failed to create {}: {error}", probe_cwd.display()))?;
    let _ = fs::remove_file(probe_cwd.join("f"));
    for (argument, environment, expected) in [(true, true, 37), (false, true, 2), (true, false, 3)]
    {
        let mut command = Command::new(&binary);
        command.env_clear().current_dir(&probe_cwd);
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
        // 成功路径与 Prima 同源：fd 1 写 'h' 与 5 字节 "\nxyz\n"，fd 2 写 'i'，
        // 最后 printf+fflush 输出 "M4STDIO\n"（stdio → writev）。
        if expected == 37 && (output.stdout != b"h\nxyz\nM4STDIO\n" || output.stderr != b"i") {
            return Err(format!(
                "Linux C probe write mismatch: stdout {:?}, stderr {:?}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    println!(
        "PASS linux stage5 C main exits 37/2/3, streams h/i plus xyz, passes file API and stdio probes"
    );
    Ok(())
}

/// M5：用冻结的编译命令在 Linux 宿主重建 sbase `echo`，并在同一份 E1–E6 输入上取得
/// stdout/退出码基线。Prima 侧必须逐字节一致。
pub(crate) fn check_stage5_linux_echo() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let sbase = workspace.join("target").join("sbase");
    ensure_sbase(&sbase)?;
    let binary = workspace.join("target/prima-os/echo-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c99"])
        .arg(sbase.join("echo.c"))
        .arg(sbase.join("libutil").join("putword.c"))
        .arg(sbase.join("libutil").join("fshut.c"))
        .arg(sbase.join("libutil").join("eprintf.c"))
        .arg("-I")
        .arg(&sbase)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for sbase echo: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux sbase echo: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    for case in &ECHO_CASES {
        let output = Command::new(&binary)
            .env_clear()
            .args(&case.args[1..])
            .output()
            .map_err(|error| format!("failed to run Linux echo {}: {error}", case.name))?;
        if output.status.code() != Some(0) || output.stdout != case.stdout {
            return Err(format!(
                "Linux echo {} mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
                case.name,
                output.status.code(),
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(case.stdout)
            ));
        }
    }
    println!("PASS linux sbase echo E1-E6 stdout/exit references");
    Ok(())
}

/// M2/T-MUSL-03：Prima crt 依赖内核已完成 RELATIVE 重定位，因此产物必须是**静态 PIE**：
/// `ET_DYN`、无 `PT_INTERP`、无非 `R_X86_64_RELATIVE` 的重定位、无 `DT_RELR`。
///
/// 只解析 ELF64 头/程序头/节头；解析不了的结构即确定性失败，不把「没看到」当成通过。
fn validate_static_pie(elf: &[u8]) -> Result<(), String> {
    if elf.len() < 64 || &elf[0..4] != b"\x7fELF" || elf[4] != 2 || elf[5] != 1 {
        return Err("C probe is not an ELF64 little-endian file".to_owned());
    }
    let e_type = read_u16(elf, 16)?;
    if e_type != 3 {
        return Err(format!("C probe e_type is {e_type}, expected ET_DYN (3)"));
    }
    let phoff = read_u64(elf, 32)? as usize;
    let phentsize = read_u16(elf, 54)? as usize;
    let phnum = read_u16(elf, 56)? as usize;
    for index in 0..phnum {
        let at = phoff
            .checked_add(index * phentsize)
            .ok_or_else(|| "program header offset overflow".to_owned())?;
        if read_u32(elf, at)? == 3 {
            return Err("C probe has PT_INTERP; dynamic linking is out of scope".to_owned());
        }
    }
    let shoff = read_u64(elf, 40)? as usize;
    let shentsize = read_u16(elf, 58)? as usize;
    let shnum = read_u16(elf, 60)? as usize;
    if shnum == 0 || shentsize < 64 {
        return Err("C probe has no usable section headers".to_owned());
    }
    for index in 0..shnum {
        let at = shoff
            .checked_add(index * shentsize)
            .ok_or_else(|| "section header offset overflow".to_owned())?;
        let sh_type = read_u32(elf, at + 4)?;
        let sh_offset = read_u64(elf, at + 24)? as usize;
        let sh_size = read_u64(elf, at + 32)? as usize;
        let sh_entsize = read_u64(elf, at + 56)? as usize;
        match sh_type {
            4 => {
                // SHT_RELA：每个条目 24 字节，r_info 低 32 位是类型。
                let entsize = if sh_entsize == 0 { 24 } else { sh_entsize };
                if entsize < 24 || !sh_size.is_multiple_of(entsize) {
                    return Err("malformed .rela section".to_owned());
                }
                for entry in 0..sh_size / entsize {
                    let r_info = read_u64(elf, sh_offset + entry * entsize + 8)?;
                    let kind = (r_info & 0xffff_ffff) as u32;
                    if kind != 8 {
                        return Err(format!(
                            "C probe relocation type {kind}; only R_X86_64_RELATIVE (8) is supported"
                        ));
                    }
                }
            }
            9 => return Err("C probe has SHT_REL; only RELA is accepted".to_owned()),
            19 => return Err("C probe has SHT_RELR; DT_RELR is not supported".to_owned()),
            _ => {}
        }
    }
    println!("PASS stage5 C probe is static PIE with R_X86_64_RELATIVE only");
    Ok(())
}

fn read_u16(bytes: &[u8], at: usize) -> Result<u16, String> {
    let slice = bytes
        .get(at..at + 2)
        .ok_or_else(|| format!("ELF read out of bounds at {at}"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    let slice = bytes
        .get(at..at + 4)
        .ok_or_else(|| format!("ELF read out of bounds at {at}"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    let slice = bytes
        .get(at..at + 8)
        .ok_or_else(|| format!("ELF read out of bounds at {at}"))?;
    let mut word = [0u8; 8];
    word.copy_from_slice(slice);
    Ok(u64::from_le_bytes(word))
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
    use super::*;

    fn synthetic_static_pie(relocation_type: u32, section_type: u32) -> Vec<u8> {
        // 64-byte header + 2 × 64-byte section headers + one 24-byte RELA entry.
        let mut elf = vec![0u8; 64 + 128 + 24];
        elf[0..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        put_u16(&mut elf, 16, 3); // ET_DYN
        put_u64(&mut elf, 40, 64); // e_shoff
        put_u16(&mut elf, 58, 64); // e_shentsize
        put_u16(&mut elf, 60, 2); // e_shnum
        // Section 1 (at 128): relocation section.
        put_u32(&mut elf, 128 + 4, section_type);
        put_u64(&mut elf, 128 + 24, 192); // sh_offset
        put_u64(&mut elf, 128 + 32, 24); // sh_size
        put_u64(&mut elf, 128 + 56, 24); // sh_entsize
        // Relocation entry at 192: r_offset, r_info.
        put_u64(&mut elf, 192 + 8, relocation_type as u64);
        elf
    }

    #[test]
    fn static_pie_accepts_only_relative_rela() {
        assert_eq!(validate_static_pie(&synthetic_static_pie(8, 4)), Ok(()));
        let wrong_type = validate_static_pie(&synthetic_static_pie(4, 4)).unwrap_err();
        assert!(wrong_type.contains("relocation type 4"));
        let relr = validate_static_pie(&synthetic_static_pie(8, 19)).unwrap_err();
        assert!(relr.contains("SHT_RELR"));
        let rel = validate_static_pie(&synthetic_static_pie(8, 9)).unwrap_err();
        assert!(rel.contains("SHT_REL"));
    }

    #[test]
    fn static_pie_rejects_non_dyn_and_missing_sections() {
        let mut exec = synthetic_static_pie(8, 4);
        put_u16(&mut exec, 16, 2); // ET_EXEC
        assert!(validate_static_pie(&exec).unwrap_err().contains("ET_DYN"));
        let mut no_sections = synthetic_static_pie(8, 4);
        put_u16(&mut no_sections, 60, 0);
        assert!(
            validate_static_pie(&no_sections)
                .unwrap_err()
                .contains("section headers")
        );
    }

    #[test]
    fn manifest_index_follows_entry_order_and_rejects_missing_kind() {
        let entries = vec![(0x100, Vec::new()), (1, Vec::new()), (2, Vec::new())];
        assert_eq!(manifest_index(&entries, 1), Ok(1));
        assert_eq!(manifest_index(&entries, 0x100), Ok(0));
        assert!(manifest_index(&entries, 3).is_err());
    }
}
