mod compat;
mod image;
mod lint;
mod linux;
mod protocol;
mod qemu;

use std::{env, fs, process};

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    let rest: Vec<String> = args.collect();

    match command.as_str() {
        "build" => {
            require_no_args(&rest)?;
            let _images = build_and_report()?;
            Ok(())
        }
        "run" => {
            require_no_args(&rest)?;
            let images = build_and_report()?;
            qemu::run_bios_interactive(&images.bios)
        }
        "test" => {
            require_no_args(&rest)?;
            test_boot_slice()
        }
        "test-vfs" => {
            let program = match rest.as_slice() {
                [] => None,
                [name] if matches!(name.as_str(), "V1" | "V2" | "V3") => Some(name.as_str()),
                _ => return Err(usage()),
            };
            test_vfs_slice(program)
        }
        "test-musl" => match rest.as_slice() {
            [] => test_musl_slice(false),
            [flag] if flag == "--host-only" => test_musl_slice(true),
            _ => Err(usage()),
        },
        "compat" => compat_command(&rest),
        "linux-trace" => linux_trace_command(&rest),
        "lint" => lint::lint_command(&rest),
        "tcb" => lint::tcb_command(&rest),
        _ => Err(usage()),
    }
}

fn build_and_report() -> Result<image::Images, String> {
    let images = image::build_images()?;
    image::report_build_success();
    Ok(images)
}

/// 与全量 test 共用原始验收；便于单独迭代文件系统而不重跑所有子系统。
fn test_vfs_slice(program: Option<&str>) -> Result<(), String> {
    if program.is_none() || program == Some("V1") {
        image::check_stage5_linux_vfs()?;
        test_program_twice(
            image::build_images_for_vfs()?,
            "V1",
            image::VFS_EXPECTED_STDOUT,
        )?;
    }
    if program.is_none() || program == Some("V2") {
        image::check_stage5_linux_vfs2()?;
        test_program_twice(
            image::build_images_for_vfs2()?,
            "V2",
            image::VFS2_EXPECTED_STDOUT,
        )?;
    }
    if program.is_none() || program == Some("V3") {
        image::check_stage5_linux_vfs3()?;
        test_program_twice(
            image::build_images_for_vfs3()?,
            "V3",
            image::VFS3_EXPECTED_STDOUT,
        )?;
    }
    Ok(())
}

fn test_program_twice(images: image::Images, name: &str, stdout: &[u8]) -> Result<(), String> {
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(&images.bios, name, stdout, ipc_baseline)
                .map_err(|error| format!("BIOS {name} run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios {name} run={run}");
    }
    Ok(())
}

fn check_musl_host() -> Result<(), String> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be under workspace root");
    let status = process::Command::new("python3")
        .arg(root.join("prima-user/musl/test_port.py"))
        .status()
        .map_err(|error| format!("failed to run musl port host checks: {error}"))?;
    if !status.success() {
        return Err("musl port host checks failed".into());
    }
    image::check_stage5_linux_fd()
}

/// Focused libc acceptance; host-only never starts QEMU or rebuilds musl.
fn test_musl_slice(host_only: bool) -> Result<(), String> {
    check_musl_host()?;
    if !host_only {
        test_program_twice(
            image::build_images_for_fd()?,
            "F1",
            image::FD_EXPECTED_STDOUT,
        )?;
        image::check_stage5_linux_pthread()?;
        test_program_twice(
            image::build_images_for_pthread()?,
            "W4",
            image::PTHREAD_EXPECTED_STDOUT,
        )?;
    }
    Ok(())
}

