---
id: report-p5-3b4-launch-exec-lost-wakeup
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-28
---

# P5-3b-4：held-launch 放置、prepared fork/线程发布、双核 exec 与丢唤醒矩阵

## 结论与范围

本切片是 [S7-B.1](../specs/2026-11-27-stage7-smp-state-partition.md) P5-3b 的第四步：
把 **`SYS_TASK_START` 的 held-launch 创建时放置**、**prepared fork
（`SYS_FORK_TRANSACTION`）** 与 **`SYS_THREAD_CREATE`/`SYS_CLONE`** 接入每 CPU
owner+入队；在 `test-sched` 中让两个真实 Task 双核同时 `SYS_EXEC_REPLACE` 换根，并在
新映像里完成 **4 轮跨核阻塞/唤醒**（丢唤醒矩阵）。期间发现并修复 **`SYS_SEND` 直投
判定使用锁外快照** 导致的真实丢唤醒缺陷。

**不是运行期启用。** 普通镜像不编译本 feature，默认 QEMU 参数与默认镜像事件不变。
本命令仅 `-smp 2/4`，只有 CPU0/CPU1 承载负载，7-B 四项未勾选，P5 未签署。Gate 内部
创建路径（`SYS_GATE_SPAWN_THREAD`/`SYS_GATE_FORK`）与用户态服务管理器的
`start_held_task` 仍未放置；`SYS_EXEC_REPLACE` 的并行 fork/exec 程序级矩阵、IPI/TLB
仍属后续。

## 基线与输入

- 代码基线：工作区含未提交的 P5-3b-3（`HEAD 2fc6d49`），本切片同样未提交。
- 上位：[S7-B.1 §5.1/§7/§8](../specs/2026-11-27-stage7-smp-state-partition.md)、
  [P5-3b-3 报告](2026-11-28-p5-3b3-queue-placement.md)。
- QEMU：`pc-i440fx-8.2`、`qemu64`、128 MiB、`tcg,thread=multi`（仅本命令；理由同
  P5-3b-3）。

## 实现内容

| 路径 | 内容 |
|---|---|
| `kernel/src/syscall.rs` | `SYS_TASK_START` 成功后把 **target** 任务按 `placement_cpu()`（在线 CPU 队列最短者）一次置 owner+入队；`SYS_FORK_TRANSACTION` selector 1 在 `bootstrap::publish_fork` 成功后按父 CPU 发布子任务；`SYS_THREAD_CREATE`/`SYS_CLONE` 成功后随调用者 CPU 发布线程；`SYS_RECV` 先登记 out 指针（发送方锁内直投总能验证缓冲）再取消息 |
| `kernel/src/bootstrap.rs` | `send_message_with_capabilities`（及 `send_message`/全局包装）返回**锁内** `direct_delivery` 判定；新增单测 `send_reports_locked_direct_decision` |
| `kernel/src/sched_probe.rs` | 真实 `task_create_held` + `memseg_map` 建 Prepared 任务（代码/栈在 PML4 槽 26），用户 program 以真实 `SYS_TASK_START` 触发放置；双方 prepared fork（寄存器标记区分普通/预备子进程）、线程创建（线程自毁）、双核 exec 屏障与手构单 `PT_LOAD` ELF、exec 后 4 轮跨核 SEND/RECV 矩阵 |
| `xtask/src/protocol.rs` | `validate_sched` 新增 `launch_ok=1`、`prepared_ok/thread_ok/exec_ok/lost_ok=2` 与正反例 |
| `kernel/src/protocol.rs` | `SMP|sched=` 见证新增五项；`SMP|stage=sched` 诊断增加每 CPU 的 prepared/thread/exec/lost 计数与 held 信息 |

**发现并修复的缺陷**：

1. **`SYS_SEND` 直投判定与锁内状态不一致**：`send_call` 在状态锁外读取
   `was_blocked`，而 `send_message_with_capabilities` 在锁内再次判定。若接收者在两次
   读取之间进入 `BlockedRecv`，消息按直投写入 pending 并把接收者置 `Ready`，但外层
   仍按“未阻塞”跳过 `frame.rax`/`recv_out` 清理与 `wake_enqueue`：接收者停在 `Ready`
   且无人入队，CPU 空闲循环轮询不到，形成**真实丢唤醒**。修复为让发送原语返回锁内
   判定，调用方只依据该值写回并唤醒。单核实现在该路径上无并发，默认行为逐字节不变。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features sched-percpu-probe -- test-sched` | PASS：`-smp 2/4` × 2 轮；同 vCPU 数两轮逐字节相同 | QEMU BIOS，MTTCG，2/4 vCPU |
| `CARGO_NET_OFFLINE=true cargo test -p xtask --features sched-percpu-probe smp_tests` | PASS：5 | host（`validate_sched` 正反例） |
| `CARGO_NET_OFFLINE=true cargo test -p kernel --features smp-ap smp::tests` | PASS：3 | host |
| `CARGO_NET_OFFLINE=true cargo test -p kernel send_reports_locked_direct_decision` | PASS：1 | host（直投标志回归） |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1257、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal、零 unsafe（50 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：36 程序/72 轮 + stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features entry-percpu-probe -- test-entry` | PASS：`-smp 2/4` × 2 | QEMU BIOS，P5-3b-2 回归 |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features smp-ap -- test-smp-ap` | PASS：`-smp 2/4` × 2 | QEMU BIOS，P5-2/P5-3a 回归 |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features gs3-probe -- test-gs3` | PASS：`-smp 2/4` × 2 | QEMU BIOS，GS-3 回归 |

