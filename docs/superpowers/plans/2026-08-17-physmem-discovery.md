# Physical Memory Discovery (S1-D3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在真实 `kernel/` 内核中解析 bootloader `BootInfo.memory_regions`，构建 4KB 物理帧清单（Usable 帧 + 内核镜像保留帧 + free），并以 host 单测 + QEMU boot-trace 集成断言双证据收尾。

**Architecture:** 把 `kernel` crate 拆成 lib（安全纯逻辑 `mem` 模块，可 host 单测）+ bin（薄入口 `main.rs` + 串口协议）。纯函数 `mem::build_inventory` 只统计 `Usable` 区、保留 `[kernel_addr, kernel_addr+kernel_len)` 镜像帧；内核经串口协议新增的 `memory_ready` 事件上报计数，`xtask` 校验器解析并断言结构不变式。

**Tech Stack:** Rust nightly-2026-07-30、edition 2024、`bootloader_api = "=0.11.17"`（无新依赖）、QEMU 11.0.50（isa-debug-exit 退出码 33/35）。

## Global Constraints

- 工具链固定 `nightly-2026-07-30`（见 `rust-toolchain.toml`），组件含 `rustfmt`/`clippy`。
- 每个 task 完成后必须通过 `cargo fmt --all --check`（clippy 不检查格式）。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings` 零告警。
- kernel crate：`#![no_std]`、`#![no_main]`、`#![deny(unsafe_op_in_unsafe_fn)]`；本切片不新增任何 `unsafe`。
- 不新增依赖；`bootloader_api` 保持精确版本 `=0.11.17`。
- 证据措辞纪律：QEMU 断言只写结构不变式，不硬编码机器相关帧计数；不宣称帧分配器/页表/物理内存读写/`KERNEL-TRACE`(COW)/`REFINEMENT-PROOF`。
- lib 名 `kernel`、bin 名 `prima-kernel`（不得改动 `image.rs` 引用的 `CARGO_BIN_FILE_KERNEL_prima-kernel`）。

---

### Task 1: 拆分 kernel 为 lib + bin 骨架

**Files:**
- Modify: `kernel/Cargo.toml`
- Create: `kernel/src/lib.rs`
- Create: `kernel/src/mem.rs`

**Interfaces:**
- Produces: lib crate `kernel`，模块 `kernel::mem`，常量 `kernel::mem::FRAME_SIZE: u64`。

- [ ] **Step 1: 在 Cargo.toml 增加 lib target**

把 `kernel/Cargo.toml` 改为（在 `[package]` 与 `[[bin]]` 之间插入 `[lib]`）：

```toml
[package]
name = "kernel"
version = "0.1.0"
edition = "2024"
publish = false

[lib]
name = "kernel"
path = "src/lib.rs"

[[bin]]
name = "prima-kernel"
path = "src/main.rs"
test = false
bench = false

[dependencies]
bootloader_api = "=0.11.17"
```

- [ ] **Step 2: 创建 lib.rs 与 mem.rs 骨架**

`kernel/src/lib.rs`：

```rust
#![cfg_attr(not(test), no_std)]

pub mod mem;
```

`kernel/src/mem.rs`：

```rust
pub const FRAME_SIZE: u64 = 4096;

#[cfg(test)]
mod tests {
    use super::FRAME_SIZE;

    #[test]
    fn frame_size_is_4kib() {
        assert_eq!(FRAME_SIZE, 4096);
    }
}
```

- [ ] **Step 3: 验证 lib 单测与 bin 无回归**

Run:

```bash
cargo test -p kernel
cargo build -p kernel
cargo xtask test
```

Expected:
- `cargo test -p kernel`：`frame_size_is_4kib` PASS（1 test）。
- `cargo build -p kernel`：lib + bin 均编译通过。
- `cargo xtask test`：六条 PASS（QEMU 两次 run 退出码 33，行为与拆分前一致）。

- [ ] **Step 4: fmt 并提交**

```bash
cargo fmt --all --check
git add kernel/Cargo.toml kernel/src/lib.rs kernel/src/mem.rs
git commit -m "refactor: split kernel crate into lib and bin targets"
```

---

### Task 2: 实现 `build_inventory` + 10 个 host 单测

**Files:**
- Modify: `kernel/src/mem.rs`

