# BLK-002 x86_64 细化草案实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 补齐阶段 0 交付项 S0-D4 / 阻塞项 BLK-002：在 COSIR 规范中定义 x86_64 `MachineState` 与抽象函数 `abstract`，并在 `cosir` 探针中用可执行代码验证 Bootstrap 的细化对应（静态不变量 + 动态机器步骤映射 + 幂等）。

**Architecture:** 分两部分。(1) 修改 `COSIR 原语规范.md`，在末部新增 §20「细化层：MachineState 与抽象函数」，定义 `MachineState` 数据结构、抽象函数 `abstract`、Bootstrap 细化对应表与细化不变量，更新实施文档的 BLK-002 状态。(2) 扩展现有 `cosir` crate：新增 `machine.rs`（MachineState 数据类型）与 `abstract.rs`（α 抽象函数 + bootstrap 迁移 + 静态/动态/幂等测试），α 复用 `cosir::state::State` 作为协域，与已验证的 exc_resume 探针语义一致。

**Tech Stack:** Rust nightly-2026-07-30、Rust 2024 edition、Cargo workspace（已有 `kernel`/`xtask`/`cosir`）。`cosir` 是 host 二进制，无新依赖。

## Global Constraints

- 权威设计为 `docs/superpowers/specs/2026-08-07-blk002-refinement-design.md`。
- 固定 Rust 工具链 `nightly-2026-07-30`；`cosir` 是 host 二进制，不依赖 `bootloader`/`bootloader_api`，不触碰 real IDT/trap。
- MachineState 起始于**交接点**（bootloader 把控制权交给 `kernel_main` 那一刻：long mode、分页开、CR3/GDT/TSS/栈有效、已收到 BootInfo）。
- **先 Bootstrap**：只实例化 Bootstrap 细化对应。MemorySegment 的 CR3/页表↔vspace 全量细化、task_create 入口细化（边界清单 #16）、异常/trap 递送（S1-D4）**不展开**，仅登记接口。
- `MachineState` 与 `abstract` 是**细化层概念**：不新增对 POSIR 可见的一等原语，不进 POSIR 抽象正文。
- 页表用简化 `MapEntry` 视图，**不实现真实多级 PML4 遍历**（S1-D4）。
- α 复用 `cosir::state::State` 作为协域，为表达 bootstrap 需给 `State` 增加 `booted: bool` 字段（POSIR 状态真实部分，默认 false）。
- 不关闭 BLK-001；S0 是否满足退出条件不虚报。BLK-002 从"无细化草案"改为"细化草案已定义"（不删除阻塞项条目，更新其问题表述与解除条件的完成标注）。
- 现有阶段 1 启动闭环与 `cargo xtask test` 不得被本计划破坏。
- 现有 `cosir` 的 exc_resume 12 个测试必须保持通过；本计划不修改既有 `ops.rs` 语义。
- `.obsidian/` 是现有未跟踪目录，不读取、不修改、不提交。
- 每个任务仅提交该任务列出的文件，不使用 `git add .` 或 `git add -A`。
- **注意**：`Prima OS 实现阶段.md` 当前有未提交改动（前一会话把 SPEC-012 与最近细化证据的测试数从 9 改为 12）。本计划 Task 6 会把该文件的全部未提交改动一次性登记提交，并在其中追加 SPEC-013 与 12 tests 的同步。不要分期提交该文件的碎片。

---

## File Structure

- Create: `docs/superpowers/plans/2026-08-07-blk002-refinement-slice.md` — 本计划文件（Task 7 提交）。
- Modify: `COSIR 原语规范.md` — 末部新增 §20 细化层章节；更新"最近规范修订"行（通过实施文档承接）。
- Create: `cosir/src/machine.rs` — MachineState 数据类型。
- Create: `cosir/src/abstract.rs` — α 抽象函数 + bootstrap 迁移 + 测试。
- Modify: `cosir/src/state.rs` — `State` 增加 `booted: bool` 字段。
- Modify: `cosir/src/main.rs` — 声明新模块；在验收线束追加 PASS 输出。
- Modify: `Prima OS 实现阶段.md` — 新增 SPEC-013 证据、更新当前首要缺口、更新 BLK-002 状态、同步 12 tests；一并提交先前未提交的 9→12 漂移修复。

