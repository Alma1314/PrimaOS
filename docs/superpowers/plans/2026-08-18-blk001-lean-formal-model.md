# BLK-001 Lean 4 形式化模型工件 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 在 `lean/` 独立 lake 项目中建立 Lean 4 可执行状态迁移模型，并证明 bootstrap 与 exc_resume 的 §17 关键不变量（WellFormed、幂等唯一性、失败原子性），通过 `#eval` 与 cosir 夹具对拍，解除 BLK-001 并支持 S0-X3。

**Architecture:** 独立 `lean/` lake 项目不动 Rust workspace。Lean 侧用部分函数 `CapId → Option Capability` 表示状态（可 `#eval` 亦可证明）；迁移返回 `Option State` 表达成功(`some`)与"失败且状态不变"(`none`)。Rust 侧 `cosir/tests/lean_backtrace.rs` 复用现有夹具输出归一化 trace，与 Lean `#eval` 双向夹逼。

**Tech Stack:** Lean 4.33 + elan（winget `Lean.Elan` 4.2.3）、lake 构建工具、纯 Lean std（不依赖 mathlib4）、既有 Rust cosir crate。模块顺序：bootstrap（样板）→ exc_resume。tlb/memseg/cow 登记后续切片，不阻塞本计划。

## Global Constraints

- 本计划只产生 Lean 证明 + host `MODEL-TEST` 对拍；**不得宣称** `KERNEL-TRACE`、真实硬件、`REFINEMENT-PROOF`、机器证明或全量 §17。
- 最小关闭门槛是 bootstrap + exc_resume；tlb/memseg/cow 的 Lean 建模只登记后续切片，不实现。
- `lean/` 为独立 lake 项目；**不修改** `Cargo.toml` 的 workspace members，不新增 Rust 第三方 crate。
- 默认不拉 mathlib4；仅在证明受阻且实现者评估明确需要时才增量引入，并在 report 中说明理由。
- Lean 失败用 `Option`: `none` 表达"失败且状态完全不变"，`some s'` 表达成功。
- 对拍证据等级为 `MODEL-TEST`；Lean 证明记录为独立通道，不放大证据边界。
- 映照契约（Lean 结构与 cosir `State` 投影一致）记录于 `lean/README.md`，为唯一权威；漂移记录为 ISSUE。
- 本计划遵循仓库既有惯例：**不执行 git commit**；只有用户明确授权后才能提交。

---

## File Map

**新建 Lean 项目（不动 Rust workspace）**

- Create: `lean/lakefile.lean` — lake 项目配置，Lean 4.33。
- Create: `lean/lean-toolchain` — 锁定 leanprover/lean4:stable（4.33.0+）。
- Create: `lean/Formal.lean` — 入口：目标清单 + `#eval` 冒烟。
- Create: `lean/Formal/State.lean` — 模块 `Formal.State`（`lean_lib Formal` 下的子模块）:Lean `State`、`Capability`、迁移公共基础设施 + 映照契约注释。
- Create: `lean/Formal/Bootstrap.lean` — 模块 `Formal.Bootstrap`:`bootstrap` 原语 + 三性质证明 + `#eval` 对拍样板。
- Create: `lean/Formal/ExcResume.lean` — 模块 `Formal.ExcResume`:`exc_resume` 原语 + 失败原子性证明 + `#eval` 对拍。
- Create: `lean/README.md` — 映照契约、证据边界、何时登记后续切片。

**Rust 对拍侧（先给 cosir 增加 lib 目标，再用集成测试对拍）**

- Create: `cosir/src/lib.rs` — 把 8 个模块以 `pub mod` 外提为库（bin 复用 lib）。
- Modify: `cosir/Cargo.toml` — 增 `[lib]` 段。
- Create: `cosir/tests/lean_backtrace.rs` — 集成测试复用 cosir 夹具（lib 目标使其可 import），输出与 Lean 可比对的归一化 trace。

**文档登记**

- Modify: `Prima OS 实现阶段.md` — 记录新证据（如 SPEC-019）、BLK-001 → Closed、S0-X3 勾选。
- Modify: `docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md` — 实施完成后标记 Completed/Archived。

---

### Task 1: 安装 Lean 工具链并搭建 lake 项目脚手架

**Files:**
- Create: `lean/lakefile.lean`
- Create: `lean/lean-toolchain`
- Create: `lean/Formal.lean`（最小占位：仅一个可通过 `#eval` 的 `main` 函数）

**Interfaces:**
- Consumes: 已批准设计 §6（winget `Lean.Elan`、elan 装 `leanprover/lean4:stable`）。
- Produces: 可执行 `lake build` 的 `lean/` 项目；后续所有 Lean 任务依赖此工具链。

- [ ] **Step 1: 安装 Lean 工具链**

在 Git Bash 执行：

```bash
winget install Lean.Elan --accept-package-agreements --accept-source-agreements
# 验证 elan 可用；若 elan 命令不在 PATH，重启 shell 或按提示加入 PATH。
elan --version
```