**Interfaces:**
- Consumes: `kernel::mem::FRAME_SIZE`（Task 1）。
- Produces:
  - `kernel::mem::FrameInventory { usable_frames: usize, image_frames: usize, free_frames: usize, usable_regions: usize, kernel_phys_start: u64, kernel_phys_end: u64 }`（`Debug, Clone, Copy, PartialEq, Eq`）。
  - `kernel::mem::MemError { EmptyImage, ImageOverlapsUsable, RegionsNotSortedOrOverlapping }`（`Debug, Clone, Copy, PartialEq, Eq`）。
  - `kernel::mem::build_inventory(regions: &[bootloader_api::info::MemoryRegion], kernel_addr: u64, kernel_len: u64) -> Result<FrameInventory, MemError>`。

- [ ] **Step 1: 追加失败测试（先红）**

把 `kernel/src/mem.rs` 末尾（`FRAME_SIZE` 常量之后）追加如下测试模块；此时 `build_inventory` / `FrameInventory` / `MemError` 尚未定义：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

    fn usable(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion { start, end, kind: MemoryRegionKind::Usable }
    }

    fn bootloader(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion { start, end, kind: MemoryRegionKind::Bootloader }
    }

    #[test]
    fn counts_frames_in_a_single_usable_region() {
        let regions = [usable(0x0, 0x1_0000), bootloader(0x100_0000, 0x100_1000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 16);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 16); // == usable，镜像不扣减
        assert_eq!(inv.usable_regions, 1);
        assert_eq!(inv.kernel_phys_start, 0x100_0000);
        assert_eq!(inv.kernel_phys_end, 0x100_1000);
    }

    #[test]
    fn drops_unaligned_region_boundary_pages() {
        let regions = [usable(0x0800, 0x5200), bootloader(0x100_0000, 0x100_1000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.free_frames, 4);
    }

    #[test]
    fn ignores_non_usable_regions() {
        let regions = [
            bootloader(0x0, 0x1000),
            usable(0x1000, 0x5000),
            MemoryRegion { start: 0x5000, end: 0x6000, kind: MemoryRegionKind::UnknownBios(2) },
            bootloader(0x100_0000, 0x100_1000),
        ];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.usable_regions, 1);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 4);
    }

    #[test]
    fn image_frames_reported_but_not_subtracted() {
        let regions = [usable(0x0, 0x1_0000), bootloader(0x100_0000, 0x100_2000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x2000).unwrap();
        assert_eq!(inv.image_frames, 2);
        assert_eq!(inv.free_frames, 16); // 镜像已从 Usable 排除，free == usable
    }

    #[test]
    fn rounds_unaligned_kernel_image_up_to_full_frames() {
        let regions = [usable(0x2000, 0x1_2000), bootloader(0x0, 0x2000)];
        let inv = build_inventory(&regions, 0x1800, 0x800).unwrap();
        assert_eq!(inv.kernel_phys_start, 0x1000);
        assert_eq!(inv.kernel_phys_end, 0x2000);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.usable_frames, 16);
        assert_eq!(inv.free_frames, 16);
    }

    #[test]
    fn rejects_empty_kernel_image() {
        let regions = [usable(0x0, 0x1_0000)];
        assert_eq!(build_inventory(&regions, 0x1000, 0), Err(MemError::EmptyImage));
    }

    #[test]
    fn rejects_image_overlapping_usable() {
        let regions = [usable(0x0, 0x5000)];
        assert_eq!(
            build_inventory(&regions, 0x1000, 0x1000),
            Err(MemError::ImageOverlapsUsable)
        );
    }

    #[test]
    fn rejects_overlapping_regions() {
        let regions = [usable(0x0, 0x3000), usable(0x2000, 0x5000)];
        assert_eq!(
            build_inventory(&regions, 0x1000, 0x1000),
            Err(MemError::RegionsNotSortedOrOverlapping)
        );
    }

    #[test]
    fn sums_multiple_usable_regions() {
        let regions = [
            usable(0x0, 0x2000),
            bootloader(0x2000, 0x3000),
            usable(0x3000, 0x7000),
            bootloader(0x100_0000, 0x100_1000),
        ];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 6);
        assert_eq!(inv.usable_regions, 2);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 6);
    }

    #[test]
    fn image_in_bootloader_does_not_reduce_free() {
        let regions = [
            usable(0x0, 0x3000),
            bootloader(0x4000, 0x5000), // 镜像落在该 Bootloader 区，不计入 usable
            usable(0x5000, 0x8000),
        ];
        let inv = build_inventory(&regions, 0x4000, 0x1000).unwrap();
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.usable_frames, 6);
        assert_eq!(inv.free_frames, 6);
    }
}
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test -p kernel`
Expected: FAIL（编译错误：`cannot find function 'build_inventory'` 等）。

- [ ] **Step 3: 用完整实现替换 `kernel/src/mem.rs`**

```rust
use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

