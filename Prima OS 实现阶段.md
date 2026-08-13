# Prima OS 实现阶段

> 文档类型：实施进度与证据跟踪
>
> 更新日期：2026-08-07
>
> 架构与阶段定义：[[Prima OS]]
>
> 精确语义规范：[[COSIR 原语规范]]

## 维护规则

本文只记录实际进度、退出条件证据和阻塞项，不重新定义架构或原语语义。

状态只允许：

- **未开始**：尚无可验收交付物；
- **进行中**：已有工作产物，但尚未满足全部退出条件；
- **阻塞**：存在阻止继续推进的明确问题；
- **已满足退出条件**：全部退出条件均有可复查证据。

维护约束：

1. “已设计”或“已定义”不等于“已实现”；
2. 检查项只有在证据列提供可访问工件后才能勾选；
3. 测试证据记录命令、日期和结果摘要；
4. 证明证据记录工具、入口文件和定理或检查名称；
5. 实验失败和反例必须保留，不得通过修改措辞隐藏；
6. 阶段定义变化先修改 [[Prima OS]]，语义变化先修改 [[COSIR 原语规范]]，再同步本文。

## 当前快照

| 字段 | 当前值 |
|---|---|
| 当前阶段 | 阶段 0 — 语义基线 |
| 当前状态 | 进行中 |
| 已满足退出条件的最高阶段 | 无 |
| 当前实现代码 | 阶段 1 启动闭环探索实现：`kernel/`、`xtask/` |
| 当前主要工件 | [[Prima OS]]、[[COSIR 原语规范]]、Rust/QEMU 启动闭环 |
| 当前首要缺口 | 形式化工具中的可执行状态模型（BLK-001） |
| 最近规范修订 | 2026-07-31 — 规范审查修复：ObjectRef 统一、ExternalInterrupt 语义明确、CapMoved 审计事件补充、Node 对象完整定义、DMA 帧/页帧信息流说明、POSIR/COSIR 边界清单、ns_mount 跨节点约束、task_create 增加 kind 参数、task_yield 调度时机明确、应用域/LibOS 关系精确化、细化要求从 21 项扩展到 25 项 |
| exc_resume（本切片） | 2026-08-07 — 定义异常恢复原语 `exc_resume`、x86_64 SretFrame 帧布局（细化层定义）、Faulted 细化态与错误码；新增 §12.10，标注细化要求 #16 与边界清单 #1/#2 |
| 最近实现证据 | 2026-08-06 — 固定 30 秒失败保护后 `cargo xtask test` 在 QEMU 11.0.50 上连续完成两次 BIOS 启动；整条验收耗时 17.156 秒，23 个 host 单元测试全部通过 |
| 最近细化证据 | TEST-004：2026-08-13 在 commit `60a61bd0e01d568541c43edf557ac12b341e0a1f` 上，Rust `MODEL-TEST` 69 tests 与 workspace 92 tests 通过；不代表真实 CR3/PTE refinement，证据边界见 TEST-004。 |
| BLK-002 细化（本切片） | 2026-08-13 — MemorySegment host 四级索引树与双视图的 `REFINEMENT-DESIGN`；`MODEL-TEST` 检查 canonical 地址拒绝、失败原子性、α mutation 隔离和 allocator partition。`cargo test -p cosir` 69 tests 通过；未覆盖真实 CR3/PTE、非叶权限与页表页物理身份、TLB、多核、真实硬件原子更新窗口或机器证明。 |

当前已形成架构白皮书和抽象语义规范，并有阶段 1 启动闭环探索实现；尚无机器证明、物理内存发现、页表与 trap 框架、POSIR Bootstrap 或兼容性测试证据。

## 证据索引

