---
id: report-p3-adapter-lock-split
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# P3-2：C 适配层数据面锁拆分与在途引用

## 结论与范围

本报告记录 P3-2 工作项 3 的第一段：C/musl 适配层的数据路径
（`read`/`write`/`readv`/`writev`/`pread64`/`pwrite64`）不再持有进程级 fd 锁跨越
同步 IPC；改为短临界区快照 fd 表、按 wire fd 的在途计数，以及最后一次引用再发送
延迟 `CLOSE`。控制面（`open`/`close`/`dup`/`stat`/路径与目录操作）仍由同一把
表锁保护，`close` 在无在途引用时仍内联发送 wire `CLOSE` 并保留错误返回；**这把
控制面锁的完全移除尚未完成，P3-2 不关闭**。v1/v2 数据面、append、共享游标与
errno 语义保持不变。

## 基线与输入

- 基线：`50500b9`（[P3-2 C 数据面 v2](2026-11-24-p3-frontend-v2-data-plane.md)）。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md) 的 P3-2
  工作项 3。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节的
  "在途 fd 与 close 完成点"。

## 动机

旧实现把整个"fast"系统调用体放在进程级 spinlock 内，包括阻塞的文件 IPC；一个
进程内的所有 fd 操作因此完全串行，锁还被 fork 后的子进程继承。会话本身按 Task
独立（`prima_session_current()`），数据面并不需要进程级串行——需要保护的是进程
共享的本地 fd 表与 wire 句柄表。

## 实现

- `prima_fd_lock_take`/`prima_fd_lock_give`：现有 spinlock 的短临界区入口。
- `prima_fd_snapshot`：在表锁内复制 `kind`/`object`；普通文件同时递增
  `prima_file_inflight[object]`，返回后即可在锁外做 IPC。
- `prima_file_unref`：表锁内递减在途计数；若计数归零且该句柄已标记
  `prima_file_retire`，在锁外发送 `PRIMA_FILE_OP_CLOSE`。
- `prima_fd_drop`：本地 fd 立即释放；最后一个本地引用为 0 时，若有在途批则只置
  `retire`（延迟 `CLOSE`），否则维持原有的内联发送并返回服务端状态。
- `prima_close_all_fds`（exit/exec）：在表锁内遍历；进程结束时由 owner close
  兜底释放仍未发送的 wire `CLOSE`。
- `prima_translate`：`read`/`readv`/`write`/`writev` 与 `pread64`/`pwrite64`
  走无外层锁的 split 路径；其余控制面操作仍取表锁。信号屏蔽保持不变。
- `readv`/`writev` 仍按向量逐次快照：跨向量的并发 `close` 语义未定义（与 POSIX
  不承诺多线程 `readv` 原子性一致）。
- host 可执行检查：`test_adapter.c` 断言快照确实持引、`close` 在在途时只置
  `retire` 且不执行 IPC。

## 复核整改（2026-11-25）

对两个 C 提交做了一次独立只读复核，按发现整改：

- **多线程 fork 继承控制页映射**：fork 前关闭调用者会话后，若同进程其它 Task
  仍持有 v1/v2 会话，返回 `EAGAIN`（与 tty/net 活动时的 fork 限制一致），避免
  子进程继承他人控制页映射、使父线程的段无法销毁。
- **doorbell 后接收失败**：会话被标记为 desync，之后的数据面调用返回 `EIO`
  直到关闭；不把丢失的完成当成新批。
- **重置/`ESTALE` 恢复**：doorbell 的 `EACCES`/`ESTALE` 回复会解除本地映射并
  丢弃能力；下一次调用重新 `OPEN`（Store 的 OPEN 会回收重置会话）。重置会话的
  `CLOSE` 回复按幂等成功处理，fork/exec 不再被卡死。
- **`pread`/`pwrite` 校验泄漏**：快照后的偏移/长度/缓冲校验失败路径补上在途
  引用释放，避免延迟 `CLOSE` 永不发送。
- **`OPEN` 本地校验失败**：解除映射后补发尽力而为的 `ASYNC_CLOSE`，不留活动
  会话导致后续 `OPEN` 永久 `EAGAIN`。
- host 断言新增：非法 `pread` 不遗留在途引用；延迟关闭只在最后一个引用触发。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 本次 | PASS；1181 passed、0 failed、1 ignored（Rust 未改动） | host |
| `python3 prima-user/musl/test_port.py` | 本次 | PASS；7 tests（含锁拆分断言） | host（C 适配层） |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy、裸机构建、44 文件零 unsafe | host/静态 |
| `cargo xtask test-vfs` | 本次 | PASS；V1/V2/V3/V4 各 2 轮 | QEMU（v1+v2 并存） |
| `cargo xtask test-process` | 本次 | PASS；W4/W5/W6/W7/W7b/W8/W9 各 2 轮 | QEMU（fork/线程/信号） |
| `cargo xtask test` | 本次 | PASS；`full BIOS suite: 36 programs, 72 runs`、`stage1 boot slice` | QEMU（完整套件） |

## 失败与限制

- **控制面锁未完全移除**：`open`/`close`/`dup`/`stat`/路径与目录操作仍在表锁内
  跨 IPC；工作项 3 的完整验收还缺这一段。
- **多线程 fork 限制**：任一其它线程持有 v1/v2 会话时 fork 返回 `EAGAIN`；这是
  为不继承他人控制页映射而加的 profile 限制。
- **丢失完成不可恢复**：doorbell 之后接收失败会把会话毒化为 `EIO`，只能关闭；
  未实现自动重放或去重（属 P6 范围）。
- 并发 `close` 与在途 I/O 交错时，`close` 立即返回 0；服务端 `CLOSE` 的状态由
  最后一次 unref 忽略。该交错的错误可见性弱于单线程路径，属本次拆分的已知代价。
- `readv`/`writev` 跨向量的 fd 复用不保证原子。
- 无 SMP、恢复/去重、性能门槛；未改变 v1 客户端、`PRIMA_BULK_PROBE` 或既有
  72 轮事件。

## 阶段/profile 影响

不关闭 P3-2 或 P3，不改变 P2 状态、阶段 7 九轨勾选或 7-A；不改变 profile。

## 后继关系

- 下一段：移除控制面的跨 IPC 表锁（`open`/`close` 的 wire 发送移到锁外，路径与
  cwd 状态另设短锁），保持 errno 与目录语义。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md)。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节。
- 前序：[P3-2 C 数据面 v2](2026-11-24-p3-frontend-v2-data-plane.md)。
