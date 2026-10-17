# SPEC-134：S0-X2 逐操作覆盖报告（DOC-007 增量复核）

> 文档类型：覆盖报告（非设计、非规范、非证明）
>
> 状态：文档覆盖复核完成；不构成组织级第三方审查或实现证明
>
> 日期：2026-10-17
>
> 基线：[[2026-09-13-s0x2-operation-coverage]]（SPEC-054，修订后 26 项常规操作，结论 S0-X2 满足）
>
> 触发：2026-10-14 双人格候选采纳审核（DOC-007，见 [[2026-10-14-cosir-dual-personality-adoption-review]]）将常规机制由 11 类修订为 12 类（加 Bootstrap 共 13 项），向冻结语义新增 §5.6 WaitObject、§7.4 `memseg_protect`、§21 全部操作与 W17。SPEC-054 覆盖报告早于该修订一个月，其检查面（26 项）不含任何新增操作，S0-X2 既有勾选的证据基线因此过期。
>
> 范围：对 [[COSIR 原语规范]] DOC-007 修订后新增的 8 项常规操作逐项核对「成功语义」与「失败/错误语义」是否明确；并对被 DOC-007 联动修订的既有操作（`task_destroy`、`advance_time`、`schedule`）做失效快检。
>
> 结论速览：8 项新增操作全部满足 SPEC-054 三分类判定口径（显式 `错误：`清单 / §6.2.1 默认规则兜底 / 显式「无失败分支」声明）；3 项被联动修订的既有操作失败语义未被削弱。**就规范文本层面，S0-X2 在 DOC-007 后的冻结面上重新成立。** 实现与证明证据（MODEL-TEST / Lean / QEMU）为零，属后续切片范畴，不阻塞本条件（判定边界见 §7、§9）。

## 1. 方法

与 SPEC-054 相同：逐读 [[COSIR 原语规范]] 操作小节，对每个常规操作摘录成功迁移与失败/错误语义，按三分类口径判定：

- **显式清单**：操作小节给出 `错误：` 枚举（§6.2 失败时保持 `State` 不变）；
- **默认规则**：未单列清单，由 §6.2.1 规定失败域 = 前置条件违反，返回 §6.2 阶段标准错误；
- **无失败分支**：操作小节显式声明，且前置条件由 §6.1 Actor 约束或内核保证成立。

检查对象为 DOC-007 引入的增量面（§5.6、§7.4、§12.5 关联字段、§16 W17、§17.9、§21），以及该修订触及的既有操作小节。本报告不重新核查未受 DOC-007 影响的其余 18 项操作——它们沿用 SPEC-054 §3 的判定。

## 2. 逐操作判定（新增 8 项）

| 类别 | 操作 | 规范位置 | 成功语义 | 失败语义类型 | 判定 |
|---|---|---|---|---|---|
| MemorySegment | `memseg_protect` | §7.4（:704–724） | 7 条前置 + 逐页原子改权 + `VspaceProtChanged` Audit | 显式清单：`InvalidTask`、`MissingRight(TaskManageVSpace)`、`NotMapped`、`InvalidAccess`、`MissingMapRight`、`ZeroSize`、`EntryMapping`、`OutOfBounds` | 通过 |
| WaitObject | `wait_object_create` | §21.1.1（:2445–2459） | 分配 id、建对象（`generation=0`、`waiters=∅`）、铸根 cap（权限恰为 `{Derive, WaitWait, WaitSignal, WaitDestroy}`） | 显式「无失败分支」声明（除 §6.1 Actor 约束与实现资源耗尽外无前置条件） | 通过 |
| WaitObject | `wait_object_observe` | §21.1.2（:2461–2476） | 纯查询返回 `generation`，不改状态 | 显式清单：`InvalidWaitObject`、`NotHeld`、`MissingRight(WaitWait)`、`TargetDead` | 通过 |
| WaitObject | `wait_object_destroy` | §21.1.3（:2478–2495） | 逐个完成 `BlockedWait` 等待者为 `WaitTargetDead`、清 `waiters`、删对象、指向 cap 变 stale | 显式清单：`InvalidWaitObject`、`NotHeld`、`MissingRight(WaitDestroy)`、`TargetDead` | 通过 |
| WaitObject | `wait_objects` | §21.2.1（:2499–2532） | 满足检查立即返回 `WaitSatisfied`/`WaitAlerted`/`WaitTimedOut`；否则写 `wait_request`、入 `waiters`、`Running→BlockedWait`、返回 `Blocked`；立即返回时保持 `Running` 且不写结果字段 | 显式清单：`InvalidWaitObject`、`NotHeld`、`MissingRight(WaitWait)`、`TargetDead`、`DuplicateWaitObject`、`EmptyWaitSet` | 通过 |
| WaitObject | `signal_object` | §21.2.2（:2534–2553） | `generation` 原子加一、按 `Any`/`All` 完成符合条件的等待者、返回唤醒数 | 显式清单：`InvalidWaitObject`、`NotHeld`、`MissingRight(WaitSignal)`、`TargetDead` | 通过 |
| Task/upcall | `task_alert` | §21.3.1（:2557–2584） | 追加 `Upcall{source,data}` 入 `pending_upcalls`；目标 `alertable` 等待则完成为 `WaitAlerted`；不消费 cap | 显式清单：`InvalidTask`、`NotHeld`、`MissingRight(TaskSend)`、`MsgTooLarge` | 通过 |
| Task/upcall | `task_upcall_recv` | §21.3.2（:2586–2596） | FIFO 返回并清空自身 `pending_upcalls`，空列表非错误 | 显式「无失败分支」声明（不接 cap、不访问其它对象） | 通过 |