---

### Task 1: 固定当前验收基线，确认 workspace 可扩展

**Files:**
- 无源码改动（仅验证）。

**Interfaces:**
- Consumes: 现有 workspace（`kernel`/`xtask`/`cosir`）。
- Produces: 证明当前 `cargo xtask test` 与 `cargo test -p cosir` 仍通过，为后续不破坏闭环提供基线。

- [ ] **Step 1: 运行现有验收，确认基线**

Run：

```bash
cargo test -p cosir
cargo xtask test
```

Expected：`cargo test -p cosir` 输出 `test result: ok. 12 passed; 0 failed`；`cargo xtask test` 输出尾部：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

不得继续本计划的其他任务，除非 `cargo xtask test` 退出码为 0。

- [ ] **Step 2: 记录基线快照**

确认 `git status --short`。预期：`Prima OS 实现阶段.md`（9→12 漂移修复，未提交）、`docs/superpowers/specs/2026-08-07-blk002-refinement-design.md` 已提交、`.obsidian/` 未跟踪。不创建任何 PASS 证据外的写入。

---

### Task 2: 在 COSIR 规范末部新增 §20 细化层章节

**Files:**
- Modify: `COSIR 原语规范.md`

**Interfaces:**
- Consumes: 设计文档 §2（MachineState）、§3（α）、§4（Bootstrap 对应）。
- Produces: 规范中的 `MachineState`/`abstract`/Bootstrap 细化对应定义（供 Task 6 登记证据时引用）、BLK-002 对应细化状态。

- [ ] **Step 1: 在 §19 之后追加 §20 章节**

定位 `COSIR 原语规范.md` 末尾 §19 结束后文件结尾。在文件末尾追加：

```markdown
## 20. 细化层：MachineState 与抽象函数

本章属于 COSIR 细化层，不新增对 POSIR 可见的一等原语。它把 x86_64 机制层与 POSIR 抽象层的对应关系显式化（见 [[Prima OS]] §13.2），为阶段 1 真实内核实现（物理内存、页表/GDT/IDT/trap）提供细化蓝本。本章当前只实例化 **Bootstrap** 的细化对应；其它原语（MemorySegment 页表↔vspace、task_create 入口细化、异常/trap 递送）仅登记接口，留待后续细化切片展开。

### 20.1 MachineState

`MachineState` 描述 bootloader 把控制权交给内核那一刻的 x86_64 可观察机器状态（交接点：long mode、分页开、CR3/GDT/TSS/栈有效、已收到 `BootInfo`）。它是可执行状态模型探针（`cosir`）中细化域的起点。

```text
MachineState {
    cr3:             Cr3              // 根页表物理地址
    address_space:   List<MapEntry>   // 简化页表视图；(vaddr, size, access{R,W,X})
    gdt:             GdtTable         // 基址/限长
    tss:             Tss              // 处理器状态段（含 RSP0 中断栈）
    rip:             VirtAddr         // kernel 入口
    rsp:             VirtAddr         // 已设栈顶
    boot_info:       BootInfoHandle   // 交接的 BootInfo 抽象（内存区域）
    idt_ready:       Bool             // IDT 是否已装载（交接点通常尚未）
    interrupt_state: InterruptState   // masked / unmasked
    booted:          Bool             // 细化层可见：bootstrap 是否已执行（W14 唯一性）
}
```

- 页表用简化 `MapEntry` 视图（`(vaddr, size, access)`），**不建模真实多级 PML4 遍历**——真实遍历属于阶段 1 页表/trap 框架工作（S1-D4）。
- 不建模 GPR 全集；只建模对 Bootstrap 有意义的状态（rip/rsp/表/地址空间）。真实寄存器保存/恢复属于异常帧细化（§18.0 边界清单 #1）。
- `MapEntry.access` 复用本规范的 R/W/X 访问位域并在探针中沿用 `cosir::state` 常量。

### 20.2 抽象函数 abstract

```text
abstract : MachineState → POSIRState
```

抽象函数把机器态投影为 POSIR 抽象状态，遵循两段投影：

1. 交接点且 `booted = false` → 投影为 POSIR `EmptyState`（无任务、无能力、未 booted）；
2. Bootstrap 迁移后（`booted = true`）→ 投影为含 init Task 的 booted 态。

抽象函数只做**投影**，不做安全判定：机器内部结构（页表布局细节、GDT 具体槽位等）不进入抽象态；安全判定仍由 POSIR 不变量与后续真实实现保证。

### 20.3 Bootstrap 细化对应

按 §14 的自定义实例化（唯一的无 Task Actor 铸造初始 Authority 的迁移）：

| POSIR 抽象（§14） | x86_64 细化 |
|---|---|
| `EmptyState` | 交接点机器态：`kernel_main` 接管、无用户 Task、无能力表 |
| `bootstrap` 迁移 | 内核固化 bootstrap 路径：铸造初始 Authority |
| 初始 Root Capability 铸造至 init Task | 交接点建立 init 上下文：初始栈、入口、内存视图 |
| `AlreadyBooted` 幂等保护 | 细化层保持 `MachineState.booted`，重复 bootstrap 不重复铸造 |
| init 只获定义初始 Authority | 交接点后 init 的能力快照是 ObjectRef/CapRecord 的机器投影 |

细化层不变量（仅细化层可见，不新增 POSIR 原语）：

- **W14 唯一性细化**：`MachineState.booted` 与 POSIR `booted` 一一对应，交错不违背唯一性；
- **初始 Authority 单调**：α 投影后 init Task 的能力集恰好等于 §14 定义的初始 Authority，无多余对象（对 S1-X2 的 α 层承接）；
- **投影保真**：`α(booted) ≠ α(pre_bootstrap)`，即 bootstrap 确改变抽象态。

### 20.4 范围边界与证伪

本切片已定义 MachineState、抽象函数 α、Bootstrap 细化对应；**待证明**：α 对全部 POSIR 状态成立（后续切片补覆盖）、真实实现满足 α（S1 之后真实 kernel 对照）、形式化验证（BLK-001）。若实现中发现需在 POSIR 层新增机制才能完成 Bootstrap 细化，构成对可信机制层小型性（H3）/机制表达力（H1）的潜在反例，须按 §18.0 末尾规则记录。
```

