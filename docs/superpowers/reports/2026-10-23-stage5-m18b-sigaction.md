# 阶段 5 / M18b：sigaction + handler 递送 + sigreturn

**日期**：2026-10-23
**范围**：用户 handler 的最小信号递送（自信号）；不包含掩码、`siginfo`、
`SA_RESTART`、`EINTR`、跨任务投递的即时性。
**结论**：W9 探针 QEMU 通过并与 Linux 同源参考逐字节一致：
`SIGACT|sigaction=1;kill=1;ran=1|ok=1;err=0`。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-abi` | `SYS_SIGACTION=57`、`SYS_SIGRETURN=58` |
| `kernel/src/signal.rs`（新） | 每 Task 32 个 handler 槽、32 位 pending 位图、被中断 `SretFrame` 槽（静态区，不占 BootstrapState）；`set_handler`/`handler_of`/`set_pending`/`pending`/`deliver`/`take_saved` |
| `kernel/src/syscall.rs` | `sigaction_call`（1..=31）；`sigreturn_call`（恢复现场并 `resume`）；`signal_call` 对有 handler 的目标只置 pending；`maybe_deliver_signals` 在 `handle`/`handle_fast` 的 `serve` 之后递送：无 handler → 默认终止（128+sig），有 handler → 保存现场（rax=syscall 返回值）并 `resume` 到 handler（rsp 16 对齐、rdi=sig）；`terminate_current` 复用退出/唤醒逻辑 |
| 探针 | `stage5_sigaction.c`：自信号 SIGUSR1、handler 置 `g_got` 后 `SYS_SIGRETURN`；Linux 参考用 `sigaction`/`kill` |
| xtask | `Sigact` 程序、Linux sigaction 参考、W9 双轮 |

## 2. 证据

- Linux 参考与 QEMU W9 输出一致；
- 全量回归：`cargo xtask test` **108 条 PASS，退出码 0**（W9 双轮 + 既有
  W6/W8/其余轮次全量重跑）；
- `cargo test --workspace` 全绿、`cargo xtask lint` PASS。

## 3. 边界

- 无 `sigprocmask`/掩码/`siginfo`/`SA_RESTART`/`EINTR`；
- 跨任务投递仅在目标下一次 syscall 返回时递送（阻塞中的任务不即时打断）；
- 同一时刻只递送一个 pending 信号；handler 返回必须显式 `SYS_SIGRETURN`
  （无 trampoline 自动返回）；
- 信号集 1..=31。

## 4. 本报告不代表

- 不表示完整 POSIX 信号语义；
- 不表示阶段 5 退出条件满足。
