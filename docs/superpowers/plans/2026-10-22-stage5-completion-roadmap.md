# 阶段 5 收尾与阶段 6 入口实施路线图

**日期**：2026-10-22
**性质**：执行计划。计划本身不代表实现完成，也不勾选任何阶段条件。
**基线**：`de6e064`（M11，工作树干净）
**上位文档**：[`Prima OS 实现阶段.md`](../../../Prima%20OS%20实现阶段.md)（当前状态与退出条件）、
[`Prima OS.md`](../../../Prima%20OS.md) §8.2/§15（阶段定义）、
[`COSIR 原语规范.md`](../../../COSIR%20原语规范.md)（操作语义）。
**当前契约**：[profile v0](../specs/2026-10-18-stage5-profile-v0.md) 已冻结但 S5-D1 未获组织级确认；
[profile v1 delta](../specs/2026-10-20-stage5-profile-v1.md) 仍是提案。

## 1. 目的与边界

本路线图把 M11 之后的阶段 5 余项与阶段 6 入口拆成有依赖顺序的里程碑，回答三个问题：

1. 阶段 5 还差什么，按什么顺序做，怎样才算完成；
2. 哪些事项必须先在 COSIR/profile/阶段定义层裁决，不能直接写代码；
3. 阶段 6 的入口条件、首批工作与证据边界是什么。

**本文件不承诺**：

- 不表示 M12–M21 或 S6-A–S6-E 已开始或已完成；
- 不改变 profile v0 的冻结边界，不自行发布 profile v1/v2；
- 不关闭 BLK-003，不预先分配 42 之后的具体 syscall 号；
- 不把 DOC-002《基础库栈技术规格 v1.1》中未评审的号段、命名、时延目标带入 ABI；
- 不把阶段 7 的候选工作（SMP、NIC/IOMMU、动态链接、配额、第二架构）算作阶段 5/6 交付。

状态判断始终以 [`Prima OS 实现阶段.md`](../../../Prima%20OS%20实现阶段.md)、冻结 profile 和最新报告为准；本文件只负责计划。

## 2. 现状基线

### 2.1 已完成的阶段 5 切片

| 里程碑 | 主题 | 主要证据 | 已覆盖的 profile 行 | 明确未覆盖 |
|---|---|---|---|---|
| M0 | musl 1.2.5 依赖盘点 | [M0 报告](../reports/2026-10-18-stage5-musl-m0-inventory.md) | P5-LIB-01 符号盘点 | 符号清单缺口、TLS 假设 |
| M1 | 固定 C 入口栈与返回码 | [初始栈](../reports/2026-10-18-stage5-c-start-stack.md)、[返回码](../reports/2026-10-18-stage5-main-return-status.md) | P5-BOOT-01、P5-PROC-01（partial） | 任意 argv、完整 auxv |
| M2 | C 系统边界与 per-task FS base | [M2 报告](../reports/2026-10-18-stage5-musl-m2-boundary.md) | P5-ABI-01 的前置 | 通用 fd/进程语义 |
| M3/M3b | 静态 musl 基座、SSE/FPU、匿名内存 | [M3 报告](../reports/2026-10-18-stage5-musl-m3-base.md) | P5-LIB-01（部分） | 通用 `mmap`、线程 |
| M4 | 单文件 fd 生命周期 | [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) | P5-IO-01/02、P5-FD-01、P5-FS-01/02（partial） | 目录、多级路径、退出自动关闭 |
| M5 | sbase `echo` E1–E6 | [M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md) | P5-LIB-01（supported 子集）、S5-X3 证据 | 任意 argv/auxv、更多开源程序 |
| M6 | POSIX 时间首切片 | [M6 报告](../reports/2026-10-19-stage5-m6-time.md) | P5-TIME-01（partial） | 墙上时钟、`EINTR`、阻塞睡眠 |
| M7 | 本地 fd 表、`dup`、`getpid` | [M7 报告](../reports/2026-10-19-stage5-m7-fd-table.md) | P5-FD-01、P5-PROC-02（partial） | 进程模型、`CLOEXEC` 语义 |
| M8 | 用户态 VFS/rootfs、多级路径 | [M8 报告](../reports/2026-10-20-stage5-m8-user-vfs.md) | P5-FS-01/03、P5-FD-01（partial） | 符号链接、`rename`、持久化 |
| M9 | `statx`、`chdir`/`getcwd`、sbase `cat`/`ls` | [M9 报告](../reports/2026-10-21-stage5-m9-metadata-cwd.md) | P5-FS-04/05（partial） | 时间戳、`st_dev`、`fchdir` |
| M10 | 匿名内存/保护 | [M10 报告](../reports/2026-10-21-stage5-m10-anon-memory.md) | P5-MM-01（partial） | 文件映射、`MAP_SHARED`、NX |
| M11 | `mkdir`/`rmdir`/`unlink` | [M11 报告](../reports/2026-10-22-stage5-m11-file-mutation.md) | P5-FS-06（partial） | `rename`、链接、权限、持久化 |

### 2.2 代码与证据层面的实质缺口

以下结论来自仓库代码与现有报告，不是对未实现功能的推测：

