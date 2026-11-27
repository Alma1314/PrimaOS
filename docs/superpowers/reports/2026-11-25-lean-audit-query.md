---
id: report-lean-audit-query
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-27
---

# Lean `Formal.AuditQuery`：§13.2 `audit(cap)` 查询最小模型

## 结论与范围

本报告记录 COSIR §13.2 `audit(cap)` 的 Lean 参考语义：新增
`lean/Formal/AuditQuery.lean` 与 `lean/Checks/AuditQuery.lean`。复用
`Formal.Audit` 的日志、`created` 与 `capHistory`，建模 root→cap 父链、链上
`created_at` 列表、位置历史、目标对象创建/销毁事件，以及 `InvalidCap`/`NotHeld`
错误语义与纯查询性。20 条定理通过 `lake build`，全部无 `sorryAx`，仅依赖
`propext`/`Quot.sound`；其中 `parentChain_complete` 在 `WellFormed` 下证明燃料
`c + 1` 未被截断。

本切片是查询结果与错误语义的最小模型；父链完备性以 `WellFormed`（W2）为前提，
完整事件载荷、跨节点 `origin` 与 Rust 细化不在范围。

## 基线与输入

- 基线：`5583cfc`（另加 `fd844ac`、`01dc0fe`、`5cc574b`、`d212cb1`、`1fed6da`、
  `05c80a5`、`e45817d`、`fc2d026`、`a6a5e8a`、`b685573`）。
- 契约：COSIR §13.2（audit 查询）、W13（Audit 完整性）、W3（记录存在）。
- 复用：`Formal.Audit` 的 `State`/`Log`/`capHistory`/`Log.objectEvents`。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 AuditQuery 节。

## 实现

- `parentChain fuel w c`：燃料有界的 parent 行走，只包含有 `CapRecord` 的 CapId；
  `auditChain` 反转得到 root→cap 顺序。
- `audit s caller capId`：返回 `Except AuditError AuditTrace`；`InvalidCap`（无
  CapRecord）、`NotHeld`（存在但不位于 `HeldBy(caller)`）、否则 `ok`。
- `AuditTrace {chain, creations, positions, objectHistory}`：父链、链上每个
  Capability 的 `created_at`、`capHistory`、目标对象创建/销毁事件。
- 查询是纯函数：不返回状态，成功不要求目标对象存活。

已证性质（20 条）：

- 父链：`parentChain_head`、`parentChain_sound`、`parentChain_length_le`、
  `auditChain_sound`、`auditChain_length_le`、`auditChain_last`。
- 完备性：`lastParentNone_getLast`、`lastParentNone_cons_of_tail`、
  `parentChain_nonempty`、`parentChain_succ_of_done`、`parentChain_done_add`、
  `parentChain_complete`、`auditChain_root_parent_none`。
- 错误/成功：`audit_invalid`、`audit_not_held`、`audit_not_held_other`、
  `audit_ok_held`。
- 结果：`audit_ok_fields`、`audit_dead_target_ok`、`audit_ok_creation_mem`。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (21 jobs)`，exit 0 | Lean 模型构建 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/AuditQuery.lean`（`lean/`） | PASS；20 条定理，仅 `propext`/`Quot.sound` | 公理审计 |

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- **父链完备性前提**：`parentChain_complete` 在 `WellFormed`（父 `<` 子）下证明
  燃料 `capId + 1` 足以走到无父根；非良构状态（如环）仍可能截断，模型不覆盖。
- 不覆盖：`origin`/跨节点代理根（v1 恒 `None`）、内容摘要、完整 §13.1 事件载荷
  （授权 Cap/权限/目标对象字段）、时钟推进、拒绝事件日志、并发或 Rust 细化。
- 查询结果与 `Container.WorldOk`、`Subscription.W15`、CDT `WellFormed` 为独立
  模型，未证明在联合不变量下的查询正确性。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile 或 ABI；把 §13.2 从“未覆盖”移入
“最小模型”，父链完备性与载荷边界保留。

## 后继关系

- 契约：[COSIR §13.2](../../../COSIR%20原语规范.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- 互补：[Audit 日志核心](2026-11-25-lean-audit.md)、[Move §4.6 位置](2026-11-25-lean-move.md)。

2026-11-27 后继：[审计对象身份修复](2026-11-27-lean-audit-object-identity.md)纠正本模型
丢失对象类型的问题；本报告的定理计数/运行观察属于 11-25 基线，现行类型和新增隔离证明
见后继报告与 Lean 映照。其它边界保留。