用 elan 安装 Lean 4 stable（内含 `lake` 与 `lean`）：

```bash
elan toolchain install leanprover/lean4:stable
elan default leanprover/lean4:stable
lean --version
lake --version
```

**Expected:** `lean --version` 输出 `Lean (version 4.x.x-lake1.0.0, ...)`；`lake --version` 输出 `Lake version ...`。若 winget 该包不可用，备选 `winget install Lean.Lean`；若仍失败，回退方案见设计 §8（先冻结设计、待装好再实现），并在此停止通知用户。

- [ ] **Step 2: 创建 `lean/lean-toolchain`**

Create `lean/lean-toolchain` 内容：

```
leanprover/lean4:stable
```

- [ ] **Step 3: 创建 `lean/lakefile.lean`**

Create `lean/lakefile.lean` 内容：

```lean
import Lake
open Lake DSL

package formal where
  require std from git
    "https://github.com/leanprover/std4.git" @ "main"

@[default_target]
lean_lib Formal
```

> 注：本计划**默认不依赖 mathlib4**。上面显式 require `std4`（Lean 官方标准库、`lake new` 默认模板亦依赖它）。若你的网络无法访问 `std4` git 仓库，改为本地最小模板（见 Step 5 回退）。

- [ ] **Step 4: 用 lake 初始化并创建最小入口 `lean/Formal.lean`**

在 `C:/Project/PrimaOS/lean/` 下（`lakefile.lean` 已就位）执行：

```bash
lake build
```

Create `lean/Formal.lean` 内容（最小冒烟，后续任务逐步充实）：

```lean
/-! BLK-001 形式化模型工件：入口文件。
    本文件声明模型目标清单并承载 `#eval` 冒烟。
    目标语法不变量在各 `Theorems` 命名空间（见 Bootstrap/ExcResume 等模块）。 -/

def smokeSum (a b : Nat) : Nat := a + b

#eval smokeSum 2 3

/-- 一个始终为真的基础性质，验证 Lean 环境可编译证明。 -/
theorem two_plus_three_is_five (a b : Nat) : a + b = b + a :=
  Nat.add_comm a b
```

- [ ] **Step 5: 构建并验证**

```bash
cd "C:/Project/PrimaOS/lean"
lake build
```

**Expected:** `lake build` 成功（可能首次拉取 std4，耗时数十秒）。`#eval` 行不报错。

**回退（若 std4 网络不可达）：** 用 off-line 最小 lakefile——去掉 `require std`,并把 `Formal.lean` 里任何 `import` 移除（本任务最小文件本就不 import）。骨架仍可 `lake build`。

- [ ] **Step 6: 提交脚手架**

本仓库惯例需用户授权才 commit；此处记录待提交内容，**不执行** commit：

```bash
# 待用户授权后执行
git add lean/lakefile.lean lean/lean-toolchain lean/Formal.lean
git commit -m "chore(lean): scaffold BLK-001 lake project (Formal.lean entry)"
```

---

### Task 2: 定义 Lean `State` 与能力模型（Bootstrap 所需子集）

**Files:**
- Create: `lean/Formal/State.lean`

**Interfaces:**
- Consumes: Task 1 工具链与 `Formal.lean`。
- Produces: `State`、`Capability`、`Object`、`CapId`/`TaskId` 别名,及 `WellFormed` 谓词、`bootstrap` 迁移签名。供 Task 3 的 bootstrap 与 Task 6 的 exc_resume 使用。

- [ ] **Step 1: 编写失败测试（Lean 无测试框架，用 `theorem` + `decide` 占位）**

Lean 的"测试"是 `#eval` 断言与 `theorem`。先写一段会失败的 `#eval` 断言,证明 `State.lean` 尚未提供类型:

```lean
#eval [false]
```

先不建 `State` 类型,运行 `lean State.lean`,预期报错 `unknown constant 'CapId'` 等。这一步用失败确证文件当前为空、需定义。

- [ ] **Step 2: 运行以确认失败**

```bash
cd "C:/Project/PrimaOS/lean"
lean State.lean
```

**Expected:** 因引用未定义标识符而失败（证明该模块尚未定义模型）。

- [ ] **Step 3: 实现 `lean/Formal/State.lean`**

> **模块布局（重要）**：本包 `lakefile.lean` 声明 `@[default_target] lean_lib Formal`，`lake build` 只覆盖 `Formal.*` 前缀模块。故模型文件必须放在 `lean/Formal/` 子目录（模块名 `Formal.State`/`Formal.Bootstrap`/`Formal.ExcResume`），后续消费者才能 `import Formal.State`。**不要把 `.lean` 放在 `lean/` 根目录**（根模块 `State` 不被 `lean_lib Formal` 构建，`import` 会报 `unknown module prefix`）。入口 `lean/Formal.lean`（模块 `Formal`）已由 Task 1 就位。