- **进程/线程**：内核只有 `Task`，没有 Process/Thread/ShareGroup 对象；没有 `fork/execve/wait4`，
  没有可持久化的退出状态，也没有父子等待。C 任务退出由服务管理器观察，fd/服务端句柄的
  自动关闭仍不完整。
- **线程/TLS/futex**：没有 `WaitObject`/futex 内核机制，没有 pthread；per-task FS base 只服务
  musl 单线程 TLS。`P5-THREAD-01` 仍是 `unsupported`。
- **信号**：只有异常订阅/`SYS_EXC_WAIT`/`SYS_EXC_RESUME` 的机制面，没有 POSIX signal 用户面、
  `sigaction`/`kill`/`sigreturn`，也没有 `EINTR`。
- **tty/console**：fd 1/2 的输出字节流已通，但没有 fd 0 的 tty 输入、行规程、`termios` 子集。
- **文件系统**：用户态 VFS 只有单路径变更；没有 `rename`、硬链接/符号链接、`fchdir`、
  权限/`umask`/`chmod`、内容区回收、跨启动持久化；stdio 文件 API 未验收。
- **内存**：匿名 `MAP_PRIVATE` 的 56 页 arena 是 partial；没有文件映射、`MAP_SHARED`、
  `mremap`/`madvise`、硬件 NX，`PROT_NONE` 只是近似只读。
- **C 入口**：只有固定探针形态；任意 `argc/argv/envp` 与完整 auxv 未实现。
- **网络**：没有 socket API，也没有用户态网络服务。loopback IPC 不能当作网络。
- **Rust 目标**：没有 `core`/`std` 的 Prima 目标与支持矩阵。
- **阶段 6**：没有 Linux 静态 ELF 装载器、Linux 初始栈、Linux persona 或 Linux syscall 递送；
  内核也没有为“OS 中立 trap/exception 递送”形成可执行验证。

### 2.3 当前容量基线

路线图中的每个里程碑都必须记录这些值的变化，不能静默放宽：

| 项 | 当前值 | 位置/含义 |
|---|---:|---|
| `MAX_TASKS` | 18 | 内核任务槽 |
| `MAX_CAPABILITY_SLOTS` | 160 | Capability 槽 |
| `MAX_AUDIT_EVENTS` | 8192 | 审计环形容量（M12b 清理查询后实测 `audit_used=1322`） |
| `SHARED_RECORDS` | 16 | 共享 trace 记录数（M12b 15→16，清理记录置末） |
| `MAX_SEGMENT_FRAMES` | 149 | 单 Segment 帧上限（M12b 实测扩容：管理器 26 页，log/ns/store 各 21 页，`ls` 映像合计 149 页） |
| `MAX_VSPACE_ENTRIES` | 384 | 逻辑 vspace 条目 |
| `MAX_MESSAGES` | 32 | IPC 队列 |
| `MAX_TIMERS` | 8 | Timer 对象 |
| `MAX_NODES`（VFS） | 32 | 用户态 rootfs 节点（M13a 16→32，内容区 3048 B） |
| `MAX_NAME`/`MAX_PATH`/`MAX_COMPONENTS` | 16/64/6 | 用户态 VFS 路径边界（M13a 未变） |
| 用户态 crate `opt-level` | 1 | `prima-user`/`prima-runtime`（M13a 容量缓解；内核不变） |
| `SERVICE_FILE_HANDLES` | 8 | 存储服务并发句柄 |

引入 Process/Thread、pthread、signal、network 后，Task/Cap/vspace/audit 的压力会显著上升。
每个里程碑先测量再扩容；扩容必须说明理由、更新报告并重跑既有 QEMU 轮次。

## 3. 决策门

这些决策不由实现者默认选择。未关闭时可以做探索原型，但不得升级 profile 状态或勾选阶段条件。

| ID | 决策 | 选项 | 建议 | 阻塞 |
|---|---|---|---|---|
| D1 | profile v1 是否按 M8–M11 delta 发布 | 发布 v1 / 退回逐行修订 / 保持 v0 | 组织级评审通过后发布 v1；v0 其余行不变 | M13/M14 的状态升级 |
| D2 | BLK-003 收口：号段、命名、错误码、入口 | 保留 1–41 只追加 / 其他 | 保留 1–41，从 42 起追加；Prima 命名；错误码只登记已实现值 | M15/M17/M18 的新 syscall |
| D3 | Process/Thread 对象模型与 `fork/execve/wait4` 归属 | 阶段 5 内 / 阶段 6 / 仅设计不实现 | 先在 COSIR 建立 Process/Thread/ShareGroup/WaitObject 语义，再决定实现阶段；不默认承诺 | M15/M16/M17 |
| D4 | 动态链接归属 | 阶段 5 P0 / 阶段 7 | 维持白皮书阶段 7；阶段 5 只做静态链接 | M20/M21 |
| D5 | “基础网络子集”范围 | loopback IP/UDP / NIC / 仅 IPC | 阶段 5 用用户态 loopback IPv4/UDP 服务；NIC/DNS/TCP 留阶段 7 | M19 |
| D6 | Rust `core`/`std` 支持范围 | 最小 target / 全量 std | 冻结最小 target 与 `core`/`alloc`/`std` 子集 | M20 |
| D7 | signal 最小集合 | 最小 POSIX 子集 / 完整 signal | `sigaction`/`kill`/`sigreturn`/`SIGSEGV`/`SIGILL`/`SIGCHLD`/`EINTR` | M18 |
| D8 | VFS 权限与持久化 | 权限+链接 / 跨启动持久化 | 阶段 5 做权限、`umask`、链接；持久化列阶段 7 候选 | M13 |
| D9 | 性能目标 | 固定环境实测 / 沿用 DOC-002 | 只按 `(QEMU, guest CPU, host CPU)` 记 `PROVISIONAL`，不阻断语义切片 | M20/M21 |
| D10 | 阶段 6 Linux persona 位置 | 内核内 / 用户态 LibOS | 用户态 persona；内核只做 OS 中立 trap/exception 递送 | S6-B/S6-C |

