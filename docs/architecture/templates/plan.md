---
id: template-plan
kind: template
authority: template
status: template
lifecycle: current
updated: 2026-10-24
---

# 执行计划模板

只在复杂任务需要拆分依赖时复制到 plans/日期-主题.md；替换 id、kind=plan、authority=execution-plan、status=proposed、lifecycle=current。计划不产生运行证据，不要求加载外部工作流或复述仓库规则。

## 目标、范围与基线

写明用户目标、基线提交、上位契约、相关问题、前序失败及排除项。不复制当前阶段状态表。

## 工作项与依赖

| 步骤 | 依赖 | 负责路径/行为 | 验收输入与预期观察 |
|---|---|---|---|
| 填写 | 填写 | 填写 | 预期而非已通过 |

## 验证与停止条件

列最小必要命令、失败时处理方式和影响边界。测试通过不自动签署阶段。

## 收口与生命周期

实施后链接真实报告和仍未解决项。完成后可转 historical；仅属重复一次性操作且无独立依据时，按治理契约登记后删除。未来排序需修订时更新该计划或显式指向后继，不在旧计划内滚动维护实现事实。
