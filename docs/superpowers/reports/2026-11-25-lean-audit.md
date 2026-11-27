---
id: report-lean-audit
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-27
---

# Lean `Formal.Audit`：§13.1 事件信封与 W13 日志核心最小模型

## 结论与范围

本报告记录 COSIR §13.1（事件模型）与 W13（Audit 完整性）的 Lean 参考语义：新增
`lean/Formal/Audit.lean` 与 `lean/Checks/Audit.lean`。建模事件信封
`{id, at, actor, kind}`、追加式日志、由日志位置推导的严格递增 EventId、
Capability 位置历史、`created_at`、Task 状态迁移与对象创建/销毁事件，以及
move/drop/mint 的一步原子追加。19 条定理通过 `lake build`；2 条无公理，其余仅
`propext`（`ids_strict` 另含 `Quot.sound`）。

本切片是日志核心与受审计操作的最小模型，不是逐类事件载荷完整性证明、`audit`
查询实现或 Rust 细化证明。

## 基线与输入

- 基线：`5583cfc`（另加 `fd844ac`、`01dc0fe`、`5cc574b`、`d212cb1`、`1fed6da`、
  `05c80a5`、`e45817d`、`fc2d026`、`a6a5e8a`）。
- 契约：COSIR §13.1（事件模型）、W13（Audit 完整性）、§4.6（位置变化事件）、
  §4.5（`CapDropped`）。
- 复用：`Formal.Container` 的 `World`、`Formal.Subscription` 的 `SubWorld`、
  `Formal.TokenLocation` 的 `replace`。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 Audit 节。

## 实现

- `Actor`、`TaskStatus`、`AuditKind`：事件所需的最小枚举；`Entry {actor, at, kind}`
  存在日志中，`Event {id, at, actor, kind}` 是查询视图。
- `Log {first, entries}`：`EventId = first + 索引`，`next_event = first + length`；
  ID 严格递增由表示保证（`ids_strict`）。
- `Log.append`：追加若干事件；`append_prefix` 证明旧日志为新日志前缀（只追加），
  `append_event_old` 证明旧事件信封不变，`append_nextEvent` 给出计数器推进。
- `capHistory`：按 CapId 过滤 `CapMoved`/`CapDropped`，每项为
  `(旧位置, 新位置)`；删除项 `(some old, none)`。
- `State {world, log, created, objects, now}`：受审计世界 + 日志 + 创建记录 +
  对象活动位 + 时钟。
- `auditedMove`/`auditedDrop`：从当前位置读取旧位置，一步更新世界并追加
  `CapMoved`/`CapDropped`；`auditedMint` 追加 `CapMinted` 并把 `created_at`
  指向该事件 ID；`auditedTaskState` 追加 `TaskStateChanged`；对象创建/销毁追加
  `ObjectCreated`/`ObjectDestroyed` 并维护活动位。

已证性质（19 条）：

- 日志：`append_entries`、`append_prefix`、`append_nextEvent`、`append_event_old`、
  `ids_strict`。
- 位置历史：`capHistory_append_moved`、`capHistory_append_dropped`、
  `capHistory_append_moved_other`。
- 受审计操作：`auditedMove_spec`、`auditedDrop_spec`、`auditedMint_created`、
  `auditedMint_other_created`、`auditedMint_event`、`auditedTaskState_event`、
  `auditedTaskState_keeps_world`、`auditedObjectCreate_event`、
  `auditedObjectCreate_live`、`auditedObjectDestroy_event`、
  `auditedObjectDestroy_dead`。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (20 jobs)`，exit 0 | Lean 模型构建 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Audit.lean`（`lean/`） | PASS；19 条定理，2 条无公理，其余仅 `propext` | 公理审计 |

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- **ID 分配**：`EventId` 由日志位置推导，因此 W13 的严格递增是表示性质，不是独立
  分配器（`next_event` 竞争/回绕）的实现证明。
- **载荷完整性**：事件类为最小子集；未逐类证明 §13.1 要求的授权 Cap、被检权限、
  目标对象、路径 Mount 链、`MessageSent` 的 payload 长度/`transport_msg` 等字段。
- **查询未建模**：§13.2 `audit` 的 parent 链、按 Cap 的位置历史拼接、对象
  创建/销毁查询、纯查询性（无状态变化）不建模。
- 不覆盖：时钟推进与真实时间（所有事件用调用者的 `now`）、拒绝事件日志、事件
  顺序与订阅/等待等其它操作的交错、并发、内容摘要、跨节点事件关联或 Rust 细化。
- `created`/`objects` 与 `Container.WorldOk`、`Subscription.W15`、CDT
  `WellFormed` 为独立不变量，未证明联合保持。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile 或 ABI；只把 §13.1/W13 的日志核心从
“未覆盖”移入“最小模型”，载荷完整性与 `audit` 查询边界保留。

## 后继关系

- 契约：[COSIR §13.1/W13](../../../COSIR%20原语规范.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- 互补：[Subscription §12.4](2026-11-25-lean-subscription.md)、[Move §4.6](2026-11-25-lean-move.md)。

2026-11-27 后继：[审计对象身份修复](2026-11-27-lean-audit-object-identity.md)纠正本模型
丢失对象类型的问题；本报告的定理计数/运行观察属于 11-25 基线，现行类型和新增隔离证明
见后继报告与 Lean 映照。其它边界保留。
