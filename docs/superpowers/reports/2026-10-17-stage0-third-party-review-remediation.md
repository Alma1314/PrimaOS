# SPEC-137：阶段 0 范畴外复核发现项整改

> 文档类型：复核整改记录（非设计、非规范、非证明）
>
> 日期：2026-10-17
>
> 复核基线：`96b7ab0`
>
> 触发：阶段 0 范畴外复核（2026-10-17）在 SPEC-058/059 的 19 项旧 ISSUE 之外发现
> 5 项未登记问题，其中 1 项违反 W16.1。按维护规则 5，发现即登记并修订规范。
>
> 结论速览：5 项均已在 `COSIR 原语规范.md` 中逐项处置；S0-X1 仍不勾选——
> 本轮修订本身仍需下一轮范畴外复核。S0-X2 的操作面计数由 34 修正为 **35**。

## 1. 逐项处置

| # | 新 ISSUE | 问题 | 规范修订 | 判定 |
|---|---|---|---|---|
| 1 | ISSUE-DEV-008 | W16.1 未被设备创建操作保持：`dev_mmio_create` 漏查 `free_frames`/DMA 帧；`dev_ioport_create`、`dev_irq_create` 声明“无失败分支”，可创建重叠端口区间/IRQ 号 | §15.4 前置 2 扩为物理地址空间资源全查（现有 MMIO、DMA 帧、`free_frames`、活动 Segment 帧）；§15.5 增加端口区间不交前置与 `Overlap`/`ZeroSize` 错误；§15.7 增加 IRQ 号唯一前置与 `Overlap` 错误；§15.9/§16 的 W16.1 按资源空间（物理地址、I/O port、IRQ 号）澄清“两两不交” | Fixed（文本） |
| 2 | ISSUE-WAIT-001 | `wait_object_observe`/`destroy`/`wait_objects`/`signal_object` 的前置要求目标活动（或 Capability `usable`），错误清单却列 `TargetDead`；因 §4.3 `usable` 含 `live(target)`，该错误不可达 | 四个操作改为“Capability 由调用者持有且 `CapRecord` 指向 `WaitObject`”；目标存活性检查失败时返回 `TargetDead`。与 `exc_unsubscribe` 等 stale-cap 清理路径的既有口径一致 | Fixed（文本） |
| 3 | ISSUE-LIFE-008 | 最后一个 Capability 位置删除后，对象仍永久活跃且再无 destroy 授权路径（Segment 帧永久占用）；原规范未记录 | §2.2 显式登记为**非目标**：不建模引用计数/可达性自动回收；§4.5 补充边界语义，说明这不是授权漏洞，实现层若引入自动回收须作为细化层资源域语义单独证明 | Recorded（边界，非缺陷） |
| 4 | ISSUE-S0X2-001 | `gpu_memseg_create` 在 §19 中是可调用操作，但不在 SPEC-054 的 26 项、也不在 SPEC-134 的 8 项增量中；原语义仅“类似于 CPU Segment” | §8.6 补全前置条件、成功动作、错误清单与 `InvalidArch` 就地定义；`InvalidArch` 加入 §6.2.1 新错误标识清单 | Fixed（文本） |
| 5 | ISSUE-AUDIT-001 | `ns_create`、`node_create`、`dev_ioport_create`、`dev_dma_create`、`dev_irq_create`（另含 `dev_mmio_create` 的 Cap 铸造事件）未在成功动作中写对象创建/Capability 铸造事件，与 W13 不一致 | 各操作成功动作补“追加对象创建/帧分配与 Capability 铸造 Audit 事件”步骤 | Fixed（文本） |

## 2. 关键取舍

- **对象自动回收**：不引入引用计数或 GC。理由：会改变授权与审计义务、需要新增可达性定义和证明义务，超出阶段 0 的语义基线；§2.2 已将资源可用性列为非目标。以显式边界登记，而非静默保留空洞。
- **设备资源唯一性**：不做“平台枚举即保证唯一”的隐含假设。POSIR 操作必须自行保持 W16.1，因此把唯一性检查放入创建操作的前置条件；平台枚举只负责提供候选值。
- **WaitObject stale cap**：选择“允许持有 stale cap 并返回 `TargetDead`”，而不是删除错误清单；这样保留了创建者清理/查询失败原因的能力，与 `ns_unbind`/`exc_unsubscribe` 的既有模式一致。

## 3. 对退出条件的影响

- **S0-X1**：新发现均已在跟踪文档登记并修订，字面条件恢复为“已知空洞均已记录且关闭”；但本轮复核与修订仍是 harness 内产出，**S0-X1 保持未勾选**，等待下一轮范畴外复核。
- **S0-X2**：操作面计数由 34 修正为 **35**（+ `gpu_memseg_create`）；`dev_ioport_create`/`dev_irq_create` 由“无失败分支”改判为显式错误清单；WaitObject 四操作的 `TargetDead` 恢复可达。S0-X2 的规范文本证据已按修正后的操作面补齐。
- 未触碰任何 Rust/Lean 代码，既有实现与 QEMU 证据不变。

## 4. 复验

| 命令 | 结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p cosir --quiet` | 146 + 42 + 3 passed |
| `lake build`（`lean/`） | 8 jobs 成功 |
| `git status` | 本切片改动限于规范、跟踪文档、`docs/README.md`、SPEC-134 追加与新增整改报告。工作区另存在与本切片无关的未提交代码改动（含 EXO-3M 注释），未纳入、未改动 |

以上复验只证明既有模型/形式化工件未回归，不证明新增规范条文已被实现或机器验证。

## 5. 证据边界

本记录是规范文本整改与文档登记；不构成 `MODEL-TEST`、`REFINEMENT-PROOF`、QEMU 证据或组织级独立性审查。WaitObject/`memseg_protect`/`gpu_memseg_create` 等新增条文在 cosir/Lean/内核中仍无对应实现（沿用 SPEC-134 §5 的登记）。
