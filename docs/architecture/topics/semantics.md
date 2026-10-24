---
id: topic-semantics
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-24
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
