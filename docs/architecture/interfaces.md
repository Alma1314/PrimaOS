---
id: guide-interfaces
kind: reference
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-27
---

# 接口、号段与版本边界

本页提供定位，不创建新的 ABI。源定义见[prima-abi](../../prima-abi/src/lib.rs)、[原生传输接口](../../prima-runtime/src/transport.rs)、[C 适配层](../../prima-user/c/prima_syscall.c)。

## 四种契约分开读取

| 契约 | 来源 | 作用 |
|---|---|---|
| 抽象操作 | COSIR 原语规范§4–15/§21 | 参数、权限、迁移、错误与不变量 |
| Prima 原生机器接口 | prima-abi::syscall、kernel/syscall | 当前 x86_64 调用号与分派 |
| 原生源码 API / 服务协议 | prima-runtime API_MAJOR/API_MINOR、protocol | safe 封装与服务协商 |
| POSIX/Linux 风格 C 边界 | musl 补丁、prima_syscall、profile 与台账 | Linux 风格号到原生机制/服务的适配 |

原语存在不意味着已有用户 syscall；原生 syscall 存在也不意味着同名 POSIX API 语义完整。

## 容易误读的号段

[compat-state](../superpowers/compat/compat-state.md)记录实现状态；当前常量以 `prima-abi::syscall` 为定义源。42–46 依次为 `SYS_WAIT_CREATE`、`SYS_WAIT_OBSERVE`、`SYS_WAIT_OBJECTS`、`SYS_WAIT_SIGNAL`、`SYS_WAIT_DESTROY`。51 是 `SYS_FORK`，53 是 `SYS_WAIT4`，54 是 `SYS_PROCESS_EXIT`，55 是 `SYS_EXEC_REPLACE`，56–58 是 SIGNAL/SIGACTION/SIGRETURN；52 未分配。

历史 M17 设计曾提议 `SYS_EXECVE=52`，后续[M17c-2 契约](../superpowers/specs/2026-10-23-stage5-m17c2-exec-syscall.md)采用 55 的 exec_replace。旧设计是前序方案，不能作为当前号表。

Linux 风格 `syscall(57)` 在 C 边界表示 fork，Prima 原生 57 表示 sigaction。报告中的续执行问题指前一种路径；历史W6走原生51，当前C fork改走64预备/交接/发布。检索和日志必须注明号空间。

## 当前追加接口与有界验收

[工作契约](../superpowers/specs/2026-10-25-process-boundary-followup-design.md)与
[工作记录](../superpowers/reports/2026-10-25-stage0-5-gap-followup.md)记录以下增量；本轮host及具体验收程序已通过，不继承历史版本证据，不表示全部参数组合均有机器见证。

| 原生号 | 作用 |
|---|---|
| 59 | Process leader/父身份，Task id仍为7 |
| 60 | 带认证sender代次/msg_id的接收；旧18保持三字输出 |
| 61 | 以leader TaskDestroy授权终止完整Process |
| 62 | 带envp的内存映像exec准备/提交；旧55保持四参数 |
| 63 | Task/Process代次、自身及所持TaskSend目标查询 |
| 64 | fork准备/发布/取消事务与ticket |
| 65 | 通用upcall动作/mask/pending/来源/现场/等待/事件路由 |
| 66 | 当前共享线程原子解除栈映射并退出 |
| 67 | 用户word比较与generation等待原子登记，有限期限取物理时钟 |
| 68 | 保留映射帧与COW的用户访问启用/禁用 |
| 69 | 显式保留范围、入口寄存器、Segment接管及提交后入口的exec交接 |
| 70 | 创建独占IRQ线路Device和可委托的订阅源 |
| 71 | 受授权mask/unmask IRQ线路 |
| 72 | 当前Task等待物理单调期限，不自动重启 |

39/40的后继实现改用单CPU PIT校准时间戳及IRQ0期限唤醒，独立于抽象Timer。

这些仍是与本工作树配套的实验性接口，当前边界查profile v2及逐入口表；旧1–58号摘要仅属于前批范围。

## 版本不互相替代

- `native_abi` 的 1.0 握手只验证原生机器接口契约。
- runtime 的源码 API_MAJOR/API_MINOR 独立维护，当前仍处于 0.x。
- boot manifest、boot info 与用户 trace 布局有独立字段与版本。
- profile v0/v1/v2 是兼容范围版本；候选 `NT_RUXOS_ABI` 等名称不因出现在文档中就成为 Prima 已采纳接口。

## 文件服务与 personality

[原生打开对象](../../prima-runtime/src/vfs/handles.rs)接受 AccessMode/SeekFrom/OpenFlags 并返回 FileInfo/语义错误；[服务边界](../../prima-user/src/service.rs)负责原生 IPC 编解码，[C personality](../../prima-user/c/prima_syscall.c)负责 Linux fd、errno、stat/dirent。HandleId/NodeId 都不是 Capability。v3 RAM 镜像版本独立于当前不变的线协议和 syscall 号空间。当前认证Process会话/OFD holder已通过本轮host及W6/W7b/N1有界见证；多LibOS的独立根/personality隔离尚未实现，详见[VFS 契约](../superpowers/specs/2026-10-23-vfs-libos-foundation-design.md)。

## 变更流程

先确认抽象语义与归属，再同步号段定义、内核分派、runtime 封装、C 头/适配层和 musl 裸 syscall 补丁，补充正负例与 trace。号段只追加、不复用空洞；若工作决定仍待组织级确认，保留该状态并在阶段跟踪登记。

编写新接口设计使用[设计模板](templates/spec.md)，记录前置授权、成功/失败状态、容量影响、阻塞/唤醒与兼容偏差。


### 原生 C 内存映像 exec（W7b有界见证）

`prima_exec_image(image, length, argv, envp)`是[进程边界后继](../superpowers/specs/2026-10-25-process-boundary-followup-design.md)的C原生入口，
共用`execve(path)`的状态交接，最终调用69。只接受静态ET_EXEC，失败返回原始负errno；
不新增POSIX符号或系统调用号。W7b将独立C子映像嵌入旧程序，在RAM rootfs容量之外验证完整新crt。

65 selector8的事件路由工作增量：event0=子退出、1=访存故障、2=非法指令、3=算术故障、4=对齐故障。
C personality将4映射SIGBUS；原生info.kind8表示对齐类别，未新增syscall号；#AC路由有host回归，尚无机器见证。


## 静态程序声明与适配层符号护栏（本轮有界验证）

[C/Rust静态护栏](../superpowers/specs/2026-10-26-static-program-contract-design.md)使用PT_NOTE可见的
`.note.prima.abi`：布局、原生major/minor、persona和adapter major/minor六个u32。
构建按所选程序声明检查；C exec在根替换/CLOEXEC前拒绝不匹配，crt强依赖`prima_libc_contract_v1_0`。
`prima_libc.map`提供PRIMA_LIBC_1_0静态输出可见性策略，不建立动态GNU符号版本解析。
逐Linux入口与非分派符号见[当前台账](../superpowers/compat/stage5-working-api.md)；note不提供能力授权或内核persona。
