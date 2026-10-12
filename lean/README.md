# BLK-001 形式化模型（Lean）

> 证据边界：本切片（BLK-001）仅以 **MODEL-TEST** 支撑 bootstrap 与 exc_resume 的可执行状态模型。
> **不宣称** `REFINEMENT-PROOF`、`KERNEL-TRACE`、`COMPAT-TEST`、机器证明，或真实硬件保证。
> 本文是 Lean 模型（`Formal` 命名空间）与 cosir host 模型之间的**唯一权威映照契约**。

## 模块与入口

- `Formal.State`（`Formal/State.lean`）：`Object`/`Capability`/`State`/`WellFormed`/`initTaskId = 1`。
- `Formal.Bootstrap`（`Formal/Bootstrap.lean`）：`alpha`/`emptyState`/`bootedState`/`bootstrap` + 7 条定理（`by rfl`，含 §14 初始 Authority 三条）。
- `Formal.ExcResume`（`Formal/ExcResume.lean`）：`excResume` + 4 条定理（`by rfl`）。
- `Formal.lean`（入口）：`import` 上述三个模块，使 `lake build` 覆盖全部 `Formal.*`；含 `smokeState` 冒烟 `#eval`。

## 证据边界

| 维度 | 本切片覆盖 | 不覆盖（后续切片） |
|---|---|---|
| 模型对象 | `bootstrap`（空态→booted 态）、`excResume`（授权检查） | `tlb_refine`、`memseg_refine`、`cow_refine` |
| 证据等级 | `MODEL-TEST`（Lean `#eval`/`theorem` 参考 + cosir host 对拍） | `REFINEMENT-PROOF`、`KERNEL-TRACE`、`COMPAT-TEST`、机器证明 |
| 目标 | 可执行状态迁移、良构谓词锚、失败原子性 | 真实硬件、CR3/PTE、TLB、多核、帧字节复制 |

## 映照契约（Lean ↔ cosir 逐字段）

| Lean（Formal 命名空间） | cosir Rust | 说明 |
|---|---|---|
| `State.booted : Bool` | `state::State.booted : bool` | POSIR bootstrap 标志 |
| `State.caps : Nat → Option Capability` | `state::State.capabilities : HashMap<CapId, Capability>` | 能力表：cap 部分函数 ↔ HashMap。空态两侧一致；**booted 态两侧亦一致**——均铸 §14 三条无父根 Capability（CapId 1/2/3） |
| `Capability.owner/obj/rights/active` | `state::Capability.owner/object/rights/active` | 字段形态对应（`rights` 两侧均为 16 位位掩码，位值一致） |
| `Object.segment/task/excSource/namespace` | `state::CapObject::Segment/Task/ExceptionSource/Namespace` | 对象引用和类型（名称差异） |
| `initTaskId = 1` | `abstract_ops::init_task_id() = 1` | init Task 恒定量 ID |
| （**不建模**，Lean `State` 无 `tasks` 字段） | `state::Task.root_ns_cap : CapId` | §5.2/§14 步骤 7 结构性根 Namespace Capability（W5）；Lean 侧只以三条根 Cap 的 `owner/obj` 隐含表示归属，**无该字段**（cosir-only） |
| （**不建模**，Lean 无 MachineState） | `machine::MachineState.image_frames`/`image_virtual_base` | §14 步骤 4 image 逐页 `vspace` 的投影输入；**cosir-only** |
| （**不建模**，Lean `State` 无 `audit`/`now`/`next_event`） | `state::AuditEvent{id,at,actor,kind}`、`state::Actor`、`state::State.now`/`next_event` | §13.1 事件信封与 §4 时间/序号、§6.1 `Actor`；**cosir-only**（Lean 不建模 Audit） |
| （**不建模**，`Capability` 无 `parent`/`origin`/位置表） | （无对应字段） | §4.2 根性（`parent = None`）、§4.6 `HeldBy` 位置、W3/W4/W13 的 `created_at → 铸造事件` **均不可表示**；注释/报告称 bootstrap 三条根 Cap「无父根」仅为 §14 语义描述，**非模型可表示字段**（cosir/Lean 共同边界） |
| `WellFormed : Prop := True` | （模型最小锚） | 良构谓词,本切片恒真 |

## 对拍机制

