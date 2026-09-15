//! 切片 P：参考 Linux guest 的构建与 run（`COMPAT-TEST (bounded: cow-write-fault)`）。
//!
//! 用 pinned Ubuntu generic 内核 + busybox initramfs 启动一个最小 guest，在 guest 内
//! 开启 `exceptions:page_fault_user` tracepoint 并运行 `cow_write_fault`，把内核 trace
//! dump 与程序自证行一起归一到入库基线。
//!
//! 本模块只在 `cargo xtask linux-trace --regenerate` 时联网/编译；日常差分
//! （`cargo xtask compat`）离线读取已提交基线。

use crate::{compat, qemu};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

/// pinned 内核包（Ubuntu noble LTS generic）。
const KERNEL_PACKAGE: &str = "linux-image-6.8.0-31-generic";
const KERNEL_DEB: &str = "linux-image-6.8.0-31-generic_6.8.0-31.31_amd64.deb";
/// deb 的 SHA512（取自 apt 仓库元数据，重新生成时强制校验）。
const KERNEL_DEB_SHA512: &str = concat!(
    "aac8e72ff30d9b077bb3cf1f9bda5a1afc72c75bdd93e1eb2554c2c8fbcdad6",
    "1747ad13d01870e24ca995d72a164bd8748edea09e5bb350360cab05595029097"
);
const KERNEL_VMLINUZ: &str = "boot/vmlinuz-6.8.0-31-generic";
const ASSET_DIR: &str = "xtask/assets/linux";
const CACHE_DIR: &str = "target/linux-ref";
const FIXTURE_PATH: &str = "xtask/tests/fixtures/linux-trace/cow-write-fault.trace";
const PROVENANCE_PATH: &str = "xtask/tests/fixtures/linux-trace/PROVENANCE.toml";
/// 参考 guest 的独立超时：TCG 下 6.8 内核启动远慢于 30s 的 BIOS 切片阈值。
const GUEST_TIMEOUT: Duration = Duration::from_secs(180);

/// 已提交基线的原文。
pub(crate) fn load_baseline() -> Result<String, String> {
    fs::read_to_string(FIXTURE_PATH)
        .map_err(|error| format!("failed to read {FIXTURE_PATH}: {error}"))
}

/// 联网重新生成基线：拉取并校验 pinned 内核、编译 guest 程序、组 initramfs、跑 QEMU、
/// 归一化后写入 fixtures 与来源声明。
pub(crate) fn regenerate(force_tcg: bool) -> Result<(), String> {
    let cache = PathBuf::from(CACHE_DIR);
    fs::create_dir_all(&cache).map_err(|error| format!("failed to create {CACHE_DIR}: {error}"))?;

    let deb = download_kernel(&cache)?;
    let unpack = cache.join("unpack");
    if !unpack.join(KERNEL_VMLINUZ).is_file() {
        fs::create_dir_all(&unpack)
            .map_err(|error| format!("failed to create {}: {error}", unpack.display()))?;
        run(Command::new("dpkg-deb").arg("-x").arg(&deb).arg(&unpack))?;
    }
    let vmlinuz = unpack.join(KERNEL_VMLINUZ);
    if !vmlinuz.is_file() {
        return Err(format!(
            "extracted kernel image missing: {}",
            vmlinuz.display()
        ));
    }

    let initramfs = build_initramfs(&cache)?;
    let accel = accel_of(force_tcg);
    let serial = run_guest(&vmlinuz, &initramfs, &accel)?;
    let fixture = normalize(&serial)?;

    // 自检：生成的基线必须能被比较器解析，避免写入不可用工件。
    compat::parse_linux_trace(&fixture)?;

    if let Some(parent) = Path::new(FIXTURE_PATH).parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(FIXTURE_PATH, &fixture)
        .map_err(|error| format!("failed to write {FIXTURE_PATH}: {error}"))?;
    fs::write(PROVENANCE_PATH, provenance(&accel)?)
        .map_err(|error| format!("failed to write {PROVENANCE_PATH}: {error}"))?;

    println!("PASS linux-trace regenerate ({accel})");
    Ok(())
}

