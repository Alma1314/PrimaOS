---
id: lean-model-mapping
kind: reference
authority: model-mapping
status: maintained
lifecycle: current
updated: 2026-10-26
---

# 形式化模型与内核映照（Lean）

> 证据边界：本切片（BLK-001）以 **MODEL-TEST** 支撑 bootstrap / exc_resume / derive 的可执行状态模型，
> 并以 **REFINEMENT-DESIGN**（独立 Lean 证明通道）支撑 §4.2 **能力派生树（CDT）不变量**的保持性。
> **不宣称** `REFINEMENT-PROOF`（内核 C 级细化）、`KERNEL-TRACE`、`COMPAT-TEST`、机器证明，或真实硬件保证。
> 本文是 Lean 模型与 cosir host / 内核句柄模型之间的**唯一权威映照契约**。新增内核句柄模型是独立命名空间，不扩充下述 CDT `State`，也不将模型证明提升为 Rust 内核的细化证明。

## 模块与入口

- `Formal.State`（`Formal/State.lean`）：`Object`/`Capability`（含 §4.2 派生树字段 `parent`）/`State`/`capDerive`/`WellFormed`（**能力派生树不变量**，非恒真）/`initTaskId = 1`。
- `Formal.Bootstrap`（`Formal/Bootstrap.lean`）：`alpha`/`emptyState`/`bootedState`/`bootstrap` + 8 条定理（其中 `bootedState_wellformed` 为**非平凡**证明）。
- `Formal.ExcResume`（`Formal/ExcResume.lean`）：`excResume` + 5 条定理（含**非平凡** `exc_resume_preserves_wellformed`）。
- `Formal.Derive`（`Formal/Derive.lean`）：§4.4 `cap_derive` 对应物 `derive` + `derive_preserves_wellformed`（**非平凡**）+ `no_self_parent`（CDT 无自环）+ 5 条 `native_decide` 可执行锚。
- `Formal.Forest`（`Formal/Forest.lean`）：`parentOf`/`ancestry` + **`ancestry_lt`（森林性/无环：父链 `k ≥ 1` 步严格递减，§4.2 头号结论）**（**非平凡**）+ `ancestry_no_cycle` + 3 条 `native_decide` 可执行锚。
- `Formal.KernelHandle`（`Formal/KernelHandle.lean`）：有界 generation、槽分配/释放、退役、任意迁移序列中的旧句柄不复活（见下节）。
- `Formal.lean`（入口）：`import` 上述六个模块，使 `lake build` 覆盖全部 `Formal.*`；含 `smokeState` 冒烟 `#eval`。

## 构建与复现

`lean-toolchain` 固定 `leanprover/lean4:v4.35.0`；模型只用 Lean 自带库，无外部 Git package。Lake 对库模块启用 `autoImplicit=false` 与 `warningAsError=true`，拒绝未声明隐式变量与含警告的证明构建。已安装该版本后，模型构建本身不需要下载第三方依赖。在 `lean/` 运行：

```sh
elan run leanprover/lean4:v4.35.0 lake build
```

通常 elan shim 下 `lake build` 也会读取固定版本。若 PATH 直接指向其他版本的二进制，则使用上面的显式命令，不能只凭文件内容声称实际使用了固定版本。本次升级重新构建全部既有模块，移除旧版本弃用定理名，不改变 CDT 迁移定义。

## 内核句柄模型（独立于 CDT）

[设计](../docs/superpowers/specs/2026-10-24-kernel-handle-model-design.md)与[证据报告](../docs/superpowers/reports/2026-10-24-kernel-handle-model-report.md)记录内核 generation 回绕、权限未知位及失败提交顺序的补强。`Formal.KernelHandle` 只证明其中的**单物理槽 generation 生命周期**，权限修复由 Rust 测试验证。

