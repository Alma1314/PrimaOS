# SPEC-140：阶段 0 对抗式复核发现项整改（12 项）

> 文档类型：复核整改记录（非设计、非规范、非证明）
>
> 日期：2026-10-17
>
> 复核基线：`8998422`
>
> 触发：阶段 0 组织级复核前的对抗式只读审查。4 个独立视角（Capability/Task、Memory/Exception/Timer/Device、S0-X2 逐操作、SPEC-139 回归）在 `COSIR 原语规范.md` 中发现 12 项未登记问题，其中 W16.1 与 W6/W16.2 的 DMA 帧矛盾是 SPEC-139 引入的回归。
>
> 结论速览：12 项均已在规范文本中逐项处置；S0-X2 的文本缺口已修但**暂回落为未勾选**，待逐项复核；S0-X1 仍不勾选——本轮整改仍是 harness 内会话，未跨过组织级独立性门槛。

## 1. 复核方法与独立性

4 个 reviewer 各自在全新上下文中只读审查，不共享主会话历史，不预设规范正确：

| 视角 | 覆盖范围 |
|---|---|
| R1 | Capability/位置、Task 生命周期、跨节点、WaitObject、Audit |
| R2 | MemorySegment、Exception/upcall、Timer、Namespace、Device |
| R3 | S0-X2：§19 的 46 项逐操作成功/失败语义 |
| R4 | SPEC-139 的 9 项修复逐项回读与回归检查 |

主会话对全部高严重度反例逐条回读规范原文核对，并与 reviewer 结论去重。**独立性边界如实声明：** 本轮仍为 harness 内独立会话，不是组织级第三方人工审查，也不构成 `MODEL-TEST`、`REFINEMENT-PROOF` 或 QEMU 证据。本记录只用于登记与整改，不能作为 S0-X1 的签署依据。

## 2. 逐项处置

