# 阶段 5 / M17b-2b：general COW 与 fork syscall（W6 部分通过）

**日期**：2026-10-23
**范围**：把 COW 原语接到用户写故障与 `SYS_FORK`；`wait4`/`process_exit` 胶水。
**结论**：fork 成功、COW 写隔离成立（父进程看不到子进程对 `g_child` 的写），
但 `wait4` 在子进程退出前以 0 返回、子进程的 `SYS_PROCESS_EXIT` 未被观测到；
W6 未接入套件，退出/唤醒顺序留后续片。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/cow.rs` | general user COW 分支：写故障且非固定见证 VA 时，取当前任务的一个映射段 id、`vspace::resolve_cow` 复制帧、`invlpg`、重试原指令；非 COW 页仍走 Faulted |
| `kernel/src/bootstrap.rs` | `task_segment_id`（COW 帧归属）、`fork_create`（新 Process + Task、`inherit_cap_space`、vspace 模型条目复制）、`wait_status_ptr` |
| `kernel/src/syscall.rs` | `SYS_FORK(51)`：新 root 清零 + `clone_root_cow`、帧复制（rax=0、rsp 不变、FS base 继承）；`SYS_WAIT4(53)`；`SYS_PROCESS_EXIT(54)`；`Errno::Child` |
| `prima-user/c/` | 适配层 Linux 57/61 → 51/53；原生 54 |
| `prima-user/c/stage5_proc.c` | W6 探针（本次未接入套件） |

## 2. 手动 W6 结果

```text
PROC|fork=1;wait=0;status=0;cow=1;echild...  （随后 panic）
```

- `fork=1`：COW 克隆与子进程创建成功；
- `cow=1`：子进程写 `g_child=1` 对父进程不可见——**COW 写隔离成立**；
- `wait=0`：父进程 `wait4` 阻塞后以保存帧的 `rax=0` 恢复，未拿到子 pid；
- 子进程的 `SYS_PROCESS_EXIT` 未被观测（无 `[X]` 标记），随后某任务在
  `SYS_RECV` 上找不到 Ready 后继而 panic。

## 3. 分析

- COW 故障路径（`resolve_cow` + `invlpg` + 重试）工作，帧来自当前任务映射段；
- `wait4` 的 Blocked 分支保存帧并 `block_to(next)`；父进程以 0 恢复说明它
  在子进程退出/唤醒之前被重新调度（或唤醒注入丢失），子进程可能未实际运行到
  `SYS_PROCESS_EXIT`；
- 下一步：在 `process_exit`/`wait4` 的唤醒路径加标记（谁唤醒、注入值、waker
  的段/根），确认子进程是否运行、`process_exit` 是否设置状态并唤醒父进程；
  同时检查 fork 子进程的 kstack/帧与 `block_to` 的交互。

## 4. 边界

- W6 未接入；fork/wait 无套件证据；
- COW 复制帧的归属为父进程的某个映射段（随该段销毁释放），子进程退出时
  不单独回收；
- 子进程退出时 root 的页表帧走延迟释放；fork 的 vspace 模型条目已复制，
  销毁路径会逐项 unmap。

## 5. 本报告不代表

- 不表示 fork/wait 可用；
- 不表示阶段 5 退出条件满足。
