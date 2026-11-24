---
id: spec-heterogeneous-isa-hybrid-execution
kind: specification
authority: heterogeneous-isa-execution-design
status: proposal
lifecycle: current
updated: 2026-11-24
---

# 异构 ISA 混合执行设计：系统层双构建与应用层用户态兼容层

> 状态：提案。系统层 v1 的范围决定已由作者于 2026-11-24 确认；应用层混合执行、第三方应用白名单和平台一致性前提仍为提案，未实现、未冻结，不改变任何阶段状态或退出勾选。

## 1. 问题与范围

### 1.1 触发

用户希望参考 Popcorn Linux 的思想，使 PrimaOS 支持多种指令架构，提供“类似单系统”的环境，并用用户态双向兼容层让 ARM 与 x86 软件均可调动所有核心。讨论后收敛为两层：

- **系统层**：自有源码的系统组件按 ISA 同源双构建，各自在对应节点原生运行。
- **应用层**：第三方二进制走 native 域 + 用户态 DBT 翻译域的混合执行；同一 guest 进程的线程可分布在不同 ISA 域。

本文记录该方向的目标、边界、已确认决定和待确认项。它是设计提案，不是实现状态、ABI 冻结或阶段签署。

### 1.2 上位来源

- [Prima OS 白皮书](../../../Prima%20OS.md)：§17 计算介质与架构边界、§18 多介质与分布式、§8.4 不在前三阶段的兼容承诺。
- [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)：§2.2 明确不证明 SMP/弱内存模型；§18 细化要求；Node、共享 Segment 与 Capability 语义。
- [Prima OS 实现阶段](../../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md)：7-B x86_64 SMP、7-H AArch64 单核机制验证。
- [阶段 7 范围裁决](2026-10-31-stage7-scope-decisions.md)、[阶段 7 启动报告](../reports/2026-11-06-stage7-start.md)与[执行计划](../plans/2026-11-06-stage7-execution.md)。

### 1.3 目标

| ID | 目标 |
|---|---|
| G1 | 同一套 POSIR 契约下，系统组件按 ISA 双构建并在各自节点原生运行；内核仍是每个 ISA 节点一个实例。 |
| G2 | 第三方二进制在匹配 ISA 上原生运行，在另一 ISA 上通过用户态 DBT 运行；同一 guest 进程的多个线程可分布在两类域，共享同一地址空间。 |
| G3 | “单系统”假象在用户态成立：统一 PID/TID、FD、signal 与线程放置；内核不感知 guest ISA。 |
| G4 | 第三方兼容层不进入内核 TCB；Capability 校验仍在 syscall 边界执行。 |

### 1.4 非目标

- 一个内核二进制跨 ISA 运行；全局共享单一 `KernelState`。
- 单线程软件自动并行化或用满所有核。
- 任意 ISA、任意二进制的无条件兼容；无白名单的“兼容一切软件”。
- 无硬件一致性平台上的透明高性能共享内存。
- 系统层 v1 的活迁移。

### 1.5 反目标

以下结果即使“技术上能跑”也视为设计失败：

- 为兼容性向内核加入 ISA 专属语义，或扩大可信机制层。
- 通过绕过 Capability 校验、共享内核状态或隐式提升 guest 权限来实现“单系统”。
- 把 QEMU 演示当作真实硬件缓存一致性或性能证据。
- 把未授权 guest 代码提升为系统权限，或让翻译器/JIT 获得环境权限。

## 2. 状态与作者确认记录

本文区分“作者已确认”与“本文提案”。未确认项不得在实现或阶段材料中写成已裁决。

