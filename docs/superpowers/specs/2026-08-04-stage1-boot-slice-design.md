# Prima OS 阶段 1 启动闭环设计

> 设计日期：2026-08-04
>
> 修订日期：2026-08-05
>
> 状态：已批准
>
> 对应路线：[[Prima OS]] 阶段 1、[[Prima OS 实现阶段]] S1-D1/S1-D2

## 1. 目标

在 Windows 11 主机上建立 Prima OS 首个可重复、无人值守的 QEMU x86_64 BIOS 启动闭环：构建 Rust freestanding kernel，生成 BIOS 与 UEFI 镜像，通过 COM1 输出机器可解析事件，并使用 `isa-debug-exit` 确定性退出。

唯一验收入口为：

```bash
cargo xtask test
```

该切片只验证构建、启动、串口协议、退出协议和测试自动化。它是阶段 1 的探索性实现，不解除阶段 0 的 `BLK-001` 或 `BLK-002`，也不代表阶段 1 已完成。

## 2. 范围

### 2.1 包含

- 在项目根目录建立 Cargo workspace；
- 使用固定日期的 Rust nightly；
- 安装并固定声明式工具链组件 `rust-src`、`llvm-tools-preview`、`rustfmt` 与 `clippy`；
- 使用 `bootloader` crate 生成 BIOS 和 UEFI 磁盘镜像；
- 使用 `bootloader_api` 接收启动信息；
- 构建 `x86_64-unknown-none`、`no_std`、`no_main` 内核；
- 初始化 COM1 并输出版本化 ASCII 事件；
- 通过 QEMU `isa-debug-exit` 表达成功或失败；
- 由 Rust `xtask` 统一构建、运行、解析和验证；
- 在当前 Windows 11 环境通过 `winget` 安装 QEMU；
- 自动运行两次 BIOS 启动测试以检查可重复性；
- UEFI 镜像必须可生成，但首轮不要求自动启动验证。

### 2.2 不包含

- 物理内存发现及页帧归属建模；
- 重建或管理页表；
- GDT、IDT、异常与中断处理框架；
- POSIR Bootstrap、init Task 或初始 Capability；
- 阶段 0 的可执行形式化状态模型；
- x86_64 `MachineState → POSIRState` 细化关系；
- Linux 主机验证、CI、UEFI 自动启动测试；
- 通用日志框架或运行时格式化系统。

## 3. 架构

项目采用 Cargo workspace 与 artifact dependency：

```text
PrimaOS/
├── Cargo.toml
├── Cargo.lock
├── rust-toolchain.toml
├── .cargo/
│   └── config.toml
├── kernel/
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
└── xtask/
    ├── Cargo.toml
    └── src/
        └── main.rs
```

### 3.1 `kernel`

`kernel` 是目标端 freestanding 二进制，职责仅限：

1. 从 `bootloader_api` 进入内核入口；
2. 初始化 COM1；
3. 发送固定启动事件；
4. 在成功或 panic 时写入 `0xF4`；
5. 写入退出端口后进入不可返回的停机循环，以防不支持该设备时继续执行。

它不负责主机构建编排、磁盘镜像生成或 QEMU 进程管理。

### 3.2 `xtask`

`xtask` 是 Windows 主机工具，提供三个命令：

- `cargo xtask build`：构建内核并生成 BIOS/UEFI 镜像；
- `cargo xtask run`：构建后以固定参数交互式启动 BIOS 镜像；
- `cargo xtask test`：构建、运行两次 QEMU、解析事件并给出稳定验收摘要。

`xtask` 必须使用明确路径和显式参数，不扫描或猜测其他镜像。它可以从 `PATH` 定位 `qemu-system-x86_64`；找不到时提供约定的 `winget` 安装提示并失败。

### 3.3 构建技术

- `kernel` 使用 `x86_64-unknown-none`；
- `bootloader` 作为主机侧镜像构建依赖；
- `kernel` 作为指定 target 的 Cargo binary artifact dependency；
- 工具链、crate 版本和 `Cargo.lock` 均固定；
- BIOS 与 UEFI 产物复制到稳定路径：

```text
target/prima-os/prima-os-bios.img
target/prima-os/prima-os-uefi.img
```

## 4. 数据流

