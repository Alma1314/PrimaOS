# 切片 Y 设计：`TaskStatus::Faulted` 状态机与 §12.5 迁移入口（ISSUE-TASK-001）

> 文档类型：真实内核实现设计（阶段 2 收尾，第三节）
>
> 日期：2026-09-24
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-23/24，范围项逐项确认）

## 0. 定位与前置

本切片是「闭合阶段 2 剩余登记项」的第三节，承接已交付的切片 W（SPEC-068）与 X（SPEC-069）。
跟踪表 `ISSUE-TASK-001` 原文大意：

> 内核 `TaskStatus` 仍缺规范 §5.2 已定义的 `Faulted`（SPEC-058 已在规范侧补齐状态与字段）。
> 解除条件：在内核状态机补 `Faulted`/`pending_exc`/`kernel_resume_frame` 并与规范对齐。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| 状态枚举 | §5.2（`COSIR 原语规范.md:358`）`TaskStatus = Ready \| Running \| BlockedRecv \| Faulted` | 内核 `TaskStatus` 只有 `Ready/Running/BlockedRecv/Dead` |
| Task 字段 | §5.2（`:387-388`）`pending_exc: Option<PendingExc>`、`kernel_resume_frame: Option<Bytes>` | `TaskRecord` 两者皆无 |
| 进入 Faulted | §12.5（`:1382`）：硬件执行异常时把 `faulting_task`（前置 4：必须 `Running`）置 `Faulted`、写入唯一 `pending_exc`、追加 `TaskStateChanged(Running → Faulted)` | `exc_raise` **从不改 Task 状态**，也无 Running 前置校验 |
| 退出 Faulted | 仅两条：§12.10 成功 `exc_resume`（`Faulted → Ready`，记 `TaskResumed`）；§8.2 内核回收 / `task_destroy` | 均无 |
| 不变量 | W9（`:1941-1948`）：`status = Faulted ⇔ pending_exc ≠ None`；`Faulted` 的 `pending_return = None` | 无该状态，不变量无从谈起 |

### 0.1 已确认的关键决策（用户逐项确认，2026-09-23/24）

1. **范围**：状态机（`Faulted` + `pending_exc` + `kernel_resume_frame`）+ §12.5 迁移入口 +
   真实 QEMU 见证。
2. **两个出口留独立 SPEC**：§12.10 `exc_resume` 的**用户化**、§8.2 的 **`KernelActor` 无 cap 回收路径**
   都是独立工作项，不并入本切片。
3. **切片划分**：W/X/Y 各为独立 SPEC、各自报告。

### 0.2 三条决定实现的容量/结构事实（侦察所得）

- `Option<SretFrame>` 是 **168 B**，×`MAX_TASKS = 8` ≈ 1344 B——**单独就超出 `BootstrapState`
  16 KiB 守卫的余量**（实测 15240 B，余量 1144 B）。故 `kernel_resume_frame` 必须放**细化层**
  （§5.2 本就写明该字段由细化层持有），`pending_exc`（≈8 B ×8）才进 `BootstrapState`。
- 内核**没有对 `TaskStatus` 的穷尽 `match`**（全是 `==`/`!=`），故新增变体**不会编译失败**——
  改造面是语义性的，必须靠不变量检查与见证兜住，不能指望编译器。
- `exc_raise` 的**通知递送**会往订阅者邮箱塞消息。boot 期 `page_fault_source` 已有内核侧订阅者，
  故见证若走完整 `raise_exception` 会扰动既有 IPC 证据（实测：`ipc_ready` 的
  `recv1/cap_count/queued` 全部改变）。见 §1.3。

## 1. 设计

### 1.1 状态与字段

- `TaskStatus` 增 `Faulted`（`Dead` 保留为细化态）。
- `TaskRecord` 增 `pending_exc: Option<PendingExc>`；`PendingExc { source_id: u32, kind: ExcKind }`。
- `sched`（细化层）新增 `KERNEL_RESUME_FRAMES: [Option<SretFrame>; MAX_TASKS]` 与
  `set_/kernel_resume_frame/clear_kernel_resume_frame`。**本切片不写入**该字段（写入者是成功的
  `exc_resume`，属后续切片），但实现并钉住两条清空路径。

