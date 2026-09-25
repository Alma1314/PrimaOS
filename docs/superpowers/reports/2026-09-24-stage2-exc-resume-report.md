# 切片 AB 报告：§12.10 `exc_resume` 用户化（`KernelActor` 代调）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-24
>
> 对象：`kernel/src/{bootstrap,resume,protocol,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 `Faulted` 的第二条出口——§12.10 `exc_resume`（`Faulted → Ready` + `TaskResumed`），
> 即 §13.1 的唯一例外与 `kernel_resume_frame` 的唯一写入者

## 0. 结论速览

- **§12.10 已落地（口径 A：`KernelActor` 代故障任务调用）**：`BootstrapState::exc_resume_faulted`
  实现前置 5/6/8 与成功动作 2/3/4/5，`kernel_exc_resume` 承担"读帧 → 先验证 → 提交 → 清零"的
  内核入口；`AuditEvent::TaskResumed` 新增。
- **判决性见证（新增独立事件 `resume_ready`，seq 34）**：
  `task=5;resumed=1;status_after=0;pending_cleared=1;frame_saved=1;frame_bytes_zeroed=1;frame_consumed=1;unrecoverable_rejected=1;reclaimed_after=1;invariant_ok=1`
  ——一条序列覆盖 §12.10 的全部成功动作、§8.4 步骤 3 的帧消费、前置 6 的不可恢复拒绝，
  并把该任务经 §8.2 内核路径回收收尾。
- **`kernel_resume_frame` 不再是"无写入者"的字段**：切片 Y 只实现了存储与两条清空路径，
  本切片补上写入者（成功动作 3），并在下次 `schedule` 时按 §8.4 步骤 3 被消费清空。
- **两项口径偏差照实登记**：调用者由 `KernelActor` 代调（`Faulted` 任务无法发起 syscall）；
  `sret_frame_seg_cap` 由内核持有的静态缓冲承担（同 SPEC-066 `HeldByKernel` 取向）。
- **不勾选任何交付项与退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不是组织级第三方审查，不构成 `REFINEMENT-PROOF`。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `bootstrap.rs` | `ExcResumeError`（7 个失败名，对齐规范错误名）；`RESUME_FRAME_BUFFER`（0x200，内核持有）+ `install_resume_frame`/`resume_frame_bytes`；`AuditEvent::TaskResumed`；`TaskTable::resume_faulted`；`BootstrapState::exc_resume_faulted`（前置 5/6/8 + 动作 2/3/4/5）；`kernel_exc_resume`（读帧→验证→提交→清零）；`ResumeWitness` + `record_resume_witness` + `resume_witness_record` + 2 条 host 单测 |
| `resume.rs` | `resume_target()` 暴露切片 D 的 `(landing, stack_bottom, stack_top)`；收尾上报 `resume_ready` |
| `main.rs` | 在 `activate_init` **之前**、紧随 §8.2 回收见证之后采集 |
| `protocol.rs` / `xtask` | 新事件 + `validate_resume_data` + 4 条反例 + `EXPECTED_EVENTS` 34→35 |

### 2.1 验证先于写入

`kernel_exc_resume` 的顺序是：**读缓冲 → `SretFrame::decode` → `validate_for_kernel_resume`
→ 调状态方法（其内部再次做全部前置，然后一次性提交）→ 成功后才清零已消费字节**。
故任一失败都不会改动 State，也不会改动缓冲（"整体原子"）。§12.10 把"清零"列为动作 1，
本实现放在提交之后——因清零无法失败且中间态不可观测，语义等价，已显式登记。

### 2.2 为什么见证只用保留槽位任务 5

任务 5 正是 §8.2 回收见证里"**可恢复 ⇒ 不得回收**"的那个反例。在它身上接着走完 §12.10 的
生命周期，一条序列就能覆盖：恢复成功（动作 1–5）→ 帧被消费 → 不可恢复拒绝（前置 6）→
内核回收收尾。**副作用被消掉**：切片 AA 报告里登记的"boot 期留下永久 `Faulted` 记录"
在本切片后不再存在（该任务最终为 `Dead`）。

## 3. 实测证据（2026-09-24）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30645;image=829;free=30645;regions=3;kstart=0x1000000;kend=0x133d000
PRIMA|v=1|seq=24|event=task_ready|status=ok|data=…;child=3;tasks=3;caps=29;root_cleared=1;grandchild=4;tasks_after=3;caps_after=29;root4_cleared=1
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31474;free=30535;segment=19;table=91;image=829;conserved=1;disjoint=1
PRIMA|v=1|seq=32|event=state_audit_ready|status=ok|data=total=54;to_running=23;to_ready=22;to_blocked=5;to_faulted=4;used=135;cap=170
PRIMA|v=1|seq=33|event=reclaim_ready|status=ok|data=task=6;negative_task=5;…;reclaimed=1;negative_rejected=1;…
PRIMA|v=1|seq=34|event=resume_ready|status=ok|data=task=5;resumed=1;status_after=0;pending_cleared=1;frame_saved=1;frame_bytes_zeroed=1;frame_consumed=1;unrecoverable_rejected=1;reclaimed_after=1;invariant_ok=1
PRIMA|v=1|seq=35|event=boot_complete|status=ok
```

