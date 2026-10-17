# 阶段 0 第三方组织级复核结论（未通过）

> 文档类型：复核结论/缺陷登记（非设计、非规范修订、非证明）
>
> 日期：2026-10-17
>
> 复核基线：`ee1eb4c`（`fix(stage0): remediate second organizational review findings (SPEC-141)`），工作区干净
>
> 复核范围：`COSIR 原语规范.md`（2753 行）的对象生命周期、W1–W17、§19 的 46 项常规操作（含 Bootstrap 单列）与 §17 安全结论；对照 `Prima OS 实现阶段.md` ISSUE 表与 SPEC-137/139/140/141 整改报告去重
>
> **整改状态（2026-10-17 晚）：** 本报告的 29 项已由 [SPEC-143 整改报告](2026-10-17-stage0-review-remediation-spec143.md) 逐项处置；§19 因新增 `node_attach` 变为 **47 项**。本报告保留为复核发现记录；S0-X1/S0-X2 仍待下一轮独立复核。
>
> 结论：**不通过。** 发现 11 项阻断级、12 项中等、6 项低危缺陷（去重后 29 项），其中跨节点 Capability 语义、`Faulted` 恢复出口、W6 保留帧表示与 GPU 帧池可达性属于设计级空洞，不能靠措辞修补关闭。S0-X1 与 S0-X2 保持未勾选，阶段 0 维持“进行中”。

## 1. 复核方法与独立性

本轮按“组织级第三方审查”的对抗性要求执行：

1. **独立视角拆分。** 五个互不共享上下文的对抗式复核流分别覆盖：Capability/对象生命周期、Task/调度/IPC/跨节点、MemorySegment/Device/Timer、Namespace/Exception/WaitObject/Audit、S0-X2 逐操作覆盖。每个复核流只读原文，要求给出精确行号、原文引用、可达反例与修订方向，并按 ISSUE 表去重。
2. **主审逐项回读。** 父会话对每个候选发现重新回读规范原文，剔除仅属措辞偏好或已被登记且已修复的项；保留的每一项都在第 3 节给出原文位置与反例。
3. **独立性声明。** 本轮不是规范作者的自我复核：复核流以证伪为目标、独立构造反例，主审只做证据核对与去重。由于本轮结论为“不通过”，未触发签署，故“组织级独立性门槛是否已跨过”这一问题本轮无需裁定；在阻断项修复并由下一轮复核确认前，任何签署都不成立。
4. **证据等级。** 本轮是规范文本审查，不产生 `MODEL-TEST`、`REFINEMENT-PROOF`、QEMU 或机器证明证据。为确认复核没有扰动既有工件，另行运行了 `cargo test --workspace`（786 passed）与 `lake build`（8 jobs）；它们只证明既有模型未回归，不证明本报告的任何判断。

## 2. 判定

| 退出条件 | 判定 | 依据 |
|---|---|---|
| S0-X1 规范没有已知且未记录的对象生命周期空洞 | **不满足** | 第 3 节 F1–F10、F25 中至少 F1–F3、F5、F6、F8、F25 是未登记的生命周期/状态可表示性空洞；F4、F7、F9、F10 使既有“已闭合”结论回归 |
| S0-X2 每项操作具有明确成功与失败语义 | **不满足** | F7 的 `task_create(kind=RemoteProxy)` 无失败出口；F12 的 10 项操作前置与错误清单不一一对应；F15/F21/F27 的错误标识/分配规则缺口；F9 的 `gpu_memseg_create` 在一切可达状态不可成功 |
| S0-D1/S0-D2 模型与操作/不变量定义 | 仍为“已定义，未证明” | 本轮发现的 W6/W9 表示缺口、W1 外来 ID 量化、`KernelEvent`/`DeviceKind` 类型不一致与代理根 rights 未绑定，说明“定义”本身仍需修订 |
| S0-D3/S0-D4/S0-X3 | 不受本轮影响 | 未复核架构假设与细化草案；Lean 工件未改动且 `lake build` 通过 |

**阶段状态：0（进行中）。** 本轮不更新任何阶段勾选，不改变阶段 1–4 的“待 S0-X1”约束。

## 3. 阻断级发现（11 项）

### F1 跨节点代理根的 `target` 未做节点限定/翻译

