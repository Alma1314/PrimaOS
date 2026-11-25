---
id: report-lean-container
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# Lean `Formal.Container`：W4 容器/位置双向一致最小模型

## 结论与范围

本报告记录 COSIR W4（容器与位置双向一致）的 Lean 参考语义：新增
`lean/Formal/Container.lean` 与 `lean/Checks/Container.lean`。复用
`Formal.TokenLocation` 的 `Snapshot`/`Consistent`/`replace` 与
`Formal.Move` 的位置概念，把 `HeldBy`/`InMessage`/`InNamespace` 与
Task/Message/Namespace 容器绑定为 `WorldOk`，并证明 send/recv/bind/unbind
的原子耦合更新保持 W3+W4。18 条定理通过 `lake build`；1 条无公理、其余仅
`propext`/`Quot.sound`，并有可执行 `#eval` 锚。

本切片是主机可执行模型证据，不是消息/Namespace 生命周期、容器销毁、订阅终止、
审计事件或 Rust 细化证明。

## 基线与输入

- 基线：`5583cfc`（另加 `fd844ac`、`01dc0fe`、`5cc574b`、`d212cb1`、`1fed6da`、
  `05c80a5`、`e45817d`）。
- 契约：COSIR W4（容器双向一致）、§4.6（移动）、W3（单一位置）。
- 复用：`Formal.TokenLocation` 的 `Consistent`/`replace`，`Formal.Move` 的转移概念。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 Container 节。

## 实现

- `Task {active}`、`Message {caps : Nat → Bool, active}`、
  `Namespace {entries : String → Option Nat, active}`：容器对象与活动位。
- `W4Ok w`：三条双向子句——`HeldBy(t)` 要求活动 Task；`locations c =
  InMessage m` ↔ 活动消息 `m` 的 caps 含 `c`；`locations c = InNamespace n p`
  ↔ 活动 Namespace `n` 的条目 `p = some c`。
- `WorldOk w := Consistent w.snap ∧ W4Ok w`：把 W3（至多一个位置、位于此则记录
  存在）与 W4 组合为联合不变量。
- `queuedSend`：`HeldBy(sender) → InMessage(msg)`，同时把 token 加入消息 caps
  （要求活动消息）；`recvFrom`：`InMessage(msg) → HeldBy(receiver)`，同时从消息
  caps 移除（要求活动接收 Task）；`bindNs`/`unbindNs`：`HeldBy ↔ InNamespace`，
  同时写/清 Namespace 条目（要求活动 Task/Namespace，绑定要求路径空闲）。
- 失败返回 `none` 且状态不变；成功时位置与容器在同一步更新。

已证性质（18 条）：

- 更新投影：`replace_loc_self`/`_other`、`messageAdd_self`/`_other`、
  `messageRemove_self`/`_other`、`namespaceSet_self`/`_other`、
  `setMessage_self`/`_other`、`setNamespace_self`/`_other`。
- 构造：`inNamespace_inj`。
- 保持：`queuedSend_preserves`、`recvFrom_preserves`、`bindNs_preserves`、
  `unbindNs_preserves`。
- 锚：`emptyWorld_ok`。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (18 jobs)`，exit 0 | Lean 模型构建与 `#eval` 锚 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Container.lean`（`lean/`） | PASS；18 条定理，1 条无公理，其余仅 `propext`/`Quot.sound` | 公理审计 |

`#eval` 锚核对：send 成功/源或消息不匹配失败、位置与消息 caps 同步、recv 移除
caps 且新 `HeldBy` 要求活动 Task、bind 同步 Namespace 条目、bind→unbind 回到
调用者、无此 Namespace 失败、Task 容器不变。

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- **函数式容器表示**：消息 caps 用 `Nat → Bool`，Namespace entries 用
  `String → Option Nat`。因此“一个 Message 的 Capability 列表无重复”与“一个
  Namespace 路径至多引用一个 Capability”由表示满足，未对 `List` 版本证明。
- **生命周期缺省**：消息/Namespace 的创建、销毁、`ns_unmount`、容器销毁与
  Task 退出不建模；`messages`/`namespaces` 只增删条目。
- **不变量组合**：`WorldOk` 不含 `Formal.State` 的 CDT `WellFormed`；两个不变量
  需独立组合，本模型不证明其联合保持。
- 不覆盖：订阅终止（§12.4）、审计事件（§13.1 `CapMoved`/`CapDropped`）、
  `InTransit`、并发移动竞争、真实 Rust 实现或 W4 的内核细化证明。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile 或 ABI；只把 W4 从“未覆盖”移入
“最小可执行模型”，列表级重复/路径唯一与生命周期边界保留。

## 后继关系

- 契约：[COSIR W4/§4.6](../../../COSIR%20原语规范.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- 互补：[Move 位置迁移](2026-11-25-lean-move.md)、[Drop 单位置删除](2026-11-25-lean-drop.md)。
