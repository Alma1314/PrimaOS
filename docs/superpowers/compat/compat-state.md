# 阶段 5 兼容状态台账（M0）

**日期**：2026-10-18。**性质**：M0 依赖盘点的状态记录，不是新的 ABI 承诺；M3/M3b 补充了
musl 基座与 `malloc/free` 证据，M4 首切片补充了单文件 fd 生命周期与 `printf` 输出证据。
冻结 profile 仍是 [profile v0](../specs/2026-10-18-stage5-profile-v0.md)；M4 把
`P5-IO-02`/`P5-FD-01`/`P5-FS-01`/`P5-FS-02`/`P5-LIB-01` 推进到 `partial`；M5 的
sbase `echo` E1–E6 把 `P5-LIB-01` 的冻结符号集推进到 `supported`（见
[M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md)）；M6 把 `P5-TIME-01` 推进到
`partial`（`CLOCK_MONOTONIC` + 轮询睡眠，见 [M6 报告](../reports/2026-10-19-stage5-m6-time.md)）；
M7 把 `P5-PROC-02` 推进到 `partial`（`getpid` → `SYS_TASK_ID`），并给 `P5-FD-01` 补上
本地 fd 表 3..10 与 `dup`/`dup2`/`dup3`（见 [M7 报告](../reports/2026-10-19-stage5-m7-fd-table.md)）；
M8 把文件后端推进到用户态 VFS/rootfs，新增通用 fd 0..18、多级路径与 `getdents64`
（见 [M8 报告](../reports/2026-10-20-stage5-m8-user-vfs.md) 与
[profile v1 delta](../specs/2026-10-20-stage5-profile-v1.md)），但**没有**勾选 S5-D*/S5-X*。
`malloc/free` 仍只代表 mallocng 在 64 页 arena 上工作，不代表通用 `mmap`；VFS 仍无符号
链接、cwd、`stat`、进程退出自动关闭与持久化；时间只是 10 ms 粒度的 PIT 轮询，不是阻塞
时钟服务。

符号级 `UNTEST` 沿用候选规格的术语：musl 会把它链接进来，但本阶段不承诺其行为。`UNTEST`
不进入 profile 支持表。

## 1. profile v0 行与 M8 的 v1 delta

| ID | 当前状态 | M0 结论 |
|---|---|---|
| P5-ABI-01 | supported | 不变；`SYS_ABI_CHECK=37` 已见证 |
| P5-BOOT-01 | partial | 固定探针形态仍成立；M8 把初始 fd 0 设为 null、fd 1/2 为 tty。任意 argv/envp 与完整 auxv 未实现。M0 发现 echo 的 E1–E6 需要按用例注入 argv，且完整 auxv 是通用 libc 的前置 |
| P5-PROC-01 | partial | 固定探针的返回码已见证；musl 的 `exit_group` 需要新的退出观察接线 |
| P5-IO-01 | partial | fd 1/2 逐字节 tty 流；M8 支持 fd 0（写丢弃）与本地 fd 3..18 的 file/dir 写入（经 VFS，3 字节分片）；目录写 `EBADF`。无短写/EPIPE/非阻塞 |
| P5-PROC-02 | partial | **M7**：`getpid` → `SYS_TASK_ID`；任务存活期间稳定、u32 回绕前不复用；无 Process/Thread 区分、无 `getppid` |
| P5-IO-02 | partial | **M8**：fd 0 读 EOF；fd 3..18 文件 `read` 可用（EOF/短读由服务端容量决定）；目录不暴露给 `read`（`EBADF`），只经 `P5-FS-03`；tty `read` 与 `readv` 未实现 |
| P5-FD-01 | partial | **M8**：本地 fd 0..18；fd 0=null、1/2=tty、file/dir 指向服务端句柄 3..10；`dup*` 共享句柄/offset/目录游标，最后引用才关闭；目标 0..18；无 fd 0 的 tty 输入、表满 `EAGAIN`、无 exec 故 `O_CLOEXEC` 无语义、无退出自动关闭 |
| P5-FS-01 | partial | **M8**：绝对/相对、多级目录、`.`/`..`、尾随 `/`、`dirfd` 起始目录；`O_CREAT`/`O_EXCL`/`O_TRUNC`/`O_APPEND`/`O_DIRECTORY`；Namespace `fs` 一次授权。无符号链接/cwd/`stat`/`mkdir`/`unlink`/`rename`；名称 ≤16 B、路径 ≤64 B、分量 ≤6；只有 `/tmp` 可新建普通文件 |
| P5-FS-02 | partial | **M8**：普通文件 SET/CUR/END；目录 fd 的 `lseek` 重置枚举游标；`O_APPEND` 在服务端每次写前定位 EOF；同一句柄经 `dup` 共享 offset；无跨 open 共享、无 `pread`/`pwrite` |
| P5-FS-03 | partial | **M8**：`getdents64(217)` 返回 `linux_dirent64` 子集（`.`/`..`、`d_ino`、`d_type`、8 字节对齐 `d_reclen`）；非目录 `ENOTDIR`、缓冲不足 `EINVAL`；`d_off` 非稳定 cookie，`telldir`/`seekdir` 与 `readdir` 端到端未验收 |
| P5-MM-01 | planned | echo 的 TLS 预计走 `builtin_tls`，不需要 `mmap`；mallocng 仅链接、不执行 |
| P5-TIME-01 | partial | **M6**：`clock_gettime(CLOCK_MONOTONIC)` 与相对/绝对 `nanosleep` 可用；PIT 10 ms 粒度，用户态轮询，无 `EINTR`/阻塞等待；`CLOCK_REALTIME` 查询 `EINVAL` |
| P5-THREAD-01 | unsupported | 不变；单线程锁在 `libc.need_locks=0` 时为空操作 |
| P5-NET-01 | unsupported | 不变 |
| P5-LIB-01 | supported | **M5**：sbase `echo` E1–E6 双轮验收了 `strcmp`、`fputs`、`fputc`/`putc`、`putchar`、`fflush`、`fclose(stdout)`、`ferror` 与 `exit`；M4 的 `printf`+`fflush`、`snprintf`/`getenv`/errno、`malloc/free/realloc` 仍成立。stdio **文件** API（`fopen`/`fread`/`fwrite`）未验收 |

