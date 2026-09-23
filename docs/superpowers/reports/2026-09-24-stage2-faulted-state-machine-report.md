# 切片 Y 报告：`TaskStatus::Faulted` 状态机与 §12.5 迁移入口（ISSUE-TASK-001）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-24
>
> 对象：`kernel/src/{bootstrap,sched,syscall,protocol,resume,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 `ISSUE-TASK-001`——在内核状态机补 §5.2 的 `Faulted`/`pending_exc`/`kernel_resume_frame`
> 并与规范对齐，含 §12.5 迁移入口与真实 QEMU 见证

## 0. 结论速览

- **状态机已落地**：`TaskStatus::Faulted` + `TaskRecord.pending_exc`（进 `BootstrapState`）+
  `kernel_resume_frame`（**细化层**持有——`Option<SretFrame>` 是 168 B，×8 任务单独即超出
  `BootstrapState` 16 KiB 守卫的余量）。
- **§12.5 迁移入口已落地**：新增 `TaskTable::fault`（前置 4：必须是 `Running`，否则拒绝且状态
  原子不变；成功动作 3：置 `Faulted` + 写唯一 `pending_exc`）与完整形态
  `exc_raise_on`（含通知递送）；`finish_destroy` 补 §8.2 步骤 6 的两字段清理，`schedule`
  补 §8.4 步骤 3 的清空。
- **判决性见证（新增独立事件 `fault_ready`）**：
  `task=7;before=1;after=3;raised=1;pending_source=1;pending_kind=1;precond_rejected=1;invariant_ok=1;unschedulable=1`
  ——保留槽位任务 7 被合法调度为 `Running` 后经 §12.5 置 `Faulted` 并写入 `pending_exc`；
  同时对 Ready 的 init raise 被拒（前置 4 反例）、W9 双向不变量成立、`Faulted` 不可被调度恢复。
- **一条必须登记的取舍**：boot 期 `page_fault_source` 已有内核侧订阅者，若见证走**完整**
  `raise_exception`（含递送）会往其邮箱塞一条通知，**实测扰动既有 IPC 证据**
  （`ipc_ready` 的 `recv1/cap_count/queued` 与 `user_cow_ready.err` 全变）。故 boot 见证只走
  **状态机本体**（前置 4 + 动作 3 + `TaskStateChanged` 审计）；完整形态由 `exc_raise_on` 实现并以
  host 单测钉住；递送一侧由既有 `exc_pending_ready` 见证覆盖（本切片未改动、实测逐字段不变）。
- **两个出口未实现（留独立 SPEC）**：§12.10 `exc_resume` 用户化、§8.2 `KernelActor` 无 cap 回收。
  故 `Faulted` 在本切片内是终态。
- **不勾选任何交付项与退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不是组织级第三方审查，不构成 `REFINEMENT-PROOF`。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `bootstrap.rs` | `TaskStatus::Faulted`；`PendingExc`；`TaskRecord.pending_exc`；`TaskTable::fault`（§12.5 前置 4 + 动作 3）与 `check_faulted_invariant`（W9 双向）；`finish_destroy` 清两字段；`schedule` 清 `kernel_resume_frame`；`ExcKind` 增 4 类 + `is_hardware`/`is_recoverable`；`ExcError::NotRunning`；`exc_raise_on`（完整 §12.5 形态）；`FAULT_WITNESS_TASK_ID = 7`、`FaultWitness`、`fault_witness`/`record_fault_witness`/`fault_witness_record` |
| `sched.rs` | `KERNEL_RESUME_FRAMES`（细化层持有 §5.2 字段）+ `set_/kernel_resume_frame/clear_kernel_resume_frame` |
| `syscall.rs` | `ExcError::NotRunning → Errno::Again` 映射 |
| `main.rs` | 在 `activate_init` **之前**采集 `Faulted` 见证 |
| `resume.rs` / `protocol.rs` | 上报 `fault_ready`；`status_code` 稳定编码 |
| `xtask/src/protocol.rs` | `EventKind::FaultReady`、`EXPECTED_EVENTS` 31→32、`validate_fault_data` + 4 条反例、golden 夹具更新；`task_ready` 的 `tasks`/`tasks_after` 期望 2→3 并写明原因 |

### 2.1 见证为什么用保留槽位且在 `activate_init` 之前

- **`activate_init` 之前**：§8.4 的 `schedule` 只允许至多一个 `Running`。boot 早期尚无任何 Task
  处于 `Running`，故"建见证 Task → 调度为 Running → §12.5 置 Faulted"是一条**合法**路径，
  不需要放宽任何前置。
- **保留槽位 id 7**：普通 id 由 `next_free_id` 从 2 起分配。若见证 Task 占用 2/3，既有
  `child=3`/`grandchild=4` 等 id 证据会整体顺移；保留槽位只让"存活 Task 数"变化（实测
  `task_ready` 仅 `tasks`/`tasks_after` 2→3，其余字段逐字段不变）。
- 见证 Task 是**裸 Task 记录**（`create_with_entry`）：不取帧、不铸 Capability、不建地址空间，
  故 `frames_ready.table`/`segment` 与全部 cap 计数**不变**。

### 2.2 为什么不走完整的 `raise_exception`

首版见证调用 `exc_raise_on`（含通知递送）。实测事件流 diff：

```
user_cow_ready: err 0x7(write_protection) → 0x4(not_present)
ipc_ready:      recv1 0x51a5…0001 → 0x10001；cap0 0x100000008 → 0x0；
                magic 0x51a5…0001 → EINVAL；queued/direct/blocks 3/3/3 → 4/2/2