- **位置：** §8.5 L979；§4.2 L169-185；§3 L81-87；W2 L2104-2109；§17.2 L2253。
- **原文：** “跨节点传输使用‘携带源节点 ID + 本地 CapId 的全局引用’格式；目标节点在接收时创建本地代理 `CapRecord`，`parent = None`、`origin = Some((源节点, 源 CapId))`.”
- **缺陷：** `CapRecord.target` 的类型是不带节点限定的 `ObjectRef`；规范没有定义源端 `ObjectRef` 到目标节点对象的翻译，也没有 `RemoteObject` 变体。
- **反例：** N1、N2 各自都有 `Segment(7)`。N1 的 A 把指向自己 `Segment(7)` 的 Cap 发给 N2 的 B；N2 铸造代理根时只能把 `target` 写成本地 `Segment(7)`——B 随后可映射/读写 N2 自己的 `Segment(7)`，而它从未被授权该对象。若 N2 没有 `Segment(7)`，该 token 永久 stale，规范同样未定义。Task/Namespace/Device 引用有同样问题。
- **为什么是阻断：** 这是授权语义层面的“同 ID 混淆”，不是细化实现能自行决定的选择；W2/§17.2 的溯源检查不会发现。
- **修订方向：** 引入节点限定的远端目标（如 `RemoteObject(NodeId, ObjectRef)`）并规定目标端本地代理对象；或限制跨节点 `transferred_caps` 只能传可移植类型（Task 引用映射为本地 `RemoteProxy`），其余返回新错误（如 `NonPortableCap`）。

### F2 跨节点接受后源端 `InTransit` 终态与事件未定义

- **位置：** §4.3 L202；§4.6 L288-289；§8.5 L979-981、L994；W4 L2118；W13 L2103、L2203。
- **原文：** §4.6 说跨节点 `send` 经 `InTransit` 后“（接收确认后）`HeldBy(receiver)`”；§8.5 说被目标接受时“转为 `HeldBy(t_r)` 或 `InMessage`”；§8.5 步骤 3 只写目标端铸造/入队。
- **缺陷：** 源节点上 `receiver` 只能是一个本地 `RemoteProxy`，不是远端 `t_r`；规范没有规定确认后源端 `InTransit` 是被删除、转成 `HeldBy(proxy)`，还是别的终态，也没有对应的 `CapMoved`/`CapDropped`/传输接收事件。
- **反例：** S 向本地代理 P 发一个 Cap；目标端铸造新 CapId `c'` 交付给 R；ack 返回。源端若按 §4.6 落在 `HeldBy(P)`，该 token 永远无法使用或派生（P 不执行任何用户操作），且这次位置变化没有事件；若实现删除源端位置，也没有规范条款与 `CapDropped` 事件授权。目标端另有 `c'`，同一 Authority 在两个节点各有一个非 `InTransit` 位置；§8.5 L996 的“同一 `CapId` 不得同时出现在两节点”按 CapId 计数，覆盖不到这种情况。
- **为什么是阻断：** 跨节点传输的线性化点与位置唯一性论证都建立在这个终态上；它缺失时 W3 扩展、W13 与 §13.2 的审计链都无法闭合。
- **修订方向：** 在 §8.5 增加“源端确认”步骤：删除源端 `locations[c]`，追加规范定义的接收/交接事件（记录源节点、源 CapId、transport/MsgId）；同步修改 §4.3/§4.6 的跨节点条目与 §13.1 事件种类。

### F3 `NodeId` 的平台身份与本地表键混用，`local_node` 未保留

- **位置：** §3 L60；§5 L335；§8.5 L914、L936、L950、L972、L976、L979；§14 L1783；W14 L2211。
- **原文：** `local_node: NodeId` 是“平台提供的本节点 ID”；`node_create` “分配 `NodeId`（使用并推进 `next_node`）”；`send` 要求“本地 `nodes` 表中存在 `NodeId = t_r.node` 的 Node”；传输携带“源节点 ID”。
- **缺陷：** 平台全局节点身份（`local_node`）与本地 `nodes` 表键（`next_node` 分配）是两套编号，但规范把它们都叫 `NodeId`，且没有“全局身份 ⇄ 本地键”的绑定。`local_node` 也没有被排除在 `next_node` 分配之外。
- **反例：** N1 的 `local_node=3`、N2 的 `local_node=5`；两节点第一次 `node_create` 都得到本地键 0。N1 的传输声明源节点 3，N2 必须“存在指向源节点的本地 Node”，但其键空间是 0,1,…，与 3 没有定义映射：按 ID 比较会全部拒绝（跨节点不可达），若碰巧有同值键则可能匹配到第三个节点（溯源错误）。另外 N1 的 `next_node` 可能分配出 3，与 `local_node` 同值，`attach_remote_task(local_node, …)` 可造出 `node = local_node` 的代理。
- **修订方向：** 分离 `NodeIdentity`（平台全局、不可变）与 `NodeKey`（本地表键），增加 `NodeIdentity ⇄ NodeKey` 绑定；`NodeTransport` 的字段明确为 `NodeIdentity`；Bootstrap 保留 `local_node`（或令 `next_node` 跳过它）；目标节点用 wire 身份查本地 Node，查不到即拒绝。