新错误标识均就地定义（`InvalidWaitObject` :2476、`DuplicateWaitObject`/`EmptyWaitSet` :2532），与 §6.2.1 显式清单优先规则一致，未引入未定义错误名。`wait_objects` 的 `Blocked` 为成功结果（阻塞语义），不计失败域。

## 3. 被联动修订的既有操作快检

DOC-007 为接入 WaitObject 修订了 3 项既有操作，逐项确认其 SPEC-054 判定未失效：

| 操作 | 修订内容 | 失败语义现状 | 判定 |
|---|---|---|---|
| `task_destroy` §8.2 | 成功动作 7 新增：从所有 WaitObject `waiters` 删除目标、清 `wait_request`/`pending_wait_result`/`pending_upcalls`，被取消等待不产生结果（:808） | 显式清单 3 项不变（`InvalidTask`/`NotHeld`/`MissingRight(TaskDestroy)`，:823） | 仍通过 |
| `advance_time` §10.3 | 新增第 6 步：把截止时间已到的 `BlockedWait` Task 按 `(d, TaskId)` 稳定顺序完成为 `WaitTimedOut`；迁移后所有 `wait_request.deadline` 为 `None` 或严格大于 `now`（:1092–1099） | 显式清单（含 `InvalidTime`）不变 | 仍通过 |
| `schedule` §8.4 / §21.4 | `TaskReturn` 区分 `RecvReturn` 与 `WaitReturn`，阻塞 `recv` 与阻塞 `wait_objects` 共用调度出口不混淆返回值（:2600、§5.2 :447） | 原「无失败分支（`KernelActor` 内部）」声明不受影响；本次只增加返回值通道判别，未改变失败域 | 仍通过 |

## 4. 不变量与证明义务增量

- **新增 W17 — WaitObject 与等待一致性**（§16 :2113–2119）：`waiters` 双向一致、`wait_request ≠ None` ⟺ `BlockedWait`、`BlockedWait` 的 `pending_return`/`pending_wait_result` 为空、`deadline` 约束、仅 CPU Task。不变量总数 16→17。
- **W9 扩展**：`pending_return` 与 `pending_wait_result` 不得同时非空（§21.4 :2600）。
- **§5.7 唯一预启动状态**已列入 `wait_objects` 表（:546）。
- **§17.9 新增证明义务**（:2203–2205）：无丢失唤醒（观察—检查—阻塞窗口内 signal 不遗漏）等。**登记为义务，尚无证明**；§17 通则已把「每个成功迁移保持 W1–W17」列为义务范围（:2133）。
- **§22（信息性附录）**：明确标注不属冻结状态机；§22.1 给出 12 类计数，§22.3 要求后续文档不得再以旧 11 类计数描述规范。

## 5. 与实现的对照（证据缺口登记）

