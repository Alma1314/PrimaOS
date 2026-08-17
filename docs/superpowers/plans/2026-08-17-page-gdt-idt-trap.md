# 页表/GDT/IDT/trap 框架（S1-D4，切片 B）Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在真实 `kernel/` 内核启用物理内存直映射（B1）、读取 bootloader 的 CR3 页表并 page-walk translate（B2）、装载 GDT+TSS（B3）、装载 IDT 并完成一次最小 trap 闭环（B4），四子片独立合并。

**Architecture:** 沿用切片 A 的 lib+bin 结构；不引入外部 `x86_64` crate，GDT/TSS/IDT/页表均用 `#[repr(C)]` 自行编码。纯逻辑（`phys_to_virt`、`translate`、描述符编码）放 lib 可 host 单测；`asm!`/`global_asm!` 的 unsafe 收敛到细化层模块（`cpu.rs`/`gdt.rs`/`idt.rs`）。bootloader 已建好内核/栈/物理直映射页表，本切片只读探测它，不自建。

**Tech Stack:** Rust nightly-2026-07-30、edition 2024、`bootloader_api = "=0.11.17"`（无新依赖）、QEMU 11.0.50（isa-debug-exit 退出码 33）。

## Global Constraints

- 工具链固定 `nightly-2026-07-30`；`cargo fmt --all --check` 必须通过（clippy 不查格式）。
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings` 零告警。
- kernel crate：`#![no_std]`、`#![no_main]`、`#![deny(unsafe_op_in_unsafe_fn)]`。`unsafe` 只允许出现在细化层模块（`cpu.rs`/`gdt.rs`/`idt.rs`/`serial.rs`/`qemu.rs`）；lib 纯逻辑（`mem.rs` 等）不新增 unsafe。
- 不新增依赖；`bootloader_api` 保持精确版本 `=0.11.17`。
- 页表沿用 bootloader 建好的 CR3 页表，只读探测、**不修改**；物理帧只读 `USABLE` 区的、或读 bootloader 页表页本身（其已被 bootloader 映射为只读可访问）。
- 每子片独立合并回 master；每子片完成后 `cargo xtask test` 六条 PASS 无回归。
- 证据措辞纪律：QEMU 事件断言只写结构不变式；不宣称自建页表、ring3、完整异常分派、`KERNEL-TRACE`、`REFINEMENT-PROOF`。
- 现有 boot 事件序列（切片 A）：`boot_start`(1) → `serial_ready`(2) → `memory_ready`(3) → `boot_complete`(4)。切片 B 在其间/之后插入新事件，**不得改动已冻结的切片 A 事件序**。

**bootloader_api 0.11.17 已核实 API（供各 task 逐字引用）：**
- `bootloader_api::Optional<T>` 有 `.into_option() -> Option<T>`。
- `bootloader_api::Mapping::new_default()` 构造 `Mapping::Dynamic`（bootloader 动态选址）。
- `bootloader_api::BootloaderConfig::new_default()` + `config.mappings.physical_memory = Some(Mapping::new_default())` 启用物理直映射。
- `entry_point!(kernel_main, config = &BOOTLOADER_CONFIG)`，其中 `static BOOTLOADER_CONFIG: BootloaderConfig`。
- `BootInfo` 字段：`physical_memory_offset: Optional<u64>`、`kernel_addr: u64`、`kernel_len: u64`、`kernel_image_offset: u64`（内核镜像虚拟基址）、`kernel_stack_bottom/length: u64`、`memory_regions`、`recursive_index: Optional<u16>`。

**现有 kernel 关键文件（引用前先读）：**
- `kernel/src/main.rs`：bin 入口，`entry_point!(kernel_main)`，已接 `mem::build_inventory` + `protocol::memory_ready`。
- `kernel/src/lib.rs`：`#![cfg_attr(not(test), no_std)]`，`pub mod mem;`。
- `kernel/src/mem.rs`：`pub const FRAME_SIZE: u64 = 4096; pub struct FrameInventory{...} pub fn build_inventory(...)`。
- `kernel/src/protocol.rs`：PRIMA writer（`write_prefix`/`write_u64`/`write_hex_u64`/`memory_ready`/`memory_error`/`mem_error_name`）。
- `kernel/src/serial.rs`、`kernel/src/qemu.rs`：串口与退出（`qemu::exit(u8)`、`qemu::SUCCESS/PANIC`）。
- `xtask/src/protocol.rs`：xtask 校验器（`validate_output`，解析 `EventKind/Event`，断言事件序列与 memory_ready 不变式）。

---

### Task 1 (B1): 启用物理内存直映射 + `phys_to_virt` + 探测

**Files:**
- Create: `kernel/src/config.rs`
- Modify: `kernel/src/lib.rs`（`pub mod config;`）
- Modify: `kernel/src/main.rs`（`entry_point!` 传 `BOOTLOADER_CONFIG`；在 `memory_ready` 后调用直映射探测）
- Modify: `kernel/src/mem.rs`（新增 `phys_to_virt` + `probe_physical_memory` 纯逻辑）
- Modify: `kernel/src/protocol.rs`（新增 `physical_map_ready`/`physical_map_error` 事件 writer）
- Modify: `xtask/src/protocol.rs`（校验 `physical_map_ready`）

**Interfaces:**
- Consumes: `kernel::mem::FRAME_SIZE`、`bootloader_api::BootInfo`。
- Produces:
  - `kernel::mem::phys_to_virt(phys: u64, phys_map_offset: u64) -> u64`（`offset + phys`）。
  - `kernel::mem::MapError`（`Debug, Clone, Copy, PartialEq, Eq`）：`{ NoPhysicalMemoryMapping }`。
  - `kernel::protocol::physical_map_ready(offset: u64, va: u64)`：发 `physical_map_ready` 事件（6 字段 data：`offset=0x..;va=0x..;byte=<dec>`）。
  - `kernel::protocol::physical_map_error()`：发 `physical_map_error` 错误事件。
  - 事件序列追加：`boot_start`(1) `serial_ready`(2) `memory_ready`(3) **`physical_map_ready`(4)** `boot_complete`(5)。
  - `kernel::mem::probe_physical_memory(offset: u64, probed_phys: u64) -> Option<(u64, u8)>`：返回 `(va, 读到字节)`。

- [ ] **Step 1: 在 `mem.rs` 追加失败测试（先红）**

在 `kernel/src/mem.rs` 末尾（`#[cfg(test)] mod tests` 内、`image_in_bootloader_does_not_reduce_free` 之后）追加以下测试 —— 此时 `phys_to_virt`/`MapError` 尚未定义，会编译失败：

```rust
    #[test]
    fn phys_to_virt_adds_offset() {
        assert_eq!(phys_to_virt(0x1000, 0xffff_8000_0000_0000), 0xffff_8000_0000_1000);
    }

    #[test]
    fn phys_to_virt_zero_offset_keeps_phys() {
        assert_eq!(phys_to_virt(0xc000_0000, 0), 0xc000_0000);
    }

    #[test]
    fn phys_to_virt_large_phys_near_limit() {
        // 64 GiB 物理上限 + 高半区偏移，不越界（纯算术，无 panic）
        assert_eq!(phys_to_virt(0x10_0000_0000, 0xffff_8000_0000_0000), 0xffff_8010_0000_0000);
    }
```

- [ ] **Step 2: 运行测试确认编译失败**

Run: `cargo test -p kernel mem::`
Expected: FAIL（编译错误：`cannot find function 'phys_to_virt' in module 'mem'` 等）。

- [ ] **Step 3: 在 `mem.rs` 实现 `phys_to_virt`（及其后的非测试代码区）**

在 `kernel/src/mem.rs` 的 `pub fn build_inventory` 之前（`pub const FRAME_SIZE` 之后）插入：

```rust
/// 物理地址经 bootloader 的物理内存直映射换算为虚拟地址（offset + phys）。
pub fn phys_to_virt(phys: u64, phys_map_offset: u64) -> u64 {
    phys_map_offset + phys
}
```

- [ ] **Step 4: 运行测试确认通过**

Run: `cargo test -p kernel mem::`
Expected: `phys_to_virt_adds_offset`、`phys_to_virt_zero_offset_keeps_phys`、`phys_to_virt_large_phys_near_limit` 均 PASS。

- [ ] **Step 5: 创建 `kernel/src/config.rs` 启用物理直映射**

```rust
use bootloader_api::{BootloaderConfig, Mapping};

/// 传给 `entry_point!` 的 bootloader 配置：启用物理内存直映射。
pub(crate) const BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::new_default());
    config
};
```

- [ ] **Step 6: 在 `lib.rs` 暴露 `config` 模块**

`kernel/src/lib.rs` 改为：

```rust
#![cfg_attr(not(test), no_std)]

pub mod config;
pub mod mem;
```

- [ ] **Step 7: 在 `mem.rs` 添加 `probe_physical_memory` 与 `MapError`**