### 1.2 `TaskTable` 的迁移与不变量

- 新增 `fault(task_id, source_id, kind)`：**§12.5 前置 4**（必须是 `Running`，否则 `NotRunning`
  且状态原子不变）+ **成功动作 3**（置 `Faulted`、写唯一 `pending_exc`）。
- 新增 `check_faulted_invariant()`：钉住 W9 的**双向**不变量 `Faulted ⇔ pending_exc != None`。
- `finish_destroy` 增 §8.2 步骤 6 的清理：`pending_exc = None` + `clear_kernel_resume_frame`。
- `schedule` 增 §8.4 步骤 3 的清理：进 `Running` 时清空 `kernel_resume_frame`。
- 既有迁移全部不碰 `pending_exc`，且 `schedule` 只接受 `Ready`，故 **`Faulted` 无法经调度离开该状态**
  （唯一的出边是 §8.2 销毁，属本切片；§12.10 恢复属后续切片）。

### 1.3 `ExcKind` 扩展与 `exc_raise_on`

`ExcKind` 增 `GeneralProtectionFault`/`DivideError`/`InvalidOpcode`/`DoubleFault`（§12.10 的
可恢复集 + 不可恢复的 `DoubleFault`），并加 `is_hardware()`（仅 `SyscallTrap` 为否）与
`is_recoverable()`。既有稳定编码 1/2 不变，新增 3/4/5/6——**既有事件不受影响**。

新增 `BootstrapState::exc_raise_on(source_id, kind, faulting_task)` 实现 §12.5 的完整形态：
硬件异常走前置 4 + 动作 3 + `TaskStateChanged` 审计，随后**仍按订阅表递送**（§12.9：未订阅也
照样进入 `Faulted`，只是不产生通知）；非硬件异常不改状态，与既有 `exc_raise` 等价。

### 1.4 QEMU 见证：保留槽位 + boot 期合法路径

见证在 `main.rs` 中 **`activate_init` 之前**采集：

```
tasks.create_with_entry(FAULT_WITNESS_TASK_ID = 7, 0)   // 保留槽位，不占普通 id
tasks.fault(INIT_TASK_ID, …)        → 被拒（init 此刻 Ready）——§12.5 前置 4 反例
tasks.schedule(7)                   → Ready → Running（此刻无任何 Task Running，合法）
tasks.fault(7, PAGE_FAULT_SOURCE_ID, PageFault)  → Running → Faulted + pending_exc
```

- **为什么必须在 `activate_init` 之前**：§8.4 的 `schedule` 只允许至多一个 `Running`；boot 早期
  尚无任何 Task 处于 `Running`，故"建见证 Task → 调度为 Running → 置 Faulted"是一条**合法**路径，
  不需要为造证据放宽任何前置。
- **为什么用保留槽位 id 7**：普通 id 由 `next_free_id` 从 2 起分配，见证 Task 若占 2/3 会让
  `child=3`/`grandchild=4` 等既有 id 证据整体顺移；保留槽位只让"存活 Task 数"变化。
- **状态机本体不经 `raise_exception` 的递送**：boot 期 `page_fault_source` 已有内核侧订阅者，
  经完整 `raise_exception` 会往其邮箱塞一条通知，扰动既有 IPC 证据（实测）。故 boot 见证只走
  **前置 4 + 成功动作 3 + `TaskStateChanged` 审计**；完整形态（含递送）由 `exc_raise_on` 实现并由
  host 单测钉住；`raise_exception` 的**递送**一侧由既有 `exc_pending_ready` 见证覆盖（本切片未改动、
  实测逐字段不变）。这是一条**有意不扰动既有证据**的取舍，登记在案。

见证结构 `FaultWitness`：`task_id`、`before`、`after`、`raised`、`pending_source`、`pending_kind`、
`precond_rejected`、`invariant_ok`、`unschedulable`。作为**新增独立事件** `fault_ready` 落在
`self_root_ready` 之后（只顺移 `boot_complete`）。

## 2. 回归面（逐条登记，实测自事件流逐条 diff）

