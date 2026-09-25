# 切片 AB 设计：§12.10 `exc_resume` 用户化（`KernelActor` 代调）

> 文档类型：真实内核实现设计（阶段 2 收尾，第六节）
>
> 日期：2026-09-24
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-24，口径 (A) 确认）

## 0. 定位与前置

本切片落实 `Faulted` 的**第二条出口**（切片 Y 登记、切片 AA 已做第一条"回收"）：§12.10 的
`exc_resume`（`Faulted → Ready`）。它是：

- **§13.1 的唯一例外**——`Faulted → Ready` 不以 `TaskStateChanged` 记录，而以 `TaskResumed` 记录；
  切片 Z 已把该例外显式登记为"待本切片处理"；
- **`kernel_resume_frame` 的唯一写入者**——切片 Y 只实现了存储与两条清空路径，写入者一直缺席；
- **收紧切片 AA 回收判据的前置**——AA 的判据是过渡口径（"种类不可恢复 / 源已销毁"），
  规范原文的判据「依据当前 State 不可能成功 `exc_resume`」要等本切片才有落点。

| 面 | 规范要求（`COSIR 原语规范.md:1511-1538`） | 改前现状 |
|---|---|---|
| 原语 | `exc_resume(exc_source_cap, sret_frame_seg_cap, frame_offset)` | 无 |
| 前置 5 | **调用者** Task 必须处于 `Faulted`，且有 `pending_exc` | 无 |
| 前置 6 | `pending_exc.kind` 必须可恢复（`PageFault`/`GeneralProtectionFault`/`DivideError`/`InvalidOpcode`）；`DoubleFault` 等 → `UnrecoverableFault` 且**保持 `Faulted`** | 无 |
| 前置 8 | 调用者的 `kernel_resume_frame = None` | 无（字段无写入者） |
| 成功动作 | 1 清零 Segment 中 `[frame_offset, frame_offset+0xA0)` 已消费帧字节；2 清 `pending_exc`；3 存 `kernel_resume_frame`；4 追加 `TaskResumed`；5 `Faulted → Ready`；**整体原子** | 无 |

### 0.1 已确认的关键决策（用户确认，2026-09-24）

**口径 (A)：`KernelActor` 代故障任务调用。** 规范要求"调用者 Task 处于 `Faulted`"，而细化层中
`Faulted` 任务不在 `Running`、无法发起 syscall；故由内核在故障路径上代调，`sret_frame_seg_cap`
由**内核持有的静态缓冲**承担。两项偏差照实登记：

1. **调用者偏差**：`Faulted` 任务的"调用"由 `KernelActor` 代为执行（与 §12.9 未订阅也照样进入
   `Faulted`、只能经内核路径处置的既有取向一致；也与切片 AA 的 §8.2 内核路径同构）。
2. **`sret_frame_seg_cap` 偏差**：POSIR 侧该参数是持有恢复现场的 Segment Capability；本细化层由
   内核持有的静态缓冲（[`RESUME_FRAME_BUFFER`]）承担——与 SPEC-066 的 `HeldByKernel` 同一取向
   （内核私有、不铸用户可持 cap）。

## 1. 设计

### 1.1 内核持有的恢复帧缓冲

```rust
pub const RESUME_FRAME_BUFFER_LEN: usize = 0x200;
static mut RESUME_FRAME_BUFFER: [u8; RESUME_FRAME_BUFFER_LEN];
pub fn install_resume_frame(frame_offset: usize, bytes: &[u8]) -> Result<(), ExcResumeError>;
pub fn resume_frame_bytes(frame_offset: usize) -> Option<[u8; SRET_FRAME_LEN]>;
```

成功动作 1「清零 Segment 中已消费的 frame bytes」在此**等价为清零该缓冲的对应字节**。

### 1.2 原语实现（`kernel/src/bootstrap.rs`）

- `BootstrapState::exc_resume_faulted(task_id, frame)`：前置 5（`Faulted` 且有 `pending_exc`）、
  前置 6（`kind.is_recoverable()`，否则 `UnrecoverableFault`）、前置 8
  （`sched::kernel_resume_frame(task_id).is_none()`）、审计容量预检。全部通过后才提交：
  清 `pending_exc`、`Faulted → Ready`（`TaskTable::resume_faulted`）、
  `sched::set_kernel_resume_frame`、追加 `AuditEvent::TaskResumed`。
- `kernel_exc_resume(task_id, frame_offset, landing, stack_bottom, stack_top)`（KernelActor 入口）：
  从缓冲读帧 → `validate_for_kernel_resume`（**验证先于任何状态写入**）→ 调上面的方法 → 成功后
  执行动作 1（清零该段字节）。

**动作顺序登记**：§12.10 把"清零"列为动作 1，本实现放在状态提交之后。因为清零无法失败、
且中间态不可观测，这不改变"整体原子"的语义；显式登记以免与规文字面混淆。

`AuditEvent::TaskResumed { task_id }` 为本切片新增（`AuditEvent` 尺寸仍为 24 B，审计容量不受影响）。

### 1.3 判决性见证（新增独立事件 `resume_ready`）

采集窗口与 W/X/Y/Z/AA 相同——**`activate_init` 之前**（此刻无 Task `Running`，`schedule` 合法）。
全过程只用**保留槽位任务 5**（正是 §8.2 回收见证里那个"可恢复 ⇒ 不得回收"的任务），
一条序列覆盖 §12.10 的全部动作与两个反例：

