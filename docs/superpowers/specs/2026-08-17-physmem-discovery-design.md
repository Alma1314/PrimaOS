# 物理内存发现（physmem discovery）设计

> 文档类型：真实内核实现设计（S1-D3）
>
> 日期：2026-08-17
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-08-17）

## 1. 背景与动机

垂直切片固定为 **COW/page fault → handler → `exc_resume` → TLB → Linux trace**。前三节点（COW / `exc_resume` / TLB）已在 `cosir` host 模型层实现并合并回 master，证据仅为 `MODEL-TEST`；真实 trap、页表、TLB 尚未兑现（规范 §18.0 边界清单 #1/#2/#5/#6）。

本切片是垂直切片走向真实内核的**第一步**，对应 tracker S1-D3「物理内存发现」。真实页表建立（S1-D4）必须先知道哪些物理帧可自由使用才能分配页表页，因此本切片落在依赖链最前端。

当前 `kernel/` 只有启动闭环（`bootloader_api` 0.11.17 + 串口协议 + QEMU isa-debug-exit 退出），`kernel_main` 完全忽略 `BootInfo`，没有任何页表/内存发现逻辑。

## 2. 目标

- 把 `kernel` 拆成 **lib（安全纯逻辑）+ bin（薄入口）**，使帧清单逻辑可 host 单测（承接 G2「safe 机制 crate」方向，不新增 unsafe）。
- 在真实 bootloader 交接处解析 `BootInfo.memory_regions`，构建**物理帧清单**：`frame_universe`（Usable 区 4KB 帧）与保留区（内核镜像 `image_frames`），并给出 `free_frames = usable - image`。
- 校验 §14 bootstrap 前置（镜像非空且 `image_frames ⊆ frame_universe`），并保证 `image ∩ free = ∅` 由构造成立（W6 帧级）。
- 以 host 单测（确定性覆盖边界情形）+ QEMU boot-trace 集成断言（真实交接）双证据收尾。

## 3. 非目标

- **不实现帧分配器**（§18.0 #12 的 buddy/bitmap/free list 选择留给切片 B，页表需要分配页表页时再引入）。
- **不建立真实页表**（§20.5/S1-D4）、不装载 GDT/TSS/IDT、不接 trap。
- **不读写物理内存**：默认配置 `physical_memory_offset = None`（无物理内存直映射），本切片仅做清单，不接触任何物理字节。
- 不宣称 COW 切片的端到端 `KERNEL-TRACE` 或任何 α 细化证明。

## 4. 架构

`kernel` 拆 lib + bin：

- `kernel/src/lib.rs`：`#![no_std]`，`pub mod mem;`（唯一安全纯逻辑模块），`pub use mem::{...}`。
- `kernel/src/main.rs`：保留 `#![no_main]` + `entry_point!`，`mod protocol/qemu/serial`，`use kernel::mem`，把 `&BootInfo` 转成 `&[MemoryRegion]` 喂给 `mem::build_inventory`，再报告/退出。
- `kernel/Cargo.toml`：新增 `[lib] name="kernel" path="src/lib.rs"`，保留 `[[bin]] name="prima-kernel"`。

lib 依赖 `bootloader_api`；其 `MemoryRegion` / `MemoryRegionKind` 类型在 host 可构造（pub 字段、`no_std` 兼容），故纯逻辑可 host 单测。切片 A **不新增 unsafe**。

### 4.1 数据结构与纯函数

```rust
pub const FRAME_SIZE: u64 = 4096;

pub struct FrameInventory {
    pub usable_frames: usize,   // Usable 区 4KB 帧总数
    pub image_frames: usize,    // 内核镜像保留帧数
    pub free_frames: usize,     // usable - image
    pub usable_regions: usize,  // Usable 区个数
    pub kernel_phys_start: u64, // align_down(kernel_addr)
    pub kernel_phys_end: u64,   // align_up(kernel_addr + kernel_len)
}

pub enum MemError {
    EmptyImage,                    // kernel_len == 0
    ImageNotUsable,                // 镜像帧未完全落在 Usable 区
    RegionsNotSortedOrOverlapping, // 区域未按地址排序/重叠（防重复计数，护 W6）
}

pub fn build_inventory(
    regions: &[MemoryRegion],
    kernel_addr: u64,
    kernel_len: u64,
) -> Result<FrameInventory, MemError>;
```

### 4.2 帧计数语义

