---
id: report-p5-3b5-gate-placement-reverse-wakeup
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-28
---

# P5-3b-5：Gate 创建路径放置与反向跨核丢唤醒矩阵

## 结论与范围

本切片是 [S7-B.1](../specs/2026-11-27-stage7-smp-state-partition.md) P5-3b 的第五步：

1. 把 `SYS_GATE_SPAWN_THREAD` 与 `SYS_GATE_FORK` 创建的子任务接入创建时放置，
   规则固定为**随客户进程的 owner CPU 发布**（门线程与客户共享 root，门 fork
   子任务由父 COW 派生；§5.1 禁止跨 CPU 共享 root）。
2. 在 `test-sched` 的 exec 后新映像中增加 **CPU1→CPU0 反向 4 轮跨核阻塞/唤醒**
   （`reverse_ok`），补上 P5-3b-4 只测单向的缺口。

**不是运行期启用。** 普通镜像不编译 `sched-percpu-probe`，默认 QEMU 参数与默认镜像
事件不变。Gate 放置只有 host 单测证据：本探针不构造 Gate 服务/请求，用户态服务管理器
（`prima-servicemgr`）的锁与 held-launch 流程仍未跑；Gate spawn/fork 的端到端双核
行为未验证。程序级并行 fork/exec 矩阵、IPI/TLB 与 P5-4 仍未开始。

## 基线与输入

- 代码基线：`HEAD a87aaea`（P5-3b-3/4），本切片未提交。
- 上位：[S7-B.1 §5.1/§7/§8](../specs/2026-11-27-stage7-smp-state-partition.md)。
- QEMU：`pc-i440fx-8.2`、`qemu64`、128 MiB、`tcg,thread=multi`（仅本命令；理由同
  P5-3b-3）。

## 实现内容

| 路径 | 内容 |
|---|---|
| `kernel/src/syscall.rs` | 新增 `publish_gate_child(child, client)`：取客户 owner CPU 后一次发布；`SYS_GATE_SPAWN_THREAD` 成功后发布门线程，`SYS_GATE_FORK` 成功后发布门 fork 子任务（失败不执行后续回执） |
| `kernel/src/bootstrap.rs` | 仅 `sched-percpu-probe`：`PROBE_TASK1_SEND_CAP_SLOT`(31) / `PROBE_TASK1_SEND_CAP_HANDLE` 与 `probe_grant_reverse_send()`，为 task 2 铸目标 `Task(1)` 的 `TaskSend` 根能力；默认镜像槽位布局不变 |
| `kernel/src/sched_probe.rs` | exec 后新映像在正向 4 轮之后增加反向相位：CPU1 每轮先等 task 1 进入 `BlockedRecv` 再 `SYS_SEND`，CPU0 阻塞 `SYS_RECV` 后核对载荷并回 ACK；新增 `REV_*` 魔数、`REV_ACK/REV_OK/REV_FAIL` 与 `reverse_ok` 见证 |
| `kernel/src/cpu.rs` | `gs3-probe` 的 host 测试桩：`install_gs`/`local_apic_id` 在 `cfg(test)` 下不写 MSR、不读 `%gs`；仅用于运行含该 feature 的 host 单测，生产镜像代码路径不变 |
| `kernel/src/protocol.rs` | `SMP|sched=` 追加 `reverse_ok`；`SMP|stage=sched` 诊断追加每 CPU `rvN` |
| `xtask/src/protocol.rs` | `validate_sched` 新增 `reverse_ok` 期望 2 与正反例；错误文案改为双向矩阵 |

**放置语义**：门线程与客户共享 root，门 fork 子任务由父 root COW 派生；两者都选择
`request.client` 的 owner CPU，而不是执行 syscall 的服务 CPU，以满足“每个 root 仅在
一个 CPU 执行”。客户在门 trap 前已经由现有创建路径发布，故 owner 必然存在；发布失败
按探针内不可达处理，使用 `expect` fail-stop（与 P5-3b-4 的创建时放置一致），不做半发布
静默丢弃。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features sched-percpu-probe -- test-sched` | PASS：`-smp 2/4` × 2 轮；同 vCPU 数两轮逐字节相同 | QEMU BIOS，MTTCG，2/4 vCPU |
| `cargo test -p xtask --features sched-percpu-probe smp_tests` | PASS：5 | host（`validate_sched` 正反例） |
| `cargo test -p kernel --features sched-percpu-probe sched_probe::tests` | PASS：3 | host（含 exec 单段与见证全等反例） |
| `cargo test -p kernel --features sched-percpu-probe gate_child_is_placed_on_the_client_owner_cpu` | PASS：1 | host（Gate 放置规则） |
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
SMP|sched=cpus=2;online=2;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;launch_ok=1;prepared_ok=2;thread_ok=2;exec_ok=2;lost_ok=2;reverse_ok=2;PASS
SMP|PASS

SMP|sched=cpus=2;online=4;place_ok=2;single_ok=2;queues_ok=2;barrier_ok=2;wake_ok=2;fork_ok=2;child_ok=2;nested_ok=2;launch_ok=1;prepared_ok=2;thread_ok=2;exec_ok=2;lost_ok=2;reverse_ok=2;PASS
SMP|PASS
```