### F4 出站 `RemoteProxy` 的创建触发、holder 与 `TaskSend` 授权未定义

- **位置：** §8.5 L892、L947-954、L976、L979-980；§6.1 L588；§9.1 L1069；§19 L2451。
- **原文：** `attach_remote_task` 是 `KernelActor` 内部迁移，参数含 `holder`；§8.5 步骤 3 只在**接收**方向“为发送方建立本地 `RemoteProxy`”；`send` 要求目标是本地 `RemoteProxy`。
- **缺陷：** 没有任何用户操作或协议步骤会在**发送**方向为接收方建立本地代理；holder 如何选择、`TaskSend` Cap 如何交给发送方，均未定义。
- **反例：** 刚启动后，A 用 `node_create` 建了到 N2 的 Node，并经带外方式得知远端 B 的 TaskId。A 要发送，需要指向“B 的本地代理”的 `TaskSend` Cap；但该代理不会被创建，A 也拿不到 Cap。只有 B 先向 A 发送（在 A 的节点上为 B 建代理）后，A 才可能回复——首条跨节点发送不可达。ISSUE-NODE-001 声称已使跨节点路径可达，实际只闭合了接收侧。
- **修订方向：** 定义显式的代理建立原语/协议步骤（如由 `NodeConnect` 授权的 `node_attach(node_cap, remote_task_id) -> task_cap`），规定 holder、`TaskSend` 委托与审计事件，并纳入 §19 或 §18.0。

### F5 跨节点拒绝/放弃路径的动作归属错位，且不能触发安全回退

- **位置：** §8.5 L976、L980-981、L985-990、L994；§4.6 L289；§17.5 L2288-2291。
- **原文：** “若接收方 Task 已被销毁，则拒绝接收，Capability 回退到源位置（源 Task 仍活动时）或丢弃”；回退触发只列“超时或网络不可达”。
- **缺陷：** `locations` 在源节点；目标节点无法执行“回退到源位置”，规范也没有定义拒绝通知与源端收到拒绝后的动作/事件。拒绝既不是“超时”也不是“网络不可达”，因此不满足 §4.6 列出的回退触发条件。§17.5“发送成功时 payload 与 token 整体进入队列或直接交付”对跨节点 `send` 的成功返回为假。
- **反例：** S 发送后 R 被销毁，N2 拒绝。N1 在超时后仍无法区分“未接收”与“ack 丢失”，只能保持 `InTransit` 并重试；§8.5 L994 的“超时窗口内进入三个终态之一”不成立（只有永久分区被豁免）。
- **修订方向：** 把“放弃/拒绝”定义为协议第三步的显式动作（目标→源消息 + 审计事件），把“目标明确拒绝”加入回退触发；§17.5 增补跨节点限定（成功仅表示进入 `InTransit`，终局由接受/安全回退/丢弃决定）。

### F6 `Faulted` 恢复出口不可表达：故障信息与原上下文没有出口

