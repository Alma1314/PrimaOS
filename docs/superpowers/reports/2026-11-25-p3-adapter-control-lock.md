---
id: report-p3-adapter-control-lock
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# P3-2：C 适配层控制面锁拆分（工作项 3 完成）

## 结论与范围

本报告记录 P3-2 工作项 3 的剩余部分：C/musl 适配层的**控制面**不再持有进程级
fd 表锁跨越同步 IPC。原实现把整个“fast”系统调用体（`open`/`close`/`dup`/
`fcntl`/`stat`/路径与目录变更/`termios`/socket 控制）放在同一把 spinlock 内，
包括阻塞的文件、终端与网络往返；一个线程等待服务端时同进程其它线程的 fd 操作
全部串行。现在每次操作只锁住本地表操作，IPC 与用户内存访问都在锁外，进程级
状态（fd 表、wire 引用、cwd、目录缓存、umask、termios）仍由同一把短锁保护。

数据面（`read`/`write`/`readv`/`writev`/`pread`/`pwrite`）的短锁快照与延迟
`CLOSE` 保持[前一段](2026-11-24-p3-adapter-lock-split.md)的语义。v1/v2 数据面、
append、共享游标与 errno 语义不变；既有 QEMU 事件未改写。工作项 3 至此完整，
P3-2 的关闭仍需本报告命令全部通过后由 P3-3 矩阵与收口确认。

## 基线与输入

- 基线：`5583cfc`（工作树在改动前干净）。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md) 的 P3-2
  工作项 3。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节的
  “在途 fd 与 close 完成点”。
- 前序：[P3-2 C 适配层数据面锁拆分](2026-11-24-p3-adapter-lock-split.md)。

## 实现

- **统一在途机制**：`prima_fd_snapshot` 的 in-flight 计数从普通文件扩展到目录；
  新增 `prima_net_inflight`/`prima_net_retire` 供 socket 操作使用。
  `prima_fd_drop_locked`（调用者持锁）只移除表项并输出应发送的 wire `CLOSE`；
  `file`/`net` 的最后一个 `unref` 负责延迟发送。`prima_wire_close_call` 在锁外
  发送 `CLOSE`，文件保留原错误返回，socket 保持既有的尽力而为（返回 0）。
- **路径解析**：`prima_normalize_path`/`prima_absolute_path`/`prima_path_target`
  改为“调用者持锁”的本地函数；`prima_path_target` 把待发送路径复制到调用方
  缓冲并返回长度，IPC 不再依赖锁或用户指针的稳定。
- **控制面分段**：`open/openat`（解析 → OPEN IPC → 锁内安装 → 锁外可选 chmod）、
  `mkdir/unlink/rmdir`、`stat/lstat/fstat/fstatat/statx`、`chmod/fchmod`、
  `chdir/getcwd/fchdir`、`rename/symlink/readlink`、`umask`、`close`、
  `dup/dup2/dup3/fcntl`、`lseek/ftruncate`、`getdents64`、`ioctl`、
  `socket/bind/getsockname/sendto/recvfrom` 全部按“短锁本地段 + 锁外 IPC”重排。
- **目录缓存**：`prima_dir_fill` 每次只在锁内压缩/合并缓存，服务端读在锁外；
  `prima_dir_take`/`prima_dir_unread` 的缓存访问是短锁段。
- **fork/exec**：`fork` 从 `fast`（持锁）分支移到 `split`（只屏蔽信号）分支；
  表锁无法让多线程的地址空间复制原子，持锁只会阻塞同进程的其它线程。子进程
  分支与 `prima_exec_restore` 清零继承的 in-flight/retire 计数，避免新 Task 的
  `CLOSE` 被永久延迟。既有“其它线程持有 v1/v2 会话时 fork 返回 `EAGAIN`”限制
  保持不变。
- host 断言新增：目录快照在途计数、socket 在途与延迟 `CLOSE`、`dup_to` 输出
  被替换目标的 wire `CLOSE`、路径目标复制（绝对路径与 dirfd 相对）。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 本次 | PASS；1181 passed、0 failed、1 ignored | host |
| `python3 prima-user/musl/test_port.py` | 本次 | PASS；7 tests（含新增控制面断言） | host（C 适配层） |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy、裸机构建、44 文件零 unsafe | host/静态 |
| `cargo xtask test-vfs` | 本次 | PASS；V1/V2/V3/V4 各 2 轮，Linux 差分对比通过 | QEMU（v1+v2 并存） |
| `cargo xtask test-process` | 本次 | PASS；W4/W5/W6/W7/W7b/W8/W9 各 2 轮 | QEMU（fork/线程/信号） |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -q -p xtask --features linux-probe -- test-linux-proxy` | 本次 | PASS；`S7PROXY abi=2 boots=2 linux=cat,fork,shared,limit native=/README proxy=1 shared_store=1 negatives=7` | QEMU（代理矩阵） |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -q -p xtask --features linux-probe -- test` | 本次 | PASS；`full BIOS suite: 36 programs, 72 runs`、`stage1 boot slice` | QEMU（完整套件） |

## 失败与限制

- **并发控制面的弱化语义**：原有表锁让同进程的所有控制面操作串行。拆分后，
  并发 `close` 与使用同一目录的 `getdents` 可能让缓存被重置（返回截断/重试），
  并发 `chdir`/`umask`/`tcsetattr` 的最终值取决于最后写者，`mkdir` 与其后
  `chmod` 之间存在窗口。这些交错的顺序 POSIX 本身不保证；单线程语义与 errno
  保持。
- **锁内用户内存**：`openat`/`chmod` 等在锁内复制或合成用户路径（与原实现一致），
  缺页会在临界区内发生；未把用户指针访问挪到锁外。
- **fork**：多线程 fork 的 fd 表快照不可原子；既有 `EAGAIN` 限制保留。子进程
  只清自己的在途计数，wire 句柄泄漏由服务端 owner close 兜底。
- **目录回退**：`getdents` 缓存与 `close` 竞争时的 `unread` 可能被缓存重置抵消，
  不保证并发语义。
- 无 SMP、恢复/去重、性能门槛；未改变 v1 客户端、`PRIMA_BULK_PROBE` 或既有
  72 轮事件；未新增 syscall。

## 阶段/profile 影响

工作项 3 完成，P3-2 在 P3-3 矩阵取得机器证据后可关闭；本报告不勾选 7-A、不改变
P2 状态、阶段 7 九轨或 profile。

## 后继关系

- 下一片：P3-3 QEMU 正反例矩阵（队列/EAGAIN/取消/唤醒/公平/恶意客户端）。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md)。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节。
- 前序：[P3-2 C 适配层数据面锁拆分](2026-11-24-p3-adapter-lock-split.md)。
