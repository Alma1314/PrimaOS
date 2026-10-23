---
id: report-documentation-restructure
kind: report
authority: documentation-evidence
status: maintained
updated: 2026-10-23
---

# 文档体系重构与验收报告

本次将文档改为“任务入口 → 主题阅读包 → 权威章节 → 实现/证据 → 限制”的检索体系，保留历史工件路径和失败结论。没有修改 Rust/C/Lean 运行行为，没有发布新 profile，也没有改变阶段退出勾选或代替组织级签署。

基线：`d48d5ef42247b40845997607e83ef717920bb8ab`，开始时工作树干净。设计见[文档系统契约](../specs/2026-10-23-documentation-system-design.md)，执行顺序见[计划](../plans/2026-10-23-documentation-restructure-plan.md)。

## 1. 交付内容

- 根 README、AGENTS、llms.md 和 docs/README 使用同一权威分工与任务路由；入口不重复维护当前阶段清单。
- 新增入门、AI 检索、系统/代码地图、术语、接口/版本、证据规则、开发验证和维护手册。
- 十个主题包连接规范/计划/报告链、代码、测试与限制；topics.json 保存人工精选路径。
- catalog.json/catalog.md 自动枚举源文档与全部 ATX 标题，包含行号范围、哈希、角色、引用和编号提及。
- 标准库 scripts/docs.py 提供 build/check/search/topic/self-test；无需安装包或外部检索服务。
- 提供设计、报告、决策模板与文档维护事务，源文档和生成物同次更新。

## 2. 状态摘要修正依据

| 重构前的混淆 | 当前整理 | 依据与保留边界 |
|---|---|---|
| 阶段5入口堆叠M0–M21过程，仍写线程/网络未实现 | 改为里程碑证据表与M21出口判定 | [M16b-3](2026-10-23-stage5-m16b3-pthread-join.md)、[M19](2026-10-23-stage5-m19-loopback-udp.md)、[M21](2026-10-23-stage5-m21-exit-review.md)；只认最小已验收子集 |
| pthread问题标关闭但描述仍停在join阻塞 | 当前摘要链接后继clear_child_tid收口 | M16b-1/2失败保留，M16b-3是后继；未扩展取消/分离/属性结论 |
| COW/wait、exec、signal段落同时保留“未接线”和后继通过 | 当前状态与历史过程分开，主题说明后继范围 | M17b-4、M17c-3/4、M18b；保留包装续执行、VFS子任务授权、信号掩码等缺口 |
| 阶段0已勾选行里仍写“暂不勾选” | 退出表只记录SPEC-145接受的文本状态 | 前序未通过与SPEC-143/144整改报告不改写 |
| 白皮书开头把整个项目概括为只有抽象设计 | 改为架构/研究契约定位并指向阶段跟踪 | 不在白皮书重建第二份实现摘要 |
| 白皮书笼统说模型不构成证明 | 区分可执行对拍与独立CDT定理 | [Lean映照契约](../../../lean/README.md)；不声称完整内核细化证明 |
| compat早期M0–M11描述与M20状态混读 | 增加按章节的基线/后继范围说明 | 不改变profile批准状态；历史planned/UNTEST仍保留 |
| compat的42–46名称不对应源码 | 修正为prima-abi现存常量，标题范围改为42–58 | 不分配52，不修改ABI；M17旧设计号由后继契约解释 |

阶段5仍进行中、M21未签署、阶段6未开始、BLK-003仍Open。当前结论见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

## 3. 链接、路径与历史保全

基线263份Markdown中有81处活跃wiki引用；整理后不再残留活跃wiki引用。部分引用随重复状态段落压缩而移除，其余转换为可移植Markdown路径；代码块与内联代码中的TOML `[[bin]]` 等不转换。

修复M20报告中多退一级的compat相对路径。消除symbol-trace两个“§5”入口的歧义，将旧未决依赖标为历史基线，保留M20调用追溯的§5。原始架构问答加历史范围提示，原对话结论不伪装成当前状态。

另外逐一比较23份被修改的既有spec/plan/report与基线：差异仅为wiki引用转换及M20相对路径修正；内联代码示例保留。`git diff --check`通过。

旧工件均留在原路径，文档整理前完整内容可以用 `git show d48d5ef:<仓库相对路径>` 获取。历史计划/报告中的实验失败、曾用号段和未覆盖范围不重写为成功。

## 4. 本次验证

本节结果只属于文档维护与本次实际执行；不把M21原有114条QEMU PASS当成本次重跑。

| 检查 | 本次结果 | 范围 |
|---|---|---|
| docs.py self-test | PASS，退出0 | Unicode、围栏、重复标题、断链/锚点、元数据、检索与确定性 |
| docs.py build / check | PASS，退出0；288份源文档、2896个章节 | 全量源文件、目录生成、唯一ID、主题路径、目录README覆盖、本地路径/锚点、无活跃wiki引用 |
| 检索场景 | PASS：4项查询与process主题关系断言 | fork/pthread/阶段5/Lean的来源与限制能被精确定位 |
| 生成物一致性/负例 | PASS：确定性重建、陈旧目录/重复ID/断链/错误锚点拒绝 | 相同输入重建相同字节；陈旧目录/错误引用使check非零 |
| cargo test --workspace | PASS，退出0；865个测试执行项、0失败 | Rust host测试；首次沙箱运行因rsproxy.cn DNS失败，获准联网重跑后通过。865为各测试目标passed计数之和，包含相同模块在不同目标中的执行，不是865个独立性质 |
| cargo xtask lint | PASS，退出0 | fmt、Clippy -D warnings、x86_64-unknown-none编译、29个kernel源文件零unsafe |
| cargo xtask test | 未运行 | 本次不改变boot、ABI或协议执行；未复跑QEMU验收 |

## 5. 限制

- 不联网探测外部网页，不保证历史远端URL仍可用；本地目标与锚点已纳入校验。
- 检索是有限片段的词法AND匹配，不自动判定主张、批准状态或后继关系。
- Markdown解析针对仓库实际子集，不是完整CommonMark实现；复杂HTML、代码示例中的裸路径仍需人工核对。
- 主题包是精选集，全量材料通过目录与全文检索覆盖；不把未选入主题的旧报告判为失效。
- 文档治理不修复已登记的OS缺陷、不替代组织级确认或扩大任何证据范围。

## 6. 后续维护入口

新增工件更新所属README与相关主题，运行self-test/build/check；行为与证据变化继续同步阶段跟踪/compat台账。操作见[维护手册](../../architecture/documentation-maintenance.md)，报告使用[模板](../../architecture/templates/report.md)。