- **位置：** §12.10 L1695、L1699；§12.9 L1667、L1679；§12.5 L1552-1556；§5.2 L446；W9 L2165；§8.4 L884。
- **原文：** `exc_resume` 要求“调用者 Task 必须处于 `Faulted`”且帧由调用者提供、RIP/RSP/CS/SS/RFLAGS 已合法；§12.9 说页故障处理方“检查 `faulting_vaddr` 与 `cause`……决定重播故障指令”。
- **缺陷：** `Faulted` Task 不能执行 `recv`（要求 `Running`），唯一可调的 `exc_resume` 不读邮箱；故障信息只剩邮箱中的通知（自身不可读）与 `pending_exc`（没有任何操作可读）。监督者可以订阅同一源并修映射，但不能代调 `exc_resume`（前置 5 要求调用者自身是 `Faulted`）。细化层可以“代表该 Task”调用（§6.1），但它只能在故障前预置帧，或用特权写入帧；POSIR 没有把故障现场交给该 actor 的原语。于是“检查 `faulting_vaddr` 后修映射并重播”在抽象模型内没有可表达的操作序列。
- **反例：** M 订阅 `page_fault_source`；M 写 COW 页 → 通知入 M 邮箱、M → `Faulted`；M 不能 `recv`；`pending_exc` 无查询操作；监督者 S 能改 M 的映射但不能调 `exc_resume`；细化层代调也只能用故障前帧。即使恢复到一个 handler，原 RIP/RSP 从未进入用户态，`pending_exc` 已在 `exc_resume` 动作 2 清除，“重播原指令”无路径。
- **说明：** 实现已登记“口径 A：`KernelActor` 代故障任务调用 + 内核静态帧缓冲”的细化偏差（决策日志 2026-09-24），但规范文本没有承认这条偏差，也没有给出故障信息/原上下文的出口；§12.8/§12.9 的 COW/按需分页叙述仍按“用户态处理并重播”书写。
- **修订方向（三选一）：** (a) 增加 `Faulted` 自身可读 `pending_exc`/原上下文的查询，并把原 RIP/RSP 纳入 `exc_resume` 的输入；(b) 允许持有目标 `TaskManageVSpace` 且订阅该源的监督者代恢复（`exc_resume` 显式目标参数）；(c) 显式降级 §12.9/§12.8 为“只能使用故障前预置帧”，并把“未预置帧的 `Faulted` 视为不可恢复”写入 §8.2 回收判据。

### F7 `task_create` 接受 `kind = RemoteProxy` 却没有前置或错误出口

- **位置：** §8.1 L792、L794-803、L819；§5.2 L438、L452；§6.2.1 L619。
- **原文：** “`task_create` 的 `kind` 必须是 `ComputeTask(...)`；`RemoteProxy` 只能由 §8.5 的内部迁移创建。”前置 1–8 没有 kind 项；错误清单也没有 kind 相关标识。
- **缺陷：** `TaskKind = ComputeTask(...) | RemoteProxy(NodeId)` 是用户可构造的合法实参；违反 L792 时既无显式错误，也不适用默认规则（该操作有显式清单），标准错误隐式包含只覆盖 Capability。若按成功路径执行，动作 6 仍写 `root_ns_cap = Some(...)`、动作 8 仍置 `node = local_node`，直接违反 §5.2 L438/L452。
- **修订方向：** 把 kind 校验升为前置，新增并登记 `InvalidTaskKind`（§6.2.1 新标识清单 + §8.1 错误清单），或把签名收窄为只接受 `ComputeTask(...)`。

### F8 W6 的“保留帧”集合没有状态载体，且与 §14 的 `free_frames` 定义冲突

- **位置：** W6 L2137、L2139；§5 L327-331；§14 L1783；§15.4 L1876；§3 L101；§17.1 L2248。
- **原文：** W6 把 `frame_universe` 划分为 `free_frames`、活动 CPU Segment 帧、保留帧三类，保留帧“由以下来源显式给出，而非事后取补集：所有活动 DMABuffer 的 frames 之并，加上细化层从 frame_universe 分配、当前未映射进任何活动 Segment 的页表帧”；bootstrap 却设 `free_frames = frame_universe \ image_frames`。
- **缺陷：** `KernelState` 只有 `frame_universe`/`free_frames`，没有 `reserved_frames`/页表帧字段。若页表帧属于 `frame_universe`（L1876/W16.1 明确要求“含页表帧”），bootstrap 后它们仍在 `free_frames` 中，同时按 W6 又应是保留帧 → 二者相交，W6 为假；若它们不属于 `frame_universe`，W16.1 的“含页表帧”无法判定，MMIO 仍可落在页表物理页上。
- **修订方向：** 在 `KernelState` 增加 `reserved_frames`（或 `table_frames`），由 bootstrap 输入初始化，§14 改为 `free_frames = frame_universe \ (image_frames ∪ reserved_frames)`；或明确页表/pin 帧在 `frame_universe` 之外，删除 W16.1 的“含页表帧”并把该限制降为细化层义务。

### F9 GPU 帧池没有初始化/扩充迁移，`gpu_memseg_create` 在一切可达状态不可成功