| 日期 | 事项 | 状态 | 来源 |
|---|---|---|---|
| 2026-11-24 | 目标包含同一进程的线程跨 ISA 迁移（Popcorn 路线），而非只做单向模拟器 | 作者确认 | 对话 |
| 2026-11-24 | 用户态双向兼容层兼容 ARM 与 x86 软件，让任意架构软件调动所有核心 | 作者确认的目标表述；范围受 §1.4 约束 | 对话 |
| 2026-11-24 | 重编译两份针对系统组件；混合域针对第三方应用 | 作者确认 | 对话 |
| 2026-11-24 | 系统层 v1 采用双构建 + 每节点部署，不做活迁移 | 作者确认 | 对话 |
| 2026-11-24 | 应用层采用 supervisor + native/translated 两域的 split-process 模型 | 本文提案 | 本文 §6 |
| 2026-11-24 | 复用现成 DBT 引擎，不自行编写翻译器 | 本文提案 | 本文 §6.3 |
| 2026-11-24 | 第三方应用 v1 限定静态链接、单进程多线程、pthread/futex 单域白名单 | 本文提案，待确认 | 本文 §7 |
| 2026-11-24 | 平台必须具备两簇核一致可见的共享内存；无一致性平台不承诺透明混合域 | 本文提案，待确认 | 本文 §6.5 |
| 2026-11-24 | 平台拓扑：单板双簇共享 DRAM，还是两节点/网络 + 共享窗口 | 待确认 | 本文 §6.5.1 |
| 2026-11-24 | v1 跨域同步只支持运行时自旋锁/屏障；跨域 futex/signal 留 v2 | 本文提案，待确认 | 本文 §6.7 |

## 3. 术语

| 术语 | 含义 |
|---|---|
| ISA 节点 / 核簇 | 一组同 ISA 的 CPU 核及其本地内核实例。 |
| 系统层 | 自有源码、受信的系统组件：内核、LibOS、supervisor、翻译器、系统服务。 |
| 应用层 | 第三方 guest 二进制及其运行时上下文；代码不可信。 |
| native 域 | host ISA 与 guest ISA 相同的执行域，guest 代码原生执行。 |
| translated 域 | host ISA 与 guest ISA 不同的执行域，guest 代码经用户态 DBT 翻译执行。 |
| guest 进程 | 第三方应用在 guest ABI 下看到的单一进程视图；由 supervisor 跨域维持。 |
| 混合域 | 同一 guest 进程的线程分布在 native 域和 translated 域执行。 |
| 迁移信封 | 跨域迁移线程时需要携带并转换的状态集合。 |

## 4. 总体架构

```text
x86 节点                                      AArch64 节点
HexArc-x86_64 (KernelState_A)                 HexArc-aarch64 (KernelState_B)
  ├─ 系统层 x86 构建                            ├─ 系统层 AArch64 构建
  │   ├─ Linux LibOS（x86_64 ABI 前端）         │   ├─ Linux LibOS（AArch64 ABI 前端）
  │   ├─ supervisor（x86 构建）                 │   ├─ supervisor peer（AArch64 构建）
  │   └─ guest 执行域                           │   └─ guest 执行域
  │       （native x86 或 ARM→x86 DBT）          │       （native AArch64 或 x86→AArch64 DBT）
  └─ 共享 arena（显式 Segment） ────────────────┴─ 共享 arena（同一物理窗口）
                    Node 消息 + 共享内存窗口
```

| 层 | 代码来源 | 执行方式 | v1 迁移策略 |
|---|---|---|---|
| 系统层 | 自有源码、受信 | 同源双构建，每节点原生 | 不做活迁移；每节点部署/复制 |
| 应用层 | 第三方二进制、不可信 | native 域 + DBT 翻译域 | guest 线程域内固定；v2 再做静止点跨域 |

图示是“两节点 + 共享窗口”的一种候选拓扑；单板双簇共享 DRAM 是另一种拓扑，见 §6.5.1。两种拓扑对 Node/网络抽象与共享窗口表达的适用性不同，不能默认互换。

核心结论：**“同一系统”不是单一内核状态，而是两个 ISA 内核实例之上、由用户态系统层维持的进程/地址空间视图。**

## 5. 系统层契约

### 5.1 内核与节点

- 每个 ISA 节点一个内核构建，拥有独立 `KernelState`；两个内核之间只通过 Node 消息、共享 Segment 和显式共享窗口交互。
- 内核永不跨 ISA 迁移，也不感知 guest 的 ISA。
- 不建立全局透明 `KernelState`；该限制与白皮书 §18.3 一致。
- 跨节点 Capability 语义按 COSIR v1：`send` 不转移 Capability；非空 `transferred_caps` 发往远端返回 `NonPortableCap`。

### 5.2 同源双构建

