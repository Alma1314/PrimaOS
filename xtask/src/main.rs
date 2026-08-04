mod image;
mod protocol;
mod qemu;

use std::{env, process};

fn main() {
    if let Err(error) = run() {
        eprintln!("ERROR {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let command = args.next().ok_or_else(usage)?;
    if args.next().is_some() {
        return Err(usage());
    }

    match command.as_str() {
        "build" => {
            let _images = build_and_report()?;
            Ok(())
        }
        "run" => {
            let images = build_and_report()?;
            qemu::run_bios_interactive(&images.bios)
        }
        "test" => test_boot_slice(),
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

fn usage() -> String {
    "usage: cargo xtask <build|run|test>".to_owned()
}
