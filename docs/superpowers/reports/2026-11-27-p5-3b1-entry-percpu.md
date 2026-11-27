---
id: report-p5-3b1-entry-percpu
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-27
---

# P5-3b-1：CPL0 `int 0x80`/#PF 每 CPU 入口现场探针

## 结论与范围

本切片是 [S7-B.1](../specs/2026-11-27-stage7-smp-state-partition.md) P5-3b 的第一步：
把 `int 0x80` 快路径写入的保存块基址（`CONTEXT_BASE`）与 #PF 的 CR2、error code、完整
现场帧改为**每 CPU 槽**，索引取自本 CPU GS 区偏移 0（GS-3 契约）。每个 CPU 在自己的启动
入口从 CPL0 执行一次 `int 0x80` 与一次未映射读 #PF，回调只写本 CPU 的见证槽；BSP 在 AP
全部停放后补测自身并汇总。

**只覆盖 CPL0。** `syscall` 快路径、`resume` 跳板、CPL3 用户入口现场与 CPL3 #PF 仍为
全局单槽；每 CPU 专用 syscall 栈与 `USER_RSP` 也未分区。本 feature 不出现在默认镜像或
任何用户态镜像：探针镜像在见证后立即退出，AP 仍只 `hlt` 停放、不参与调度。7-B 与任何
阶段退出条件均未勾选，P5 未签署。

## 基线与输入

- 代码基线：`5ebf279`（GS-3 提交）。
- 上位：[阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 设计 §5.1/§8](../specs/2026-11-27-stage7-smp-state-partition.md)、
  [GS-3 报告](2026-11-27-p5-gs3-percpu-gs-area.md)。
- QEMU：既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、128 MiB；仅把 `-smp 1` 换成 2/4。

## 实现内容

| 路径 | 内容 |
|---|---|
| `prima-ostd/src/entry.rs` | `entry-percpu-probe`：`PerCpuEntry`（0x100 步长；`context_base`/`fault_addr`/`fault_error` 与 0xA0 字节现场帧）+ 访问器；CPL0-only 的 `syscall_entry_percpu`/`page_fault_entry_percpu`（`%gs:0` 选槽，CPL3 来源 `ud2`）；`int80_probe`；`SaveBlock::addr` |
| `prima-ostd/src/arch.rs` | `reload_code_segment`：AP 装载本 CPU GDT 后用远返回重装 CS |
| `kernel/src/entry_probe.rs` | 探针回调、每 CPU 自检（syscall/ctx/fault/frame）、见证与失败诊断 |
| `kernel/src/idt.rs` | 探针镜像把 vector 14 与 0x80 换成每 CPU 入口；默认镜像不变 |
| `kernel/src/smp.rs` | AP 装载 GDT 后重装 CS，再装载共享 IDT，随后执行本 CPU 入口探针 |
| `kernel/src/main.rs`、`protocol.rs` | 注册探针回调、BSP 补测与 `SMP|entry=` 见证 |
| `xtask` | feature `entry-percpu-probe`、命令 `test-entry`、runner/校验器与 host 正反例 |

**发现并修复的缺陷**：P5-3a 之后 AP 的 CS 仍是跳板临时 GDT 的选择子 `0x18`；装载本 CPU
GDT 后该槽已是 TSS 描述符，任何 `iretq` 都会按新表重新校验 CS 并 #GP。本切片在 AP 装载
GDT 后统一经远返回重装 CS 到内核代码段 `0x8`（对既有 `test-smp-ap` 事件无影响）。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features entry-percpu-probe -- test-entry` | PASS：`-smp 2/4` × 2 轮，exit=33 | QEMU BIOS，2/4 vCPU |
| `CARGO_NET_OFFLINE=true cargo test -p prima-ostd --features entry-percpu-probe` | PASS：47 + 2 + 5 | host |
| `CARGO_NET_OFFLINE=true cargo test -p xtask --features entry-percpu-probe smp_tests` | PASS：4 | host（校验器正反例） |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features smp-ap -- test-smp-ap` | PASS：`-smp 2/4` × 2 | QEMU BIOS，单核事件回归 |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features gs3-probe -- test-gs3` | PASS：`-smp 2/4` × 2 | QEMU BIOS，GS-3 回归 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1255 passed、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（49 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p kernel --features entry-percpu-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：36 程序/72 轮 + stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |

关键见证（`ENTRY-2-*` 与 `ENTRY-4-*`）：

```text
SMP|aps=1;online=2;percpu=1;tss=distinct
SMP|entry=cpus=2;syscall_ok=2;ctx_ok=2;fault_ok=2;frame_ok=2;PASS
SMP|PASS

