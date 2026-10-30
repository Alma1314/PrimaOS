---
id: topic-synchronization
kind: topic
authority: explanatory
status: maintained
lifecycle: current
updated: 2026-10-30
---

# 等待、唤醒、futex、时间与 TTY

返回[主题地图](README.md) · [AI 检索协议](../ai-retrieval.md) · [当前阶段](../../../Prima%20OS%20实现阶段.md)

## 解决的问题与检索词

observe/generation 如何避免丢唤醒？销毁与超时怎样恢复等待者？线程槽回收为什么不能简单放宽容量？TTY 输入在哪轮注入？

检索别名：WaitObject, wait_objects, signal_object, futex, BlockedWait, COM1, termios, tty, PIT。

## 权威章节与阅读顺序

原语规范§5.6、§21、W17；M15 等待设计；M14 tty 与 M6 时间子集。

先确认阶段跟踪与契约，再按下面链读取设计、计划和报告。历史工件中未开始/未实现的表述只属于其当时基线。

M15a-1 立即路径 → M15a-2 阻塞模型 → 线程容量调查 → M15b-2b W2 阻塞端到端。M16a futex WAIT/WAKE 建在等待机制上。M14 输入和 M6 轮询时间有自己的证据，不能被 WaitObject 的存在自动升级。

## 契约、计划与证据入口

- [COSIR 原语规范.md](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)
- [2026-10-23-stage5-m14-tty-input-design.md](../../superpowers/specs/2026-10-23-stage5-m14-tty-input-design.md)
- [2026-10-23-stage5-m15-process-thread-wait-design.md](../../superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)
- [2026-10-23-stage5-m15a-waitobject.md](../../superpowers/reports/2026-10-23-stage5-m15a-waitobject.md)
- [2026-10-23-stage5-m15a2-waitobject-blocking.md](../../superpowers/reports/2026-10-23-stage5-m15a2-waitobject-blocking.md)
- [2026-10-23-stage5-m15b2b-thread-slots.md](../../superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md)
- [2026-10-23-stage5-m16a-futex.md](../../superpowers/reports/2026-10-23-stage5-m16a-futex.md)
- [2026-10-23-stage5-m14-tty-input.md](../../superpowers/reports/2026-10-23-stage5-m14-tty-input.md)
- [2026-10-19-stage5-m6-time.md](../../superpowers/reports/2026-10-19-stage5-m6-time.md)
- [2026-10-23-stage5-thread-capacity-investigation.md](../../superpowers/reports/2026-10-23-stage5-thread-capacity-investigation.md)

OSTD 后继补强：[Asterinas 移植契约](../../superpowers/specs/2026-10-24-asterinas-reuse-design.md)与
[验证报告](../../superpowers/reports/2026-10-24-asterinas-reuse-report.md)限定对齐工具、StaticCell IRQ-safe 锁与调度跳转；
来源与许可见 [来源与许可](../asterinas-provenance.md)。候选组件不等于已迁入。

## 源码定位

- [bootstrap.rs](../../../kernel/src/bootstrap.rs)
- [syscall.rs](../../../kernel/src/syscall.rs)
- [sched.rs](../../../kernel/src/sched.rs)
- [preempt.rs](../../../kernel/src/preempt.rs)
- [tty.rs](../../../prima-user/src/tty.rs)
- [servicemgr.rs](../../../prima-user/src/servicemgr.rs)
- [prima_syscall.c](../../../prima-user/c/prima_syscall.c)

- [align.rs](../../../prima-ostd/src/align.rs)
- [sync.rs](../../../prima-ostd/src/sync.rs)
- [lock.rs](../../../prima-ostd/src/sync/lock.rs)

## 测试与观察入口

- [stage5_wait.c](../../../prima-user/c/stage5_wait.c)
- [stage5_wait2.c](../../../prima-user/c/stage5_wait2.c)
- [stage5_futex.c](../../../prima-user/c/stage5_futex.c)
- [stage5_time.c](../../../prima-user/c/stage5_time.c)
- [stage5_ttyin.c](../../../prima-user/c/stage5_ttyin.c)

测试命令与证据分类见[开发手册](../development.md)和[证据规则](../evidence.md)。路径指向模块内联测试或探针入口，不保证所有分支都已覆盖。

## 限制与修改注意

futex 仅冻结 WAIT/WAKE 子集。TTY 有读者时仍轮询 COM1；WaitObject 已实现并不意味着轮询已替换。线程惰性回收保留 task_create 满容量 ENOSPC 见证；不能用扩容隐藏负例。

## 启动信息的一次发布（OSTD 后继）

[契约](../../superpowers/specs/2026-10-24-asterinas-once-publication-design.md)与
[验证报告](../../superpowers/reports/2026-10-24-asterinas-once-publication-report.md)限定 Once 的错误/unwind 重试、
IRQ-safe 初始化和析构，以及 user::KernelRootInfo 的根/镜像描述整体发布。
get 无等待；set 争用返回原值；递归初始化仍禁止。self_root_ready.published_once 来自重复构建与帧计数不变见证。
检索：Once、get_or_try_init、KernelRootInfo、published_once；实现见
[sync.rs](../../../prima-ostd/src/sync.rs)、[user.rs](../../../kernel/src/user.rs)、
[trace 校验](../../../xtask/src/protocol.rs)。该机制未替换所有 boot 标志或证明 SMP。

## S6-M1 私有完成等待

[协议 §2](../../superpowers/specs/2026-10-29-stage6-m1-protocol.md)与
[内核等待基础证据](../../superpowers/reports/2026-10-30-stage6-private-completion-waits.md)：
[completion.rs](../../../kernel/src/completion.rs) 使用真实 WaitObject/WaitRequest，
以 Task cookie 和每次登记的 CompletionTicket 区分迟到完成；私有等待不可警报、没有普通
WaitSignal/WaitDestroy 权限。公共 32 与私有 80 的对象预算独立。
证据为 host 内核状态机与既有 QEMU 回归，Gate attach/trap/reply 尚未调用这些接口，
不能据此认定 G1/G2 或 S6-D2 完成。

[审计停止后继](../../superpowers/reports/2026-10-30-stage6-audit-fail-stop.md)将私有等待
AuditFull 收紧为首次诊断与持久停止，并在分派、调度及 IRQ 入口消费停止标记；
[gate_audit.rs](../../../kernel/src/gate_audit.rs) 的记录与普通日志池独立，不能释放压力后重试。
Gate 完整事务与 G1–G3 仍需后继验证。

[裸机耗尽验证后继](../../superpowers/reports/2026-10-30-stage6-audit-stop-qemu.md)提供
显式编译启用的七边界 QEMU 探针与停止调用变异拒绝；实现见
[gate_audit_probe.rs](../../../kernel/src/gate_audit_probe.rs)、
[专用 runner/校验器](../../../xtask/src/gate_audit.rs)。这是既有控制器的故障注入证据，
不是门控 ring3 服务、Gate 生命周期或 G1–G3；普通镜像不包含该探针。
