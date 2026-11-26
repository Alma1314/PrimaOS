use std::{env, path::PathBuf, process::Command};

fn run(command: &mut Command, what: &str) {
    let status = command
        .status()
        .unwrap_or_else(|error| panic!("{what}: could not run {error}"));
    assert!(status.success(), "{what} failed with {status}");
}

fn main() {
    println!("cargo:rerun-if-changed=probes");
    if env::var_os("CARGO_FEATURE_LINUX_PROBE").is_none() {
        return;
    }
    for name in ["p1", "p2", "process", "process-child", "forklimit"] {
        let source =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join(format!("probes/{name}.S"));
        let output = PathBuf::from(env::var("OUT_DIR").unwrap()).join(format!("stage6-{name}.elf"));
        run(
            Command::new("cc")
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
                .arg(output),
            "Linux P1 assembly/link",
        );
    }
    run(
        Command::new("cc")
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
            .arg(PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("probes/pthread.c"))
            .arg("-o")
            .arg(PathBuf::from(env::var("OUT_DIR").unwrap()).join("stage6-pthread.elf")),
        "Linux pthread compile/link",
    );
    for name in ["matrix", "libos-bench"] {
        if name == "libos-bench" && env::var_os("CARGO_FEATURE_LIBOS_BENCH").is_none() {
            continue;
        }
        run(
            Command::new("cc")
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
                    PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
                        .join(format!("probes/{name}.c")),
                )
                .arg("-o")
                .arg(
                    PathBuf::from(env::var("OUT_DIR").unwrap()).join(format!("stage6-{name}.elf")),
                ),
            "Linux matrix compile/link",
        );
    }
}
