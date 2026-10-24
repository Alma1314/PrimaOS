---
id: specifications-home
kind: index
authority: navigation
status: maintained
lifecycle: current
updated: 2026-10-24
---

# Specs 索引

本目录保存**设计/规格工件**：它们说明预期语义、接口与实现方案，不代表实现完成。当前实现状态以 [`Prima OS 实现阶段.md`](../../../Prima%20OS%20实现阶段.md) 和实现报告为准。

> 本索引按阶段/主题分组，文件日期用于历史定位。新增文档同步更新本页和相关主题路由，再运行 `python3 scripts/docs.py build` / `check`。统一入口见[文档中心](../../README.md)。

## OSTD / 跨项目代码复用

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-asterinas-pod-buffer-design.md](2026-10-24-asterinas-pod-buffer-design.md) | Asterinas 参考续篇：POD 与恢复帧缓冲边界 |
| 2026-10-24 | [2026-10-24-asterinas-reuse-design.md](2026-10-24-asterinas-reuse-design.md) | Asterinas 复用与 OSTD 同步契约 |

## 跨阶段 / 文档体系

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-23 | [2026-10-23-documentation-system-design.md](2026-10-23-documentation-system-design.md) | 文档系统设计与维护契约 |

## 阶段 5

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-musl-port-boundary-design.md](2026-10-24-musl-port-boundary-design.md) | musl 静态移植：调用边界、fd 控制与可重放构建 |
| 2026-10-23 | [2026-10-23-vfs-libos-foundation-design.md](2026-10-23-vfs-libos-foundation-design.md) | VFS 打开对象与 LibOS 适配边界 |
| 2026-10-22 | [2026-10-22-stage5-m11-file-mutation-design.md](2026-10-22-stage5-m11-file-mutation-design.md) | 阶段 5 / M11：文件树变更（设计） |
| 2026-10-22 | [2026-10-22-stage5-m12-c-startup-exit-design.md](2026-10-22-stage5-m12-c-startup-exit-design.md) | 阶段 5 / M12：C 启动、退出与 fd 清理（设计） |
| 2026-10-22 | [2026-10-22-stage5-m13a-vfs-links-design.md](2026-10-22-stage5-m13a-vfs-links-design.md) | 阶段 5 / M13a：符号链接、rename 与 fchdir（设计） |
| 2026-10-22 | [2026-10-22-stage5-m13b-permissions-stdio-design.md](2026-10-22-stage5-m13b-permissions-stdio-design.md) | 阶段 5 / M13b：权限位、umask 与 stdio 文件 API（设计） |
| 2026-10-23 | [2026-10-23-stage5-m19-loopback-udp-design.md](2026-10-23-stage5-m19-loopback-udp-design.md) | 阶段 5 / M19：用户态 loopback IPv4/UDP 子集（设计） |
| 2026-10-23 | [2026-10-23-stage5-m14-tty-input-design.md](2026-10-23-stage5-m14-tty-input-design.md) | 阶段 5 / M14：tty 输入与最小 console（设计） |
| 2026-10-23 | [2026-10-23-stage5-m15-process-thread-wait-design.md](2026-10-23-stage5-m15-process-thread-wait-design.md) | 阶段 5 / M15：Process/Thread/ShareGroup 与 WaitObject（D3 提案） |
| 2026-10-23 | [2026-10-23-stage5-m17-fork-exec-design.md](2026-10-23-stage5-m17-fork-exec-design.md) | 阶段 5 / M17：fork/execve/wait4（D3 提案） |
| 2026-10-23 | [2026-10-23-stage5-m17c2-exec-syscall.md](2026-10-23-stage5-m17c2-exec-syscall.md) | 阶段 5 / M17c-2：execve syscall 契约 |
| 2026-10-21 | [2026-10-21-stage5-m10-anon-memory-design.md](2026-10-21-stage5-m10-anon-memory-design.md) | 阶段 5 / M10：匿名内存与保护（设计） |
| 2026-10-20 | [2026-10-20-stage5-user-vfs-design.md](2026-10-20-stage5-user-vfs-design.md) | 阶段 5 / M8：用户态 VFS/rootfs、通用 fd 与多级路径（设计） |
| 2026-10-20 | [2026-10-20-stage5-profile-v1.md](2026-10-20-stage5-profile-v1.md) | 阶段 5 POSIX profile v1（delta，提案） |
| 2026-10-18 | [2026-10-18-stage5-profile-v0.md](2026-10-18-stage5-profile-v0.md) | 阶段 5 POSIX profile v0（冻结） |
| 2026-10-11 | [2026-10-11-baselib-stack-walkthrough.md](2026-10-11-baselib-stack-walkthrough.md) | RuxOS 基础库栈 — 走查与扫雷审查记录 |
| 2026-10-11 | [2026-10-11-baselib-stack-spec-v1.1.md](2026-10-11-baselib-stack-spec-v1.1.md) | RuxOS 基础库栈（基座库）技术规格书 v1.1 |