- 帧区间统一按 `[align_up(start, FRAME_SIZE), align_down(end, FRAME_SIZE))` 截取；非 4KB 对齐的边界页丢弃，不计入。
- 只统计 `MemoryRegionKind::Usable` 区；`Bootloader` / `UnknownUefi` / `UnknownBios` 区忽略（bootloader 已把自身占用标为 `Bootloader`）。
- 区域按地址排序且不重叠：`region[i].start >= region[i-1].end`，否则 `RegionsNotSortedOrOverlapping`（重叠会导致重复计数，破坏 W6 帧级唯一归属）。
- 镜像帧 `[align_down(kernel_addr), align_up(kernel_addr + kernel_len))` 必须全部落在 Usable 区（§14 前置 `image_frames ⊆ frame_universe`）；`kernel_len == 0` 则 `EmptyImage`。
- `free_frames = usable_frames - image_frames`；镜像完全在 Usable 内时该差即真正的空闲帧数。

## 5. 协议扩展

PRIMA 记录从固定 5 字段扩为「5 或 6 字段」，新增可选第 6 字段 `data`（`;` 分隔 `k=v` 对）：

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=126976;image=32;free=126944;regions=3;kstart=0x0;kend=0x20000
```

- 成功事件序列：`boot_start`(1) → `serial_ready`(2) → `memory_ready`(3) → `boot_complete`(4)。
- 错误路径：`boot_start` → `serial_ready` → `memory_error`（`data` 含 `err=<variant>`）→ 直接 `qemu::exit(PANIC)`（**不经过** Rust panic handler，保留结构化错误信号）。
- `xtask/src/protocol.rs`：`parse_line` 接受 5/6 字段；`Event` 增加 `data: Option<String>`（原始 data 载荷），仅对 `memory_ready` 解析为 `MemoryData` 结构（`usable/image/free/regions/kstart/kend`）；`validate_output` 拒绝 `panic`/`memory_error`，校验 4 事件序列，并对 `memory_ready` 断言结构不变式（见 §7）。

## 6. 错误处理

`kernel_main` 分支：

- `Ok(inv)` → `protocol::memory_ready(&inv)` + `protocol::boot_complete()` → flush → `qemu::exit(SUCCESS)`。
- `Err(e)` → `protocol::memory_error(e)` → flush → `qemu::exit(PANIC)`。

`MemError` 是 bootloader 交接输入的唯一失败面；三种错误均映射到 `memory_error`，不外泄其他错误原语，状态在退出前保持确定。

## 7. 测试与验收

### 7.1 host 单测（`kernel/src/mem.rs` `#[cfg(test)]`，约 10 例）

单 Usable 区帧计数；非 4KB 对齐边界页丢弃；非 Usable 区忽略；镜像保留并计 `free = usable - image`；镜像非对齐向上取整（`kstart/kend`）；`EmptyImage`；`ImageNotUsable`；重叠/乱序区域报错；多 Usable 区求和；镜像跨连续 Usable 区正常。

### 7.2 QEMU 集成（xtask）

`cargo xtask test` 两次 run 均要求：QEMU 退出码 33；合法 4 事件序列；`memory_ready` 数据满足

```
free > 0 ∧ image > 0 ∧ usable == image + free ∧ kstart % 4096 == 0 ∧ kend % 4096 == 0 ∧ kstart < kend
```

### 7.3 验收命令（对齐 TEST-006 口径）

```
cargo fmt --all --check
cargo test -p kernel
cargo test -p xtask
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

## 8. 证据边界（措辞纪律）

本切片仅证明：真实 bootloader 交接处，内核正确解析内存图并算出满足 §14 bootstrap 前置（镜像非空且 ⊆ usable）的帧清单，`image ∩ free = ∅` 由构造保证（W6 帧级）。

**不宣称**：

- 帧分配器（§18.0 #12）；
- 真实页表 / CR3 / PTE（§20.5 / S1-D4）；
- 物理内存读写（默认配置无 `physical_memory_offset` 映射）；
- COW 切片端到端 `KERNEL-TRACE`；
- `REFINEMENT-PROOF` 或机器证明（BLK-001 仍 Open）。

本切片证据 = host 单测（真实内核 lib 的纯逻辑）+ QEMU boot-trace 集成断言；**不是** COW 垂直切片的端到端 trap 轨迹。

## 9. 增量提交划分（供计划阶段参考）

1. `kernel` 拆 lib+bin（`lib.rs` + `mem.rs` 骨架 + `Cargo.toml` 增 `[lib]`）。
2. `mem::build_inventory` + `MemError` 实现（含 4KB 对齐、Usable 过滤、镜像保留、不变式校验）。
3. `mem` host 单测（约 10 例）。
4. `main.rs` 接入 `build_inventory`，`protocol.rs` 新增 `memory_ready`/`memory_error` + data 编码。
5. `xtask/src/protocol.rs` 扩展 6 字段解析 + `MemoryData` 断言 + 事件序列更新 + 对应单测。
6. 全量验收命令与证据记录更新到实现阶段文档 tracker（新增 SPEC 证据行）。
