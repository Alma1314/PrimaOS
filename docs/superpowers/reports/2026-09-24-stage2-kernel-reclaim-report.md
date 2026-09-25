# 切片 AA 报告：§8.2 内核回收路径（`KernelActor`，无 `task_cap`）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-24
>
> 对象：`kernel/src/{bootstrap,syscall,resume,protocol,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：落实 §8.2 规范侧定义的**内核回收路径**——`KernelActor` 无需 `task_cap` 回收
> 「不可恢复的 `Faulted`」任务；这是切片 Y 登记的两条 `Faulted` 出口之一

## 0. 结论速览

- **回收路径已落地**：新增 `is_unrecoverable_faulted`（判据）与 `kernel_reclaim_task`
  （§8.2 步骤 1–9 的 KernelActor 路径，**无 cap**）。清理体与 `task_destroy` 共用
  `destroy_task_body`（含 §8.2 步骤 6 清空 `pending_exc` 与 `kernel_resume_frame`）；
  地址空间拆除抽成 `teardown_vspace` 由两条路径共用。
- **判决性见证（新增独立事件 `reclaim_ready`，seq 33）**：
  `task=6;negative_task=5;unrecoverable=1;reclaimed=1;negative_rejected=1;after_dead=1;pending_cleared=1;invariant_ok=1`
  ——不可恢复（`DoubleFault`）的 `Faulted` 任务被 KernelActor 回收为 `Dead`、`pending_exc` 清空；
  **可恢复**（`PageFault` 且源存活）的 `Faulted` 任务**被拒绝回收**（它仍是 §12.10 的候选）。
- **判据是过渡口径并已登记**：规范原文的判据「依据当前 State 不可能成功 `exc_resume`」在
  `exc_resume` 未用户化前没有落点，故先落在两个可判定量上——**种类不可恢复**（§12.10 前置 6）
  或**异常源已不存在**（§12.6 L1440）。待 §12.10 落地后必须收紧。
- **不勾选任何交付项与退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不是组织级第三方审查，不构成 `REFINEMENT-PROOF`。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `bootstrap.rs` | `TaskError::NotUnrecoverable`；`RECLAIM_WITNESS_TASK_ID = 6` / `RECLAIM_NEGATIVE_TASK_ID = 5`；`is_unrecoverable_faulted`；`kernel_reclaim_task`；抽出 `teardown_vspace`（`task_destroy` 复用）；`ReclaimWitness` + `reclaim_witness` + `record_reclaim_witness`/`reclaim_witness_record` + 4 条 host 单测 |
| `syscall.rs` | `TaskError::NotUnrecoverable → Errno::Invalid`（仅为穷尽性；内核路径不经用户面） |
| `main.rs` | 在 `activate_init` **之前**、紧随 `Faulted` 见证之后采集回收见证 |
| `resume.rs` / `protocol.rs` | 上报 `reclaim_ready` |
| `xtask/src/protocol.rs` | `EventKind::ReclaimReady`、`EXPECTED_EVENTS` 33→34、`validate_reclaim_data` + 3 条反例、golden 夹具与 `task_ready` 计数期望 3→4 |

### 2.1 为什么见证用两个保留槽位任务

- **id 6（正例）**：调度为 `Running` 后以 `DoubleFault` 置 `Faulted` → 判据成立 → 被内核回收为 `Dead`。
- **id 5（反例）**：同样置 `Faulted` 但种类为可恢复的 `PageFault` → 回收**必须被拒**且字段级不变。
  这条反例是本切片最关键的断言：若"回收"能随手销毁可恢复的 `Faulted`，就等于绕过了 §12.10
  未来的恢复路径。
- 两者都在 **`activate_init` 之前**采集（此刻无任何 Task `Running`，§8.4 的 `schedule` 合法），
  且都用保留槽位——普通 id（`child=3`/`grandchild=4`）**不顺移**。

## 3. 实测证据（2026-09-24）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30656;image=819;free=30656;regions=3;kstart=0x1000000;kend=0x1333000
PRIMA|v=1|seq=24|event=task_ready|status=ok|data=…;child=3;tasks=4;caps=29;root_cleared=1;grandchild=4;tasks_after=4;caps_after=29;root4_cleared=1
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31475;free=30546;segment=19;table=91;image=819;conserved=1;disjoint=1
PRIMA|v=1|seq=32|event=state_audit_ready|status=ok|data=total=52;to_running=22;to_ready=22;to_blocked=5;to_faulted=3;used=131;cap=170
PRIMA|v=1|seq=33|event=reclaim_ready|status=ok|data=task=6;negative_task=5;unrecoverable=1;reclaimed=1;negative_rejected=1;after_dead=1;pending_cleared=1;invariant_ok=1
PRIMA|v=1|seq=34|event=boot_complete|status=ok
```

