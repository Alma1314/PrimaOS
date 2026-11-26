---
id: lean-model-mapping
kind: reference
authority: model-mapping
status: maintained
lifecycle: current
updated: 2026-11-25
---

# 形式化模型与内核映照（Lean）

> 证据边界：本切片（BLK-001）以 **MODEL-TEST** 支撑 bootstrap / exc_resume / derive 的可执行状态模型，
> 并以 **REFINEMENT-DESIGN**（独立 Lean 证明通道）支撑 §4.2 **能力派生树（CDT）不变量**的保持性。
> **不宣称** `REFINEMENT-PROOF`（内核 C 级细化）、`KERNEL-TRACE`、`COMPAT-TEST`、机器证明，或真实硬件保证。
> 2026-10-31 的独立 `refinement/` 包新增两个生产 Rust 方法到单槽模型的条件性源码细化证明，范围见文末；它不改变以上 CDT/整内核边界。
> 2026-11-25 的 `Formal.Tlb` 切片把 cosir `tlb_refine` 的最小参考语义纳入模型（每 Task 缓存、命中/未命中、显式失效与新鲜性），抽象边界见文末；它不改变 CDT/句柄边界，也不构成 SMP、TLB shootdown 或四级页表证明。
> 2026-11-25 的 `Formal.Memseg` 切片把 cosir `memseg_refine` 的最小参考语义纳入模型（双视图一致、权限收窄、帧 owner 转移与失败原子性），抽象边界见文末；它不构成四级树、多页原子映射或 W6 全量扫描证明。
> 2026-11-25 的 `Formal.Cow` 切片把 cosir `cow_refine` 的最小参考语义纳入模型（COW 建立、写故障、私有化、单运行任务与销毁），抽象边界见文末；它不构成 capability 授权、异常源/恢复帧或 TLB 失效证明。
> 2026-11-25 的 `Formal.Revoke` 切片把 Prima OS §6.2 撤销域（Bootstrap 宿主）的最小参考语义纳入模型（子树失效、HeldBy 删除、无关域不变、原子与幂等），抽象边界见文末。撤销判定现为**可执行**有界 `Bool` 链遍历（`descB`，燃料 `c + 1`），不再依赖经典可判定性；仍不构成 7-D 或 Rust 细化证明。
> 2026-11-25 的 `Formal.Drop` 切片把 COSIR §4.5 `cap_drop` 的规范层语义纳入可执行模型（单位置删除、不传播、记录保留、structural 保护），与 `Formal.Revoke` 正交，见文末。
> 2026-11-25 的 `Formal.Move` 切片把 COSIR §4.6 的位置变化纳入可执行模型（转移表、单位置原子更新、原位置/新位置与直投 `MsgId` 记录、删除到 `None`、跨节点不移动、W3 表示层唯一），抽象边界与 W4 容器侧缺口见文末。
> 2026-11-25 的 `Formal.Container` 切片把 COSIR W4 的容器双向一致纳入可执行模型（`HeldBy` 的 Task 活动性、`InMessage` ↔ 消息 caps、`InNamespace` ↔ Namespace entries），并证明 send/recv/bind/unbind 的原子耦合更新保持 W3+W4；函数式容器表示使“列表无重复/路径唯一”由构造满足，见文末。
> 2026-11-25 的 `Formal.Subscription` 切片把 §12.4/W15 订阅生命周期纳入关系模型（订阅表、usable `ExcSubscribe` 见证、subscribe/unsubscribe、失去最后一个授权 Cap 时的原子终止与源销毁），不带 `#eval` 锚，见文末。
> 2026-11-25 的 `Formal.Audit` 切片把 §13.1/W13 审计日志核心纳入模型（信封、追加式日志、由位置推导的严格递增 EventId、位置历史、created_at、对象创建事件与受审计操作），其后的 `audit` 查询由 `Formal.AuditQuery` 最小建模（`WellFormed` 下父链燃料完备性已证），事件载荷完整性仍不覆盖，见文末。
> 本文是 Lean 模型与 cosir host / 内核句柄模型之间的**唯一权威映照契约**。新增内核句柄模型是独立命名空间，不扩充下述 CDT `State`，也不将模型证明提升为 Rust 内核的细化证明。

## 模块与入口

- `Formal.State`（`Formal/State.lean`）：`Object`/`Capability`（含 §4.2 派生树字段 `parent`）/`State`/`capDerive`/`WellFormed`（**能力派生树不变量**，非恒真）/`initTaskId = 1`。
- `Formal.Bootstrap`（`Formal/Bootstrap.lean`）：`alpha`/`emptyState`/`bootedState`/`bootstrap` + 8 条定理（其中 `bootedState_wellformed` 为**非平凡**证明）。
- `Formal.ExcResume`（`Formal/ExcResume.lean`）：`excResume` + 5 条定理（含**非平凡** `exc_resume_preserves_wellformed`）。
- `Formal.Derive`（`Formal/Derive.lean`）：§4.4 `cap_derive` 对应物 `derive` + `derive_preserves_wellformed`（**非平凡**）+ `no_self_parent`（CDT 无自环）+ 5 条 `native_decide` 可执行锚。
- `Formal.Forest`（`Formal/Forest.lean`）：`parentOf`/`ancestry` + **`ancestry_lt`（森林性/无环：父链 `k ≥ 1` 步严格递减，§4.2 头号结论）**（**非平凡**）+ `ancestry_no_cycle` + 3 条 `native_decide` 可执行锚。
- `Formal.KernelHandle`（`Formal/KernelHandle.lean`）：有界 generation、槽分配/释放、退役、任意迁移序列中的旧句柄不复活（见下节）。
- `Formal.KernelHandleTable`（`Formal/KernelHandleTable.lean`）：有限槽表操作的逐槽投影、良构保持、跨槽隔离和整表任意序列无复活（2026-10-31 增量，见下节）。
- `Formal.Tlb`（`Formal/Tlb.lean`）：每 Task TLB 缓存、页表命中/未命中、单地址失效、全量刷新、任务移除，以及显式失效后的陈旧性边界与新鲜性保持（2026-11-25 增量，见下节）。
- `Formal.Memseg`（`Formal/Memseg.lean`）：每 Task 页表/视图双视图、W6 帧 owner 划分、map/unmap/create/destroy/destroyTask/spawn 的守卫与成功状态，以及失败原子性（2026-11-25 增量，见下节）。
- `Formal.Cow`（`Formal/Cow.lean`）：COW 对的建立/写故障/私有化、单运行任务、让出/调度与销毁，以及组合不变量 `RunningOk`/`FaultedOk`/`CowPairOk`（2026-11-25 增量，见下节）。
- `Formal.Revoke`（`Formal/Revoke.lean`）：选择性撤销的子树失效、HeldBy 位置删除、非 HeldBy 保留、无关域不变、原子守卫与幂等（2026-11-25 增量，见下节）。
- `Formal.Drop`（`Formal/Drop.lean`）：`cap_drop` 的单位置删除、不传播到派生子项、记录与 CDT 保持、structural 守卫与幂等（2026-11-25 增量，见下节）。
- `Formal.Move`（`Formal/Move.lean`）：§4.6 位置迁移表、原子 `applyMove`、原/新位置与直投 `MsgId` 事件、删除到 `None`、跨节点不移动、W3 表示层唯一位置（2026-11-25 增量，见下节）。
- `Formal.Container`（`Formal/Container.lean`）：W4 的 Task/Message/Namespace 容器与 `locations` 双向一致、W3+W4 组合不变量 `WorldOk`、四个耦合原子操作（send/recv/bind/unbind）的保持性与空世界锚（2026-11-25 增量，见下节）。
- `Formal.Subscription`（`Formal/Subscription.lean`）：§12.4/W15 订阅表与 usable `ExcSubscribe` 见证、subscribe/unsubscribe、drop/send 失去最后授权 Cap 的原子终止、源销毁，以及 W15 保持（2026-11-25 增量，见下节）。
- `Formal.Audit`（`Formal/Audit.lean`）：§13.1 事件信封与追加式日志、W13 严格递增 ID、Capability 位置历史、`created_at`、Task 状态与对象创建事件，以及受审计 move/drop/mint 的一步原子追加（2026-11-25 增量，见下节）。
- `Formal.AuditQuery`（`Formal/AuditQuery.lean`）：§13.2 `audit(cap)` 参数化查询——父链（root→cap）、created_at 列表、位置历史、目标对象事件、`InvalidCap`/`NotHeld` 错误语义，以及 `WellFormed` 下的父链燃料完备性（2026-11-25 增量，见下节）。
- `Formal.lean`（入口）：`import` 上述模块，使 `lake build` 覆盖全部 `Formal.*`；含 `smokeState` 冒烟 `#eval`。

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

