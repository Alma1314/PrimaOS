# 阶段 5 / M17b-1：wait4 状态机模型实现报告

**日期**：2026-10-23
**范围**：M17 的 wait4 模型片（无 syscall/COW）。设计见
[M17 设计](../specs/2026-10-23-stage5-m17-fork-exec-design.md)。
**结论**：bootstrap 模型具备 `process_exit`/`wait4` 的阻塞、唤醒、回收与
`ECHILD`/`WNOHANG` 语义，host 用例通过；内核 syscall、COW root 克隆与
ELF exec 尚未接线。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/bootstrap.rs` | `Process.waiters`（u32 位图 + 计数）、`TaskRecord.waiting_process`/`wait_status_ptr`；`process_exit(process, status) -> WaitWake`；`wait4(caller, pid, status_ptr, no_hang) -> Wait4Outcome`；`Wait4Outcome`/`Wait4Error`；全局包装 |
| 语义 | 子进程未退出时父 Task 置 `BlockedWait` 并登记到子 Process；子退出设置 `exit_status` 并唤醒；再次 `wait4` 返回 `(pid, status)` 并回收子 Process/Task；无子进程 `EChild`；`WNOHANG` 返回 `None` |

## 2. 证据

```text
cargo test -p kernel wait4_            → 2 passed
cargo test --workspace                 → 全绿
cargo xtask lint / fmt                 → PASS
```

用例：
- `wait4_blocks_then_child_exit_wakes_and_reaps`：BlockedWait → 子退出唤醒 →
  返回 `(2, 7)` → 再 wait 得 `ECHILD`；
- `wait4_nohang_reports_running_child_and_echild_without_children`。

## 3. 追加实验：共享 root 的 fork 不可用

尝试先用“共享地址空间”的 fork（SYS_FORK 51 + SYS_PROCESS_EXIT 54 +
SYS_WAIT4 53）验证生命周期，QEMU 实测：

- 父线程 fork 返回、wait4 阻塞（`[F][W]`），子线程（id 5）为 Ready；
- 子线程被调度后**没有执行退出**，而是在 7/7/8 号 syscall 上循环；
- 根因：fork 子与父**共享用户栈**，父线程在 write/wait 路径上的调用帧与
  子线程的执行互相踩踏；共享 root 的“fork-lite”不是可用语义。

结论：fork 必须先有**独立的地址空间**（深拷贝或 COW；含独立用户栈），
不能用线程式共享 root 近似。实验代码已回退，未提交。

### 3.1 深拷贝实验（本片追加）

随后实现 `vspace::copy_user_space`（遍历用户 4 KiB 叶、可写页复制到保留页表帧、
只读页共享）并接 `SYS_FORK`。QEMU 中 fork 在 `copy_user_space` 的首个页表帧
分配处失败（`[VA]`）：C 进程的可写映射包含约 1.8 MB `.bss`，**保留页表帧池**
不足以承载全量深拷贝。结论：fork 必须走 **COW**（父子共享帧、写时复制，
帧来自段池/按需分配），而不是全量深拷贝；且新 root 帧需先清零。
该实验已回退；`ISSUE-FORK-COW-001` 更新为“COW + 按需帧 + 段/池归属”的
实现要求。

## 4. 边界与下一步

- 未实现 `SYS_FORK`/`SYS_WAIT4`/`SYS_PROCESS_EXIT` 的 syscall 胶水；
- 未实现 fork 的 COW root 克隆与 general user COW（当前 `clone_root` 仅浅拷贝
  PML4，fork 需要深拷贝/COW 故障处理）；
- 未实现 ELF exec；
- 下一步：内核 fork（深拷贝用户页或 COW）+ 进程退出 syscall + wait4 胶水 +
  W6 探针（fork/COW/退出码/ECHILD），随后 W7 exec/argv。

## 4. 本报告不代表

- 不表示 fork/exec/wait 用户态可用；
- 不表示阶段 5 退出条件满足。
