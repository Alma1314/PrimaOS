use bootloader::{BiosBoot, UefiBoot};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
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

/// 本次启动里替换 C 探针映像的托管程序。
enum Stage5Program<'a> {
    /// 既有固定 C 探针（argv 冻结在服务管理器里）。
    Probe,
    /// M5：sbase `echo` 冻结用例，argv 由单页参数块传入。
    Echo(&'a [&'a str]),
    /// M6：POSIX 时间探针（无 argv 语义，走同一托管程序通道）。
    Time,
    /// M7：fd 表 / dup / getpid 探针；自行执行两次 open，故不预置 harness。
    Fd,
    /// M8：用户态 VFS/rootfs、通用 fd、多级路径与目录枚举探针。
    Vfs,
    /// M9：statx / chdir / getcwd 元数据与工作目录探针。
    Meta,
    /// M10：匿名 mmap/munmap/mprotect 与原生 memseg_protect 探针。
    Mem,
    /// M11：mkdir/rmdir/unlink（含 *at）文件树变更探针。
    Mut,
    /// M13a：符号链接/readlink/rename/fchdir 探针。
    Vfs2,
    /// M13b：chmod/fchmod/umask 与 stdio 文件 API 探针。
    Vfs3,
    /// M19：用户态 loopback IPv4/UDP 探针。
    Net,
    /// M14：fd 0 tty 输入（canonical/raw/EOF/termios）探针。
    Ttyin,
    /// M15：WaitObject 原生机制探针。
    Wait,
    /// M15b-2：共享地址空间线程唤醒 WaitObject 探针。
    Wait2,
    /// M16a：futex WAIT/WAKE 探针。
    Futex,
    Pthread,
    Share,
    /// M9：sbase `cat`（文件读取）冻结用例。
    Cat(&'a [&'a str]),
    /// M9：sbase `ls`（目录枚举 + statx + cwd）。
    Ls(&'a [&'a str]),
}

pub(crate) fn build_images() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Probe)
}

/// M5：用 sbase `echo` 替换 C 探针映像，并打包本次用例的 argv。
pub(crate) fn build_images_for_echo(args: &[&str]) -> Result<Images, String> {
    build_images_with_program(Stage5Program::Echo(args))
}

/// M6：用时间探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_time() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Time)
}

/// M7：用 fd 表探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_fd() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Fd)
}

/// M8：用 VFS/rootfs 探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_vfs() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Vfs)
}

/// M9：用 statx / chdir / getcwd 探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_meta() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Meta)
}

/// M10：用匿名内存/保护探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_mem() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Mem)
}

/// M11：用文件树变更探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_mut() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Mut)
}

/// M13a：用符号链接/rename/fchdir 探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_vfs2() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Vfs2)
}

/// M13b：用 chmod/umask/stdio 探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_vfs3() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Vfs3)
}

/// M19：用 loopback UDP 探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_net() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Net)
}

/// M14：用 fd 0 tty 输入探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_ttyin() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Ttyin)
}

/// M15：用 WaitObject 探针替换 C 探针映像并跑同一双轮验证。
pub(crate) fn build_images_for_wait() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Wait)
}

/// M15b-2：用共享线程唤醒 WaitObject 探针替换 C 探针映像。
pub(crate) fn build_images_for_wait2() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Wait2)
}

/// M16a：用 futex 探针替换 C 探针映像。
pub(crate) fn build_images_for_futex() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Futex)
}

pub(crate) fn build_images_for_pthread() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Pthread)
}

pub(crate) fn build_images_for_share() -> Result<Images, String> {
    build_images_with_program(Stage5Program::Share)
}

/// M9：用 sbase `cat` 替换 C 探针映像并打包本次用例的 argv。
pub(crate) fn build_images_for_cat(args: &[&str]) -> Result<Images, String> {
    build_images_with_program(Stage5Program::Cat(args))
}

/// M9：用 sbase `ls` 替换 C 探针映像并打包本次用例的 argv。
pub(crate) fn build_images_for_ls(args: &[&str]) -> Result<Images, String> {
    build_images_with_program(Stage5Program::Ls(args))
}

