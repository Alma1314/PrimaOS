---
id: report-p5-gs2-trap-swapgs-nmi
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-27
---

# GS-2：异常/中断/IRQ 入口出口 `swapgs` 与 NMI/IST

## 结论与范围

本切片实现 [Windows 阻断处置](../specs/2026-11-27-windows-blocker-decisions.md) §3.3 的
**GS-2**，承接作者既定顺序 `P5-3a → GS-1 → GS-2`：

1. `int 0x80`、`#PF`（CPL3 路径）、IRQ0 时钟、外部 IRQ 四条入口在 **`GS_SWAP` 运行期开关**
   下按 `CS.RPL==3`（用户来源）条件 `swapgs`，出口同判据配对换回；内核来源（嵌套）不交换。
   默认镜像 `GS_SWAP=0`，整段跳过，行为不变。
2. NMI（vector 2）注册为 **IST1 门**（TSS 每 CPU IST 槽 + 独立 16 KiB NMI 栈）；NMI handler
   只记录自身 `rsp` 并 `lock inc` 计数器后 `iretq`，**不触碰 GPR/GS/调用者栈**。
3. 用 LAPIC ICR **NMI delivery + self shorthand** 自注入**真实 NMI**，验证命中内核已交换态、
   用户态、**半交换窗口**（`swapgs; 注入; swapgs` 每轮都在奇数态交付）三类场景后 GS 原样不变，
   且 handler 栈指针落在独立 IST 栈内。
4. 扩展 ring3 探针覆盖 `int 0x80`、`syscall` 指令、**用户态 #PF（COW 重试）** 与 **外部 IRQ**
   （`int 0x25` 走 DPL3 门 → `external_irq_entry`）往返后用户 GS 不变；内核半交换态下再触发一次
   **CPL0 #PF（嵌套语义）**，验证 CPL0 路径不交换、GS 不动。

**不是运行期启用。** 默认镜像、AP 调度与 Windows 运行期仍不使用 GS；NMI 栈单 CPU；每 CPU GS 区
（GS-3）与 Windows TEB/TLS（GS-4）未涉及。7-B 与任何阶段退出条件均未勾选。

## 基线与输入

- 代码基线：`4426661`（GS-1 提交）。
- 上位：[阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 设计](../specs/2026-11-27-stage7-smp-state-partition.md)、
  [GS-1 报告](2026-11-27-p5-gs1-syscall-swapgs.md)。
- QEMU：既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、128 MiB、单核，参数冻结。

## 实现内容

| 路径 | 内容 |
|---|---|
| `prima-ostd/src/entry.rs` | `syscall_entry`（int80）、`page_fault_entry` CPL3 路径、`timer_entry`、`external_irq_entry!` 宏：入口按 `[rsp+8]` 的 CS.RPL 与 `GS_SWAP` 条件 `swapgs`，出口同判据配对；`nmi_entry`（IST 门 handler，`lock inc` + 记录 `rsp`）、`nmi_count`/`nmi_rsp`/`reset_nmi_count`；`Tss::set_ist` |
| `kernel/src/gdt.rs` | `gs-probe` 下 NMI IST 栈（16 KiB）与 `nmi_stack_bounds`；`init_cpu_gdt` 置 `IST1` |
| `kernel/src/idt.rs` | `set_handler_addr_ist`；`gs-probe` 下注册 vector 2 IST1 门与 vector 0x25 的 DPL3 门 |
| `kernel/src/nmi_probe.rs` | LAPIC 可写窗口映射、`IA32_APIC_BASE`/SVR/TPR 使能、ICR self-NMI、NMI 矩阵与 `NmiWitness`；`CPL0 #PF` 嵌套检查 |
| `kernel/src/user.rs` | GS 探针机码追加用户 #PF(COW) 与 `int 0x25` 两段，`GsWitness` 增加 `post_pf`/`post_irq` |
| `kernel/src/protocol.rs`、`main.rs` | `GS|nmi=...` 见证与在 `init_idt` 后运行 NMI 矩阵；`GS|...` 行增加 `post_pf`/`post_irq` |
| `xtask` | `validate_gs` 校验 `GS|nmi` 行与新增字段；host 正反例 |

