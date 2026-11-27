---
id: spec-stage7-smp-state-partition
kind: specification
authority: stage7-smp-partition-design
status: working-design
lifecycle: current
updated: 2026-11-27
---

# 阶段 7 S7-B.1：SMP 状态所有权与按 CPU 分区设计

## 1. 身份与效力

作者在 2026-11-27 要求转回阶段 7 P5/P6。本文件是
[S7-B.1](../plans/2026-11-06-stage7-execution.md) 的第一项可审阅交付：**per-CPU 状态/
所有权设计**，并附第一步已落地的代码边界。D15（SMP 前先做状态按 CPU 分区，禁止在单一
全局状态上直接加锁）继续有效。

本文是工作设计，不是 7-B 的冻结契约，也不勾选任何退出条件。它不改变
[阶段跟踪](../../../Prima%20OS%20实现阶段.md)的阶段状态、profile、QEMU 参数或既有
证据；AP 启动、IPI、TLB shootdown 与 2/4 vCPU 验收仍待后续切片。

## 2. 事实基线（2026-11-27，`0386ca3` 之后的工作树）

| 事实 | 位置 | 影响 |
|---|---|---|
| 单一 `BOOTSTRAP_STATE` 承载任务表、能力、命名空间、段、定时器、设备、wait 与 vspace 条目 | `kernel/src/bootstrap.rs` | 全局单锁是 SMP 的主要串行点，不能靠“多插一把锁”解决 |
| 调度器按 Task id 的全局数组：帧、根、内核栈顶、FS 基址、FPU、阻塞接收、待写 status | `kernel/src/sched.rs` | 任务级共享状态；以 Task id 索引，任何 CPU 都可能访问目标任务 |
| 当前任务 id 原为全局 `AtomicU32` | 原 `kernel/src/sched.rs` | **已迁移**为 CPU 本地槽（§4） |
| 单 GDT/TSS，`TSS.rsp0` 每次 `resume` 改写 | `kernel/src/gdt.rs`、`sched::resume` | 每 CPU 必须有独立 TSS（或独立 rsp0 槽）；P5-2 |
| 单 IDT | `kernel/src/idt.rs` | 只读表可由各 CPU 共享，AP 装载同一表 |
| 时钟/预抢占状态为全局：tick 计数、校准、per-task 抢占计数 | `kernel/src/preempt.rs` | `TIME_COUNTER`/`TICKS` 应每 CPU；per-task 计数属任务级 |
| IRQ 待决位为全局 `AtomicU16` | `kernel/src/irq.rs` | 中断属于 CPU 本地；P5-3 分区 |
| COW 故障上下文与用户故障记录为全局单槽 | `kernel/src/cow.rs` | 陷入上下文必须每 CPU；P5-3 分区 |
| 入口汇编使用单个 `SYSCALL_STACK`、`USER_RSP`、`CONTEXT_BASE`、`RESUME_FRAME_PTR`，#PF 记录也为单槽 | `prima-ostd/src/entry.rs` | Rust 分派前即可发生覆盖；必须在 AP 执行用户任务前完成入口状态分区 |
| 页帧注册表、Gate arena、审计 arena/cursor 为全局共享 | `frames.rs`、`gate.rs`、`bootstrap.rs` | 保持共享，按锁顺序访问 |
| `StaticCell` 是 IRQ-safe CAS 自旋锁 | `prima-ostd/src/sync/lock.rs` | 已能提供互斥；问题在粒度、锁顺序与阻塞，不是缺少原子性 |
| 内核零 `unsafe` | `kernel/src/lib.rs` | AP 实模式跳板、LAPIC、GS 访问只能进 `prima-ostd` |
| bootloader 提供 RSDP | `bootloader_api` `BootInfo.rsdp_addr` | AP 枚举可走 ACPI MADT，不需另找 BIOS 表 |
| QEMU 固定 `-smp 1` | `xtask/src/qemu.rs` | 2/4 vCPU 属新启动配置，需独立测试命令与见证，不改默认事件 |

## 3. 状态所有权分类

原则：**能在 CPU 本地闭合的状态，绝不共享；必须共享的状态，用单一所有者与明确锁顺序，
不复制副本充当隔离。**

