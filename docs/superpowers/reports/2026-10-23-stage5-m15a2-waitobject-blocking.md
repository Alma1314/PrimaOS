# 阶段 5 / M15a-2：WaitObject 阻塞/唤醒/超时实现报告

**日期**：2026-10-23
**范围**：M15a-2。在 M15a-1 的立即路径之上接线 `BlockedWait`、等待队列、
signal/destroy/超时唤醒与结果注入。
**结论**：WaitObject 的阻塞语义在 host 状态机完整覆盖；`W1` 的阻塞用例因
“没有第二个用户任务可 signal”而移除（保留立即路径证据），阻塞唤醒的
QEMU 端到端验证留待 M15b 的多 Task 场景。`W1` 双轮与全量回归通过。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/bootstrap.rs` | `TaskStatus::BlockedWait`；`WaitEntry`/`WaitRequest`/`WaitResultCode`；`TaskRecord.wait_request`/`pending_wait_result`；`WaitObject.waiters`（≤8）；`WaitWake`/`SignalOutcome`；block/signal/destroy/expire；`WaitSignaled` 审计；`detach_waiter` |
| `kernel/src/syscall.rs` | `wait_objects` 阻塞并切走、signal/destroy 唤醒注入 rax、`serve` 入口推进超时与消费 pending 结果 |
| `prima-user/c/stage5_wait.c` | 移除“无 signaler 的阻塞”字段（会挂起）；保留 create/observe/signal/satisfied/timeout/invalid/destroy/dead |
| `xtask/src/image.rs` | `WAIT_EXPECTED_STDOUT` 同步 |

## 2. 语义

- `wait_objects`：满足检查 → 立即超时 → **登记** `WaitRequest`、加入每个对象的
  waiters、状态 `BlockedWait`、追加 `TaskStateChanged`、返回 `Blocked`；syscall
  保存帧（rax=0）并 `block_to(next)`。
- `signal_object`：generation++、`WaitSignaled`；行为：
  - Any：任一 entry 的 generation 超过观测即唤醒；
  - All：全部 entry 超过观测才唤醒；
  - 唤醒 = 从所有 waiters 移除、清空请求、`pending_wait_result=Satisfied`、
    状态 `Ready`、追加 `TaskStateChanged`。
- `destroy`：所有等待者以 `TargetDead` 唤醒；对象删除、`ObjectDestroyed`。
- 超时：`serve` 入口调用 `expire_waits(now)`；deadline 到期者以 `TimedOut`
  唤醒（无 IRQ 时时间由 syscall 推进）。
- 结果注入：signal/destroy/expire 的 waker 在 syscall 上下文调用
  `sched::set_frame_rax(woken, code)`；被唤醒任务返回时 `rax` 即 0/1/2。

## 3. 证据

### 3.1 host

```text
cargo test -p kernel wait_  → 12 passed
```

新增 3 例：
- `wait_blocks_then_signal_wakes_satisfied`：BlockedWait → signal → Ready、
  `Satisfied`、waiters 清空；
- `wait_expires_only_at_the_deadline`：deadline-1 不唤醒，deadline 唤醒
  `TimedOut`；
- `wait_destroy_wakes_target_dead`：destroy → `TargetDead`。

### 3.2 QEMU

`W1` 双轮（探针已移除无 signaler 的阻塞字段）：

```text
WAIT|create=1;gen0=1;signal=1;satisfied=1;timeout=1;invalid=1;destroy=1;dead=1|ok=1;err=0
```

阻塞路径的 QEMU 验证需要第二个用户 Task（M15b 的共享地址空间线程或
manager 派生任务）在等待期间 signal；当前 W1 不能安全构造，登记为限制。

### 3.3 全量回归

```text
cargo xtask test
```

结果（83 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 / T1 / F1 / V1 / V2 / V3 / N1 / W1 / I1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 全部参考
PASS stage1 boot slice
```

`W1` 两轮与既有轮次均通过；host 12 个 wait 用例全绿。

## 4. 边界与后续

- `alertable`/upcall（`WaitAlerted`）留 M18；等待者上限 8/对象。
- `pending_wait_result` 在任务下一次 syscall 入口消费；结果本身已在 rax。
- 阻塞等待要求存在另一个 Ready Task 才能切走；`pick_next(None)` 会 panic
  （与 SYS_RECV 相同的前置）。
- 下一步：M15b 共享地址空间 Thread（可 signal 的第二个执行流）、把 W1 的
  阻塞用例端到端化。

## 5. 复现命令

```sh
cargo test --workspace
cargo xtask test
```

## 6. 本报告不代表

- 不表示 QEMU 已验证阻塞唤醒；
- 不表示 Process/Thread 或 D3 已关闭；
- 不表示阶段 5 退出条件已满足。
