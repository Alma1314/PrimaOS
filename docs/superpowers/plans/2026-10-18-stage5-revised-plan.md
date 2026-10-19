# 阶段 5 修订计划：从原生机制到 POSIX 源码兼容

**日期**：2026-10-18
**基线**：`master` `eba03c4`；依据[阶段 1–4 实现复核](../reports/2026-10-18-stage1-4-implementation-review.md)及[准入分析](../reports/2026-10-18-stage5-entry-analysis.md)
**状态**：执行计划；片 B 的 C ELF 容量门槛已关闭，片 A 的[原生 ABI 1.0 握手](../reports/2026-10-18-stage5-native-abi-check.md)和片 C 的[固定 C 初始栈](../reports/2026-10-18-stage5-c-start-stack.md)、[同源 `main` 返回码](../reports/2026-10-18-stage5-main-return-status.md)及片 C 的[tty 字节流 `write(1/2)`/`_exit`](../reports/2026-10-18-stage5-c-tty-write.md)已获见证；M5 首切片的 [sbase `echo` E1–E6](../reports/2026-10-19-stage5-m5-sbase-echo.md)已双轮通过，S5-X3 证据已具备；M6 的 [POSIX 时间首切片](../reports/2026-10-19-stage5-m6-time.md)把 `P5-TIME-01` 推进到 `partial`（PIT 轮询、10 ms 粒度）。POSIX profile 的其余行、通用 fd/文件/进程语义仍待完成；S5-D1–D7/X1–X2 未完成，S5-X3 待组织级复核。BLK-003 仍 Open；DOC-002 仍为外部候选。ISSUE-IPC-001 已在[阶段 2 签署后整改](../reports/2026-10-18-stage2-ipc-review-remediation.md)中关闭。

## 1. 目标与取舍

权威目标是白皮书 §8.2/§15 与跟踪页 S5：**选定开源 C/POSIX 程序重新编译后，在 Prima 上保持冻结功能语义**。每个进入 profile 的 API 都必须有实现映射、状态标记、可重复测试和 Linux 参考结果；阶段退出时选定开源程序通过冻结测试。阶段 6 的“未修改 Linux 静态 ELF”不在此阶段冒充完成。

DOC-002 的 musl/IDL/`ld.so`/Rust `std`/信号/`fork`/250 ns IPC 是候选分解，不直接给当前 `SYS_*` 号表或内核对象赋义。计划从**静态链接的源码兼容**纵向切片开始；`ld.so` 是否纳入阶段 5 P0 与白皮书阶段 7 的冲突，须在 BLK-003 裁决时修改权威阶段定义后才可安排。不能因为首片静态通过就勾选 S5-D2/D6/X3。

## 2. 阶段 1–4 输入与必须新增的边界

| 可复用输入 | 仍需新建或裁决 |
|---|---|
| 初始 Authority、逐帧归属、自建页表、用户 ELF 程序段 | 用户程序的通用 ELF 装载、C 入口栈/auxv、段权限与 W^X；不得把 kernel image 的 `ISSUE-AUTH-001` 分叉延伸到新程序 |
| generation Capability、Task/Segment、1 个 `u64` + 最多 2 Cap 的 IPC、4 页共享段示例 | Process/Thread/退出状态、完整字节流传输、资源容量（BLK-004）；带 Cap 阻塞接收已修复并验证（ISSUE-IPC-001） |
| Capability Namespace、Timer、Audit | 用户态 fd 表、文件树/目录、时间单位与时钟语义、长时间运行时 Audit 容量策略 |
| log/ns/store 与服务管理器 | tty 字节流、文件读写、基础网络服务；现有 log/store 的 `u64` 状态不是这些 API 的后端 |

## 3. 顺序与每片验收