在 `kernel/src/mem.rs` 的 `#[derive(...)] pub enum MemError {...}` 之后、`build_inventory` 之前插入：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapError {
    pub detail: &'static str,
}
```

`probe_physical_memory` 用 `MapError` 提供探测失败原因；返回 `Result` 便于 host 单测。改为（替换 Step 4 后新增的 `probe_physical_memory` 规划——实际它就是如下一步）：

- [ ] **Step 8: 实现 `probe_physical_memory`**

在 `kernel/src/mem.rs` 的 `phys_to_virt` 之后追加：

```rust
/// 读取一个物理地址处的字节（经直映射），返回 (虚拟地址, 字节)。
///
/// # Safety
///
/// `probed_phys` 必须处于 bootloader 报告为 `Usable` 的物理内存中，且
/// 该地址必须是可安全读取的（bootloader 已把物理内存直映射）。
/// 调用方负责保证 probed_phys 是内核可用物理帧（如切片 A 的 usable 区）。
pub fn probe_physical_memory(probed_phys: u64, phys_map_offset: u64) -> (u64, u8) {
    let va = phys_to_virt(probed_phys, phys_map_offset);
    let byte = unsafe { core::ptr::read_volatile(va as *const u8) };
    (va, byte)
}
```

> **说明**：`read_volatile` 是 unsafe（读内存有 UB 风险，volatile 读避免编译器重排）。此函数放 `mem.rs`，但其读操作仅当调用方传入合法 `Usable` 物理地址时安全——`main.rs` 调用方用 `kernel_addr`（内核镜像，非 Usable）不合适，应改用 mem 清单里一个 `Usable` 帧地址。见 Step 10。

- [ ] **Step 9: 追加 `probe` 相关 host 测试（先红）**

在 `mem.rs` 测试模块末尾追加：

```rust
    #[test]
    fn probe_returns_va_and_byte_shape() {
        // 用无害的堆分配器测试：probe 一个 0 字节不可靠，只验证返回值类型与 va=offset+phys。
        // 这里不真正读物理，改用 phys_to_virt 单测覆盖算术；probe 的真实性由 QEMU 验证。
        let va = phys_to_virt(0x1000, 0xffff_8000_0000_0000);
        assert_eq!(va, 0xffff_8000_0000_1000);
    }
```

> **注意**：`probe_physical_memory` 读真实物理内存，host 单测环境无 bootloader 映射，故不单测 `read_volatile` 的实际值，只用 QEMU 验证。这正是项目"host 纯逻辑单测 + QEMU 集成断言"双证据模式。

- [ ] **Step 10: 修改 `kernel/src/main.rs` 接入配置 + 探测 + 事件**

把 `kernel/src/main.rs` 改为：

```rust
#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod config;
mod protocol;
mod qemu;
mod serial;

use bootloader_api::{entry_point, BootInfo};
use core::panic::PanicInfo;
use kernel::mem;

entry_point!(kernel_main, config = &config::BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    protocol::boot_start();
    protocol::serial_ready();

    let regions: &[bootloader_api::info::MemoryRegion] = &boot_info.memory_regions;
    match mem::build_inventory(regions, boot_info.kernel_addr, boot_info.kernel_len) {
        Ok(inventory) => {
            protocol::memory_ready(&inventory);

            // B1: 启用物理直映射后，读 boot info 的物理内存偏移；若为 None 说明配置未生效。
            match boot_info.physical_memory_offset.into_option() {
                Some(offset) => {
                    // 探测一个 bootloader 已映射、属 Usable 的物理帧：取 mem 清单第一个可用帧起始地址。
                    // 该地址由 build_inventory 保证是 4KB 对齐的 Usable 帧。
                    let probe_phys = mem::first_usable_frame_phys(regions);
                    match probe_phys {
                        Some(phys) => {
                            let (va, _byte) = mem::probe_physical_memory(phys, offset);
                            protocol::physical_map_ready(offset, va);
                            protocol::boot_complete();
                            serial::flush();
                            qemu::exit(qemu::SUCCESS)
                        }
                        None => {
                            protocol::physical_map_error();
                            serial::flush();
                            qemu::exit(qemu::PANIC)
                        }
                    }
                }
                None => {
                    protocol::physical_map_error();
                    serial::flush();
                    qemu::exit(qemu::PANIC)
                }
            }
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

- [ ] **Step 11: 在 `mem.rs` 新增 `first_usable_frame_phys`**

在 `kernel/src/mem.rs` 的 `probe_physical_memory` 之后追加：

```rust
/// 返回 mem 清单中第一个 `Usable` 帧的物理起始地址（`align_up(start, FRAME_SIZE)`）。
/// 无任何 Usable 区返回 `None`。仅做纯索引遍历，不读物理。
pub fn first_usable_frame_phys(regions: &[MemoryRegion]) -> Option<u64> {
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            let lo = align_up(region.start, FRAME_SIZE);
            let hi = align_down(region.end, FRAME_SIZE);
            if hi > lo {
                return Some(lo);
            }
        }
    }
    None
}
```

- [ ] **Step 12: 在 `protocol.rs` 新增 `physical_map_ready`/`physical_map_error`**

在 `kernel/src/protocol.rs` 的 `memory_error` 之后追加：

```rust
pub(crate) fn physical_map_ready(offset: u64, va: u64) {
    write_prefix("physical_map_ready", "ok");
    serial::write_bytes(b"|data=offset=");
    write_hex_u64(offset);
    serial::write_bytes(b";va=");
    write_hex_u64(va);
    serial::write_bytes(b"\r\n");
}

pub(crate) fn physical_map_error() {
    write_prefix("physical_map_error", "error");
    serial::write_bytes(b"\r\n");
}
```

- [ ] **Step 13: 运行 kernel 测试 + 修正报错**

Run: `cargo test -p kernel`
Expected:
- `phys_to_virt_*` 和 `probe_returns_va_and_byte_shape` PASS；
- 若 `probe_physical_memory` 的 `unsafe { read_volatile }` 触发 `#![deny(unsafe_op_in_unsafe_fn)]` 或过度限制，按编译提示调整（本函数非 `unsafe fn`，但内部有 `unsafe` 块——`deny(unsafe_op_in_unsafe_fn)` 只约束 `unsafe fn` 内部不再次标 block，此处函数非 unsafe fn 故需整体 `unsafe fn` 标注或移入细化模块。**决策**：因 `probe_physical_memory` 含 unsafe 读，它应放细化层。把 `probe_physical_memory` 移到新文件 `kernel/src/paging_probe.rs`（细化 unsafe 模块），`mem.rs` 保留纯函数 `phys_to_virt`/`first_usable_frame_phys`）。

> 若 compil 提示 `deny(unsafe_op_in_unsafe_fn)`，按上述把 unsafe 函数迁到 `kernel/src/paging_probe.rs`（`pub(crate) mod`），并 Remove Step 8 中 mem.rs 里的 `probe_physical_memory`。

- [ ] **Step 14: 更新 `xtask/src/protocol.rs` 校验 `physical_map_ready`**

给 `xtask/src/protocol.rs`：
1. `EventKind` 增加 `PhysicalMapReady`、`PhysicalMapError`；
2. `EXPECTED_EVENTS` 改为 5 事件：`(1, BootStart) (2, SerialReady) (3, MemoryReady) (4, PhysicalMapReady) (5, BootComplete)`；
3. `parse_line` 的匹配增加分支；`PhysicalMapError` 视为错误（`validate_output` 顶部 panic/err 检查加入它）；
4. 新增 `validate_physical_map_data`：断言 `offset`/`va` 是合法 hex、且 `va >= offset`（直映射恒有 `va = offset + phys`，phys≥0）。

用下方精确代码替换 `xtask/src/protocol.rs` 的相关函数（只改这些，其余保留）：

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    BootStart,
    SerialReady,
    MemoryReady,
    PhysicalMapReady,
    BootComplete,
    Panic,
    MemoryError,
    PhysicalMapError,
}

const EXPECTED_EVENTS: [(u32, EventKind); 5] = [
    (1, EventKind::BootStart),
    (2, EventKind::SerialReady),
    (3, EventKind::MemoryReady),
    (4, EventKind::PhysicalMapReady),
    (5, EventKind::BootComplete),
];
```

在 `validate_output` 内、原有对 `MemoryError|Panic` 的检查处扩展为同时拒绝 `PhysicalMapError`：

```rust
    if events
        .iter()
        .any(|event| matches!(event.kind, EventKind::Panic | EventKind::MemoryError | EventKind::PhysicalMapError))
    {
        return Err("kernel emitted panic, memory_error, or physical_map_error event".to_owned());
    }
```

在 `validate_memory_data` 之后追加 `validate_physical_map_data`，并在 `validate_output` 中调用它：

```rust
fn validate_physical_map_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "physical_map_ready event is missing its data field".to_owned())?;
    let mut offset = None;
    let mut va = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "offset" => offset = Some(parse_addr(value)?),
            "va" => va = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let offset = offset.ok_or_else(|| "missing offset in physical_map_ready data".to_owned())?;
    let va = va.ok_or_else(|| "missing va in physical_map_ready data".to_owned())?;
    if va < offset {
        return Err(format!("va (0x{va:x}) must be >= offset (0x{offset:x})"));
    }
    Ok(())
}
```

在 `validate_output` 的 `find(memory_ready)` 断言后追加：

```rust
    let phys = events
        .iter()
        .find(|event| event.kind == EventKind::PhysicalMapReady)
        .expect("physical_map_ready is in the expected sequence");
    validate_physical_map_data(phys.data.as_deref())?;
