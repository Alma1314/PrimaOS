# 切片 AK 报告：阶段 3 第二片 —— Timer（§10 全量）

> 文档类型：真实内核实现报告（阶段 3 第二片）
>
> 日期：2026-09-25
>
> 设计：`docs/superpowers/specs/2026-09-25-stage3-timer-design.md`

## 1. 交付物

| 交付 | 内容 |
|---|---|
| 时间模型 | `BootstrapState.now: Instant`（u64，8 B）；`time_now`（纯查询）；`advance_time`（§10.3：`new_now < now` ⇒ `InvalidTime` 且状态不变；按 `(deadline, timer_id)` **稳定序**处理到期 Timer） |
| Timer 对象 | `ObjectRef::Timer(u32)`（`ObjectKind::Timer`/`Right::TimerCancel`/`Right::TimerDestroy` 早已预留）；`TimerState`（`Pending`/`Fired`/`Cancelled`，含 `notification: u32`）；`TimerTable`（内联，`MAX_TIMERS = 8`）；`TIMER_RIGHTS = {Derive, TimerCancel, TimerDestroy}` |
| 四个操作 | `timer_set`（未来 ⇒ `Pending`；`deadline ≤ now` ⇒ **同一原子操作内** `TimerFired` + 通知 + `Fired`，S3-D4）、`timer_cancel`（三态，`AlreadyFired`/`AlreadyCancelled` 是**状态返回**）、`timer_destroy`（`Pending` 先取消、`Fired` 已排队通知不撤回）、`advance_time` |
| 内核事件投递 | `enqueue_kernel_event`（§6.3）——**不复用** `send_message_with_capabilities`（它要 `TaskSend`，而 `FromKernel` 无 sender Task），直接复用其投递主体（直投 / 排队两条路径）；payload 编码 `TIMER_FIRED_TAG \| timer_id` |
| `task_destroy` 联动 | §8.2 步骤 5：recipient 的全部 Timer，`Pending` 先 `TimerCancelled(RecipientDestroyed)`，随后一律删除（W11）；审计预算计入 `destroy_audit_events` |
| 审计事件 | `TimerSet` / `TimerFired` / `TimerCancelled`（§13.1 三项齐备；对象销毁沿用 `ObjectDestroyed`） |
| PIT 接入 | `TICKS` 仍在 `preempt_tick` 里递增（只在 CPL3 且抢占启用时），但 `advance_time` 落在**抢占收尾点**（`resume::preempt_finish`）一次性推进到累计 tick——理由见 §6 第 2 条 |
| ring3 面 | 4 个 syscall（28–31：`SYS_TIME_NOW`/`SYS_TIMER_SET`/`SYS_TIMER_CANCEL`/`SYS_TIMER_DESTROY`）+ `user.rs` 的约 60 条指令见证块；回执缓存（`cache_timer_receipts`，因为 `after_resume` 已切回内核根、`USER_Y_VA` 不再映射） |
| 协议 | 新事件 `timer_ready`（seq 27）+ `validate_timer_data` + 6 条反例单测 |

## 2. 判决性见证（`timer_ready`，seq 27，紧随 `preempt_ready`）

```
now=4;timers=2;sets=3;fired=2;cancelled=1;destroyed=1;immediate_fired=1;
already_cancelled=1;exactly_once_ok=1;sets_ok=1;cancel_ok=1;already_cancelled_ok=1;evidence_ok=1
```

逐项判决含义：

- `now=4`：**抽象时钟由 PIT tick 推进**（ring3 阶段 `now` 恒为 0，收尾时为 4）——S3-D3 的
  「绝对单调时间」在实现层的落点；
- `sets=3` / `timers=2`：三次 `timer_set`（t1 未来、t2 立即、t3 未来），t2 被 `timer_destroy`、
  t3 被取消，活动表剩 t1 与 t3；
- `immediate_fired=1`：**t2 的 `deadline ≤ now` 在同一原子操作内触发**（S3-D4）——过去/当前的
  deadline 不是错误；
- `fired=2`：t2 的立即触发 + **t1 由 `advance_time` 在 tick 2 驱动到期**；
- `cancelled=1` / `already_cancelled=1`：`timer_cancel(t3)` 得 `Cancelled`，再次得
  `AlreadyCancelled`（§10.4 的**状态返回**，不产生新事件）；