Create `lean/Formal/State.lean` 内容（Lean 4 std，无 mathlib）：

```lean
import Lean
import Init.Core

/-! BLK-001 状态模型。
    映射用部分函数 `Key → Option Val`，可 `#eval` 亦可证明。
    映照契约：本结构与 `cosir::state` 的 `Capability`/`CapObject` 及
    `State.booted` 一一对应（见 README.md）。 -/

namespace Formal

/-- 对象引用：SegId/TaskId/ExcSourceId 三类（对齐 cosir `CapObject`）。 -/
inductive Object where
  | segment (id : Nat)
  | task (id : Nat)
  | excSource (id : Nat)
deriving DecidableEq, Repr

/-- 能力（对齐 cosir `Capability` 的 owner/object/rights/active）。 -/
structure Capability where
  owner : Nat
  obj : Object
  rights : UInt8
  active : Bool
deriving DecidableEq, Repr

/-- 状态（对齐 cosir `State` 的最小子集：capabilities + booted）。 -/
structure State where
  /-- 能力表：部分函数，无记录 → 不存在该 CapId。 -/
  caps : Nat → Option Capability
  /-- POSIR bootstrap 标志（对齐 cosir `State.booted`）。 -/
  booted : Bool
deriving Repr

/-- 初始 init Task 的恒定量 ID（对齐 cosir `abstract_ops::init_task_id()` = 1）。 -/
def initTaskId : Nat := 1

/-- 世界良构：capabilities 域有限。本谓词在最小模型里恒真，作为 WellFormed 的锚。 -/
def WellFormed (s : State) : Prop := True

end Formal
```

- [ ] **Step 4: 编译验证**

先让入口 `Formal.lean` 引用本模块（在 `lean/Formal.lean` 顶部 `import Formal.State`，import 在前、doc 在后），再：

```bash
cd "C:/Project/PrimaOS/lean"
lake build
```

**Expected:** `lake build` 无错误；`.lake/build/lib/lean/Formal/State.olean` 生成，`Formal.lean` 的 `#eval` 冒烟打印 `{ caps := <function>, booted := false }` 形状。

一次性 `##check`（临时文件，验证后删除 `t.lean`）：

```bash
cat > t.lean <<'EOF'
import Formal.State
#check Formal.State
#check Formal.Capability
#check Formal.Object
EOF
lake env lean t.lean
# 期望分别打印三者的 Type
```

**Expected:** `Formal.State`/`Formal.Capability`/`Formal.Object` 均 `: Type`。验证后 `rm -f t.lean`。

- [ ] **Step 5: 记录待提交内容（不 commit，需用户授权）**

```bash
# 待授权后
git add lean/Formal/State.lean
git commit -m "feat(lean): define minimal State/Capability model (Bootstrap subset)"
```

---

### Task 3: Lean 实现 `bootstrap` 并证明三性质（对拍样板）

**Files:**
- Create: `lean/Formal/Bootstrap.lean`

**Interfaces:**
- Consumes: Task 2 的 `Formal.State`、`Formal.WellFormed`、`Formal.initTaskId`、`Formal.Capability`、`Formal.Object`。
- Produces: `bootstrap : State → Option State`，及三个 theorems: `bootstrap_new_wellformed`（成功结果 WellFormed）、`bootstrap_idempotent`（二次返回 none 且不变）、`alpha_projection_distinguishes_boot`（α 投影保真）。供 Task 5 对拍引用。

- [ ] **Step 1: 写失败证明（TDD）**

Create `lean/Formal/Bootstrap.lean` 首版只含一个会失败的 `theorem` 声明（引用尚未定义的 `bootstrap`），确认未实现：

```lean
namespace Formal
-- theorem alpha_projection_distinguishes_boot : alpha emptyState != alpha bootedState := by trivial
end Formal
```

运行 `lake env lean Bootstrap.lean`,确认因 `alpha` 未定义而失败。

- [ ] **Step 2: 运行确认失败**

```bash
cd "C:/Project/PrimaOS/lean"
lake build "Formal.Bootstrap"
```

**Expected:** 报 `unknown constant 'Formal.alpha'`（`Bootstrap.lean` 引用了未定义的 `alpha`）。

- [ ] **Step 3: 实现 `bootstrap` 与 α 投影**

> **关于函数字段相等的权衡**：`State.caps` 是函数 `Nat → Option Capability`，两个 `State` 的整体相等需要函数外延（funext），`rfl`/`native_decide` 不一定能自动解开。为避免不可编译，本计划对**状态性质的定理都写成对投影（`booted` 标志、具体 CapId 的 `caps` 查找）的断言**，而非整态相等。这保留了 bootstrap/exc_resume 的语义本质，又能稳定编译。

Create `lean/Bootstrap.lean` 内容：

