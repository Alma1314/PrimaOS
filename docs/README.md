---
id: docs-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-24
---

# PrimaOS 文档中心

PrimaOS 是研究型操作系统：以对象能力机制承载原生运行时与可替换的 OS personality。文档按“问题 → 权威契约 → 实现 → 证据 → 限制”组织；有效证据链保留；冗余或废弃材料按生命周期规则清理。

**当前阶段与退出结论只查[实现阶段](../Prima%20OS%20实现阶段.md)**。本入口不复制进度快照，避免多个入口各自过期。

## 按任务进入

| 任务 | 第一入口 | 然后读取 |
|---|---|---|
| 第一次接触项目 | [入门路线](architecture/getting-started.md) | [系统与代码地图](architecture/system-map.md) → 白皮书 |
| AI 接手任务、压缩上下文 | [AI 检索协议](architecture/ai-retrieval.md) | 主题包 → 权威章节 → 最新证据 → 代码 |
| 判断实现进度与阻塞 | [实现阶段](../Prima%20OS%20实现阶段.md) | 指向的报告、兼容台账与决策门 |
| 修改某个子系统 | [主题地图](architecture/topics/README.md) | 同主题规范、设计、计划、实现与测试 |
| 查术语、缩写或编号 | [术语表](architecture/glossary.md) | [接口与编号边界](architecture/interfaces.md) |
| 构建、运行与排错 | [开发与验证手册](architecture/development.md) | 命令实现、报告中的运行环境 |
| 评估安全、兼容或证明主张 | [证据规则](architecture/evidence.md) | 具体报告范围、反例和未覆盖项 |
| 新写或维护文档 | [文档治理规范](superpowers/specs/2026-10-23-documentation-system-design.md) | [维护手册与模板](architecture/documentation-maintenance.md) |
| 精确查文件与章节 | [全量目录](architecture/catalog.md) | [机器目录 JSON](catalog.json)；本地全文检索 |

## 权威来源按问题分工

| 问题 | 权威来源 | 使用限制 |
|---|---|---|
| 研究目标、层次、阶段定义 | [Prima OS 白皮书](../Prima%20OS.md) | 研究目标不能直接当作已实现能力 |
| 抽象状态、原语、不变量、失败语义 | [COSIR 原语规范](../COSIR%20原语规范.md) | POSIR 抽象契约不等同于机器 ABI |
| 当前阶段、退出勾选、活跃阻塞 | [Prima OS 实现阶段](../Prima%20OS%20实现阶段.md) | 报告通过不能自动替代阶段签署 |
| 某切片的设计约束 | [specs 索引](superpowers/specs/README.md) | 先确认该文档是提案、冻结契约还是已被局部修订 |
| 当时的执行步骤 | [plans 索引](superpowers/plans/README.md) | 计划中的“尚未实现”描述属于当时基线 |
| 实际运行、修复、失败和复核 | [reports 索引](superpowers/reports/README.md) | 按范围与后续整改链解释 |
| API 兼容状态、符号调用链 | [compat 索引](superpowers/compat/README.md) | supported 只覆盖登记的 profile 与测试集 |
| Lean 与 Rust 模型字段映照 | [Lean README](../lean/README.md) | 只覆盖其中明确建模的对象与定理 |

实现和规范冲突时，记录偏差；不要把代码自动提升为规范。多个报告冲突时，读取明确的后继整改关系；不要仅按日期挑一篇。

## 文档布局

```text
README.md                         项目介绍与最短入口
AGENTS.md                         仓库工作约束与检索入口
Prima OS.md                       研究架构与阶段定义
COSIR 原语规范.md                 平台无关抽象语义
Prima OS 实现阶段.md              当前阶段、退出条件、活跃问题
lean/README.md                    Lean ↔ Rust 映照契约
docs/
  README.md                       本入口
  catalog.json                    自动生成：文档/章节/关系/行号/哈希/生命周期
  lifecycle.json                  人工维护：废弃/删除理由、替代入口和 Git 恢复
  architecture/
    ai-retrieval.md                AI 检索与上下文组装协议
    system-map.md                 层次、源码归属与调用路径
    topics/                       按子系统组织的阅读包
    topics.json                   人工维护的机器可读主题路由
    catalog.md                    自动生成的全量目录
    templates/                    设计、报告、决策模板
  superpowers/
    specs/                        预期行为与契约
    plans/                        执行记录
    reports/                      证据、审查、失败与整改
    compat/                       profile/符号兼容台账
scripts/docs.py                   无外部依赖的目录生成、检索与校验
```

## 离线检索

在仓库根目录运行：

```sh
python3 scripts/docs.py topic process
python3 scripts/docs.py search 'ISSUE-FORK-CONTINUATION-001' --limit 5
python3 scripts/docs.py search 'fork' --kind report --json
rg -n 'SYS_FORK|space_of' kernel/src prima-user/c
```

主题路由提供精选材料，全文检索覆盖所有源文档。机器目录记录所有 Markdown 标题的起止行；按需读章节，无需把整个历史档案载入上下文。

## 编写与废弃入口

先更新既有权威来源；需要新工件时用[模板与维护手册](architecture/documentation-maintenance.md)。文档的 status 与生命周期独立：历史报告不等于失效，current 不等于批准。默认检索排除 deprecated，但保留历史证据；`search --lifecycle all` 可读全部保留材料，`inspect <旧路径>` 查询删除记录。[生命周期台账](lifecycle.json)保留删除理由、替代入口、原身份与可验证的 Git 恢复版本。
