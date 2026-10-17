# 阶段 3 实现复核与整改（2026-10-18）

**基线**：`1b8048b`（阶段 2 IPC 整改后、`master` 工作区干净）。
**范围**：S3-D1–D5 / X1–X4 的 Namespace、Timer、Audit 实现及 SPEC-148 证据链。
**结论**：发现并修复 4 项实现缺陷；阶段 3 原签署不替代本轮复核，更新后的 host 与 QEMU 有界见证见下文。

## 发现与修复

| 编号 | 反例 / 影响 | 修复与验证 |
|---|---|---|
| S3-R1 Mount 环漏判 | `namespace_reaches` 的 DFS 栈按 8 个 Namespace 分配，但最多有 16 条 Mount 边。A 的前 8 条边都指向 B 时，第 9 条 A→C 被静默丢弃；已有 C→D 时，D→A 可被错误接受，破坏 S3-X1。 | 栈按最大边数 `MAX_NS_ENTRIES` 分配；测试以 8 条重复边及后续 A→C→D 验证可达性和 `MountCycle` 拒绝。 |
| S3-R2 `ns_destroy` 审计缺项 | §11.8 要求被删除的每个 Bound/Mount 条目同时写 `CapDropped` 与 `NsUnbound`/`NsUnmounted`；实现只写前者，使位置历史不完整。 | 销毁前预留每条 2 个审计槽，收集完整条目并逐条写两种事件。测试覆盖 Bound 与 Mount；QEMU 的 `namespace_ready.unbinds` 随合法记录从 1 变为 2。 |
| S3-R3 不同对象类型串账 | `ObjectCreated/ObjectDestroyed` 仅用 `segment: bool` 标识类型。Timer(1) 销毁会被 `audit(Namespace(1))` 当成其销毁；Device 创建也会被误计。Task/ExcSource 的专用生命周期事件未纳入摘要。 | 事件携带准确 `ObjectKind`；Audit 按 ID+种类匹配，并纳入 Task/ExcSource/Timer 的专用生命周期事件。测试让同 ID 的 Timer 销毁与 Device 创建共存，Namespace 与 Task 的摘要仍准确。 |
| S3-R4 历史派生链被截断 | 追加式 Cap 历史允许父 Cap 丢弃后复用槽，合法祖先链可长于 144 个同时存活槽；`audit_trace` 却以槽数为链长上界，返回 `InvalidCap`。 | 改用 512 条历史记录容量作为上界；测试连续派生并丢弃旧父项，验证 145 级合法链可查询。 |

## 证据与边界

- `cargo test --workspace`：通过，含上述回归测试。
- `cargo xtask lint`：格式、Clippy、bare-metal build、kernel 零 `unsafe` 通过。
- `cargo xtask test`：BIOS/UEFI 镜像、BIOS QEMU 两次、stage1 boot slice 通过。
- QEMU `namespace_ready` 保持 `evidence_ok=1`，现有 `unbinds=2`；新增销毁事件使 `state_audit_ready.used` 从 SPEC-151 基线 604 变为 606。Timer `exactly_once_ok=1`、Audit `evidence_ok=1` 保持。
- 本轮是内核实现、host 单测和 QEMU 的有限证据，不声称 Lean 细化证明或所有可达状态穷举。

## 结果

S3-X1 的活动 Mount 图环检测、S3-X3 的跨类型/长链审计、S3-X4 的 Namespace 销毁审计已按上述反例修正。S3-D3/D4/X2 的 Timer 路径经现有 host 与 QEMU 回归，未发现需修改的行为。