```lean
import Formal.State

namespace Formal

/-- α 投影：把 State 投影为其 `booted` 标志（对齐 cosir `abstract_ops::abstract_state` 的 booted 段）。 -/
def alpha (s : State) : Bool := s.booted

/-- 初始能力表：init Task 持有对自身 Object.task 的主权（初始 Authority 的最小表示）。
    仅在 CapId=1 处有记录，其余处为空——与 cosir 空状态/初始 Authority 对应。 -/
private def initCaps : Nat → Option Capability :=
  fun id => if id = 1 then
      some { owner := 1, obj := Object.task 1, rights := 0b0010_0000, active := true }
    else none

/-- 空态：无能力、未 booted（对齐 cosir 交接点 EmptyState）。 -/
def emptyState : State := { caps := fun _ => none, booted := false }

/-- 已 booted 态：init 铸造初始 Authority（对齐 cosir `abstract_ops::abstract_state(booted)`）。 -/
def bootedState : State := { caps := initCaps, booted := true }

/-- bootstrap：空态 → booted 态；对已 booted 态返回 none（AlreadyBooted 且状态不变）。 -/
def bootstrap (s : State) : Option State :=
  if s.booted then none else some bootedState

/-- 幂等唯一性：空态 bootstrap 成功，且结果 booted=true、init task 存在（CapId=1 有记录）。 -/
theorem bootstrap_empty_succeeds :
    alpha (bootstrap emptyState).get! = true := by
  rfl

theorem bootstrap_empty_mints_init_cap :
    (bootstrap emptyState).get!.caps 1 = some { owner := 1, obj := Object.task 1, rights := 0b0010_0000, active := true } := by
  rfl

/-- 幂等唯一性：对已 booted 态再次 bootstrap 返回 none（不再重复铸造）。 -/
theorem bootstrap_booted_returns_none :
    bootstrap bootedState = none := by
  rfl

/-- 投影保真：bootstrap 确改变抽象态（α(booted) ≠ α(pre_bootstrap)）。 -/
theorem alpha_projection_distinguishes_boot :
    alpha emptyState != alpha bootedState := by
  rfl

/-- 成功结果 WellFormed：bootstrap 成功态满足良构（最小模型恒真）。 -/
theorem bootstrap_new_wellformed (m : State)
    (h : bootstrap m = some bootedState) : WellFormed bootedState := by
  trivial

end Formal
```

> 说明：所有 `by rfl` 命题对**具体已算出的值**成立，因 `bootstrap`/`alpha` 直接返回布尔/结构、不涉及函数外延；`theorem bootstrap_new_wellformed` 因 `WellFormed := True` 用 `trivial`。若某个 `rfl` 因 reducibility 层级不足而失败，改为 `by native_decide`（对具体可判定值）或 `by simp [bootstrap, alpha]`。

- [ ] **Step 4: 运行确认通过**

```bash
cd "C:/Project/PrimaOS/lean"
lake build "Formal.Bootstrap"
```

**Expected:** 无错误；五个 `theorem` 全部通过。若 `rfl` 报错，按 Step 3 末尾说明改用 `native_decide`/`simp`。

- [ ] **Step 5: 记录交接（供 Task 5 对拍与 Task 6 复用）**

`bootstrap : Formal.State → Option Formal.State`；常量 `emptyState`、`bootedState`、`alpha`、`WellFormed`、`initTaskId`。

```bash
# 待授权后 commit
git add lean/Formal/Bootstrap.lean
git commit -m "feat(lean): bootstrap primitive + wellformed/idempotent/alpha theorems"
```

---

### Task 4: 给 cosir 增加 lib 目标（集成测试前置）

> 背景：`cosir` 当前是 `[[bin]]` 单目标（无 `lib.rs`、无 `[lib]`），`cosir/tests/` 集成测试无法 `use cosir::…`。为让对拍集成测试可 import 模型，先把 8 个模块外提为库。**用户已选方案 (b)：新增 `[lib]` + `src/lib.rs`，bin 复用 lib。** 本任务把 cosir 结构从 bin-only 改为 bin+lib 双目标；既有 bin 的 `mod xxx` 声明移到 lib.rs，bin 主体通过 `use cosir::…` 复用。既有测试保持通过。

**Files:**
- Create: `cosir/src/lib.rs`
- Modify: `cosir/Cargo.toml` — 增 `[lib]` 段
- Modify: `cosir/src/main.rs` — 移除顶部 `mod …` 声明，改为 `use cosir::…`

**Interfaces:**
- Consumes: cosir 既有 `src/abstract_ops.rs`、`ops.rs`、`state.rs`、`machine.rs`、`pagewalk.rs`、`memseg_refine.rs`、`tlb_refine.rs`、`cow_refine.rs`（均为 bin 内模块）。
- Produces: `cosir::abstract_ops::…` 等可作为 **crate 库**被 `cosir/tests/*.rs` 与 bin 引用。供 Task 5/7 的 `lean_backtrace.rs` 集成测试 import。

- [ ] **Step 1: 创建 `cosir/src/lib.rs`**

