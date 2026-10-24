---
id: topic-boot
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-24
---

# 启动、硬件与可信边界

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

谁建立初始 Authority？init 如何获得 boot info？unsafe 应放在哪里？启动见证与帧守恒在哪里检查？

检索别名：bootstrap, framekernel, GDT, IDT, trap, CR3, EXO, TCB。

## 权威章节与阅读顺序

原语规范§14、§18–20；白皮书§12；阶段1与 stage2/framekernel 证据。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

stage1 boot 设计/计划 → SPEC-146 退出复核 → SPEC-150/151 后续整改。Framekernel skeleton/entry inversion → closure；EXO-2D 是后续通用 manifest 与 boot ABI 证据。

## 契约、计划与证据入口

- [COSIR 原语规范.md](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)
- [2026-08-04-stage1-boot-slice-design.md](../../superpowers/specs/2026-08-04-stage1-boot-slice-design.md)
- [2026-08-04-stage1-boot-slice.md](../../superpowers/plans/2026-08-04-stage1-boot-slice.md)
- [2026-09-25-framekernel-skeleton-design.md](../../superpowers/specs/2026-09-25-framekernel-skeleton-design.md)
- [2026-09-25-framekernel-entry-inversion-design.md](../../superpowers/specs/2026-09-25-framekernel-entry-inversion-design.md)
- [2026-09-25-framekernel-closure-report.md](../../superpowers/reports/2026-09-25-framekernel-closure-report.md)
- [2026-10-17-stage1-exit-review-spec146.md](../../superpowers/reports/2026-10-17-stage1-exit-review-spec146.md)
- [2026-10-18-stage1-open-issues-spec151.md](../../superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)
- [2026-10-16-exo2d-manifest-boot-abi-report.md](../../superpowers/reports/2026-10-16-exo2d-manifest-boot-abi-report.md)

OSTD 后继补强：[Asterinas 移植契约](../../superpowers/specs/2026-10-24-asterinas-reuse-design.md)与
[验证报告](../../superpowers/reports/2026-10-24-asterinas-reuse-report.md)限定对齐工具、StaticCell IRQ-safe 锁与调度跳转；
来源与许可见 [来源与许可](../asterinas-provenance.md)。候选组件不等于已迁入。

## 源码定位

- [main.rs](../../../kernel/src/main.rs)
- [bootstrap.rs](../../../kernel/src/bootstrap.rs)
- [gdt.rs](../../../kernel/src/gdt.rs)
- [idt.rs](../../../kernel/src/idt.rs)
- [arch.rs](../../../prima-ostd/src/arch.rs)
- [entry.rs](../../../prima-ostd/src/entry.rs)
- [lib.rs](../../../prima-initgen/src/lib.rs)
- [lib.rs](../../../prima-abi/src/lib.rs)
- [image.rs](../../../xtask/src/image.rs)

- [align.rs](../../../prima-ostd/src/align.rs)
- [sync.rs](../../../prima-ostd/src/sync.rs)
- [lock.rs](../../../prima-ostd/src/sync/lock.rs)

## 测试与观察入口

- [protocol.rs](../../../xtask/src/protocol.rs)
- [qemu.rs](../../../xtask/src/qemu.rs)
- [lint.rs](../../../xtask/src/lint.rs)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

kernel/src 必须零 unsafe，必要特权操作放在 prima-ostd 并说明不变量。Authority 退出条件已接受的 image/vspace 细化分叉不等于完整§14 细化证明。BIOS 验收不能外推为 UEFI 执行已验收。