SMP|aps=3;online=4;percpu=3;tss=distinct
SMP|entry=cpus=4;syscall_ok=4;ctx_ok=4;fault_ok=4;frame_ok=4;PASS
SMP|PASS
```

日志 SHA-256：`ENTRY-2-{1,2}` 均为
`e88e207bf4ad8ca801fb80d3ff2f18845e722e3e1a1568df688c328113d72db8`；
`ENTRY-4-{1,2}` 均为
`b263516481bd1346862f075f608df01cf3e5e13f006d86e2b929b7c3ab003f9e`。
同 vCPU 数两轮逐字节相同。

逐项对应本片验收：

- **`syscall_ok`**：入口回调经 `%gs` 读到本 CPU 索引，要求调用号为 `MAGIC+cpu` 且返回
  `RET_MAGIC+cpu`；编码同时出现在调用号与返回值，证明入口没有把调用交给别的 CPU 槽。
- **`ctx_ok`**：入口写入本 CPU 槽的保存块基址必须落在**本 CPU 自己的内核栈区间**（AP 为
  16 KiB AP 栈，BSP 为 bootloader 内核栈）；共享槽会让后运行的 CPU 覆盖前者，使前者读回
  零或落在别人的栈上。
- **`fault_ok`**：每个 CPU 用各不相同的 2 MiB 以上未映射地址；要求本 CPU 槽的 CR2、error
  code（0）与回调独立读取的 CR2 三者一致。
- **`frame_ok`**：本 CPU 槽的现场帧 `CS.RPL==0` 且 `RIP` 等于回调从参数帧独立记录的 rip。

## 失败、反例与限制

1. **只到 CPL0**：`syscall` 快路径（`SYSCALL_STACK`/`USER_RSP`/`RESUME_FRAME_PTR`）、
   `resume` 跳板、CPL3 用户 `int 0x80`/#PF 与用户 `USER_FRAME` 仍是全局单槽；探针入口对
   CPL3 来源直接 `ud2`，不把未分区路径伪装成已覆盖。AP 用户态执行仍被 S7-B.1 §5.1 的
   准入条件阻断。
2. **不是并发锁步**：AP 在自己的启动入口测量，BSP 在 AP 全部在线后测量；本 QEMU 配置下
   "所有 vCPU 同时自旋"的验证协议不可靠（GS-3 已登记）。本片证明的是**每槽只保留本 CPU
   的数据**：任何单槽/固定槽 0 实现都会让至少一个 CPU 读回零或被覆盖。它不证明同时交错
   执行下的无串扰。
3. **单线程 TCG、xAPIC**：未测 MTTCG/真机并发时序，未测 x2APIC。
4. **CS 重装是必要前置而非本片目标**：它改变 AP 的 CS 选择子（`0x18`→`0x8`），但
   `test-smp-ap` 的 SMP 事件逐字节不变；BSP 的 CS 仍沿用 bootloader/内核 GDT 的既有值。
5. **诊断行不属验收**：`SMP|stage=entry-*` 仅用于定位；校验器只接受完整 `SMP|entry=`
   行与单个 `SMP|PASS`。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`；
默认镜像不编译本 feature。P5-3b 仍未完成（用户态入口、快路径与可运行队列/创建时放置
待做）。

## 后继关系

- 前序：[GS-3 每 CPU GS 区](2026-11-27-p5-gs3-percpu-gs-area.md)。
- 上位：[阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：P5-3b-2 把 CPL3 `int 0x80`/#PF、`syscall` 快路径（每 CPU syscall 栈、
  `USER_RSP`、`RESUME_FRAME_PTR`）与 `USER_FRAME` 纳入同一分区，并以双核同时
  `syscall`/`int 0x80`、阻塞恢复与嵌套故障反例验证；随后 P5-3b 的可运行队列与创建时
  放置、P5-4 IPI/TLB shootdown。
