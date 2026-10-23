---
id: plan-documentation-restructure
kind: plan
authority: historical-process
status: maintained
updated: 2026-10-23
---

# 文档体系重构执行计划

日期：2026-10-23。基线：`d48d5ef42247b40845997607e83ef717920bb8ab`。

本计划实施[文档系统设计](../specs/2026-10-23-documentation-system-design.md)，最终结果见[验收报告](../reports/2026-10-23-documentation-restructure-report.md)。计划不代表步骤已通过。

## 执行顺序

1. 盘点所有 Markdown、权威文档、目录索引、当前阶段和最新退出报告；检查源码入口、工具链与测试命令。
2. 建立任务导航、AI 检索协议、源码地图、术语、接口/版本、证据规则与开发手册。
3. 按子系统建立契约/报告链/源码/测试阅读包及 topics.json，不移动历史工件。
4. 将阶段跟踪中的过程堆叠改为当前摘要；按 M16/M17/M18/M19/M21 修正已过时表述，保留失败、偏差与未签署状态。
5. 用标准库工具生成全量文件/章节/哈希/引用目录；规范 Markdown 链接，修复可明确解析的断链。
6. 加入维护契约和模板；补 AGENTS/README/llms.md 的统一路由，避免多入口复制状态。
7. 执行工具自检、生成、文档校验、检索验收和仓库要求的测试/lint；报告实际范围与限制。

## 验收场景

- AI 查询 fork，能同时找到契约、W6 成功、包装续执行失败及 VFS 子任务授权限制。
- 查询 pthread，能读到从 clone/join 阻塞到 clear_child_tid 收口的后继链。
- 查询阶段5，能识别 M21 未签署，不能由测试 PASS 推出阶段6已启动。
- 查询 Lean，能区分 CDT 定理、模型未覆盖项与完整内核细化证明。
- 新增/修改文档后，生成物陈旧、断链或错误锚点能使 check 非零退出。

## 不包含

不改 Rust/C/Lean 运行行为，不发布 profile，不代签阶段，不获取新外部证据，不重写历史报告结论。
