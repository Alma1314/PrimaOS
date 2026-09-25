# 切片 AC 设计：内核回收判据收紧（SPEC-074）

> 文档类型：真实内核实现设计（阶段 2 收尾，第七节）
>
> 日期：2026-09-24
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-24，"三项都解决"）

## 0. 定位与前置

切片 AA（SPEC-072）实现了 §8.2 内核回收路径，但判据是**过渡口径**——因为当时 §12.10
`exc_resume` 还没实现，规范原文的判据「依据当前 State 不可能成功 `exc_resume`」没有落点。
AA 的报告把"待 §12.10 落地后收紧"显式登记为遗留工作。切片 AB（SPEC-073）已把 §12.10 落地，
故本切片执行这次收紧。

## 1. 收紧内容

**AA 的过渡口径（撤销）**：

```
Faulted ∧ ( ¬recoverable(kind)  ∨  异常源已不存在(§12.6 L1440) )
```

**收紧后（判词 = `exc_resume` 必定失败）**：

```
Faulted ∧ pending_exc.is_some() ∧ ¬recoverable(kind)
```

**为什么"源已销毁"那一支必须撤销**：AB 的实现里 §12.10 的 `exc_source_cap` 授权语义**尚未
实现**——`exc_resume` 根本不查异常源。因此源销毁之后，该任务**照样能恢复**。把"源已销毁"当作
回收条件，会让内核销毁一个**实际可恢复**的任务，正好绕过 AB 刚建立的恢复路径。判词必须与实现的
前置**逐条对应**，否则收回的就可能是活的。

收紧后的判词与 `exc_resume_faulted` 的前置一一对应：

| 判词条件 | `exc_resume` 的前置 |
|---|---|
| `status == Faulted` | 前置 5（调用者必须处于 `Faulted`） |
| `pending_exc.is_some()` | 前置 5（且有 `pending_exc`） |
| `¬recoverable(kind)` | 前置 6（种类必须可恢复，否则 `UnrecoverableFault`） |

（前置 8 `kernel_resume_frame == None` 不纳入判词：已有待消费帧意味着恢复**正在进行中**，
不构成"不可能恢复"。）

## 2. 回归面

**QEMU 事件逐字段不变**（实测事件流 diff 为空）：两条见证（`reclaim_ready` 用 `DoubleFault`、
`resume_ready` 走完整生命周期）都只依赖"种类不可恢复"这一支，撤销"源已销毁"支不影响它们。
这也从侧面证明：撤销的那一支此前**未被任何证据覆盖**。

host 单测：`destroyed_fault_source_makes_a_faulted_task_unrecoverable` →
`destroyed_fault_source_does_not_make_a_faulted_task_reclaimable`（**断言反向**，并加一条
"真正的不可恢复终态仍由种类判定"的正向断言）。

## 3. 验证

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **305**、xtask **167** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：六条 PASS（事件序列与 data 全部不变）。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 边界

- 证据为真实内核 + QEMU，独立性为 **harness 内**；**不勾选**任何交付项与退出条件。
- 本次收紧**撤销了规范 §12.6 L1440 的一支**（"源销毁后不可恢复"）。这不是"规范错了"，而是
  **本细化层尚未实现源 cap 授权语义**的必然后果：实现比规范弱，判词就必须按实现的真实能力写。
  若后续补上 `exc_source_cap` 的授权语义，该支应**随之恢复**——届时"源已销毁"才重新成为
  不可恢复的依据。这条**双向登记**（撤销理由 + 恢复条件）写在此处，避免后人误以为规范条文被否定。
- `ISSUE-FAULT-005` 行状态不变；闭合判断交复核者。