### 4.1 构建流

```text
cargo xtask build
  → Cargo 以 x86_64-unknown-none 构建 kernel ELF
  → bootloader 生成 BIOS 磁盘镜像
  → bootloader 生成 UEFI 磁盘镜像
  → xtask 将产物放入 target/prima-os/
```

构建成功要求两个镜像均存在且长度大于零。

### 4.2 运行流

```text
cargo xtask run
  → 定位 qemu-system-x86_64
  → 以 BIOS 镜像和固定机器参数启动
  → COM1 连接当前终端
  → 内核写入 0xF4 后 QEMU 退出
```

固定参数至少包含：

```text
-display none
-serial stdio
-no-reboot
-device isa-debug-exit,iobase=0xf4,iosize=0x04
```

QEMU 11.0.50 Windows 实测表明，`isa-debug-exit` 的端口写入是 guest shutdown 请求；`-no-shutdown` 会抑制该请求，导致 QEMU 进程不退出并最终触发测试超时。因此，自动测试 profile 必须保留 `-no-reboot`，不得传递 `-no-shutdown`。

BIOS drive 使用 `format=raw,file=...`，不设置 `readonly=on`。QEMU 11.0.50 Windows 实测在 bootloader 启动前对 `readonly=on` 报告 `Block node is read-only`；每次 `xtask build` 或 `xtask test` 调用都会删除并重建镜像。当前 kernel 不执行块 I/O，因此该可写 raw image 例外不引入跨命令 guest 状态证据。用 `xtask run` 时同样经过构建流。统一使用该可写 raw image profile，不得把 `readonly=on` 加回自动测试参数。

实现还必须固定机器类型和内存大小。测试模式不得打开图形窗口。该 profile 的完整参数以实现中的固定参数测试为准。任何后续参数调整都必须以实际 QEMU 版本验证并同步本设计。确定性退出仍要求 guest 成功值 `0x10` 映射为主机退出码 `33`，panic 值 `0x11` 映射为 `35`。命令超时仍是错误处理保护，不是正确性证据。

### 4.3 测试流

```text
cargo xtask test
  → build
  → 启动 BIOS run=1
  → 捕获串口、等待退出、验证协议
  → 启动 BIOS run=2
  → 捕获串口、等待退出、验证协议
  → 输出稳定 PASS 摘要
```

两次启动都必须独立创建并回收 QEMU 子进程，且获得相同的协议事件序列。

## 5. 串口事件协议

事件采用单行、版本化、固定字段顺序的 ASCII 格式：

```text
PRIMA|v=1|seq=<u32>|event=<name>|status=<status>
```

首轮成功序列严格为：

```text
PRIMA|v=1|seq=1|event=boot_start|status=ok
PRIMA|v=1|seq=2|event=serial_ready|status=ok
PRIMA|v=1|seq=3|event=boot_complete|status=ok
```

约束：

- 每条事件以 CRLF 或 LF 结束，解析时只规范化行尾；
- 字段名称和顺序固定；
- `seq` 必须从 1 开始严格递增；
- 成功事件不得缺失、重复或乱序；
- 固件或 bootloader 的非 `PRIMA|` 输出可以忽略；
- 任何以 `PRIMA|` 开头但不符合协议的行都必须导致测试失败；
- 首轮只实现输出这些固定事件所需的最小串口函数，不建立通用日志抽象。

COM1 初始化完成前无法可靠报告事件，因此 `boot_start` 表示内核入口已到达并完成最低限度串口可写准备；`serial_ready` 表示串口配置检查点已通过。该定义避免声称在串口可用之前已经观测到事件。

## 6. 退出与错误处理

### 6.1 内核退出协议

- 成功：向 I/O port `0xF4` 写入 `0x10`；QEMU 预期主机退出码为 `33`，即 `(0x10 << 1) | 1`；
- panic：输出一条结构化失败事件，再向 `0xF4` 写入 `0x11`；预期退出码为 `35`；
- panic 事件格式：

```text
PRIMA|v=1|seq=<n>|event=panic|status=error
```

panic 输出不包含源码路径、行号或自由文本，以保持验证结果稳定。`seq` 使用下一个可用序号；实现必须保证正常路径的三个序号固定。