| 片 | 工作与依赖 | 验收证据 |
|---|---|---|
| A 准入冻结 | 冻结 profile v0 的 API、具体参数/错误/partial 边界；逐项裁决 BLK-003（保留已用 1–37，仅追加新号；Prima 命名；错误码与 C 入口；动态链接阶段归属）；选定开源程序的版本、许可证与冻结用例。原生 ABI 1.0 握手已完成。 | S5-D1 的版本化 profile [v0](../specs/2026-10-18-stage5-profile-v0.md) 已冻结（待组织级确认勾选）；每行有 planned/supported/partial/unsupported、Linux 参考行为、Prima 后端和测试 ID。未评审的 DOC-002 条目不得混入 ABI。 |
| B 容量 | 解决 BLK-004，保证全量现有服务启动后还可建、运行、退出、回收新 Task；沿用已修复的带 Cap 直投作为 IPC 基线。 | host 状态机 + ring3 QEMU 正反例；峰值容量/错误语义记录；旧阶段 1–4 事件不回归。通用 Cap IPC 的 supported 范围仍限已实现的消息大小与 2 Cap 上限。 |
| C 首个 C 程序 | 用当前用户 ELF 机制构建最小静态 C crt/syscall 适配；定义 ABI 版本、退出状态与观察者；接入新 tty 字节流服务，先覆盖 `_start`、`write(1/2)`、`_exit`。 | 同一 C 源码在 Linux/Prima 编译运行；输出和退出状态与冻结预期一致；非法 fd/坏指针确定失败；不宣称已有 libc/文件系统。 |
| D 文件与路径 | 在用户态建立 fd 生命周期、`openat/read/write/close/lseek` 的冻结子集、目录/路径服务与真实字节存储；利用 Namespace 做授权入口，不把它当文件树。 | 每个支持的 API 有 Linux/Prima 差分；`dup`/`CLOEXEC`/目录等未实现者标 partial/unsupported；服务崩溃/失效句柄有确定行为。 |
| E 内存、时间、线程 | 有限 `mmap/munmap/mprotect`、单调时间映射；设计 Process/Thread 与共享地址空间、TLS、mutex/cond/wait 对应机制。`memseg_protect`、WaitObject 目前只有规范文本，须独立实现与验证。 | 每个承诺 flag、权限转移、超时和同步边界都有成功/失败 QEMU 测试；W^X 由 ELF 装载、map 和 protect 三路径共同强制。 |
| F 网络与库栈 | 冻结“基础网络子集”的地址族、协议和功能测试，建设用户态网络服务/传输；按 profile 所需移植 libc，补 Rust `core/std` 的实际支持矩阵。 | 网络 API 不可用时明确 unsupported；S5-D3/D6 只能按实际程序与测试勾选。自研 C 冒烟仅是中间证据，不能替代开源程序。 |
| G 出口 | 在固定编译器/sysroot、Linux 参考环境与 Prima QEMU 上重跑完整 profile、差分矩阵和选定开源程序；记录 API 状态、偏差、版本、产物哈希。 | S5-X1 每项映射和重复测试；S5-X2 每项状态；S5-X3 开源程序源码重编译与冻结功能集 PASS。余项未通过则阶段继续进行中。 |

片 B 的容量门槛已由[首个 C ELF 的 QEMU 见证](../reports/2026-10-18-stage5-c-elf-qemu.md)关闭；片 C 的最小 POSIX 风格入口、`write(1/2)` 与退出状态已由[独立报告](../reports/2026-10-18-stage5-c-tty-write.md)见证；按 `PT_LOAD` 权限的私有写段/BSS 与 W^X 已由[装载报告](../reports/2026-10-18-stage5-ptload-wx.md)见证；musl 的 M0 依赖盘点已完成（见[盘点报告](../reports/2026-10-18-stage5-musl-m0-inventory.md)）；M2 首个系统边界切片已完成（追加 `SYS_TASK_SET_FS_BASE=38`、C 适配层与同源探针，见[M2 报告](../reports/2026-10-18-stage5-musl-m2-boundary.md)）；M3 静态 musl 基座与 M3b 匿名内存已完成（SSE/FPU 契约、可重放补丁、Prima crt 接 `__libc_start_main`、探针链接 `libc.a`、`malloc/free/realloc`，见[M3 报告](../reports/2026-10-18-stage5-musl-m3-base.md)）。M4 文件首切片已完成（单文件 `f` 的 fd 生命周期、`openat/read/write/lseek/close` 冻结子集、Namespace `f` 授权与 `printf`+`fflush` 输出，见 [M4 报告](../reports/2026-10-18-stage5-musl-m4-file.md)）；M5 首切片已完成（sbase `echo` E1–E6 在 Prima 双轮通过、与 Linux 参考逐字节一致，见 [M5 报告](../reports/2026-10-19-stage5-m5-sbase-echo.md)）；M6 时间首切片已完成（`CLOCK_MONOTONIC` + 轮询睡眠，`P5-TIME-01` partial，见 [M6 报告](../reports/2026-10-19-stage5-m6-time.md)）；目录/多级路径/完整 fd 表、完整 auxv/任意 argv 与真实进程退出仍未完成；匿名 arena 不是通用 `mmap` 承诺，PIT 轮询不是阻塞时钟服务。