fn test_boot_slice() -> Result<(), String> {
    check_musl_host()?;
    // 既有探针路径保持不变：两次 BIOS 运行逐字节校验阶段 1–5 的既有事件。
    let images = build_and_report()?;
    image::check_stage5_linux_probe()?;
    for run in 1..=2 {
        qemu::run_bios_test(&images.bios)
            .map_err(|error| format!("BIOS run {run} failed: {error}"))?;
        println!("PASS qemu bios run={run}");
    }
    // M5：sbase `echo` 的六个冻结用例各跑两轮；每个用例单独构建一次 ramdisk，
    // 使被冻结的 argv 成为该次启动输入。
    image::check_stage5_linux_echo()?;
    for case in &image::ECHO_CASES {
        let images = image::build_images_for_echo(case.args)?;
        let mut ipc_baseline = None;
        for run in 1..=2 {
            ipc_baseline = Some(
                qemu::run_bios_test_echo(&images.bios, case, ipc_baseline)
                    .map_err(|error| format!("BIOS {} run {run} failed: {error}", case.name))?,
            );
            println!("PASS qemu bios {} run={run}", case.name);
        }
    }
    // M6：POSIX 时间探针走同一托管程序通道，各跑两轮并逐轮比较 IPC 计数。
    image::check_stage5_linux_time()?;
    let images = image::build_images_for_time()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "T1",
                image::TIME_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS T1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios T1 run={run}");
    }
    // M7：本地 fd 表 / dup / getpid 探针，各跑两轮并逐轮比较 IPC 计数。
    image::check_stage5_linux_fd()?;
    let images = image::build_images_for_fd()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "F1",
                image::FD_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS F1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios F1 run={run}");
    }
    test_vfs_slice(None)?;
    // M19：用户态 loopback IPv4/UDP，各跑两轮。
    image::check_stage5_linux_net()?;
    let images = image::build_images_for_net()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "N1",
                image::NET_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS N1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios N1 run={run}");
    }
    // M15a-1：WaitObject 原生机制（Prima 专用），各跑两轮。
    let images = image::build_images_for_wait()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W1",
                image::WAIT_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W1 run={run}");
    }
    // M15b-2：共享线程 signal 唤醒阻塞等待（Prima 专用），各跑两轮。
    let images = image::build_images_for_wait2()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W2",
                image::WAIT2_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W2 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W2 run={run}");
    }
    // M16a：futex WAIT/WAKE（Prima 专用），各跑两轮。
    let images = image::build_images_for_futex()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W3",
                image::FUTEX_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W3 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W3 run={run}");
    }
    // M17b：fork/wait/COW 隔离/退出码/ECHILD（同源 Linux 参考），跑两轮。
    image::check_stage5_linux_proc()?;
    let images = image::build_images_for_proc()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W6",
                image::PROC_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W6 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W6 run={run}");
    }
    // M18a：kill/默认终止/wait4 信号状态（同源 Linux 参考），跑两轮。
    image::check_stage5_linux_sig()?;
    let images = image::build_images_for_sig()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W8",
                image::SIG_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W8 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W8 run={run}");
    }
    // M18b：sigaction/handler/sigreturn（同源 Linux 参考），跑两轮。
    image::check_stage5_linux_sigact()?;
    let images = image::build_images_for_sigact()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W9",
                image::SIGACT_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W9 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W9 run={run}");
    }
    // M17c：exec_replace（内存中的静态 ELF、argv、ENOEXEC 负例），跑两轮。
    let images = image::build_images_for_exec()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W7",
                image::EXEC_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W7 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W7 run={run}");
    }
    // M17c-4：VFS 读取镜像 → fork → exec_replace，跑两轮。
    let images = image::build_images_for_execve()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W7b",
                image::EXECVE_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W7b run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W7b run={run}");
    }
    // M16b：pthread 创建/join 与共享内存可见性（同源 Linux 参考），各跑两轮。
    image::check_stage5_linux_pthread()?;
    let images = image::build_images_for_pthread()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W4",
                image::PTHREAD_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W4 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W4 run={run}");
    }
    let images = image::build_images_for_share()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "W5",
                image::SHARE_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS W5 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios W5 run={run}");
    }
    // M14：fd 0 tty 输入（canonical/raw/EOF），marker 驱动串口输入，各跑两轮。
    image::check_stage5_linux_ttyin()?;
    let images = image::build_images_for_ttyin()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_input(
                &images.bios,
                "I1",
                image::TTYIN_EXPECTED_STDOUT,
                ipc_baseline,
                &[("TTYREADY", b"hi\n"), ("TTYRAW", b"xy"), ("TTYEOF", &[4u8])],
            )
            .map_err(|error| format!("BIOS I1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios I1 run={run}");
    }
    // M9：statx / chdir / getcwd 元数据与工作目录探针，各跑两轮。
    image::check_stage5_linux_meta()?;
    let images = image::build_images_for_meta()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "M1",
                image::META_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS M1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios M1 run={run}");
    }
    // M9：sbase `cat` 读取 rootfs 文件，Linux 参考与 Prima 逐字节比较。
    image::check_stage5_linux_cat_ls()?;
    for case in &image::CAT_CASES {
        let images = image::build_images_for_cat(case.args)?;
        let mut ipc_baseline = None;
        for run in 1..=2 {
            ipc_baseline = Some(
                qemu::run_bios_test_program(&images.bios, case.name, case.stdout, ipc_baseline)
                    .map_err(|error| format!("BIOS {} run {run} failed: {error}", case.name))?,
            );
            println!("PASS qemu bios {} run={run}", case.name);
        }
    }
    // M9：sbase `ls`（opendir/readdir + stat/lstat + chdir/getcwd），各跑两轮。
    for case in &image::LS_CASES {
        let images = image::build_images_for_ls(case.args)?;
        let mut ipc_baseline = None;
        for run in 1..=2 {
            ipc_baseline = Some(
                qemu::run_bios_test_program(&images.bios, case.name, case.stdout, ipc_baseline)
                    .map_err(|error| format!("BIOS {} run {run} failed: {error}", case.name))?,
            );
            println!("PASS qemu bios {} run={run}", case.name);
        }
    }
    // M11：mkdir/rmdir/unlink（含 *at）文件树变更探针，各跑两轮。
    image::check_stage5_linux_mut()?;
    let images = image::build_images_for_mut()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "D1",
                image::MUT_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS D1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios D1 run={run}");
    }
    // M10：匿名 mmap/munmap/mprotect 与原生 memseg_protect 探针，各跑两轮。
    image::check_stage5_linux_mem()?;
    let images = image::build_images_for_mem()?;
    let mut ipc_baseline = None;
    for run in 1..=2 {
        ipc_baseline = Some(
            qemu::run_bios_test_program(
                &images.bios,
                "A1",
                image::MEM_EXPECTED_STDOUT,
                ipc_baseline,
            )
            .map_err(|error| format!("BIOS A1 run {run} failed: {error}"))?,
        );
        println!("PASS qemu bios A1 run={run}");
    }
    println!("PASS stage1 boot slice");
    Ok(())
}