- [ ] **Step 2: 校验 §20 追加成功**

Run：

```bash
python -c 'from pathlib import Path; s=Path("COSIR 原语规范.md").read_text(encoding="utf-8"); assert "## 20. 细化层：MachineState 与抽象函数" in s; assert "abstract : MachineState → POSIRState" in s; assert "MachineState.booted" in s; print("PASS spec §20 added")'
```

Expected：`PASS spec §20 added`。

- [ ] **Step 3: 提交 §20**

```bash
git add -- "COSIR 原语规范.md"
git commit -m "$(cat <<'EOF'
docs: define MachineState and abstract function refinement

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: cosir 新增机器态类型 machine.rs 与 State.booted 字段

**Files:**
- Create: `cosir/src/machine.rs`
- Modify: `cosir/src/state.rs`
- Modify: `cosir/src/main.rs`

**Interfaces:**
- Consumes: `cosir::state` 的 `VirtAddr` 与 `ACCESS_R`/`ACCESS_W`/`ACCESS_X`。
- Produces:
  - `machine::{Cr3, MapEntry, GdtTable, Tss, BootInfoHandle, InterruptState, MachineState}`。
  - `state::State` 增加 `booted: bool` 字段（Task 4 的 abstract.rs 读写）。
  - `main.rs` 声明 `mod machine;`。

- [ ] **Step 1: 给 State 增加 booted 字段（先写会导致编译失败的引用测试）**

TDD：先在 `main.rs` 新增一个断言模块？更直接：本任务先实现机器态类型与字段，测试由 Task 4 承载。因此本 Task 先做最小可编译实现，保证不破坏既有 12 测试。

Replace `cosir/src/state.rs` 的 `State` 定义，追加 `booted` 字段：

```rust
#[derive(Default)]
pub struct State {
    pub tasks: HashMap<TaskId, Task>,
    pub exc_sources: HashMap<ExcSourceId, ExceptionSource>,
    pub pending_exc: HashMap<TaskId, PendingException>,
    pub segments: HashMap<SegId, SegRange>,
    /// POSIR 状态的 bootstrap 标志（真实部分）；默认 false
    pub booted: bool,
}
```

（保持现有其余代码不变——`build_state`、既有 ops、既有 12 测试都在使用 `State::default()` 与命名字段初始化，新增字段不破坏它们。）

- [ ] **Step 2: 书写 machine.rs 类型**

Create `cosir/src/machine.rs`：

```rust
use crate::state::VirtAddr;