## 阶段 4

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-native-runtime-service-design.md](2026-10-24-native-runtime-service-design.md) | 原生运行时与服务：握手和分片事务边界 |
| 2026-10-13 | [2026-10-13-stage4-string-service-names-design.md](2026-10-13-stage4-string-service-names-design.md) | SPEC-101 设计：字符串服务名与端点名的运行期指派（闭合 SPEC-095 §5.1） |
| 2026-10-13 | [2026-10-13-stage4-ring3-service-manager-design.md](2026-10-13-stage4-ring3-service-manager-design.md) | SPEC-102 设计：把服务管理器迁到 ring3 Rust（服务机制收敛） |
| 2026-10-12 | [2026-10-12-stage4-runtime-service-registry-design.md](2026-10-12-stage4-runtime-service-registry-design.md) | SPEC-095 设计：运行期服务注册表（闭合 SPEC-093 §5.2） |
| 2026-10-12 | [2026-10-12-stage4-alpha-independent-stack-design.md](2026-10-12-stage4-alpha-independent-stack-design.md) | SPEC-094 设计：`ALPHA` 端点独立栈段（闭合 SPEC-093 §5.1） |
| 2026-10-11 | [2026-10-11-stage4-vspace-arena-design.md](2026-10-11-stage4-vspace-arena-design.md) | SPEC-091 设计：逻辑 vspace 存储移出 `BootstrapState`（arena 化） |
| 2026-10-11 | [2026-10-11-stage4-service-manager-design.md](2026-10-11-stage4-service-manager-design.md) | SPEC-090 设计：服务管理器（init 按名解析并路由） |
| 2026-10-11 | [2026-10-11-stage4-multi-endpoint-service-design.md](2026-10-11-stage4-multi-endpoint-service-design.md) | SPEC-092 设计：独立第二服务端点（init 按名路由到不同任务） |
| 2026-10-11 | [2026-10-11-stage4-independent-service-images-design.md](2026-10-11-stage4-independent-service-images-design.md) | SPEC-093 设计：独立第二服务端点（独立二进制映像 + 独立物理栈段） |
| 2026-10-10 | [2026-10-10-stage4-service-negotiation-design.md](2026-10-10-stage4-service-negotiation-design.md) | SPEC-088 设计：服务端解析与协商（阶段 4 S4-D2 前置） |
| 2026-10-10 | [2026-10-10-stage4-ring3-runtime-hello-design.md](2026-10-10-stage4-ring3-runtime-hello-design.md) | SPEC-085 设计：阶段 4 独立 ring3 原生客户端与 IPC Hello 握手 |
| 2026-10-10 | [2026-10-10-stage4-init-client-handshake-design.md](2026-10-10-stage4-init-client-handshake-design.md) | SPEC-087 设计：init ↔ 存活 ring3 客户端双向握手 |
| 2026-09-25 | [2026-09-25-stage4-runtime-api-design.md](2026-09-25-stage4-runtime-api-design.md) | SPEC-084 设计：阶段 4 Prima 原生运行时 API 与版本规则 |

## 阶段 3

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-25 | [2026-09-25-stage3-timer-design.md](2026-09-25-stage3-timer-design.md) | 切片 AK 设计：阶段 3 第二片 —— Timer（§10 全量） |
| 2026-09-25 | [2026-09-25-stage3-namespace-design.md](2026-09-25-stage3-namespace-design.md) | 切片 AJ 设计：阶段 3 第一片 —— Namespace 路径操作（§11 全量） |
| 2026-09-25 | [2026-09-25-stage3-audit-trace-design.md](2026-09-25-stage3-audit-trace-design.md) | 切片 AL 设计：阶段 3 第三片 —— `audit(cap)` 审计查询（§13.2） |

