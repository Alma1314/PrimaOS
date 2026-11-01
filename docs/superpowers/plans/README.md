---
id: plans-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-31
---

# Plans 索引

本目录保存**执行计划**：它们记录当时的任务拆分、顺序和验收方式。计划是历史工件，不表示任务已经完成；证据以 `reports/` 中的报告为准。

> 本索引按阶段/主题分组，文件日期用于历史定位。新增文档同步更新本页和相关主题路由，再运行 `python3 scripts/docs.py build` / `check`。统一入口见[文档中心](../../README.md)。

## 阶段 6–7（提案）

当前阶段7范围见[作者确认的综合交付裁决](../specs/2026-10-31-stage7-scope-decisions.md)。执行计划不替代范围、实现或签署。

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-27 | [2026-10-27-stage6-7-refinement.md](2026-10-27-stage6-7-refinement.md) | 阶段 6–7 细化提案：Linux 静态 ELF 子阶段（S6-M1–M6）与多 LibOS 并存的阶段 7 子轨（7-0…7-H） |

> 原计划的 D11–D16 已有[后继裁决](../specs/2026-10-28-stage6-7-release-decisions.md)；2026-10-31 已进一步确认阶段7范围。旧计划的估算和当时摘要按其历史基线解释。

## Windows personality 独立候选扩展

| 日期 | 文档 | 范围与状态 |
|---|---|---|
| 2026-10-31 | [Windows LibOS 计划书](2026-10-31-windows-libos-roadmap.md) | 作者选择控制台与服务优先；PE64、DLL、文件、进程/线程、同步与异常。技术路线及profile为提案，未启动，不改变阶段7的双personality必交付组合 |

## 阶段 5

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-23 | [2026-10-23-vfs-libos-foundation-plan.md](2026-10-23-vfs-libos-foundation-plan.md) | VFS / LibOS 基础重构执行计划 |
| 2026-10-22 | [2026-10-22-stage5-completion-roadmap.md](2026-10-22-stage5-completion-roadmap.md) | 阶段 5 收尾与阶段 6 入口实施路线图（M12–M21 + S6-A–E） |
| 2026-10-18 | [2026-10-18-stage5-revised-plan.md](2026-10-18-stage5-revised-plan.md) | 阶段 5 修订计划：从原生机制到 POSIX 源码兼容 |
| 2026-10-18 | [2026-10-18-musl-port-plan.md](2026-10-18-musl-port-plan.md) | 阶段 5：musl 移植计划 |

> 2026-10-22 路线图是 M11 之后剩余工作的当前总入口；它仍是执行计划，不代表实现状态。

## 阶段 1

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-16 | [2026-09-16-stage1-closure.md](2026-09-16-stage1-closure.md) | Stage 1 closure: frame ownership + initial Authority Implementation Plan |
| 2026-08-04 | [2026-08-04-stage1-boot-slice.md](2026-08-04-stage1-boot-slice.md) | Prima OS 阶段 1 启动闭环 Implementation Plan |

## 阶段 0–1 细化 / 审计

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-05 | [2026-09-05-pf-error-code-semantics.md](2026-09-05-pf-error-code-semantics.md) | PF error-code capture + write-protect semantics (slice C continuation) Implementation Plan |
| 2026-08-17 | [2026-08-17-physmem-discovery.md](2026-08-17-physmem-discovery.md) | Physical Memory Discovery (S1-D3) Implementation Plan |
| 2026-08-17 | [2026-08-17-page-gdt-idt-trap.md](2026-08-17-page-gdt-idt-trap.md) | 页表/GDT/IDT/trap 框架（S1-D4，切片 B）Implementation Plan |
| 2026-08-16 | [2026-08-16-primaos-audit-remediation.md](2026-08-16-primaos-audit-remediation.md) | PrimaOS Audit Remediation Implementation Plan |
| 2026-08-15 | [2026-08-15-tlb-refine.md](2026-08-15-tlb-refine.md) | tlb_refine Implementation Plan |
| 2026-08-14 | [2026-08-14-cow-refinement-slice.md](2026-08-14-cow-refinement-slice.md) | COW fork 组合协议细化实现计划 |
| 2026-08-13 | [2026-08-13-exc-resume-security-remediation.md](2026-08-13-exc-resume-security-remediation.md) | exc_resume Security Remediation Implementation Plan |
| 2026-08-12 | [2026-08-12-memseg-refinement-remediation.md](2026-08-12-memseg-refinement-remediation.md) | MemorySegment Refinement Remediation Implementation Plan |
| 2026-08-12 | [2026-08-12-memseg-pagewalk-refinement-slice.md](2026-08-12-memseg-pagewalk-refinement-slice.md) | BLK-002 细化续：MemorySegment 页表↔vspace 细化实现计划 |
| 2026-08-07 | [2026-08-07-exc-resume-slice.md](2026-08-07-exc-resume-slice.md) | exception 恢复细化 exc_resume 实现计划 |
| 2026-08-07 | [2026-08-07-blk002-refinement-slice.md](2026-08-07-blk002-refinement-slice.md) | BLK-002 x86_64 细化草案实现计划 |

## 跨阶段

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-08-18 | [2026-08-18-blk001-lean-formal-model.md](2026-08-18-blk001-lean-formal-model.md) | BLK-001 Lean 4 形式化模型工件 Implementation Plan |

## 兼容性验证

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-15 | [2026-09-15-linux-trace-compat.md](2026-09-15-linux-trace-compat.md) | Linux trace + bounded COMPAT-TEST (cow-write-fault) Implementation Plan |