| # | ISSUE | 问题 | 规范修订 | 判定 |
|---|---|---|---|---|
| 1 | ISSUE-DEV-010 | W16.1 把 DMA 帧列为物理地址空间资源，又要求物理地址空间资源不与 `frame_universe` 任何帧重叠；W6 把活动 `DMABuffer` 帧纳入 `frame_universe`，任何 DMA 缓冲区都使 W16.1 为假 | §16 与 §15.9 的 W16.1 限定为“MMIO 区域之间两两不交，且 MMIO 区域不与 `frame_universe` 全帧集重叠”；DMA 帧隔离明确由 W6/W16.2 规定；§15.4 前置 2 同步 | Fixed（文本） |
| 2 | ISSUE-TASK-005 | 非 CPU Task 运行期不闭合：self cap 含 `TaskSend`/`TaskManageVSpace`、`initial_caps` 无 kind 限制、`memseg_map`/`dev_mmio_map` 可写非 CPU 空 `vspace`、`recv` 可令其进入 `BlockedRecv`、`send` 可向其邮箱排队 | §8.1：非 CPU self cap 限为 `{Derive, TaskDestroy}`，`initial_caps` 仅允许 Segment；§8.6：非 CPU 除结构性 `root_ns_cap` 与受限 self cap 外不持其他 Cap；§6.1/§8.4/§9.1/§9.2：非 CPU 不执行用户指令流，映射/阻塞/消息目标限 CPU 或代理；W7 增加非 CPU `vspace`/`dev_vspace` 为空；W9 同步 | Fixed（文本） |
| 3 | ISSUE-S0X2-006 | 前置失败无错误出口：`memseg_map` 空 `access` 与缺 `TaskManageVSpace`、`dev_dma_create` 零页无 `ZeroSize`、`ns_resolve` 缺 `MissingRight(NsResolve)`、`dev_mmio_unmap` 引用 §7.3 不存在的 `Overlap` | §7.2 补 `InvalidAccess`/`MissingRight(TaskManageVSpace)`；§15.6 补 `ZeroSize`；§11.7 补 `MissingRight(NsResolve)`；§15.4 重写 `dev_mmio_unmap` 的前置/成功/错误；§8.1 补 `StructuralCap`/`InvalidRootRights` 并定义相关错误 | Fixed（文本） |
| 4 | ISSUE-EXC-014 | W17.7 要求每个 `ExceptionRaised` 事件的源都活动；源销毁后追加式 Audit 必须保留历史事件，§12.6 也保留已排队通知 | W17.7 改为“事件产生时源活动”；历史事件与已排队通知可继续引用该 ID，但不得据此进行新的订阅或授权 | Fixed（文本） |
| 5 | ISSUE-FAULT-007 | `task_create` 与 Bootstrap 未初始化 `pending_exc`/`kernel_resume_frame`；Bootstrap 还漏 `kind`/`dev_vspace`/`entry`/`next_*` | §8.1 步骤 8 与 §14 步骤 1–8 显式初始化全部字段、`local_node` 与 `next_*`；Bootstrap 的 image 映射追加 `SegmentMapped` | Fixed（文本） |
| 6 | ISSUE-EXC-015 | 丢弃最后一个 `ExcSubscribe` Cap 后订阅仍存在，而 `exc_unsubscribe` 又要求持有该 Cap，订阅无法主动撤销 | §4.5/§4.6/§12.4：调用者不再持有任何指向该源且含 `ExcSubscribe` 的 Cap 时（无论 drop、发送、绑定或移动），订阅终止并追加 `ExcUnsubscribed` | Fixed（文本） |
| 7 | ISSUE-NODE-001 | 跨节点分支不可达：`task_create` 总置本地 node、`usable` 要求本地活动目标、无远端 Task 记录；确认后 `HeldBy(remote)` 违反 W4；`Namespace` 无 `node` 字段 | 新增 `KernelState.local_node`、`TaskKind.RemoteProxy`、`Task.remote_id`、内部迁移 `attach_remote_task`、`Namespace.node`、错误 `CrossNodeMount`；W4/W5/W9/W10/W12/W14 同步 | Fixed（文本） |
| 8 | ISSUE-DEV-011 | `dev_irq_create` 发放的 `ExcSubscribe` Cap 权限未写明：若含 `ExcSourceDestroy` 可独立销毁源并破坏 W16.3 | §15.7 固定为 `{Derive, ExcSubscribe}`，关联 `ExcSource` 只能经 `dev_destroy` 销毁；W16.3 保持 | Fixed（文本） |
| 9 | ISSUE-MEM-007 | GPU Segment 的 `memseg_destroy` 只查 CPU `vspace`，Running 非 CPU Task 仍可使用该 Segment | §7.5 增加前置：不存在 Running 非 CPU Task 持有该 Segment 的可用 Cap；新增 `InUseByAccelerator` | Fixed（文本） |
| 10 | ISSUE-AUDIT-002 | `memseg_map`/`unmap`、`dev_mmio_map`/`unmap`、`dev_destroy`、`ns_destroy`、`node_destroy` 未写映射/销毁事件 | §13.1 增 `DeviceMapped`/`DeviceUnmapped`；各成功动作补 `SegmentMapped`/`SegmentUnmapped`/`DeviceMapped`/`DeviceUnmapped`/`ObjectDestroyed` | Fixed（文本） |
| 11 | ISSUE-CAP-004 | `ObjectRef.Device(DevId)` 不含 `DeviceKind`，`well_typed_rights` 无法按设备种类检查 W2，stale 设备 Cap 无类型依据 | `ObjectRef` 改为 `Device(DevId, DeviceKind)`；§4.1/§15.2 说明 kind 随引用携带、销毁后仍可检查 | Fixed（文本） |
| 12 | ISSUE-DOC-003 | 46 项计数与 SPEC-134 §11 的推导、缺口报告的 34 口径冲突；`LengthMismatch`、§19 `now` 等文字不一致 | SPEC-134 §11 改为只以 §19 枚举为准；缺口报告 34→46；§19 的 `now` 改为 `time_now`；`LengthMismatch` 改为“两个应等长的输入列表” | Fixed（文本/证据） |

### 低危文字修正

- `wait_object_observe`：成功动作改为“目标存活时返回 generation；目标已销毁返回 `TargetDead`”。
- `exc_unsubscribe`：明确源已销毁时优先 `TargetDead`，否则未订阅 `NotSubscribed`。
- `exc_subscribe`：删除已无语义的 `exception_kind` 参数；审计记录 `covered_kinds`；`KindMismatch` 仅保留为内核缺陷路径。
- `cap_derive`：注明 `WrongRightType` 是良构状态下不可达的防御性检查。
- `dev_mmio_create`：`InvalidPhysRange` 不再表示“大小非法”，零长度只返回 `ZeroSize`。
- `ns_mount`：跨节点错误改为 `CrossNodeMount`，并注明其由 W12 保证在良构状态不可达。
- SMP 叙述：`task_destroy`/`task_yield`/`raise_exception` 的“没有 Running Task”限定为“该 CPU 核上”；其他核由各自调度器维护。