**决策记录方式**：关闭 D1–D10 时，新增/更新对应 spec 或决策报告，并在
[`Prima OS 实现阶段.md`](../../../Prima%20OS%20实现阶段.md) 的决策日志登记；本路线图只保留
“建议/阻塞/关闭证据”的指针。

## 4. 里程碑总表

工作量级是相对量级（S/M/L/XL），不是工期承诺。

| ID | 主题 | 前置 | 主要交付 | 主要 profile 行 | 证据门槛 | 量级 |
|---|---|---|---|---|---|---|
| M12 | C 启动、退出与 fd 清理基座 | M11 | 任意 argv/envp、冻结 auxv、退出状态、fd/cap/segment 清理 | P5-BOOT-01、P5-PROC-01、P5-FD-01 | host + QEMU 双轮 + 同源差分 | M |
| M13 | VFS 完成 | M12、D8 | `rename`、链接/符号链接、`fchdir`、权限、内容回收、stdio 文件 API | P5-FS-01/04/05/06、P5-LIB-01 | 同源差分 + 负例 + 槽位压力 | L |
| M14 | tty 输入与最小 console | M12 | fd0 `read`/`readv`、canonical/raw、`termios` 子集、EOF | P5-IO-02、P5-FD-01、P5-BOOT-01 | QEMU 串口输入 + 同源差分 | M |
| M15 | Process/Thread 模型与 WaitObject/futex | M12、D3 | Process/Thread/ShareGroup、等待队列、线程原语 | P5-PROC-01/02、P5-THREAD-01 | host 状态机 + QEMU + 容量 | XL |
| M16 | pthread/TLS/mutex/cond | M15、D3 | `pthread_create/join`、mutex/cond、TLS、futex 映射 | P5-THREAD-01 | 同源 pthread 探针 + 双轮 | L |
| M17 | `fork/execve/wait4` | M12/M13/M15、D3 | 单线程 fork、静态 ELF exec、wait、fd 继承/CLOEXEC | P5-PROC-01/02、P5-FD-01 | 同源进程探针 + 负例 | XL |
| M18 | POSIX signal 最小集 | M15/M16、D7 | `sigaction`/`kill`/`sigreturn`、异常→signal、`EINTR` | 新增 P5-SIG-01（v2 提案） | 同源 signal 探针 + 双轮 | L |
| M19 | 用户态网络子集 | M12/M13、D5 | loopback IPv4/UDP、socket 子集 | P5-NET-01 | 同源 UDP 探针 + 错误路径 | L |
| M20 | Rust core/std 与兼容矩阵 | M13–M19、D6 | target、std 子集、profile v2、逐 API 矩阵、符号追溯、兼容护栏 | S5-D4/D5/D6/D7 | Rust/差分/兼容矩阵 | XL |
| M21 | 阶段 5 出口评审 | M12–M20 | 组织级复核与签署建议 | S5-D1–D7、S5-X1–X3 | 全量重复运行 + 报告审计 | S |
| S6-A | Linux 静态 ELF 装载器与初始栈 | M21 | ET_EXEC/ET_DYN 静态装载、完整 auxv/argv/envp | S6-D1 | 未修改 ELF 双跑 | L |
| S6-B | OS 中立 trap/exception 递送与 Linux persona | S6-A、D10 | trap 递送路径、用户态 Linux persona | S6-D2 | QEMU trace + 内核无 Linux 语义检查 | L |
| S6-C | Linux LibOS syscall profile | S6-B | fd/VFS、mmap、futex/TLS、signal、process 映射 | S6-D3、S6-D4 | Linux/Prima 差分 | XL |
| S6-D | 未修改静态 ELF 白名单与测试镜像 | S6-C | 白名单、哈希、测试镜像、差分矩阵 | S6-D5、S6-X1、S6-X2、S6-X4 | 白名单程序双跑 + 未支持 syscall 审计 | L |
| S6-E | 阶段 6 出口评审 | S6-D | 出口证据审计与签署 | S6-X1–X4 | 全量证据 + 内核纯净性复核 | S |

### 4.1 依赖关系

```text
M12 ─┬─> M13 ───────────────────────────────┐
     ├─> M14 ───────────────────────────────┤
     ├─> M15 ─> M16 ─> M18 ─────────────────┤
     ├─> M15 ─> M17 ────────────────────────┤
     └─> M19 ───────────────────────────────┤
                                            v
                                   M20 ─> M21 ─> S6-A ─> S6-B ─> S6-C ─> S6-D ─> S6-E
```

