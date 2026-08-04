# Prima OS 阶段 1 启动闭环 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 Windows 11 上建立可重复构建、连续两次无人值守启动并确定性退出的 Prima OS QEMU x86_64 BIOS 启动闭环，同时生成 UEFI 镜像。

**Architecture:** Cargo workspace 包含 `kernel` 与主机侧 `xtask`。`kernel` 是使用 `bootloader_api` 的 `x86_64-unknown-none` freestanding 二进制，只负责 COM1 事件和 `isa-debug-exit`；`xtask` 通过 Cargo artifact dependency 取得 kernel ELF，使用 `bootloader` 生成 BIOS/UEFI 镜像，并负责 QEMU 生命周期和协议验证。

**Tech Stack:** Rust nightly-2026-07-30、Rust 2024 edition、Cargo artifact dependencies、`bootloader`/`bootloader_api` 0.11.17、QEMU 11.0.50、Windows 11、Python 3 标准库文档校验

## Global Constraints

- 权威设计为 `docs/superpowers/specs/2026-08-04-stage1-boot-slice-design.md`。
- 固定 Rust 工具链为 `nightly-2026-07-30`，组件为 `rust-src`、`llvm-tools-preview`，targets 为 `x86_64-unknown-none` 与 `x86_64-unknown-uefi`。
- 固定 `bootloader = 0.11.17`、`bootloader_api = 0.11.17`，并提交 `Cargo.lock`。
- 固定 QEMU winget 包 `SoftwareFreedomConservancy.QEMU` 版本 `11.0.50`。
- BIOS 是首轮自动验收路径；UEFI 只要求镜像生成且非空。
- 唯一功能验收入口是 `cargo xtask test`；该命令必须独立运行两次 BIOS 启动。
- 成功串口序列必须恰好为 `boot_start`、`serial_ready`、`boot_complete`，协议版本为 `v=1`，序号从 1 严格递增。
- QEMU 成功 guest 值为 `0x10`，主机预期退出码为 `33`；panic guest 值为 `0x11`，主机预期退出码为 `35`。
- 固定 QEMU machine 为 `pc-i440fx-10.0`，CPU 为 `qemu64`，内存为 `128M`，加速器为单线程 TCG，测试超时为 10 秒。
- 不实现物理内存发现、页表、GDT/IDT、trap、POSIR Bootstrap、init Task、Capability 或形式化细化。
- 阶段 0 继续是当前阶段，`BLK-001`、`BLK-002` 保持 Open；不得把阶段 1 标记为满足退出条件。
- `.obsidian/` 是现有未跟踪目录，不读取、不修改、不提交。
- 每个任务仅提交该任务列出的文件，不使用 `git add .` 或 `git add -A`。

---

## File Structure

- Modify: `.gitignore` — 保留工作树隔离规则并忽略 Cargo 构建产物。
- Create: `.cargo/config.toml` — 开启 artifact dependency，并定义 `cargo xtask` alias。
- Create: `rust-toolchain.toml` — 固定 nightly、组件和 targets。
- Create: `Cargo.toml` — workspace、成员和 kernel panic profile。
- Create: `Cargo.lock` — 锁定完整依赖图。
- Create: `kernel/Cargo.toml` — freestanding kernel package 与精确 `bootloader_api` 依赖。
- Create: `kernel/src/main.rs` — bootloader 入口、成功路径与 panic 路由。
- Create: `kernel/src/serial.rs` — COM1 初始化和字节写入。
- Create: `kernel/src/protocol.rs` — 固定启动事件及十进制序号输出。
- Create: `kernel/src/qemu.rs` — `isa-debug-exit` 与不可返回的停机循环。
- Create: `xtask/Cargo.toml` — 主机工具、`bootloader` 与 kernel artifact dependency。
- Create: `xtask/src/main.rs` — `build`、`run`、`test` 命令编排。
- Create: `xtask/src/image.rs` — kernel artifact 校验及 BIOS/UEFI 镜像生成。
- Create: `xtask/src/protocol.rs` — `PRIMA|` 事件解析和严格序列验证。
- Create: `xtask/src/qemu.rs` — QEMU 定位、固定参数、超时、输出捕获和退出码校验。
- Modify: `Prima OS 实现阶段.md` — 只在完整验收通过后登记实现与测试证据。

---

### Task 1: 固定主机环境并建立镜像构建闭环

**Files:**
- Modify: `.gitignore`
- Create: `.cargo/config.toml`
- Create: `rust-toolchain.toml`
- Create: `Cargo.toml`
- Create: `Cargo.lock`（由 Cargo 生成）
- Create: `kernel/Cargo.toml`
- Create: `kernel/src/main.rs`
- Create: `xtask/Cargo.toml`
- Create: `xtask/src/main.rs`
- Create: `xtask/src/image.rs`

**Interfaces:**
- Consumes: Rustup、winget，以及设计中固定的 `bootloader` 0.11.17 API。
- Produces: `image::Images { bios: PathBuf }`、`image::build_images() -> Result<Images, String>`、`image::report_build_success()`；稳定产物 `target/prima-os/prima-os-bios.img` 和 `target/prima-os/prima-os-uefi.img`。

