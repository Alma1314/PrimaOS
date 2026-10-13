# Task Capability 审计与派生链（ISSUE-EXC-011）+ 审计存储扩容

> 文档类型：实现报告（非设计、非规范、非证明）
>
> 日期：2026-10-13
>
> 范围：`kernel/src/bootstrap.rs` 的 `task_create`（§8.1 成功动作 3/4/8）与审计容量常量。
>
> 独立性边界（如实声明）：同一会话产出，**不等价于**组织级第三方审查；不改变阶段 0/2「进行中」与勾选状态。

## 1. 缺口（`ISSUE-EXC-011`）

§8.1 成功动作 8 明文要求 `task_create` 追加「`TaskCreated` **以及本操作中全部 Capability 铸造/移动事件**」。实现有两处不符：

1. **动作 3/4 铸造 `manager_cap`/`self_cap` 不发 `CapabilityMinted`** —— `mint_root_capability` 只 `can_append(1)` **预留**额度、由调用者补发（`ns_create` 即如此，`bootstrap.rs:4144-4147`），`task_create` 漏补；
2. **动作 4 的 self Cap 应为「从管理 Cap 派生」**（§8.1 L700 原文），实现却走 `mint_root_capability`（`parent = None`）——应发 `CapabilityDerived` 而非 `CapabilityMinted`，W2 溯源链自此处断裂。

## 2. 前置：审计存储墙（实测）

先尝试直接补事件，QEMU 失败，根因是**容量**而非语义：

- 仅补 manager 一条 `CapabilityMinted`（每次 `task_create` +1）⇒ `state_audit_ready` 报 **`used=254 / cap=256`**，随即 `timer_ready.now` 由 >0 变 **0**、`evidence_ok=0`（某处 `AuditFull` 级联致抽象时钟不推进）；
- 再把 self Cap 改 `derive`（再 +1/次）⇒ 内核 **seq 30 panic**；
- 全部回退后基线重跑，6 项全 PASS——确认改动引起，非环境。

即 `ISSUE-TASK-003` 所记的「进一步存储增长必须先做审计存储重构」在这里先兑现。

## 3. 修复

**扩容**：`MAX_AUDIT_EVENTS` 256 → **512**。SPEC-074 已把 `AuditLog` 移出 `BootstrapState`（进 arena），其注释明写「容量不再受 `BootstrapState` 守卫约束，故提到 256 以留足余量」——512 是**既定升级路径**，不占结构体、不受 16 KiB 守卫约束。

**两处补全**（`task_create`）：

```
// 动作 3：铸造即补发
self.audit.append(AuditEvent::CapabilityMinted { cap: manager_cap, holder: caller });
// 动作 4：self Cap 由 mint 改为从 manager Cap 派生
let Ok(self_cap) =
    self.derive_capability_to_holder(caller, manager_cap, MANAGER_TASK_RIGHTS, new_id)
```

`derive_capability_to_holder` 内部追加 `CapabilityDerived{cap, parent: manager_cap, holder: new_id}`，接通 W2 溯源链（`audit(self_cap)` 现可上溯至 manager Cap）。审计顺序符合 §8.1 动作序 3 → 4 → 6 → 7 → 8。

## 4. 风险评估更正

先前登记时判「`self_cap` 句柄被 ring3 机码当立即数引用（`user.rs`），改来源风险高」——**该判断有误**：`self_cap` 经 `syscall.rs` 的 `register_created_task(outcome.self_cap.encode(), …)` **运行期传入**、写入新 Task 初始帧的 `rdi`（`user.rs:3497-3507`），不是内嵌立即数。故句柄来源可变，无稳定性风险。

## 5. 回归面

- 每次 `task_create` 审计 **+2 条**（`CapabilityMinted` + `CapabilityDerived`）；
- `total`/`to_running`/`to_ready`/`to_blocked`/`to_faulted`（`TaskStateChanged` 计数）**不变**——两条新事件均非状态迁移；
- `cap` 字段 256 → 512（容量）；其余事件 `seq`/`data` 不变；
- `validate_output` 为结构化约束校验（非逐字节钉值），故 `used`/`cap` 数值变动不构成期望串失配。

## 6. 门禁

- `cargo test -p kernel` ⇒ **367 passed / 0 failed**；
- `cargo test -p xtask` ⇒ **204 passed / 0 failed**；
- `cargo xtask test` ⇒ `PASS image bios / image uefi / qemu bios run=1 / qemu bios run=2 / stage1 boot slice`；
- `cargo fmt --check` 无差异；`cargo clippy -p kernel -p xtask --all-targets -- -D warnings` 零告警。

## 7. 边界

- 闭合判断交复核者；阶段 0/2 门槛与勾选状态不动；
- `MAX_AUDIT_EVENTS` 提到 512 后余量充足，但仍是有限量——再增长须登记。
