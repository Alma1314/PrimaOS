---
id: report-architecture-isa-re-review
kind: report
authority: scoped-design-review
status: reviewed-with-open-findings
lifecycle: historical
updated: 2026-11-27
---

# 架构规划修订与异构 ISA 复核记录

维护说明：当前[融合内核方案](../specs/2026-11-24-heterogeneous-isa-hybrid-execution-design.md)
已直接重写为工作设计，移除旧提案、决策选项表、平台比较和旧分期。本文保留审查当时的发现、
旧段号/门编号与证据边界，不再充当现行方案或当前实施清单；未修复的撤销容器偏差仍按阶段跟踪处理。

## 结论与范围

作者要求“修订问题并重新审核异构 ISA 提案”。本次修订上一轮的七项设计/契约问题，
并重新审查[异构 ISA 提案](../specs/2026-11-24-heterogeneous-isa-hybrid-execution-design.md)。
执行中作者进一步要求“将复制内核方案改为融合内核方案”并指定参考 UNIFICO。最终结论：
**系统层改为同一融合域多 ISA 执行体共享通用权威状态；应用层仍为 proposal，不具备 v1 实施准入。**
guest 状态迁移混淆、ABI 选择、原子互操作、worker 保护边界及故障语义已作文本纠偏；
平台、引擎、原样 ELF 白名单与协议/证明仍未交付，不能把补上要求视为验证通过。

本次仅修改文档与生成目录；没有 SMP、GS、撤销或 DBT 代码变化，没有 OS 运行/证明新证据，
不是独立第三方复核、profile 冻结、组织级签署或新增项目排期。旧独立状态部署决定仅作为历史保留，
当前融合方向以作者新指令为准；无系统组件活迁移等未被改变的边界保留。参考 UNIFICO 不代表
作者已选择其工具链，新增建议不冒称平台、引擎、共享字段布局或应用范围已批准。

## 基线与输入

- 基线：`1c283ec`，开始时工作树干净；上一轮只读审查的七项发现为本次输入。
- 上位：[白皮书](../../../Prima%20OS.md)、[原语规范](../../../COSIR%20原语规范.md)、
  [阶段 7 范围裁决](../specs/2026-10-31-stage7-scope-decisions.md)、[当前阶段](../../../Prima%20OS%20实现阶段.md)。
- 相关设计：[S7-B.1](../specs/2026-11-27-stage7-smp-state-partition.md)、
  [Windows 阻断处置](../specs/2026-11-27-windows-blocker-decisions.md)。
- 原始证据：[P5-3a](2026-11-27-p5-3a-percpu-gdt-tss.md)、
  [Lean Revoke](2026-11-25-lean-revoke.md)、[Gate 容器特定整改](2026-11-06-stage0-6-review-remediation.md)。
  后两者不建立普通子树撤销与容器的完整等价性。
- 静态核对：`prima-ostd/src/entry.rs` 的入口栈/现场；`kernel/src/vspace.rs::clone_root_cow`；
  `kernel/src/preempt.rs::time_ticks` 与 `BootstrapState::expire_waits_on`；
  `revoke_capability`/`retire_capability`/`ns_unbind`；`lean/Formal/Revoke.lean::revoked`。

## 上轮七项的修订与保留项