- **位置：** §5.7 L570-574；§5 L330-331；§14 L1765-1791；§8.6 L1024-1041；W6 L2139。
- **原文：** `EmptyState` 设 `gpu_frame_universe = gpu_free_frames = empty map`；bootstrap 只初始化 CPU 池；唯一写 `gpu_free_frames` 的是 `gpu_memseg_create`（移除）与 `memseg_destroy`（加回，但要求已有 GPU Segment）。
- **缺陷：** 归纳得任意可达状态下 `gpu_free_frames[arch] = ∅`（且缺键时 `gpu_free_frames[arch]` 可能未定义）。因此 `gpu_memseg_create` 恒返回 `NoMemory`，不存在活动 GPU Segment；W6 的 GPU 三划分、§7.5 的 `pool = GPU` 分支与 `InUseByAccelerator` 全部不可达；§19 把 `gpu_memseg_create` 列为 46 项之一，但该项只有“永失败”语义。
- **修订方向：** 给 `bootstrap` 增加 GPU 池输入并在成功动作中初始化（同步 §5.7、§14、W6），或新增显式的 `KernelActor`/平台内部迁移建立 GPU 池（需状态字段与事件），并把 `gpu_free_frames` 定义为全函数。

### F10 `memseg_destroy` 的加速器保护只覆盖 GPU 池，Running 加速器持有的 CPU Segment 可被销毁并复用

- **位置：** §7.5 L760-762、L772；§8.1 L803；§8.6 L1017、L1047、L1052；ISSUE-MEM-007 的验收条件（跟踪文档 L334）。
- **原文：** “若目标 Segment 的 `pool = GPU(arch)`，不存在状态为 `Running` 的非 CPU Task 持有指向该 Segment 的可用 Capability。”
- **缺陷：** 前置只对 `pool = GPU(arch)` 生效；非 CPU `task_create` 的 `initial_caps` 只要求“指向 Segment”，没有池限制。Running GPU Task 持有的 CPU 池 Segment 不受保护。
- **反例：** init 建 CPU 池 Segment S（不映射）；`task_create(kind=ComputeTask(GPU), initial_caps=[S_cap])` 合法；细化层把 G 置 Running 并提交 kernel，kernel 通过 unified/host 内存路径读写 S；`memseg_destroy(S_cap)` 的前置 2 通过（G 的 `vspace` 为空），前置 3 因 `pool=CPU` 被跳过；S 的帧回 `free_frames`，随后 `memseg_create` 复用并清零，而 G 仍在读写 → 加速器侧 use-after-free。W6 的帧归属仍自洽，不会抓到。
- **修订方向：** 删除前置 3 的 `pool = GPU(arch)` 限定，对所有池生效；或把“非 CPU Task 只持同池 Segment”写成 `task_create` 前置 + W 级不变量，并同步 §8.6 的措辞。

### F25 跨节点代理根的 `rights` 未绑定源 Capability，目标节点可铸造更高权限

- **位置：** §4.2 L185；§8.5 L979；W2 L2104-2109；§17.2 L2255、L2259。
- **原文：** “目标节点在接收时创建本地代理 `CapRecord`，`parent = None`、`origin = Some((源节点, 源 CapId))`”；“`origin` 仅供溯源，不参与本节点授权判定”。
- **缺陷：** 规范没有规定代理根的 `rights` 与源 `CapRecord.rights` 的关系；W2 只检查权限类型与派生链，而代理根 `parent = None`，§17.2 的“权限单调”只约束派生链，对代理根空真；跨节点 wire 也不携带源 rights。
- **反例：** A 的 Task `s` 持 `C = cap(Segment(7), {Derive, SegMapR})` 并跨节点发给 B 的 `r`。B 铸造代理根 `C'` 时若给 `{Derive, SegMapR, SegMapW, SegDestroy}`，W2 的类型检查通过，`r` 获得 `s` 从未拥有的写/销毁权并可继续派生。§17.2 的“不可伪造/权限单调”不会发现，因为 `origin` 只用于溯源。
- **为什么是阻断：** 这是跨节点授权放大的直接路径，且无法由细化层“自觉”修复——规范必须给出 rights 绑定。
- **修订方向：** 规定代理根 `rights = 源 CapRecord.rights`，把源 rights 写入传输/接收事件并作为 W2 子句；§17.2 增“代理根 rights 等于源记录”的证明义务。

## 4. 中等发现（12 项）

