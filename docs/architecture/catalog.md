# 全量文档目录

> 自动生成：`python3 scripts/docs.py build`。勿手改。
> 范围：仓库根目录、docs/、lean/ 的 Markdown 源文档；本生成页不自索引。

返回[文档入口](../README.md) · [主题地图](topics/README.md) · [机器目录](../catalog.json)

文件日期用于定位历史，不能用于推断当前实现或批准状态。

## agent-instructions（1）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [Repository Guidelines](../../AGENTS.md) | current | `AGENTS.md` |

## compatibility（2）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [阶段 5 兼容状态台账与历史基线](../superpowers/compat/compat-state.md) | unclassified | `docs/superpowers/compat/compat-state.md` |
| [阶段 5 符号 ↔ 系统调用追溯（musl 1.2.5 / echo）](../superpowers/compat/symbol-trace.md) | unclassified | `docs/superpowers/compat/symbol-trace.md` |

## guide（5）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [AI 检索与上下文组装协议](ai-retrieval.md) | current | `docs/architecture/ai-retrieval.md` |
| [开发、构建与验证手册](development.md) | current | `docs/architecture/development.md` |
| [文档编写、复查与废弃操作手册](documentation-maintenance.md) | current | `docs/architecture/documentation-maintenance.md` |
| [入门与任务阅读路线](getting-started.md) | current | `docs/architecture/getting-started.md` |
| [系统层次与源码地图](system-map.md) | current | `docs/architecture/system-map.md` |

## index（10）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [Prima OS](../../README.md) | current | `README.md` |
| [PrimaOS 文档中心](../README.md) | current | `docs/README.md` |
| [架构、开发与检索手册](README.md) | current | `docs/architecture/README.md` |
| [按子系统检索](topics/README.md) | current | `docs/architecture/topics/README.md` |
| [设计、执行与证据档案](../superpowers/README.md) | current | `docs/superpowers/README.md` |
| [兼容性台账索引](../superpowers/compat/README.md) | current | `docs/superpowers/compat/README.md` |
| [Plans 索引](../superpowers/plans/README.md) | current | `docs/superpowers/plans/README.md` |
| [Reports 索引](../superpowers/reports/README.md) | current | `docs/superpowers/reports/README.md` |
| [Specs 索引](../superpowers/specs/README.md) | current | `docs/superpowers/specs/README.md` |
| [PrimaOS](../../llms.md) | current | `llms.md` |

## ledger（1）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [阶段 6 静态 ELF 白名单（D12 冻结）](../superpowers/compat/stage6-static-whitelist.md) | current | `docs/superpowers/compat/stage6-static-whitelist.md` |

## plan（20）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [Prima OS 阶段 1 启动闭环 Implementation Plan](../superpowers/plans/2026-08-04-stage1-boot-slice.md) | historical | `docs/superpowers/plans/2026-08-04-stage1-boot-slice.md` |
| [BLK-002 x86_64 细化草案实现计划](../superpowers/plans/2026-08-07-blk002-refinement-slice.md) | historical | `docs/superpowers/plans/2026-08-07-blk002-refinement-slice.md` |
| [exception 恢复细化 exc_resume 实现计划](../superpowers/plans/2026-08-07-exc-resume-slice.md) | historical | `docs/superpowers/plans/2026-08-07-exc-resume-slice.md` |
| [BLK-002 细化续：MemorySegment 页表↔vspace 细化实现计划](../superpowers/plans/2026-08-12-memseg-pagewalk-refinement-slice.md) | deprecated | `docs/superpowers/plans/2026-08-12-memseg-pagewalk-refinement-slice.md` |
| [MemorySegment Refinement Remediation Implementation Plan](../superpowers/plans/2026-08-12-memseg-refinement-remediation.md) | historical | `docs/superpowers/plans/2026-08-12-memseg-refinement-remediation.md` |
| [exc_resume Security Remediation Implementation Plan](../superpowers/plans/2026-08-13-exc-resume-security-remediation.md) | historical | `docs/superpowers/plans/2026-08-13-exc-resume-security-remediation.md` |
| [COW fork 组合协议细化实现计划](../superpowers/plans/2026-08-14-cow-refinement-slice.md) | historical | `docs/superpowers/plans/2026-08-14-cow-refinement-slice.md` |
| [tlb_refine Implementation Plan](../superpowers/plans/2026-08-15-tlb-refine.md) | historical | `docs/superpowers/plans/2026-08-15-tlb-refine.md` |
| [PrimaOS Audit Remediation Implementation Plan](../superpowers/plans/2026-08-16-primaos-audit-remediation.md) | historical | `docs/superpowers/plans/2026-08-16-primaos-audit-remediation.md` |
| [页表/GDT/IDT/trap 框架（S1-D4，切片 B）Implementation Plan](../superpowers/plans/2026-08-17-page-gdt-idt-trap.md) | historical | `docs/superpowers/plans/2026-08-17-page-gdt-idt-trap.md` |
| [Physical Memory Discovery (S1-D3) Implementation Plan](../superpowers/plans/2026-08-17-physmem-discovery.md) | historical | `docs/superpowers/plans/2026-08-17-physmem-discovery.md` |
| [BLK-001 Lean 4 形式化模型工件 Implementation Plan](../superpowers/plans/2026-08-18-blk001-lean-formal-model.md) | historical | `docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md` |
| [PF error-code capture + write-protect semantics (slice C continuation) Implementation Plan](../superpowers/plans/2026-09-05-pf-error-code-semantics.md) | historical | `docs/superpowers/plans/2026-09-05-pf-error-code-semantics.md` |
| [Linux trace + bounded COMPAT-TEST (cow-write-fault) Implementation Plan](../superpowers/plans/2026-09-15-linux-trace-compat.md) | historical | `docs/superpowers/plans/2026-09-15-linux-trace-compat.md` |
| [Stage 1 closure: frame ownership + initial Authority Implementation Plan](../superpowers/plans/2026-09-16-stage1-closure.md) | historical | `docs/superpowers/plans/2026-09-16-stage1-closure.md` |
| [阶段 5：musl 移植计划](../superpowers/plans/2026-10-18-musl-port-plan.md) | historical | `docs/superpowers/plans/2026-10-18-musl-port-plan.md` |
| [阶段 5 修订计划：从原生机制到 POSIX 源码兼容](../superpowers/plans/2026-10-18-stage5-revised-plan.md) | historical | `docs/superpowers/plans/2026-10-18-stage5-revised-plan.md` |
| [阶段 5 收尾与阶段 6 入口实施路线图](../superpowers/plans/2026-10-22-stage5-completion-roadmap.md) | current | `docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md` |
| [VFS / LibOS 基础重构执行计划](../superpowers/plans/2026-10-23-vfs-libos-foundation-plan.md) | historical | `docs/superpowers/plans/2026-10-23-vfs-libos-foundation-plan.md` |
| [阶段 6–7 细化提案：从 Linux 静态 ELF 到多 LibOS 并存](../superpowers/plans/2026-10-27-stage6-7-refinement.md) | current | `docs/superpowers/plans/2026-10-27-stage6-7-refinement.md` |

## reference（9）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [Prima OS 实现阶段](../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md) | current | `Prima OS 实现阶段.md` |
| [Prima OS：架构与研究白皮书](../../Prima%20OS.md) | current | `Prima OS.md` |
| [Asterinas 来源与修改](asterinas-provenance.md) | current | `docs/architecture/asterinas-provenance.md` |
| [证据范围与结论规则](evidence.md) | current | `docs/architecture/evidence.md` |
| [术语与检索别名](glossary.md) | current | `docs/architecture/glossary.md` |
| [接口、号段与版本边界](interfaces.md) | current | `docs/architecture/interfaces.md` |
| [许可证：GPLv3 与本仓库的许可分布](licensing.md) | current | `docs/architecture/licensing.md` |
| [阶段 5 当前工作 API 与符号调用边界](../superpowers/compat/stage5-working-api.md) | current | `docs/superpowers/compat/stage5-working-api.md` |
| [形式化模型与内核映照（Lean）](../../lean/README.md) | current | `lean/README.md` |