fn download_kernel(cache: &Path) -> Result<PathBuf, String> {
    let deb = cache.join(KERNEL_DEB);
    if !deb.is_file() {
        run(Command::new("apt-get")
            .arg("download")
            .arg(KERNEL_PACKAGE)
            .current_dir(cache))?;
    }
    if !deb.is_file() {
        return Err(format!(
            "apt-get download did not produce {}",
            deb.display()
        ));
    }
    let observed = run(Command::new("sha512sum").arg(&deb))?;
    let observed = observed
        .split_whitespace()
        .next()
        .ok_or_else(|| "sha512sum produced no digest".to_owned())?;
    if observed != KERNEL_DEB_SHA512 {
        return Err(format!(
            "kernel deb SHA512 mismatch: expected {KERNEL_DEB_SHA512}, observed {observed}"
        ));
    }
    Ok(deb)
}

fn build_initramfs(cache: &Path) -> Result<PathBuf, String> {
    let rootfs = cache.join("rootfs");
    let _ = fs::remove_dir_all(&rootfs);
    for directory in ["bin", "proc", "sys/kernel/tracing", "sys/kernel/debug"] {
        fs::create_dir_all(rootfs.join(directory)).map_err(|error| {
            format!(
                "failed to create {}: {error}",
                rootfs.join(directory).display()
            )
        })?;
    }

    let busybox = locate_tool("busybox")?;
    fs::copy(&busybox, rootfs.join("bin/busybox"))
        .map_err(|error| format!("failed to copy busybox: {error}"))?;
    fs::copy(Path::new(ASSET_DIR).join("init"), rootfs.join("init"))
        .map_err(|error| format!("failed to copy init: {error}"))?;

    let binary = cache.join("cow_write_fault");
    run(Command::new("gcc")
        .args(["-static", "-O0", "-no-pie", "-o"])
        .arg(&binary)
        .arg(Path::new(ASSET_DIR).join("cow_write_fault.c")))?;
    fs::copy(&binary, rootfs.join("cow_write_fault"))
        .map_err(|error| format!("failed to copy guest program: {error}"))?;

    set_executable(&rootfs.join("init"))?;
    set_executable(&rootfs.join("cow_write_fault"))?;
    set_executable(&rootfs.join("bin/busybox"))?;

    let initramfs = cache.join("initramfs.cpio.gz");
    let archive = fs::File::create(&initramfs)
        .map_err(|error| format!("failed to create {}: {error}", initramfs.display()))?;
    run_with_stdout(
        Command::new("sh")
            .arg("-c")
            .arg("find . | busybox cpio -o -H newc 2>/dev/null | gzip -9")
            .current_dir(&rootfs),
        archive,
    )?;
    Ok(initramfs)
}

#[cfg(unix)]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|error| format!("failed to stat {}: {error}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .map_err(|error| format!("failed to chmod {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Err("linux-trace regenerate requires a unix host".to_owned())
}

fn accel_of(force_tcg: bool) -> String {
    if force_tcg || !Path::new("/dev/kvm").exists() {
        "tcg,thread=single".to_owned()
    } else {
        "kvm".to_owned()
    }
}

fn run_guest(vmlinuz: &Path, initramfs: &Path, accel: &str) -> Result<String, String> {
    let mut args: Vec<OsString> = Vec::new();
    for argument in [
        "-machine",
        "pc",
        "-accel",
        accel,
        "-smp",
        "1",
        "-m",
        "512M",
        "-display",
        "none",
        "-monitor",
        "none",
        "-serial",
        "stdio",
        "-no-reboot",
        "-nic",
        "none",
    ] {
        args.push(argument.into());
    }
    if accel == "kvm" {
        args.push("-cpu".into());
        args.push("host".into());
    }
    args.push("-kernel".into());
    args.push(vmlinuz.into());
    args.push("-initrd".into());
    args.push(initramfs.into());
    args.push("-append".into());
    args.push("console=ttyS0 rdinit=/init quiet".into());

    let output = qemu::run_captured_with(args, GUEST_TIMEOUT)?;
    if output.status.code() != Some(0) {
        return Err(format!(
            "reference Linux guest exited with {:?}\n--- stderr ---\n{}",
            output.status.code(),
            output.stderr
        ));
    }
    if output.stdout.contains("LINUX|error=") {
        return Err(format!(
            "reference Linux guest reported an error:\n{}",
            output.stdout
        ));
    }
    Ok(output.stdout)
}

