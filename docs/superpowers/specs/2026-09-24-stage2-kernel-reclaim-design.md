# 切片 AA 设计：§8.2 内核回收路径（`KernelActor`，无 `task_cap`）

> 文档类型：真实内核实现设计（阶段 2 收尾，第五节）
>
> 日期：2026-09-24
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-24，路线项确认）

## 0. 定位与前置

切片 Y（SPEC-070）在报告里登记了一条未实现项：

> **两个出口未实现**：§12.10 `exc_resume` 用户化（`Faulted → Ready`）与 §8.2 `KernelActor`
> 无 cap 回收路径。故 `Faulted` 在本切片内是**终态**（无人能回收）。

本切片落实**后者**——§8.2 规范侧定义的「内核回收路径」。相关的 `ISSUE-FAULT-005`
（Faulted Task 回收可达性）在 SPEC-058 时被标为 Fixed，但那是**规范侧**的定义
（"定义 Faulted Task 的可达回收路径（如内核 actor 回收或显式终止原语）"）；内核实现一直到
切片 Y 都缺这条路径。故本切片不是"重开已完成项"，而是**把规范侧机制落到内核**。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| 回收路径 | §5.2（`COSIR 原语规范.md:396`）「不可恢复异常（如 `DoubleFault`）使 Task 永久保持 `Faulted`，只能经 §8.2 的内核回收路径（`KernelActor`，**无需 `task_cap`**）回收」；§8.2 L735 同 | `task_destroy` 一律要求调用者持有 `TaskDestroy` 的 cap；无 KernelActor 路径 |
| 回收判据 | §12.6 L1440：源销毁后该故障不可再恢复，只能经内核心回收 | 无判据 |
| 清理义务 | §8.2 步骤 6：清空目标的 `pending_exc` 与 `kernel_resume_frame` | 切片 Z 起已由 `finish_destroy` 承担 |

### 0.1 已确认的关键决策（用户确认，2026-09-24）

「回收判据的细化口径」是本切片唯一的分叉：§12.10 原文的判据是「依据当前 State 不可能成功
`exc_resume`」，而 `exc_resume` 用户化属**后续切片**、尚无落点。经用户确认，**先把判词落在
现有可判定量上**，并显式登记待收紧：

- 目标为 `Faulted`，且
- `pending_exc` 的种类**不可恢复**（§12.10 前置 6，如 `DoubleFault`），**或**
- 该故障的异常源**已不存在**（§12.6 L1440：源销毁后不可能再恢复）。

登记：§12.10 落地后，本判词应收紧为「依据当前 State 不可能成功 `exc_resume`」——彼时才有
`kernel_resume_frame`、源 cap 可达性等更细依据。

## 1. 设计

### 1.1 判据与回收入口（`kernel/src/bootstrap.rs`）

```rust
pub fn is_unrecoverable_faulted(&self, task_id: u32) -> bool
pub fn kernel_reclaim_task(&mut self, task_id: u32, root_phys: Option<u64>, phys_offset: u64)
    -> Result<(), TaskError>
```

- **前置**（全部先于任何写入）：`task_id != INIT_TASK_ID`（沿用 `CannotDestroyInit`）；
  `is_unrecoverable_faulted(task_id)` 为真（否则 `TaskError::NotUnrecoverable`）；
  审计容量预检（`destroy_audit_events`）。
- **成功动作**：拆除目标地址空间（`root_phys` 给出时，与 `task_destroy` 同一语义）→
  §8.2 步骤 7 从所有异常源订阅集合移除 → `destroy_task_body`（步骤 2/3/4/8/9 +
  `finish_destroy`，其中含 §8.2 步骤 6 清空 `pending_exc` 与 `kernel_resume_frame`）。
- **共用抽取**：把 `task_destroy` 里原有的 vspace 拆除抽成 `teardown_vspace`，两条销毁路径共用，
  避免同一段逻辑两份实现。

### 1.2 判决性见证（新增独立事件 `reclaim_ready`）

采集窗口与切片 Y 的 `Faulted` 见证相同——**`activate_init` 之前**（此刻无任何 Task 处于
`Running`，§8.4 的 `schedule` 合法）。用两个**保留槽位**任务（id 6 / id 5），不占用普通 id、
不铸 cap、不取帧：

1. **反例先行**：id 5 建好后调度为 `Running`，以**可恢复**的 `PageFault` 置 `Faulted`
   → `kernel_reclaim_task(5)` 必须返回 `NotUnrecoverable` 且任务字段级不变（它仍是 §12.10 的候选）。
