# 切片 V 报告：Task 自毁与延迟帧归还（ISSUE-TASK-002）

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-17（验证与实测完成于 2026-09-18）
>
> 对象：`kernel/src/{bootstrap,sched,syscall,user,protocol,main}.rs`、`xtask/src/protocol.rs`
>
> 目的：交付 `ISSUE-TASK-002`——§8.2「删除自身是合法的」，含延迟帧归还与反例单测

## 0. 结论速览

- **自毁已实现**：`SYS_TASK_DESTROY` 的目标为调用者自身时走 `task_destroy_self`——模型清理
  （vspace 拆除、订阅移除、cap/消息/记录清理）照做，**页表帧（含根帧）只入延迟队列**，
  由 `sched::flush_deferred_frames` 在 `sched::resume` 的 `write_cr3` **之后**归还。
- **两个既有判定顺序缺陷被本切片暴露**：(1) `task_destroy` 原先先调 `prepare_destroy` 再判
  `task_id == caller`，Running 的调用者会先撞 `CannotDestroyRunning`，自毁分支**结构上不可达**；
  (2) `destroy_task_model` 内部又调一次 `prepare_destroy`，故清理体必须拆出
  `destroy_task_body`（不含 Running 前置）供自毁复用。
- **QEMU 主见证 `task_ready`**：`task4_cap=0x20000000b;task4_stack=0x0;task4_recv=0x4;
  task4_map_dead=ENOENT;grandchild=4;tasks_after=2;caps_after=29;root4_cleared=1`。
  task 4 由第二次 `SYS_TASK_CREATE` 建立（**id 4，不复用已销毁的 task 3 的 3**），自报 id 后
  **自毁**；自毁后向它映射被拒（`ENOENT`）、其调度登记已清。
- **一条非平凡的自毁约束（实测暴露）**：§8.2 的 `task_destroy` 会丢弃 sender 为该 Task 的消息，
  故自毁者**必须先让出**、等父任务取走刚排队的 id，再自毁——否则它自报的消息会被自己的销毁
  清理掉。初版在同一时间片内「发送后立即自毁」，父任务 `SYS_RECV` 实测取不到（`EAGAIN`）。
- **不勾选任何交付项与阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 1. 方法与独立性声明

- 真实内核实现 + host 单测 + QEMU 实测；无机器证明。
- 复核独立性为 **harness 内会话级别**，不是组织级第三方审查，不构成 `REFINEMENT-PROOF`。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| 延迟释放 | `sched.rs` 新增 `DEFERRED_FRAMES`/`DEFERRED_COUNT`（容量 40 = 根 1 + 用户映射拆除回收的空表帧 ≤ `MAX_VSPACE_ENTRIES`）、`defer_table_frame`（满队列返回 `DeferredQueueFull`，不静默丢弃）、`flush_deferred_frames`、`deferred_frame_count`（供单测） |
| 释放点 | `sched::resume` 在 `cpu::write_cr3(root)` **之后**、`exc::resume` 之前调 `flush_deferred_frames()`——此刻 CR3 已不指向任何延迟根 |
| 调度入口 | 新增 `sched::exit_to(next)`：只 `schedule_task_global(next)` → `set_current` → `resume`，**不触碰 cur**（自毁者已 Dead，走 `switch_to` 会在 `yield_task_global` panic） |
| 判定顺序 | `BootstrapState::task_destroy` 把 `task_id == caller` 判断提到 `prepare_destroy` **之前**；新增 `task_destroy_self`；清理体拆为 `destroy_task_body`（不含 Running 前置），两条路径复用 |
| 错误面 | 删除 `TaskError::CannotDestroySelf`（不再产生）；新增 `Errno::Again`（EAGAIN）——自毁时无 Ready 后继的确定性失败（不 panic、不改状态） |
| syscall | `task_destroy_call` 自毁分支：先 `pick_next(caller)`，`None` → `EAGAIN`；成功后 `clear_task(caller)` + `exit_to(next)`（`-> !`） |
| self cap 传递 | `register_created_task` 把 `outcome.self_cap.encode()` 写入初始帧 `rdi`（`SretFrame` 有该字段，`exc::resume` 恢复全部 GPR；细化层自由度，报告显式声明），子任务据此才知道自毁用哪个句柄 |
| task4 承载 | 复用切片 T 的代码段同一页：`TASK4_CODE_OFFSET=256`，`task4_code()` 109 字节（`mov rbx,rdi` → `SYS_TASK_ID` → `SYS_SEND` 自报 → `SYS_YIELD` → `SYS_TASK_DESTROY(rbx)` 自毁 → `jmp $`）。**不新增代码段帧**（`frames_ready.segment` 不变） |
| 新 cap | 新增槽 51（`TASK4_SEND_CAP`，gen1）作为第二次 create 的 `initial_cap0`——task 3 的那条已随其销毁被撤销，不能复用同槽 |
| 栈复用 | task 4 复用 task 1 为 task 3 建的栈段（段对象不随 task 3 销毁），映射进 **task 4** 的根 |
| ring3 | `user_code()` 追加自毁见证块（2636..2979），`USER_CODE_LEN` 2637→**2980**；末尾补一次唤醒 task 2（切片 V 的首次让出已把它排空成 `BlockedRecv`，抢占阶段需要 Ready 任务轮转） |