### 6.2 `xtask` 错误规则

以下任一情况返回非零：

- Cargo 或镜像构建失败；
- BIOS/UEFI 镜像缺失或为空；
- 找不到 QEMU；
- QEMU 未在固定超时内退出；
- QEMU 退出码不是当前测试期望值；
- 事件缺失、重复、乱序或格式非法；
- 捕获到 panic 事件；
- 子进程无法创建、等待或终止。

超时时，`xtask` 只终止本次测试创建的 QEMU 子进程，保留并报告已捕获输出。超时是测试失败保护，不构成正确性证据。

Windows 对进程退出码的呈现按 `std::process::ExitStatus` 的平台行为校验；验收测试必须用实测确认 `isa-debug-exit` 的退出码映射，不能静默接受任意非零值。

## 7. 测试设计

### 7.1 `xtask` 单元测试

事件解析器至少覆盖：

- 接受完整合法序列；
- 拒绝缺失事件；
- 拒绝乱序事件；
- 拒绝重复事件；
- 拒绝非法字段、版本、状态或序号；
- 忽略不以 `PRIMA|` 开头的固件输出；
- 拒绝 panic 事件；
- 区分成功和失败 QEMU 退出码。

### 7.2 构建验证

- kernel ELF 构建成功；
- BIOS 镜像存在且非空；
- UEFI 镜像存在且非空；
- 产物位于稳定路径。

### 7.3 QEMU 集成验证

BIOS 镜像连续独立运行两次。每次都必须：

- 在超时前退出；
- 返回预期成功退出码；
- 恰好产生三条合法成功事件；
- 事件序列完全一致；
- 不产生 panic 或非法 `PRIMA|` 行。

### 7.4 稳定摘要

全部通过时输出：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

诊断细节可以输出到 stderr，但不得伪造或提前输出 PASS 行。

## 8. 工具链与主机环境

首轮已验证主机范围为 Windows 11。实现应避免无必要的 Windows 专属路径拼接，但 Linux 运行暂不属于验收条件。

开发前使用 `winget` 安装 QEMU。安装属于共享主机状态变更，只执行用户已明确授权的 QEMU 包安装，不安装其他非必要软件。

仓库固定：

- 一个明确日期的 Rust nightly；
- `rust-src`；
- `llvm-tools-preview`；
- `rustfmt`；
- `clippy`；
- `bootloader` 和 `bootloader_api` 精确版本；
- `Cargo.lock`；
- QEMU 机器类型、内存大小和全部启动参数；
- 测试超时。

若选定 nightly 与依赖版本不兼容，应调整到一个经过实际构建验证的固定组合，并在实施记录中写明最终版本；不得改为跟随最新 nightly。

## 9. 验收与阶段证据

实现完成的唯一功能验收命令是：

```bash
cargo xtask test
```

只有该命令在实际安装的 QEMU 上完整通过，才能在 `Prima OS 实现阶段.md` 中登记证据。登记内容包括：

- 测试日期；
- 完整命令；
- 固定工具链与 QEMU 版本；
- PASS 摘要；
- 对应源码工件；
- 支持 `S1-D1` 与 `S1-D2` 的证据 ID。

进度更新必须保持以下边界：

- 当前阶段仍是阶段 0；
- `BLK-001`、`BLK-002` 保持 Open；
- 可把 `S1-D1`、`S1-D2` 标记为已有探索实现和测试证据；
- `S1-D3`、`S1-D4`、`S1-D5` 仍未完成；
- `S1-X1` 可记录无人值守 BIOS 启动证据；
- `S1-X2` 与 `S1-X3` 不得勾选，因为 Bootstrap/Authority 和页帧归属尚未实现；
- 阶段 1 状态保持“进行中”，不得标记为“已满足退出条件”。

## 10. 明确非声明

本切片通过后仍不得声称：

- Prima OS 已实现内存隔离或对象能力安全；
- POSIR 状态机已有机器证明；
- Rust 内核是 POSIR 的正确细化；
- init Task 已建立或持有正确 Authority；
- 页帧唯一归属不变量已成立；
- 阶段 0 或阶段 1 已满足全部退出条件；
- UEFI 启动路径已通过运行验证；
- Linux 主机构建已受支持。
