# 切片 V 设计：Task 自毁与延迟帧归还（ISSUE-TASK-002）

> 文档类型：真实内核实现设计（阶段 2 收尾，补 §8.2「删除自身是合法的」）
>
> 日期：2026-09-17
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-17，三项关键决策逐项确认）

## 0. 定位与前置

规范 §8.2 写明 `task_destroy` 的目标可以是调用者自身，而内核当前在 `target == caller` 时返回
`CannotDestroySelf`（`bootstrap.rs:2773`）。ISSUE 表登记的验收条件原文：「实现自毁：模型清理 +
把自身根页表帧的归还推迟到 CR3 切换之后，并加反例测试证明切换前不发生帧分配」。

| 面 | 规范/登记要求 | 改前现状 |
|---|---|---|
| 自毁合法性 | §8.2 允许 `target == caller` | `TaskError::CannotDestroySelf` 拒绝 |
| 根帧归还 | 必须在不再使用该 CR3 之后 | `task_destroy` 立即 `release_table_frame(root)`（`bootstrap.rs:2797`） |
| 页表帧归还 | 同上（自毁时中间层页表帧也在当前 CR3 中） | unmap 回调里立即归还 |
| 调度入口 | 切到后继任务 | `switch_to` 会先 `yield_task_global(cur)`——对已从任务表删除的自毁者会 panic |
| 可达性检查顺序 | — | `prepare_destroy` 在 `task_id == caller` 判断**之前**调用，Running 的 caller 先撞 `CannotDestroyRunning`（`bootstrap.rs:440`），自毁分支根本不可达 |

### 0.1 已确认的关键决策（用户逐项确认，2026-09-17）

1. **独立切片**：与 `ISSUE-AUTH-002`（平台段 cap 移交，SPEC-066）分开，各一份设计/报告。
2. **QEMU 见证由新增的 task 4 承载**：task 3 维持切片 T 原样（被 task 1 销毁），切片 T 的
   `destroy`/`again`/`map_dead` 三条负例全部保留。
3. **粒度：完整交付一个领域**——自毁路径连同延迟释放、调度入口、反例单测一次做完。

## 1. 设计

### 1.1 控制流（自毁）

```
task_destroy_call(target == caller)
  0. pick_next(caller) 为 None → 确定性 errno（不 panic、不改状态）；见 §1.4
  1. 拆除自己的用户映射（vspace::unmap_4k）——只动用户 PML4 子树，内核高半区在当前 CR3 下仍可见
  2. 页表帧（含根帧）不立即归还，改为入延迟队列
  3. exc_sources.remove_subscriber(caller)
  4. destroy_task_model(caller)：cap/消息清理 + finish_destroy 置 Dead
  5. sched::clear_task(caller)
  6. sched::exit_to(next)
       → schedule_task_global(next) → set_current(next) → resume(next)
            → set_rsp0 → write_cr3(next_root) → flush_deferred_frames()   ← 释放点
            → exc::resume(frame)   // iretq 进用户态，不返回
```

### 1.2 延迟帧归还（`sched.rs`）

- `static mut DEFERRED_FRAMES: [u64; MAX_DEFERRED_FRAMES]` + `count`（风格同既有静态数组）；
- `defer_table_frame(frame)` 入队（满则返回错误；自毁路径的帧数有上界：根 1 + 用户映射拆除
  回收的空表帧 ≤ `MAX_VSPACE_ENTRIES`）；
- `flush_deferred_frames()` 逐帧调 `frames::release_table_frame` 并清空队列；
- **释放点**：`resume()`（`sched.rs:187`）在 `cpu::write_cr3(root)`（`:193`）**之后**、
  `exc::resume` 之前调用 `flush_deferred_frames()`。此刻 CR3 已不指向任何延迟根，释放安全。

「切换前不发生释放」在一个函数内可测：host 单测用可注入的 release 闭包计数，断言
`write_cr3` 之前为 0、之后为 1。

### 1.3 `sched::exit_to(next)`

语义同 `block_to`（`:173`）：`schedule_task_global(next)` → `set_current(next)` → `resume(next)`，
**不触碰 cur**（它已 Dead，走 `switch_to` 会在 `yield_task_global` panic）。

### 1.4 边界：无 Ready 后继

`pick_next(caller)` 返回 `None` 时**不执行任何清理**，返回确定性 errno（`EAGAIN`），
不 panic。该边界登记为细化层限制（自毁要求系统在自毁者之外至少有一个 Ready 任务），
host 单测钉住。

### 1.5 task 4 承载自毁见证

- **代码复用同一页**：`task3_code()` 仅 134 字节，代码段 1 页 4 KiB。新增 `task4_code()`
  写在**同页内偏移 256**（`TASK4_CODE_OFFSET`），第二次 `SYS_TASK_CREATE` 的 `entry` 指向
  `USER3_CODE_VA + 256`——§8.1 只要求 `entry ∈ [load_base, load_base + code_pages*4KiB)`，
  同页内偏移合法。这样**不新增代码段帧**（`frames_ready.segment` 不变）。
