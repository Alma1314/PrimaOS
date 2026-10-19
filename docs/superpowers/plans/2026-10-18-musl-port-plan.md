# 阶段 5：musl 移植计划

**日期**：2026-10-18

**性质**：实施计划；M0 依赖盘点、M2 系统边界、M3 静态 musl 基座、M3b 匿名内存
（`malloc/free/realloc`）、M4 文件首切片、M5 sbase `echo` 首切片、M6 POSIX 时间首切片与
M7 完整本地 fd 表已完成（2026-10-18/19，见[盘点报告](../reports/2026-10-18-stage5-musl-m0-inventory.md)、[M2 报告](../reports/2026-10-18-stage5-musl-m2-boundary.md)、[M3 报告](../reports/2026-10-18-stage5-musl-m3-base.md)、[M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md)、[M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md)、[M6 报告](../reports/2026-10-19-stage5-m6-time.md)与 [M7 报告](../reports/2026-10-19-stage5-m7-fd-table.md)）。`printf`+`fflush` 输出路径已验收；sbase `echo` E1–E6 已双轮通过，但完整 auxv/任意 argv 仍受限；时间只覆盖 `CLOCK_MONOTONIC` 与轮询睡眠；fd 表支持 `dup`/`dup2`/`dup3` 与 `getpid`，仍无目录/多级路径/进程退出自动关闭。BLK-004 已关闭、BLK-003 仍 Open，S5-D/X 均未勾选。

**上位计划**：[阶段 5 修订计划](2026-10-18-stage5-revised-plan.md)；本计划细化其中的 ABI 准入、C 程序启动与 libc 工作。M1 的固定 C 探针[初始栈见证](../reports/2026-10-18-stage5-c-start-stack.md)与 [`main` 返回码观察](../reports/2026-10-18-stage5-main-return-status.md)已通过；完整 auxv、任意 argv 与 musl 通用入口仍待完成。

## 目标与边界

BLK-004 的 C ELF 容量门槛已由[QEMU 见证](../reports/2026-10-18-stage5-c-elf-qemu.md)关闭；M1 的固定探针启动栈与返回码已见证，通用 auxv/argv 与 musl 入口仍待实施。

目标是使选定版本的开源 C 程序用 Prima sysroot 重新编译、**静态链接**，在 QEMU 中通过冻结的功能测试。首个可运行程序应验证 C 启动、输出、退出和错误路径；完整 musl 构建与阶段 5 退出须分别验收，不能由一个 hello 程序代替。动态链接器、真实线程、信号、`fork/execve/wait4` 与网络按冻结 profile 排期，不能用成功返回或空实现冒充支持。

