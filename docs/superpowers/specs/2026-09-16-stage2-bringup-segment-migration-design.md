# 切片 R 设计：bring-up 13 区域迁移为真实 Segment 对象

> 文档类型：真实内核实现设计（阶段 2 收口 `ISSUE-AUTH-002`）
>
> 日期：2026-09-16
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-16，四项关键决策逐项确认）

## 0. 定位与前置

切片 Q（SPEC-062）把句柄编码、Capability 生命周期与 `memseg_*` 原语做成了用户可调的真实
内核路径，但**平台 bring-up 的 13 个区域仍只是帧归属标签**：`frames::SEG_VSPACE=1` ..
`SEG_SPIN2_DATA=13` + 帧计数，既没有 `SegmentObject`，也没有 `creator`、vspace 记账或
Capability。为避开这些标签，用户 Segment 被迫使用独立帧标签命名空间
（`frame_label(id) = 100 + id`）。

本切片按 [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md) §5.1（Segment 对象）、§4.3（位置模型）、§7.1（`memseg_create`
的取帧/清零/铸根 cap）、§16 W6/W7 落地迁移，闭合 `ISSUE-AUTH-002`。

### 0.1 已确认的关键决策（用户逐项确认）

1. **范围：全 13 区域一次迁完**（含 task2/spin 的 7 个区域）。
2. **映射路径**：11 个用户可见区域（task1 code/stack/X/Y、task2 code/stack/Y、spin1/2 的
   code/data）经 `memseg_map` 授权中介装入任务根；仅 `SEG_VSPACE`（需 `set_leaf` 换叶重映射
   见证）与 `SEG_COW`（fault-time 换映射、内核私有页）保留**内核直映射**并登记为边界。
3. **见证**：新增独立事件 `platform_segments_ready`（seq 25），
   `AuthorityReady`/`FramesReady`/`BootComplete` 顺移至 26/27/28。
4. **根 Capability 持有者：新增 `CapabilityLocation::HeldByKernel`**。既不复用
   `HeldByTask(KERNEL_ACTOR)` 假造一个不存在的 Task，也不弱化
   `validate_authority` 的持有者存活检查——内核持有一个**独立的位置类**，其存活性由定义保证。

### 0.2 动手前核对到的实现缺口

| 项 | 规范要求 | 改前现状 |
|---|---|---|
| 平台区域的对象性 | §5.1 每个 Segment 有 id/creator/pool/frames/allowed_access/active | 只有 `frames.rs` 标签 + 计数；`BootstrapState.segments` 里没有对应条目 |
| 平台区域的 Capability | §4.2 随对象创建铸根 Cap，可被 `authorize` 查询 | 无任何 cap；`ISSUE-AUTH-002` |
| 帧归属标签 | W6 逐帧归属可查询 | label（1..13）与 Segment id 无关联；用户段被迫用 `100+id` 规避 |
| 逻辑 vspace 记账 | §7.2 `TaskVspaceTable` 记录映射视图 | 平台区域只有真实 PTE，无逻辑条目（W7 无法判定） |
| COW 段帧数 | §5.1 `Vector<FrameId>` | fault-time 私有帧只标注签、不进对象 |
| Task 2 的 vspace 能力 | §7.2 需要 `TaskManageVSpace` | `platform_init` 已 `create_task(2)`，但没有指向它的 Task Capability |

## 1. 设计

### 1.1 帧标签统一（`frames.rs`）

平台区域的归属标签**就是** Segment 槽位 id：`SEG_VSPACE=2` .. `SEG_SPIN2_DATA=14`
（id 1 保留给 image，其帧计入 `image` 类，从不在注册表 open）。`frame_label` 与
`USER_SEGMENT_FRAME_LABEL_BASE` 删除，调用点直接传 segment id。用户经 `memseg_create`
创建的 Segment 从 `USER_SEGMENT_ID_FIRST = 15` 起分配。

### 1.2 位置模型：`CapabilityLocation::HeldByKernel`

```rust
pub enum CapabilityLocation { HeldByTask(u32), HeldByKernel, InMessage(u32) }
```

- `holder_id()`：`HeldByKernel → Some(KERNEL_ACTOR)`，故 `authorize`/`drop_capability`
  现有的「`holder_of(cap) == Some(caller)`」比较对内核调用者（`KERNEL_ACTOR = 0`）无需特判。
- `is_held_by(holder)`：统一 `HeldByTask`/`HeldByKernel` 的持有判定，替换原先散落的
  `location != HeldByTask(h)` 比较（`move_to_task`/`drop_from_task`/`move_to_message`）。
- `validate_authority` 的 `HeldByKernel` 分支**不做 Task 存活检查**——这不是对检查的豁免，
  而是该位置类的语义：持有者不是 `TaskTable` 条目，其存活由定义保证。`HeldByTask` 分支的
  检查一字未改。
- `CapabilityPositionTable::register_location` 是新的登记入口，`register` 成为其薄封装。

### 1.3 显式槽位的根 Capability