| 类别 | 状态 | 归属 | 计划 |
|---|---|---|---|
| A. CPU 固有 | 当前任务 id | 每 CPU | **本片已迁移**（§4） |
| A. CPU 固有 | `TSS.rsp0`、内核栈切换目标 | 每 CPU | P5-2：每 CPU TSS；`resume` 只写本 CPU |
| A. CPU 固有 | GDT/TSS 描述符 | 每 CPU（TSS 必须；GDT 每 CPU 更简单） | P5-2 |
| A. CPU 固有 | LAPIC 基址、APIC ID、在线位 | 每 CPU | P5-1/P5-2 |
| A. CPU 固有 | IF/嵌套深度、本地 IRQ 屏蔽 | 每 CPU（硬件 + 现有 guard） | 保持 |
| A. CPU 固有 | 本地抢占 tick 与硬件计时器状态 | 每 CPU | P5-3；期限统一时间域见 §5.2 |
| A. CPU 固有 | 硬件 IRQ 待决位 | 每 CPU | P5-3 |
| A. CPU 固有 | 故障陷入上下文（COW/用户故障） | 每 CPU | P5-3 |
| A. CPU 固有 | syscall 入口栈、用户 RSP 暂存、当前保存块与恢复帧指针 | 每 CPU/嵌套入口帧 | P5-3b 前置；禁止汇编继续引用单个全局槽，不能靠进入 Rust 后加锁补救 |
| A. CPU 固有 | AP 上线握手状态 | 每 CPU + 一个全局位图（原子） | P5-2 |
| B. 任务级共享 | `FRAMES`、`ROOTS`、`ROOT_GENERATIONS`、`KSTACK_TOPS`、`FS_BASES`、`FPU_AREAS`、`KERNEL_RESUME_FRAMES`、`RECV_OUTS`、`PENDING_STATUS` | 共享（按 Task id） | 不复制到每 CPU；P5-3 固定锁顺序与跨表不变量 |
| B. 任务级共享 | 阻塞调用、信号状态、wait 关联 | 共享（按 Task id） | 同上 |
| B. 任务级共享 | 任务的内核栈内容 | 任务所有；任一时刻只在一个 CPU 上运行 | 迁移时随 CR3/`rsp0` 切换 |
| C. 全局共享 | `BOOTSTRAP_STATE`（任务表、能力、命名空间、段、定时器、设备、wait、vspace 条目、审计计数） | 全局单锁 | P5-3：按域拆分与所有者；**不是**给单锁加嵌套锁 |
| C. 全局共享 | 页帧注册表、VSpace arena、fork_tx | 全局 | 保持锁；P5-4 决定跨 CPU 失效点 |
| C. 全局共享 | Gate arena/表、完成票据 | 全局 | P5-3 锁顺序；P5-4 跨 CPU 唤醒 |
| C. 全局共享 | 审计 arena/cursor | 全局 | 保持；跨 CPU 顺序契约列为待决 |
| C. 全局共享 | 串口与协议序号 | 全局 | 默认启动仍单 CPU；AP 输出统一由 BSP 汇总（P5-2 再评估） |
| D. 只读 | IDT、引导期内存清单、内核页表上层 | 共享只读 | AP 直接使用 |
| E. 分析性 | bench/probe 静态 | 全局 | 不进产品路径，不因 SMP 改语义 |

## 4. 本片已落地的分区边界

`kernel/src/cpu.rs::local` 是 CPU 本地状态的唯一入口：

- `MAX_CPUS = 4`（S7-SCOPE-13 的 2/4 vCPU 上限），索引 0 固定为 BSP；
- 每 CPU 一个 `current_task: AtomicU32`，BSP 初值为 bootstrap init Task；
- `current_index()` 在 AP 上线前恒返回 BSP；
- `sched::current()/init()/set_current()` 改为经该入口读写，调用点语义不变；
- 越界 CPU 索引 panic，不静默读写错误槽位；
- host 测试钉住“两个 CPU 槽互不影响”和越界拒绝。

这一步不引入任何锁、不改变 QEMU 参数、不改变启动事件。`cpu::local` 是内核调度侧的
CPU 身份入口；汇编直接访问的状态仍归 `prima-ostd`，后续须通过同一 CPU 索引映照分区，
并为嵌套陷入保留独立现场。当前 `current_index()` 仍返回 BSP，AP parked 不证明运行期分区安全。

## 5. CPU 身份与后续机制

- **身份**：CPUID leaf 1 `EBX[31:24]` 初始 APIC ID；MADT 枚举 AP。索引 0 = BSP，
  AP 按 MADT 次序取 1..；CPU 索引表在启动期一次写定，之后只读。热路径缓存方式（GS 或
  其它）见待决项 2。
- **AP 启动**（P5-2）：`BootInfo.rsdp_addr` → XSDT/RSDT → MADT；MMIO LAPIC
  `0xFEE0_0000` 经物理直映射；低于 1 MiB 的实模式跳板承载 INIT-SIPI-SIPI；AP 进长模式后
  装载共享 IDT、本 CPU GDT/TSS 与内核栈，以原子在线位握手。全部 `unsafe` 收敛在
  `prima-ostd`，`kernel/src` 保持零 unsafe。