| Lean KernelHandle | Rust 内核 | 映照限制 |
|---|---|---|
| `Slot.generation : Nat` | `SlotEntry.generation : u32` | `WellFormed limit` 限定 ≤ limit；生产实例 limit=`maxGeneration=2^32-1`，0 为退役 |
| `Slot.inUse : Bool` | `SlotEntry.in_use : bool` | 单个固定物理槽；不建模整张表或最小槽选择 |
| `allocate` | `CapabilitySlotTable::alloc` / `reserve` 经 `slot_free` | 只分配 generation 非 0 且非活动的槽；Rust 另有索引/保留区间检查 |
| `release limit s handle` | `CapabilitySlotTable::free` / `Handle::next_generation` | 成功时 < limit 则加一，否则退役；无效/不活动/不匹配返回 none，Rust 返回 false 且不提交 |
| `check s generation` | `CapabilitySlotTable::check(handle) == Live` | 假定是同一合法非零槽索引，只表达 generation/活动条件，不区分 Free 与 Stale |
| `Step` / `Reachable` | 任意次分配、释放、无状态变化的失败/其他槽操作 | 模型不允许任意外部写表、重置或恢复旧快照；不是从 Rust 自动提取的状态机 |

一般性定理（均非 `native_decide`）：分配与释放保持 `WellFormed`；任意序列保持范围；退役为吸收状态；释放产生 `Invalidated`，该谓词对任意 `Reachable` 序列保持；`freed_handle_never_live` 得出释放后的旧 generation 永不 Live。六条 `native_decide` 锚只求值末两代、退役、重复释放和错误 generation。

本模型不复用 CDT CapId；内核的逻辑 `CapabilityRecord.cap_id` 与物理槽 generation 仍是两层编号。不覆盖 encode/decode 的位运算证明、CapId/历史提交、权限位集、holder/location、对象存活、整表分配策略、并发、硬件或完整内核安全性。Rust 的末代/退役/全表耗尽/固定槽测试为映照实例，**不是自动双向差分或 Rust refinement proof**。下方原有“5 条实质证明”计数只属于 CDT 通道，不包含本模块。

## 证据边界

| 维度 | 本切片覆盖 | 不覆盖（后续切片） |
|---|---|---|
| 模型对象 | `bootstrap`（空态→booted 态）、`excResume`（授权检查）、`derive`（§4.4 能力派生） | `tlb_refine`、`memseg_refine`、`cow_refine` |
| 证据等级 | `MODEL-TEST`（Lean `#eval`/`theorem` 参考 + cosir host 对拍）+ `REFINEMENT-DESIGN`（独立 Lean 证明通道：CDT 不变量保持性） | `REFINEMENT-PROOF`（内核 C 级细化）、`KERNEL-TRACE`、`COMPAT-TEST`、机器证明 |
| 目标 | 可执行状态迁移、**非平凡良构谓词（CDT 不变量）及其保持性**、失败原子性 | 真实硬件、CR3/PTE、TLB、多核、帧字节复制 |

## 映照契约（Lean ↔ cosir 逐字段）

