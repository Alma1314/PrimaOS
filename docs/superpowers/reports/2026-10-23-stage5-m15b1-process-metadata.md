# 阶段 5 / M15b-1：Process/Thread 元数据实现报告

**日期**：2026-10-23
**范围**：M15b 的第一片（无用户 API）。设计见
[M15 设计](../specs/2026-10-23-stage5-m15-process-thread-wait-design.md)。
**结论**：内核 bootstrap 模型新增 Process 表与 Task→Process 归属；`task_create`
为新 Task 建立单线程 Process，销毁时摘除（最后一个线程离开即删除 Process）。
host 用例覆盖共享/摘除/幂等/非法 id。**用户态线程创建与共享地址空间尚未接线**；
发现并登记了阻塞 M16 的 `ISSUE-PTHREAD-CAPS-001`（每 Task Capability 空间与
线程共享 fd/cap 的冲突）。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/bootstrap.rs` | `Process`/`ProcessTable`（MAX_PROCESSES=MAX_TASKS）、`TaskRecord.process`、`create_process`/`attach_thread`/`detach_thread`/`process_of`/`process_threads`；`task_create` 前置容量检查 + 成功动作 8 建 Process；`destroy_task_body` 摘除 |
| 线程表示 | Process 用 `u32` 位图（task id 1..=MAX_TASKS）而非数组，守住 `BootstrapState` 栈预算（13944 → 约 12.2 KiB） |
| 证据 | 新 host 用例 `process_groups_tasks_and_last_thread_removes_it` |

## 2. 语义（本切片）

- 每个经 `task_create` 创建的 Task 自动获得一个新的 Process（单线程）；
- `attach_thread(process, task)`：校验 Task 存在、未重复、位图有位；重复挂载幂等；
  同一 Task 只属于一个 Process；
- `detach_thread(task)`：清位、计数减一；计数归零删除 Process；
- `process_of`/`process_threads` 供模型与后续 syscall 使用；
- Process 容量与 Task 表同（18）；满时 `task_create` 返回 `CapacityExceeded`。

## 3. 证据

### 3.1 host

```text
cargo test -p kernel process_groups                      → 1 passed
cargo test -p kernel bootstrap_state_stays_within_the_stack_budget → 1 passed
cargo test --workspace                                   → 全绿
```

### 3.2 QEMU

本切片不改变用户可见行为；既有轮次（含 W1/I1）全量重跑：

```text
cargo xtask test
```

结果（83 条 `PASS` 行，退出码 0）：与 M15a-2 完全一致；本切片只增加模型
簿记，不改变用户可见行为。

## 4. ISSUE-PTHREAD-CAPS-001（新登记）

COSIR 的 Capability 位置空间是 **每 Task** 的；线程共享地址空间后，适配层的
fd 表与内核 cap 句柄位于共享内存，但线程 B 调用 `holder_of(cap) == B` 会失败
（cap 由线程 A 持有）。因此：
- 用户态 `thread_create`（47）即使共享 root/CR3，也无法让新线程使用既有 fd/cap；
- M16 的 pthread/futex 需要先决定：**Process 共享 Capability 空间**，或
  **线程创建时复制/迁移 cap 且保持句柄值一致**（当前句柄值按 holder 唯一，
  需要改模型）。

在 D3/该问题关闭前，本切片不实现用户态线程创建；`M15b-2` 阻塞在此。

## 5. 边界

- 无用户 API、无共享 CR3、无 TLS/join；Process 只是模型对象。
- 未与异常/等待状态交互（线程退出、BlockedWait 清理在后续片）。
- 不改变 `task_create` 的既有语义/审计事件序列。

## 6. 本报告不代表

- 不表示线程/共享地址空间已实现；
- 不表示 M16 pthread 可开始；
- 不表示阶段 5 退出条件已满足。
