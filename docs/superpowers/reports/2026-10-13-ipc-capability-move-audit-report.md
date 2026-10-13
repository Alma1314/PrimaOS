# IPC 携带 Capability 的位置变化审计（ISSUE-EXC-012）

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-13
>
> 范围：`kernel/src/bootstrap.rs` 的 `send_message_with_capabilities` / `receive_message` / `destroy_task_body`。
>
> 独立性边界（如实声明）：同一会话产出，**不等价于**组织级第三方审查；不改变阶段 0/2「进行中」与勾选状态。

## 1. 缺口（`ISSUE-EXC-012`）

扫描范围：`task_destroy`/`task_destroy_self` 的审计与 Capability 撤销路径（由 `ISSUE-EXC-011` 的 §13.1 合规检查延续）。

`task_destroy` 自身的 cap 撤销是合规的——`revoke_task_capabilities` → `revoke_capability` 对每棵被撤销的子树追加 `CapabilityRevoked`（SPEC-031）。但沿该路径查到的**消息通道**三处一直静默：

1. **`send_message_with_capabilities` 排队投递**：cap 位置 `HeldBy(sender)` → `InMessage(msg)`（`positions.move_to_message`），**不追加事件**；
2. **`send_message_with_capabilities` 直投**（receiver 处于 `BlockedRecv`）：`HeldBy(sender)` → `HeldBy(receiver)`（`move_to_task`），**不追加事件**；
3. **`receive_message`**：`InMessage(msg)` → `HeldBy(receiver)`（`move_from_message`），**不追加事件**；
4. **`destroy_task_body`**：消息内 Capability 被 `retire_capability` **静默删除位置**（§8.2 动作 3），无任何丢弃事件（动作 9 要求）。

均违反 §13.1「**任何** Capability 位置变化都必须追加 `CapMoved`（删除位置时为 `CapDropped`）」。后果：§13.2 `audit(cap)` 结果 3「本 cap 自创建以来的全部位置变化」对**经 IPC 传递**的 Cap 不完整——`IPC` 见证用的共享段 Cap 正是此类。

**登记依据**：`SPEC-031` 行早已以「不等同于 **IPC 内部自动移动的完整审计**」声明该边界，但**从未登记为 ISSUE**。按维护规则 5「未登记 gap = 缺陷」，本轮登记并修复。

## 2. 修复

- `send_message_with_capabilities`：两分支各在 `positions.move_*` 后追加 `CapabilityMoved`
  - 直投 → `to = HeldBy(receiver)`；排队 → `to = InMessage(message.id)`；
  - `from` 取迁移前 `positions.find(cap).location`（与 `move_capability` 同法）。
- `receive_message`：`move_from_message` 后追加 `CapabilityMoved`（`InMessage` → `HeldBy(receiver)`）。
- `destroy_task_body`：消息内 Cap 的 `retire_capability` 后追加 `CapabilityDropped`；`dropped_cap_count` 计入 `audit_events` 预检（`destroy_audit_events` 同步）。
- 审计预检同步：send `(if direct { 2 } else { 1 }) + capability_ids.len()`；recv `1 + MAX_MESSAGE_CAPABILITIES`（上界，`receive` 已取出消息，取上界以免回滚）；destroy `+ dropped_cap_count`。

## 3. 回归面

- 经 IPC 传递的 Capability **每次迁移新增 1 条审计**；
- `cargo test -p kernel` 的单测 `ipc_transfers_capability_atomically_through_message` 由 9→**10** 条：直投路径新增的 `CapabilityMoved{cap: IMAGE_CAP_HANDLE, from: HeldBy(1), to: HeldBy(2)}` 位于 `MessageSent` 之前（`set_pending_task` 的唤醒 `TaskStateChanged` 之后），故断言取 `len() - 4`；
- 其余事件的 `seq`/`data` 不变。

## 4. 门禁

- `cargo test -p kernel` ⇒ **367 passed / 0 failed**；
- `cargo test -p xtask` ⇒ **204 passed / 0 failed**；
- `cargo xtask test` ⇒ `PASS image bios / image uefi / qemu bios run=1 / qemu bios run=2 / stage1 boot slice`；
- `cargo fmt --check` 无差异；`cargo clippy -p kernel -p xtask --all-targets -- -D warnings` 零告警。

## 5. 边界

- 闭合判断交复核者；阶段 0/2 门槛与勾选状态不动；
- `receive_message` 的预检取 `MAX_MESSAGE_CAPABILITIES` 上界（而非按实际消息），是刻意的保守取值，代价是极端容量下可能提前 `AuditFull`；当前容量 512 下不可达。