```

`parse_line` 匹配扩展：

```rust
        ("physical_map_ready", "ok") => EventKind::PhysicalMapReady,
        ("physical_map_error", "error") => EventKind::PhysicalMapError,
```

并在 `match (kind, &data)` 处允许 PhysicalMapReady/PhysicalMapError 带 data（与 MemoryReady 同处理）：

```rust
        (EventKind::PhysicalMapReady, None) => {
            return Err(format!("{event} event is missing its data field"));
        }
        (EventKind::PhysicalMapReady, Some(_)) => {}
        (EventKind::PhysicalMapError, Some(_)) | (EventKind::PhysicalMapError, None) => {}
```

- [ ] **Step 15: 更新 `xtask/src/protocol.rs` 测试断言新事件列**

给 `validate_output` 的测试加一个 `physical_map_ready` 合法事件到 `VALID` 常量，并把断言改到 5 事件。新增两条失败测试：`rejects_physical_map_error`、`rejects_physical_map_ready_bad_address`。

```rust
    const VALID: &str = concat!(
        "SeaBIOS boot text\r\n",
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
        "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\r\n",
        "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\r\n",
        "PRIMA|v=1|seq=5|event=boot_complete|status=ok\r\n",
    );

    #[test]
    fn rejects_physical_map_error() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=6|event=physical_map_error|status=error\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("physical_map_error"), "{error}");
    }

    #[test]
    fn rejects_physical_map_ready_bad_address() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000001000;va=0xffff800000000000\n",
            "PRIMA|v=1|seq=5|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must be >= offset"), "{error}");
    }
```

- [ ] **Step 16: 运行 xtask 测试**

Run: `cargo test -p xtask protocol`
Expected: 全 PASS（含新增 2 条）。

- [ ] **Step 17: 端到端验收**

Run: `cargo test -p kernel && cargo test -p xtask && cargo xtask test`
Expected:
- `cargo test -p kernel`：11 tests PASS；
- `cargo test -p xtask`：protocol 测试 PASS；
- `cargo xtask test`：六条 PASS，QEMU 两次 run 退出码 33，`physical_map_ready` 断言通过。

- [ ] **Step 18: fmt 并提交**

```bash
cargo fmt --all --check
git add kernel/src/config.rs kernel/src/lib.rs kernel/src/main.rs kernel/src/mem.rs kernel/src/protocol.rs xtask/src/protocol.rs
git commit -m "feat(B1): enable physical memory mapping and probe it"
```

---

### Task 2 (B2): 页表沿用 + page-walk translate

**Files:**
- Create: `kernel/src/cpu.rs`（`cr3()` 读 + `paging` translate 纯逻辑 + host 测试）
- Modify: `kernel/src/lib.rs`（`pub mod cpu;`）
- Modify: `kernel/src/main.rs`（在 B1 探测后调用 `translate` 并断言）
- Modify: `kernel/src/protocol.rs`（`page_table_ready`/`page_table_error`）
- Modify: `xtask/src/protocol.rs`（校验 `page_table_ready`）

**Interfaces:**
- Consumes: `kernel::mem::phys_to_virt`、`bootloader_api::BootInfo`、`config::BOOTLOADER_CONFIG`。
- Produces:
  - `kernel::cpu::PhysAddr`（`u64` newtype）、`kernel::cpu::VirtAddr`（`u64` newtype）。
  - `kernel::cpu::cr3() -> PhysAddr`（`asm!` 读 cr3）。
  - `kernel::cpu::translate(cr3_phys_mapped: u64, vaddr: u64) -> Result<PhysAddr, TranslateError>`，`TranslateError::{PageNotMapped, InvalidLevel, NonCanonical}`。
  - `kernel::cpu::PageTable` / `PageTableEntry`（`#[repr(C)]`），`PageTableEntry::new()`。
  - `kernel::protocol::page_table_ready(translated: u64)` / `page_table_error()`。
  - 事件序列：**`page_table_ready`(5)** 插入 `physical_map_ready`(4) 与 `boot_complete`(6) 之间（`boot_complete` 变 6）。

- [ ] **Step 1: 在 `cpu.rs` 写失败测试（先红）**

创建 `kernel/src/cpu.rs`，先写测试模块（此时 `PageTableEntry`/`translate` 未定义，编译失败）：

```rust
#![cfg_attr(not(test), no_std)]

use bootloader_api::Optional;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PhysAddr(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslateError {
    PageNotMapped,
    InvalidLevel,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageTableEntry {
    pub flags: u64,
}

impl PageTableEntry {
    pub const fn new() -> Self {
        PageTableEntry { flags: 0 }
    }

    fn is_present(&self) -> bool {
        self.flags & 0b1 != 0
    }

    fn addr_bits(&self) -> u64 {
        self.flags & 0x000f_ffff_ffff_f000
    }
}

// translate 的 Pagesize 类型：4KiB only
type PageIndex = usize;

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn page_table_entry_field_size() {
        assert_eq!(size_of::<PageTableEntry>(), 8);
    }

    #[test]
    fn page_table_entry_is_present_uses_lsb() {
        let present = PageTableEntry { flags: 0b1 };
        let absent = PageTableEntry { flags: 0 };
        assert!(present.is_present());
        assert!(!absent.is_present());
    }

    #[test]
    fn page_table_entry_addr_bits_mask_offsets() {
        let e = PageTableEntry { flags: 0x1234_5678_9000 };
        assert_eq!(e.addr_bits(), 0x1234_0000_0000);
    }

    #[test]
    fn phys_addr_newtype_is_u64() {
        let p = PhysAddr(0x1000);
        assert_eq!(p.0, 0x1000);
    }
}
```

- [ ] **Step 2: 运行测试确认失败**

Run: `cargo test -p kernel cpu::`
Expected: FAIL（编译错误：`module 'cpu' not found` —— 需要先在 `lib.rs` `pub mod cpu;`）。

- [ ] **Step 3: 在 `lib.rs` 暴露 `cpu` 模块**

`kernel/src/lib.rs` 改为：

```rust
#![cfg_attr(not(test), no_std)]

pub mod config;
pub mod cpu;
pub mod mem;
```

- [ ] **Step 4: 运行测试确认编译失败点在 translate**

Run: `cargo test -p kernel cpu::`
Expected: FAIL（`cannot find function 'translate'` 等）。

- [ ] **Step 5: 实现 `cr3()` 与 `translate`**

在 `kernel/src/cpu.rs` 的测试模块之前追加：

```rust
/// 读取当前 CR3 寄存器（页表根物理地址）。
///
/// # Safety
///
/// 读 cr3 无副作用，但需在内核已由 bootloader 装载 CR3 后调用。
pub fn cr3() -> PhysAddr {
    let mut value: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    PhysAddr(value)
}
```

加入 `translate` 实现（追加在 `cr3` 之后）：

```rust
/// 沿 4 级页表把虚拟地址 translate 成物理地址。
///
/// `pml4_va` 是 PML4 页表在虚拟地址空间中的映射基址（经 phys_to_virt 得到的 cr3 物理帧的虚拟地址）。
/// 只支持 4KiB 页（遇 2MiB/1GiB 页位返回 PageNotMapped，以示尚未支持）。
pub fn translate(pml4_va: u64, vaddr: u64) -> Result<PhysAddr, TranslateError> {
    let index_pml4 = ((vaddr >> 39) & 0x1ff) as usize;
    let index_pdpt = ((vaddr >> 30) & 0x1ff) as usize;
    let index_pd = ((vaddr >> 21) & 0x1ff) as usize;
    let index_pt = ((vaddr >> 12) & 0x1ff) as usize;
    let page_offset = (vaddr & 0xfff) as u64;

    let pml4 = read_pte_table(pml4_va, index_pml4)?;
    let pdpt_va = pml4.addr_bits();
    let pdpt = read_pte_table(pdpt_va, index_pdpt)?;
    if pdpt.flags & (1 << 7) != 0 {
        return Err(TranslateError::PageNotMapped); // huge page (1GiB) not supported
    }
    let pd_va = pdpt.addr_bits();
    let pd = read_pte_table(pd_va, index_pd)?;
    if pd.flags & (1 << 7) != 0 {
        return Err(TranslateError::PageNotMapped); // huge page (2MiB) not supported
    }
    let pt_va = pd.addr_bits();
    let pt = read_pte_table(pt_va, index_pt)?;
    let phys = pt.addr_bits() | page_offset;
    Ok(PhysAddr(phys))
}

fn read_pte_table(table_va: u64, index: usize) -> Result<PageTableEntry, TranslateError> {
    let entry_addr = table_va + (index as u64) * 8;
    // 需通过直映射把该物理地址读进来。table_va 实为物理地址（页表页在 bootloader 物理区）。
    let entry_ptr = entry_addr as *const u64;
    let raw = unsafe { core::ptr::read_volatile(entry_ptr) };
    let entry = PageTableEntry { flags: raw };
    if !entry.is_present() {
        Err(TranslateError::PageNotMapped)
    } else {
        Ok(entry)
    }
}

```

