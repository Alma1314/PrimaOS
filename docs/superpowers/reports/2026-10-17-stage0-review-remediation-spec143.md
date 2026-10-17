# SPEC-143：阶段 0 第三方组织级复核发现项整改（29 项）

> 文档类型：复核整改记录（非设计、非规范、非证明）
>
> 日期：2026-10-17
>
> 复核基线：`ee1eb4c`；整改对象：SPEC-142 登记的 29 项发现（11 阻断 / 12 中 / 6 低）
>
> 结论速览：29 项均已在 `COSIR 原语规范.md` 中处置。跨节点 Capability 传输出让被收窄为 **v1 仅传消息、不传 Capability**；`Faulted` 的“原上下文重播”被显式降级为细化层职责；`KernelState` 新增保留帧/GPU 池字段，Bootstrap 新增对应输入；新增 `node_attach` 后 §19 操作面由 46 项变为 **47 项**。S0-X1/S0-X2 仍不勾选：本轮是整改而非独立复核，且 `cosir`/Lean 尚未同步新字段与新操作。

## 1. 整改原则与范围

- **只改规范文本。** 未改 Rust/Lean/QEMU 代码；`cargo test --workspace`（786 passed）与 `lake build`（8 jobs）只证明既有工件未回归。
- **阻断项优先做“收窄/显式降级”，不做无依据的机制扩张。** 跨节点 Capability 委托在 v1 被明确排除（§2.2）；`Faulted` 原上下文重播被降级为细化层职责；这样既关闭安全空洞，又不把未验证的分布式/恢复语义写成 POSIR 承诺。
- **新增状态必须可初始化。** `reserved_frames`/`gpu_reserved_frames`/`gpu_pools` 都由 Bootstrap 输入显式给出，避免“不变量引用不存在字段”。
- **保留后续版本的类型不删除。** `InTransit`/`origin`/`TransfersInProgress` 保留为保留字段/保留错误，正文声明 v1 不产生；后续跨节点委托版本可复用。

## 2. 逐项处置

