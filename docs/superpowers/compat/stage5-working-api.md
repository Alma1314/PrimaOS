---
id: compat-stage5-working-api
kind: reference
authority: scoped-compatibility-ledger
status: working-scoped-validation
lifecycle: current
updated: 2026-10-27
---

# 阶段 5 当前工作 API 与符号调用边界

本表是当前源码的逐入口映射；全部行均为 **partial**，不提升为完整POSIX支持。
具体执行、失败修复和未覆盖范围见[本轮分层验证](../reports/2026-10-25-stage0-5-gap-followup.md)；完整单次68轮及20项Linux参考已通过，仍按下表限制解释。
历史支持范围仍查[兼容台账](compat-state.md)、[符号追溯](symbol-trace.md)及其报告链。
本轮预期范围见[profile v2](../specs/2026-10-25-stage5-profile-v2.md)，尚未发布。
公开libc符号走musl对应包装及 `__syscall0..6`，下表按实际Linux号单列，不等同整个POSIX符号闭包。
错误列列出主要确定拒绝；公共原生授权/容量错误仍可能透传，不是所有错误的穷举。

## C 适配层实际 Linux 入口

| 入口（Linux x86_64号） | Prima后端 | 原生号 | 主要失败/边界 | 验收入口（实际通过范围见报告） |
|---|---|---|---|---|
| `read` (0) | fd读取/TTY或socket等待 | 17/60/65/70/71 | EBADF/EFAULT/EINVAL/EINTR/EAGAIN；有界分片、确认/取消；坏非NULL地址不承诺恢复 | F1/V1/I1/N1 |
| `readv` (19) | fd读取/TTY或socket等待 | 17/60/65/70/71 | EBADF/EFAULT/EINVAL/EINTR/EAGAIN；有界分片、确认/取消；坏非NULL地址不承诺恢复 | F1/V1/I1/N1 |
| `write` (1) | fd写入/TTY队列 | 17/60 | EBADF/EFAULT/EINVAL/EAGAIN；TTY多字节不原子，无外设已呈现保证 | F1/V3/默认 |
| `writev` (20) | fd写入/TTY队列 | 17/60 | EBADF/EFAULT/EINVAL/EAGAIN；TTY多字节不原子，无外设已呈现保证 | F1/V3/默认 |
| `close` (3) | 本地fd与共享OFD holder | 17/60 | EBADF/EMFILE/EINVAL；fd0..18；SETFL仅支持APPEND不变，不支持通用fcntl命令 | F1/W6/W7b/N1 |
| `dup` (32) | 本地fd与共享OFD holder | 17/60 | EBADF/EMFILE/EINVAL；fd0..18；SETFL仅支持APPEND不变，不支持通用fcntl命令 | F1/W6/W7b/N1 |
| `dup2` (33) | 本地fd与共享OFD holder | 17/60 | EBADF/EMFILE/EINVAL；fd0..18；SETFL仅支持APPEND不变，不支持通用fcntl命令 | F1/W6/W7b/N1 |
| `dup3` (292) | 本地fd与共享OFD holder | 17/60 | EBADF/EMFILE/EINVAL；fd0..18；SETFL仅支持APPEND不变，不支持通用fcntl命令 | F1/W6/W7b/N1 |
| `fcntl` (72) | 本地fd与共享OFD holder | 17/60 | EBADF/EMFILE/EINVAL；fd0..18；SETFL仅支持APPEND不变，不支持通用fcntl命令 | F1/W6/W7b/N1 |
| `open` (2) | 认证会话/VFS路径打开 | 17/60/63 | ENOENT/ENOTDIR/EACCES/ELOOP/EMFILE/ENOSPC/EINVAL；固定RAM树；64B路径/16B名称；非一般设备文件系统 | V1/V2/V3/W6 |
| `openat` (257) | 认证会话/VFS路径打开 | 17/60/63 | ENOENT/ENOTDIR/EACCES/ELOOP/EMFILE/ENOSPC/EINVAL；固定RAM树；64B路径/16B名称；非一般设备文件系统 | V1/V2/V3/W6 |
| `lseek` (8) | 共享OFD游标 | 17/60 | EBADF/EINVAL；SET/CUR/END；dup和fork共享；无pread/pwrite | F1/W6/W7b |
| `getdents64` (217) | 目录记录翻译 | 17/60 | EBADF/ENOTDIR/EINVAL；8字节对齐，不承诺跨变更稳定cookie | L1–L3 |
| `stat` (4) | 元数据翻译 | 17/60 | ENOENT/EBADF/EINVAL/EFAULT/ENOTDIR；newfstatat对应musl fstatat；无真实时间戳/device/mount同步 | M1/V2/V3 |
| `lstat` (6) | 元数据翻译 | 17/60 | ENOENT/EBADF/EINVAL/EFAULT/ENOTDIR；newfstatat对应musl fstatat；无真实时间戳/device/mount同步 | M1/V2/V3 |
| `fstat` (5) | 元数据翻译 | 17/60 | ENOENT/EBADF/EINVAL/EFAULT/ENOTDIR；newfstatat对应musl fstatat；无真实时间戳/device/mount同步 | M1/V2/V3 |
| `newfstatat` (262) | 元数据翻译 | 17/60 | ENOENT/EBADF/EINVAL/EFAULT/ENOTDIR；newfstatat对应musl fstatat；无真实时间戳/device/mount同步 | M1/V2/V3 |
| `statx` (332) | 元数据翻译 | 17/60 | ENOENT/EBADF/EINVAL/EFAULT/ENOTDIR；newfstatat对应musl fstatat；无真实时间戳/device/mount同步 | M1/V2/V3 |
| `chdir` (80) | Process适配层cwd/路径视图 | 17/60 | ENOENT/ENOTDIR/EBADF/ERANGE/EINVAL；有限dirfd和词法cwd，不承诺全部符号链接cwd语义 | M1/V2/W7b |
| `fchdir` (81) | Process适配层cwd/路径视图 | 17/60 | ENOENT/ENOTDIR/EBADF/ERANGE/EINVAL；有限dirfd和词法cwd，不承诺全部符号链接cwd语义 | M1/V2/W7b |
| `getcwd` (79) | Process适配层cwd/路径视图 | 17/60 | ENOENT/ENOTDIR/EBADF/ERANGE/EINVAL；有限dirfd和词法cwd，不承诺全部符号链接cwd语义 | M1/V2/W7b |
| `mkdir` (83) | VFS原子树变更/延迟回收 | 17/60 | EEXIST/ENOENT/ENOTDIR/EACCES/ENOSPC/EINVAL；固定节点/内容区，非空目录拒绝，无持久化 | D1/V1 |
| `mkdirat` (258) | VFS原子树变更/延迟回收 | 17/60 | EEXIST/ENOENT/ENOTDIR/EACCES/ENOSPC/EINVAL；固定节点/内容区，非空目录拒绝，无持久化 | D1/V1 |
| `rmdir` (84) | VFS原子树变更/延迟回收 | 17/60 | EEXIST/ENOENT/ENOTDIR/EACCES/ENOSPC/EINVAL；固定节点/内容区，非空目录拒绝，无持久化 | D1/V1 |
| `unlink` (87) | VFS原子树变更/延迟回收 | 17/60 | EEXIST/ENOENT/ENOTDIR/EACCES/ENOSPC/EINVAL；固定节点/内容区，非空目录拒绝，无持久化 | D1/V1 |
| `unlinkat` (263) | VFS原子树变更/延迟回收 | 17/60 | EEXIST/ENOENT/ENOTDIR/EACCES/ENOSPC/EINVAL；固定节点/内容区，非空目录拒绝，无持久化 | D1/V1 |
| `rename` (82) | VFS成对暂存/提交 | 17/60 | ENOENT/ENOTDIR/EACCES/ELOOP/ENOSPC/EINVAL；同一RAM树；有限链接深度；无硬链接/renameat2 | V2 |
| `renameat` (264) | VFS成对暂存/提交 | 17/60 | ENOENT/ENOTDIR/EACCES/ELOOP/ENOSPC/EINVAL；同一RAM树；有限链接深度；无硬链接/renameat2 | V2 |
| `symlink` (88) | VFS成对暂存/提交 | 17/60 | ENOENT/ENOTDIR/EACCES/ELOOP/ENOSPC/EINVAL；同一RAM树；有限链接深度；无硬链接/renameat2 | V2 |
| `symlinkat` (266) | VFS成对暂存/提交 | 17/60 | ENOENT/ENOTDIR/EACCES/ELOOP/ENOSPC/EINVAL；同一RAM树；有限链接深度；无硬链接/renameat2 | V2 |
| `readlink` (89) | VFS成对暂存/提交 | 17/60 | ENOENT/ENOTDIR/EACCES/ELOOP/ENOSPC/EINVAL；同一RAM树；有限链接深度；无硬链接/renameat2 | V2 |
| `readlinkat` (267) | VFS成对暂存/提交 | 17/60 | ENOENT/ENOTDIR/EACCES/ELOOP/ENOSPC/EINVAL；同一RAM树；有限链接深度；无硬链接/renameat2 | V2 |
| `chmod` (90) | 单用户mode/cwd状态 | 17/60 | EBADF/ENOENT/EACCES/EINVAL；owner权限/0777；非完整uid/gid/ACL；fchmodat flags=0 | V3/W7b |
| `fchmod` (91) | 单用户mode/cwd状态 | 17/60 | EBADF/ENOENT/EACCES/EINVAL；owner权限/0777；非完整uid/gid/ACL；fchmodat flags=0 | V3/W7b |
| `fchmodat` (268) | 单用户mode/cwd状态 | 17/60 | EBADF/ENOENT/EACCES/EINVAL；owner权限/0777；非完整uid/gid/ACL；fchmodat flags=0 | V3/W7b |
| `umask` (95) | 单用户mode/cwd状态 | 17/60 | EBADF/ENOENT/EACCES/EINVAL；owner权限/0777；非完整uid/gid/ACL；fchmodat flags=0 | V3/W7b |
| `ioctl` (16) | TTY属性/行规程协议 | 17/60 | EBADF/ENOTTY/EINVAL/EFAULT；仅冻结termios/ioctl子集；无job control/流控 | I1 |
| `socket` (41) | 用户态loopback UDP/Process holders | 17/60/63 | ENOTSOCK/EAFNOSUPPORT/EADDRINUSE/EMSGSIZE/EINVAL/EINTR/EAGAIN；IPv4 loopback，4端点，4×64B队列；无NIC/TCP/DNS/poll；逐Task事务 | N1 |
| `bind` (49) | 用户态loopback UDP/Process holders | 17/60/63 | ENOTSOCK/EAFNOSUPPORT/EADDRINUSE/EMSGSIZE/EINVAL/EINTR/EAGAIN；IPv4 loopback，4端点，4×64B队列；无NIC/TCP/DNS/poll；逐Task事务 | N1 |
| `getsockname` (51) | 用户态loopback UDP/Process holders | 17/60/63 | ENOTSOCK/EAFNOSUPPORT/EADDRINUSE/EMSGSIZE/EINVAL/EINTR/EAGAIN；IPv4 loopback，4端点，4×64B队列；无NIC/TCP/DNS/poll；逐Task事务 | N1 |
| `sendto` (44) | 用户态loopback UDP/Process holders | 17/60/63 | ENOTSOCK/EAFNOSUPPORT/EADDRINUSE/EMSGSIZE/EINVAL/EINTR/EAGAIN；IPv4 loopback，4端点，4×64B队列；无NIC/TCP/DNS/poll；逐Task事务 | N1 |
| `recvfrom` (45) | 用户态loopback UDP/Process holders | 17/60/63 | ENOTSOCK/EAFNOSUPPORT/EADDRINUSE/EMSGSIZE/EINVAL/EINTR/EAGAIN；IPv4 loopback，4端点，4×64B队列；无NIC/TCP/DNS/poll；逐Task事务 | N1 |
| `getpid` (39) | Process身份/父身份 | 59 | 原生错误透传；线程同Process id；无进程组/session接口 | F1/W4/W6 |
| `getppid` (110) | Process身份/父身份 | 59 | 原生错误透传；线程同Process id；无进程组/session接口 | F1/W4/W6 |
| `fork` (57) | 预备子Process/认证服务交接/受控发布 | 11/12/17/60/63/64 | EAGAIN/ENOMEM/EACCES/ENOSPC；仅单线程；活动TTY/net等待拒绝；受控继承例外A；非pthread_atfork全语义 | W6/N1 |
| `wait4` (61) | 四字节POSIX退出状态/回收 | 53 | ECHILD/EINVAL/EFAULT/EINTR；pid>0或-1；0/WNOHANG；不支持rusage；waitpid由musl映射 | W6/W8/W9 |
| `execve` (59) | 静态ELF/原生69/RX桥/新crt | 13/14/15/16/41/63/65/68/69 | ENOEXEC/EACCES/ENOENT/E2BIG/EAGAIN/ENOMEM；≤4 PT_LOAD；16 argv/envp各128B；拒绝persona/adapter不匹配；完整C文件受2920B rootfs容量约束 | W7/W7b |
| `clone` (56) | 共享Process线程/FPU/TLS | 48/50 | EINVAL/EAGAIN/ENOMEM；冻结flags集合；单CPU；musl clone.s替换路径另见下文 | W2/W4/W5 |
| `futex` (202) | 每Task WaitObject/原子word预检 | 42/43/45/46/67 | EAGAIN/EINVAL/EFAULT/EINTR/ETIMEDOUT；WAIT/WAKE及PRIVATE位；无PI/robust/requeue；物理期限 | W3/W4 |
| `rt_sigaction` (13) | libc布局/原生upcall/保存现场 | 65 | EINVAL/EFAULT/EINTR；64槽合并通知、8层现场；保留不可屏蔽位；无altstack/实时排队/完整停止继续 | W9 |
| `rt_sigprocmask` (14) | libc布局/原生upcall/保存现场 | 65 | EINVAL/EFAULT/EINTR；64槽合并通知、8层现场；保留不可屏蔽位；无altstack/实时排队/完整停止继续 | W9 |
| `rt_sigpending` (127) | libc布局/原生upcall/保存现场 | 65 | EINVAL/EFAULT/EINTR；64槽合并通知、8层现场；保留不可屏蔽位；无altstack/实时排队/完整停止继续 | W9 |
| `rt_sigsuspend` (130) | libc布局/原生upcall/保存现场 | 65 | EINVAL/EFAULT/EINTR；64槽合并通知、8层现场；保留不可屏蔽位；无altstack/实时排队/完整停止继续 | W9 |
| `pause` (34) | libc布局/原生upcall/保存现场 | 65 | EINVAL/EFAULT/EINTR；64槽合并通知、8层现场；保留不可屏蔽位；无altstack/实时排队/完整停止继续 | W9 |
| `kill` (62) | Process/Task受授权通知 | 11/17/63/65 | ESRCH/EINVAL/EPERM；自进程及受控子Task；无任意进程广播/会话权限模型；SIGCHLD与故障独立路由 | W8/W9/I1 |
| `tkill` (200) | Process/Task受授权通知 | 11/17/63/65 | ESRCH/EINVAL/EPERM；自进程及受控子Task；无任意进程广播/会话权限模型；SIGCHLD与故障独立路由 | W8/W9/I1 |
| `tgkill` (234) | Process/Task受授权通知 | 11/17/63/65 | ESRCH/EINVAL/EPERM；自进程及受控子Task；无任意进程广播/会话权限模型；SIGCHLD与故障独立路由 | W8/W9/I1 |
| `sched_yield` (24) | Task让出 | 8 | 原生错误透传；不保证公平/实时优先级 | W4/R2 |
| `clock_gettime` (228) | 物理单调时钟 | 39/40 | EINVAL/EFAULT；仅CLOCK_MONOTONIC；PIT校准100Hz单CPU，无wall-clock | T1/R2 |
| `nanosleep` (35) | 绝对物理期限/可中断阻塞 | 39/40/72 | EINVAL/EFAULT/EINTR；相对REALTIME/MONOTONIC，绝对仅MONOTONIC；相对EINTR写rem | T1/W9 |
| `clock_nanosleep` (230) | 绝对物理期限/可中断阻塞 | 39/40/72 | EINVAL/EFAULT/EINTR；相对REALTIME/MONOTONIC，绝对仅MONOTONIC；相对EINTR写rem | T1/W9 |
| `mmap` (9) | 匿名arena/Segment/COW与硬件访问控制 | 13/14/15/16/41/68 | EINVAL/ENOMEM/EACCES；96页mmap+有限brk；PROT_NONE真禁用；无文件/SHARED映射；普通写后恢复不能绕过COW | A1/W6 |
| `munmap` (11) | 匿名arena/Segment/COW与硬件访问控制 | 13/14/15/16/41/68 | EINVAL/ENOMEM/EACCES；96页mmap+有限brk；PROT_NONE真禁用；无文件/SHARED映射；普通写后恢复不能绕过COW | A1/W6 |
| `mprotect` (10) | 匿名arena/Segment/COW与硬件访问控制 | 13/14/15/16/41/68 | EINVAL/ENOMEM/EACCES；96页mmap+有限brk；PROT_NONE真禁用；无文件/SHARED映射；普通写后恢复不能绕过COW | A1/W6 |
| `brk` (12) | 匿名arena/Segment/COW与硬件访问控制 | 13/14/15/16/41/68 | EINVAL/ENOMEM/EACCES；96页mmap+有限brk；PROT_NONE真禁用；无文件/SHARED映射；普通写后恢复不能绕过COW | A1/W6 |
| `arch_prctl` (158) | FS base/当前Task id | 7/38 | EINVAL/EFAULT；SET_FS/GET_FS；clear_child_tid真实登记由线程创建路径负责 | 默认/W4 |
| `set_tid_address` (218) | FS base/当前Task id | 7/38 | EINVAL/EFAULT；SET_FS/GET_FS；clear_child_tid真实登记由线程创建路径负责 | 默认/W4 |
| `exit` (60) | 线程原子出口或Process退出/托管管理器回收 | 17/18/20/49/50/54/66 | 不返回；fork子清理其holder；受托main先停驻再由管理器销毁；非POSIX所有atexit错误路径保证 | 默认/F1/W4/W6 |
| `exit_group` (231) | 线程原子出口或Process退出/托管管理器回收 | 17/18/20/49/50/54/66 | 不返回；fork子清理其holder；受托main先停驻再由管理器销毁；非POSIX所有atexit错误路径保证 | 默认/F1/W4/W6 |