- **Lean 侧为参考语义**：模块中的 `theorem`（`by rfl`）与入口 `Formal.lean` 的 `#eval` 冒烟给出可执行、可检查的参考结果。
- **Rust 侧对拍**：`cosir/tests/lean_backtrace.rs` 把同一输入在 cosir host 模型跑一遍，断言与 Lean 语义一致：
  - `bootstrap_parity_lean`：α 投影保真（`α(booted) ≠ α(pre_bootstrap)`）、init task 存在，**逐条断言 §14 三条根 Capability**（数量=3、CapId 1/2/3 的对象与 rights，对应 Lean `bootstrap_empty_mints_{image,namespace,task}_cap`），并**钉住 cosir 侧的 §14 步骤 4/7 投影**（image 逐页 `vspace` 的基址/条数、`Task.root_ns_cap == 2`）——这两项 Lean 未建模，见下表与「已知不覆盖」。
  - `exc_resume_failure_parity_lean`：caller 无源能力 → `Err(InvalidExcSource)` 且状态完全不变，对应 Lean `excResume … = none` 的失败原子性。
- 两端一致即本轮 `MODEL-TEST` 通过；一致性不升级为 refinement 结论。
- **定理性质（如实声明）**：Lean 侧 11 条 `theorem` 全部是定义展开（`by rfl` / `trivial`，且 `WellFormed := True` 恒真），**不提供独立的不变量保证**；Lean `State` 是最小可表达能力子集（**无 `tasks` 字段**，init 以三条指向 Segment/Namespace/Task 的根 Capability 表示），`excResume` 只建模授权检查、**不建模 §12.10 的成功提交段**（清零帧/清 pending/存 kernel frame/追加 Audit/置 Ready）。

## 已知不覆盖（后续切片）

- `tlb_refine`：非一致刷新 / 显式 `invalidate_va` / `flush_all`（见 cosir `tlb_refine.rs`，Lean 建模留待后续切片）。
- `memseg_refine`：MemorySegment host 四级索引树与 W6/W7 双视图（见 cosir `memseg_refine.rs`，Lean 建模留待后续切片）。
- `cow_refine`：COW 组合序列（H1 承载体，见 cosir `cow_refine.rs`，Lean 建模留待后续切片）。
- **§14 步骤 4/7（image vspace 与结构性根）**：§20.3「init 能力集恰好等于 §14 初始 Authority」的**能力部分已兑现**（三条根 Capability 两侧一致）；**cosir 侧已投影** §14 步骤 4 的 image 逐页 `vspace`（由 `MachineState.image_frames`/`image_virtual_base` 生成 `PageMapping{backing}`）与步骤 7 的结构性根（`Task.root_ns_cap = 2`），并由 `bootstrap_parity_lean` 钉住。**Lean 侧未建模**这两项（`State` 无 `tasks` 字段、无 MachineState）；该分歧已在映照契约表如实标注（cosir-only）。
- **`raise_exception` 的 Audit（§12.5/§13.1）**：**cosir 侧已补齐**（2026-10-12）——`AuditEvent` 新增 `ExceptionRaised`（§12.5 动作 1）与 `TaskStateChanged`（§13.1），`ops::raise_exception` 按 §12.5 顺序追加；同日续片（`ISSUE-EXC-008`）再把 `AuditEvent` 重构为 §13.1 **信封 `{id, at, actor, kind}`** 并补 `State.now`/`next_event` 与 §6.1 `Actor`。**Lean 侧不建模 `audit`**（`State` 无该字段/无时间），故无对拍项、此分歧为 cosir-only。cosir 的 `now` 无推进源（不建模 `time_advance`/Timer），`at` 恒为 0（规范 §20.4 已登记）。
- **§14 步骤 9 的 bootstrap 初始 Audit（`ISSUE-EXC-009`，2026-10-13）**：**cosir 侧已补齐**——`abstract_ops::abstract_state(booted)` 写入第一组 Audit：`Bootstrapped` → 两个 `ObjectCreated`（image Segment、init Namespace，`segment` 标志区分同 id 对象）→ init Task 的**创建**事件。顺序与形态镜像真实内核（`kernel/src/bootstrap.rs`）；init Task 用 §13.1 的 `TaskCreated`（**创建非迁移**，故不误用 `TaskStateChanged`）。三条根 Capability **不发 `CapMinted`**（§13.1 该事件用于运行时铸造路径，内核亦不发）。**Lean 侧不建模 `audit`**，故 cosir-only、无对拍项。
- 以上内容在本切片**仅登记为待办**，不标为已完成。
