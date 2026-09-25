# 切片 AJ 设计：阶段 3 第一片 —— Namespace 路径操作（§11 全量）

> 文档类型：真实内核实现设计（阶段 3 第一片）
>
> 日期：2026-09-25
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-25：阶段 3 拆 **3 片**；新原语**要求 ring3 用户可见调用与证据**；
> `now` 由 **PIT tick 驱动**——后者属切片 AK）

## 0. 定位与前置

本切片是阶段 3（Namespace、Timer 与 Audit）的第一片，落实 **S3-D1**（Namespace
bind/unbind/mount/unmount/resolve）与 **S3-D2**（Mount 环检测与 stale Mount 清理），并覆盖退出
条件 **S3-X1**（所有 Namespace 解析均终止，活动 Mount 图无环）。S3-D5（Capability 派生、位置变化
与对象生命周期 Audit）在本片完成其 Namespace 部分（`Ns*` 事件 + `CapMinted`）。

规范锚点：§3.1 `Path`、§4.6 位置变化、§5.4 `Namespace`、§11.1–§11.8 七个操作、§13.1 事件清单。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| `Path`/`Component` | §3.1：非空 ASCII 分量（≤255 B、不含 `/`）、`.`/`..` 禁止、非空、分量数 ≤ `MAX_PATH_COMPONENTS` | **完全不存在** |
| Namespace 条目 | §5.4 `entries: Path ⇀ NsEntry`，`NsEntry = Bound(CapId) \| Mount(CapId)` | `NamespaceObject.entries` 只是 `usize` 计数，无存储；仅物化空 init ns（`bootstrap.rs:1891`） |
| ns 操作 | §11.2–§11.8 共 7 个 | 全部不存在 |
| Capability 位置 | §4.3/§4.6：`InNamespace(ns, path)` 是一等位置 | 只有 `HeldByTask`/`HeldByKernel`/`InMessage`（`bootstrap.rs:808`） |
| 审计 | §13.1：`CapMinted` + `NsBound/Unbound/Mounted/Unmounted/Resolved` | **全部不存在**（`AuditEvent` 17 变体，`bootstrap.rs:129`） |
| 结构性根保护 | §11.8 `StructuralCap`（活动 Task 的根 ns） | 判定已存在（`bootstrap.rs:2404`），可复用 |

### 0.1 本片的三条实现口径（照实登记）

1. **细化层容量上限**：规范说分量 ≤ 255 B、分量数上限名为 `MAX_PATH_COMPONENTS` 但**未给值**。
   本片取 `MAX_COMPONENT_LEN = 16`、`MAX_PATH_COMPONENTS = 4`、`MAX_NS_ENTRIES = 16`——
   比规范允许的上限紧，登记为细化层约束偏差（规范 §3.1 只要求「不得超过」，未要求取最大）。
2. **`InNamespace` 位置不带路径**：位置只编码 `ns` id，具体路径由 `NsEntry` 反查、由 `Ns*`
   审计事件显式记录。理由是位置表按槽直索引（`bootstrap.rs:842`）、且 §4.6 明确「移动不改变
   `CapRecord`」——把 69 B 的路径塞进每条位置会无谓放大位置表。
3. **`ns_create` 铸造根 Capability 使用预留槽区间**：ring3 见证程序以 const fn 拼装机码，
   句柄必须是编译期常量；故仿 SPEC-066 的 `PLATFORM_SEGMENT_CAP_SLOT_BASE` 做法，为 ns 根
   cap 预留 `NS_ROOT_CAP_SLOT_BASE = 52` 起的 8 个槽（与 `MAX_NAMESPACE_OBJECTS = 8` 对应）。

## 1. 设计

### 1.1 `Path` 与条目类型（新增 `kernel/src/namespace.rs`）

```rust
pub const MAX_COMPONENT_LEN: usize = 16;
pub const MAX_PATH_COMPONENTS: usize = 4;
pub const MAX_NS_ENTRIES: usize = 16;

pub struct Component { len: u8, bytes: [u8; MAX_COMPONENT_LEN] }
pub struct Path { len: u8, components: [Component; MAX_PATH_COMPONENTS] }

pub enum NsEntryKind { Bound, Mount }
pub struct NsEntry { pub ns: u32, pub path: Path, pub kind: NsEntryKind,
                     pub cap: Handle, pub creator: u32 }
```

- `Path::parse(bytes) -> Result<Path, PathError>`：逐条对应 §3.1 约束——非空、无前导/尾随 `/`、
  无 `//`（即无空分量）、分量非空、分量 ≤ `MAX_COMPONENT_LEN`、非 `.`/`..`、分量数 ≤
  `MAX_PATH_COMPONENTS`、分量字节为可打印 ASCII 且非 `/`。