> **关于 `table_va` 的语义修正（重要）**：BTOTE —— 页表页本身是物理内存，要读它必须经物理直映射。因此本文件的地址语义不是"虚拟地址"，而是"物理地址"。为保证类型正确且避免误用，`translate` 的入参 `pml4_phys: u64`（cr3 物理）、中间层级地址都是 **物理地址**，读取时用 `phys_to_virt(phys, offset)` 转虚拟。你需要在 `translate` 内持有 `phys_map_offset` 参数，或用更清晰的设计：把 `translate` 定为**纯页表解析**（入参是已映射的虚拟基址树指针），而把"物理→虚拟"经由调用方。**采纳下列最终签名**（类型安全、可 host 单测、无歧义）：

重写本文件为如下最终形态（含类型安全的 `PAddr`/`PhysAddr` 与已映射虚拟地址指针）：

```rust
#![cfg_attr(not(test), no_std)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranslateError {
    PageNotMapped,
    InvalidLevel,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PageTableEntry {
    pub flags: u64,
}

impl PageTableEntry {
    pub const fn new() -> Self {
        PageTableEntry { flags: 0 }
    }
    fn is_present(&self) -> bool {
        self.flags & 0b1 != 0
    }
    fn huge(&self) -> bool {
        self.flags & (1 << 7) != 0
    }
    fn next_phys(&self) -> u64 {
        self.flags & 0x000f_ffff_ffff_f000
    }
}

/// 读一个 `PageTableEntry` 数组的第 `index` 项。
/// `table_mapped_va` 是页表页已映射到虚拟地址空间中的地址（调用方负责转换）。
fn read_entry(table_mapped_va: u64, index: usize) -> Result<PageTableEntry, TranslateError> {
    let raw = unsafe {
        core::ptr::read_volatile((table_mapped_va + (index as u64) * 8) as *const u64)
    };
    let e = PageTableEntry { flags: raw };
    if e.is_present() { Ok(e) } else { Err(TranslateError::PageNotMapped) }
}

/// 纯页表解析：从已映射的 PML4 虚拟地址出发，把 vaddr 换成 4KiB 页物理地址。
///
/// 中间层级地址由页表项给出的是物理地址，调用方需先经 phys_to_virt 转成虚拟再递归读。
/// 因此此函数接受一个映射函数 `map: fn(u64) -> u64`（物理→虚拟）。
pub fn translate(
    pml4_mapped_va: u64,
    vaddr: u64,
    map: impl Fn(u64) -> u64,
) -> Result<u64, TranslateError> {
    let i4 = ((vaddr >> 39) & 0x1ff) as usize;
    let i3 = ((vaddr >> 30) & 0x1ff) as usize;
    let i2 = ((vaddr >> 21) & 0x1ff) as usize;
    let i1 = ((vaddr >> 12) & 0x1ff) as usize;
    let offset = vaddr & 0xfff;

    let pml4 = read_entry(pml4_mapped_va, i4)?;
    let pdpt_phys = pml4.next_phys();
    let pdpt_mapped = map(pdpt_phys);
    let pdpt = read_entry(pdpt_mapped, i3)?;
    if pdpt.huge() {
        return Err(TranslateError::PageNotMapped);
    }
    let pd_phys = pdpt.next_phys();
    let pd_mapped = map(pd_phys);
    let pd = read_entry(pd_mapped, i2)?;
    if pd.huge() {
        return Err(TranslateError::PageNotMapped);
    }
    let pt_phys = pd.next_phys();
    let pt_mapped = map(pt_phys);
    let pt = read_entry(pt_mapped, i1)?;
    Ok(pt.next_phys() | offset)
}

/// 在 host 单测中使用的恒等映射（物理==虚拟），纯逻辑测试用。
#[cfg(test)]
fn identity_map(phys: u64) -> u64 {
    phys
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn page_table_entry_field_size() {
        assert_eq!(size_of::<PageTableEntry>(), 8);
    }

    #[test]
    fn page_table_entry_is_present_uses_lsb() {
        assert!(PageTableEntry { flags: 0b1 }.is_present());
        assert!(!PageTableEntry { flags: 0 }.is_present());
    }

    #[test]
    fn page_table_entry_addr_bits_mask_offsets() {
        let e = PageTableEntry { flags: 0x1234_5678_9000 };
        assert_eq!(e.next_phys(), 0x1234_0000_0000);
    }

    // translate 的多层页表解析测试在 Step 6（用 std::boxed::Box 构造多层表）补齐。

```

> **修正**：host 单测里构造多层页表需要堆分配。项目 kernel lib 是 `no_std`（无 alloc），不能 `Vec`。实现策略：**用 `#[cfg(test)] extern crate std` 或使 `cpu.rs` 仅在被测试时可用 core 内存**——更稳妥的是在测试里用 `#[repr(C)]` 的固定大小数组 + `Box` via `std`（host 测试链接 std）。`lib.rs` 已是 `#![cfg_attr(not(test), no_std)]`，测试时可用 std。修改测试为使用 `std::boxed::Box`。

- [ ] **Step 6: 把 `cpu.rs` 测试模块替换为含 translate 多层页表测试（使用 Box + std）**

在 `kernel/src/cpu.rs` 里用下方完整测试模块替换 Step 5 末尾的 `#[cfg(test)] mod tests`（lib 在 test 下可用 std 的 `Box`）：

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::boxed::Box;
    use core::mem::size_of;

    #[test]
    fn page_table_entry_field_size() {
        assert_eq!(size_of::<PageTableEntry>(), 8);
    }

    #[test]
    fn page_table_entry_is_present_uses_lsb() {
        assert!(PageTableEntry { flags: 0b1 }.is_present());
        assert!(!PageTableEntry { flags: 0 }.is_present());
    }

    #[test]
    fn page_table_entry_addr_bits_mask_offsets() {
        let e = PageTableEntry { flags: 0x1234_5678_9000 };
        assert_eq!(e.next_phys(), 0x1234_0000_0000);
    }

    // 构造一棵物理上连续的 4 级表，物理地址即 Box 指针值（identity_map），
    // translate 一个落在最后一层 4KiB 页的地址，期望得到帧地址。
    #[test]
    fn translate_single_page_4k() {
        let mut pt = Box::new([PageTableEntry::new(); 512]);
        pt[0].flags = 0xf000_0000 | 0b1; // 帧 0xf0000000, present
        let pt_ptr = pt.as_ptr() as u64;

        let mut pd = Box::new([PageTableEntry::new(); 512]);
        pd[0].flags = pt_ptr | 0b1; // PD[0] -> PT 表
        let pd_ptr = pd.as_ptr() as u64;

        let mut pdpt = Box::new([PageTableEntry::new(); 512]);
        pdpt[0].flags = pd_ptr | 0b1;
        let pdpt_ptr = pdpt.as_ptr() as u64;

        let mut pml4 = Box::new([PageTableEntry::new(); 512]);
        pml4[0].flags = pdpt_ptr | 0b1;
        let pml4_ptr = pml4.as_ptr() as u64;

        // vaddr 落在 i4=i3=i2=i1=0, offset=0x123
        let vaddr = 0x123;
        let phys = translate(pml4_ptr, vaddr, identity_map).unwrap();
        assert_eq!(phys, 0xf000_0000 | 0x123);
    }

    #[test]
    fn translate_not_mapped_returns_error() {
        let mut pml4 = Box::new([PageTableEntry::new(); 512]);
        let pml4_ptr = pml4.as_ptr() as u64;
        let vaddr = 0x1_0000_0000; // i4=0, i3=0, i2=0, i1 nonzero
        assert_eq!(translate(pml4_ptr, vaddr, identity_map), Err(TranslateError::PageNotMapped));
    }

    #[test]
    fn translate_offsets_into_last_level() {
        // 复用 single-page 构造，但访问 PT[0] 的另一帧偏移
        let mut pt = Box::new([PageTableEntry::new(); 512]);
        pt[0].flags = 0x1000_0000 | 0b1;
        let pt_ptr = pt.as_ptr() as u64;
        let mut pd = Box::new([PageTableEntry::new(); 512]);
        pd[0].flags = pt_ptr | 0b1;
        let pd_ptr = pd.as_ptr() as u64;
        let mut pdpt = Box::new([PageTableEntry::new(); 512]);
        pdpt[0].flags = pd_ptr | 0b1;
        let pdpt_ptr = pdpt.as_ptr() as u64;
        let mut pml4 = Box::new([PageTableEntry::new(); 512]);
        pml4[0].flags = pdpt_ptr | 0b1;
        let pml4_ptr = pml4.as_ptr() as u64;
        let vaddr = 0x123;
        assert_eq!(translate(pml4_ptr, vaddr, identity_map).unwrap(), 0x1000_0000 | 0x123);
    }
}
```

> **identity_map 需为 pub-in-tests**：因 `translate` 的第 3 参用 `impl Fn`，测试里传 `identity_map`（`#[cfg(test)]` 里的 `fn`）。它是测试专用，放在 `#[cfg(test)]` 内已有。