## 不走上述 Linux 分派的符号

| 符号或入口 | 实际边界 | 状态、失败与预定见证 |
|---|---|---|
| `_start` / `__libc_start_main` | Prima五寄存器、版本锚点、可选交接、musl TLS/构造 | partial；默认/A2/W7b；原生create参数载体16/8项、单串255B，exec边界另见上表 |
| `pthread_create` / `pthread_join` / `pthread_detach` / `pthread_cancel` / `pthread_self` | musl、替换clone.s原生48、出口66、用户态futex表 | partial；W4/W5；有界栈、单CPU；异步取消不提供完整资源清理/任意程序安全保证 |
| `sigreturn` / musl `__restore_rt` | 适配层trampoline及原生65带token恢复；旧原生58仍有独立机制面 | partial；W9；不发Linux15进内核；不是任意ucontext/setcontext支持 |
| `malloc` / `calloc` / `realloc` / `free` | musl mallocng→brk/匿名mmap，最终同上 | partial；默认/A1/W7b；有限arena，非通用换页/过量承诺 |
| `printf` / `fprintf` / `fputs` / `fputc` / `putc` / `putchar` / `fflush` / `ferror` | musl stdio→writev及fd | partial；E1–E6/F1；不以可链接宣称所有格式/错误路径支持 |
| `fopen` / `fdopen` / `fread` / `fwrite` / `fseek` / `fclose` | musl stdio→open/fcntl/readv/writev/lseek/close | partial；V3/F1；fdopen APPEND必须已有，freopen/setvbuf全部参数未验收 |
| `opendir` / `readdir` / `closedir` | musl目录流→open/getdents64/close | partial；L1–L3；有限目录视图 |
| `vfork` | 显式musl汇编替换→`__syscall_ret(-ENOSYS)` | unsupported；F1拒绝用例 |
| `link` / `linkat` / `renameat2` / `poll` / `select` / `posix_spawn` / `dlopen` | 未登记的Linux调用/未提供装载路径 | unsupported；不保证libc全部包装均能初始化或统一失败，不扩充profile |
| `prima_native` / `prima_native_syscall0` / `prima_native_syscall3` / `prima_native_syscall4` | 原生直通，不翻译Linux号 | 仅实验诊断入口，不属于POSIX支持；不得输入Linux号 |
| `prima_exec_image` | 与execve共用交接/声明检查，输入内存ELF，原生69 | partial；W7b独立完整C子映像；不等同完整C文件路径装载 |
| `prima_libc_contract_v1_0` | 只读44B ELF note及crt强链接版本依赖 | partial；C/Rust解析器负例与构建检查；[静态护栏](../specs/2026-10-26-static-program-contract-design.md) |

