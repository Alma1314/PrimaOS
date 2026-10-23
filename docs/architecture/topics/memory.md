---
id: topic-memory
kind: topic
authority: explanatory
status: maintained
updated: 2026-10-23
---

# 页帧、映射、TLB 与 COW

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

抽象帧所有权与真实映射如何对应？权限变化和 TLB 刷新在哪里？匿名映射与 fork COW 各验证什么？

检索别名：MemorySegment, memseg, vspace, pagewalk, TLB, COW, mmap, mprotect, W^X。

## 权威章节与阅读顺序

原语规范§5.1–5.2、§7、W6/W7/W8、§12.8–12.9、§20.5。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

host memseg/pagewalk、COW、TLB 设计实验 → 阶段1/2 实际页表与帧管理 → M10 匿名内存保护 → M17b-2 COW 页表原语/故障接线 → M17b-4 W6。host 细化实验与内核 COW 执行不能合并为同一证据。

旧 MemorySegment 设计/计划明确由 2026-08-12 细化审查纠偏设计限定证据与术语；2026-08-13 历史工件一致性报告记录了这次校准。host W8 支持仅限 unmap 入口页 guard，不代表完整 W8 抽象投影。

## 契约、计划与证据入口

- [COSIR 原语规范.md](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)
- [2026-08-12-memseg-pagewalk-refinement-design.md](../../superpowers/specs/2026-08-12-memseg-pagewalk-refinement-design.md)
- [2026-08-12-memseg-pagewalk-refinement-slice.md](../../superpowers/plans/2026-08-12-memseg-pagewalk-refinement-slice.md)
- [2026-08-14-cow-refinement-design.md](../../superpowers/specs/2026-08-14-cow-refinement-design.md)
- [2026-08-14-cow-refinement-slice.md](../../superpowers/plans/2026-08-14-cow-refinement-slice.md)
- [2026-08-15-tlb-refine-design.md](../../superpowers/specs/2026-08-15-tlb-refine-design.md)
- [2026-08-15-tlb-refine.md](../../superpowers/plans/2026-08-15-tlb-refine.md)
- [2026-10-21-stage5-m10-anon-memory-design.md](../../superpowers/specs/2026-10-21-stage5-m10-anon-memory-design.md)
- [2026-10-21-stage5-m10-anon-memory.md](../../superpowers/reports/2026-10-21-stage5-m10-anon-memory.md)
- [2026-10-23-stage5-m17b2-cow-primitives.md](../../superpowers/reports/2026-10-23-stage5-m17b2-cow-primitives.md)
- [2026-10-23-stage5-m17b4-fork-wait.md](../../superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)

- [2026-08-12-refinement-audit-remediation-design.md](../../superpowers/specs/2026-08-12-refinement-audit-remediation-design.md)
- [2026-08-12-memseg-refinement-remediation.md](../../superpowers/plans/2026-08-12-memseg-refinement-remediation.md)
- [2026-08-13-task-6-historical-artifact-consistency.md](../../superpowers/reports/2026-08-13-task-6-historical-artifact-consistency.md)

## 源码定位

- [frames.rs](../../../kernel/src/frames.rs)
- [mem.rs](../../../kernel/src/mem.rs)
- [vspace.rs](../../../kernel/src/vspace.rs)
- [cow.rs](../../../kernel/src/cow.rs)
- [mm.rs](../../../prima-ostd/src/mm.rs)
- [memseg_refine.rs](../../../cosir/src/memseg_refine.rs)
- [pagewalk.rs](../../../cosir/src/pagewalk.rs)
- [tlb_refine.rs](../../../cosir/src/tlb_refine.rs)
- [cow_refine.rs](../../../cosir/src/cow_refine.rs)
- [prima_syscall.c](../../../prima-user/c/prima_syscall.c)

## 测试与观察入口

- [memseg_refine_tests.rs](../../../cosir/src/memseg_refine_tests.rs)
- [pagewalk_tests.rs](../../../cosir/src/pagewalk_tests.rs)
- [tlb_refine_tests.rs](../../../cosir/src/tlb_refine_tests.rs)
- [cow_refine_tests.rs](../../../cosir/src/cow_refine_tests.rs)
- [stage5_mem.c](../../../prima-user/c/stage5_mem.c)
- [stage5_proc.c](../../../prima-user/c/stage5_proc.c)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

匿名 mmap 是受限 arena，不能宣称文件映射/MAP_SHARED 等已实现。M17 COW 的工作路径不消除 fork 包装续执行问题。涉及帧池/映射容量的改动要保留 ENOSPC、清零、失败回滚与回收见证。