2. **正例**：id 6 同样调度后以**不可恢复**的 `DoubleFault` 置 `Faulted`
   → `is_unrecoverable_faulted(6)` 为真 → `kernel_reclaim_task(6)` 成功 → 目标不再存活、
   `pending_exc` 已清空、W9 双向不变量仍成立。

见证结构 `ReclaimWitness { task_id, negative_task, unrecoverable, reclaimed, negative_rejected,
after_dead, pending_cleared, invariant_ok }`。xtask 侧断言三项以外的全部标志为 1，且
`task != negative_task` 且均非零。

## 2. 回归面（逐条登记，实测自事件流逐条 diff）

| 事件 | 改前（SPEC-071） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30659;image=816;kend=0x1330000` | `usable=30656;image=**819**;kend=**0x1333000**` | 回收路径与见证代码使镜像 +3 帧 |
| `bootstrap_ready` | `image_len=3338936;image_frames=816` | `image_len=3353912;image_frames=819` | 同上 |
| `task_ready` | `tasks=3;…;tasks_after=3` | `tasks=**4**;…;tasks_after=**4**` | 存活 Task 增加 `reclaim_ready` 的**反例任务 5**（回收见证的任务 6 已 Dead，不计入）；其余字段（`child=3`/`grandchild=4`/`caps=29`/`root_cleared=1`）**逐字段不变** |
| `preempt_ready` | `rsp1=0x1000008aa88` 等 | `rsp1=0x1000008bca8` 等 | 镜像增长后内核栈后移 |
| `frames_ready` | `free=30549;table=91;image=816` | `free=30546;table=**91**;image=819` | 镜像增长；**`table`/`segment` 不变**（不取帧、不新增段） |
| `self_root_ready` | `pages=168` | `pages=169` | 镜像页随代码增长 |
| `state_audit_ready` | `total=50;to_faulted=1;used=128` | `total=**52**;to_faulted=**3**;used=**131**` | 见证新增两次 `Running→Faulted` 迁移（两任务各一次）；容量 170 尚有 39 槽余量 |
| **新增** `reclaim_ready` | — | `task=6;negative_task=5;unrecoverable=1;reclaimed=1;negative_rejected=1;after_dead=1;pending_cleared=1;invariant_ok=1` | 本切片判决性见证 |
| `boot_complete` | seq=33 | seq=**34** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 25 条 | — | **逐字段不变** | 含 `ipc_ready`、`exc_*`、`user_cow_ready`、`fault_ready` |

## 3. host 单测（新增 5 条，kernel 299→303；xtask 160→163）

- `kernel_reclaim_destroys_an_unrecoverable_faulted_task`：`DoubleFault` → 回收成功 → `Dead`、
  `pending_exc` 清空、不再存活、W9 不变量仍成立。
- `kernel_reclaim_rejects_a_recoverable_faulted_task`：`PageFault`（源存活）→ `NotUnrecoverable`
  且字段级不变。
- `kernel_reclaim_rejects_non_faulted_and_init`：`Ready` 目标与 init 都被拒。
- `destroyed_fault_source_makes_a_faulted_task_unrecoverable`：源记录被移除后，可恢复种类也变成
  不可恢复终态（§12.6 L1440 分支）。
- xtask 3 条反例：`negative_rejected`/`after_dead`/`pending_cleared` 任一为 0。

## 4. 验证

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **303**、xtask **163** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**34 条事件**序列 + `validate_reclaim_data` 及 3 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 5. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」（组织级门槛）。
- **回收判据是过渡口径**（§0.1）：以"种类不可恢复 / 源已销毁"近似"不可能成功 `exc_resume`"，
  待 §12.10 落地后必须收紧。这是一条**已知会过期的判据**，显式登记。
- **`exc_resume` 仍缺**：本切片只做"回收"，不做"恢复"。故 `Faulted` 的**唯一**出边现在是把
  目标销毁；§12.10 的 `Faulted → Ready` 与 `TaskResumed`（§13.1 的唯一例外）仍属后续切片。
- **地址空间拆除依赖调用方给出 `root_phys`**：内核回收方若未持有根帧，只能做模型清理
  （与 `task_destroy` 的既有语义一致）。见证任务的裸记录本就无地址空间。
- 见证在 boot 期留下一个**永久 Faulted 的反例任务（id 5）**——它刻意不可回收（正是不变量
  的见证）；登记为见证的副作用。
- `ISSUE-FAULT-005` 的规范侧早已 Fixed，本切片只是补齐内核实现，**不改该行状态**；
  实现侧的闭合判断交复核者。
