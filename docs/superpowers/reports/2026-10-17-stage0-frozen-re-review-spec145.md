# SPEC-145：阶段 0 冻结基线复核与签署

> 文档类型：阶段 0 退出条件复核/签署记录
>
> 日期：2026-10-17
>
> 冻结基线：`16ff1fc`（`fix(stage0): close node_attach re-review residuals (SPEC-144)`）
>
> 唯一审查对象：`git show 16ff1fc:"COSIR 原语规范.md"`（2754 行，SHA-256 `17f80349…af93b`）
>
> 结论：**S0-X1、S0-X2 在规范文本层面通过。** 阶段 0 退出条件可勾选；本结论不覆盖 `cosir`/Lean/内核实现与形式化证明（见 §5 边界）。

## 1. 复核范围与基线

- 复核 1：SPEC-142 第三方组织级审查（五路独立对抗复核 + 主审回读）发现 29 项（11 阻断）。
- 整改：SPEC-143 逐项修订（跨节点 v1 收窄、`node_attach`、`NodeIdentity`/`NodeId`、`reserved_frames`/GPU 池、`Faulted` 降级等）。
- 残修：SPEC-144 修复复核发现的 3 项 `node_attach` 残修（`peer` 绑定、CapId 取回、目标端代理创建顺序）。
- 本轮：以 `16ff1fc` 的**冻结 blob** 为唯一对象，核对 29 项整改、3 项残修、§19 现行 47 项操作与文档口径；不再读取移动中的工作区。

## 2. 核对结果

| 检查 | 结果 |
|---|---|
| 29 项 ISSUE 状态 | 跟踪表中 29 项均为 `Fixed（SPEC-143，2026-10-17）；闭合判断交复核者`；另有 `ISSUE-NODE-011` 为 `Fixed（SPEC-144）` |
| 跨节点 v1 | `§2.2` 声明不传 Capability；`§8.5`/`§9.1` 以 `NonPortableCap` 拒绝非空 `transferred_caps`；`InTransit`/`origin` 标记为 v1 不产生；`node_attach` 提供首条发送入口；`attach_remote_task` 内部迁移的 TaskId/CapId 路径已明确 |
| Node 身份 | `§3` 定义 `NodeId`（本地键）与 `NodeIdentity`（平台身份）；`local_node`/`Task.node`/`Node.peer`/`NodeTransport` 均为 `NodeIdentity`；`NodeId` 由 `next_node` 分配 |
| 帧/GPU | `KernelState` 有 `reserved_frames`/`gpu_reserved_frames`；`§14` 有 `reserved_frames`/`gpu_pools`/`gpu_reserved_frames` 输入；W6/W16.1/§15.6/§15.8/§7.1/§7.5 的分配/归还路径一致 |
| 恢复/调度 | `§2.2`/`§12.9`/`§8.2` 明确不做原上下文重播；`cpu_core_count` 为平台常量，`schedule` 有全系统上界；非 CPU `Faulted` 被 W9 排除 |
| 操作面 | §19 表实算 **47 项常规操作 + Bootstrap**；`node_attach` 在 Node 行；`§19` 注释与跟踪表/白皮书/缺口报告均为 47 |
| 失败语义 | 47 项均有显式错误清单或“无失败分支”声明；`NonPortableCap`/`InvalidTaskKind`/`InvalidGeneration`/`AlreadyConnected`/`AlreadyBooted` 均已就地定义 |
| 审计/事件 | `ns_*` 成功动作补 `CapMoved`/`CapDropped`/`CapDerived`/`Ns*`；`task_destroy` 补映射/订阅事件；`timer_set` 补 `CapMinted`；`next_event`/`next_msg` 有分配规则 |
| 残修核对 | `node_attach` 使用 `node.peer`；CapId 以扫描 `HeldBy(caller)` 取回；目标端先验证接收方再创建代理；三项均已在冻结 blob 中 |
| 文档口径 | 跟踪表 S0 行、白皮书阶段 0、缺口复核包、SPEC-143/144 报告无 46 项现行口径；历史行保留旧口径但不作现行结论 |

## 3. 结论

- **S0-X1（无已知且未记录的对象生命周期空洞）：通过（规范文本层面）。** 已知空洞均已登记并在 SPEC-143/144 中修订；本轮冻结复核未发现新的未登记空洞。
- **S0-X2（每项操作有明确成功/失败语义）：通过（规范文本层面）。** §19 现行 47 项均有成功动作与失败语义；新增 `node_attach` 与 `NonPortableCap`/`InvalidTaskKind`/`InvalidGeneration`/`AlreadyConnected` 均有定义。
- **S0-D1–D4、S0-X3：** 不因本轮改变；仍为“已定义，未证明/已有 Lean 可执行模型”。

## 4. 签署

- 按用户“视为通过第三方组织级审查”的授权口径，本记录作为阶段 0 的**范畴外复核签署**。
- 签署只覆盖规范文本与操作语义；不覆盖实现、QEMU、`REFINEMENT-PROOF` 或机器证明。
- 本记录与 SPEC-142/143/144 共同构成 S0-X1/S0-X2 的证据链。

## 5. 证据边界（必须保留）

1. `cosir`/Lean/内核**尚未同步** `reserved_frames`/`gpu_pools`、`node_attach`、47 项操作面、`DeviceKind` 载荷联合与 `NodeIdentity`/`NodeId` 分离；这些是阶段 1–4 的实现/细化缺口，不能把“文本通过”外推为“实现或证明通过”。
2. `InTransit`/`origin`/`TransfersInProgress` 是保留构造；后续跨节点委托版本必须重新证明 W3/W4/W13。
3. `node_attach` 的“扫描 `HeldBy(caller)` 取回新 CapId”是规范层的抽象选择；实现必须保证扫描唯一命中，否则应改为内部迁移返回 `(TaskId, CapId)` 并重新证明 W14。
4. 本轮是冻结文本复核；未运行新的 QEMU/内核测试。既有复验为 `cargo test --workspace` 786 passed、`lake build` 8 jobs，仅证明既有工件未回归。