DOC-007 审核报告自身声明「不产生 MODEL-TEST、QEMU、Lean 或 COMPAT-TEST 证据」（:156–157），跟踪文档同步登记「实现/证明证据待后续切片」（`Prima OS 实现阶段.md` :400）。本次核对确认零实现属实：

| 检查 | 方法 | 结果 |
|---|---|---|
| `cosir/` 可执行模型 | 全目录检索 `WaitObject`/`wait_object`/`memseg_protect`/`task_alert`/`signal_object` | **无任何匹配**（含 `state.rs`/`ops.rs`/`abstract_ops.rs`） |
| `kernel/`、`prima-ostd/`、`prima-runtime/`、`prima-user/` | 全仓 `*.rs` 检索同一组标识 | **无任何匹配** |
| `lean/` | 模型范围仍为 BLK-001 三原语（bootstrap/excResume/derive） | **未建模** WaitObject/protect |
| W17 / §17.9 | `cosir` 的 `well_formed`/`check_invariants` 系列 | **未检查** W17；§17.9 义务无对应证明或模型检验 |

即：新增 8 操作目前**只有规范文本证据**。该缺口属阶段 2/3/4 实现与 `REFINEMENT-PROOF` 范畴（与 SPEC-054 §4 的 Timer/Namespace/Device「仅规范定义」同类），不是 S0-X2 定义的失败语义缺口。

## 6. 计数与口径一致性

- 常规操作面：SPEC-054 检查 26 项 → 本报告新增 8 项 → DOC-007 后冻结面常规操作共 **34 项**；机制计数 12 类常规 + Bootstrap 共 13 项（§22.1 :2620）。
- **矛盾登记**：[[Prima OS]] 第 777 行仍写「S0-X1 与 S0-X2（逐操作覆盖报告）均未补齐」，与跟踪文档 :419–420（S0-X2 已勾、S0-X1 未勾）矛盾。按 README 约定，进度权威为跟踪文档；白皮书该段是 2026-08 前后的过期快照，应在本报告定稿时一并修订。

## 7. 判定

1. **字面条件**：34 项常规操作（含本报告 8 项增量）在 DOC-007 冻结面上均满足三分类口径之一 → **S0-X2「每项操作具有明确成功与失败语义」的规范文本条件重新成立**，SPEC-054 勾选的证据基线经本次增量复核补全。
2. **不据此宣称**：新增操作已被机器验证（cosir 无实现、Lean 无建模、QEMU 无路径，§5）；W17/§17.9 无证明；本报告为文档审查、harness 内产出，独立性不满足 S0-X1 所设的组织级门槛——**S0-X2 与 S0-X1 的门槛差异沿 SPEC-054 §6 的既有口径执行**。
3. **登记结果**：本报告作为 SPEC-054 的 DOC-007 增量证据，补入跟踪文档 S0-X2 证据栏。S0-X1 的独立性门槛不变。

## 8. 遗留事项

- [x] 登记 SPEC-134，追加至跟踪文档 S0-X2 证据行与 `docs/README.md` 目录。
- [x] 同步修订 [[Prima OS]] 的阶段 0 过期表述（§6 矛盾项）。
- [ ] 为 8 项新操作立实现切片：`cosir` 模型（MODEL-TEST）→ `kernel`/`prima-ostd`（QEMU）→ Lean（W17/§17.9 义务，尤其无丢失唤醒）。建议纳入阶段 2/3 后续或双人格 holder 模型 H1 一并排期。
- [ ] W17 与 §17.9 尚无逐条检查/证明登记，§17.3 的 W1–W17 全量保持义务在 Lean 侧仅覆盖 BLK-001 切片面（见 `lean/README.md` 证据边界），需在实现切片启动时一并声明覆盖增量。
- [x] 检查现行权威文档中的「11 类」表述：`COSIR 原语规范.md` §22.1 和跟踪文档的历史决策行均明确标为修订前计数；旧计划与 2026-10-14 审核报告是历史记录，无现行计数冲突。

## 9. 证据边界

本报告是对规范文本的覆盖核对，证据等级为文档审查（harness 内会话）；不构成机器检查，不声称实现与规范一致（`REFINEMENT-PROOF`），不覆盖 §2.2 非目标（调度公平性/无饥饿、隐蔽信道、硬件 MMU/IOMMU、SMP、资源配额），不推进也不关闭 S0-X1。§2/§3 的判定引用均给出规范行号，供独立复核者复查原文。
