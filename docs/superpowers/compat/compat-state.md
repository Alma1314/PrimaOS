# 阶段 5 兼容状态台账与历史基线

> 阅读范围：§1–4 保留 M0–M11 时期的 profile/链接盘点与实现基线，其中“未实现”不是当前总状态。§5–6 为 M20 后继汇总，§7 记录 2026-10-24 musl 补强；当前阶段与签署以[阶段跟踪](../../../Prima%20OS%20实现阶段.md)和[M21](../reports/2026-10-23-stage5-m21-exit-review.md)为准。本文维护不发布新 profile。

**日期**：2026-10-18（2026-10-23 更新 M14–M18 与 M20 矩阵）。**性质**：M0 依赖盘点的状态记录，不是新的 ABI 承诺；M3/M3b 补充了
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
[profile v1 delta](../specs/2026-10-20-stage5-profile-v1.md)）；M9 新增 stat 家族
（x86_64 musl 走 kstat 4/5/6/262，另实现 `statx(332)`）与 `chdir`/`getcwd`，并用 sbase `cat`/`ls` 做端到端
验收（见 [M9 报告](../reports/2026-10-21-stage5-m9-metadata-cwd.md)）；M10 追加
`SYS_MEMSEG_PROTECT=41` 与 C 侧 56 页匿名 mmap 区的按页分配/真正 unmap/复映射/改权，
把 `P5-MM-01` 推进到 `partial`（见 [M10 报告](../reports/2026-10-21-stage5-m10-anon-memory.md)）；M11 追加单路径 `mkdir`/`unlink`/`rmdir` 与删除槽位复用，把 `P5-FS-06` 推进到 `partial`（见 [M11 报告](../reports/2026-10-22-stage5-m11-file-mutation.md)）；M12 追加固定探针 envp/auxv 与退出自动关闭、`getppid`、`O_CLOEXEC` 记录（见 [M12 报告](../reports/2026-10-22-stage5-m12-c-startup.md)），M13a 追加符号链接/`readlink`/`rename`/`fchdir` 与内容区压实（见 [M13a 报告](../reports/2026-10-22-stage5-m13a-vfs-links.md)），M13b 追加节点 mode 位、`chmod`/`fchmod`/`fchmodat`/`umask`、`readv` 与 stdio 文件 API（见 [M13b 报告](../reports/2026-10-22-stage5-m13b-permissions-stdio.md)），M19 追加用户态 loopback IPv4/UDP 子集（见 [M19 报告](../reports/2026-10-23-stage5-m19-loopback-udp.md)），M14 追加 fd 0 tty 输入与 termios 子集（见 [M14 报告](../reports/2026-10-23-stage5-m14-tty-input.md)），
但**没有**勾选 S5-D*/S5-X*。`malloc/free` 仍只代表 mallocng 在 96 页 arena 上工作，不代表通用 `mmap`；
VFS 的 `mkdir`/`unlink`/`rmdir`、`rename`、符号链接/`readlink` 与 `fchdir` 已是 `partial`（P5-FS-06/07），但仍无硬链接、
跨启动持久化；M13b 已有 owner mode/umask 与 stdio，M13a 已有内容压实，M12 已有退出自动关闭。时间只是 10 ms 粒度的 PIT 轮询，
不是阻塞时钟服务。M11 之后的剩余 API/子系统与阶段 6 入口见
[阶段 5 收尾与阶段 6 入口路线图](../plans/2026-10-22-stage5-completion-roadmap.md)；
该路线图是计划，不改变本台账状态。

符号级 `UNTEST` 沿用候选规格的术语：musl 会把它链接进来，但本阶段不承诺其行为。`UNTEST`
不进入 profile 支持表。

2026-10-23 的[VFS / LibOS 后继报告](../reports/2026-10-23-vfs-libos-foundation-report.md)记录打开对象提取、延迟回收与稀疏写修复；P5-FS-01/06/07、P5-FD-01 保持 partial，不据此发布新 profile 或声明第二种 OS personality 可用。

## 1. profile v0 行与 M8/M9 的 v1 delta

