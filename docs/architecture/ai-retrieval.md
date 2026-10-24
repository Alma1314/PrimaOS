---
id: guide-ai-retrieval
kind: guide
authority: retrieval-protocol
status: maintained
lifecycle: current
updated: 2026-10-24
---

# AI 检索与上下文组装协议

目标是让任何接手的 AI 从有限上下文找到正确契约、代码和证据，同时保留不确定性。入口为[文档中心](../README.md)，机器路由为[topics.json](topics.json)，全量元数据为[catalog.json](../catalog.json)。

## 检索顺序

1. 读取根目录 `AGENTS.md`，确认任务约束；读取[阶段跟踪](../../Prima%20OS%20实现阶段.md)确认进度与活跃问题。
2. 用主题路由选一个或多个包：`python3 scripts/docs.py topic process`。包中的 `sources` 是推荐阅读关系，不代表全部文档都是冻结契约。
3. 读取原语规范的相关章节及不变量，再读包中的设计、计划、最新报告和前序失败。
4. 用标识符检索精确命中，例如 `ISSUE-FORK-CONTINUATION-001`、`S5-D6`；用英文符号与中文别名交叉检索。
5. 读取源码/测试实现，回答或修改后重新核对契约与证据。

## 有界读取

```sh
python3 scripts/docs.py search 'fork' --kind report --limit 5 --json
python3 scripts/docs.py search 'S5-D6' --limit 5
rg -n 'SYS_FORK|space_of' kernel/src prima-user/c
sed -n '127,310p' 'COSIR 原语规范.md'
```

`search` 实时读文件，返回至多 100 个文档（默认 8），每个文档至多 4 条正文片段，每条至多 240 字符。查询以空格分词、AND 匹配，保留大小写不敏感的符号精度；多个同义词请分别查询。默认排除 deprecated，但保留 historical，以免隐藏前序失败；`--lifecycle all` 包含废弃材料，`--lifecycle current` 只读显式当前材料。已移除正文用 `inspect <旧路径>` 查询替代与 Git 恢复信息。它是词法检索，不是语义搜索，也不判定主张真伪。

目录的 `sections` 提供标题层级、`line`、`end_line`、`anchor`；父章节范围包含子章节，可选最小相关子节读取。`sha256` 表示文件内容，行号只对同一哈希有效。更改文档后重新生成，不能引用旧目录的行号。

## 机器目录字段契约（schema_version 2）

| 字段 | 解释 |
|---|---|
| path_base / documents[].path | 源文档路径相对于仓库根；不是机器绝对路径 |
| documents[].id | front matter 明确 id，否则使用完整相对路径；跨目录同名文件不合并 |
| kind / authority / status / date | 类型、权威角色、文档状态和日期线索；未知历史状态保留显式占位 |
| lifecycle / lifecycle_reason / replacements | 阅读用途、人工废弃理由、保留的替代入口；current 不等于批准或实现，historical 不等于失效；unclassified 须读正文 |
| retired_documents[] | 已移除路径、保留身份、原因、替代入口、恢复 revision/sha256；不含旧正文 |
| sha256 / lines | 当前 UTF-8 内容哈希与总行数，用于判断引用是否陈旧 |
| sections[] | level/title/line/end_line/anchor；ATX 标题，行号从1开始 |
| topic_ids[] | 人工精选关系反向映射到文档，供搜索结果继续路由；未分配不等于失效 |
| links[] | 原始 target 与所在行；相对 target 基于该源文档所在目录解析 |
| mentioned_ids[] | 正文中提及的 SPEC/DOC/BLK/ISSUE/阶段条件；不等于本文身份或完成状态 |
| topics[] | 与 topics.json 相同的精选路由；sources 不是隐式实现/证明/取代关系 |

目录不含完整正文。消费端按 path 和章节行号读取需要的文本；先校验哈希再缓存片段。v1 → v2 新增 lifecycle/lifecycle_reason/replacements 与 retired_documents，旧字段意义不变。消费端必须识别 v2 后按生命周期解释结果；原始日期、status 和 mentioned_ids 都没有升级为批准证据。topic 命令在精选路由之外附 source_lifecycle；默认路由仍可保留被纠偏文档作为历史上下文。

## 上下文包最少保留什么

| 信息 | 必须保留的内容 |
|---|---|
| 用户任务 | 本次目标、明确排除项与已授权动作 |
| 当前状态 | 阶段、退出条件、相关未决问题；链接阶段跟踪 |
| 契约 | 权威路径、章节、必要前置条件与失败原子性 |
| 实现 | 负责模块、入口符号、跨层调用关系 |
| 证据 | 报告路径/日期、工件版本、命令、实际结果与边界 |
| 反例 | 失败条件、workaround、后继修复和仍未覆盖项 |
| 待办 | 尚需完成的具体步骤，不能把计划写成完成 |

不要把整个仓库打成一个超长 `llms-full.md`。根 [llms.md](../../llms.md) 只提供轻量路由；机器目录保存结构，正文保持单一来源。

## 权威与时间规则

当前进度以阶段跟踪为准；抽象行为以原语规范为准；研究目标以白皮书为准；实现证据按对应报告声明的范围判断。历史计划中的“未实现”不覆盖后续验收，最新报告也不自动覆盖不同主题的旧结论。

不能从 `SPEC-xxx` 出现在正文里推断它是本文件的唯一身份。`mentioned_ids` 只表示提及；文档身份使用明确的 `id` 或完整仓库相对路径。同一个报告可能提及数十个 SPEC/ISSUE。

## 回答与交接格式

先给结论，再给“路径 + 章节/当前行号 + 证据范围 + 未覆盖项”。区分规范要求、代码观察、报告记录和本次实跑；未重跑时写“报告记录”，不要复用旧 PASS 声称本次运行通过。

发现冲突时记录双方来源、冲突范围、已有后继材料与无法裁决的部分。文档治理可以修正陈旧摘要，不能代替组织级签署或发布新 profile。