| ID | 类型 | 工件或命令 | 日期 | 结果 | 支持的检查项 |
|---|---|---|---|---|---|
| DOC-001 | 架构文档 | [[Prima OS]] | 2026-07-30 | 已形成决策型架构白皮书；不代表实现完成 | S0-D3 |
| SPEC-001 | 抽象规范 | [[COSIR 原语规范]] | 2026-07-30 | 已定义对象能力状态机（11 类原语、16 个不变式）；尚无机器证明 | S0-D1、S0-D2 |
| IMPL-001 | 实现工件 | `Cargo.toml`、`kernel/`、`xtask/`、`Cargo.lock` | 2026-08-05 | 固定 nightly-2026-07-30 与 bootloader 0.11.17；生成 BIOS/UEFI 镜像 | S1-D1、S1-D2 |
| TEST-001 | QEMU 集成测试 | `cargo xtask test` | 2026-08-05 | QEMU 11.0.50；固定 BIOS profile 连续启动两次，结构化串口序列和退出码均通过 | S1-D1、S1-D2、S1-X1 |
| TEST-002 | 最终审查加固验收 | `cargo test -p xtask`、`cargo xtask test` | 2026-08-06 | 23 个 host 单元测试通过；严格 LF/CRLF、reader 启动失败 cleanup 与 UART TEMT drain 加固后，QEMU 11.0.50 固定 BIOS profile 连续启动两次并输出六条稳定 PASS 摘要 | S1-D1、S1-D2、S1-X1 |
| TEST-003 | 高负载失败保护验收 | `cargo test -p xtask`、`cargo xtask test` | 2026-08-06 | 固定 30 秒失败保护、不增加自动重试；23 个 host 单元测试通过，QEMU 11.0.50 两次 BIOS run 输出六条 PASS，整条验收耗时 17.156 秒 | S1-D1、S1-D2、S1-X1 |
| SPEC-012 | 细化规范与可执行模型 | `COSIR 原语规范.md` §12.10、`cosir/` | 2026-08-07 | 定义 `exc_resume` 原语、x86_64 SretFrame 与 Faulted 细化态；`cargo test -p cosir` 12 tests 通过；`cargo xtask test` 六条 PASS | S0-D1、S0-D2 |
| SPEC-013 | 细化规范与可执行模型 | `COSIR 原语规范.md` §20、`cosir/src/machine.rs`、`cosir/src/abstract_ops.rs` | 2026-08-07 | 定义 x86_64 MachineState、抽象函数 α 与 Bootstrap 细化对应；`cargo test -p cosir` 17 tests 通过；`cargo xtask test` 六条 PASS | S0-D4 |
| SPEC-014 | MODEL-TEST + REFINEMENT-DESIGN | COSIR 原语规范 §20.5、cosir/src/pagewalk.rs、cosir/src/memseg_refine.rs | 2026-08-12 | host 四级索引树与 vspace 双视图设计；`MODEL-TEST` 检查 W6/W7，W8 仅有 unmap entry-page guard；未覆盖非叶权限、页表页物理身份、TLB、多核、真实 CR3/PTE 与原子更新窗口；`cargo test -p cosir` 42 tests 通过 | S0-D4 |
| TEST-004 | 阶段一命令验收 | `cargo fmt --all --check`、`cargo test -p cosir`、`cargo clippy --workspace --all-targets --no-deps -- -D warnings`、`cargo test --workspace`、`cargo xtask test` | 2026-08-13 | 在 `60a61bd0e01d568541c43edf557ac12b341e0a1f`，cargo 1.99.0-nightly (3efb1f477 2026-07-17)、rustc 1.99.0-nightly (1a833e165 2026-07-29) 上全部退出码 0：fmt 无差异；cosir 69/69；workspace cosir 69/69 + xtask 23/23（92/92）；clippy 无 warnings；QEMU BIOS run=1/2 输出六条 PASS。直接支持 host `MODEL-TEST`、lint 与既有 QEMU 启动闭环无回归；不支持真实 CR3/PTE 细化、非叶权限/页表页物理身份、TLB、多核、真实硬件原子更新窗口、机器证明或阶段一退出条件已满足。 | S0-D4、S1-D1、S1-D2、S1-X1 |

新增证据时使用稳定 ID；阶段检查项只引用该表中的 ID，不复制大段输出。

## 已知问题与纠偏计划