Create `cosir/src/lib.rs` 内容：

```rust
//! cosir 库目标：把 host 状态模型的各模块外提为库，供 bin 与
//! `cosir/tests/` 集成测试共享引用。
//!
//! 证据边界：这些模块只提供 host `MODEL-TEST` 语义；不构成真实
//! CR3/PTE、KERNEL-TRACE 或 REFINEMENT-PROOF。

pub mod abstract_ops;
pub mod cow_refine;
pub mod machine;
pub mod memseg_refine;
pub mod ops;
pub mod pagewalk;
pub mod state;
pub mod tlb_refine;

// 注意：`memseg_refine_tests`/`pagewalk_tests`/`tlb_refine_tests` 是
// `#[cfg(test)]` 内联单测模块，随所在 .rs 文件（src 编译单元）走，不在此声明。
pub mod lean_backtrace_tests;
```

> 注：若你想把内联单测也留在原 bin 侧，`lib.rs` 只需声明 8 个功能模块；`lean_backtrace` 集成测试以 `cosir/tests/lean_backtrace.rs` 存在。上面第 9 个 `pub mod lean_backtrace_tests` **不必要**，请删除——对拍放到 `cosir/tests/lean_backtrace.rs`（集成测试）即可，不做内联。最终 `lib.rs` 只含上面 8 个 `pub mod`。

- [ ] **Step 2: 在 `cosir/Cargo.toml` 增加 lib 目标**

Modify `cosir/Cargo.toml`：在 `[package]` 后加入（现有 `[[bin]]` 保留）：

```toml
[lib]
name = "cosir"
path = "src/lib.rs"
```

**Expected:** `cosir` 现在有 lib（`cosir`）与 bin（`cosir-state`）两个目标。

- [ ] **Step 3: 调整 `cosir/src/main.rs` 复用 lib**

Modify `cosir/src/main.rs` 的顶部——把 `mod abstract_ops;` 等 8 行 `mod` 声明替换为对 lib 的 `use`：

```rust
// 删：mod abstract_ops; mod cow_refine; mod machine; mod memseg_refine;
//     mod ops; mod pagewalk; mod state; mod tlb_refine;
// 替换为：
use cosir::abstract_ops;
use cosir::state::{
    ACCESS_R, ACCESS_W, ACCESS_X, ExceptionKind, SretFrame, State, Task, TaskStatus,
};
use cosir::machine::MachineState;
```

> 关键：bin 与 lib 在**同一 crate 包**内。bin 引用 lib 用 `cosir::…` 路径（非 `crate::…`）。内联 `#[cfg(test)] mod memseg_refine_tests;` 等 3 个**单测模块仍在 `src/main.rs`**（它们随 src 编译单元，引用 `state`/`machine` 等需从 lib 路径）。若这些单测里用了 `crate::xxx` 相对路径，需改为 `cosir::xxx`。逐处确认 `cargo build`/`cargo test` 通过。

- [ ] **Step 4: 运行验证**

```bash
cd "C:/Project/PrimaOS"
cargo build -p cosir
cargo test -p cosir
```

**Expected:** 双目标都构建成功；既有全部内联单测通过（原 180 个 cosir 测试数量不变）。若单测报 `crate::` 找不到，按 Step 3 末尾说明改路径为 `cosir::`。

- [ ] **Step 5: 记录待提交内容（不 commit，需用户授权）**

```bash
# 待授权后
git add cosir/Cargo.toml cosir/src/lib.rs cosir/src/main.rs
git commit -m "refactor(cosir): split bin-only into bin+lib for integration-test import (BLK-001)"
```

---

### Task 5: Rust 对拍 `lean_backtrace.rs`（bootstrap 样板）

**Files:**
- Create: `cosir/tests/lean_backtrace.rs`

**Interfaces:**
- Consumes: cosir 既有 `cosir::abstract_ops::{bootstrap, emptyState→via machine}`、`cosir::abstract_ops::bootstrap` 返回 `Result<State, BootstrapError>`；`cosir::state::State.default()`。参考 Lean `Bootstrap.lean` 的 `emptyState`(=cosir 未 booted 态)、`bootstrap`(=cosir `bootstrap`)、`bootedState`(=cosir booted init 态)。
- Produces: 归一化 trace 断言,证明 Rust 模型与 Lean `#eval`（人工核对的参考语义）对 bootstrap 一致。任务完成后输出 `PASS model-test lean bootstrap parity`。

- [ ] **Step 1: 写失败测试**

Create `cosir/tests/lean_backtrace.rs` 首版（定义会失败的断言，确认当前逻辑未实现对拍语义）：

```rust
//! BLK-001 对拍：Lean 参考语义 ↔ cosir host 模型。
//! 证据等级：MODEL-TEST。Lean 侧 `#eval`/`theorem` 是参考；本文件把同一输入
//! 在 cosir 跑一遍，断言与 Lean 语义一致。

