# 切片 AK 设计：阶段 3 第二片 —— Timer（§10 全量）

> 文档类型：真实内核实现设计（阶段 3 第二片）
>
> 日期：2026-09-25
>
> 状态：已获用户确认（2026-09-25：阶段 3 拆 **3 片**；新原语**要求 ring3 用户可见调用与证据**；
> `now` 由 **PIT tick 驱动**）

## 0. 定位与前置

本切片落实 **S3-D3**（绝对单调 Timer）与 **S3-D4**（`deadline ≤ now` 时原子立即触发），并覆盖退出
条件 **S3-X2**（每个 Fired Timer 恰有一次对应内核通知）。S3-X4 的 Timer 侧（`task_destroy` 不破坏
其他对象生命周期）也在本片落地。

规范锚点：§5.5 `Timer`/`TimerState`、§10.1–§10.6、§6.3 `enqueue_or_deliver`、§8.2 成功动作 5、
§13.1 事件清单（`TimerSet`/`TimerFired`/`TimerCancelled`）。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| `now` / `time_now` | §10.1：单调不减的抽象绝对时间；`time_now` 是纯查询 | **无 `now`**、无任何时钟抽象 |
| `timer_set` | §10.2：只能为自己创建；铸造 `Derive\|TimerCancel\|TimerDestroy` 根 cap；`deadline > now` → `Pending`，`deadline ≤ now` → **同一原子操作内** `TimerFired` + 通知 + `Fired` | 不存在 |
| `advance_time` | §10.3：`KernelActor` 内部迁移，`new_now ≥ now`；按 `(deadline, TimerId)` 稳定序处理全部到期 Timer | 不存在 |
| `timer_cancel`/`timer_destroy` | §10.4/§10.5：`Pending`→`Cancelled`（不产生通知）；`Fired`→`AlreadyFired`、`Cancelled`→`AlreadyCancelled` 均为**状态返回**非失败 | 不存在 |
| Timer 对象 | §5.5 `Timer { creator, state }`，`ObjectRef`/`ObjectKind` 里 `Timer` 已预留 | `ObjectRef` **无** `Timer`；无 Timer 表 |
| 通知 | §6.3/§10.6：恰一个 `TimerFired` 审计事件 **且** 恰一个 `FromKernel(TimerFired(id))` 的 `MessageSent`，两者指向同一 `notification: MsgId` | 内核事件投递不存在（`send_message_with_capabilities` 要求指向 receiver 的 `TaskSend`，而 `FromKernel` 无 sender Task） |
| `task_destroy` 联动 | §8.2 步骤 5：recipient 的全部 Timer，`Pending` 先 `TimerCancelled(RecipientDestroyed)`，随后一律从活动表删除 | 不存在 |

### 0.1 本片的三条实现口径（照实登记）

1. **`now` 的单位是 PIT tick**：`PIT_DIVISOR = 11932`（1193182/11932 ≈ 100 Hz），故 1 tick ≈ 10 ms。
   规范 §5.6 允许「具体平台的单调时钟用固定偏移细化抽象的零起点」——本实现取**零起点 = 首次进入
   CPL3 抢占阶段**，`now` 等于 [`preempt`] 的 `TICKS` 计数。抽象层不规定单位与精度，故这是合法细化。
2. **`advance_time` 的驱动者是 `KernelActor`**（§10.3），在实现层落在**抢占收尾点**
   （`preempt_finish`）：一次把 `now` 推进到已累计的 tick 数。`now` 只在 ring3 抢占阶段推进
   （启动期与 ring0 执行期恒为 0）。
   **为什么不逐 tick 在 `preempt_tick` 里调用**（实测踩到的坑）：那会在**中断上下文**里做通知
   投递，而投递会唤醒任务、改调度状态——此刻 `preempt_tick` 正在保存/切换上下文；并且中断处理
   时长会进入用户任务的执行时序，使 `preempt_ready.mark1` 由稳定值 `0x1` 变成 `0x0`（地址空间
   隔离的判决性证据被削弱）。收尾点推进同样满足 §10.3 的单调性与「迁移后无 `deadline ≤ now`
   的 `Pending`」两条要求，且 `now` 的最终值就是累计 tick 数（实测 `now=4 = ticks`）。
3. **Timer 表内联**：`MAX_TIMERS = 8`，`TimerObject` 约 32 B，合计约 256 B——`BootstrapState` 在
   SPEC-074 后余量约 3.2 KiB，可容；这与切片 AJ 的「大存储走 arena」不矛盾（那一片的条目含 69 B
   路径且上限 16）。

## 1. 设计

### 1.1 时间模型（`kernel/src/bootstrap.rs`）

```rust
pub type Instant = u64;

// BootstrapState 新增字段（8 B）
pub now: Instant,
```