| ID | 当前状态 | M0 结论 |
|---|---|---|
| P5-ABI-01 | supported | 不变；`SYS_ABI_CHECK=37` 已见证 |
| P5-BOOT-01 | partial | 固定探针形态仍成立；M8 把初始 fd 0 设为 null、fd 1/2 为 tty；**M12**：固定探针 envp 为三项，auxv 含 `AT_PAGESZ`/`AT_PHDR`/`AT_PHENT`/`AT_PHNUM`/`AT_ENTRY`/`AT_BASE`/`AT_EXECFN`/`AT_NULL`；参数块支持 legacy/版本化 argv/envp（16/8 上限）。仍为固定 argv，独立参数载体触发 ISSUE-START-ARGS-001 |
| P5-PROC-01 | partial | 固定探针的返回码已见证；musl 的 `exit_group` 需要新的退出观察接线 |
| P5-IO-01 | partial | fd 1/2 逐字节 tty 流；M8 支持 fd 0（写丢弃）与本地 fd 3..18 的 file/dir 写入（经 VFS，3 字节分片）；目录写 `EBADF`。无短写/EPIPE/非阻塞 |
| P5-PROC-02 | partial | **M7**：`getpid` → `SYS_TASK_ID`；任务存活期间稳定、u32 回绕前不复用；**M12b**：`getppid` → 管理器标识（非零、不同于 `getpid`，非正式父子关系）；无 Process/Thread 区分 |
| P5-IO-02 | partial | **M8**：fd 0 读 EOF；fd 3..18 文件 `read` 可用（EOF/短读由服务端容量决定）；目录不暴露给 `read`（`EBADF`），只经 `P5-FS-03`；tty `read` 与 `readv` 未实现 |
| P5-FD-01 | partial | **M8**：本地 fd 0..18；fd 0=null、1/2=tty、file/dir 指向服务端句柄 3..10；`dup*` 共享句柄/offset/目录游标，最后引用才关闭；目标 0..18；无 fd 0 的 tty 输入、表满 `EAGAIN`；**M12b**：退出前自动关闭本地 fd 与最后一个服务端句柄，`O_CLOEXEC` 只在本地表记录（无 exec 语义） |
| P5-FS-01 | partial | **M8/M9**：绝对/相对、多级目录、`.`/`..`、尾随 `/`、`dirfd` 起始目录；M9 起 `AT_FDCWD` 的相对路径从当前 cwd 解析（初始 `/`）；`O_CREAT`/`O_EXCL`/`O_TRUNC`/`O_APPEND`/`O_DIRECTORY`；Namespace `fs` 一次授权；**M13a**：`open` 跟随末项符号链接（相对目标以链接父目录为基准，深度上限 8）。无 `O_NOFOLLOW`，悬空链接上 `O_CREAT` 未实现；名称 ≤16 B、路径 ≤64 B、分量 ≤6；只有 `/tmp` 可新建普通文件 |
| P5-FS-02 | partial | **M8**：普通文件 SET/CUR/END；目录 fd 的 `lseek` 重置枚举游标；`O_APPEND` 在服务端每次写前定位 EOF；同一句柄经 `dup` 共享 offset；无跨 open 共享、无 `pread`/`pwrite` |
| P5-FS-03 | partial | **M8/M9**：`getdents64(217)` 返回 `linux_dirent64` 子集（`.`/`..`、`d_ino`、`d_type`、8 字节对齐 `d_reclen`）；非目录 `ENOTDIR`、缓冲不足 `EINVAL`；M9 经 musl `opendir`/`readdir` 的 sbase `ls` 端到端验收；`d_off` 非稳定 cookie，`telldir`/`seekdir` 未验收 |
| P5-FS-04 | partial | **M9**：x86_64 musl 经 kstat 4/5/6/262 提供类型/权限/大小/inode/nlink/blksize/blocks（同一字段集经 `statx(332)`）；目录 0555/0777、文件 0444/0666、`/dev/null` 为 `S_IFCHR\|0666`；接受 `AT_NO_AUTOMOUNT`/`AT_SYMLINK_NOFOLLOW`/`AT_EMPTY_PATH`；时间戳与 `st_dev`/`st_rdev` 恒 0，无 `statx` sync/mount 语义 |
| P5-FS-05 | partial | **M9**：每任务 cwd，初始 `/`；`chdir` 确认目录后保存词法规范化的绝对路径（`..` 按字面回退、根处钳制）；`getcwd` 返回含 NUL 长度，缓冲不足 `ERANGE`；**M13a**：`fchdir` 恢复目录 fd 打开时记录的规范路径（非目录 `ENOTDIR`）；无 cwd 继承/`buf=NULL` 分配形式 |
| P5-FS-07 | partial | **M13a**：同一 VFS 内 `rename`/`renameat`（文件/目录/链接；替换文件或空目录；子树/非空拒绝）、`symlink`/`symlinkat`/`readlink`/`readlinkat`（目标 ≤64 B，缓冲不足按 Linux 截断）与 `fchdir`；硬链接/`linkat` 返回 `ENOSYS`；`lstat` 仍跟随末项；**LibOS 基础重构**：替换已打开普通文件保留旧目标对象到最后 close，源/目标父目录均检查写权限 |
| P5-TTY-01 | partial | **M14**：fd 0 输入；canonical/raw、回显、`Ctrl-D` EOF、单次读 ≤3 B；`TCGETS`/`TCSETS`/`TIOCGWINSZ`；无 job control/signal/pty/流控/`VMIN`/`VTIME`；有读者时轮询 COM1（M15 后替换） |
| P5-NET-01 | partial | **M19**：AF_INET/SOCK_DGRAM/127.0.0.1；4 端点、64 B 数据报、bind/sendto/recvfrom/getsockname/close；`bind(0)` 临时端口；EADDRINUSE/EAFNOSUPPORT/EMSGSIZE/未绑定丢弃；无 TCP/IPv6/connect/poll/NIC/DNS |
| P5-FS-08 | partial | **M13b**：节点 mode（0..0o777）；`chmod` 跟随链接、`fchmod` 按 fd、`fchmodat` flags=0；进程级 `umask`（默认 022）在创建后由适配层收紧；只强制 owner 读写位，无 uid/gid/ACL/setuid |
| P5-LIB-02 | partial | **M13b**：`fopen`/`fread`/`fwrite`/`fseek`/`fclose` 经 musl 实跑；依赖适配层 `readv`；未验收 `fdopen`/`freopen`/`setvbuf` |
| P5-FS-06 | partial | **M11**：`mkdir`/`mkdirat`/`unlink`/`unlinkat`/`rmdir` 单路径；`AT_FDCWD` 相对路径合成 cwd，`dirfd` 以目录节点为起点；`AT_REMOVEDIR` 支持，其它 flags `EINVAL`；节点删除标记 `parent=0xFF` 与槽位复用；**M13a**：节点表 32 槽，删除的内容区由写入压力触发的压实回收；**LibOS 基础重构**：普通文件 unlink 后保留打开对象到最后 close；打开目录仍拒绝删除（`EACCES`）；非空目录/目录 `unlink` 用 `EACCES` 代替 `ENOTEMPTY`/`EISDIR`；M13b owner mode 已实现，无跨启动持久化；`rename`/符号链接见 P5-FS-07 |
| P5-MM-01 | partial | **M10**：匿名 `MAP_PRIVATE` 子集在 56 页固定 arena 内按页 first-fit 分配，`munmap` 真正拆 PTE、复用地址时复映射；`mprotect` 经内核 `memseg_protect(41)` 改权（RW↔R 等）；`PROT_NONE` 近似只读，匿名 `PROT_EXEC` 返回 `EACCES`（W^X）；无文件/`MAP_SHARED`/`mremap`、无硬件 NX |
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
| 用户态适配（无新内核机制） | `writev`/`write`/`close(1/2)`/`ioctl` 的 Linux 号映射；`set_tid_address` → `SYS_TASK_ID`；`exit(60)`/`exit_group(231)` → 退出消息 + 自毁；`__syscall4..6` 全参数透传；`brk`/匿名 `mmap`/`mprotect`/`munmap` → 预映射 arena（M10 起按页分配、真正 unmap/复映射，`mprotect` 走内核 `SYS_MEMSEG_PROTECT=41`；partial）；**M8/M9**：本地 fd 0..18 与 `open`/`openat`/`read`/`write`/`lseek`/`close`/`dup`/`dup2`/`dup3`/`getdents64` 翻译、句柄引用计数与共享 offset/目录游标、3 字节分片、OPEN/PATH/COMMIT 三段式路径、Namespace `fs` 一次授权、用户态 VFS/rootfs；M9 增加 STAT 请求（不占句柄）与 `statx`/`chdir`/`getcwd`；M11 增加 MUTATE 请求（mkdir/unlink/rmdir，复用 PATH/COMMIT）；`getpid` → `SYS_TASK_ID`；`__stdio_*`、`printf`/`fflush` 等 musl 路径 |
| 需新增内核机制（已落地 M2/M3/M6） | `arch_prctl(ARCH_SET_FS)` → `SYS_TASK_SET_FS_BASE=38`，per-task FS base；M3 另启用 SSE（`CR4.OSFXSR/OSXMMEXCPT`）与 per-task FXSAVE/FXRSTOR；M6 追加 `SYS_TIME_ENABLE=39`（只编程 PIT 通道 0，不配置 PIC/IF）与 `SYS_TIME_TICKS=40`（锁存并累计硬件倒计数） |
| musl 静态基座（M3/M3b/M4 已落地） | 锁定快照 + 两个可重放补丁；Prima crt 接 `__libc_start_main`；静态 `libc.a`/sysroot；`snprintf`/`getenv`/errno、`malloc/free/realloc`、`printf`+`fflush` 已验；C 任务由服务管理器预映射 96 页匿名 arena（与栈同段；M9 起 40 页 brk + 56 页 mmap；M10 起 mmap 区按页分配/回收） |
| 明确不支持（本片） | 信号、`execve`、网络、动态链接、完整 POSIX `mmap`/文件映射、符号链接、`rename`/`fchdir`、权限位、持久化、进程退出自动关闭 fd；`fork`/`wait4` 见 §5（机制部分落地、用户态未验收），不得据此宣称支持 |
| 仅链接、不执行（E1–E6） | mallocng `calloc`、futex/`__wait`（`libc.need_locks=0`）、`poll`/`ppoll`（非 secure auxv 早退）；`open`/`lseek` 已由 M4 适配层承接，但不在 echo 闭包 |

