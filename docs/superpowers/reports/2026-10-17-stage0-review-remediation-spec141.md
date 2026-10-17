# SPEC-141：阶段 0 第二轮组织级复核发现项整改（11 项）

> 文档类型：复核整改记录（非设计、非规范、非证明）
>
> 日期：2026-10-17
>
> 复核基线：`55cb0aa`
>
> 触发：第二轮组织级阶段 0 复核在 SPEC-139/140 整改后仍发现 11 项未登记问题，涉及 Task 代码 Segment 存活性、SMP 调度语义、Node/RemoteProxy 状态完整性、跨节点投递、订阅授权不变量、W16.1 物理地址映射与 S0-X2 错误语义。
>
> 结论速览：11 项均已在 `COSIR 原语规范.md` 中逐项处置；S0-X1/S0-X2 仍不勾选——本轮整改仍为 harness 内会话，且整改本身需要下一轮逐项复核。

## 1. 复核方法与独立性

第二轮复核以 `55cb0aa` 为基线，逐条回读 `COSIR 原语规范.md` 与跟踪文档，并对 SPEC-139/140 的修复做回归检查。复核按五个视角组织：Task/Memory/SMP、Node/RemoteProxy、Exception/WaitObject、Device/Frame、S0-X2 操作面与错误出口。

**独立性边界如实声明：** 本轮仍为 harness 内独立复核，不是组织级第三方人工审查，也不构成 `MODEL-TEST`、`REFINEMENT-PROOF` 或 QEMU 证据。本记录只用于登记与整改，不能单独作为 S0-X1 的签署依据。

## 2. 逐项处置

| # | ISSUE | 问题 | 规范修订 | 判定 |
|---|---|---|---|---|
| 1 | ISSUE-TASK-006 | `task_create` 前置 1 只要求 `code_seg_cap` 持有且授权 `SegMapR/X`，未要求目标 Segment 活动/可用；stale cap 仍满足字面前置，成功动作会建立指向已销毁 Segment 的映射（W7），且错误清单无 `InvalidSegment`/`TargetDead` | §8.1 前置 1 改为“持有、可用且指向活动 Segment”，并记 `segment = code_seg_cap.target`；错误清单补 `InvalidSegment`/`TargetDead` | Fixed（文本） |
| 2 | ISSUE-TASK-007 | §8.4 `schedule` 的“当前没有 Running CPU Task”“任意时刻至多有一个 Running Task”与 §5.2 SMP 扩展、W9 的“每核至多一个/全系统 ≤ 核数”矛盾；SPEC-140 只修了 `task_destroy`/`task_yield`/`raise_exception` | §8.4 前置改为“该 CPU 核无 Running CPU Task”；成功语义改为每核至多一个、全系统不超过核数；注明抽象状态不记录核归属，由细化层投影 | Fixed（文本） |
| 3 | ISSUE-NODE-002 | `node_create` 未初始化 `Node.creator`/`status`，未分配 NodeId/推进 `next_node`；`attach_remote_task` 未写 RemoteProxy 的 `entry`，铸根 Cap 未推进 `next_cap`；`NodeStatus`/`NodeConnect` 无消费者；holder 销毁后代理失去唯一 `TaskSend` 且无法重建 | §8.5 `node_create` 显式分配 NodeId/推进 `next_node`、`creator = caller`、`status = Connected`；写明 `NodeConnect`/`status` 的细化层职责；`attach_remote_task` 设 `entry = 0` 并推进 `next_cap`；§8.5 登记代理不可达边界（与 §2.2 一致） | Fixed（文本/边界） |
| 4 | ISSUE-NODE-003 | `attach_remote_task` 先列 3 条前置，随后称“无失败分支”，下一段又称“本操作无前置条件”；目标节点需为发送方建 proxy，但协议未保证目标节点存在源 Node；失败语义段误提 `transport`（`node_create` 参数） | §8.5 改写 `attach_remote_task` 失败语义为“协议前置不成立属传输层/内核缺陷，不返回用户错误”；跨节点协议增加目标节点源 Node 前置；`transport` 段移动到 `node_create` | Fixed（文本） |
| 5 | ISSUE-SEND-001 | 跨节点确认阶段只校验源端本地 `RemoteProxy`；目标节点 `enqueue_or_deliver` 不校验远端实际接收方 kind，可向 GPU/NPU/FPGA Task 投递消息，绕过 §8.6/§9.1 | §8.5 确认阶段增加 kind 检查：非 CPU 接收方拒绝接收，并按与目标已销毁相同的回退/丢弃规则处置 | Fixed（文本） |
| 6 | ISSUE-EXC-016 | W15 只要求 subscriber 是活动 Task 且唯一，未要求其持有含 `ExcSubscribe` 的可用 Cap；§4.5/§4.6/§12.4 的级联终止规则没有反映为可检验不变量 | W15 增加“每个 subscriber 至少对应一个持有、可用、指向该源且含 `ExcSubscribe` 的 Capability；失去最后一个即终止订阅” | Fixed（文本） |
| 7 | ISSUE-DEV-012 | W16.1/`dev_mmio_create` 用 `PhysAddr` 区间与 `Set<FrameId>` 判重叠，但 §3 称 FrameId 与 PhysAddr 互不混用且全文无 `FrameId → PhysAddr` 映射 | §3 定义平台静态单射 `frame_paddr : FrameId → PhysAddr`；§15.4/§15.9/§16 改用帧物理区间 `[frame_paddr(f), frame_paddr(f)+PAGE_SIZE)` | Fixed（文本） |
| 8 | ISSUE-S0X2-007 | §15.7 `dev_irq_mask`/`dev_irq_unmask` 有前置、成功动作与错误清单，却不在 §19；DOC-007 单方面按细化层排除，规范正文未分类 | §15.7 与 §19 注记明确二者只改变中断控制器屏蔽状态、不改变抽象状态，不计入 46 项 POSIR 公开操作 | Fixed（文本/证据） |
| 9 | ISSUE-S0X2-008 | §7.2 `memseg_map` 的 `access ⊄ segment.allowed_access` 无清晰错误出口（`InvalidAccess` 括号只写空集，§7.4 同类违反却写成“空或超过”） | §7.2 `InvalidAccess` 改为“`access` 为空或超出 `segment.allowed_access`”，与 §7.4 一致 | Fixed（文本） |
| 10 | ISSUE-S0X2-009 | `ns_resolve` 的 `MountTargetDead` 未按 §6.2.1“新标识就地定义”规则定义，也不在 §6.2.1 列表 | §11.7 就地定义“解析路径经过的 Mount 目标 Namespace 已销毁”，并加入 §6.2.1 新标识列表 | Fixed（文本） |
| 11 | ISSUE-S0X2-010 | Bootstrap 的输入前置（`image_frames` 重复/非子集、`entry` 越界）无失败语义；§6.2.1 默认规则也缺对应标准错误标识 | §14 增加失败语义：重复调用返回 `AlreadyBooted`；其余前置失败属平台引导输入/内核缺陷，不产生用户可观察错误且状态不变 | Fixed（文本/边界） |