守恒：`30535 + 19 + 91 + 829 = 31474 = universe` ✅；`conserved=1`、`disjoint=1`。
`table=91`、`segment=19` **不变**。审计 `used=135 ≤ cap=170`。

`state_audit_ready` 里 `to_running=23`（+1，见证的 `schedule(5)`）与 `to_faulted=4`（+1，见证的
`DoubleFault`），而 `Faulted → Ready` **不计入** `TaskStateChanged`——正是 §13.1 声明的那条例外。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **305**、xtask **167** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**35 条事件**序列 + `validate_resume_data` + 4 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件 | 变化 | 原因 |
|---|---|---|
| `memory_ready` | `usable` 30656→30645、`image` 819→**829**、`kend` 0x1333000→**0x133d000** | `exc_resume` 与见证代码使镜像 +10 帧 |
| `bootstrap_ready` | `image_len` 3353912→3392496、`image_frames`→829 | 同上 |
| `task_ready` | `tasks`/`tasks_after` 4→**3**；其余字段（含 `child=3`/`grandchild=4`/`caps=29`）**逐字段不变** | 保留槽位任务 5 在见证收尾被回收为 `Dead` |
| `preempt_ready` | `rsp1` 0x1000008bca8→**0x1000008eeb8**（`rsp2`/`kstack*_top` 同步） | 镜像增长后内核栈后移 |
| `frames_ready` | `universe` 31475→**31474**、`free` 30546→**30535**、`image`→829；**`table=91`、`segment=19` 不变** | 镜像增长；不取帧、不新增段 |
| `self_root_ready` | `pages` 169→**173** | 镜像页随代码增长 |
| `state_audit_ready` | `total` 52→**54**、`to_running` 22→**23**、`to_faulted` 3→**4**、`used` 131→**135** | 见证新增一次 `schedule` 与一次 `Running→Faulted`；`Faulted→Ready` 按 §13.1 记 `TaskResumed` 不计入 |
| **新增** `resume_ready` | seq=34 | 本切片判决性见证 |
| `boot_complete` | seq 34→**35** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 26 条 | **逐字段不变** | 含 `ipc_ready`、`exc_*`、`user_cow_ready`、`fault_ready`、`reclaim_ready` |

以上按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. host 单测

见设计文档 §3（kernel 新增 2 条、xtask 新增 4 条反例）。核心把三件事钉死：**成功动作逐项真的发生**
（含"恰追加一条 `TaskResumed`"）、**三条前置的反例都字段级不变**、**不可恢复种类必须被拒且保持 Faulted**。

## 6. 边界与反例保留

- **两项口径偏差**（§0.1/设计文档）：调用者由 `KernelActor` 代调；`sret_frame_seg_cap` 由内核
  静态缓冲承担。这是**有意选择**（`Faulted` 任务在细化层无法发起 syscall），不等于规范字面已满足。
- **不恢复现场执行**：`kernel_resume_frame` 被保存并按 §8.4 步骤 3 被消费清空，但**不**真的
  `iretq` 进该帧。本切片证明 §12.10 的**状态机与现场保存语义**，不宣称"故障任务被真正恢复执行"。
- **`exc_source_cap` 未参与校验**：三个参数里只用 `frame_offset`；其余两者的授权语义随口径偏差
  一并登记为未实现。
- **切片 AA 的回收判据仍是过渡口径**：本切片给出了判据落点（`exc_resume` 的前置），但**未**回头
  收紧 AA 的判词——"依据当前 State 不可能成功 `exc_resume`"需要含源 cap 可达性的完整推导，
  登记为后续工作。
- `ISSUE-TASK-001` 与 `ISSUE-FAULT-005` 的行状态**不因本切片改动**；闭合判断交复核者。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。

## 7. 下一步

1. 六节（W/X/Y/Z/AA/AB）的**范畴外复核**（组织级门槛）——阶段 2 剩余的唯一独立性义务。
2. 其他候选（各独立 SPEC）：审计存储重构（Z 登记的结构性瓶颈）、收紧 AA 回收判据、
   `exc_resume` 的 `exc_source_cap`/`sret_frame_seg_cap` 授权语义与真实 `iretq` 恢复、
   §14 `{R,X}` 链接脚本级段切分、阶段 3 起点（Namespace / Timer / Audit）。