/// 根页表物理地址（交接点已建分页）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cr3(pub u64);

/// 简化页表视图条目：(vaddr, size, access)；access 复用 state 的 R/W/X 位域常量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub vaddr: VirtAddr,
    pub size: u64,
    pub access: u8,
}

/// GDT 表：简化只建模基址/限长。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GdtTable {
    pub base: u64,
    pub limit: u16,
}

/// 处理器状态段：简化仅 RSP0 中断栈。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tss {
    pub rsp0: VirtAddr,
}

/// 交接的 BootInfo 抽象：物理内存范围。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootInfoHandle {
    pub phys_mem_start: u64,
    pub phys_mem_len: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptState {
    Masked,
    Unmasked,
}

/// 交接点的 x86_64 机器状态（细化域起点）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineState {
    pub cr3: Cr3,
    pub address_space: Vec<MapEntry>,
    pub gdt: GdtTable,
    pub tss: Tss,
    pub rip: VirtAddr,
    pub rsp: VirtAddr,
    pub boot_info: BootInfoHandle,
    pub idt_ready: bool,
    pub interrupt_state: InterruptState,
    /// 细化层可见：bootstrap 是否已执行（W14 唯一性）。
    pub booted: bool,
}
```

- [ ] **Step 3: 在 main.rs 声明 machine 模块**

Modify `cosir/src/main.rs` 顶部的模块声明，改为（只加 `machine`；`abstract_ops` 到 Task 4 才声明，避免引用尚不存在的文件）：

```rust
mod machine;
mod ops;
mod state;
```

- [ ] **Step 4: 编译校验**

Run：

```bash
cargo build -p cosir
```

Expected：编译通过（可能有 about Task 4 才引用的 dead_code warning，可先在 Task 4 接入后清理；若 warning 出现，确认是 `abstract_ops` 尚未定义导致的 module 声明错误，则先临时注释掉该行、完成 Task 4 后再恢复。若报 `can't find crate for abstract_ops`，在 Task 4 创建该文件前先把 Step 3 的声明改为仅 `mod machine;`，并在此处同步）。

若因 `abstract_ops` 尚不存在而编译失败，按此修正 Step 3：将 main.rs 模块声明改为 `mod machine; mod ops; mod state;`，待 Task 4 真正创建 `abstract_ops.rs` 后再在声明中加入 `mod abstract_ops;`。

- [ ] **Step 5: 运行既有 12 测试确认未破坏**

Run：

```bash
cargo test -p cosir
```

Expected：`test result: ok. 12 passed; 0 failed`。

- [ ] **Step 6: 提交 machine 类型与 State.booted**

```bash
git add -- cosir/src/machine.rs cosir/src/state.rs cosir/src/main.rs
git commit -m "$(cat <<'EOF'
feat: add x86_64 machine state types and State.booted

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: 实现 α 抽象函数与 bootstrap 迁移（TDD）

**Files:**
- Create: `cosir/src/abstract_ops.rs`
- Modify: `cosir/src/main.rs`（若上 Task 未声明 `mod abstract_ops;`，此处加）

**Interfaces:**
- Consumes: `machine::MachineState`；`state::{State, Task, TaskStatus, TaskId}`。
- Produces:
  - `abstract_ops::abstract_state(&MachineState) -> State`（α 抽象函数）。
  - `abstract_ops::bootstrap(&MachineState) -> Result<State, BootstrapError>`（bootstrap 迁移）。
  - `abstract_ops::BootstrapError`（`AlreadyBooted`）。
  - 内部辅助 `abstract_ops::init_task_id() -> TaskId`（恒定 1）。

本 Task 采用 TDD：**先写失败测试**（Step 1-2），**后写最小实现**（Step 3），再验证（Step 4）。

- [ ] **Step 1: 写失败测试（测试引用尚不存在的函数 → 编译失败）**

Create `cosir/src/abstract_ops.rs`，仅含测试（尚不定义 `BootstrapError`/`init_task_id`/`abstract_state`/`bootstrap`）：

```rust
#[cfg(test)]
mod tests {
    use super::{abstract_state, bootstrap, init_task_id, BootstrapError};
    use crate::machine::{
        BootInfoHandle, Cr3, GdtTable, InterruptState, MachineState, MapEntry, Tss,
    };
    use crate::state::{TaskStatus, ACCESS_R, ACCESS_X};

