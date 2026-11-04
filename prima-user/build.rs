use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=linux-probes");
    if env::var_os("CARGO_FEATURE_LINUX_PROBE").is_none() {
        return;
    }
    for name in ["p1", "p2", "process", "process-child"] {
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
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("linux-probes/matrix.c"),
        )
        .arg("-o")
        .arg(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("stage6-matrix.elf"))
        .status()
        .expect("build raw Linux syscall matrix with cc");
    assert!(status.success(), "Linux matrix compile/link failed");
}
