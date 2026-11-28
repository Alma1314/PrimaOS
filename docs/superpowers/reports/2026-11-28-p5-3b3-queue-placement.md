---
id: report-p5-3b3-queue-placement
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-28
---

# P5-3b-3：每 CPU 可运行队列、创建时放置、唯一运行者与真实双核负载

## 结论与范围

本切片是 [S7-B.1](../specs/2026-11-27-stage7-smp-state-partition.md) P5-3b 的第三步：
引入**每 CPU 可运行队列**、**owner（首次放置）** 与**单运行者守卫**，让 AP 进入真实
调度循环，并在两个 CPU 上同时运行两个真实 Task（task 1/2，来自真实
`BootstrapState`）。双核负载依次完成：内核屏障同时到齐、CPU1 真实 `SYS_RECV` 阻塞与
CPU0 真实 `SYS_SEND` 跨核唤醒、双方真实 `SYS_FORK`（子任务按父 CPU 入队并执行）、
用户只读页 #PF 中嵌套一次 CPL0 #PF 后换映射成功重试。

**不是运行期启用。** 普通镜像不编译本 feature，默认 QEMU 参数与默认镜像事件不变。本
命令仅 `-smp 2/4`，只有 CPU0/CPU1 承载工作负载，其余 AP 在线后停放。7-B 四项仍未
勾选，P5 未签署；`SYS_EXEC_REPLACE` 并发、用户态服务管理器的 held-launch 放置、
完整阻塞恢复与 P5-4 IPI/TLB 仍未做。

## 基线与输入

- 代码基线：`2fc6d49`（P5-3b-2 提交）。
- 上位：[S7-B.1 §5.1/§7/§8](../specs/2026-11-27-stage7-smp-state-partition.md)、
  [P5-3b-2 报告](2026-11-28-p5-3b2-user-fastpath-percpu.md)。
- QEMU：`pc-i440fx-8.2`、`qemu64`、128 MiB；本命令在既有 `-smp 2/4` 上把
  `-accel tcg,thread=single` 换为 `tcg,thread=multi`（理由见“缺陷 4”）。其余命令与
  默认镜像仍用单线程 TCG。

## 实现内容

| 路径 | 内容 |
|---|---|
| `prima-ostd/src/entry.rs` | `PerCpuEntry` 增加 `user_gs`（0xF0）；`context_base()` 在 feature 下按 `%gs:0` 读本 CPU 保存块；新增 `resume_percpu_user`/裸入口：`iretq` 前把本任务用户 GS 写入 `GS_BASE`、内核 GS 存入 `KERNEL_GS_BASE` |
| `kernel/src/cpu.rs` | `index_from_gs_base()`：只读 `IA32_GS_BASE` 定位 CPU，早期 GS 未安装时安全落 BSP |
| `kernel/src/idt.rs` | 本 feature 下 `fault_addr`/`last_fault_err`/`fault_frame` 读本 CPU 入口槽（真实 `cow::dispatch` 在 AP 上也读自己的现场） |
| `kernel/src/sched.rs` | 每 CPU 队列与 owner/running 状态（单一 IRQ-safe 锁）；`publish_placed` 一次置 owner+入队；`publish_fork` 子随父 CPU；`pick_next` 只看本 CPU 队列；`switch_to`/`block_to`/`exit_to`/`fault_to`/`block_current`/`exit_current` 按队列取出并维护单运行者；`idle_for` 只消费本 CPU 队列；本 feature 下 `resume` 用 `resume_percpu_user`、本 CPU `TSS.rsp0` |
| `kernel/src/bootstrap.rs` | 本 feature 下 `TaskTable::schedule` 从“全系统至多一个 Running”改为“同一任务不得重复 Running”；新增 `probe_ensure_process`（平台直建 Task 补 Process）与 `probe_block_recv_global`（探针停放） |
| `kernel/src/syscall.rs` | `SYS_FORK` 成功后按父 CPU 发布子任务；`SYS_SEND` 直投唤醒与 wait 唤醒按 owner 入队；非 held `SYS_TASK_CREATE` 成功后随调用者 CPU 发布，失败回滚 |
| `kernel/src/sched_probe.rs` | 两个真实 Task 的独立根/代码/栈/数据/只读页与用户机器码；包装 syscall/#PF/CPU 异常；内核对齐屏障、跨核 SEND/RECV、真实 fork、嵌套 CPL0 故障；见证与看门狗 |
| `kernel/src/smp.rs` | AP 上线后等待 BSP 发布，再进入本 CPU 调度循环；`-smp 4` 的多余 AP 停放；在线位图访问器。**修复 `ICR_INIT` 广播简写缺陷**（见下） |
| `kernel/src/main.rs` | 本 feature 精简默认 init 页面/段/boot 见证，保留真实 `BootstrapState` 与 IPC 能力；收尾回调输出 sched 见证 |
| `xtask/` | 新 feature `sched-percpu-probe`、命令 `test-sched`、MTTCG 参数与 `validate_sched`（正反例） |

