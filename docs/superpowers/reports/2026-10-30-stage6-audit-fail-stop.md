---
id: report-stage6-audit-fail-stop
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-30
---

# S6-M1 审计耗尽：首次诊断与全局停止控制器

## 结论与范围

基线 `759c422`（工作区干净）上，把[私有等待](2026-10-30-stage6-private-completion-waits.md)
的 AuditFull 从可重试错误收紧为持久停止标记，并接入分派、调度返回和时钟入口。
首次耗尽在任何等待/Task 迁移前封存独立诊断；释放日志压力、后续 reserve/complete/release
以及调度/让出不能解除停止。控制器在 State 锁外走既有 terminal panic 路径，单核上停止整个
系统（QEMU failure exit；硬件路径退出端口后 hlt），不会把 AuditFull 作为客户端结果返回。

这是[M1 协议 §5](../specs/2026-10-29-stage6-m1-protocol.md)的内核控制器增量。
仍无 Gate attach/trap/reply、服务能力丢失钩子或代理复制 ABI；Gate 层须调用共同预检并补齐
Gate/Request/cookie/原始号与参数审计。本次不认定 G1–G3 或 S6-D2 完成。

## 输入与实现

契约链：[计划](../plans/2026-10-27-stage6-7-refinement.md) →
[递送设计](../specs/2026-10-29-stage6-m1-delivery-design.md) →
[R1–R6 发现](2026-10-29-stage6-m1-decision-review.md) →
[M1a 整改](2026-10-29-stage6-m1-blocker-closure.md) → 上轮私有等待。
沿用 pinned nightly、已有离线依赖和单核 QEMU TCG；没有 SMP、性能或 Lean 主张。

- 新 `kernel/src/gate_audit.rs` 保存首个 operation、owner cookie、所需条数、已用条数及
  预留条数。诊断静态存储与普通审计池独立，复用 AuditLog 的独占分区编号定位；不增加
  BootstrapState 字段，不分配帧，不消耗普通审计尾部空间，无清除/恢复 API。
- 私有 reserve/block/complete/release 统一在修改前调用 `preflight_gate_audit`。
  已停止时即使请求零条日志也失败，首次诊断不被后续操作覆盖。
- 带私有等待的 Task 销毁、自毁、回收和线程退出共用失败关闭预检；Process 组终止的总预算
  在清理任何成员之前检查。原生无私有等待的普通资源错误保持既有语义。
- int80 入口、共享分派入口、两种 syscall 返回出口、pick_next、resume、IRQ0 回调接入
  全局检查。Ready→Running / Running→Ready 的状态操作也拒绝已停止状态。
  panic 发生前不持有 BOOTSTRAP_STATE 锁；诊断输出依赖现有 panic handler，结构化记录
  留在独立内核单元中，不声称现有串口会打印全部诊断字段。

## 失败、测试与证据

首版内联诊断、随后新增 arena 句柄均触发既有 13 KiB 大小守卫；改为复用审计分区身份后通过。
没有放宽守卫、内核栈、日志容量或测试超时。源码复核修正了让出路径的入口定位，补充
停止后 yield 不修改状态的断言；第一次完整回归主动 SIGTERM 终止，退出码 143（[中止日志](artifacts/stage6-audit/interrupted-full-suite.txt)），不能计为通过。最终命令从头重跑。

原有 7 组等待测试改为：耗尽后只能以独立新实例测试其它失败点，不能释放压力后继续旧请求。
新增两组测试验证首次诊断、独立实例隔离、控制器 terminal panic、停止后完成/释放/调度/
让出均拒绝，以及已有双向登记保持。实际 panic 分支的测试在 host 使用 catch_unwind，
不把它表述成 QEMU 耗尽注入。

| 命令 | 本次结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1048 passed，0 failed，1 个既有外部 ELF 样本 ignored；[日志](artifacts/stage6-audit/workspace.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy、裸机编译，kernel/src 34 文件零 unsafe；[日志](artifacts/stage6-audit/lint.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：最终单次命令 exit=0，36 程序/72 轮及全部主机参考；[日志](artifacts/stage6-audit/full-suite.txt) |
| 文档四项 / 空白检查 | PASS：self-test、build、check、check --base 759c422；334 文档及 Git 空白检查 |

完整回归获准沙箱外运行，因为既有 Linux N1/R3 参考需要主机套接字。日志归档仅移除末尾
额外空行。既有 QEMU 套件只验证未触发停止时的回归；本轮没有 ring3 Gate、审计耗尽 QEMU
注入或真实 copy/reply 的全链路 fail-stop 证据。

## 阶段影响与后继

本报告收紧上一报告“AuditFull 只是内核错误、全局控制器待实现”的范围，保留其历史事实。
控制器已接入真实入口，但 Gate 对象、能力、copy/reply 等后继事务仍必须主动预检和遵守
不调度区间；该控制器不能自动追踪尚不存在的服务授权，不能据此宣称全部 Gate 操作均已
原子化或已完成审计。停止后的可用性、恢复和日志 drain 仍留后续阶段。
当前状态见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)，不勾选交付或退出条件。

裸机验证后继：[七个执行边界的 QEMU 耗尽注入](2026-10-30-stage6-audit-stop-qemu.md)
使用专用编译配置验证停止位置与预期失败变异；本文原有 host 与普通回归范围保持。