| # | ISSUE | 整改 | 判定 |
|---|---|---|---|
| F1 | ISSUE-NODE-004 | §2.2/§8.5/§9.1：跨节点 `send` 只传数据；`transferred_caps` 非空发往 `RemoteProxy` 返回 `NonPortableCap`；不再为远端对象铸造代理根，因而不存在“源端 `ObjectRef` 当本地 ID”的翻译问题 | Fixed（文本/范围收窄） |
| F2 | ISSUE-NODE-005 | §4.3/§4.6/W4/§8.2/§18：`InTransit` 与跨节点 Cap 移动标记为 v1 不产生、保留给后续版本；源端跨节点 `send` 不再产生需确认的 Capability 终态 | Fixed（文本/范围收窄） |
| F3 | ISSUE-NODE-006 | §3 新增 `NodeIdentity` 与 `NodeId` 的区分；§5/§8.5 的 `Node.peer`/`local_node`/`Task.node`/`Namespace.node`/`NodeTransport` 统一为 `NodeIdentity`；`NodeId` 仍是本地表键；Bootstrap 不把 `local_node` 当本地键 | Fixed（文本/类型） |
| F4 | ISSUE-NODE-007 | §8.5 新增公开 `node_attach(node_cap, remote_task_id) -> task_cap`（`NodeConnect` 授权，复用/创建本地代理并返回 `{Derive, TaskSend}` Cap）；`attach_remote_task` 保留为内部迁移（目标端投递）；§19 Node 行与总数更新为 47 | Fixed（文本/新增操作） |
| F5 | ISSUE-NODE-008 | §8.5/§17.5：跨节点投递改为“交给 transport 即成功、目标端异步接受或 `MessageDropped`”；不再要求目标端执行源端回退；本地原子性限定为本地投递 | Fixed（文本） |
| F6 | ISSUE-EXC-017 | §2.2 增加“原上下文重播不在 POSIR v1”；§12.9 删除“重播故障指令”的承诺；§8.2 允许细化层把“没有预置恢复帧”作为不可恢复判据输入 | Fixed（文本/显式降级） |
| F7 | ISSUE-TASK-008 | §8.1 增加前置 0：`kind = ComputeTask(...)`；错误清单新增 `InvalidTaskKind`；§6.2.1 登记 | Fixed（文本） |
| F8 | ISSUE-MEM-008 | §5 `KernelState` 新增 `reserved_frames`/`gpu_reserved_frames`；§14 Bootstrap 新增 `reserved_frames` 输入并设 `free_frames = universe \ (image ∪ reserved)`；W6/W16.1/§15.9 改用显式集合 | Fixed（文本/状态） |
| F9 | ISSUE-MEM-009 | §5.7/§14：Bootstrap 新增 `gpu_pools`/`gpu_reserved_frames` 输入，初始化 `gpu_frame_universe`/`gpu_free_frames`/`gpu_reserved_frames`；§8.6 `gpu_memseg_create` 因此可达 | Fixed（文本/状态） |
| F10 | ISSUE-MEM-010 | §7.5 删除 `pool = GPU(arch)` 限定，Running 非 CPU Task 持有任意池 Segment 都阻止销毁；`InUseByAccelerator` 同步 | Fixed（文本） |
| F11 | ISSUE-TASK-009 | §3 定义平台常量 `cpu_core_count ≥ 1`；§5.2 改写 SMP 段；§8.4 `schedule` 增加“全系统 Running < cpu_core_count”前置；W9 用该常量并把每核约束标为细化层投影 | Fixed（文本） |
| F12 | ISSUE-S0X2-011 | §7.2/§7.3/§7.5/§15.4/§15.7/§11.3–§11.8/§8.2/§10.4–§10.5/§12.6/§15.8：统一补“由调用者持有、可用”与 `NotHeld`/`TargetDead`；§7.2 删除无前置的 `RightsExceeded` | Fixed（文本） |
| F13 | ISSUE-AUDIT-003 | §11.3–§11.8 成功动作补 `CapMoved`/`CapDropped`/`CapDerived`/`Ns*`；§13.1 事件集补 `ExcSourceCreated`/`ExcSubscribed`/`ExcUnsubscribed`/`ExceptionRaised` | Fixed（文本） |
| F14 | ISSUE-WAIT-002 | §21.2.1 前置 5 要求 `observed_generation ≤ generation`，新增 `InvalidGeneration`；W17 同步 | Fixed（文本） |
| F15 | ISSUE-AUDIT-004 | §6.3 明确使用并推进 `next_msg`；§13.1 明确使用并推进 `next_event`；§9.1/§8.5 跨节点源端 `MessageSent` 分配本端 `MsgId` 并记录 `transport_msg` | Fixed（文本） |
| F16 | ISSUE-TASK-010 | §12.5 前置 4 改为 `Running` 的 CPU Task；W9 增加“非 CPU Task 不得 `Faulted`” | Fixed（文本） |
| F17 | ISSUE-DOC-004 | §17.8 改写为“订阅由 `dev_irq_create` 委托的 `ExcSubscribe` 授权；`DevInterruptSubscribe` 只管 mask/unmask” | Fixed（文本） |
| F18 | ISSUE-DEV-013 | §15.8 IOPort 销毁增加“删除前撤销全部 Task 的 IOPL/TSS bitmap 授权，不等切换”；§15.5 改为持有集合变化即同步 | Fixed（文本/细化义务） |
| F19 | ISSUE-DOC-005 | §5.3 删除只含 `TimerFired` 的重复定义，改为引用 §12.2 的完整 `KernelEvent` | Fixed（文本/类型） |
| F20 | ISSUE-DOC-006 | §15.2 `DeviceKind` 改为带载荷联合（`MMIORegion(MMIORegion)` 等）；§15.4/§15.5/§15.6/§15.7 的创建动作同步 | Fixed（文本/类型） |
| F21 | ISSUE-S0X2-012 | §14 就地定义 `AlreadyBooted`；§7.2 删除多余 `RightsExceeded`；§15.4 `dev_mmio_map` 增加溢出前置与 `OutOfBounds` | Fixed（文本） |
| F22 | ISSUE-AUDIT-005 | §10.2 `timer_set` 补根 Cap `CapMinted`；§10.5 Pending 销毁补 `TimerCancelled(ByTask(caller))`；§13.1 要求映射/保护事件记录 `seg_cap`/`dev_cap`/`task_cap` | Fixed（文本） |
| F23 | ISSUE-DOC-007 | §5.5 引用改为“见 §10.6 与 W11”；W11 增加已删除 Timer 通知豁免；§12.3 `kind` 改为构造子集合 | Fixed（文本/类型） |
| F24 | ISSUE-DOC-008 | §17.3 拆分 `vspace` 与 `dev_vspace` 两段；明确 `dev_destroy` 的 `TaskManageVSpace` 例外；“新映射”限定为 Segment 映射 | Fixed（文本） |
| F25 | ISSUE-NODE-009 | 代理根不再铸造（跨节点 Cap 传递被 v1 排除），W2/§17.2 不再需要“代理根 rights = 源 rights”；`origin` 标记为保留字段 | Fixed（范围收窄） |
| F26 | ISSUE-NODE-010 | `attach_remote_task` 内部铸造仍显式使用并推进 `next_cap` 并追加 `CapMinted`；代理根接收事件随 v1 收窄退出可达集 | Fixed（文本/范围收窄） |
| F27 | ISSUE-LIFE-010 | W1 限定为“本节点分配的 ID”，并显式声明 `Task.remote_id`/`Node.peer`/远端 `NodeIdentity` 不参与本地新鲜性 | Fixed（文本） |
| F28 | ISSUE-DOC-009 | §5 的 creator 语义增加 Timer 例外（recipient 绑定生命周期） | Fixed（文本） |
| F29 | ISSUE-AUDIT-006 | §8.2 步骤 10 补 `SegmentUnmapped`/`DeviceUnmapped`/`ExcUnsubscribed`；§13.1 事件集补齐 | Fixed（文本） |

