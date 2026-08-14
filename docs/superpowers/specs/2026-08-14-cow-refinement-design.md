# COW fork 组合协议细化：cow_refine 验证层设计

> 文档类型：阶段 0 细化层设计草案
>
> 关联规范：[[COSIR 原语规范]]（§12.8 COW fork 建模示例、§12.9/§12.10 page fault 与 exc_resume、研究主张 H1 有限原语完备性）
>
> 状态：设计草案，待实施
>
> 关联实现：`cosir/src/ops.rs`、`cosir/src/memseg_refine.rs`、`cosir/src/pagewalk.rs`（均为既有 host 模型，本切片不修改它们）

## 1. 目标、范围与证据边界

本切片的可证伪目标是：**证明 COSIR §12.8 已声明的原语组合——不使用任何新增内核机制——能在 host 模型中精确表达 COW fork**。若成功路径在某一步确实需要借用既有原语之外的能力，则那一步升级为对研究主张 H1（有限原语完备性）的反例，并记录为研究停止条件，而不是通过扩大原语集掩盖。

本切片只提供 host 可执行状态模型及其 `MODEL-TEST + REFINEMENT-DESIGN`：

- **不修改** `cosir/src/ops.rs` 与 `cosir/src/memseg_refine.rs` 两个既有、已独立验收的 host 模型；
- **不实现**真实 IDT/trap、真实访问指令执行或真实页复制硬件；`write_fault` 是对"写访问遭遇保护违规"的 host 近似；
- **不提供** `KERNEL-TRACE` 或 `REFINEMENT-PROOF`，形式化义务留给 BLK-001；
- **不新增** POSIR 一等原语；本切片的全部动作均为既有原语的组合与断言。

证据边界：本切片成立时，为 §12.8 的"COW 在用户态"声明在 host 层提供支持证据；反例存在时不掩盖边界。两者都写入阶段追踪器。

## 2. 架构与模型对齐

### 2.1 关键约束：两套独立表示

COW 组合横跨两个独立、已验收的 host 模型，二者以 `TaskId` 对齐但状态完全分开：

| 模型 | 文件 | Task 表示 | 关注 |
|---|---|---|---|
| `RefineState` | `memseg_refine.rs` | `TaskCtx { table: PageTable, vspace: BTreeMap<VirtAddr, SegmentRef> }` | 真实四级索引树、PTE 权限、segment、帧池、W6/W7 α |
| `ops::State` | `ops.rs` | `Task { vspace: Vec<PageMapping>, status, kernel_resume_frame }` | `Faulted`/`pending_exc`、订阅、`raise_exception`/`exc_resume`/`schedule`、Audit |

同一 Task 的"页权限"（在 `RefineState`）与"执行状态"（在 `ops::State`）不由任一模型单独承载。本切片显式建模并断言二者的一致性——这本身就是 COW 表达需要维持的细化事实。

### 2.2 验证层

新增 `cosir/src/cow_refine.rs`，只 `use` 两个既有模型的公开项，不通过 `pub(crate)` 侵入它们内部：

- 公开函数直接暴露 `&mut RefineState` 与 `&mut ops::State`（方案 A），内部自行组织调用顺序；
- 提供 `cow_state_consistent` 跨模型一致性断言，供测试与场景复用；
- 失败/原子性用例比较完整组合状态二元组 `(pf, exc)`，而非单个模型。

## 3. 核心动作语义

### 3.1 `probe_write`

```text
probe_write(pf: &RefineState, task: TaskId, va: VirtAddr) -> CowProbe
CowProbe ::= ReadOnly | Writable | NotMapped
```

纯查询 `RefineState` 四级树中该页的可观察权限，**无副作用、不修改任何状态**：

- `NotMapped`：无叶 PTE、或叶非 user、或 `access` 不含 W；
- `Writable`：存在 user 且可写叶；
- `ReadOnly`：存在 user 但只读——COW 目标命中。

这一步是"页表视角的可观察事实"，可被任意测试独立断言。

### 3.2 `write_fault`

```text
write_fault(exc: &mut ops::State, pf: &mut RefineState, task: TaskId, va: VirtAddr)
    -> WriteOutcome
WriteOutcome ::= Wrote | Faulted(ExceptionKind)
```

模拟一条写指令的可观察结果，忠实"指令要么写成功、要么 fault"：

1. 先 `probe_write` 判定；
2. 若 `Writable` → 返回 `Wrote`（host 不模拟内容字节，状态不变）；
3. 若 `ReadOnly` → 确保 `task` 为 `Running`（`schedule`），再调用 `ops::raise_exception(PageFault{ cause = PageProtectionViolation })` → `Running → Faulted` + `pending_exc`，返回 `Faulted(...)`；
4. `NotMapped`：COW 复制流程不适用于未映射页，故返回 `Faulted(PageFault{ page_present=false })` 而非隐式映射。这属于 COW 场景的边界（应由按需映射而非 COW 复制处理），主成功路径断言其不出现。

`probe_write` 与 fault 递送是分离可观察的两级动作；`write_fault` 是它们的组合。

### 3.3 `cow_state_consistent`

```text
cow_state_consistent(pf: &RefineState, exc: &ops::State, task: TaskId) -> bool
```

断言该 Task 在两模型中的可观察状态一致。覆盖至少：`RefineState.vspace[va]` 的 allow/权与 `ops::State` 对应 `PageMapping` 的 `access/user` 相符；两模型的 Task 都存在且 status 不矛盾（`RefineState` 无 status，故只核对映射侧）。

## 4. COW 成功路径（主用例）

