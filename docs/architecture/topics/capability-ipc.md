---
id: topic-capability-ipc
kind: topic
authority: explanatory
status: maintained
updated: 2026-10-23
---

# Capability、句柄、Task 与 IPC

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

句柄怎样防 stale 重新授权？cap 位置、派生与移动如何一致？阻塞接收的带 cap 直投是否有见证？

检索别名：cap, rights, handle, generation, holder, send, recv, FIFO, 能力, 消息。

## 权威章节与阅读顺序

原语规范§4、§5.2–5.3、§8–9、W1–W4/W9/W10。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

阶段2开局/用户 IPC/task lifecycle → SPEC-147 退出复核 → 2026-10-18 IPC 签署后整改。后者修复并验证带 Capability 的 BlockedRecv 直投，读取旧报告时应一起读取。

## 契约、计划与证据入口

- [COSIR 原语规范.md](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)
- [2026-09-16-stage2-handles-caps-segments-design.md](../../superpowers/specs/2026-09-16-stage2-handles-caps-segments-design.md)
- [2026-09-16-stage2-ipc-user-surface-design.md](../../superpowers/specs/2026-09-16-stage2-ipc-user-surface-design.md)
- [2026-09-17-stage2-task-lifecycle-design.md](../../superpowers/specs/2026-09-17-stage2-task-lifecycle-design.md)
- [2026-09-16-stage2-opening-report.md](../../superpowers/reports/2026-09-16-stage2-opening-report.md)
- [2026-09-16-stage2-ipc-user-surface-report.md](../../superpowers/reports/2026-09-16-stage2-ipc-user-surface-report.md)
- [2026-09-17-stage2-task-lifecycle-report.md](../../superpowers/reports/2026-09-17-stage2-task-lifecycle-report.md)
- [2026-10-17-stage2-exit-review-spec147.md](../../superpowers/reports/2026-10-17-stage2-exit-review-spec147.md)
- [2026-10-18-stage2-ipc-review-remediation.md](../../superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)
- [2026-10-13-ipc-capability-move-audit-report.md](../../superpowers/reports/2026-10-13-ipc-capability-move-audit-report.md)

## 源码定位

- [handle.rs](../../../kernel/src/handle.rs)
- [rights.rs](../../../kernel/src/rights.rs)
- [bootstrap.rs](../../../kernel/src/bootstrap.rs)
- [syscall.rs](../../../kernel/src/syscall.rs)
- [sched.rs](../../../kernel/src/sched.rs)
- [capability.rs](../../../prima-runtime/src/capability.rs)
- [ipc.rs](../../../prima-runtime/src/ipc.rs)
- [task.rs](../../../prima-runtime/src/task.rs)

## 测试与观察入口

- [test_support.rs](../../../kernel/src/test_support.rs)
- [protocol.rs](../../../xtask/src/protocol.rs)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

原生 Task 不等同于 POSIX Process。共享能力空间是阶段5的额外工作决定，见 process 主题。规范第一版 cap_drop 不隐含选择性撤销；探索实现不能反过来默改抽象基线。
