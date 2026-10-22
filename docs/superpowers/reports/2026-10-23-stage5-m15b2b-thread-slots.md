# 阶段 5 / M15b-2b：线程槽位回收与 W2 端到端报告

**日期**：2026-10-23
**范围**：关闭 `ISSUE-THREAD-CAPACITY-001`，让共享线程在 18 槽位配置下可创建、
可端到端唤醒 WaitObject。设计见
[M15 设计](../specs/2026-10-23-stage5-m15-process-thread-wait-design.md)。
**结论**：`W2` 双轮通过；主线程阻塞等待、共享 Process 线程 signal、线程自毁、
主线程继续并销毁对象全部成立；`task_ready`/`TASKCAP` 容量见证保持 12/132。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/bootstrap.rs` | `TaskTable.status_of`/`is_dead`/墓碑位；`ensure_thread_slot`（仅线程用）：表满时优先复用**没有存活 cap 指向**的 Dead 记录，否则 `revoke_task_caps` 后复用；`cap_targets_task`；`create_task_record` |
| `kernel/src/user.rs`、`syscall.rs` | 无接口变化（`thread_create` 走新的槽位分配） |
| `prima-user/c/stage5_wait2.c` | 共享线程探针（重新加入） |
| `xtask/src/image.rs`、`main.rs` | `Stage5Program::Wait2`、`WAIT2_EXPECTED_STDOUT`、`W2` 双轮 |

关键取舍：**`task_create` 不回收 Dead 槽位**，保留 manager 容量负例“表满 →
`ENOSPC`”的语义与 boot 见证；只有 `thread_create` 在确实需要时惰性回收，且优先
选择无 cap 指向的记录，避免改变 `caps_live` 计数。

## 2. 语义

- `finish_destroy` 仍保留 Dead 记录（`task_status` 返回 Dead、`live_task_count`
  排除 Dead），因此 boot 的容量负例与 `tasks=12` 见证不变。
- `thread_create`：`next_free_id` 为空时遍历 Dead 记录：
  - 若没有存活 cap 指向该 Task → 直接移除记录并复用 id（cap 计数不变）；
  - 否则先 `revoke_task_caps`（撤销全部 target 为该 Task 的 cap）再复用，
    防止旧句柄复活；
  - 仍无可用槽位 → `ENOSPC`。
- `revoke_task_caps` 通过 `CapabilityRecordTable::get_index` 扫描全部槽位，
  收集后逐条 `retire_capability`（避免借用冲突）。

## 3. 证据

### 3.1 host

```text
cargo test --workspace  → 414 passed
```

（`status_of` 墓碑路径、Dead 记录复用与既有生命周期用例全部通过。）

### 3.2 QEMU

`W2` 双轮输出逐字节一致：

```text
WAIT2|create=1;thread=1;satisfied=1;gen=1;destroy=1|ok=1;err=0
```

证据含义：
- `thread=1`：18 槽位已满时线程仍创建成功（Dead 槽位回收）；
- `satisfied=1`：主线程 `BlockedWait` 被共享线程的 `signal_object` 唤醒，
  syscall 返回 `WaitSatisfied`；
- `gen=1`：跨线程使用同一能力空间的 WaitObject cap；
- `destroy=1`：主线程销毁对象；
- 线程通过 `rdi` 的自毁 cap 调用 `SYS_TASK_DESTROY` 退出，未泄漏 Task。

`TASKCAP|id=17;dead=1;root_cleared=1;tasks=12;caps=132` 与既有轮次一致。

### 3.3 全量回归

```text
cargo xtask test
```

结果（86 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 / T1 / F1 / V1 / V2 / V3 / N1 / W1 / W2 / I1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 全部参考
PASS stage1 boot slice
```

`W2` 两轮与既有轮次全部通过；`task_ready`/`TASKCAP` 见证保持 12/132。

## 4. 边界

- 线程仍无 join/分离/取消；`tls=0` 继承 leader 的 FS base。
- 只有 `thread_create` 回收 Dead 槽位；`task_create` 满时仍 `ENOSPC`（有意）。
- 若所有 Dead 记录都有 cap 指向，回收会撤销这些 cap（可能改变 `caps_live`）；
  当前 boot 配置走“无 cap”分支，计数不变。
- 共享能力空间是工作决定（`ISSUE-PTHREAD-CAPS-001`），仍待 COSIR 组织级确认。

## 5. 问题关闭

`ISSUE-THREAD-CAPACITY-001` 关闭：18 槽位下线程可创建，W2 端到端通过，
容量见证未变。`ISSUE-PTHREAD-CAPS-001` 维持工作决定状态。

## 6. 本报告不代表

- 不表示 M16 pthread/futex 已实现；
- 不表示阶段 5 退出条件已满足。