    fn pre_boot_machine() -> MachineState {
        MachineState {
            cr3: Cr3(0x1000),
            address_space: vec![
                MapEntry { vaddr: 0x4000_0000, size: 0x1000, access: ACCESS_R | ACCESS_X },
            ],
            gdt: GdtTable { base: 0x2000, limit: 0x40 },
            tss: Tss { rsp0: 0x7000_0000 },
            rip: 0x5000_0000,
            rsp: 0x6fff_f000,
            boot_info: BootInfoHandle { phys_mem_start: 0x0, phys_mem_len: 0x8000 },
            idt_ready: false,
            interrupt_state: InterruptState::Masked,
            booted: false,
        }
    }

    #[test]
    fn alpha_pre_bootstrap_projects_to_empty_state() {
        let m = pre_boot_machine();
        let s = abstract_state(&m);
        assert!(!s.booted);
        assert!(s.tasks.is_empty());
    }

    #[test]
    fn bootstrap_produces_booted_state_with_init_task() {
        let m = pre_boot_machine();
        let s = bootstrap(&m).unwrap();
        assert!(s.booted);
        let init = s.tasks.get(&init_task_id()).expect("init task present");
        assert_eq!(init.status, TaskStatus::Ready);
        assert_eq!(s.tasks.len(), 1, "no extra objects");
    }

    #[test]
    fn bootstrap_idempotent_returns_already_booted() {
        let m = pre_boot_machine();
        let mut already = m.clone();
        already.booted = true;
        let err = bootstrap(&already).unwrap_err();
        assert_eq!(err, BootstrapError::AlreadyBooted);
    }

    #[test]
    fn alpha_projection_preserves_booted_flag() {
        let mut m = pre_boot_machine();
        m.booted = true;
        let s = abstract_state(&m);
        assert!(s.booted);
        assert!(!s.tasks.is_empty());
    }

    #[test]
    fn dynamic_machine_steps_map_to_bootstrap_migration() {
        // 交接点（pre-boot）：内部配置步骤（GDT/IDT 装载）不改变抽象态
        let m = pre_boot_machine();
        let before = abstract_state(&m);
        let mut idle = m.clone();
        idle.gdt = GdtTable { base: 0x3000, limit: 0x40 }; // 内部步骤：改 GDT 基址
        idle.idt_ready = true; // 内部步骤：装载 IDT
        let internal = abstract_state(&idle);
        // 内部步骤不改变抽象态（仍是 EmptyState）
        assert_eq!(before.booted, internal.booted);
        assert!(internal.tasks.is_empty());
        // 可观察步骤：bootstrap → POSIR 迁移（EmptyState → booted init 态）
        let after = bootstrap(&idle).unwrap();
        assert!(after.booted && !after.tasks.is_empty());
        assert_ne!(internal.booted, after.booted);
    }
}
```

- [ ] **Step 2: 运行测试确认编译失败（TDD 红灯）**

Run：

```bash
cargo test -p cosir abstract_ops
```

Expected：编译失败，报 `abstract_state` / `bootstrap` / `init_task_id` / `BootstrapError` 未定义。这是期望的"先失败"。

- [ ] **Step 3: 写最小实现使测试通过（TDD 绿灯）**

Replace `cosir/src/abstract_ops.rs` 为：

```rust
use crate::machine::MachineState;
use crate::state::{State, Task, TaskId, TaskStatus};