日志 SHA-256：`SCHED-2-{1,2}` 均
`eef0c062c9fcf72cc3e51be7131aafbc88cdd0674e3e3f684274c03365bd7e57`；
`SCHED-4-{1,2}` 均
`9faa0dbd1e83784fbf094b3c648ac9cad3bcd2ae5eae61b89a826e22c04b9e25`。
同 vCPU 数两轮逐字节相同（含 MTTCG 并发执行）。

逐项对应本片验收：

- **Gate 放置（host）**：`publish_gate_child` 对已发布客户取 owner CPU；单测证明子任务
  的 owner 与客户一致，而不是调用者所在 CPU；与 §5.1 的共享 root 单 CPU 限制一致。
- **`reverse_ok`**：exec 后新映像先完成正向 4 轮；随后 CPU1 每轮等待 task 1 进入
  `BlockedRecv`，再真实 `SYS_SEND`；CPU0 的阻塞 `SYS_RECV` 被直投唤醒、核对
  `PAYLOAD+round` 后回 ACK；两核在 `MAGIC_REV_DONE` 复核对全部 4 轮 ACK 为 1 且无等待超时。

## 失败、反例与限制

1. **Gate 放置无 QEMU 端到端证据**：`test-sched` 不构造 Gate 服务/请求；仅 host 单测
   覆盖“随客户 owner CPU”规则。`SYS_GATE_SPAWN_THREAD`/`SYS_GATE_FORK` 的双核实际行为、
   门绑定初始状态与回滚路径未在机器上观察。
2. **服务管理器 held-launch 流程未跑**：`prima-servicemgr` 的用户态门流程仍在探针镜像
   之外。
3. **放置失败用 `expect`**：客户未发布或队列容量耗尽时 fail-stop，不返回错误；探针内
   按不可达处理，但没有回滚到门 attach 之前。
4. **只证明 2 个负载 CPU**：`-smp 4` 的 AP2/AP3 在线后停放；本片不声称 4 路负载。
5. **本命令使用 MTTCG**；默认命令仍单线程 TCG。
6. **反向矩阵为 exec 后同映像内追加相位**：仍是手构单 `PT_LOAD` ELF、无动态链接/argv，
   接收输出放在可写 exec 栈上；不主张通用 ELF 或信号/截止时间语义。
7. **host 测试桩**：`cpu.rs` 的 `gs3-probe`/`cfg(test)` 桩只让 host 单测可编译运行，
   不进入任何镜像；不改变 GS-3 的机器证据。
8. **无 IPI**：反向唤醒仍按 owner 入队、由目标 CPU 空闲循环轮询；未实现确认/超时协议，
   P5-4 前不声称跨核唤醒确认。
9. **程序级并行 fork/exec 矩阵未做**：本片只增加 exec 后的反向 SEND/RECV；fork 与 exec
   交叉、共享 root 并行与 handoff 未覆盖。
10. **探针资源不回收**：任务/页表帧、held 任务代码段与栈段在收尾前不回收；不跑帧守恒
    或能力/审计总量审计。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`；
默认镜像不编译本 feature。P5-3b 的“Gate 创建路径放置规则”（host）与双向丢唤醒矩阵
（exec 后）完成；服务管理器流程、程序级 fork/exec 矩阵、IPI/TLB 与 P5-3c 统一时间域
未开始，P5-4 未开始。

## 后继关系

- 前序：[P5-3b-4 held-launch 放置、双核 exec 与丢唤醒矩阵](2026-11-28-p5-3b4-launch-exec-lost-wakeup.md)。
- 上位：[S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：Gate spawn/fork 端到端双核探针（含门绑定初始状态与失败回滚）；服务管理器
  held-launch 流程；程序级并行 fork/exec 丢唤醒矩阵；随后 P5-4 IPI/TLB shootdown 与
  跨核唤醒确认。