- **关键路径**：`M12 → M15 → M16 → M18 → M20 → M21`（进程/线程与 signal 最重）。
- **可并行**：M13（VFS）、M14（tty 输入）、M19（网络）在 M12 后与 M15/M16 并行；
  但每个里程碑仍需独立 spec/报告/QEMU 轮次，不能用一个探针代替。
- **串行约束**：M16 依赖 M15；M17 依赖 M15 的进程/地址空间模型；M18 依赖 M16 的每线程上下文；
  M20 依赖前面所有 profile 行稳定；S6-B 之后才能做 S6-C/D。

## 5. 里程碑详细条目

每个条目统一要求：设计 spec（涉及新对象/ABI 时）→ 实现 → host 单测 → QEMU 双轮 →
Linux/Prima 同源差分（适用时）→ 容量/审计登记 → 报告与文档更新。下面只列出各里程碑的特有内容。

### M12：C 启动、退出与 fd 清理基座

**目标**：把固定探针形态推进为通用 C 入口，并让退出成为可观察、可清理、可重复的路径。

**范围**：

- 任意 `argc/argv/envp` 注入；冻结 auxv 字段（至少 `AT_PAGESZ`、`AT_PHDR`、`AT_PHENT`、
  `AT_PHNUM`、`AT_ENTRY`、`AT_BASE`、`AT_NULL`；是否加入 `AT_RANDOM`/`AT_EXECFN` 由 spec 决定）。
- 退出状态在 Task 自毁后仍可被父/服务管理器观察；退出路径自动关闭本地 fd 与服务端句柄，
  归还 cap/segment；错误路径不得泄漏。
- `getppid` 的最小子集（没有 Process 模型时只做稳定父标识，或明确留到 M15）。
- `O_CLOEXEC` 只做“可记录”的准备，不承诺 exec 语义。

**ABI/对象**：默认优先用户态退出记录（服务管理器/进程服务）；确需内核字段时，按 D2 从 42 起
追加，禁止复用空洞。不在本里程碑引入 Process/Thread 对象。

**测试**：argv/envp 矩阵（0/1/多参数、超长、空串）；auxv 逐字段；退出码 0/非 0/信号终止的
可观察行为（信号终止在 M18 前只做占位拒绝）；fd 泄漏探针；容量水位。

**profile**：P5-BOOT-01 从 `partial` 推进；P5-PROC-01 保持 `partial` 直到 M17/M21 复核；
P5-FD-01 的“退出自动关闭”子项推进。

**退出条件**：任意 argv 的同源探针在 Linux/Prima 逐字节一致；退出后无残留 fd/cap/segment；
报告记录容量变化。

### M13：VFS 完成

**目标**：补齐阶段 5 profile 内的路径/目录/文件 API，不把用户态 rootfs 冒充通用文件系统。

**范围**：

- `rename`/`renameat`（同一 VFS 内原子替换；跨树/不存在/非空目录等错误集先冻结）。
- `link`/`linkat`、`symlink`/`symlinkat`/`readlink`/`readlinkat`（是否全做由 D8 决定；
  若不做，profile 明确 partial/unsupported 与理由）。
- `fchdir`、`chmod`/`fchmod`/`umask` 的最小权限模型；`/tmp` 可写边界。
- 删除后的内容区回收；节点/路径容量提升（`MAX_NODES`、`MAX_NAME`、`MAX_PATH`、
  `MAX_COMPONENTS` 的取值由实测决定）。
- stdio 文件 API：`fopen`/`fread`/`fwrite`/`fclose` 的冻结子集，经 musl 实际执行而非只链接。

**非目标**：跨启动持久化、通用磁盘文件系统、`statfs`、并发写锁、`O_TMPFILE`、`mmap` 文件映射。

**ABI/对象**：预计不需要新 syscall；扩展 `prima-runtime` 协议 tag、用户态 VFS 与存储服务。
若新增协议 tag，必须登记在 `protocol.rs` 并有 host 单测。

**测试**：同源 `stage5_vfs2.c`（或等价探针）覆盖成功/失败/边界；Linux 参考用一次性目录；
删除打开句柄、非空目录、只读父目录、符号链接环（若实现）等负例；节点/内容槽压力。

**profile**：P5-FS-01/04/05/06、P5-LIB-01 的相应子项；只有证据齐全才改 `partial`/`supported`。

**退出条件**：冻结 API 行有同源差分；未实现项在 profile v2 中明确标 partial/unsupported；
VFS 容量与错误码表更新；既有 M8/M9/M11 轮次不回归。

### M14：tty 输入与最小 console

**目标**：让 `read(0)` 成为可用输入路径，同时保留现有 fd 1/2 输出字节流。

**范围**：

- fd 0 的 `read`/`readv`、阻塞语义、EOF；tty 服务输入队列；QEMU 串口输入注入。
- canonical/raw 的最小行规程：行缓冲、退格/删除、回显、`Ctrl-D` EOF；raw 模式直通。
- `termios`/`ioctl` 子集：`TCGETS`/`TCSETS`/`TIOCGWINSZ`；其余 `ioctl` 明确失败。
- 不做 job control；`Ctrl-C`/`SIGINT` 留给 M18；不做 pty。