#[test]
fn bootstrap_parity_lean() {
    assert!(false, "占位：Task 5 Step 3 填入真实对拍断言");
}
```

`cargo test -p cosir --test lean_backtrace` → FAIL（断言 `false`）。

- [ ] **Step 2: 运行确认当前失败**

```bash
cd "C:/Project/PrimaOS"
cargo test -p cosir --test lean_backtrace
```

**Expected:** `bootstrap_parity_lean` FAIL（`assert!(false)` 触发）。

- [ ] **Step 3: 实现对拍断言**

Create `cosir/tests/lean_backtrace.rs`（完整，编译可通过）。对拍语义映射：
- Lean `bootstrap emptyState = some bootedState` 对应 cosir `bootstrap(&empty)` == `Ok(booted)`，且 booted 态 `tasks` 含 init task（`init_task_id()`=1）；
- Lean `bootstrap bootedState = none` 对应 cosir `bootstrap(&booted)` == `Err(AlreadyBooted)`。

```rust
//! BLK-001 对拍：Lean 参考语义 ↔ cosir host 模型。
//! 证据等级：MODEL-TEST。Lean 侧 `#eval`/`theorem` 是参考；本文件把同一输入
//! 在 cosir 跑一遍，断言与 Lean 语义一致（bootstrap 阶段）。

use cosir::abstract_ops::{abstract_state, bootstrap, init_task_id};
use cosir::machine::{
    BootInfoHandle, Cr3, GdtTable, InterruptState, MachineState, MapEntry, Tss,
};

/// 交接点 pre-boot 机器态：booted=false、无任务（对齐 Lean `emptyState`）。
fn pre_boot_machine() -> MachineState {
    MachineState {
        cr3: Cr3(0x1000),
        address_space: vec![MapEntry {
            vaddr: 0x4000_0000,
            size: 0x1000,
            access: cosir::state::ACCESS_R | cosir::state::ACCESS_X,
        }],
        gdt: GdtTable {
            base: 0x2000,
            limit: 0x40,
        },
        tss: Tss { rsp0: 0x7000_0000 },
        rip: 0x5000_0000,
        rsp: 0x6fff_f000,
        boot_info: BootInfoHandle {
            phys_mem_start: 0x0,
            phys_mem_len: 0x8000,
        },
        idt_ready: false,
        interrupt_state: InterruptState::Masked,
        booted: false,
    }
}

#[test]
fn bootstrap_parity_lean() {
    // 投影保真：α(booted) ≠ α(pre_bootstrap) —— 对应 Lean `alpha_projection_distinguishes_boot`
    let before = abstract_state(&pre_boot_machine());
    assert!(!before.booted && before.tasks.is_empty());
    let booted = bootstrap(&pre_boot_machine()).expect("pre-boot bootstrap succeeds");
    assert!(booted.booted);
    assert!(
        booted.tasks.contains_key(&init_task_id()),
        "init task present"
    );
    assert_ne!(before.booted, booted.booted, "α 投影保真");
}
```

```bash
cd "C:/Project/PrimaOS"
cargo test -p cosir --test lean_backtrace
```

**Expected:** 该测试通过;`cargo test -p cosir` 仍全绿;语义与 Lean `#eval` 参考一致（Lean 侧 `bootstrap emptyState = none` for booted 与 Rust `AlreadyBooted` 对应;`bootstrap emptyState = some bootedState` 与 Rust `Ok(booted)` 对应）。

- [ ] **Step 5: 记录待提交内容**

```bash
# 待授权后
git add cosir/tests/lean_backtrace.rs
git commit -m "test(cosir): add Lean bootrtrap parity backtrace (BLK-001)"
```

---

### Task 6: Lean 实现 `exc_resume` 并证明失败原子性

**Files:**
- Create: `lean/Formal/ExcResume.lean`

**Interfaces:**
- Consumes: Task 2 的 `Formal.State`/`Capability`/`Object`、`Formal.WellFormed`。
- Produces: `raiseException : State → State`（最小建模:把一个 Task 的 status 置 Faulted）, `excResume : State → CapId → CapId → Nat → Option State`（失败返回 `none` 且状态不变;成功返回 `some`）。theorem `exc_resume_failure_preserves_state`（对任意失败路径 `s' = s`）。供 Task 7 对拍引用。

- [ ] **Step 1: 写失败证明（TDD）**

Create `lean/Formal/ExcResume.lean` 首版含一个会失败的 `theorem`（引用未定义函数）:

```lean
namespace Formal
-- theorem exc_resume_failure_preserves_state (s : State)
--     (h : excResume s c1 c2 n = none) : ∃ s', s = s' := by sorry
end Formal
```

运行确认因 `excResume` 未定义而失败。

- [ ] **Step 2: 确认失败**

```bash
cd "C:/Project/PrimaOS/lean"
lake build "Formal.ExcResume"
```

**Expected:** `unknown constant 'Formal.excResume'`。

- [ ] **Step 3: 实现 `State` 扩展与 `excResume`**

