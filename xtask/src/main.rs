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
        "test-rust" => {
            require_no_args(&rest)?;
            test_rust_slice()
        }
        "test-process" => {
            let case = match rest.as_slice() {
                [] => None,
                [name] if matches!(name.as_str(), "W6" | "W7" | "W7b" | "W8") => {
                    Some(name.as_str())
                }
                _ => return Err(usage()),
            };
            test_process_slice(case)
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

/// Focused process acceptance, with the same validators and two-run IPC baseline as test.
fn test_process_slice(case: Option<&str>) -> Result<(), String> {
    if case.is_none() || case == Some("W6") {
        image::check_stage5_linux_proc()?;
        test_program_twice(
            image::build_images_for_proc()?,
            "W6",
            image::PROC_EXPECTED_STDOUT,
        )?;
    }
    if case.is_none() || case == Some("W7") {
        test_program_twice(
            image::build_images_for_exec()?,
            "W7",
            image::EXEC_EXPECTED_STDOUT,
        )?;
    }
    if case.is_none() || case == Some("W7b") {
        test_program_twice(
            image::build_images_for_execve()?,
            "W7b",
            image::EXECVE_EXPECTED_STDOUT,
        )?;
    }
    if case.is_none() || case == Some("W8") {
        image::check_stage5_linux_sig()?;
        test_program_twice(
            image::build_images_for_sig()?,
            "W8",
            image::SIG_EXPECTED_STDOUT,
        )?;
    }
    Ok(())
}

fn test_program_twice(images: image::Images, name: &str, stdout: &[u8]) -> Result<(), String> {
    let mut ipc_baseline = None;
    let mut failures = Vec::new();
    for run in 1..=2 {
        match qemu::run_bios_test_program(&images.bios, name, stdout, ipc_baseline) {
            Ok(ipc) => {
                ipc_baseline = Some(ipc);
                println!("PASS qemu bios {name} run={run}");
            }
            Err(error) => failures.push(format!("BIOS {name} run {run} failed: {error}")),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
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

fn test_rust_slice() -> Result<(), String> {
    test_program_twice(
        image::build_images_for_rust(false)?,
        "R1",
        image::RUST_CORE_STDOUT,
    )?;
    test_program_twice(
        image::build_images_for_rust(true)?,
        "R2",
        image::RUST_STD_STDOUT,
    )
}

type ImageBuilder = fn() -> Result<image::Images, String>;
type ReferenceCheck = fn() -> Result<(), String>;

fn record_result(failures: &mut Vec<String>, name: &str, result: Result<(), String>) {
    if let Err(error) = result {
        let error = format!("{name}: {error}");
        eprintln!("FAIL {error}");
        failures.push(error);
    }
}

/// Each image still runs exactly twice. Failed independent groups remain failures
/// while later groups execute; there are no retries or relaxed validators.
fn test_boot_slice() -> Result<(), String> {
    let mut failures = Vec::new();
    record_result(&mut failures, "musl host", check_musl_host());
    let references: &[(&str, ReferenceCheck)] = &[
        ("default", image::check_stage5_linux_probe),
        ("echo", image::check_stage5_linux_echo),
        ("cat/ls", image::check_stage5_linux_cat_ls),
        ("A2", image::check_stage5_linux_start),
        ("T1", image::check_stage5_linux_time),
        ("F1", image::check_stage5_linux_fd),
        ("V1", image::check_stage5_linux_vfs),
        ("V2", image::check_stage5_linux_vfs2),
        ("V3", image::check_stage5_linux_vfs3),
        ("N1", image::check_stage5_linux_net),
        ("W6", image::check_stage5_linux_proc),
        ("W8", image::check_stage5_linux_sig),
        ("W9", image::check_stage5_linux_sigact),
        ("W4", image::check_stage5_linux_pthread),
        ("I1", image::check_stage5_linux_ttyin),
        ("M1", image::check_stage5_linux_meta),
        ("D1", image::check_stage5_linux_mut),
        ("A1", image::check_stage5_linux_mem),
    ];
    for (name, check) in references {
        record_result(&mut failures, &format!("Linux {name}"), check());
    }
    match build_and_report() {
        Ok(images) => {
            for run in 1..=2 {
                let result = qemu::run_bios_test(&images.bios);
                if result.is_ok() {
                    println!("PASS qemu bios run={run}");
                }
                record_result(&mut failures, &format!("default run={run}"), result);
            }
        }
        Err(error) => record_result(&mut failures, "default build", Err(error)),
    }
    for case in &image::ECHO_CASES {
        let result = image::build_images_for_echo(case.args)
            .and_then(|images| test_program_twice(images, case.name, case.stdout));
        record_result(&mut failures, case.name, result);
    }
    let programs: &[(&str, ImageBuilder, &[u8])] = &[
        (
            "A2",
            image::build_images_for_start,
            image::START_EXPECTED_STDOUT,
        ),
        (
            "T1",
            image::build_images_for_time,
            image::TIME_EXPECTED_STDOUT,
        ),
        ("F1", image::build_images_for_fd, image::FD_EXPECTED_STDOUT),
        (
            "V1",
            image::build_images_for_vfs,
            image::VFS_EXPECTED_STDOUT,
        ),
        (
            "V2",
            image::build_images_for_vfs2,
            image::VFS2_EXPECTED_STDOUT,
        ),
        (
            "V3",
            image::build_images_for_vfs3,
            image::VFS3_EXPECTED_STDOUT,
        ),
        (
            "N1",
            image::build_images_for_net,
            image::NET_EXPECTED_STDOUT,
        ),
        (
            "W1",
            image::build_images_for_wait,
            image::WAIT_EXPECTED_STDOUT,
        ),
        (
            "W2",
            image::build_images_for_wait2,
            image::WAIT2_EXPECTED_STDOUT,
        ),
        (
            "W3",
            image::build_images_for_futex,
            image::FUTEX_EXPECTED_STDOUT,
        ),
        (
            "W6",
            image::build_images_for_proc,
            image::PROC_EXPECTED_STDOUT,
        ),
        (
            "W8",
            image::build_images_for_sig,
            image::SIG_EXPECTED_STDOUT,
        ),
        (
            "W9",
            image::build_images_for_sigact,
            image::SIGACT_EXPECTED_STDOUT,
        ),
        (
            "W7",
            image::build_images_for_exec,
            image::EXEC_EXPECTED_STDOUT,
        ),
        (
            "W7b",
            image::build_images_for_execve,
            image::EXECVE_EXPECTED_STDOUT,
        ),
        (
            "W4",
            image::build_images_for_pthread,
            image::PTHREAD_EXPECTED_STDOUT,
        ),
        (
            "W5",
            image::build_images_for_share,
            image::SHARE_EXPECTED_STDOUT,
        ),
        (
            "I1",
            image::build_images_for_ttyin,
            image::TTYIN_EXPECTED_STDOUT,
        ),
        (
            "M1",
            image::build_images_for_meta,
            image::META_EXPECTED_STDOUT,
        ),
        (
            "D1",
            image::build_images_for_mut,
            image::MUT_EXPECTED_STDOUT,
        ),
        (
            "A1",
            image::build_images_for_mem,
            image::MEM_EXPECTED_STDOUT,
        ),
    ];
    for (name, build, stdout) in programs {
        let result = build().and_then(|images| {
            if *name == "I1" {
                test_tty_twice(images)
            } else {
                test_program_twice(images, name, stdout)
            }
        });
        record_result(&mut failures, name, result);
    }
    for case in &image::CAT_CASES {
        let result = image::build_images_for_cat(case.args)
            .and_then(|images| test_program_twice(images, case.name, case.stdout));
        record_result(&mut failures, case.name, result);
    }
    for case in &image::LS_CASES {
        let result = image::build_images_for_ls(case.args)
            .and_then(|images| test_program_twice(images, case.name, case.stdout));
        record_result(&mut failures, case.name, result);
    }
    for (standard, name, stdout) in [
        (false, "R1", image::RUST_CORE_STDOUT),
        (true, "R2", image::RUST_STD_STDOUT),
    ] {
        let result = image::build_images_for_rust(standard)
            .and_then(|images| test_program_twice(images, name, stdout));
        record_result(&mut failures, name, result);
    }
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/prima-os");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::write(
        directory.join("full-suite-errors.log"),
        failures.join("\n\n"),
    )
    .map_err(|error| error.to_string())?;
    if !failures.is_empty() {
        return Err(format!(
            "full BIOS suite: {} failed groups/checks; details in target/prima-os/full-suite-errors.log",
            failures.len()
        ));
    }
    println!("PASS full BIOS suite: 34 programs, 68 runs");
    println!("PASS stage1 boot slice");
    Ok(())
}

fn test_tty_twice(images: image::Images) -> Result<(), String> {
    let mut baseline = None;
    let mut failures = Vec::new();
    for run in 1..=2 {
        let result = qemu::run_bios_test_input(
            &images.bios,
            "I1",
            image::TTYIN_EXPECTED_STDOUT,
            baseline,
            &[("TTYREADY", b"hi\n"), ("TTYRAW", b"xy"), ("TTYEOF", &[4u8])],
        );
        match result {
            Ok(ipc) => {
                baseline = Some(ipc);
                println!("PASS qemu bios I1 run={run}");
            }
            Err(error) => failures.push(format!("I1 run={run}: {error}")),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
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
    "usage: cargo xtask <build|run|test|test-vfs|test-process|test-rust|test-musl|compat|linux-trace|lint|tcb>\n\
     \x20      cargo xtask test-vfs [V1|V2|V3]\n\
     \x20      cargo xtask test-process [W6|W7|W7b|W8]\n\
     \x20      cargo xtask test-musl [--host-only]\n\
     \x20      cargo xtask compat [--baseline <path>]\n\
     \x20      cargo xtask linux-trace [--regenerate [--tcg]]"
        .to_owned()
}