**ABI/对象**：优先复用 `SYS_DEV_IOPORT_IN` 与现有 tty IPC；若需要新的等待/唤醒，复用
M15 的 WaitObject，或先用现有阻塞 `SYS_RECV`。不新增 Linux 语义到内核。

**测试**：xtask 串口输入辅助；`cat` 从 stdin 读、行编辑、EOF、raw 模式、非法 `termios`；
Linux/Prima 同源探针（Linux 侧用 pty/管道，Prima 侧用 tty 服务；差异在报告中登记）。

**profile**：P5-IO-02、P5-FD-01 的 fd0 项、P5-BOOT-01 的初始 fd 描述。

**退出条件**：输入探针双轮 PASS；无输入时不忙等/不破坏输出；termios 状态可查询/恢复；
既有 tty 输出轮次不回归。

### M15：Process/Thread 模型与 WaitObject/futex

**目标**：为线程、进程、fork/exec 和阻塞同步建立可验证的底层对象模型。

**前置**：D3 关闭；先有 COSIR 语义修订与设计 spec。不得直接在 `Task` 上叠加隐式语义。

**范围**：

- Process（地址空间、fd 表归属、cwd、cap 域）与 Thread（执行上下文、TLS/FS、signal mask、
  调度状态）的边界；Task 与 Thread 的映射/迁移策略。
- `WaitObject`/futex 等待队列：阻塞、唤醒、超时、取消；与现有 `BlockedRecv`/`SYS_EXC_WAIT`
  的关系。
- 线程创建/销毁、共享地址空间、每线程栈、TLS 初始化、退出与回收。
- 容量：Task/Cap/vspace/audit 的预算与上限。

**ABI/对象**：按 D2 从 42 起追加；具体号与名称在 D2/spec 冻结前只写“42+”。禁止把 Linux
`clone`/`futex` 号泄漏进内核。

**测试**：host 状态机（创建/阻塞/唤醒/超时/销毁/竞争）；QEMU 线程创建/join、futex wait/wake、
TLS 读写、共享地址空间；错误路径（无权限、超时、销毁中唤醒）；容量峰值。

**profile**：P5-THREAD-01 从 `unsupported` 推进到 `partial`（仅冻结子集）；P5-PROC-01/02
的进程/线程区分在 M17 再升级。

**退出条件**：设计 spec 获批；线程/等待探针双轮 PASS；无忙等；退出后线程栈/TLS/等待队列
全部回收；容量报告齐全。

### M16：pthread/TLS/mutex/cond

**目标**：让 musl 的 pthread 子集在 M15 的机制上真实运行，不用空实现占位。

**范围**：

- `pthread_create`/`pthread_join`、`pthread_self`、每线程 `errno`、TLS。
- mutex/cond 的冻结子集（普通/递归/错误检查中的哪一种由 profile 决定）；`futex` 映射。
- `clone`/`set_tid_address` 的适配层映射；线程栈分配与 guard。
- 不做取消、优先级、`pthread_atfork`、完整 `pthread_*`。

**测试**：同源 `stage5_thread.c`（创建/join、TLS 隔离、mutex 互斥、cond 唤醒、错误路径）；
Linux/Prima 逐字节/退出码一致；QEMU 双轮；竞态压力有界且可重复。

**profile**：P5-THREAD-01 按实际覆盖标 `partial` 或 `supported` 子集；不支持项逐条列出。

**退出条件**：pthread 探针双轮 PASS；单线程/多线程切换不破坏既有输出；TLS 在上下文切换后
仍正确；容量与 audit 变化登记。

### M17：`fork/execve/wait4`

**目标**：提供最小 POSIX 进程生命周期，支撑需要子进程的目标程序。

**前置**：D3 关闭；M12 的退出状态、M13 的 VFS、M15 的进程/线程模型。

**范围**：

- 单线程 `fork`（COW 地址空间、fd 表继承、cap 继承/收窄）；多线程 `fork` 返回 `EAGAIN`。
- 静态 ELF `execve`：替换地址空间、重建初始栈、argv/envp、fd 继承与 `CLOEXEC`。
- `wait4`/`waitpid`：退出状态、阻塞/非阻塞、`ECHILD`、`SIGCHLD` 占位（M18 前只做状态）。
- 不做进程组/session、`ptrace`、`vfork`、`posix_spawn`、动态链接。

**ABI/对象**：按 D2 从 42 起追加；可能需要 Process 对象/等待队列；具体在 spec 冻结。
`fork` 的 COW 必须复用已有 COW 机制并补反例测试。

**测试**：同源 fork/exec/wait 探针；子进程退出码/信号终止（M18 后）；fd 继承/`CLOEXEC`；
COW 写隔离；多线程 fork `EAGAIN`；`ECHILD`；资源回收。

**profile**：P5-PROC-01/02、P5-FD-01、P5-BOOT-01 的相关子项；若 D3 决定不纳入阶段 5，
则 profile 标 `unsupported`，并说明 S5-D2 如何在不含 fork/exec 的情况下满足。

