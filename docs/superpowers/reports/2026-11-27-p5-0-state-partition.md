---
id: report-p5-0-state-partition
kind: report
authority: scoped-design-and-regression
status: implemented-with-limits
lifecycle: current
updated: 2026-11-27
---

# P5-0：SMP 状态所有权设计与 CPU 本地状态基座

## 结论与范围

本切片是 [S7-B.1](../plans/2026-11-06-stage7-execution.md) 的第一项：按 D15 在引入 AP
或任何新锁之前，先固定 SMP 的状态所有权与分区边界，并把调度器的「当前任务」从全局静态
迁到 CPU 本地入口。

交付两部分：

1. **设计**：[阶段 7 S7-B.1 SMP 状态所有权与按 CPU 分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)
   （`working-design`）：状态分类（每 CPU / 任务级共享 / 全局共享 / 只读）、CPU 身份、
   AP 启动与 IPI/TLB 分期、锁顺序与禁止项、六项待决、P5-0…P5-5 验收观察。
2. **代码基座**：新增 `kernel/src/cpu.rs::local` 作为 CPU 本地状态的唯一入口，每 CPU 一个
   `current_task` 原子槽（`MAX_CPUS = 4`，索引 0 固定 BSP）；`sched::current()/init()/
   set_current()` 改为经该入口读写。越界索引 panic，host 测试钉住槽位独立与越界拒绝。

**这不是 SMP**：没有 AP 启动、没有 `-smp 2/4`、没有 IPI/TLB shootdown、没有每 CPU TSS，
也没有给 `BOOTSTRAP_STATE` 加重入锁。7-B 四项退出条件一项都未勾选，P6 未开始。

## 基线与输入

- 代码基线：`0386ca3`（WIN-M3-4 文档提交）；本切片工作树。
- 上位：[阶段 7 执行计划](../plans/2026-11-06-stage7-execution.md) S7-B.1、
  [LibOS 数据通路计划](../plans/2026-11-10-libos-data-path.md) P5、
  [D15](../specs/2026-10-28-stage6-7-release-decisions.md#d15smp-之前的状态分区)。
- 事实基线：内核零 `unsafe`（`kernel/src/lib.rs` 的 `deny(unsafe_code)`）；
  `StaticCell` 已是 IRQ-safe CAS 自旋锁（`prima-ostd/src/sync/lock.rs`）；
  bootloader 提供 `BootInfo.rsdp_addr`；QEMU 固定 `-smp 1`。

## 实现内容

| 路径 | 内容 |
|---|---|
| `kernel/src/cpu.rs` | 新增 `pub mod local`：`MAX_CPUS=4`、`BSP=0`、每 CPU `current_task: AtomicU32`、`current_index/current_task/set_current_task/task_on/set_task_on`；BSP 初值 = `bootstrap::INIT_TASK_ID`，与迁移前 `sched::CURRENT` 一致；越界 panic；2 个 host 用例 |
| `kernel/src/sched.rs` | 删除全局 `CURRENT: AtomicU32`；`current()` 读本 CPU 槽；`init()/set_current()` 写本 CPU 槽；其余 API 与调用点不变 |
| 设计文档 | 新增上述规格；状态表覆盖 `bootstrap.rs`、`sched.rs`、`gdt.rs`、`idt.rs`、`preempt.rs`、`irq.rs`、`cow.rs`、`frames.rs`、`gate.rs` 的实际全局项 |

行为等价性：单核启动只有 BSP，`current_index()` 恒为 0，BSP 初值与 `init()` 都写
`INIT_TASK_ID`，因此所有 `sched::current()` 调用点看到的值与迁移前相同。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1236 passed、0 failed、1 ignored（基线 1234 + 本片 2 个用例） | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：完整 BIOS 套件 36 程序/72 轮、stage1 boot slice | QEMU BIOS（单核） |

新增 host 用例：`cpu::local::tests::cpu_slots_hold_independent_current_tasks`（两个 CPU 槽
互不影响、`current_task()` 取自 `current_index()`）与 `out_of_range_cpu_index_panics`
（`MAX_CPUS` 越界拒绝）。全量 72 轮 BIOS 通过表明启动与既有事件未变。

## 失败、反例与限制

- 本片没有失败运行；这是纯等价重构加设计文档，没有可失败的新运行时路径。
- **未启动 AP**：`current_index()` 尚未由 APIC ID 推导，热路径缓存方式（CPUID/GS）列为
  待决，且 GS/`swapgs` 与 WIN-D3 用户 GS 有冲突，不能提前引入。
- **未分区 BOOTSTRAP_STATE**：设计只给分类与锁顺序，没有拆分 18k 行单锁状态；P5-3 才实施。
- **未分区中断/时钟/COW 上下文**：`irq::PENDING`、`preempt::TIME_COUNTER/TICKS`、
  `cow::COW_CTX/USER_FAULT` 仍是全局单槽，当前单核正确；SMP 前必须按设计迁移。
- **未做 2/4 vCPU**：`xtask/src/qemu.rs` 仍固定 `-smp 1`；新增 SMP 启动必须是独立测试
  命令与独立见证，不改既有事件。
- 设计中的 AP 失败策略、x2APIC、任务迁移、跨 CPU 审计顺序等六项仍是待决，不是契约。
- P6 未开始；按计划 P6 依赖 P5 的机器证据，host 设计不能替代 7-D/7-G 验收。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件，不改变 personality、profile、默认 boot 事件或 QEMU
参数。阶段 7 从「P5–P6 未开始」推进到「P5-0 设计/基座已落盘，P5 未签署，P6 未开始」。

## 后继关系

- 前序：[P4-1 Gate 递送/恢复分段确认](2026-11-26-p4-segmentation.md)、
  [D15](../specs/2026-10-28-stage6-7-release-decisions.md#d15smp-之前的状态分区)。
- 设计：[S7-B.1 SMP 状态所有权与按 CPU 分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：P5-1（RSDP/MADT 与 LAPIC 探测）→ P5-2（AP 启动与在线握手）→ P5-3（调度分区与
  锁域）→ P5-4（IPI/TLB shootdown）→ P5-5（2/4 vCPU 并发与扩展曲线）；P6 在 P5 机器证据
  后推进。