- 系统组件使用同一份源码按目标 ISA 构建；构建产物分别部署在对应节点。
- 双构建不是无脑重编译：Linux LibOS 前端包含 arch 分支（syscall 号段、结构布局、TLS、signal frame、`auxv` 等），本质是两套 ABI 前端。
- 内核侧新增 AArch64 实例的前置是 7-H 的机制验证；7-H 本身不要求应用/设备对等。每节点单核原型不要求 7-B，但“用满所有核”的承诺要求对应节点的 7-B（x86 SMP）完成。

### 5.3 v1 部署与迁移策略

作者已确认 v1 采用：

- 双构建 + 每节点部署；
- 无状态系统服务重启即可；
- 有状态系统服务按复制/日志切换处理；
- 不做系统组件的跨 ISA 活迁移。

若未来确需系统组件活迁移，必须单独评审，并至少满足：

- 仅在静止点迁移，不在 syscall、持锁或未决请求中途迁移；
- 具备多 ISA 代码映像与入口表，不能直接搬 PC；
- 完成寄存器、ABI、TLS、信号帧等状态转换；
- 迁移失败可回退，且 Capability 位置与撤销语义不破坏。

### 5.4 系统层不变量

- POSIR 抽象语义不因异构部署而改变；所有节点运行同一 POSIR 契约的不同 COSIR 细化。该主张以 §6.5.1 的平台 carve-out 解释为前提；若需要 POSIR 可见的共享窗口对象，则前提不成立，按 H1 反例处理。
- 每 ISA 的 `COMPAT-TEST` 必须覆盖能力隔离、内存、调度、IPC 的对应观察语义。
- 若第二架构或混合部署要求新的独立可观察语义或原语类，按白皮书 §14 登记为 H1 反例，而不是默认扩展内核。

## 6. 应用层契约

### 6.1 guest 进程模型

本文提案采用 **split-process** 模型：

- 一个 guest 进程由 supervisor、native 域和 translated 域共同承载；每个域是独立 host 进程/节点。
- 三个组件把同一份 guest 地址空间映射到共享 arena，并**使用相同 VA**；跨域指针有效以同 VA 为前提，不能作为 best-effort。
- 所有可能被跨域访问的内存（全局、堆、跨域共享的栈/TLS）必须在 arena 内；仅当线程私有内存确定不会被跨域访问时才可放在 arena 外，加载器/分配器必须强制该约束。
- supervisor 负责：加载 ELF、构建共享地址空间、统一 PID/TID、FD 表、signal 路由、线程放置策略、域生命周期与失败处理。
- guest 看到的仍是单一进程；host 侧看不到单一内核对象。

### 6.2 执行域与线程放置

- native 域：host ISA 与 guest ISA 一致，guest 代码原生执行。
- translated 域：host ISA 与 guest ISA 不一致，guest 代码经用户态 DBT 翻译执行。
- v1 中 guest 线程一旦分配到域就固定在该域；跨域线程迁移留到 v2。
- “用满所有核”指 guest 的多线程被分布到两类域，以及 DBT/运行时自身在所在节点使用多核；不包含单线程自动并行化。

### 6.3 兼容层组成

应用层兼容层由两部分组成：

1. **guest ABI 前端**：LibOS 提供 Linux AArch64/x86_64 的 syscall、FD、signal、TLS、`auxv` 等视图，并把 guest 请求翻译为 Prima 原语。
2. **CPU 翻译器**：用户态 DBT 把 guest 指令翻译为 host ISA 指令；优先复用现成引擎（ARM→x86 可先评估 QEMU user TCG；x86→AArch64 可评估 FEX/Box64 类方案），不自行从零编写。

翻译器把 guest 代码视为不可信输入：JIT 代码页保持 W^X，guest 不能借翻译器获得额外权限，所有系统调用仍经 LibOS 的 Capability 校验。

集成契约不能省略：

- v0 可以把 QEMU user / FEX / Box64 当作整进程黑盒；v1 的 split-process 要求**可嵌入的翻译循环、syscall 重定向和共享 VA**。现成引擎默认自己接管 Linux syscall 和整个进程地址空间；若无法嵌入，v1 需要自研集成层或翻译器，这是主要成本，不能当作“选一个引擎”的小事。
- v1 建议先做一个方向（ARM guest 在 x86 host 上翻译）；反向（x86 guest 在 AArch64 host 上）对称但需单独验收。“双向”是目标，不等于 v1 同时交付两个方向。