### 2.1 自毁 + 消息：一条必须显式处理的时序约束

规范 §8.2 的成功动作要求丢弃 sender 或 receiver 为目标的全部消息。这意味着**自毁者无法用
「发消息给父任务再自毁」在同一时间片内完成自报**：消息刚入队就被自己的销毁清理掉。
本切片把 task 4 的序列改为「发送 → 让出 → （被再次调度时）自毁」，父任务在两次让出之间取走消息。
该约束登记为细化层的实现事实，不是规范偏差。

## 3. 实测证据（2026-09-17/18）

```
PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=30698;image=778;free=30698;regions=3;kstart=0x1000000;kend=0x130a000
PRIMA|v=1|seq=23|event=ipc_ready|status=ok|data=create=0x100000008;map1=0x0;send1=0x0;send2=0x0;recv1=0x51a5000000000001;cap0=0x100000008;magic=0x51a5000000000001;map2=0x0;cross=0xfffffffffffffff3;send_denied=0xfffffffffffffff3;recv2=0xd1ec700000000002;cap_count=1;queued=3;direct=3;blocks=3
PRIMA|v=1|seq=24|event=task_ready|status=ok|data=stack_seg=0x100000009;create=0x10000000a;bad_entry=0xffffffffffffffea;no_x=0xfffffffffffffff3;stack_map=0x0;recv=0x3;deny=0xfffffffffffffff3;destroy=0x0;again=0xfffffffffffffff3;map_dead=0xfffffffffffffffe;map_own=0x0;map_value=0xc0ffee0000000000;map_other=0xfffffffffffffff3;task4_cap=0x20000000b;task4_stack=0x0;task4_recv=0x4;task4_map_dead=0xfffffffffffffffe;child=3;tasks=2;caps=29;root_cleared=1;grandchild=4;tasks_after=2;caps_after=29;root4_cleared=1
PRIMA|v=1|seq=25|event=preempt_ready|status=ok|data=ticks=4;pre1=2;pre2=2;root1=0xc000;root2=0x15000;cr31=0xc000;cr32=0x15000;rsp1=0x1000007f358;rsp2=0x10000083358;kstack1_top=0x1000007f380;kstack2_top=0x10000083380;data1=0x1e000;data2=0x22000;mark1=0x1;mark2=0x2
PRIMA|v=1|seq=27|event=platform_segments_ready|status=ok|data=segments=13;first_id=2;last_id=14;roots=13;user_roots=11;kernel_roots=2;holders=0,0,1,1,1,1,2,2,2,1,1,2,2;task_caps=2;task_holders=1,2;mapped=11;direct=2;direct_ids=2,3;vspace=12;slot_base=32;task1_cap=0x10000002d;task2_cap=0x10000002e;caps=0x100000020,...,0x10000002c;frames=2,3,1,1,1,1,1,1,1,1,1,1,1;unified=1
PRIMA|v=1|seq=29|event=frames_ready|status=ok|data=universe=31476;free=30653;segment=19;table=26;image=778;conserved=1;disjoint=1
PRIMA|v=1|seq=30|event=boot_complete|status=ok
```

解读：

- `task4_recv=0x4` + `grandchild=4`：task 4 由第二次 create 选定为 **id 4**——`finish_destroy`
  把 task 3 的记录置 Dead 但保留，`next_free_id` 的 `find()` 仍命中，故 **id 不复用**（S2-X4 的
  同一机制在任务维度的见证）。
- `task4_map_dead=ENOENT` + `root4_cleared=1` + `tasks_after=2`：自毁后其地址空间已拆、
  调度登记已清、任务表中只剩 init 与 task 2。
- `task4_stack=0`：调用者经 manager cap 把栈映射进 **task 4** 的根（授权记账完整）。
- `caps=29` / `caps_after=29`：task 4 的 manager cap 在其自毁后仍是 live 槽位（目标 Dead 但
  token 未回收，`again` 式的 stale 语义），加上其 self/root-ns cap 已随销毁撤销、槽 51 已释放。
- `frames_ready.table` 25→**26**：task 4 的根页表帧；`segment=19` **不变**（代码复用同页、栈复用
  既有段）；`free + segment + table + image = 30653 + 19 + 26 + 778 = 31476 = universe` ✅。
