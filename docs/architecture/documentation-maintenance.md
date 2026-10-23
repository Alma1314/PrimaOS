---
id: guide-documentation-maintenance
kind: guide
authority: operational
status: maintained
updated: 2026-10-23
---

# 文档维护、模板与验收

正式规则见[文档系统契约](../superpowers/specs/2026-10-23-documentation-system-design.md)。本页描述实际操作，返回[文档中心](../README.md)。

## 放到哪里

| 你在写什么 | 位置 | 模板 |
|---|---|---|
| 预期行为/接口/设计 | docs/superpowers/specs/日期-主题.md | [设计模板](templates/spec.md) |
| 分步执行与依赖 | docs/superpowers/plans/日期-主题.md | 使用明确任务、依赖、验收命令，不用勾选冒充阶段状态 |
| 实跑/审查/失败/修复 | docs/superpowers/reports/日期-主题.md | [报告模板](templates/report.md) |
| 跨层工作决定/尚待评审的裁决 | specs 下设计或 reports 下决策记录 | [决策模板](templates/decision.md) |
| API 状态/符号追溯 | docs/superpowers/compat/ | 复用既有表，不另建平行矩阵 |
| 概念、源码地图、检索/开发手册 | docs/architecture/ | 本目录说明页格式 |
| 主题阅读包 | docs/architecture/topics/ | 现有主题六段结构，同时更新 topics.json |

## 一次更新的收口顺序

1. 阅读权威章节和前序失败；明确本文类型、日期、范围与身份。
2. 写正文与相对 Markdown 链接，补元数据；报告必须区分“本次实跑”和“引用旧证据”。
3. 新工件加入所属目录 README；相关主题需要时加入 sources，并解释前序/后继范围。
4. 行为或退出证据改变时更新阶段跟踪；兼容 API 改变时同步 profile/两表。纯解释页不复制当前状态。
5. 运行以下文档命令并检查差异；目录文件与正文同次提交。

```sh
python3 scripts/docs.py self-test
python3 scripts/docs.py build
python3 scripts/docs.py check
python3 scripts/docs.py search '你的标识符' --limit 5
```

## 生成物规则

[catalog.json](../catalog.json)与[catalog.md](catalog.md)自动生成，不能手工修复行号/哈希。[topics.json](topics.json)人工维护，是主题事实来源；主题正文负责解释阅读关系。

`check`不修改文件；生成物过期会返回非零。`build`只更新目录，不保证全部链接正确，必须随后运行 check。检索实时读源文件，即使目录陈旧也可定位正文；引用目录行号前检查哈希/重新生成。

## 历史迁移规则

保留工件原路径及原始结论，只修复可明确解析的链接与当前入口描述。路径变更须检查入站引用并提供迁移说明。历史草案中号段/范围与现行契约不同，应在主题阅读链或该文件顶部注明后继，不在旧提案里伪装成从来没有分歧。

无法从现有资料裁决的问题应保留“未知/待确认”；没有报告的测试不能补写 PASS。文档验收结果单独放在报告，不把导航修复当作阶段验收。

## 校验范围与人工审查

自动检查覆盖源文件、ID、精选引用、本地路径、ATX 锚点和生成物新鲜度；不访问外网、不证明技术正确性、不自动验证历史链与批准真实性。引用 shell/代码片段中的裸路径、复杂 HTML/Markdown 语法仍需人工检查。

审查时重点看：是否存在新的第二份状态表；是否遗漏前序失败或未覆盖项；是否把模板字段当成真实证据；是否在 API 号空间、W 编号和 profile 版本之间混用概念。
