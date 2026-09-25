# 切片 Z 报告：调度路径的 `TaskStateChanged` 与审计容量（ISSUE-TASK-003）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-24
>
> 对象：`kernel/src/{bootstrap,resume,protocol}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 `ISSUE-TASK-003`——按 §13.1/W13 为**每条** Task 状态迁移追加 `TaskStateChanged`，
> 并处置由此产生的审计容量问题

## 0. 结论速览

- **四条迁移路径全部接上事件**：`schedule`（`Ready→Running`）、`yield`/抢占（`Running→Ready`）、
  `block`（`→BlockedRecv`）、`wake` 与 pending 直投（`BlockedRecv→Ready`）。这三处原先直接调用
  `self.tasks.*` 的迁移改走 `BootstrapState` 级包装，使**所有**迁移都经过审计点。
- **判决性见证（新增独立事件 `state_audit_ready`，seq 32）**：
  `total=50;to_running=22;to_ready=22;to_blocked=5;to_faulted=1;used=128;cap=170`
  ——四类目标状态计数**都非零**，即 S2-X1 登记缺口（"调度路径的迁移事件仍缺"）在实现侧闭合。
- **容量确实不足，ISSUE 的判断成立**：补齐后审计需求恰为 **128 条 = 原容量**（**零余量**）。
  按项目既有先例（切片 R：保留 16 KiB 守卫、调整容量并显式登记）把 `MAX_AUDIT_EVENTS`
  **128 → 170**；`AuditEvent` = 24 B，`BootstrapState` 由 15304 B 增至 **16312 B**，
  仍 ≤ 守候 16384 B——**守卫未放宽**，但余量只剩 72 B。
- **两处判断更正（如实登记）**：
  1. 我最初的探针估数（33 次迁移 → 判断"无需扩容量"）**不完整**（漏计 `receive_message` 空队列
     阻塞、`send_message` 投递唤醒、`prepare_user_ipc_caps` 等路径）；QEMU 实测真实
     `TaskStateChanged` 为 **50** 条、日志被恰好占满。以实测为准。
  2. ISSUE 原文写的"须先做审计存储重构"——本切片**不做重构**，改按切片 R 的先例扩容量；
     理由是重构要动 79 处 `.audit` 访问 + 12 处测试断言，且审计改为模块级静态后 host 单测
     （并行线程）会共享日志而互相污染，收益不足以抵偿回归风险。
     **代价如实登记**：结构体余量只剩 72 B，任何进一步存储增长都**必须先做审计存储重构**。
- **不勾选任何交付项与退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不是组织级第三方审查，不构成 `REFINEMENT-PROOF`。
- 度量方法登记：审计容量决策以 **QEMU 实测的 `used`/分类计数**为准；探针统计只作粗估，
  本轮已证明其会漏计（见 §0）。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| `bootstrap.rs` | `MAX_AUDIT_EVENTS` 128→**170**（注释写明实测依据与"余量枯竭"结论）；新增 `audit_transition`；`schedule_task`/`yield_task` 补事件 + 容量预检；新增 `block_recv_task`/`wake_recv_task`/`set_pending_task`；把 3 处直接 `self.tasks.*` 迁移改走包装；`exc_raise` 预检 `1+direct*3+queued`；新增 `TaskStateAudit` + `task_state_audit()`（含全局包装）；`bootstrap_state_stays_within_the_stack_budget` 增"余量已枯竭"断言 |
| `resume.rs` / `protocol.rs` | 上报 `state_audit_ready` |
| `xtask/src/protocol.rs` | `EventKind::StateAuditReady`、`EXPECTED_EVENTS` 32→33、`validate_state_audit_data` + 3 条反例、golden 夹具更新 |

### 2.1 原子性

容量预检**先于**迁移提交：`can_append(1)` 失败即 `TaskError::AuditFull` 且状态字段级不变——
不静默丢事件、也不写出半个迁移（§6.5 先验证后原子提交）。host 单测
`transition_is_rejected_when_the_audit_log_is_full` 钉住这一点。

### 2.2 无迁移不追加

`wake_recv_task` 对非 `BlockedRecv` 目标、`set_pending_task` 对非阻塞目标都**不**追加事件，
避免把"通知"误记成"迁移"。`Dead`（细化态、非 POSIR 状态）按 §13.1 的枚举**不追加**（登记）。

## 3. 实测证据（2026-09-24）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30659;image=816;free=30659;regions=3;kstart=0x1000000;kend=0x1330000
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31475;free=30549;segment=19;table=91;image=816;conserved=1;disjoint=1
PRIMA|v=1|seq=30|event=self_root_ready|status=ok|data=root=0x5b000;kernel_root=0xa000;image_table=0xb000;owned=1;pages=168;translated=0x1000000
PRIMA|v=1|seq=31|event=fault_ready|status=ok|data=task=7;before=1;after=3;raised=1;pending_source=1;pending_kind=1;precond_rejected=1;invariant_ok=1;unschedulable=1
PRIMA|v=1|seq=32|event=state_audit_ready|status=ok|data=total=50;to_running=22;to_ready=22;to_blocked=5;to_faulted=1;used=128;cap=170
PRIMA|v=1|seq=33|event=boot_complete|status=ok
```

