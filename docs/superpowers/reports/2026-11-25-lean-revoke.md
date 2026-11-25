---
id: report-lean-revoke
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# Lean `Formal.Revoke`：选择性撤销域最小参考模型（7-D 抽象前瞻）

## 结论与范围

本报告记录 [Prima OS §6.2 实现注记](../../../Prima%20OS.md)所述 Bootstrap 宿主
选择性撤销（对应 COSIR §4.7 撤销边界）的最小 Lean 参考模型：新增
`lean/Formal/Revoke.lean` 与 `lean/Checks/Revoke.lean`。15 条普适定理通过
`lake build`；3 条无公理，其余依赖 `propext`/`Classical.choice`/`Quot.sound`。

本切片是**命题级经典模型**（`revoked` 为 noncomputable，没有 `#eval` 锚），
证据强度低于同期三个可执行细化模型（TLB/Memseg/COW），已如实登记。它不是
7-D 实现证据、并发撤销证明或 Rust 源码细化。

## 基线与输入

- 基线：`5583cfc`（另加 `fd844ac`、`01dc0fe`、`5cc574b`）。
- 契约：[Prima OS §6.2 实现注记](../../../Prima%20OS.md)（撤销域）、COSIR §4.7
  （第一版无 `cap_revoke`，探索实现先行）。
- 复用：`Formal.State` 的 `Capability`/`Location`/`State`/`WellFormed`。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 Revoke 节。

## 实现

- `Desc s root c`：`c = root` 或沿 `cap.parent` 指针可达 root 的归纳谓词。
- `revoked s root`：子树内 capability 置 `active := false`；子树内 `HeldBy`
  位置删除；`InMessage`/`InNamespace` 位置保留；子树外状态不变；记录与 parent
  指针保留。
- `revoke s root holder`：仅当 root 当前位置为 `HeldBy holder` 时成功并返回
  `(revoked …, true)`；否则原状态返回 `(s, false)`（宿主 `HolderMismatch`）。
- `ActiveWellFormed`：只约束 active capability 的 CDT 不变量（父存在/活动/
  更小 CapId/同对象）。原 `WellFormed` 要求所有有父项的父 active；撤销后父子
  同时 inactive，因此原谓词不再适用于该子树——这是明确的范围变化，不是对撤销前
  不变量的放宽。

已证性质（15 条）：

- 子树：`desc_trans`、`not_desc_parent`。
- 失效：`revoked_caps_desc`、`revoked_loc_desc_held`、
  `revoked_loc_desc_message`、`revoked_loc_desc_namespace`。
- 隔离：`revoked_caps_outside`、`revoked_loc_outside`、`revoked_parent`。
- 操作：`revoke_fail`、`revoke_success`、`revoke_idempotent`。
- 不变量：`wellFormed_active`、`revoke_preserves_active_wf`、`revoked_caps`。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (15 jobs)`，exit 0 | Lean 模型构建 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Revoke.lean`（`lean/`） | PASS；15 条定理，3 条无公理，其余 `propext`/`Classical.choice`/`Quot.sound` | 公理审计 |

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- **经典模型**：`Desc` 的可判定性来自 `Classical.propDecidable`，`revoked` 为
  noncomputable，无法 `#eval`；因此没有可执行锚，也不能与 Rust 自动对拍。
  这不是 `sorry` 或 `native_decide`，但证据强度低于可执行切片。
- **范围变化**：原 `WellFormed` 对 inactive 子树不再适用；模型证明的是
  `ActiveWellFormed` 的保持。该变化与规范第 1 条“被撤销 token 进入 Inactive、
  保留记录”一致，已在 README 登记。
- 不覆盖：并发/移动竞争、跨节点 `InTransit`、在途消息/命名空间的收回、审计
  容量预演与事件、对象销毁回收、`cap_drop` 的完整授权语义、W3 唯一位置、真实
  Rust 实现或 7-D 退出条件。
- Bootstrap `revoke_capability` 的 host 测试与 SPEC-022/023 证据是独立实现证据，
  不是本模型的自动对拍。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile 或 ABI。7-D 仍要求并发、会话/映射/
DMA 与未决请求的完成点分别定义；本切片只覆盖其单线程、HeldBy 根的抽象子集。

## 后继关系

- 契约：[Prima OS §6.2](../../../Prima%20OS.md)、[COSIR §4.7](../../../COSIR%20原语规范.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- 实现证据：[Bootstrap 撤销实现注记](../../../Prima%20OS.md)（SPEC-022/023）。