关键见证：

```text
SMP|sched=cpus=2;online=2;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;launch_ok=1;prepared_ok=2;thread_ok=2;exec_ok=2;lost_ok=2;PASS
SMP|PASS

SMP|sched=cpus=2;online=4;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;launch_ok=1;prepared_ok=2;thread_ok=2;exec_ok=2;lost_ok=2;PASS
SMP|PASS
```

日志 SHA-256：`SCHED-2-{1,2}` 均
`e37874d928d830349efca916cd8bc4a9c02ab6f0ae4649034233e8d055f89a46`；
`SCHED-4-{1,2}` 均
`9489a19b3f4b233aac20a9914ff0807d2ba7ff79e691ee11ebb22f6da0f86545`。
同 vCPU 数两轮逐字节相同（含 MTTCG 并发执行）。

逐项对应本片验收：

- **`launch_ok`**：CPU0 用户程序以真实 `SYS_TASK_START` 启动内核侧用
  `task_create_held`+`memseg_map` 建的 Prepared 任务；放置 CPU 由最短在线队列选出，
  被启动任务只能在其 owner CPU 上执行并停放（`HELD_RAN` + owner/EXEC_CPU 一致）。
- **`prepared_ok`**：双方 `SYS_FORK_TRANSACTION` selector 0/1；子任务以父 CPU 为
  owner、真实执行后停放，父任务核对 owner、执行 CPU 与阻塞态。
- **`thread_ok`**：双方 `SYS_THREAD_CREATE`；线程随调用者 CPU 发布、真实执行并
  `SYS_THREAD_EXIT` 自毁，父任务确认单线程恢复与 owner 撤销。
- **`exec_ok`**：双核屏障后双方同时 `SYS_EXEC_REPLACE`；新映像是在 ELF 段内的同一段
  代码，入口核验根已替换、旧探针代码映射消失、owner 不变。
- **`lost_ok`**：exec 后新映像执行 4 轮跨核 `SYS_SEND`/`SYS_RECV`；每轮接收方阻塞并被
  直投唤醒、载荷逐轮核对后确认，发送方等待确认再进入下一轮。

## 失败、反例与限制

1. **只证明 2 个负载 CPU**：`-smp 4` 的 AP2/AP3 在线后停放；本片不声称 4 路负载。
2. **本命令使用 MTTCG**；默认命令仍单线程 TCG，两轮逐字节相同只覆盖本命令。
3. **held-launch 由内核侧建任务、用户态触发启动**：`SYS_TASK_START` 的放置路径是真实
   syscall；但用户态服务管理器（`prima-servicemgr`）的 held-launch 流程未跑。
4. **Gate 内部创建未放置**：`SYS_GATE_SPAWN_THREAD` 与 `SYS_GATE_FORK` 建的子任务仍
   未调用放置，P5-3b 准入规则未整体闭合；这两个路径需要先确定门绑定子任务的初始状态
   语义，避免把非 Ready 任务入队。
5. **exec 并发语义有限**：两个任务各自独立的 root 同时换根；不覆盖共享根、并行
   fork/exec 程序级矩阵或 `SYS_EXEC_REPLACE_ENV`/handoff。
6. **探针 ELF 是手构最小映像**：单 `PT_LOAD`、无动态链接、无 argv/envp；只用于验证内核
   exec 并发与换根，不主张通用 ELF 装载覆盖。
7. **丢唤醒矩阵只测单向**：4 轮均为 CPU0→CPU1；反向需要 task 2 的发送凭据，当前
   bootstrap 只播种单向 IPC cap。
8. **无 IPI**：跨核唤醒仍按 owner 入队、由目标 CPU 空闲循环轮询；P5-4 前不声称确认或
   超时协议。
9. **探针资源不回收**：任务/页表帧、held 任务代码段与栈段在收尾前不回收；不跑帧守恒
   或能力/审计总量审计。
10. **失败回滚的不可达分支**：新放置调用对刚创建、owner 未设的 id 使用 `expect`；
    队列容量与 owner 冲突在该路径上不可达，容量反例由队列层先检查后写保证。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`；
默认镜像不编译本 feature。P5-3b 的 held-launch、prepared fork、线程发布与 exec 并发
探针范围完成；Gate 内部创建路径、程序级 fork/exec 矩阵、IPI/TLB 与 P5-3c 统一时间域
未开始，P5-4 未开始。

## 后继关系

- 前序：[P5-3b-3 每 CPU 队列与真实双核负载](2026-11-28-p5-3b3-queue-placement.md)。
- 上位：[S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：Gate spawn/fork 创建路径的放置与门绑定初始状态；程序级并行 fork/exec 丢唤醒
  矩阵（含反向发送凭据）；随后 P5-4 IPI/TLB shootdown 与跨核唤醒确认。