## 阶段 2 / Framekernel

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-24 | [2026-10-24-kernel-handle-model-design.md](2026-10-24-kernel-handle-model-design.md) | 内核句柄耗尽与 Lean 迁移模型 |
| 2026-09-25 | [2026-09-25-framekernel-skeleton-design.md](2026-09-25-framekernel-skeleton-design.md) | 切片 AE 设计：framekernel 分层与 unsafe 收敛（SPEC-076） |
| 2026-09-25 | [2026-09-25-framekernel-mm-pod-design.md](2026-09-25-framekernel-mm-pod-design.md) | 切片 AF 设计：物理内存与页表 POD 收敛（SPEC-077） |
| 2026-09-25 | [2026-09-25-framekernel-global-state-design.md](2026-09-25-framekernel-global-state-design.md) | 切片 AG 设计：内核全局状态 safe 化（SPEC-078） |
| 2026-09-25 | [2026-09-25-framekernel-entry-inversion-design.md](2026-09-25-framekernel-entry-inversion-design.md) | 切片 AH 设计：trap/syscall 入口回调反转 + IDT/GDT 下沉（SPEC-079） |
| 2026-09-25 | [2026-09-25-framekernel-closure-design.md](2026-09-25-framekernel-closure-design.md) | 切片 AI 设计：收口与 TCB 量化（SPEC-080） |
| 2026-09-24 | [2026-09-24-stage2-self-built-cr3-design.md](2026-09-24-stage2-self-built-cr3-design.md) | 切片 X 设计：自建内核 CR3 与镜像逐页映射（ISSUE-AUTH-001） |
| 2026-09-24 | [2026-09-24-stage2-scheduler-state-audit-design.md](2026-09-24-stage2-scheduler-state-audit-design.md) | 切片 Z 设计：调度路径的 `TaskStateChanged` 与审计容量（ISSUE-TASK-003） |
| 2026-09-24 | [2026-09-24-stage2-reclaim-predicate-tightening-design.md](2026-09-24-stage2-reclaim-predicate-tightening-design.md) | 切片 AC 设计：内核回收判据收紧（SPEC-074） |
| 2026-09-24 | [2026-09-24-stage2-kernel-reclaim-design.md](2026-09-24-stage2-kernel-reclaim-design.md) | 切片 AA 设计：§8.2 内核回收路径（`KernelActor`，无 `task_cap`） |
| 2026-09-24 | [2026-09-24-stage2-faulted-state-machine-design.md](2026-09-24-stage2-faulted-state-machine-design.md) | 切片 Y 设计：`TaskStatus::Faulted` 状态机与 §12.5 迁移入口（ISSUE-TASK-001） |
| 2026-09-24 | [2026-09-24-stage2-exc-resume-design.md](2026-09-24-stage2-exc-resume-design.md) | 切片 AB 设计：§12.10 `exc_resume` 用户化（`KernelActor` 代调） |
| 2026-09-24 | [2026-09-24-stage2-audit-storage-refactor-design.md](2026-09-24-stage2-audit-storage-refactor-design.md) | 切片 AD 设计：审计存储重构（SPEC-075） |
| 2026-09-23 | [2026-09-23-stage2-image-frame-vector-design.md](2026-09-23-stage2-image-frame-vector-design.md) | 切片 W 设计：image Segment 的真实逐帧向量（ISSUE-FRAME-001） |
| 2026-09-17 | [2026-09-17-stage2-task-self-destroy-design.md](2026-09-17-stage2-task-self-destroy-design.md) | 切片 V 设计：Task 自毁与延迟帧归还（ISSUE-TASK-002） |
| 2026-09-17 | [2026-09-17-stage2-task-lifecycle-design.md](2026-09-17-stage2-task-lifecycle-design.md) | 切片 T 设计：Task create/yield/destroy 用户原语与单核调度（S2-D4） |
| 2026-09-17 | [2026-09-17-stage2-platform-cap-handover-design.md](2026-09-17-stage2-platform-cap-handover-design.md) | 切片 U 设计：平台段 Capability 按属主移交（ISSUE-AUTH-002 剩余项） |
| 2026-09-16 | [2026-09-16-stage2-ipc-user-surface-design.md](2026-09-16-stage2-ipc-user-surface-design.md) | 切片 S 设计：IPC 用户化与两 ring3 任务共享 Segment |
| 2026-09-16 | [2026-09-16-stage2-handles-caps-segments-design.md](2026-09-16-stage2-handles-caps-segments-design.md) | 切片 Q 设计：阶段 2 启动——句柄编码、Capability 生命周期、Segment 原语 |
| 2026-09-16 | [2026-09-16-stage2-bringup-segment-migration-design.md](2026-09-16-stage2-bringup-segment-migration-design.md) | 切片 R 设计：bring-up 13 区域迁移为真实 Segment 对象 |

