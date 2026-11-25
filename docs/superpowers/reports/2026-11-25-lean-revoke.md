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
`lean/Formal/Revoke.lean` 与 `lean/Checks/Revoke.lean`。25 条普适定理通过
`lake build`；**4 条无公理，其余仅依赖 `propext`/`Quot.sound`，无
`Classical.choice`**。`revoked`/`revoke` 为可执行普通定义，带 11 个 `#eval` 锚。

首版模型以经典命题可判定性判定子树，依赖 `Classical.choice` 且不可执行；本次
把子树判定改为燃料有界的 `Bool` 链遍历（`descB`/`descAt`），在 `WellFormed`
下证明与 `Desc` 等价，并保留原有全部定理名。它仍不是 7-D 实现证据、并发撤销
证明或 Rust 源码细化。

## 基线与输入

- 基线：`5583cfc`（另加 `fd844ac`、`01dc0fe`、`5cc574b`、`d212cb1`）。
- 契约：[Prima OS §6.2 实现注记](../../../Prima%20OS.md)（撤销域）、COSIR §4.7
  （第一版无 `cap_revoke`，探索实现先行）。
- 复用：`Formal.State` 的 `Capability`/`Location`/`State`/`WellFormed`。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 Revoke 节。

## 实现

- `Desc s root c`：`c = root` 或沿 `cap.parent` 指针可达 root 的归纳谓词，保留为
  规范陈述；`revoked` 不再用它做运行期判定。
- `descB n s root c`：`Bool` 递归，每步消耗燃料沿 `parent` 上行；`descAt` 固定
  燃料 `c + 1`。`descB_sound` 对任意燃料健全；`descB_succ`/`descB_mono` 给出燃料
  单调；`desc_ge_root` + `descB_complete` 在 `WellFormed`（父 CapId 严格小于子）
  下证明燃料 `c + 1` 完备。
- `revoked s root`：子树内 capability 置 `active := false`；子树内 `HeldBy`
  位置删除；`InMessage`/`InNamespace` 位置保留；子树外状态不变；记录与 parent
  指针保留。
- `revoke s root holder`：仅当 root 当前位置为 `HeldBy holder` 时成功并返回
  `(revoked …, true)`；否则原状态返回 `(s, false)`（宿主 `HolderMismatch`）。
- `ActiveWellFormed`：只约束 active capability 的 CDT 不变量（父存在/活动/
  更小 CapId/同对象）。原 `WellFormed` 要求所有有父项的父 active；撤销后父子
  同时 inactive，因此原谓词不再适用于该子树——这是明确的范围变化，不是对撤销前
  不变量的放宽。

已证性质（25 条）：

- 判定：`descB_self`/`descAt_self`、`descB_succ`/`descB_mono`、`descB_sound`、
  `desc_ge_root`、`descB_complete`/`descAt_sound`/`descAt_complete`。
- 子树：`desc_trans`、`not_desc_parent`。
- 失效：`revoked_caps_desc`、`revoked_loc_desc_held`、
  `revoked_loc_desc_message`、`revoked_loc_desc_namespace`。
- 隔离：`revoked_caps_outside`、`revoked_loc_outside`、`revoked_parent`。
- 操作：`revoke_fail`、`revoke_success`、`revoked_loc_root`、`revoke_idempotent`。
- 不变量：`wellFormed_active`、`revoke_preserves_active_wf`、`revoked_caps`。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (16 jobs)`，exit 0 | Lean 模型构建与 `#eval` 锚 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Revoke.lean`（`lean/`） | PASS；25 条定理，4 条无公理，其余仅 `propext`/`Quot.sound` | 公理审计 |

`#eval` 锚核对：成功守卫、根与派生子的 `active=false`、根与子项 HeldBy 删除、
子树内 `InMessage` 保留、域外 cap/位置不变、错误持有者失败且状态不变、二次撤销
失败。

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- **燃料前提**：`descAt` 的完备性依赖 `WellFormed`（祖先 CapId 严格递减）。
  `revoked_caps_desc`/`revoked_loc_desc_*`/`revoke_preserves_active_wf`/
  `descB_complete` 携带该假设；`revoked_parent`、`revoked_caps_outside`、
  `revoked_loc_outside`、`revoked_loc_root`、`revoke_fail`/`revoke_success`/
  `revoke_idempotent` 不需要。非良构状态（如父指针环）不在模型范围。
- **范围变化**：原 `WellFormed` 对 inactive 子树不再适用；模型证明的是
  `ActiveWellFormed` 的保持。该变化与规范第 1 条“被撤销 token 进入 Inactive、
  保留记录”一致，已在 README 登记。
- 不覆盖：并发/移动竞争、跨节点 `InTransit`、在途消息/命名空间的收回、审计
  容量预演与事件、对象销毁回收、`cap_drop` 的完整授权语义、W3 唯一位置、真实
  Rust 实现或 7-D 退出条件。
- Bootstrap `revoke_capability` 的 host 测试与 SPEC-022/023 证据是独立实现证据，
  不是本模型的自动对拍或 refinement proof。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile 或 ABI。7-D 仍要求并发、会话/映射/
DMA 与未决请求的完成点分别定义；本切片只覆盖其单线程、HeldBy 根的抽象子集。

## 后继关系

- 契约：[Prima OS §6.2](../../../Prima%20OS.md)、[COSIR §4.7](../../../COSIR%20原语规范.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- 实现证据：[Bootstrap 撤销实现注记](../../../Prima%20OS.md)（SPEC-022/023）。