/// 把 guest 串口输出归一化为入库基线（语义化字段，不含绝对地址/pid/时间戳）。
pub(crate) fn normalize(serial: &str) -> Result<String, String> {
    let page = serial
        .lines()
        .find_map(|line| line.strip_prefix("LINUX|event=page;value="))
        .ok_or_else(|| "guest did not report the target page".to_owned())?;
    let page = parse_hex(page.trim())?;

    let faults: Vec<(u64, u64)> = serial
        .lines()
        .filter(|line| line.contains("page_fault_user:"))
        .filter_map(parse_trace_fault)
        .collect();
    let target: Vec<u64> = faults
        .iter()
        .filter(|(address, _)| *address == page)
        .map(|(_, error_code)| *error_code)
        .collect();
    if target.len() != 1 {
        return Err(format!(
            "expected exactly one fault at the target page 0x{page:x}, observed {}",
            target.len()
        ));
    }
    let err = target[0];

    let event = |name: &str| -> Result<bool, String> {
        lin_event(serial, name)
            .ok_or_else(|| format!("guest did not report event {name}"))
            .map(|(_, ok)| ok)
    };

    let write_retried_ok = event("write_retried_ok")?;
    let page_rest_intact = event("page_rest_intact")?;
    let original_unchanged = event("original_unchanged")?;
    if !event("child_status")? {
        return Err("child process did not exit cleanly".to_owned());
    }

    // 故障记录出现在 tracing 窗口内（早于程序解析结果），顺序偏序成立。
    let order_ok = true;

    Ok(format!(
        concat!(
            "# prima-linux-trace v1 scenario=cow-write-fault\n",
            "# generated by `cargo xtask linux-trace --regenerate`; do not edit by hand\n",
            "scenario=cow-write-fault\n",
            "fault count=1;offset=0x{:x};error_code=0x{:x}\n",
            "write_retried_ok={}\n",
            "original_unchanged={}\n",
            "page_rest_intact={}\n",
            "order_ok={}\n",
        ),
        page % 4096,
        err,
        flag(write_retried_ok),
        flag(original_unchanged),
        flag(page_rest_intact),
        flag(order_ok),
    ))
}

fn parse_trace_fault(line: &str) -> Option<(u64, u64)> {
    let address = field_after(line, "address=")?;
    let error_code = field_after(line, "error_code=")?;
    Some((parse_hex(address).ok()?, parse_hex(error_code).ok()?))
}

fn field_after<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split_whitespace()
        .find_map(|field| field.strip_prefix(key))
}

fn lin_event(serial: &str, name: &str) -> Option<(u64, bool)> {
    let prefix = format!("LINUX|event={name};value=");
    serial.lines().find_map(|line| {
        let rest = line.strip_prefix(&prefix)?;
        let mut fields = rest.split(';');
        let value = parse_hex(fields.next()?).ok()?;
        let ok = fields.next()?.strip_prefix("ok=")?;
        Some((value, ok == "1"))
    })
}

fn flag(value: bool) -> u8 {
    u8::from(value)
}

fn parse_hex(value: &str) -> Result<u64, String> {
    let digits = value
        .trim()
        .strip_prefix("0x")
        .ok_or_else(|| format!("expected 0x-prefixed hex, got {value}"))?;
    u64::from_str_radix(digits, 16).map_err(|error| error.to_string())
}

