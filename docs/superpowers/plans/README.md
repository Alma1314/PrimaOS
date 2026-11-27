---
id: plans-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-11-27
---

# Plans 索引

本目录保存任务拆分、依赖和预期验收。计划不是运行证据；旧文中的未开始、候选、工具链和阶段快照按当时基线解释。**当前阶段/阻塞只查[阶段跟踪](../../../Prima%20OS%20实现阶段.md)**，实际结果读[报告索引](../reports/README.md)，按子系统检索用[主题包](../../architecture/topics/README.md)。

阶段 7 范围以[综合交付裁决](../specs/2026-10-31-stage7-scope-decisions.md)为准；Windows 路线需连读[计划审计](../reports/2026-11-27-windows-plan-audit.md)及[阻塞裁决](../specs/2026-11-27-windows-blocker-decisions.md)，不能从旧索引“未启动”推断现状。阶段 5 已由 [SPEC-152](../reports/2026-10-27-stage5-exit-signoff.md)签署，旧完成路线图不再承担当前进度。

VFS 五步短计划已合入[设计的历史执行顺序](../specs/2026-10-23-vfs-libos-foundation-design.md#执行顺序历史记录)；三份早期大计划压缩保留原路径/身份、关键选择与失败观察，原文恢复见[整理记录](../reports/2026-11-27-documentation-consolidation.md)。deprecated 计划保留纠偏关系，禁止照旧执行。

## 阶段 6–7（路线与切片）

| 日期 | 文档 |
|---|---|
| 2026-11-26 | [P4 Gate/IPC 路径收窄执行切片](2026-11-26-p4-path-narrowing.md) |
| 2026-11-18 | [P3 有界异步批量切片计划](2026-11-18-p3-async-slices.md) |
| 2026-11-16 | [P2b 与 holder 模型依赖切片计划](2026-11-16-p2b-holder-model-slices.md) |
| 2026-11-10 | [LibOS 数据通路完整改造计划](2026-11-10-libos-data-path.md) |
| 2026-11-06 | [阶段7执行计划：工程底座与九轨冻结顺序](2026-11-06-stage7-execution.md) |
| 2026-10-31 | [Windows LibOS 计划书：PE64 控制台与服务兼容路径](2026-10-31-windows-libos-roadmap.md) |
| 2026-10-27 | [阶段 6–7 细化提案：从 Linux 静态 ELF 到多 LibOS 并存](2026-10-27-stage6-7-refinement.md) |

## 阶段 5

| 日期 | 文档 |
|---|---|
| 2026-10-22 | [阶段 5 收尾与阶段 6 入口实施路线图](2026-10-22-stage5-completion-roadmap.md) |
| 2026-10-18 | [阶段 5 修订计划：从原生机制到 POSIX 源码兼容](2026-10-18-stage5-revised-plan.md) |
| 2026-10-18 | [阶段 5：musl 移植计划](2026-10-18-musl-port-plan.md) |

## 阶段 1

| 日期 | 文档 |
|---|---|
| 2026-09-16 | [Stage 1 closure: frame ownership + initial Authority Implementation Plan](2026-09-16-stage1-closure.md) |
| 2026-08-04 | [Prima OS 阶段 1 启动闭环 Implementation Plan](2026-08-04-stage1-boot-slice.md) |

## 阶段 0–1 细化 / 审计

| 日期 | 文档 |
|---|---|
| 2026-09-05 | [PF error-code capture + write-protect semantics (slice C continuation) Implementation Plan](2026-09-05-pf-error-code-semantics.md) |
| 2026-08-17 | [Physical Memory Discovery (S1-D3) Implementation Plan](2026-08-17-physmem-discovery.md) |
| 2026-08-17 | [页表/GDT/IDT/trap 框架（S1-D4，切片 B）Implementation Plan](2026-08-17-page-gdt-idt-trap.md) |
| 2026-08-16 | [PrimaOS Audit Remediation Implementation Plan](2026-08-16-primaos-audit-remediation.md) |
| 2026-08-15 | [tlb_refine Implementation Plan](2026-08-15-tlb-refine.md) |
| 2026-08-14 | [COW fork 组合协议细化实现计划](2026-08-14-cow-refinement-slice.md) |
| 2026-08-13 | [exc_resume Security Remediation Implementation Plan](2026-08-13-exc-resume-security-remediation.md) |
| 2026-08-12 | [MemorySegment Refinement Remediation Implementation Plan](2026-08-12-memseg-refinement-remediation.md) |
| 2026-08-12 | [BLK-002 细化续：MemorySegment 页表↔vspace 细化实现计划](2026-08-12-memseg-pagewalk-refinement-slice.md)（deprecated；纠偏入口见正文） |
| 2026-08-07 | [exception 恢复细化 exc_resume 实现计划](2026-08-07-exc-resume-slice.md) |
| 2026-08-07 | [BLK-002 x86_64 细化草案实现计划](2026-08-07-blk002-refinement-slice.md) |

## 跨阶段

| 日期 | 文档 |
|---|---|
| 2026-08-18 | [BLK-001 Lean 4 形式化模型工件 Implementation Plan](2026-08-18-blk001-lean-formal-model.md) |

## 兼容性验证

| 日期 | 文档 |
|---|---|
| 2026-09-15 | [Linux trace + bounded COMPAT-TEST (cow-write-fault) Implementation Plan](2026-09-15-linux-trace-compat.md) |

新增/维护规则与模板统一见[维护手册](../../architecture/documentation-maintenance.md)；生成目录并校验后与正文一起交付。