**发现并修复的缺陷**：

1. **`ICR_INIT` 广播**：`0x000c4500` 的 destination shorthand（bits 19:18）是 `0b11`
   （all-excluding-self），启动第 2 个 AP 的 INIT 会重置此前所有在线 AP。P5-2/P5-3a/
   GS-1/GS-2/GS-3/P5-3b-1/2 的 AP 在入口完成测量后即停放，缺陷未暴露；本片要求 AP1
   在 AP2/3 启动后继续运行才发现。改为定向值 `0x00004500`（其余 SMP 回归命令重跑通过）。
2. **全局单 Running 假设**：`TaskTable::schedule` 拒绝任何已有 Running 任务，AP 无法
   调度 task 2。本 feature 下改为按任务判定；每 CPU 的同时运行由队列 owner 与
   `running` 守卫约束（每个 Task 仍只在一个 CPU 上执行）。默认镜像保持原语义。
3. **平台 Task 无 Process**：`platform_init` 只建 Task 记录，`SYS_FORK` 因
   `process_of == None` 返回 `EINVAL`。探针发布前用 `probe_ensure_process` 为 task 1/2
   建独立 Process（单线程 leader）。
4. **单线程 TCG 饿死自旋 AP**：`-smp ≥ 3` 时单线程 TCG 可能不再调度已自旋的 AP1（其余
   AP 又占执行槽）。本命令改用 MTTCG 观察真实并行；`-smp 4` 的多余 AP 在线后停放，
   只让 CPU0/CPU1 承载负载。默认命令与镜像不受影响。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features sched-percpu-probe -- test-sched` | PASS：`-smp 2/4` × 2 轮；同 vCPU 数两轮逐字节相同 | QEMU BIOS，MTTCG，2/4 vCPU |
| `CARGO_NET_OFFLINE=true cargo test -p xtask --features sched-percpu-probe smp_tests` | PASS：5 | host（`validate_sched` 正反例） |
| `CARGO_NET_OFFLINE=true cargo test -p kernel --features smp-ap smp::tests` | PASS：3 | host（含 INIT/SIPI 定向回归） |
| `CARGO_NET_OFFLINE=true cargo test -p prima-ostd --features entry-percpu-probe` | PASS：47 + 2 + 5 + doctests | host |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features entry-percpu-probe -- test-entry` | PASS：`-smp 2/4` × 2 | QEMU BIOS，P5-3b-2 回归 |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features smp-ap -- test-smp-ap` | PASS：`-smp 2/4` × 2 | QEMU BIOS，P5-2/P5-3a 回归 |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features gs3-probe -- test-gs3` | PASS：`-smp 2/4` × 2 | QEMU BIOS，GS-3 回归 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1256 passed、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（50 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p kernel --features sched-percpu-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：36 程序/72 轮 + stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |

关键见证：

```text
SMP|sched=cpus=2;online=2;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;PASS
SMP|PASS

SMP|sched=cpus=2;online=4;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;PASS
SMP|PASS
```

日志 SHA-256：`SCHED-2-{1,2}` 均
`22b5ff6a0f63d57380410ae67624e0af2abcc941bf67eae9c02e2c637dcc27bc`；
`SCHED-4-{1,2}` 均
`0998c8ea73a88132fab218569eb1696330a87ebd4d107990990a38b6404c7400`。
同 vCPU 数两轮逐字节相同（含 MTTCG 并发执行）。

逐项对应本片验收：

- **`place_ok`**：task 1/2 分别以 CPU0/CPU1 为 owner 发布；`EXEC_CPU[task]` 只能观察
  到 owner CPU。`-smp 4` 只让 CPU0/CPU1 承载负载，`online=4` 证明四个 CPU 均已上线。
- **`single_ok`**：队列 `take` 以 `(owner, !running)` 原子取得运行权；跨 CPU 观察计数与
  调度违例都必须为 0。
- **`queues_ok`**：每个活动 CPU 的入队与出队次数均非零（发布/唤醒入队，空闲循环出队）。
- **`barrier_ok`**：两个真实任务在内核中自旋至 `ARRIVED == 2`；任一方未被真实并行调度
  都会超时失败（MTTCG 下为真实同时驻留）。