## 4. 非承诺

- 本文件不表示 musl 已完整移植、stdio 已可用或 echo 已通过 E1–E6。M4 只覆盖单文件
  `f` 的 fd 生命周期与 `printf`+`fflush` 输出；stdio 文件 API、线程/信号/网络未验收。
- 文件树变更（M11）只支持单路径 `mkdir`/`unlink`/`rmdir`：无 `rename`/`mv`、链接、
  权限位/`umask`/`chmod`、内容区回收或跨启动持久化；带打开句柄的删除返回 `EACCES`；
  非空目录/目录 `unlink` 用 `EACCES` 代替 `ENOTEMPTY`/`EISDIR`。
- 匿名 `mmap` 是 96 页用户态 arena 的 partial 适配：40 页 brk + 56 页 mmap；M10 起
  mmap 区按页分配、`munmap` 真正拆 PTE、复用地址时复映射，`mprotect` 经内核 `memseg_protect`；
  但 `PROT_NONE` 近似只读、匿名不可执行（无硬件 NX）、无文件/共享映射，单次上限 56 页；
  不得据此宣称完整 POSIX `mmap`。
- M6 的时间是 PIT 自由运行计数 + 用户态轮询：10 ms 粒度，无 `EINTR`、无阻塞睡眠、
  无墙上时钟；相邻 `SYS_TIME_TICKS` 间隔超过约 55 ms 会少计。不得据此宣称完整
  `clock_gettime`/`nanosleep` 或 POSIX 定时器。