| # | 缺陷 | 位置 | 登记 |
|---|---|---|---|
| F11 | W9 的 SMP 上界不可保持：`cpu_core_count` 未定义、每核子句无状态载体、`schedule` 不检查全系统 Running 上界；§5.2 仍写“当前 W9 要求至多一个 Running” | §5.2 L458-462；§8.4 L874/L886；W9 L2158；§5 L311-348 | ISSUE-TASK-009 |
| F12 | 10 项 Capability 参数前置只写“授权”，未写“由调用者持有/可用”，错误清单也无 `NotHeld`/`TargetDead`；异 Task 持有的 CapId 或 stale 目标会在 §6.2 第 2 阶段失败，但没有对应的错误标识。**注意：`§6.2` 的位置/存活性校验仍会拒绝，故这不是“授权可跨转移行使”的绕过**；缺陷是失败语义不明确（S0-X2 要求错误清单与前置一一对应），`ns_bind`/`ns_mount`/`ns_destroy` 还缺少对目标 Namespace 活动性的显式前置 | §7.2 L692/L709；§7.3 L719/L728；§7.5 L760/L772；§15.4 L1895/L1897/L1910、L1902/L1916；§11.3 L1255-1265；§11.4 L1275/L1282；§11.5 L1290-1308；§11.6 L1318/L1323；§11.8 L1362-1377；§6.2.1 L619 | ISSUE-S0X2-011 |
| F13 | `ns_bind`/`ns_unbind`/`ns_mount`/`ns_unmount` 成功动作未写 §13.1 明文要求的 `CapMoved`/`CapDropped`/`CapDerived`/`Ns*`；`ns_mount` 新 `CapRecord` 无 `created_at` 事件；`ns_destroy` 删位置无 `CapDropped` | §11.3 L1261；§11.4 L1278；§11.5 L1299-1304；§11.6 L1321；§11.8 L1369-1373；§13.1 L1720；W13 L2202-2203 | ISSUE-AUDIT-003 |
| F14 | `observed_generation` 无约束（未要求 `≤ generation`），超前观测会使等待期间的 `signal_object` 被静默漏掉，§17.9 的“无丢失唤醒”作为无条件断言不成立 | §21.2.1 L2629/L2631/L2637-2639；§5.6 L562；W17 L2228-2238；§17.9 L2320 | ISSUE-WAIT-002 |
| F15 | `next_msg`/`next_event` 没有分配规则（`enqueue_or_deliver` 只说“分配 MsgId”，§13.1 只说事件唯一）；跨节点源端 `MessageSent` 不走 `enqueue_or_deliver` 因而不分配 MsgId；W1 的“历史 ID < next_*”被破坏 | §6.3 L629；§8.5 L979；§9.1 L1079；§13.1 L1738；W1 L2100；W10 L2179；§5 L346-347 | ISSUE-AUDIT-004 |
| F16 | `raise_exception` 的硬件异常前置只要求 `faulting_task` 为 `Running`；非 CPU Task 也可 Running，故可被置 `Faulted`，但它无指令流/异常帧、不能作 Actor、不能订阅异常源 | §12.5 L1547；§12.2 L1452；§8.6 L1055/L1057；W9 L2160-2165；§6.1 L589 | ISSUE-TASK-010 |
| F17 | §17.8 称“只有持有 `DevInterruptSubscribe` 的 Task 可通过 `exc_subscribe` 订阅 IRQ 源”；实际门槛是 `dev_irq_create` 委托的 `{Derive, ExcSubscribe}` Cap。该断言既不必要也不充分，且不能靠给 ExcSource Cap 加设备权限修复（违反 W2） | §17.8 L2315；§15.7 L2013-2014；§4.1 L157；§12.4 L1502-1504；W15 L2217 | ISSUE-DOC-004 |
| F18 | IOPort 销毁只“释放端口范围 + `ObjectDestroyed`”，没有与 MMIO 对称的撤销硬件 bitmap 授权动作；Running Task 在销毁后仍可 in/out，新的同范围设备可被访问，W16.1 在硬件投影上双授权 | §15.8 L2049；§15.5 L1936/L1944-1948；§17.8 L2316 | ISSUE-DEV-013 |
| F19 | `KernelEvent` 在 §5.3 定义为只含 `TimerFired`，在 §12.2 又定义为 `TimerFired \| ExceptionRaised`；§12.5/§17.7 构造 `FromKernel(ExceptionRaised(...))`，按 §5.3 的类型不成立 | §5.3 L475-477；§12.2 L1446-1447；§12.5 L1553；§17.7 L2304 | ISSUE-DOC-005 |
| F20 | `DeviceKind = MMIORegion \| IOPortRange \| DMABuffer \| InterruptLine` 是零载荷标签，但 §15.4/§15.7 用 `kind = MMIORegion{...}` 赋值，W6/W16 读取 `DMABuffer.frames`、`MMIORegion.base_paddr/size_pages/allowed_access`、`InterruptLine.exc_source`；`Device{creator,kind}` 无处安放这些字段 | §15.2 L1814；§15.4 L1855-1858、L1881；§15.7 L2012；W6/W16 L2137/L2223 | ISSUE-DOC-006 |
| F26 | 代理根的本地 `CapId` 分配与 `created_at` 事件未规定：§8.5 L979 既未说“使用并推进 `next_cap`”，也未说复用源 CapId；`created_at` 应指向“传输接收事件”，但 §13.1 事件集没有该 kind（`CapMinted`/`CapDerived` 也不带 `origin`），`audit` 的第 2/5 项无落点 | §8.5 L979；W1 L2100；W13 L2202；§13.1 L1722-1738；§13.2 L1750-1756 | ISSUE-NODE-010 |
| F27 | W1 未限定“本节点分配的 ID”：`Task.remote_id` 与 `CapRecord.origin` 中的外来 TaskId/CapId 可以 ≥ 本地 `next_*`，跨节点可达状态按 W1 字面即为假（如本地 `next_task=2`、`remote_id=100`） | W1 L2100；§5.2 L439；§4.2 L173 | ISSUE-LIFE-010 |

