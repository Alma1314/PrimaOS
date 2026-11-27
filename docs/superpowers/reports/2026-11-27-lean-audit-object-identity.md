---
id: report-lean-audit-object-identity
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: historical
updated: 2026-11-27
---

# Lean 审计对象类型丢失修复

## 结论与范围

修复 `Formal.Audit` / `Formal.AuditQuery` 把带类型对象引用降为裸自然数的错误：
同编号的 Segment 与 Namespace 原先会共享存活位并混入彼此的创建/销毁历史。
事件参数、存活表及查询参数现在全程使用 `Formal.Object`，查询直接匹配 `cap.obj`。
新增五条通用定理和七个回归锚；原 Audit/AuditQuery 定理全部重新构建。

本报告是 Lean 参考语义修复及证明证据，不是 Rust 源码细化、真实内核修复或阶段签署。
Lean 模型的对象相关参数类型有意从 `Nat` 改为 `Object`，删除丢失类型的 `objectId`；
不改机器 ABI、Rust API 或已冻结 profile。

## 基线与契约

- Git 基线：`ce33f03`；保留同一工作区前序撤销证明与工具恢复改动。
- 契约：[COSIR §3](../../../COSIR%20原语规范.md#3-基础域与记号)规定对象引用带类型，
  各类 ID 不可互相解释；§13.2 查询只返回目标对象的创建/销毁事件。
- 前序：[Audit 日志模型](2026-11-25-lean-audit.md)、[AuditQuery 查询模型](2026-11-25-lean-audit-query.md)。
- 执行背景：[BLK-001 设计](../specs/2026-08-18-blk001-lean-formal-model-design.md)、[历史计划](../plans/2026-08-18-blk001-lean-formal-model.md)。
- 映照：[lean/README.md](../../../lean/README.md)。Rust `cosir/src/state.rs::AuditKind::ObjectCreated`
  已用 `segment` 标志区分类型；`cosir/src/abstract_ops.rs` 的 bootstrap 同时创建
  `Segment(1)` 与 `Namespace(1)`，所以同编号情形不是不可达假设。

## 发现、反例与修复

修复前 `objectId (.segment 1) = objectId (.namespace 1)` 可由 `decide` 证明；
`AuditKind.objectCreated/objectDestroyed` 与 `State.objects` 都只接受 `Nat`。
对含两个裸 ID=1 生命周期事件的日志，分别查询上述两种对象均返回长度 2，
事件载荷已无法恢复其来源类型。这与 §3/§13.2 冲突；构建通过不能排除这种语义错误。

- `AuditKind.objectCreated/objectDestroyed` 携带完整 `Object`；`State.objects`、
  `State.createBit` 及对象创建/销毁操作使用同一个带类型键。
- `objectEvents` 使用完整 `Object` 过滤，并复用 `Log.eventAt?` 保留 EventId、时间、actor、kind。
- `auditedObjectCreate_other` / `auditedObjectDestroy_other` 证明不同对象的存活位不受影响。
- `objectEvents_sound` 证明返回项属于目标对象；`objectEvents_mem_iff` 给出匹配事件与
  原日志 `eventAt?` 的双向对应，保证匹配事件不丢失、信封不变。
- `audit_ok_object_mem` 把隔离性质连接到成功的 `audit` 返回值。
- `Checks/AuditQuery.lean` 的七个回归覆盖：同编号异类、同类异编号、无匹配对象、
  无关事件排除、跨类型销毁隔离，以及 bootstrap 两条根 Cap 在目标已销毁时的查询。
  混合日志中 Segment(1) 只返回 EventId `[10,13]`，Namespace(1) 只返回 `[11,12]`。

同时修正 AuditQuery 模块注释中“父链完备性未形式化”的旧描述；既有完备性证明仍以
`WellFormed` 为前提，没有借本修复扩大它的适用状态。

## 命令与结果

使用固定 `leanprover/lean4:v4.35.0`。原始输出留在 `/tmp/lean-audit-identity-*.log`
与 ignored `target/ci/formal/`，不提交完整日志或附件。

| 命令 | 结果 | 范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS，exit 0，21 jobs | 全部主模型 |
| 全部 `Checks/*.lean` 逐文件运行同一版本的 `lake env lean` | PASS，11 文件，exit 0 | 全部检查文件与七个新回归 |
| `Checks/Audit.lean` / `Checks/AuditQuery.lean` 公理集合与源码命名定理集合比对 | PASS，21 / 23 条；仅 `propext`/`Quot.sound` 或无公理 | 覆盖全部命名定理，无 sorry/native 依赖 |
| `python3 scripts/ci.py formal` | PASS，exit 0；含语义 mutation | 离线主模型、句柄表、独立 Rust 句柄源码细化与新鲜度、公理审计 |
| `cargo test --workspace` / `cargo xtask lint` | PASS，exit 0 | Rust host 回归与格式/Clippy/裸机构建/边界检查 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base ce33f03` | PASS，exit 0 | 文档治理 |

## 限制与阶段影响

查询结果与 `Container.WorldOk` / `Subscription.W15` / CDT `WellFormed` 的联合不变量
仍未证明；不覆盖完整事件载荷、TaskCreated 等其它事件类与对象历史的映射、Device
对象（`Formal.Object` 尚无该变体）、时钟、并发、跨节点或审计模块的 Rust 细化。
独立 formal 中的源码细化仍仅覆盖既有两个 Rust 句柄方法，不能外推到本次审计修复。
没有 Rust/启动路径改动，未运行 QEMU；本轮不重复前序工具下载。

阶段跟踪仅增加本次范围化证据，不改阶段、退出勾选、profile 或 ABI；
`ISSUE-REVOKE-CONTAINER-001` 仍开放。本报告只在对象类型隔离范围补强/纠正 11-25
两个报告；原报告的历史执行结果和其它未覆盖项保留。