task_ready:     tasks 2 → 3（预期之内）
```

任务 1 的第一次 `SYS_RECV` 收到了 `0x10001`（= page_fault 通知 payload），说明 boot 期
`page_fault_source` 已有内核侧订阅者、递送往其邮箱塞了一条通知。该副作用与 `ISSUE-TASK-001`
无关，且会污染既有 IPC 证据。改为只走状态机本体后，上述字段全部恢复基线（见 §4）。

## 3. 实测证据（2026-09-24）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30662;image=813;free=30662;regions=3;kstart=0x1000000;kend=0x132d000
PRIMA|v=1|seq=24|event=task_ready|status=ok|data=…;child=3;tasks=3;caps=29;root_cleared=1;grandchild=4;tasks_after=3;caps_after=29;root4_cleared=1
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31475;free=30552;segment=19;table=91;image=813;conserved=1;disjoint=1
PRIMA|v=1|seq=30|event=self_root_ready|status=ok|data=root=0x5b000;kernel_root=0xa000;image_table=0xb000;owned=1;pages=167;translated=0x1000000
PRIMA|v=1|seq=31|event=fault_ready|status=ok|data=task=7;before=1;after=3;raised=1;pending_source=1;pending_kind=1;precond_rejected=1;invariant_ok=1;unschedulable=1
PRIMA|v=1|seq=32|event=boot_complete|status=ok
```

守恒：`30552 + 19 + 91 + 813 = 31475 = universe` ✅；`conserved=1`、`disjoint=1`。
`table=91` 与 `segment=19` 均**不变**——本切片不取帧、不新增 Segment。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **296**、xtask **157** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**32 条事件**序列 + `validate_fault_data` + 4 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件 | 变化 | 原因 |
|---|---|---|
| `memory_ready` | `usable` 30671→30662、`image` 804→**813**、`kend` 0x1324000→**0x132d000** | 状态机与见证代码使镜像 +9 帧 |
| `bootstrap_ready` | `image_len` 3289112→3328528、`image_frames`→813、`free_frames`→30662 | 同上 |
| `task_ready` | `tasks`/`tasks_after` 2→**3**；其余字段（含 `child=3`/`grandchild=4`/`caps=29`/`root_cleared=1`）**逐字段不变** | 存活 Task 增加**保留槽位的 Faulted 见证 Task**（Faulted 是存活状态） |
| `preempt_ready` | `rsp1` 0x10000088228→**0x10000089988**、`rsp2`/`kstack*_top` 同步后移 | 镜像增长后内核栈后移 |
| `frames_ready` | `free` 30561→**30552**、`image` 804→**813**；**`table=91`、`segment=19` 不变** | 镜像增长；不取帧、不新增段 |
| `self_root_ready` | `pages` 165→**167** | 镜像页随代码增长 |
| **新增** `fault_ready` | seq=31 | 本切片判决性见证 |
| `boot_complete` | seq 31→**32** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 25 条 | **逐字段不变** | 含 `user_cow_ready`、`ipc_ready`、`exc_ready`、`exc_pending_ready`、`syscall_ready` 等 |

以上按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. host 单测

见设计文档 §3（kernel 新增 8 条、xtask 新增 4 条反例）。核心是把**前置 4 的原子性**
（拒绝时字段级不变）、**W9 双向不变量**、**§8.2/§8.4 的两条清空路径**、**§12.10 可恢复性矩阵**
与**非硬件异常不改状态**都钉死。

## 6. 边界与反例保留

- **见证的是 §12.5 的 KernelActor 进入路径，不是真实 CPU trap**：`idt` 的 `#PF` dispatcher 仍恒
  跳过/重试故障指令，把真实 trap 改为可产生 `Faulted` 属后续切片。本切片不宣称"真实硬件故障 →
  Faulted"。
- **两个出口未实现**：`Faulted` 在本切片内是**终态**（`schedule` 只接受 `Ready`，故调度器不会
  隐式恢复；唯一出边是 §8.2 销毁，也不是"恢复"）。
- `kernel_resume_frame` **无写入者**（属 §12.10 用户化切片）；W9 的相关蕴含式在无写入者时
  空真成立。存储与两条清空路径已实现并测试。
- `TaskStateChanged` 的**调度路径补齐**仍缺（`ISSUE-TASK-003`，S3-D5）。
- `ExcKind` 新增种类尚未接入真实 trap 分类（`idt` 仍只按 `PageFault`/`SyscallTrap` 递送）。
- `ISSUE-TASK-001` 的**闭合判断交复核者**；不自行宣告满足解除条件。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。

## 7. 下一步

1. 三节（W/X/Y）的**范畴外复核**（组织级门槛）——这是阶段 2 剩余的唯一独立性义务。
2. 后续候选（各独立 SPEC）：§12.10 `exc_resume` 用户化（`Faulted → Ready` + `TaskResumed`）、
   §8.2 `KernelActor` 无 cap 回收、真实 trap → `Faulted`、`ISSUE-TASK-003`（调度路径
   `TaskStateChanged`，留 S3-D5）、§14 `{R,X}` 的链接脚本级段切分。