/// 切片 P：跑一次 Prima 镜像，把 `user_cow_ready` 观测与参考 Linux 基线做有界差分。
///
/// 默认读取已提交基线（离线）；`--baseline <path>` 可替换基线，用于负例注入。
fn compat_command(args: &[String]) -> Result<(), String> {
    let baseline_path = parse_baseline(args)?;
    let images = build_and_report()?;
    let prima_output = qemu::run_bios_capture(&images.bios)?;
    let prima = compat::parse_prima(&prima_output)
        .map_err(|error| format!("failed to extract the Prima observation: {error}"))?;
    let baseline = match baseline_path {
        Some(path) => {
            fs::read_to_string(&path).map_err(|error| format!("failed to read {path}: {error}"))?
        }
        None => linux::load_baseline()?,
    };
    let linux_reference = compat::parse_linux_trace(&baseline)
        .map_err(|error| format!("failed to parse the Linux baseline: {error}"))?;

    let verdict = compat::compare_cow_write_fault(&prima, &linux_reference);
    if verdict.is_pass() {
        println!("PASS compat cow-write-fault");
        Ok(())
    } else {
        Err(format!(
            "compat cow-write-fault diverged: {}",
            verdict.describe()
        ))
    }
}

/// 切片 P：参考 Linux 基线管理。
///
/// `linux-trace`（无参数）离线校验已提交基线可解析；
/// `linux-trace --regenerate [--tcg]` 联网重建基线（默认 KVM，`--tcg` 强制软件模拟）。
fn linux_trace_command(args: &[String]) -> Result<(), String> {
    let regenerate = args.iter().any(|arg| arg == "--regenerate");
    let force_tcg = args.iter().any(|arg| arg == "--tcg");
    if args
        .iter()
        .any(|arg| arg != "--regenerate" && arg != "--tcg")
    {
        return Err(usage());
    }

    if regenerate {
        return linux::regenerate(force_tcg);
    }
    let baseline = linux::load_baseline()?;
    compat::parse_linux_trace(&baseline)
        .map_err(|error| format!("committed baseline is not parseable: {error}"))?;
    println!("PASS linux-trace baseline");
    Ok(())
}

fn parse_baseline(args: &[String]) -> Result<Option<String>, String> {
    let mut path = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--baseline" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                if path.replace(value.clone()).is_some() {
                    return Err(usage());
                }
                index += 2;
            }
            _ => return Err(usage()),
        }
    }
    Ok(path)
}

fn require_no_args(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(usage())
    }
}

fn usage() -> String {
    "usage: cargo xtask <build|run|test|test-vfs|test-musl|compat|linux-trace|lint|tcb>\n\
     \x20      cargo xtask test-vfs [V1|V2|V3]\n\
     \x20      cargo xtask test-musl [--host-only]\n\
     \x20      cargo xtask compat [--baseline <path>]\n\
     \x20      cargo xtask linux-trace [--regenerate [--tcg]]"
        .to_owned()
}