- `Path::is_prefix_of`、`Path::strict_prefix_of`、`Path::parent`、`Path::extend`、`Path::render`
  （规范化文本，供审计与事件）。
- `Path` 是 `Copy` 且只含 `u8` 数组，故 `AuditEvent` 仍可 `Copy`。

### 1.2 条目存储：通用分段 arena（新增 `kernel/src/arena.rs`）

`BootstrapState` 已贴 13 KiB 守卫（`bootstrap.rs:7074`），**新存储不得内联**。新增泛型

```rust
pub struct Arena<T: Copy, const SLOTS: usize> { /* StaticCell<[Option<T>; SLOTS * REGIONS]> + AtomicUsize */ }
impl<T: Copy, const SLOTS: usize> Arena<T, SLOTS> {
    pub const fn new() -> Self;                       // 全 None
    pub fn claim(&self) -> ArenaSlice;                // 原子领段（耗尽即确定性 panic）
    pub fn get(&self, slice: ArenaSlice, index: usize) -> Option<T>;
    pub fn set(&self, slice: ArenaSlice, index: usize, value: Option<T>);
}
```

段数：内核 1 段、`#[cfg(test)]` 多段——**与 `AUDIT_ARENA` 同一理由**（host 单测并行构造多个
state，各自领段互不干扰）。段由 `AtomicUsize` 游标分配，`claim` 失败即确定性 panic（不静默复用
他人区段）。

`static NS_ARENA: Arena<NsEntry, MAX_NS_ENTRIES>`；`MAX_NAMESPACE_OBJECTS` 2 → 8
（`NamespaceObject` 仍 16 B，增量 ~96 B，守卫余量约 3.2 KiB 可容），`NamespaceObject.entries`
由「无意义的 0」改为**真实条目计数**（由 `NS_ARENA` 承载内容）。

**不迁移 `AUDIT_ARENA`**：它已在工作（`bootstrap.rs:244`），且有 `AUDIT_REGIONS = 512` 的特化；
迁移会无谓放大本片回归面。登记为遗留（两份段分配逻辑并存，待后续统一）。

### 1.3 位置模型扩展（`bootstrap.rs`）

`CapabilityLocation` 增 `InNamespace(u32)`：

- `holder_id()` 对 `InNamespace` 返回 `None`（命名空间不是持有者）——`authorize`
  （`bootstrap.rs:2379`）、`derive_capability`、`drop_capability` 的 `holder_of(cap) != Some(caller)`
  因此**自然**给出 `NotHeld`/`NotHolder`，正是 §4.5/§4.6 的语义（移动只在规范列出的操作内发生）。
- 新增 `move_to_namespace(handle, holder, ns)` 与 `move_from_namespace(handle, ns, holder)`，
  与既有 `move_to_task`/`move_to_message`/`move_from_message`（`bootstrap.rs:1494-1566`）同构
  （都先校验当前位置再改写）。
- **逐调用点复核**：`holder_of`/`authorize`/`is_held_by`/`revoke_subtree`/`holder_id` 全部读一遍，
  确认 `InNamespace` 不会被误判为「由 kernel 持有」或「无位置」。

### 1.4 审计事件（`bootstrap.rs`）

新增 6 个变体：`CapabilityMinted { cap, holder }`、`NsBound { ns, path, cap }`、
`NsUnbound { ns, path, cap }`、`NsMounted { ns, path, source_ns, cap }`、
`NsUnmounted { ns, path, cap }`、
`NsResolved { start_cap, path, mount_count, mounts: [Handle; MAX_PATH_COMPONENTS], parent, cap }`。

**尺寸登记**（重要）：`AuditEvent` 由 24 B 增至约 **132 B**（`NsResolved` 最大）；存储仍在
`AUDIT_ARENA` 中，**不占 `BootstrapState`**，故 13 KiB 守卫不受影响。但 arena 的 bss 由
约 3.1 MB（256×24×512）增至约 17 MB（host 测试；内核只 1 段 ≈ 33 KB）。这是本片可复测的尺寸
变化，写进报告。

`CapabilityMinted` 对应 §13.1 的 `CapMinted`（既有实现只有 `CapabilityDerived`）；`bootstrap` 期
的三条初始根**保持** `InitialCapability` 表示、不补发 `CapMinted`（它们不是「随对象创建铸造」的
运行时路径），登记口径。

### 1.5 七个操作（`BootstrapState` 方法 + `pub fn` 全局包装，同既有惯例）