本模型不复用 CDT CapId；内核的逻辑 `CapabilityRecord.cap_id` 与物理槽 generation 仍是两层编号。单槽模块不覆盖 encode/decode 的位运算证明、CapId/历史提交、权限位集、holder/location、对象存活、整表分配策略、并发、硬件或完整内核安全性。有限表组合证明见后继增量，最低槽/保留区间选择策略仍未证明。Rust 的末代/退役/全表耗尽/固定槽测试为映照实例，**不是自动双向差分或 Rust refinement proof**。下方原有“5 条实质证明”计数只属于 CDT 通道，不包含本模块。

## 证据边界

| 维度 | 本切片覆盖 | 不覆盖（后续切片） |
|---|---|---|
| 模型对象 | `bootstrap`（空态→booted 态）、`excResume`（授权检查）、`derive`（§4.4 能力派生） | 细化层模型（`tlb_refine`、`memseg_refine`、`cow_refine`）已有最小参考模型，见文末 |
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
| `Capability.parent`（Lean/cosir 均已建模） | `state::Capability.parent` | §4.2 根性（`parent = None`）**两侧均可表示**（`WellFormed`/`well_formed` 已用，`cdt_parity_lean` 对拍）；§4.6 位置迁移与 W3/W4 已由 `Formal.Move`/`Formal.Container` 建模（函数式容器），W13 的 `created_at → 铸造事件` 仍**不可表示**（cosir/Lean 共同边界未消） |
| `WellFormed : Prop` | `abstract_ops::well_formed` | **能力派生树不变量（非恒真）**：每条派生能力的 `parent < c`、父存在/活动/同对象（§4.2）。cosir 侧对应 `well_formed`（2026-10-13 补入），由 `cdt_parity_lean` 在 booted 态与派生后各断言一次 |

## 对拍机制

- **Lean 侧为参考语义**：模块中的 `theorem`（`by rfl`）与入口 `Formal.lean` 的 `#eval` 冒烟给出可执行、可检查的参考结果。
- **Rust 侧对拍**：`cosir/tests/lean_backtrace.rs` 把同一输入在 cosir host 模型跑一遍，断言与 Lean 语义一致：
  - `bootstrap_parity_lean`：α 投影保真（`α(booted) ≠ α(pre_bootstrap)`）、init task 存在，**逐条断言 §14 三条根 Capability**（数量=3、CapId 1/2/3 的对象与 rights，对应 Lean `bootstrap_empty_mints_{image,namespace,task}_cap`），并**钉住 cosir 侧的 §14 步骤 4/7 投影**（image 逐页 `vspace` 的基址/条数、`Task.root_ns_cap == 2`）——这两项 Lean 未建模，见下表与「已知不覆盖」。
  - `exc_resume_failure_parity_lean`：caller 无源能力 → `Err(InvalidExcSource)` 且状态完全不变，对应 Lean `excResume … = none` 的失败原子性。
  - `cdt_parity_lean`（**关 `ISSUE-FORMAL-001`**）：§4.2/§4.4 CDT 对拍——三条根 Cap 无父、`well_formed(booted)` 成立（对应 Lean `bootedState_wellformed`）；`derive_capability` 派生后子指父、同对象、`well_formed` 保持（对应 `derive_preserves_wellformed`）；父链严格递减并终止于根（对应 `ancestry_lt`/`ancestry_no_cycle`）；四条失败锚（父 ≥ 子 / 无父 / 缺 `CapDerive` / 槽占用）对应 Lean `derive` 的 4 条 `native_decide` 锚。
- 两端一致即本轮 `MODEL-TEST` 通过；一致性不升级为 refinement 结论。
- **定理性质（如实声明）**：Lean 侧现有 5 条**实质证明**（**非** `by rfl`/`trivial`，`WellFormed` 亦**非恒真**）——`bootedState_wellformed`（逐 CapId 展开 `initCaps` 三分支证三条根 Cap 满足 CDT 不变量）、`exc_resume_preserves_wellformed`（成功返回原状态、失败返回 `none`）、`derive_preserves_wellformed`（§4.4 派生保持良构，按「新槽位 / 旧槽位」两路，用到 `parent < newId` 与空槽前提）、`parentOf_lt`（§4.2 单步下降）、**`ancestry_lt`（森林性：沿父链 `k ≥ 1` 步 CapId 严格递减 ⇒ 无环，§4.2 头号结论，对 `k` 归纳）**；`no_self_parent`/`ancestry_no_cycle` 为其直接推论。其余定理（§14 三条根 Cap 具体值、`bootstrap` 幂等、投影保真、`exc_resume` 失败各路径、`derive` 的 5 条 `native_decide` 锚）仍是**具体实例的定义展开或求值**——它们钉住可执行语义，但不单独构成普适不变量。Lean `State` 仍是**最小可表达能力子集**（**无 `tasks` 字段**，init 以三条指向 Segment/Namespace/Task 的根 Capability 表示），`excResume` 只建模授权检查、**不建模 §12.10 的成功提交段**；`derive` 只建模 §4.4 的单步派生、**不建模 §4.5 撤销与在途消息**。
- **CDT 覆盖边界**：`WellFormed` 现表达 §4.2 的「父 `<` 子、父存在/活动、父子同对象」三条，且**无环性已由 `ancestry_lt` 独立证成**（不止于不变量中的局部「父 `<` 子」）；**未**表达「子权限 ⊆ 父权限」（该性质作为 `derive` 的前置条件，在保持性定理中携带，见 `Formal/Derive.lean` 注释），§4.6 的位置转移表、单位置原子更新与从/到记录已由 `Formal.Move` 最小建模，W3 在表示层（`locations` 为 `Option`）与 carrier 视图唯一性上成立；W4 的 Task 活动性、`InMessage` 与 `InNamespace` 双向一致已由 `Formal.Container` 建模（函数式容器表示），列表级重复/路径唯一与对象生命周期仍未表达。§4.7 撤销边界（**规范第一版无 `cap_revoke`**）的 Bootstrap 子集已由 `Formal.Revoke` 给出最小参考模型；§4.5 `cap_drop` 的单位置、不传播与 structural 守卫已由 `Formal.Drop` 最小建模，订阅终止与审计事件仍不覆盖。