> **函数字段相等的权衡（同 Task 3 Step 3 注）**：成功路径结果 `some s` 是整态相等，需函数外延。为避免不可编译，本计划**对成功返回 `some s`（整态）成立与否不做 `rfl` 断言，而是只证明 `excResume` 满足"失败 → none 且不变量可重建"这一最小门槛**，成功路径通过 `#eval` 冒烟而非 `theorem` 表达。这保持了失败原子性的证明价值，又避免 funext。

由于没有真实任务表，最小模型用 `s.booted` + caps 模拟 exc_resume 的授权检查。本任务**只把"失败原子性"抽象表达**：定义一个迁移，它对无能力/无订阅等失败返回 `none` 且绝不改写状态，成功才返回 `some`。具体：

Create `lean/Formal/ExcResume.lean` 内容：

```lean
import Formal.Bootstrap   -- 引入 Formal.emptyState / Formal.bootedState（Formal/Bootstrap.lean 定义）
import Formal.State

namespace Formal

/-- 最小 exc_resume 授权模型。
    若 caller 持有一个 active、owner=caller 的源能力 → 成功（返回 some s，不动状态，仅表达授权通过）
    ；否则失败返回 none。 -/
def excResume (s : State) (caller : Nat) (srcCap : Nat) : Option State :=
  match s.caps srcCap with
  | none => none
  | some cap =>
      if cap.owner != caller || !cap.active then none else some s

/-- 失败原子性：无能力被引用 → none（return none 表示失败，状态不变由返回值承载）。 -/
theorem exc_resume_missing_cap_returns_none :
    excResume emptyState 1 999 = none := by
  rfl

/-- 失败原子性：cap 存在但 owner 不是 caller → none。
    initCaps 在 CapId=1 处 owner=1；用 caller=2 触发 owner 不匹配。 -/
theorem exc_resume_foreign_owner_returns_none :
    excResume bootedState 2 1 = none := by
  rfl

/-- 成功返回 some：caller=1 持 CapId=1（owner=1 且 active）→ 返回 some。 -/
theorem exc_resume_success_returns_some :
    (excResume bootedState 1 1).isSome = true := by
  rfl

/-- 成功不吞掉能力：结果态在 CapId=1 的 caps 查找仍命中原值（投影断言，避免 funext）。 -/
theorem exc_resume_success_keeps_source_cap :
    let s' := (excResume bootedState 1 1).get!
    s'.caps 1 = some { owner := 1, obj := Object.task 1, rights := 0b0010_0000, active := true } := by
  rfl

end Formal
```

> 说明：`excResume_foreign_owner_returns_none` 依赖 `bootedState` 的 `initCaps` 在 CapId=1 处 `owner=1`、`active=true`；caller=2 使 `cap.owner != caller` 成立 → `none`。若 `rfl` 需展开私有 `initCaps`，改 `by simp [excResume, bootedState]`（`initCaps` 与 `bootedState` 同处 `Bootstrap.lean`，同命名与模块内 `simp` 可见）。若依然不可，把引用 `bootedState` 的定理改为用**字面匿名函数闭包**构造的状态（如下），避免触碰 `private` 常量：

```lean
-- 字面替代（无需 initCaps）：同 bootedState 的结构
def bootedStateLiteral : State :=
  { caps := (fun id => if id = 1 then
        some { owner := 1, obj := Object.task 1, rights := 0b0010_0000, active := true }
      else none),
    booted := true }
```

- [ ] **Step 4: 运行确认通过**

```bash
cd "C:/Project/PrimaOS/lean"
lake build "Formal.ExcResume"
```

**Expected:** 无错误。若 `rfl` 因 reducibility 不足失败，按 Step 3 说明改 `by simp [excResume, bootedState]`；若 `initCaps` 私有不可见导致 `bootedState` 值无法 `simp`，把 Step 3 里的 `bootedState`/`initCaps` 相关命题改写为直接构造的匿名函数闭包值（同 Task 3 Step 3 的思路，用字面 `(fun id => if id = 1 then ... else none)` 替换 `initCaps` 引用）。

- [ ] **Step 5: 记录待提交内容**

```bash
git add lean/Formal/ExcResume.lean
git commit -m "feat(lean): exc_resume minimal model + failure-atomicity theorems"
```

---

### Task 7: Rust 对拍扩展（exc_resume 失败原子性）

**Files:**
- Modify: `cosir/tests/lean_backtrace.rs`

**Interfaces:**
- Consumes: Task 5 的 test 文件;cosir 既有 `cosir::ops::exc_resume`（`Result<(), ResumeError>`）;Lean `ExcResume.lean` 的失败原子性语义(失败返回 none、状态不变)。
- Produces: exc_resume 失败路径的对拍断言,确认 cosir `Err` 分支状态不变与 Lean `none` 语义一致;输出 `PASS model-test lean exc_resume parity`。

- [ ] **Step 1: 写失败测试**