- **self cap 传递**：`task_create` 已把 `MANAGER_TASK_RIGHTS`（含 `TaskDestroy`，
  `bootstrap.rs:283`）铸给新任务的 self cap（`:2707`）。缺口只是子任务不知道句柄值：
  `register_created_task`（`user.rs`）把 `outcome.self_cap.encode()` 写入初始帧 `rdi`
  （`SretFrame` 有 `rdi` 字段，`exc::resume` 恢复全部 GPR；细化层自由度，报告显式声明），
  不改句柄分配。task4 代码首条即 `mov rbx, rdi`。
- **send cap**：新增 `TASK4_SEND_CAP_SLOT = 51`（gen1，铸给 task 1，指向 `Task(1)` + `TaskSend`），
  作为第二次 create 的 `initial_cap0` 移入 task4——task3 的那条（槽 50）已随 task3 销毁被撤销，
  不能复用。
- **栈复用**：task 1 在切片 T 里为 task3 建的栈段 cap 仍由它持有（段对象不随 task3 销毁），
  第二次 create 后用它把同一段映射进 **task4** 的根（`USER3_STACK_VA`）。task3 的映射已随其
  销毁拆除，故不 `Overlap`。
- **用户序列**（追加在 SPEC-066 块之后、`int3` 之前）：
  1. 重设 `rdx = TASK_REQ_VA`（SPEC-066 块用过 rdx），改写请求块 `entry = USER3_CODE_VA+256`、
     `initial_cap0 = TASK4_SEND_CAP`；
  2. `SYS_TASK_CREATE` → manager cap 存 `[Y+472]`；用 task3 的栈段 cap + 该 manager cap 经
     `SYS_MEMSEG_MAP` 把栈映射进 task4 根，回执 `[Y+480]`；
  3. `SYS_YIELD`×2（第一次排空 task 2——切片 T 末尾刚 `SYS_SEND` 唤醒它，它执行尾部阻塞
     接收后变 `BlockedRecv`；第二次才轮到 task4）；
  4. `SYS_RECV` 取 task4 自报的 id，回执 `[Y+488]`（应为 4）；
  5. 再次用同一栈段 cap + task4 manager cap 映射 → 应 `ENOENT`（task4 已自毁，其调度登记
     已清），回执 `[Y+496]`；
  6. `int3` 收尾。
- **task4 代码**（偏移 256，118 字节）：`mov rbx, rdi`（self cap）→ `SYS_TASK_ID` 写入自己栈页
  → `SYS_SEND(1, send_cap, id, 0,0,0)` → `SYS_TASK_DESTROY(rbx)`（自毁）→ 写入「不该到达」
  回执 → `jmp $`。

### 1.6 顺带见证 id 不复用

`finish_destroy` 把记录置 Dead 但**保留**，`next_free_id` 的 `find()` 仍命中，故 task4 得到
id **4** 而非复用 task3 的 3。`task_ready` 的 `grandchild` 字段直接钉住这一点。

## 2. 回归面（逐条登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| `task_ready` | 新增自毁字段：`self4`（task4 的自报 id）、`stack4`（栈映射回执）、`destroy4`（自毁后向 task4 映射的 errno）、`grandchild`（内核实读的 task4 id）、`tasks_after`/`caps_after`/`root4_cleared` |
| `memory_ready` | `image`/`kend`/`usable`/`universe` 随 `user_code` 增长 |
| `frames_ready` | `image` 随之上移；task4 的**根页表帧**使 `table` +1，`segment` 不变（代码复用同页、栈复用既有段）；`free`/`universe` 同步（守恒等式必须仍成立） |
| `preempt_ready` | `rsp*`/`kstack*_top` 随镜像增长整体后移 |
| `TASK_READY_LIVE_CAPS` | 需按新事件实读重算（新增 task4 send cap；task4 的 self/root_ns cap 随自毁撤销，其 manager cap 仍占槽） |
| `platform_segments_ready` | 不变（本切片不动平台 cap） |
| host 单测 | `bootstrap_state_stays_within_the_stack_budget`（16 KiB）需复测；`MAX_AUDIT_EVENTS=128` 余量需实测 |

## 3. host 单测

- `defer_table_frame` 后帧仍未归还（`deferred_frame_count() > 0`）；`flush_deferred_frames` 后才归还；
- 队列满时确定性失败且不丢弃已入队帧；
- 自毁正例（让调用者处于 `Running` 后销毁自身成功、目标从任务表消失）+ init 保护；
- 非自毁目标仍在 Running 时 `CannotDestroyRunning` 的既有语义不变。

**覆盖边界（实施时如实修正）**：`exit_to` 与 `resume` 中的释放点涉及 `iretq`，无法在 host 上
执行——「释放发生在 `write_cr3` 之后」由 `deferred_frame_count` 的顺序性质（入队期间不归还）
加 QEMU 实测共同支持。原设计的「注入式 release 计数闭包」为可测性引入注入点，属过度设计，
未采用。

## 4. 验证

- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo fmt --all --check`
- `cargo xtask test`（QEMU BIOS run=1/2，全部事件按新序列与 data 断言）
- `cargo xtask compat`（`PASS compat cow-write-fault`）

## 5. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」。
- 自毁要求至少一个 Ready 后继（§1.4），不宣称可调度性完备。
- 延迟队列容量有上界，溢出时返回确定性错误而非静默丢弃（登记）。
- `ISSUE-TASK-003`（调度路径 `TaskStateChanged`）不在本切片（留 S3-D5）。