| Lean（Formal 命名空间） | cosir Rust | 说明 |
|---|---|---|
| `State.booted : Bool` | `state::State.booted : bool` | POSIR bootstrap 标志 |
| `State.caps : Nat → Option Capability` | `state::State.capabilities : HashMap<CapId, Capability>` | 能力表：cap 部分函数 ↔ HashMap。空态两侧一致；**booted 态两侧亦一致**——均铸 §14 三条无父根 Capability（CapId 1/2/3） |
| `Capability.owner/obj/rights/active` | `state::Capability.owner/object/rights/active` | 字段形态对应（`rights` 两侧均为 32 位位掩码，位值一致） |
| `Capability.parent : Option Nat` | `state::Capability.parent : Option<CapId>` | §4.2 派生树的父 CapId（`none` = 铸造根），seL4 CDT 对应物。内核（`CapRecord.parent`，`kernel/src/bootstrap.rs` 的 `CapabilityRecord`、`derive_capability`/`CapabilityDerived`）、Lean、cosir（**2026-10-13 补入**）**三方一致**；cosir 侧由 `abstract_ops::well_formed`/`derive_capability` 表达，`lean_backtrace::cdt_parity_lean` 对拍 |
| `Object.segment/task/excSource/namespace` | `state::CapObject::Segment/Task/ExceptionSource/Namespace` | 对象引用和类型（名称差异） |
| `initTaskId = 1` | `abstract_ops::init_task_id() = 1` | init Task 恒定量 ID |
| （**不建模**，Lean `State` 无 `tasks` 字段） | `state::Task.root_ns_cap : CapId` | §5.2/§14 步骤 7 结构性根 Namespace Capability（W5）；Lean 侧只以三条根 Cap 的 `owner/obj` 隐含表示归属，**无该字段**（cosir-only） |
| （**不建模**，Lean 无 MachineState） | `machine::MachineState.image_frames`/`image_virtual_base` | §14 步骤 4 image 逐页 `vspace` 的投影输入；**cosir-only** |
| （**不建模**，Lean `State` 无 `audit`/`now`/`next_event`） | `state::AuditEvent{id,at,actor,kind}`、`state::Actor`、`state::State.now`/`next_event` | §13.1 事件信封与 §4 时间/序号、§6.1 `Actor`；**cosir-only**（Lean 不建模 Audit） |
| `Capability.parent`（Lean/cosir 均已建模） | `state::Capability.parent` | §4.2 根性（`parent = None`）**两侧均可表示**（`WellFormed`/`well_formed` 已用，`cdt_parity_lean` 对拍）；§4.6 `HeldBy` 位置、W3/W4/W13 的 `created_at → 铸造事件` 仍**不可表示**（cosir/Lean 共同边界未消） |
| `WellFormed : Prop` | `abstract_ops::well_formed` | **能力派生树不变量（非恒真）**：每条派生能力的 `parent < c`、父存在/活动/同对象（§4.2）。cosir 侧对应 `well_formed`（2026-10-13 补入），由 `cdt_parity_lean` 在 booted 态与派生后各断言一次 |

## 对拍机制

- **Lean 侧为参考语义**：模块中的 `theorem`（`by rfl`）与入口 `Formal.lean` 的 `#eval` 冒烟给出可执行、可检查的参考结果。
- **Rust 侧对拍**：`cosir/tests/lean_backtrace.rs` 把同一输入在 cosir host 模型跑一遍，断言与 Lean 语义一致：
  - `bootstrap_parity_lean`：α 投影保真（`α(booted) ≠ α(pre_bootstrap)`）、init task 存在，**逐条断言 §14 三条根 Capability**（数量=3、CapId 1/2/3 的对象与 rights，对应 Lean `bootstrap_empty_mints_{image,namespace,task}_cap`），并**钉住 cosir 侧的 §14 步骤 4/7 投影**（image 逐页 `vspace` 的基址/条数、`Task.root_ns_cap == 2`）——这两项 Lean 未建模，见下表与「已知不覆盖」。
  - `exc_resume_failure_parity_lean`：caller 无源能力 → `Err(InvalidExcSource)` 且状态完全不变，对应 Lean `excResume … = none` 的失败原子性。
  - `cdt_parity_lean`（**关 `ISSUE-FORMAL-001`**）：§4.2/§4.4 CDT 对拍——三条根 Cap 无父、`well_formed(booted)` 成立（对应 Lean `bootedState_wellformed`）；`derive_capability` 派生后子指父、同对象、`well_formed` 保持（对应 `derive_preserves_wellformed`）；父链严格递减并终止于根（对应 `ancestry_lt`/`ancestry_no_cycle`）；四条失败锚（父 ≥ 子 / 无父 / 缺 `CapDerive` / 槽占用）对应 Lean `derive` 的 4 条 `native_decide` 锚。