/// bootstrap 迁移的错误类型（最小集）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BootstrapError {
    /// machine.booted 已为 true，重复 bootstrap。
    AlreadyBooted,
}

/// init Task 的恒定量 ID。
pub fn init_task_id() -> TaskId {
    1
}

/// 抽象函数 α：MachineState → State。
/// 两段投影：booted=false → EmptyState（booted=false 的空 State）；
///             booted=true  → 含 init Task 的 booted State。
pub fn abstract_state(m: &MachineState) -> State {
    let mut s = State::default();
    s.booted = m.booted;
    if m.booted {
        let init = Task {
            id: init_task_id(),
            status: TaskStatus::Ready,
            vspace: Vec::new(),
        };
        s.tasks.insert(init_task_id(), init);
    }
    s
}

/// bootstrap 迁移：空态 → 含 init Task 的 booted 态。
/// 仅当 machine.booted == false 成功；否则返回 AlreadyBooted 且状态不变。
pub fn bootstrap(m: &MachineState) -> Result<State, BootstrapError> {
    if m.booted {
        return Err(BootstrapError::AlreadyBooted);
    }
    let booted_machine = MachineState { booted: true, ..m.clone() };
    Ok(abstract_state(&booted_machine))
}
```

（保持 Step 1 的 `tests` 模块在其后，或复用该文件中的测试不变。）

- [ ] **Step 4: 运行测试确认通过**

Run：

```bash
cargo test -p cosir
```

Expected：既有的 12 个 exc_resume 测试 + 5 个新 abstract_ops 测试全部通过。

- [ ] **Step 5: 在 main.rs 验收线束追加 PASS 输出**

Modify `cosir/src/main.rs`：确保 `mod abstract_ops;` 已声明（Task 3 若因顺序问题未声明，在此补上）。在 `main()` 末尾追加：

```rust
mod abstract_ops;
mod machine;
mod ops;
mod state;

use abstract_ops::bootstrap;
use machine::{... /* 不需全部导入，仅在需要时 */};

fn main() {
    // ... 既有 exc_resume 线束保持不变 ...

    // BLK-002 细化：bootstrap 的 α 投影冒烟
    let m = build_boot_machine();
    let st_after = bootstrap(&m).expect("bootstrap should succeed");
    assert!(st_after.booted);
    assert!(st_after.tasks.contains_key(&abstract_ops::init_task_id()));
    println!("PASS cosir bootstrap abstract");
    println!("PASS cosir bootstrap alpha projection");
}
```

其中 `build_boot_machine()` 在 `main.rs` 定义如下：

```rust
fn build_boot_machine() -> machine::MachineState {
    machine::MachineState {
        cr3: machine::Cr3(0x1000),
        address_space: vec![machine::MapEntry {
            vaddr: 0x4000_0000,
            size: 0x1000,
            access: state::ACCESS_R | state::ACCESS_X,
        }],
        gdt: machine::GdtTable { base: 0x2000, limit: 0x40 },
        tss: machine::Tss { rsp0: 0x7000_0000 },
        rip: 0x5000_0000,
        rsp: 0x6fff_f000,
        boot_info: machine::BootInfoHandle { phys_mem_start: 0x0, phys_mem_len: 0x8000 },
        idt_ready: false,
        interrupt_state: machine::InterruptState::Masked,
        booted: false,
    }
}
```

- [ ] **Step 6: 运行线束与 lint**

Run：

```bash
cargo run -p cosir --bin cosir-state
cargo clippy -p cosir -- -D warnings
```

Expected：线束输出包含 `PASS cosir bootstrap abstract` 与 `PASS cosir bootstrap alpha projection`；clippy 零警告（清理任何 unused import/struct）。

- [ ] **Step 7: 提交 abstract 动画函数与测试**

```bash
git add -- cosir/src/abstract_ops.rs cosir/src/main.rs
git commit -m "$(cat <<'EOF'
feat: implement abstract fn and bootstrap refinement

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: 全 workspace 静态校验，确认不破坏启动闭环

**Files:**
- 无源码改动（仅验证）。