pub const FRAME_SIZE: u64 = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInventory {
    pub usable_frames: usize,
    pub image_frames: usize,
    pub free_frames: usize,
    pub usable_regions: usize,
    pub kernel_phys_start: u64,
    pub kernel_phys_end: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemError {
    EmptyImage,
    ImageOverlapsUsable,
    RegionsNotSortedOrOverlapping,
}

pub fn build_inventory(
    regions: &[MemoryRegion],
    kernel_addr: u64,
    kernel_len: u64,
) -> Result<FrameInventory, MemError> {
    if kernel_len == 0 {
        return Err(MemError::EmptyImage);
    }

    let mut prev_end = 0u64;
    for region in regions {
        if region.start < prev_end {
            return Err(MemError::RegionsNotSortedOrOverlapping);
        }
        prev_end = region.end;
    }

    let mut usable_frames = 0usize;
    let mut usable_regions = 0usize;
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            usable_regions += 1;
            let lo = align_up(region.start, FRAME_SIZE);
            let hi = align_down(region.end, FRAME_SIZE);
            if hi > lo {
                usable_frames += ((hi - lo) / FRAME_SIZE) as usize;
            }
        }
    }

    let kernel_phys_start = align_down(kernel_addr, FRAME_SIZE);
    let kernel_phys_end = align_up(kernel_addr + kernel_len, FRAME_SIZE);
    let image_frames = ((kernel_phys_end - kernel_phys_start) / FRAME_SIZE) as usize;

    // 内核镜像由 bootloader 存放在 Bootloader 区（不在 Usable 内）。校验镜像与所有
    // Usable 帧不相交（W6）：若任一 frames 重叠，则该帧既被视为空闲又被镜像占用。
    for region in regions {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        let lo = align_up(region.start, FRAME_SIZE).max(kernel_phys_start);
        let hi = align_down(region.end, FRAME_SIZE).min(kernel_phys_end);
        if hi > lo {
            return Err(MemError::ImageOverlapsUsable);
        }
    }

    Ok(FrameInventory {
        usable_frames,
        image_frames,
        // 镜像已从 Usable 排除，Usable 帧即自由帧，不扣减镜像。
        free_frames: usable_frames,
        usable_regions,
        kernel_phys_start,
        kernel_phys_end,
    })
}

fn align_up(value: u64, align: u64) -> u64 {
    value.div_ceil(align) * align
}

fn align_down(value: u64, align: u64) -> u64 {
    value / align * align
}

#[cfg(test)]
mod tests {
    use super::*;
    use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

    fn usable(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion { start, end, kind: MemoryRegionKind::Usable }
    }

    fn bootloader(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion { start, end, kind: MemoryRegionKind::Bootloader }
    }

    #[test]
    fn counts_frames_in_a_single_usable_region() {
        let regions = [usable(0x0, 0x1_0000), bootloader(0x100_0000, 0x100_1000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 16);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 16); // == usable，镜像不扣减
        assert_eq!(inv.usable_regions, 1);
        assert_eq!(inv.kernel_phys_start, 0x100_0000);
        assert_eq!(inv.kernel_phys_end, 0x100_1000);
    }