## 2. P5-LIB-01 符号级状态（M0 盘点）

sbase `echo` 在锁定 commit 上的对象级未解析符号（`nm -u`，`-O2 -std=c99 -fno-stack-protector`）：

- 正常路径（E1–E6）：`strcmp`、`fputs`、`fputc`、`putc`、`fflush`、`fclose`、`ferror`；
  `putchar` 在 GCC/Clang `-O2` 下会降为 `putc`，两者都必须可用。
- 错误路径（`fshut` 的 `weprintf`，正常用例不触发）：`fprintf`、`vfprintf`、`perror`、`strlen`、`strncmp`。
- 终止：`exit`（`__libc_start_main` 正常路径总会调用）。
- 应用自身：`putword`、`fshut`、`weprintf`；`argv0` 与 `putword` 的 `static first` 位于可写段。

| 符号 | 状态 | 说明 |
|---|---|---|
| `strcmp` | supported | M5 E1–E6 的 `-n`/`--` 判定经 Prima QEMU 双轮验证 |
| `fputs` | supported | `fwrite` → `__fwritex` → `stdout`；M5 E3–E6 输出经 tty |
| `fputc` / `putc` | supported | 词间空格由 `putword` 的 `fputc` 输出；M5 E5 验证 |
| `putchar` | supported | 换行由 `putchar` 输出；M5 E1/E3/E5/E6 验证 |
| `fflush` | supported | `fshut` → `fflush` → `writev`；M5 每个用例经 `fclose`/退出前冲刷 |
| `fclose` | supported | `fshut` → `fflush` + `fclose(stdout)`；M5 六个用例均返回 0 |
| `ferror` | supported | `fshut` 的错误检查；M5 成功路径返回 0 |
| `fprintf` | UNTEST | 仅错误路径；v0 行未列，需 v1 登记 |
| `vfprintf` | UNTEST | 同上；链接闭包远大于 v0 行 |
| `perror` | UNTEST | 仅错误路径 |
| `strlen` / `strncmp` | UNTEST | 纯函数；`fputs`/`xvprintf` 引用 |
| `exit` | supported | M5 六个用例经 musl `exit` 退出码 0，管理器与内核双重观察 |

**v1 提案**：`putc` 与 `exit` 已由 M5 升为 `supported`；`fprintf`、`vfprintf`、`perror`、
`strlen`、`strncmp` 仍为 `UNTEST`（只被 `fshut` 的错误路径链接，成功路径未执行），不得对外
宣称可用。P5-IO-01 的 Prima 后端仍注记“stdio 使用 `writev`”。

## 3. 已确认可复用 / 必须新建 / 不支持（M0）

