---
id: spec-stage5-profile-v2
kind: specification
authority: proposed-compatibility-contract
status: proposed
lifecycle: current
updated: 2026-10-25
---

# POSIX profile v2 提案

## 状态、版本和继承

承接[冻结 v0](2026-10-18-stage5-profile-v0.md)、[v1 delta](2026-10-20-stage5-profile-v1.md)、
[M20 汇总](../reports/2026-10-23-stage5-m20-compat-final.md)和
[兼容台账](../compat/compat-state.md)。本文件补齐 v2 提案，不修改 v0 冻结文本，
不构成发布、组织级批准、BLK-003 关闭或 S5-D1/X3 签署。

v2 的部分实现证据来自历史报告，当前源版本的整套重跑结果单独见
[本轮报告](../reports/2026-10-25-stage5-gap-closure-report.md)。下表列 API 家族；
同一行多个符号共享列出的有界后端，不表示所有 POSIX 参数组合受支持。
没有证据的子项不从其它同名 API 推导为 supported。

## API 映射和边界

状态沿用 v0：supported 仅限冻结测试和参数边界，partial 必须读限制，unsupported 不承诺。
本提案保守使用 partial；原台账范围较窄的 supported 见证仍按原报告解释。

| ID | API / 后端 | v2 状态 | 测试与限制 |
|---|---|---|---|
| P5-ABI-01 | ABI check → 原生37 | supported | 默认探针；major1/minor0，保留位必须0 |
| P5-BOOT-01 | argc/argv/envp/auxv → ring3 创建栈 | partial | 默认、E1–E6、A2；16/8项，单串255B、总载体一页，无 AT_RANDOM |
| P5-PROC-01 | main/_exit/exit_group → 管理器或 Process exit54 | partial | 默认、F1、W6；低8位，托管进程退出交接；fork fd清理仍受共享空间限制 |
| P5-PROC-02 | getpid/getppid → Task id/管理器标识 | partial | F1；getppid 尚非完整父子关系，线程 getpid 非标准 Process id |
| P5-PROC-03 | fork → 原生51/COW | partial | W6三C路径与Linux；单线程，MXCSR/x87继承；cap/close隔离未修 |
| P5-PROC-04 | waitpid/wait4 → 原生53 | partial | W6/W8；四字节status、阻塞/WNOHANG、ECHILD；无rusage，信号status仍128+sig编码 |
| P5-PROC-05 | exec_replace → 原生55 | partial | W7/W7b；静态ET_EXEC内存镜像、≤4 argv/64B；无envp/CLOEXEC/PIE/解释器；完整libc execve不支持 |
| P5-IO-01 | write/writev → tty或VFS分片 | partial | 默认/F1；fd0..18，多IPC不原子，无EPIPE/非阻塞/EINTR/取消保证 |
| P5-IO-02 | read/readv → tty或VFS分片 | partial | F1/V1/I1；短传输即停，向量边界预检；非NULL坏指针仍可终止任务 |
| P5-FD-01 | close/dup/dup2/dup3/fcntl → 本地fd表/OFD | partial | F1；F_GETFD/SETFD/DUPFD/CLOEXEC/GETFL，SETFL只接受APPEND不变；exec不执行CLOEXEC，fork关闭不隔离 |
| P5-FS-01 | open/openat → 用户态VFS | partial | V1/V2/V3；路径64B、名称16B、固定节点/内容容量；子fork任务回复授权仍缺 |
| P5-FS-02 | lseek → OFD offset | partial | F1/V1；SET/CUR/END，dup共享；无pread/pwrite |
| P5-FS-03 | getdents64/opendir/readdir → VFS目录视图 | partial | L1–L3；8字节目录记录对齐，无稳定cookie承诺 |
| P5-FS-04 | stat/lstat/fstat/fstatat/statx → 元数据翻译 | partial | M1/V2/V3；有限字段，无真实时间戳/dev/mount同步语义 |
| P5-FS-05 | chdir/fchdir/getcwd → 适配层cwd | partial | M1/V2；词法路径、有限dirfd视图，不保证全部符号链接cwd语义 |
| P5-FS-06 | mkdir/mkdirat/unlink/unlinkat/rmdir → VFS树变更 | partial | D1/V1；普通打开文件延迟回收，非空目录等错误依现行映射；无硬链接/持久化 |
| P5-FS-07 | rename/renameat/symlink/symlinkat/readlink/readlinkat → VFS | partial | V2；同VFS、有限链接深度和容量；无renameat2/linkat |
| P5-FS-08 | chmod/fchmod/fchmodat/umask → 单用户mode | partial | V3；仅owner读写位/0777，非完整uid/gid/ACL模型 |
| P5-LIB-01 | malloc/free/realloc、格式化/字符串 → musl | partial | 默认/A1；有限brk/mmap arena，无SSP/安全熵保证 |
| P5-LIB-02 | fopen/fdopen/fread/fwrite/fseek/fclose → musl+fd | partial | V3/F1；fdopen追加要求既有APPEND，freopen/setvbuf/错误注入未完整验收 |
| P5-TIME-01 | clock_gettime/nanosleep/clock_nanosleep → 原生Timer | partial | T1；逻辑tick，无完整wall-clock/信号中断语义 |
| P5-MM-01 | brk/mmap/munmap/mprotect → 匿名arena+Segment | partial | A1；MAP_PRIVATE/ANONYMOUS，PROT_NONE近似只读，无文件映射/硬件NX |
| P5-THREAD-01 | pthread_create/join/self、clone → 原生47/48 | partial | W2/W4/W5；共享leader cap_space，工作决定未组织级确认；无取消/分离/属性保证 |
| P5-THREAD-02 | futex WAIT/WAKE → WaitObject | partial | W3；无PI/robust/requeue |
| P5-SIGNAL-01 | kill → 原生56 | partial | W8；默认终止1..31，无完整权限/进程组/SIGCHLD通知 |
| P5-SIGNAL-02 | 原生handler/sigreturn57/58 | partial | W9；不等于libc sigaction，无mask/siginfo/EINTR/SA_RESTART |
| P5-TTY-01 | read/ioctl/tcgetattr/tcsetattr → COM1行规程 | partial | I1；canonical/raw/EOF，有读者轮询，无IRQ等待/VTIME/流控/job control |
| P5-NET-01 | socket/bind/getsockname/sendto/recvfrom/close → loopback UDP | partial | N1；仅IPv4环回，4端点/64B报文，无NIC/TCP/DNS/poll |
| P5-RUST-01 | x86_64-prima core/alloc → 原生Runtime+应用allocator | partial | R1；工作target，单线程静态PIE，探针arena不单独回收allocation |
| P5-RUST-02 | restricted std → 内存API及unsupported PAL | partial | R2；BTreeMap/内存Read trait，文件等OS API不支持；无lang_start或默认System allocator |