- 两端一致即本轮 `MODEL-TEST` 通过；一致性不升级为 refinement 结论。
- **定理性质（如实声明）**：Lean 侧现有 5 条**实质证明**（**非** `by rfl`/`trivial`，`WellFormed` 亦**非恒真**）——`bootedState_wellformed`（逐 CapId 展开 `initCaps` 三分支证三条根 Cap 满足 CDT 不变量）、`exc_resume_preserves_wellformed`（成功返回原状态、失败返回 `none`）、`derive_preserves_wellformed`（§4.4 派生保持良构，按「新槽位 / 旧槽位」两路，用到 `parent < newId` 与空槽前提）、`parentOf_lt`（§4.2 单步下降）、**`ancestry_lt`（森林性：沿父链 `k ≥ 1` 步 CapId 严格递减 ⇒ 无环，§4.2 头号结论，对 `k` 归纳）**；`no_self_parent`/`ancestry_no_cycle` 为其直接推论。其余定理（§14 三条根 Cap 具体值、`bootstrap` 幂等、投影保真、`exc_resume` 失败各路径、`derive` 的 5 条 `native_decide` 锚）仍是**具体实例的定义展开或求值**——它们钉住可执行语义，但不单独构成普适不变量。Lean `State` 仍是**最小可表达能力子集**（**无 `tasks` 字段**，init 以三条指向 Segment/Namespace/Task 的根 Capability 表示），`excResume` 只建模授权检查、**不建模 §12.10 的成功提交段**；`derive` 只建模 §4.4 的单步派生、**不建模 §4.5 撤销与在途消息**。
- **CDT 覆盖边界**：`WellFormed` 现表达 §4.2 的「父 `<` 子、父存在/活动、父子同对象」三条，且**无环性已由 `ancestry_lt` 独立证成**（不止于不变量中的局部「父 `<` 子」）；**未**表达「子权限 ⊆ 父权限」（该性质作为 `derive` 的前置条件，在保持性定理中携带，见 `Formal/Derive.lean` 注释），亦未表达 §4.6 `HeldBy` 位置与 W3 唯一位置。§4.5 `cap_drop` 与 §4.7 撤销边界（**规范第一版无 `cap_revoke`**）留待后续切片。

## 已知不覆盖（后续切片）

- `tlb_refine`：非一致刷新 / 显式 `invalidate_va` / `flush_all`（见 cosir `tlb_refine.rs`，Lean 建模留待后续切片）。
- `memseg_refine`：MemorySegment host 四级索引树与 W6/W7 双视图（见 cosir `memseg_refine.rs`，Lean 建模留待后续切片）。
- `cow_refine`：COW 组合序列（H1 承载体，见 cosir `cow_refine.rs`，Lean 建模留待后续切片）。
- **§14 步骤 4/7（image vspace 与结构性根）**：§20.3「init 能力集恰好等于 §14 初始 Authority」的**能力部分已兑现**（三条根 Capability 两侧一致）；**cosir 侧已投影** §14 步骤 4 的 image 逐页 `vspace`（由 `MachineState.image_frames`/`image_virtual_base` 生成 `PageMapping{backing}`）与步骤 7 的结构性根（`Task.root_ns_cap = 2`），并由 `bootstrap_parity_lean` 钉住。**Lean 侧未建模**这两项（`State` 无 `tasks` 字段、无 MachineState）；该分歧已在映照契约表如实标注（cosir-only）。
- **`raise_exception` 的 Audit（§12.5/§13.1）**：**cosir 侧已补齐**（2026-10-12）——`AuditEvent` 新增 `ExceptionRaised`（§12.5 动作 1）与 `TaskStateChanged`（§13.1），`ops::raise_exception` 按 §12.5 顺序追加；同日续片（`ISSUE-EXC-008`）再把 `AuditEvent` 重构为 §13.1 **信封 `{id, at, actor, kind}`** 并补 `State.now`/`next_event` 与 §6.1 `Actor`。**Lean 侧不建模 `audit`**（`State` 无该字段/无时间），故无对拍项、此分歧为 cosir-only。原bootstrap探针的now初值仍为0；2026-10-26 authority事件组合新增advance_time/Timer，本轮host回归已通过，Lean仍无时间/audit。
- **§14 步骤 9 的 bootstrap 初始 Audit（`ISSUE-EXC-009`，2026-10-13）**：**cosir 侧已补齐**——`abstract_ops::abstract_state(booted)` 写入第一组 Audit：`Bootstrapped` → 两个 `ObjectCreated`（image Segment、init Namespace，`segment` 标志区分同 id 对象）→ init Task 的**创建**事件。顺序与形态镜像真实内核（`kernel/src/bootstrap.rs`）；init Task 用 §13.1 的 `TaskCreated`（**创建非迁移**，故不误用 `TaskStateChanged`）。三条根 Capability **不发 `CapMinted`**（§13.1 该事件用于运行时铸造路径，内核亦不发）。**Lean 侧不建模 `audit`**，故 cosir-only、无对拍项。
- ~~**能力派生树（CDT）在 cosir 侧未建模（Lean-ahead 分歧）**~~ **已闭合（2026-10-13，SPEC-099）**：cosir `state::Capability` 已补 `parent : Option<CapId>`，新增 `abstract_ops::well_formed`/`derive_capability`，`lean_backtrace::cdt_parity_lean` 对拍 Lean 的 CDT 定理（关 `ISSUE-FORMAL-001`）。**Lean 仍不覆盖**：§4.5 `cap_drop`、§4.6 `HeldBy` 位置、W3 唯一位置、§4.7 撤销（规范第一版不提供）。
  cosir 新增本地 Task/消息/Namespace Bound 的位置、drop 与溯源状态面，见[工作契约](../docs/superpowers/specs/2026-10-25-capability-location-model-design.md)与[分层验证记录](../docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)。
  原 `Capability.owner` 仅映照创建时拥有者，移动不改它；当前授权改查 `State.locations`。
  2026-10-26新增位置/载体更新的有界Lean证明，2026-10-27 lake build通过，见下节；完整消息/Namespace及事件迁移仍没有Lean保持性证明。CDT显式ID helper不升级为完整用户授权操作。
