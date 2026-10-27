---
id: spec-stage5-profile-v2
kind: specification
authority: proposed-compatibility-contract
status: proposed
lifecycle: current
updated: 2026-10-27
---

# POSIX profile v2 提案

## 状态、版本和继承

承接[冻结 v0](2026-10-18-stage5-profile-v0.md)、[v1 delta](2026-10-20-stage5-profile-v1.md)、
[M20 汇总](../reports/2026-10-23-stage5-m20-compat-final.md)和
[兼容台账](../compat/compat-state.md)。本文件补齐 v2 提案，不修改 v0 冻结文本，
不构成发布、组织级批准、BLK-003 关闭或 S5-D1/X3 签署。

v2 的部分实现证据来自历史报告，当前源版本的整套重跑结果单独见
[本轮报告](../reports/2026-10-25-stage0-5-gap-followup.md)。本次矩阵按2026-10-27源码与统一技术证据修订，68轮及20项Linux参考通过；不继承旧版本PASS，也不构成发布/冻结。下表列 API 家族；
同一行多个符号共享列出的有界后端，不表示所有 POSIX 参数组合受支持。
没有证据的子项不从其它同名 API 推导为 supported。

## API 映射和边界

状态沿用 v0：supported 仅限冻结测试和参数边界，partial 必须读限制，unsupported 不承诺。
本提案保守使用 partial；原台账范围较窄的 supported 见证仍按原报告解释。

