# Prima OS × Framekernel 差距分析报告

> 文档类型：差距分析报告（非设计、非规范、非证据）
>
> 日期：2026-08-16
>
> 委托范围：联网调研 Asterinas framekernel 一手资料，与 Prima OS 现状做三维对照（TCB 边界哲学、架构分层映射、对 H1–H4 与当前切片的启示），产出差距清单供决策；本报告不修改任何权威文档与代码，不为 H1–H4 产生新证据。

## 1. 调研依据与方法

**一手资料（2026-08-16 访问）：**

| # | 资料 | 来源 |
|---|---|---|
| 1 | ATC'25 论文《Asterinas: A Linux ABI-Compatible, Rust-Based Framekernel OS with a Small and Sound TCB》全文 PDF | https://www.usenix.org/system/files/atc25-peng-yuke.pdf |
| 2 | The Framekernel Architecture（官方 book 章节原文） | https://asterinas.github.io/book/kernel/the-framekernel-architecture.html（仓库 `book/src/kernel/the-framekernel-architecture.md`） |
| 3 | Asterinas OSTD（官方 book 章节原文） | 仓库 `book/src/ostd/README.md` |
| 4 | What Soundness Means for an OS Framework（含信任模型） | 仓库 `book/src/ostd/soundness/what-soundness-means.md` |
| 5 | Linux Compatibility（syscall 支持表） | 仓库 `book/src/kernel/linux-compatibility/README.md` |
| 6 | Roadmap | 仓库 `book/src/kernel/roadmap.md` |
| 7 | 仓库顶层与 `ostd/src`、`kernel/` 目录结构 | GitHub API（asterinas/asterinas，默认分支） |

**Prima 侧依据：** `Prima OS.md`（H1–H4 §3、分层 §4、安全模型 §6、研究对照 §16）、`COSIR 原语规范.md`（章节结构）、`Prima OS 实现阶段.md`（当前阶段与证据边界，2026-08-07 快照）。

**方法：** 逐条摘录带来源事实（§2），再做三个维度的分析对照（§3–§5），最后给出差距清单（§6）与证据边界（§7）。论文自报数据未经独立复现，标注为"自报"。

## 2. Framekernel 概念摘要（仅列带来源的事实）

**架构定义**（来源 2、1）：整个 OS 驻留单一地址空间（如宏内核），全部用 Rust 编写；内核逻辑上分为两半——**特权 OS 框架（OSTD）**与**去特权 OS 服务**。只有框架允许使用 unsafe Rust；服务（含 syscall、文件系统、网络协议、设备驱动）必须完全用 safe Rust。内存安全因而归约到框架，TCB 最小化；单地址空间保证函数调用/共享内存的高效通信。口号："The security of a microkernel, the speed of a monolithic kernel."

**框架四要求**（来源 2）：Soundness（safe API 对任意 safe 调用者不触发 UB）、Expressiveness（safe Rust 足以实现含驱动在内的 OS 功能）、Minimalism（能在框架外实现的绝不进框架）、Efficiency（safe API 近零开销）。

**Soundness 定义与边界**（来源 4）：
- 目标："No sequence of calls to OSTD's safe public API — combined with any behavior from user-space programs and peripheral devices — from a client in safe Rust can trigger undefined behaviors."
- **Soundness ≠ correctness**：逻辑 bug（饿死任务的调度器等）允许存在，但不得升级为 UB；系统可崩溃、死锁、行为错误，但不静默破坏内存。
- UB 分三级：语言级（Rust Reference 定义）、环境级（栈/堆/代码页等执行环境假设）、架构级（控制寄存器、页表、IDT、IOMMU 误配置）。
- 信任模型：trusted = rustc/core/alloc、OSTD、bootloader/firmware、核心硬件（CPU、内存控制器、IOMMU）；**not trusted = `kernel/` 下全部服务（含全部驱动）、注入策略（调度器、frame/slab allocator）、用户态程序、外设**。