| ISSUE ID | 问题与影响 | 当前证据 | 修复阶段 | 可执行验收条件 | 状态 |
|---|---|---|---|---|---|
| ISSUE-MEM-001 | pagewalk 接受未对齐/非 canonical 地址，high-half 投影缺少符号扩展 | MODEL-TEST 暴露缺口 | 阶段一 | 地址别名、对齐、高低半测试通过；`cargo test -p cosir pagewalk_tests -- --nocapture`、`cargo test -p cosir memseg_refine_tests -- --nocapture`、`cargo test -p cosir` | Fixed |
| ISSUE-MEM-002 | 地址、页数、offset、usize 转换和 SegId 递增可能溢出 | MODEL-TEST 暴露缺口 | 阶段一 | 每个溢出返回稳定错误且状态不变；`cargo test -p cosir pagewalk_tests -- --nocapture`、`cargo test -p cosir memseg_refine_tests -- --nocapture`、`cargo test -p cosir` | Fixed |
| ISSUE-MEM-003 | α 未验证物理帧、offset 和 PTE 权限位 | REFINEMENT-DESIGN 不完整 | 阶段一 | 七项独立 mutation 均被拒绝 | Fixed |
| ISSUE-MEM-004 | allocator 未实施严格有限集合语义 | MODEL-TEST 暴露缺口 | 阶段一 | 重复、外部帧、重复归还均显式失败 | Fixed |
| ISSUE-MEM-005 | memseg_create 错误要求预存 Segment 映射权 | 与 POSIR §7.1 不一致 | 阶段一 | 删除 seg_cap_rights 参数 | Fixed |
| ISSUE-MEM-006 | SPEC-014/PASS 可能被外推为真实页表细化 | MODEL-TEST + REFINEMENT-DESIGN | 阶段一 | 三份权威文档和 harness 文案一致 | Fixed |
| ISSUE-EXC-001 | exc_resume 未检查 Capability、持有者、活跃状态和订阅 | MODEL-TEST 缺口 | 阶段二 | 每项授权 mutation 原子失败 | Open |
| ISSUE-EXC-002 | 恢复帧没有实际字节编码，RIP/RSP/CS/SS/RFLAGS 未完整验证 | MODEL-TEST 缺口 | 阶段二 | wire-format 与字段 mutation tests 通过 | Open |
| ISSUE-EXC-003 | 成功恢复未清零用户帧、保存 kernel frame 或追加 Audit | MODEL-TEST 缺口 | 阶段二 | 成功提交顺序和重复消费测试通过 | Open |
| ISSUE-EXC-004 | §12.9/§12.10、旧设计与实现接口矛盾 | REFINEMENT-DESIGN 不一致 | 阶段二 | 三份文档与最终接口一致 | Open |

## 研究停止条件与下一垂直切片

阶段一采用以下停止条件：若真实实现要求修改已冻结的 POSIR observable semantics、需要新增 OS/架构专属原语才能维持映射、`COMPAT-TEST` 出现不可接受且无法由既有机制表达的语义偏差，或真实页表/trap trace 无法建立到抽象迁移的可审查对应，则停止扩大实现并记录为 H1/H3 或细化分层的反例。`MODEL-TEST` 或 `REFINEMENT-DESIGN` 不能解除这些停止条件，也不能单独升级为 refinement 结论。

下一垂直切片固定为：**COW/page fault → handler → `exc_resume` → TLB → Linux trace**。该切片必须先冻结 page-fault/COW 的 observable semantics，再以真实 trap、页表更新和 TLB 行为建立端到端 trace，最后与选定 Linux 行为形成 `COMPAT-TEST`；在该闭环完成前不横向扩大 MemorySegment、跨架构或异构实现范围。

## 当前阻塞项

| ID | 阶段 | 问题 | 解除条件 | 状态 |
|---|---|---|---|---|
| BLK-001 | 0 | 尚未选择并建立形式化模型工件 | 明确工具、入口文件和可执行状态迁移 | Open |
| BLK-002 | 0 | 草案已放宽——已覆盖 MemorySegment 页表↔vspace 细化 | 定义 MachineState、抽象函数和至少 Bootstrap（及 MemorySegment）细化对应 | 草案已成 |

## 决策日志