## 3. 关键取舍

- **代码 Segment 存活性**：选择把 `usable` 写进前置并显式补 `InvalidSegment`/`TargetDead`，而不是依赖 §6.2.1 的隐式包含；这样避免 stale cap 在成功路径上生成悬空映射。
- **SMP 调度**：选择按“每 CPU 核”表述 `schedule`，并声明抽象状态不记录核归属；这样与 §5.2/W9 一致，同时不引入新的核对象或 per-core 状态字段。
- **Node/RemoteProxy 状态**：选择在 `node_create` 显式初始化 `creator`/`status` 与 `next_node`，在 `attach_remote_task` 显式写 `entry = 0` 并推进 `next_cap`；`NodeConnect`/`NodeStatus` 保留为传输/细化层职责，而不是删除权利或字段。
- **目标节点 Node 前置**：选择在跨节点协议中要求目标节点存在源 Node，而不是让 `attach_remote_task` 隐式创建 Node；这样 `node_create` 仍是唯一的 Node 创建入口。
- **跨节点非 CPU 目标**：选择在目标节点确认阶段拒绝，而不是在源端要求 `RemoteProxy` 携带 kind；源端在代理创建时尚不知道远端 Task 的实际 kind，目标节点是唯一能可靠判定的位置。
- **W15 订阅授权**：选择把“订阅由最后一个 `ExcSubscribe` Cap 支撑”提升为不变量，而不是只保留 §4.5/§4.6/§12.4 的散文规则；这样级联终止可被证明/检查捕获。
- **`frame_paddr`**：选择增加平台静态单射函数，而不是把 `FrameId` 直接当作物理页号；这样保持“ID 互不混用”的既有约定，同时让 W16.1 可判定。
- **`dev_irq_mask`/`unmask`**：选择明确归类为 COSIR 细化层操作，而不是把它们加入 §19 使操作面变为 48 项；这与 §18.0 的中断控制器细化职责一致。
- **Bootstrap 失败语义**：选择“重复调用 `AlreadyBooted`，其余前置失败属引导/内核缺陷”，而不是发明用户可见错误码；Bootstrap 在用户态启动前由 `KernelActor` 调用。

## 4. 对退出条件的影响

- **S0-D1**：Node/RemoteProxy 状态初始化、W15 订阅授权不变量与 W16.1 物理地址映射得到补全，对象能力状态模型的一致性提高。
- **S0-X1**：本轮新增的 11 项已登记并修订；字面条件“已知空洞均已记录且关闭”在文本层面恢复，但**组织级独立性门槛未跨过**，保持未勾选。
- **S0-X2**：`task_create`、`memseg_map`、`MountTargetDead`、Bootstrap 与 `dev_irq_mask/unmask` 分类得到明确处置；操作面仍按 §19 的 46 项计数。仍需逐项复核后才恢复勾选。
- **S0-X3**：未改 Lean 模型；`lake build` 8 jobs 复验通过。

## 5. 复验

| 命令 | 结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p cosir --quiet` | 146 + 42 + 3 passed |
| `lake build`（`lean/`） | 8 jobs，成功 |
| `git status --short` | 仅规范、跟踪文档、`docs/README.md`、DOC-007 与新整改报告改动；无 Rust/Lean/QEMU 代码改动 |

以上复验只证明既有模型/形式化工件未回归，不证明新增规范条文已被实现或机器验证。

## 6. 证据边界

- 本记录是规范文本整改与文档登记；不构成 `MODEL-TEST`、`REFINEMENT-PROOF`、QEMU 证据或组织级独立性审查。
- 新增的 `frame_paddr`、W15 订阅授权不变量、跨节点非 CPU 拒绝、`node_create`/`attach_remote_task` 状态初始化等条文在 `cosir`、Lean 与内核中**仍无对应实现**；不能把“文本闭合”外推为“实现或证明闭合”。
- `dev_irq_mask`/`dev_irq_unmask` 的细化层分类、`RemoteProxy` 不可达边界与 Bootstrap 输入缺陷边界均需在后续细化/复核中确认。
- 本轮为 harness 内复核；即使文本问题全部修复，也不能替代组织级第三方签署。
