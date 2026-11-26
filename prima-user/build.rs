use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("{what}: could not run {error}"));
    assert!(status.success(), "{what} failed with {status}");
}

fn build_winpe_fixture() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let source = manifest.join("windows-probes");
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
    println!("cargo:rerun-if-changed=linux-probes");
    println!("cargo:rerun-if-changed=windows-probes");
    if env::var_os("CARGO_FEATURE_WINDOWS_PE_PROBE").is_some() {
        build_winpe_fixture();
    }
    if env::var_os("CARGO_FEATURE_LINUX_PROBE").is_none() {
        return;
    }
    for name in ["p1", "p2", "process", "process-child", "forklimit"] {
        let source = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
            .join(format!("linux-probes/{name}.S"));
        let output =
            PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(format!("stage6-{name}.elf"));
        let status = Command::new("cc")
            .args([
                "-m64",
                "-nostdlib",
                "-static",
                "-no-pie",
                "-Wl,--build-id=none",
                "-Wl,--strip-all",
            ])
            .arg(source)
            .arg("-o")
            .arg(output)
            .status()
            .expect("build Linux P1 with cc");
        assert!(status.success(), "Linux {name} assembly/link failed");
    }
    let status = Command::new("cc")
        .args([
            "-m64",
            "-static",
            "-no-pie",
            "-pthread",
            "-O2",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-Wl,--build-id=none",
            "-Wl,--strip-all",
        ])
        .arg(
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
                .join("linux-probes/pthread.c"),
        )
        .arg("-o")
        .arg(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("stage6-pthread.elf"))
        .status()
        .expect("build static Linux pthread witness with cc");
    assert!(status.success(), "Linux pthread compile/link failed");
    for name in ["matrix", "libos-bench"] {
        if name == "libos-bench" && env::var_os("CARGO_FEATURE_LIBOS_BENCH").is_none() {
            continue;
        }
        let status = Command::new("cc")
            .args([
                "-m64",
                "-nostdlib",
                "-static",
                "-no-pie",
                "-O2",
                "-fno-stack-protector",
                "-fno-builtin",
                "-Wall",
                "-Wextra",
                "-Werror",
                "-Wl,--build-id=none",
                "-Wl,--strip-all",
            ])
            .arg(
                PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap())
                    .join(format!("linux-probes/{name}.c")),
            )
            .arg("-o")
            .arg(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join(format!("stage6-{name}.elf")))
            .status()
            .expect("build raw Linux syscall matrix with cc");
        assert!(status.success(), "Linux matrix compile/link failed");
    }
}
