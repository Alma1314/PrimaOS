---
id: template-spec
kind: template
authority: template
status: template
lifecycle: current
updated: 2026-10-24
---

# 设计/接口规格模板

复制到 specs/日期-主题.md 并替换元数据。本模板没有批准或实现效力。

复制后替换 id/kind/authority/status；设计或活跃决策使用 lifecycle=current，带版本的证据报告通常使用 historical。正文必须给适用范围与重审触发；退役时遵守[治理契约](../../superpowers/specs/2026-10-23-documentation-system-design.md)。

## 问题与范围

说明具体触发条件、预期观察、非目标与上位契约。链接相关原语章节、阶段条件和前序工件。

## 状态与决定

写明提案/冻结/待确认的具体状态、决策来源与未决项。候选号段不得写成已分配。

## 成功与失败契约

记录参数/类型/单位、前置授权、成功状态迁移、返回值、错误映射、失败原子性、阻塞/唤醒/超时、并发与生命周期。区分抽象语义与具体机器接口。

## 实现与跨层影响

列出相关 crate/文件/符号、调用链、容量与回收、不变量和 ABI/协议影响。

## 验证与反例

为每项行为指定 host/QEMU/Lean/差分证据，说明输入、观察、负例与通过标准；计划中的测试不得标 PASS。

## 兼容偏差与后继

记录偏差、限制、相关 ISSUE/BLK、影响的 profile/阶段条件、预计后继工件。
