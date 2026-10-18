# 阶段 5：musl 移植计划

**日期**：2026-10-18

**性质**：实施计划；尚未移植 musl；BLK-004 已关闭、BLK-003 仍 Open，S5-D/X 均未勾选。

**上位计划**：[阶段 5 修订计划](2026-10-18-stage5-revised-plan.md)；本计划细化其中的 ABI 准入、C 程序启动与 libc 工作。M1 的固定 C 探针[初始栈见证](../reports/2026-10-18-stage5-c-start-stack.md)已通过，通用入口和退出状态仍待完成。

## 目标与边界

BLK-004 的 C ELF 容量门槛已由[QEMU 见证](../reports/2026-10-18-stage5-c-elf-qemu.md)关闭；下文 M1 的 POSIX 启动契约仍待实施。

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
| M0：依赖盘点 | 对锁定的 musl 源码，按首版 profile 生成“公开符号 → 内部函数 → syscall/服务 → Prima 机制”清单；标明 Linux 专有 `ioctl`、`procfs`、TLS、futex、signal、loader 和文件路径假设。对每项归类为复用、用户态适配、需新增机制或不支持。审查 DOC-002 的 5 点替换清单与构建开关是否真实可用。 | `compat-state.md` 和符号追溯表覆盖所有首版符号；每个未决依赖有归属、测试 ID 与负责人；无未分类的“链接成功即支持”项。 |
| M1：C 程序启动 | 制定 x86_64 C 入口契约：ELF `PT_LOAD` 权限、BSS 清零、栈对齐、`argc/argv/envp/auxv`、初始 fd/Capability、退出状态。先用无 libc 的 C/汇编探针跑通装载、`_start`、用户栈和 `_exit`。解决 BLK-004 后加入现有 boot image/manifest 与 QEMU 启动路径。 | ring3 QEMU 记录入口寄存器、栈布局、读写段权限、退出状态；错 ELF、坏指针、容量耗尽确定失败；旧阶段 1–4 事件不回归。 |
| M2：系统边界 | 建一个供 C 使用的 Prima syscall/服务适配层：最多六参数、原始负 errno、取消点边界、fd 到 Capability 的用户态映射、字节流 tty 服务。按冻结 profile 实现 `write`/`read`/`close`/退出等首批调用，错误集中转译并保留原始码供调试。不要把内核 Capability token 当成普通有符号返回值解码；`prima-runtime::decode` 的高位 token 特例需单独核对。 | 裸 C 探针验证参数寄存器、返回值、非法 fd、无授权、坏缓冲区和短写；同源 Linux/Prima 结果矩阵；无内核 POSIX 对象或 Linux syscall 号泄漏。 |
| M3：静态 musl 基座 | 以隔离的 sysroot 构建 musl `libc.a` 与所需 crt 对象；用最小可重放补丁接入 M2，逐项处理启动初始化、初始线程 TLS/`errno`、内部锁、`malloc` 所需内存服务和 `stdio`。不得用假的 `pthread_create` 成功绕过真实线程需求；不支持的路径应明确失败并在 profile 标记。 | 静态 C 探针覆盖 `main` 返回/`exit`、`errno`、`printf`/`fflush`、分配/释放、环境变量及失败路径；`readelf`/`nm` 检查 ELF、未解析符号和宿主 glibc 污染；补丁可从锁定快照重放。 |
| M4：文件与路径 | 在用户态建立 fd 生命周期和 `openat/read/write/close/lseek` 的冻结子集，接入真实字节流存储/目录服务。Namespace 只作授权入口，不能当作 VFS；`dup`、`CLOEXEC`、共享 offset 等须按选定 profile 明确支持或保留。 | 每个支持 API 的 Linux/Prima 正反例差分：内容、偏移、错误码、句柄失效、资源回收；失败时不留下 Cap 或服务端状态泄漏。 |
| M5：开源程序与扩展 | 以 M0 选定的开源程序做首次源码重编译和冻结功能测试。随后按 profile 的实际需求扩展时间单位、`mmap/mprotect`、TLS/线程、信号、进程、网络；每一组先补机制/服务契约，再开启对应 musl 路径。 | 构建命令、源码/工具链/sysroot 哈希、QEMU 结果和 Linux 参考结果可重复；API 行逐项标 supported/partial/unsupported，偏差有测试与解释。 |

M1–M3 的最小闭环优先保证静态 C 程序可靠启动与退出。M4/M5 对应阶段 5 全部 profile 的后续交付；若选定程序需要线程或路径，必须先实现相应语义，不能跳过依赖。

## 构建与验证

- 将上游快照、补丁、Prima 适配层和生成的 sysroot 分开存放；构建产物不入 Git。宿主编译器只用于交叉构建，不得混入宿主 libc 头文件或库。
- 每个里程碑运行与改动对应的 host 单测、`cargo test --workspace`、`cargo xtask lint`；引导或 syscall 路径变化再运行 `cargo xtask test`，检查 BIOS 两次执行及既有 trace。C 探针的编译/运行/差分应由 `xtask` 自动化，不以手工 QEMU 截图作唯一证据。
- 从上游推荐的 [libc-test](https://wiki.musl-libc.org/porting) 选与冻结 profile 相符的用例，分别报告通过、失败、未运行和不适用；musl 仓库 `make` 成功不是运行时兼容证据。每次升级 musl 快照都重放补丁并重跑该矩阵。
- 阶段签署按 S5-X1–X3：每项 API 有语义映射和重复测试、状态标签完整、选定开源程序在冻结功能集通过。只完成 `libc.a` 或自写探针时，S5-D2 和 S5-X3 保持未勾选。

## 单独裁决

- **动态链接**：DOC-002 将 `ld.so` 列为阶段 5 P0，白皮书列为阶段 7 候选。先裁决阶段归属，再评审 `PT_INTERP`、重定位、共享库/TLS 与加载器测试；本计划不把它混进静态闭环。
- **线程和信号**：musl 内部可能在单线程场景使用 TLS、锁和取消点；M0 必须做真实调用图审查。对外 `pthread_*`、futex、POSIX signal 不能由当前 Task/Timer/Exception 接口直接推定支持。
- **时间和内存**：当前 `SYS_TIME_NOW` 是 PIT tick，尚无 POSIX 时间单位；`memseg_map` 也未覆盖 `mmap` flags 或 W^X 全路径。两者只能在语义与错误边界冻结后进入支持表。