### 6.4 权限与信任边界

- 系统层（内核、LibOS、supervisor、翻译器）受信；guest 二进制不可信。
- guest 的权限止于其被授予的 Capability，例如共享 arena、受限服务通道；不存在环境权限或隐式跨域权限。
- v1 不做跨节点 Capability 转移；共享 arena 由 bootstrap 在两侧分别授权，不依赖 `send` 搬 Capability。
- supervisor 不充当“任意 Capability 铸造者”；它只能使用自身已获得的授权。
- 翻译器与运行时是用户态 TCB 的一部分，不进入内核 TCB；其崩溃不得影响其他进程或系统层。
- FD/signal 的“统一视图”与数据路径分开设计：v1 建议 supervisor 集中 FD 元数据与 signal 路由，域持有受限代理，I/O 数据路径尽量在域本地完成；跨域 I/O RPC 仅用于需要统一语义的操作。该模型待设计确认。

### 6.5 平台一致性前提

混合域要求 native 域和 translated 域看到一致的共享内存。该前提必须显式声明：

- x86 与 ARM 核簇通常没有硬件缓存一致性；Popcorn 的原型依靠软件 DSM 补足。
- 对第三方二进制做透明 DSM 只能依赖缺页拦截/页保护等机制，代价高，且原子、锁和弱内存序语义难以完整保持。
- QEMU 不模拟真实缓存一致性，可作为架构演示，不能作为真实硬件可行性或性能证据。
- 若无一致性保证，混合域只能缩小到平台能保证的范围，或退回“同一 ISA 内原生执行 + 跨域消息”，不得承诺任意第三方二进制的透明共享。

### 6.5.1 拓扑与共享窗口的 POSIR 表达（待确认）

候选拓扑有两种，不能默认互换：

| 拓扑 | 形态 | 影响 |
|---|---|---|
| T-A 单板双簇 | x86 与 AArch64 核簇在同一机器上共享 DRAM | Popcorn 形态；需要共享 DRAM carve-out；Node/网络不是唯一抽象；缓存一致性通常缺失 |
| T-B 两节点 | 两台机器或两个 QEMU 实例，通过网络 + 共享窗口（ivshmem/RDMA）连接 | 直接对应 COSIR Node/RemoteProxy；共享窗口是平台设备；DSM 成本高 |

本文图示默认 T-B；若选 T-A，Node/消息模型的适用性必须重新评估。

无论哪种拓扑，都要回答“同一物理窗口如何进入两个 `KernelState`”：

- `memseg_create` 从本地 `frame_universe` 取帧；W6 是每个 `KernelState` 内的物理页归属；Segment 不跨节点。
- 方案 (i) 平台 carve-out：bootstrap 在两侧保留同一物理窗口并分别授予本地 Capability；它位于正常帧分配之外，跨租户隔离必须在 POSIR 之外单独论证。
- 方案 (ii) POSIR 可见的共享窗口对象或跨节点 Segment：这是新的可观察语义/原语类，按白皮书 §14 属于 H1 反例，需要 POSIR v2 评审与证明义务。

因此，§5.4/§10 的“POSIR v1 不变”只在方案 (i) 成立且隔离论证通过时成立；若需要方案 (ii)，本文必须改写为 POSIR 扩展，不能继续宣称 v1 不变。

### 6.5.2 候选证据平台（未选型）

当前没有主流商用的 x86+ARM 对等缓存一致 SMP；下列两个候选分别覆盖软件模拟与真实硬件方向，均为候选，不是已选型。