入口配对判据（统一）：`cmp [GS_SWAP],0; je skip; test byte ptr [rsp+8],3; jz skip; swapgs`。
入口时 `rsp` 指向硬件帧，`cs` 恒在 `[rsp+8]`（有/无特权切换两种情况一致）。退出在弹完 GPR 后、
`iretq` 前以同一判据换回；若 handler 内阻塞切换任务则不回到出口，改由 `sched::resume` 的
`activate_gs` 强制重设（GS-1 既有机制）。`swapgs` 只改 RFLAGS，不动 GPR/栈。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features gs-probe -- test-gs` | PASS：单核 × 2 轮，exit=33；日志逐字节相同 | QEMU BIOS，单核 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1251 passed、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（48 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：完整 BIOS 套件 36 程序/72 轮、stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |

关键见证（`GS-1.log` 与 `GS-2.log` 逐字节相同；SHA-256 均为
`59d0bcd71a702b90aabc1b595e110905f9d0503f716ec7262576204f1d77e881`）：

```text
GS|nmi=kernel=1;userlike=1;window=1;delivered=8;ist=1;nested=1;PASS
GS|read1=0x47535f54315f4d41;read2=0x47535f54325f4d42;invalid=0xffffffffffffffea;post_yield=0x47535f54315f4d41;post_syscall=0x47535f54315f4d41;echo=0xffff0000ffff1234;post_pf=0x47535f54315f4d41;post_irq=0x47535f54315f4d41;ok=1
GS|PASS
```

逐项对应 GS-2 验收：

- **异常/中断入口出口正确**：`post_pf`（用户 #PF→COW 重试后读 `gs:[0]`）、`post_irq`（用户
  `int 0x25`→`external_irq_entry` 后读）、`post_yield`（int80 让出往返后读）、`post_syscall`
  （`syscall` 指令往返后读）均为 task 1 自己的魔数，证明各入口/出口 `swapgs` 配对且未污染用户 GS。
- **嵌套 #PF/IRQ**：`nested=1`——内核半交换态下触发 `CPL0 #PF`，其 CPL0 路径不交换，处理完
  `(GS_BASE, KERNEL_GS_BASE)` 原样不变。
- **NMI 半交换窗口**：`nmi=...window=1;delivered=8`——8 轮 `swapgs; ICR 自注入; swapgs` 每次都在
  “已交换”的奇数态交付真实 NMI，末态 GS 复原；`kernel=1`/`userlike=1` 另测内核已交换态与用户态
  各注入一次；`ist=1` 证明 handler 运行在独立 IST 栈（`nmi_rsp` 落在 `[base, top)`）。

新增 host 用例：`gs_witness_requires_all_fields_and_one_pass` 覆盖 `post_pf`/`post_irq`/NMI
各字段缺失或错误。

## 失败、反例与限制

- 本切片无失败运行；首轮 NMI 即交付成功（`delivered=8`），见下文失败历史仅 GS-1 的期望值笔误。
- **IRQ0 时钟路径**：`timer_entry` 使用与外部 IRQ **逐字节相同**的入口/出口守卫，但**未单独用 GS
  注入**（默认抢占套件在 `GS_SWAP=0` 下运行 IRQ0）；本片的 IRQ 机器证据来自外部 IRQ 路径
  （`int 0x25`）。二者的 GS 交换逻辑同源，但该子路径无独立 GS 见证。
- **NMI 单 CPU**：IST 栈为单一静态；AP 未启用 NMI。嵌套 NMI 重入未测（单次注入，NMI 门内不再触发）。
- **不可返回的异常不交换**：`cpu_exception_entry!`（#DE/#UD/#GP 等）一律 `ud2` 不返回，
  处理中若经 `sched::resume` 恢复用户则由 `activate_gs` 强制设 GS；故其入口不做 `swapgs`。该路径
  的正确性不依赖入口交换，但也没有“异常入口已交换”的机器证据。
- **NMI 自注入依赖 xAPIC**：`IA32_APIC_BASE` + ICR self shorthand；x2APIC 未测。
- 探针镜像替换用户程序；`test-gs` 只校验退出码与 `GS|` 见证，不跑完整用户-phase 校验器；
  默认镜像的完整回归由 `cargo xtask test` 覆盖。
- 内核 GS 基址仍为 0（无每 CPU GS 区）；GS-3 之前热路径 CPU 索引继续用 CPUID/APIC ID。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件、默认 QEMU `-smp 1`。
GS-2 落盘，GS-3（每 CPU GS 区替换 CPUID 热路径索引）为下一片；Windows M3 运行期退出仍被
GS-3/GS-4 与 D4 阻塞。

## 后继关系

- 前序：[GS-1 `syscall` 路径 `swapgs`](2026-11-27-p5-gs1-syscall-swapgs.md)。
- 上位：[阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：GS-3 每 CPU GS 区与热路径 CPU 索引；P5-3b 每 CPU 可运行队列与固定放置。