| 类别 | 结论 |
|---|---|
| 可复用（用户态现有机制） | fd 1/2 的逐字节 `SYS_SEND` 到服务管理器、管理器转发 tty；`SYS_TASK_ID`；`SYS_TASK_DESTROY`；私有可写 `PT_LOAD` |
| 用户态适配（无新内核机制） | `writev`/`write`/`close(1/2)`/`ioctl` 的 Linux 号映射；`set_tid_address` → `SYS_TASK_ID`；`exit(60)`/`exit_group(231)` → 退出消息 + 自毁；`__syscall4..6` 全参数透传；`brk`/匿名 `mmap`/`mprotect`/`munmap` → 预映射 arena 的 bump 分配（partial，见 M3b）；**M8**：本地 fd 0..18 与 `open`/`openat`/`read`/`write`/`lseek`/`close`/`dup`/`dup2`/`dup3`/`getdents64` 翻译、句柄引用计数与共享 offset/目录游标、3 字节分片、OPEN/PATH/COMMIT 三段式路径、Namespace `fs` 一次授权、用户态 VFS/rootfs；`getpid` → `SYS_TASK_ID`；`__stdio_*`、`printf`/`fflush` 等 musl 路径 |
| 需新增内核机制（已落地 M2/M3/M6） | `arch_prctl(ARCH_SET_FS)` → `SYS_TASK_SET_FS_BASE=38`，per-task FS base；M3 另启用 SSE（`CR4.OSFXSR/OSXMMEXCPT`）与 per-task FXSAVE/FXRSTOR；M6 追加 `SYS_TIME_ENABLE=39`（只编程 PIT 通道 0，不配置 PIC/IF）与 `SYS_TIME_TICKS=40`（锁存并累计硬件倒计数） |
| musl 静态基座（M3/M3b/M4 已落地） | 锁定快照 + 两个可重放补丁；Prima crt 接 `__libc_start_main`；静态 `libc.a`/sysroot；`snprintf`/`getenv`/errno、`malloc/free/realloc`、`printf`+`fflush` 已验；C 任务由服务管理器预映射 64 页匿名 arena（与栈同段） |
| 明确不支持（本片） | 线程/`clone`、futex 等待、信号、`fork/execve/wait4`、网络、动态链接、通用 `mmap`/文件映射、符号链接、cwd/`chdir`/`getcwd`、`stat`/`mkdir`/`unlink`/`rename`、持久化、进程退出自动关闭 fd |
| 仅链接、不执行（E1–E6） | mallocng `calloc`、futex/`__wait`（`libc.need_locks=0`）、`poll`/`ppoll`（非 secure auxv 早退）；`open`/`lseek` 已由 M4 适配层承接，但不在 echo 闭包 |

## 4. 非承诺

- 本文件不表示 musl 已完整移植、stdio 已可用或 echo 已通过 E1–E6。M4 只覆盖单文件
  `f` 的 fd 生命周期与 `printf`+`fflush` 输出；stdio 文件 API、线程/信号/网络未验收。
- 匿名 `mmap` 是 64 页用户态 arena 的 partial 适配：无逐映射回收、无权限变更、
  无文件/共享映射，单次上限 48 页；不得据此宣称通用 `mmap`。
- M6 的时间是 PIT 自由运行计数 + 用户态轮询：10 ms 粒度，无 `EINTR`、无阻塞睡眠、
  无墙上时钟；相邻 `SYS_TIME_TICKS` 间隔超过约 55 ms 会少计。不得据此宣称完整
  `clock_gettime`/`nanosleep` 或 POSIX 定时器。
- **M8** 的文件服务是用户态 **VFS/rootfs**：静态根树、多级路径、目录枚举、8 个服务端
  句柄、通用 fd 0..18；但无符号链接/cwd/`stat`/`mkdir`/`unlink`/`rename`/文件映射/
  持久化，只有 `/tmp` 可新建普通文件。管理器只做 `fs` 入口的一次授权与路由，不是文件树。
  `dup` 共享服务端 offset 与目录游标；`O_CLOEXEC` 因无 exec 而无语义。
- M7 的 `getpid` 是 Task id 映射，不是 Process/PID 语义：无父子关系、无进程组、
  无 `getppid`；不得据此宣称 `fork`/`wait4` 支持。
- M4 有新的 QEMU 运行证据（双轮 `cargo xtask test`）。既有 h/i/xyz 字节流保持；
  容量与调度事件的变化（`ipc_ready` direct/blocks、`task_ready`/`TASKCAP` caps、
  `namespace_ready` entries/binds/resolves、`CAPACITY` 与 `state_audit_ready` 数值）
  逐条登记在 [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md) §4，属真实计数
  增长，不是断言放宽。`frames_ready` 的 image/free 数值随内核镜像变化。
- SSP/fortify 在 Prima 构建中预计关闭（避免 `AT_RANDOM` 与 glibc 专有 `*_chk` 符号）；
  这不是安全承诺，需在 M3 中作为已知边界登记。
