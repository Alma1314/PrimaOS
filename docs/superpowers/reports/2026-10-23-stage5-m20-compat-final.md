# 阶段 5 / M20：兼容矩阵定稿

**日期**：2026-10-23
**范围**：按已验收证据定稿 [compat-state](../compat/compat-state.md) §6 与
[symbol-trace](../compat/symbol-trace.md) §5。本文只汇总，不新增实现。
**结论**：M12–M18 的进程/线程/信号/exec 子系统均有 QEMU 双轮（部分含 Linux
同源）证据；剩余项集中在信号掩码、pthread 取消/分离、TTY 轮询、fork 子任务
VFS 授权与 `syscall(57)` 续执行。

## 1. 证据矩阵（摘要）

| 项 | 证据 |
|---|---|
| W1/W2 wait object + thread | 双轮 |
| W3 futex | 双轮 |
| W4 pthread create/join | 双轮 + Linux 参考 |
| W6 fork/wait/COW/status/ECHILD | 双轮 + Linux 参考 |
| W7 exec（内存镜像、ENOEXEC） | 双轮 |
| W7b VFS 镜像 exec | 双轮 |
| W8 kill 默认终止 + wait4 信号状态 | 双轮 + Linux 参考 |
| W9 sigaction/handler/sigreturn | 双轮 + Linux 参考 |
| 全量套件 | 114 条 PASS（`cargo xtask test`） |

## 2. 新增 syscall 台账（42–58）

- 42–46 WaitObject、47 `thread_create`、48 `clone`、49 `thread_exit`、
  50 `thread_clear_tid`、51 `fork`、53 `wait4`、54 `process_exit`、
  55 `exec_replace`、56 `signal`、57 `sigaction`、58 `sigreturn`：均已实现
  并有对应探针；52 预留。
- 所有入口均为 Prima 原生号，适配层负责 Linux 号（56/57/59/61/62 等）映射。

## 3. 明确不支持 / 已知限制

- 信号：掩码、`siginfo`、`EINTR`、`SA_RESTART`、实时信号、`SIGCHLD` 自动通知；
- 进程：进程组/session、`vfork`、`posix_spawn`、`ptrace`、多线程 fork（EAGAIN）、
  `syscall(57)` 的 fork 子进程续执行（`ISSUE-FORK-CONTINUATION-001`）；
- exec：动态链接/PIE/解释器、`envp`、`CLOEXEC`、子任务内 VFS `execve`（按任务
  授权限制）；
- 线程：pthread 取消/分离/属性/优先级；
- TTY：字间定时/流控；网络：仅环回 UDP 子集；
- VFS：无硬链接、跨启动持久化。

## 4. 本报告不代表

- 不表示完整 POSIX 兼容；
- 不表示阶段 5 退出条件全部满足（见跟踪页与路线图）。