**TCB 度量与数据**（来源 1，自报）：用 LLVM IR 链接后行数（LCS）度量——Asterinas 总 75,285 行，TCB（OSTD）10,571 行 = **14.0%**；对比 Tock 43.8%、Theseus 62.4%、RedLeaf 66.1%。OSTD 核心 crate 约 10K 行，官方称"比许多微内核还小"（来源 3）。TCB 增长受控：非 TCB 增长快于 TCB（来源 1 Figure 7）。

**关键安全机制**（来源 1）：10 条关键不变量（新分配帧必来自未用内存；内核态 CPU 状态不可被 client/外设篡改；敏感内存不可被 client/用户态/外设篡改；敏感 I/O 不可被 client 篡改；Task 至多占一个 CPU；HeapSlot 不越过父 Slab 生命周期等）。资源按敏感/非敏感分类（内核代码/栈/堆/页表敏感；用户与设备内存非敏感；APIC/IOMMU 敏感、外设非敏感）。**untyped memory**（UFrame/USegment）以 POD read/write 接口处理外部可变内存（用户映射、DMA 内存），不允许对其建立 Rust 引用。**Safe policy injection**：调度器、帧分配器、slab 分配器作为可注入策略移出 TCB（各约 1.2–1.6 KLoC），机制在 TCB 内强制不变量、策略犯错不导致 UB。

**验证手段**（来源 1、3）：KernMiri（为 OS 改造的 Miri），134 个单元测试覆盖全部 unsafe 代码实例、子模块行覆盖 >90%，检出的 UB 已修复。形式化验证官方明确"不是当前优先级"，依赖 soundness 论证 + 社区审查（来源 3）。

**Linux 兼容与性能**（来源 1、5，自报）：论文发表时 210+ syscall，book 当前 240+，逐 syscall 标注 flag 覆盖度；不加载 Linux 内核模块。LMbench 几何均值 1.08（对 Linux 归一化）；Nginx 1.17、Redis 1.31、SQLite 0.85；安全检查开销多 <3%（最高为帧分配 6.7%）；IOMMU pooling 后应用层开销最大 2.3%。

**明示限制**（来源 1、6）：优化集中单核，SMP 可扩展性为 future work；网络栈用 smoltcp 无拥塞控制（部分网络测试偏快）；未实现 Linux RCU-walk 文件名查找；当前仅支持 x86-64 VM（virtio 驱动）；目标场景为云 VM guest、TEE、安全容器。

## 3. 维度一：TCB 边界哲学对照

| | Prima OS | Asterinas framekernel |
|---|---|---|
| TCB 构成 | HexArc 内核 + COSIR 细化层（"待验证的可信基"，§6.4） | OSTD + Rust 工具链/core 库 + bootloader/固件 + 核心硬件 |
| 边界机制 | **CPU 特权级**（ring0/ring3）+ Capability 用户态不可伪造 | **Rust safe/unsafe 类型边界**（intra-kernel privilege separation） |
| 保护目标 | 抽象状态机的**显式授权安全**（无授权操作不改变他方状态；明确不是完全非干扰，§6.4） | **内存安全**（无 UB；明确不保证逻辑正确性） |
| 不可信方 | 全部用户态：应用、LibOS、系统服务 | 内核态全部服务（含驱动）、注入策略、用户态、外设 |
| 服务位置 | 用户态（微内核式，独立 Task + 最小 Capability） | 内核态（safe Rust，函数调用直达） |
| 验证路线 | 规划机器证明（BLK-001，Open）+ host MODEL-TEST | soundness 论证 + KernMiri 可执行检查；形式化非当前优先级 |

**分析判断：**

