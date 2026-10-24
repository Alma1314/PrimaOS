---
id: template-decision
kind: template
authority: template
status: template
lifecycle: current
updated: 2026-10-24
---

# 决策记录模板

按性质放入 specs/ 或 reports/，不建立与现有阶段决策日志平行的批准系统。

复制后替换 id/kind/authority/status；设计或活跃决策使用 lifecycle=current，带版本的证据报告通常使用 historical。正文必须给适用范围与重审触发；退役时遵守[治理契约](../../superpowers/specs/2026-10-23-documentation-system-design.md)。

## 决策身份与状态

填写 D/ISSUE/BLK 或独立唯一 ID、日期、提案/工作决定/已确认/被取代；没有批准证据时保持待确认。链接阶段跟踪中的决策入口。

## 触发问题与约束

写明当前失败或架构问题、上位语义、受影响调用方、必要证据与约束。

## 选项与依据

| 选项 | 收益 | 代价/风险 | 证据 |
|---|---|---|---|
| 填写 | 填写 | 填写 | 链接原始观察 |

## 选择与后果

说明采取何种选择、授权/确认来源、实现影响、兼容偏差、保留的限制与回滚/重审触发条件。

## 实施与验收

链接设计、代码、测试、报告、profile 与阶段条件。工作决定不得被自动解释为冻结规范或组织级签署。
