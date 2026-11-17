//! P2b-4：两 ABI 代理场景镜像与 QEMU 断言。
//!
//! 镜像由 init（initgen 生成）、`prima-smoke`（既有 init 握手客户端）、
//! log/ns/store 基础服务、Linux personality、原生/POSIX 应用与服务管理器组成。
//! manifest 含 `SERVICE_IMAGE_KIND_LINUX` 即让管理器进入代理场景：Store 就绪后
//! 创建两个应用，签发文件会话与代理会话；两侧各自读取 Store 内的不同资源。

use crate::{image, qemu};
use prima_runtime::protocol::{
    SERVICE_BEHAVIOR_ALPHA, SERVICE_BEHAVIOR_LOG, SERVICE_BEHAVIOR_NS, SERVICE_BEHAVIOR_STORE,
    SERVICE_IMAGE_KIND_LINUX, SERVICE_IMAGE_KIND_NATIVE_APP, SERVICE_IMAGE_KIND_SERVICEMGR,
};
use std::{fs, path::Path};

const LINUX_WITNESS: &str = "S7LX|read=1;proxy=1;status=0";
/// `hello from prima\n` 的 `S6WRITE` 十六进制流。
const LINUX_BYTES: &str = "S6WRITE|fd=1;hex=68656c6c6f2066726f6d207072696d610a";
/// `prima rootfs\n`（13 B）——直接客户端看到的服务根，与 `/linux` 私有视图不同。
const NATIVE_WITNESS: &str =
    "S7NA|abi=native;path=/README;hex=7072696d6120726f6f7466730a;read=13;status=0";
const COMBINED: &str = "S7ABI|linux=1;native=1;proxy=1;status=0";

pub(crate) fn test() -> Result<(), String> {
    let (images, output) = run_round()?;
    for expected in [LINUX_WITNESS, LINUX_BYTES, NATIVE_WITNESS, COMBINED] {
        if !output.contains(expected) {
            return Err(format!(
                "proxy scenario missing witness {expected:?}\n{}",
                tail(&output)
            ));
        }
    }
    if output.contains("S6PANIC|") || output.contains("|event=panic|") {
        return Err(format!("proxy scenario panicked\n{}", tail(&output)));
    }
    let _ = images;
    println!("PASS S7PROXY abi=2 linux=cat native=/README proxy=1 shared_store=1");
    Ok(())
}

fn tail(output: &str) -> String {
    let lines: Vec<&str> = output.lines().rev().take(40).collect();
    lines.into_iter().rev().collect::<Vec<_>>().join("\n")
}

/// Build the scenario image once; the QEMU run is the evidence.
fn run_round() -> Result<(image::Images, String), String> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/prima-os");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let mut manifest: Vec<(u64, Vec<u8>)> = vec![(prima_abi::boot_image::KIND_INIT, Vec::new())];
    for (kind, binary) in [
        (
            SERVICE_BEHAVIOR_ALPHA,
            env!("CARGO_BIN_FILE_PRIMA_USER_prima-smoke"),
        ),
        (
            SERVICE_BEHAVIOR_LOG,
            env!("CARGO_BIN_FILE_PRIMA_USER_prima-log"),
        ),
        (
            SERVICE_BEHAVIOR_NS,
            env!("CARGO_BIN_FILE_PRIMA_USER_prima-ns"),
        ),
        (
            SERVICE_BEHAVIOR_STORE,
            env!("CARGO_BIN_FILE_PRIMA_USER_prima-store"),
        ),
        (
            SERVICE_IMAGE_KIND_LINUX,
            env!("CARGO_BIN_FILE_PRIMA_USER_prima-linux"),
        ),
        (
            SERVICE_IMAGE_KIND_NATIVE_APP,
            env!("CARGO_BIN_FILE_PRIMA_USER_prima-native-scenario"),
        ),
        (
            SERVICE_IMAGE_KIND_SERVICEMGR,
            env!("CARGO_BIN_FILE_PRIMA_USER_prima-servicemgr"),
        ),
    ] {
        let packaged = directory.join(format!("stage7-proxy-{kind:#x}.elf"));
        image::strip_debug(Path::new(binary), &packaged)?;
        let bytes = fs::read(&packaged).map_err(|error| error.to_string())?;
        if bytes.is_empty() {
            return Err(format!("empty scenario image for kind {kind:#x}"));
        }
        manifest.push((kind, bytes));
    }
    let index = |kind: u64| -> Result<usize, String> {
        manifest
            .iter()
            .position(|entry| entry.0 == kind)
            .ok_or_else(|| format!("scenario manifest missing kind {kind:#x}"))
    };
    let alpha_index = index(SERVICE_BEHAVIOR_ALPHA)?;
    let servicemgr_index = index(SERVICE_IMAGE_KIND_SERVICEMGR)?;
    manifest[0].1 = image::build_init_elf(alpha_index, servicemgr_index)?;
    // The init image is initgen's historical two-page program, which expects
    // the platform witness task reserved by the version-2 manifest layout.
    let images = image::package_images(|target| image::write_manifest(target, manifest))?;
    let output = qemu::run_bios_linux_capture(&images.bios)?;
    fs::write(directory.join("S7PROXY.log"), &output).map_err(|error| error.to_string())?;
    Ok((images, output))
}