## 3. 关键取舍

- **跨节点 Capability 委托从 v1 移除。** 白皮书已把“跨节点 Capability 委托与统一命名”列为低成熟度未来项（★☆☆）；把 v1 收窄为“消息数据 + Task 代理”是关闭 F1/F2/F25/F26 的最小可信方案。`InTransit`/`origin`/`TransfersInProgress` 保留为后续版本的保留构造，正文声明 v1 不产生。
- **新增 `node_attach`。** 没有它，发送方永远无法获得远端 Task 的本地代理 `TaskSend`，首条跨节点发送不可达。该操作由 `NodeConnect` 授权，返回本地代理的 `{Derive, TaskSend}` Cap；§19 因此变为 47 项。
- **`Faulted` 原上下文重播降级。** 规范现在明确：`exc_resume` 只恢复调用者/细化层提供的已验证帧；把故障 RIP/RSP 导出给用户态并重播原指令不在 POSIR v1。这样 §12.8/§12.9 不再宣称一个无法由原语表达的能力。
- **保留帧/GPU 池进入 `KernelState`。** 不再用“事后取补集”解释 W6；Bootstrap 必须显式给出 `reserved_frames`/`gpu_pools`/`gpu_reserved_frames`，否则 W6/W16.1 无判定依据。
- **每核调度约束留在细化层。** POSIR 只保留“全系统 Running CPU Task ≤ `cpu_core_count`”；`schedule` 增加该前置；每核至多一个由细化层投影保持，避免给抽象状态强加核归属字段。

## 4. 对退出条件的影响

- **S0-X1**：29 项均已登记并修订，字面条件“已知空洞均已记录且关闭”在文本层面恢复；但**本轮不是独立复核**，且 `cosir`/Lean 未同步 `reserved_frames`/GPU 池/`node_attach`/47 项操作面，故 S0-X1 继续未勾选，待下一轮范畴外复核。
- **S0-X2**：`task_create(kind)`、Capability 前置/错误清单、`AlreadyBooted`/`RightsExceeded`/`OutOfBounds`、`gpu_memseg_create` 可达性均已修；操作面由 46 项变为 **47 项**（新增 `node_attach`），需按新清单逐项复核后才恢复勾选。
- **S0-D1/S0-D2**：`NodeIdentity`/`NodeId`、`reserved_frames`、`gpu_*`、`DeviceKind` 载荷联合与 47 项操作面已更新，属于模型/操作定义修订；仍为“已定义，未证明”。
- **S0-X3**：未改 Lean；`lake build` 8 jobs 通过；新字段/新操作尚未建模，登记为后续细化缺口。

## 5. 复验

| 命令 | 结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | 786 passed（cosir 146 + cosir_state 42 + lean_backtrace 3 + kernel 377 + prima-ostd 21 + prima-runtime 1 + prima-initgen 1 + xtask 195） |
| `lake build`（`lean/`） | 8 jobs，成功 |
| `git status --short` | 仅规范、跟踪文档、白皮书与索引；无 Rust/Lean/QEMU 代码改动 |

以上只证明既有模型/形式化工件未回归，不证明新增条文已被实现或机器验证。

## 6. 证据边界与残留

- 本记录是规范文本整改；不构成 `MODEL-TEST`、`REFINEMENT-PROOF`、QEMU 证据或组织级独立性审查。
- `node_attach`、47 项操作面、`reserved_frames`/GPU 池输入、`DeviceKind` 载荷联合、`NonPortableCap`/`InvalidTaskKind`/`InvalidGeneration`/`AlreadyConnected` 在 `cosir`、Lean 与内核中**尚无对应实现**；不能把“文本闭合”外推为“实现或证明闭合”。
- `InTransit`/`origin`/`TransfersInProgress` 是保留构造；v1 的可达状态不产生它们，后续跨节点委托版本必须重新证明 W3/W4/W13。
- 下一轮复核必须按新版 §19 的 47 项逐项签署，并复核本轮“范围收窄/显式降级”是否被接受；在下一轮签署前 S0-X1/S0-X2 保持未勾选。