fn build_images_with_program(program: Stage5Program<'_>) -> Result<Images, String> {
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
    build_ramdisk(&ramdisk, program)?;

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
fn build_ramdisk(path: &Path, program: Stage5Program<'_>) -> Result<(), String> {
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
    let sbase = workspace.join("target").join("sbase");
    let (binary, sources, label) = match &program {
        Stage5Program::Echo(_) => {
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
        Stage5Program::Time => (
            path.with_file_name("stage5-time.elf"),
            vec![c_dir.join("stage5_time.c"), c_dir.join("echo_harness.c")],
            "stage5 time probe",
        ),
        Stage5Program::Fd => (
            path.with_file_name("stage5-fd.elf"),
            vec![c_dir.join("stage5_fd.c")],
            "stage5 fd probe",
        ),
        Stage5Program::Vfs => (
            path.with_file_name("stage5-vfs.elf"),
            vec![c_dir.join("stage5_vfs.c")],
            "stage5 vfs probe",
        ),
        Stage5Program::Meta => (
            path.with_file_name("stage5-meta.elf"),
            vec![c_dir.join("stage5_meta.c")],
            "stage5 meta probe",
        ),
        Stage5Program::Mem => (
            path.with_file_name("stage5-mem.elf"),
            vec![c_dir.join("stage5_mem.c")],
            "stage5 mem probe",
        ),
        Stage5Program::Mut => (
            path.with_file_name("stage5-mut.elf"),
            vec![c_dir.join("stage5_mut.c")],
            "stage5 mut probe",
        ),
        Stage5Program::Vfs2 => (
            path.with_file_name("stage5-vfs2.elf"),
            vec![c_dir.join("stage5_vfs2.c")],
            "stage5 vfs2 probe",
        ),
        Stage5Program::Vfs3 => (
            path.with_file_name("stage5-vfs3.elf"),
            vec![c_dir.join("stage5_vfs3.c")],
            "stage5 vfs3 probe",
        ),
        Stage5Program::Net => (
            path.with_file_name("stage5-net.elf"),
            vec![c_dir.join("stage5_net.c")],
            "stage5 net probe",
        ),
        Stage5Program::Ttyin => (
            path.with_file_name("stage5-ttyin.elf"),
            vec![c_dir.join("stage5_ttyin.c")],
            "stage5 ttyin probe",
        ),
        Stage5Program::Wait => (
            path.with_file_name("stage5-wait.elf"),
            vec![c_dir.join("stage5_wait.c")],
            "stage5 wait probe",
        ),
        Stage5Program::Wait2 => (
            path.with_file_name("stage5-wait2.elf"),
            vec![c_dir.join("stage5_wait2.c")],
            "stage5 wait2 probe",
        ),
        Stage5Program::Futex => (
            path.with_file_name("stage5-futex.elf"),
            vec![c_dir.join("stage5_futex.c")],
            "stage5 futex probe",
        ),
        Stage5Program::Pthread => (
            path.with_file_name("stage5-pthread.elf"),
            vec![c_dir.join("stage5_pthread.c")],
            "stage5 pthread probe",
        ),
        Stage5Program::Share => (
            path.with_file_name("stage5-share.elf"),
            vec![c_dir.join("stage5_share.c")],
            "stage5 share probe",
        ),
        Stage5Program::Cat(_) => {
            ensure_sbase(&sbase)?;
            (
                path.with_file_name("cat.elf"),
                vec![
                    sbase.join("cat.c"),
                    sbase.join("libutil").join("concat.c"),
                    sbase.join("libutil").join("writeall.c"),
                    sbase.join("libutil").join("eprintf.c"),
                    sbase.join("libutil").join("fshut.c"),
                ],
                "sbase cat",
            )
        }
        Stage5Program::Ls(_) => {
            ensure_sbase(&sbase)?;
            (
                path.with_file_name("ls.elf"),
                vec![
                    sbase.join("ls.c"),
                    sbase.join("libutil").join("ealloc.c"),
                    sbase.join("libutil").join("eprintf.c"),
                    sbase.join("libutil").join("fshut.c"),
                    sbase.join("libutil").join("reallocarray.c"),
                    sbase.join("libutil").join("human.c"),
                    sbase.join("libutf").join("rune.c"),
                    sbase.join("libutf").join("runetype.c"),
                    sbase.join("libutf").join("utf.c"),
                    sbase.join("libutf").join("isprintrune.c"),
                    sbase.join("libutf").join("iscntrlrune.c"),
                ],
                "sbase ls",
            )
        }
        Stage5Program::Probe => (
            path.with_file_name("stage5-probe.elf"),
            vec![stage5_probe_source()],
            "stage5 C probe",
        ),
    };
    let uses_sbase = matches!(
        program,
        Stage5Program::Echo(_) | Stage5Program::Cat(_) | Stage5Program::Ls(_)
    );
    // `ls` 的 human.c 需要 double 转换；x86_64 libgcc 不提供 __floatdidf，故该程序
    // 允许编译器使用 SSE（M3 已启用 ring3 SSE 与 per-task FXSAVE/FXRSTOR）。
    let allow_sse = matches!(program, Stage5Program::Ls(_));
    let mut clang = Command::new("clang");
    let mut flags = vec![
        "-target",
        "x86_64-unknown-none",
        "-O2",
        "-std=c11",
        "-Werror",
        "-ffreestanding",
        "-fno-stack-protector",
        // musl 构建：跳过会覆盖 musl TLS 基址的 M2 FS 实验。
        "-DPRIMA_WITH_LIBC",
        "-fPIC",
        "-nostdlib",
        "-fuse-ld=lld",
        "-static-pie",
        "-Wl,-e,_start",
        "-Wl,--build-id=none",
        "-Wl,--gc-sections",
        "-nostdinc",
    ];
    if !allow_sse {
        // 探针/适配层 TU 只用通用寄存器：SSE 路径由 libc.a 提供，这里保持 M2 的
        // XMM 往返断言不被编译器自身的 XMM 使用干扰。
        flags.push("-mgeneral-regs-only");
    }
    clang.args(&flags);
    clang.arg("-isystem").arg(musl_prefix.join("include"));
    if uses_sbase {
        // 与 sbase Makefile 的 CPPFLAGS 对齐（PATH_MAX、strdup、_FILE_OFFSET_BITS）。
        clang.args([
            "-DPREFIX=\"/usr/local\"",
            "-D_DEFAULT_SOURCE",
            "-D_NETBSD_SOURCE",
            "-D_BSD_SOURCE",
            "-D_XOPEN_SOURCE=700",
            "-D_FILE_OFFSET_BITS=64",
        ]);
        clang.arg("-I").arg(&sbase);
    }
    // M5/M6 共用的前置文件授权冒烟：保持 NsWitness 的两次 open 解析计数。
    // M7/M8/M9 的探针自行执行业务 open，不再叠加 harness。
    if matches!(program, Stage5Program::Echo(_) | Stage5Program::Time) {
        clang.arg("-DPRIMA_ECHO_HARNESS").arg("-I").arg(&c_dir);
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
    let program_args: Option<(&[&str], &[&str])> = match &program {
        Stage5Program::Echo(args) => Some((args, &[])),
        // 时间/fd/vfs/meta 探针不需要参数；托管程序通道仍要求一个非空 argv。
        Stage5Program::Time => Some((&["stage5-time"], &[])),
        Stage5Program::Fd => Some((&["stage5-fd"], &[])),
        Stage5Program::Vfs => Some((&["stage5-vfs"], &[])),
        Stage5Program::Meta => Some((&["stage5-meta"], &[])),
        Stage5Program::Mem => Some((&["stage5-mem"], &[])),
        Stage5Program::Mut => Some((&["stage5-mut"], &[])),
        Stage5Program::Vfs2 => Some((&["stage5-vfs2"], &[])),
        Stage5Program::Vfs3 => Some((&["stage5-vfs3"], &[])),
        Stage5Program::Net => Some((&["stage5-net"], &[])),
        Stage5Program::Ttyin => Some((&["stage5-ttyin"], &[])),
        Stage5Program::Wait => Some((&["stage5-wait"], &[])),
        Stage5Program::Wait2 => Some((&["stage5-wait2"], &[])),
        Stage5Program::Futex => Some((&["stage5-futex"], &[])),
        Stage5Program::Pthread => Some((&["stage5-pthread"], &[])),
        Stage5Program::Share => Some((&["stage5-share"], &[])),
        Stage5Program::Cat(args) => Some((args, &[])),
        Stage5Program::Ls(args) => Some((args, &[])),
        Stage5Program::Probe => None,
    };
    if let Some((args, env)) = program_args {
        manifest.push((
            prima_runtime::protocol::SERVICE_IMAGE_KIND_ECHO_ARGS,
            pack_program_args(args, env)?,
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

/// 参数块冻结上限（M12）：数量与单串长度由 u8 长度字节约束，总量受单页参数载体约束。
const MAX_PROGRAM_ARGS: usize = 16;
const MAX_PROGRAM_ENV: usize = 8;

/// argv/envp 参数块。内核把每个 manifest 条目都当 ELF 解析，故参数块放进一个单页
/// ET_DYN（一个覆盖全页的 `PT_LOAD`）的固定偏移之后。
///
/// 无 envp 时沿用 M5 的冻结格式 `[argc][len, bytes]...`（envp 由管理器写死
/// `PRIMA_ABI=1`）；带 envp 时使用版本化格式
/// `[0xa5][2][argc][len, bytes]... [envc][len, bytes]...`。两种格式都只含 u8
/// 长度；`0xa5` 不可能与合法 `argc` 混淆（上限 16）。
fn pack_program_args(args: &[&str], env: &[&str]) -> Result<Vec<u8>, String> {
    if args.is_empty() || args.len() > MAX_PROGRAM_ARGS {
        return Err(format!(
            "program argv count {} is outside 1..={MAX_PROGRAM_ARGS}",
            args.len()
        ));
    }
    if env.len() > MAX_PROGRAM_ENV {
        return Err(format!(
            "program envp count {} exceeds {MAX_PROGRAM_ENV}",
            env.len()
        ));
    }
    let mut env_strings: Vec<&str> = env.to_vec();
    if !env_strings.is_empty()
        && !env_strings
            .iter()
            .any(|item| item.starts_with("PRIMA_ABI="))
    {
        if env_strings.len() == MAX_PROGRAM_ENV {
            return Err(format!(
                "program envp is full ({MAX_PROGRAM_ENV}) and lacks PRIMA_ABI"
            ));
        }
        env_strings.push("PRIMA_ABI=1");
    }
    let mut payload = Vec::new();
    if env.is_empty() {
        payload.push(args.len() as u8);
    } else {
        payload.push(0xa5);
        payload.push(2);
        payload.push(args.len() as u8);
    }
    for arg in args {
        if arg.len() > 255 {
            return Err(format!("program argument too long: {} bytes", arg.len()));
        }
        payload.push(arg.len() as u8);
        payload.extend_from_slice(arg.as_bytes());
    }
    if !env.is_empty() {
        payload.push(env_strings.len() as u8);
        for item in &env_strings {
            if item.len() > 255 {
                return Err(format!(
                    "program environment too long: {} bytes",
                    item.len()
                ));
            }
            payload.push(item.len() as u8);
            payload.extend_from_slice(item.as_bytes());
        }
    }
    let offset = prima_runtime::protocol::SERVICE_ECHO_ARGS_OFFSET as usize;
    let total = offset + payload.len();
    if total > 0x1000 {
        return Err("program argv/envp block exceeds one page".to_owned());
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
            command
                .env("PRIMA_ABI", "1")
                .env("M12_ENV", "one")
                .env("M12_EMPTY", "");
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

/// M6 冻结的 Linux 参考输出：同一份 `stage5_time.c` 在宿主上必须全项通过。
pub(crate) const TIME_EXPECTED_STDOUT: &[u8] =
    b"TIME|ok=1;clock=1;nsec=1;mono=1;badclk=1;badptr=1;badnsec=1;zero=1;rel=1;abs=1;past=1\n";

/// M6：用宿主 glibc 重建 `stage5_time.c`，取得 clock_gettime/nanosleep 的成功、错误与
/// 单调性基线。Prima 侧必须逐字节一致并以退出码 0 结束。
pub(crate) fn check_stage5_linux_time() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_time.c");
    let binary = workspace.join("target/prima-os/time-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 time: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 time probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let output = Command::new(&binary)
        .env_clear()
        .output()
        .map_err(|error| format!("failed to run Linux stage5 time probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != TIME_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 time mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(TIME_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 time clock/nanosleep references");
    Ok(())
}

/// M7 冻结的 Linux 参考输出：同一份 `stage5_fd.c` 在宿主上必须全项通过。
pub(crate) const FD_EXPECTED_STDOUT: &[u8] =
    b"FD|ok=1;pid=1;ppid=1;open=1;dup=1;share=1;dup2=1;dup3=1;close=1;self=1;reuse=1;bad=1;cloexec=1;held=1\n";

/// M7：用宿主 glibc 重建 `stage5_fd.c`，取得 dup/dup2/dup3/getpid 的参考结果。
/// 在一次性 cwd 中运行，使探针的路径 `f` 与 Prima 的单文件服务等价。
pub(crate) fn check_stage5_linux_fd() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_fd.c");
    let binary = workspace.join("target/prima-os/fd-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 fd: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 fd probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let run_dir = workspace.join("target/prima-os/fd-linux-run");
    fs::create_dir_all(&run_dir)
        .map_err(|error| format!("failed to create {}: {error}", run_dir.display()))?;
    let _ = fs::remove_file(run_dir.join("f"));
    let output = Command::new(&binary)
        .current_dir(&run_dir)
        .env_clear()
        .output()
        .map_err(|error| format!("failed to run Linux stage5 fd probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != FD_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 fd mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(FD_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 fd dup/getpid references");
    Ok(())
}

/// M8 冻结的 Linux 参考输出：同一份 `stage5_vfs.c` 在宿主上必须全项通过。
pub(crate) const VFS_EXPECTED_STDOUT: &[u8] = b"VFS|ok=1;motd=1;multi=1;parent=1;dirfd=1;dupdir=1;enoent=1;enotdir=1;eacces=1;eexist=1;create=1;append=1;null=1;getdents=1\n";

#[cfg(unix)]
fn set_probe_mode(path: &Path, mode: u32) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| format!("failed to set mode {mode:o} on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_probe_mode(path: &Path, mode: u32) -> Result<(), String> {
    let _ = (path, mode);
    Ok(())
}

/// M8：用宿主 glibc 重建 `stage5_vfs.c`，在一次性目录中布置与 Prima rootfs 同形的树，
/// 取得多级路径/目录枚举/fd 语义的参考结果。stdin 接 `/dev/null`，使 fd 0 的 EOF 断言一致。
pub(crate) fn check_stage5_linux_vfs() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_vfs.c");
    let binary = workspace.join("target/prima-os/vfs-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 vfs: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 vfs probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let run_dir = workspace.join("target/prima-os/vfs-linux-run");
    prepare_stage5_reference_tree(&run_dir)?;
    let output = Command::new(&binary)
        .current_dir(&run_dir)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run Linux stage5 vfs probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != VFS_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 vfs mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(VFS_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 vfs multi-level path and directory references");
    Ok(())
}

/// M13a：符号链接/readlink/rename/fchdir 的 Linux 参考运行（同一源码）。
pub(crate) fn check_stage5_linux_vfs2() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_vfs2.c");
    let binary = workspace.join("target/prima-os/vfs2-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 vfs2: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 vfs2 probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let run_dir = workspace.join("target/prima-os/vfs2-linux-run");
    prepare_stage5_reference_tree(&run_dir)?;
    let output = Command::new(&binary)
        .current_dir(&run_dir)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run Linux stage5 vfs2 probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != VFS2_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 vfs2 mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(VFS2_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 symlink/rename/fchdir references");
    Ok(())
}

/// M13b：chmod/fchmod/umask 与 stdio 文件 API 的 Linux 参考运行（同一源码）。
pub(crate) fn check_stage5_linux_vfs3() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_vfs3.c");
    let binary = workspace.join("target/prima-os/vfs3-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 vfs3: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 vfs3 probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let run_dir = workspace.join("target/prima-os/vfs3-linux-run");
    prepare_stage5_reference_tree(&run_dir)?;
    let output = Command::new(&binary)
        .current_dir(&run_dir)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run Linux stage5 vfs3 probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != VFS3_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 vfs3 mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(VFS3_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 chmod/umask/stdio references");
    Ok(())
}

/// M19：loopback UDP 的 Linux 参考运行（同一源码，真实 127.0.0.1 套接字）。
pub(crate) fn check_stage5_linux_net() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_net.c");
    let binary = workspace.join("target/prima-os/net-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 net: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 net probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let output = Command::new(&binary)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run Linux stage5 net probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != NET_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 net mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(NET_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 loopback UDP references");
    Ok(())
}

/// M14：fd 0 tty 输入的 Linux 参考运行（管道 stdin）。
pub(crate) fn check_stage5_linux_ttyin() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_ttyin.c");
    let binary = workspace.join("target/prima-os/ttyin-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 ttyin: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 ttyin probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    use std::io::Write;
    let mut child = Command::new(&binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_clear()
        .spawn()
        .map_err(|error| format!("failed to run Linux stage5 ttyin probe: {error}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(b"hi\n");
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("failed to wait for Linux stage5 ttyin probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != TTYIN_LINUX_STDOUT {
        return Err(format!(
            "Linux stage5 ttyin mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(TTYIN_LINUX_STDOUT)
        ));
    }
    println!("PASS linux stage5 tty input references");
    Ok(())
}

/// M8/M9：布置与 Prima rootfs 同形的宿主参考树，权限也按冻结的 mode 设置：
/// 文件 0444、`f` 0666、目录 0555、`tmp` 0777。`/dev/null` 只在 Prima 侧存在。
fn prepare_stage5_reference_tree(run_dir: &Path) -> Result<(), String> {
    if run_dir.exists() {
        // 树内的 0555 目录会阻止 unlink，先恢复目录写权限再删。
        make_tree_removable(run_dir);
        fs::remove_dir_all(run_dir)
            .map_err(|error| format!("failed to clean {}: {error}", run_dir.display()))?;
    }
    for dir in ["etc", "usr/lib", "tmp", "dev"] {
        fs::create_dir_all(run_dir.join(dir))
            .map_err(|error| format!("failed to create {dir}: {error}"))?;
    }
    fs::write(run_dir.join("etc/motd"), b"welcome\n")
        .map_err(|error| format!("failed to write motd: {error}"))?;
    fs::write(run_dir.join("etc/hostname"), b"prima\n")
        .map_err(|error| format!("failed to write hostname: {error}"))?;
    fs::write(run_dir.join("usr/lib/version"), b"1.0\n")
        .map_err(|error| format!("failed to write version: {error}"))?;
    fs::write(run_dir.join("README"), b"prima rootfs\n")
        .map_err(|error| format!("failed to write README: {error}"))?;
    fs::write(run_dir.join("f"), b"").map_err(|error| format!("failed to write f: {error}"))?;
    for file in ["etc/motd", "etc/hostname", "usr/lib/version", "README"] {
        set_probe_mode(&run_dir.join(file), 0o444)?;
    }
    set_probe_mode(&run_dir.join("f"), 0o666)?;
    for dir in ["etc", "usr", "usr/lib", "dev"] {
        set_probe_mode(&run_dir.join(dir), 0o555)?;
    }
    set_probe_mode(&run_dir.join("tmp"), 0o777)?;
    Ok(())
}

/// 删除参考树前恢复目录写权限（树内目录可能已按 rootfs 设为 0555）。
#[cfg(unix)]
fn make_tree_removable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.is_dir()
    {
        let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o755));
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                make_tree_removable(&entry.path());
            }
        }
    }
}

#[cfg(not(unix))]
fn make_tree_removable(_path: &Path) {}

/// M9 冻结的元数据/工作目录探针输出（Linux 与 Prima 逐字节相同）。
pub(crate) const META_EXPECTED_STDOUT: &[u8] = b"META|ok=1;motd=1;hostname=1;readme=1;version=1;etc=1;tmp=1;f=1;null=1;lstat=1;fstat=1;fstatat=1;enoent=1;enotdir=1;empty=1;chdir=1;cwd_abs=1;cwd_etc=1;cwd_parent=1;openrel=1;chdir_file=1;erange=1\n";

/// M10 冻结的匿名内存/保护探针输出（Linux 与 Prima 逐字节相同；`exec`/`native`
/// 是各平台的冻结期望，不是同一实现）。
pub(crate) const MEM_EXPECTED_STDOUT: &[u8] = b"MEM|ok=1;map=1;zero=1;write=1;protect=1;protect_back=1;fixed=1;reuse=1;none=1;exec=1;mmap_err=1;munmap_err=1;mprotect_err=1;unmapped=1;big=1;native=1\n";

/// M11 冻结的文件树变更探针输出（Linux 与 Prima 逐字节相同；`nonempty`/`unlink_dir`
/// 是各平台冻结期望）。
pub(crate) const MUT_EXPECTED_STDOUT: &[u8] = b"MUT|ok=1;mkdir=1;exists=1;stat_dir=1;nested=1;nonempty=1;rmdir=1;unlink_file=1;unlink_dir=1;rmdir_file=1;readonly=1;missing=1;enoent=1;at=1\n";

/// M13a 冻结的符号链接/rename/fchdir 探针输出（Linux 与 Prima 逐字节相同）。
pub(crate) const VFS2_EXPECTED_STDOUT: &[u8] = b"VFS2|sym=1;chain=1;ren=1;renlink=1;dir=1;subtree=1;nonempty=1;fchdir=1;ro=1;loop=1;type=1;trunc=1;missing=1|ok=1;err=0\n";

/// M13b 冻结的 chmod/fchmod/umask/stdio 探针输出（Linux 与 Prima 逐字节相同）。
pub(crate) const VFS3_EXPECTED_STDOUT: &[u8] =
    b"VFS3|umask0=1;umask077=1;chmod=1;readonly=1;stdio=1;stdio_ro=1;missing=1|ok=1;err=0\n";

/// M19 冻结的 loopback UDP 探针输出（Linux 与 Prima 逐字节相同）。
pub(crate) const NET_EXPECTED_STDOUT: &[u8] =
    b"NET|pair=1;zero=1;large=1;drop=1;reuse=1;badfam=1;closed=1|ok=1;err=0\n";

/// M14：Prima 侧 tty 输入探针输出（含回显与 raw/EOF）。
pub(crate) const TTYIN_EXPECTED_STDOUT: &[u8] =
    b"TTYRD|termios=1\nTTYREADY\nhi\n;line=1\nTTYRAW\n;raw=1\nTTYEOF\n;eof=1|ok=1;err=0\n";

/// M14：Linux 参考用管道 stdin，只验证行与 EOF（termios/回显需真实 tty）。
pub(crate) const TTYIN_LINUX_STDOUT: &[u8] = b"TTYRD|line=1;eof=1|ok=1;err=0\n";

/// M15a-1：WaitObject 原生探针输出（Prima 专用，无 Linux 对应物）。
pub(crate) const WAIT_EXPECTED_STDOUT: &[u8] =
    b"WAIT|create=1;gen0=1;signal=1;satisfied=1;timeout=1;invalid=1;destroy=1;dead=1|ok=1;err=0\n";

/// M15b-2：共享线程 signal 唤醒阻塞等待（Prima 专用）。
pub(crate) const WAIT2_EXPECTED_STDOUT: &[u8] =
    b"WAIT2|create=1;thread=1;satisfied=1;gen=1;destroy=1|ok=1;err=0\n";

/// M16a：futex WAIT/WAKE 探针输出（Prima 专用）。
pub(crate) const FUTEX_EXPECTED_STDOUT: &[u8] =
    b"FUTEX|thread=1;wake=1;wait=1;eagain=1;nowake=1|ok=1;err=0\n";

pub(crate) const PTHREAD_EXPECTED_STDOUT: &[u8] =
    b"PTHREAD|create=1;join=1;ret=1;value=1;self=1|ok=1;err=0\n";

/// M16b：pthread 的 Linux 参考（真实 pthread）。
pub(crate) fn check_stage5_linux_pthread() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_pthread.c");
    let binary = workspace.join("target/prima-os/pthread-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-pthread"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 pthread: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 pthread probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let output = Command::new(&binary)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .env_clear()
        .output()
        .map_err(|error| format!("failed to run Linux stage5 pthread probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != PTHREAD_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 pthread mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(PTHREAD_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 pthread references");
    Ok(())
}

pub(crate) const SHARE_EXPECTED_STDOUT: &[u8] =
    b"SHARE|create=1;thread=1;wake=1;word=1;destroy=1|ok=1;err=0\n";

/// 一个 sbase 冻结用例：argv（含程序名）与逐字节冻结的 stdout。
pub(crate) struct SbaseCase {
    pub(crate) name: &'static str,
    pub(crate) args: &'static [&'static str],
    pub(crate) stdout: &'static [u8],
}

/// M9：sbase `cat` 读取 rootfs 文件。
pub(crate) const CAT_CASES: [SbaseCase; 1] = [SbaseCase {
    name: "C1",
    args: &["cat", "etc/motd"],
    stdout: b"welcome\n",
}];

/// M9：sbase `ls` 的目录枚举、cwd 与 stat 用例。
pub(crate) const LS_CASES: [SbaseCase; 3] = [
    SbaseCase {
        name: "L1",
        args: &["ls", "etc"],
        stdout: b"hostname\nmotd\n",
    },
    SbaseCase {
        name: "L2",
        args: &["ls", "."],
        stdout: b"README\ndev\netc\nf\ntmp\nusr\n",
    },
    SbaseCase {
        name: "L3",
        args: &["ls", "etc/motd"],
        stdout: b"etc/motd\n",
    },
];

/// M9：用宿主 glibc 重建 `stage5_meta.c`，在一次性树中取得 statx/chdir/getcwd 参考。
pub(crate) fn check_stage5_linux_meta() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_meta.c");
    let binary = workspace.join("target/prima-os/meta-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 meta: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 meta probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let run_dir = workspace.join("target/prima-os/meta-linux-run");
    prepare_stage5_reference_tree(&run_dir)?;
    let output = Command::new(&binary)
        .current_dir(&run_dir)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run Linux stage5 meta probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != META_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 meta mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(META_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 statx/chdir/getcwd references");
    Ok(())
}

/// M10：用宿主 glibc 重建 `stage5_mem.c`，取得 mmap/munmap/mprotect 参考。
pub(crate) fn check_stage5_linux_mem() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_mem.c");
    let binary = workspace.join("target/prima-os/mem-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 mem: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 mem probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let output = Command::new(&binary)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run Linux stage5 mem probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != MEM_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 mem mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(MEM_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 mmap/munmap/mprotect references");
    Ok(())
}

/// M11：用宿主 glibc 重建 `stage5_mut.c`，在一次性目录（含只读 `ro/`）中取得参考。
pub(crate) fn check_stage5_linux_mut() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let source = workspace.join("prima-user/c/stage5_mut.c");
    let binary = workspace.join("target/prima-os/mut-linux");
    let compiled = Command::new("cc")
        .args(["-O2", "-std=c11", "-Wall", "-Werror", "-DPRIMA_HOST"])
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .map_err(|error| format!("failed to run Linux cc for stage5 mut: {error}"))?;
    if !compiled.status.success() {
        return Err(format!(
            "failed to build Linux stage5 mut probe: {}",
            String::from_utf8_lossy(&compiled.stderr)
        ));
    }
    let run_dir = workspace.join("target/prima-os/mut-linux-run");
    let _ = fs::remove_dir_all(&run_dir);
    fs::create_dir_all(run_dir.join("ro"))
        .map_err(|error| format!("failed to create {}: {error}", run_dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(run_dir.join("ro"), fs::Permissions::from_mode(0o555))
            .map_err(|error| format!("failed to set ro permissions: {error}"))?;
    }
    let output = Command::new(&binary)
        .current_dir(&run_dir)
        .env_clear()
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("failed to run Linux stage5 mut probe: {error}"))?;
    if output.status.code() != Some(0) || output.stdout != MUT_EXPECTED_STDOUT {
        return Err(format!(
            "Linux stage5 mut mismatch: exit {:?}, stdout {:?}, expected exit 0, stdout {:?}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(MUT_EXPECTED_STDOUT)
        ));
    }
    println!("PASS linux stage5 mkdir/rmdir/unlink references");
    Ok(())
}

/// M9：用宿主 glibc 重建 sbase `cat`/`ls`，在同一参考树上取得逐字节 stdout。
pub(crate) fn check_stage5_linux_cat_ls() -> Result<(), String> {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let sbase = workspace.join("target").join("sbase");
    ensure_sbase(&sbase)?;
    let output_dir = workspace.join("target/prima-os");
    let cpp_flags = [
        "-DPREFIX=\"/usr/local\"",
        "-D_DEFAULT_SOURCE",
        "-D_NETBSD_SOURCE",
        "-D_BSD_SOURCE",
        "-D_XOPEN_SOURCE=700",
        "-D_FILE_OFFSET_BITS=64",
    ];
    let cat_binary = output_dir.join("cat-linux");
    let ls_binary = output_dir.join("ls-linux");
    let build = |binary: &Path, sources: &[PathBuf]| -> Result<(), String> {
        let mut command = Command::new("cc");
        command
            .args(["-O2", "-std=c11", "-Wall", "-Werror"])
            .args(cpp_flags);
        command.arg("-I").arg(&sbase);
        for source in sources {
            command.arg(source);
        }
        let built =
            command.arg("-o").arg(binary).output().map_err(|error| {
                format!("failed to run Linux cc for {}: {error}", binary.display())
            })?;
        if !built.status.success() {
            return Err(format!(
                "failed to build Linux {}: {}",
                binary.display(),
                String::from_utf8_lossy(&built.stderr)
            ));
        }
        Ok(())
    };
    build(
        &cat_binary,
        &[
            sbase.join("cat.c"),
            sbase.join("libutil").join("concat.c"),
            sbase.join("libutil").join("writeall.c"),
            sbase.join("libutil").join("eprintf.c"),
            sbase.join("libutil").join("fshut.c"),
        ],
    )?;
    build(
        &ls_binary,
        &[
            sbase.join("ls.c"),
            sbase.join("libutil").join("ealloc.c"),
            sbase.join("libutil").join("eprintf.c"),
            sbase.join("libutil").join("fshut.c"),
            sbase.join("libutil").join("reallocarray.c"),
            sbase.join("libutil").join("human.c"),
            sbase.join("libutf").join("rune.c"),
            sbase.join("libutf").join("runetype.c"),
            sbase.join("libutf").join("utf.c"),
            sbase.join("libutf").join("isprintrune.c"),
            sbase.join("libutf").join("iscntrlrune.c"),
        ],
    )?;
    let run_dir = workspace.join("target/prima-os/sbase-linux-run");
    prepare_stage5_reference_tree(&run_dir)?;
    let run = |binary: &Path, args: &[&str]| -> Result<Vec<u8>, String> {
        let output = Command::new(binary)
            .args(args)
            .current_dir(&run_dir)
            .env_clear()
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("failed to run {}: {error}", binary.display()))?;
        if output.status.code() != Some(0) {
            return Err(format!(
                "Linux {} {args:?} exited {:?}: {}",
                binary.display(),
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(output.stdout)
    };
    if run(&cat_binary, &["etc/motd"])? != CAT_CASES[0].stdout {
        return Err("Linux sbase cat mismatch".to_owned());
    }
    for case in &LS_CASES {
        // argv[0] 只作为程序名；实际参数与 Prima 用例一致。
        let args = &case.args[1..];
        if run(&ls_binary, args)? != case.stdout {
            return Err(format!("Linux sbase ls {} mismatch", case.name));
        }
    }
    println!("PASS linux sbase cat/ls references");
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

    fn payload(image: &[u8]) -> &[u8] {
        &image[prima_runtime::protocol::SERVICE_ECHO_ARGS_OFFSET as usize..]
    }

    #[test]
    fn program_args_without_env_keep_the_m5_legacy_block() {
        let image = pack_program_args(&["echo", "a"], &[]).unwrap();
        assert_eq!(&image[0..4], b"\x7fELF");
        assert_eq!(payload(&image), &[2, 4, b'e', b'c', b'h', b'o', 1, b'a']);
    }

    #[test]
    fn program_args_with_env_are_versioned_and_append_prima_abi() {
        let image = pack_program_args(&["p"], &["M12_ENV=one", "M12_EMPTY="]).unwrap();
        let bytes = payload(&image);
        assert_eq!(&bytes[..3], &[0xa5, 2, 1]);
        assert_eq!(bytes[3], 1);
        assert_eq!(bytes[4], b'p');
        let env_count = bytes[5];
        assert_eq!(env_count, 3);
        let text = String::from_utf8_lossy(&bytes[5..]);
        assert!(text.contains("M12_ENV=one"));
        assert!(text.contains("M12_EMPTY="));
        assert!(text.contains("PRIMA_ABI=1"));
    }

    #[test]
    fn program_args_reject_capacity_overflow_without_truncation() {
        let many = vec!["a"; MAX_PROGRAM_ARGS + 1];
        assert!(pack_program_args(&many, &[]).is_err());
        let env = vec!["A=1"; MAX_PROGRAM_ENV];
        assert!(pack_program_args(&["p"], &env).is_err());
        let long = "x".repeat(256);
        assert!(pack_program_args(&[&long], &[]).is_err());
    }
}