**退出条件**：单线程 fork/exec/wait 探针双轮 PASS；COW 反例通过；无僵尸/泄漏；
不支持项有确定错误。

### M18：POSIX signal 最小集

**目标**：在异常递送机制上建立最小 POSIX signal 用户面，支撑错误处理、子进程状态和 tty 中断。

**前置**：D7 关闭；M15/M16 的线程上下文与每线程 mask。

**范围**：

- `sigaction`/`sigprocmask`/`kill`/`raise`/`sigreturn`；最小信号集由 D7 冻结。
- 异常→signal 映射（`SIGSEGV`/`SIGILL`/`SIGFPE`/`SIGBUS`）；`SIGCHLD`（与 M17 配合）；
  `SIGINT`（与 M14 配合，若纳入）。
- `EINTR`：阻塞调用被信号打断后的返回值/恢复语义；不做完整 `SA_RESTART` 矩阵，除非 D7 扩展。
- 不做实时信号、`sigaltstack`、`siginfo` 全字段、`ptrace`。

**ABI/对象**：尽量复用 `SYS_EXC_SUBSCRIBE`/`SYS_EXC_WAIT`/`SYS_EXC_RESUME`，由用户态
signal 框架解释；确需新内核入口时按 D2 追加。内核不得直接解释 POSIX signal 号。

**测试**：同源 signal 探针（handler、mask、kill/raise、`sigreturn`、`EINTR`、故障信号）；
QEMU 双轮；嵌套/屏蔽/恢复帧边界；未处理信号导致确定终止。

**profile**：新增 P5-SIG-01（作为 profile v2 提案，不直接改 v0）；状态只按证据升级。

**退出条件**：最小集合全项通过；信号处理不破坏 syscall 恢复；内核无 POSIX signal 语义；
文档更新到 v2 提案并等待评审。

### M19：用户态网络子集

**目标**：提供真实的 socket 语义，不用 IPC 转发冒充网络。

**前置**：D5 关闭；M12 的 fd/退出清理（M13 可并行，不是硬依赖）。

**范围**：

- 用户态网络服务：loopback 链路 + IPv4/UDP；`socket`/`bind`/`sendto`/`recvfrom`/`close`。
- fd 表接入、阻塞/非阻塞的最小语义、地址结构/字节序、错误码。
- 不做 NIC 驱动、DNS、TCP listen/connect、`poll`/`select`/`epoll`，除非 D5 明确扩展。
- 明确“loopback IPC 不算网络”：必须有 IP/UDP 报文与 socket 语义。

**ABI/对象**：预计不需要新 syscall；用户态服务 + 协议 tag + fd 表。若需要共享内存环，
必须复用 `MemorySegment`，不得新增内核 socket 对象。

**测试**：同源 `stage5_net.c`（UDP echo、bind/sendto/recvfrom、错误地址、关闭后行为）；
Linux/Prima 差分；QEMU 双轮；无网络硬件依赖。

**profile**：P5-NET-01 从 `unsupported` 推进到 `partial`；TCP/外部网络明确 unsupported。

**退出条件**：UDP loopback 探针双轮 PASS；socket fd 生命周期与退出清理正确；
D5 记录的范围外能力有确定失败。

### M20：Rust core/std 与兼容矩阵

**目标**：把“能运行 C 程序”推进为“有可重复的兼容矩阵和 Rust 目标”，为阶段 5 出口收口。

**前置**：D6 关闭；M13–M19 的 profile 行稳定。

**范围**：

- 自定义 Rust target（名称由 D6/spec 冻结），`core`/`alloc`/`std` 的最小可用子集；
  `std::io`/`fs`/`thread`/`net`/`time`/`process` 的哪些模块可承诺，逐项矩阵。
- profile v2：把 M12–M19 的实际覆盖写成逐 API 行；更新符号↔syscall 追溯、`compat-state`、
  ABI 握手、persona 预留位、符号版本策略。
- 选定至少两个开源程序（含已有 sbase 程序）做锁定源码重编译 + 冻结功能测试；许可证/
  源码哈希/编译参数/差异全部登记。
- S5-D4/D5/D7 的矩阵与护栏；S5-D6 的 Rust 目标。

**非目标**：全量 `std`、动态链接、完整 POSIX 兼容、LTP/PISA。

**测试**：Rust `no_std`/`std` 探针；`cargo` 交叉构建；同源 C/Rust 程序；profile 逐行差分；
符号追溯完整性；兼容矩阵缺口审计。

**profile**：发布 profile v2 提案；S5-D1 的组织级确认仍单独进行。

**退出条件**：矩阵每行有状态/证据/偏差；至少两个开源程序通过；Rust 子集有可重复测试；
未覆盖项明确 unsupported。

### M21：阶段 5 出口评审

**目标**：按证据判定阶段 5 是否满足退出条件，而不是按完成度主观判断。

**工作**：

- 全量重跑：`cargo test --workspace`、`cargo xtask lint`、`cargo xtask test`（BIOS 双轮 +
  所有 stage5 轮次）、适用的 `cargo xtask compat`。