片 C 是首个最小纵向证明，D–G 才构成完整阶段交付。任何新增内核机制先对照 COSIR 与代码现状；语义变化按跟踪规则先改规范，再改实现与证据。

## 4. 首版 profile 的冻结清单

片 A 必须给下列类别写**逐项**矩阵，不能以类别名替代测试。建议的最小候选为：进程启动/退出与 `getpid` 映射；`read/write/openat/close/lseek`；`mmap/munmap/mprotect` 的指定 flag；目录枚举/路径错误；`clock_gettime/nanosleep`；`pthread_create/join`、mutex/cond 的指定语义；`socket/bind/sendto/recvfrom/close` 的一个明确地址族与协议（可先评审 loopback，但不得把纯 IPC 称为网络）。具体范围及选定开源程序共同冻结；若某类别最终只标 unsupported，必须解释 S5-D3 如何满足，不能据此勾选。

每行至少包含 `API | Linux 参考语义 | Prima 后端/权限 | 状态 | 已知偏差 | 正反测试 ID | 证据命令`。只有真实实现和重复测试后才能从 planned 改为 supported；partial 必须列出保留的边界。差分既比较成功结果，也比较失败码、数据、顺序和资源回收；现有 `cargo xtask compat` 只是 COW 单场景先例。

musl 的依赖盘点、静态链接闭环和逐步验收见 [musl 移植计划](2026-10-18-musl-port-plan.md)。M2 已将 `write`/`writev`/`close(1/2)`/`ioctl`/`set_tid_address`/`arch_prctl`/`exit(60)`/`exit_group` 接入适配层，并透传 `__syscall4..6` 全部参数；M3 已用锁定快照构建静态 `libc.a`、启用 per-task SSE 状态并把探针链接到 musl（`snprintf`/`getenv`/errno 已验）；M3b 以栈同段的 64 页匿名 arena 支撑 `malloc/free/realloc`。M4 首切片已把 fd 3..6、`openat/read/write/lseek/close`（单文件 `f`）与 `printf`+`fflush` 输出接入并通过同源差分；目录/多级路径、`dup`/`CLOEXEC`、完整 fd 表仍未实现。匿名 arena 是 partial 的 `mmap` 适配，不得据此升级 profile 状态。

## 5. 暂缓裁决

- `fork/execve/wait4`、完整 POSIX signal、`ld.so` 和 Rust `std` 全量覆盖仍在 S5-D2/D6 候选范围内；各自必须先决定阶段归属与所需 Process/Thread、loader、VFS 语义。不能把单线程 `pthread` 假实现或返回 `ENOSYS` 算成线程交付。
- DOC-002 的 `0x0xx` 号表、`NT_RUXOS_ABI`、`x86_64-ruxos-none`、`nightly-2026-09-15` 不直接使用。仓库原有号 1–36 保持，37 用于原生 ABI 握手；工具链仍为 `nightly-2026-07-30`。
- 性能目标（如 IPC ≤250 ns）仅在固定 `(QEMU, guest CPU, host CPU)` 上实测后定门槛；不得阻断语义正确的首片。
