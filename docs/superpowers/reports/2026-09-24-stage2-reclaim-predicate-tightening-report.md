# 切片 AC 报告：内核回收判据收紧（SPEC-074）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-24
>
> 对象：`kernel/src/bootstrap.rs`
>
> 目的：把 SPEC-072 的**过渡判据**收紧为规范原文的「依据当前 State 不可能成功 `exc_resume`」

## 0. 结论速览

- **判词已收紧**：`is_unrecoverable_faulted` 现在只认
  `Faulted ∧ pending_exc ≠ None ∧ ¬recoverable(kind)`，与 §12.10 `exc_resume` 的前置 5/6
  **逐条对应**——判据不再近似，而是"该次 `exc_resume` 必定失败"的直接投影。
- **撤销了 SPEC-072 过渡口径里的"异常源已不存在"一支**：AB 的实现不查 `exc_source_cap`，
  源销毁后该任务**照样可恢复**；把源销毁当回收条件会销毁一个实际可恢复的任务，绕过 AB 刚建立的
  恢复路径。
- **QEMU 证据逐字段不变**：两条见证都只依赖"种类不可恢复"支，说明被撤销的那一支**此前未被任何
  证据覆盖**——这正是它该被撤销的旁证：它既不符合实现、也未被见证。
- **不勾选任何交付项与退出条件**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不构成 `REFINEMENT-PROOF`。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `bootstrap.rs` | `is_unrecoverable_faulted` 收紧为三条件合取；判词与 `exc_resume_faulted` 的前置一一对应（写在文档注释里） |
| 测试 | `destroyed_fault_source_makes_a_faulted_task_unrecoverable` → `destroyed_fault_source_does_not_make_a_faulted_task_reclaimable`（断言反向 + 保留正向的"种类不可恢复"断言） |

## 3. 实测证据（2026-09-24）

事件流逐条 diff（上一提交 `440db29` vs 本切片）：**空**。`reclaim_ready` / `resume_ready` /
`state_audit_ready` 等全部逐字段一致——收紧判据不改变既有见证。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **305**、xtask **167** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. host 单测

`destroyed_fault_source_does_not_make_a_faulted_task_reclaimable`：把异常源记录摘掉后，
`is_unrecoverable_faulted` 必须**仍为假**、`kernel_reclaim_task` 必须仍被拒；随后把 kind 改成
`DoubleFault`，判据才为真。

## 5. 边界与反例保留

- **本次撤销的是"实现比规范弱"的必然后果，不是否定规范**：§12.6 L1440 的"源销毁后不可恢复"
  在**源 cap 授权语义实现之后**应随之恢复。设计文档 §4 写了**双向登记**（撤销理由 + 恢复条件），
  避免后人误读。
- 判词仍不覆盖前置 8（`kernel_resume_frame` 已有值）——那意味着恢复**正在进行**，不是"不可能恢复"。
- `ISSUE-FAULT-005` 行状态不变；闭合判断交复核者。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。