## report（186）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [Task 6 Historical Artifact Consistency Report](../superpowers/reports/2026-08-13-task-6-historical-artifact-consistency.md) | historical | `docs/superpowers/reports/2026-08-13-task-6-historical-artifact-consistency.md` |
| [Prima OS × Framekernel 差距分析报告](../superpowers/reports/2026-08-16-framekernel-gap-analysis.md) | historical | `docs/superpowers/reports/2026-08-16-framekernel-gap-analysis.md` |
| [Capability 派生 / 位置 / 溯源建模缺口备忘](../superpowers/reports/2026-08-17-capability-derivation-model-gap.md) | historical | `docs/superpowers/reports/2026-08-17-capability-derivation-model-gap.md` |
| [S0-X1 修复独立复核报告：SPEC-056 九项生命周期修复的对抗式再验证](../superpowers/reports/2026-09-13-s0x1-fix-reverification.md) | historical | `docs/superpowers/reports/2026-09-13-s0x1-fix-reverification.md` |
| [S0-X1 独立规格审查报告：对象生命周期](../superpowers/reports/2026-09-13-s0x1-independent-lifecycle-review.md) | historical | `docs/superpowers/reports/2026-09-13-s0x1-independent-lifecycle-review.md` |
| [S0-X2 逐操作覆盖报告（修订后复核）](../superpowers/reports/2026-09-13-s0x2-operation-coverage.md) | historical | `docs/superpowers/reports/2026-09-13-s0x2-operation-coverage.md` |
| [SPEC-058 统一修订记录：对象生命周期 19 项 Open ISSUE](../superpowers/reports/2026-09-13-spec058-lifecycle-unification.md) | historical | `docs/superpowers/reports/2026-09-13-spec058-lifecycle-unification.md` |
| [切片 P 报告：Linux trace 与有界差分（`cow-write-fault`）](../superpowers/reports/2026-09-15-linux-trace-compat-report.md) | historical | `docs/superpowers/reports/2026-09-15-linux-trace-compat-report.md` |
| [切片 P 报告：阶段 1 退出条件——帧唯一归属与初始 Authority 物化](../superpowers/reports/2026-09-16-stage1-closure-report.md) | historical | `docs/superpowers/reports/2026-09-16-stage1-closure-report.md` |
| [切片 R 报告：bring-up 13 区域迁移为真实 Segment 对象](../superpowers/reports/2026-09-16-stage2-bringup-segment-migration-report.md) | historical | `docs/superpowers/reports/2026-09-16-stage2-bringup-segment-migration-report.md` |
| [切片 S 报告：IPC 用户化与两 ring3 任务共享 Segment](../superpowers/reports/2026-09-16-stage2-ipc-user-surface-report.md) | historical | `docs/superpowers/reports/2026-09-16-stage2-ipc-user-surface-report.md` |
| [切片 Q 报告：阶段 2 启动——句柄编码、Capability 生命周期、Segment 原语](../superpowers/reports/2026-09-16-stage2-opening-report.md) | historical | `docs/superpowers/reports/2026-09-16-stage2-opening-report.md` |
| [切片 U 报告：平台段 Capability 按属主移交（ISSUE-AUTH-002 剩余项）](../superpowers/reports/2026-09-17-stage2-platform-cap-handover-report.md) | historical | `docs/superpowers/reports/2026-09-17-stage2-platform-cap-handover-report.md` |
| [切片 T 报告：Task create/yield/destroy 用户原语与单核调度](../superpowers/reports/2026-09-17-stage2-task-lifecycle-report.md) | historical | `docs/superpowers/reports/2026-09-17-stage2-task-lifecycle-report.md` |
| [切片 V 报告：Task 自毁与延迟帧归还（ISSUE-TASK-002）](../superpowers/reports/2026-09-17-stage2-task-self-destroy-report.md) | historical | `docs/superpowers/reports/2026-09-17-stage2-task-self-destroy-report.md` |
| [切片 W 报告：image Segment 的真实逐帧向量（ISSUE-FRAME-001）](../superpowers/reports/2026-09-23-stage2-image-frame-vector-report.md) | historical | `docs/superpowers/reports/2026-09-23-stage2-image-frame-vector-report.md` |
| [切片 AD 报告：审计存储重构（SPEC-075）](../superpowers/reports/2026-09-24-stage2-audit-storage-refactor-report.md) | historical | `docs/superpowers/reports/2026-09-24-stage2-audit-storage-refactor-report.md` |
| [切片 AB 报告：§12.10 `exc_resume` 用户化（`KernelActor` 代调）](../superpowers/reports/2026-09-24-stage2-exc-resume-report.md) | historical | `docs/superpowers/reports/2026-09-24-stage2-exc-resume-report.md` |
| [切片 Y 报告：`TaskStatus::Faulted` 状态机与 §12.5 迁移入口（ISSUE-TASK-001）](../superpowers/reports/2026-09-24-stage2-faulted-state-machine-report.md) | historical | `docs/superpowers/reports/2026-09-24-stage2-faulted-state-machine-report.md` |
| [切片 AA 报告：§8.2 内核回收路径（`KernelActor`，无 `task_cap`）](../superpowers/reports/2026-09-24-stage2-kernel-reclaim-report.md) | historical | `docs/superpowers/reports/2026-09-24-stage2-kernel-reclaim-report.md` |
| [切片 AC 报告：内核回收判据收紧（SPEC-074）](../superpowers/reports/2026-09-24-stage2-reclaim-predicate-tightening-report.md) | historical | `docs/superpowers/reports/2026-09-24-stage2-reclaim-predicate-tightening-report.md` |
| [切片 Z 报告：调度路径的 `TaskStateChanged` 与审计容量（ISSUE-TASK-003）](../superpowers/reports/2026-09-24-stage2-scheduler-state-audit-report.md) | historical | `docs/superpowers/reports/2026-09-24-stage2-scheduler-state-audit-report.md` |
| [切片 X 报告：自建内核 CR3 与镜像逐页映射（ISSUE-AUTH-001）](../superpowers/reports/2026-09-24-stage2-self-built-cr3-report.md) | historical | `docs/superpowers/reports/2026-09-24-stage2-self-built-cr3-report.md` |
| [切片 AI 报告：收口与 TCB 量化（SPEC-080）](../superpowers/reports/2026-09-25-framekernel-closure-report.md) | historical | `docs/superpowers/reports/2026-09-25-framekernel-closure-report.md` |
| [切片 AH 报告：trap/syscall 入口回调反转 + IDT/GDT 下沉（SPEC-079）](../superpowers/reports/2026-09-25-framekernel-entry-inversion-report.md) | historical | `docs/superpowers/reports/2026-09-25-framekernel-entry-inversion-report.md` |
| [切片 AG 报告：内核全局状态 safe 化（SPEC-078）](../superpowers/reports/2026-09-25-framekernel-global-state-report.md) | historical | `docs/superpowers/reports/2026-09-25-framekernel-global-state-report.md` |
| [切片 AF 报告：物理内存与页表 POD 收敛（SPEC-077）](../superpowers/reports/2026-09-25-framekernel-mm-pod-report.md) | historical | `docs/superpowers/reports/2026-09-25-framekernel-mm-pod-report.md` |
| [切片 AE 报告：framekernel 分层与 unsafe 收敛（SPEC-076）](../superpowers/reports/2026-09-25-framekernel-skeleton-report.md) | historical | `docs/superpowers/reports/2026-09-25-framekernel-skeleton-report.md` |
| [切片 AL 报告：阶段 3 第三片 —— `audit(cap)` 审计查询（§13.2）](../superpowers/reports/2026-09-25-stage3-audit-trace-report.md) | historical | `docs/superpowers/reports/2026-09-25-stage3-audit-trace-report.md` |
| [切片 AJ 报告：阶段 3 第一片 —— Namespace 路径操作（§11 全量）](../superpowers/reports/2026-09-25-stage3-namespace-report.md) | historical | `docs/superpowers/reports/2026-09-25-stage3-namespace-report.md` |
| [切片 AK 报告：阶段 3 第二片 —— Timer（§10 全量）](../superpowers/reports/2026-09-25-stage3-timer-report.md) | historical | `docs/superpowers/reports/2026-09-25-stage3-timer-report.md` |
| [SPEC-084 报告：阶段 4 原生运行时 API 初始实现](../superpowers/reports/2026-09-25-stage4-runtime-api-prototype-report.md) | historical | `docs/superpowers/reports/2026-09-25-stage4-runtime-api-prototype-report.md` |
| [SPEC-083 实现恢复报告：`audit(cap)` 在 master 上落地（§13.2）](../superpowers/reports/2026-10-10-stage3-audit-trace-restore-report.md) | historical | `docs/superpowers/reports/2026-10-10-stage3-audit-trace-restore-report.md` |
| [SPEC-087 报告：init ↔ 存活 ring3 客户端双向握手](../superpowers/reports/2026-10-10-stage4-init-client-handshake-report.md) | historical | `docs/superpowers/reports/2026-10-10-stage4-init-client-handshake-report.md` |
| [SPEC-086 报告：阶段 4 期内的 mm/vspace 正确性与性能修复批次](../superpowers/reports/2026-10-10-stage4-mm-vspace-fixes-report.md) | historical | `docs/superpowers/reports/2026-10-10-stage4-mm-vspace-fixes-report.md` |
| [SPEC-085 报告：独立 ring3 原生客户端与 IPC Hello 握手](../superpowers/reports/2026-10-10-stage4-ring3-runtime-hello-report.md) | historical | `docs/superpowers/reports/2026-10-10-stage4-ring3-runtime-hello-report.md` |
| [SPEC-088 报告：服务端解析与协商（阶段 4 S4-D2 前置）](../superpowers/reports/2026-10-10-stage4-service-negotiation-report.md) | historical | `docs/superpowers/reports/2026-10-10-stage4-service-negotiation-report.md` |
| [SPEC-093 报告：独立第二服务端点（独立二进制映像 + 独立物理栈段）](../superpowers/reports/2026-10-11-stage4-independent-service-images-report.md) | historical | `docs/superpowers/reports/2026-10-11-stage4-independent-service-images-report.md` |
| [SPEC-089 报告：畸形句柄越界（阶段 4 缺陷修复，`ISSUE-CAP-001`）](../superpowers/reports/2026-10-11-stage4-malformed-handle-bounds-report.md) | historical | `docs/superpowers/reports/2026-10-11-stage4-malformed-handle-bounds-report.md` |
| [SPEC-092 报告：独立第二服务端点（init 按名路由到不同任务）](../superpowers/reports/2026-10-11-stage4-multi-endpoint-service-report.md) | historical | `docs/superpowers/reports/2026-10-11-stage4-multi-endpoint-service-report.md` |
| [SPEC-090 报告：服务管理器（init 按名解析并路由，阶段 4 S4-D2 前置）](../superpowers/reports/2026-10-11-stage4-service-manager-report.md) | historical | `docs/superpowers/reports/2026-10-11-stage4-service-manager-report.md` |
| [SPEC-091 报告：逻辑 vspace 存储移出 `BootstrapState`（arena 化）](../superpowers/reports/2026-10-11-stage4-vspace-arena-report.md) | historical | `docs/superpowers/reports/2026-10-11-stage4-vspace-arena-report.md` |
| [阶段 0（语义基线）代码评审报告](../superpowers/reports/2026-10-12-stage0-code-review.md) | historical | `docs/superpowers/reports/2026-10-12-stage0-code-review.md` |
| [SPEC-094 报告：`ALPHA` 端点独立栈段（闭合 SPEC-093 §5.1）](../superpowers/reports/2026-10-12-stage4-alpha-independent-stack-report.md) | historical | `docs/superpowers/reports/2026-10-12-stage4-alpha-independent-stack-report.md` |
| [Capability 槽位区间分离（`ISSUE-CAP-002`）](../superpowers/reports/2026-10-12-stage4-cap-slot-separation-report.md) | historical | `docs/superpowers/reports/2026-10-12-stage4-cap-slot-separation-report.md` |
| [SPEC-095 报告：运行期服务注册表（闭合 SPEC-093 §5.2）](../superpowers/reports/2026-10-12-stage4-runtime-service-registry-report.md) | historical | `docs/superpowers/reports/2026-10-12-stage4-runtime-service-registry-report.md` |
| [cosir 侧补能力派生树 `parent` 并建模 CDT（关 ISSUE-FORMAL-001）](../superpowers/reports/2026-10-13-cosir-cdt-parent-report.md) | historical | `docs/superpowers/reports/2026-10-13-cosir-cdt-parent-report.md` |
| [异常源销毁 `exc_source_destroy`（§12.6 补齐）](../superpowers/reports/2026-10-13-exc-source-destroy-report.md) | historical | `docs/superpowers/reports/2026-10-13-exc-source-destroy-report.md` |
| [IPC 携带 Capability 的位置变化审计（ISSUE-EXC-012）](../superpowers/reports/2026-10-13-ipc-capability-move-audit-report.md) | historical | `docs/superpowers/reports/2026-10-13-ipc-capability-move-audit-report.md` |
| [形式化推进（参考 seL4）：Lean 证成能力派生树的森林性（无环）](../superpowers/reports/2026-10-13-lean-cdt-forest-report.md) | historical | `docs/superpowers/reports/2026-10-13-lean-cdt-forest-report.md` |
| [形式化推进（参考 seL4）：Lean 模型的能力派生树（CDT）不变量与保持性证明](../superpowers/reports/2026-10-13-lean-cdt-invariant-report.md) | historical | `docs/superpowers/reports/2026-10-13-lean-cdt-invariant-report.md` |
| [阶段 0 / 内核 Audit 修复：bootstrap init Task 的创建事件（ISSUE-EXC-010）](../superpowers/reports/2026-10-13-stage0-bootstrap-taskcreated-report.md) | historical | `docs/superpowers/reports/2026-10-13-stage0-bootstrap-taskcreated-report.md` |
| [SPEC-101 报告：字符串服务名与端点名的运行期指派（闭合 SPEC-095 §5.1）](../superpowers/reports/2026-10-13-stage4-string-service-names-report.md) | historical | `docs/superpowers/reports/2026-10-13-stage4-string-service-names-report.md` |
| [Task Capability 审计与派生链（ISSUE-EXC-011）+ 审计存储扩容](../superpowers/reports/2026-10-13-task-create-capability-audit-report.md) | historical | `docs/superpowers/reports/2026-10-13-task-create-capability-audit-report.md` |
| [COSIR/POSIR 双人格扩展候选采纳审核](../superpowers/reports/2026-10-14-cosir-dual-personality-adoption-review.md) | historical | `docs/superpowers/reports/2026-10-14-cosir-dual-personality-adoption-review.md` |
| [COSIR/POSIR 双人格底座差距分析](../superpowers/reports/2026-10-14-cosir-dual-personality-gap-analysis.md) | historical | `docs/superpowers/reports/2026-10-14-cosir-dual-personality-gap-analysis.md` |
| [双人格 Holder 模型 H0 评审报告](../superpowers/reports/2026-10-14-dual-personality-holder-model-h0-review.md) | historical | `docs/superpowers/reports/2026-10-14-dual-personality-holder-model-h0-review.md` |
| [SPEC-107 报告：销毁路径 vspace 栈快照治本（滚动拆除取代栈上 VA 快照）](../superpowers/reports/2026-10-14-stage4-destroy-stack-snapshot-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-destroy-stack-snapshot-report.md` |
| [SPEC-111：Namespace 服务代理内核解析（S4-D3 收口）](../superpowers/reports/2026-10-14-stage4-namespace-service-proxy-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-namespace-service-proxy-report.md` |
| [SPEC-104（前置片）报告：对端 task id 的入口 ABI](../superpowers/reports/2026-10-14-stage4-peer-id-entry-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-peer-id-entry-report.md` |
| [SPEC-103 报告：ring3 服务管理器任务的引入（迁移第一片）](../superpowers/reports/2026-10-14-stage4-ring3-servicemgr-task-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-ring3-servicemgr-task-report.md` |
| [SPEC-109：单一服务映像段（代码段移交下沉到 ring3 服务管理器）](../superpowers/reports/2026-10-14-stage4-service-image-segment-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-service-image-segment-report.md` |
| [SPEC-105 报告：ring3 服务管理器**表驱动创建端点**](../superpowers/reports/2026-10-14-stage4-servicemgr-endpoint-creation-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-servicemgr-endpoint-creation-report.md` |
| [SPEC-104（主体）报告：注册表/按名解析/路由转发迁入 ring3 servicemgr](../superpowers/reports/2026-10-14-stage4-servicemgr-migration-body-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-servicemgr-migration-body-report.md` |
| [SPEC-108 报告：服务**栈段创建**下沉到 ring3 服务管理器](../superpowers/reports/2026-10-14-stage4-stack-segment-creation-sink-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-stack-segment-creation-sink-report.md` |
| [SPEC-110：有状态服务请求协议（三服务语义下沉）](../superpowers/reports/2026-10-14-stage4-stateful-service-protocol-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-stateful-service-protocol-report.md` |
| [SPEC-106 报告：S4-D3 三个基础服务（日志 / Namespace / 基础存储）接入 ring3 服务表](../superpowers/reports/2026-10-14-stage4-three-base-services-report.md) | historical | `docs/superpowers/reports/2026-10-14-stage4-three-base-services-report.md` |
| [SPEC-115 / EXO-1：通用 boot image（ramdisk）装载](../superpowers/reports/2026-10-15-exo1-boot-image-report.md) | historical | `docs/superpowers/reports/2026-10-15-exo1-boot-image-report.md` |
| [SPEC-116 / EXO-2A：无服务名的映像目录](../superpowers/reports/2026-10-15-exo2a-name-free-service-directory-report.md) | historical | `docs/superpowers/reports/2026-10-15-exo2a-name-free-service-directory-report.md` |
| [SPEC-117 / EXO-2B + EXO-4A：共享映像格式与设备可见撤销](../superpowers/reports/2026-10-15-exo2b-exo4a-report.md) | historical | `docs/superpowers/reports/2026-10-15-exo2b-exo4a-report.md` |
| [SPEC-118 / EXO-3A：内核不再解释服务名](../superpowers/reports/2026-10-15-exo3a-kernel-name-decoupling-report.md) | historical | `docs/superpowers/reports/2026-10-15-exo3a-kernel-name-decoupling-report.md` |
| [SPEC-119 / EXO-3B：witness 期望值移出内核](../superpowers/reports/2026-10-15-exo3b-witness-expectations-report.md) | historical | `docs/superpowers/reports/2026-10-15-exo3b-witness-expectations-report.md` |
| [DOC-008 / EXO-5：机制基线再评估（审查记录）](../superpowers/reports/2026-10-15-exo5-mechanism-baseline-review.md) | historical | `docs/superpowers/reports/2026-10-15-exo5-mechanism-baseline-review.md` |
| [SPEC-113：设备 `IOPortRange` 对象与受控 I/O 端口路径（S4-D4 内核侧）](../superpowers/reports/2026-10-15-stage4-device-ioport-capability-report.md) | historical | `docs/superpowers/reports/2026-10-15-stage4-device-ioport-capability-report.md` |
| [SPEC-112：日志/存储服务持内核 MemorySegment 落数据（S4-D3 收口）](../superpowers/reports/2026-10-15-stage4-service-storage-segment-report.md) | historical | `docs/superpowers/reports/2026-10-15-stage4-service-storage-segment-report.md` |
| [SPEC-114：第一个 ring3 用户态驱动（`prima-driver`，S4-D4 第二步）](../superpowers/reports/2026-10-15-stage4-user-driver-ioport-report.md) | historical | `docs/superpowers/reports/2026-10-15-stage4-user-driver-ioport-report.md` |
| [SPEC-120 / EXO-2C：init 机器码迁移到 ramdisk 的 ring3 opaque ELF](../superpowers/reports/2026-10-16-exo2c-init-loader-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo2c-init-loader-report.md` |
| [SPEC-121 / EXO-2D：通用 entry ABI + 用户态 manifest](../superpowers/reports/2026-10-16-exo2d-manifest-boot-abi-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo2d-manifest-boot-abi-report.md` |
| [SPEC-122 / EXO-3C：协议 tag 不再进入内核 witness](../superpowers/reports/2026-10-16-exo3c-protocol-tag-decoupling-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo3c-protocol-tag-decoupling-report.md` |
| [SPEC-123 / EXO-3D：通用 trace 通道 + `runtime_hello_ready` 迁移](../superpowers/reports/2026-10-16-exo3d-generic-trace-runtime-hello-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo3d-generic-trace-runtime-hello-report.md` |
| [SPEC-124 / EXO-3E：`service_reply_ready` 迁移到通用 trace](../superpowers/reports/2026-10-16-exo3e-service-reply-trace-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo3e-service-reply-trace-report.md` |
| [SPEC-125 / EXO-3F：`service_negotiated_ready` 迁移到通用 trace](../superpowers/reports/2026-10-16-exo3f-service-negotiated-trace-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo3f-service-negotiated-trace-report.md` |
| [SPEC-126 / EXO-3G：`service_selected_ready` 迁移到通用 trace](../superpowers/reports/2026-10-16-exo3g-service-selected-trace-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo3g-service-selected-trace-report.md` |
| [SPEC-127 / EXO-3H：`servicemgr_ready` 迁移到通用 trace](../superpowers/reports/2026-10-16-exo3h-servicemgr-trace-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo3h-servicemgr-trace-report.md` |
| [SPEC-128 / EXO-3I：原始 trace 调试通道](../superpowers/reports/2026-10-16-exo3i-raw-trace-debug-channel-report.md) | historical | `docs/superpowers/reports/2026-10-16-exo3i-raw-trace-debug-channel-report.md` |
| [SPEC-130：阶段 4 原生应用、最小权限与故障隔离收口](../superpowers/reports/2026-10-16-stage4-closure-report.md) | historical | `docs/superpowers/reports/2026-10-16-stage4-closure-report.md` |
| [SPEC-129：原生应用的消息控制与共享 Segment 数据路径](../superpowers/reports/2026-10-16-stage4-native-shared-segment-app-report.md) | historical | `docs/superpowers/reports/2026-10-16-stage4-native-shared-segment-app-report.md` |
| [SPEC-133 / EXO-3J：原始 trace 进入 QEMU 验收](../superpowers/reports/2026-10-17-exo3j-raw-trace-validation-report.md) | historical | `docs/superpowers/reports/2026-10-17-exo3j-raw-trace-validation-report.md` |
| [SPEC-135 / EXO-3K：共享页原始 trace 字节流](../superpowers/reports/2026-10-17-exo3k-shared-trace-bytes-report.md) | historical | `docs/superpowers/reports/2026-10-17-exo3k-shared-trace-bytes-report.md` |
| [SPEC-136 / EXO-3L：共享 trace 成为唯一记录通道](../superpowers/reports/2026-10-17-exo3l-shared-trace-only-report.md) | historical | `docs/superpowers/reports/2026-10-17-exo3l-shared-trace-only-report.md` |
| [SPEC-138 / EXO-3M：内核服务 witness 组装退役](../superpowers/reports/2026-10-17-exo3m-kernel-service-witness-retirement-report.md) | historical | `docs/superpowers/reports/2026-10-17-exo3m-kernel-service-witness-retirement-report.md` |
| [Prima OS 内核原语对照评估](../superpowers/reports/2026-10-17-kernel-primitives-comparative-review.md) | historical | `docs/superpowers/reports/2026-10-17-kernel-primitives-comparative-review.md` |
| [SPEC-134：S0-X2 逐操作覆盖报告（DOC-007 增量复核）](../superpowers/reports/2026-10-17-s0x2-coverage-increment-doc007.md) | historical | `docs/superpowers/reports/2026-10-17-s0x2-coverage-increment-doc007.md` |
| [阶段 0–4 缺口复核包](../superpowers/reports/2026-10-17-stage0-4-gap-audit.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-4-gap-audit.md` |
| [SPEC-145：阶段 0 冻结基线复核与签署](../superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md` |
| [SPEC-139：组织级阶段 0 审查发现项整改（9 项）](../superpowers/reports/2026-10-17-stage0-organizational-review-remediation.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-organizational-review-remediation.md` |
| [SPEC-144：SPEC-143 复核残修（3 项）](../superpowers/reports/2026-10-17-stage0-re-review-spec144.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-re-review-spec144.md` |
| [SPEC-140：阶段 0 对抗式复核发现项整改（12 项）](../superpowers/reports/2026-10-17-stage0-review-remediation-spec140.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-review-remediation-spec140.md` |
| [SPEC-141：阶段 0 第二轮组织级复核发现项整改（11 项）](../superpowers/reports/2026-10-17-stage0-review-remediation-spec141.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-review-remediation-spec141.md` |
| [SPEC-143：阶段 0 第三方组织级复核发现项整改（29 项）](../superpowers/reports/2026-10-17-stage0-review-remediation-spec143.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-review-remediation-spec143.md` |
| [SPEC-137：阶段 0 范畴外复核发现项整改](../superpowers/reports/2026-10-17-stage0-third-party-review-remediation.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-third-party-review-remediation.md` |
| [阶段 0 第三方组织级复核结论（未通过）](../superpowers/reports/2026-10-17-stage0-thirdparty-organizational-review-verdict.md) | historical | `docs/superpowers/reports/2026-10-17-stage0-thirdparty-organizational-review-verdict.md` |
| [SPEC-146：阶段 1 退出条件复核与签署](../superpowers/reports/2026-10-17-stage1-exit-review-spec146.md) | historical | `docs/superpowers/reports/2026-10-17-stage1-exit-review-spec146.md` |
| [SPEC-150：阶段 1 补充复核与整改](../superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md) | historical | `docs/superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md` |
| [SPEC-147：阶段 2 退出条件复核与签署](../superpowers/reports/2026-10-17-stage2-exit-review-spec147.md) | historical | `docs/superpowers/reports/2026-10-17-stage2-exit-review-spec147.md` |
| [SPEC-148：阶段 3 退出条件复核与签署](../superpowers/reports/2026-10-17-stage3-exit-review-spec148.md) | historical | `docs/superpowers/reports/2026-10-17-stage3-exit-review-spec148.md` |
| [SPEC-149：阶段 4 退出条件复核与签署](../superpowers/reports/2026-10-17-stage4-exit-review-spec149.md) | historical | `docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md` |
| [SPEC-131：init 自检读取动态 Capability 句柄](../superpowers/reports/2026-10-17-stage4-init-dynamic-handles-report.md) | historical | `docs/superpowers/reports/2026-10-17-stage4-init-dynamic-handles-report.md` |
| [SPEC-132：从 manifest 顺序确定 init 服务索引](../superpowers/reports/2026-10-17-stage4-manifest-index-report.md) | historical | `docs/superpowers/reports/2026-10-17-stage4-manifest-index-report.md` |
| [阶段 1–4 实现复核（阶段 5 输入）](../superpowers/reports/2026-10-18-stage1-4-implementation-review.md) | historical | `docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md` |
| [SPEC-151：阶段 1 遗留 Open ISSUE 整改（AUTH-003/004、AUDIT-007、CAP-004）](../superpowers/reports/2026-10-18-stage1-open-issues-spec151.md) | historical | `docs/superpowers/reports/2026-10-18-stage1-open-issues-spec151.md` |
| [阶段 2 实现复核与 IPC 整改](../superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md) | historical | `docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md` |
| [阶段 3 实现复核与整改（2026-10-18）](../superpowers/reports/2026-10-18-stage3-review-remediation.md) | historical | `docs/superpowers/reports/2026-10-18-stage3-review-remediation.md` |
| [阶段 5：首个 C ELF 的服务后 QEMU 见证](../superpowers/reports/2026-10-18-stage5-c-elf-qemu.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-c-elf-qemu.md` |
| [阶段 5：C Task 初始栈见证](../superpowers/reports/2026-10-18-stage5-c-start-stack.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-c-start-stack.md` |
| [阶段 5 片 C：tty 字节流、`write(1/2)` 与同源 `_exit` 见证](../superpowers/reports/2026-10-18-stage5-c-tty-write.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-c-tty-write.md` |
| [阶段 5：服务后静态容量水位](../superpowers/reports/2026-10-18-stage5-capacity-watermarks.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-capacity-watermarks.md` |
| [阶段 5 准入分析：POSIX 源码兼容](../superpowers/reports/2026-10-18-stage5-entry-analysis.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-entry-analysis.md` |
| [阶段 5 启动：首个新 Task 容量与 profile 草案](../superpowers/reports/2026-10-18-stage5-kickoff.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-kickoff.md` |
| [阶段 5：同源 C `main` 返回码见证](../superpowers/reports/2026-10-18-stage5-main-return-status.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-main-return-status.md` |
| [阶段 5：musl M0 依赖盘点](../superpowers/reports/2026-10-18-stage5-musl-m0-inventory.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-musl-m0-inventory.md` |
| [阶段 5 · musl M2：C 系统边界与 per-task FS base](../superpowers/reports/2026-10-18-stage5-musl-m2-boundary.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-musl-m2-boundary.md` |
| [阶段 5 · musl M3：SSE/FPU 契约与静态 musl 基座](../superpowers/reports/2026-10-18-stage5-musl-m3-base.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-musl-m3-base.md` |
| [阶段 5 / M4：文件与 fd 生命周期（首切片）](../superpowers/reports/2026-10-18-stage5-musl-m4-file.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-musl-m4-file.md` |
| [阶段 5：原生 ABI 版本握手见证](../superpowers/reports/2026-10-18-stage5-native-abi-check.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-native-abi-check.md` |
| [阶段 5：`PT_LOAD` 写段/BSS 私有装载与 W^X](../superpowers/reports/2026-10-18-stage5-ptload-wx.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-ptload-wx.md` |
| [阶段 5：服务后 Task 16 的 QEMU 见证](../superpowers/reports/2026-10-18-stage5-task16-qemu.md) | historical | `docs/superpowers/reports/2026-10-18-stage5-task16-qemu.md` |
| [阶段 5 · M5 首切片：sbase `echo` E1–E6](../superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md) | historical | `docs/superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md` |
| [阶段 5 M6：POSIX 时间首切片（clock_gettime / nanosleep）](../superpowers/reports/2026-10-19-stage5-m6-time.md) | historical | `docs/superpowers/reports/2026-10-19-stage5-m6-time.md` |
| [阶段 5 M7：完整本地 fd 表、`dup` 与 `getpid`](../superpowers/reports/2026-10-19-stage5-m7-fd-table.md) | historical | `docs/superpowers/reports/2026-10-19-stage5-m7-fd-table.md` |
| [阶段 5 / M8：用户态 VFS/rootfs、通用 fd 与多级路径](../superpowers/reports/2026-10-20-stage5-m8-user-vfs.md) | historical | `docs/superpowers/reports/2026-10-20-stage5-m8-user-vfs.md` |
| [阶段 5 / M10：匿名内存与保护（`mmap`/`munmap`/`mprotect` + 内核 `memseg_protect`）](../superpowers/reports/2026-10-21-stage5-m10-anon-memory.md) | historical | `docs/superpowers/reports/2026-10-21-stage5-m10-anon-memory.md` |
| [阶段 5 / M9：`statx` 元数据、`chdir`/`getcwd` 与 sbase `cat`/`ls`](../superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md) | historical | `docs/superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md` |
| [阶段 5 / M11：文件树变更（`mkdir`/`rmdir`/`unlink`）](../superpowers/reports/2026-10-22-stage5-m11-file-mutation.md) | historical | `docs/superpowers/reports/2026-10-22-stage5-m11-file-mutation.md` |
| [阶段 5 / M12：C 启动栈与退出清理（M12a/M12b）](../superpowers/reports/2026-10-22-stage5-m12-c-startup.md) | historical | `docs/superpowers/reports/2026-10-22-stage5-m12-c-startup.md` |
| [阶段 5 / M13a：符号链接、rename 与 fchdir 实现报告](../superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md) | historical | `docs/superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md` |
| [阶段 5 / M13b：权限位、umask 与 stdio 文件 API 报告](../superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md) | historical | `docs/superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md` |
| [文档体系重构与验收报告](../superpowers/reports/2026-10-23-documentation-restructure-report.md) | historical | `docs/superpowers/reports/2026-10-23-documentation-restructure-report.md` |
| [阶段 5 / M14：tty 输入与最小 console 实现报告](../superpowers/reports/2026-10-23-stage5-m14-tty-input.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m14-tty-input.md` |
| [阶段 5 / M15a-1：WaitObject 无阻塞切片实现报告](../superpowers/reports/2026-10-23-stage5-m15a-waitobject.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m15a-waitobject.md` |
| [阶段 5 / M15a-2：WaitObject 阻塞/唤醒/超时实现报告](../superpowers/reports/2026-10-23-stage5-m15a2-waitobject-blocking.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m15a2-waitobject-blocking.md` |
| [阶段 5 / M15b-1：Process/Thread 元数据实现报告](../superpowers/reports/2026-10-23-stage5-m15b1-process-metadata.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m15b1-process-metadata.md` |
| [阶段 5 / M15b-2a：共享能力空间的线程创建实现报告](../superpowers/reports/2026-10-23-stage5-m15b2a-thread-create.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m15b2a-thread-create.md` |
| [阶段 5 / M15b-2b：线程槽位回收与 W2 端到端报告](../superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md` |
| [阶段 5 / M16a：futex WAIT/WAKE 子集实现报告](../superpowers/reports/2026-10-23-stage5-m16a-futex.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m16a-futex.md` |
| [阶段 5 / M16b-1：clone/线程退出的内核基础与 musl 阻塞点](../superpowers/reports/2026-10-23-stage5-m16b1-clone-basis.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m16b1-clone-basis.md` |
| [阶段 5 / M16b-2：musl clone 路由与 pthread 进展](../superpowers/reports/2026-10-23-stage5-m16b2-musl-clone.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m16b2-musl-clone.md` |
| [阶段 5 / M16b-3：pthread 创建/join 打通与 clear_child_tid](../superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md` |
| [阶段 5 / M17b-1：wait4 状态机模型实现报告](../superpowers/reports/2026-10-23-stage5-m17b1-wait4-model.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17b1-wait4-model.md` |
| [阶段 5 / M17b-2b：general COW 与 fork syscall（W6 部分通过）](../superpowers/reports/2026-10-23-stage5-m17b2-cow-fork.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17b2-cow-fork.md` |
| [阶段 5 / M17b-2a：COW 页表原语实现报告](../superpowers/reports/2026-10-23-stage5-m17b2-cow-primitives.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17b2-cow-primitives.md` |
| [阶段 5 / M17b-3：fork 子进程僵尸保留与唤醒回收](../superpowers/reports/2026-10-23-stage5-m17b3-zombie-wait.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17b3-zombie-wait.md` |
| [阶段 5 / M17b-4：fork/wait 打通（W6）](../superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md` |
| [阶段 5 / M17c-1：静态 ELF 解析与初始栈（host 证据）](../superpowers/reports/2026-10-23-stage5-m17c1-elf-plan.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17c1-elf-plan.md` |
| [阶段 5 / M17c-3：exec_replace 打通（W7）](../superpowers/reports/2026-10-23-stage5-m17c3-exec.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17c3-exec.md` |
| [阶段 5 / M17c-4：VFS 镜像 → fork → exec_replace（W7b）](../superpowers/reports/2026-10-23-stage5-m17c4-execve-vfs.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m17c4-execve-vfs.md` |
| [阶段 5 / M18a：最小信号（kill 默认终止）与 wait4 信号状态](../superpowers/reports/2026-10-23-stage5-m18a-signal-kill.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m18a-signal-kill.md` |
| [阶段 5 / M18b：sigaction + handler 递送 + sigreturn](../superpowers/reports/2026-10-23-stage5-m18b-sigaction.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m18b-sigaction.md` |
| [阶段 5 / M19：用户态 loopback IPv4/UDP 子集实现报告](../superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md` |
| [阶段 5 / M20：兼容矩阵定稿](../superpowers/reports/2026-10-23-stage5-m20-compat-final.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md` |
| [阶段 5 / M21：出口评审（未签署）](../superpowers/reports/2026-10-23-stage5-m21-exit-review.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md` |
| [阶段 5 / ISSUE-THREAD-CAPACITY-001 调查记录](../superpowers/reports/2026-10-23-stage5-thread-capacity-investigation.md) | historical | `docs/superpowers/reports/2026-10-23-stage5-thread-capacity-investigation.md` |
| [VFS / LibOS 基础重构验证报告](../superpowers/reports/2026-10-23-vfs-libos-foundation-report.md) | historical | `docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md` |
| [Asterinas 参考续篇：一次初始化与启动发布验证](../superpowers/reports/2026-10-24-asterinas-once-publication-report.md) | historical | `docs/superpowers/reports/2026-10-24-asterinas-once-publication-report.md` |
| [Asterinas 参考续篇：POD 与恢复帧缓冲验证](../superpowers/reports/2026-10-24-asterinas-pod-buffer-report.md) | historical | `docs/superpowers/reports/2026-10-24-asterinas-pod-buffer-report.md` |
| [Asterinas 复用与 OSTD 临界区验证报告](../superpowers/reports/2026-10-24-asterinas-reuse-report.md) | historical | `docs/superpowers/reports/2026-10-24-asterinas-reuse-report.md` |
| [文档生命周期治理与过时材料清理报告](../superpowers/reports/2026-10-24-documentation-lifecycle-report.md) | historical | `docs/superpowers/reports/2026-10-24-documentation-lifecycle-report.md` |
| [fork 续执行与进程边界整改验证](../superpowers/reports/2026-10-24-fork-continuation-report.md) | historical | `docs/superpowers/reports/2026-10-24-fork-continuation-report.md` |
| [内核句柄、权限与 Lean 模型补强](../superpowers/reports/2026-10-24-kernel-handle-model-report.md) | historical | `docs/superpowers/reports/2026-10-24-kernel-handle-model-report.md` |
| [musl 移植补强：调用边界与 fd/stdio 验证](../superpowers/reports/2026-10-24-musl-port-boundary-report.md) | historical | `docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md` |
| [原生运行时与服务重构验证](../superpowers/reports/2026-10-24-native-runtime-service-report.md) | historical | `docs/superpowers/reports/2026-10-24-native-runtime-service-report.md` |
| [阶段 0–5 缺口后继工作记录与统一验收](../superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | historical | `docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md` |
| [阶段 5 缺口补强验证记录](../superpowers/reports/2026-10-25-stage5-gap-closure-report.md) | historical | `docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md` |
| [实现质量独立审查（2026-10-27）](../superpowers/reports/2026-10-27-implementation-quality-review.md) | historical | `docs/superpowers/reports/2026-10-27-implementation-quality-review.md` |
| [POSIX std 后端实现与验证](../superpowers/reports/2026-10-27-posix-std-backend-report.md) | historical | `docs/superpowers/reports/2026-10-27-posix-std-backend-report.md` |
| [POSIX std 后继：文件I/O与IPC接收归账](../superpowers/reports/2026-10-27-posix-std-file-io-report.md) | historical | `docs/superpowers/reports/2026-10-27-posix-std-file-io-report.md` |
| [阶段 5 出口签署（SPEC-152）](../superpowers/reports/2026-10-27-stage5-exit-signoff.md) | current | `docs/superpowers/reports/2026-10-27-stage5-exit-signoff.md` |
| [阶段 6 准备性切片：静态 ELF 装载一致性与初始 auxv](../superpowers/reports/2026-10-28-stage6-loader-slice.md) | current | `docs/superpowers/reports/2026-10-28-stage6-loader-slice.md` |
| [S6-M1 当前评审阻塞整改、COSIR 技术评审与 7-A 有界探索](../superpowers/reports/2026-10-29-stage6-m1-blocker-closure.md) | historical | `docs/superpowers/reports/2026-10-29-stage6-m1-blocker-closure.md` |
| [S6-M1 六项决定复审：保留最小路线，纠正安全与活性主张](../superpowers/reports/2026-10-29-stage6-m1-decision-review.md) | historical | `docs/superpowers/reports/2026-10-29-stage6-m1-decision-review.md` |
| [阶段 6 启动与首个 S6-M2 增量：确定性 `AT_RANDOM`](../superpowers/reports/2026-10-29-stage6-start.md) | current | `docs/superpowers/reports/2026-10-29-stage6-start.md` |
| [S6-M1 审计耗尽：首次诊断与全局停止控制器](../superpowers/reports/2026-10-30-stage6-audit-fail-stop.md) | historical | `docs/superpowers/reports/2026-10-30-stage6-audit-fail-stop.md` |
| [S6-M1 审计停止控制器：裸机耗尽与停止边界验收](../superpowers/reports/2026-10-30-stage6-audit-stop-qemu.md) | historical | `docs/superpowers/reports/2026-10-30-stage6-audit-stop-qemu.md` |
| [S6-M1 内核私有等待：登记、信号隔离与代次完成](../superpowers/reports/2026-10-30-stage6-private-completion-waits.md) | historical | `docs/superpowers/reports/2026-10-30-stage6-private-completion-waits.md` |
| [S6-M1 接线前置：真实页表输出预检与 COW 事务](../superpowers/reports/2026-10-30-stage6-user-copy-transactions.md) | historical | `docs/superpowers/reports/2026-10-30-stage6-user-copy-transactions.md` |
| [阶段 1～5 复核与整改](../superpowers/reports/2026-10-31-stage1-5-regression-remediation.md) | historical | `docs/superpowers/reports/2026-10-31-stage1-5-regression-remediation.md` |

## spec（4）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [VFS 打开对象与 LibOS 适配边界](../superpowers/specs/2026-10-23-vfs-libos-foundation-design.md) | current | `docs/superpowers/specs/2026-10-23-vfs-libos-foundation-design.md` |
| [内核句柄耗尽与 Lean 迁移模型](../superpowers/specs/2026-10-24-kernel-handle-model-design.md) | current | `docs/superpowers/specs/2026-10-24-kernel-handle-model-design.md` |
| [musl 静态移植：调用边界、fd 控制与可重放构建](../superpowers/specs/2026-10-24-musl-port-boundary-design.md) | current | `docs/superpowers/specs/2026-10-24-musl-port-boundary-design.md` |
| [原生运行时与服务：握手和分片事务边界](../superpowers/specs/2026-10-24-native-runtime-service-design.md) | current | `docs/superpowers/specs/2026-10-24-native-runtime-service-design.md` |

## specification（85）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [COSIR 原语规范：平台无关抽象语义](../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md) | current | `COSIR 原语规范.md` |
| [Prima OS 阶段 1 启动闭环设计](../superpowers/specs/2026-08-04-stage1-boot-slice-design.md) | unclassified | `docs/superpowers/specs/2026-08-04-stage1-boot-slice-design.md` |
| [BLK-002 细化草案：x86_64 MachineState → POSIR State](../superpowers/specs/2026-08-07-blk002-refinement-design.md) | unclassified | `docs/superpowers/specs/2026-08-07-blk002-refinement-design.md` |
| [exception 恢复细化：exc_resume 原语设计](../superpowers/specs/2026-08-07-exc-resume-design.md) | unclassified | `docs/superpowers/specs/2026-08-07-exc-resume-design.md` |
| [BLK-002 细化续：MemorySegment 页表 ↔ vspace 细化](../superpowers/specs/2026-08-12-memseg-pagewalk-refinement-design.md) | deprecated | `docs/superpowers/specs/2026-08-12-memseg-pagewalk-refinement-design.md` |
| [PrimaOS 细化审查纠偏设计](../superpowers/specs/2026-08-12-refinement-audit-remediation-design.md) | unclassified | `docs/superpowers/specs/2026-08-12-refinement-audit-remediation-design.md` |
| [COW fork 组合协议细化：cow_refine 验证层设计](../superpowers/specs/2026-08-14-cow-refinement-design.md) | unclassified | `docs/superpowers/specs/2026-08-14-cow-refinement-design.md` |
| [TLB 细化（tlb_refine）设计](../superpowers/specs/2026-08-15-tlb-refine-design.md) | unclassified | `docs/superpowers/specs/2026-08-15-tlb-refine-design.md` |
| [PrimaOS 审计缺陷修复设计](../superpowers/specs/2026-08-16-primaos-audit-remediation-design.md) | unclassified | `docs/superpowers/specs/2026-08-16-primaos-audit-remediation-design.md` |
| [页表/GDT/IDT/trap 框架设计（S1-D4，切片 B）](../superpowers/specs/2026-08-17-page-gdt-idt-trap-design.md) | unclassified | `docs/superpowers/specs/2026-08-17-page-gdt-idt-trap-design.md` |
| [物理内存发现（physmem discovery）设计](../superpowers/specs/2026-08-17-physmem-discovery-design.md) | unclassified | `docs/superpowers/specs/2026-08-17-physmem-discovery-design.md` |
| [BLK-001 形式化模型工件：Lean 4 可执行模型 + cosir 对拍](../superpowers/specs/2026-08-18-blk001-lean-formal-model-design.md) | unclassified | `docs/superpowers/specs/2026-08-18-blk001-lean-formal-model-design.md` |
| [切片 C(续)设计：#PF error-code 捕获与写保护语义分类](../superpowers/specs/2026-09-05-pf-error-code-semantics-design.md) | unclassified | `docs/superpowers/specs/2026-09-05-pf-error-code-semantics-design.md` |
| [切片 P 设计：Linux trace 与有界 COMPAT-TEST（`cow-write-fault`）](../superpowers/specs/2026-09-15-linux-trace-compat-design.md) | unclassified | `docs/superpowers/specs/2026-09-15-linux-trace-compat-design.md` |
| [切片 P 设计：阶段 1 退出条件——帧唯一归属与初始 Authority 物化](../superpowers/specs/2026-09-16-stage1-closure-design.md) | unclassified | `docs/superpowers/specs/2026-09-16-stage1-closure-design.md` |
| [切片 R 设计：bring-up 13 区域迁移为真实 Segment 对象](../superpowers/specs/2026-09-16-stage2-bringup-segment-migration-design.md) | unclassified | `docs/superpowers/specs/2026-09-16-stage2-bringup-segment-migration-design.md` |
| [切片 Q 设计：阶段 2 启动——句柄编码、Capability 生命周期、Segment 原语](../superpowers/specs/2026-09-16-stage2-handles-caps-segments-design.md) | unclassified | `docs/superpowers/specs/2026-09-16-stage2-handles-caps-segments-design.md` |
| [切片 S 设计：IPC 用户化与两 ring3 任务共享 Segment](../superpowers/specs/2026-09-16-stage2-ipc-user-surface-design.md) | unclassified | `docs/superpowers/specs/2026-09-16-stage2-ipc-user-surface-design.md` |
| [切片 U 设计：平台段 Capability 按属主移交（ISSUE-AUTH-002 剩余项）](../superpowers/specs/2026-09-17-stage2-platform-cap-handover-design.md) | unclassified | `docs/superpowers/specs/2026-09-17-stage2-platform-cap-handover-design.md` |
| [切片 T 设计：Task create/yield/destroy 用户原语与单核调度（S2-D4）](../superpowers/specs/2026-09-17-stage2-task-lifecycle-design.md) | unclassified | `docs/superpowers/specs/2026-09-17-stage2-task-lifecycle-design.md` |
| [切片 V 设计：Task 自毁与延迟帧归还（ISSUE-TASK-002）](../superpowers/specs/2026-09-17-stage2-task-self-destroy-design.md) | unclassified | `docs/superpowers/specs/2026-09-17-stage2-task-self-destroy-design.md` |
| [切片 W 设计：image Segment 的真实逐帧向量（ISSUE-FRAME-001）](../superpowers/specs/2026-09-23-stage2-image-frame-vector-design.md) | unclassified | `docs/superpowers/specs/2026-09-23-stage2-image-frame-vector-design.md` |
| [切片 AD 设计：审计存储重构（SPEC-075）](../superpowers/specs/2026-09-24-stage2-audit-storage-refactor-design.md) | unclassified | `docs/superpowers/specs/2026-09-24-stage2-audit-storage-refactor-design.md` |
| [切片 AB 设计：§12.10 `exc_resume` 用户化（`KernelActor` 代调）](../superpowers/specs/2026-09-24-stage2-exc-resume-design.md) | unclassified | `docs/superpowers/specs/2026-09-24-stage2-exc-resume-design.md` |
| [切片 Y 设计：`TaskStatus::Faulted` 状态机与 §12.5 迁移入口（ISSUE-TASK-001）](../superpowers/specs/2026-09-24-stage2-faulted-state-machine-design.md) | unclassified | `docs/superpowers/specs/2026-09-24-stage2-faulted-state-machine-design.md` |
| [切片 AA 设计：§8.2 内核回收路径（`KernelActor`，无 `task_cap`）](../superpowers/specs/2026-09-24-stage2-kernel-reclaim-design.md) | unclassified | `docs/superpowers/specs/2026-09-24-stage2-kernel-reclaim-design.md` |
| [切片 AC 设计：内核回收判据收紧（SPEC-074）](../superpowers/specs/2026-09-24-stage2-reclaim-predicate-tightening-design.md) | unclassified | `docs/superpowers/specs/2026-09-24-stage2-reclaim-predicate-tightening-design.md` |
| [切片 Z 设计：调度路径的 `TaskStateChanged` 与审计容量（ISSUE-TASK-003）](../superpowers/specs/2026-09-24-stage2-scheduler-state-audit-design.md) | unclassified | `docs/superpowers/specs/2026-09-24-stage2-scheduler-state-audit-design.md` |
| [切片 X 设计：自建内核 CR3 与镜像逐页映射（ISSUE-AUTH-001）](../superpowers/specs/2026-09-24-stage2-self-built-cr3-design.md) | unclassified | `docs/superpowers/specs/2026-09-24-stage2-self-built-cr3-design.md` |
| [切片 AI 设计：收口与 TCB 量化（SPEC-080）](../superpowers/specs/2026-09-25-framekernel-closure-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-framekernel-closure-design.md` |
| [切片 AH 设计：trap/syscall 入口回调反转 + IDT/GDT 下沉（SPEC-079）](../superpowers/specs/2026-09-25-framekernel-entry-inversion-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-framekernel-entry-inversion-design.md` |
| [切片 AG 设计：内核全局状态 safe 化（SPEC-078）](../superpowers/specs/2026-09-25-framekernel-global-state-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-framekernel-global-state-design.md` |
| [切片 AF 设计：物理内存与页表 POD 收敛（SPEC-077）](../superpowers/specs/2026-09-25-framekernel-mm-pod-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-framekernel-mm-pod-design.md` |
| [切片 AE 设计：framekernel 分层与 unsafe 收敛（SPEC-076）](../superpowers/specs/2026-09-25-framekernel-skeleton-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-framekernel-skeleton-design.md` |
| [切片 AL 设计：阶段 3 第三片 —— `audit(cap)` 审计查询（§13.2）](../superpowers/specs/2026-09-25-stage3-audit-trace-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-stage3-audit-trace-design.md` |
| [切片 AJ 设计：阶段 3 第一片 —— Namespace 路径操作（§11 全量）](../superpowers/specs/2026-09-25-stage3-namespace-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-stage3-namespace-design.md` |
| [切片 AK 设计：阶段 3 第二片 —— Timer（§10 全量）](../superpowers/specs/2026-09-25-stage3-timer-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-stage3-timer-design.md` |
| [SPEC-084 设计：阶段 4 Prima 原生运行时 API 与版本规则](../superpowers/specs/2026-09-25-stage4-runtime-api-design.md) | unclassified | `docs/superpowers/specs/2026-09-25-stage4-runtime-api-design.md` |
| [SPEC-087 设计：init ↔ 存活 ring3 客户端双向握手](../superpowers/specs/2026-10-10-stage4-init-client-handshake-design.md) | unclassified | `docs/superpowers/specs/2026-10-10-stage4-init-client-handshake-design.md` |
| [SPEC-085 设计：阶段 4 独立 ring3 原生客户端与 IPC Hello 握手](../superpowers/specs/2026-10-10-stage4-ring3-runtime-hello-design.md) | unclassified | `docs/superpowers/specs/2026-10-10-stage4-ring3-runtime-hello-design.md` |
| [SPEC-088 设计：服务端解析与协商（阶段 4 S4-D2 前置）](../superpowers/specs/2026-10-10-stage4-service-negotiation-design.md) | unclassified | `docs/superpowers/specs/2026-10-10-stage4-service-negotiation-design.md` |
| [RuxOS 基础库栈（基座库）技术规格书 v1.1](../superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md) | historical | `docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md` |
| [RuxOS 基础库栈 — 走查与扫雷审查记录](../superpowers/specs/2026-10-11-baselib-stack-walkthrough.md) | historical | `docs/superpowers/specs/2026-10-11-baselib-stack-walkthrough.md` |
| [SPEC-093 设计：独立第二服务端点（独立二进制映像 + 独立物理栈段）](../superpowers/specs/2026-10-11-stage4-independent-service-images-design.md) | unclassified | `docs/superpowers/specs/2026-10-11-stage4-independent-service-images-design.md` |
| [SPEC-092 设计：独立第二服务端点（init 按名路由到不同任务）](../superpowers/specs/2026-10-11-stage4-multi-endpoint-service-design.md) | unclassified | `docs/superpowers/specs/2026-10-11-stage4-multi-endpoint-service-design.md` |
| [SPEC-090 设计：服务管理器（init 按名解析并路由）](../superpowers/specs/2026-10-11-stage4-service-manager-design.md) | unclassified | `docs/superpowers/specs/2026-10-11-stage4-service-manager-design.md` |
| [SPEC-091 设计：逻辑 vspace 存储移出 `BootstrapState`（arena 化）](../superpowers/specs/2026-10-11-stage4-vspace-arena-design.md) | unclassified | `docs/superpowers/specs/2026-10-11-stage4-vspace-arena-design.md` |
| [SPEC-094 设计：`ALPHA` 端点独立栈段（闭合 SPEC-093 §5.1）](../superpowers/specs/2026-10-12-stage4-alpha-independent-stack-design.md) | unclassified | `docs/superpowers/specs/2026-10-12-stage4-alpha-independent-stack-design.md` |
| [SPEC-095 设计：运行期服务注册表（闭合 SPEC-093 §5.2）](../superpowers/specs/2026-10-12-stage4-runtime-service-registry-design.md) | unclassified | `docs/superpowers/specs/2026-10-12-stage4-runtime-service-registry-design.md` |
| [SPEC-102 设计：把服务管理器迁到 ring3 Rust（服务机制收敛）](../superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md) | unclassified | `docs/superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md` |
| [SPEC-101 设计：字符串服务名与端点名的运行期指派（闭合 SPEC-095 §5.1）](../superpowers/specs/2026-10-13-stage4-string-service-names-design.md) | unclassified | `docs/superpowers/specs/2026-10-13-stage4-string-service-names-design.md` |
| [双人格 Holder 模型设计草案：AddressSpace / CapTable / ShareGroup / Thread](../superpowers/specs/2026-10-14-dual-personality-holder-model-design.md) | unclassified | `docs/superpowers/specs/2026-10-14-dual-personality-holder-model-design.md` |
| [Exokernel 对齐：规划与实现逻辑优化设计（提案）](../superpowers/specs/2026-10-15-exokernel-alignment-design.md) | unclassified | `docs/superpowers/specs/2026-10-15-exokernel-alignment-design.md` |
| [阶段 5 POSIX profile v0（冻结）](../superpowers/specs/2026-10-18-stage5-profile-v0.md) | unclassified | `docs/superpowers/specs/2026-10-18-stage5-profile-v0.md` |
| [阶段 5 POSIX profile v1（delta，M8/M9/M10/M11/M12 提案）](../superpowers/specs/2026-10-20-stage5-profile-v1.md) | unclassified | `docs/superpowers/specs/2026-10-20-stage5-profile-v1.md` |
| [阶段 5 / M8：用户态 VFS/rootfs、通用 fd 与多级路径（设计）](../superpowers/specs/2026-10-20-stage5-user-vfs-design.md) | unclassified | `docs/superpowers/specs/2026-10-20-stage5-user-vfs-design.md` |
| [阶段 5 / M10：匿名内存与保护（设计）](../superpowers/specs/2026-10-21-stage5-m10-anon-memory-design.md) | unclassified | `docs/superpowers/specs/2026-10-21-stage5-m10-anon-memory-design.md` |
| [阶段 5 / M11：文件树变更（`mkdir`/`rmdir`/`unlink`）设计](../superpowers/specs/2026-10-22-stage5-m11-file-mutation-design.md) | unclassified | `docs/superpowers/specs/2026-10-22-stage5-m11-file-mutation-design.md` |
| [阶段 5 / M12：C 启动、退出与 fd 清理基座设计](../superpowers/specs/2026-10-22-stage5-m12-c-startup-exit-design.md) | unclassified | `docs/superpowers/specs/2026-10-22-stage5-m12-c-startup-exit-design.md` |
| [阶段 5 / M13a：符号链接、rename 与 fchdir 设计](../superpowers/specs/2026-10-22-stage5-m13a-vfs-links-design.md) | unclassified | `docs/superpowers/specs/2026-10-22-stage5-m13a-vfs-links-design.md` |
| [阶段 5 / M13b：权限位、umask 与 stdio 文件 API 设计](../superpowers/specs/2026-10-22-stage5-m13b-permissions-stdio-design.md) | unclassified | `docs/superpowers/specs/2026-10-22-stage5-m13b-permissions-stdio-design.md` |
| [文档系统设计与维护契约](../superpowers/specs/2026-10-23-documentation-system-design.md) | current | `docs/superpowers/specs/2026-10-23-documentation-system-design.md` |
| [阶段 5 / M14：tty 输入与最小 console 设计](../superpowers/specs/2026-10-23-stage5-m14-tty-input-design.md) | unclassified | `docs/superpowers/specs/2026-10-23-stage5-m14-tty-input-design.md` |
| [阶段 5 / M15：Process/Thread/ShareGroup 与 WaitObject 设计（D3 提案）](../superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md) | unclassified | `docs/superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md` |
| [阶段 5 / M17：fork/execve/wait4 设计（D3 提案）](../superpowers/specs/2026-10-23-stage5-m17-fork-exec-design.md) | unclassified | `docs/superpowers/specs/2026-10-23-stage5-m17-fork-exec-design.md` |
| [阶段 5 / M17c-2：execve 的 syscall 契约（设计）](../superpowers/specs/2026-10-23-stage5-m17c2-exec-syscall.md) | unclassified | `docs/superpowers/specs/2026-10-23-stage5-m17c2-exec-syscall.md` |
| [阶段 5 / M19：用户态 loopback IPv4/UDP 子集设计](../superpowers/specs/2026-10-23-stage5-m19-loopback-udp-design.md) | unclassified | `docs/superpowers/specs/2026-10-23-stage5-m19-loopback-udp-design.md` |
| [Asterinas 参考续篇：启动信息的一次发布](../superpowers/specs/2026-10-24-asterinas-once-publication-design.md) | current | `docs/superpowers/specs/2026-10-24-asterinas-once-publication-design.md` |
| [Asterinas 参考续篇：POD 类型边界与恢复帧所有权](../superpowers/specs/2026-10-24-asterinas-pod-buffer-design.md) | current | `docs/superpowers/specs/2026-10-24-asterinas-pod-buffer-design.md` |
| [Asterinas 代码复用与 OSTD 同步契约](../superpowers/specs/2026-10-24-asterinas-reuse-design.md) | current | `docs/superpowers/specs/2026-10-24-asterinas-reuse-design.md` |
| [fork 续执行、COW 提交与等待写回](../superpowers/specs/2026-10-24-fork-continuation-design.md) | current | `docs/superpowers/specs/2026-10-24-fork-continuation-design.md` |
| [Capability 位置与溯源 host 模型补齐](../superpowers/specs/2026-10-25-capability-location-model-design.md) | current | `docs/superpowers/specs/2026-10-25-capability-location-model-design.md` |
| [fork 隔离所需的能力继承裁决](../superpowers/specs/2026-10-25-fork-authority-decision.md) | current | `docs/superpowers/specs/2026-10-25-fork-authority-decision.md` |
| [阶段 5 进程与栈边界后继](../superpowers/specs/2026-10-25-process-boundary-followup-design.md) | current | `docs/superpowers/specs/2026-10-25-process-boundary-followup-design.md` |
| [阶段 5 缺口补强：启动、栈保护、退出及 Rust 最小目标](../superpowers/specs/2026-10-25-stage5-gap-closure-design.md) | current | `docs/superpowers/specs/2026-10-25-stage5-gap-closure-design.md` |
| [POSIX profile v2（冻结）](../superpowers/specs/2026-10-25-stage5-profile-v2.md) | current | `docs/superpowers/specs/2026-10-25-stage5-profile-v2.md` |
| [位置模型与 Timer、异常、等待和远端数据通路的组合](../superpowers/specs/2026-10-26-event-authority-model-design.md) | current | `docs/superpowers/specs/2026-10-26-event-authority-model-design.md` |
| [S5-D6：Rust 目标与 std 范围裁决](../superpowers/specs/2026-10-26-rust-d6-scope-decision.md) | current | `docs/superpowers/specs/2026-10-26-rust-d6-scope-decision.md` |
| [阶段 5 出口的具体评审工件](../superpowers/specs/2026-10-26-stage5-release-decisions.md) | current | `docs/superpowers/specs/2026-10-26-stage5-release-decisions.md` |
| [静态程序的 persona 与适配层版本护栏](../superpowers/specs/2026-10-26-static-program-contract-design.md) | current | `docs/superpowers/specs/2026-10-26-static-program-contract-design.md` |
| [Prima POSIX std 后端](../superpowers/specs/2026-10-27-posix-std-backend-design.md) | current | `docs/superpowers/specs/2026-10-27-posix-std-backend-design.md` |
| [阶段 6–7 前置裁决（D11–D16）](../superpowers/specs/2026-10-28-stage6-7-release-decisions.md) | current | `docs/superpowers/specs/2026-10-28-stage6-7-release-decisions.md` |
| [S6-M1 递送设计：OS 中立的同步 syscall 门控（proposal）](../superpowers/specs/2026-10-29-stage6-m1-delivery-design.md) | current | `docs/superpowers/specs/2026-10-29-stage6-m1-delivery-design.md` |
| [S6-M1 递送机制的六项裁决（D17–D22）](../superpowers/specs/2026-10-29-stage6-m1-mechanism-decisions.md) | current | `docs/superpowers/specs/2026-10-29-stage6-m1-mechanism-decisions.md` |
| [S6-M1 协议收口：候选 A、单服务者与不可警报等待](../superpowers/specs/2026-10-29-stage6-m1-protocol.md) | current | `docs/superpowers/specs/2026-10-29-stage6-m1-protocol.md` |

## template（4）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [决策记录模板](templates/decision.md) | current | `docs/architecture/templates/decision.md` |
| [执行计划模板](templates/plan.md) | current | `docs/architecture/templates/plan.md` |
| [实现/验证/整改报告模板](templates/report.md) | current | `docs/architecture/templates/report.md` |
| [设计/接口规格模板](templates/spec.md) | current | `docs/architecture/templates/spec.md` |

## topic（10）

| 文档 | 生命周期 | 路径 |
|---|---|---|
| [启动、硬件与可信边界](topics/boot.md) | current | `docs/architecture/topics/boot.md` |
| [Capability、句柄、Task 与 IPC](topics/capability-ipc.md) | current | `docs/architecture/topics/capability-ipc.md` |
| [POSIX profile、musl、网络与 Linux 路线](topics/compatibility.md) | current | `docs/architecture/topics/compatibility.md` |
| [页帧、映射、TLB 与 COW](topics/memory.md) | current | `docs/architecture/topics/memory.md` |
| [Namespace、Timer 与 Audit](topics/namespace-time-audit.md) | current | `docs/architecture/topics/namespace-time-audit.md` |
| [进程、线程、fork、exec 与信号](topics/process.md) | current | `docs/architecture/topics/process.md` |
| [原生运行时、ring3 服务与用户态 I/O](topics/runtime.md) | current | `docs/architecture/topics/runtime.md` |
| [抽象语义与形式化](topics/semantics.md) | current | `docs/architecture/topics/semantics.md` |
| [等待、唤醒、futex、时间与 TTY](topics/synchronization.md) | current | `docs/architecture/topics/synchronization.md` |
| [用户态 VFS、文件描述符与路径](topics/vfs.md) | current | `docs/architecture/topics/vfs.md` |

## 已移除文档

恢复版本、原因和替代入口见 [生命周期台账](../lifecycle.json)。
用 `python3 scripts/docs.py inspect <旧路径>` 查询；已移除正文不参与全文搜索。

- `docs/superpowers/plans/2026-07-30-implementation-stage-tracker.md`：初建阶段跟踪的执行脚本；仍断言无代码、非 Git 和旧 Windows 路径。维护规则已由阶段跟踪与治理契约承接，不含运行证据。
- `docs/superpowers/plans/2026-07-30-cosir-primitive-fixes.md`：一次性早期修文步骤；Faulted/跨节点规则已发生后继修订，不能继续执行旧替换块。现行语义与签署材料保留。
- `docs/superpowers/plans/2026-10-23-documentation-restructure-plan.md`：已完成的文档搭建步骤，与维护手册重复；验收报告单独保留。
- `docs/superpowers/reports/2026-10-14-dual-personality-handoff.md`：明确声明非证据的会话交接快照；旧的下一阶段描述已失效，采纳审核与 H0 原始评审保留。
- `docs/architecture/primaos-architecture-qa.md`：重复的对话整理，含单文件存储、缺 stdin/exec 等过时实现摘要；无独立契约或实验效力。