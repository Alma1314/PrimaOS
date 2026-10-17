# SPEC-148：阶段 3 退出条件复核与签署

> 文档类型：阶段 3 退出条件复核/签署记录
>
> 日期：2026-10-17
>
> 基线：`6388330`（阶段 2 签署后的 HEAD）
>
> 范围：S3-D1–D5、S3-X1–X4（Namespace、Timer、Audit）。
>
> 结论：**S3-D1–D5、S3-X1–X4 通过（实现 + host 测试 + QEMU 集成断言）。** 阶段 3 退出条件勾选；状态改为“已完成”，当前阶段推进到阶段 4。

## 1. 复核方法

- 基线 `6388330`；回读 `kernel/src/namespace.rs`、`bootstrap.rs`（Namespace/Timer/Audit 方法）、`user.rs`、`protocol.rs`、`xtask/src/protocol.rs` 与 SPEC-081–083 报告/设计。
- 运行 `CARGO_NET_OFFLINE=true cargo xtask test`：`PASS build kernel`、`PASS image bios`、`PASS image uefi`、`PASS qemu bios run=1`、`PASS qemu bios run=2`、`PASS stage1 boot slice`，退出 0。
- 既有 `cargo test --workspace` 786 passed 与 `cargo xtask lint` 通过；本复核不新增代码。

## 2. 交付项

| ID | 证据 | 结果 |
|---|---|---|
| S3-D1 | `ns_create`/`bind`/`unbind`/`mount`/`unmount`/`resolve`/`destroy`；`namespace_ready` 的 `binds=2;unbinds=1;mounts=2;unmounts=1;resolves=2` | 通过 |
| S3-D2 | `namespace_reaches`/DFS + `visited` 位集；`cycles_rejected=1`、`acyclic_ok=2`；stale Mount 可由 `ns_unmount` 清理（host 单测） | 通过 |
| S3-D3 | `time_now` + `advance_time`；`timer_ready.now=4` 由 PIT tick 推进；`new_now < now` 返回 `InvalidTime` 且状态不变 | 通过 |
| S3-D4 | `deadline ≤ now` 在同一原子操作内 `TimerFired` + 通知 + `Fired`；`timer_ready.immediate_fired=1` | 通过 |
| S3-D5 | `AuditEvent` 的 Capability 派生/移动/位置变化 + `audit(cap)` 链；`audit_ready` 的 `chain_len=2;derived=1;moves=2;created=1;destroyed=0`；`state_audit_ready.used` 在查询前后不变证明纯查询 | 通过 |

## 3. 退出条件

| ID | 证据 | 结果 |
|---|---|---|
| S3-X1 | 每个分量消费后路径缩短，DFS 有 `visited` 位集、节点数 ≤ `MAX_NAMESPACE_OBJECTS`，确定性终止；`namespace_ready.cycles_rejected=1`、`acyclic_ok=2` | 通过 |
| S3-X2 | `timer_ready.exactly_once_ok=1`：每个 `Fired` 活动 Timer 恰一条 `TimerFired`，其 `notification` 恰对应一条 `MessageSent`；`fired=2;cancelled=1;destroyed=1` | 通过 |
| S3-X3 | `audit_ready.chain_len=2;derived=1;moves=2;created=1;stale_ok=1;evidence_ok=1`；`audit` 前置仅“cap 由 caller 持有”，目标可已销毁；`SYS_AUDIT` 查询不追加事件 | 通过 |
| S3-X4 | `task_destroy` 按 recipient 处置 Timer；`ns_destroy` 删除条目与位置、不全局扫描 stale Mount；host 单测 `destroy_timers_of_recipient_only_touches_that_recipient`；`namespace_ready` 的 c Namespace 销毁后其他 Namespace 不受影响 | 通过 |

## 4. 已登记的实现边界（不阻塞 S3）

- `ns_destroy`/`ns_unmount` 只删除 Capability **位置**，不释放槽位/不移除 `CapRecord`（与 §4.6 “移动不改变 CapRecord”一致）；这是细化层资源回收边界，测试已登记。
- `ns_destroy` 不全局扫描其他 Namespace 中指向它的 Mount（§11.8 明示）；解析得到 `MountTargetDead`，由 `ns_unmount` 清理。
- `timer_ready.destroyed` 由实现层 `TimerStats.destroyed` 计数，而非 `ObjectDestroyed` 通用事件；这是避免对象类型混计的实现口径，已由报告登记。
- `audit(cap)` 的 `chain_len` 受现有派生链长度限制；`origin` 在 v1 恒为 `None`（跨节点 Cap 委托不在 v1）。
- 本复核不宣称 `REFINEMENT-PROOF` 或所有 §17 安全结论；证据为真实内核 + host 单测 + QEMU。

## 5. 结论与签署

- **S3-D1–D5、S3-X1–X4 全部通过。** 阶段 3 退出条件勾选，状态改为“已完成（2026-10-17，SPEC-148）”。
- 当前阶段推进到阶段 4；阶段 4 的 S4-D1–D5/X1–X4 技术证据已在 SPEC-130 收口，正式晋级将在阶段 4 复核中确认。
