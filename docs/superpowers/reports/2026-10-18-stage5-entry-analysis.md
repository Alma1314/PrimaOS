# 阶段 5 准入分析：POSIX 源码兼容

**日期**：2026-10-18
**基线**：`eba03c4`（SPEC-151），主分支工作区原本干净
**性质**：设计与现状核对；无实现、无测试证据、无阶段退出条件签署

## 1. 结论

阶段 0–4 已签署，阶段 5 可以开始**范围和 ABI 准入评审**。当前尚不能按 DOC-002《基础库栈技术规格 v1.1》直接开发：它是外部候选，BLK-003 仍 Open，且其调用号、对象模型、项目命名、工具链版本与现有实现不一致。本轮不解除 BLK-003，也不勾选 S5-D/X。

阶段 5 的权威出口仍是跟踪页 S5-X1–X3：冻结的 POSIX profile 中每项 API 有语义映射、可重复测试和 supported/partial/unsupported 标记；选定**开源程序**可重新编译并通过冻结功能测试。DOC-002 §11 的自研 hello/fd/fork/abort 用例不足以单独满足 S5-X3。

随后对阶段 1–4 的实现回读、回归结果和新增前置见[复核报告](2026-10-18-stage1-4-implementation-review.md)；执行顺序见[修订计划](../plans/2026-10-18-stage5-revised-plan.md)。

## 2. 已有能力与实质缺口

| 领域 | 已有基础 | 阶段 5 缺口 |
|---|---|---|
| 用户入口与 ABI | `prima-abi` 定义实验性 `SYS_*` 1–36；`prima-runtime` 封装原生 syscall；内核 errno 已采用部分 Linux 数值 | 无冻结的 C ABI、版本握手、C 启动栈/auxv、crt 或 libc；`prima-runtime` 的 `API_MAJOR=0` 是源码 API 版本，不是内核 ABI 版本 |
| 任务与内存 | ring3 Rust ELF、`task_create/destroy`、`memseg_create/map/unmap/destroy`、COW 机制和用户态服务 | 无通用 POSIX 进程/线程、`fork/execve/wait4`、`mmap/mprotect` 语义、匿名映射策略及 W^X 端到端约束；现有 ELF 解析服务于 boot 映像装载，不构成 `PT_INTERP`/动态链接器 |
| IPC 与服务 | `send/recv`、阻塞接收直投、命名空间和基础日志/存储服务 | 无候选所述 endpoint `call/reply`、multi-wait、futex、文件描述符表、VFS/tty 协议；现有存储服务不等于 POSIX 文件系统 |
| 异常与时间 | 异常订阅、`SYS_TRAP/EXC_WAIT`、时间/定时器 | 无 POSIX 信号投递与 `sigreturn`、`clock_gettime/nanosleep` 契约 |
| 兼容验证 | `cargo xtask compat` 的单场景 Linux COW trace 有界差分 | 无 POSIX API 矩阵、同一源码编译的 Linux/Prima 功能差分、选定开源程序冻结测试 |

以上是源码和现有证据的边界判断；不能把可复用机制写成已实现的 POSIX API。

## 3. BLK-003 逐项评审建议