    #[test]
    fn drops_unaligned_region_boundary_pages() {
        let regions = [usable(0x0800, 0x5200), bootloader(0x100_0000, 0x100_1000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.free_frames, 4);
    }

    #[test]
    fn ignores_non_usable_regions() {
        let regions = [
            bootloader(0x0, 0x1000),
            usable(0x1000, 0x5000),
            MemoryRegion { start: 0x5000, end: 0x6000, kind: MemoryRegionKind::UnknownBios(2) },
            bootloader(0x100_0000, 0x100_1000),
        ];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.usable_regions, 1);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 4);
    }

    #[test]
    fn image_frames_reported_but_not_subtracted() {
        let regions = [usable(0x0, 0x1_0000), bootloader(0x100_0000, 0x100_2000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x2000).unwrap();
        assert_eq!(inv.image_frames, 2);
        assert_eq!(inv.free_frames, 16); // 镜像已从 Usable 排除，free == usable
    }

    #[test]
    fn rounds_unaligned_kernel_image_up_to_full_frames() {
        let regions = [usable(0x2000, 0x1_2000), bootloader(0x0, 0x2000)];
        let inv = build_inventory(&regions, 0x1800, 0x800).unwrap();
        assert_eq!(inv.kernel_phys_start, 0x1000);
        assert_eq!(inv.kernel_phys_end, 0x2000);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.usable_frames, 16);
        assert_eq!(inv.free_frames, 16);
    }

    #[test]
    fn rejects_empty_kernel_image() {
        let regions = [usable(0x0, 0x1_0000)];
        assert_eq!(build_inventory(&regions, 0x1000, 0), Err(MemError::EmptyImage));
    }

    #[test]
    fn rejects_image_overlapping_usable() {
        let regions = [usable(0x0, 0x5000)];
        assert_eq!(
            build_inventory(&regions, 0x1000, 0x1000),
            Err(MemError::ImageOverlapsUsable)
        );
    }

    #[test]
    fn rejects_overlapping_regions() {
        let regions = [usable(0x0, 0x3000), usable(0x2000, 0x5000)];
        assert_eq!(
            build_inventory(&regions, 0x1000, 0x1000),
            Err(MemError::RegionsNotSortedOrOverlapping)
        );
    }

    #[test]
    fn sums_multiple_usable_regions() {
        let regions = [
            usable(0x0, 0x2000),
            bootloader(0x2000, 0x3000),
            usable(0x3000, 0x7000),
            bootloader(0x100_0000, 0x100_1000),
        ];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 6);
        assert_eq!(inv.usable_regions, 2);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 6);
    }

    #[test]
    fn image_in_bootloader_does_not_reduce_free() {
        let regions = [
            usable(0x0, 0x3000),
            bootloader(0x4000, 0x5000), // 镜像落在该 Bootloader 区，不计入 usable
            usable(0x5000, 0x8000),
        ];
        let inv = build_inventory(&regions, 0x4000, 0x1000).unwrap();
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.usable_frames, 6);
        assert_eq!(inv.free_frames, 6);
    }
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p kernel`
Expected: 10 tests PASS。

- [ ] **Step 5: fmt 并提交**

```bash
cargo fmt --all --check
git add kernel/src/mem.rs
git commit -m "feat: add physical memory frame inventory with host tests"
```

---

### Task 3: 协议 v2 —— 内核上报 `memory_ready`，xtask 校验

**Files:**
- Modify: `kernel/src/main.rs`
- Modify: `kernel/src/protocol.rs`
- Modify: `xtask/src/protocol.rs`（实现 + 测试）

**Interfaces:**
- Consumes: `kernel::mem::{build_inventory, FrameInventory, MemError}`（Task 2）。
- Produces:
  - 内核协议：`protocol::memory_ready(&FrameInventory)`、`protocol::memory_error(MemError)`。
  - 内核事件序列：`boot_start`(1) → `serial_ready`(2) → `memory_ready`(3) → `boot_complete`(4)；错误路径 `boot_start` → `serial_ready` → `memory_error`。
  - xtask：`validate_output` 校验 4 事件序列 + `memory_ready` data 不变式。

- [ ] **Step 1: 写 xtask 失败测试（先红）**

用下方 `xtask/src/protocol.rs` 完整文件（含新测试）替换现有文件；此时新事件枚举/`data` 解析尚未实现，测试会编译失败。

- [ ] **Step 2: 运行 xtask 测试确认失败**

Run: `cargo test -p xtask protocol`
Expected: FAIL（编译错误：`EventKind::MemoryReady` / `MemoryError` 未定义等）。

- [ ] **Step 3: 用协议 v2 实现替换 `xtask/src/protocol.rs`**

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    BootStart,
    SerialReady,
    MemoryReady,
    BootComplete,
    Panic,
    MemoryError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    sequence: u32,
    kind: EventKind,
    data: Option<String>,
}

const EXPECTED_EVENTS: [(u32, EventKind); 4] = [
    (1, EventKind::BootStart),
    (2, EventKind::SerialReady),
    (3, EventKind::MemoryReady),
    (4, EventKind::BootComplete),
];

pub(crate) fn validate_output(output: &str) -> Result<(), String> {
    let mut events = Vec::new();

    for raw_line in output.split_inclusive('\n') {
        let Some(line) = raw_line.strip_suffix('\n') else {
            if raw_line.starts_with("PRIMA|") {
                return Err("PRIMA record must end with LF or CRLF".to_owned());
            }
            continue;
        };
        if line.starts_with("PRIMA|") {
            let line = match line.strip_suffix('\r') {
                Some(line) if line.ends_with('\r') => {
                    return Err("PRIMA record must end with LF or CRLF".to_owned());
                }
                Some(line) => line,
                None => line,
            };
            events.push(parse_line(line)?);
        }
    }

    if events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Panic | EventKind::MemoryError))
    {
        return Err("kernel emitted panic or memory_error event".to_owned());
    }

    let observed: Vec<(u32, EventKind)> = events
        .iter()
        .map(|event| (event.sequence, event.kind))
        .collect();
    if observed != EXPECTED_EVENTS {
        return Err(format!(
            "unexpected PRIMA event sequence: expected {EXPECTED_EVENTS:?}, got {observed:?}"
        ));
    }

    let memory = events
        .iter()
        .find(|event| event.kind == EventKind::MemoryReady)
        .expect("memory_ready is in the expected sequence");
    validate_memory_data(memory.data.as_deref())
}

fn validate_memory_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "memory_ready event is missing its data field".to_owned())?;

    let mut usable = None;
    let mut image = None;
    let mut free = None;
    let mut kstart = None;
    let mut kend = None;

    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "usable" => usable = Some(parse_count(value)?),
            "image" => image = Some(parse_count(value)?),
            "free" => free = Some(parse_count(value)?),
            "kstart" => kstart = Some(parse_addr(value)?),
            "kend" => kend = Some(parse_addr(value)?),
            "regions" => parse_count(value)?,
            _ => return Err(format!("unknown data key: {key}")),
        }
    }

    let usable = usable.ok_or_else(|| "missing usable in memory_ready data".to_owned())?;
    let image = image.ok_or_else(|| "missing image in memory_ready data".to_owned())?;
    let free = free.ok_or_else(|| "missing free in memory_ready data".to_owned())?;
    let kstart = kstart.ok_or_else(|| "missing kstart in memory_ready data".to_owned())?;
    let kend = kend.ok_or_else(|| "missing kend in memory_ready data".to_owned())?;

    if image == 0 {
        return Err("image frame count must be > 0".to_owned());
    }
    if free == 0 {
        return Err("free frame count must be > 0".to_owned());
    }
    if free != usable {
        return Err(format!(
            "free ({free}) must equal usable ({usable}) because the kernel image is carried by the bootloader outside Usable"
        ));
    }
    if kstart % 4096 != 0 || kend % 4096 != 0 {
        return Err(format!(
            "kernel image addresses must be page-aligned: kstart=0x{kstart:x}, kend=0x{kend:x}"
        ));
    }
    if kstart >= kend {
        return Err(format!(
            "kernel image start (0x{kstart:x}) must be below end (0x{kend:x})"
        ));
    }

    Ok(())
}

fn parse_count(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid count {value}: {error}"))
}

fn parse_addr(value: &str) -> Result<u64, String> {
    let hex = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("invalid address (missing 0x prefix): {value}"))?;
    u64::from_str_radix(hex, 16).map_err(|error| format!("invalid address {value}: {error}"))
}

fn parse_line(line: &str) -> Result<Event, String> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() != 5 && fields.len() != 6 {
        return Err(format!(
            "PRIMA line must contain five or six fields, got {}: {line}",
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
    let data = if fields.len() == 6 {
        Some(
            fields[5]
                .strip_prefix("data=")
                .ok_or_else(|| format!("invalid data field: {}", fields[5]))?
                .to_owned(),
        )
    } else {
        None
    };

    let kind = match (event, status) {
        ("boot_start", "ok") => EventKind::BootStart,
        ("serial_ready", "ok") => EventKind::SerialReady,
        ("memory_ready", "ok") => EventKind::MemoryReady,
        ("boot_complete", "ok") => EventKind::BootComplete,
        ("memory_error", "error") => EventKind::MemoryError,
        ("panic", "error") => EventKind::Panic,
        _ => return Err(format!("unsupported event/status pair: {event}/{status}")),
    };

    match (kind, &data) {
        (EventKind::MemoryReady | EventKind::MemoryError, None) => {
            return Err(format!("{event} event is missing its data field"));
        }
        (EventKind::MemoryReady | EventKind::MemoryError, Some(_)) => {}
        (_, Some(_)) => return Err(format!("{event} event must not carry a data field")),
        (_, None) => {}
    }

    Ok(Event { sequence, kind, data })
}

#[cfg(test)]
mod tests {
    use super::validate_output;

    const VALID: &str = concat!(
        "SeaBIOS boot text\r\n",
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
        "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\r\n",
        "PRIMA|v=1|seq=4|event=boot_complete|status=ok\r\n",
    );

    #[test]
    fn accepts_expected_sequence_and_ignores_firmware_output() {
        assert_eq!(validate_output(VALID), Ok(()));
    }

    #[test]
    fn accepts_unterminated_non_prima_firmware_tail() {
        let output = format!("{VALID}firmware tail without a line terminator");
        assert_eq!(validate_output(&output), Ok(()));
    }

    #[test]
    fn rejects_prima_record_without_line_feed_terminator() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("LF or CRLF"), "{error}");
    }

    #[test]
    fn rejects_prima_record_with_cr_only_terminator() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\r",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("LF or CRLF"), "{error}");
    }

    #[test]
    fn rejects_prima_record_with_extra_cr_before_line_feed() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\r\r\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("LF or CRLF"), "{error}");
    }

    #[test]
    fn rejects_missing_event() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_out_of_order_event() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_duplicate_event() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n"
        );
        assert!(validate_output(&output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_malformed_prima_line() {
        let error = validate_output("PRIMA|v=1|seq=1|event=boot_start\n").unwrap_err();
        assert!(error.contains("five or six fields"), "{error}");
    }

    #[test]
    fn rejects_unsupported_version() {
        let error =
            validate_output("PRIMA|v=2|seq=1|event=boot_start|status=ok\n").unwrap_err();
        assert!(error.contains("version"), "{error}");
    }

    #[test]
    fn rejects_invalid_status() {
        let error =
            validate_output("PRIMA|v=1|seq=1|event=boot_start|status=error\n").unwrap_err();
        assert!(error.contains("event/status"), "{error}");
    }

    #[test]
    fn rejects_invalid_sequence_number() {
        let error = validate_output("PRIMA|v=1|seq=x|event=boot_start|status=ok\n").unwrap_err();
        assert!(error.contains("sequence number"), "{error}");
    }

    #[test]
    fn rejects_panic_event() {
        let output = format!("{}{}", VALID, "PRIMA|v=1|seq=5|event=panic|status=error\n");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("panic"), "{error}");
    }

    #[test]
    fn rejects_memory_error_event() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=5|event=memory_error|status=error|data=err=EmptyImage\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("memory_error"), "{error}");
    }

    #[test]
    fn rejects_memory_ready_missing_data() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("missing its data field"), "{error}");
    }

    #[test]
    fn rejects_memory_ready_inconsistent_counts() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=5;free=10;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must equal"), "{error}");
    }

    #[test]
    fn rejects_memory_ready_unaligned_address() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x1;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("page-aligned"), "{error}");
    }

    #[test]
    fn rejects_non_data_event_carrying_data() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok|data=foo=1\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must not carry"), "{error}");
    }

    #[test]
    fn rejects_unknown_data_key() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000;bogus=1\n",
            "PRIMA|v=1|seq=4|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("unknown data key"), "{error}");
    }
}
```