- **M8** 的文件服务是用户态 **VFS/rootfs**：静态根树、多级路径、目录枚举、8 个服务端
  句柄、通用 fd 0..18；M9 另加 `statx`/`chdir`/`getcwd`，M11 加单路径 `mkdir`/`unlink`/`rmdir`；
  仍无 `rename`/链接/符号链接/权限位/文件映射/持久化，只有 `/tmp` 可新建普通文件。管理器只做 `fs` 入口的一次授权与路由，不是文件树。
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


## 5. 阶段 5 追加的原生 syscall（42–58）状态

本节把 M13–M17 新增的 Prima 原生入口按**当前证据**登记；这是状态台账，
不是兼容性承诺。随后缀 `partial` 的项只表示机制/模型落地，用户态语义未验收。

| 号 | 名称 | 状态 | 证据 |
|---|---|---|---|
| 42–46 | `SYS_WAIT_CREATE`、`SYS_WAIT_OBSERVE`、`SYS_WAIT_OBJECTS`、`SYS_WAIT_SIGNAL`、`SYS_WAIT_DESTROY` | 已实现 | [M15a-1](../reports/2026-10-23-stage5-m15a-waitobject.md) / [M15b-2b](../reports/2026-10-23-stage5-m15b2b-thread-slots.md)，W1/W2 双轮 |
| 47 | `SYS_THREAD_CREATE` | 已实现 | W2 探针双轮（fresh entry 线程） |
| 48 | `SYS_CLONE` | 已实现 | W4（musl clone 路由、pthread_create/join） |
| 49 | `SYS_THREAD_EXIT` | 已实现 | W4、W2 |
| 50 | `SYS_THREAD_CLEAR_TID` | 已实现 | M16b-3（clear_child_tid 唤醒） |
| 51 | `SYS_FORK` | 单线程 COW；本轮 W6 双轮覆盖真实 musl fork/syscall/C适配层续执行，父同步 TLB、子正常 `_exit` | [续执行后继](../reports/2026-10-24-fork-continuation-report.md)；M17b-4 保留原绕行 |
| 52 | （预留） | — | — |
| 53 | `SYS_WAIT4` | 已实现（W6 双轮）：阻塞/唤醒/回收/ECHILD；WNOHANG 有 host 用例 | M17b-4 报告 |
| 57 | `SYS_SIGACTION` | 已实现（W9 双轮）：登记 handler；无掩码/siginfo | M18b 报告 |
| 58 | `SYS_SIGRETURN` | 已实现（W9 双轮）：恢复被中断现场 | M18b 报告 |
| 55 | `SYS_EXEC_REPLACE` | 已实现（W7/W7b 双轮）：静态 ET_EXEC、内存/VFS 镜像、argv 初始栈；适配层在子任务内直接 execve 受按任务授权限制 | M17c-3/4 报告 |
| 56 | `SYS_SIGNAL` | 已实现（W8 双轮）：`kill(pid, sig)` 默认终止，状态 128+sig 经 wait4 写回；无 handler/sigaction/sigreturn | M18a 报告 |
| 54 | `SYS_PROCESS_EXIT` | 已实现（W6 双轮）：退出状态经 pending-status 在父根写回 | M17b-4 报告 |