| 前置 | 当前判断 | 建议裁决方向 |
|---|---|---|
| 调用号、类型和命名 | 候选号段从 `0x000` 起分配，直接撞上现有 1–36；例如候选 `0x020` 为 `untyped_decant`，现有 32 为 `SYS_AUDIT`。候选 `Page/AS/Thread/Endpoint` 也并非当前 `MemorySegment/Task` 模型。 | 保留已占号和现有对象语义；从 37 起按实际机制追加。使用 Prima 命名。先冻结一份映射表，再决定是否需要生成器；不得逐字移植候选号表。 |
| 错误码与版本 | 现有 errno 为 Linux 数值子集，候选希望 `-1..-132` 全覆盖和扩展码。`NT_RUXOS_ABI` 名称尚无本项目承诺。 | 保留已用数值；逐 API 明确 POSIX 转译与原始错误追踪。为首次公开 C ABI 设计 Prima 版本握手；ELF note 的名字和校验时机随 loader 方案冻结。 |
| 直接 IPC | 内核有排队/阻塞接收直投，但没有候选的同步 `call/reply`。`≤250 ns` 属未实测的性能目标。 | 源码兼容首片复用现有消息机制；同步调用语义和性能优化分别评审。基准按环境记录，不把候选时延当准入前提。 |
| futex / wait_multi | 均未实现。 | 随真线程、`poll/select` 语义设计一起决定；单线程首片不预分配 syscall 或宣称支持。 |
| 信号 | 异常订阅并非 POSIX signal ABI。 | 明确最小 `abort`/故障/`sigaction` 范围、递送时点、屏蔽和恢复帧后，再加内核入口；先把未支持项标清。 |
| `fork/execve/wait4` | 当前 Task 生命周期和 COW 不提供 POSIX 父子进程、地址空间替换和等待状态。 | 先落通用用户态装载、进程状态和 fd 生命周期；单线程 fork/多线程 `EAGAIN` 只能作为待评审取舍。 |
| tty/console | 有日志服务和受控 I/O 端口驱动，无 tty API。 | 优先定义用户态 stdout/stderr 服务协议；内核直写控制台只覆盖明确的早期启动需求。 |
| 匿名页、`mmap`、W^X | `memseg_create/map` 可作底层基础，但不覆盖 flag、保护变更和文件映射。 | 按选定 profile 逐 flag 制定语义；W^X 需同时检查 ELF 段权限、映射和后续保护变更。 |
| `ld.so` 归属 | DOC-002 将最小动态链接列为阶段 5 P0；白皮书列在阶段 7 候选。 | 先以静态链接的**源码兼容**闭环验证阶段 5；动态链接是否提前需修改两处权威阶段定义后再排期。 |

DOC-002 自身也有需先消歧的文本：§21 D-15 建议 `mlock/munlock` no-op 成功，而走查 RED-1 写 `EUNIMPLEMENTED`；其工具链钉 `nightly-2026-09-15`，仓库当前钉 `nightly-2026-07-30`。两者都不能静默选用候选值。

## 4. 建议的最小启动顺序

1. **准入裁决**：从 S5-D1 写一个有限、版本化的 profile v0，逐 API 记录语义、后端、状态和测试；同时裁决 BLK-003 的号段/命名、最小 syscall 清单、错误码、ABI 版本、动态链接阶段归属。选定一个开源程序及其固定版本、功能测试和许可证。此步属于规格工作，不表示 BLK-003 自动 Closed。
2. **首个端到端切片**：让一个静态链接的 C 程序通过最小 crt/C 接口启动，借现有 IPC 和新增字节流服务输出，测试 `write`/退出状态/确定失败路径；与 Linux 上同源码的冻结用例比较。现有 ABI 没有 `exit`/等待状态接口，首片必须先定任务退出及状态回收的最小契约。仅将实际覆盖的 API 标 supported，其他 profile 行标 partial/unsupported。
3. **按依赖扩展**：文件与路径（用户态 fd 表 + 存储/命名空间协议）→ 时间/内存映射 → 线程/TLS/信号 → `fork/execve/wait4` → Rust `core/std` 和更多应用。每片同时更新语义矩阵、可重复测试和已知偏差。

首片优先复用 `prima-abi`、`prima-runtime`、现有服务和 xtask 的 QEMU/差分基础；仅在现有层无法表达所选 API 时新增内核机制。先定实际需要的 C ABI，再判断 IDL 生成器和 musl 移植边界的规模。

## 5. 此轮证据边界

本报告只回读 `Prima OS 实现阶段.md`、`Prima OS.md`、DOC-002 与走查记录、`prima-abi`、`prima-runtime`、内核 syscall/ELF 入口和现有用户服务。未修改代码、未运行构建/QEMU/测试；所有 S5-D1–D7、S5-X1–X3 保持未勾选，BLK-003 保持 Open。
