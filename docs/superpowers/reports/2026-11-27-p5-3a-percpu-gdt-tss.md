---
id: report-p5-3a-percpu-gdt-tss
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-27
---

# P5-3a：每 CPU GDT/TSS 与共享 IDT

## 结论与范围

本切片补齐 [P5-2 AP 启动原型](2026-11-27-p5-2-ap-bringup.md) 明确留下的
“没有每 CPU GDT/TSS/IDT”限制：

1. `kernel/src/gdt.rs` 把单份 GDT/TSS 改为每 CPU 槽（`MAX_CPUS=4`）：`init_cpu_gdt`
   为指定 CPU 装配并装载 GDT/TSS，BSP 走槽 0，AP 在自己的入口装载各自槽位。
2. AP 装载 BSP 已初始化的**只读共享 IDT**；BSP 在 `init_idt` 之后才启动 AP，避免 AP
   装载未初始化表。
3. 见证从“读回 TR 选择子”加强为**从本 CPU 的 GDTR 与 TR 解出 TSS 描述符基址**
   （`sgdt` + `str`）：每个 AP 必须读回自己槽位的 TSS 基址，BSP 再校验 BSP+AP 的 TSS
   基址互异。

**这不是调度**：AP 仍 `hlt` 且 IF=0，不参与调度、不接收中断，没有可运行队列、IPI、
TLB shootdown 或并行 fork/exec。P5-3 整体未完成，7-B 四项退出条件均未勾选。

## 基线与输入

- 代码基线：`5513c63`（P5-2 AP 启动原型）。
- 上位：[S7-B.1 设计](../specs/2026-11-27-stage7-smp-state-partition.md) P5-2/P5-3；
  [阶段 7 执行计划](../plans/2026-11-06-stage7-execution.md)。
- QEMU：既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、128 MiB；仅 `-smp 2/4`，其余冻结。

## 实现内容

| 路径 | 内容 |
|---|---|
| `kernel/src/gdt.rs` | `GDTS`/`TSSES: [StaticCell<...>; MAX_CPUS]`；`init_cpu_gdt(cpu, rsp0)`（越界确定性失败）、`init_kernel_gdt`＝BSP 槽；`set_rsp0_for(cpu,..)` 与 BSP 的 `set_rsp0`；`tss_base(cpu)`；`decode_tss_base` 与 `loaded_tss_base()`（`sgdt`+`str` 解本 CPU 描述符） |
| `kernel/src/idt.rs` | `load_shared()`：AP 装载 BSP 同一只读 IDT |
| `kernel/src/smp.rs` | `ap_entry` 装载本 CPU GDT/TSS 与共享 IDT，记录自身描述符 TSS 基址后置在线位；`verify_per_cpu` 校验 BSP+AP 基址互异且每个 AP 读回本槽；失败返回 `ap-percpu-init` |
| `kernel/src/main.rs` | AP 启动移到 `init_idt` 之后（共享 IDT 已初始化）；探测结果在 BSP 早期暂存 |
| `prima-ostd/src/arch.rs` | `sgdt()` 与 `task_register()`（均为 CPL0 只读原语） |
| `kernel/src/protocol.rs`、`xtask` | 见证扩展为 `SMP\|aps=N;online=N+1;percpu=N;tss=distinct`；专用校验器要求 AP 数、在线数、每 CPU 数与 `tss=distinct` 同时成立 |

每 CPU GDT/TSS 的选择子布局与 BSP 相同（内核 CS=0x8、DS=0x10、TSS=0x18），故 AP 装载
共享 IDT 后，门的 0x8 选择子在本 CPU 仍指内核代码段。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features smp-ap -- test-smp-ap` | PASS：`-smp 2/4` × 2 轮；`percpu=1/3`、`tss=distinct`；exit=33；复用既有启动校验器 | QEMU BIOS，2/4 vCPU |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1248 passed、0 failed、1 ignored | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（47 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p kernel --features smp-ap --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：完整 BIOS 套件 36 程序/72 轮、stage1 boot slice（默认单核镜像不变） | QEMU BIOS，单核 |

关键见证（`SMPAP-4-1.log`；`SMP|stage=` 行省略）：

```text
SMP|rsdp=0;xsdt=0;cpus=4;bsp_apic=0;bsp_index=0;lapic=0xfee00000;lapic_id=0;x2apic=0;online=1
SMP|aps=3;online=4;percpu=3;tss=distinct
SMP|PASS
```

`-smp 2` 对应 `aps=1;online=2;percpu=1`。日志 SHA-256：`SMPAP-2-{1,2}` 均为
`0b5aaa01aab26212513e33f08e55c5c3a650c00b61db21e370be8baacd8e0cce`；`SMPAP-4-{1,2}`
均为 `d0babc1b7f10f3ce4a772f8a7245e877ffd902bc53bca1e4654f837d5fbd0aa0`。剔除
`SMP|` 行后，2 vCPU 与 4 vCPU 的启动输出逐字节相同，并通过同一组既有校验器。

新增/更新 host 用例：每 CPU TSS 基址互异且描述符编码指向本槽（`gdt`）、smp 槽位基址
互异、见证正反例覆盖 `percpu`/`tss` 缺失与错误值。

## 失败、反例与限制

- 本切片没有失败运行；AP 首次启动即通过。此前的 P5-2 失败（LAPIC 只读、补丁未对齐、
  EFER.NXE）保留在 P5-2 报告。
- **AP 仍停放**：IF=0，不接收中断；LAPIC 在 AP 上未使能，没有 IPI/定时器；AP 不参与
  调度，`hlt` 后只能被 NMI/复位唤醒。
- **运行期 CPU 身份未定**：`set_rsp0` 仍固定写 BSP 槽；AP 的 TSS.rsp0 只在入口设初值。
  内核 GS/`swapgs` 方向已在 [Windows 阻断处置](../specs/2026-11-27-windows-blocker-decisions.md)
  选择，但 GS-1…GS-3 未实现前不用于 P5。
- 共享 IDT 已装载但 AP 中断路径未被触发；门的正确性只由 BSP 既有时钟/异常证据覆盖。
- 证据来自 `sgdt`+`str` 解描述符，不是 AP 实际发生 CPL3→CPL0 换栈；TSS.rsp0 在 AP 上
  未被使用。
- xAPIC only、>4 CPU 拒绝、MMIO 窗口 WB 等 P5-2 限制不变；`smp-ap` 之外的默认镜像不
  编译 AP 路径，默认 `-smp 1` 与既有事件不变。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`。
P5 推进到“P5-3a 每 CPU 特权状态完成”，P5-3 调度整体未完成，P6 未开始。

## 后继关系

- 前序：[P5-2 AP 启动原型](2026-11-27-p5-2-ap-bringup.md)。
- 后继：P5-3b 每 CPU 可运行队列与固定放置（先不迁移）；P5-3c AP LAPIC timer 预抢占；
  P5-4 IPI 与 TLB shootdown；P5-5 2/4 vCPU 并发与扩展曲线。
