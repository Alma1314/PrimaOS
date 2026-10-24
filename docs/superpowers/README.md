---
id: artifacts-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-24
---

# 设计、执行与证据档案

本目录按工件类型保留研究过程和可追溯证据。统一入口为[文档中心](../README.md)，按子系统阅读请用[主题地图](../architecture/topics/README.md)；当前进度只查[阶段跟踪](../../Prima%20OS%20实现阶段.md)。

## 工件类型

| 目录 | 回答什么 | 不能据此推断 |
|---|---|---|
| [specs](specs/README.md) | 预期行为、接口、设计、冻结或候选契约 | 设计已实现，候选已经批准 |
| [plans](plans/README.md) | 当时的分步计划、依赖、验收方式 | 任务已经执行，旧基线仍是当前状态 |
| [reports](reports/README.md) | 实现、审查、失败、整改、实跑证据 | 报告范围外的安全/兼容/证明结论 |
| [compat](compat/README.md) | API 和符号的兼容状态及偏差 | 完整 POSIX/Linux 兼容 |

## 按问题追溯

1. 先从阶段跟踪确认相关条件、活跃阻塞和证据指针。
2. 从主题包定位上位规范、切片设计和执行计划。
3. 连续读取发现 → 修复 → 复核报告；后继只在明确范围内补强/取代前序。
4. 对照源码/探针；需要实证时按开发手册重跑并记录版本与范围。

阶段0–4的签署入口、签署后整改，以及阶段5出口复核均集中在阶段跟踪；本页不复制第二份动态清单。保留材料可通过[全量目录](../architecture/catalog.md)与离线全文检索找到；已删除路径通过[生命周期台账](../lifecycle.json)查询原因、替代入口与 Git 恢复版本。

## 命名与维护

新规格、计划、报告采用 `YYYY-MM-DD-topic.md`；目录 README 按主题分组，表中日期是工件日期。新文档使用明确 id/类型/权威角色/状态/维护日期，历史文档不强行推断批准/完成状态；生命周期与退役规则由治理契约统一规定。

正文使用相对 Markdown 链接。报告保留实际失败、反例、环境与未覆盖项；不能为了与新实现一致而改写旧实验结论。详细规则见[文档契约](specs/2026-10-23-documentation-system-design.md)、[维护手册](../architecture/documentation-maintenance.md)与[证据规则](../architecture/evidence.md)。

```sh
python3 scripts/docs.py self-test
python3 scripts/docs.py build
python3 scripts/docs.py check
```