| 操作 | 前置（§11） | 成功动作 | 追加事件 |
|---|---|---|---|
| `ns_create(caller)` | Actor 约束；无其它（§11.2 无失败分支） | 新 ns 对象（`creator = caller`）+ 铸造根 cap（`NAMESPACE_RIGHTS`，预留槽） | `ObjectCreated` + `CapabilityMinted` |
| `ns_bind(caller, ns_cap, path, cap)` | `NsBind`；path 合法非空；cap 由调用者持有且 `usable`；cap 非结构性根；path 无终端祖先/同名/后代 | `cap → InNamespace(ns, path)`，建立 `Bound(cap)` | `CapabilityMoved` + `NsBound` |
| `ns_unbind(caller, ns_cap, path) -> Handle` | `NsUnbind`；path 精确指向 `Bound(cap)` | 删条目；`cap → HeldBy(caller)` | `CapabilityMoved` + `NsUnbound` |
| `ns_mount(caller, dst_cap, path, src_cap)` | `NsMount`；src 由调用者持有、可用、指向 ns、含 `NsResolve`+`Derive`；path 无冲突；**无环**；同节点 | 从 src 派生 `{NsResolve, Derive}` 子 cap，直接置于 `InNamespace(dst, path)`，建立 `Mount(child)` | `CapabilityDerived` + `NsMounted` |
| `ns_unmount(caller, ns_cap, path)` | `NsUnmount`；path 精确指向 `Mount(cap)` | 删挂载条目 + 删子 cap 位置 | `CapabilityDropped` + `NsUnmounted` |
| `ns_resolve(caller, ns_cap, path, rights) -> Handle` | 持有可用 `ns_cap` 且含 `NsResolve`；path 合法非空 | §11.7 解析算法；派生新 cap 到 `HeldBy(caller)` | `CapabilityDerived` + `NsResolved` |
| `ns_destroy(caller, ns_cap)` | `NsDestroy`；非活动 Task 的结构性根 | 删全部条目与这些 cap 的位置；从活动 ns 表删除 | 各 `CapabilityDropped` + `ObjectDestroyed` |

错误枚举 **`NsError`** 与规范错误名一一对应（`InvalidNamespace`、`MissingRight(Right)`、
`InvalidPath`、`Overlap`、`InvalidCap`、`NotHeld`、`StructuralCap`、`NotDelegable`、`MountCycle`、
`NotFound`、`NotDirectory`、`TargetDead`、`MountTargetDead`、`RightsExceeded`），外加实现层
`CapacityExceeded`、`AuditFull`。

**失败不改状态**（§6.2）：全部前置 + `audit.can_append(n)` 通过后才提交；`ns_resolve` 的派生与
`ns_mount` 的派生子 cap 都先完成全部校验再 `slots.alloc`。

**stale Mount / stale Bound**：源 ns 已销毁时挂载条目保留（`MountTargetDead` 只在**解析**时返回），
`ns_unmount`/`ns_unbind` 仍可清理且目标已销毁不构成失败（§11.4/§11.6/§11.8）。

### 1.6 环检测与解析终止（S3-X1 的判据）

- **环检测**：`ns_mount` 提交前，从 `source` ns 出发沿既有 `Mount` 边做 DFS（节点 ≤
  `MAX_NAMESPACE_OBJECTS`，`visited` 位集去重 → DFS **必然终止**）；若可达 `destination` 则
  `MountCycle` 且不改状态。这是确定性判据，不是「限制深度」的近似。
- **解析终止**：`ns_resolve` 每消费一个分量就减少剩余分量数，且 `Mount` 切换不重复消费同一分量
  → 步数 ≤ 路径分量数，**结构性终止**。挂载链也不会被无界跟随，因为每次跟随都消耗一个分量。
- 两者各有单测；再各有一个**反例**（删掉 `visited` / 允许不消费分量的跟随），如实登记能否构造出
  失败（若构造不出，就登记「无对应失败见证」）。

### 1.7 ring3 syscall 面（`syscall.rs` + `user.rs`）

新 syscall 号 21–27：`SYS_NS_CREATE`/`SYS_NS_BIND`/`SYS_NS_UNBIND`/`SYS_NS_MOUNT`/
`SYS_NS_UNMOUNT`/`SYS_NS_RESOLVE`/`SYS_NS_DESTROY`。

- **path 传入**：`(ptr, len)` 指向用户页内的字节串，内核用既有 `copy_in`（`syscall.rs:1497`）
  按 8 字节分段读入栈缓冲（上限 `MAX_PATH_BYTES = 64`），再 `Path::parse`；坏指针 → `EFAULT`，
  超长/非法 → 对应 errno。`copy_in` 的 present+user 与不跨页校验天然复用。
- **返回**：成功为 `Handle::encode()`，失败为负 errno（既有编码惯例）；新增 errno：`Overlap`、
  `MountCycle`、`NotDirectory`、`MountTargetDead`、`NotDelegable`、`EEXIST`→复用 `Overlap`。