## 3. 关键取舍

- **W16.1 的范围**：选择只把“不与 `frame_universe` 全帧集重叠”施加到 MMIO 区域，而不是维持“所有物理地址空间资源”。DMA 帧按 W6 就是 `frame_universe` 的保留帧，原措辞不可满足；DMA 的隔离已由 W6/W16.2 覆盖，不需要重复。
- **非 CPU Task**：保留 `task_create` 作为统一创建原语，不新增 GPU 专用创建操作。通过“受限 self cap + Segment-only `initial_caps` + CPU-only 映射/阻塞/消息目标 + Actor 限制”闭合运行期语义；非 CPU 的 `Running` 迁移明确由细化层经 `KernelActor` 完成。
- **跨节点**：选择扩展模型而不是把多节点声明为非目标。新增 `local_node`、`RemoteProxy` 与 `attach_remote_task`，使 `receiver.node ≠ local_node` 可达、`HeldBy(receiver)` 在源节点仍是本地活动对象（满足 W4），并以本地代理的 `TaskCreated` 事件满足 W10。`attach_remote_task` 是 `KernelActor` 内部迁移，不计入 §19 的 46 项公开操作。代价是引入远端 TaskId 与本地 TaskId 的连接绑定，该绑定明确划为传输/细化层。
- **订阅生命周期**：选择“失去最后一个 `ExcSubscribe` Cap 即终止订阅”，而不是把订阅改为独立 Capability 或放宽 `exc_unsubscribe` 的持有前置。这样保留了“订阅期间不要求每次通知都持 Cap”的既有语义，同时保证订阅始终可撤销；需要委托时调用者可先派生一个子 Cap 再转交。
- **设备权限类型**：选择让 `ObjectRef` 携带 `DeviceKind`，而不是让 `well_typed_rights` 读取活动对象表。这样目标销毁后的 stale Cap 仍能做 W2 类型检查，不需要为已销毁设备保留额外状态表。
- **GPU Segment 销毁**：选择增加 `InUseByAccelerator` 前置，而不是把加速器使用期屏障完全推给细化层。POSIR 以“不存在 Running 非 CPU Task 持有可用 Cap”为可判定代理；真正“kernel 已完成/已终止”的判定仍属细化层。

## 4. 对退出条件的影响

- **S0-X1**：本轮新增的 12 项已登记并修订；字面条件“已知空洞均已记录且关闭”在文本层面恢复，但**组织级独立性门槛未跨过**，保持未勾选。
- **S0-X2**：SPEC-140 修复了 `memseg_map`、`dev_dma_create`、`ns_resolve`、`dev_mmio_unmap` 等错误出口，并补齐错误定义；但本轮同时改动了 §19 相关文本，**暂回落为未勾选**，待逐项复核后恢复。
- **S0-X3**：未改 Lean 模型；`lake build` 8 jobs 复验通过。
- **S0-D1/D2**：对象能力模型与不变量文本的内部一致性提高；仍为“已定义，未证明”。

## 5. 复验

| 命令 | 结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p cosir --quiet` | 146 + 42 + 3 passed |
| `lake build`（`lean/`） | 8 jobs，成功 |
| `git status --short` | 仅规范、跟踪文档与报告改动；无 Rust/Lean/QEMU 代码改动 |

以上复验只证明既有模型/形式化工件未回归，不证明新增规范条文已被实现或机器验证。

## 6. 证据边界

- 本记录是规范文本整改与文档登记；不构成 `MODEL-TEST`、`REFINEMENT-PROOF`、QEMU 证据或组织级独立性审查。
- 新增的 `RemoteProxy`/`attach_remote_task`、订阅级联终止、`InUseByAccelerator`、`DeviceMapped`/`DeviceUnmapped` 等条文在 `cosir`、Lean 与内核中**仍无对应实现**；不能把“文本闭合”外推为“实现或证明闭合”。
- 跨节点模型的“远端 TaskId ↔ 本地 TaskId”绑定、GPU kernel 完成/终止的判定、IOMMU/DMA in-flight 屏障仍属细化层义务，需在后续细化切片中单独证明。
- 本轮为 harness 内对抗式复核；即使文本问题全部修复，也不能替代组织级第三方签署。