- `destroyed=1`：`timer_destroy(t2)`（`Fired` 的已排队通知**不撤回**）；
- `exactly_once_ok=1`：**每个 `Fired` 状态的活动 Timer 恰有一条 `TimerFired` 事件、且其
  `notification: MsgId` 恰对应一条 `MessageSent`**——S3-X2 的核心判据。

## 3. 实施中的失败与反例（全部如实保留）

1. **`destroyed` 从审计日志统计是错的口径（真实缺陷，QEMU 见证暴露）**：初版用
   `AuditEvent::ObjectDestroyed` 计数，但该事件**不区分对象类型**（Segment/Namespace/Timer 都用
   它），实测得 `destroyed=4` 而非 1，`evidence_ok=0`。改为实现层计数（`TimerStats.destroyed`，
   只计用户 `timer_destroy` 成功次数）。**host 单测没覆盖这一口径**——`timer_audit` 的单测只断言
   `sets`/`fired`/`exactly_once_ok`，故只有 QEMU 端到端能暴露。
2. **新增 Capability 消耗未同步 xtask 常量**：`TASK_READY_LIVE_CAPS` 是硬编码 40，新增 3 个 Timer
   根 cap 后实测 43，`cargo xtask test` 报 `caps_after is 43, expected 40`。这是**设计使然**的显式
   失败（该常量本就要求随 cap 消耗同步），登记为「新增根 Capability 时必须复测该常量」。
3. **ring3 代码区越界（host 单测抓到）**：namespace 块 + Timer 块合计超出路径文本数据区 12 字节，
   `user_code` 内的 `assert!(at < NS_PATH_DATA_OFFSET)` 在 `user_code_bytes_are_fixed` 里失败。
   处置：路径数据紧凑排布（NUL 分隔，29 B 而非 7×8=56 B）并把数据区后移到 4040。
4. **路径数据长度算错（host 单测抓到）**：紧凑化后 `NS_PATH_DATA_LEN` 写成 24，实际串长 29 ⇒
   写入越界 panic。修正偏移常量（0/4/8/12/20/25/27）与数据区长度 32。
5. **`int3` 定位断言太弱（host 单测抓到）**：原断言用 `find(0xCC)` 找收尾 `int3`，但机器码的
   `mov r64, imm64` 立即数里可能出现 0xCC，于是匹配到中间位置、断言「其后全零」失败。改为
   `rfind`（区间内最后一个 0xCC——收尾 `int3` 之后是零填充，故它就是 `int3`）。

第 3–5 条是同一段机码的迭代性错误（都由 host 单测在编译/测试期抓住，未进入 QEMU）；第 1 条是
**语义口径错误**，只有端到端见证能暴露。

## 4. 回归面实测（事件流逐条 diff，对照切片 AJ 终点 `d55173e`）

**新增 1 条事件**（`timer_ready` 插在 `preempt_ready` 之后，因为它依赖 PIT tick 推进），故 seq 27
起顺移一位（`exc_resume_ready` 27→28 … `boot_complete` 36→37）。

**7 条既有事件的数据变化**：

| 事件 | 字段 | 旧 → 新 |
|---|---|---|
| `memory_ready` | `usable` / `image` / `kend` | 30507 → 30454 / 965 → 1018 / 0x13c5000 → 0x13fa000 |
| `bootstrap_ready` | `image_len` | 3950552 → 4165768（+215216 B） |
| `bootstrap_ready` | `image_frames` / `free_frames` | 965 → 1018 / 30507 → 30454 |
| `task_ready` | `caps` / `caps_after` | 40 → 43（+3 个 Timer 根 cap） |
| `preempt_ready` | `rsp1`/`rsp2`/`kstack*_top` | 后移 |
| `frames_ready` | `free` / `image` | 30397 → 30344 / 965 → 1018（`universe`/`segment`/`table` 不变） |
| `self_root_ready` | `pages` | 202 → 211 |
| `state_audit_ready` | `used` | 163 → 175（+12） |