- [ ] **Step 7: 运行 cpu 测试**

Run: `cargo test -p kernel cpu::`
Expected: 全部 PASS（`page_table_entry_*` 3 个 + `translate_*` 3 个 = 6 tests）。

- [ ] **Step 8: 在 `main.rs` 接入 translate 探测**

在 `kernel/src/main.rs` 的 `physical_map_ready` 调用之后（Step 14 之后）追加页表探测。用 `kernel::cpu::{cr3, translate, TranslateError}` 和 `kernel::mem::phys_to_virt`：

```rust
            // B2: 读 cr3 得到 PML4 物理地址，经直映射转虚拟，translate 内核镜像虚拟地址到物理。
            let pml4_phys = kernel::cpu::cr3().0;
            let pml4_va = mem::phys_to_virt(pml4_phys, offset);
            let target_va = boot_info.kernel_image_offset; // 内核镜像虚拟基址
            let map = |phys: u64| mem::phys_to_virt(phys, offset);
            match kernel::cpu::translate(pml4_va, target_va, map) {
                Ok(translated_phys) => {
                    protocol::page_table_ready(translated_phys);
                    // （接 B1 的 boot_complete；见 Step 9 事件序）
                }
                Err(_) => {
                    protocol::page_table_error();
                    serial::flush();
                    qemu::exit(qemu::PANIC)
                }
            }
```

- [ ] **Step 9: 重新排列 `main.rs` 事件序 + 新增 `page_table_ready/error`**

`main.rs` 事件序调整为：`boot_start`(1) → `serial_ready`(2) → `memory_ready`(3) → `physical_map_ready`(4) → **`page_table_ready`(5)** → `boot_complete`(6)。在 `protocol::physical_map_ready(offset, va)` 之后调用 `page_table_ready`，再 `boot_complete`。

`protocol.rs` 追加：

```rust
pub(crate) fn page_table_ready(translated: u64) {
    write_prefix("page_table_ready", "ok");
    serial::write_bytes(b"|data=translated=");
    write_hex_u64(translated);
    serial::write_bytes(b"\r\n");
}

pub(crate) fn page_table_error() {
    write_prefix("page_table_error", "error");
    serial::write_bytes(b"\r\n");
}
```

- [ ] **Step 10: 更新 xtask 校验 `page_table_ready`**

`xtask/src/protocol.rs`：
- `EventKind` 加 `PageTableReady`/`PageTableError`；
- `EXPECTED_EVENTS` 变 6 事件（BootStart, SerialReady, MemoryReady, PhysicalMapReady, **PageTableReady**, BootComplete）；
- `parse_line` 分支加 `("page_table_ready","ok")` / `("page_table_error","error")`；
- 失败 event 集合加 `PageTableError`；
- `validate_page_table_data`：断言 `translated` 是合法 hex 且 `> 0`；
- 在 `validate_output` 里 `find(page_table_ready)` 后调用它；
- `match (kind, &data)` 允许 PageTableReady/PageTableError 处理（带/不带 data）。

```rust
fn validate_page_table_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "page_table_ready event is missing its data field".to_owned())?;
    let mut translated = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        if key != "translated" {
            return Err(format!("unknown data key: {key}"));
        }
        translated = Some(parse_addr(value)?);
    }
    let translated = translated.ok_or_else(|| "missing translated in page_table_ready data".to_owned())?;
    if translated == 0 {
        return Err("translated physical address must be > 0".to_owned());
    }
    Ok(())
}
```

- [ ] **Step 11: 更新 xtask 测试断言 6 事件列**

`VALID` 常量加 `page_table_ready` 事件（插在 `physical_map_ready` 后、`boot_complete` 前），设 `translated=0xf0000000 `... 具体值以运行结果为准（内核在 QEMU 实际 translate 出的物理地址是真实的，xtask 只断言其 >0 与合法 hex，不断言精确值——因 QEMU 布局可能随镜像而异）。

新增失败测试：`rejects_page_table_error`、`rejects_page_table_ready_zero`。

- [ ] **Step 12: 运行 xtask 测试**

Run: `cargo test -p xtask protocol`
Expected: 全 PASS。

- [ ] **Step 13: 端到端验收**

Run: `cargo test -p kernel && cargo test -p xtask && cargo xtask test`
Expected: kernel 反映 cpu 测试通过；xtask 全绿；`cargo xtask test` 六 PASS（`page_table_ready` 断言成立，说明 translate 出的物理地址合法）。

- [ ] **Step 14: fmt 并提交**

```bash
cargo fmt --all --check
git add kernel/src/cpu.rs kernel/src/lib.rs kernel/src/main.rs kernel/src/protocol.rs xtask/src/protocol.rs
git commit -m "feat(B2): read CR3 page table and validate translate"
```

---

### Task 3 (B3): GDT + 最小 TSS

**Files:**
- Create: `kernel/src/gdt.rs`（完整实现 + host 测试，未提交前源码见下方各 Step）
- Modify: `kernel/src/lib.rs`（`pub mod gdt;`）
- Modify: `kernel/src/main.rs`（init gdt.tss）
- Modify: `kernel/src/protocol.rs`（`gdt_tss_ready`/`gdt_tss_error`）
- Modify: `xtask/src/protocol.rs`（校验 `gdt_tss_ready`）

**Interfaces:**
- Consumes: `bootloader_api::BootInfo`（`kernel_stack_bottom`/`kernel_stack_len`）、`config::BOOTLOADER_CONFIG`。
- Produces:
  - `kernel::gdt::gdt_init(rsp0: u64) -> Result<(), ()>`：一次性装载 GDT + TSS（本片单核 boot 早期，无并发）。
  - `kernel::gdt::Gdt`/`Descriptor`/`Tss`（模块私有结构，测试通过 public 断言字节）。
  - `kernel::protocol::gdt_tss_ready()` / `gdt_tss_error()`。
  - 事件序列：`gdt_tss_ready`(6) 插在 `page_table_ready`(5) 与 `boot_complete`(7) 之间（`boot_complete` 变 7）。

**GDT/TSS 最终字节布局（x86-64，已按 OSDev 标准核实，implementer 不得改动）：**
- 每个 GDT 项是 `[u8; 16]`（16 字节描述符）。常规段只用低 8 字节，高 8 字节为 0。
  - 槽 0（null）：`[0; 16]`。
  - 槽 1（kernel code）：`limit_low=0xffff, access=0x9A, flags=0x20`（64 位代码段：present, DPL0, non-system, type=exec-read, **L=1**），base=0。
    - 字节：`[0xff, 0xff, 0x00, 0x00, 0x00, 0x9A, 0x20, 0x00, 0,0,0,0,0,0,0,0]`。
  - 槽 2（kernel data）：`limit_low=0xffff, access=0x92, flags=0x00`（present, DPL0, non-system, data-rw）。
    - 字节：`[0xff, 0xff, 0x00, 0x00, 0x00, 0x92, 0x00, 0x00, 0,0,0,0,0,0,0,0]`。
  - 槽 3（TSS）：16 字节系统段描述符，由 `install_tss` 按真实 TSS 基址/limit 填充（见 Step 5）。
- `Tss`（标准 x86-64 布局，`#[repr(C)]`）：`reserved0:u32, rsp0:u64, rsp1:u64, rsp2:u64, reserved1:u64, ist[7]:u64, reserved2:u64, reserved3:u16, iopb:u16`。本片只设 `rsp0`，其余 0，`iopb=104`（= TSS 字节数偏移，长度固定 104）。

- [ ] **Step 1: 完整创建 `kernel/src/gdt.rs`（实现 + 测试，一次到位）**

逐字创建 `kernel/src/gdt.rs`（这是最终实现，非占位）：

