---
id: report-p5-1-acpi-madt-host
kind: report
authority: scoped-host-and-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-27
---

# P5-1：ACPI/MADT 解析与 2/4 vCPU 只读探测

## 结论与范围

本切片完成 [S7-B.1 设计](../specs/2026-11-27-stage7-smp-state-partition.md) P5-1：
**解析器 + 只读探测**。

1. 新增 `kernel/src/acpi.rs`：RSDP v0/v2、RSDT/XSDT 表项与 MADT CPU 拓扑的纯解析
   （零 unsafe，接收 `&[u8]`）。
2. 新增 `kernel/src/smp_probe.rs`（仅 `smp-probe` feature）：从 `BootInfo.rsdp_addr`
   经物理直映射读表，找到 MADT，用 CPUID 与 LAPIC 自识别 BSP 的 APIC ID。
3. 新增专用 `cargo xtask test-smp`：同一镜像分别以 `-smp 2` 与 `-smp 4` 启动两轮，
   校验 SMP 见证 **并** 复用既有全部启动校验器。

**这不是 SMP**：AP 未启动（`online=1`），`cpus=` 只是固件报告；没有每 CPU TSS、IPI、
TLB shootdown、调度分区或并行任务。7-B 四项退出条件一项未勾选，P5 未签署，P6 未开始。

## 基线与输入

- 代码基线：`eb8623f`（P5-0 已提交）。
- 上位：[S7-B.1 SMP 状态所有权与按 CPU 分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)
  P5-1；[阶段 7 执行计划](../plans/2026-11-06-stage7-execution.md)。
- QEMU：仓库既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、128 MiB、`isa-debug-exit`；
  仅把 `-smp 1` 替换为 2/4，其余参数冻结。SeaBIOS 提供 RSDP rev 0（RSDT，无 XSDT）。

## 实现内容

| 路径 | 内容 |
|---|---|
| `kernel/src/acpi.rs` | `parse_rsdp`（v0 前 20 字节 / v2 长度与 36 字节扩展校验和）、`table_phys`（RSDT u32 / XSDT u64、声明长度与整表校验和）、`parse_madt`（type 0/9、enabled 过滤、x2APIC 优先、4 CPU 上限、条目边界）、`CpuTopology::{local_apic_phys,count,apic_id,index_of,uses_x2apic}` |
| `kernel/src/smp_probe.rs` | `run(rsdp_phys, phys_offset)`：读 RSDP → 选 XSDT/RSDT → 逐项找 `APIC` → 解析拓扑 → CPUID leaf 1 `EBX[31:24]` → `index_of` → 读 LAPIC ID（xAPIC `0xFEE00020` bit24..31 / x2APIC MSR `0x802`）比对；表长上限 1 MiB；错误确定性返回 |
| `prima-ostd/src/{arch,mm}.rs` | 新增安全原语 `arch::cpuid(leaf, subleaf)`（内部保存/恢复 rbx）与 `mm::read_u32` |
| `kernel/src/main.rs`、`protocol.rs` | 仅 `smp-probe` 下在物理直映射就绪后运行探测，输出 `SMP\|begin`、发现行与 `SMP\|PASS`；失败打印 `SMP\|FAIL\|<reason>` 后继续正常启动，专用校验器要求完整见证 |
| `xtask/src/{main,qemu,protocol}.rs`、`Cargo.toml` | 新增 `smp-probe` feature 与 `test-smp`：`-smp 2/4` 各两轮、写 `target/prima-os/SMP-{2,4}-{1,2}.log`、校验 CPU 数/BSP 索引/LAPIC 一致/`online=1`，并复用既有启动校验器 |

探测只读：不改页表、不写全局状态、不发 IPI；默认镜像（无 feature）不编译该模块。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features smp-probe -- test-smp` | PASS：`-smp 2/4` × 2 轮；exit=33；每轮复用既有启动校验器全部通过 | QEMU BIOS，2/4 vCPU 平台（AP parked） |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1245 passed、0 failed、1 ignored | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（46 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p kernel --features smp-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：完整 BIOS 套件 36 程序/72 轮、stage1 boot slice（默认单核镜像不变） | QEMU BIOS，单核 |

关键见证（`SMP-2-1.log` / `SMP-4-1.log`）：

```text
SMP|begin
SMP|rsdp=0;xsdt=0;cpus=2;bsp_apic=0;bsp_index=0;lapic=0xfee00000;lapic_id=0;x2apic=0;online=1
SMP|PASS
```

`-smp 4` 仅 `cpus=4` 不同。日志 SHA-256：`SMP-2-{1,2}` 均为
`6b5c6fda259925c4e1ba670a0928e59afe87c19ecc1e66d707d3b4d1b2bae9ac`；`SMP-4-{1,2}`
均为 `01766751f76c1aa58045aeb4606c53b618f7500234bdd0b26dc2532781c6e70f`。
剔除 `SMP|` 行后，2 vCPU 与 4 vCPU 的启动输出逐字节相同，且通过同一组既有校验器
（IPC 计数、服务 trace、原生应用、容量、tty、用户故障）。

新增 host 用例：8 个 ACPI 解析用例（校验和、v0/v2、条目边界、enabled、x2APIC 优先、
CPU 上限、RSDT/XSDT 项宽）、1 个探测表长拒绝、1 个 `validate_smp` 见证正反例。

## 失败、反例与限制

- 首次 bare-metal 编译失败：显式 `out("ebx")` 被 LLVM 拒绝（rbx 为保留寄存器）；改为
  保存/恢复 rbx 的通用寄存器序列后通过。这是实现修正，不是放宽断言。
- **AP 未启动**：`online=1` 只表示 BSP 探测完成；`cpus=2/4` 来自 MADT，不代表 AP 可被
  唤醒或参与调度。
- QEMU/SeaBIOS 给出 RSDP rev 0 + RSDT，**XSDT 与 x2APIC 路径未被真实表覆盖**；只有宿主
  合成用例。LAPIC 读取走 xAPIC MMIO；x2APIC 分支（MSR `0x802`）未经 QEMU 验证。
- CPUID 只取 leaf 1 的 8 位初始 APIC ID；不处理 x2APIC ID >255 的机器，也不处理 MADT
  LAPIC 地址覆盖。
- 物理直映射访问固件保留区依赖 bootloader；本机通过，但未在其它固件/内存区验证。
- `test-smp` 镜像运行完整正常启动后才退出，SMP 行与既有事件共存；默认镜像不包含探测。
- 无真实硬件、无 Windows/WIN-D3 交互；不改 profile、不勾选 7-B。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`。
阶段 7 的 P5 推进到“P5-1 完成：解析 + 2/4 vCPU 只读探测”，P5 未签署，P6 未开始。

## 后继关系

- 前序：[P5-0 SMP 状态所有权设计](2026-11-27-p5-0-state-partition.md)。
- 后继：P5-2 AP 启动（实模式跳板、INIT-SIPI-SIPI、每 CPU GDT/TSS/栈、在线握手）；
  需先确定 AP 失败策略与内核 GS/`swapgs` 方案（设计 §7 待决 1/2）。