**Interfaces:**
- Consumes: Task 4 的 `abstract_ops`；既有 `kernel`/`xtask`/`cosir`。
- Produces: 全 workspace 干净编译与既有验收基线证据。

- [ ] **Step 1: 全 workspace 静态与单元校验**

Run：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
```

Expected：所有命令退出码 0；`cargo test --workspace` 中 cosir 的 12 + 5 = 17 个测试与 xtask 既有的 host 测试全部通过。此命令**不**运行 QEMU（QEMU 验收走 `cargo xtask test`）。

- [ ] **Step 2: 重新运行既有启动验收**

Run：

```bash
cargo xtask test
```

Expected tail：

```text
PASS build kernel
PASS image bios
PASS image uefi
PASS qemu bios run=1
PASS qemu bios run=2
PASS stage1 boot slice
```

只有这条真实通过，才符合"不得破坏启动闭环"约束。

- [ ] **Step 3: 校验差异**

Run：

```bash
git diff --check && git status --short
```

Expected：无 diff error；工作区仅 `Prima OS 实现阶段.md`（未提交，含 9→12 修复与即将的 SPEC-013）与 `.obsidian/`（未跟踪）。无意外源码改动。

---

### Task 6: 登记 BLK-002 细化产出证据，同步实现阶段文档

**Files:**
- Modify: `Prima OS 实现阶段.md`

**Interfaces:**
- Consumes: Task 2/4/5 的真实证据（`cargo test -p cosir`、`cargo xtask test`、规范 §20）。
- Produces: SPEC-013 证据条目；当前首要缺口更新；BLK-002 状态更新；12 tests 漂移修复一并入账。

- [ ] **Step 1: 运行最终静态与单元校验**

Run：

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-deps -- -D warnings
cargo test --workspace
cargo xtask test
```

Expected：全部退出码 0；`cargo test --workspace` 含 17 个 cosir 测试；`cargo xtask test` 六条 PASS。

仅在此命令全部通过后才执行以下证据编辑。

- [ ] **Step 2: 更新文档头部与当前快照**

在 `Prima OS 实现阶段.md` 中：

- 将文档头 `> 更新日期：` 值更新为 `2026-08-07`（保持 `> 更新日期：2026-08-07`；若已是则跳过）；
- 在"当前快照"表新增一行（追加到"最近细化证据"行之后）：

```markdown
| BLK-002 细化（本切片） | 2026-08-07 — 定义 x86_64 MachineState（交接点起）、抽象函数 α 与 Bootstrap 细化对应；cosir 新增 machine/abstract_ops 模块；规范新增 §20；`cargo test -p cosir` 17 tests 通过；`cargo xtask test` 六条 PASS |
```

- 将"| 当前首要缺口 |"行更新为：

```markdown
| 当前首要缺口 | 形式化工具中的可执行状态模型（BLK-001） |
```

  （移除"x86_64 机器状态到 POSIR 状态的细化草案（BLK-002）"表述，因为该缺口已由本切片给出草案。）

- [ ] **Step 3: 更新证据索引**

在证据索引表末尾（`SPEC-012` 行之后）增加：

```markdown
| SPEC-013 | 细化规范与可执行模型 | `COSIR 原语规范.md` §20、`cosir/src/machine.rs`、`cosir/src/abstract_ops.rs` | 2026-08-07 | 定义 x86_64 MachineState、抽象函数 α 与 Bootstrap 细化对应；`cargo test -p cosir` 17 tests 通过；`cargo xtask test` 六条 PASS | S0-D4 |
```

- [ ] **Step 4: 更新 BLK-002 阻塞项行**

在"当前阻塞项"表中将 `| BLK-002 |` 行更新为已给出草案（问题表述改为已定义、状态标注为已解除条件中的草案部分完成，但**条目不删除**，因完整细化对应仍需真实 kernel 对照与 S1 覆盖）：

```markdown
| BLK-002 | 0 | x86_64 → POSIR 细化草案已定义（MachineState、抽象函数、Bootstrap 对应），待真实 kernel 对照与全原语覆盖 | 定义 MachineState、抽象函数和至少 Bootstrap 的对应关系 | 草案已成 |
```