- `time_now()` → `now`（纯查询，不改状态；§10.1 无失败分支）。
- `advance_time(new_now) -> Result<(), TimerError>`（§10.3）：
  1. `new_now < now` ⇒ `InvalidTime`，**`now` 与全部 Timer 状态不变**；
  2. 否则更新 `now`，收集全部 `Pending ∧ deadline ≤ new_now` 的 Timer，按 `(deadline, timer_id)`
     稳定序逐个：追加 `TimerFired { timer_id, deadline, fired_at: new_now }`、`enqueue_kernel_event`
     投递通知（`FromKernel(TimerFired(id))`）并取 `MsgId`、Timer 以该 `MsgId` 进入 `Fired`；
  3. 迁移后不变式：所有 `Pending` 的 `deadline > now`。

### 1.2 Timer 对象与表

```rust
pub enum TimerCancelReason { ByTask(u32), RecipientDestroyed }

pub enum TimerState {
    Pending { deadline: Instant, recipient: u32 },
    Fired { deadline: Instant, recipient: u32, fired_at: Instant, notification: u32 },
    Cancelled { deadline: Instant, recipient: u32, reason: TimerCancelReason },
}

pub struct TimerObject { pub id: u32, pub creator: u32, pub state: TimerState }

pub struct TimerTable { entries: [Option<TimerObject>; MAX_TIMERS], count: usize }
```

`ObjectRef::Timer(u32)` 加入（`ObjectKind::Timer`/`Right::TimerCancel`/`Right::TimerDestroy` 已存在，
`Rights::well_typed(Timer)` 已定义）。`target_live(ObjectRef::Timer(id))` 查 Timer 表。

根 Timer Capability 权限 `{Derive, TimerCancel, TimerDestroy}`（§10.2 成功动作 2），用 `slots.alloc()`
最小空闲槽——ring3 见证从 `SYS_TIMER_SET` 的返回值读取句柄（无需编译期常量）。

### 1.3 四个操作

| 操作 | 前置 | 成功动作 | 事件 |
|---|---|---|---|
| `timer_set(caller, deadline) -> Handle` | §6.1 Actor 约束；**无其它**（§10.2 无失败分支） | 建 Timer（`creator = recipient = caller`）；铸根 cap；`TimerSet`；`deadline > now` → `Pending`；否则**同一原子操作内** `TimerFired` + 通知 + `Fired` | `TimerSet`（+ 立即触发时 `TimerFired`） |
| `timer_cancel(caller, cap) -> TimerCancelOutcome` | `TimerCancel`；cap 指向有效 Timer 且由 caller 持有 | `Pending` → `Cancelled(ByTask(caller))` + `TimerCancelled`；`Fired` → `AlreadyFired`；`Cancelled` → `AlreadyCancelled`（后两者状态不变，**不是**失败） | `TimerCancelled`（仅前者） |
| `timer_destroy(caller, cap)` | `TimerDestroy`；同上 | 从活动表删除：`Pending` 先 `TimerCancelled`；`Fired` 已排队通知不撤回；`Cancelled` 不产生新取消事件；最后 `ObjectDestroyed` | `TimerCancelled`（条件）+ `ObjectDestroyed` |
| `advance_time` | `KernelActor` 内部 | 见 §1.1 | 每个到期 Timer 一条 `TimerFired` |

`TimerError`：`InvalidCaller`、`InvalidCap`、`InvalidTimer`、`NotHeld`、`MissingRight(Right)`、
`InvalidTime`（§10.3）。`AlreadyFired`/`AlreadyCancelled` 走**返回值**而非 `Err`。

### 1.4 内核事件投递（§6.3 / §10.6 的 exactly-once）

新增内部辅助 `enqueue_kernel_event(receiver, timer_id)`：

- **不能复用 `send_message_with_capabilities`**：它要求指向 receiver 的 `TaskSend` Capability，而
  `FromKernel` 事件没有 sender Task（§6.3「内核事件当前不携带 Capability」）；
- 因而直接复用其**投递主体**：`receiver` 处于 `BlockedRecv` 时走
  `set_pending_task`（含 §13.1 的 `→ Ready` 迁移）+ `MessageSent` + `MessageReceived`；否则走
  `ipc.send` + `MessageSent`（消息进 `messages` 表与 mailbox）；
- payload 编码 `TIMER_FIRED_TAG | timer_id` 作为不可伪造的 `FromKernel(TimerFired(id))` 载体；
- `sender` 记为 [`KERNEL_ACTOR`]（与既有异常通知同构）。

**exactly-once 判据（S3-X2）**：对每个进入 `Fired` 的 Timer，审计日志中恰有一条
`TimerFired { timer_id }` 且恰有一条 `MessageSent` 携带同一 `notification: MsgId`。