**不得据此宣称**：`fork`/`wait4`/`execve` 的 POSIX 语义、进程组/session、信号、
`vfork`/`posix_spawn`、动态链接。


## 6. 阶段 5 收尾矩阵（M12–M20）

本节是 **M20 定稿矩阵**：按已验收的 QEMU/Linux 同源证据汇总各子系统的当前
状态；`partial` 表示该子集可用但不完整，`supported` 表示有双轮证据且边界
已登记。证据链接见各行与 [M20 报告](../reports/2026-10-23-stage5-m20-compat-final.md)。

| 子系统 | 状态 | 证据 | 主要边界 |
|---|---|---|---|
| 进程退出/返回码 | supported | E1–E6、M12 | 固定探针形态 |
| fork（单线程，COW） | supported（W6 双轮 + Linux） | M17b-4 | 多线程 EAGAIN；fd 表 COW 继承、close 语义共享（ISSUE-FORK-CAPS-001） |
| wait4 | supported（W6/W8 双轮） | M17b-4、M18a | 仅 WNOHANG/阻塞；无 rusage；信号状态用 128+sig 编码 |
| exec（静态 ET_EXEC） | supported（W7/W7b 双轮） | M17c-3/4 | 内存/VFS 镜像；无 envp/CLOEXEC/PIE/解释器；子任务内 VFS IPC 受授权限制 |
| kill（默认终止） | supported（W8 双轮 + Linux） | M18a | 仅 1..31；无 SIGCHLD 自动通知 |
| sigaction + handler + sigreturn | supported（W9 双轮 + Linux） | M18b | 无掩码/siginfo/EINTR/SA_RESTART；跨任务投递在下一次 syscall 返回 |
| 线程（thread_create/clone pthread） | supported（W2/W4 双轮） | M15b/M16b | pthread 取消/分离/属性未做；线程操作数上限 24 |
| futex（WAIT/WAKE 子集） | partial（W3 双轮） | M16a | 无 PI/robust/requeue |
| tty 输入/termios | partial（I1 双轮） | M14 | fd 0 轮询；无字间定时/流控 |
| loopback IPv4/UDP | partial（N1 双轮） | M19 | 仅环回、无 TCP/路由/分片 |
| VFS/rootfs | partial（C1–C3/L1–L3/V1–V3 等双轮） | M8/M9/M11/M13 | 无硬链接、跨启动持久化 |
| 网络服务 | partial | M19 | 无 socket 全语义 |
| 信号掩码/EINTR | unsupported | — | 未实现 |
| 进程组/session、vfork、posix_spawn、ptrace | unsupported | — | 明确不做 |