| 项 | 本次文本修订 | 尚未完成/关闭条件 |
|---|---|---|
| A1 SMP 入口现场遗漏 | S7-B.1 补入 `SYSCALL_STACK`、`USER_RSP`、`CONTEXT_BASE`、`RESUME_FRAME_PTR` 和故障记录；列为 AP 用户态准入条件 | 实现按 CPU/嵌套现场分区；双核同时进入、阻塞恢复与故障负例；当前 AP parked 不覆盖 |
| A2 撤销域矛盾 | 白皮书/COSIR 注记区分授权失效与容器回收；Lean 映照登记具体偏差 | `ISSUE-REVOKE-CONTAINER-001` 保持开放，见下节；未修代码、不声称故障已消失 |
| A3 TLB 分期空窗 | 跨 CPU 活动页表修改/共享 root 前须交付最小 shootdown；受限过渡期必须证明固定 root 所有者、禁止远端修改和共享映射变更 | active CPU 集合、确认/超时与帧回收协议；无法约束现有路径时提前 P5-4 |
| A4 时基不一致 | 本地抢占与统一单调期限分开；统一转换前由 BSP 处理物理期限 | 冻结 epoch/单位/偏移/误差界；偏移、回退、频差和 idle/跨核等待测试 |
| A5 GS 部分启用 | GS-1 仅独立探针；探针仍须处理可达 NMI/异常；普通运行期须 GS-2 与联合调度恢复评审 | 完整入口、MSR 状态识别、混用入口和 NMI 窗口的机器证据 |
| A6 AP 初次放置缺口 | 保留随父 CPU 与不迁移决定；补充未运行任务创建时放置的待评审规则 | P5-3b 实现前冻结发布/回滚与 owner；不同 CPU 同时有有效负载 |
| A7 迁移模型混淆 | guest 固定 ISA 的后端切换与同源双构建组件迁移分开 | 版本化 guest 状态、共同 feature profile、唯一 runnable owner 与不确定提交协议；均属 v2 待验证 |

### ISSUE-REVOKE-CONTAINER-001 的精确范围

`Formal.Revoke.revoked` 使子树能力 `active=false`，仅删除 HeldBy 位置，保留消息与
Namespace 位置。当前内核 `revoke_capability` 不按后代位置过滤，调用 `retire_capability`
删除位置/活动记录并释放槽位，容器仍可能保存旧句柄。静态反例：派生 parent/child，把
普通 child 绑定到 Namespace，撤销 HeldBy 的 parent，再执行 `ns_unbind`；条目被清除后
`move_from_namespace` 可能返回 `NotFound`，违反该操作失败不改变状态的要求。

本次没有实跑该反例，不能把静态路径当作测试 FAIL；已有 HeldBy 子树测试不覆盖它。
Gate 终结的 `retired_gate_request_cap` 特例仅覆盖 Gate 请求，不是普通 child 的解决方案。
后继须先固定容器引用/退休 token 的统一映照，再修复共享退休路径及其调用方，覆盖
Namespace 与消息、槽复用、审计容量不足、成功/失败原子性，重跑 Rust/QEMU 相关验收。
旧报告的有界 PASS 保留；本次不签署 7-D，也不把它改写为失败的历史运行。

## 异构 ISA 重新审核

### 已纠正的设计缺陷

| 发现 | 修订后的约束 | 证据边界 |
|---|---|---|
| R1 guest ISA 与 host 构建混淆 | 前端按 guest ISA 选择；native/DBT 共享固定 feature profile；v2 传 guest PC/逻辑状态，不索取另一个 ISA 的应用映像 | 提案 §5.2/§6.3/§6.6；没有引擎或 guest 实测 |
| R2 缓存一致性被当作原子性保证 | CAS/RMW 宽度、对齐、共享内存属性和 LL/SC 单列；进程内 helper 锁不能约束另一域 native | 提案 §6.7/I2；需锁定引擎与平台交互测试 |
| R3 复用 DBT 被当作安全边界 | worker 可失陷、仅持本 guest 权限；supervisor 私有控制状态不映射给 guest；native 陷入和 translated 重定向均不得绕过授权 | 提案 §6.3/§6.4/I3；W^X 单独不足以提供沙箱 |
| R4 carve-out 被视为 POSIR 不变的充分条件 | 融合方向要求共享内核 ABI、唯一 Authority/资源账本及到单一 POSIR 的联合映照；物理共享不替代不变量 | 提案 §5.1/§5.4/§6.5.1/I1；是否需要新语义仍未决 |
| R5 固定 VA/统一 FD 隐含全局原子性 | 发布前两侧准备成功才运行；首片固定映射；OFD 单 owner、dup/close/偏移在同一会话线性化 | 提案 §6.1/§6.4.1/I4；协议和应用清单未冻结 |
| R6 未修改静态程序可自动使用运行时锁 | 明确审计 libc/malloc/stdio/create/join/退出的隐含同步；无合适 ELF 则不得验收混合域 | 提案 §6.7/§7/P9；不能用改写程序逃避二进制范围 |
| R7 失败与迁移回退可产生双重执行 | v1 无透明恢复、未知 I/O 不重放；共享窗口未 fencing 不复用；v2 提交不明时暂停对账 | 提案 §6.4.1/§6.6/I5；epoch 过滤不阻止旧 CPU/DMA 写入 |
| R8 “所有核”前置不完整 | x86 SMP 与 AArch64 SMP 分开；7-H 只单核机制验证；v0a 引擎验证与 v0b Prima 移植分期 | 提案 §5.2/§8/§10；不向阶段 7 自动增加 ARM SMP |
| R9 融合方向仍残留复制内核语义 | 域内为单一抽象节点/身份域；每 ISA 本地架构状态投影到共享权威；域外独立 Node 才沿用远端 Capability 禁止规则 | 提案 §5.1/§6.5.1、白皮书 §18.5；F0 共享机制证据尚缺 |
| R10 将 UNIFICO 等同于融合内核或原样二进制方案 | 分开 Stramash 的内核机制参考、UNIFICO 的可重编译组件路线及固定 guest ISA 的 DBT；不恢复论文假设的复制内核 | 提案 §5.5/D-ISA-19；编译器源码、Rust/库集成与迁移 profile 均未审计/验证 |