守恒：`30546 + 19 + 91 + 819 = 31475 = universe` ✅；`conserved=1`、`disjoint=1`。
`table=91`、`segment=19` **不变**——本切片不取帧、不新增 Segment。审计 `used=131 ≤ cap=170`。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **303**、xtask **163** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**34 条事件**序列 + `validate_reclaim_data` + 3 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件 | 变化 | 原因 |
|---|---|---|
| `memory_ready` | `usable` 30659→30656、`image` 816→**819**、`kend` 0x1330000→**0x1333000** | 回收路径与见证代码使镜像 +3 帧 |
| `bootstrap_ready` | `image_len` 3338936→3353912、`image_frames`→819 | 同上 |
| `task_ready` | `tasks`/`tasks_after` 3→**4**；其余字段（含 `child=3`/`grandchild=4`/`caps=29`）**逐字段不变** | 存活 Task 增加见证的**反例任务 5**（任务 6 已回收为 Dead，不计入） |
| `preempt_ready` | `rsp1` 0x1000008aa88→**0x1000008bca8**（`rsp2`/`kstack*_top` 同步） | 镜像增长后内核栈后移 |
| `frames_ready` | `free` 30549→**30546**、`image`→819；**`table=91`、`segment=19` 不变** | 镜像增长；不取帧、不新增段 |
| `self_root_ready` | `pages` 168→**169** | 镜像页随代码增长 |
| `state_audit_ready` | `total` 50→**52**、`to_faulted` 1→**3**、`used` 128→**131** | 见证新增两次 `Running→Faulted` 迁移（两任务各一次） |
| **新增** `reclaim_ready` | seq=33 | 本切片判决性见证 |
| `boot_complete` | seq 33→**34** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 25 条 | **逐字段不变** | 含 `ipc_ready`、`exc_*`、`user_cow_ready`、`fault_ready` |

以上按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. host 单测

见设计文档 §3（kernel 新增 4 条、xtask 新增 3 条反例）。核心把三件事钉死：**只回收不可恢复者**、
**可恢复者必须被拒且原子不变**、**回收真的有清理效果**（Dead + `pending_exc` 清空 + 不变量仍成立）；
另加"源已销毁 ⇒ 不可恢复"这一 §12.6 分支。

## 6. 边界与反例保留

- **判据会过期**：以"种类不可恢复 / 源已销毁"近似"不可能成功 `exc_resume`"，待 §12.10 落地后
  **必须收紧**。这是显式登记的过渡口径，不是终态判据。
- **只做回收、不做恢复**：`Faulted` 的**唯一**出边现在是把目标销毁；§12.10 的 `Faulted → Ready`
  与 `TaskResumed`（§13.1 的唯一例外）仍属后续切片。
- **地址空间拆除依赖调用方给出 `root_phys`**（与 `task_destroy` 既有语义一致）；内核回收方
  未持有根帧时只能做模型清理。
- **见证副作用**：boot 期留下一个**永久 `Faulted` 的反例任务（id 5）**——它刻意不可回收，
  正是不变量的见证；登记在案。
- `ISSUE-FAULT-005` 规范侧早已 Fixed，本切片只补内核实现，**不改该行状态**。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。

## 7. 下一步

1. 五节（W/X/Y/Z/AA）的**范畴外复核**（组织级门槛）——阶段 2 剩余的唯一独立性义务。
2. 仍缺的一项：**§12.10 `exc_resume` 用户化**（`Faulted → Ready` + `TaskResumed`），
   它同时是 §13.1 的唯一例外与"收紧回收判据"的前置；其规范语义（"调用者必须是 Faulted 任务"
   而内核只有 `Running` 任务能发起 syscall）需要一个细化口径的裁决。
3. 其他候选：审计存储重构（Z 登记的结构性瓶颈）、§14 `{R,X}` 链接脚本级段切分、
   阶段 3 起点（Namespace / Timer / Audit）。