把 §12.8 编译为 8 步，每步后断言 `cow_state_consistent` 与 `memseg_alpha`：

| 步 | §12.8 动作 | host 组合 | 关键断言 |
|---|---|---|---|
| 1 | 父 P 持私有可写 S | `RefineState: spawn_task(P)+memseg_create+memseg_map(RW)`；`ops: tasks[P]` RW mapping | P 正常写不 fault（`write_fault`→`Wrote`） |
| 2 | 子 C 只读共享 S | `spawn_task(C)` + `memseg_map(C, R)`；`ops: tasks[C]` R mapping | C 只读共享，α 成立 |
| 3 | C 写只读页 → PF | `write_fault(exc, pf, C, va)` | 返回 `Faulted`；C 在 ops 为 `Faulted` + pending（PageProtectionViolation） |
| 4 | LibOS 分配副本并复制 | `memseg_create(pf, new_seg)`，复制原帧地址集合 | 新 seg 帧池合法；分配原子 |
| 5 | C 换映射为私有可写 | `memseg_unmap(C, 只读)` + `memseg_map(C, RW)`；`ops: tasks[C].vspace` 同步为 RW | `cow_state_consistent` 成立（两模型不漂移） |
| 6 | 恢复 | 准备 canonical user `SretFrame` 写入 frame Segment，授权 capability + 订阅就绪，`ops::exc_resume(exc, C, src_cap, frame_cap, off)` | `Faulted → Ready`，`kernel_resume_frame` 记录，Audit `TaskResumed` |
| 7 | 恢复执行 | `ops::schedule(exc, C)` | `Ready → Running` |
| 8 | 终态可证伪 | `cow_state_consistent(pf, exc, C)` 等 | C 的 va 双视图均 RW（私有副本）；P 私有页仍私有 RW；**父子对原共享页的写隔离成立，未新增内核对象** |

第 5 步是全切片最微妙的细化事实：复制换映射要求同时更新两个模型的映射表示，并靠 `cow_state_consistent` 保证不漂移。这一步是为 §12.8"在用户态换映射"提供的可证伪 host 表达。

## 5. 反例与边界（证伪部分）

本切片不只在成功路径上证明可实现，还诚实列出现有原语组合的边界：

| ID | 场景 | 期望结果（可证伪断言） |
|---|---|---|
| R1 | 父子**同时**写原共享只读页；两者都 fault 到 `Faulted`+pending | 现原语提供**互斥/仲裁**原语，故并发 COW 的用户态协调是显式责任；不把这缺失当作新增原语的理由，而作为用户态协调边界记录 |
| R2 | 父子各自 pending 指向同一 source 时，对其中一个 `exc_resume` | `ops::PendingException` 按 Task 分别存在（每 Task 一条）；对一个 Task 的 `exc_resume` 只消费其自身 pending，不串扰另一个 Task。用"另一个 Task 的 pending 仍可独立恢复"验证原子性隔离 |
| R3 | 子已拿私写副本后，父**又**改写自己的私有页 | 父页是 RW 私有 → 不触发 fault（`write_fault`→`Wrote`）；COW 之后正常写不误触，正向隔离成立 |
| R4 | 试图让**内核**在 cause 时自动复制页（而非用户态 LibOS） | 内核不解释异常语义、无"复制页"原语 → **内核式 COW 不可由现有原语组合表达**；这不是反例，而是对 §12.8"所有 COW 逻辑在用户态"的直接支持证据与边界澄清 |

若成功路径的某一步在实施中发现确实需要一个既有原语无法提供的能力，则该步升级为 H1 反例，记入阶段追踪器研究停止条件，并停止扩大原语集。

## 6. 错误模型与测试策略

- `probe_write` 无错误（纯查询三态）；
- `write_fault` 返回 `WriteOutcome` 枚举（`Wrote`/`Faulted`），不把"写成功"建模为错误；
- `exc_resume` 复用 `ops::ResumeError`，不新增错误域；
- **测试**：成功路径每步断言组合不变量；R1–R4 各含失败/原子性用例，比较完整组合状态 `(pf.clone(), exc.clone())`；每个 mutation 证明状态深拷贝相等，延续 exc_resume/memseg 切片的原子性风格。

## 7. 产物与证据定位

- 新增 `cosir/src/cow_refine.rs`（`#![allow(dead_code)]`）与 `cosir/src/cow_refine_tests.rs`；
- `cosir/src/main.rs` 增补 COW smoke 场景（沿用既有 memseg smoke 风格）；
- `Prima OS 实现阶段.md` 新增 `SPEC-0xx`（`MODEL-TEST + REFINEMENT-DESIGN + 反例`），记录成功路径与 R1–R4 结果；
- `COSIR 原语规范.md` §12.8 增补证据边界注释，声明其"COW 在用户态"声明在本切片的 host 层证据边界；
- 不修改 `ops.rs`、`memseg_refine.rs`、`pagewalk.rs`、`state.rs` 的既有语义。

证据结论表述对称：切片成立 → 对 H1 提供支持证据；发现不可表达 → 记为 H1 反例。二者都写入追踪器，不通过修改措辞掩盖。

## 8. 层归属

COW 的业务语义（何时复制、为谁换映射、如何仲裁并发写方）属于用户态 LibOS；本切片只验证内核既有原语组合能否承载该业务所需的全部状态转换。`probe_write`/`write_fault` 是 host 模型对"写访问遭遇保护违规"的近似，不声称模拟真实访问指令。其他架构必须另行验证各自页表/异常 ABI；本文不据此声称跨架构 COW 已经成立。