**不得据此宣称**：完整 POSIX 进程/信号/线程/文件/网络语义；`execve` 的
VFS 子任务内路径；动态链接。

## 7. 2026-10-24 musl 调用边界与 fd/stdio 后继

下表为[工作实现契约](../specs/2026-10-24-musl-port-boundary-design.md)中的子集，证据见[本次报告](../reports/2026-10-24-musl-port-boundary-report.md)。未发布新冻结 profile，已有 API 行保持 partial，不用此表覆盖历史验收或阶段签署。

| API / 关联行 | 当前子集状态 | 已实现与实跑证据 | 仍有限制 |
|---|---|---|---|
| `fcntl` / P5-FD-01 | partial | F_GETFD/SETFD、F_DUPFD/CLOEXEC、F_GETFL；F1 Linux 参考 + Prima 双轮；host 包含 EINVAL/EBADF/EMFILE | fd 0..18；F_SETFL 仅 APPEND 不变的受限 no-op；无锁/异步/非阻塞/动态 append 切换 |
| `dup/dup2/dup3` / P5-FD-01 | partial | 修正新 fd 的 CLOEXEC：dup/dup2 清除、dup2(fd,fd) 保留、dup3 显式设置；F1 双轮 | 既有 open/dup 表满 EAGAIN 不变；exec 尚不自动关闭；fork close 隔离未修 |
| `fdopen/fopen` / P5-LIB-02 | partial | fdopen 读模式、已 O_APPEND fd 的追加模式、fopen("a") 与内容校验；F1 Linux 参考 + Prima 双轮 | 对非 append fd 的 fdopen("a") 返回 EINVAL；完整 freopen/setvbuf 未验收 |
| `readv/writev` / P5-IO-01/02 | partial | 0 向量、0 长度、数量上限、SSIZE_MAX 预校验、后续错误保留进度、EOF/短传输结束；F1 + host | 多 IPC 不原子；非 NULL 坏指针可终止任务；无取消/EINTR；不复刻所有 Linux MAX_RW_COUNT 细节 |
| 取消点内部调用 | partial（内部边界） | __syscall_cp_asm 检查 pending 状态并完整转发 n+六参数；host 汇编测试与 F1 实跑；W4 create/join 重跑双轮 | 不能据此提升 pthread_cancel/detach 为 supported；没有异步信号取消机制 |
| `vfork` | unsupported | 确定 -1/ENOSYS，F1 Prima 专有负例 | 不发送 Linux 58，避免调用 Prima sigreturn |
| libc 信号入口 | unsupported（本切片） | restorer 改为原生 58；归档陷入检查 | rt_sigaction 仍无完整 libc 映射；W9 的原生机制证据不等于 libc sigaction 可用 |
| 分离线程自清理 | unsupported | __unmapself 不再发 Linux 11/60，改走现有退出适配层 | 保留映射到进程生命周期结束；没有栈回收/资源不泄漏验收 |

