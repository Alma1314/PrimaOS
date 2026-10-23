---
id: plan-vfs-libos-foundation
kind: plan
authority: historical
status: execution-record
updated: 2026-10-23
---

# VFS / LibOS 基础重构执行计划

契约见[设计](../specs/2026-10-23-vfs-libos-foundation-design.md)，实际结果见[报告](../reports/2026-10-23-vfs-libos-foundation-report.md)。本记录不作为当前阶段状态源。

1. 追踪 M8 → M11 → M13a → M13b 的设计/证据和 service → VFS → Storage 调用链。
2. 提取 OpenFiles 与类型化原生意图，让存储服务复用同一套 host 可测试语义；保留既有线协议。
3. 实现延迟回收、表满前置、权限/路径/稀疏写修复，增加能复现旧缺陷的 host 回归。
4. 扩展已有 V3 同源探针和期望输出；运行 workspace、lint、Linux 参考及 QEMU 双轮全量回归。
5. 按实际结果更新当前阶段、兼容台账、主题路由和机器目录；记录失败和未覆盖项，不签署阶段出口。
