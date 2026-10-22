# 阶段 5 / M17b-4：fork/wait 打通（W6）

**日期**：2026-10-23
**范围**：单线程 fork（COW 地址空间）、子进程退出、wait4 状态/回收、ECHILD。
**结论**：W6 探针在 QEMU 中通过：
`PROC|fork=1;wait=1;status=1;cow=1;echild=1|ok=1;err=0`，与 Linux 同源参考
逐字节一致；COW 写隔离、退出码、ECHILD 均成立。**经 musl/适配层
`syscall(57)` 的 fork 子进程返回值仍损坏**，W6 使用内联 asm 直发
`SYS_FORK=51`（Prima 原生路径）；该偏差已登记。

## 1. 根因与修复

| 问题 | 修复 |
|---|---|
| 子进程 `SYS_PROCESS_EXIT` 的 `task_destroy_self` 因 `unmap_4k(BadRoot)` 失败（子根中模型条目未映射）→ `NoSuchTask`，子 Task 仍 Running，`exit_to` 报 `AlreadyRunning` | 拆除时把 `BadRoot` 视为“已拆除”，继续处理其余条目 |
| 唤醒者（子进程）写父进程的 `status_ptr` 落在**子进程的 COW 副本**上，父进程看到 0 | `sched` 增加 pending status：`process_exit_call` 记录 `(ptr,status)`；`resume` 在**切到目标根之后**先对目标页 `resolve_cow`、再写 4 字节 |
| 子进程经 musl/适配层返回时 `fork` 返回值损坏（走父路径） | W6 用内联 asm 直发 51；`syscall(57)` 路径待后续修复（ISSUE-FORK-CONTINUATION-001） |
| W6 的容量 trace/退出码 | 随探针完成自动满足（全量 100 条 PASS） |

## 2. 证据

- Linux 参考（`cc -DPRIMA_HOST`，真实 fork/wait4/_exit）：
  `PROC|fork=1;wait=1;status=1;cow=1;echild=1|ok=1;err=0`
- QEMU W6 双轮：与上逐字节一致（`PASS qemu bios W6 run=1/run=2`）
- 全量回归：`cargo xtask test` **100 条 PASS，退出码 0**（W6 双轮 +
  Linux fork/wait 参考 + 既有轮次全量重跑）
- `cargo test --workspace`：全绿（host 模型含僵尸/唤醒/回收用例）
- `cargo xtask lint`、`fmt`：PASS

## 3. 边界

- 只覆盖**单线程** fork；多线程 fork 的 `EAGAIN` 未接线（模型已返回 Busy）。
- fork 子进程的 COW 复制帧归属父进程的映射段，随该段销毁释放；子进程退出时
  页表帧走延迟释放。
- `execve` 未实现（M17c）。
- `syscall(57)` 的 musl 返回路径损坏（子进程拿到非 0 返回值），W6 用原生 51。

## 3.1 追加：多线程 fork 的 EAGAIN 边界

`fork_create` 已对 `process_threads > 1` 返回 `ThreadError::Busy`（syscall 层
映射 EAGAIN）；host 用例 `fork_rejects_multithreaded_process`（父进程 + 一个
`thread_create` 线程 → fork 返回 Busy）通过。

## 4. 本报告不代表

- 不表示 fork/exec/wait 的完整 POSIX 语义（进程组/session、vfork、
  posix_spawn、CLOEXEC 等未做）；
- 不表示阶段 5 退出条件满足。
