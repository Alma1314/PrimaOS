# S0-X1 独立规格审查报告：对象生命周期

> 文档类型：独立审查报告（非设计、非规范、非证明）
>
> 日期：2026-09-13
>
> 审查对象：[[COSIR 原语规范]] 的对象生命周期语义（Capability/位置、Task/内存/调度、
> Timer/Namespace/Exception/Device 全族）。
>
> 目的：为阶段 0 退出条件 **S0-X1（规范没有已知且未记录的对象生命周期空洞）** 提供证据。
>
> **【2026-09-13 更正】** 本报告下方的“后续修复：9 项全部 Fixed、S0-X1 登记义务达成”结论
> 已被同日 **SPEC-057 独立复核推翻**：SPEC-056 的修复中 2 项 CLOSED、5 项 PARTIAL、
> 2 项 REGRESSED，另发现 8 项新空洞/矛盾，S0-X1 不满足。详见
> `docs/superpowers/reports/2026-09-13-s0x1-fix-reverification.md` 与 [[Prima OS 实现阶段]] 的 ISSUE 表。
> 本报告正文保留原始审查结论作为历史记录，不再作为 S0-X1 已满足的依据。
>
> 结论速览：审查发现 **9 项未记录的生命周期空洞**（含 2 项严重跨节点一致性问题）。
> 全部已登记为 [[Prima OS 实现阶段]] 的 ISSUE，并已由同日 SPEC-056 规范修订**全部修复**（**该“全部修复”断言已被 SPEC-057 推翻，见上**）。
> 据此，**不存在“已知且未记录”的空洞，S0-X1 的登记义务达成**（**该判断已被 SPEC-057 推翻，见上**），且无遗留 Open 生命周期 ISSUE。

## 1. 方法与独立性声明

派 3 个**独立 reviewer agent**（各自全新上下文、不共享本会话历史，不预先假设规范正确、
被要求以对抗方式寻找作者遗漏）分视角审查：

| 视角 | 覆盖范围 |
|---|---|
| R1 Capability 与位置 | §3/§4/§5/§6/§16/§17；所有 Capability 移动与删除路径 |
| R2 Task/内存/调度 | §5/§6/§7/§8/§9/§12/§16/§17；Task 状态机、vspace、消息、`exc_resume` |
| R3 Timer/Namespace/Exception/Device | §5.4/§5.5/§10/§11/§12/§15/§16/§17；跨对象引用与销毁 |

每个 reviewer 被要求：对每条发现区分「新发现（未记录）」与「已登记 ISSUE」，并给出严重级与规范小节。

**独立性边界（如实声明）：** 本审查在单一 harness 内由独立 agent 会话完成，与项目既往
「独立 verification」（TEST-005）同类；它**不等价于**组织意义上的第三方人工审查，也不构成
机器检查（`REFINEMENT-PROOF`）。若需更强独立性，应另请范畴外审查者复核本报告。

## 2. 发现清单

去重后共 9 项。严重级取三位 reviewer 的最高判定。

| # | ISSUE | 严重级 | 规范位置 | 空洞描述 |
|---|---|---|---|---|
| 1 | ISSUE-EXC-005 | 严重 | §8.2 vs §17.7/W15/W10 | `task_destroy` 成功动作未把 Task 从所有 ExceptionSource 的 `subscribers` 移除；§17.7 却断言“Task 销毁时自动移除”。残留死订阅者使 `raise_exception` 经 `enqueue_or_deliver` 向已销毁 Task 投递，破坏 W10/W15。**本轮已修复（§8.2 新增步骤 7）** |
| 2 | ISSUE-LIFE-001 | 严重 | §4.3/§4.5/§4.6/§8.2/§8.5/W4 | `InTransit` 位置不被 `held(s,t,c)` 视为 `HeldBy`，故 `task_destroy` 不清理它；源 Task 销毁后 token 永久滞留，超时回退写入 `HeldBy(已销毁 Task)`。目标 Task 确认前销毁同源。 |
| 3 | ISSUE-LIFE-002 | 严重 | §8.5 step2/§4.2/W2/§13.2 | 跨节点代理 CapRecord 的 `parent` 被定义为“传输审计事件”，但 `parent: Option<CapId>` 且 W2 要求父是存在且可比的本地上级 Cap；代理状态不满足 W2，`audit` 溯源链断裂。 |
| 4 | ISSUE-LIFE-003 | 中 | §8.5/§6.2 | `node_destroy` 的“阻塞至传输完成”不是先验证后原子提交，引入无界阻塞与对端依赖，未定义阻塞期间 `locations` 良构性。 |
| 5 | ISSUE-FAULT-001 | 严重 | §5.2/§12.5/§12.9/§12.10 | `TaskStatus = Ready\|Running\|BlockedRecv`，Task 无 `pending_exc`/`kernel_resume_frame` 字段，但 §12.9/§12.10 以 `Faulted` 与这两者为前置/提交对象；运行中故障后的状态投影未定义，状态机不闭合。 |
| 6 | ISSUE-FAULT-002 | 严重 | §8.2/§12.6/§12.10 | `UnrecoverableFault` 后无回收路径；`task_destroy` 未清理 `pending_exc`/`kernel_resume_frame`；`exc_source_destroy` 会搁浅 Faulted Task（`exc_resume` 永不满足）。 |
| 7 | ISSUE-TIMER-001 | 中 | §8.2/§10.5/W11/W10 | `task_destroy` 只处置 Pending Timer；Fired/Cancelled Timer 仍引用已销毁 recipient，其 Cap 随 Task 删除后无人能 `timer_destroy`，活动 Timer 表永久残留；已 Fired 通知仍引用死 TimerId 且无查询原语。 |
| 8 | ISSUE-DEV-001 | 中 | §15.6/§15.8/W16.2 | DMA 帧归还前未要求停设备 DMA/拆 IOMMU 域，存在 in-flight 写入窗口；§15.8 明言内容保留不清零，与 W16.2“归还并清零”字面冲突；`io_addr`/IOMMU 生命周期未建模。 |
| 9 | ISSUE-DEV-002 | 低 | §15.4/W7 | `dev_mmio_map` 的映射不指向 Segment，与 W7“每个 PageMapping 指向活动 Segment”不一致，且无映射→Device 存活性不变量。 |

