---
id: architecture-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-24
---

# 架构、开发与检索手册

返回[文档中心](../README.md)。本目录是解释层；抽象语义、研究架构和阶段状态分别由根权威文档负责。

| 材料 | 用途 |
|---|---|
| [首次阅读](getting-started.md) | 按角色选择最小充分阅读集 |
| [AI检索协议](ai-retrieval.md) | 有界检索、上下文包、主张溯源与交接 |
| [系统/代码地图](system-map.md) | crate职责、启动与调用路径、改动影响 |
| [主题地图](topics/README.md) | 十个子系统的契约/证据/代码/测试入口 |
| [术语表](glossary.md) | 中英文别名与容易冲突的编号 |
| [许可证](licensing.md) | GPLv3 范围、MPL-2.0 例外与第三方归属 |
| [接口与版本](interfaces.md) | 抽象操作、原生号、C适配与多个版本域 |
| [证据规则](evidence.md) | host/Lean/QEMU/差分/签署的范围 |
| [开发与验证](development.md) | 依赖、命令、构建产物与故障定位 |
| [文档维护](documentation-maintenance.md) | 放置、更新事务、生成与验收 |
| [计划模板](templates/plan.md) | 复杂任务的目标/依赖/预期验收/收口 |
| [设计模板](templates/spec.md) | 参数/权限/迁移/失败/验证契约 |
| [报告模板](templates/report.md) | 版本/输入/命令/结果/反例/后继 |
| [决策模板](templates/decision.md) | 工作决定/批准状态/备选/后果 |
| [全量目录](catalog.md) | 自动生成的源文档索引 |

机器接口为[topics.json](topics.json)与[catalog.json](../catalog.json)。维护规则见[文档系统契约](../superpowers/specs/2026-10-23-documentation-system-design.md)。
