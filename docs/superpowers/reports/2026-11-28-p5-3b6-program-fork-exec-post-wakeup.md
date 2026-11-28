---
id: report-p5-3b6-program-fork-exec-post-wakeup
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-28
---

# P5-3b-6：程序级 fork/exec 与 fork/exec 后双向丢唤醒矩阵

## 结论与范围

本切片是 [S7-B.1](../specs/2026-11-27-stage7-smp-state-partition.md) P5-3b 的第六步：

1. 普通 `SYS_FORK` 子进程不再只报告停放；它在 fork 点直接 `SYS_EXEC_REPLACE`
   数据页中的第二映像（独立根槽），两个 CPU 的 fork 子进程与父进程后续阶段/exec
   并发执行。
2. 父进程把子 id 保存在 `r15`（exec 只重置 `rax`/`rdi`），exec 后核对：子进程已运行
   第二映像、旧父映像消失、新映像存在、随父 CPU 停放。
3. exec 后追加 **fork/exec 后双向唤醒**（CPU0→CPU1、CPU1→CPU0 各一轮），验证
   fork/exec 交叠之后跨核阻塞/唤醒仍不丢失。

**不是运行期启用。** 普通镜像不编译 `sched-percpu-probe`，默认 QEMU 参数与默认镜像
事件不变。第二映像仍是探针内手构单段 ELF；“程序级”指真实 syscall 路径上的用户程序
流程，不是 `prima-user`/musl 二进制。Gate 端到端双核、服务管理器 held-launch、
IPI/TLB 与 P5-4 仍未开始。

## 基线与输入

- 代码基线：`HEAD fe9e5f4`（P5-3b-5），本切片未提交。
- 上位：[S7-B.1 §5.1/§7/§8](../specs/2026-11-27-stage7-smp-state-partition.md)。
- QEMU：`pc-i440fx-8.2`、`qemu64`、128 MiB、`tcg,thread=multi`（仅本命令；理由同
  P5-3b-3）。

## 实现内容

| 路径 | 内容 |
|---|---|
| `kernel/src/sched_probe.rs` | pre-exec fork 子进程（`r13=0`）在报告 `CHILD_RUN` 后读取数据页 `0xC00` 的第二 ELF 并 `SYS_EXEC_REPLACE`；父进程 fork 后 `mov r15, rax` 保存子 id；exec 后 `MAGIC_FORK_EXEC_DONE` 核对子进程状态；随后双向一轮 `SEND`/`RECV`（`post_ok`） |
| `kernel/src/sched_probe.rs` | 第二映像 `child_exec_elf()`：单 `PT_LOAD`、RX、入口 `EXEC_CHILD_VA+120`；`EXEC_CHILD_VA` 取任务根已清零的 `U_SLOT`（24），避开 bootloader 低半区可能存在的 supervisor PML4 项 |
| `kernel/src/sched_probe.rs` | 数据页布局：父 ELF 从 `0x800` 起（≤1 KiB），子 ELF 固定 `0xC00`；`build_task` 写入并断言两者不重叠；`exec_elf()` 不再内嵌子映像 |
| `kernel/src/protocol.rs` | `SMP|sched=` 追加 `forkexec_ok`、`post_ok`；诊断追加每 CPU `fxN/psN/fecN` 与 `FECODE` |
| `xtask/src/protocol.rs` | `validate_sched` 扩到 16 个字段：`forkexec_ok=2`、`post_ok=2` 与正反例 |

**发现并规避的失败**：最初尝试在 exec 后的新映像里再次 `SYS_FORK`。父/子返回用户态后
在同一指令处取指缺页（`err=0x4`）；同一探针在旧映像 fork 可正常执行，故本片改用
pre-exec fork 子进程执行第二映像，父进程 exec 后核对。该限制保留在下方失败清单。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features sched-percpu-probe -- test-sched` | PASS：`-smp 2/4` × 2 轮；同 vCPU 数两轮逐字节相同 | QEMU BIOS，MTTCG，2/4 vCPU |
| `cargo test -p xtask --features sched-percpu-probe smp_tests` | PASS：5 | host（`validate_sched` 正反例） |
| `cargo test -p kernel --features sched-percpu-probe sched_probe::tests` | PASS：5 | host（含子 ELF 可装载与数据页布局） |
| `cargo test -p kernel --features smp-ap smp::tests` | PASS：3 | host |
| `cargo test --workspace` | PASS：1257、0 failed | host |
| `cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal、零 unsafe（50 文件）、S6-X3 | host/静态 |
| `cargo clippy -p kernel --features sched-percpu-probe --target x86_64-unknown-none -- -D warnings` | PASS | 静态 |
| `cargo xtask test` | PASS：36 程序/72 轮 + stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |
| `cargo run -q -p xtask --features entry-percpu-probe -- test-entry` | PASS：`-smp 2/4` × 2 | QEMU BIOS，P5-3b-2 回归 |
| `cargo run -q -p xtask --features smp-ap -- test-smp-ap` | PASS：`-smp 2/4` × 2 | QEMU BIOS，P5-2/P5-3a 回归 |
| `cargo run -q -p xtask --features gs3-probe -- test-gs3` | PASS：`-smp 2/4` × 2 | QEMU BIOS，GS-3 回归 |