- **调度边界**（P5-3）：AP 上线初期只进 idle，不取任务；任务分区/迁移与可运行队列在
  独立切片引入，避免“AP 一上线就并发改任务表”的不可审查状态。
- **TLB 与 IPI**（P5-4）：本地 `invlpg` 保持；共享内核映射或跨任务撤销导致的失效，需要
  按 generation/ticket 发送 IPI 并在本地 handler 失效。发送方不得持地址空间锁等待确认，
  否则会与目标 CPU 的同一把锁互等。

### 5.1 AP 用户态调度准入与初次放置

P5-3b 开启 AP 用户态执行前，必须完成 CPU 身份、入口栈/现场分区和任务唯一运行者协议，
并以双核同时 `syscall`/`int 0x80`、阻塞恢复和嵌套故障反例验证；AP 仅上线、TSS 基址互异不替代这些条件。
保留作者已决的 fork/exec 随父 CPU、唤醒回目标 CPU、不迁移/不 work stealing。
尚缺的**首批任务创建时放置**须在 P5-3b 实现前冻结：启动管理器为尚未运行的独立任务指定
在线 CPU，发布时一次设置 owner 与入队，失败不留下半发布任务；不得靠迁移 Running Task 补初始负载。
这是待评审的补充规则，不冒称作者已裁决。验收必须观察不同 CPU 同时执行有效负载。

TLB shootdown 的最小协议是允许跨 CPU 修改活动页表、共享 root 并行运行或回收其页帧前的
前置。P5-3 可先在受限配置验证独立 root 固定所有者：每个 root 仅在一个 CPU 执行，映射
修改交由该所有者，并禁止远端活动 root 修改、跨 CPU 共享 root 及共享内核映射变化。
若现有 Gate/COW/销毁路径无法满足限制，必须把 P5-4 的必要部分提前；不得先开放再等后续修补。
P5-4 须冻结 active-CPU 集合、generation/ticket、确认与超时失败策略；刷新未确认前禁止复用相关数据帧/页表帧。

### 5.2 时间域与期限

本地抢占计数不作为进程可见绝对时间。P5-3c 前须冻结一个全局单调时间域：各 CPU 的
硬件采样通过校准与偏移转换到同一 epoch/单位后，才可写入或比较全局等待表中的 deadline。
在该转换和单调性证据就绪前，只由 BSP 处理物理期限，AP 不得用本地 tick 扫描全局等待者。
这不把物理期限时钟与 COSIR 抽象 `now` 合并。验收注入各核启动偏移、采样回退和频率差，
证明不会提前超时，并覆盖 idle 唤醒与跨 CPU 等待；误差界与延迟界另行冻结。

## 6. 锁顺序与禁止项

草案顺序（高 → 低）：CPU 本地（无锁原子）→ 调度/任务域 → 地址空间/页帧域 → 全局
资源/审计域。要求：

1. 同一临界区不得逆序获取；新代码必须能指出它在顺序中的位置。
2. 持锁期间不得阻塞、切任务、等待另一个 CPU 的确认或执行无界工作。
3. IRQ handler 只做最小工作并记录待决，不获取可能被中断上下文持有的高序锁。
4. 跨表不变量（任务状态 + 根 + 帧）必须在一个所有者临界区内完成，不靠事后“再检查”。
5. `StaticCell` 递归加锁仍禁；SMP 下该限制从“会死锁”升级为“跨 CPU 死锁”。

## 7. 决策与待决项

| # | 问题 | 推荐/决定 | 代价/说明 |
|---|---|---|---|
| 1 | AP 启动失败策略 | **已决（作者 2026-11-27）：默认确定性 fail-stop，不静默降级；仅显式 profile 可退化为 BSP-only 并写审计事件** | 当前 `test-smp-ap` 仍是“继续启动 + 校验器拒绝”；生产启动的 fail-stop 接线属 P5-3/7-B |
| 2 | 内核热路径 CPU 索引缓存：CPUID vs GS | 先 CPUID（冷路径）跑通；GS/`swapgs` 方向已由 [Windows 阻断处置](2026-11-27-windows-blocker-decisions.md)选择内核 GS，但 GS-1…GS-3 未实现前 P5 不依赖它 | P5-3 不等待 GS；GS-3 后才评估热路径迁移 |
| 3 | xAPIC vs x2APIC | 先 xAPIC（QEMU `qemu64` 默认）；x2APIC 记录但不作为验收前提 | 需 CPUID 探测 |
| 4 | 任务迁移策略 | **已决（作者 2026-11-27）：每 CPU 就绪队列 + 固定放置（fork/exec 在父 CPU，唤醒按目标 CPU）；首片运行中任务不迁移、不做 work stealing；迁移/steal 留到扩展曲线切片** | 每 CPU 队列与放置规则增加唤醒路由和丢唤醒反例的验证量 |
| 5 | 跨 CPU 审计顺序 | 保留全局序号，允许并发 append 时按获取顺序线性化 | 需明确“顺序”是锁顺序还是提交顺序 |
| 6 | per-CPU 时基与 sleep deadline | 本地抢占与全局单调期限分开；统一 epoch/单位前由 BSP 处理期限 | 校准/偏移转换与误差界待冻结；不得直接跨核比较本地绝对 tick（§5.2） |

