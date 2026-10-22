# 阶段 5 / M16b-3：pthread 创建/join 打通与 clear_child_tid

**日期**：2026-10-23
**范围**：修复 pthread join，完成 `pthread_create`/`pthread_join` 的
Linux/Prima 同源证据。
**结论**：`W4`（pthread 创建/join）与 `W5`（跨线程共享内存可见性）双轮通过；
`pthread_create`、`pthread_join`、返回值与共享写入全部正确。根因是
**`CLONE_CHILD_CLEARTID` 未实现**：musl 依赖内核在 clone 时把 tid 写入
`child_tid`、在线程退出时清零并 futex 唤醒 `__thread_list_lock`；此前
join 的 `__tl_sync` 在该锁上永久等待，表现为“join 阻塞/无 Ready 任务”。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-abi/src/lib.rs` | `SYS_THREAD_CLEAR_TID=50` |
| `kernel/src/bootstrap.rs` | `TaskRecord.clear_child_tid` 与 set/get |
| `kernel/src/syscall.rs` | `SYS_CLONE` 写 `child_tid` 并记录；`SYS_THREAD_CLEAR_TID` 返回当前线程地址 |
| `prima-user/c/prima_syscall.c` | 线程退出（clone 表路径与 exit(60) 回退）先读 child_tid、写 0、futex 唤醒，再 `SYS_THREAD_EXIT` |
| `prima-user/c/stage5_pthread.c`、`stage5_share.c` | W4/W5 探针 |
| `xtask/src/image.rs`、`main.rs` | `Pthread`/`Share` 程序、Linux pthread 参考、`W4`/`W5` 双轮 |

## 2. 根因与语义

musl 的 clone 以 `&__thread_list_lock` 作为 `child_tid`：
- 内核在 clone 时把子 tid 写入该地址（锁被“占用”），线程退出时清零并
  futex 唤醒，线程列表锁因此释放；
- 此前我们只写 `parent_tid`、忽略 `child_tid`，退出也不清零/唤醒 →
  `pthread_join` 的 `__tl_sync` 在 `__thread_list_lock` 上永久等待，
  而退出线程已 Dead，内核 `SYS_WAIT` 无 Ready 后继 → panic。
- 现在：`SYS_CLONE` 记录并写入 `child_tid`；适配层在退出路径调用
  `SYS_THREAD_CLEAR_TID` 取地址、写 0、用既有 futex 表唤醒，再退出线程。

## 3. 证据

### 3.1 Linux 参考

```text
PTHREAD|create=1;join=1;ret=1;value=1;self=1|ok=1;err=0
```

### 3.2 QEMU

`W4` 双轮与 Linux 输出逐字节一致：

```text
PTHREAD|create=1;join=1;ret=1;value=1;self=1|ok=1;err=0
```

`W5`（共享内存可见性最小探针）双轮：

```text
SHARE|create=1;thread=1;wake=1;word=1;destroy=1|ok=1;err=0
```

`W5` 证明子线程写入对父线程可见（word=0x5a），排除了此前怀疑的共享内存
可见性问题；join 的根因确为线程列表锁的 child_tid 释放缺失。`self` 字段
移到 join 之后（join 保证子线程已运行），避免 Linux 上“子线程尚未调度”
导致的竞态。

### 3.3 全量回归

```text
cargo xtask test
```

结果（96 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 / T1 / F1 / V1 / V2 / V3 / N1 / W1 / W2 / W3 / W4 / W5 / I1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 pthread references
PASS linux stage5 其余参考
PASS stage1 boot slice
```

`W4`/`W5` 两轮与 Linux pthread 参考全部通过。

## 4. 边界

- 只覆盖 pthread 创建/join 的基本路径；分离线程、取消、TLS 析构、调度属性
  未验证。
- `clear_child_tid` 的清除/唤醒由适配层在 `exit(60)` 路径执行；若线程以
  其它方式退出（例如直接 `SYS_THREAD_EXIT`），需调用者自行处理。
- futex 子集与 M16a 相同（无 PI/robust/requeue）。

## 5. 问题关闭

`ISSUE-PTHREAD-CLONE-001` 关闭：musl clone 路由、pthread_create、pthread_join
与共享内存可见性均有 QEMU 双轮与 Linux 参考证据。

## 6. 本报告不代表

- 不表示 pthread 全量语义（取消/分离/属性）完成；
- 不表示阶段 5 退出条件满足。