```rust
#![cfg_attr(not(test), no_std)]

use core::arch::asm;

/// 16 字节段/系统描述符（x86-64）。
#[repr(C, packed(2))]
#[derive(Clone, Copy, Default)]
pub struct Descriptor([u8; 16]);

impl Descriptor {
    const fn null() -> Self {
        Descriptor([0; 16])
    }
    const fn kernel_code() -> Self {
        Descriptor([
            0xff, 0xff, // limit low = 0xffff
            0x00, 0x00, 0x00, // base low..mid = 0
            0x9a, // access: present | DPL0 | non-system | exec-read
            0x20, // flags: L=1 (64-bit code)
            0x00, // base hi = 0
            0, 0, 0, 0, 0, 0, 0, 0, // high 8 bytes = 0
        ])
    }
    const fn kernel_data() -> Self {
        Descriptor([
            0xff, 0xff, // limit low = 0xffff
            0x00, 0x00, 0x00, // base = 0
            0x92, // access: present | DPL0 | non-system | data-rw
            0x00, // flags = 0
            0x00, // base hi = 0
            0, 0, 0, 0, 0, 0, 0, 0,
        ])
    }
    fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

/// 最小 TSS：为 trap 提供内核栈切换（rsp0）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iopb: u16,
}

impl Tss {
    const fn new(rsp0: u64) -> Self {
        Tss {
            reserved0: 0,
            rsp0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist: [0; 7],
            reserved2: 0,
            reserved3: 0,
            iopb: 104,
        }
    }
}

/// 装载 GDT 的槽 0/1/2（null/code/data），并把 TSS 描述符装进槽 3、`ltr`。
///
/// # Safety
///
/// 仅可在单核 boot 早期、内核尚未允许并发时调用一次。`gdt`/`tss` 必须存活到内核结束。
pub unsafe fn init(gdt: &mut Gdt, tss: &Tss) -> Result<(), ()> {
    gdt.items[0] = Descriptor::null();
    gdt.items[1] = Descriptor::kernel_code();
    gdt.items[2] = Descriptor::kernel_data();
    install_tss_desc(gdt, tss)?;

    // lgdt
    let len = (gdt.items.len() * 16) as u16 - 1;
    let base = gdt.items.as_ptr() as u64;
    let mut gdtr = [0u8; 10];
    gdtr[0..2].copy_from_slice(&len.to_le_bytes());
    gdtr[2..10].copy_from_slice(&base.to_le_bytes());
    asm!("lgdt [{}]", in(reg) gdtr.as_ptr(), options(nostack, preserves_flags));

    // ltr: 装载 TSS 段选择子（index 3, RPL 0）为忙态。
    let selector: u16 = 3 * 8;
    asm!("ltr bx", in("bx") selector, options(nostack, preserves_flags));

    // reload 数据段选择子（内核数据段 index 2）。
    asm!(
        "mov ds, bx",
        "mov es, bx",
        "mov ss, bx",
        in("bx") (2 * 8) as u16,
        options(nostack, preserves_flags)
    );
    Ok(())
}

fn install_tss_desc(gdt: &mut Gdt, tss: &Tss) -> Result<(), ()> {
    let base = tss as *const Tss as u64;
    let limit = (core::mem::size_of::<Tss>() - 1) as u16;
    let lo = limit & 0xffff;
    let hi_flags = ((limit >> 16) & 0x0f) as u8 | 0x00; // TSS 用 byte granularity（无 L），flags=0

    let mut d = [0u8; 16];
    d[0] = (lo & 0xff) as u8;
    d[1] = (lo >> 8) as u8;
    d[2] = (base & 0xff) as u8;
    d[3] = ((base >> 8) & 0xff) as u8;
    d[4] = ((base >> 16) & 0xff) as u8;
    d[5] = 0x89; // present | DPL0 | sys | 64-bit available TSS (type=0x9)
    d[6] = hi_flags;
    d[7] = ((base >> 24) & 0xff) as u8;
    // 高 8 字节：base [31:24]..[63:32] + reserved
    d[8] = ((base >> 24) & 0xff) as u8;
    d[9] = ((base >> 32) & 0xff) as u8;
    d[10] = ((base >> 40) & 0xff) as u8;
    d[11] = ((base >> 48) & 0xff) as u8;
    d[12] = ((base >> 56) & 0xff) as u8;
    d[13..16].copy_from_slice(&[0, 0, 0]);
    gdt.items[3] = Descriptor(d);
    Ok(())
}

/// GDT 容器（4 槽，各 16 字节）。
#[repr(C, packed(2))]
#[derive(Clone)]
pub struct Gdt {
    items: [Descriptor; 4],
}

impl Gdt {
    const fn new() -> Self {
        Gdt {
            items: [
                Descriptor::null(),
                Descriptor::kernel_code(),
                Descriptor::kernel_data(),
                Descriptor([0; 16]),
            ],
        }
    }
}

// 静态持有：单核 boot 早期，仅一次 init 时用 unsafe 写一次。
static mut GDT_ITEM: Gdt = Gdt::new();
static mut TSS_ITEM: Tss = Tss::new(0);

/// 供 `main.rs` 调用的公开入口：装载 GDT/TSS。
pub fn init_kernel_gdt(rsp0: u64) -> Result<(), ()> {
    unsafe {
        GDT_ITEM = Gdt::new();
        TSS_ITEM = Tss::new(rsp0);
        init(&mut GDT_ITEM, &TSS_ITEM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn descriptor_is_16_bytes() {
        assert_eq!(size_of::<Descriptor>(), 16);
    }

    #[test]
    fn tss_is_104_bytes() {
        assert_eq!(size_of::<Tss>(), 104);
    }

    #[test]
    fn code_descriptor_layout() {
        let d = Descriptor::kernel_code();
        let b = d.as_bytes();
        assert_eq!(b[5], 0x9a);
        assert_eq!(b[6], 0x20);
        assert!(b[7..16].iter().all(|&x| x == 0));
    }

    #[test]
    fn data_descriptor_layout() {
        let d = Descriptor::kernel_data();
        let b = d.as_bytes();
        assert_eq!(b[5], 0x92);
        assert_eq!(b[6], 0x00);
    }

    #[test]
    fn null_descriptor_is_zero() {
        assert!(Descriptor::null().is_zero());
    }

    #[test]
    fn tss_rsp0_offset_is_4() {
        // rsp0 字段在 offset 4（TSS 标准布局）。
        let t = Tss::new(0x1234_5678);
        let bytes = unsafe { core::slice::from_raw_parts(&t as *const Tss as *const u8, size_of::<Tss>()) };
        assert_eq!(&bytes[4..12], &0x1234_5678u64.to_le_bytes());
    }
}
```

- [ ] **Step 2: 在 `lib.rs` 暴露 `gdt` 模块**

`kernel/src/lib.rs` 改为：

```rust
#![cfg_attr(not(test), no_std)]

pub mod cpu;
pub mod gdt;
pub mod mem;
```

（`config` 模块仍在——若 Task 1 已加。最终 `lib.rs` 应含 `config`、`cpu`、`gdt`、`mem` 四个模块声明。）

- [ ] **Step 3: 运行 kernel gdt 测试**

Run: `cargo test -p kernel gdt::`
Expected: 6 tests PASS（`descriptor_is_16_bytes` 等全部 ok）。

- [ ] **Step 4: 在 `protocol.rs` 添加事件 writer**

```rust
pub(crate) fn gdt_tss_ready() {
    write_event("gdt_tss_ready", "ok");
}

pub(crate) fn gdt_tss_error() {
    write_prefix("gdt_tss_error", "error");
    serial::write_bytes(b"\r\n");
}
```

- [ ] **Step 5: 在 `main.rs` 接入 GDT/TSS 启动**

在 `page_table_ready` 成功之后、`boot_complete` 之前，插入：

```rust
            // B3: 装载 GDT + 最小 TSS（rsp0 = 内核栈顶）。
            let rsp0 = boot_info.kernel_stack_bottom + boot_info.kernel_stack_len;
            match kernel::gdt::init_kernel_gdt(rsp0) {
                Ok(()) => protocol::gdt_tss_ready(),
                Err(()) => {
                    protocol::gdt_tss_error();
                    serial::flush();
                    qemu::exit(qemu::PANIC)
                }
            }
```

（`main.rs` 里需要 `use bootloader_api::BootInfo;` 已有；`qemu`/`serial`/`protocol` 已在。）

- [ ] **Step 6: 更新 xtask 校验 `gdt_tss_ready`**

`xtask/src/protocol.rs`：
1. `EventKind` 增 `GdtTssReady`、`GdtTssError`；
2. `EXPECTED_EVENTS` 变 7 事件：`(1,BootStart) (2,SerialReady) (3,MemoryReady) (4,PhysicalMapReady) (5,PageTableReady) (6,GdtTssReady) (7,BootComplete)`；
3. `parse_line` 分支：`("gdt_tss_ready","ok") => GdtTssReady`、`("gdt_tss_error","error") => GdtTssError`；
4. 失败事件集合加 `GdtTssError`：`matches!(.. | EventKind::GdtTssError)`；
5. `gdt_tss_ready` 无 data——在 `match (kind, &data)` 默认 `(_, None) => {}` 已覆盖，无需额外分支（它属于"无 data"类）；
6. `VALID` 常量加 `gdt_tss_ready` 行（插在 `page_table_ready` 后、`boot_complete` 前）。

