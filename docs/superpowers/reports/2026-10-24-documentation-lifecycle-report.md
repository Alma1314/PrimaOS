---
id: report-documentation-lifecycle
kind: report
authority: documentation-evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-24
---

# 文档生命周期治理与过时材料清理报告

本次删除冗余或失效的交接、对话和一次性搭建计划；以可验证的退役记录替代永久保留全部正文的规则。保留有效契约、未决候选、原始失败与整改/签署证据。仅修改文档与 scripts/docs.py，不改变 Rust/C/Lean/boot 行为，不发布 profile，不签署阶段。

## 基线与契约

清理前基线 `e71caef0ef1bfeb9dfd5d200615d6f96ee1d2abc`，开始时工作树干净。规则沿用原身份的[文档系统契约](../specs/2026-10-23-documentation-system-design.md)并于本次修订；原[文档重构验收](2026-10-23-documentation-restructure-report.md)的运行结论保留。使用[维护手册](../../architecture/documentation-maintenance.md)与[生命周期台账](../../lifecycle.json)。

## 删除判定与替代

| 原路径（相对于仓库根） | 判定 | 仍保留的承接来源 |
|---|---|---|
| docs/architecture/primaos-architecture-qa.md | 817 行对话整理；仍写单文件存储、无 stdin/exec 等旧状态；不是契约或运行证据 | 白皮书、系统地图、VFS/进程主题 |
| docs/superpowers/plans/2026-07-30-implementation-stage-tracker.md | 438 行一次性初始化步骤；无实现/非 Git/Windows 目录的基线已失效 | 阶段跟踪及维护手册 |
| docs/superpowers/plans/2026-07-30-cosir-primitive-fixes.md | 286 行早期修文替换块；Faulted、跨节点 v1 等语义已有后继裁决 | 现行原语规范、SPEC-145 原审查 |
| docs/superpowers/plans/2026-10-23-documentation-restructure-plan.md | 35 行已完成搭建顺序；维护流程已有单一来源 | 维护手册、原验收报告 |
| docs/superpowers/reports/2026-10-14-dual-personality-handoff.md | 71 行会话快照，明确非规范/非设计/非证据；下一阶段描述已失效 | 原语规范、进程主题、原 H0 评审 |

共移除 5 份、1647 行源正文；不是按日期或引用数批量删除。逐份读全文并查 Markdown 入站、目录、主题及裸文件名引用。所有旧路径、身份、精确 SHA-256 和可恢复提交保存在台账；inspect 可查询。原文从该基线 Git 提交恢复，不复制 archive 或留下重复正文占位页。

## 保留与纠偏

MemorySegment 的 2026-08-12 旧设计和执行计划正文已明确 superseded，且是模型边界纠偏链的一部分：保留为 deprecated，替代入口为 2026-08-12 纠偏设计/计划。默认搜索不再把它们当普通结果，all/deprecated 筛选和主题包仍可定位；主题输出附生命周期与替代提示。

DOC-002 基座库候选规格及风险走查与 BLK-003 仍关联，登记 historical，继续保留，未声明被批准或放弃。其余失败、反例、整改、签署报告和冻结 profile 没有删除。historical 仅表示带版本过程/证据；既不表示失效，也不表示执行完成。活跃阶段 5 路线图例外登记 current，但不替代阶段跟踪。

## 编写与工具行为

新工件要先判断是否可更新既有来源；只有复杂任务需要独立计划。规范、计划、报告、决策四种模板明确身份、范围、证据、限制与生命周期；维护指南不维护第二份实现状态。

目录 schema_version 从 1 升为 2：原有字段意义不变，新增 lifecycle/lifecycle_reason/replacements 与 retired_documents。已有维护文档补显式 lifecycle，未迁移旧 plan/report 按类型归 historical，其他旧材料 unclassified；不推断批准和实现。人工台账例外与 front matter 必须一致，退役 id 不得复用。

check 校验删除记录和真实 Git blob 的哈希/身份；替代必须指向保留且非 deprecated 的来源，避免失效链/循环。check --base 基于 Git 树与当前保存文件（包括未跟踪文件）检查新增元数据和所有范围内的删除；缺失恢复历史、漏登记删除、重复身份、新文件无元数据均拒绝。inspect 查元数据、结构入站与退役路径；裸路径仍需 rg 人工审查。

## 验证

所有下列 PASS 均为本次实际执行；不引用旧 musl 的 62 轮 BIOS PASS 作为本次实跑。

| 命令/检查 | 实际结果 | 范围 |
|---|---|---|
| python3 scripts/docs.py self-test | PASS，退出 0 | Markdown/Unicode/元数据/链接；Git 临时仓库中验证未登记删除、缺失新增元数据、退役 ID 复用、恢复缺失/错哈希、重建旧路径、缺失/自指替代、无效生命周期/日期、默认排除废弃与显式筛选 |
| python3 scripts/docs.py build / check | PASS，退出 0；291 份源文档、2859 个章节 | 目录、ID、主题、生命周期、本地路径/锚点及新鲜度；实际重建字节确定性 |
| python3 scripts/docs.py check --base e71caef0ef1bfeb9dfd5d200615d6f96ee1d2abc | PASS，退出 0 | 两份新增 Markdown 元数据、5 份删除登记与基线内容一致，Git 原文恢复哈希/身份 |
| inspect / topic memory / search 场景 | PASS，退出 0 | 5 个旧路径无悬空入站；memory 附纠偏提示；默认排除 deprecated，all/deprecated 可找旧设计；fork 未决问题仍可检索 |
| CARGO_BUILD_JOBS=2 cargo test --workspace | PASS，退出 0；876 个测试执行项、0 失败 | Rust host 测试；执行项之和，不是独立性质数量 |
| CARGO_BUILD_JOBS=2 cargo xtask lint | PASS，退出 0 | fmt、Clippy -D warnings、bare-metal build、29 个 kernel 文件零 unsafe |
| git diff --check / 阶段勾选对比 | PASS | 所有阶段交付/退出 checkbox 与基线逐个一致 |
| cargo xtask test | 未运行 | 未修改 boot/ABI/服务协议或 Rust/C/Lean 运行行为，本次不新增 QEMU 证据 |

本地过程日志位于 gitignored 的 target/docs-lifecycle-validation/，不作为可永久下载的仓库工件；本报告记录命令、结果与范围。

## 限制与阶段影响

外部旧 URL/文件路径不再直接打开正文，台账和 Git 提供恢复入口。浅克隆缺恢复提交会校验失败，需补历史。工具不访问外网、不判定批准真实性、替代的语义完整性或复杂 Markdown；这些仍由正文审查负责。生命周期与搜索分数不是权威签署。

OS 阶段勾选、阶段状态、BLK-003 与 profile 均不改变。现有规范路径和独立失败/整改结论保留；本报告只支持文档治理与检索行为的验证结论。