守恒：`30549 + 19 + 91 + 816 = 31475 = universe` ✅；`conserved=1`、`disjoint=1`。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **299**、xtask **160** 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**33 条事件**序列 + `validate_state_audit_data` + 3 条反例。
- `cargo xtask compat`：`PASS compat cow-write-fault`。
- 事件流逐条 diff（上一提交 `f95760f` vs 本切片）：改动面见设计文档 §3。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件 | 变化 | 原因 |
|---|---|---|
| `memory_ready` | `usable` 30662→30659、`image` 813→**816**、`kend` 0x132d000→**0x1330000** | 迁移审计与见证代码使镜像 +3 帧 |
| `bootstrap_ready` | `image_len` 3328528→3338936、`image_frames`→816 | 同上 |
| `preempt_ready` | `rsp1` 0x10000089988→**0x1000008aa88**（`rsp2`/`kstack*_top` 同步） | 镜像增长后内核栈后移 |
| `frames_ready` | `free` 30552→**30549**、`image`→816；**`table=91`、`segment=19` 不变** | 镜像增长；不取帧、不新增段 |
| `self_root_ready` | `pages` 167→**168** | 镜像页随代码增长 |
| **新增** `state_audit_ready` | seq=32 | 本切片判决性见证 |
| `boot_complete` | seq 32→**33** | 上述新事件造成的**唯一** seq 顺移 |
| 其余 26 条 | **逐字段不变** | 含 `ipc_ready`、`exc_ready`、`exc_pending_ready`、`task_ready`、`user_cow_ready` |

**特别登记**：上述 26 条逐字段不变，说明新增的 50 条审计事件**没有挤掉任何既有事件**，
也没有让任何投递因容量不足而失败（旧容量 128 恰好够用，但**余量为零**——这正是必须扩容的理由）。

以上按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. host 单测

见设计文档 §4（kernel 新增 3 条、xtask 新增 3 条反例）。核心是把**每条迁移都留痕**、
**无迁移不追加**、**容量不足时原子失败**三件事钉死。

## 6. 边界与反例保留

- **§13.1 的唯一例外未处理**：`Faulted → Ready` 应以 `TaskResumed` 记录，该迁移的实现
  （§12.10 `exc_resume` 用户化）属后续切片，故本切片不产生 `TaskResumed`。
- **`Dead` 无事件**（细化态非 POSIR 状态）；若复核者认为对象消失也应留痕，需另立迁移类型。
- **审计容量余量仅 72 B（16312/16384）**：任何进一步存储增长都**必须先做审计存储重构**
  （把 `AuditLog` 移出 `BootstrapState`）。本切片显式登记该瓶颈，并用测试钉住"余量已枯竭"，
  防止后续静默扩容撞守卫。
- **容量耗尽的表现是 panic 而非优雅失败**：`sched::switch_to`/`block_to`/`exit_to` 用
  `.expect(...)` 处理调度返回值，故 `AuditFull` 会以 panic 终止。余量（42 条）使其在本切片
  不触发，但登记为已知粗糙点。
- **本切片不做审计存储重构**：理由与代价见 §0；这是一条**有意选择**，不等于否定重构的必要性。
- `ISSUE-TASK-003` 的**闭合判断交复核者**；不自行宣告满足解除条件。
- 未实现：`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。

## 7. 下一步

1. 四节（W/X/Y/Z）的**范畴外复核**（组织级门槛）——阶段 2 剩余的唯一独立性义务。
2. 后续候选（各独立 SPEC）：
   - **审计存储重构**（把 `AuditLog` 移出 `BootstrapState`）——本切片已把它登记为**结构性瓶颈**；
   - §12.10 `exc_resume` 用户化（`Faulted → Ready` + `TaskResumed`，§13.1 的唯一例外）；
   - §8.2 `KernelActor` 无 cap 回收；
   - §14 `{R,X}` 的链接脚本级段切分；
   - 阶段 3 起点（Namespace / Timer / Audit）。
