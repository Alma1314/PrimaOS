---
id: topic-semantics
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-31
---

# 抽象语义与形式化

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

对象与状态如何定义？失败是否原子？哪些 CDT 性质已经在 Lean 证明，哪些仅有实例测试？

检索别名：POSIR, COSIR, CDT, Lean, 良构, refinement。

## 权威章节与阅读顺序

原语规范§1–6、§16–20；白皮书§3、§13–14。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

阶段0第三方复核未通过 → SPEC-143/144 整改 → SPEC-145 冻结基线文本复核。CDT 的 cosir parent 补齐、Lean 不变量保持性与森林性是不同的证据链，应连同 lean/README 的映照契约阅读。

## 契约、计划与证据入口

- [COSIR 原语规范.md](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)
- [Prima OS.md](../../../Prima%20OS.md)
- [README.md](../../../lean/README.md)
- [2026-08-18-blk001-lean-formal-model-design.md](../../superpowers/specs/2026-08-18-blk001-lean-formal-model-design.md)
- [2026-08-18-blk001-lean-formal-model.md](../../superpowers/plans/2026-08-18-blk001-lean-formal-model.md)
- [2026-10-13-cosir-cdt-parent-report.md](../../superpowers/reports/2026-10-13-cosir-cdt-parent-report.md)
- [2026-10-13-lean-cdt-invariant-report.md](../../superpowers/reports/2026-10-13-lean-cdt-invariant-report.md)
- [2026-10-13-lean-cdt-forest-report.md](../../superpowers/reports/2026-10-13-lean-cdt-forest-report.md)
- [2026-10-17-stage0-frozen-re-review-spec145.md](../../superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md)
- [2026-10-17-stage0-thirdparty-organizational-review-verdict.md](../../superpowers/reports/2026-10-17-stage0-thirdparty-organizational-review-verdict.md)

- [内核句柄与 Lean 补强设计](../../superpowers/specs/2026-10-24-kernel-handle-model-design.md)
- [实现、证明与回归报告](../../superpowers/reports/2026-10-24-kernel-handle-model-report.md)

## 源码定位

- [state.rs](../../../cosir/src/state.rs)
- [abstract_ops.rs](../../../cosir/src/abstract_ops.rs)
- [machine.rs](../../../cosir/src/machine.rs)
- [ops.rs](../../../cosir/src/ops.rs)
- [State.lean](../../../lean/Formal/State.lean)
- [Derive.lean](../../../lean/Formal/Derive.lean)
- [Forest.lean](../../../lean/Formal/Forest.lean)

- [KernelHandle.lean](../../../lean/Formal/KernelHandle.lean)

## 测试与观察入口

- [lean_backtrace.rs](../../../cosir/tests/lean_backtrace.rs)
- [Formal.lean](../../../lean/Formal.lean)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

SPEC-145 是规范文本范围。Lean 不覆盖完整 Task/vspace/Audit，cosir 也存在明确未建模项；CDT 定理不能推广为 Rust 内核细化证明。修改全局不变量必须检查所有受影响原语，不只改一个成功路径。

内核句柄的有界 generation/永久退役模型独立于 CDT：`freed_handle_never_live` 覆盖单槽任意后续迁移序列；不证明完整 Rust 内核、整表分配、权限或硬件。范围以 [Lean 映照契约](../../../lean/README.md)为准。


2026-10-31 [有限槽表后继](../../superpowers/reports/2026-10-31-kernel-handle-table-proof.md)将单槽结论组合为任意整表序列的无复活/退役保持，并证明跨槽隔离与良构保持。新模块为 [KernelHandleTable.lean](../../../lean/Formal/KernelHandleTable.lean)，[公理审计](../../../lean/Checks/KernelHandleTable.lean)和[真实表的有界回归](../../../kernel/src/handle_table_tests.rs)可独立重跑。采用任意可用槽的过近似，仍未证明最低槽/预留区间策略或 Rust 源码 refinement。


[同日源码细化后继](../../superpowers/reports/2026-10-31-rust-handle-source-refinement.md)以 Charon/Aeneas 从生产 Rust 提取句柄构造/后继方法；[细化定理](../../../lean/refinement/Refinement/Handle.lean)通过普适证明连接单槽模型，[重放脚本](../../../scripts/check_handle_refinement.py)拒绝陈旧/手改生成物并提供真实源码 mutation。只覆盖所列两个方法与 generation 核心，尚非整张 Rust 槽表或完整内核细化。


2026-11-25 [TLB 缓存最小模型](../../superpowers/reports/2026-11-25-lean-tlb-refine.md)把 cosir `tlb_refine` 的命中/未命中、单地址失效、全量刷新与任务移除纳入 [Formal.Tlb](../../../lean/Formal/Tlb.lean)；[公理审计](../../../lean/Checks/Tlb.lean)只显示 `propext`/`Quot.sound`。抽象页表与守卫取代四级页表、帧所有权和真实 CR3，不覆盖 SMP、TLB shootdown 或 Rust 源码细化。