| 候选 | 类型 | 可支持的主张 | 不可替代的证据 |
|---|---|---|---|
| **Stramash-QEMU** | 研究模拟器：多 QEMU 实例 + MESI 缓存一致性模型，x86-64 + AArch64；来自 Edinburgh / Virginia Tech 的 Stramash 工作（ASPLOS '25） | 在受控环境中验证 T-A 的跨 ISA 共享内存协议与 fused-kernel/共享内存 OS 设计；补足 plain QEMU + ivshmem 不建模缓存一致性的缺口 | 不是硅片；不能证明真实硬件一致性、时序与故障行为；Stramash 自身报告的模拟误差与相对 Popcorn 加速不是 PrimaOS 证据 |
| **CXL 3.x 多主机共享内存** | 工业标准 + 设备/交换机制导一致性：Shared FAM、HDM-DB、Back-Invalidate（BI） | 真实硬件方向最接近跨主机（可含 x86 与 ARM 主机）共享内存一致性的标准化机制；可作为 T-B 的长期硬件目标 | 不是原生 x86+ARM CPU SMP；一致性由设备/交换机维护；跨 ISA 内存模型、原子、分区/故障与商用平台可用性仍需验证；CXL 2.0 pooling 是分区而非共享 |

选型含义：

- 纯软件原型（v1 架构验证）优先 **Stramash-QEMU**，因为它至少建模 MESI；plain QEMU + ivshmem 只证明运行时架构，不证明一致性行为。
- 真实硬件验证另立轨道，候选 **CXL 3.x 多主机**；在拿到平台规格与跨 ISA 验证前，不得把 CXL 3.x 的“多主机一致性”直接等同于原生异构 SMP。
- 两者都不改变 §1.5 的反目标：QEMU 类模拟结果不能当作真实硬件一致性证据。

### 6.6 迁移信封（v2）

跨域迁移线程（或未来系统组件活迁移）至少携带：

| 类别 | 内容 | 备注 |
|---|---|---|
| 架构状态 | GPR、SIMD/FP、flags、TLS 基址 | 按 ISA 逐项转换 |
| 续执行点 | 每 ISA 代码映像 + 入口表 | 不直接搬 PC；同源码不同 ISA 地址不同 |
| 内存 | 共享 arena 或迁移私有页 | 同 VA；私有页需复制/重映射 |
| 权限 | 本地 Capability | v1 不跨节点转移；应用层限定授权 profile |
| 身份与同步 | TID、TLS、futex 等待、signal mask/pending | v1 不迁移阻塞中的 futex/signal 状态 |
| 锁与未决操作 | 无持锁、无未决 syscall、无在途 I/O | 只在静止点迁移 |

### 6.7 内存模型与同步

- guest 的原子、fence、exclusive monitor/LL-SC 语义由翻译器按 guest ISA 保持；不能把 host 的更强内存序当作 guest 语义的替代。
- v1 只允许跨域共享数据在保守 full-fence 自旋锁/屏障下访问；不做无锁共享，不做跨域阻塞 futex。
- 因此 v1 的跨域同步只支持运行时提供的自旋锁/屏障；guest 的 pthread mutex/condvar/futex 只在同一域内受支持。跨域阻塞同步属于 v2；v1 白名单应用不得依赖它。
- 跨域 futex、signal、条件变量等阻塞同步留到 v2。
- QEMU TCG 不能作为弱内存序正确性的唯一证据；需要真实硬件或额外的 litmus/压力测试。

## 7. 第三方应用 v1 提案范围

以下为本文建议，尚未由作者确认：

- guest ISA：Linux AArch64 与 Linux x86_64。
- 静态链接、单进程、多线程。
- pthread/futex 仅在单域内受支持；跨域同步只允许运行时自旋锁/屏障，不允许跨域阻塞 futex/condvar。
- 固定且两侧相同的 VA 布局；堆、mmap 与可能跨域访问的栈/TLS 限制在共享 arena 内。
- 不做 `fork`/`exec`、动态链接、自修改代码、guest 内 JIT、设备直通。
- 不支持“单线程软件用满所有核”的承诺。

不在 v1 的范围：任意 ISA、任意二进制、无白名单、无一致性平台的透明共享。

## 8. 分期