构建默认 make -j2，`PRIMA_MUSL_JOBS` 可调整。该约束仅作用于 musl 编译；不代表 QEMU 低 CPU 或修复已有启动超时。锁定快照、补丁重放和归档检查由 build.sh/build.py 实施，测试命令 `cargo xtask test-musl --host-only` 可独立执行，不启动 QEMU。

## 8. fork 续执行与等待输出后继（2026-10-24）

[工作契约](../specs/2026-10-24-fork-continuation-design.md)与
[实际运行](../reports/2026-10-24-fork-continuation-report.md)接续 M17b-4：W6 三种C路径、
父子栈/全局/TLS errno、musl子 `_exit`、waitpid/wait4、int邻接哨兵与ECHILD；W7/W7b/W8撤去内联51。
COW克隆增加同步TLB、构造失败回收及NX保留，wait状态四字节、跨页/逐级权限检查和COW写回。
本片不扩充已冻结profile：共享cap/close隔离、子VFS授权、CLOEXEC、信号掩码、多线程fork和完整生命周期仍受原限制。
聚焦与完整命令的结果分别读取报告，不能将host用例或某一双轮外推为全套验收。


## 9. 2026-10-25 启动/退出/Rust 工作增量

本节依据[工作契约](../specs/2026-10-25-stage5-gap-closure-design.md)，运行结果读取
[本轮报告](../reports/2026-10-25-stage5-gap-closure-report.md)。新profile为[v2提案](../specs/2026-10-25-stage5-profile-v2.md)，仍待组织级评审；旧冻结边界不改。

| 路径 | 当前工作边界 | 测试与未覆盖项 |
|---|---|---|
| C启动参数 | 实际参数大小决定16对齐RSP；保留16/8项、单串255B、一页载体；拒绝NUL | A2独立版本化载体、Linux与QEMU；不是任意程序保证 |
| 托管main/_exit/exit_group | 报告后停在RECV，管理器收到状态后先销毁Task再回收段 | 默认/F1及全部既有组；fork子仍原生54、不关闭继承fd |
| fork/clone FPU | 当前运行父任务FXSAVE后复制子槽 | W6三种C路径检查非默认MXCSR/x87控制字；非XSAVE/AVX/SMP证明 |
| Rust core/alloc | x86_64-prima，单线程静态PIE，原生Runtime，应用自带allocator | R1；一次生命周期的有限单调arena，无生产回收保证 |
| restricted std | BTreeMap、内存Read等无需OS初始化的子集 | R2；metadata明确Unsupported，默认System返回NULL；无std文件/线程/网络/时间/进程后端 |

Rust最小目标不关闭S5-D6，profile提案不关闭S5-D1；能力隔离、子VFS授权、完整exec/CLOEXEC、信号、TTY IRQ及模型位置/溯源仍按阶段跟踪登记。


## 2026-10-26 未验收实现增量

本节只登记工作代码，不能把上表状态升级为supported。契约见[进程边界后继](../specs/2026-10-25-process-boundary-followup-design.md)，
状态见[工作记录](../reports/2026-10-25-stage0-5-gap-followup.md)。

| 工作路径 | 增量 | 待验收入口 |
|---|---|---|
| 信号/线程 | 65通用upcall、66原子解除栈退出、67原子word等待；libc掩码/取消/嵌套现场 | W4/W9 |
| execve | 62/69、envp、实际VFS读取、提交后CLOEXEC及新crt状态导入 | W7b；完整C新映像见证已写入、未运行 |
| TTY | 70/71 IRQ4、有界FIFO、认证序号/取消/确认、受授权输入通知 | I1新增中断、取消后再读、重启与handler stat |
| 时间 | PIT校准的单CPU物理时钟，72阻塞期限；相对EINTR rem | T1/W9；抽象44的时钟保持独立 |