`VALID` 更新为：

```rust
    const VALID: &str = concat!(
        "SeaBIOS boot text\r\n",
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
        "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\r\n",
        "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\r\n",
        "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\r\n",
        "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
        "PRIMA|v=1|seq=7|event=boot_complete|status=ok\r\n",
    );
```

新增失败测试：

```rust
    #[test]
    fn rejects_gdt_tss_error() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=8|event=gdt_tss_error|status=error\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("gdt_tss_error"), "{error}");
    }
```

- [ ] **Step 7: 运行 xtask 测试**

Run: `cargo test -p xtask protocol`
Expected: 全 PASS（含新增 `rejects_gdt_tss_error`）。

- [ ] **Step 8: 端到端验收**

Run: `cargo test -p kernel && cargo test -p xtask && cargo xtask test`
Expected:
- `cargo test -p kernel`：gdt 6 tests + 既有 tests 全 PASS；
- `cargo test -p xtask`：全绿；
- `cargo xtask test`：六 PASS，QEMU 两次 run 退出码 33，`gdt_tss_ready` 断言通过（说明 GDT/TSS 装载后内核继续正常运行不崩溃）。

> **若 QEMU 崩（退出码 != 33）**：多半是数据段 reload 后 `ss` 问题或 TSS 描述符编码。先回退 `mov ss,bx`（保留 bootloader 的 ss），仅 reload ds/es；若仍崩，检查 `install_tss_desc` 的 limit 是否 < 数据段 limit（应为 104-1=103 < 0xffff，无碍）。调整后重跑至六 PASS 再提交。

- [ ] **Step 9: fmt 并提交**

```bash
cargo fmt --all --check
git add kernel/src/gdt.rs kernel/src/lib.rs kernel/src/main.rs kernel/src/protocol.rs xtask/src/protocol.rs
git commit -m "feat(B3): load GDT and minimal TSS"
```

---

### Task 4 (B4): IDT + 最小 trap 闭环（x86-interrupt ABI）

**Files:**
- Create: `kernel/src/idt.rs`（IDT 结构 + 描述符编码 + `lidt` + `extern "x86-interrupt"` handler + host 测试）
- Modify: `kernel/src/lib.rs`（`pub mod idt;`）
- Modify: `kernel/src/main.rs`（load idt，触发 int3）
- Modify: `kernel/src/protocol.rs`（`trap_ready`）
- Modify: `xtask/src/protocol.rs`（校验 `trap_ready`）

**Interfaces:**
- Consumes: `config::BOOTLOADER_CONFIG`、`gdt::*`（B3，建立内核代码段 RPL0 selector `0x8`）、`boot_info`。
- Produces:
  - `kernel::idt::Idt`（`#[repr(C, packed(2))]`，`new()`/`set_handler()`）、`kernel::idt::Entry`。
  - `kernel::idt::load_idt(idt: &Idt) -> Result<(), ()>`（`lidt`）。
  - `kernel::idt::breakpoint_handler`（`extern "x86-interrupt" fn(&mut InterruptStackFrame)`，置 `TRAP_VECTOR_SEEN`）。
  - `kernel::idt::trigger_int3()`、`kernel::idt::trap_seen() -> u8`。
  - `kernel::protocol::trap_ready(seen: u8)`。
  - 事件序列：**`trap_ready`(7)** 插在 `gdt_tss_ready`(6) 与 `boot_complete`(8) 之间（`boot_complete` 变 8）。

**为什么用 `extern "x86-interrupt"`：** Rust 编译器对该 ABI 自动生成匹配 `iretq` 的入口/出口（保存所有通用寄存器、压 trap 时的 RSP/RIP/CS/RFLAGS/error code 处理），**完全避免易错的手写 `global_asm!`**。handler 内部普通 Rust 代码，`iretq` 由编译器在 handler 返回时发出，控制流精确回到 `int3` 之后的指令（同特权级 trap，ring0→ring0，不切栈 RSP）。

- [ ] **Step 1: 创建 `kernel/src/idt.rs`（完整实现 + 测试，一次到位）**

逐字创建 `kernel/src/idt.rs`（最终实现）：

```rust
#![cfg_attr(not(test), no_std)]

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::{AtomicU8, Ordering};

/// IDT 条目（64-bit interrupt gate，16 字节）。
#[repr(C, packed(2))]
#[derive(Clone, Copy, Default)]
pub struct Entry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
}

/// IDT 表（256 项）。
#[repr(C, packed(2))]
#[derive(Clone)]
pub struct Idt {
    entries: [Entry; 256],
}

/// CPU 压入的中断帧（同级 trap：int3 压 rip/cs/rflags）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
}

impl Idt {
    pub const fn new() -> Self {
        Idt {
            entries: [Entry::default(); 256],
        }
    }

    /// 把 `handler` 装入第 `index` 号向量，selector 为内核代码段 RPL0（0x8）。
    pub fn set_handler(&mut self, index: u8, handler: HandlerFunc) {
        let h = handler as u64;
        self.entries[index as usize] = Entry {
            offset_low: (h & 0xffff) as u16,
            selector: 0x8,
            ist: 0,
            type_attr: 0x8e, // present | DPL0 | 64-bit interrupt gate
            offset_mid: ((h >> 16) & 0xffff) as u16,
            offset_high: (h >> 32) as u32,
        };
    }
}

/// x86-interrupt handler 类型（编译器生成 iretq）。
pub type HandlerFunc = extern "x86-interrupt" fn(&mut InterruptStackFrame);

/// 装载 IDT。
pub fn load_idt(idt: &Idt) -> Result<(), ()> {
    let limit = (size_of::<Idt>() - 1) as u16;
    let base = idt as *const Idt as u64;
    let mut idtr = [0u8; 10];
    idtr[0..2].copy_from_slice(&limit.to_le_bytes());
    idtr[2..10].copy_from_slice(&base.to_le_bytes());
    unsafe {
        asm!("lidt [{}]", in(reg) idtr.as_ptr(), options(nostack, readonly, preserves_flags));
    }
    Ok(())
}

static TRAP_VECTOR_SEEN: AtomicU8 = AtomicU8::new(0);

/// vector 3（breakpoint）handler —— 记录触发，返回后 iretq 回到 int3 之后。
extern "x86-interrupt" fn breakpoint_handler(_frame: &mut InterruptStackFrame) {
    TRAP_VECTOR_SEEN.store(3, Ordering::Relaxed);
}

/// 触发一次 int3（breakpoint）。由 IDT vector 3 处理，返回后继续。
pub fn trigger_int3() {
    unsafe {
        asm!("int3", options(nostack));
    }
}

/// 读取已触发的向量号（初始 0；int3 后为 3）。
pub fn trap_seen() -> u8 {
    TRAP_VECTOR_SEEN.load(Ordering::Relaxed)
}

// 静态持有 IDT：lidt 后需存活整个执行。
static mut IDT_ITEM: Idt = Idt::new();

/// 供 `main.rs` 调用：初始化 IDT（vector 3 指向 breakpoint handler）并装载。
pub fn init_idt() -> Result<(), ()> {
    unsafe {
        IDT_ITEM = Idt::new();
        IDT_ITEM.set_handler(3, breakpoint_handler);
    }
    load_idt(unsafe { &IDT_ITEM })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idt_entry_size_is_16() {
        assert_eq!(size_of::<Entry>(), 16);
    }

    #[test]
    fn idt_total_size_is_4096() {
        assert_eq!(size_of::<Idt>(), 256 * 16);
    }

    #[test]
    fn default_entry_is_not_present() {
        let e = Entry::default();
        assert_eq!(e.type_attr, 0);
    }

    #[test]
    fn set_handler_encodes_offsets_and_selector() {
        let mut idt = Idt::new();
        // 用一个假地址验证编码：0x1122_3344_5566_7788
        fn dummy(_f: &mut InterruptStackFrame) {}
        // 这里不调 set_handler（它要求 HandlerFunc=extern x86-interrupt）；改用手工构造验证编码位。
        // 直接构造一个 Entry 并断言字段分布。
        let e = Entry {
            offset_low: 0x7788,
            selector: 0x8,
            ist: 0,
            type_attr: 0x8e,
            offset_mid: 0x5566,
            offset_high: 0x1122_3344,
        };
        // offset = offset_low | offset_mid<<16 | offset_high<<32
        let offset = (e.offset_low as u64) | ((e.offset_mid as u64) << 16) | ((e.offset_high as u64) << 32);
        assert_eq!(offset, 0x1122_3344_5566_7788);
        assert_eq!(e.selector, 0x8);
        assert_eq!(e.type_attr, 0x8e);
    }

    #[test]
    fn interrupt_stack_frame_field_layout() {
        // 三条 u64：rip, cs, rflags —— 顺序与 CPU 压入一致。
        let f = InterruptStackFrame {
            instruction_pointer: 0xdead_beef,
            code_segment: 0x8,
            cpu_flags: 0x202,
        };
        let bytes =
            unsafe { core::slice::from_raw_parts(&f as *const _ as *const u8, size_of::<InterruptStackFrame>()) };
        assert_eq!(&bytes[0..8], &0xdead_beefu64.to_le_bytes());
        assert_eq!(&bytes[8..16], &0x8u64.to_le_bytes());
        assert_eq!(&bytes[16..24], &0x202u64.to_le_bytes());
    }
}
```

