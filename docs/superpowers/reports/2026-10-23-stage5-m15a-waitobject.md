# 阶段 5 / M15a-1：WaitObject 无阻塞切片实现报告

**日期**：2026-10-23
**范围**：M15 的第一片。设计见
[M15 设计](../specs/2026-10-23-stage5-m15-process-thread-wait-design.md)；
COSIR §21 的 WaitObject 语义。
**结论**：`wait_object_create/observe/signal/destroy` 与 `wait_objects` 的
**立即路径**已在内核落地（42–46），host 状态机 3 个用例与 QEMU `W1` 双轮通过。
**阻塞路径（BlockedWait/waiters/超时唤醒）尚未实现**：需要阻塞时返回
`EAGAIN`；这是本切片的明确边界，下一片接线。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `prima-abi/src/lib.rs` | `SYS_WAIT_CREATE/OBSERVE/OBJECTS/SIGNAL/DESTROY` = 42–46（D2 工作决定） |
| `kernel/src/bootstrap.rs` | `ObjectRef::WaitObject`、`WaitObjectTable`（8 个）、`WAIT_RIGHTS`、`WaitError`/`WaitOutcome`、create/observe/signal/wait/destroy；审计 `ObjectCreated`/`ObjectDestroyed`/`CapabilityMinted` |
| `kernel/src/syscall.rs` | 5 个 syscall 胶水与 errno 映射；`WouldBlock → EAGAIN` |
| `prima-user/c/prima_abi.h`、`prima_syscall.c` | 原生号 42–46；`prima_native` 直通（避免与 Linux 42–46 冲突） |
| `prima-user/c/stage5_wait.c` | Prima 专用探针（create/observe/signal/satisfied/EAGAIN/timeout/invalid/destroy/ENOENT） |
| `xtask/src/image.rs`、`main.rs` | `Stage5Program::Wait`、`W1` 双轮 |

## 2. 语义（本切片）

- `wait_object_create`：分配对象、generation=0、根 cap
  `{Derive, WaitWait, WaitSignal, WaitDestroy}`；对象表满 `ENOSPC`。
- `observe`：纯查询；目标销毁后 `ENOENT`。
- `signal`：generation += 1（无等待者）。
- `wait_objects`：校验 cap/rights/两对象非重复/观测不超前；Any/All 立即
  满足返回 0；已观测最新且 `timeout <= now` 返回 1（超时）；否则 `EAGAIN`。
- `destroy`：删除对象；cap 变为目标失活，后续 observe/signal 返回 `ENOENT`。
- 错误：`EINVAL`（非法 cap/观测超前/未知模式）、`EACCES`（非持有者/缺权限）、
  `ENOENT`（目标已销毁）、`ENOSPC`（容量/审计满）、`EAGAIN`（需阻塞）。

## 3. 证据

### 3.1 host

```text
cargo test -p kernel wait_object  → 3 passed
cargo test --workspace            → 全绿
```

覆盖：create/observe/signal/destroy、立即满足、`WouldBlock`、立即超时、
超前观测、Any/All、重复对象、权限位。

### 3.2 QEMU

`W1` 双轮输出逐字节一致：

```text
WAIT|create=1;gen0=1;signal=1;satisfied=1;block=1;timeout=1;invalid=1;destroy=1;dead=1|ok=1;err=0
```

（`block=1` 表示等待返回 `EAGAIN`，即“需要阻塞”的确定路径。）

### 3.3 全量回归

```text
cargo xtask test
```

结果（83 条 `PASS` 行，退出码 0）：

```text
PASS qemu bios run=1 / run=2
PASS qemu bios E1..E6 / T1 / F1 / V1 / V2 / V3 / N1 / W1 / I1 / M1 / C1 / L1 / L2 / L3 / D1 / A1 run=1 / run=2
PASS linux stage5 C main / echo / time / fd / vfs / meta / cat-ls / mut / mem
PASS linux stage5 symlink/rename/fchdir / chmod/umask/stdio / loopback UDP / tty input references
PASS stage1 boot slice
```

`W1` 两轮 stdout、退出码与容量记录一致；既有轮次无回归。

## 4. 边界与后续

- **未实现**：`BlockedWait` 状态、waiters 集合、signal 唤醒、destroy
  唤醒 `WaitTargetDead`、超时唤醒、`alertable`/upcall（M18）。
- generation 递增目前没有独立审计事件（COSIR §21 要求）；下一片补
  `WaitSignaled` 或等价事件。
- `prima_native` 是测试/机制探针的直通；不改变 Linux 号段翻译。
- 下一片：TaskStatus::BlockedWait、TaskRecord.wait_request、waiters、
  唤醒结果注入、`expire_waits(now)`，并补 host 阻塞/唤醒/超时用例。

## 5. 复现命令

```sh
cargo test --workspace
cargo xtask test
```

## 6. 本报告不代表

- 不表示 WaitObject 的阻塞语义已完成；
- 不表示 Process/Thread、pthread 或 D3 已关闭；
- 不表示阶段 5 退出条件已满足。
