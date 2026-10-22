# 阶段 5 / M17b-3：fork 子进程僵尸保留与唤醒回收

**日期**：2026-10-23
**范围**：wait4 的进程生命周期模型修正（无 QEMU 证据）。
**结论**：fork 子进程退出后其 Process 作为僵尸保留到 `wait4` 回收；有等待者时
状态随唤醒交付并立即回收。host 用例覆盖；QEMU W6 仍有调度顺序问题（父线程
`wait4` 阻塞后以 `rax=0` 恢复、子进程未运行到 `SYS_PROCESS_EXIT`），未接入套件。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/bootstrap.rs` | `detach_thread` 仅在“无 parent 且无 exit_status”时删除 Process（fork 子进程保留僵尸）；`process_exit` 在存在被唤醒的等待者时回收 Process（状态已随唤醒交付）；新增两个 host 用例 |
| host 用例 | `forked_child_process_survives_task_destroy_until_reaped`（僵尸保留 → wait4 回收）、`process_exit_with_waiter_reaps_the_zombie`（唤醒即回收） |

## 2. 证据

```text
cargo test -p kernel forked_child_process / process_exit_with_waiter → 2 passed
cargo test --workspace → 全绿
cargo xtask lint / fmt → PASS
```

## 3. QEMU 现状（W6 仍失败）

手动 W6（未接入）：

```text
PROC[W]|fork=1;wait=0;status=0;cow=1;...
```

- `fork=1`、`cow=1`：COW fork 与写隔离成立；
- 父线程 `wait4` 进入 Blocked（`[W]`）后，以保存帧的 `rax=0` 恢复（`wait=0`）；
- 子进程从未打印 `SYS_PROCESS_EXIT` 入口标记（`[X]` 缺失），随后某任务在
  `SYS_RECV` 上无 Ready 后继 panic。
- 说明父线程在子进程运行/退出前被重新调度，或其保存帧被覆盖；下一步在
  `block_to`/`resume` 与 `process_exit` 的唤醒注入处加任务 id 级标记，
  确认实际调度序列。

## 3.1 调度/COW 追迹（本轮追加）

在 `resume`/`wait4`/`process_exit`/general COW 分支加任务 id 级标记后：

```text
[R08][R05] × 多次 → [R16] |fork=1;wait=0;status=0;cow=1...
COW 解析：成功 4 次（[K]），失败 2 次（[k]）；[X]（SYS_PROCESS_EXIT）0 次
```

- 子进程（id 5）与任务 8 反复交替，子进程**从未进入 SYS_PROCESS_EXIT**；
- general COW 有 4 次成功、2 次失败：失败的具体 VA/任务/原因（BadRoot 还是
  NoFrame）尚未打印；
- 父进程最终以 `wait=0` 恢复。

补充（同一轮后续）：`fork` 的返回标记为 `[F7>5]`（调用者 task 17、子 task 5），
说明 `fork` 正常返回；改用 `WNOHANG` 轮询时 `nohang` 字段为失败（非 0），
即子进程仍未被 `wait4` 找到（与 `[C0]` 一致）。下一轮需在 wait4 的 EChild
分支打印 Process 表的 `id/parent/leader` 与 caller/pid（标记要落盘并用
grep 校验），确认是 parent 匹配还是 leader/pid 语义问题；同时确认子进程
是否真正执行到 `SYS_PROCESS_EXIT`（`[X]` 仍缺失）。

## 4. 边界

- 僵尸保留只覆盖有 `parent` 的 fork 子进程；普通 Task 的最后一个线程离开仍
  删除 Process。
- W6 未接入；fork/exec/wait 的用户态证据仍缺。

## 5. 本报告不代表

- 不表示 fork/wait 可用；
- 不表示阶段 5 退出条件满足。
