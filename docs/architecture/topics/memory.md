---
id: topic-memory
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-31
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

S6-M2 的[大映像帧契约](../../superpowers/specs/2026-10-31-stage6-exec-frame-capacity.md)
与[失败/容量证据](../../superpowers/reports/2026-10-31-stage6-exec-frame-capacity.md)扩展
私有exec向量及全局预算，验证526页、失败回收与大映像再次exec；普通段仍≤149页，
不代表完整VFS、冻结busybox、Gate或LibOS运行。

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

OSTD 后继补强：[Asterinas 移植契约](../../superpowers/specs/2026-10-24-asterinas-reuse-design.md)与
[验证报告](../../superpowers/reports/2026-10-24-asterinas-reuse-report.md)限定对齐工具、StaticCell IRQ-safe 锁与调度跳转；
来源与许可见 [来源与许可](../asterinas-provenance.md)。候选组件不等于已迁入。

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

- [align.rs](../../../prima-ostd/src/align.rs)
- [sync.rs](../../../prima-ostd/src/sync.rs)
- [lock.rs](../../../prima-ostd/src/sync/lock.rs)

## 测试与观察入口

- [memseg_refine_tests.rs](../../../cosir/src/memseg_refine_tests.rs)
- [pagewalk_tests.rs](../../../cosir/src/pagewalk_tests.rs)
- [tlb_refine_tests.rs](../../../cosir/src/tlb_refine_tests.rs)
- [cow_refine_tests.rs](../../../cosir/src/cow_refine_tests.rs)
- [stage5_mem.c](../../../prima-user/c/stage5_mem.c)
- [stage5_proc.c](../../../prima-user/c/stage5_proc.c)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

匿名 mmap 是受限 arena，不能宣称文件映射/MAP_SHARED 等已实现。M17 COW 原工作路径与 fork 续执行后继的范围须分别读取下方报告。涉及帧池/映射容量的改动要保留 ENOSPC、清零、失败回滚与回收见证。

## POD 与借用缓冲后继

[Asterinas POD/恢复帧契约](../../superpowers/specs/2026-10-24-asterinas-pod-buffer-design.md)和
[本轮证据](../../superpowers/reports/2026-10-24-asterinas-pod-buffer-report.md)覆盖
[pod.rs](../../../prima-ostd/src/pod.rs) 的类型审计、TSS 布局及恢复缓冲的 checked slice / 临界区。
仍不提供完整用户内存权限验证或 fault-safe 拷贝；原始 VA 接口的契约边界保留。

## 启动信息的一次发布（OSTD 后继）

[契约](../../superpowers/specs/2026-10-24-asterinas-once-publication-design.md)与
[验证报告](../../superpowers/reports/2026-10-24-asterinas-once-publication-report.md)限定 Once 的错误/unwind 重试、
IRQ-safe 初始化和析构，以及 user::KernelRootInfo 的根/镜像描述整体发布。
get 无等待；set 争用返回原值；递归初始化仍禁止。self_root_ready.published_once 来自重复构建与帧计数不变见证。
检索：Once、get_or_try_init、KernelRootInfo、published_once；实现见
[sync.rs](../../../prima-ostd/src/sync.rs)、[user.rs](../../../kernel/src/user.rs)、
[trace 校验](../../../xtask/src/protocol.rs)。该机制未替换所有 boot 标志或证明 SMP。

## fork / 等待写回整改后继

[工作契约](../../superpowers/specs/2026-10-24-fork-continuation-design.md)与
[实际验证](../../superpowers/reports/2026-10-24-fork-continuation-report.md)接续 M17b 的续执行失败链：
clone_root_cow 的构造/写保护/同步刷新顺序，分配失败回收、NX 保留、wait_status_bytes 的四字节边界、
跨页/COW 输出以及 musl 子进程退出。`cargo xtask test-process [W6|W7|W7b|W8]` 使用原双轮验收。
没有新增抽象原语、SMP、fd close 隔离或子任务完整 VFS 授权；历史失败不删除。

## S6-M1 输出与复制的内核前置

[M1 协议](../../superpowers/specs/2026-10-29-stage6-m1-protocol.md)与
[真实页表事务证据](../../superpowers/reports/2026-10-30-stage6-user-copy-transactions.md)给出
显式 root 读入、完整页权限预检和全部 COW 帧先预留/失败归还的路径。
`PreparedUserWrite` 不能跨调度或映射变更保存；原生输出、IPC 接收和 wait 写回共用该路径。
内存原语不承担 Gate 请求授权/Task 代次检查，G1–G3 与服务接线仍待完成。
测试见 [vspace.rs](../../../kernel/src/vspace.rs) 与
[W9 跨页 COW 探针](../../../prima-user/c/stage5_sigaction.c)。
