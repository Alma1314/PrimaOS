use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("{what}: could not run {error}"));
    assert!(status.success(), "{what} failed with {status}");
}

fn build_winpe_fixture() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("probes");
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let stub_obj = out.join("winpe-stub.obj");
    let stub_dll = out.join("winpe-stub.dll");
    let stub_lib = out.join("winpe-stub.lib");
    let entry_obj = out.join("winpe-entry.obj");
    let image = out.join("winpe-probe.exe");
    let clang_args = [
        "--target=x86_64-pc-windows-msvc",
        "-ffreestanding",
        "-fno-stack-protector",
        "-fno-asynchronous-unwind-tables",
        "-O2",
        "-c",
    ];

    run(
        Command::new("clang")
            .args(clang_args)
            .arg(source.join("stub.c"))
            .arg("-o")
            .arg(&stub_obj),
        "WINPE stub compile",
    );
    run(
        Command::new("lld-link")
            .arg("/dll")
            .arg("/noentry")
            .arg("/export:Stub")
            .arg(format!("/implib:{}", stub_lib.display()))
            .arg(format!("/out:{}", stub_dll.display()))
            .arg(&stub_obj),
        "WINPE stub link",
    );
    run(
        Command::new("clang")
            .args(clang_args)
            .arg(source.join("winentry.c"))
            .arg("-o")
            .arg(&entry_obj),
        "WINPE entry compile",
    );
    run(
        Command::new("lld-link")
            .arg("/entry:entry")
            .arg("/subsystem:console")
            .arg("/nodefaultlib")
            .arg("/dynamicbase")
            .arg(format!("/out:{}", image.display()))
            .arg(&entry_obj)
            .arg(&stub_lib),
        "WINPE entry link",
    );
}

fn main() {
    println!("cargo:rerun-if-changed=probes");
    if env::var_os("CARGO_FEATURE_WINPE_PROBE").is_some() {
        build_winpe_fixture();
    }
}