- [ ] **Step 1: 运行尚不存在的验收入口，确认基线失败**

Run：

```bash
cargo xtask build
```

Expected：FAIL，Cargo 报告没有 `xtask` 子命令或项目根目录没有 `Cargo.toml`。此时不得创建任何 PASS 证据。

- [ ] **Step 2: 安装固定 QEMU 包**

Run：

```bash
winget install --id SoftwareFreedomConservancy.QEMU --exact --version 11.0.50 --accept-package-agreements --accept-source-agreements --silent
```

Expected：命令退出码为 0，结果为成功安装或已安装相同版本。只允许安装该包。

Verify：

```bash
"/c/Program Files/qemu/qemu-system-x86_64.exe" --version
```

Expected：首行包含 `QEMU emulator version 11.0.50`。

- [ ] **Step 3: 安装固定 Rust 工具链**

Run：

```bash
rustup toolchain install nightly-2026-07-30 --profile minimal --component rust-src --component llvm-tools-preview --target x86_64-unknown-none --target x86_64-unknown-uefi
```

Verify：

```bash
rustc +nightly-2026-07-30 --version && rustup component list --toolchain nightly-2026-07-30-x86_64-pc-windows-msvc --installed
```

Expected：`rustc` 输出包含 `nightly`，组件列表包含 `rust-src`、`llvm-tools`，targets 包含 `x86_64-unknown-none` 与 `x86_64-unknown-uefi`。

- [ ] **Step 4: 写入 workspace 与工具链配置**

Modify `.gitignore`，保留已有隔离规则并写为：

```gitignore
/.worktrees/
/target/
```

Create `.cargo/config.toml`：

```toml
[alias]
xtask = "run --package xtask --"

[unstable]
bindeps = true
```

Create `rust-toolchain.toml`：

```toml
[toolchain]
channel = "nightly-2026-07-30"
profile = "minimal"
components = ["rust-src", "llvm-tools-preview"]
targets = ["x86_64-unknown-none", "x86_64-unknown-uefi"]
```

Create root `Cargo.toml`：

```toml
[workspace]
resolver = "3"
members = ["kernel", "xtask"]
default-members = ["xtask"]

[profile.dev]
panic = "abort"

[profile.release]
panic = "abort"
```

- [ ] **Step 5: 写入最小 freestanding kernel**

Create `kernel/Cargo.toml`：

```toml
[package]
name = "kernel"
version = "0.1.0"
edition = "2024"
publish = false

[[bin]]
name = "prima-kernel"
path = "src/main.rs"
test = false
bench = false

[dependencies]
bootloader_api = "=0.11.17"
```

Create `kernel/src/main.rs`：

```rust
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

use bootloader_api::{entry_point, BootInfo};
use core::{arch::asm, panic::PanicInfo};

entry_point!(kernel_main);

fn kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    halt()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    halt()
}

fn halt() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}
```

该 kernel 只用于证明镜像管线可构建；它尚不输出事件，也不会主动退出。

- [ ] **Step 6: 写入 xtask artifact dependency 与镜像生成器**

Create `xtask/Cargo.toml`：

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
bootloader = "=0.11.17"
kernel = { path = "../kernel", artifact = "bin", target = "x86_64-unknown-none" }
```

Create `xtask/src/image.rs`：

```rust
use bootloader::{BiosBoot, UefiBoot};
use std::{fs, path::PathBuf};

const KERNEL_BINARY: &str = env!("CARGO_BIN_FILE_KERNEL_prima-kernel");
const BIOS_IMAGE_NAME: &str = "prima-os-bios.img";
const UEFI_IMAGE_NAME: &str = "prima-os-uefi.img";

#[derive(Debug)]
pub(crate) struct Images {
    pub(crate) bios: PathBuf,
}

pub(crate) fn build_images() -> Result<Images, String> {
    let kernel = PathBuf::from(KERNEL_BINARY);
    ensure_non_empty(&kernel, "kernel ELF")?;

    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root")
        .to_path_buf();
    let output = workspace.join("target").join("prima-os");
    fs::create_dir_all(&output)
        .map_err(|error| format!("failed to create {}: {error}", output.display()))?;

    let bios = output.join(BIOS_IMAGE_NAME);
    let uefi = output.join(UEFI_IMAGE_NAME);
    remove_stale_image(&bios)?;
    remove_stale_image(&uefi)?;

    BiosBoot::new(&kernel)
        .create_disk_image(&bios)
        .map_err(|error| format!("failed to create BIOS image: {error:#}"))?;
    UefiBoot::new(&kernel)
        .create_disk_image(&uefi)
        .map_err(|error| format!("failed to create UEFI image: {error:#}"))?;

    ensure_non_empty(&bios, "BIOS image")?;
    ensure_non_empty(&uefi, "UEFI image")?;

    Ok(Images { bios })
}

pub(crate) fn report_build_success() {
    println!("PASS build kernel");
    println!("PASS image bios");
    println!("PASS image uefi");
}

fn remove_stale_image(path: &PathBuf) -> Result<(), String> {
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("failed to remove stale {}: {error}", path.display()))?;
    }
    Ok(())
}