（状态列值可保持 `Open` 或标 `草案已成`，取决于仓库现有列宽与语义；以不删除条目、明确草案已给定为准。）

- [ ] **Step 5: 校验文档边界未虚报，并同步 12 tests**

确认 `SPEC-013` 出现、`BLK-001` 保持 Open、阶段 0 不标"已满足退出条件"：

Run：

```bash
python -c 'from pathlib import Path; s=Path("Prima OS 实现阶段.md").read_text(encoding="utf-8"); assert "SPEC-013" in s; assert "12 tests" in s or "17 tests" in s; assert "**状态：已满足退出条件**" not in s; print("PASS evidence boundaries")'
```

Expected：`PASS evidence boundaries`。同时确认先前 9→12 修复在本文中一致（`grep "9 tests"` 应为 0 命中）。

- [ ] **Step 6: 提交证据登记（含先前 9→12 漂移修复）**

```bash
git diff --check
git add -- "Prima OS 实现阶段.md"
git commit -m "$(cat <<'EOF'
docs: record BLK-002 refinement draft evidence; sync cosir test count

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

（此提交同时包含已存在的 9→12 漂移修复与本切片的 SPEC-013 登记，确保不产生半提交状态。）

- [ ] **Step 7: 最终验证**

Run：

```bash
git status --short
cargo xtask test
```

Expected：Git 只显示既有未跟踪 `.obsidian/`；`cargo xtask test` 六条 PASS。不得把 `.obsidian/` 加入任何提交。

---

### Task 7: 提交本计划文件

**Files:**
- Add: `docs/superpowers/plans/2026-08-07-blk002-refinement-slice.md`

**Interfaces:**
- Consumes: 上述各 Task 的提交结果。
- Produces: 计划文件入库，供复盘引用。

- [ ] **Step 1: 校验计划文件未被遗漏**

Run：

```bash
git status --short
```

Expected：除 `.obsidian/` 外无未跟踪源码文件；本计划文件应未入库。

- [ ] **Step 2: 提交计划文件**

```bash
git add -- "docs/superpowers/plans/2026-08-07-blk002-refinement-slice.md"
git commit -m "$(cat <<'EOF'
docs: BLK-002 refinement slice implementation plan

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 3: 最终确认**

Run：

```bash
git log --oneline -6
cargo xtask test
```

Expected：最近若干提交含 §20 规范、machine 类型、abstract_ops、证据登记、计划文件；`cargo xtask test` 六条 PASS。

---

## Plan Self-Review

- **Spec coverage:** 设计 §1 目标/产出 → Task 2/3/4/6；§2 MachineState → Task 3；§3 抽象函数 α（复用 State、两段投影）→ Task 4 `abstract_state`；§4 Bootstrap 细化对应表与不变量 → Task 2 §20.3 + Task 4 测试；§5 错误与测试三件套（静态/动态/幂等）→ Task 4 tests；§6 范围边界/证伪 → Task 2 §20.4 + Task 6 BLK-002 留 Open。
- **Placeholder scan:** 所有代码/命令/期望均给具体内容；无 scaffold。Task 4 采用严格 TDD（Step 1 仅测试→Step 2 红灯→Step 3 最小实现→Step 4 绿灯），`main.rs` 线束给出具体 `build_boot_machine` 完整实现。
- **Type consistency:** `MachineState` 字段在 machine.rs、测试 `pre_boot_machine`、`main.rs::build_boot_machine` 三处一致（cr3/address_space/gdt/tss/rip/rsp/boot_info/idt_ready/interrupt_state/booted）。`BootstrapError::AlreadyBooted`、`init_task_id()`、`abstract_state(&MachineState)->State`、`bootstrap(&MachineState)->Result<State,BootstrapError>` 在 abstract_ops.rs 定义，被 Task 4 测试与 main.rs 一致引用。`State.booted` 在 state.rs 定义、abstract_ops 读写、既有 build_state（命名字段初始化）不破坏。
- **注意点已内建:** 先前同会话已把 `Prima OS 实现阶段.md` 的 SPEC-012 与最近细化证据测试数从 9 改为 12 并留待提交；本计划 Task 6 Step 6 把它们一次性并入证据登记提交，避免碎片提交。
