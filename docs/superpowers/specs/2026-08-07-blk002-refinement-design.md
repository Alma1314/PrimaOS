# BLK-002 细化草案：x86_64 MachineState → POSIR State

> 文档类型：阶段 0 细化层设计
>
> 关联规范：[[COSIR 原语规范]]（§14 Bootstrap、§18.0 边界清单）；[[Prima OS]]（§13.2 COSIR 细化验证）
>
> 状态：已获批准，待实现

## 1. 目标与范围

补齐阶段 0 交付项 **S0-D4 / 阻塞项 BLK-002**：定义 x86_64 机器状态到 POSIR 状态的细化关系，至少覆盖 Bootstrap 的对应关系。解除 BLK-002 的条件是"定义 `MachineState`、抽象函数和至少 Bootstrap 的对应关系"。

细化草图为后续真实内核工作（S1-D3 物理内存、S1-D4 页表/GDT/IDT/trap）提供抽象函数蓝本——它把 x86_64 机制层与 POSIR 抽象层的对应关系显式化，使后续实现有可审查的细化依据。

职责分离是总原则：

- **MachineState 与抽象函数是细化层概念**，不新增任何对 POSIR 可见的一等原语；
- **抽象函数 `α` 只做投影**，不做安全判定；安全判定仍由 POSIR 不变量与后续真实实现保证；
- **本切片只实例化 Bootstrap**；其它原语（MemorySegment / task_create 等）仅登记接口，留待后续切片展开。

产出：

1. 设计文档（本文，`docs/superpowers/specs/2026-08-07-blk002-refinement-design.md`）；
2. COSIR 规范补充：新增细化层章节，定义 `MachineState`、抽象函数 `abstract`、Bootstrap 细化对应，登记 BLK-002；
3. Rust host 探针（扩展现有 `cosir` crate）：`machine.rs`（MachineState）+ `abstract.rs`（α 实现与静态/动态测试）；
4. 实施进度文档：登记 SPEC-013 证据，更新 BLK-002 状态。

明确不做：

- 不实现真实 kernel 页表/trap/内存发现（S1-D3/D4/D5，本切片只提供细化蓝本）；
- 不做 Lean/Coq 形式化（仍在 BLK-001）；
- 不覆盖 Bootstrap 之外的其它原语（MemorySegment/task_create 等留后续切片）；
- 不新增对 POSIR 可见的一等原语；MachineState 与 α 不进 POSIR 抽象正文；
- 页表建模用简化视图（MapEntry），不实现真实多级 PML4 遍历（S1-D4）。

## 2. MachineState 数据结构

`MachineState` 从 **交接点** 建模——bootloader 已把控制权交给 `kernel_main` 那一刻（`bootloader_api::entry_point!` 语义：已 long mode、分页开、CR3/GDT/TSS/栈有效、收到 `BootInfo`）。这是 kernel 真实可见的可观察机器状态，探针对其忠实建模。

探针建模交接点 x86_64 机器上支撑 Bootstrap 细化的**最小**可见状态集。字段如下（`cosir/src/machine.rs`）：

```rust
// 交接点的 x86_64 机器状态（细化域起点）
pub struct MachineState {
    /// 根页表物理地址（交接时 bootloader 已建分页）
    pub cr3: Cr3,
    /// 简化页表视图：(vaddr, phys, size, flags)；真实多级遍历留给 S1-D4
    pub address_space: Vec<MapEntry>,
    /// GDT 基址/限长
    pub gdt: GdtTable,
    /// 处理器状态段（含 RSP0 中断栈）
    pub tss: Tss,
    /// 执行上下文入口（kernel_main）
    pub rip: VirtAddr,
    /// 已设栈指针
    pub rsp: VirtAddr,
    /// 交接的 BootInfo 抽象（内存区域等）
    pub boot_info: BootInfoHandle,
    /// IDT 是否已装载（交接点通常尚未）
    pub idt_ready: bool,
    /// 中断屏蔽状态
    pub interrupt_state: InterruptState,
    /// 细化层可见：bootstrap 是否已执行（W14 唯一性，POSIR 不可见）
    /// α 依据此标志做两段投影：false → EmptyState，true → booted init 态
    pub booted: bool,
}
```

### 2.1 关键取舍

- **不建模 GPR 全集**：只建模对 Bootstrap 有意义的状态（rip/rsp/表/地址空间）。真实寄存器保存/恢复属于异常帧细化（§18 边界清单 #1），非本切片范围。
- **页表用简化 `MapEntry` 视图**：交接点只需表达"地址空间有哪些映射、flags 如何"，不需要多级 PML4 遍历逻辑。真实遍历是 S1-D4 的页表/trap 框架工作。
- `MapEntry` 的 `flags` 复用 cosir 的 R/W/X 位域常量（`ACCESS_R`/`ACCESS_W`/`ACCESS_X`），保持访问权限语义一致。