fn ensure_non_empty(path: &PathBuf, label: &str) -> Result<(), String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("{label} missing at {}: {error}", path.display()))?;
    if metadata.len() == 0 {
        return Err(format!("{label} is empty at {}", path.display()));
    }
    Ok(())
}
```

Create `xtask/src/main.rs`：

```rust
mod image;

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
            let _images = image::build_images()?;
            image::report_build_success();
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: cargo xtask build".to_owned()
}
```

- [ ] **Step 7: 格式化并构建 BIOS/UEFI 镜像**

Run：

```bash
cargo fmt --all && cargo xtask build
```

Expected tail：

```text
PASS build kernel
PASS image bios
PASS image uefi
```

Verify：

```bash
python -c 'from pathlib import Path; paths=[Path("target/prima-os/prima-os-bios.img"),Path("target/prima-os/prima-os-uefi.img")]; assert all(p.is_file() and p.stat().st_size>0 for p in paths), [(str(p),p.stat().st_size if p.exists() else None) for p in paths]; print("PASS images non-empty")'
```

Expected：`PASS images non-empty`。

- [ ] **Step 8: 提交镜像构建闭环**

```bash
git add -- .gitignore .cargo/config.toml rust-toolchain.toml Cargo.toml Cargo.lock kernel/Cargo.toml kernel/src/main.rs xtask/Cargo.toml xtask/src/main.rs xtask/src/image.rs
git commit -m "$(cat <<'EOF'
feat: establish boot image workspace

Co-Authored-By: Claude GPT-5.6-Sol <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: 用测试锁定串口事件协议

**Files:**
- Create: `xtask/src/protocol.rs`
- Modify: `xtask/src/main.rs`

**Interfaces:**
- Consumes: 原始 QEMU stdout `&str`。
- Produces: `protocol::validate_output(output: &str) -> Result<(), String>`；只接受完整成功序列，忽略非 `PRIMA|` 行，拒绝非法协议行和 panic。

- [ ] **Step 1: 在 `xtask/src/main.rs` 声明协议模块**

在文件开头改为：

```rust
mod image;
mod protocol;
```

其余内容保持不变。

- [ ] **Step 2: 写入协议解析器的失败测试**

Create `xtask/src/protocol.rs`，先只写测试和空实现：

```rust
pub(crate) fn validate_output(_output: &str) -> Result<(), String> {
    Err("protocol validator is not implemented".to_owned())
}

#[cfg(test)]
mod tests {
    use super::validate_output;

    const VALID: &str = concat!(
        "SeaBIOS boot text\r\n",
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
        "PRIMA|v=1|seq=3|event=boot_complete|status=ok\r\n",
    );

    #[test]
    fn accepts_expected_sequence_and_ignores_firmware_output() {
        assert_eq!(validate_output(VALID), Ok(()));
    }

    #[test]
    fn rejects_missing_event() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=3|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_out_of_order_event() {
        let output = concat!(
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=3|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_duplicate_event() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=3|event=boot_complete|status=ok\n"
        );
        assert!(validate_output(&output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_malformed_prima_line() {
        let error = validate_output("PRIMA|v=1|seq=1|event=boot_start\n").unwrap_err();
        assert!(error.contains("five fields"));
    }

    #[test]
    fn rejects_unsupported_version() {
        let error = validate_output("PRIMA|v=2|seq=1|event=boot_start|status=ok\n").unwrap_err();
        assert!(error.contains("version"));
    }

    #[test]
    fn rejects_invalid_status() {
        let error = validate_output("PRIMA|v=1|seq=1|event=boot_start|status=error\n")
            .unwrap_err();
        assert!(error.contains("event/status"));
    }

    #[test]
    fn rejects_invalid_sequence_number() {
        let error = validate_output("PRIMA|v=1|seq=x|event=boot_start|status=ok\n")
            .unwrap_err();
        assert!(error.contains("sequence number"));
    }

    #[test]
    fn rejects_panic_event() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=4|event=panic|status=error\n"
        );
        assert!(validate_output(&output).unwrap_err().contains("panic"));
    }
}
```

- [ ] **Step 3: 运行协议测试并确认失败**

Run：

```bash
cargo test -p xtask protocol::tests
```

Expected：9 tests 中 `accepts_expected_sequence_and_ignores_firmware_output` 失败，因为空实现返回 `protocol validator is not implemented`。

- [ ] **Step 4: 实现严格协议解析与序列校验**

Replace `xtask/src/protocol.rs` 中测试模块之前的空实现 with：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    BootStart,
    SerialReady,
    BootComplete,
    Panic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Event {
    sequence: u32,
    kind: EventKind,
}

const EXPECTED_EVENTS: [Event; 3] = [
    Event {
        sequence: 1,
        kind: EventKind::BootStart,
    },
    Event {
        sequence: 2,
        kind: EventKind::SerialReady,
    },
    Event {
        sequence: 3,
        kind: EventKind::BootComplete,
    },
];

pub(crate) fn validate_output(output: &str) -> Result<(), String> {
    let mut events = Vec::new();

    for raw_line in output.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.starts_with("PRIMA|") {
            events.push(parse_line(line)?);
        }
    }

    if events
        .iter()
        .any(|event| event.kind == EventKind::Panic)
    {
        return Err("kernel emitted panic event".to_owned());
    }

    if events.as_slice() != EXPECTED_EVENTS.as_slice() {
        return Err(format!(
            "unexpected PRIMA event sequence: expected {EXPECTED_EVENTS:?}, got {events:?}"
        ));
    }

    Ok(())
}

