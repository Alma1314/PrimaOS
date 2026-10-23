---
id: topic-namespace-time-audit
kind: topic
authority: explanatory
status: maintained
updated: 2026-10-23
---

# Namespace、Timer 与 Audit

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

挂载图如何终止和拒绝环？Timer 立即触发/取消/恰一次通知怎么测试？审计查询是否修改状态？

检索别名：ns_bind, ns_mount, ns_resolve, timer, audit, mount, 命名, 审计。

## 权威章节与阅读顺序

原语规范§10–11、§13、W11–W13。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

stage3 namespace/timer/audit 设计与实现 → audit 恢复 → SPEC-148 签署 → 2026-10-18 mount DFS、销毁审计与派生链容量整改。旧报告中的计数保留为历史，不当成当前常量。

## 契约、计划与证据入口

- [COSIR 原语规范.md](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)
- [2026-09-25-stage3-namespace-design.md](../../superpowers/specs/2026-09-25-stage3-namespace-design.md)
- [2026-09-25-stage3-timer-design.md](../../superpowers/specs/2026-09-25-stage3-timer-design.md)
- [2026-09-25-stage3-audit-trace-design.md](../../superpowers/specs/2026-09-25-stage3-audit-trace-design.md)
- [2026-09-25-stage3-namespace-report.md](../../superpowers/reports/2026-09-25-stage3-namespace-report.md)
- [2026-09-25-stage3-timer-report.md](../../superpowers/reports/2026-09-25-stage3-timer-report.md)
- [2026-10-10-stage3-audit-trace-restore-report.md](../../superpowers/reports/2026-10-10-stage3-audit-trace-restore-report.md)
- [2026-10-17-stage3-exit-review-spec148.md](../../superpowers/reports/2026-10-17-stage3-exit-review-spec148.md)
- [2026-10-18-stage3-review-remediation.md](../../superpowers/reports/2026-10-18-stage3-review-remediation.md)

## 源码定位

- [namespace.rs](../../../kernel/src/namespace.rs)
- [bootstrap.rs](../../../kernel/src/bootstrap.rs)
- [preempt.rs](../../../kernel/src/preempt.rs)
- [namespace.rs](../../../prima-runtime/src/namespace.rs)
- [transport.rs](../../../prima-runtime/src/transport.rs)

## 测试与观察入口

- [protocol.rs](../../../xtask/src/protocol.rs)
- [namespace.rs](../../../kernel/src/namespace.rs)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

Namespace 是能力命名，不直接提供 POSIX 文件语义。抽象 Timer now 与阶段5 POSIX PIT ticks 是不同时间通路。有限 QEMU 见证不等于模型中的所有生命周期已被机器证明。