1. **两者解决不同问题，哲学正交。** Prima 的 capability 边界管"授权"（谁能对哪个对象做什么），framekernel 的类型边界管"实现安全"（内核代码不破坏内存）。Prima 的威胁模型不覆盖内核实现的语言级内存安全（目前 `kernel/` 无 unsafe 收敛纪律）；Asterinas 不覆盖策略级多实体隔离（单 personality，无跨域授权概念）。
2. **可组合，不互斥。** Prima 的内核态（HexArc + COSIR）内部完全可以再划一条 safe/unsafe 边界：COSIR 的硬件细化部分（页表操作、trap 入口、上下文切换、MMIO/DMA、TLB 控制）收敛为唯一允许 unsafe 的"特权框架"，HexArc 的 POSIR 对象能力状态机用纯 safe Rust 实现。特权级边界继续防用户态，类型边界新增防内核内部——两层独立成立。
3. **一个真实张力：服务位置。** Prima 把文件系统/网络/驱动放用户态（H2 多 LibOS 共存与最小权限拆分的要求），Asterinas 明确把驱动留在内核态 safe Rust（Expressiveness 要求，理由是驱动占成熟内核代码约 70%）。这是设计差异而非缺陷：Prima 用 IPC 成本换隔离与可替换性，Asterinas 用类型安全换零 IPC 成本。对 Prima 的提示是：**"用户态"不是 TCB 压缩的唯一杠杆**——即使维持用户态服务架构，内核态内部的 unsafe 收敛仍有独立的 TCB 收益。
4. **策略注入是对 Prima 直接可用的思想。** Asterinas 把调度器、帧分配器、slab 分配器移出 TCB（机制留 TCB 内强制不变量）。Prima 阶段 2 实现 Task/调度与物理帧分配时，同样可以把"选择哪个 Task 运行""选哪个帧"作为可注入策略，使策略 bug 不破坏内存安全与（Prima 语境下的）对象归属不变量。

## 4. 维度二：架构分层映射

| Prima 层（`Prima OS.md` §4） | Asterinas 对应 | 关系判定 |
|---|---|---|
| POSIR（平台无关对象能力状态机规范） | 无对应 | **Prima 特有**：Asterinas 无抽象规范层，其"规范"即 OSTD API 文档与 soundness 论证 |
| COSIR（x86_64 细化：页表/trap/handle/ABI/细化关系） | **OSTD**（`ostd/src`：arch/、mm/、task/、user.rs、irq/、cpu/、boot/） | **强对应**：OSTD 可视为 COSIR 细化义务的"已兑现版本"（含 TLB、trap、用户态进入、DMA） |
| HexArc（实现 POSIR 的最小可信机制内核） | OSTD 整体作为 TCB | **部分对应**：同为"最小化可信层"哲学；但 HexArc 内含授权策略规则（Capability 派生/衰减），OSTD 刻意把策略注入移出 |
| 用户态系统服务（FS/网络/驱动/加载器） | `kernel/core`（fs/、net/、process/、vm/、syscall/ 等）+ `kernel/core/comps`（block、nvme、virtio、network 等） | **设计冲突**：同功能集，用户态 vs 内核态 safe Rust |
| LibOS / ABI 兼容层（用户态、可按域绑定） | `kernel/core/src/syscall`（Linux ABI 直接在内核实现） | **路线冲突**：可替换用户态 personality vs 内核固化单 ABI |
| Capability（POSIR §4：运行期、跨地址空间、可派生 token） | `kernel/libs/aster-rights`（类型级 rights，编译期）+ OSTD 资源 RAII 所有权（Frame/Segment 等） | **弱对应**：Asterinas 的"权利"是类型系统内的编译期约束，不是跨 Task 的运行期授权 token；两者可共存于不同层 |
| Audit / Namespace / Node | 无对应 | Prima 特有 |

映射结论：两套架构在**细化层（COSIR↔OSTD）高度同构**——这是可借鉴度最高的层；在**服务与兼容层路线相反**——这是不可调和、也不应调和的设计差异（源自 H1/H2 的多 personality 目标）。Asterinas 没有 POSIR 式抽象规范，因此也没有"细化关系证明"义务；Prima 保留这一义务意味着 BLK-001 类工作不可替代，但 OSTD 的抽象划分（哪些进框架、哪些注入、哪些留服务）可作为 COSIR 细化层组织的参照。