完整libc.a中的普通纯计算符号只因可链接不进入上述支持集。汇编裸陷入白名单随
[musl输入锁](musl-snapshot.lock.toml)核查；当前八项锁定补丁已重建并通过归档审计，host5项通过；不扩大符号支持集。

## Rust 入口

R1/R2直接经原生Runtime，使用persona 0 / adapter 0.0。core/alloc、stdio/time/thread和所有
明确Unsupported/NULL/panic边界逐模块见[D6具体裁决表](../specs/2026-10-26-rust-d6-scope-decision.md)。
D6用户已选择A；本表不把C文件/网络/线程符号支持外推到Rust std。

## 号段、协议和检查边界

原生号1–72中52保留；源为prima-abi。新59–72及39/40语义变更的本轮host和具体验收程序结果见报告；逐参数、保留位、
授权与失败原子性见[接口说明](../../architecture/interfaces.md)和[进程边界契约](../specs/2026-10-25-process-boundary-followup-design.md)。
18三字输出保留，60五字认证元数据；64两阶段fork，65通用upcall，66栈退出，67word等待，68访问启停，
69exec交接，70/71设备IRQ，72物理期限。TTY 5EEB–5EED与网络CANCEL/FINISH均为用户态协议，非syscall。
QueueFull→EAGAIN、AuditFull→ENOSPC的区别必须保留。

xtask的台账回归只检查C分派入口均有行，不证明符号执行、错误全集或语义正确。
实际执行/容量/差分结果按版本及轮次记录于[工作报告](../reports/2026-10-25-stage0-5-gap-followup.md)。
