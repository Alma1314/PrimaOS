# 切片 Q 设计：阶段 2 启动——句柄编码、Capability 生命周期、Segment 原语

> 文档类型：真实内核实现设计（阶段 2 交付 S2-D1/D2/D3）
>
> 日期：2026-09-16
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-16）

## 0. 定位与前置

阶段 2 未开始；阶段 1 已把归属基础铺好（`FrameRegistry` 是 W6 唯一取帧点，image Segment
物化，初始 Authority 为 3 条根 cap）。本切片按 [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md) §3（ID 不可伪造表示）、
§18 第 1 条（索引 + generation）、§4.1~§4.6（权限、CapRecord、位置、派生/丢弃、移动边界）、
§7.1~§7.4（`memseg_create/map/unmap/destroy`）与 §16 W6/W7 落地 S2-D1/D2/D3。

### 0.1 动手前核对到的实现缺口

| 项 | 规范要求 | 改前现状 |
|---|---|---|
| opaque ID 表示 | §18 第 1 条：索引 + generation 或受保护句柄 | `CapId` 是裸 `u32`；派生 id **由调用者给定**，无 generation、无槽位复用 |
| `cap_derive` | §4.4：内核分配新 CapId、父权限子集、类型匹配 | 仅 host 模型函数，无用户 syscall |
| `cap_drop` | §4.5：删位置、Audit、结构性根保护、stale token 可丢 | 无；`positions.drop_from_task` 无根保护 |
| stale token | §4.3：目标销毁后不可授权/派生/发送，但可 drop/查询 | 无 `TargetDead` 区分 |
| `memseg_*` | §7.1~§7.4：取帧/清零/映射/卸载/销毁归还 | 无任何用户可见原语 |
| 帧归还 | §7.4：销毁归还来源池，W6 守恒仍成立 | 分配器有 `release`，但 `FrameRegistry` 只增不减、无 owner 映射 |
| unmap | §7.3：清叶 + 回收空表结点 | `vspace` 只有 `map_4k`/`set_leaf`/`read_leaf`，**无清叶/回收** |

### 0.2 已确认的关键决策（用户逐项确认）

1. **严格句柄编码**：`Handle = (generation << 32) | index`，generation 从 1 起、index/generation
   为 0 一律非法。既有事件的 cap 数值**允许改值但逐条显式登记**（见 §4）。
2. **「移动」本轮只做到模型 + Audit**：位置迁移 + `CapabilityMoved` + generation 感知 + host
   测试；不新增 send/recv/task_create 用户原语。规范 §4.6 明确无用户可调 `move`，用户可见移动
   属 S2-D4/D5 的外层操作。
3. **一次交付 D1+D2+D3**。
4. **不改变既有帧分配顺序**；阶段 2 的取帧/页表帧在验收流程内净零回收，`frames_ready` 守恒与
   `segment=16`/`table=24` 不变（`image` 随内核镜像大小变化）。

## 1. S2-D1：不可伪造且不复用的句柄

新增 `kernel/src/handle.rs`：

- `Handle { index: u32, generation: u32 }`，`encode/decode` 严格
  `(generation << 32) | index`；`index == 0 || generation == 0` → `Malformed`；
  `Handle::ZERO` 表示「无句柄」（如 Task 未设结构性根）。
- `next_generation()` 在槽释放时递增且不回绕到 0。

`bootstrap.rs` 引入 `CapabilitySlotTable`（槽位 + generation 的分配权威）与逐槽索引的
`CapabilityPositionTable` / `CapabilityRecordTable`：

- 槽 1/2/3 = 初始根（gen1），槽 4/5 = 预定义异常源（gen1），其后为铸造根与派生 Capability。
- `alloc()` 取最小空闲槽；`free()` 置空闲并递增 generation → 旧句柄永久不再匹配。
- `check(handle)` 区分 `Free` / `Stale`（槽已复用、generation 落后）/ `Live`。
- `ObjectRef { Segment(u32), Task(u32), Namespace(u32), ExcSource(u32) }` 承载目标类型与
  `live(target)` 判定；`CapabilityRecord { handle, parent: Option<Handle>, holder, rights, target }`
  同时表达「随对象创建铸造的根」（`parent = None`，§4.2）与「派生」（`parent = Some`）。

**§4.2 的显式偏离**：handle.index 是物理槽而非 §3 的逻辑单调 `CapId`，故
`parent CapId < child CapId` 的字面序不成立。无环性不依赖索引序，而由构造保证：派生时父必须
存活且被调用者持有、子槽从空闲表新分配（因此不可能是任何活动 Capability 祖先的槽），并以
`parent` 链显式记录。

## 2. S2-D2：Capability 生命周期用户化

新增 syscall：

- `SYS_CAP_DERIVE(11)`：`args[0]=parent handle`、`args[1]=rights bitset` → 子句柄。
  前置（§4.4）：调用者持有父、父 `usable`、父含 `Derive`、`requested ⊆ parent.rights`、
  `well_typed(target_kind, rights)`。
- `SYS_CAP_DROP(12)`：删位置、释放槽（generation 递增）、追加 `CapabilityDropped`；
  **不检查目标存活**（stale token 可丢弃）；活动 Task 的结构性根 Namespace cap → `StructuralCap`。

错误与 errno：解码非法 → `EINVAL`；槽位从未分配 / generation 落后 → `ENOENT`（对用户不可区分，
避免泄露槽位复用信息）；非持有者 / 缺权限 / 目标已销毁 → `EACCES`。

