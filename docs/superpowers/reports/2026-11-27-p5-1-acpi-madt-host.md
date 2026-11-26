---
id: report-p5-1-acpi-madt-host
kind: report
authority: scoped-host-evidence
status: host-tests-pass-limits
lifecycle: current
updated: 2026-11-27
---

# P5-1 部分：ACPI RSDP/RSDT/XSDT 与 MADT 解析宿主模型

## 结论与范围

本切片是 [S7-B.1 设计](../specs/2026-11-27-stage7-smp-state-partition.md) P5-1 的第一部分：
新增 `kernel/src/acpi.rs`，对调用方给出的字节切片做 **RSDP v0/v2、RSDT/XSDT 表项与
MADT CPU 拓扑**的纯解析，并用 8 个 host 用例钉住边界。

**未接线**：不读取 `BootInfo.rsdp_addr`、不访问 LAPIC MMIO、不做 CPUID、不启动 AP、
不改 QEMU `-smp`。这是解析器宿主模型，不是 SMP 或任何机器证据；不能替代 P5-2。

## 基线与输入

- 代码基线：`eb8623f`（P5-0 已提交）。
- 上位：[S7-B.1 SMP 状态所有权与按 CPU 分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)
  P5-1；[阶段 7 执行计划](../plans/2026-11-06-stage7-execution.md)。
- 约束：`kernel/src` 零 `unsafe`（`deny(unsafe_code)`），故解析只接收 `&[u8]`；
  物理读取留到 P5-2，经 `prima_ostd::mm::bytes_at` 或等价安全访问器。

## 实现内容

| 项 | 内容 |
|---|---|
| `parse_rsdp` | 校验 `RSD PTR `、v0 前 20 字节校验和；v2 还校验声明长度（≥36、≤输入）与 36 字节扩展校验和；返回 RSDT 物理地址与可选 XSDT |
| `table_phys` | 按签名 `RSDT`/`XSDT` 取第 `index` 项（u32/u64），校验表头声明长度、整表校验和与项边界 |
| `parse_madt` | 校验 `APIC` 表头与校验和；遍历条目，只接受长度合法的 type 0（Local APIC，≥8 B）与 type 9（x2APIC，≥16 B）；只计 enabled 位；x2APIC 条目存在时优先；超过 `MAX_CPUS=4` 返回 `TooManyCpus`；无启用条目返回 `NoCpu` |
| `CpuTopology` | 暴露 LAPIC 基址、CPU 数、按序号取 APIC ID、按 APIC ID 反查索引、是否 x2APIC；索引次序即 MADT 次序，BSP 归零由 P5-2 负责 |

条目遍历对 `entry_len < 2`、越出声明长度与固定头不足确定性返回 `BadLength`，不做
无限循环或截断接受。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib acpi::` | PASS：8/8 | host |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1244 passed、0 failed、1 ignored | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（45 文件）、S6-X3 | host/静态 |

新增用例：RSDP v2 双校验和与两表地址、RSDP v0 无 XSDT、MADT 只收 enabled 且保持次序、
x2APIC 优先、截断/坏校验和/无 CPU 拒绝、条目长度越界拒绝、超过 `MAX_CPUS` 拒绝、
RSDT/XSDT 项宽与越界。未运行 `cargo xtask test`：本模块不在启动路径上，既有 36 组
72 轮证据不变。

## 失败、反例与限制

- 宿主机夹具是合成的 ACPI 字节，**没有**与 QEMU/真实固件导出的表做对照；解析器按
  ACPI 字段布局编写，真实表验证属 P5-2。
- 只识别 type 0 与 type 9；不解析 IOAPIC、中断覆盖、MCFG、SRAT、AML。
- 未处理 RSDP 位于 EBDA/只读区、XSDT 缺失时的回退选择、表地址为 0 或不可映射等
  运行期条件；这些是 P5-2 的读取层职责。
- 不验证 X2APIC ID 的连通性、不读取 LAPIC 版本/ID 寄存器、不做 CPUID 自识别。
- 解析器当前未被任何生产路径调用；`cargo xtask lint` 只证明它在 bare-metal 目标可编译。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或 QEMU 参数。
P5-1 仍为部分交付，P5 未签署、P6 未开始。

## 后继关系

- 前序：[P5-0 SMP 状态所有权设计](2026-11-27-p5-0-state-partition.md)。
- 后继：P5-2 读取 `BootInfo.rsdp_addr` 与物理表、映射并探测 LAPIC、CPUID 自识别与
  CPU 索引表；随后 AP 启动、每 CPU TSS 与在线握手。