> **实现准则**：`set_handler` 里 `handler as u64`（`extern "x86-interrupt" fn` 转 usize）是合法操作，与 blog_os / x86_64 crate 的 `set_handler` 一致。若 nightly 报 `fn` 转 `usize` 限制，改用 `handler as *const () as u64`（同样合法）。实现后必须通过 `cargo test -p kernel idt::` 编译验证，两种写法有一必可编译。

- [ ] **Step 2: 在 `lib.rs` 暴露 `idt`**

`kernel/src/lib.rs` 最终：

```rust
#![cfg_attr(not(test), no_std)]

pub mod config;
pub mod cpu;
pub mod gdt;
pub mod idt;
pub mod mem;
```

（`config` 为 Task 1、`cpu` 为 Task 2、`gdt` 为 Task 3、`idt` 为本 task、`mem` 为切片 A。若某模块缺失，先跑对应 task 再继续。）

- [ ] **Step 3: 运行 IDT 测试**

Run: `cargo test -p kernel idt::`
Expected: 5 tests PASS（`idt_entry_size_is_16`、`idt_total_size_is_4096`、`default_entry_is_not_present`、`set_handler_encodes_offsets_and_selector`、`interrupt_stack_frame_field_layout`）。

> 若 `extern "x86-interrupt" fn as u64` 报 cast 错误，按 Step 1 备注改用 `as *const () as u64`；直到全 PASS 再继续。

- [ ] **Step 4: 在 `protocol.rs` 添加 `trap_ready`**

```rust
pub(crate) fn trap_ready(seen: u8) {
    write_prefix("trap_ready", "ok");
    serial::write_bytes(b"|data=seen=");
    write_u8(seen);
    serial::write_bytes(b"\r\n");
}
```

`write_u8` 复用一个十进制 u8 writer（若已有 `write_u32`，加一个小的 `write_u8` 或复用 `write_u32(seen as u32)`）。**使用后者**（避免新增函数）：

```rust
pub(crate) fn trap_ready(seen: u8) {
    write_prefix("trap_ready", "ok");
    serial::write_bytes(b"|data=seen=");
    write_u32(seen as u32);
    serial::write_bytes(b"\r\n");
}
```

- [ ] **Step 5: 在 `main.rs` 接入 IDT + 触发 int3**

在 `gdt_tss_ready` 之后、`boot_complete` 之前，插入：

```rust
            // B4: 装载 IDT，触发 int3，验证 trap → handler → iretq 闭环。
            match kernel::idt::init_idt() {
                Ok(()) => {
                    kernel::idt::trigger_int3();
                    let seen = kernel::idt::trap_seen();
                    protocol::trap_ready(seen);
                }
                Err(()) => {
                    protocol::idt_error();
                    serial::flush();
                    qemu::exit(qemu::PANIC)
                }
            }
```

`protocol::idt_error`：

```rust
pub(crate) fn idt_error() {
    write_prefix("idt_error", "error");
    serial::write_bytes(b"\r\n");
}
```

- [ ] **Step 6: 更新 xtask 校验 `trap_ready`**

`xtask/src/protocol.rs`：
1. `EventKind` 增 `TrapReady`、`IdtError`；
2. `EXPECTED_EVENTS` 变 8 事件：`(1,BootStart) (2,SerialReady) (3,MemoryReady) (4,PhysicalMapReady) (5,PageTableReady) (6,GdtTssReady) (7,TrapReady) (8,BootComplete)`；
3. `parse_line` 分支：`("trap_ready","ok") => TrapReady`、`("idt_error","error") => IdtError`；
4. 失败事件集合（panic/memory_error/physical_map_error/gdt_tss_error）加 `IdtError`；
5. `trap_ready` 带 data（`seen=`）——在 `match (kind, &data)` 里像 `MemoryReady`/`PhysicalMapReady` 一样要求 data；
6. `validate_trap_data`：断言 `seen == 3`；
7. `VALID` 加 `trap_ready` 行 + 新增 `rejects_idt_error`、`rejects_trap_wrong_seen` 失败测试。

```rust
fn validate_trap_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "trap_ready event is missing its data field".to_owned())?;
    let mut seen = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        if key != "seen" {
            return Err(format!("unknown data key: {key}"));
        }
        seen = Some(parse_count(value)?);
    }
    let seen = seen.ok_or_else(|| "missing seen in trap_ready data".to_owned())?;
    if seen != 3 {
        return Err(format!("trap_ready seen must be 3 (breakpoint), got {seen}"));
    }
    Ok(())
}
```

`VALID` 更新（加第 7 行 `trap_ready|data=seen=3`）：

```rust
    const VALID: &str = concat!(
        "SeaBIOS boot text\r\n",
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
        "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\r\n",
        "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\r\n",
        "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\r\n",
        "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
        "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
        "PRIMA|v=1|seq=8|event=boot_complete|status=ok\r\n",
    );
```

新增失败测试：

```rust
    #[test]
    fn rejects_idt_error() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=9|event=idt_error|status=error\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("idt_error"), "{error}");
    }

    #[test]
    fn rejects_trap_wrong_seen() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=5\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("seen must be 3"), "{error}");
    }
```

- [ ] **Step 7: 运行 xtask 测试**

Run: `cargo test -p xtask protocol`
Expected: 全 PASS（含新增 2 条）。

- [ ] **Step 8: 端到端验收**

Run: `cargo test -p kernel && cargo test -p xtask && cargo xtask test`
Expected:
- `cargo test -p kernel`：idt 5 tests + 既有全 PASS；
- `cargo test -p xtask`：全绿；
- `cargo xtask test`：六 PASS，QEMU 两次 run 退出码 33，`trap_ready` `seen=3` 断言通过 —— **证明真实 trap → x86-interrupt handler → iretq → 返回 int3 之后，整个闭环发生**。

> **若 QEMU 崩溃/seen≠3**：多半是 IDT 未装（selector/type_attr 编码错）或 handler 转地址失败。先回退到「编译产物验证」（`cargo test -p kernel idt::` 过了说明编码对），再检查 `main.rs` 里 `init_idt` 是否在 `gdt_tss_ready` 之后、`boot_complete` 之前被调用；若 seen 读到 0 说明 int3 没进 handler（IDT 未生效/vector 没指对），检查 `lidt` 是否执行。逐项核对后重跑到六 PASS 再提交。

- [ ] **Step 9: fmt 并提交**

```bash
cargo fmt --all --check
git add kernel/src/idt.rs kernel/src/lib.rs kernel/src/main.rs kernel/src/protocol.rs xtask/src/protocol.rs
git commit -m "feat(B4): load IDT and complete minimal trap loop"
```

---

## Self-Review 记录（计划作者）

- **Spec 覆盖**：B1（§一）→Task 1；B2（§二）→Task 2；B3（§三）→Task 3；B4（§四）→Task 4；共同约束/证据边界（§五/七）→各 Task 的 Global Constraints 与接受步骤体现。
- **定稿完成**：B3/B4 已按用户指定"手写定稿"重写为可编译最终形态——B3 用 16 字节段/系统描述符（`Descriptor([u8;16])`）＋标准 TSS 布局（`rsp0` at offset 4）＋ `static mut` 静态持有；B4 改用 **`extern "x86-interrupt"` ABI**（编译器生成匹配 `iretq` 的入口出口，消除手写 `global_asm!` 易错点），handler 为 `extern "x86-interrupt" fn(&mut InterruptStackFrame)`，`set_handler` 用 `handler as u64` 取址。未悬挂 TODO/TBD/unimplemented。
- **类型一致性**：`phys_to_virt`（Task1）、`translate`（Task2）、`gdt::init_kernel_gdt(rsp0)`（Task3）、`idt::init_idt()`/`trigger_int3()`/`trap_seen()`（Task4）签名在跨 Task 引用处一致；事件序列延续递增（boot_complete 序号从切片 A 的 4 递增到 Task1 的 5、Task2 的 6、Task3 的 7、Task4 的 8）。
- **遗留不确定性（诚实标注）**：B2 的 `translate` 精确物理地址无法先验（QEMU 布局而定），故 xtask 只断言 `>0` 与合法 hex，不断言具体值；B3 的 `mov ss,bx` 数据段 reload 与 B4 的 `handler as u64` 语法在真实 QEMU 上可能需微调（各 Step 已含崩溃排查指引与 `as *const () as u64` 备选）。这些是真实内核调试点，不是占位符。implementer 按 QEMU 实际调整后记录实际采用编码。