### 作者新决定后的融合内核复核

本次实际改变的是研究架构，不是将“复制”字样改名：共享对象/能力位置/资源账本只有一份
权威记录，架构寄存器/页表/栈/本地队列按 ISA/CPU 保留，主引导只初始化一次初始 Authority，
共享 ABI 禁止裸 Rust 默认布局与跨 ISA 函数指针。F0 要证明两执行体实际访问同一权威对象，
并覆盖并发派生/移动、全局唯一位置、唤醒与映射失效；仅共享 guest 数据页不能算内核融合。

新增共同故障代价也已登记：融合成员内核属于同一可信边界，v1 默认成员内核故障使整个域
fail-stop，不能沿用“另一独立内核仍运行”的恢复主张。worker 崩溃仍按应用故障处理；
共享锁/半提交修复、成员移除及内核恢复没有证据，故保持不承诺。TCB 增量和联合细化需独立计量。

### 仍阻塞 v1 的五道门

| 门 | 当前结论 | 要提交的实际工件 |
|---|---|---|
| I1 平台/共享权威 | 未通过 | 融合拓扑、共享 ABI、唯一引导/加入与资源身份裁决；到单一抽象状态的映照 |
| I2 原子/执行 | 未通过 | 跨 ISA 内核锁/通知/失效/回收；锁定平台/引擎/CPU features 与 native/DBT 原子反例 |
| I3 ABI/引擎 | 未通过 | 引擎版本/许可证/Prima 移植清单、入口授权与 worker 保护域证据 |
| I4 进程/profile | 未通过 | 原样 ELF 哈希、两侧映射/FD/OFD/线程与隐含同步支持矩阵、确定拒绝语义 |
| I5 故障/证据 | 未通过 | 成员内核与 worker/控制面故障分开；全域 fail-stop、超时/fencing、TCB 增量、证明/运行证据 |

F0 须先冻结融合适用的 I1/I2/I5，再交共享内核机制证据；v0a 可以独立为引擎选型收集证据，
不由本报告授权具体工程排期。只有满足本地适用门后才有 v0b，v1 实现须交齐五门评审工件
并取得 F0 证据。没有上述工件不能把“方向已选”升级为“可直接实现”。
特别是未修改第三方 ELF 的混合域可行性仍待证明：若跨域 pthread/futex 是必要前提，
必须重审 v1 范围或提前对应机制，不能以运行时自旋锁文案替代。

### 外部原始来源与使用限制

本次在线读取下列资料，均只作为约束/设计来源，不是 PrimaOS 证据或完整源码审计：

