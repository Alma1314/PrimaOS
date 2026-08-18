# BLK-001 形式化模型（Lean）

> 证据边界：本切片（BLK-001）仅以 **MODEL-TEST** 支撑 bootstrap 与 exc_resume 的可执行状态模型。
> **不宣称** `REFINEMENT-PROOF`、`KERNEL-TRACE`、`COMPAT-TEST`、机器证明，或真实硬件保证。
> 本文是 Lean 模型（`Formal` 命名空间）与 cosir host 模型之间的**唯一权威映照契约**。

## 模块与入口

- `Formal.State`（`Formal/State.lean`）：`Object`/`Capability`/`State`/`WellFormed`/`initTaskId = 1`。
- `Formal.Bootstrap`（`Formal/Bootstrap.lean`）：`alpha`/`emptyState`/`bootedState`/`bootstrap` + 5 条定理（`by rfl`）。
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
| `State.caps : Nat → Option Capability` | `state::State.capabilities : HashMap<CapId, Capability>` | 能力表：cap 部分函数 ↔ HashMap |
| `Capability.owner/obj/rights/active` | `state::Capability.owner/object/rights/active` | 字段形态对应 |
| `Object.segment/task/excSource` | `state::CapObject::Segment/Task/ExceptionSource` | 对象引用和类型（名称差异） |
| `initTaskId = 1` | `abstract_ops::init_task_id() = 1` | init Task 恒定量 ID |
| `WellFormed : Prop := True` | （模型最小锚） | 良构谓词,本切片恒真 |

## 对拍机制

- **Lean 侧为参考语义**：模块中的 `theorem`（`by rfl`）与入口 `Formal.lean` 的 `#eval` 冒烟给出可执行、可检查的参考结果。
- **Rust 侧对拍**：`cosir/tests/lean_backtrace.rs` 把同一输入在 cosir host 模型跑一遍，断言与 Lean 语义一致：
  - `bootstrap_parity_lean`：α 投影保真（`α(booted) ≠ α(pre_bootstrap)`）与 init task 存在，对应 Lean `alpha_projection_distinguishes_boot`/`bootstrap_empty_mints_init_cap`。
  - `exc_resume_failure_parity_lean`：caller 无源能力 → `Err(InvalidExcSource)` 且状态完全不变，对应 Lean `excResume … = none` 的失败原子性。
- 两端一致即本轮 `MODEL-TEST` 通过；一致性不升级为 refinement 结论。

## 已知不覆盖（后续切片）

- `tlb_refine`：非一致刷新 / 显式 `invalidate_va` / `flush_all`（见 cosir `tlb_refine.rs`，Lean 建模留待后续切片）。
- `memseg_refine`：MemorySegment host 四级索引树与 W6/W7 双视图（见 cosir `memseg_refine.rs`，Lean 建模留待后续切片）。
- `cow_refine`：COW 组合序列（H1 承载体，见 cosir `cow_refine.rs`，Lean 建模留待后续切片）。
- 以上内容在本切片**仅登记为待办**，不标为已完成。