## 已知不覆盖（后续切片）

- **上下文与共享线程（2026-11-01）**：83–89、受管COW进程/映像替换/审计读取、TaskContext、整帧/FXSAVE答复与同Gate共享root
  未纳入Lean/COSIR Gate模型；[协议与构建报告](../docs/superpowers/reports/2026-11-01-stage6-context-signals-threads.md)
  不从既有W17/句柄/CDT证明推出该新协议正确。Linux信号、futex与探针属于用户态；新增Rust检查已在[统一验收](../docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)中执行，不扩大Lean证明范围。

- **受管理的初始装载（2026-11-01）**：内核`TaskRecord.launch_held`、`Prepared`初始停车、
  79–82机器入口、原子Gate挂接/发布、目标TLS与boot manifest v3未纳入Lean或COSIR Gate可执行模型。
  该细化与既有prepared fork分开标记，契约见[受管理装载](../docs/superpowers/specs/2026-11-01-stage6-managed-launch.md)。
  新增Rust检查已在[统一验收](../docs/superpowers/reports/2026-11-02-stage6-process-and-differential.md)中执行；不从现有句柄/CDT/位置定理推导该完整协议正确。

- ~~`tlb_refine`：非一致刷新 / 显式 `invalidate_va` / `flush_all`~~ **最小模型已纳入（2026-11-25，见文末）**：抽象页表与地址/任务守卫取代四级页表、帧所有权和真实 CR3；多核 shootdown 与平台 TLB 行为仍不覆盖。
- ~~`memseg_refine`：MemorySegment host 四级索引树与 W6/W7 双视图~~ **最小模型已纳入（2026-11-25，见文末）**：四级树、页表帧归属扫描、多页原子映射与 `StillMapped` 枚举被抽象成函数式表与调用方守卫；`cow_refine` 见 2026-11-25 Cow 节。
- ~~`cow_refine`：COW 组合序列（H1 承载体，见 cosir `cow_refine.rs`）~~ **最小模型已纳入（2026-11-25，见文末）**：capability 授权、异常源/订阅、SRET 恢复帧、`exc_resume` 校验与 TLB 失效被抽象成前置或省略；细化层三个宿主的“后续切片”清单至此全部有最小参考模型。
- **§14 步骤 4/7（image vspace 与结构性根）**：§20.3「init 能力集恰好等于 §14 初始 Authority」的**能力部分已兑现**（三条根 Capability 两侧一致）；**cosir 侧已投影** §14 步骤 4 的 image 逐页 `vspace`（由 `MachineState.image_frames`/`image_virtual_base` 生成 `PageMapping{backing}`）与步骤 7 的结构性根（`Task.root_ns_cap = 2`），并由 `bootstrap_parity_lean` 钉住。**Lean 侧未建模**这两项（`State` 无 `tasks` 字段、无 MachineState）；该分歧已在映照契约表如实标注（cosir-only）。
- **`raise_exception` 的 Audit（§12.5/§13.1）**：**cosir 侧已补齐**（2026-10-12）——`AuditEvent` 新增 `ExceptionRaised`（§12.5 动作 1）与 `TaskStateChanged`（§13.1），`ops::raise_exception` 按 §12.5 顺序追加；同日续片（`ISSUE-EXC-008`）再把 `AuditEvent` 重构为 §13.1 **信封 `{id, at, actor, kind}`** 并补 `State.now`/`next_event` 与 §6.1 `Actor`。**Lean 侧不建模 `audit`**（`State` 无该字段/无时间），故无对拍项、此分歧为 cosir-only。原bootstrap探针的now初值仍为0；2026-10-26 authority事件组合新增advance_time/Timer，本轮host回归已通过，Lean仍无时间/audit。
- **§14 步骤 9 的 bootstrap 初始 Audit（`ISSUE-EXC-009`，2026-10-13）**：**cosir 侧已补齐**——`abstract_ops::abstract_state(booted)` 写入第一组 Audit：`Bootstrapped` → 两个 `ObjectCreated`（image Segment、init Namespace，`segment` 标志区分同 id 对象）→ init Task 的**创建**事件。顺序与形态镜像真实内核（`kernel/src/bootstrap.rs`）；init Task 用 §13.1 的 `TaskCreated`（**创建非迁移**，故不误用 `TaskStateChanged`）。三条根 Capability **不发 `CapMinted`**（§13.1 该事件用于运行时铸造路径，内核亦不发）。**Lean 侧不建模 `audit`**，故 cosir-only、无对拍项。
- ~~**能力派生树（CDT）在 cosir 侧未建模（Lean-ahead 分歧）**~~ **已闭合（2026-10-13，SPEC-099）**：cosir `state::Capability` 已补 `parent : Option<CapId>`，新增 `abstract_ops::well_formed`/`derive_capability`，`lean_backtrace::cdt_parity_lean` 对拍 Lean 的 CDT 定理（关 `ISSUE-FORMAL-001`）。**Lean 仍不覆盖**：审计事件载荷完整性（W13 日志核心见 `Formal.Audit` 节，查询与 `WellFormed` 下父链完备性见 `Formal.AuditQuery` 节）、W4 的列表级重复/路径唯一与对象生命周期；§12.4/W15 订阅生命周期见 `Formal.Subscription` 节，§4.6 位置迁移与 W3 表示层唯一见 2026-11-25 `Formal.Move` 节，W4 双向一致见 `Formal.Container` 节，§4.5 单位置 drop 见 `Formal.Drop` 节，§4.7 撤销的 Bootstrap 子集见 `Formal.Revoke` 节（规范第一版仍不提供 `cap_revoke`）。
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


## 2026-10-29 门控探索的映照边界

[S6-M1 协议](../docs/superpowers/specs/2026-10-29-stage6-m1-protocol.md)与
[整改报告](../docs/superpowers/reports/2026-10-29-stage6-m1-blocker-closure.md)新增 host-only
`cosir::gate`/`gate_memory` 组合模型；复用真实 `BlockedWait`/WaitObject 双向注册和 W17 检查，
Gate/Request 是外围实验对象，未加入冻结的 `CapObject`，也没有新增 Lean 构造子或定理。
模型私有 token 与服务者检查不等于真实 CapTable 派生/移动/drop 已接线；跨任务页复制只证明
host 私有/COW 页预检和提交，不证明硬件页表、TLB、调度、分配失败或共享可写页。

内核与 Runtime 的机器权限载体扩为 u64，既有有效权限仍限 bit0–30，所有未知高位被拒绝；
GateAttach/GateServe 仅有协议中的 bit31/32 目标，尚未发布。上表 COSIR u32 / Lean UInt32
属于独立抽象权限命名空间，保持原样。本轮 Rust 测试不扩展任何 Lean 证明覆盖，未重新运行 Lean。


## 2026-10-31 有限内核槽表组合证明