网络继承/等待事务已写入但待验收；Rust OS PAL及组织级门仍开放。上述探针均未执行。


### Socket 后继待统一验收（2026-10-26）

[后继记录](../reports/2026-10-25-stage0-5-gap-followup.md)第11节已写入Process holder/fork关闭隔离、逐Task发送与接收快照、请求序号/认证来源及CANCEL/FINISH握手。N1扩充fork/dup、EINTR/取消、SA_RESTART/嵌套stat；内部网络布局替换历史client字段，范围见[工作契约](../specs/2026-10-25-process-boundary-followup-design.md)。全部待运行，不扩大M19证据或profile支持状态。


### 完整新 C crt 后继待统一验收（2026-10-26）

W7b保留真实execve(path)紧凑ELF，另经共享交接的原生C `prima_exec_image`执行独立静态C/musl子映像，检查TLS/构造/fd状态后再次从新libc execve rootfs tiny ELF。Linux同源memfd参考与crt/reexec/ofd字段已接入，均未执行。2920B rootfs内容预算不扩大；不宣称可存储完整C文件或动态链接。范围见[工作契约](../specs/2026-10-25-process-boundary-followup-design.md)。


### 当前工作逐入口与静态护栏（2026-10-26，未验收）

[当前表](stage5-working-api.md)逐项登记C Linux分派入口、非分派符号、原生后端、主要失败及预定见证。
前文“Rust OS后端未实现/完整新crt待补”等是对应版本记录；当前有界PAL和完整新C映像已写入，均未统一验收。
[静态护栏](../specs/2026-10-26-static-program-contract-design.md)增加persona/adapter声明、C exec拒绝、crt版本化链接锚点
和符号可见性脚本；不宣称动态版本解析、AS persona或Linux二进制人格。
[具体评审工件](../specs/2026-10-26-stage5-release-decisions.md)保留D6待选择及组织级签署边界，全部新探针未执行。


## 2026-10-27 当前后继证据与历史范围

上文2026-10-26“未运行/待选择”是当时工作记录；当前以
[逐入口表](stage5-working-api.md)与[统一报告](../reports/2026-10-25-stage0-5-gap-followup.md)为准。
用户已选择D6 A，原生stdout/stderr/Instant/sleep/yield及确定失败路径完成R2双轮，R1也双轮通过；
不支持的std文件/网络/进程等仍不纳入。workspace1016、lint、两种Rust Clippy、Lean、
musl八补丁归档/5项host、compat、20项Linux参考和完整单次68轮BIOS均通过。

| 后继调用链 | 本轮见证及边界 |
|---|---|
| fork64准备/认证holder交接/受控发布 → 独立视图、共享OFD | W6/N1；用户选定继承A，单线程fork |
| execve实际OFD读取/静态声明检查 → 69/RX桥/新crt → 再exec | W7/W7b；CLOEXEC/envp/原子失败，完整C内存映像不等于rootfs可存完整C文件 |
| musl信号/取消 → C适配层65/66/67 | W4/W9；SIGCANCEL=33，有界现场/合并通知，不提供完整signal/pthread |
| read(tty) → 序号/取消/确认 → IRQ4/70/71及60认证回复 | I1；总消息/阻塞数守恒，保留功能与清理断言 |
| UDP等待 → Process holder/逐Task快照/CANCEL/FINISH | N1；总消息及blocks-direct守恒，不是NIC/TCP/DNS |
| Instant/sleep → 原生39/40/72 → PIT单CPU物理期限 | T1/W9/R2；100Hz，无wall-clock/SMP保证 |

当前clone.lo仅原生48，restore.lo仅58；线程出口通过C清理再49/66，新的锁定归档已审计。
旧supported、planned、UNTEST均保留其历史范围，本轮不据此发布profile v2或自动勾选阶段。
#AC仅有host路由，没有机器见证；最终profile/静态护栏/组织级签署仍见
[具体评审工件](../specs/2026-10-26-stage5-release-decisions.md)。
