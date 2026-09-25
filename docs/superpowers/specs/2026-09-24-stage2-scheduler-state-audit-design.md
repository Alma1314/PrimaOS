# 切片 Z 设计：调度路径的 `TaskStateChanged` 与审计容量（ISSUE-TASK-003）

> 文档类型：真实内核实现设计（阶段 2 收尾，第四节）
>
> 日期：2026-09-24
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-24，范围按跟踪表登记项）

## 0. 定位与前置

跟踪表 `ISSUE-TASK-003` 原文大意：

> §13.1/W13 要求每次 Task 状态迁移都追加 `TaskStateChanged`，而调度路径
> （`yield`/抢占/`block`/`wake`）全部未追加；`BootstrapState` 实测 15240 B、guard 上限 16384 B，
> 余量约 47 条 24 B 事件，补齐须先做审计存储重构。目标切片：S3-D5（对象生命周期 Audit）。

这条 ISSUE 同时是**阶段 2 退出条件 S2-X1 的登记缺口**：S2-X1 未勾选的理由之一即
「调度路径的 `Running↔Ready`/`BlockedRecv` 迁移事件仍缺（`ISSUE-TASK-003`）」。故本切片虽被
排到 S3-D5，仍是阶段 2 内部唯一未闭合的实现缺口。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| 状态迁移审计 | §13.1（`COSIR 原语规范.md:1544`）「**每次** Task 状态迁移都必须追加 `TaskStateChanged`（唯一例外：§12.10 的 `Faulted → Ready` 以 `TaskResumed` 记录），覆盖 `Running↔Ready`、`Running→BlockedRecv`、`BlockedRecv→Ready`、`Running→Faulted`」 | 只有 `activate_init`（init 首次 `Ready→Running`）与 SPEC-070 的 `Faulted` 各追加一条；`schedule`/`yield`/`block`/`wake`/pending 直投唤醒**全部未追加** |
| 审计容量 | — | `MAX_AUDIT_EVENTS = 128`，boot 末期实测 **81/128** |

## 1. 实测：为什么必须扩容量（并更正两处判断）

### 1.1 首版估数不完整（更正自己的量测）

探针统计"会被补上事件的迁移次数"时得 33，据此一度判断"81 + 32 = 113 ≤ 128，无需扩容量"。
实现后 QEMU 实测 `used=128; cap=128` —— **审计被恰好占满**。真实 `TaskStateChanged` 条数为
**50**（探针漏计了 `receive_message` 空队列阻塞、`send_message` 投递唤醒、`prepare_user_ipc_caps`
等路径的迁移）。

**更正**：以 QEMU 实测的 `used`/目标状态分类计数为准，不以探针估数为准。

### 1.2 容量确实不足（ISSUE 的判断成立）

`size_of`：`AuditEvent` = 24 B（`Option` 经 niche 优化同为 24 B），`BootstrapState` = **15304 B**
（SPEC-070 的 `pending_exc` 使其 +64 B），守卫 16384 B → 仅余 **1080 B = 45 条**。
补齐后审计需求恰为 **128 条 = 原容量**：**零余量**——再多一条事件（任何切片）都会让受审计操作
以 `AuditFull` 失败，或在无预检的路径上越界。故扩容是必需的。

### 1.3 处置：按项目既有先例扩容量，而不是重构

跟踪表写的"补齐须先做审计存储重构"（把 `AuditLog` 移出 `BootstrapState`）。**本切片不做重构**，
理由与代价登记如下：

- 项目既有先例（切片 R）：`MAX_SEGMENT_OBJECTS` 16→32 时，"把 16 KiB 结构体守卫**继续保留**并
  在测试里钉住实测值"——**调整容量、保留守卫、显式登记**。本切片照此办理。
- 重构的影响面：`BootstrapState` 内 79 处 `.audit` 访问 + 12 处测试断言；且审计改为模块级静态后，
  host 单测（并行线程）会共享同一日志而互相污染。相对收益（把日志容量与该结构解耦），
  在本切片的需求下不足以抵偿回归风险。
- **因此把容量 128 → 170**（`15304 + 42 × 24 = 16312 ≤ 16384`，守卫未放宽），并在测试中把
  **"余量只剩 72 B"** 钉住：任何进一步存储增长都**必须先做审计存储重构**——这是本切片显式
  登记的已知瓶颈，不留含糊。

## 2. 设计

### 2.1 迁移事件的落点与原子性

在 `BootstrapState` 级包装上追加事件（`TaskTable` 保持纯状态迁移）：

| 包装 | 迁移 | 事件 |
|---|---|---|
| `schedule_task` | `Ready → Running` | `TaskStateChanged(Running)` |
| `yield_task` | `Running → Ready` | `TaskStateChanged(Ready)` |
| `block_recv_task`（新） | `Running\|Ready → BlockedRecv` | `TaskStateChanged(BlockedRecv)` |
| `wake_recv_task`（新） | `BlockedRecv → Ready` | `TaskStateChanged(Ready)` |
| `set_pending_task`（新） | `BlockedRecv → Ready`（pending 直投） | `TaskStateChanged(Ready)` |

