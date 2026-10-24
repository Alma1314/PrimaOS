---
id: guide-documentation-maintenance
kind: guide
authority: operational
status: maintained
lifecycle: current
updated: 2026-10-24
---

# 文档编写、复查与废弃操作手册

规则以[文档系统契约](../superpowers/specs/2026-10-23-documentation-system-design.md)为准；本页给出可执行步骤。返回[文档中心](../README.md)。

## 开始前选择操作

| 触发 | 应做的工作 |
|---|---|
| 已有文档回答相同问题 | 更新原权威章节和引用，避免另写第二份当前摘要 |
| 新接口或跨层设计 | 写规格，明确预期、失败和权限；未批准仍标提案 |
| 复杂任务需拆依赖/验收 | 写计划；简单修改直接在报告记录范围与命令 |
| 有实跑、审查、失败或纠偏 | 写报告，记录版本、实际观察、限制和后继关系 |
| 旧文字部分过时 | 修当前说明或登记 historical；不删除整份独立证据 |
| 旧材料被明确取代且仍解释纠偏 | 登记 deprecated，保留全文与后继入口 |
| 冗余交接/对话或失效一次性操作 | 检查引用、登记可恢复信息后删除 |

## 类型、位置与最小内容

| 内容 | 位置 | 模板 |
|---|---|---|
| 预期行为/接口/设计 | docs/superpowers/specs/日期-主题.md | [设计](templates/spec.md) |
| 执行依赖与顺序 | docs/superpowers/plans/日期-主题.md | [计划](templates/plan.md) |
| 实跑/审查/失败/修复 | docs/superpowers/reports/日期-主题.md | [报告](templates/report.md) |
| 有依据的选择或待确认决定 | specs 或 reports，按提案/实证性质 | [决策](templates/decision.md) |
| API/符号支持范围 | docs/superpowers/compat/ | 复用两表，不建平行矩阵 |
| 概念、源码地图、检索/开发手册 | docs/architecture/ | 现有指南格式 |
| 主题阅读包 | docs/architecture/topics/ | 契约、前序失败、后继、代码、测试、限制 |

新文档必须填写 id/kind/authority/status/lifecycle/updated；模板 id 必须换成真实身份。status 描述提案、冻结或验证状态，lifecycle 描述维护用途：两者互不推导。current 不代表实现或批准；historical 不代表完成或失效；deprecated 表示停止照此实施；removed 只存在台账；unclassified 表示旧文档尚未分类。

## 编写与复查事务

1. 保存基线提交 `git rev-parse HEAD`，读上位契约、相关主题和前序失败。以行为变更、默认命令变化、上游快照变化、证据或断链为复查触发，不按文档年龄批量废弃。
2. 先更新已有来源；需要新工件时写适用范围、版本、结论/预期、来源、实际证据、限制及重审条件。计划里预期 PASS 必须标预期，报告里未跑就写未跑。
3. 新工件加入所属目录 README；主题涉及新材料时更新 topics.json 和主题正文。不要把新的计划当成第二份阶段状态表。
4. 行为或证据变化同步阶段跟踪；兼容变化同步两表/profile。纯文档清理不勾选阶段、不发布 profile。
5. 跑下列文档命令，生成物和正文同次提交；脚本逻辑变化同时补自检。仓库要求的 Rust 测试/lint 按 AGENTS 执行；不涉及 boot/协议不需重跑 QEMU。

```sh
python3 scripts/docs.py self-test
python3 scripts/docs.py build
python3 scripts/docs.py check
python3 scripts/docs.py check --base <本轮基线提交>
python3 scripts/docs.py search '相关标识符' --limit 5 --json
```

## 废弃或删除事务

先读全文，确认材料是否唯一承载契约、决策、失败、实验或签署；这类材料继续保留。外部候选尚待裁决也不等于废弃。旧设计被纠偏但有追溯价值时保留 deprecated，不把原失败改成成功。

```sh
python3 scripts/docs.py inspect docs/原路径.md
rg -n '旧文件名|旧文档ID' --glob '*.md' --glob '*.json'
git show <清理前提交>:docs/原路径.md
```

inspect 显示结构化入站链接、主题、生命周期和替代入口；rg 补查裸路径、代码片段和非 Markdown 引用。自动工具不裁决删除价值。

[lifecycle.json](../lifecycle.json)是人工维护台账。deprecated 条目写 path/lifecycle/updated/reason/replacements；新维护文档若有 front matter lifecycle，值须与台账一致。删除再增加原 id、40 位 revision、SHA-256；旧文档无 id 时保留原相对路径为身份。可用 `git rev-parse HEAD` 获取版本，`sha256sum <旧路径>` 获取哈希，在删前确认该版本保存的是同样内容。未提交的历史正文先形成可恢复提交，不能只留下无法恢复的哈希。

删除正文并修复所有入站链接、README、主题路由；历史报告的链接改为退役台账并说明删除，不将它的实跑结论改写。没有功能替代时 replacements 指向解释放弃理由和保留限制的报告。重新 build/check，并用 check --base 防止漏登记删除或新文档元数据。

查询旧路径：`python3 scripts/docs.py inspect <旧路径>`；恢复正文：`git show <revision>:<旧路径>`。退役 id 不复用；不创建占位正文或另一套 archive 副本。外部旧链接会失效，但旧路径在台账和 Git 中可发现、可核对、可恢复。

## 生成、检索与人工检查

[catalog.json](../catalog.json)和[catalog.md](catalog.md)自动生成，禁止手修；[topics.json](topics.json)和生命周期台账人工维护。schema_version 2 目录保留正文哈希、章节范围、生命周期和删除记录。

search 默认包括历史报告和未分类旧文档，排除 deprecated；用 `--lifecycle all` 查全部留存材料，`--lifecycle current` 只查显式当前材料。已移除正文不全文检索，用 inspect 查询。topic 输出为精选路径附生命周期提示；仍须读正文判断批准、实现与证据范围。

check 不修改文件；它校验结构、链接、恢复 Git blob/哈希/身份及生成物新鲜度。check --base 包含当前已保存的未跟踪文档，额外检查新增元数据和删除登记；发布时基线应为合并前主分支或本轮开始提交，不用已经包含变更的 HEAD 逃避差异检查。

浅克隆缺恢复提交时需补历史。远端 URL 不联网验证；裸路径与复杂 Markdown 语法、替代范围、批准真实性和语义正确性仍需人工审查。日期与低引用数不会触发自动删除。
