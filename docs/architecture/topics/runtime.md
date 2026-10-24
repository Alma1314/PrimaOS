---
id: topic-runtime
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-24
---

# 原生运行时、ring3 服务与用户态 I/O

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

服务发现/协商在哪层？内核怎样摆脱具体服务名？共享 Segment 和受控 I/O 的边界如何验收？

检索别名：prima-runtime, servicemgr, init, service, protocol, EXO, driver, ioport。

## 权威章节与阅读顺序

白皮书§4、§8、§11–12；原语规范§8.1、§9、§15。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

runtime 原型 → 独立服务/管理器/协议切片 → SPEC-130 原生应用与故障隔离收口 → SPEC-149 退出复核。EXO-2D 与 EXO-3M 分别记录 boot manifest 和服务见证从内核退役的后续证据。

## 契约、计划与证据入口

- [原生握手与分片事务契约](../../superpowers/specs/2026-10-24-native-runtime-service-design.md)
- [运行时/服务后继验证](../../superpowers/reports/2026-10-24-native-runtime-service-report.md)

- [Prima OS.md](../../../Prima%20OS.md)
- [2026-09-25-stage4-runtime-api-design.md](../../superpowers/specs/2026-09-25-stage4-runtime-api-design.md)
- [2026-10-13-stage4-ring3-service-manager-design.md](../../superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md)
- [2026-10-15-exokernel-alignment-design.md](../../superpowers/specs/2026-10-15-exokernel-alignment-design.md)
- [2026-10-16-stage4-closure-report.md](../../superpowers/reports/2026-10-16-stage4-closure-report.md)
- [2026-10-17-stage4-exit-review-spec149.md](../../superpowers/reports/2026-10-17-stage4-exit-review-spec149.md)
- [2026-10-18-stage1-4-implementation-review.md](../../superpowers/reports/2026-10-18-stage1-4-implementation-review.md)
- [2026-10-16-exo2d-manifest-boot-abi-report.md](../../superpowers/reports/2026-10-16-exo2d-manifest-boot-abi-report.md)
- [2026-10-17-exo3m-kernel-service-witness-retirement-report.md](../../superpowers/reports/2026-10-17-exo3m-kernel-service-witness-retirement-report.md)

## 源码定位

- [staging.rs](../../../prima-user/src/staging.rs) — 生产使用且可 host 验证的路径状态机

- [lib.rs](../../../prima-runtime/src/lib.rs)
- [transport.rs](../../../prima-runtime/src/transport.rs)
- [protocol.rs](../../../prima-runtime/src/protocol.rs)
- [servicemgr.rs](../../../prima-user/src/servicemgr.rs)
- [service.rs](../../../prima-user/src/service.rs)
- [log.rs](../../../prima-user/src/log.rs)
- [ns.rs](../../../prima-user/src/ns.rs)
- [store.rs](../../../prima-user/src/store.rs)
- [driver.rs](../../../prima-user/src/driver.rs)
- [tty.rs](../../../prima-user/src/tty.rs)

## 测试与观察入口

- [protocol.rs](../../../xtask/src/protocol.rs)
- [main.rs](../../../prima-user/src/main.rs)
- [beta.rs](../../../prima-user/src/beta.rs)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

原生 API/版本规则与稳定外部 ABI 不同。用户态驱动的受控 COM1/IO port 子集不能当成通用设备栈、NIC 或 IOMMU 已完成。测试见证属于 xtask/用户态协议，不应把服务语义重新耦合进内核。Hello 先校验头/offer 一致性；路径分片仅接受顺序组装，错误后必须重新开始，双路径确认与消费规则见后继契约。