平台根 cap 必须铸在**预留区间**（32..46），不能用 `slots.alloc()`（最低空闲）：后者会占用
槽 6/7，而 ring3 见证程序把 `CAP_CHILD*`/`MEMSEG_CAP*` 句柄作为立即数内嵌在机器码里，
`handle_ready`（1..5）、`cap_lifecycle_ready`（槽 6 的两代）、`memseg_ready`（槽 6/7）
都依赖这些槽号。

新增 `mint_root_capability_at(holder, slot, rights, target)`：显式槽、槽位占用/越界即失败、
**不重置 generation**（`reserve` 改为保留既有 generation，避免旧句柄复活）。同时
`CapabilitySlotTable::slot_free` 提供前置判定。

### 1.4 公共构造核

`memseg_create` 的主体抽成 `BootstrapState::create_segment_object(id, creator, page_count,
allowed_access, phys_offset, cap_slot)`：取帧 → 逐帧清零（S2-X3）→ 建对象 → 铸根 cap →
`ObjectCreated`，失败原子回滚（归还已取帧、关闭归属标签）。

- `memseg_create` 保留原有校验顺序与语义，取 `alloc_segment_slot()` 后以 `cap_slot = None`
  调用构造核。
- `platform_create_segment(id, ...)` 是平台入口：id 固定为 2..14、**允许空 `allowed_access`**
  （VSPACE/COW 是内核私有页）、以 `cap_slot = Some(...)` 铸内核持有的根 cap。

### 1.5 平台入口与调用点

| 入口 | 作用 |
|---|---|
| `platform_create_segment` | 建平台 Segment 对象 + 铸内核持有的根 cap |
| `platform_segment_frame(id, index)` | 取第 `index` 帧（直映射与内容初始化用） |
| `platform_append_frame(id)` | 为内核私有段追加一帧（COW fault-time 私有页），仍经唯一取帧入口 |
| `platform_prepare_task_caps` | 铸 `Task(1)`/`Task(2)` 的 `TaskManageVSpace` cap（内核持有），`main.rs` 在 `platform_init_exception_sources()` 之后调用 |
| `platform_segments_record` | 按需组装阶段末见证（不落存储字段） |

调用点**顺序与位置不变**（`main.rs` 158/169/174/188/194/200），且每个区域的数据帧仍是
「全部取完 + 内容写好」之后才由 `memseg_map` 取页表帧——保持与切片 Q 逐字节相同的物理帧序列。

### 1.6 见证事件

`platform_segments_ready`（seq 25，`resume.rs::after_resume`，在 `authority_ready` 之前）：
`segments/first_id/last_id/roots/task_caps/mapped/direct/direct_ids/vspace/holder/slot_base/
task1_cap/task2_cap/caps×13/frames×13/unified`。其中 `unified` 由**逐帧查
`frames::owner_of(frame) == FrameOwner::Segment(id)`** 得出，即「标签与 id 恒等」是被
真实查询验证过的断言，而不是构造性声明。

## 2. 显式边界（不夸大）

- **VSPACE/COW 保留内核直映射**：它们的 `allowed_access` 为空、`TaskVspaceTable` 视图为空，
  故 `memseg_destroy` 的 `StillMapped` 检查看不到它们，W7 的「逻辑视图 vs 真实 PTE」不交性
  在这两个区域上未建模。
- **COW 私有 X 帧的归属分叉**：`USER_X_VA` 的 vspace 条目仍指向 `SEG_TASK1_X` 的共享帧，
  而 COW 换映射后的私有帧归属 `SEG_COW`。真实逻辑/物理分叉，如实登记；不试图改写
  `TaskVspaceTable`（那会替换共享映射并破坏 `cow_ready.shared0`）。
- **`unified` 无 host 覆盖**：host 测试未安装全局帧注册表，该字段的权威证据只有 QEMU。
- **cap 持有者是内核 actor**：平台段与两条 task cap 目前由 `KERNEL_ACTOR` 持有，
  init 的初始 Authority 严格保持 3 条；**用户可达的移交属于 S2-D4/D5**，本切片不宣称。
- **机制偏差**：平台区域经 `platform_create_segment`（与 `memseg_create` 共用构造核）创建，
  而不是 `memseg_create` 入口本身——后者拒绝空访问集的内核私有区域，并按最低空闲槽分配 id。
- 未实现：S2-D4、S2-D5、`REFINEMENT-PROOF`、`dev_vspace`/W16.4。
- **不勾选任何阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 3. 测试面

- host：`frames::SEG_*` 布局与 cap 槽位区间不交、`mint_root_capability_at`（显式槽/gen1/
  内核持有/槽占用与越界拒绝/`validate_authority` 通过）、`platform_segments_record`
  的对象/cap/vspace 计数、`alloc_segment_slot` 起点、`platform_create_segment` 的前置校验、
  `BootstrapState` 结构体大小守卫。
- QEMU：28 条事件按新序列断言；新校验器钉住 13/2/11/2/11/0/32、13 条 cap 句柄、帧数序列与
  `unified=1`；负例（未统一标签、槽位漂移、帧数不符、直映射边界不符）。
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo fmt --all --check`、`cargo xtask test`、`cargo xtask compat`。