- [QEMU user 手册](https://www.qemu.org/docs/master/user/main.html)：其 syscall/signal/线程模型依赖所支持宿主；本文据此要求独立验证 Prima 移植与重定向，未断言现成可嵌入。
- [QEMU TCG 安全策略](https://www.qemu.org/docs/master/system/security.html#non-virtualization-use-case)：不能把 TCG 当作恶意 guest 的隔离保证；外层最小权限边界由 Prima 设计承担。
- [Stramash 原论文 §6.5/§7](https://ssrg.ece.vt.edu/papers/asplos25.pdf)：用于对照共享内核状态/本地架构状态、跨 ISA 原子及通知；Prima 采用融合方向后仍须自行定义 Capability 与失败原子性，不照搬 Linux 对象或外推性能。
- [UNIFICO CC '24 论文](https://www.pure.ed.ac.uk/ws/portalfiles/portal/416254222/UNIFICO_MAVROGEORGIS_DOA23122023_AFV_CC_BY.pdf)与[作者工件目录](https://zenodo.org/records/10567311)：用于编译路线参考；其 OS 假设与当前融合决定分别解释。只读论文/目录，未下载工件、审计编译器源码或移植；不能据此宣称 Rust 内核、当前 musl 或多线程迁移已支持。
- [CXL Consortium Q&A](https://computeexpresslink.org/blog/cxl-3-0-webinar-qa-recap-1075/)：GFAM back-invalidate 涉及设备机制；不能从 CXL 标签推出一个已选平台、native/DBT 原子互操作或性能结论。
- [Linux x86 入口说明](https://www.kernel.org/doc/html/v6.2/x86/entry_64.html)与[TLB 接口约束](https://docs.kernel.org/core-api/cachetlb.html)：支持本次对 NMI 交换窗口与跨核失效前置的审查；不要求照搬 Linux 实现。

## 命令与结果

| 命令 | 本次结果 | 范围 |
|---|---|---|
| `python3 scripts/docs.py self-test` | PASS，exit 0 | Markdown/元数据/检索/确定性/生命周期/Git 恢复/变更守卫 |
| `python3 scripts/docs.py build` | PASS，exit 0；448 documents | 生成目录，不代表语义审查通过 |
| `python3 scripts/docs.py check` | PASS，exit 0；448 documents | 身份、主题、本地链接/锚点与目录新鲜度 |
| `python3 scripts/docs.py check --base 1c283ec` | PASS，exit 0 | 新文档元数据与删除/恢复守卫；无删除 |
| `git diff --check` | PASS，exit 0 | 空白检查 |

本次 Rust、QEMU、Lean、DBT、参考 Linux/Windows 执行均未运行，因为没有可执行实现改动；
不能引用历史 PASS 充当本次新证据。上述命令在最终文本更新后重新执行，原始输出不入库。

## 阶段/profile 影响与后继

阶段 7 保持进行中，AP parked、7-B/7-D 未签署；所有退出勾选、已冻结 profile 和默认启动配置保持。
阶段跟踪登记 `ISSUE-REVOKE-CONTAINER-001`，并链接本次设计修订；报告/规格索引与主题路由同步。
本报告补强上次只读发现和异构提案的设计边界，不取代任何已存运行/证明报告。

平台、引擎、guest feature、白名单、故障范围、授权表达或窗口回收方式变化时重新审核。
后续实施应分别提交契约、代码/反例、实际结果与映照，不能统一勾选本报告中的待办制造完成。

## 后继：方案正文收敛

按作者“异构 ISA 提案直接修改成融合内核方案，移除以前的提案和方案”的要求，原路径与
文档身份保留，正文直接重写为当前融合内核工作设计。移除旧部署选项、决策演变表、平台比较、
应用执行备选方案和旧分期；现行实施顺序为共享基座、对象融合、执行融合、集成验收。
UNIFICO 保留为统一编译布局参考，不作为融合内核实现或 Rust 迁移证据。

白皮书、原语规范、阶段跟踪、主题路由及规格/报告索引同步到当前方案。上文复核作为历史记录
保留，不再作为现行实施清单；独立失败发现、现存证据与删除文件的 Git 恢复记录仍可追溯。
本次仅有文档变更，不新增系统实现、冻结契约、阶段签署或运行/证明证据。

正文改写后的文档验证（与上文首次复核的 448 篇结果分开记录）：

| 命令 | 结果 |
|---|---|
| `python3 scripts/docs.py self-test` | PASS，exit 0 |
| `python3 scripts/docs.py build` | PASS，exit 0；447 documents，4194 sections |
| `python3 scripts/docs.py check` | PASS，exit 0；目录、身份、路由、链接与锚点通过 |
| `python3 scripts/docs.py check --base 1c283ec1707637190368eff20dba4702ed247d60` | PASS，exit 0；元数据与删除/恢复守卫通过 |
| `git diff --check` | PASS，exit 0 |

两份已删除候选的 `inspect` 均返回 `removed` 且无入链；退役身份、替代材料、完整 Git 版本与
SHA-256 可追溯。原 ISA 文档保留路径和身份，不另留一份并行旧方案；提交前原文可从上述基线恢复。

## 后继：统一部署、硬件基础与内核设计整合

按作者继续讨论及“写成并优化现有文档并提交所有修改”的要求，将设计写回原方案，
不新增平行提案。整合范围：

- 同一发布镜像内含多 ISA 原生载荷，纯 x86、纯 ARM 与混合配置使用相同对象语义；
  EFI 多架构入口只提供启动格式基础，成员启动与平台准入另行验收，不承诺任意硬件可启动。
- FCSA 1.1.0、CHI C2C、UCIe Streaming 与先进封装分别作为系统组织、一致性事务、传输和
  物理集成依据；逐项登记原子/顺序、通知、时基、故障阻断和 x86 桥接的待验证条件。
- 明确 `kernel` 通用机制、`prima-ostd` ISA/平台后端与 `xtask` 镜像职责；共享布局及 Rust
  别名/同步边界、本地现场标识、固定放置与远端唤醒票据接受共同审查。
- 将 Stramash、Popcorn、Barrelfish、Helios 和 UNIFICO 对照集中到现行方案；区分原型实测、
  模拟器、编译器及单一系统视图，明确来源取舍。未迁入、构建或复现这些项目的代码。
- 首个融合运行切片收敛为两 ISA 各一个 CPU 并发操作同一权威 Capability；既有撤销容器
  偏差仍须先修复，首片不以完整 SMP 或跨 ISA 活迁移为前置。

上位白皮书、阶段跟踪、系统地图、LibOS 主题及检索/规格/报告入口同步。旧提案正文和旧
分期仍已移除；新的项目对照仅用于解释现行机制来源。部署目标与设计约束不构成实现完成，
阶段/退出/profile 不变；此次修改全部为 Markdown、生命周期/主题 JSON 与生成目录。

来源核对基于官网规范/项目及原论文：FCSA 1.1.0（2026-10）、CHI C2C Issue A、UCIe
Streaming 1.1 说明、UEFI 2.10 A 启动规范与方案所列论文。未全面审计硬件 IP、桥接器、
上游源码或许可组合；未取得真实 x86/ARM 芯粒互操作、ARM 后端或统一镜像运行证据。

最终整合验证基线仍为 `1c283ec1707637190368eff20dba4702ed247d60`，包含本会话之前
尚未提交的架构纠偏和提案清理，全部同次提交。命令与结果：

| 命令 | 结果与范围 |
|---|---|
| `python3 scripts/docs.py self-test` | PASS，exit 0；含元数据、确定性、生命周期与 Git 恢复守卫 |
| `python3 scripts/docs.py build` | PASS，exit 0；447 documents；生成目录与正文一起提交 |
| `python3 scripts/docs.py check` | PASS，exit 0；目录、身份、主题、本地链接与锚点 |
| `python3 scripts/docs.py check --base 1c283ec1707637190368eff20dba4702ed247d60` | PASS，exit 0；新增元数据及删除/恢复守卫 |
| `git diff --check` | PASS，exit 0 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS，exit 0；1248 passed、0 failed、1 ignored；host 回归 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS，exit 0；fmt、Clippy、x86 裸机构建、内核零 unsafe（47 文件）、S6-X3 |

另逐个读取两删除路径的台账恢复 Git blob，SHA-256 均相符。Rust 测试与 lint 是现有代码
回归，不能证明文档中的融合机制已实现。未运行 QEMU、Lean 或上游复现：本次未修改可执行
代码与启动配置；历史失败、未覆盖项和 `ISSUE-REVOKE-CONTAINER-001` 仍按原范围保留。
原始测试输出仅保留于 `/tmp/prima-fusion-workspace-test.log` 与 `/tmp/prima-fusion-lint.log`，不入库。