- **`wake_ok`**：CPU1 的 `SYS_RECV` 进入真实 `BlockedRecv`；CPU0 观察到阻塞后
  `SYS_SEND` 返回 0，载荷按 owner 入队并被 CPU1 的 `SYS_RECV` 取回；两侧回执匹配。
- **`fork_ok`/`child_ok`**：双方 `SYS_FORK` 成功；子任务 owner 等于父 CPU，经父任务
  `SYS_YIELD` 在父 CPU 上执行、报告后停放；父任务核对子 id、owner、阻塞态与执行 CPU。
- **`nested_ok`**：用户写只读页（`P|W|US`）进入每 CPU 的 CPL3 现场；处理程序记录外层
  用户帧后，在同一 CPU 触发一次未映射内核读（CPL0 #PF），内层经 `cow::dispatch` 跳过
  故障指令返回；外层确认 `USER_FRAME` 未被内层覆盖，再复制私有帧、改为可写并重试成功。

## 失败、反例与限制

1. **只证明 2 个负载 CPU**：`-smp 4` 的 AP2/AP3 在线后停放；本片不声称 4 路负载或
   4 路扩展曲线。7-B 的 2/4 vCPU 启动只覆盖“在线”，不覆盖 4 路调度。
2. **本命令使用 MTTCG**：单线程 TCG 下自旋 vCPU 会被饿死，无法观察真实同时执行。
   MTTCG 下两轮输出仍逐字节相同，但该命令与其余单线程 TCG 命令的时序模型不同；默认
   镜像与既有命令未改变。
3. **held-launch 与其它创建路径未接线**：`publish_placed` 已用于内核初始独立 Task、
   非 prepared `SYS_FORK` 子任务与非 held `SYS_TASK_CREATE`；用户态服务管理器的
   `start_held_task` 发布、prepared fork（`SYS_FORK_TRANSACTION`）与
   `SYS_THREAD_CREATE` 仍未调用它，这些路径的运行期放置与失败回滚未验证。P5-3b 准入
   规则因此未整体闭合。
4. **`SYS_EXEC_REPLACE` 并发未做**：本片只证明真实 `SYS_FORK` 与初始发布；exec 替换
   地址空间的双核并发、Linux 程序级并行 fork/exec 与丢唤醒矩阵仍待 P5-3b-4。
5. **嵌套故障限制**：每 CPU 的 CPL0 故障槽是单帧（后写覆盖先写）；本探针的外层处理
   程序在触发内层前保存外层用户帧与故障帧，所验证的是“内层 #PF 不破坏外层用户现场且
   外层成功重试”。未实现通用的嵌套故障帧栈。
6. **无 IPI**：跨核唤醒只入队，目标 CPU 的空闲循环轮询自有队列（`hlt` 不用于等待）；
   丢失唤醒窗口由“先改状态/入队、后休眠”的顺序约束，但未做 IPI 确认或超时协议。
7. **探针专用资源**：用户页/页表帧取自保留页表帧分配器，探针退出前不回收；子任务内核
   栈复用既有 `KSTACK_TASKS`；不跑帧守恒、能力/审计总量审计。
8. **模型语义扩展是 feature 局部**：`TaskTable::schedule` 的“同一任务不得重复 Running”
   只在本镜像启用；抽象模型仍无 CPU 字段，P5-3b 的完整唯一运行者语义待后续冻结。
9. **诊断行不属验收**：`SMP|stage=sched` 只在失败时输出阶段/自旋/异常信息；校验器只
   接受完整 `SMP|sched=` 行与单个 `SMP|PASS`。
10. **boot 事件集缩减**：探针镜像跳过默认 init 页面/平台能力移交/SPEC-070…073 见证，
    以保留 fork 所需的审计、能力与帧预算；本命令不主张这些默认见证，默认镜像仍完整。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`；
默认镜像不编译本 feature。P5-3b 仍未完成（held-launch 放置、exec 并发、完整阻塞恢复/
丢唤醒矩阵、IPI 与 TLB shootdown 待做），P5-4 未开始。

## 后继关系

- 前序：[P5-3b-2 CPL3 入口与快路径每 CPU 分区](2026-11-28-p5-3b2-user-fastpath-percpu.md)。
- 上位：[S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：P5-3b-4 接线 held-launch 放置、补 `SYS_EXEC_REPLACE` 并发与并行 fork/exec
  丢唤醒矩阵；随后 P5-4 IPI/TLB shootdown 与跨核唤醒确认。