[设计增量](../docs/superpowers/specs/2026-10-24-kernel-handle-model-design.md#2026-10-31-有限槽表组合增量)与[本次证据](../docs/superpowers/reports/2026-10-31-kernel-handle-table-proof.md)将单槽结果组合到 `Formal.KernelHandleTable`；不改变 CDT 模型和阶段 6 机制。

| Lean | Rust 映照与边界 |
|---|---|
| `Table size = Fin size → Slot` | `CapabilitySlotTable.entries`，生产 size=`MAX_CAPABILITY_SLOTS`；不含 `peak_live` 计数 |
| `Handle size` | 已解码且索引在范围内的 `(index, generation)`；保留零槽拒绝，不证明 u64 位编码及越界解码 |
| `allocateAt` | `alloc` 成功选择的槽，或 `slot_free` 守卫后的 `reserve`；允许任意可用非零槽，因此是安全性质的过近似，不证明最低槽和内部保留区间选择策略 |
| `releaseAt limit` | `free` 对单个活动匹配槽的更新；limit 生产取 `2^32-1`，无效操作为 none、无状态提交 |
| `Step` / `Reachable` | 串行分配、固定保留、释放和失败停留的任意长度交错；不含直接改表或恢复旧快照 |
| 表 `WellFormed` | 每槽满足既有 generation 上界/退役不活动约束；不等价于完整 Authority 不变量 |
| `check` | `CapabilitySlotTable::check == SlotState::Live`，不区分拒绝时的 Free 与 Stale |

10 条一般定理证明：操作只改目标槽；每条表迁移/序列可投影为每个槽的合法单槽迁移/序列；任意序列保持整表良构；分配返回 Live；其他槽句柄的校验结果不变；退役沿任意序列保持；一次成功释放后，旧完整句柄在任意后续整表操作中永不 Live。
三条 `native_decide` 仅为零槽分配/释放拒绝、其他槽保持 Live 的实例，不用于上述一般证明。

复现：在 `lean/` 执行固定版本 `lake build`，再运行：

```sh
elan run leanprover/lean4:v4.35.0 lake env lean Checks/KernelHandleTable.lean
```

此文件审计全部 10 条一般定理的公理依赖；本次仅有 Lean 标准 `propext`/`Quot.sound`，无 `sorryAx` 或 native 求值依赖。
Rust 的 `kernel/src/handle_table_tests.rs` 遍历三槽、深度六的实际分配/固定保留/释放序列，注入末两代与退休状态，并检查不串槽、旧句柄不复活和失败无修改。它是有限 host 见证，不是自动抽取/双向差分或 Rust 源码 refinement proof；Lean 本身的序列长度不受六步限制。


## 2026-10-31 Rust 源码细化：构造与 generation 核心

[契约](../docs/superpowers/specs/2026-10-31-rust-handle-refinement-design.md)与[实测报告](../docs/superpowers/reports/2026-10-31-rust-handle-source-refinement.md)建立第一条自动源码链。`refinement/source.rs` 直接引用生产 `kernel/src/handle.rs`；Charon 从 Rust MIR 提取 LLBC，Aeneas 生成 `refinement/Refinement/Generated/{Types,Funs}.lean`，再由 `refinement/Refinement/Handle.lean` 证明与既有模型的关系。

| 源码/提取物 | 映照与证明范围 |
|---|---|
| `Handle::new` → `RustHandle.handle.Handle.new` | 对任意 U32 index/generation，准确拒绝零分量，成功值保持输入 |
| `Handle::next_generation` → 同名生成方法 | `.val` 投影与 `<2^32-1` 时加一、否则永久退役 0 一致 |
| `generationAfter : Option U32 → Nat` | Some g→g.val，None→0，对应实际 free 使用的 `next_generation().unwrap_or(0)` 的值；**未**自动提取 free 的守卫、写槽等其余源码 |
| `source_release_generation_refines` | 任意成功模型 release 的 generation 与提取函数返回值投影一致 |
| `source_successor_no_revival` | 以提取返回值建立释放态，再沿任意**模型**序列，旧非零 generation 不再 Live；后续真实 Rust 整表操作尚无逐步细化定理 |

生成函数采用 Aeneas 的 Result/Option/U32，BitVec 范围不是未经约束的 Nat。5 条普适定理由 `Checks.lean` 审计，只有标准 `propext`、`Classical.choice`、`Quot.sound` 依赖。Rust→LLBC→Lean 翻译器、Rust core 的 Aeneas 模型和工具链仍在 TCB；没有证明它们自身正确或两个 Rust 编译器版本的输出等价。

从仓库根运行：

```sh
python3 scripts/check_handle_refinement.py
python3 scripts/check_handle_refinement.py --mutation-test
```

每次重新提取，并核对工具锁、生产文件/入口 hash 和生成文件字节，拒绝陈旧工件；随后构建细化证明与审计公理。`--regenerate` 仅供评审后的源码变化，更新工件后仍须证明。mutation 在临时 Rust 中改为 Some(1)，要求**重新生成后的 Lean 定理**失败，正式源码不变。

这是独立 Lake 包，使用 `toolchain.lock.json`/`lake-manifest.json` 固定依赖；安装与缓存准备见报告。原无外部依赖的 `lean/` 普通 `lake build` 不包含此包；默认脚本不下载工具或依赖。当前生产构建仍使用原 pinned Rust，抽取工具链单独安装。

本增量在上述两个方法/后继值上将证据推进至**受信任翻译边界下的源码级细化**；原 CDT、有限表模型的遗漏不因此消失。下一步仍需自动提取整表读写/选槽和检查过程，并建立真实 Rust 状态到 `KernelHandleTable` 的模拟关系。尚不覆盖编解码、整表所有操作、LLVM/二进制、并发或完整内核安全性。

## 2026-11-25 TLB 缓存映照切片

[BLK-001 计划](../docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md)把 `tlb_refine` 列为后续任务；
本次在 `Formal.Tlb` 建模 cosir `tlb_refine.rs` 的最小参考语义，未修改 Rust 源码或既有定理。

| Lean | cosir / Rust | 边界 |
|---|---|---|
| `TlbState = TaskId → VAddr → Option CachedXlat` | `TlbState.per_task : BTreeMap<TaskId, Tlb>` | 只表达点查询/更新；不建模迭代顺序或容量 |
| `CachedXlat.framePhys/accessRead/accessWrite/accessExec` | `CachedXlat.frame_phys/access` + `ACCESS_R/W/X` | 权限是三个独立布尔，不是 8 位掩码或 u64 ABI |
| `validAddr` | `validate_page_vaddr`（对齐 + canonical） | 抽象成任意守卫；不证明位运算、四级索引或 canonical 半区 |
| `taskExists` | `tasks` 与 `memory.tasks` 双重存在检查 | 合成一个守卫；不区分调度表与页表表项 |
| `pt : TaskId → VAddr → Option CachedXlat` | `PageTable` 四级树 + `translate` | 抽象页表；不建模 PML4/PDPT/PD/PT、帧所有权或 `check_invariants` |
| `translate` 命中/未命中/失败 | `tlb_refine::translate` | 未命中填充、未映射不填充、失败不改状态一致 |
| `invalidateVa` / `flushAll` / `removeTask` | `invalidate_va` / `flush_all` / `remove_task` | 守卫顺序与错误面一致；`remove_task` 无存在性前置 |
| `FreshFor` | （无显式类型；测试断言） | 新增证明谓词：缓存项与页表一致 |

31 条普适定理（非 `native_decide`），全部只依赖 `propext`/`Quot.sound`（`funext` 引入后者）：

- 点更新：`insert_same`/`insert_other`；失效/刷新/移除分别清除目标，其他 Task 与其他地址不变（`*_other_task`、`*_other_va`）。
- 幂等：重复失效、刷新、任务移除不再改变状态。
- 守卫与错误面：成功/非法地址/未知任务各返回预期结果；失败路径原样返回输入 TLB（`invalidate_*`、`flush_*`、`translate_*`）。
- 命中与填充：命中返回缓存值且不改状态；未命中读抽象页表并填充；未映射返回 `notMapped` 且不填充。
- **陈旧性边界（核心）**：`translate_after_invalidate`——在有效地址与存活任务下，单地址失效后的下一次翻译完全由当前页表决定，不复用旧缓存。
- **新鲜性保持**：`invalidate_fresh_for`、`flush_fresh_for`、`removeTask_fresh_for`、`translate_miss_fresh`——干净缓存经失效、刷新、移除和页表填充后仍与页表一致。
- 任务隔离：`insert_preserves_other_task`；`translate` 的唯一状态变化是 `insert`。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Tlb.lean
```

`Checks/Tlb.lean` 对全部 31 条定理打印公理依赖；本次只有 `propext`/`Quot.sound`，无 `sorryAx` 或 native 求值依赖。`#eval` 锚覆盖命中陈旧帧、失效后新帧、单地址/单任务作用域、刷新/移除清空、三类错误面与权限位矩阵。

边界：不覆盖四级页表与 canonical/对齐的位级证明、帧所有权与 `check_invariants`、容量与替换策略、真实 CR3/INVLPG/INVPCID、TLB shootdown、多核顺序或内存模型、硬件页表遍历与 SMP。
`tlb_refine_tests.rs` 的 Rust 测试是独立的 host 见证，不是自动抽取、双向差分或 Rust 源码 refinement proof。
本切片不改变阶段状态、7-B/7-H 勾选、profile 或既有 COW/Gate 边界。

## 2026-11-25 Memseg 双视图与帧归属切片

[BLK-001 计划](../docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md)把 `memseg_refine` 列为后续任务；
本次在 `Formal.Memseg` 建模 cosir `memseg_refine.rs` 的最小参考语义，未修改 Rust 源码或既有定理。

| Lean | cosir / Rust | 边界 |
|---|---|---|
| `TaskCtx.table : VAddr → Option Pte`、`vspace : VAddr → Option Mapping` | `TaskCtx.table` 四级树 + `vspace : BTreeMap` | 抽象为函数；不建模树结构与表帧归属检查 |
| `Pte`（present/user/writable/nx/frame/mapping） | `pagewalk::Pte` | 只保留 W7 检查的字段；帧由 `Segment.frames` 经 `nth?` 决定 |
| `Segment.frames/allowed` | `Segment.frames/allowed_access` | 顺序列表；不建模分配器顺序或清零 |
| `Access.valid/sub` | `access & !0x07 == 0`、`allowed & access == access` | 三个 Bool，不是 u8 位掩码 |
| `State.frameUniverse/free/owner` | `FrameAllocator.universe/free` + 段/表帧扫描 | W6 用单值 `owner` 函数表达唯一归属；`free ↔ owner = free` |
| `mapEntry/unmapEntry` | `map_verified/unmap_verified`（单页） | 单页原语；多页原子循环不在模型 |
| `createSegment/destroySegment` | `memseg_create/destroy_verified` | 帧集由调用方选择（过近似）；`StillMapped` 是调用方 Bool 守卫 |
| `destroyTask/spawnTask` | `destroy_task_memory/spawn_task` | 表帧一帧；不建模 `new_owned_task_table` 的分配失败 |

29 条普适定理（非 `native_decide`），只依赖 `propext`/`Quot.sound`：

- 双视图：`viewsAgree_map`/`viewsAgree_unmap` 同时更新表与视图后仍一致；`mapEntry_success_views`/`unmapEntry_success_views` 把该结论接到成功返回的新 Task。
- 权限：成功映射的 PTE 由 `makePte` 从段帧与 access 构造；`makePte_ok` 证明 present/user/W/NX、帧偏移与 `access ⊆ allowed`。
- W6 转移：`createSegment_owned`/`createSegment_other_frame` 证明选中帧变为 `Owner.segment sid` 且离开 free、其他帧不变；`destroySegment_freed`/`destroySegment_removed` 证明归还与段移除；`destroyTask_freed` 归还表帧；`spawnTask_ok` 把一帧从 free 转为 `Owner.table`。
- 守卫与原子性：`mapEntry_no_task`/`_invalid_access`/`_invalid_segment`/`_overlap`、`unmapEntry_entry`/`_not_mapped`、`createSegment_fail_*`、`destroySegment_still` 都原样返回输入状态；`mapEntry_ok`/`unmapEntry_ok`/`createSegment_ok`/`destroySegment_ok`/`destroyTask_ok` 钉住成功后的精确状态。
- entry 保护：`unmapEntry_entry` 先于 `notMapped` 拒绝 entry 页。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Memseg.lean
```

`Checks/Memseg.lean` 对 29 条定理打印公理依赖；只有 `propext`/`Quot.sound`，无 `sorryAx`/native。`#eval` 锚覆盖创建/销毁的 owner 转移、map 成功与五类错误、unmap 的 entry/未映射、spawn 表帧转移与任务销毁归还。

边界：不覆盖四级页表树与 `check_frame_ownership`、多页原子 map/unmap、地址对齐/canonical、`next_seg` 溢出、分配器顺序、帧清零、`StillMapped` 视图枚举（抽象为 guard）或真实 CR3 操作。
`memseg_refine_tests.rs` 的 Rust 测试是独立 host 见证，不是自动抽取、双向差分或 Rust 源码 refinement proof。
本切片不改变阶段状态、7-B/7-C/7-H 勾选、profile 或既有 COW/Gate 边界。

## 2026-11-25 Cow 组合切片

[BLK-001 计划](../docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md)把 `cow_refine` 列为后续任务；
本次在 `Formal.Cow` 建模 cosir `cow_refine.rs` 的最小参考语义，复用 `Formal.Memseg` 的 `Access`/`Segment`/`Mapping`/`setAt`，未修改 Rust 源码或既有定理。

| Lean | cosir / Rust | 边界 |
|---|---|---|
| `State.segments/tasks/current/pending/cows` | `CowSliceState.memory/ops/tlb/current/cow_mappings` | 只保留任务状态、当前任务、待处理故障和 COW 对；capability/订阅/TLB 省略 |
| `Task.status : ready/running/faulted` | `TaskStatus`（Ready/Running/Faulted） | 不建模 Dead/僵尸与 wait |
| `Mapping`（复用） | `PageMapping`/`SegmentRef` | 不含 backing/frame 冗余字段 |
| `establishCow` | `establish_cow` | 父 RW→R、子新增 R、COW 对追加；授权/第三方共享检查为前置 |
| `access` | `access` | 未映射→present=false；无权限→present=true；故障置 Faulted、清 current、记 pending |
| `privatize` | `privatize_cow` | 要求 Faulted、非 current、同 va 写故障、旧映射只读；换成私有段 RW 并回 Ready |
| `schedule`/`yieldTask`/`destroyTask`/`destroySegment` | 同名 | `stillMapped` 仍为调用方守卫 |
| `RunningOk`/`FaultedOk`/`CowPairOk` | `check_invariants` 的对应合取 | 帧级全不交检查只保留“映射帧不同” |

25 条普适定理（非 `native_decide`），只依赖 `propext`（两条无公理）：

- COW 建立：`establishCow_ok` 钉住精确状态，`establishCow_cows` 钉住追加的对，`readOnly_write` 证明新映射写位为 false；三类冲突/缺任务返回原状态。
- 访问：`access_read_allowed`（读只读页允许且状态不变）、`access_write_fault`（写只读页→present=true/writable=true、任务 Faulted、current 清空）、`access_unmapped_fault`（present=false）、`access_rejects_*`（非 current/无任务返回原状态）。
- 私有化：`privatize_ok` 钉住状态，`privatized_task` 证明任务回 Ready、pending 清除、COW 列表不变；`privatize_wrong_va`/`_not_faulted` 原状态失败。
- 调度：`schedule_ok`/`schedule_busy`/`yieldTask_ok`；销毁：`destroyTask_ok`/`_removed`/`_keeps_other`/`_rejects_faulted`、`destroySegment_still`/`_ok`。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Cow.lean
```

`Checks/Cow.lean` 对 25 条定理打印公理依赖；只有 `propext`（两条无公理），无 `sorryAx`/native。`#eval` 锚覆盖建立/冲突、读允许、写故障（present=true）、未映射故障、非当前拒绝、私有化回 Ready、调度忙/空闲、销毁任务/段守卫。

边界：不覆盖 capability 权限/持有者、异常源与订阅、SRET 恢复帧与 `exc_resume`、帧级全不交检查、TLB 失效、COW 对过滤的成员性质、Dead 任务/wait，或真实 CR3 操作。
`cow_refine` 的 Rust 测试（71 项）是独立 host 见证，不是自动抽取、双向差分或 Rust 源码 refinement proof。
本切片不改变阶段状态、7-B/7-C/7-D/7-H 勾选、profile 或既有 COW/Gate 边界。

## 2026-11-25 Revoke 撤销域切片

[Prima OS §6.2 实现注记](../Prima%20OS.md)给出 Bootstrap 宿主的撤销域定义（对应 COSIR §4.7 撤销边界）；
本次在 `Formal.Revoke` 建模其最小参考语义，复用 `Formal.State` 的 `Capability`/`Location`/`WellFormed`，未改 Rust。

| Lean | Bootstrap 宿主 / 说明 | 边界 |
|---|---|---|
| `Desc s root c` | 由 `parent` 指针可达（含 root），用于规范陈述 | 传递闭包；不含位置/消息节点 |
| `descB n s root c` / `descAt` | 可执行、燃料有界的 `Bool` 链遍历（`descAt` 用 `c + 1` 步） | 穷尽性依赖 `WellFormed` 的父 `<` 子 |
| `revoked s root` | 子树 token 置 Inactive、删 HeldBy、保留记录与 parent | `InMessage`/`InNamespace` 位置不动 |
| `revoke s root holder` | `revoke_capability`：仅接受 HeldBy(holder) 的根 | 非 HeldBy 根返回原状态 |
| `ActiveWellFormed` | 仅约束 active cap 的 CDT 不变量 | 撤销后父子同 inactive，原 `WellFormed` 不适用于该形状 |
| 原子性/幂等 | 单次操作全树生效；二次撤销无效果 | 并发/跨节点不在范围 |

`descB` 对任意燃料**健全**（`descB_sound`），在 `WellFormed` 下燃料 `c + 1` **完备**（`descB_complete`）；根始终自成员（`descB_self`，无需 `WellFormed`）。

25 条定理（4 条无公理，其余仅 `propext`/`Quot.sound`，**无 `Classical.choice`**）：

- 判定：`descB_self`/`descAt_self`（根自成员）、`descB_succ`/`descB_mono`（燃料单调）、`descB_sound`（健全）、`desc_ge_root`（后代 CapId ≥ 根）、`descB_complete`/`descAt_sound`/`descAt_complete`（`WellFormed` 下等价）。
- 子树：`desc_trans`（传递闭包）、`not_desc_parent`（根外节点的父也在根外）。
- 失效：`revoked_caps_desc`（子树 cap active=false）、`revoked_loc_desc_held`（HeldBy 删除）、`revoked_loc_desc_message`/`_namespace`（非 HeldBy 位置保留）。
- 隔离：`revoked_caps_outside`/`revoked_loc_outside`（域外不变）、`revoked_parent`（记录/parent 保留）。
- 操作：`revoke_fail`/`revoke_success`（守卫）、`revoked_loc_root`（根 HeldBy 删除，无需 `WellFormed`）、`revoke_idempotent`（二次失败且状态不变）。
- 不变量：`wellFormed_active`（原 `WellFormed` ⇒ `ActiveWellFormed`）、`revoke_preserves_active_wf`（撤销保持仅约束 active 的 CDT 不变量）。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Revoke.lean
```

`Checks/Revoke.lean` 打印 25 条定理的公理依赖；4 条无公理，其余仅 `propext`/`Quot.sound`，无 `Classical.choice`/`sorryAx`/native。`revoked`/`revoke` 为普通可执行定义，`#eval` 锚覆盖成功/失败守卫、根与子项失效、HeldBy 删除、`InMessage` 保留、域外不变与二次撤销失败。**子树定理在 `WellFormed` 下陈述**（燃料 `c + 1` 的完备性依赖父 CapId 严格递减）；该前提在 `revoked_loc_root`/`revoke_*`/`revoked_parent` 与表外隔离性质中不需要。

边界：不覆盖并发/移动竞争、跨节点 `InTransit`、在途消息/命名空间的收回、审计容量预演与事件、对象销毁回收、`cap_drop` 的完整授权语义，或 Rust 源码细化。
Bootstrap `revoke_capability` 的 Rust 证据（SPEC-022/023）与 host 测试是独立实现证据，不是本模型的自动对拍。
本切片不改变阶段状态、7-D 或其它轨道的退出勾选。

## 2026-11-25 Drop 切片

COSIR §4.5 的 `cap_drop`：删除调用者单个 `HeldBy` 位置，不传播、不删记录、
structural 根受保护。复用 `Formal.TokenLocation` 的 `Snapshot`/`dropToken`，只加
规范层性质，未改 Rust。

| Lean | cosir / Rust | 边界 |
|---|---|---|
| `drop s caller c` | `cap_drop` / `TokenLocation::dropToken` | 单位置；订阅终止见 `Formal.Subscription` 节，不含审计事件 |
| `drop_clears` | `locations c := none` | 只清目标 |
| `drop_does_not_propagate` | §4.5“丢弃父不撤销已派生子项” | 子位置/记录保留 |
| `drop_structural_fails` | structural 根保护 | — |
| `drop_preserves_wellformed` | caps 不变 | CDT parent 结构不变 |

10 条定理（公理仅 `propext`/`Quot.sound`）：structural/错误持有者守卫、清位置、
保其他位置与记录、不传播、父子独立、CDT 保持、Consistent 保持、二次 drop 失败。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Drop.lean
```

`#eval` 锚覆盖：派生子项 drop 成功、structural 根与错误持有者拒绝、目标位置清空、
父记录保留、子记录保留。`Checks/Drop.lean` 显示 10 条定理公理仅 `propext`/`Quot.sound`。

边界：不覆盖审计事件、`cap_drop` 的跨位置移动竞争，也不与 `Formal.Revoke`
的实现同构（Revoke 是子树失效，Drop 是单位置；规范明确二者正交）。

## 2026-11-25 Move 位置迁移切片

COSIR §4.6 规定 Capability 只能在列出的原子操作内改变位置，且每次变化记录原位置
与新位置（删除为 `None`，直投记录 `MsgId`）。本次在 `Formal.Move` 建模该转移表，
复用 `Formal.TokenLocation` 的 `Snapshot`/`Consistent`/`replace`，未改 Rust。

| Lean | §4.6 / cosir | 边界 |
|---|---|---|
| `queuedSend` / `directSend` / `recvDeliver` | 排队 send、直投 send、邮箱 recv | 位置三元组抽成 `Transition` |
| `taskCreate` / `nsBind` / `nsUnbind` | 注入、绑定、解绑 | 不含派生/路径解析 |
| `deletion src` | `cap_drop`/`task_destroy`/`ns_unmount`/容器或消息销毁 | `dst = none` |
| `applyMove s c t` | 单位置原子更新 | 源不匹配返回 `none` |
| `Transition {src, dst, msgId}` | 原/新位置与直投 `MsgId` 记录 | 事件即转移记录 |
| `crossNodeSend` | 跨节点 send v1 不移动 | 非空 `transferred` 失败 |

15 条定理（7 条无公理，其余仅 `propext`）：

- 操作：`applyMove_requires_source`（源守卫）、`applyMove_dest`（新位置）、`applyMove_event`（事件即转移）、`applyMove_keeps_caps`（记录不变）。
- 一致性：`applyMove_preserves_consistency`、`carriers_after_move`（carrier 视图恰在新位置；删除时无 carrier）。
- 唯一性：`w3_unique_location`（函数式 `locations` 至多一个位置）、`carriers_after_move`。
- 转移表：`queuedSend_dest`、`directSend_msgId`、`recvDeliver_msgId`、`nsBind_dest`、`deletion_sink`。
- 跨节点：`crossNodeSend_no_move`（位置不变）、`crossNodeSend_empty_ok`、`crossNodeSend_nonportable_fails`（非空 `transferred` 失败）。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Move.lean
```

`#eval` 锚覆盖：排队 send 成功/源不匹配失败、新位置与 carrier 同步、直投 `MsgId`、
recv 交付、删除位置与 carrier 清除、`CapRecord` 不变、跨节点非空失败且位置不变、
空 `transferred` 成功。

边界：`Formal.Location` 在 v1 不含 `InTransit`（§4.6/W3），模型由构造排除了该位置；
W4 容器双向一致随后由 2026-11-25 `Formal.Container` 节补上，订阅终止见 `Formal.Subscription` 节；审计事件仍不覆盖。
`applyMove` 的原子性由单步函数表示，没有时序/并发模型中“中间不可观察”的形式化。

## 2026-11-25 Container 切片（W4）

COSIR W4 要求位置与容器双向一致：`HeldBy(t)` 要求 Task `t` 活动；`InMessage(m)`
当且仅当活动 Message `m` 的 caps 含该 token；`InNamespace(n,p)` 当且仅当活动
Namespace `n` 的条目 `p` 引用该 token。本次在 `Formal.Container` 建模容器与
`locations` 的双向义务，并证明 §4.6 的四个耦合更新保持 W3+W4，未改 Rust。

| Lean | W4 / §4.6 | 边界 |
|---|---|---|
| `Task`/`Message`/`Namespace` | 容器对象与活动位 | caps/entries 用函数表示 |
| `W4Ok` | 三条双向子句（Task 活动性、消息 caps、Namespace entries） | 含容器活动性 |
| `WorldOk` | `Consistent`（W3）+ `W4Ok` | carrier 与容器联合不变量 |
| `queuedSend` | `HeldBy → InMessage` 且消息 caps 加入 token | 需活动消息 |
| `recvFrom` | `InMessage → HeldBy(receiver)` 且移除 caps | 需活动接收 Task |
| `bindNs` / `unbindNs` | `HeldBy ↔ InNamespace`，路径占用则失败 | 需活动 Task/Namespace |
| `emptyWorld_ok` | 空世界满足 W3+W4 | 非空锚由 `#eval` 场景覆盖 |

18 条定理（1 条无公理，其余仅 `propext`/`Quot.sound`）：

- 更新投影：`replace_loc_self`/`_other`、`messageAdd_self`/`_other`、`messageRemove_self`/`_other`、`namespaceSet_self`/`_other`、`setMessage_self`/`_other`、`setNamespace_self`/`_other`。
- 构造：`inNamespace_inj`（条目蕴含 n/p 相等）。
- 保持：`queuedSend_preserves`、`recvFrom_preserves`、`bindNs_preserves`、`unbindNs_preserves`（位置与容器同步更新下 W3+W4 保持）。
- 锚：`emptyWorld_ok`。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Container.lean
```

`#eval` 锚覆盖：send 成功/源或消息不匹配失败、位置与消息 caps 同步、recv 移除
caps 且新 `HeldBy` 要求活动 Task、bind 同步 Namespace 条目、bind→unbind 回到
调用者、无此 Namespace 失败、Task 容器不变。

边界：消息 caps 与 Namespace entries 用函数表示，因此“消息列表无重复”与“同一路径
至多一个 Capability”由构造满足，未对 `List` 版本证明；消息/Namespace 对象生命周期、
容器销毁、审计事件与 Rust 细化不覆盖；订阅终止见 `Formal.Subscription` 节。`WorldOk` 不含 `Formal.State` 的
CDT 不变量，需与 `WellFormed` 独立组合。

## 2026-11-25 Subscription 切片（§12.4 / W15）

订阅是异常源的内部状态：每个 Task 一个 bit（因此“每 Task 每源至多一个活跃订阅”
由表示保证）。W15 要求订阅者是有活动 Task，且持有至少一个指向该源、可用、含
`ExcSubscribe` 的 Capability；失去最后一个此类 Capability 时订阅同终止。本次在
`Formal.Subscription` 以关系模型（携带源/Capability 见证）建模生命周期，未改 Rust。

| Lean | §12.4 / W15 | 边界 |
|---|---|---|
| `Source {active, subscribers}` | 订阅位 + 源活动性 | 每 Task 一 bit |
| `UsableFor w holder source capId` | 持有 + active + right + 指向源 | `ExcSubscribe` bit 为模型本地 0x80 |
| `W15` | 订阅者活动 Task + usable 见证 | — |
| `SubscribeStep` / `UnsubscribeStep` | subscribe/unsubscribe 一步 | 关系模型，无执行函数 |
| `DropLastStep` / `SendLastStep` | 失去最后授权 Cap 时位置与订阅同步终止 | “最后”为调用方前置，未证明检查器 |
| `DestroySourceStep` | 源销毁移除全部订阅 | 通知投递不建模 |

24 条定理（2 条无公理，其余仅 `propext`/`Quot.sound`）：

- 更新投影：`setSource_self`/`_other`、`addSubscriber_self`/`_other`/`_active`、`clearSubscriber_self`/`_other`/`_active`、`endSub_self`/`_other`、`endSubCap_excSource`。
- 见证迁移：`usableFor_snap`（同一源的见证跨位置删除保持）、`usableFor_snap_of_ne`（其他源）、`w15_active`/`w15_witness`。
- 生命周期：`SubscribeStep_adds`/`_preserves`、`UnsubscribeStep_clears`/`_preserves`、`DropLastStep_clears`/`_preserves`、`SendLastStep_clears`/`_preserves`、`DestroySourceStep_removes`/`_preserves`。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Subscription.lean
```

本切片**没有 `#eval` 锚**：步骤以关系表述，携带不可执行的见证与前置条件；证据为
`lake build` 与公理审计。

边界：不证明实现层“最后一个授权 Cap”检查器的正确性；不建模订阅在仍有其他授权
Cap 时的存续活性（liveness）、审计事件（`ExcSubscribed`/`ExcUnsubscribed`）、源
kind/`KindMismatch`、通知投递、源销毁通知、并发、或 Rust 细化。`SubWorld` 的
`W15` 与 `Container.WorldOk`/CDT `WellFormed` 为独立不变量，需另行组合。

## 2026-11-25 Audit 切片（§13.1 / W13）

§13.1 要求每次成功状态修改追加带唯一递增 `EventId` 的事件，信封为
`{id, at, actor, kind}`；位置变化记录旧/新位置（删除为 `None`），历史只追加。
本次在 `Formal.Audit` 建模日志核心与受审计操作，未改 Rust。

| Lean | §13.1 / W13 | 边界 |
|---|---|---|
| `Event {id, when, actor, kind}` | 事件信封 | 事件类为最小子集 |
| `Log {first, entries}` | 追加式日志；`id = first + i` | `next_event = first + length` |
| `ids_strict` | W13 严格递增（由表示保证） | 非不变量维护 |
| `capHistory` | Capability 位置历史（`CapMoved`/`CapDropped`） | 只按 CapId 过滤 |
| `created_at`（`State.withCreated`） | 铸造事件 ID | 派生/跨节点来源不含 |
| `auditedMove` / `auditedDrop` / `auditedMint` | 位置/创建事件的一步追加 | 不含载荷完整校验 |
| `auditedTaskState` / `auditedObjectCreate/Destroy` | Task 迁移与对象创建事件 | 通知/审计查询不建模 |

19 条定理（2 条无公理，其余仅 `propext`，`ids_strict` 另含 `Quot.sound`）：

- 日志：`append_entries`（追加式）、`append_prefix`（旧日志为前缀）、`append_nextEvent`、`append_event_old`（旧事件不变）、`ids_strict`。
- 位置历史：`capHistory_append_moved`/`_dropped`（历史追加一条）、`capHistory_append_moved_other`（其他 Cap 不变）。
- 受审计操作：`auditedMove_spec`（位置+日志+历史+created/objects 不变）、`auditedDrop_spec`、`auditedMint_created`/`_other_created`/`_event`、`auditedTaskState_event`/`_keeps_world`、`auditedObjectCreate_event`/`_live`、`auditedObjectDestroy_event`/`_dead`。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/Audit.lean
```

本切片无 `#eval` 锚（日志以列表与函数记录表示，未选执行入口）；证据为
`lake build` 与公理审计。

边界：EventId 由日志位置推导，故 W13 严格递增是表示性质而非跨实现的分配器证明；
事件载荷只覆盖已用到的字段，未逐类断言 §13.1 的授权 Cap/权限/目标对象；`audit`
（§13.2）的父链完备性见 `Formal.AuditQuery` 节；内容摘要、时钟推进、跨节点
`transport_msg`、拒绝事件与 Rust 细化均不覆盖。审计日志与
`Container.WorldOk`/`Subscription.W15`/CDT `WellFormed` 为独立不变量。

## 2026-11-25 AuditQuery 切片（§13.2）

§13.2 `audit(cap)` 是纯查询：返回 root→cap 的 parent 链、链上各 Capability 的
铸造/派生事件、该 Capability 的位置历史、目标对象创建/销毁事件；目标对象已销毁
不构成失败。本次在 `Formal.AuditQuery` 建模查询与错误语义，未改 Rust。

| Lean | §13.2 | 边界 |
|---|---|---|
| `parentChain fuel w c` | 燃料有界的 parent 行走（含记录） | `WellFormed` 下燃料 `c+1` 完备 |
| `auditChain s capId` | 反转后的 root→cap 链 | 长度 ≤ `capId + 1` |
| `audit s caller capId` | 返回 `Except AuditError AuditTrace` | 纯查询，无状态输出 |
| `AuditError.invalidCap`/`notHeld` | 未知 CapId vs 未被调用者持有 | 目标可已销毁 |
| `AuditTrace.creations` | 链上每个 Cap 的 `created_at` | 派生事件的完整载荷不含 |
| `AuditTrace.positions` | `capHistory` 原样 | 与 §4.6 事件对齐 |
| `AuditTrace.objectHistory` | 目标对象的创建/销毁事件 | 按对象 ID 过滤 |

20 条定理（全部无 `sorryAx`，仅 `propext`/`Quot.sound`）：

- 父链：`parentChain_head`、`parentChain_sound`（成员都有 CapRecord）、`parentChain_length_le`（长度 ≤ 燃料）、`auditChain_sound`、`auditChain_length_le`（≤ `capId + 1`）、`auditChain_last`（末端为查询 CapId）。
- 完备性：`lastParentNone_getLast`、`lastParentNone_cons_of_tail`、`parentChain_nonempty`、`parentChain_succ_of_done`（完成的链不受额外燃料影响）、`parentChain_done_add`、`parentChain_complete`（`WellFormed` + 燃料 `c+1` ⇒ 末项无父）、`auditChain_root_parent_none`（链首/根无父）。
- 错误/成功：`audit_invalid`（无记录 → `InvalidCap`）、`audit_not_held`/`audit_not_held_other`（未由调用者 HeldBy → `NotHeld`）、`audit_ok_held`（成功 ⇒ 记录存在且 `HeldBy(caller)`）。
- 结果：`audit_ok_fields`（链、created_at、位置历史、对象事件与模型定义一致）、`audit_dead_target_ok`（目标已销毁仍成功）、`audit_ok_creation_mem`（created 列表项对应链成员与 `created_at`）。

复现：

```sh
cd lean
elan run leanprover/lean4:v4.35.0 lake build
elan run leanprover/lean4:v4.35.0 lake env lean Checks/AuditQuery.lean
```

本切片无 `#eval` 锚；证据为 `lake build` 与公理审计。

边界：父链完备性以 `WellFormed`（W2：父 `<` 子）为前提；非良构状态、`origin`/跨节点
代理根、内容摘要、完整 §13.1 事件载荷、时钟推进、拒绝事件与 Rust 细化不覆盖；查询
结果与 `WorldOk`/`W15` 为独立模型，未证明联合查询正确性。