fn parse_line(line: &str) -> Result<Event, String> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() != 5 {
        return Err(format!(
            "PRIMA line must contain five fields, got {}: {line}",
            fields.len()
        ));
    }
    if fields[0] != "PRIMA" {
        return Err(format!("invalid PRIMA prefix: {line}"));
    }
    if fields[1] != "v=1" {
        return Err(format!("unsupported PRIMA protocol version: {}", fields[1]));
    }

    let sequence = fields[2]
        .strip_prefix("seq=")
        .ok_or_else(|| format!("invalid sequence field: {}", fields[2]))?
        .parse::<u32>()
        .map_err(|error| format!("invalid sequence number in {}: {error}", fields[2]))?;
    let event = fields[3]
        .strip_prefix("event=")
        .ok_or_else(|| format!("invalid event field: {}", fields[3]))?;
    let status = fields[4]
        .strip_prefix("status=")
        .ok_or_else(|| format!("invalid status field: {}", fields[4]))?;

    let kind = match (event, status) {
        ("boot_start", "ok") => EventKind::BootStart,
        ("serial_ready", "ok") => EventKind::SerialReady,
        ("boot_complete", "ok") => EventKind::BootComplete,
        ("panic", "error") => EventKind::Panic,
        _ => return Err(format!("unsupported event/status pair: {event}/{status}")),
    };

    Ok(Event { sequence, kind })
}
```

保留 Step 2 的完整 `#[cfg(test)] mod tests` 不变。

- [ ] **Step 5: 运行协议测试并确认通过**

Run：

```bash
cargo fmt --all && cargo test -p xtask protocol::tests
```

Expected：`9 passed; 0 failed`。

- [ ] **Step 6: 提交协议验证器**

```bash
git add -- xtask/src/main.rs xtask/src/protocol.rs
git commit -m "$(cat <<'EOF'
test: lock boot event protocol

Co-Authored-By: Claude GPT-5.6-Sol <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: 实现固定 QEMU 命令与安全超时

**Files:**
- Create: `xtask/src/qemu.rs`
- Modify: `xtask/src/main.rs`

**Interfaces:**
- Consumes: `image::Images::bios` 与 `protocol::validate_output(&str)`。
- Produces: `qemu::run_bios_interactive(image: &Path) -> Result<(), String>` 和 `qemu::run_bios_test(image: &Path) -> Result<(), String>`。
- Constants: 成功退出码 `33`，panic 退出码 `35`，测试超时 10 秒。

- [ ] **Step 1: 在 `xtask/src/main.rs` 声明 QEMU 模块**

文件开头改为：

```rust
mod image;
mod protocol;
mod qemu;
```

其余内容保持不变。

- [ ] **Step 2: 写入 QEMU 参数与退出码的失败测试**

Create `xtask/src/qemu.rs`：

```rust
use std::{ffi::OsString, path::Path, time::Duration};

const SUCCESS_EXIT_CODE: i32 = 33;
const PANIC_EXIT_CODE: i32 = 35;
const TEST_TIMEOUT: Duration = Duration::from_secs(10);

fn qemu_args(_image: &Path) -> Vec<OsString> {
    Vec::new()
}

fn validate_exit_code(_actual: Option<i32>, _expected: i32) -> Result<(), String> {
    Err("exit-code validator is not implemented".to_owned())
}

#[cfg(test)]
mod tests {
    use super::{qemu_args, validate_exit_code, PANIC_EXIT_CODE, SUCCESS_EXIT_CODE, TEST_TIMEOUT};
    use std::{path::Path, time::Duration};

    #[test]
    fn arguments_are_fixed_single_core_and_headless() {
        let image = Path::new("C:/prima/prima-os-bios.img");
        let actual: Vec<String> = qemu_args(image)
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            actual,
            vec![
                "-machine",
                "pc-i440fx-10.0",
                "-accel",
                "tcg,thread=single",
                "-cpu",
                "qemu64",
                "-smp",
                "1",
                "-m",
                "128M",
                "-display",
                "none",
                "-monitor",
                "none",
                "-serial",
                "stdio",
                "-no-reboot",
                "-no-shutdown",
                "-nic",
                "none",
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
                "-drive",
                "format=raw,readonly=on,file=C:/prima/prima-os-bios.img",
                "-boot",
                "order=c,strict=on",
            ]
        );
        assert_eq!(TEST_TIMEOUT, Duration::from_secs(10));
    }

    #[test]
    fn accepts_success_exit_code() {
        assert_eq!(validate_exit_code(Some(SUCCESS_EXIT_CODE), SUCCESS_EXIT_CODE), Ok(()));
    }

    #[test]
    fn rejects_panic_exit_code_when_success_is_expected() {
        let error = validate_exit_code(Some(PANIC_EXIT_CODE), SUCCESS_EXIT_CODE).unwrap_err();
        assert!(error.contains("expected 33"));
        assert!(error.contains("got 35"));
    }

    #[test]
    fn rejects_missing_process_exit_code() {
        let error = validate_exit_code(None, SUCCESS_EXIT_CODE).unwrap_err();
        assert!(error.contains("no numeric exit code"));
    }
}
```

- [ ] **Step 3: 运行 QEMU 单元测试并确认失败**

Run：

```bash
cargo test -p xtask qemu::tests
```

Expected：4 tests 中参数测试和成功退出码测试失败。

- [ ] **Step 4: 实现 QEMU 定位、固定参数、输出捕获和超时清理**

Replace `xtask/src/qemu.rs` with：

```rust
use crate::protocol;
use std::{
    env,
    ffi::OsString,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SUCCESS_EXIT_CODE: i32 = 33;
const PANIC_EXIT_CODE: i32 = 35;
const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const QEMU_INSTALL_COMMAND: &str = "winget install --id SoftwareFreedomConservancy.QEMU --exact --version 11.0.50";

struct CapturedOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
}