- 逐项核对 S5-D1–D7/X1–X3；未满足项保留 partial/unsupported，不勾选。
- 复核 profile v2、compat 台账、符号追溯、报告证据边界。
- 出具阶段 5 出口评审报告；若通过，更新跟踪页并登记阶段 6 入口。

**退出条件**：S5-X1/X2 的矩阵完整；S5-X3 的选定程序通过且经组织级复核；所有未支持项
有确定失败与审计边界；阶段 6 入口决策（D10）已记录。

## 6. 阶段 5 出口映射

| 阶段 5 条件 | 覆盖里程碑 | 最终证据 |
|---|---|---|
| S5-D1 冻结且版本化的 POSIX profile | D1 + M12/M13/M15/M16/M19/M20 | profile v0/v1/v2 及其评审记录 |
| S5-D2 libc 适配 | M2–M20 的累积 | musl 补丁、适配层、libc-test/同源程序 |
| S5-D3 fd、路径、线程、基础网络子集 | M13/M14/M16/M19 | 各 M 的同源差分与 QEMU 双轮 |
| S5-D4 逐 API 语义映射矩阵 | M20 | profile v2 + 符号追溯 + compat-state |
| S5-D5 与 Linux 的差分测试矩阵 | 每个 M + M20 | 同源编译、逐字节/退出码/错误码对比 |
| S5-D6 Rust 目标与 core/std | M20 | target、构建脚本、Rust 探针 |
| S5-D7 兼容性护栏 | M12/M20 | persona 预留、符号版本、ABI 握手、台账 |
| S5-X1 每项 API 有语义映射和可重复测试 | M20 | 矩阵行 + 测试 ID |
| S5-X2 每项 API 明确 supported/partial/unsupported | M20 | profile v2 状态列 |
| S5-X3 选定开源程序通过冻结功能集 | M21 | 锁定源码重编译 + 双轮 + 组织级复核 |

## 7. 阶段 6 入口

阶段 6 只在 M21 通过后进入正式实施；之前可以做设计/原型，但不得登记为阶段 6 完成。

### S6-A：Linux 静态 ELF 装载器与初始栈

- 装载 ET_EXEC/ET_DYN 静态 ELF；拒绝 `PT_INTERP`/`DT_NEEDED`；按 `PT_LOAD` 权限映射，
  BSS 清零，W^X。
- 完整 Linux 初始栈：`argc/argv/envp/auxv`，冻结 `AT_PHDR`/`AT_PHENT`/`AT_PHNUM`/
  `AT_ENTRY`/`AT_BASE`/`AT_PAGESZ`/`AT_RANDOM` 等字段的取舍；栈对齐。
- 证据：未修改静态 ELF 的 Linux/Prima 双跑；错误 ELF/坏权限/超容量确定失败。

### S6-B：OS 中立 trap/exception 递送与 Linux persona

- 复用 COSIR §12 的 ExceptionSource/订阅/递送语义；syscall trap 作为 `SyscallTrap` 递送，
  内核不解释 Linux syscall 号/errno。
- 用户态 Linux persona 负责 Linux 号→Prima 服务/机制的翻译、错误码、阻塞/恢复。
- 证据：QEMU trace 显示 trap 递送路径；内核源码/对象检查证明无 Linux 专属对象或语义
  （S6-X3 的预检）。

### S6-C：Linux LibOS syscall profile

- 冻结首批 Linux syscall 子集：进程/线程、fd/VFS、mmap、futex/TLS、signal、时间。
- 复用阶段 5 的用户态服务与 Capability；persona 只做 ABI 翻译与视图，不把 Linux 语义下沉。
- 产出 S6-D4 要求的版本化 Linux LibOS syscall profile（逐号、逐错误、逐阻塞语义）。
- 证据：与参考 Linux 的差分矩阵；未支持 syscall 的确定、可审计失败。

### S6-D：未修改静态 ELF 白名单与测试镜像

- 选定白名单程序/版本/许可证/源码哈希/构建方式；测试镜像不入 Git。
- 覆盖正常路径、错误路径、资源回收、未支持 syscall；S6-X2 要求逐 syscall 的 Linux/Prima 差分。
- 证据：白名单程序无需修改即可运行；S6-X1 双跑通过；S6-X2 差分通过；S6-X4 未支持项有审计记录。

### S6-E：阶段 6 出口评审

- 逐项核对 S6-D1–D5/X1–X4；重点是 S6-X3 的内核纯净性和 S6-X4 的确定失败。
- 出具阶段 6 出口报告；未满足项保留阶段进行中。

## 8. 横切约束

### 8.1 证据分级

host 测试、QEMU 执行、Linux 差分、Lean 模型各自只能证明其范围。每个里程碑报告必须写：
命令、版本、产物、通过/失败/未运行、已知偏差。不得用“编译成功”“链接成功”代替运行时证据。

### 8.2 容量与审计

- 每个 M 记录 `CAPACITY`、`state_audit_ready` 以及 task/cap/segment/vspace/message/audit 峰值。
- 扩容必须：说明触发场景、给出新上限、重跑全部既有 QEMU 轮次、在报告中逐条登记变化。
- 优先用事件合并/容量契约解决 audit 压力，不用“放宽断言”掩盖。
- 对可能耗尽的上限（`MAX_TASKS`、`MAX_CAPABILITY_SLOTS`、`MAX_AUDIT_EVENTS`）先在
  M12/M15 测量，再决定 M16/M17 的预算。