**`preempt_ready` 的核心字段（`ticks`/`pre1`/`pre2`/`root*`/`cr3*`/`data*`/`mark*`）逐字段不变**
——`advance_time` 不触碰调度与 CR3，与设计预期一致（若变化即说明实现越界）。

`state_audit_ready.used` 的 +12 是逐项可核的：3 × (`TimerSet` + `CapabilityMinted`) + 2 ×
`TimerFired`（t2 立即 + t1 到期）+ `TimerCancelled`（t3）+ `ObjectDestroyed`（t2）+ 2 ×
`MessageSent`（t2、t1 各一次）。

**其余 29 条逐字段不变**（含 `vspace_ready`/`cow_ready`/全部用户阶段 IPC/Task 事件、
`namespace_ready`、`platform_segments_ready`、`authority_ready`、`fault_ready`、`reclaim_ready`、
`exc_resume_ready`、`resume_ready`）。**同镜像连跑两次输出逐字节相同**。

## 5. 验证（2026-09-25）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **351**（343→351）、prima-ostd 21、xtask **188**（182→188） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件·0 命中） |
| `cargo xtask tcb` | `trusted_lines=1217 mechanism_lines=18685 ratio=6.11%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**37 条事件**） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

**G3 基线复测**：占比 6.43% → **6.11%**（机制层 +987 行、可信面未动，故继续下降）。

## 6. 口径偏差登记

1. **`now` 的单位是 PIT tick**（约 10 ms，`PIT_DIVISOR = 11932`），零起点 = 首次进入 CPL3 抢占
   阶段；规范 §5.6 允许平台时钟以固定偏移细化零起点，抽象层不规定单位与精度。
2. **`advance_time` 的驱动者是 `KernelActor`**（§10.3），实现层落在**抢占收尾点**
   （`resume::preempt_finish`）：一次把 `now` 推进到已累计的 tick。**不逐 tick 在 `preempt_tick`
   里调用**——那会在**中断上下文**做通知投递，而投递会唤醒任务、改调度状态（此刻 `preempt_tick`
   正在保存/切换上下文），并且中断处理时长会进入用户任务的执行时序，扰动 `preempt_ready.mark*`
   这类时序敏感见证（实测：逐 tick 调用会让 `mark1` 由 `0x1` 变成 `0x0`，地址空间隔离的判决性
   证据被削弱）。收尾点推进同样满足 §10.3 的单调性与「迁移后无 `deadline ≤ now` 的 `Pending`」，
   且 `now` 的最终值就是累计 tick 数（实测 `now = 4 = ticks`）。
   用户在 ring3 阶段读到 `now = 0`（抢占尚未启用），故 `ring3` 机码按**相对偏移**构造 deadline
   而不是把 0 写死。
3. **Timer 表内联**（8 × 约 32 B ≈ 256 B）：与切片 AJ 的「大存储走 arena」不矛盾——那一片的条目
   含 69 B 路径且上限 16。
4. **通知 payload 编码** `TIMER_FIRED_TAG | timer_id` 作为不可伪造的 `FromKernel(TimerFired(id))`
   载体；sender 记为 `KERNEL_ACTOR`（与既有异常通知同构）。
5. **`timer_destroy` 后指向该 Timer 的 Capability 成为 stale token**：槽仍活动但目标不在活动表，
   故后续操作得 `InvalidTimer`（§10.5）。
6. **`destroyed` 只计用户 `timer_destroy`**（实现层计数）：审计的 `ObjectDestroyed` 不区分对象类型，
   无法单独统计 Timer 销毁。
7. **`advance_time` 在预检不过（审计/队列容量）时返回错误且不改状态**：§10.3 声明无失败分支，
   资源耗尽属 §6.2 排除项——登记为细化层映射。
8. **safe-by-construction ≠ sound**：沿用既有「信任调用方」契约。

## 7. 遗留与后续

- 阶段 3 的 **`audit(cap)` 查询**（S3-X3）未做，属切片 AL。
- **不勾选任何阶段 3 交付项或退出条件**——沿用组织级独立性门槛。
- S3-X4 的「Task 容器销毁不破坏其他对象生命周期」在本片覆盖 Timer 侧（§8.2 步骤 5）；Namespace
  侧已由切片 AJ 覆盖；Message 侧的完整反例留在切片 AL 与 Audit 查询一并给。