- `preempt_ready`：`ticks=4`、`pre1=pre2=2`、CR3 与数据帧隔离不变，两任务内核栈仍各自独立。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **278**、xtask 149 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，30 条事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. host 单测

- `task_destroy_allows_self_and_keeps_init_protected`：让调用者处于 `Running` 后销毁自身成功、
  目标不再是活动任务；init 仍 `CannotDestroyInit`。
- `task_destroy_still_rejects_running_others`：非自毁目标仍在 Running 时 `CannotDestroyRunning`
  的既有语义不变。
- `deferred_frames_stay_pending_until_flush`：入队后 `deferred_frame_count() > 0`（**尚未归还**），
  `flush_deferred_frames` 后才为 0；二次 flush 幂等。
- `deferred_queue_rejects_overflow`：满队列确定性失败且不丢弃已入队帧。

**覆盖边界（登记）**：`exit_to` 与 `resume` 中的释放点涉及 `iretq`，无法在 host 上执行；
「释放发生在 `write_cr3` 之后」由 `deferred_frame_count` 的顺序性质（入队期间不归还）加
QEMU 实测共同支持，不宣称 host 级端到端覆盖。设计文档原写的「注入式 release 闭包计数」
未实现（需为可测性引入注入点，属过度设计），此处如实修正。

## 5. 既有事件的变化（显式登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| `task_ready` | 新增 `task4_cap`/`task4_stack`/`task4_recv`/`task4_map_dead`/`grandchild`/`tasks_after`/`caps_after`/`root4_cleared`；`caps` 28→**29**（task 4 的 manager cap 仍占槽）；其余字段逐字段不变 |
| `ipc_ready` | `queued/direct/blocks` 2/2/2→**3/3/3**（task 4 的自报排队 + 一次直投唤醒 task 2 + 一次阻塞） |
| `memory_ready` | `usable` 30705→**30698**、`image` 771→**778**、`kend` 0x1303000→**0x130a000**（`user_code` +343 字节、自毁路径与延迟队列代码）；`universe` 仍 31476 |
| `frames_ready` | `free` 30661→**30653**、`table` 25→**26**（task 4 根页表帧）、`image` 771→**778**；`universe=31476`、`segment=19` 不变；`conserved=1`、`disjoint=1` 保持 |
| `preempt_ready` | `rsp1` 0x1000007ccf8→**0x1000007f358**、`rsp2` 0x10000080cf8→**0x10000083358**、`kstack1_top` 0x1000007cd20→**0x1000007f380**、`kstack2_top` 0x10000080d20→**0x10000083380**（镜像增长后移）；`root/cr3/data/ticks/pre/mark` 不变 |
| 其余 26 条事件 | **逐字段不变**（含 `platform_segments_ready`、`authority_ready`） |
| `TASK_READY_LIVE_CAPS` | 28 → **29** |
| `BootstrapState` 尺寸守卫 | 延迟队列是 `sched.rs` 的独立静态数组，不入 `BootstrapState`；16 KiB 守卫测试仍通过 |

## 6. 边界与反例保留

- **自毁要求至少一个 Ready 后继**：`pick_next` 为 `None` 时返回 `EAGAIN` 且不改状态（不 panic）。
  这是细化层限制，登记；不宣称可调度性完备。
- **`exit_to` 无 host 覆盖**（涉及 `iretq`），由 QEMU 见证。
- **延迟队列容量 40**：溢出返回 `DeferredQueueFull`，自毁路径将其映射为 `NoSuchTask` 并中止
  （回滚未做——自毁是单向操作，登记为边界）。
- **task 4 的 manager cap 在自毁后仍占槽**（目标 Dead 的 stale token），槽位不回收属既有语义
  （§4.6 只在显式 drop/撤销时释放）。
- `ISSUE-TASK-003`（调度路径 `TaskStateChanged`）不在本切片，留 S3-D5。
- 未实现：`dev_vspace`/W16.4、`REFINEMENT-PROOF`、完整 `COMPAT-TEST`。
- 既有 `cargo xtask test` 的 30s/无网络/精确序列语义不变。

## 7. 关闭口径与下一步

- **不勾选 `ISSUE-TASK-002`、任何交付项与退出条件、阶段 2 维持「进行中」**：证据虽为真实内核
  + QEMU，独立性仍为 harness 内，需范畴外第三方复核——与阶段 0/1 同一门槛。
- 阶段 2 两条尾巴（`ISSUE-AUTH-002` 剩余项、`ISSUE-TASK-002`）至此在实现侧均已交付。
- 下一步候选：
  1. 阶段 2 各退出条件的范畴外复核；
  2. `ISSUE-FRAME-001`、`ISSUE-TASK-001`（内核缺 `Faulted`）、`ISSUE-TASK-003`（留 S3-D5）的处置；
  3. 阶段 3 起点（Namespace/Timer/Audit）。
