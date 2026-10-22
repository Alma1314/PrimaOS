# 阶段 5 / M16b-2：musl clone 路由与 pthread 进展

**日期**：2026-10-23
**范围**：为 musl 的快照打 `clone.s` 补丁，让 `pthread_create` 走 Prima 的
`SYS_CLONE(48)`；验证到 `create` 成功、`join` 仍有一个唤醒顺序问题。
**结论**：
- musl `clone.s` 的裸 `syscall` 56 → 48（子线程退出回退 60 → 49）；
- 内核 `SYS_CLONE` 按 Linux 参数序 `(flags, stack, parent_tid, child_tid, tls)`
  接收并写 `parent_tid`；用户栈对齐要求放宽到 8 字节（musl 传 `...8`）；
- 适配层 PROT_NONE 的 `mmap` 改为按 RW 映射（musl `pthread_create` 先用
  PROT_NONE 占位再 `mprotect`）；
- QEMU 中 `pthread_create` **返回 0**（不再 EAGAIN），但 `pthread_join` 在
  futex 等待处 panic：子线程（复用 Dead id 5）在父线程登记 futex 等待前已
  退出，`wait_objects` 阻塞时无 Ready 后继。**W4 未接入套件**，继续排查。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-user/musl/patches/clone.s`、`build.sh` | musl x86_64 `clone.s` 补丁并纳入构建戳：`mov $56` → `$48`、子线程回退 `$60` → `$49` |
| `kernel/src/syscall.rs` | `clone_call(flags, stack, parent_tid, child_tid, tls)`：Linux 参数序、`parent_tid` 写 u32、`CLONE_DETACHED` 允许（musl 设置） |
| `kernel/src/bootstrap.rs` | `thread_create` 用户栈对齐放宽到 8 字节（musl `stack-8`） |
| `prima-user/c/prima_syscall.c` | PROT_NONE mmap 按 RW 映射；`clone(56)` 适配（Linux 参数序）；`exit(60)` 在线程表无记录时先试 `SYS_THREAD_EXIT(49)` 再回退进程退出 |

## 2. 证据

- Linux 参考：`PTHREAD|create=1;join=1;ret=1;value=1|ok=1;err=0`（真实 pthread）。
- QEMU（未接入）：`PTHREAD[S][K]|create=1` 后主线程 join 阻塞，
  内核 panic `SYS_WAIT: no Ready task`；调试转储显示子线程 id 5 已是 `Dead`，
  父线程是 `BlockedWait`。说明子线程在父线程进入 futex 等待前已完成退出，
  且 join 的 `detach_state` 唤醒未被父线程的 futex 等待捕获（典型
  “唤醒先于等待登记”顺序）。
- 既有套件：`cargo xtask test` **89 条 PASS**（EXIT 0），W1/W2/W3 与既有轮次全量重跑通过，clone.s 补丁与 mmap 变更无回归。

## 3. 分析与下一步

- musl `pthread_join` 在 `t->detach_state` 上 `__timedwait_cp`；退出线程在
  `__pthread_exit` 里 `a_store(DT_EXITED); __wake(detach_state)` 后 `exit(60)`。
- 我们的 futex 是适配层的地址表 + WaitObject：唤醒以 generation 计。若
  **子线程在父线程首次为该地址创建表项之前**完成 `__wake`，该唤醒会被丢弃；
  父线程随后检查 `detach_state` 时若已看到 `DT_EXITED` 则不应等待，但当前
  观察到父线程仍进入等待（转储显示其在 `wait_objects` 阻塞）。
- 下一片：在 join 前确保 `detach_state` 的 futex 表项已建立（例如父线程在
  `pthread_create` 后先 `wait_object_create+observe` 该地址，或适配层在
  `clone` 时预建），并复核 `__timedwait_cp` 的 word 检查；若仍存在竞态，
  考虑让内核的 `SYS_WAIT` 在阻塞前重新检查用户 word（需要额外的用户读
  或者由适配层在 recheck 后禁止抢占）。

## 4. 边界

- pthread 的 join 尚无 QEMU 证据；`pthread_create` 已可成功。
- 线程退出路径依赖适配层的 `exit(60)` 识别；musl 的 clone.s 绕过适配层，
  因此 `clear_child_tid` 的 futex 唤醒不经过适配层（join 使用 detach_state，
  不受影响）。
- 补丁改变了 musl 快照构建（`target/musl-prefix` 重建），需在同一 PR 中
  保留 patch + build.sh 的戳更新。

## 5. 本报告不代表

- 不表示 pthread_create/join 可用；
- 不表示 M16 完成或阶段 5 退出条件满足。
