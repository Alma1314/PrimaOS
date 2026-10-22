# 阶段 5 / M18a：最小信号（kill 默认终止）与 wait4 信号状态

**日期**：2026-10-23
**范围**：`SYS_SIGNAL(56)` = `kill(pid, sig)` 的默认动作（终止），不包含
handler/sigaction/sigreturn/阻塞掩码。
**结论**：W8 探针 QEMU 通过并与 Linux 同源参考逐字节一致：
`SIG|kill=1;wait=1;signal=1|ok=1;err=0`。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-abi` | `SYS_SIGNAL = 56`（`kill(pid, sig)`） |
| `kernel/src/syscall.rs` | `signal_call`：校验 sig（1..=31）；`process_exit(process, 128+sig)`；唤醒 wait4 等待者（pending status + rax 注入）；自信号走 `process_exit_call`；其它目标销毁其 leader Task（root 拆除，`BadRoot` 容忍） |
| 适配层 | Linux `kill=62` → 56 |
| 探针 | `stage5_signal.c`：`raw_fork` 子进程（不应运行）→ 父 `kill(pid,15)` → `wait4` 校验状态（Linux `status&0x7f==15`；Prima `status>>8&0xff==143`） |
| xtask | `Sig` 程序、Linux kill/wait 参考、W8 双轮 |

## 2. 证据

- Linux 参考（真实 fork/kill/wait4）与 QEMU W8 输出一致；
- 全量回归：`cargo xtask test` **104 条 PASS，退出码 0**（W8 双轮 + 既有
  W6/其余轮次全量重跑）；
- 状态编码：Prima 把信号终止记为退出码 `128+sig`，经 `wait4` 的
  `(status & 0xff) << 8` 写回；探针同时接受 Linux 的 `WIFSIGNALED` 形态。

## 3. 边界

- 只有**默认终止**；`sigaction`/handler/`sigreturn`/`sigprocmask` 未实现；
- 信号集为 1..=31，无实时信号；
- 目标进程的**所有线程**未逐个清理（当前单线程 fork 场景只销毁 leader）；
- `SIGCHLD` 自动通知、`EINTR`、`SA_RESTART` 未做。

## 4. 本报告不代表

- 不表示 POSIX 信号可用；
- 不表示阶段 5 退出条件满足。