## 5. 低危发现（6 项）

| # | 缺陷 | 位置 | 登记 |
|---|---|---|---|
| F21 | `AlreadyBooted` 未按 §6.2.1 定义/登记；`memseg_map` 错误清单多出无前置对应的 `RightsExceeded`；`dev_mmio_map` 缺 `OutOfBounds`/溢出错误 | §14 L1793/L1795；§6.2.1 L623；§7.2 L709；§15.4 L1893-1902 | ISSUE-S0X2-012 |
| F22 | `timer_set` 铸造根 Timer Cap 却只追加 `TimerSet`，未追加 `CapMinted`；`timer_destroy` 对 Pending 的取消未给 `reason`；`SegmentMapped`/`DeviceMapped` 不记 `task_cap`，`VspaceProtChanged` 不记授权 Cap | §10.2 L1134-1136；§10.5 L1198；W13 L2202；§13.1 L1738；§7.4 L746 | ISSUE-AUDIT-005 |
| F23 | §5.5 “（见 W11）”是悬空引用：W11 只约束活动 Timer，豁免实际在 §10.6；`ExceptionSource.kind` 写成 `ExceptionKind → Type` 全函数，与“构造子集合”语义冲突 | §5.5 L542；W11 L2182-2187；§10.6 L1208；§12.3 L1473/L1484-1487 | ISSUE-DOC-007 |
| F24 | §17.3 把 `dev_destroy` 归入“持有 `TaskManageVSpace` 的调用”，§15.8 明确它不需要 `TaskManageVSpace` 并反向引用 §17.3；§17.3“每个新映射都有 Segment 访问权限”对 `DeviceMapping` 不成立 | §17.3 L2263/L2265；§15.8 L2048；§15.4 L1895-1902 | ISSUE-DOC-008 |
| F28 | §5 的“creator 销毁不改变对象存活性”与 Timer 生命周期矛盾：`timer_set` 下 Timer 的 `recipient = creator`，`task_destroy` 步骤 5 删除以该 Task 为 recipient 的 Timer，creator 销毁确实改变 Timer 存活性 | §5 L353；§5.5 L535/L542；§8.2 L835 | ISSUE-DOC-009 |
| F29 | `task_destroy` 步骤 10 只列 Task/消息/Cap/等待/Timer 事件，未为步骤 1 删除的映射追加 `SegmentUnmapped`/`DeviceUnmapped`、未为步骤 8 删除的订阅追加 `ExcUnsubscribed`；§13.1 的事件集也未列 `ExcSubscribed`/`ExcUnsubscribed`/`ExceptionRaised`（虽然正文要求） | §8.2 L838/L840；§4.5 L268；§4.6 L291；§13.1 L1726-1727 | ISSUE-AUDIT-006 |

## 6. 已逐项复核、未发现新缺陷的范围

