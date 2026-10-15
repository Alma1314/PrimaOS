# SPEC-115 / EXO-1：通用 boot image（ramdisk）装载

**日期**：2026-10-15
**阶段**：阶段 4（Exokernel 对齐系列的第一片）
**基线**：`bba61ef` + 本片工作树

## 目标

把 7 个用户态服务 ELF 从**内核映像**中移出，改由 bootloader 的 ramdisk 传入；内核只解析
通用格式头与 ELF program header，并按 opaque 块装载。

## 设计

### ramdisk 格式

`prima-abi::boot_image` 定义最小格式：magic/version/count + `(offset, len)` 条目表 +
7 个 opaque ELF 块，按 8 字节对齐。格式中没有服务名、权限或服务协议字段。

### xtask 侧

`xtask` 读取 7 个 `prima-user` artifact binary，打包
`target/prima-os/prima-os-ramdisk.img`，并通过 `BiosBoot::set_ramdisk` /
`UefiBoot::set_ramdisk` 写入 BIOS/UEFI 镜像。

### 内核侧

- `kernel/Cargo.toml` 删除 `prima-user` 依赖；内核不再 `include_bytes!` 服务 ELF。
- `kernel/src/user.rs` 新增 `install_boot_image`/`BootLayout`/`ImageEntry`，解析每个 ELF
  的 program header 并计算页偏移/页数/入口。
- `user_code()` 与 `write_image_directory()` 改为读取运行时 `BootLayout`。
- `kernel/src/main.rs` 在 bootstrap 安装后安装 boot image；失败走 `panic_event`。
- `kernel/src/vspace.rs` 增加 `VspaceError::NoBootImage`。
### 改动

| 文件 | 内容 |
|---|---|
| `prima-abi/src/lib.rs` | `boot_image` 格式常量 |
| `xtask/Cargo.toml` | 增加 `prima-abi`；把 `prima-user` artifact 依赖迁入构建工具 |
| `xtask/src/image.rs` | 打包 ramdisk；BIOS/UEFI 均 `set_ramdisk` |
| `kernel/Cargo.toml` | 删除 `prima-user` 依赖 |
| `kernel/src/user.rs` | 删除 7 个 `include_bytes!`；新增 boot image 解析/布局；动态 `user_code` 与目录 |
| `kernel/src/main.rs` | 安装 boot image 失败即 `panic_event` |
| `kernel/src/vspace.rs` | `NoBootImage` |

## 证据

### 构建与镜像

- `cargo xtask build`：`PASS build kernel`、`PASS image bios`、`PASS image uefi`。
- `target/prima-os/prima-os-ramdisk.img` 约 307,264 B；内核 ELF 不再携带 7 个服务 ELF。

### QEMU

- `cargo xtask test`：**6/6 PASS**（build kernel / image bios / image uefi / qemu bios run=1 /
  qemu bios run=2 / stage1 boot slice）。
- BIOS 两次运行：退出码 **33**、**43 条 PRIMA 事件**、`boot_complete` 正常。
- 服务仍全部由 ramdisk 装载并完成原有握手；`servicemgr_ready`、`namespace_ready`、
  `timer_ready` 等阶段 4 事件均通过既有校验。

### 相对 SPEC-114 终点的结构性变化

服务 ELF 移出内核后，内核映像与页表页数下降；事件数量与 `seq` 不变。

| 字段 | SPEC-114 报告 | 本片 |
|---|---|---|
| `frames_ready.image` | 1177 | **1117** |
| `self_root_ready.pages` | 444 | **372** |

### Host 回归

- `cargo test --workspace`：**795 passed, 0 failed**
  （cosir 146、cosir_state 42、lean_backtrace 3、kernel 376、prima-ostd 21、
  prima-runtime 1、xtask 206）。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted_lines=1262 mechanism_lines=21696 total_lines=22958 ratio=5.50%`；
  机制层 `unsafe=0`。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 未决

1. 服务名/协议 tag/共享段 witness 与 `user_code()` 机器码仍在 `kernel/src/user.rs`，属 EXO-2/3。
2. ramdisk 仍是固定 7 项顺序；EXO-2 需要把 manifest/服务选择移到 ring3 loader。
3. bootloader 将 ramdisk 标为 `Bootloader`（非 `Usable`），因此现有 `FrameInventory` 不把它
   计入；后续若要给 init 显式 `MemorySegment` cap，需要新增 boot image 帧归属与不交性证据。
4. UEFI 只构建未启动，与既有验收口径一致。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件；它是 Exokernel 对齐系列 EXO-1 的实现证据。