| 阶段 | 内容 | 退出观察 |
|---|---|---|
| v0 单域 | 一个方向（如 ARM guest 在 x86 节点）用 QEMU user 级 DBT 跑起来；在 PrimaOS 当前单核上只验证 LibOS + DBT + 单线程/FD/signal；“用满 host 多核”需 7-B SMP，可在 Linux host 先做工程 spike，但不算 PrimaOS 证据 | 与原生执行差分一致；无跨域；不涉及异构核；多核结论需 7-B 证据 |
| v1 混合域 | 前置：最小 AArch64 HexArc 实例 + Linux-AArch64 ABI 前端（7-H 是必要但不充分条件，且 7-H 不要求应用对等）；系统层双构建每节点部署；guest 静态共享 arena；guest 线程分布到 native/translated 两域；锁保护；不迁移；证据平台候选见 §6.5.2 | 同一 guest 进程同时使用两簇核；共享数据正确；权限不越界；前置依赖与 7-H 范围分别登记 |
| v2 | 静止点跨域线程迁移；跨域 futex/signal；动态链接；按需 DSM；必要时评估系统层活迁移 | 迁移正确性、唤醒不丢失、失败可回退 |
| 永不承诺 | “任意架构、任意软件、无白名单、自动并行化” | 以白名单和声明范围替代 |

## 9. 验证与反例

| 对象 | 证据 | 反例/失败条件 |
|---|---|---|
| 系统层每 ISA 契约 | 冻结 observable semantics 的 `COMPAT-TEST`；能力、内存、调度、IPC | 需要新原语类 → 登记 H1 反例，不默认扩核 |
| guest ABI | 与原生 Linux 参考的差分测试；syscall、FD、signal、TLS | 仅“能启动”不算通过 |
| 翻译正确性 | native vs translated vs hybrid 同输入同输出；指令/ABI 覆盖矩阵 | 翻译域输出偏差即失败 |
| 并发/内存模型 | 跨域锁、屏障、futex 压力；litmus；丢失唤醒注入 | 依赖 QEMU 单一证据不算通过 |
| 迁移（v2） | 静止点迁移、状态转换、回退、失败注入 | 持锁/未决 syscall 迁移必须拒绝 |
| 安全 | Capability 不可伪造；syscall 边界；JIT W^X；guest 逃逸测试 | 任何 env 权限或越权访问即失败 |
| 平台一致性 | 声明平台 profile；真实硬件验证 | 无一致性平台不得宣称透明混合域 |
| 证据拓扑 | plain QEMU + ivshmem 只证明运行时架构；Stramash-QEMU 建模 MESI，更接近一致性行为但仍为模拟；真实硬件需 CXL 3.x 等多主机平台单独验收 | 把模拟结果当作真实硬件一致性通过即失败；真实硬件验收单列 |
| 性能 | 翻译开销、混合域加速/减速、先基线后门槛 | 未测先承诺性能即失败 |

所有计划中的测试只能标为待验证，不得在本文标 PASS。

## 10. 与现有契约的关系

- **POSIR v1 不变（有条件）**：本设计不新增 ISA 感知原语；应用层假象由用户态系统层维持。共享窗口按 §6.5.1 方案 (i) 的平台 carve-out 解释时才成立；若需要 POSIR 可见的共享窗口/跨节点 Segment，则按白皮书 §14 记录 H1 反例，并改写为 POSIR 扩展。
- **COSIR §2.2**：SMP、弱内存模型与锁实现不在抽象规范证明范围；它们属于细化层义务。
- **COSIR §18**：页表/TLB、SMP 核间同步、Cache coherence/fence、跨节点传输均为细化要求，不改变 POSIR 观察语义。
- **白皮书 §18.3**：明确拒绝全局透明 `KernelState`、跨节点 Namespace 挂载与跨节点同步一致性；本文与之保持一致。
- **阶段 7**：7-B（x86 SMP）与 7-H（AArch64 单核机制验证）是相关前置；7-H 是 v1 的必要但不充分条件（不含应用/设备对等），7-B 是“用满所有核”的必要条件。本文不改变阶段状态、退出勾选或证据索引。v0/v1 若实施，属于有界探索原型，不自动构成阶段签署。
- **文档状态**：本文是提案；实现、运行证据、profile 冻结和阶段签署必须另行记录。

## 11. 开放决策