musl 上游[安装说明](https://git.musl-libc.org/cgit/musl/tree/INSTALL)明确以 Linux syscall 接口为运行前提；[移植资料](https://wiki.musl-libc.org/porting)主要讨论新 CPU 架构，并建议使用 libc-test。Prima 是另一套 OS 语义，因此本项目需要逐函数核对 musl 对进程、内存、fd、时间、线程及信号的依赖。DOC-002 所称“仅 5 个替换点”是待验证估计，不作为改动范围上限；尤其不能把替换 syscall 汇编桩等同于完成 libc 移植。

## 准入门槛

| 门槛 | 必须形成的决策或证据 |
|---|---|
| BLK-003：ABI | 原生 ABI 1.0 握手已用追加号 37 验证；仍需冻结首版 POSIX profile、C 调用约定、初始栈/auxv、错误码映射及后续调用号。保留 `prima-abi` 已用 1–37 号，只追加、不复用空洞；不照搬 DOC-002 的 `0x0xx` 号表和 RuxOS 命名。 |
| BLK-004：容量 | 现有服务全量存活时，能创建、运行、退出并回收额外 C 程序 Task；记录 Task、栈、Cap、Segment、消息和 Audit 峰值及容量错误。 |
| 目标程序 | 选定开源程序的版本、许可证、源码哈希、编译参数、依赖与冻结用例。自写 hello 仅作启动探针。 |
| 上游快照 | 以 DOC-002 提议的 musl 1.2.5 为候选，复核后锁定 tag/commit、归档哈希和许可文本；记录本地补丁与重放方法。版本在此步确定，不随系统包隐式变化。 |

准入评审可与依赖盘点并行；任何需要新公开 ABI 的实现都必须等相应契约冻结。`prima-runtime` 的 `API_MAJOR=0` 是 Rust 源码 API 版本，不能充当 C ABI 版本号。

## 工作包与验收顺序

| 步骤 | 实施内容 | 完成证据 |
|---|---|---|
| M0：依赖盘点 | **已完成（2026-10-18）**：锁定 musl 1.2.5（commit `0784374d…`），按首版 profile 生成“公开符号 → 内部函数 → syscall/服务 → Prima 机制”清单；标明 Linux 专有 `ioctl`、TLS、futex、loader 等假设。结论见[盘点报告](../reports/2026-10-18-stage5-musl-m0-inventory.md)。 | [compat-state](../compat/compat-state.md) 与[符号追溯表](../compat/symbol-trace.md)覆盖首版闭包；未决依赖均有归属与测试 ID；无“链接成功即支持”项。发现 P5-LIB-01 符号清单缺口，待 profile v1。 |
| M1：C 程序启动 | 制定 x86_64 C 入口契约：ELF `PT_LOAD` 权限、BSS 清零、栈对齐、`argc/argv/envp/auxv`、初始 fd/Capability、退出状态。先用无 libc 的 C/汇编探针跑通装载、`_start`、用户栈和 `_exit`。解决 BLK-004 后加入现有 boot image/manifest 与 QEMU 启动路径。 | ring3 QEMU 记录入口寄存器、栈布局、读写段权限、退出状态；错 ELF、坏指针、容量耗尽确定失败；旧阶段 1–4 事件不回归。 |
| M2：系统边界 | **首个纵向切片已完成（2026-10-18）**：追加 `SYS_TASK_SET_FS_BASE=38` + per-task FS base；C 适配层 `__syscall0..6`（Linux 号只在适配层翻译、原始负 errno）实现 `write`/`writev`/`close(1/2)`/`ioctl`/`set_tid_address`/`arch_prctl`/退出；`read` 与通用 fd 表明确未实现；Prima crt 骨架保存 `prima_ctx`。边界收尾已补齐 `exit(60)` 与 `__syscall4..6` 全参数透传（取消点的适配层路径就绪，musl 侧替换属 M3）；通用 fd→Capability 映射仍待后续切片。见[M2 报告](../reports/2026-10-18-stage5-musl-m2-boundary.md)。 | 裸 C 探针验证写入/短写（writev）、非法 fd、无授权、NULL 缓冲区、未知号与 FS base 往返；同源 Linux/Prima 结果矩阵；无内核 POSIX 对象或 Linux syscall 号泄漏；静态 PIE 在构建期校验。 |
| M3：静态 musl 基座 | **已完成（2026-10-18，含 M3b）**：SSE/FPU 契约（`CR4.OSFXSR/OSXMMEXCPT`、per-task `FXSAVE/FXRSTOR`）、锁定快照 + 两个可重放补丁（`syscall_arch.h`、`__set_thread_area.s`）、Prima crt 接 `__libc_start_main`、静态 `libc.a`/sysroot 构建脚本；M3b 由服务管理器预映射 64 页匿名 arena（栈同段），适配层实现 `brk`/匿名 `mmap`/`mprotect`/`munmap`，`malloc/free/realloc` 通过。`printf`+`fflush` 输出路径由 M4 验收。见[M3 报告](../reports/2026-10-18-stage5-musl-m3-base.md)。 | 静态 C 探针覆盖 `main` 返回/`exit`、`errno`、`snprintf` 格式化、环境变量、`malloc/free/realloc`（含 140000 字节独立 mmap）及失败路径；`readelf` 检查静态 PIE/RELATIVE-only；补丁可从锁定快照重放。 |
| M4：文件与路径 | **首切片已完成（2026-10-18）**：用户态 fd 3..6 生命周期、`openat/read/write/lseek/close` 的冻结子集、存储服务的单文件字节流、Namespace `f` 授权入口；`printf`+`fflush` 输出路径一并验收。无目录/多级路径/`dup`/`CLOEXEC`/共享 offset，逐项标 partial。见[M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md)。 | 每个支持 API 的 Linux/Prima 同源正反例差分：内容、偏移、错误码、句柄失效、句柄复用；失败时不泄漏 cap（授权解析 cap 立即归还）。**目录/多级路径仍待后续切片；完整 fd 表已由 M7 补齐。** |
| M5：开源程序与扩展 | **首切片已完成（2026-10-19）**：锁定 commit 的 sbase `echo` 源码用 Prima sysroot 重编译，E1–E6 六用例各双轮 QEMU 通过；随后按 profile 的实际需求扩展时间单位、`mmap/mprotect`、TLS/线程、信号、进程、网络；每一组先补机制/服务契约，再开启对应 musl 路径。 | 首切片证据见 [M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md)；构建命令、源码哈希、QEMU 结果和 Linux 参考可重复；API 行仍按实际覆盖逐项标 supported/partial/unsupported。 |

M1–M3 的最小闭环优先保证静态 C 程序可靠启动与退出。M4/M5 对应阶段 5 全部 profile 的后续交付；若选定程序需要线程或路径，必须先实现相应语义，不能跳过依赖。

**M0 结论（2026-10-18）**：echo 的 musl 闭包需要新增 per-task FS base（建议追加
`SYS_TASK_SET_FS_BASE=38`）；静态 PIE 必须用 Prima crt 跳过 `rcrt1` 的自重定位；stdio 实际走
`writev`，并需要 `close(1/2)`/`ioctl` 适配；`exit_group` 需要 `prima_ctx` 退出观察；P5-LIB-01
的符号清单需 v1 补齐（`putc`/`fprintf`/`vfprintf`/`perror`/`strlen`/`strncmp`/`exit`）；
E1–E6 另需 argv 注入。echo 无 TLS，预计使用 `builtin_tls`，不依赖 `mmap`。详见
[盘点报告](../reports/2026-10-18-stage5-musl-m0-inventory.md)。

**M2 切片（2026-10-18）**：per-task FS base 与追加号 `38` 已落地；C 适配层提供
`write`/`writev`/`close(1/2)`/`ioctl`/`set_tid_address`/`arch_prctl`/`exit(60)`/`exit_group`，
Linux 号只在适配层翻译并保持原始负 errno；`__syscall4..6` 全参数透传，取消点的适配层路径
就绪（musl 侧替换属 M3）；`read`/通用 fd 表仍未实现。C 探针在 Linux 与 QEMU
均通过同一 `main`（退出 37），静态 PIE 由 `xtask` 在构建期校验。内核未启用 SSE，Prima C
构建固定 `-mgeneral-regs-only`；M3 需裁决是否补 FPU 契约。详见
[M2 报告](../reports/2026-10-18-stage5-musl-m2-boundary.md)。

**M3 切片（2026-10-18）**：启用 x87/SSE 与 per-task FXSAVE/FXRSTOR；musl 1.2.5 快照
经两个补丁接入适配层，静态 `libc.a` 由 `prima-user/musl/build.sh` 以哈希缓存构建；
Prima crt 直接接 `__libc_start_main`（跳过 `rcrt1` 自重定位）；C 探针链接 musl 后
既有 h/i/xyz 与退出码 37 逐字节不变，另验 `snprintf`/`getenv`/errno。

**M3b 切片（2026-10-18）**：服务管理器把 C 入口栈扩为 72 页（8 页栈 + 64 页匿名
arena，同一 Segment）；适配层在固定 VA 上实现 `brk`（首 16 页）与匿名 `mmap`
（其余 48 页 bump），`munmap`/`mprotect` 为明确 partial；`malloc(64/4096/140000)`、
`free` 与 `realloc` 均通过。内核侧 `MAX_VSPACE_ENTRIES 192→256`、`MAX_FRAME_OWNERS
192→256`，并把归属表移入 `OWNER_ARENA` 以保持 `FrameRegistry` 的 32 KiB 按值预算。
`printf`+`fflush` 输出路径由 M4 验收；完整 auxv/argv 仍待后续。详见
[M3 报告](../reports/2026-10-18-stage5-musl-m3-base.md)。

**M4 切片（2026-10-18）**：存储服务扩为单文件字节流后端（内容驻留 SPEC-112 的
段，4 个句柄、服务端 offset）；C 适配层实现 fd 3..6、`openat/read/write/lseek/close`
与 3 字节分片；管理器以 `isolated_ns` 的 `f` 绑定做 open 授权并转发/回送。Linux 与
Prima 同源文件差分通过，`printf`+`fflush` 走 `writev` 输出 `M4STDIO`。容量按实测扩展
（`MAX_SEGMENT_FRAMES=128`、`MAX_VSPACE_ENTRIES=320`、`MAX_CAPABILITY_SLOTS=160`、
`MAX_AUDIT_EVENTS=2048`）；既有容量/IPC/Namespace 事件的变化逐条登记。详见
[M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md)。

**M5 首切片（2026-10-19）**：每个 echo 用例以 sbase `echo.elf` 替换 C 探针映像，并追加单页
ET_DYN 参数载体；服务管理器从固定偏移读取 argv、构造冻结初始栈。`prima_crt` 在 echo 构建下
先跑两次静默 `openat("f")/close`，保持 `NsWitness` 的 M4 授权解析计数不因换程序而放宽；tty
对 echo 字节走独立 tag，echo 退出后用排空屏障确认串口收尾。E1–E6 双轮通过，Linux/Prima
stdout 与退出码逐字节一致。详见 [M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md)。

**M6 时间首切片（2026-10-19）**：追加 `SYS_TIME_ENABLE=39`/`SYS_TIME_TICKS=40`——只编程
8254 通道 0 的自由运行计数，不配置 PIC、不打开 IF；适配层把 `clock_gettime(CLOCK_MONOTONIC)`、
`nanosleep`、`clock_nanosleep` 翻译为 10 ms 粒度的轮询。时间探针经托管程序通道双轮运行，
Linux/Prima 输出逐字节一致。中断驱动的 tick-only 方案在协作阶段触发 `#DF`，已放弃并记录。
P5-TIME-01 仍为 `partial`（轮询、无 `EINTR`、无墙上时钟）。详见
[M6 报告](../reports/2026-10-19-stage5-m6-time.md)。

**M7 fd 表切片（2026-10-19）**：适配层把 M4 的固定 fd 3..6 扩为本地 fd 表 3..10，
文件项指向服务端句柄并做引用计数；`dup`/`dup2`/`dup3` 共享打开描述与 offset，最后一个
引用才发 `CLOSE`；`getpid` 映射 `SYS_TASK_ID`。新探针使用 libc `open()` 时发现
musl/x86_64 走 `SYS_open=2`（非 `openat`），适配层补上同号翻译。Linux/Prima 同源输出
逐字节一致，F1 双轮 QEMU PASS。P5-FD-01/P5-PROC-02 仍为 `partial`，详见
[M7 报告](../reports/2026-10-19-stage5-m7-fd-table.md)。

## 构建与验证

- 将上游快照、补丁、Prima 适配层和生成的 sysroot 分开存放；构建产物不入 Git。宿主编译器只用于交叉构建，不得混入宿主 libc 头文件或库。
- 每个里程碑运行与改动对应的 host 单测、`cargo test --workspace`、`cargo xtask lint`；引导或 syscall 路径变化再运行 `cargo xtask test`，检查 BIOS 两次执行及既有 trace。C 探针的编译/运行/差分应由 `xtask` 自动化，不以手工 QEMU 截图作唯一证据。
- 从上游推荐的 [libc-test](https://wiki.musl-libc.org/porting) 选与冻结 profile 相符的用例，分别报告通过、失败、未运行和不适用；musl 仓库 `make` 成功不是运行时兼容证据。每次升级 musl 快照都重放补丁并重跑该矩阵。
- 阶段签署按 S5-X1–X3：每项 API 有语义映射和重复测试、状态标签完整、选定开源程序在冻结功能集通过。只完成 `libc.a` 或自写探针时，S5-D2 和 S5-X3 保持未勾选。

## 单独裁决

- **动态链接**：DOC-002 将 `ld.so` 列为阶段 5 P0，白皮书列为阶段 7 候选。先裁决阶段归属，再评审 `PT_INTERP`、重定位、共享库/TLS 与加载器测试；本计划不把它混进静态闭环。
- **线程和信号**：musl 内部可能在单线程场景使用 TLS、锁和取消点；M0 必须做真实调用图审查。对外 `pthread_*`、futex、POSIX signal 不能由当前 Task/Timer/Exception 接口直接推定支持。
- **时间和内存**：M6 已冻结 `CLOCK_MONOTONIC` 的 10 ms PIT 轮询子集并落地；`SYS_TIME_NOW` 仍是
  供 Timer 使用的抽象时钟，与 POSIX 时间分离。墙上时钟、`EINTR`、阻塞睡眠仍未冻结；
  `memseg_map` 也未覆盖 `mmap` flags 或 W^X 全路径。两者只能在语义与错误边界冻结后进入支持表。
