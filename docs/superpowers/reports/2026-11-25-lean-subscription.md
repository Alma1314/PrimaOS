---
id: report-lean-subscription
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# Lean `Formal.Subscription`：§12.4 / W15 订阅生命周期最小模型

## 结论与范围

本报告记录 COSIR §12.4 订阅生命周期与 W15 一致性：新增
`lean/Formal/Subscription.lean` 与 `lean/Checks/Subscription.lean`。复用
`Formal.Container` 的 Task/World 与 `Formal.TokenLocation` 的位置语义，把订阅表、
usable `ExcSubscribe` 见证、subscribe/unsubscribe、失去最后一个授权 Cap 时的
原子终止与源销毁建模为**关系步骤**，并提供可执行函数（`excSubscribe`、
`excUnsubscribe`、`dropSubCap`、`sendSubCap`、`destroySource`）与 `*_step`
桥接。39 条定理通过 `lake build`；2 条无公理，其余仅 `propext`/`Quot.sound`，
并带 `#eval` 锚。

本切片不是实现层“最后一个授权 Cap”检查器的正确性证明、订阅存续活性、审计事件
或 Rust 细化证明。

## 基线与输入

- 基线：`5583cfc`（另加 `fd844ac`、`01dc0fe`、`5cc574b`、`d212cb1`、`1fed6da`、
  `05c80a5`、`e45817d`、`fc2d026`、`a6a5e8a`、`b685573`、`c5229a4`、`664958a`）。
- 契约：COSIR §12.4（订阅生命周期）、W15（ExceptionSource 一致性）、§4.5/§4.6
  （失去最后授权 Cap 的终止触发）。
- 复用：`Formal.Container` 的 `Task`/`World`，`Formal.TokenLocation` 的
  `replace`/一致性。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 Subscription 节。

## 实现

- `Source {active, subscribers : Nat → Bool}`：订阅是每 Task 一个 bit，因此“每
  Task 每源至多一个活跃订阅”由表示保证。
- `UsableFor w holder source capId`：位置为 `HeldBy(holder)`、记录 `active`、
  含 `ExcSubscribe` 位、`obj = excSource source`。
- `W15 w`：对每个活动源，所有订阅者都是活动 Task，且各有一个 `UsableFor` 见证。
- `SubscribeStep` / `UnsubscribeStep`：§12.4 前置（持有、可用、权限、活动源、
  未订阅/已订阅）与加/清订阅位一步。
- `DropLastStep` / `SendLastStep`：位置删除/移入消息与订阅终止同步一步；
  “这是最后一个授权 Cap”是调用方前置，模型不证明该检查器。
- `DestroySourceStep`：源销毁移除订阅表条目。
- 可执行函数：`excSubscribe`/`excUnsubscribe` 检查同一组前置；`dropSubCap` 删除
  指向源的位置并终止持有者订阅；`sendSubCap` 调用 `Formal.Container.queuedSend`
  并终止发送者订阅；`destroySource` 移除源。每个成功结果由 `*_step` 桥接到对应
  关系步骤，再复用关系保持定理。

已证性质（39 条）：

- 更新投影：`setSource_self`/`_other`、`addSubscriber_self`/`_other`/`_active`、
  `clearSubscriber_self`/`_other`/`_active`、`endSub_self`/`_other`、
  `endSubCap_excSource`。
- 见证迁移：`usableFor_snap`、`usableFor_snap_of_ne`、`w15_active`、`w15_witness`。
- 生命周期：`SubscribeStep_adds`/`_preserves`、`UnsubscribeStep_clears`/`_preserves`、
  `DropLastStep_clears`/`_preserves`、`SendLastStep_clears`/`_preserves`、
  `DestroySourceStep_removes`/`_preserves`。
- 可执行：`excSubscribe_step`、`excUnsubscribe_step`、`dropSubCap_step`、
  `sendSubCap_step`、`destroySource_step`（桥接）；`excSubscribe_preserves`/`_adds`、
  `excUnsubscribe_preserves`/`_clears`、`dropSubCap_preserves`/`_clears`、
  `sendSubCap_preserves`/`_clears`、`destroySource_preserves`/`_removes`。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (21 jobs)`，exit 0 | Lean 模型构建 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Subscription.lean`（`lean/`） | PASS；39 条定理，2 条无公理，其余仅 `propext`/`Quot.sound` | 公理审计 |

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- **可执行范围**：`dropSubCap`/`sendSubCap` 只接受指向源的 Capability；其他
  Capability 的 drop/send 不触发订阅逻辑，本模块返回 `none`（通用 drop 见
  `Formal.Drop`/`Formal.Move`）。
- **lastness 检查器**：`DropLastStep`/`SendLastStep` 及其可执行对应物假定调用方
  已判定“这是最后一个授权 Cap”；模型证明保持 W15 并清除订阅位，但不证明检查器
  正确，也不证明“仍有其他授权 Cap 时订阅必须存续”的活性（liveness）。
- 不覆盖：审计事件（`ExcSubscribed`/`ExcUnsubscribed`）、源 kind 与
  `KindMismatch`、通知投递、源销毁通知、并发、stale token 的源销毁路径，或
  Rust 细化。`W15` 与 `Container.WorldOk`/CDT `WellFormed` 为独立不变量，未证明
  联合保持。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile 或 ABI；只把 §12.4/W15 从“未覆盖”移入
“关系步骤 + 可执行函数”，审计与活性边界保留。

## 后继关系

- 契约：[COSIR §12.4/W15](../../../COSIR%20原语规范.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- 互补：[Container W4](2026-11-25-lean-container.md)、[Move 位置迁移](2026-11-25-lean-move.md)。