以下范围经独立复核未发现需要登记的新空洞，列出以便下一轮复用：

- 单机 Task 状态机出口：`task_create`/`task_destroy`/`yield`/`schedule`/`send`/`recv`，`BlockedRecv`/`BlockedWait`/`Faulted` 的机械出口，pending 直投与邮箱一致性，W7/W8/W10（F11 的 SMP 上界与 F16 的非 CPU `Faulted` 除外）。
- 订阅生命周期：`cap_drop`/发送/绑定/`task_create` 注入/跨节点 `InTransit` 的级联终止，`task_destroy` 步骤 8，W15（F6 的恢复出口除外）。
- WaitObject 的 signal/timeout/alert/destroy 竞态：四个完成源都在“转 `Ready` + 清 `wait_request`”同一步删除 waiter，未找到结果覆盖或丢失路径（F14 的超前观测除外）。
- Namespace 的 Mount 环检测、stale Bound/Mount 清理、`ns_unmount` 的源已销毁路径（F13 的审计事件与 F12 的前置除外）。
- Timer 的 Fired/Cancelled 不可回 Pending、`advance_time` 稳定顺序、`task_destroy` 的清理与通知保留（F22 的 CapMinted 与 reason 除外）。

## 7. 回归证据

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | 786 passed；146 cosir + 42 cosir_state + 3 lean_backtrace + 377 kernel + 0 prima-abi + 1 prima-initgen + 21 prima-ostd + 1 prima-runtime + 195 xtask | 既有 host 模型、内核、运行时与 xtask 测试未回归；不证明本报告判断 |
| `lake build`（`lean/`） | 8 jobs，成功 | 既有 Lean 工件未回归；不是全规范细化证明 |
| `git status --short` | 复核开始时干净；本轮只新增报告并更新跟踪文档、白皮书阶段状态与索引，未改 Rust/Lean/QEMU 代码或规范语义 | 规范未被本轮修改，阻断项仍待整改 |

本轮未运行 QEMU；没有代码或规范语义改动需要 QEMU 证据。

## 8. 结论与下一步

**本轮作为“第三方组织级审查”的结论是：不通过。** 29 项缺陷已登记（第 3–5 节与跟踪表），其中 F1–F10、F25 是阻断级；它们不是措辞瑕疵，而是：

- 跨节点 Capability 的目标绑定、源端终态、Node 身份、出站代理、拒绝路径与代理根 rights 绑定（F1–F5、F25）——需要一次专门的跨节点语义修订；
- `Faulted` 恢复出口（F6）——需要在“故障信息出口/代恢复/显式降级”之间做设计选择；
- W6 保留帧与 GPU 帧池（F8/F9）——需要给 `KernelState`/Bootstrap 增加状态与输入；
- `task_create` 的 kind 出口（F7）与加速器 CPU Segment 销毁（F10）——需要前置与错误/不变量修订；
- 代理根的本地 CapId/`created_at` 与 W1 外来 ID 量化（F26/F27）——需要随跨节点修订一起处理。

在这些项关闭并由下一轮独立复核确认前：

1. S0-X1、S0-X2 保持未勾选；阶段 0 维持“进行中”。
2. 不建议在本轮直接做“逐条措辞修补”——F1–F3、F6、F8/F9、F25/F26 需要先定设计，再改规范；否则会重复 SPEC-139/140/141 的“修一处、回归另一处”循环。
3. 建议开一个独立的阶段 0 整改切片（SPEC-143），按 F1/F25→F2/F26→F3→F4/F5→F6→F9→F10→F11/F12→其余 的顺序处置；每项都要给出反例回归（至少覆盖第 3 节的反例序列），并在下一轮范畴外复核中逐项签署。

## 9. 证据边界

- 本报告是规范文本复核与缺陷登记；不构成 `MODEL-TEST`、`REFINEMENT-PROOF`、`KERNEL-TRACE`、`COMPAT-TEST` 或组织级人工审查的替代。
- 报告中的反例均为抽象状态机内的可达/应可达序列；未在 Rust/Lean/内核中实现或执行这些反例。
- F12 特别说明：缺陷是“失败出口未定义”，不是“异 Task 持有的 Cap 可以成功行使”；§6.2 第 2 阶段仍会拒绝该操作。
- F6 的实现侧“口径 A”偏差已由决策日志登记；本报告指出的是规范文本未承认该偏差、且缺少故障信息/原上下文出口，二者不是同一问题。
