---
id: compat-stage6-linux-profile-v1
kind: compatibility
authority: frozen-bounded-syscall-profile
status: frozen-with-partial-coverage
lifecycle: current
updated: 2026-11-06
---

# Linux LibOS profile v1（有界冻结）

## 状态与边界

本台账对应`prima-user/src/linux.rs`与`linux_main.rs`的用户态实现。[统一报告](../reports/2026-11-02-stage6-process-and-differential.md)记录同一ELF在pinned Linux与Prima的11项双轮差分通过；最终普通完整回归通过，按本页有界范围冻结。
它不替代[阶段5 profile v2](../specs/2026-10-25-stage5-profile-v2.md)，不宣称完整Linux或POSIX兼容，
v1冻结范围以上表边界与命名差分观察为准。当前阶段与退出条件只查[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。
上游为[D12白名单](stage6-static-whitelist.md)、[停车/装载协议](../specs/2026-11-01-stage6-managed-launch.md)
、[受管进程契约](../specs/2026-11-02-stage6-gate-process.md)和[阶段6–7计划](../plans/2026-10-27-stage6-7-refinement.md)。

运行模型：一个原生ring3装载/LibOS服务；Process间独立root，同组至多8个显式受管线程共享root，全部陷入先交给Gate。
服务端按完整Task cookie选择状态。Linux号、errno、flags、fd、utsname、stat与dirent布局只在用户态解释。
大ELF以原生只读映像内的原字节载体提供，普通RAM VFS的2920字节内容区不扩容；execve只登记/busybox、/bin/busybox与/bin/M4-child，非任意大文件VFS执行。

## 单次启动的资源边界

[RV-1复核](../reports/2026-11-06-stage6-implementation-review.md)发现：每次Gate交付均铸造请求根能力，
消耗内核共享的1280项只追加能力历史；答复、销毁或drop不回收历史。至少一个Gate根也占历史，
因此单次启动累计交付至多1279次；其它能力创建会进一步减少可用次数，**1279不是保证可完成的调用数**。
该限制跨Gate、家族与案例累计，不因新建Process或Gate而重置；并发上限仍是每家族4进程×8线程。
历史耗尽时`gate_wait`返回原生ENOSPC；当前服务的`unwrap`终止服务，客户端按挂接时的孤儿值终结，
不提供无限运行或耗尽后继续服务保证。现有命名差分每案例独立启动，不证明长时运行。
可回收历史/独立计数器的设计留待阶段7后继，不扩大v1范围；本次[整改](../reports/2026-11-06-stage6-review-remediation.md)只登记此既有边界。

## 有界变体与状态裁定

下表各Linux入口均裁定为`partial`，只覆盖声明的范围及下节命名观察；不把有限参数矩阵外推为完整Linux syscall。`supported`只用于D12八个固定案例的通过判据；未实现能力列为`unsupported`。本版已冻结；新增变体须发布v2。

| Linux x86_64号 | 接线 | 状态与当前边界 |
|---|---|---|
| 0/1/20 | read/write/writev | `partial`；stdin为固定有限非终端流；stdout/stderr为串口编码输出；单次代理≤4096，writev≤16项 |
| 2/257/3 | open/openat/close | `partial`；复用现有VFS和OpenFiles；只读、DIRECTORY、CLOEXEC、LARGEFILE及NONBLOCK；有限RAM文件/目录不阻塞，OFD保留查询flags；最多16 fd；关闭标准流后再open的变体ENOSYS |
| 5/262 | fstat/newfstatat | `partial`；x86_64 144字节布局；EMPTY_PATH、SYMLINK_NOFOLLOW；合成inode/时间与有限RAM VFS元数据；不是全字段Linux对等 |
| 8/217 | lseek/getdents64 | `partial`；有界VFS游标；dirent64以8字节对齐，保留整条记录与NUL；只在代理输出成功后推进目录游标 |
| 9/11 | mmap/munmap | `partial`；仅无提示地址的PRIVATE+ANONYMOUS（可加MAP_STACK）、NONE/R/RW、offset=0；每段≤149页、至多32个映射；munmap仅完整已登记段 |
| 10 | mprotect | `partial`；NONE用原生68禁用用户访问；R/W由41改权后恢复用户位；匿名X拒绝EACCES，NX与逻辑权限同步 |
| 12 | brk | `partial`；固定初始堆基址、单调增长≤149页；资源失败/缩小时返回旧break；不是libc brk包装的0/-1约定 |
| 16 | ioctl | `partial`；非终端流返回ENOTTY；没有成功终端ioctl实现 |
| 39/186 | getpid/gettid | `partial`；组leader/current Task id；生命周期身份仍由完整cookie索引 |
| 24/56 | sched_yield/clone | `partial`；有界共享VM/FS/FILES/SIGHAND/THREAD/SYSVSEM，TLS/tid写出/clear_tid；其它clone变体ENOSYS；静态glibc pthread双轮通过 |
| 60/231 | exit/exit_group | `partial`；非leader只清理自身；leader先退停车至最后线程；组退出先子线程后leader/root/fd/匿名段；非原子全组撤销主张 |
| 13/14/15/62/130/234 | 信号与rt_sigreturn | `partial`；组内kill/tgkill、共享动作及逐线程mask/pending、Linux帧/GPR/FXSAVE、sigsuspend与futex真实EINTR/SA_RESTART；Ready在Gate边界交付；备用栈/实时队列未实现 |
| 63 | uname | `partial`；Linux/Prima声明的六个65字节字段；默认uname应输出Linux；版本字段不伪装成参考内核 |
| 72/79/89 | fcntl/getcwd/readlink | `partial`；有界GETFD/SETFD/GETFL，固定cwd=/，仅/proc/self/exe虚拟路径；其它变体不冒称成功 |
| 102/104/107/108 | uid/gid/euid/egid | `partial`；单身份0；无账号、身份切换或多用户权限主张 |
| 158/218 | arch_prctl/set_tid_address | `partial`；逐线程FS SET/GET、低半区地址检查；逐线程clear_tid退出清零并按非PRIVATE键唤醒 |
| 202 | futex | `partial`；WAIT/WAKE、WAIT_BITSET/WAKE_BITSET，PRIVATE键区分；每组至多8等待；真实Delivered等待；跨独立Process共享、非零timeout未实现；普通信号可中断，SA_RESTART恢复原futex陷入后重新比较 |
| 57/59/61/110 | fork/execve/wait4/getppid | `partial`；每家族最多4进程、仅单线程fork/exec；独立COW、共享OFD游标、独立CLOEXEC；只登记原始ELF路径；wait4仅-1/实际pid与WNOHANG，无rusage、SIGCHLD或信号打断wait |
| 302 | prlimit64 | `partial`；仅本进程RLIMIT_STACK只读查询，当前八页栈=32768字节；设置和其它资源ENOSYS |

未实现的号或合法但未实现的变体得到`-ENOSYS`，原始号/参数与终结值经生产Gate链留在内核审计，
服务另输出含cookie/number/-38的记录。非法参数、坏地址、无权限和资源耗尽保留对应错误，不统一伪装成ENOSYS。
`read`输出失败恢复VFS游标；目录输出失败不推进游标。代理复制与COW失败原子性复用Gate契约。


| 入口组 | 实际差分观察 |
|---|---|
| 0/1/20/2/257/3/5/262/8/217/72 | 六applet及M3：零长度、坏fd/指针、读取字节/游标、CLOEXEC/GETFL选定位、stat大小、目录alpha/beta成员；M4验证共享OFD及exec关闭fd |
| 9/10/11/12 | M3：匿名8192B清零/写入、R→NONE→RW与内容保留、坏对齐、整段munmap、brk查询及增长一页 |
| 13/14/15/62/130/234 | P1：阻塞/暂时mask、siginfo、rt_sigreturn/GPR/XMM/DF；M4：futex真实EINTR与SA_RESTART；M3：忽略动作/不可捕获信号拒绝/组内信号0 |
| 24/56/158/186/202/218 | P2及M4：两个真实Task独立FS、clone/tid/clear_tid、先wake后wait、比较不等/错对齐/坏指针/零时限、真实等待及唤醒；BITSET完整等待组合未做差分签署 |
| 57/59/61/110 | M4-fork：父子返回/COW/共享OFD/exec失败保留/成功CLOEXEC与argv/env/wait23；第二子进程EFAULT后ECHILD；M3正getppid谓词 |
| 16/39/63/79/89/102/104/107/108/302 | M3：ENOTTY、正pid且tid=pid、uname.sysname、cwd=/、readlink存在、身份0、RLIMIT_STACK=32768；其它utsname/stat/path字段只按声明值，不全字段比对 |
| 60/231及默认未知号 | 全案例退出；P2/M4线程退出、M4进程退出；P1/M3未知2147483647的-38与六个原始参数、cookie及唯一终结经89读回核对 |

`partial`还包括：PID/tid使用Task编号，不提供完整pid命名空间；多代/孤儿收养、多未回收子进程的PID保留与复用场景未验证。
`unsupported`包括下文未实现项及其它未实现号/变体；前者不因某个兼容占位返回而获得supported状态。
新变体或更宽主张须补同源差分并发布v2，不能默默扩大v1。

## 明确未实现（unsupported）

其它clone进程变体、任意VFS大文件exec、rusage/job control/SIGCHLD及wait信号打断、XSAVE/AVX/CET、
备用信号栈/实时信号队列、robust-list与rseq、网络、文件写入、动态链接、磁盘持久化及完整/proc。
上述号不下落至Prima原生分派。NX在通用map/protect及私有ELF同步，首请求实际拒绝NX栈作为PC；不宣称所有页或全部异常组合的硬件审计。
v1未知号默认`-38`不是完成这些功能的替代品。

## 验收矩阵与冻结门

```sh
PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo run -p xtask --features linux-probe -- build-linux
PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo run -p xtask --features linux-probe -- test-linux
```

第一条只打包，不执行程序。第二条先验证D12 SHA-256和服务内嵌原始ELF，再用pinned Linux 6.8.0-31 guest
运行同一制品的true、echo hello、cat /fixture.txt、ls /fixturedir、uname、sort；Prima侧每项独立启动固定原生服务/Gate并新建家族。
再复制同一P1/P2裸syscall ELF到guest；另有静态glibc pthread、fork/exec/wait及M3逐调用矩阵，共11项；顺序、退出0、stdout/stderr逐字节，以及通过89读回的原始内核审计与服务失败记录/计数由宿主校验。Prima每案例独立启动、每案例双轮；不放宽每次30秒门限。
参考侧仅指定32KiB栈软硬限制和C locale，不使用宿主运行结果替代pinned guest。

P1/P2的[构建入口](../reports/2026-11-01-stage6-context-signals-threads.md)保留；同一P1/P2、静态pthread、fork/exec/wait与M3已双轮实际通过。失败、修复和原始制品哈希见[统一报告](../reports/2026-11-02-stage6-process-and-differential.md)；最终整体回归已通过。
M3按明确命名谓词比较变化的pid/地址、选定结构字段、目录成员及路径存在性；稳定返回值/errno逐值输出，不声称全字段/全参数空间等价。
阶段勾选须以整体复核及[阶段跟踪](../../../Prima%20OS%20实现阶段.md)为准。

语义参考为上游维护者的[arch_prctl](https://man7.org/linux/man-pages/man2/arch_prctl.2.html)、
[brk（含裸syscall返回约定）](https://man7.org/linux/man-pages/man2/brk.2.html)、
[getdents64](https://man7.org/linux/man-pages/man2/getdents.2.html)与
[prlimit](https://man7.org/linux/man-pages/man2/prlimit.2.html)；它们不证明本实现已符合完整Linux。