- [ ] **Step 4: 运行 xtask 测试确认通过**

Run: `cargo test -p xtask protocol`
Expected: 全部 protocol 测试 PASS（`cargo test -p xtask` 应含既有 qemu 测试共 PASS）。

- [ ] **Step 5: 用协议 v2 实现替换内核 `kernel/src/protocol.rs`**

```rust
use crate::serial;
use core::sync::atomic::{AtomicU32, Ordering};
use kernel::mem::{FrameInventory, MemError};

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

pub(crate) fn memory_ready(inventory: &FrameInventory) {
    write_prefix("memory_ready", "ok");
    serial::write_bytes(b"|data=usable=");
    write_u64(inventory.usable_frames as u64);
    serial::write_bytes(b";image=");
    write_u64(inventory.image_frames as u64);
    serial::write_bytes(b";free=");
    write_u64(inventory.free_frames as u64);
    serial::write_bytes(b";regions=");
    write_u64(inventory.usable_regions as u64);
    serial::write_bytes(b";kstart=");
    write_hex_u64(inventory.kernel_phys_start);
    serial::write_bytes(b";kend=");
    write_hex_u64(inventory.kernel_phys_end);
    serial::write_bytes(b"\r\n");
}

pub(crate) fn memory_error(error: MemError) {
    write_prefix("memory_error", "error");
    serial::write_bytes(b"|data=err=");
    serial::write_bytes(mem_error_name(error).as_bytes());
    serial::write_bytes(b"\r\n");
}

fn mem_error_name(error: MemError) -> &'static str {
    match error {
        MemError::EmptyImage => "EmptyImage",
        MemError::ImageOverlapsUsable => "ImageOverlapsUsable",
        MemError::RegionsNotSortedOrOverlapping => "RegionsNotSortedOrOverlapping",
    }
}

fn write_event(event: &str, status: &str) {
    write_prefix(event, status);
    serial::write_bytes(b"\r\n");
}

fn write_prefix(event: &str, status: &str) {
    let sequence = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    serial::write_bytes(b"PRIMA|v=1|seq=");
    write_u32(sequence);
    serial::write_bytes(b"|event=");
    serial::write_bytes(event.as_bytes());
    serial::write_bytes(b"|status=");
    serial::write_bytes(status.as_bytes());
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

fn write_u64(mut value: u64) {
    let mut buffer = [0_u8; 20];
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

fn write_hex_u64(value: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";

    serial::write_bytes(b"0x");
    if value == 0 {
        serial::write_bytes(b"0");
        return;
    }

    let mut buffer = [0_u8; 16];
    let mut index = buffer.len();
    let mut remaining = value;
    while remaining != 0 {
        index -= 1;
        buffer[index] = DIGITS[(remaining & 0xf) as usize];
        remaining >>= 4;
    }

    serial::write_bytes(&buffer[index..]);
}
```