| 日期 | 决策 | 理由 | 影响 |
|---|---|---|---|
| 2026-07-30 | 首个平台采用 QEMU x86_64 | 先验证启动链、语义和细化关系 | 阶段 1–3 以单核 QEMU profile 为准 |
| 2026-07-30 | 兼容路线采用原生服务 → POSIX 源码 → Linux 静态 ELF | 分离原生模型验证与传统 ABI 复杂度 | 对应阶段 4–6 |
| 2026-07-30 | 不把原语数量作为约束 | 以语义最小、边界清晰和可证明为准 | 新机制必须经过必要性与中立性审查 |
| 2026-07-30 | 新增 Exception 原语而非依赖 Send/Recv 模拟 | 异常/中断的不可伪造来源、广播递送和硬件同步语义与通用消息 IPC 根本不同 | 原语集从 9 类扩展到 11 类；异常事件源作为一等对象 |
| 2026-07-30 | 新增 Device 原语封装 I/O 资源 | MMIO/DMA 的物理资源隔离、地址连续性和缓存策略与通用 Segment 根本不同 | I/O 对象独立于 MemorySegment；用户态驱动通过 Capability 访问硬件 |
| 2026-07-31 | 规范审查与修复 | 修正 COSIR 原语规范中 18 项语义/一致性/完整性/实现细节问题 | 见审查报告（本次会话）；ObjectRef 统一、CapMoved 审计事件补充、Node 完整定义、POSIR/COSIR 边界清单等 |
| 2026-08-06 | BIOS 测试失败保护固定为 30 秒 | 原 10 秒阈值在主机 CPU 高负载下出现间歇误报；同一镜像与参数在负载回落后实测仍需 7.079 秒启动 | 不增加自动重试；成功仍必须同时满足 QEMU 退出码 33 与严格 PRIMA 事件序列 |

## 阶段 0 — 语义基线

**状态：进行中**

### 交付检查

| ID | 交付物 | 状态 | 证据 |
|---|---|---|---|
| S0-D1 | 一致的对象能力状态模型 | [x] 已定义，未证明 | SPEC-001 |
| S0-D2 | 操作、错误、全局不变量和非目标 | [x] 已定义，未证明 | SPEC-001 |
| S0-D3 | 关键研究假设与反例标准 | [x] 已定义 | DOC-001 |
| S0-D4 | x86_64 机器状态到 POSIR 状态的细化草案 | [x] 已定义，未证明 | SPEC-013 |

### 退出条件

| ID | 条件 | 满足 | 证据 |
|---|---|---|---|
| S0-X1 | 规范没有已知且未记录的对象生命周期空洞 | [ ] | 需要独立规格审查或机器检查证据 |
| S0-X2 | 每项操作具有明确成功与失败语义 | [ ] | 需要逐操作覆盖报告 |
| S0-X3 | 形式化工具能够表达并执行状态与迁移 | [ ] | BLK-001 |

阶段 0 不能因文档篇幅完整而直接关闭；必须补齐形式化工件、覆盖报告和细化草案。

## 阶段 1 — 启动与最小平台层

**状态：进行中**

### 交付检查

- [x] S1-D1：QEMU 可重复启动；证据：TEST-001、TEST-002、TEST-003（固定 BIOS profile 连续两次）。
- [x] S1-D2：串口结构化输出与确定性退出；证据：IMPL-001、TEST-001、TEST-002、TEST-003。
- [ ] S1-D3：物理内存发现；证据：页帧清单与保留区域测试。
- [ ] S1-D4：初始页表、GDT/IDT 与 trap 框架；证据：异常和权限测试。
- [ ] S1-D5：Bootstrap 创建 init Task；证据：初始对象和 Capability 快照。

### 退出条件

- [x] S1-X1：脚本可无人值守启动并退出 QEMU；证据：TEST-001、TEST-002、TEST-003。
- [ ] S1-X2：init 只获得规范定义的初始 Authority。
- [ ] S1-X3：活动 Segment 与空闲页帧满足唯一归属不变量。

## 阶段 2 — Capability、Segment、Task 与 IPC

**状态：未开始**

### 交付检查

- [ ] S2-D1：不可伪造且不复用的句柄表示。
- [ ] S2-D2：Capability 派生、移动、drop 与 stale 检测。
- [ ] S2-D3：Segment create/map/unmap/destroy。
- [ ] S2-D4：Task create/yield/destroy 与单核调度。
- [ ] S2-D5：Send/Recv、FIFO 邮箱与阻塞恢复。

### 退出条件

- [ ] S2-X1：每项成功和失败路径具有状态转换测试。
- [ ] S2-X2：两个不可信 Task 只能经授权消息或共享 Segment 交互。
- [ ] S2-X3：页帧在对新 Segment 可见前清零。
- [ ] S2-X4：stale handle 不会因 ID 复用重新获得 Authority。