| ID | 决策 | 当前建议 | 状态 |
|---|---|---|---|
| D-ISA-01 | 系统层 v1 是否活迁移 | 双构建 + 每节点部署，不活迁移 | 作者已确认 |
| D-ISA-02 | 第三方 v1 范围 | 静态、单进程多线程、pthread/futex 单域白名单；跨域自旋锁/屏障 | 待确认 |
| D-ISA-03 | 目标平台与一致性 profile | 纯软件原型优先 Stramash-QEMU（MESI 模拟）；真实硬件候选 CXL 3.x 多主机；必须声明一致性 profile | 待确认 |
| D-ISA-04 | DBT 引擎 | 复用 QEMU user TCG / FEX / Box64，不自行编写 | 待确认 |
| D-ISA-05 | guest 进程模型 | split-process（supervisor + 两域） | 提案 |
| D-ISA-06 | native/translated 域身份与 FD/signal 一致性 | supervisor 统一持有；域只持有受限代理 | 待设计 |
| D-ISA-07 | 跨域线程迁移时机 | v1 固定域；v2 静止点迁移 | 提案 |
| D-ISA-08 | 系统层未来是否需要活迁移 | 仅对明确的无状态计算服务评估 | 待确认 |
| D-ISA-09 | “任意架构软件”的白名单治理 | 按 ISA 对、ABI profile、测试集逐项冻结 | 待设计 |
| D-ISA-10 | 性能门槛 | 先测翻译开销与混合域收益，再冻结 | 待基线 |
| D-ISA-11 | 拓扑（T-A 单板双簇 / T-B 两节点）与共享窗口表达 | 先选 T-B + 方案 (i) 平台 carve-out，并单独论证跨租户隔离；若需方案 (ii) 则改为 POSIR v2 | 待确认 |
| D-ISA-12 | v1 跨域同步范围 | 只支持运行时自旋锁/屏障；跨域 futex/signal 留 v2 | 待确认 |
| D-ISA-13 | DBT 集成方式 | v0 黑盒整进程；v1 需可嵌入翻译循环 + syscall 重定向 + 共享 VA；先做一个方向 | 待确认 |
| D-ISA-14 | FD/signal 状态模型 | supervisor 集中元数据/路由，域持受限代理；数据路径本地 | 待设计 |
| D-ISA-15 | v0/v1 前置与证据边界 | v0 单核可先做；多核需 7-B；v1 需最小 AArch64 HexArc + AArch64 LibOS 前端 | 事实/待排期 |

### 11.1 当前问题清单（按阻塞程度）

先列出已确认项，避免与问题混淆：

- 系统层 v1 采用双构建 + 每节点部署，不做活迁移（D-ISA-01，作者已确认）。
- 系统组件重编译两份、第三方应用走混合域（作者确认）。
- 已确认不等于已实现；这些方向仍缺实现、证据和阶段签署。

| # | 问题 | 级别 | 现状 | 关闭条件 / 需要决定 |
|---|---|---|---|---|
| P1 | 共享窗口的 POSIR 表达 | 阻塞 v1 启动 | `memseg_create` 只从本地 `frame_universe` 取帧；W6 是每个 `KernelState` 的物理页归属；Segment 不跨节点。方案 (i) 平台 carve-out 与方案 (ii) 新 POSIR 对象未选 | 选 T-A/T-B；选 (i) 并补跨租户隔离论证，或选 (ii) 登记 H1 反例并走 POSIR v2（§6.5.1、D-ISA-11） |
| P2 | 平台一致性缺失 | 阻塞 v1 验收 | 没有商用 x86+ARM 对等一致 SMP；Stramash-QEMU 是 MESI 模拟；CXL 3.x 多主机是设备/交换机制导，不是原生异构 SMP | 选证据平台并声明一致性 profile；真实硬件验证另立轨道；不得把模拟结果当硬件一致性（§6.5.2、D-ISA-03） |
| P3 | v1 跨域同步与第三方 pthread 冲突 | 阻塞 v1 范围 | 提案只允许跨域自旋锁/屏障；pthread/futex 仅单域；跨域阻塞留 v2。第三方二进制通常依赖 pthread/futex 阻塞 | 接受这组收缩并冻结白名单，或把跨域 futex 提前（成本高）；两者不能同时默认（§6.7、§7、D-ISA-02/12） |
| P4 | DBT 集成契约未定 | 阻塞 v1 实现 | QEMU user / FEX / Box64 默认自管进程与 Linux syscall；v1 split-process 需要可嵌入翻译循环 + syscall 重定向 + 共享 VA | 选可嵌入引擎或自研集成层；v1 先做一个方向；不能用“整进程黑盒”直接交付 split-process（§6.3、D-ISA-13） |
| P5 | 7-B/7-H 前置未就绪 | 阻塞 v1 排期 | 7-H 是单核机制验证、P2/后期；7-B x86 SMP 未做；当前内核单核。v1 需要最小 AArch64 HexArc + Linux-AArch64 ABI 前端 | 排期；明确 7-H 是否扩展到应用对等；放弃“用满所有核”或等 7-B（§5.2、§8、D-ISA-15） |
| P6 | 同 VA / FD / signal 模型未设计 | 阻塞 v1 设计 | 同 VA 是硬要求；动态链接、fork/exec 已排除；FD/signal 跨域模型未设计 | 设计 loader/allocator/mmap 镜像；定义 FD/signal 代理、共享文件偏移与 `dup` 语义（§6.1、§6.4、D-ISA-06/14） |
| P7 | 安全 / TCB / 性能风险 | 研究风险 | 翻译器是用户态大 TCB；无威胁模型；无翻译开销与混合域收益基线 | 威胁模型、JIT W^X/沙箱、guest 逃逸测试；先测翻译开销与混合域收益（§6.3、§9、D-ISA-10） |
| P8 | 范围与白名单未冻结 | 治理 / 验收 | “任意架构、任意软件、单线程用满所有核”不可承诺；ISA 对、ABI profile、白名单未冻结 | 冻结 ISA 对、ABI profile 与 v1 白名单；明确阶段定位（§1.4、§7、§8、D-ISA-02/09） |