- [ ] **Step 6: 用协议 v2 实现替换内核 `kernel/src/main.rs`**

```rust
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod protocol;
mod qemu;
mod serial;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;
use kernel::mem;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    protocol::boot_start();
    protocol::serial_ready();

    let regions: &[bootloader_api::info::MemoryRegion] = &boot_info.memory_regions;
    match mem::build_inventory(regions, boot_info.kernel_addr, boot_info.kernel_len) {
        Ok(inventory) => {
            protocol::memory_ready(&inventory);
            protocol::boot_complete();
            serial::flush();
            qemu::exit(qemu::SUCCESS)
        }
        Err(error) => {
            protocol::memory_error(error);
            serial::flush();
            qemu::exit(qemu::PANIC)
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::init();
    protocol::panic_event();
    serial::flush();
    qemu::exit(qemu::PANIC)
}
```

- [ ] **Step 7: 端到端验证**

Run:

```bash
cargo test -p kernel
cargo test -p xtask
cargo xtask test
```

Expected:
- `cargo test -p kernel`：10 tests PASS。
- `cargo test -p xtask`：protocol + qemu 全 PASS。
- `cargo xtask test`：两次 QEMU run 均退出码 33，六条 PASS（含 `memory_ready` 断言通过）。

- [ ] **Step 8: fmt 并提交**

