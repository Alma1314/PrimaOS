# Prima OS 内核原语对照评估

> 日期：2026-10-17
> 类型：架构审查记录；不修改原语规范，也不宣告阶段晋级。
> 范围：对照 Prima 当前抽象语义与实验性 x86_64 ABI。外部项目只按所引一手资料比较，不把其设计目标当作已验证的性质。

## 结论

Prima 的长处是把 Capability 的派生、唯一位置、消息内原子转移、对象生命周期及失败原子性写成可检查的状态迁移，并已用独立 ring3 服务和共享 Segment 跑通原生应用路径。主要风险在于：部分抽象操作尚未细化为用户接口，资源归属和背压不构成拒绝服务保证，且一处已知 IPC 行为与规范不一致。规范的“12 类常规机制”是类别数，不是已实现 syscall 数；当前 [阶段跟踪](../../../Prima%20OS%20实现阶段.md)仍把正式阶段置于阶段 0。

## 与其他内核的对照

| 维度 | 外部参照 | Prima 的位置与取舍 |
|---|---|---|
| 权限与资源分配 | seL4 以 [Capability](https://docs.sel4.systems/Tutorials/capabilities.html) 授权，并将 [Untyped 内存](https://docs.sel4.systems/Tutorials/untyped.html)交给用户态再类型化；Zircon 用 [Handle 权限与 Job](https://fuchsia.dev/fuchsia-src/concepts/kernel/concepts)约束对象访问及部分资源使用。 | Prima 的 [§4](../../../COSIR%20原语规范.md)明确衰减、位置与 stale token；第一版不提供公开的派生子树撤销，`memseg_create` 等环境操作也不要求预持资源凭据。固定全局容量不等于任务配额。 |
| IPC 与背压 | seL4 使用 [Endpoint/IPC](https://docs.sel4.systems/Tutorials/ipc.html)；Zircon 的 [Channel](https://fuchsia.dev/fuchsia-src/concepts/kernel/concepts)原子移动 Handle；FTL 采用异步 Channel，并讨论 [peek/receive 与拉取式背压](https://seiya.me/blog/microkernel-ipc-design)。 | Prima 用指向 Task 的 `TaskSend`、`send/recv` 与共享 Segment；现有 ABI 单条消息携带一个 `u64` payload、最多两条 Capability。大数据路径已有 8 KiB QEMU 证据，但通用背压、超时与执行中取消未闭合。直接绑定 Task 身份便于授权检查，也使服务替换时的凭据重发成为待验证成本。 |
| 等待与地址空间 | Zircon 有 [wait one/many 与 Port](https://fuchsia.dev/fuchsia-src/concepts/kernel/concepts)，内存对象为 VMO/VMAR；seL4 将内存对象的分配与映射分别授权。 | Prima 已在规范中采纳 `WaitObject` 与 `memseg_protect`，但 [实验性 syscall 表](../../../prima-abi/src/lib.rs)尚无对应入口；阶段 5 的 futex/wait_multi、匿名页和 W^X 仍是[待裁决前置](../../../Prima%20OS%20实现阶段.md)。 |
| 设备边界 | Zircon 用 [Resource](https://fuchsia.dev/fuchsia-src/reference/kernel_objects/resource)授权 MMIO、I/O port 与 IRQ 范围。 | Prima 规范覆盖 I/O port、MMIO、IRQ、DMA；当前真实用户接口只验证受控 I/O port 路径，不能外推为完整设备隔离。 |
| 证明等级 | seL4 的代码级证明适用于[列明的配置](https://docs.sel4.systems/projects/sel4/verified-configurations.html)，并非任意平台或功能的通用证明。 | Prima 的 Lean 工件只覆盖[列明的模型子集](../../../lean/README.md)；host 测试、QEMU trace 与模型证明均不能替代真实内核到完整抽象语义的细化证明。 |

## 优点

1. **授权语义具体。** `CapId` 的派生链、位置唯一性与 stale token 规则减少“句柄还在但权限已失效”的歧义；`send` 的 Capability 移动与审计被放在同一原子迁移里，见[规范 §4、§6、§9](../../../COSIR%20原语规范.md)。
2. **机制与服务职责有边界。** 内核不解释服务名或业务 trace；服务注册、业务日志、存储和受控端口驱动由 ring3 程序承担，Namespace 与 Audit 暂留机制层，见[机制基线审查](2026-10-15-exo5-mechanism-baseline-review.md)与[阶段 4 收口证据](2026-10-16-stage4-closure-report.md)。
3. **失败路径可复核。** 规范要求先验证再提交，失败保持抽象状态不变；实现中已有固定容量预检和部分负例测试，但这只证明相应测试范围内的行为。

## 缺陷与风险

| 优先级 | 发现 | 影响与证据 |
|---|---|---|
| P0 | 抽象 `send` 允许向 `BlockedRecv` 直接递送 Capability；当前 [`send_call`](../../../kernel/src/syscall.rs)拒绝这一路径。 | 服务需先发不带 Capability 的唤醒消息，增加协议步骤；见[服务管理器迁移报告 §5](2026-10-14-stage4-servicemgr-migration-body-report.md)。需统一规范与实现，不能仅靠服务层约定掩盖差异。 |
| P0 | 规范 §6.2 将资源耗尽、配额与拒绝服务排除在安全结论之外；实现使用共享的消息、Capability、对象和审计容量。 | 容量满时返回错误只能保证局部失败可预期，不能保证一个任务无法耗尽其他任务所需资源。先明确资源归属、保留量与拒绝策略，再对所有创建和排队路径做原子准入。 |
| P1 | `WaitObject`、`memseg_protect`、Node、MMIO/IRQ/DMA 等规范面大于已实现 ABI。 | 不应把“已定义”写成“可调用”。尤其等待和页权限调整是阶段 5 的近期依赖；完整设备与跨节点机制属于更远范围。 |
| P1 | `Task` 还不是完整的 Process/Thread/ShareGroup 持有者模型；公开子树撤销未冻结。 | 多线程共享地址空间、Capability 归属、服务重启后的授权回收须另行设计；见[规范 §22](../../../COSIR%20原语规范.md)。 |
| P2 | Namespace 与 Audit 放在内核可获得可信解析/溯源，但扩大了机制层语义及存储压力。 | 应持续用可替代性、性能和 TCB 计量评审其归属；现有[机制基线审查](2026-10-15-exo5-mechanism-baseline-review.md)决定暂保留并限制业务语义进入内核。 |

## 建议的闭合顺序与验收

1. **IPC 一致性。** 让阻塞接收者可靠收到 payload 与全部 Capability 句柄，核对位置、审计及失败原子性；QEMU 覆盖直接投递与排队两条路径，随后移除不再需要的唤醒协议。若保留当前拒绝行为，必须先修订抽象规范并解释其与目标语义的关系。
2. **资源隔离。** 为消息、对象、页帧与审计选择统一的任务或上级持有者预算模型；测试单个不可信任务耗尽自身预算后，另一任务仍能完成受保留容量保障的操作。区分硬上限、可回收资源与追加式审计历史。
3. **规范到 ABI 的逐项台账。** 每个操作登记“仅规范 / 模型 / 内核 / ring3 可调用 / QEMU 验证”；只有实现路径、失败负例和对应运行证据齐备才升级状态。优先评审 `WaitObject`、`memseg_protect` 与阶段 5 的具体消费者；Node、完整 MMIO/IRQ/DMA 保持未实现标记。
4. **兼容层入口。** 在冻结 POSIX/NT 接口前裁决 Task 与共享持有者模型、撤销域及超时/取消语义，再用一个实际程序检验 IPC 次数、内存复制和恢复成本。

本报告是比较与建议，不是上述修复的完成证据；任何阶段状态变化仍以[实施进度与证据跟踪](../../../Prima%20OS%20实现阶段.md)为准。