## 阶段 3 — Namespace、Timer 与 Audit

**状态：未开始**

### 交付检查

- [ ] S3-D1：Namespace bind/unbind/mount/unmount/resolve。
- [ ] S3-D2：Mount 环检测与 stale Mount 清理。
- [ ] S3-D3：绝对单调 Timer。
- [ ] S3-D4：`deadline ≤ now` 时原子立即触发。
- [ ] S3-D5：Capability 派生、位置变化与对象生命周期 Audit。

### 退出条件

- [ ] S3-X1：所有 Namespace 解析均终止，活动 Mount 图无环。
- [ ] S3-X2：每个 Fired Timer 恰有一次对应内核通知。
- [ ] S3-X3：Audit 能重建 Capability 派生链和位置历史。
- [ ] S3-X4：Task、Namespace、Message 或 Timer 容器销毁不破坏其他对象生命周期。

## 阶段 4 — Prima 原生运行时与服务

**状态：未开始**

### 交付检查

- [ ] S4-D1：原生运行时 API 与版本规则。
- [ ] S4-D2：init / 服务管理器。
- [ ] S4-D3：日志、Namespace 和基本存储服务。
- [ ] S4-D4：第一条用户态驱动或受控 I/O 路径。
- [ ] S4-D5：使用消息与共享 Segment 的原生示例应用。

### 退出条件

- [ ] S4-X1：不依赖 POSIX LibOS 即可自举并运行应用。
- [ ] S4-X2：服务按最小权限拆分，证据列出各服务初始 Capability。
- [ ] S4-X3：大数据路径可使用共享 Segment，且有取消和错误恢复测试。
- [ ] S4-X4：故障服务不能直接破坏无关应用域。

## 阶段 5 — POSIX 源码兼容

**状态：未开始**

### 交付检查

- [ ] S5-D1：冻结且版本化的 POSIX profile。
- [ ] S5-D2：libc 适配。
- [ ] S5-D3：文件描述符、路径、线程和基础网络子集。
- [ ] S5-D4：逐 API 语义映射矩阵。
- [ ] S5-D5：与参考 Linux 的差分测试矩阵。

### 退出条件

- [ ] S5-X1：profile 内每项 API 都有语义映射和可重复测试。
- [ ] S5-X2：每项 API 明确标记 supported / partial / unsupported。
- [ ] S5-X3：选定开源程序可重新编译并通过冻结的功能测试集。

## 阶段 6 — Linux x86_64 静态 ELF

**状态：未开始**

### 交付检查

- [ ] S6-D1：静态 ELF 加载器与 Linux 初始栈。
- [ ] S6-D2：OS 中立的 syscall / exception 递送机制。
- [ ] S6-D3：版本化的 Linux LibOS syscall profile。
- [ ] S6-D4：所选 profile 必需的 futex、TLS 和进程环境。
- [ ] S6-D5：未修改静态 ELF 白名单与测试镜像。

### 退出条件

- [ ] S6-X1：白名单中的静态 ELF 无需修改即可运行。
- [ ] S6-X2：支持的 syscall 与参考 Linux 通过差分测试。
- [ ] S6-X3：HexArc / COSIR 不包含 Linux 专属对象或 syscall 语义。
- [ ] S6-X4：未支持 syscall 得到确定且可审计的失败。

## 阶段 7 — 扩展与硬化

**状态：未开始；范围未冻结**

候选工作不预先视为承诺：

- [ ] SMP；
- [ ] IOMMU 与更完整的用户态驱动；
- [ ] 动态链接；
- [ ] 资源计量与配额；
- [ ] 选择性撤销；
- [ ] 更强的故障恢复；
- [ ] 第二硬件架构。

只有在阶段 0–6 的证据揭示实际优先级后，才把候选项拆成带退出条件的独立里程碑。

## 阶段晋级规则

1. 阶段状态由该阶段的退出条件决定，不由提交数量、代码行数或主观完成度决定；
2. 前一阶段未满足退出条件时，后一阶段可以做探索原型，但不能成为“当前已完成阶段”；
3. 任何失败测试、未解决反例或实现偏差都必须进入阻塞项或决策日志；
4. 关闭阶段时，在证据索引中记录完整命令、工件版本和结果；
5. 架构或语义变化必须先更新权威文档，再重新评估受影响的已关闭检查项。