```bash
cargo fmt --all --check
git add kernel/src/main.rs kernel/src/protocol.rs xtask/src/protocol.rs
git commit -m "feat: emit and validate memory_ready in boot protocol"
```

---

### Task 4: 全量验收 + 记录证据（SPEC-017）

**Files:**
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Consumes: Task 3 的完整实现。

- [ ] **Step 1: 跑全量验收**

Run（顺序执行，全部退出码 0）：

```bash
cargo fmt --all --check
cargo test -p kernel
cargo test -p xtask
cargo test -p cosir
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

记录实际输出中的测试计数：`cargo test -p kernel` 的 `<K>`、`cargo test -p xtask` 的 `<X>`、`cargo test -p cosir` 的 `<C>`、`cargo test --workspace` 的 `<W>`。

- [ ] **Step 2: 更新 `Prima OS 实现阶段.md`**

2a. 把阶段 1 交付检查中 S1-D3 行从：

```
- [ ] S1-D3：物理内存发现；证据：页帧清单与保留区域测试。
```

改为：

```
- [x] S1-D3：物理内存发现；证据：页帧清单与保留区域测试（SPEC-017）。
```

2b. 在证据索引表末尾（SPEC-016 行之后）新增一行（把 `<K>`/`<X>`/`<W>` 替换为 Step 1 实测值）：

```
| SPEC-017 | 真实内核 MODEL-TEST + QEMU 集成断言 | `kernel/src/mem.rs`（`build_inventory`/`MemError`/`FrameInventory`）、`kernel/src/main.rs`、`kernel/src/protocol.rs`、`xtask/src/protocol.rs` | 2026-08-17 | 真实 bootloader 交接处解析 `BootInfo.memory_regions` 构建 4KB 帧清单：只统计 `Usable` 区（内核可自由使用帧）、报告保持内核镜像帧（`image_frames`）；bootloader 把内核镜像所在区标为 `Bootloader`（已从 `Usable` 排除），故 `free_frames = usable_frames`，强制不变量为镜像与所有 Usable 帧**不相交**（`image ∩ usable = ∅`，W6 帧级）、`image_frames > 0`。host 单测覆盖对齐截断/非 Usable 忽略/镜像不扣减（`free == usable`）/镜像对齐取整/`EmptyImage`/`ImageOverlapsUsable`/重叠区报错/多区求和/镜像落 Bootloader 区不占 usable；QEMU boot-trace 断言 `memory_ready` 数据满足 `free>0 ∧ image>0 ∧ free==usable ∧ kstart/kend 4KB 对齐 ∧ kstart<kend`。验收：`cargo test -p kernel` `<K>`/`<K>`、`cargo test -p xtask` `<X>`/`<X>`、workspace `<W>`/`<W>`、clippy 零告警、`cargo xtask test` 两 run 退出码 33。不宣称帧分配器（§18.0 #12）、真实页表（S1-D4）、物理内存读写（默认配置无 `physical_memory_offset`）、COW 端到端 `KERNEL-TRACE` 或 `REFINEMENT-PROOF` | S1-D3 |
```

2c. 在「当前快照」表新增一行（放「最近审计验收」之后）：

```
| 物理内存发现（本切片） | 2026-08-17 — 真实内核 `mem::build_inventory` 帧清单 + `memory_ready` 协议上报 + xtask 结构不变式断言；证据 SPEC-017 |
```

- [ ] **Step 3: 提交**

```bash
git add "Prima OS 实现阶段.md"
git commit -m "docs: record physmem discovery evidence (SPEC-017)"
```

---

## Self-Review 记录（计划作者）

- **Spec 覆盖**：§2 目标（lib+bin 拆分→Task 1；帧清单→Task 2；协议→Task 3；双证据→Task 2/3/4）；§3 非目标（无分配器/页表/物理读写/unsafe——计划未引入）；§4 架构（Task 1/2）；§5 协议（Task 3）；§6 错误处理（Task 3 `main.rs` 分支）；§7 测试（Task 2 单测 + Task 3 xtask + Task 4 验收）；§8 证据边界（Task 4 SPEC-017 措辞）；§9 提交划分（Task 1-4 一一对应）。
- **占位符**：Task 4 的 `<K>/<X>/<W>` 是"实测值回填"标记（验收数无法先验得知），非逻辑占位；其余步骤无 TBD。
- **类型一致性**：`FrameInventory`/`MemError`/`build_inventory` 在 Task 2 定义、Task 3 内核与 Task 4 文档引用一致；`FRAME_SIZE` 在 Task 1 定义、Task 2 沿用。