## 3. 抽象函数 α

`α : MachineState → cosir::state::State`（`cosir/src/abstract.rs`）把机器态投影为 POSIR 抽象状态。α 复用 `cosir::state::State` 作为协域——与已验证的 exc_resume 探针语义严格一致，不新增抽象态类型（避免 COSIR §3 的 ID 混用风险）。

Bootstrap 阶段的核心投影是**两段**：

1. **交接点 / bootstrap 前**：机器态投影为 POSIR `EmptyState`（无任务、无能力、未 booted）；
2. **Bootstrap 迁移后**：`booted = true`，初始 Authority 铸造给 init Task，初始对象/能力快照生成。

抽象函数满足：
- **投影不引入多余状态**：α 只把机器态中"有 POSIR 对应"的部分投影进抽象态，机器内部结构（如页表布局细节）不进抽象态；
- **层归属清晰**：MachineState 与 α 模块位于 cosir（细化层探针），不并入 POSIR 抽象正文。

## 4. Bootstrap 细化对应

按 COSIR §14 的自定义：`bootstrap` 是唯一允许在没有 Task Actor 时铸造初始 Authority 的内部迁移；创建的初始 Capability 直接位于 init Task；Bootstrap 后再次调用必须返回 `AlreadyBooted` 且状态不变。

在下 x86_64 交接点上实例化：

| POSIR 抽象（COSIR §14） | x86_64 细化（本切片定义） |
|---|---|
| `EmptyState`（无任务、无能力、未 booted） | 交接点机器态：`kernel_main` 接管、无用户 Task、无能力表 |
| `bootstrap` 迁移 | 内核固化 bootstrap 路径：铸造初始 Authority |
| 初始 Root Capability 铸造至 init Task | 交接点建立 init 上下文：初始栈、入口、内存视图 |
| `AlreadyBooted` 幂等保护 | 细化层保持 `booted` 标志，重复 bootstrap 不重复铸造 |
| init 只获定义初始 Authority | 交接点后 init 的能力快照是 ObjectRef/CapRecord 的机器投影 |

### 4.1 细化层可见（不新增 POSIR 原语）的不变量

- **W14 唯一性细化**：机器侧 `booted` 标志与 POSIR `booted` 一一对应，交错不违背唯一性；
- **初始 Authority 单调**：α 投影后，init Task 的能力集恰好等于规范定义的初始 Authority，无多余能力（对 S1-X2 的在 α 层承接）；
- **投影保真**：`α(booted_machine) ≠ α(pre_bootstrap_machine)`，即 bootstrap 确改变抽象态。

### 4.2 明确不覆盖（留后续切片）

MemorySegment 的 CR3/页表↔vspace 全量细化、task_create 入口细化（边界清单 #16）、异常/trap 递送（S1-D4）——这些只登记接口，本切片不对其展开。

## 5. 错误处理与测试计划

### 5.1 错误语义

Bootstrap 的 `AlreadyBooted` 在 α 层表示为：对已 booted 机器态再调 bootstrap 返回 `AlreadyBooted` 且状态不变。探针验证该幂等性。

### 5.2 测试三件套（TDD，先失败后实现）

1. **静态** `test_bootstrap_init_authority_preserved`：交接点机器态 → α → 断言输出满足 Bootstrap 后 WellFormed，且 init 无多余能力；
2. **动态** `test_machine_steps_map_to_bootstrap_migration`：机器步骤序列（交接 → GDT/IDT 配置 → bootstrap → booted）每步映射断言——每个可观察机器步骤恰好对应一个 POSIR 迁移（Bootstrap），内部配置步骤映射为"不改变抽象状态"的内部步骤；
3. **幂等** `test_bootstrap_idempotent`：二次 bootstrap 返回错误且状态不变。

### 5.3 验收

- `cargo test -p cosir` 全绿（既含 exc_resume 12 测试，含新增 bootstrap 测试）；
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings` 零警告；
- `cargo xtask test` 六条 PASS 不破坏启动闭环；
- 实施文档登记 SPEC-013，BLK-002 状态从"无细化草案"变为"细化草案已定义"（不关闭 —— S0-X1/X2/X3 仍需覆盖报告；BLK-001 仍 Open）。

## 6. 范围边界（证伪边界）

本研究主张（见项目记忆 `project_research_claims`）要求区分"已定义机制"与"待证明性质"。本切片：

- **已定义**：x86_64 MachineState（交接点起）、抽象函数 α、Bootstrap 细化对应；
- **待证明**：α 的完整性（对全部 POSIR 状态成立）、真实实现满足 α（需 S1 之后真实 kernel 对照）、形式化验证（BLK-001）。

若实现中发现"需在 POSIR 层新增机制才能完成 Bootstrap 细化"，构成对 H3（小型可信机制层）/H1（机制表达力）的潜在反例，须记录在案（见 COSIR §18.0 末尾规则）。