`SYS_AUTHORIZE` 改为接受并返回句柄，检查顺序为
槽位（Free/Stale）→ 目标存活（`TargetDead`）→ 持有者 → 权限，故 **stale 句柄在槽位阶段即被拒**，
不会因槽位复用重获 Authority（S2-X4）。

移动（§4.6）保持为模型能力：`move_capability` 只改位置、句柄与 generation 不变，追加
`CapabilityMoved`；无用户可调 `move` syscall。

## 3. S2-D3：Segment 原语与帧归还/回收

- `frames.rs`：新增逐帧 owner 表（`(frame, label)`）与 `owner_of(frame)`、
  `release_frame`、`release_table_frame`、`close_segment`、`check_disjoint`。
  守恒式仍为 `free + segment + table + image == universe`；`check_disjoint` 额外判定
  owner 列表两两不交且不与 image 区间相交（ISSUE-FRAME-002）。
- `mem.rs`：新增 `zero_frame(phys, offset)`（S2-X3）。
- `vspace.rs`：新增 `unmap_4k`（清叶 + 自底向上回收变空的、由本内核分配的表帧）与
  `VspaceError::NotMapped`。
- `bootstrap.rs`：
  - `SegmentObject { id, generation, creator, pool, frames: SegmentFrames, allowed_access, active }`；
    `SegmentFrames::Vector` 是 `memseg_create` 新建 Segment 的**真实帧向量**（ISSUE-FRAME-001），
    image 仍用 `Contiguous { base, count }`。
  - `TaskVspaceTable`：每 Task 的逻辑映射视图（`overlap` / `StillMapped` / W7）。
  - `memseg_create(page_count, allowed_access)`：取帧 → **逐帧清零** → 建对象 → 铸根 cap
    （`Derive | SegDestroy | SegMap{access}`）→ `ObjectCreated`。
  - `memseg_map`：`SegMap*` + `TaskManageVSpace` 双授权 → 边界/重叠 → 装真实 PTE + 逻辑视图。
  - `memseg_unmap`：`TaskManageVSpace` → 每页须已映射、区间不含 Task `entry` 页 → 清叶 + 回收。
  - `memseg_destroy`：`SegDestroy` → 无活动映射（`StillMapped`）→ 归还全部帧 + `close_segment`
    → `ObjectDestroyed`；指向它的 cap 变 stale token（`TargetDead`）。
  - 新 syscall `SYS_MEMSEG_{CREATE=13,MAP=14,UNMAP=15,DESTROY=16}`。

## 4. 见证事件与证据改动登记

新增三条事件（插在 `exc_wait_ready`(19) 之后）：`handle_ready`(20)、`cap_lifecycle_ready`(21)、
`memseg_ready`(22)，其后既有事件 seq 顺移（`preempt_ready` 20→23 … `boot_complete` 24→27）。

| 事件/回执 | 变化 |
|---|---|
| `bootstrap_ready` | `image_cap/namespace_cap/task_cap` 1/2/3 → `0x100000001/2/3` |
| `syscall_auth_ready` | `granted_ret` 1 → `0x100000001`；`unknown_ret` 仍 `ENOENT`（未分配槽句柄） |
| `authority_ready` | `root_ns` 2 → `0x100000002` |
| `frames_ready` | 新增 `disjoint=1`（其余字段不变：`conserved=1`、`segment=16`、`table=24`） |
| `exc_ready` | 不变（返回 source_id 1/2，非 cap） |
| 新事件 | `preempt_ready` 起 seq 20..24 → 23..27 |
| `user_code` | 长度 622 → 1478；末尾 int3 621 → 1477；新增回执偏移 Y+168..Y+328 |
| 启动栈 | bootloader 默认 80KiB → 256KiB（`BootstrapState` 扩容后组合栈帧超限，QEMU 实测三重故障；见报告） |

## 5. 证据边界（不夸大）

- **bring-up 13 区域未迁移**为真实 Segment 对象：本轮 `memseg_*` 只服务新建 Segment；
  `ISSUE-AUTH-002` 保持 Open（与计划的一处偏差，报告中显式登记）。
- image Segment 仍是 `(base, count)` 区间表达，不是 `Vector<FrameId>`。
- `SegMapX` 只记录在权限与逻辑视图，**未做 NX 强制**（无 EFER.NXE 配置）。
- `memseg_map/unmap` 只在调用者自身 Task 上实证（无跨 Task `TaskManageVSpace` 场景）。
- 用户 Segment 的帧归属标签暂用独立命名空间（`100 + id`），待 bring-up 迁移统一。
- 无 host 单测覆盖 `memseg_map/unmap` 的真实页表路径（需真实 CR3），仅 QEMU 证据；
  `memseg_create` 的授权/容量分支由 host 单测与 QEMU 共同覆盖。
- 未实现：S2-D4（Task create/yield/destroy）、S2-D5（send/recv）、`dev_vspace`/W16.4、
  `REFINEMENT-PROOF`。
- **不勾选任何阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛）。

## 6. 测试面

- host：`handle`（编码/解码/拒绝）、`rights`（`union`）、`frames`（owner/release/disjoint）、
  `bootstrap`（generation 递增、stale 拒绝、结构性根保护、派生越权/类型、移动/撤销、IPC 句柄）。
- QEMU：27 条事件按新序列断言；新事件字段与反例注入（裸索引、无 generation 递增、stale 被授予、
  复用帧未清零、`disjoint=0`）。
- `cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、
  `cargo fmt --all --check`、`cargo xtask test`、`cargo xtask compat`。