### 8.3 测试接线

- 扩展 `xtask` 的 `Stage5Program`、`build_images_with_program`、`check_stage5_linux_*` 与
  `test_boot_slice` 双轮；每个里程碑至少一个同源探针。
- tty 输入需要新增 QEMU 串口输入辅助；线程/signal/network 探针要保持确定性，避免竞态导致
  不可重复。
- 保留 M5–M11 的全部既有轮次；任何新容量/协议改动都要重跑。
- 阶段 6 的 xtask 命令/程序在 S6-A 时新增，不复用 stage5 名称冒充。

### 8.4 文档与语义纪律

- 新对象/新语义先改 COSIR，再改实现；阶段定义变化先改 `Prima OS.md`，再改跟踪页。
- 每个 M 完成后新增报告，更新 tracker、profile、compat 和索引；历史报告不改写结论。
- profile 状态只能由证据推进；`partial` 必须列边界，`unsupported` 必须给确定失败路径。
- 不把 DOC-002 的未评审条目静默并入 `prima-abi`。

### 8.5 安全边界

W^X、无硬件 NX、SSP/fortify 关闭、`PROT_NONE` 近似只读等已知限制继续登记。M15/M17
不得用“成功返回的空实现”冒充线程/进程；M18 不得把异常订阅直接称为 POSIX signal。

## 9. 风险与回退

| ID | 风险 | 触发条件 | 缓解/回退 |
|---|---|---|---|
| R1 | BLK-003/profile v1 评审延迟 | D1/D2 未关闭 | 只做 host/原型；不升级状态；先把 spec 和测试准备好 |
| R2 | Process/Thread 模型改动 COSIR | D3 发现与现行语义冲突 | 先修订 COSIR/阶段定义；M15 不启动实现 |
| R3 | 容量耗尽 | 线程/进程/审计峰值触顶 | 先测量再扩容；必要时限制 profile 子集并登记 |
| R4 | QEMU 串口输入不稳定 | M14 输入探针时序敏感 | 用固定脚本/排空屏障；失败保留反例，不伪造 PASS |
| R5 | 网络范围争议 | D5 对“基础网络”解释不一 | 先冻结 loopback IPv4/UDP 子集；NIC 留阶段 7 |
| R6 | Rust std 范围爆炸 | M20 发现依赖面过大 | 冻结最小 target/模块；其余标 unsupported |
| R7 | 阶段 6 内核被 Linux 语义污染 | S6-B 设计走偏 | 内核只做中立递送；persona 在用户态；S6-X3 预检 |
| R8 | fork + COW 正确性 | M17 写时复制/页表反例 | 复用已有 COW 机制；先 host 反例再 QEMU |
| R9 | 文档漂移 | 多个计划/台账各自更新 | 本路线图为剩余工作总入口；索引与 tracker 同步 |
| R10 | 安全限制被误读为已解决 | W^X/NX/SSP 边界 | 报告和 profile 明确 partial；不宣称完整缓解 |

**回退规则**：任一里程碑无法在冻结边界内完成时，优先缩小 profile 子集并保留确定失败，
而不是扩大 `supported` 口径。M17/M18 若被 D3/D7 判为不纳入阶段 5，则在 profile 标
`unsupported`，并说明 S5-D2/D3 如何在不含它们的情况下满足。

## 10. Definition of Done

每个里程碑完成必须同时满足：

1. 涉及新对象/新 ABI 时，有获批的设计 spec 和语义评审记录；
2. host 单测覆盖成功、失败、边界和资源回收；
3. `cargo xtask test` BIOS 双轮 + 对应 stage5 探针 PASS；
4. 适用时有 Linux/Prima 同源逐字节/退出码/错误码差分；
5. 容量、审计、旧轮次变化逐条登记；
6. 新增报告，并更新 tracker、profile、compat 与索引；
7. 不勾选任何超出证据范围的阶段条件。

## 11. 与现有计划的关系

- [阶段 5 修订计划](2026-10-18-stage5-revised-plan.md) 是 2026-10-18 的高层切片计划；
  本路线图承接其片 D–G 的剩余部分，并细化到 M12–M21。
- [musl 移植计划](2026-10-18-musl-port-plan.md) 仍是 libc 适配主线（M0–M5 历史、
  M6–M11 已完成的增量记录）；本路线图负责跨子系统排序与阶段 6 入口。
- 旧计划保留为历史执行记录；当前“下一步”以本路线图和
  [`Prima OS 实现阶段.md`](../../../Prima%20OS%20实现阶段.md) 为准。

## 12. 本文件不代表的结论

- 不表示阶段 5 已完成，不表示任何 S5-D*/S5-X* 已满足；
- 不表示阶段 6 已开始，不表示 S6-D*/S6-X* 有证据；
- 不表示 Process/Thread、pthread、signal、网络、Rust std 或动态链接已经可用；
- 不表示 profile v1/v2 已发布；
- 不表示 BLK-003 已关闭；
- 不表示 `Prima OS.md` 的阶段定义已经修改。
