mod compat;
mod image;
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
        "compat" => compat_command(&rest),
        "linux-trace" => linux_trace_command(&rest),
        _ => Err(usage()),
    }
}

fn build_and_report() -> Result<image::Images, String> {
    let images = image::build_images()?;
    image::report_build_success();
    Ok(images)
}

fn test_boot_slice() -> Result<(), String> {
    let images = build_and_report()?;
    for run in 1..=2 {
        qemu::run_bios_test(&images.bios)
            .map_err(|error| format!("BIOS run {run} failed: {error}"))?;
        println!("PASS qemu bios run={run}");
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
    "usage: cargo xtask <build|run|test|compat|linux-trace>\n\
     \x20      cargo xtask compat [--baseline <path>]\n\
     \x20      cargo xtask linux-trace [--regenerate [--tcg]]"
        .to_owned()
}