1. 构造一帧合法恢复现场（`rip = resume_landing`、`rsp = 恢复栈顶-16`、`cs/ss` 内核选择子、
   固定位 + ZF 的 `rflags`、两个哨兵寄存器），编码后装入内核缓冲；
2. `kernel_exc_resume(5, 0, …)` → 成功 → 断言：状态 `Ready`、`pending_exc` 已清、
   `kernel_resume_frame` 已写、缓冲该段**已清零**；
3. `schedule(5)` → 断言 `kernel_resume_frame` 被消费清空（§8.4 步骤 3）；
4. 以 `DoubleFault` 再置 `Faulted` → 再次代调必须返回 `UnrecoverableFault` 且**保持 `Faulted`**（前置 6）；
5. 该任务随后经 §8.2 内核路径回收为 `Dead`——生命周期收尾，**不再留下永久 `Faulted` 记录**
   （这同时消掉了切片 AA 报告里登记的"见证副作用"）。

见证结构 `ResumeWitness { task_id, resumed, status_after, pending_cleared, frame_saved,
frame_bytes_zeroed, frame_consumed, unrecoverable_rejected, reclaimed_after, invariant_ok }`。

## 2. 回归面（逐条登记，实测自事件流逐条 diff）

| 事件 | 改前（SPEC-072） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30656;image=819;kend=0x1333000` | `usable=30645;image=**829**;kend=**0x133d000**` | `exc_resume` 与见证代码使镜像 +10 帧 |
| `bootstrap_ready` | `image_len=3353912;image_frames=819` | `image_len=3392496;image_frames=829` | 同上 |
| `task_ready` | `tasks=4;…;tasks_after=4` | `tasks=**3**;…;tasks_after=**3**` | 保留槽位任务 5 在见证收尾被回收为 `Dead`，存活 Task 回到 init + task 2 + 任务 7；其余字段**逐字段不变** |
| `preempt_ready` | `rsp1=0x1000008bca8` 等 | `rsp1=0x1000008eeb8` 等 | 镜像增长后内核栈后移 |
| `frames_ready` | `universe=31475;free=30546;image=819` | `universe=**31474**;free=30535;image=**829**` | 镜像 +10 帧使可用帧减少 1；**`table=91`、`segment=19` 不变** |
| `self_root_ready` | `pages=169` | `pages=173` | 镜像页随代码增长 |
| `state_audit_ready` | `total=52;to_running=22;to_faulted=3;used=131` | `total=**54**;to_running=**23**;to_faulted=**4**;used=**135**` | 见证新增 `schedule(5)` 一次与 `DoubleFault` 一次（`Faulted→Ready` 按 §13.1 记 `TaskResumed`，**不计入** `TaskStateChanged`） |
| **新增** `resume_ready` | — | `task=5;resumed=1;status_after=0;pending_cleared=1;frame_saved=1;frame_bytes_zeroed=1;frame_consumed=1;unrecoverable_rejected=1;reclaimed_after=1;invariant_ok=1` | 本切片判决性见证 |
| `boot_complete` | seq=34 | seq=**35** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 26 条 | — | **逐字段不变** | 含 `ipc_ready`、`exc_*`、`user_cow_ready`、`fault_ready`、`reclaim_ready` |

## 3. host 单测（新增 2 条，kernel 303→305；xtask 163→167）

- `exc_resume_moves_a_recoverable_faulted_task_to_ready`：成功动作 2/3/4/5 逐项断言，
  且成功动作 4 **恰追加一条** `TaskResumed`。
- `exc_resume_rejects_precondition_violations_atomically`：前置 5（`Ready` 目标）、
  前置 6（`DoubleFault`）、前置 8（已有待消费帧）三条反例，全部**字段级不变**。
- xtask 4 条反例：`status_after` 非 `Ready`、`frame_bytes_zeroed=0`、`frame_consumed=0`、
  `unrecoverable_rejected=0`。

## 4. 验证

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **305**、xtask **167** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**35 条事件**序列 + `validate_resume_data` 及 4 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 5. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」（组织级门槛）。
- **两项口径偏差已登记**（§0.1）：调用者是 `KernelActor` 代调；`sret_frame_seg_cap` 由内核静态缓冲承担。
- **本切片不恢复现场执行**：`kernel_resume_frame` 被保存并在下次 `schedule` 时按 §8.4 步骤 3 清空，
  但**不**真的 `iretq` 进该帧（那是切片 D 的 ring0 跳板所证的另一件事）。故本切片证明的是
  §12.10 的**状态机与现场保存语义**，不宣称"故障任务真的被恢复执行"。
- **`exc_source_cap` 未参与校验**：§12.10 的三个参数里，本切片只用 `frame_offset`；
  `exc_source_cap`/`sret_frame_seg_cap` 的授权语义随上面的口径偏差一并登记为未实现。
- **AA 的回收判据仍是过渡口径**：本切片给出了判据的落点（`exc_resume` 的前置），但**未**回头收紧
  AA 的判词——那需要"依据当前 State 不可能成功 `exc_resume`"的完整推导（含源 cap 可达性），
  登记为后续工作。
- `ISSUE-TASK-001` 与 `ISSUE-FAULT-005` 的行状态**不因本切片改动**；闭合判断交复核者。