- 以上内容在本切片**仅登记为待办**，不标为已完成。


## 2026-10-26 映照增量与2026-10-27验证范围

[工作契约](../docs/superpowers/specs/2026-10-26-event-authority-model-design.md)扩展cosir组合状态。
本节明确取代上文关于“State无位置、rights16位、excResume查询owner”的旧映照描述；历史证明结论仍只对应各报告版本。
本轮workspace 1016项与lake build（10 jobs）通过；只声明下表及既有对拍覆盖的字段，不外推完整组合状态或机器细化。

| Lean | cosir | 边界 |
|---|---|---|
| Object.timer/node/waitObject | CapObject::Timer/Node/WaitObject | 只增加引用构造子，不含对象状态 |
| Capability.rights UInt32 | Capability.rights u32 | 保留原bit0..14，新增bit15..22；非机器ABI位值 |
| State.locations | State.locations | HeldBy/InMessage/InNamespace，owner保持创建投影；Bootstrap三根位置一致 |
| excResume位置守卫 | ops::exc_resume的held检查 | 最小Lean模型仍不验证源类型/权限/帧/pending或成功提交；不能作为完整恢复证明 |
| derive登记新HeldBy | 内部显式ID derive/helper | 新位置复制父HeldBy、非持有父则不产生位置；完整用户授权仍属于authority操作 |
| TokenLocation.Snapshot.carriers | messages.caps/bindings/held位置的载体投影 | 仅逐token的存在关系；不表示FIFO顺序/对象存活 |
| transfer/dropToken + Consistent | relocate/remove_location的原子更新核心 | 保持双索引一致、唯一载体、历史记录不变、当前持有者及结构根守卫；任务/目标合法性由外层操作预检 |

Formal.Location已被Formal.lean导入，保持性和唯一载体证明通过lake build；不再声称位置完全未建模。
Timer/异常fan-out/WaitObject/upcall/远端数据的cosir host回归已通过；Lean不证明这些迁移。
原memseg/tlb/cow/GPU/设备/调度的范围限制继续有效；本次不自动升级REFINEMENT-PROOF或完整W1–W17。

本轮cosir authority模型21项Miri检查通过；它属于host解释执行证据，不扩展本页Lean定理或机器细化范围。