### 1.5 `task_destroy` 联动（§8.2 步骤 5）

`task_destroy`（含内核回收路径，二者共用清理体）新增：遍历 Timer 表，对 `recipient == 目标 Task`
的每个 Timer，`Pending` 先追加 `TimerCancelled { reason: RecipientDestroyed }`，随后**一律**删除并追加
`ObjectDestroyed`。删除后活动表不再含以该 Task 为 recipient 的条目（W11）。

### 1.6 ring3 面与见证

syscall：`SYS_TIME_NOW = 28`、`SYS_TIMER_SET = 29`、`SYS_TIMER_CANCEL = 30`、`SYS_TIMER_DESTROY = 31`。
`SYS_TIME_NOW` 让用户能读到 `now`（否则无法构造 `deadline > now` 的 Pending Timer）。

ring3 序列（CPL3，`cs = 0x33`）：

```
now = SYS_TIME_NOW
t1 = SYS_TIMER_SET(now + 2)      → Pending（未来 deadline）
t2 = SYS_TIMER_SET(now)          → deadline ≤ now ⇒ 立即 Fired（S3-D4）
t3 = SYS_TIMER_SET(now + 1)      → Pending
SYS_TIMER_CANCEL(t3)             → 0（Cancelled）
SYS_TIMER_CANCEL(t3)             → AlreadyCancelled（状态返回，不产生新事件）
SYS_TIMER_DESTROY(t2)            → 0（Fired 的已排队通知不撤回）
让出 → 进入抢占阶段 …
```

抢占阶段 `advance_time` 随 tick 推进，`t1`（`deadline = now + 2`）在 `now` 达到该值时到期：追加
`TimerFired` + 投递一次通知（`t1` 的 recipient 是 task 1，此刻 Ready/Running ⇒ 走排队路径）。

新事件 `timer_ready`（seq 26，紧随 `namespace_ready`）：字段
`now`、`timers`（活动数）、`sets`、`fired`、`cancelled`、`immediate_fired`、`already_cancelled`、
`destroyed`、`exactly_once_ok`、`pending_invariants_ok`。`xtask` 侧新增 `validate_timer_data`
逐字段硬断言 + 每个判决位一条反例单测。

## 2. 回归面

预期：既有 36 条事件中 `timer_ready` 使 `preempt_ready` 及之后 seq 顺移 26→27…36→37；镜像布局类
字段（`image_len`、`memory_ready`/`frames_ready` 计数、`preempt_ready` 栈地址）随代码增长变化；
**`advance_time` 在抢占阶段产生的新 `TimerFired`/`MessageSent`/`MessageReceived` 会改变
`state_audit_ready.used`**（该事件在 `after_resume` 采集，位于抢占阶段之后）——这三项逐条按维护
规则 5 登记，实测自 HEAD worktree 的逐条事件流 diff。

`preempt_ready` 自身的字段（`ticks`/`pre1`/`pre2`/`root*`/`cr3*`/`rsp*`/`data*`/`mark*`）**不应变化**
——`advance_time` 不触碰调度与 CR3。若变化即说明实现越界，须查明。

## 3. host 单测

`timer_set` 的 Pending/立即 Fired 两条分支；`advance_time` 的 `InvalidTime`（失败不改状态）、
稳定序（同一 `deadline` 多个 Timer 按 id 升序）、不变式（迁移后无 `deadline ≤ now` 的 `Pending`）；
`timer_cancel` 的三态；`timer_destroy` 的三态与已排队通知不撤回；`task_destroy` 的
`RecipientDestroyed` 与「删除后活动表无该 recipient」；exactly-once（`TimerFired` 与 `MessageSent`
各恰一次、指向同一 `MsgId`）。

## 4. 验证

`cargo test --workspace`、`cargo xtask lint`（四门）、`cargo xtask tcb`（复测登记）、
`cargo xtask test`（37 条事件）、`cargo xtask compat`；外加事件流逐条 diff 与同镜像连跑两次。

## 5. 边界（不夸大）

1. 本片**不实现** `audit(cap)` 查询（切片 AL）。
2. **不勾选任何阶段 3 交付项或退出条件**——沿用组织级独立性门槛。
3. `now` 只在 CPL3 抢占阶段推进（启动期与 ring0 执行期恒为 0）；`advance_time` 不被用户直接调用
   （§10.3 明定它是 `KernelActor` 内部迁移）。
4. Timer 的 `recipient` 恒为创建者（§10.2「调用者只能为自己创建」），故本实现不涉及「为他人设
   Timer」的授权面。
5. Timer 通知**不携带** Timer Capability（§10.2 明示）；接收方经 payload 里的 `timer_id` 识别来源。
6. safe-by-construction ≠ sound：沿用「信任调用方」契约。