| 事件 | 改前（SPEC-069） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30671;image=804;kend=0x1324000` | `usable=30662;image=813;kend=0x132d000` | 状态机与见证代码使镜像 +9 帧 |
| `bootstrap_ready` | `image_len=3289112;image_frames=804;free_frames=30671` | `image_len=3328528;image_frames=813;free_frames=30662` | 同上 |
| `task_ready` | `tasks=2;…;tasks_after=2` | `tasks=**3**;…;tasks_after=**3**` | 存活 Task = init + task 2 + **保留槽位的 Faulted 见证 Task**（Faulted 是存活状态）。其余字段（含 `child=3`/`grandchild=4`）**逐字段不变**——保留槽位使普通 id 不顺移 |
| `preempt_ready` | `rsp1=0x10000088228` 等 | `rsp1=0x10000089988` 等 | 镜像增长后内核栈后移 |
| `frames_ready` | `free=30561;table=91;image=804` | `free=30552;table=**91**;image=813` | 镜像增长；**`table` 与 `segment=19` 不变**（本切片不取帧、不新增段） |
| `self_root_ready` | `pages=165` | `pages=167` | 镜像页随代码增长 |
| **新增** `fault_ready` | — | `task=7;before=1;after=3;raised=1;pending_source=1;pending_kind=1;precond_rejected=1;invariant_ok=1;unschedulable=1` | 本切片判决性见证 |
| `boot_complete` | seq=31 | seq=**32** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 25 条 | — | **逐字段不变** | 含 `ipc_ready`、`exc_ready`、`exc_pending_ready`、`user_cow_ready` 等 |

**特别登记**：`ipc_ready`/`exc_*`/`user_cow_ready` 的**逐字段不变**是本切片一条有意守住的结果——
首版经完整 `raise_exception` 递送时它们全部改变（见 §1.4 最后一条），改用只走状态机本体后恢复。

## 3. host 单测（新增 8 条，kernel 288→296；xtask 153→157）

- `faulting_a_running_task_sets_pending_exc_and_faulted_status`
- `faulting_a_non_running_task_is_rejected_without_state_change`（前置 4，字段级不变）
- `faulted_invariant_holds_across_transitions_and_destroy`（W9 双向 + §8.2 步骤 6 清空两字段）
- `exception_kind_classification_matrix`（§12.10 可恢复集 + `DoubleFault` 不可恢复）
- `exc_raise_on_keeps_status_for_non_hardware_kinds`（`SYS_TRAP` 语义不变）
- `exc_raise_on_faults_a_running_task_and_delivers_to_nobody`
- `exc_raise_on_rejects_a_non_running_faulting_task`
- `kernel_resume_frame_storage_contract`（细化层契约，含越界 id 无操作）
- xtask 4 条反例：`before` 非 Running、`after` 非 Faulted、`pending_exc` 不符、三条前提标志为 0。

## 4. 验证

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **296**、xtask **157** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**32 条事件**序列 + `validate_fault_data` 及 4 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。
- 事件流逐条 diff 复核改动面恰为 §2 所列。

## 5. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」（组织级门槛，与阶段 0/1 一致）。
- **见证的是 §12.5 的 KernelActor 进入路径，不是真实 CPU trap**：把 `#PF` dispatcher 改成
  可产生 `Faulted`（当前恒跳过/重试故障指令）另属后续切片。
- **两个出口未实现**：§12.10 `exc_resume` 用户化（`Faulted → Ready`）与 §8.2 `KernelActor`
  无 cap 回收路径。故 `Faulted` 在本切片内是**终态**，见证 Task 留在任务表中。
- `kernel_resume_frame` 只实现存储与清空，**无写入者**（写入者属 §12.10 用户化切片）；
  W9 中"`kernel_resume_frame ≠ None` 蕴含曾成功 `exc_resume`"在无写入者时空真成立。
- `TaskStateChanged` 的**调度路径补齐**（`yield`/抢占/`block`/`wake`）仍缺，属 `ISSUE-TASK-003`
  （S3-D5），不在本切片。
- `ExcKind` 新增的种类尚未接入真实 trap 分类（`idt` 仍只按 `PageFault`/`SyscallTrap` 递送）。
- `ISSUE-TASK-001` 的**闭合判断交复核者**：状态机与字段已按 §5.2 补齐、§12.5 迁移入口有真实见证，
  但"与规范对齐"是否要求两个出口同时具备，由复核者裁定。