新增 R1/R2/A2 的运行结论必须读取本轮报告，不因本提案存在即判定通过。

## 明确不支持及确定失败

未映射的 Linux 调用经 C 适配层返回 ENOSYS；vfork 有专用拒绝，不能直接发 Linux58进入原生sigreturn。
libc rt_sigaction/rt_sigprocmask、完整execve、pthread取消/分离/属性、文件映射、TCP/外部网络、
进程组/session/ptrace/posix_spawn、动态链接不承诺。不能把内核原生机制的成功视为libc符号支持。
受限Rust std只验收metadata返回Unsupported与System返回NULL；其他OS API不进入支持契约，
不承诺全部函数的失败/初始化语义。std::random的数据生成函数明确panic；上游HashMap的地址种子fallback不是安全熵，未进入支持范围。

## ABI、persona 和符号版本护栏

原生号源仍是 prima-abi：1–58 中52空洞保留，Linux调用号只在适配层；本片不新增原生号。
原生握手仍major1/minor0；profile版本不改变ABI版本。符号调用链见[symbol-trace](../compat/symbol-trace.md)。
本profile只静态链接，不承诺共享库符号版本；ELF版本节点、运行时persona选择和动态装载均unsupported。
Rust target的vendor标识不是内核persona对象，也不能代替将来的双人格holder模型。
发布/冻结v2、D3/D6、S5-D1/D7/X3仍待组织级审查；不得只因补齐矩阵而勾选阶段出口。