## 5. 维度三：对 H1–H4 与当前切片的启示

以下均为**分析判断**（非新证据）；判定为"支持/挑战/中立"指 framekernel 的既有事实对该假设论证的影响。

### H1 — 机制表达力：**间接支持（强度有限）**

OSTD 以约 10K 行的"小而表达力强"安全 API 集合，支撑了 240+ syscall 的完整 Linux personality（含驱动）——这为"有限机制可表达完整 OS 语义"的一般方向提供了一个工业级实例。但证据强度有限：Asterinas 的机制在内核态、面向单一 personality，且允许按需要扩展框架；H1 的额外约束（无 OS 专属对象、多 personality、语义保留矩阵）未被其检验。

### H2 — 多 LibOS 共存：**中立**

Asterinas 单 personality、无多 LibOS 概念，不产生支持或反例。其类型级 rights（编译期）与 Prima 的运行期 Capability 不在同一层，无法互相替代。

### H3 — 小型可信机制层：**最强启示（方法级挑战 + 路线级支持）**

- **路线级支持**：framekernel 证明"小而受控的可信层 + 大而不受信的功能层"在通用 OS 上工程可行，TCB 可低至代码量 14% 且增长受控。这与 H3 的方向一致。
- **方法级挑战**：Asterinas 给出一套**可度量、可执行**的 TCB 收敛方法——LCS 度量 unsafe 占比、策略注入控制增长、KernMiri 系统性 UB 检查。Prima 的 H3 验证条件目前是定性的（"足够小且规则化"）且依赖尚未启动的机器证明（BLK-001）。framekernel 提示三点可采纳改进：(1) 给 H3 配一个**量化指标**（如可信代码行/总行数占比）使"小"可证伪；(2) 内核态 unsafe 收敛 + 策略注入可在不改动 Prima 架构的前提下进一步压缩 TCB；(3) **KernMiri 式可执行检查是 BLK-001 之外的务实验证路线**，成本低且已被同行验证有效——不是形式化的替代品，但能覆盖"实现是否引入 UB"这一形式化暂时够不到的风险面。

### H4 — 兼容性与性能渐进获得：**路线竞争证据（需记录在案的反方基线）**

Asterinas 以"内核直接实现 Linux ABI"路线，三年达成 240+ syscall、性能与 Linux 持平、生产场景落地（云/TEE/容器）——这证明"内存安全内核 + Linux 兼容"可行，但其兼容不是"渐进获得"的，而是从第一天就内核固化。对 H4 的含义：Prima 需要论证其渐进 LibOS 路线（原生 → POSIX 源码 → Linux ELF）相比直接 ABI 实现的增量价值（多 personality、OS 中立性、可替换 LibOS），否则"渐进"不构成优势而是成本。建议把 Asterinas 的兼容度与性能数字作为 H4 的反方基线写入后续兼容路线的评估依据。

### 对当前垂直切片（COW → trap → TLB → Linux trace）的启示

1. **untyped memory 直接解答了 COW 切片已知的抽象缺口。** Prima host 模型"不模拟内存字节"，导致"复制内容"无法表达（SPEC-015 已如实注明）；OSTD 的 UFrame/USegment 用 POD read/write 接口在真实内核层表达外部可变内存的读写——这正是 COW 切片进入真实 trap 层后表达"复制帧内容"所需的抽象形态。切片设计真实复制路径时应参照此模式（读写接口而非 Rust 引用）。
2. **OSTD 的 VmSpace/UserMode/UserContext 与本切片的 page fault → exc_resume 路径逐点对应**；其 TLB 处理在 `ostd/src/mm/` 内（本报告未深入，切片进入真实 TLB 时可对照源码）。
3. Asterinas 的 fork（`fork` syscall flag 覆盖 💯，来源 5）是内核自动 COW 的实现实例；Prima 的 ISSUE-COW-001 已判定"内核式自动 COW 不可由既有原语表达"为用户态 COW 的边界澄清。Asterinas 不推翻该判定（其 COW 在内核态、无 capability 约束），但它是"内核态自动 COW 与小 TCB 可兼得"的现存证据——若未来 LibOS 兼容要求内核式 COW 性能，可沿"框架新增安全抽象"而非"新增 POSIR 原语"的方向评估，这与 H1 的约束相容性需届时判决。