要点：

- **容量预检先于迁移提交**（§6.5 先验证后原子提交）：`can_append(1)` 失败即返回
  `TaskError::AuditFull` 且**状态不变**；不静默丢事件、也不写出半个迁移。
- **无迁移则不追加**：`wake_recv_task` 对非 `BlockedRecv` 目标、`set_pending_task` 对非阻塞目标
  都不追加——避免把"通知"误记成"迁移"。
- 把 3 处 `self.tasks.*` 的直接迁移调用改走包装，使**所有**迁移都经过审计点。
- `exc_raise` 的审计预检随之从 `1 + direct*2 + queued` 改为 `1 + direct*3 + queued`
  （待递送唤醒时每多一条迁移事件）。
- **`Dead` 不追加**：它是细化态、不是 POSIR 状态，§13.1 的枚举里没有它。登记在案。

### 2.2 判决性见证（新增独立事件 `state_audit_ready`）

`TaskStateAudit { to_running, to_ready, to_blocked, to_faulted, used, cap }`，
由 `BootstrapState::task_state_audit()` 遍历审计日志按**目标状态**分类统计。xtask 断言：

- 四类目标状态计数**都非零**——即 `schedule`、`yield`/抢占、`block`/`wake`、§12.5 故障
  四条路径都真的接上了事件（这正是 S2-X1 登记缺口的内容）；
- `total` 等于分项之和；
- `used ≤ cap` 且 `cap ≥ total`——**日志未被打满**（余量枯竭会让后续受审计操作失败）。

事件落在 `fault_ready` 之后（只顺移 `boot_complete`）。

## 3. 回归面（逐条登记，实测自事件流逐条 diff）

| 事件 | 改前（SPEC-070） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30662;image=813;kend=0x132d000` | `usable=30659;image=**816**;kend=**0x1330000**` | 迁移审计与见证代码使镜像 +3 帧 |
| `bootstrap_ready` | `image_len=3328528;image_frames=813` | `image_len=3338936;image_frames=816` | 同上 |
| `preempt_ready` | `rsp1=0x10000089988` 等 | `rsp1=0x1000008aa88` 等 | 镜像增长后内核栈后移 |
| `frames_ready` | `free=30552;table=91;image=813` | `free=30549;table=**91**;image=816` | 镜像增长；**`table`/`segment` 不变**（本切片不取帧、不新增段） |
| `self_root_ready` | `pages=167` | `pages=168` | 镜像页随代码增长 |
| **新增** `state_audit_ready` | — | `total=50;to_running=22;to_ready=22;to_blocked=5;to_faulted=1;used=128;cap=170` | 本切片判决性见证 |
| `boot_complete` | seq=32 | seq=**33** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 26 条 | — | **逐字段不变** | 含 `ipc_ready`、`exc_ready`、`exc_pending_ready`、`task_ready`、`user_cow_ready` |

**特别登记**：`ipc_ready`/`exc_*`/`task_ready` 逐字段不变，说明新增的审计事件**没有挤掉任何
既有事件、也没有让任何投递因容量不足而失败**（旧容量 128 恰好够，但余量为零）。

## 4. host 单测（新增 3 条，kernel 296→299；xtask 157→160）

- `every_scheduler_transition_appends_a_state_change_event`：四条迁移各追加一条且目标状态正确；
  无迁移的重复唤醒**不**追加。
- `transition_is_rejected_when_the_audit_log_is_full`：日志填满后 `schedule_task` 返回
  `AuditFull` 且任务状态字段级不变（原子性）。
- `task_state_audit_counts_each_target_status`：汇总按目标状态分类（相对基线断言）。
- xtask 3 条反例：某路径分项为 0、日志溢出（`used > cap`）、`total` 与分项不符。

## 5. 验证

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **299**、xtask **160** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**33 条事件**序列 + `validate_state_audit_data` 及 3 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 6. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」（组织级门槛）。
- **§13.1 的唯一例外未在本切片处理**：`Faulted → Ready` 应以 `TaskResumed` 记录，但该迁移的
  实现（§12.10 `exc_resume` 用户化）属后续切片，故本切片不产生 `TaskResumed`。
- **`Dead` 无事件**（细化态，非 POSIR 状态）——登记为有意决定；若复核者认为 `task_destroy`
  的"对象消失"也应留痕，则需另立迁移类型。
- **审计容量已近枯竭**（16312/16384，余 72 B）：任何进一步的结构体存储增长都必须先做
  **审计存储重构**（把 `AuditLog` 移出 `BootstrapState`）。这是本切片显式登记的瓶颈。
- 容量不足时调度迁移会**显式失败**（`TaskError::AuditFull`）；当前 `sched::switch_to` /
  `block_to` / `exit_to` 用 `.expect(...)` 处理调度返回值，故容量耗尽会以 panic 终止而非静默
  继续。容量余量（42 条）使其在本切片不触发，但这条"失败即 panic"的形态登记为已知粗糙点。
- `ISSUE-TASK-003` 的**闭合判断交复核者**。