pub(crate) fn run_bios_interactive(image: &Path) -> Result<(), String> {
    let qemu = locate_qemu()?;
    let status = Command::new(&qemu)
        .args(qemu_args(image))
        .status()
        .map_err(|error| format!("failed to start {}: {error}", qemu.display()))?;
    validate_exit_code(status.code(), SUCCESS_EXIT_CODE)
}

pub(crate) fn run_bios_test(image: &Path) -> Result<(), String> {
    let output = run_captured(image)?;
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_output(&output.stdout).map_err(|error| with_output(error, &output))
}

fn run_captured(image: &Path) -> Result<CapturedOutput, String> {
    let qemu = locate_qemu()?;
    let mut child = Command::new(&qemu)
        .args(qemu_args(image))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", qemu.display()))?;

    let stdout = child
        .stdout
        .take()
        .expect("stdout is piped when the child is created");
    let stderr = child
        .stderr
        .take()
        .expect("stderr is piped when the child is created");
    let stdout_reader = read_in_background(stdout);
    let stderr_reader = read_in_background(stderr);
    let started = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < TEST_TIMEOUT => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let cleanup = terminate_child(&mut child);
                let stdout = join_reader(stdout_reader, "stdout")?;
                let stderr = join_reader(stderr_reader, "stderr")?;
                return Err(format!(
                    "QEMU timed out after {}s; {cleanup}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    TEST_TIMEOUT.as_secs()
                ));
            }
            Err(error) => {
                let cleanup = terminate_child(&mut child);
                let stdout = join_reader(stdout_reader, "stdout")?;
                let stderr = join_reader(stderr_reader, "stderr")?;
                return Err(format!(
                    "failed while waiting for QEMU: {error}; {cleanup}\nstdout:\n{stdout}\nstderr:\n{stderr}"
                ));
            }
        }
    };

    Ok(CapturedOutput {
        status,
        stdout: join_reader(stdout_reader, "stdout")?,
        stderr: join_reader(stderr_reader, "stderr")?,
    })
}

fn locate_qemu() -> Result<PathBuf, String> {
    let executable = if cfg!(windows) {
        "qemu-system-x86_64.exe"
    } else {
        "qemu-system-x86_64"
    };

    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            let candidate = directory.join(executable);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    #[cfg(windows)]
    {
        let winget_path = PathBuf::from(r"C:\Program Files\qemu\qemu-system-x86_64.exe");
        if winget_path.is_file() {
            return Ok(winget_path);
        }
    }

    Err(format!(
        "qemu-system-x86_64 not found; install it with: {QEMU_INSTALL_COMMAND}"
    ))
}

fn qemu_args(image: &Path) -> Vec<OsString> {
    vec![
        "-machine".into(),
        "pc-i440fx-10.0".into(),
        "-accel".into(),
        "tcg,thread=single".into(),
        "-cpu".into(),
        "qemu64".into(),
        "-smp".into(),
        "1".into(),
        "-m".into(),
        "128M".into(),
        "-display".into(),
        "none".into(),
        "-monitor".into(),
        "none".into(),
        "-serial".into(),
        "stdio".into(),
        "-no-reboot".into(),
        "-no-shutdown".into(),
        "-nic".into(),
        "none".into(),
        "-device".into(),
        "isa-debug-exit,iobase=0xf4,iosize=0x04".into(),
        "-drive".into(),
        format!("format=raw,readonly=on,file={}", image.display()).into(),
        "-boot".into(),
        "order=c,strict=on".into(),
    ]
}

fn validate_exit_code(actual: Option<i32>, expected: i32) -> Result<(), String> {
    match actual {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!(
            "unexpected QEMU exit code: expected {expected}, got {actual}; panic exit code is {PANIC_EXIT_CODE}"
        )),
        None => Err(format!(
            "QEMU returned no numeric exit code; expected {expected}"
        )),
    }
}