2026-11-27 执行顺序（作者）：先落盘 P5-3 的每 CPU GDT/TSS/IDT；随后 GS-1（仅
`syscall` 路径的 `swapgs`）；再 GS-2（异常/IRQ）。Windows 运行期 TEB/TLS 不早于
GS-4；P5-3 不等待 GS。

## 8. 分期工作包与验收观察

| 包 | 范围 | 验收观察 | 状态 |
|---|---|---|---|
| P5-0 | 本文 + CPU 本地入口 + 调度当前任务分区 | host 测试与既有 BIOS 启动逐字节回归；不声称 SMP | **本片** |
| P5-1 | MADT/RSDP 解析、LAPIC MMIO、CPU 索引表（宿主模型 + 只读探测） | 宿主夹具用例；BSP 上读出自身 APIC ID；不改默认启动 | **完成**：解析 + `-smp 2/4` 只读探测（AP 未启动，[报告](../reports/2026-11-27-p5-1-acpi-madt-host.md)）；运行期索引与 AP 属 P5-2 |
| P5-2 | AP 实模式跳板、每 CPU GDT/TSS/栈、在线握手、AP idle | 专用 QEMU `-smp 2/4` 见证 `online=2/4`；既有事件不改 | **原型完成**：跳板 + INIT-SIPI-SIPI + 在线位，`-smp 2/4` 通过（AP parked，[报告](../reports/2026-11-27-p5-2-ap-bringup.md)）；每 CPU GDT/TSS/IDT 与调度属 P5-3 |
| P5-3 | 调度分区、可运行队列、锁域拆分、唤醒与阻塞反例 | 受 §5.1 准入限制；并行 fork/exec 无丢唤醒、入口现场无串扰；任务状态与根一致 | 进行中：**P5-3a 完成**（每 CPU GDT/TSS/共享 IDT，AP 仍 IF=0，[报告](../reports/2026-11-27-p5-3a-percpu-gdt-tss.md)）；P5-3b 先补入口状态、创建时放置与固定所有者；P5-3c 先冻结统一期限时间域再接 AP 预抢占 |
| P5-4 | IPI 与 TLB shootdown，跨 CPU 撤销失效 | 旧映射失效、确认前帧不复用、超时不静默成功 | 待做；允许跨 CPU 活动映射变更前必须交付最小协议，必要时提前到 P5-3 |
| P5-5 | 2/4 vCPU 并发 I/O 与扩展曲线 | 1/2/4 vCPU 曲线与门槛登记 | 待做 |

7-B 退出仍要求全部四项：2/4 vCPU 启动、并行 fork/exec 无丢唤醒、TLB shootdown 正确、
扩展曲线。本设计不替代任何一项。

## 9. 风险与停止条件

- **AP 启动若要求重写 bootloader 或引入不可审查的跳板**，暂停并回到架构假设评审，不
  “边写边扩”成一个无界项目。
- **若 7-A 的按域状态划分已覆盖所需分区**（D15 重裁触发），改写依赖并复用，不重复
  重构。
- **若共享状态无法在不阻塞的前提下取得一致**（例如跨 CPU 唤醒与撤销互等），P5-4 停止并
  回到本文件的锁顺序评审。
- 任何切片不得以“运行通过”替代上述性质；失败与未覆盖项随报告保留。

## 10. 后继关系

- 上位：[阶段 7 执行计划](../plans/2026-11-06-stage7-execution.md) S7-B.1；
  [LibOS 数据通路计划](../plans/2026-11-10-libos-data-path.md) P5；
  [D15](../specs/2026-10-28-stage6-7-release-decisions.md#d15smp-之前的状态分区)。
- 本片证据：[P5-0 报告](../reports/2026-11-27-p5-0-state-partition.md)。
- P6（撤销/恢复与总验收）依赖 P5 的机器证据；其契约设计可在 P5 并行准备，但不得把
  host 模型当作 7-D/7-G 验收。
- 2026-11-27 [架构修订与异构 ISA 复核](../reports/2026-11-27-architecture-isa-re-review.md)
  补齐入口状态、准入、初次放置与统一期限要求；本次只修订设计，未实现上述机制。