- **返回的动态句柄**（`ns_resolve`、`cap_derive` 式的 `slots.alloc()`）只写回执，不作为后续
  syscall 的编译期常量参数——ring3 程序以固定槽句柄（ns 根 cap）驱动 create/bind/mount/unbind/
  destroy 全链路。

`user.rs` 的机器码序列新增一段真实 CPL3 调用（`USER_CODE_LEN` 常量同步）：

```
create(→ ns_a) → create(→ ns_b) → create(→ ns_c)
  → derive 副本 b_copy/c_copy（挂载会移走源 cap，副本留作环检测与销毁见证）
  → mount(ns_a, "x", ns_b) 成功（建立边 a→b）
  → mount(b_copy, "y", ns_a) ⇒ MountCycle 负例（a 沿边可达 b）
  → bind(init, "lib", child1) 成功；再 bind(init, "lib", child2) ⇒ Overlap 负例
  → bind(ns_c, "sub", child2) → mount(init, "mnt", c_copy) → resolve(init, "mnt/sub") 跨挂载解析
  → bind(init, "/lib", …) ⇒ InvalidPath 负例（前导 `/` 产生空分量）
  → unbind(init, "lib") → unmount(init, "mnt") → destroy(c_copy)
```

（路径文本按 §3.1 约束 3 **不含**前导/尾随 `/`；`Path::render` 亦如此。）

回执写入 Y 页的 `NS_RECEIPT_*` 偏移；每一跳都以 `cs = 0x33` 证明来自 CPL3。

### 1.8 判决性见证（新增独立事件 `namespace_ready`）

一条新事件，字段分三组：

- **活动对象**：`namespaces`（活动 Namespace 数）、`entries`（存活条目数）；
- **审计计数**（§13.1 的 `Ns*` 事件条数）：`binds`、`unbinds`、`mounts`、`unmounts`、`resolves`；
- **判决位**：`cycles_rejected`、`overlaps_rejected`、`invalid_path_rejected`（拒绝计数，失败不产生
  审计事件故用实现层计数）、`acyclic_ok`（成功执行环检测并判无环的次数，应等于 `mounts`）、
  以及回执类布尔 `create_ok`/`resolve_ok`/`overlap_rejected`/`cycle_rejected`/`evidence_ok`。

实测值（2026-09-25，QEMU BIOS run=1）：

```
namespaces=3;entries=1;binds=2;unbinds=1;mounts=2;unmounts=1;resolves=2;
cycles_rejected=1;overlaps_rejected=1;invalid_path_rejected=1;acyclic_ok=2;
create_ok=1;resolve_ok=1;overlap_rejected=1;cycle_rejected=1;evidence_ok=1
```

`xtask` 侧新增 `validate_namespace_data` 逐字段硬断言 + **每个判决位一条反例单测**
（环未拒 / 环检测次数不足 / 路径审计事件缺失 / 活动对象数不符 / 整体判决失败 / data 缺失）。

## 2. 回归面

预期：既有 35 条事件的**语义与顺序不变**，`namespace_ready` 使 `boot_complete` 及之后 seq 顺移
35 → 36；镜像体积/布局类字段（`bootstrap_ready.image_len`、`memory_ready.*`、`frames_ready.*`、
`preempt_ready` 栈地址、`self_root_ready.pages`）随代码增长变化——逐条按维护规则 5 登记，实测自
HEAD worktree 的逐条事件流 diff。

## 3. host 单测（预期显著增加）

`namespace.rs`（Path 的每条拒绝分支 + 前缀/父/扩展）、`arena.rs`（段不重叠、容量边界、耗尽 panic）、
`bootstrap.rs`（7 个操作的正例/负例、环检测、stale 清理、失败不改状态、结构性根保护）。

## 4. 验证

`cargo test --workspace`、`cargo xtask lint`（四门）、`cargo xtask tcb`（复测占比登记）、
`cargo xtask test`（36 条事件）、`cargo xtask compat`；外加事件流逐条 diff 与同镜像连跑两次
（确定性）。

## 5. 边界（不夸大）

1. 本片**不实现** Timer 与 `audit(cap)` 查询（切片 AK/AL）。
2. **不勾选任何阶段 3 交付项或退出条件**——沿用阶段 0/1/2 的组织级独立性门槛，闭合判断交复核者。
3. `Path` 的容量上限是细化层约束（0.1 节第 1 条）。
4. `record.holder`（铸造时持有者）与位置表在移动后**分离**（§4.6）；`ns_*` 一律按**位置**判定
   持有，既有 `exc_subscribe` 用 `record.holder` 的口径差异**不在本片修正**，登记为注意事项。