fn read_in_background<R>(mut reader: R) -> JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn join_reader(
    reader: JoinHandle<io::Result<Vec<u8>>>,
    stream_name: &str,
) -> Result<String, String> {
    let bytes = reader
        .join()
        .map_err(|_| format!("{stream_name} reader thread panicked"))?
        .map_err(|error| format!("failed to read QEMU {stream_name}: {error}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn terminate_child(child: &mut Child) -> String {
    let kill_result = child.kill();
    let wait_result = child.wait();
    match (kill_result, wait_result) {
        (Ok(()), Ok(_)) => "QEMU child terminated".to_owned(),
        (kill, wait) => format!("QEMU cleanup result: kill={kill:?}, wait={wait:?}"),
    }
}

fn with_output(error: String, output: &CapturedOutput) -> String {
    format!(
        "{error}\nstdout:\n{}\nstderr:\n{}",
        output.stdout, output.stderr
    )
}

#[cfg(test)]
mod tests {
    use super::{qemu_args, validate_exit_code, PANIC_EXIT_CODE, SUCCESS_EXIT_CODE, TEST_TIMEOUT};
    use std::{path::Path, time::Duration};

    #[test]
    fn arguments_are_fixed_single_core_and_headless() {
        let image = Path::new("C:/prima/prima-os-bios.img");
        let actual: Vec<String> = qemu_args(image)
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            actual,
            vec![
                "-machine",
                "pc-i440fx-10.0",
                "-accel",
                "tcg,thread=single",
                "-cpu",
                "qemu64",
                "-smp",
                "1",
                "-m",
                "128M",
                "-display",
                "none",
                "-monitor",
                "none",
                "-serial",
                "stdio",
                "-no-reboot",
                "-no-shutdown",
                "-nic",
                "none",
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
                "-drive",
                "format=raw,readonly=on,file=C:/prima/prima-os-bios.img",
                "-boot",
                "order=c,strict=on",
            ]
        );
        assert_eq!(TEST_TIMEOUT, Duration::from_secs(10));
    }

    #[test]
    fn accepts_success_exit_code() {
        assert_eq!(validate_exit_code(Some(SUCCESS_EXIT_CODE), SUCCESS_EXIT_CODE), Ok(()));
    }

    #[test]
    fn rejects_panic_exit_code_when_success_is_expected() {
        let error = validate_exit_code(Some(PANIC_EXIT_CODE), SUCCESS_EXIT_CODE).unwrap_err();
        assert!(error.contains("expected 33"));
        assert!(error.contains("got 35"));
    }

    #[test]
    fn rejects_missing_process_exit_code() {
        let error = validate_exit_code(None, SUCCESS_EXIT_CODE).unwrap_err();
        assert!(error.contains("no numeric exit code"));
    }
}
```

- [ ] **Step 5: 运行 QEMU 单元测试并确认通过**

Run：

```bash
cargo fmt --all && cargo test -p xtask qemu::tests
```

Expected：`4 passed; 0 failed`。

- [ ] **Step 6: 提交 QEMU runner**

```bash
git add -- xtask/src/main.rs xtask/src/qemu.rs
git commit -m "$(cat <<'EOF'
feat: add deterministic QEMU runner

Co-Authored-By: Claude GPT-5.6-Sol <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: 用 BIOS 集成测试驱动 kernel 串口与退出实现

**Files:**
- Modify: `xtask/src/main.rs`
- Modify: `kernel/src/main.rs`
- Create: `kernel/src/serial.rs`
- Create: `kernel/src/protocol.rs`
- Create: `kernel/src/qemu.rs`

**Interfaces:**
- Consumes: `image::build_images()`、`qemu::run_bios_interactive()`、`qemu::run_bios_test()`。
- Produces: CLI `cargo xtask build|run|test`；kernel 依次输出三个协议事件并调用 `qemu::exit(qemu::SUCCESS)`，panic 输出失败事件并调用 `qemu::exit(qemu::PANIC)`。

- [ ] **Step 1: 接通完整 CLI，但保留尚不会退出的 kernel**

Replace `xtask/src/main.rs` with：

```rust
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
```

- [ ] **Step 2: 运行端到端测试并确认它以受控超时失败**

Run：

```bash
cargo fmt --all && cargo xtask test
```

Expected：命令在第一个 BIOS run 后返回非零，错误包含：

```text
QEMU timed out after 10s
```

QEMU 子进程必须已被回收；不得手工杀死其他 QEMU 进程。失败证明测试能观察到当前 kernel 缺少串口与退出协议。

- [ ] **Step 3: 实现最小 COM1 驱动**

Create `kernel/src/serial.rs`：

```rust
use core::{
    arch::asm,
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

const COM1: u16 = 0x3f8;
const LINE_STATUS: u16 = COM1 + 5;
const TRANSMITTER_EMPTY: u8 = 1 << 5;

static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) fn init() {
    if INITIALIZED.load(Ordering::Acquire) {
        return;
    }

    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x03);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xc7);
        outb(COM1 + 4, 0x0b);
    }

    INITIALIZED.store(true, Ordering::Release);
}

pub(crate) fn write_bytes(bytes: &[u8]) {
    init();
    for &byte in bytes {
        while unsafe { inb(LINE_STATUS) } & TRANSMITTER_EMPTY == 0 {
            spin_loop();
        }
        unsafe {
            outb(COM1, byte);
        }
    }
}

unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}
```

- [ ] **Step 4: 实现固定事件编码和单调序号**

Create `kernel/src/protocol.rs`：

```rust
use crate::serial;
use core::sync::atomic::{AtomicU32, Ordering};

static NEXT_SEQUENCE: AtomicU32 = AtomicU32::new(1);

pub(crate) fn boot_start() {
    write_event("boot_start", "ok");
}

pub(crate) fn serial_ready() {
    write_event("serial_ready", "ok");
}

pub(crate) fn boot_complete() {
    write_event("boot_complete", "ok");
}

pub(crate) fn panic_event() {
    write_event("panic", "error");
}

fn write_event(event: &str, status: &str) {
    let sequence = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    serial::write_bytes(b"PRIMA|v=1|seq=");
    write_u32(sequence);
    serial::write_bytes(b"|event=");
    serial::write_bytes(event.as_bytes());
    serial::write_bytes(b"|status=");
    serial::write_bytes(status.as_bytes());
    serial::write_bytes(b"\r\n");
}

fn write_u32(mut value: u32) {
    let mut buffer = [0_u8; 10];
    let mut index = buffer.len();

    loop {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    serial::write_bytes(&buffer[index..]);
}
```

- [ ] **Step 5: 实现 QEMU 退出端口和停机后备路径**

Create `kernel/src/qemu.rs`：

```rust
use core::arch::asm;

const EXIT_PORT: u16 = 0xf4;
pub(crate) const SUCCESS: u8 = 0x10;
pub(crate) const PANIC: u8 = 0x11;

pub(crate) fn exit(value: u8) -> ! {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") EXIT_PORT,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }

    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}
```

- [ ] **Step 6: 接通 kernel 正常与 panic 路径**

Replace `kernel/src/main.rs` with：

```rust
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod protocol;
mod qemu;
mod serial;

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    protocol::boot_start();
    protocol::serial_ready();
    protocol::boot_complete();
    qemu::exit(qemu::SUCCESS)
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::init();
    protocol::panic_event();
    qemu::exit(qemu::PANIC)
}
```

- [ ] **Step 7: 运行单元测试和唯一端到端验收**

Run：

```bash
cargo fmt --all && cargo test -p xtask
```

Expected：`13 passed; 0 failed`。

Run：

```bash
cargo xtask test
```

Expected tail，且顺序完全一致：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

任何 `ERROR`、panic、协议非法、退出码不符或超时都必须使命令返回非零。

- [ ] **Step 8: 提交完整启动闭环**

```bash
git add -- kernel/src/main.rs kernel/src/serial.rs kernel/src/protocol.rs kernel/src/qemu.rs xtask/src/main.rs
git commit -m "$(cat <<'EOF'
feat: complete BIOS boot test loop

Co-Authored-By: Claude GPT-5.6-Sol <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: 完整验证并登记阶段证据

**Files:**
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Consumes: 实际通过的 `cargo xtask test` 输出，以及固定工具链 `nightly-2026-07-30`、QEMU `11.0.50`。
- Produces: 证据 `IMPL-001`、`TEST-001`；更新 S1-D1、S1-D2、S1-X1，同时保持阶段 0 与所有未实现边界不变。

- [ ] **Step 1: 运行完整静态与单元验证**

Run：

```bash
cargo fmt --all --check && cargo clippy --workspace --all-targets --no-deps -- -D warnings && cargo test -p xtask
```

Expected：所有命令退出码为 0；xtask 测试结果为 `13 passed; 0 failed`。

- [ ] **Step 2: 重新运行唯一功能验收并保留真实结果**

Run：

```bash
cargo xtask test
```

Expected tail：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

只有获得该真实输出后才执行后续证据编辑。

- [ ] **Step 3: 更新跟踪文档头部与当前快照**

在 `Prima OS 实现阶段.md` 中作以下精确修改：

```markdown
> 更新日期：2026-08-04
```

将当前快照中的相关行改为：

```markdown
| 当前阶段 | 阶段 0 — 语义基线 |
| 当前状态 | 进行中 |
| 已满足退出条件的最高阶段 | 无 |
| 当前实现代码 | 阶段 1 启动闭环探索实现：`kernel/`、`xtask/` |
| 当前主要工件 | [[Prima OS]]、[[COSIR 原语规范]]、Rust/QEMU 启动闭环 |
| 当前首要缺口 | x86_64 机器状态到 POSIR 状态的细化草案；形式化工具中的可执行状态模型；异常帧格式与 `exc_resume` 原语（COSIR 细化层缺口） |
| 最近规范修订 | 2026-07-31 — 规范审查修复：ObjectRef 统一、ExternalInterrupt 语义明确、CapMoved 审计事件补充、Node 对象完整定义、DMA 帧/页帧信息流说明、POSIR/COSIR 边界清单、ns_mount 跨节点约束、task_create 增加 kind 参数、task_yield 调度时机明确、应用域/LibOS 关系精确化、细化要求从 21 项扩展到 25 项 |
| 最近实现证据 | 2026-08-04 — `cargo xtask test` 在 QEMU 11.0.50 上连续完成两次固定 BIOS 启动；同时生成非空 BIOS/UEFI 镜像 |
```

将快照后的说明改为：

```markdown
当前已形成架构白皮书和抽象语义规范，并有阶段 1 启动闭环探索实现；尚无机器证明、物理内存发现、页表与 trap 框架、POSIR Bootstrap 或兼容性测试证据。
```

- [ ] **Step 4: 增加实现和测试证据**

在证据索引的 `SPEC-001` 行后增加：

```markdown
| IMPL-001 | 实现工件 | `Cargo.toml`、`kernel/`、`xtask/`、`Cargo.lock` | 2026-08-04 | 固定 nightly-2026-07-30 与 bootloader 0.11.17；生成 BIOS/UEFI 镜像 | S1-D1、S1-D2 |
| TEST-001 | QEMU 集成测试 | `cargo xtask test` | 2026-08-04 | QEMU 11.0.50；固定 BIOS profile 连续启动两次，结构化串口序列和退出码均通过 | S1-D1、S1-D2、S1-X1 |
```

不得关闭 `BLK-001` 或 `BLK-002`。

- [ ] **Step 5: 更新阶段 1 状态与检查项**

将阶段 1 开头及检查项改为：

```markdown
## 阶段 1 — 启动与最小平台层

**状态：进行中**

### 交付检查

- [x] S1-D1：QEMU 可重复启动；证据：TEST-001（固定 BIOS profile 连续两次）。
- [x] S1-D2：串口结构化输出与确定性退出；证据：IMPL-001、TEST-001。
- [ ] S1-D3：物理内存发现；证据：页帧清单与保留区域测试。
- [ ] S1-D4：初始页表、GDT/IDT 与 trap 框架；证据：异常和权限测试。
- [ ] S1-D5：Bootstrap 创建 init Task；证据：初始对象和 Capability 快照。

### 退出条件

- [x] S1-X1：脚本可无人值守启动并退出 QEMU；证据：TEST-001。
- [ ] S1-X2：init 只获得规范定义的初始 Authority。
- [ ] S1-X3：活动 Segment 与空闲页帧满足唯一归属不变量。
```

不得修改阶段 0 的退出条件，也不得勾选 S1-D3、S1-D4、S1-D5、S1-X2 或 S1-X3。

- [ ] **Step 6: 校验证据边界没有虚报**

Run：

```bash
python -c 'from pathlib import Path; s=Path("Prima OS 实现阶段.md").read_text(encoding="utf-8"); required=["| 当前阶段 | 阶段 0 — 语义基线 |","| 已满足退出条件的最高阶段 | 无 |","IMPL-001","TEST-001","- [x] S1-D1","- [x] S1-D2","- [x] S1-X1","- [ ] S1-D3","- [ ] S1-D4","- [ ] S1-D5","- [ ] S1-X2","- [ ] S1-X3","| BLK-001 |","| BLK-002 |"]; missing=[x for x in required if x not in s]; assert not missing, missing; stage1=s.split("## 阶段 1 —",1)[1].split("## 阶段 2 —",1)[0]; assert "**状态：进行中**" in stage1; assert "**状态：已满足退出条件**" not in stage1; assert "| BLK-001 | 0 |" in s and "| BLK-002 | 0 |" in s; print("PASS evidence boundaries")'
```

Expected：`PASS evidence boundaries`。

- [ ] **Step 7: 检查差异并提交证据**

Run：

```bash
git diff --check && git status --short
```

Expected：`git diff --check` 无输出；状态中仅有本任务预期的 `Prima OS 实现阶段.md`，以及始终未跟踪且不提交的 `.obsidian/`。

Commit：

```bash
git add -- "Prima OS 实现阶段.md"
git commit -m "$(cat <<'EOF'
docs: record stage 1 boot evidence

Co-Authored-By: Claude GPT-5.6-Sol <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 8: 最终验证提交后的仓库和功能**

Run：

```bash
git status --short && cargo xtask test
```

Expected：Git 只显示既有未跟踪 `.obsidian/`；功能验收再次输出六条规定的 PASS 摘要。不得把 `.obsidian/` 加入任何提交。

---

## Plan Self-Review

- **Spec coverage:** Task 1 覆盖固定工具链、Cargo workspace、artifact dependency 和双镜像；Task 2 覆盖版本化事件协议；Task 3 覆盖 QEMU 参数、退出码、超时和进程清理；Task 4 覆盖 COM1、panic、连续两次 BIOS 验收及统一 CLI；Task 5 覆盖静态验证与证据边界。
- **Scope:** 计划只实现启动闭环，没有引入物理内存、页表、GDT/IDT、trap、Bootstrap、Capability、CI、Linux 主机验证或 UEFI 运行验证。
- **Type consistency:** `image::Images::bios` 从 Task 1 传给 Task 4；`protocol::validate_output` 从 Task 2 传给 Task 3；`qemu::run_bios_test` 和 `run_bios_interactive` 的签名在 Tasks 3–4 一致。
- **Protocol consistency:** kernel 输出字段、顺序、序号、成功值 `0x10` 和 panic 值 `0x11` 与 xtask 验证器及设计规范一致。
- **Evidence boundary:** 只更新 S1-D1、S1-D2、S1-X1；阶段 0、两个阻塞项和阶段 1 未实现检查项保持原状态。
- **Placeholder scan:** 所有代码、命令、版本、路径、预期结果和文档替换均已明确，无未决实现槽位。