## 阶段 1

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-16 | [2026-09-16-stage1-closure-design.md](2026-09-16-stage1-closure-design.md) | 切片 P 设计：阶段 1 退出条件——帧唯一归属与初始 Authority 物化 |
| 2026-09-05 | [2026-09-05-pf-error-code-semantics-design.md](2026-09-05-pf-error-code-semantics-design.md) | 切片 C(续)设计：#PF error-code 捕获与写保护语义分类 |
| 2026-08-17 | [2026-08-17-physmem-discovery-design.md](2026-08-17-physmem-discovery-design.md) | 物理内存发现（physmem discovery）设计 |
| 2026-08-17 | [2026-08-17-page-gdt-idt-trap-design.md](2026-08-17-page-gdt-idt-trap-design.md) | 页表/GDT/IDT/trap 框架设计（S1-D4，切片 B） |
| 2026-08-04 | [2026-08-04-stage1-boot-slice-design.md](2026-08-04-stage1-boot-slice-design.md) | Prima OS 阶段 1 启动闭环设计 |

## 阶段 0–2 / 形式化与细化

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-08-18 | [2026-08-18-blk001-lean-formal-model-design.md](2026-08-18-blk001-lean-formal-model-design.md) | BLK-001 形式化模型工件：Lean 4 可执行模型 + cosir 对拍 |
| 2026-08-16 | [2026-08-16-primaos-audit-remediation-design.md](2026-08-16-primaos-audit-remediation-design.md) | PrimaOS 审计缺陷修复设计 |
| 2026-08-15 | [2026-08-15-tlb-refine-design.md](2026-08-15-tlb-refine-design.md) | TLB 细化（tlb_refine）设计 |
| 2026-08-14 | [2026-08-14-cow-refinement-design.md](2026-08-14-cow-refinement-design.md) | COW fork 组合协议细化：cow_refine 验证层设计 |
| 2026-08-12 | [2026-08-12-refinement-audit-remediation-design.md](2026-08-12-refinement-audit-remediation-design.md) | PrimaOS 细化审查纠偏设计 |
| 2026-08-12 | [2026-08-12-memseg-pagewalk-refinement-design.md](2026-08-12-memseg-pagewalk-refinement-design.md) | BLK-002 细化续：MemorySegment 页表 ↔ vspace 细化 |
| 2026-08-07 | [2026-08-07-exc-resume-design.md](2026-08-07-exc-resume-design.md) | exception 恢复细化：exc_resume 原语设计 |
| 2026-08-07 | [2026-08-07-blk002-refinement-design.md](2026-08-07-blk002-refinement-design.md) | BLK-002 细化草案：x86_64 MachineState → POSIR State |

## 跨阶段 / 兼容性验证

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-09-15 | [2026-09-15-linux-trace-compat-design.md](2026-09-15-linux-trace-compat-design.md) | 切片 P 设计：Linux trace 与有界 COMPAT-TEST（`cow-write-fault`） |

## 跨阶段 / 架构专题

| 日期 | 文档 | 标题 |
|---|---|---|
| 2026-10-15 | [2026-10-15-exokernel-alignment-design.md](2026-10-15-exokernel-alignment-design.md) | Exokernel 对齐：规划与实现逻辑优化设计（提案） |
| 2026-10-14 | [2026-10-14-dual-personality-holder-model-design.md](2026-10-14-dual-personality-holder-model-design.md) | 双人格 Holder 模型设计草案：AddressSpace / CapTable / ShareGroup / Thread |