## 6. 差距清单与可选行动项

定性分为：**可采纳**（低成本、与架构方向一致）、**需实验判决**（价值依赖实证）、**不适用**（与 Prima 目标冲突，仅记录差异）。本次不执行任何行动项。

| ID | 差距 / 发现 | 定性 | 说明 |
|---|---|---|---|
| G1 | §16 研究对照表无 framekernel/Asterinas 条目 | 可采纳（文档） | 补一行对照：借鉴点=safe/unsafe 内核内特权分离与 TCB 度量；区别=单 ABI 内核固化 vs 多 LibOS 渐进 |
| G2 | `kernel/` 无 unsafe 收敛纪律 | 可采纳（约束） | 趁当前仅 boot 闭环、代码量小，立规成本最低：未来内核代码按"唯一允许 unsafe 的细化 crate + safe 机制 crate"组织；写入项目约束文档（非 POSIR 语义） |
| G3 | H3 的"小"无量化指标 | 可采纳（规范表述） | 给 H3 验证条件增加可证伪度量（可信代码行占比及其增长趋势），参考 LCS 方法但可用简单行数起步 |
| G4 | 无 Miri 类可执行 UB 检查 | 需实验判决 | 试点 KernMiri 式检查（或评估 Miri 对 host 模型的适用性）；与 BLK-001 形式化互补而非替代；试点后再决定是否入验证路线 |
| G5 | 调度器/分配器策略未与机制分离 | 需实验判决 | 阶段 2 实现 Task/调度与帧分配时按"机制留可信层、策略可注入"评估；判决点是注入边界能否同时保住对象归属不变量 |
| G6 | 服务内核态化 | 不适用（记录差异） | 与 H1/H2 的多 LibOS、最小权限用户态服务冲突；不采纳，文档中说明差异理由即可 |
| G7 | Linux ABI 内核固化实现 | 不适用（记录反方基线） | 与 H4 渐进 LibOS 路线冲突；其 240+ syscall 与性能数字应作为 H4 评估的反方基线记录 |
| G8 | unsafe 审计与 crate 重组路线图 | 后续工作 | 当前代码量小，审计过早；待 kernel 代码增长后执行（本报告不展开） |
| G9 | COW 真实复制路径缺抽象形态 | 可采纳（切片输入） | 切片进入真实 trap 层时参照 OSTD untyped memory 的 POD read/write 接口设计帧内容读写；属切片设计输入，非本报告执行 |

## 7. 证据边界与不确定性

1. Asterinas 侧性能与 TCB 数字均为论文自报（ATC'25，2025-07），本报告未独立复现；syscall 数有两个版本（论文发表时 210+，book 当前 240+），以时间戳区分。
2. 官方 book 与仓库结构访问于 2026-08-16；`the-approach/capabilities.md` 与 `the-approach/framekernel.md` 为占位 stub（26/34 字节），类型级 rights 的细节未获取，相关判断以论文与 `kernel/libs/aster-rights` 目录存在性为限。
3. 本报告未深入 OSTD 的 TLB shootdown、fork/COW 的具体实现（仅确认其存在与所属模块）；切片进入真实 TLB/COW 实现阶段时需补源码级对照。
4. 所有"启示"与"差距"是分析判断，不构成 H1–H4 的新证据，也不改变任何退出条件状态；采纳任一行动项需走既有设计→计划→实施流程。
5. 论文的 soundness 论证以 x86-64 为目标（官方称可推广到其他架构）；Prima 若采纳 safe/unsafe 边界思想，其跨架构含义需在第二架构（阶段 7 候选）时重新评估。
