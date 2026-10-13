# 阶段 0 / 内核 Audit 修复：bootstrap init Task 的创建事件（ISSUE-EXC-010）

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-13
>
> 范围：`kernel/src/bootstrap.rs` 的 §14 步骤 9 初始 Audit 序列；`xtask/src/protocol.rs` 的 `state_audit_ready` 期望。
>
> 独立性边界（如实声明）：本报告由同一会话产出，**不等价于**组织级第三方审查；不改变阶段 0「进行中」与 S0-X1 未勾选。

## 1. 发现

承 `ISSUE-EXC-009`（同日，cosir 侧补写 §14 步骤 9 初始 Audit）。该行把「cosir 用 `TaskCreated`、内核用 `TaskStateChanged`」记为 cosir 的**有意分歧**。复核判定该**定性反向**：

- W13（`COSIR 原语规范.md` §17）：「每个活动对象都有**创建事件**」；
- §13.1 事件种类并列 `TaskCreated / TaskStateChanged / TaskResumed / TaskDestroyed`——`TaskCreated` 是创建事件，`TaskStateChanged` 只用于**状态迁移**；
- 内核自身 `task_create` 路径（`kernel/src/bootstrap.rs:5359-5361`，注释「成功动作 8：TaskCreated」）正是用 `TaskCreated`；
- 唯 `bootstrap`（`kernel/src/bootstrap.rs:6718-6731`）以 `TaskStateChanged{init, Ready}` 记录 init Task 的**创建**。

故缺口在**内核侧**：init Task 无 `TaskCreated` 创建事件，违反 W13。cosir（`ISSUE-EXC-009`）的 `TaskCreated` 才是规范合规的一侧。

## 2. 修复

`kernel/src/bootstrap.rs`：§14 步骤 9 的第四条事件

```
- AuditEvent::TaskStateChanged { task_id: INIT_TASK_ID, status: TaskStatus::Ready }
+ AuditEvent::TaskCreated { task_id: INIT_TASK_ID }
```

即与同文件 `task_create` 的成功动作 8 取齐。§14 步骤 8 的「init Task 初始状态为 Ready」由创建事件承载（与 `task_create` 同形）。

`xtask/src/protocol.rs`：`state_audit_ready`（seq 39）期望

```
- data=total=52;to_running=22;to_ready=22;to_blocked=5;to_faulted=3;used=131;cap=170
+ data=total=51;to_running=22;to_ready=21;to_blocked=5;to_faulted=3;used=131;cap=170
```

「`total` 必须等于分项之和」负例的替换锚点同步 `total=52;` → `total=51;`。

## 3. 影响面

- 事件**条数不变**（仍 4 条，`used=131`、`cap=170` 不变）；仅一条事件的**种类**由 `TaskStateChanged` 变 `TaskCreated`；
- `total` 与 `to_ready` 各 −1（`total` 是 `TaskStateChanged` 的分项和，创建事件不再计入）；
- 事件 `seq` 与其余 41 条事件数据不变。

## 4. 门禁

- `cargo test -p kernel` ⇒ **367 passed / 0 failed**；
- `cargo test -p xtask` ⇒ **204 passed / 0 failed**；
- `cargo xtask test` ⇒ `PASS image bios / image uefi / qemu bios run=1 / qemu bios run=2 / stage1 boot slice`（两次 bios run 证明事件流与新的 `TaskCreated` 一致）。

## 5. 未消项 / 边界（如实登记）

- cosir 的 `AuditKind::TaskCreated` 携带 `status`，内核的 `TaskCreated` 只带 `task_id`——**模型抽象差异**，本次不改 cosir（`ISSUE-EXC-009` 的 `TaskCreated{task, status}` 保持）；
- 本项为**实现补齐**，规范 §13.1/§17 W13 无需改动；
- 闭合判断交复核者。
