# 阶段 5 / M16a：futex WAIT/WAKE 子集实现报告

**日期**：2026-10-23
**范围**：M16 的第一片。futex 在**适配层**用用户态地址表 + 既有 WaitObject
实现（内核无新 syscall）。设计契约见
[M15 设计](../specs/2026-10-23-stage5-m15-process-thread-wait-design.md) 与
[COSIR §21](../../../COSIR%20原语规范.md)。
**结论**：Linux `futex(202)` 的 `WAIT`/`WAKE` 子集在 Prima 上工作：等待、唤醒
计数、值不匹配 `EAGAIN`、无等待者 `wake=0`；`W3` 双轮通过。pthread 的
`clone` 映射仍未实现（M16b）。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-user/c/prima_syscall.c` | futex 静态表（8 个地址槽）；`WAIT`：值检查 → 观察 generation → 重查 → `wait_objects`（超时换算 PIT ticks）；`WAKE`：`signal_object` 返回的被唤醒计数；Linux 号 202 翻译 |
| `prima-user/c/prima_abi.h` | `PRIMA_LINUX_SYS_futex=202`、WAIT/WAKE、`ETIMEDOUT` |
| `kernel/src/syscall.rs` | `signal_object` 返回值打包：低 32 位 generation、高 32 位被唤醒数（futex wake 需要） |
| `prima-user/c/stage5_futex.c` | W3 探针 |
| `xtask/src/image.rs`、`main.rs` | `Stage5Program::Futex`、`W3` 双轮 |

## 2. 语义

- `futex(uaddr, FUTEX_WAIT, val)`：
  - `*uaddr != val` → `EAGAIN`；
  - 取该地址的 WaitObject cap（同进程共享能力空间），观察 generation 后**重查**
    `*uaddr`，再 `wait_objects`；
  - 超时：`timespec` 相对时间换算为绝对 PIT tick 截止；`NULL` 表示无限；
    超时返回 `ETIMEDOUT`；
  - 被唤醒返回 0。
- `futex(uaddr, FUTEX_WAKE, n)`：无等待者返回 0；有则信号该对象的 cap，
  返回 `min(被唤醒数, n)`（内核把唤醒数放在信号返回值高 32 位）。
- `FUTEX_PRIVATE_FLAG` 按同一进程内语义处理；`CLOCK_REALTIME` 与 unsupported
  op 返回 `ENOSYS`/忽略（见边界）。
- 表满（8 个地址）→ `EAGAIN`；不新增内核对象/syscall，复用 WaitObject。

## 3. 证据

### 3.1 QEMU

`W3` 双轮输出逐字节一致：

```text
FUTEX|thread=1;wake=1;wait=1;eagain=1;nowake=1|ok=1;err=0
```

- `thread=1`：共享线程创建（复用 M15b 槽位回收）；
- `wake=1`：主线程 `FUTEX_WAKE` 返回 1；
- `wait=1`：被唤醒线程的 `FUTEX_WAIT` 返回 0；
- `eagain=1`：值已变为 1 时再 `WAIT(0)` 返回 `EAGAIN`；
- `nowake=1`：无等待者时 `WAKE` 返回 0。

### 3.2 全量回归

```text
cargo xtask test
```

结果（89 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 / T1 / F1 / V1 / V2 / V3 / N1 / W1 / W2 / W3 / I1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 全部参考
PASS stage1 boot slice
```

`W3` 两轮与既有轮次全部通过。

## 4. 边界

- 只实现 `WAIT`/`WAKE`；无 `REQUEUE`/`CMP_REQUEUE`/`WAIT_BITSET`/PI/robust。
- 表按进程映像静态分配（8 地址）；跨进程共享 futex 不支持（PRIVATE 语义）。
- `CLOCK_REALTIME` 未区分；generation 型等待在“wake 后 word 未变”的极端序列
  可能产生一次假唤醒（Linux futex 不保证无假唤醒；调用方需循环检查谓词）。
- pthread 的 `clone` 翻译（CLONE_VM/THREAD/SETTLS、parent/child tid、
  clear_child_tid join）未实现 → M16b。

## 5. 复现命令

```sh
cargo test --workspace
cargo xtask test
```

## 6. 本报告不代表

- 不表示 pthread_create/join 可用；
- 不表示阶段 5 退出条件已满足。
