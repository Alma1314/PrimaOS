# 切片 AF 设计：物理内存与页表 POD 收敛（SPEC-077）

> 文档类型：真实内核实现设计（framekernel 分层序列第 2 片）
>
> 日期：2026-09-25
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-25，方案批准：独立底层 crate / 完整序列 / 纳入 G3）
>
> 前置：切片 AE（SPEC-076）已建立 `prima-ostd` 骨架与 `arch`/`sync`，内核 lib+bin 已 `#![deny(unsafe_code)]`。

## 1. 目标

把内核里**穿透映射内存的裸访问**（`read_volatile`/`write_volatile`/`from_raw_parts`）收敛进框架 `prima-ostd::mm`，暴露 safe API。这是 unsafe 的第二大来源，也是页表/帧/用户帧/IPC 缓冲操作的唯一入口。

## 2. 站点盘点（26 处，实测）

| 类别 | 站点 |
|---|---|
| u64 读（任意已映射 VA） | `cow.rs:136`、`cpu.rs:92,163`、`preempt.rs:257`、`syscall.rs:367,574,1168,1858`、`user.rs:1139,1224`、`vspace.rs:63` |
| u64 写（任意已映射 VA） | `cow.rs:140`、`syscall.rs:1025`、`user.rs:1143`、`vspace.rs:67` |
| u8 读 | `safety.rs:15`、`syscall.rs:1552` |
| u8 写（用户代码页） | `user.rs:1350,1425,1492,1522` |
| 帧清零 | `mem.rs:19` |
| 中断帧读 | `preempt.rs:156` |
| 字节切片（静态缓冲/TSS） | `bootstrap.rs:1305,1318,4734`、`gdt.rs:274` |

## 3. 框架 API（`prima-ostd::mm`）

所有访问器为**安全** `fn`，`#[inline]`；契约写在文档里：**`va` 必须已映射且可读/可写对应的长度**，由调用方保证（内核侧由页表/帧归属不变量保证）。

```rust
pub fn read_u64(va: u64) -> u64
pub fn write_u64(va: u64, value: u64)
pub fn read_u8(va: u64) -> u8
pub fn write_u8(va: u64, value: u8)
pub fn read_u64_at(va: u64, index: u64) -> u64      // va + index*8
pub fn write_u64_at(va: u64, index: u64, value: u64)
pub fn read_u8_at(va: u64, index: u64) -> u8
pub fn write_u8_at(va: u64, index: u64, value: u8)
pub fn fill_zero(va: u64, len: u64)                 // 帧清零
pub fn bytes_of<T>(value: &T) -> &[u8]              // POD 字节视图
pub fn bytes_at_mut(va: u64, len: usize) -> &'static mut [u8]
pub fn bytes_at(va: u64, len: usize) -> &'static [u8]
```

**偏差登记（相对设计草图）**：不引入 `PhysMap{base,len}` 类型。内核已有 `mem::phys_to_virt` 纯换算，且 `PhysMap` 的"边界检查"仅由调用方传入的 base/len 决定、并不比 VA 访问器更强；引入类型会波及所有 `map` 闭包。故以 VA 访问器承载，口径与切片 AE 一致（safe-by-construction = 信任调用方，非 sound）。

## 4. 迁移映射

| 内核位置 | 改为 |
|---|---|
| `safety.rs`（整个文件） | 删除；`mem.rs` 直接调 `mm::read_u8` |
| `mem.rs::zero_frame` | `mm::fill_zero(base, FRAME_SIZE)` |
| `vspace.rs::read_raw/write_raw` | `mm::read_u64`/`mm::write_u64` |
| `cpu.rs::read_entry`、`audit_reachable_page_tables` 扫描 | `mm::read_u64_at` |
| `cow.rs::read_qword/write_qword` | `mm::read_u64`/`mm::write_u64` |
| `user.rs::read_qword/write_qword` + 代码页写入 | `mm::read_u64*`/`write_u8*` |
| `syscall.rs` 缓冲读写 | `mm::read_u64_at`/`write_u64_at`/`read_u8_at` |
| `preempt.rs` 中断帧/物理读 | `mm::read_u64` |
| `gdt.rs` TSS 字节视图 | `mm::bytes_of` |
| `bootstrap.rs` RESUME_FRAME_BUFFER 视图 | `mm::bytes_at_mut`/`bytes_at` |

## 5. 收敛进度目标

AF 后**非测试代码转为零 unsafe 的模块**：`safety`（删除）、`mem`、`vspace`、`cpu`（测试模块仍用 `alloc`，加 `#[allow(unsafe_code)]` 于测试模块，非整个模块）。`cow`/`user`/`syscall`/`bootstrap`/`preempt`/`gdt` 的**静态可变状态**属 AG，本片不处理，其模块级 allow 保留。

## 6. 回归面

预期**事件序列与 seq 不变**。本片只替换等价的内存访问形态（同一 `read_volatile` 语义），**不改逻辑**；镜像尺寸可能因代码布局轻微变化，若 2 条镜像相关事件（`bootstrap_ready.image_len`、`preempt_ready` 栈地址）再次移动，按维护规则 5 登记。其余事件应逐字段不变，以 HEAD 独立 worktree 手工 diff 复核。

## 7. 验证

- `cargo test --workspace`（五组计数不下滑）
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`
- `cargo xtask test`（35 条事件序列 + 校验器）、`cargo xtask compat`
- `cargo build -p kernel --target x86_64-unknown-none`
- 事件流逐条 diff（HEAD worktree 对照）

## 8. 口径偏差登记

1. safe-by-construction ≠ sound：VA 访问器信任调用方保证映射有效；误用产生 UB，设计不宣称已消除。
2. `bytes_at`/`bytes_at_mut` 返回 `&'static` 借出静态缓冲内存；调用方不得制造重叠可变别名（单核顺序访问契约，同 `StaticCell`）。
3. 不引入 `PhysMap` 类型（见 §3）。