| ID | API / 后端 | v2 状态 | 测试与限制 |
|---|---|---|---|
| P5-ABI-01 | ABI check → 原生37 | partial | 1.0握手、保留位/拒绝例已纳入本轮host/机器见证；profile版本不改变原生ABI |
| P5-BOOT-01 | argc/argv/envp/auxv → ring3 创建栈 | partial | 默认、E1–E6、A2；16/8项，单串255B、总载体一页，无 AT_RANDOM |
| P5-PROC-01 | main/_exit/exit_group → 管理器或Process退出54 | partial | 默认/F1/W6；低8位正常退出，信号退出另行编码；fork子清理自身holder，托管出口先停驻后销毁 |
| P5-PROC-02 | getpid/getppid → 原生59 | partial | F1/W4/W6；线程返回共同Process id；无进程组/session |
| P5-PROC-03 | fork → 原生64预备/服务认证交接/发布 | partial | W6/N1；单线程COW、FPU、独立cap视图；用户选择受控继承A；活动TTY/net等待返回EAGAIN |
| P5-PROC-04 | waitpid/wait4 → 原生53 | partial | W6/W8/W9；四字节POSIX status、pid>0/-1、阻塞/WNOHANG、ECHILD/EINTR；无rusage |
| P5-PROC-05 | execve/原生内存exec → 静态ELF+69交接 | partial | W7/W7b；≤4 PT_LOAD、16 argv/envp各128B、CLOEXEC、新crt；声明拒绝返回ENOEXEC；完整C文件仍受rootfs容量限制；无PIE/解释器 |
| P5-IO-01 | write/writev → tty或VFS分片 | partial | 默认/F1；fd0..18，TTY多IPC不原子；队列满重试；非NULL坏指针不保证恢复，无通用EPIPE/非阻塞保证 |
| P5-IO-02 | read/readv → tty或VFS分片 | partial | F1/V1/I1；短传输即停，向量边界预检；非NULL坏指针仍可终止任务 |
| P5-FD-01 | close/dup/dup2/dup3/fcntl → 本地fd表/Process OFD holder | partial | F1/W6/W7b/N1；GETFD/SETFD/DUPFD/CLOEXEC/GETFL，SETFL仅APPEND不变；fork独立关闭、exec提交后关闭CLOEXEC |
| P5-FS-01 | open/openat → 认证会话/用户态VFS | partial | V1/V2/V3/W6；64B路径、16B名称、32节点/2920B内容区；子fork会话预备后发布；无多LibOS根隔离 |
| P5-FS-02 | lseek → OFD offset | partial | F1/V1；SET/CUR/END，dup共享；无pread/pwrite |
| P5-FS-03 | getdents64/opendir/readdir → VFS目录视图 | partial | L1–L3；8字节目录记录对齐，无稳定cookie承诺 |
| P5-FS-04 | stat/lstat/fstat/fstatat/statx → 元数据翻译 | partial | M1/V2/V3；有限字段，无真实时间戳/dev/mount同步语义 |
| P5-FS-05 | chdir/fchdir/getcwd → 适配层cwd | partial | M1/V2；词法路径、有限dirfd视图，不保证全部符号链接cwd语义 |
| P5-FS-06 | mkdir/mkdirat/unlink/unlinkat/rmdir → VFS树变更 | partial | D1/V1；普通打开文件延迟回收，非空目录等错误依现行映射；无硬链接/持久化 |
| P5-FS-07 | rename/renameat/symlink/symlinkat/readlink/readlinkat → VFS | partial | V2；同VFS、有限链接深度和容量；无renameat2/linkat |
| P5-FS-08 | chmod/fchmod/fchmodat/umask → 单用户mode | partial | V3；仅owner读写位/0777，非完整uid/gid/ACL模型 |
| P5-LIB-01 | malloc/free/realloc、格式化/字符串 → musl | partial | 默认/A1；有限brk/mmap arena，无SSP/安全熵保证 |
| P5-LIB-02 | fopen/fdopen/fread/fwrite/fseek/fclose → musl+fd | partial | V3/F1；fdopen追加要求既有APPEND，freopen/setvbuf/错误注入未完整验收 |
| P5-TIME-01 | clock_gettime/nanosleep/clock_nanosleep → 39/40/72 | partial | T1/W9；PIT校准单CPU物理100Hz时钟与阻塞期限，相对EINTR rem；无wall-clock；抽象44保持独立 |
| P5-MM-01 | brk/mmap/munmap/mprotect → 匿名arena+Segment | partial | A1/W6；96页mmap/有限brk，PROT_NONE真实禁用、恢复改权保留COW；无文件/SHARED映射 |
| P5-THREAD-01 | pthread_create/join/self/detach/cancel、clone → 48/66及musl | partial | W2/W4/W5；共享Process cap视图、TLS/FPU及栈出口；冻结属性子集；不承诺任意异步取消清理/完整pthread属性 |
| P5-THREAD-02 | futex WAIT/WAKE → WaitObject/原子word等待67 | partial | W3/W4；每Task waiter、物理期限和EINTR；无PI/robust/requeue |
| P5-SIGNAL-01 | kill/tkill/tgkill → 受授权原生通知65 | partial | W8/W9；自进程/受控子Task及SIGCHLD；无任意广播/完整权限/进程组/停止继续 |
| P5-SIGNAL-02 | libc sigaction/sigprocmask/sigpending/sigsuspend、sigreturn → 65 | partial | W9；64槽合并、8层现场、mask/siginfo、故障、EINTR/SA_RESTART；无altstack/实时排队；#AC无机器见证 |
| P5-TTY-01 | read/ioctl/tcgetattr/tcsetattr → COM1 IRQ/行规程 | partial | I1；canonical/raw/EOF、IRQ4、FIFO/认证序号/取消/确认、受授权ISIG通知；无VTIME/流控/job control/可变控制字符 |
| P5-NET-01 | socket/bind/getsockname/sendto/recvfrom/close → loopback UDP | partial | N1；4端点/4×64B报文、Process holder、逐Task事务/快照、EINTR/重启/取消；无NIC/TCP/DNS/poll |
| P5-RUST-01 | x86_64-prima core/alloc → 原生Runtime+应用allocator | partial | R1；工作target，单线程静态PIE，探针arena不单独回收allocation |
| P5-RUST-02 | restricted std → 原生stdio/time/thread PAL与内存API | partial | R2；stdout/stderr、Instant、sleep/yield；stdin/spawn=Unsupported、System=NULL；无lang_start/文件/网络/进程OS后端；D6用户已选A |