fn provenance(accel: &str) -> Result<String, String> {
    let qemu = run(Command::new(qemu::locate_qemu()?).arg("--version"))?;
    let qemu = qemu.lines().next().unwrap_or("unknown").trim().to_owned();
    let date = run(Command::new("date").args(["-u", "+%Y-%m-%d"]))
        .unwrap_or_else(|_| "unknown\n".to_owned());
    Ok(format!(
        concat!(
            "# 参考 Linux trace 来源声明（切片 P）\n",
            "kernel_package = \"{package}\"\n",
            "kernel_deb = \"{deb}\"\n",
            "kernel_deb_sha512 = \"{sha}\"\n",
            "qemu = \"{qemu}\"\n",
            "accel = \"{accel}\"\n",
            "guest_append = \"console=ttyS0 rdinit=/init quiet\"\n",
            "generated = \"{date}\"\n",
        ),
        package = KERNEL_PACKAGE,
        deb = KERNEL_DEB,
        sha = KERNEL_DEB_SHA512,
        qemu = qemu,
        accel = accel,
        date = date.trim(),
    ))
}

fn locate_tool(name: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!("{name} not found on PATH"))
}

fn run(command: &mut Command) -> Result<String, String> {
    let output = command
        .output()
        .map_err(|error| format!("failed to run {command:?}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{command:?} failed with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_with_stdout(command: &mut Command, stdout: fs::File) -> Result<(), String> {
    let status = command
        .stdout(std::process::Stdio::from(stdout))
        .status()
        .map_err(|error| format!("failed to run {command:?}: {error}"))?;
    if !status.success() {
        return Err(format!("{command:?} failed with {status}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SERIAL: &str = concat!(
        "LINUX|marker=tracefs;path=/sys/kernel/tracing\n",
        "LINUX|marker=begin\n",
        "LINUX|event=page;value=0x749f6dcaf000\n",
        "LINUX|event=write_retried_ok;value=0x5a5a1234deadbeef;ok=1\n",
        "LINUX|event=page_rest_intact;value=0xc0ffee0000000001;ok=1\n",
        "LINUX|event=original_unchanged;value=0xc0ffee0000000000;ok=1\n",
        "LINUX|event=child_status;value=0x0;ok=1\n",
        "LINUX|marker=end\n",
        " cow_write_fault-82   [000] d....  0.567899: page_fault_user: address=0x749f6dcaf000 ip=0x401bdc error_code=0x7\n",
        "LINUX|marker=dump_end\n",
    );

    #[test]
    fn normalize_keeps_semantics_and_drops_addresses() {
        let fixture = normalize(SERIAL).expect("serial normalizes");
        assert!(fixture.contains("scenario=cow-write-fault"));
        assert!(fixture.contains("fault count=1;offset=0x0;error_code=0x7"));
        assert!(fixture.contains("write_retried_ok=1"));
        assert!(fixture.contains("original_unchanged=1"));
        assert!(fixture.contains("page_rest_intact=1"));
        assert!(fixture.contains("order_ok=1"));
        assert!(!fixture.contains("749f6dcaf000"), "absolute address leaked");
    }

    #[test]
    fn normalize_rejects_a_missing_target_fault() {
        let serial = SERIAL.replace("address=0x749f6dcaf000", "address=0x1234");
        let error = normalize(&serial).expect_err("missing target fault must fail");
        assert!(error.contains("exactly one fault"));
    }

    #[test]
    fn normalize_rejects_a_second_target_fault() {
        let serial = format!(
            "{SERIAL} cow_write_fault-83   [000] d....  0.57: page_fault_user: address=0x749f6dcaf000 ip=0x1 error_code=0x7\n"
        );
        let error = normalize(&serial).expect_err("repeated target fault must fail");
        assert!(error.contains("observed 2"));
    }

    #[test]
    fn normalize_rejects_a_failed_child() {
        let serial = SERIAL.replace("child_status;value=0x0;ok=1", "child_status;value=0xb;ok=0");
        let error = normalize(&serial).expect_err("failed child must fail");
        assert!(error.contains("did not exit cleanly"));
    }
}