关键见证：

```text
SMP|sched=cpus=2;online=2;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;launch_ok=1;prepared_ok=2;thread_ok=2;exec_ok=2;lost_ok=2;reverse_ok=2;forkexec_ok=2;post_ok=2;PASS
SMP|PASS

SMP|sched=cpus=2;online=4;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;launch_ok=1;prepared_ok=2;thread_ok=2;exec_ok=2;lost_ok=2;reverse_ok=2;forkexec_ok=2;post_ok=2;PASS
SMP|PASS
```

日志 SHA-256：`SCHED-2-{1,2}` 均
`6ad02d6cc5dbd1e85bdb13e0c631ade95b09025042790d850cc39f914b60bb47`；
`SCHED-4-{1,2}` 均
`674aef0f0401db9997dc04016106ef4bffb346e2816c5864645ce57063944c6f`。
同 vCPU 数两轮逐字节相同（含 MTTCG 并发执行）。

逐项对应本片验收：

- **`forkexec_ok`**：双方 pre-exec fork 子进程各 exec 数据页中的第二映像；父进程完成
  自身 `SYS_EXEC_REPLACE` 后，以 `r15` 中的子 id 核对子进程已运行第二映像、旧父映像
  映射消失、新映像存在、owner 与父一致且已停放。
- **`post_ok`**：exec 后 CPU0 先发送、再阻塞接收反向；CPU1 先阻塞接收、再发送反向；
  两轮载荷逐轮核对并确认，证明 fork/exec 交叠之后唤醒仍按 owner 路由。

## 失败、反例与限制

1. **exec 后的二次 fork 未采用**：在 exec 后新映像中直接 `SYS_FORK`，父/子返回用户态
   后在同一指令处取指缺页（`err=0x4`，用户取指 not-present），页表检查却显示叶存在；
   未定位根因，本片改为 pre-exec fork 子进程 exec 第二映像。该路径不得外推为
   “exec 后可安全 fork”的证据。
2. **“程序级”仍是探针用户程序**：第二映像为手构单 `PT_LOAD` ELF，无动态链接/argv/envp；
   不是 `prima-user` 或 musl 程序，也不覆盖真实文件系统 exec。
3. **非严格同时 exec 屏障**：子进程 exec 与父进程后续 held-launch/prepared fork/线程/
   exec 并发，但没有“双方子进程同一时刻 exec”的屏障；`forkexec_ok` 证明的是并发窗口内
   的顺序正确性。
4. **`r15` 传递子 id**：依赖 exec 只重置 `rax`/`rdi` 的现有实现；若未来 exec 寄存器
   语义变化，需改用显式交接。
5. **第二映像槽位**：`EXEC_CHILD_VA` 选用已清零的 `U_SLOT`(24)，因为其他低半区槽可能
   含 bootloader supervisor 项；不是通用槽位分配策略。
6. **只证明 2 个负载 CPU**：`-smp 4` 的 AP2/AP3 在线后停放；本命令使用 MTTCG。
7. **Gate 端到端与服务管理器未接线**：Gate spawn/fork 仍只有创建时放置的 host 单测
   （P5-3b-5），用户态 `prima-servicemgr` 流程未跑。
8. **无 IPI**：跨核唤醒仍按 owner 入队、由目标 CPU 空闲循环轮询；未实现确认/超时协议。
9. **探针资源不回收**：任务/页表帧、第二映像段与栈在收尾前不回收；不跑帧守恒或
   能力/审计总量审计。
10. **失败码诊断有限**：`FE_CODE` 记录位掩码或子进程 exec errno，但父进程核对与子进程
    失败可能分属不同 CPU；失败时以 `SMP|stage=sched` 的 `fecN` 定位。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`；
默认镜像不编译本 feature。P5-3b 的“并行 fork/exec 无丢唤醒”探针范围进一步闭合到
“fork 子进程 exec 第二映像 + 后置双向唤醒”；Gate 端到端、服务管理器流程、IPI/TLB
与 P5-3c 统一时间域未开始，P5-4 未开始。

## 后继关系

- 前序：[P5-3b-5 Gate 创建路径放置与反向丢唤醒](2026-11-28-p5-3b5-gate-placement-reverse-wakeup.md)。
- 上位：[S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：exec 后 fork 的取指缺页根因定位；Gate spawn/fork 端到端双核探针；服务管理器
  held-launch 流程；P5-4 IPI/TLB shootdown 与跨核唤醒确认。