逐Linux入口、绕过分派的libc符号、错误与见证映射见[当前工作台账](../compat/stage5-working-api.md)。本轮统一技术验收已完成；每项实际执行及未覆盖参数须分范围读取报告。

## 明确不支持及确定失败

未映射的 Linux 调用经 C 适配层返回 ENOSYS；vfork 有专用拒绝，不能直接发 Linux58进入原生sigreturn。
多线程fork、文件映射、TCP/外部网络、进程组/session/ptrace/posix_spawn、动态链接不承诺。
libc信号、单线程fork、CLOEXEC、取消/分离与完整新C crt已有工作代码，限制见上表，不能继续标作一概未实现。
非NULL坏指针、任意异步取消、siglongjmp跨活动TTY/net等待及完整POSIX生命周期不在支持范围。
受限Rust std仅声明D6表中的具体API；未列OS API不承诺统一失败/初始化语义。
std::random的数据生成函数明确panic；HashMap地址种子fallback不是安全熵，未进入支持范围。

## ABI、persona 和符号版本护栏

原生号源仍是prima-abi：1–72中52空洞保留，Linux调用号只在适配层。
原生握手仍major1/minor0；profile版本与实验性功能协商分开。
[静态护栏](2026-10-26-static-program-contract-design.md)加入可检查persona/adapter ELF note、
crt强链接版本锚点与PRIMA_LIBC_1_0符号可见性脚本；C exec提交前拒绝不兼容声明。
这不建立内核persona、运行时人格选择或动态GNU符号版本解析；动态装载仍unsupported。
Rust vendor标识本身不作为persona；其原生程序声明与C静态POSIX声明分别验证。
发布/冻结v2、D3/D6及S5-D1/D7/X3仍待评审，不因矩阵或护栏代码存在而勾选阶段出口。

## 2026-10-26 未验收工作增量（不升级上述支持状态）

[进程边界工作契约](2026-10-25-process-boundary-followup-design.md)追加原生59–72及39/40实现变更；
其中65的libc mask/siginfo/返回现场、66/67的线程/futex、68保护、69跨映像和70–72 IRQ/期限均尚未统一验收。
5EEB–5EED为TTY取消/确认工作协议；I1/W9增加中断、重启、嵌套回复及睡眠rem回归代码。
本次表格登记当前实现边界，统一使用partial且不表示验收；这些代码不能继承旧1–58号范围的PASS。
D6及组织级profile/ABI/静态护栏决定仍须单独记录；全部代码完成后统一测试。

2026-10-26 Rust工作增量接通stdout/stderr、Instant与sleep/yield；stdin/spawn保留Unsupported，
System保持NULL。具体边界和D6待选项见[Rust裁决工件](2026-10-26-rust-d6-scope-decision.md)。
原表R2属于前批证据范围，不覆盖本次PAL；本增量尚未统一测试。


## 2026-10-27 统一技术证据（不发布profile）

前两节2026-10-26“未验收”保留为当时实现记录；本轮新增路径的实际结果以
[统一报告第19节](../reports/2026-10-25-stage0-5-gap-followup.md#19-最终统一验收与评审边界2026-10-27)为准。
workspace1016、lint、Rust两种Clippy、Lean构建、musl锁定归档/host、compat、
全部20项Linux参考及完整单次BIOS68轮通过。D6 A已由用户选定并完成R1/R2双轮。
I1/N1跨轮验收按明确时序不变量，失败记录不删除；Miri只覆盖authority模型21项；没有取得全包Miri或#AC机器证据。
本矩阵保留partial，静态persona/符号护栏的S5-D7适用性和profile冻结仍待
[最终范围评审](2026-10-26-stage5-release-decisions.md)。