最小决策集：

1. **拓扑与共享窗口**：T-A 单板双簇共享 DRAM，还是 T-B 两节点 + 共享窗口；共享窗口走 (i) carve-out 还是 (ii) 新原语。这决定 P1/P2 的答案。
2. **v1 收缩**：是否接受“静态链接、单进程多线程、pthread 仅单域、跨域只自旋锁、单向 DBT”。不接受，P3/P4/P6 的成本会跳一个量级。
3. **阶段定位**：阶段 8 研究轨，还是阶段 7 有界探索原型；是否等 7-H 的最小 AArch64 内核就绪。这决定 P5/P8 的排期。

建议关闭顺序：P1/P2 → P3/P4 → P5/P6；P7/P8 可与前两组并行，但不得在 P1/P2 未定时承诺实现或性能。

## 12. 重审触发

- POSIR/COSIR 修订，或第二架构实例暴露新原语类。
- 7-B SMP 或 7-H AArch64 验证结果改变前置条件。
- 目标平台拓扑（T-A/T-B）、一致性 profile 或共享窗口表达方式改变。
- 候选证据平台（Stramash-QEMU / CXL 3.x）的选型或能力范围改变。
- DBT 引擎/集成契约、guest 进程模型或第三方白名单范围改变。
- 出现跨域同步、futex、signal、迁移失败的反例。
- 7-H 的范围从“单核机制验证”扩展或收缩，影响 v1 的 AArch64 内核与 ABI 前端前置。
- 作者改变系统层迁移策略或第三方兼容范围。

## 13. 参考

- [Prima OS 白皮书](../../../Prima%20OS.md)
- [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)
- [Prima OS 实现阶段](../../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md)
- [阶段 7 范围裁决](2026-10-31-stage7-scope-decisions.md)
- [阶段 7 启动报告](../reports/2026-11-06-stage7-start.md)
- [阶段 7 执行计划](../plans/2026-11-06-stage7-execution.md)
- [文档系统设计与维护契约](2026-10-23-documentation-system-design.md)
- [Stramash (ASPLOS '25)](https://dl.acm.org/doi/abs/10.1145/3676641.3716275)
- [Stramash paper PDF](https://ssrg.ece.vt.edu/papers/asplos25.pdf)
- [CXL Consortium](https://computeexpresslink.org/about-cxl/)
- [CXL 3.0 tutorial](http://computeexpresslink.org/wp-content/uploads/2023/12/CXL_FMS-2023-Tutorial_FINAL.pdf)
- [CXL HDM-DB / Back-Invalidate](https://lpc.events/event/19/contributions/2062/attachments/1810/3891/lpc25-dbueso-cxl-backinvalidate.pdf)