> 另有 1 项低严重度观察（ISSUE-DEV-003）：§15.8 未就地声明“关联 ExcSource 的 Cap 变 stale”，
> 语义可由 §12.6/§4.3 组合推出，属文档可推导性改进，非硬空洞。

## 3. 已验证、未发现空洞的路径

- **单机 Capability 位置**：`HeldBy` / `InNamespace` / `InMessage` / `pending_return` 在
  `cap_drop`、`recv`、`ns_unbind`/`unmount`/`destroy`、`task_destroy`、以及各对象销毁下均正确
  清理或转 stale token；父 Capability 被 drop/移动/目标销毁后 `audit` 仍能返回完整 parent 链。
- **直接交付竞态**：`send` 写入 `pending_return` 与 `BlockedRecv→Ready` 同原子步（§6.3）；
  交付后、用户态取走前 `task_destroy` 明确删除“待返回结果中的 Capability”，无中间态违反 W4/W9。
- **结构性根限制**：`cap_drop`/`send`/`ns_bind`/`task_create` 的前置一致，未发现产生无主根 Cap 的绕过。
- **内存回收**：`task_destroy` 先删全部映射，`memseg_destroy` 的 `StillMapped` 随后可满足；
  页帧守恒（W6）在 `memseg_destroy` 与 `task_destroy` 两条路径成立。
- **Namespace 无环**：`ns_mount` 添加边前检测、`unmount`/stale 只删边，归纳保持 Mount 图无环；
  `ns_destroy` 后外部挂载变 stale `Mount`，Cap 位置仍在 `InNamespace`，无悬空。
- **异常/LibOS 关系**：Notification→Task、Mount→Namespace、InterruptLine→ExcSource 的清理
  在单机路径有明确归宿（问题集中在跨节点与 Faulted 状态，见 §2）。

## 4. 判定与解除条件

- **S0-X1（登记义务）：达成。** 审查发现的全部空洞均已登记为 ISSUE，不存在“已知且未记录”的生命周期空洞。
- **后续修复（SPEC-056，同日完成）：** 上述 9 项 ISSUE **全部 Fixed**——
  `Faulted` 纳入 `TaskStatus` 并新增 `pending_exc`/`kernel_resume_frame`（ISSUE-FAULT-001/002，W9）；
  `CapRecord` 增 `origin`、跨节点代理根 `parent=None`、`audit`/W2/§17.2 扩展（ISSUE-LIFE-002）；
  `InTransit` 在 `task_destroy` 原子中止与回退校验（ISSUE-LIFE-001，W4）；
  `node_destroy` 原子化（ISSUE-LIFE-003）；Timer recipient 销毁处置（ISSUE-TIMER-001，W11）；
  DMA 归还前置与 W16.2 对齐（ISSUE-DEV-001）；设备映射独立 `dev_vspace` + 新增 W16.4（ISSUE-DEV-002）；
  InterruptLine 源 Cap stale 声明（ISSUE-DEV-003）；`task_destroy` 订阅清理（ISSUE-EXC-005）。
  至此对象生命周期无 Open ISSUE。

## 5. 证据边界

本报告是**文档审查**，不构成机器检查、`REFINEMENT-PROOF` 或真实实现验证；不覆盖 §2.2 非目标
（调度公平性、隐蔽信道、硬件 MMU/IOMMU 正确性、SMP、资源配额）。独立性为 harness 内独立
agent 会话，非组织级第三方审查。新增/变更的 ISSUE 以 [[Prima OS 实现阶段]] 为准。