2026-11-25 [Memseg 双视图最小模型](../../superpowers/reports/2026-11-25-lean-memseg-refine.md)把 W7 表/视图一致、W6 帧 owner 转移、权限收窄和失败原子性纳入 [Formal.Memseg](../../../lean/Formal/Memseg.lean)；[公理审计](../../../lean/Checks/Memseg.lean)只显示 `propext`/`Quot.sound`。四级树、多页原子映射与 `StillMapped` 枚举被抽象，不构成 7-B/7-C/7-H 证据。

2026-11-25 [Cow 组合最小模型](../../superpowers/reports/2026-11-25-lean-cow-refine.md)把 COW 建立、写故障、私有化、单运行任务与销毁纳入 [Formal.Cow](../../../lean/Formal/Cow.lean)；[公理审计](../../../lean/Checks/Cow.lean)只显示 `propext`。capability 授权、异常源/恢复帧与 TLB 失效为抽象边界，不构成 7-B/7-C/7-D/7-H 证据。

2026-11-25 [Revoke 撤销域最小模型](../../superpowers/reports/2026-11-25-lean-revoke.md)把 Prima OS §6.2 的子树失效、HeldBy 删除、非 HeldBy 保留与原子/幂等纳入可执行 [Formal.Revoke](../../../lean/Formal/Revoke.lean)（有界 `descB`，`WellFormed` 下与 `Desc` 等价）；[公理审计](../../../lean/Checks/Revoke.lean)显示 4 条无公理、其余仅 `propext`/`Quot.sound`，无 `Classical.choice`。不构成 7-D 证据。

2026-11-25 [cap_drop 最小模型](../../superpowers/reports/2026-11-25-lean-drop.md)把 §4.5 的单位置删除、不传播、记录/位置保持与 structural 守卫纳入可执行 [Formal.Drop](../../../lean/Formal/Drop.lean)；[公理审计](../../../lean/Checks/Drop.lean)仅 `propext`/`Quot.sound`。订阅终止与审计事件仍不覆盖。

2026-11-25 [Move 位置迁移最小模型](../../superpowers/reports/2026-11-25-lean-move.md)把 §4.6 的转移表、单位置原子更新、从/到与直投 `MsgId` 记录、删除到 `None`、跨节点不移动与 W3 表示层唯一纳入可执行 [Formal.Move](../../../lean/Formal/Move.lean)；[公理审计](../../../lean/Checks/Move.lean)显示 7 条无公理、其余仅 `propext`。W4 容器双向一致与审计事件仍不覆盖。

2026-11-25 [W4 容器一致性最小模型](../../superpowers/reports/2026-11-25-lean-container.md)把 `HeldBy` 的 Task 活动性、`InMessage` ↔ 消息 caps、`InNamespace` ↔ Namespace entries 的双向一致与 send/recv/bind/unbind 的 W3+W4 保持纳入可执行 [Formal.Container](../../../lean/Formal/Container.lean)；[公理审计](../../../lean/Checks/Container.lean)仅 `propext`/`Quot.sound`。列表级重复/路径唯一、生命周期与审计事件仍不覆盖。

2026-11-25 [§12.4/W15 订阅生命周期最小模型](../../superpowers/reports/2026-11-25-lean-subscription.md)在 [Formal.Subscription](../../../lean/Formal/Subscription.lean) 中建模订阅表、usable `ExcSubscribe` 见证、subscribe/unsubscribe、失去最后授权 Cap 的原子终止与源销毁；[公理审计](../../../lean/Checks/Subscription.lean)仅 `propext`/`Quot.sound`。关系模型不证明 lastness 检查器、活性或审计事件。

2026-11-25 [§13.1/W13 审计日志核心最小模型](../../superpowers/reports/2026-11-25-lean-audit.md)在 [Formal.Audit](../../../lean/Formal/Audit.lean) 中建模信封、追加式日志、严格递增 EventId、Capability 位置历史、created_at 与受审计 move/drop/mint；[公理审计](../../../lean/Checks/Audit.lean)仅 `propext`/`Quot.sound`。载荷完整性与 audit 父链查询不覆盖。

2026-11-25 [§13.2 audit 查询最小模型](../../superpowers/reports/2026-11-25-lean-audit-query.md)在 [Formal.AuditQuery](../../../lean/Formal/AuditQuery.lean) 中建模 parent 链、created_at 列表、位置历史、对象事件与 InvalidCap/NotHeld 错误语义；[公理审计](../../../lean/Checks/AuditQuery.lean)仅 `propext`/`Quot.sound`。`WellFormed` 下燃料完备性已证；完整载荷不覆盖。