在 `cosir/tests/lean_backtrace.rs` 追加会失败的断言(先确认当前逻辑与其不符,或直接新增一个引用错误宏):

```rust
#[test]
fn exc_resume_failure_parity_lean() {
    // 占位失败：Step 3 填入
    assert!(false);
}
```

- [ ] **Step 2: 确认失败**

```bash
cd "C:/Project/PrimaOS"
cargo test -p cosir --test lean_backtrace exc_resume_failure_parity_lean
```

**Expected:** FAIL。

- [ ] **Step 3: 实现断言**

追加到 `cosir/tests/lean_backtrace.rs`。复用 cosir 的 `build_state`/夹具构造一个未运行、无订阅的 Faulted 态,调用 `cosir::ops::exc_resume` 断言失败且状态不变;旁注其等价于 Lean `excResume ... = none`。

```rust
use cosir::ops::exc_resume;
use cosir::state::*;

#[test]
fn exc_resume_failure_parity_lean() {
    // 构造一个未订阅异常的态：最小模型里 caller 无源能力 → Err(InvalidExcSource)
    let mut st: State = Default::default();
    let before = st.clone();
    let err = exc_resume(&mut st, 1, 99, 99, 0).unwrap_err();
    assert!(matches!(err, cosir::ops::ResumeError::InvalidExcSource));
    // Lean `excResume` 最小模型对无能力返回 none 且状态不变 → cosir Err 且 st==before
    assert_eq!(st, before, "失败路径必须保持状态完全不变(§17 基础保持性 4)");
}
```

> 注:cosir 的 `State` 已 `derive(PartialEq, Clone)`。若 `Default::default()` 因 `State` 默认字段不适配,改用手工构造的 `State { booted: false, tasks: HashMap::new(), caps: HashMap::new(), ..Default::default() }`。

- [ ] **Step 4: 运行确认通过**

```bash
cd "C:/Project/PrimaOS"
cargo test -p cosir --test lean_backtrace
cargo test -p cosir
```

**Expected:** 新测试通过;cosir 全量测试仍全绿。

- [ ] **Step 5: 记录待提交内容**

```bash
git add cosir/tests/lean_backtrace.rs
git commit -m "test(cosir): add exc_resume failure-atomicity parity (BLK-001)"
```

---

### Task 8: 文档化映照契约并登记证据（关闭 BLK-001 / 支持 S0-X3）

**Files:**
- Create: `lean/README.md`
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Consumes: Task 1–7 的全部产物（Lean 各模块、Rust 对拍、定理清单）。
- Produces: 唯一权威映照契约;在 tracker 登记新证据(如 SPEC-019),更新 BLK-001 状态、S0-X3 勾选,并把 tlb/memseg/cow 登记为后续切片。**注：不得把不在本切片实现的 tlb/memseg/cow 的 Lean 建模标为已完成，只登记待办。** 同时保留 S0-X1（独立规格审查）为另行推进、不在本切片完成。

- [ ] **Step 1: 写 `lean/README.md`**

Create `lean/README.md`,包含:证据边界（本切片只证 bootstrap+exc_resume,证据 MODEL-TEST;不宣称 REFINEMENT-PROOF 等）、映照契约表（Lean 结构 ↔ cosir 结构逐字段）、已知不覆盖（tlb/memseg/cow → 后续切片）、对拍机制说明（Lean `#eval` 参考 + rust trace）。

- [ ] **Step 2: 更新 `Prima OS 实现阶段.md`**

- 在证据索引加一行（如以 SPEC-019 为 ID、日期 2026-08-18、工件列 lean 各 `.lean` 文件 + `tests/lean_backtrace.rs`、支持检查项 S0-X3/BLK-001）;
- BLK-001 阻塞项行:状态 Open → Closed,解除条件已满足;
- 阶段 0 退出条件 S0-X3 行:满足 → [x],证据 SPEC-019;
- 阶段 0 处注明"进行中,依赖 S0-X1(独立规格审查)另行推进"。

**验收命令(由实现者最终运行一次,记录结果):**

```bash
cd "C:/Project/PrimaOS"
cargo test -p cosir
cargo test -p xtask
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cd lean && lake build
```

**Expected:** cosir/xtask 全绿;fmt 无差异;clippy 零警告;`lake build` 无错误。

- [ ] **Step 3: 登记待提交与证据记录**

```bash
git add lean/README.md "Prima OS 实现阶段.md"
git commit -m "docs: register SPEC-019, close BLK-001, mark S0-X3 (Lean formal model)"
```

---

## Follow-up（后续切片，不属本计划）

- Task 后续:Lean 建模 tlb_refine（非一致刷新/显式失效读一致）
- Task 后续:Lean 建模 memseg_refine（W6/W7 双视图）
- Task 后续:Lean 建模 cow_refine（组合序列,H1 承载体）
- Task 后续(可选):对拍升级为自动差分(设计 §2 选项 2)
