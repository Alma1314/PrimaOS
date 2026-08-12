# BLK-002 细化续：MemorySegment 页表 ↔ vspace 细化

> 文档类型：阶段 0 细化层设计
>
> 关联规范：[[COSIR 原语规范]]（§5 Segment/vspace、§7 memseg 操作、§18.0 边界清单 #5/#6、§20 细化层）；[[Prima OS]]（§13.2 COSIR 细化验证）
>
> 状态：已获批准，待实现

## 1. 目标与范围

在 BLK-002（x86_64 MachineState → POSIR State，Bootstrap 细化）基础上，把 **MemorySegment 页表 ↔ vspace** 的细化对应展开为真实四级页表结构，作为阶段 1 真实内核页表/trap 工作（S1-D4）与 vspace 管理（S2-D2/D3）的忠实细化蓝本。

上一切片（BLK-002）明确把「MemorySegment 页表↔vspace 全量细化」「真实多级 PML4 遍历」划为留 S1-D4；本切片经批准把该部分**前移进细化层**，用可执行探针（`cosir`）实例化。其余两项候选（task_create 入口细化边界清单 #16、异常/trap 递送）**仍不展开**，仅维持登记者状态。

范围决策（已与用户确认）：

1. **方向**：MemorySegment 页表↔vspace；Create/Map/Unmap/Destroy 四操作全展开。
2. **页表建模尺度**：真实四级页表结构（PML4 → PDPT → PD → PT → 叶）。
3. **页面粒度**：仅 4KB，四级固定走到底；**不做大页递归/分裂/合并逻辑**。
4. **叶 PTE 结构**：方案 C——双字段 `{frame_phys, mapping: PageMapping}`，物理地址视图与 POSIR 引用视图显式并列。

职责分离延续 BLK-002 原则：

- **页表机制**（`pagewalk.rs`）与**细化对应**（`memseg_refine.rs`）分离；
- 两者都**不新增对 POSIR 可见的一等原语**，不进 POSIR 抽象正文；
- 抽象函数只做**投影与迁移**，不做安全判定。

## 2. 四级页表组件（`pagewalk.rs`）

仅 4KB 粒度的真实四级结构。每个 CPU Task 持有自己的一棵 CR3 树：

```rust
// 每棵四级树由 Task 的 CR3 指向；4KB leaf，四级全走到底
pub struct PageTable { root: Pml4 }

pub struct Pml4 { entries: [Option<NonLeafEntry>; 512] }
pub struct Pdpt { entries: [Option<NonLeafEntry>; 512] }
pub struct Pd   { entries: [Option<NonLeafEntry>; 512] }
pub struct Pt   { entries: [Option<Pte>; 512] }   // leaf：占 4KB

pub struct NonLeafEntry { child: PageTableTreeNode }  // 指向下一级，真实 PTE 的物理指针经框架抽象

pub struct Pte {
    pub present:  bool,       // real x86_64 bit0
    pub writable: bool,       // bit1
    pub user:     bool,       // bit2（用户态可达）
    pub nx:       bool,       // bit63
    pub frame_phys: PhysAddr, // 物理帧地址（方案 C 物理视图）
    pub mapping:  SegmentRef, // 方案 C POSIR 引用视图 {segment, offset, access}，见 §2.1
}
```

关键点：

- **四级固定走到底**：MapEntry 到 4KB 叶；每个非叶表 512 槽；`translate(vaddr)` 用 `vaddr` 的 `47:39|38:30|29:21|20:12` 四级索引遍历到叶。
- **`PhysAddr`**：`u64`，帧大小 `PAGE_SIZE = 0x1000`。
- **数据流接口**：`map_vaddr(tree, vaddr, pte)`、`unmap_vaddr(tree, vaddr)`（清叶并向上回收空表结点）、`translate(tree, vaddr) -> Option<Pte>`。
- 树内**不存 Task 引用**；Task↔树关联由细化层 `Map<TaskId, ...>` 维护（见 §4）。

### 2.1 引用视图类型分层

规范 §5.2 的 `PageMapping {segment, offset, access}` 是 POSIR 抽象层的完整记录。探针内既有 `state.rs` 因 exc_resume 切片需要，把 `PageMapping` 简化为 `(VirtAddr, u8)` tuple（第 9-12 行，u8 为 access 位域）——那是 POSIR 协域的**简化视图**，本切片**不复用、不改动**它。

本切片在细化层 `pagewalk.rs` 新增完整引用类型：

```rust
pub struct SegmentRef {   // 规范 §5.2 PageMapping 的细化层忠实表达
    pub segment: SegId,
    pub offset:  u64,     // PageOffset
    pub access:  Access,  // R/W/X
}
```

`Pte.mapping: SegmentRef` 携带足够信息供 W7 校验（指向活动 Segment、offset 在段内、access ≤ allowed_access）。`state.rs` 的简化 `vspace` 保留给 exc_resume 探针；本切片的树↔POSIR 双视图以 `SegmentRef` 为细化侧、以 spec 语义为 POSIR 侧，**不**依赖 `state.rs` 的简化 tuple。

### 2.2 权限位编码（POSIR `Access{R,W,X}` ↔ 页表位）

细化层给出编码对照，写入 COSIR §20：

| POSIR `Access` | real x86_64 位 |
|---|---|
| `R` | `present`（真实只读由 `writable=0` 表达） |
| `W` | `writable`（bit1） |
| `X` | `!nx`（可执行 = NX 位清零） |
| 用户态可达 | `user`（bit2，内核映射时由内核对页表标记） |

`present=true` 隐含 R 的硬件语义：真实 x86_64 中 `present=1, writable=0` 即只读。叶 `Pte.mapping` 与 `Pte` 位由细化层保持一致（由 `memseg_refine.rs` 负责写时同步）。

## 3. 内存细化：帧管理与 W6

细化层沿用 `state.rs` 既有的 `frame_universe`/`free_frames` 概念（探针内轻量复现）：

- `PhysAddr = u64`；`FrameAllocator { free: Vec<PhysAddr>, universe: FiniteSet<PhysAddr> }`；
- `alloc()` 从 `free` 取出帧并断言不在任何活动 segment 的 `frames` 中（W6）；
- `free(p)` 归还 `free`，断言不与其它 segment 交叠（W6）。

## 4. 细化对应范畴（`memseg_refine.rs`）

**标签域对应：**

| POSIR 抽象（§5/§7） | x86_64 细化（本切片） |
|---|---|
| Task `vspace: VirtPage ⇀ PageMapping` | Task 的 CR3 指向的四级页表树叶集 |
| `PageMapping {segment, offset, access}` | 叶 `Pte.mapping = SegmentRef {segment, offset, access}`（POSIR 引用视图） |
| Segment `frames: Vector<FrameId>` | 帧物理地址列表（映射时进入叶 `frame_phys`） |
| `frame_universe` / `free_frames` | `FrameAllocator.universe` / `.free` |
| `memseg_map` 写入映射 | 遍历树到底插入叶 `Pte{frame_phys, mapping}`，同步 `task.vspace` |
| `memseg_unmap` 删除映射 | 清叶并向上回收空表，同步 `task.vspace` |
| W6 物理页唯一归属 | 帧 alloc/free 断言 |
| W7 虚拟映射有效 | `task.vspace` 与叶 `mapping` 集一致（§5 α 投影验证） |
| W8 可执行入口显式授权 | `unmap` 前置拒绝含 `entry` 页（`EntryMapping`） |

**Task↔树关联与双视图**：细化层维护 `Map<TaskId, Pml4 根>` 与一张细化侧的 `Map<TaskId, Vec<(VirtAddr, SegmentRef)>>`（即该 Task 的 vspace 完整视图，**不复用** `state.rs` 的简化 tuple，见 §2.1）。树叶 `mapping` 集与该细化 vspace 视图保持同步（写树必写 vspace，反之亦然），由 α 投影校验一致性。`state.rs` 的 `Task.vspace`（简化 tuple）维持 exc_resume 探针现状，本切片不触碰。

## 5. 数据流：memseg 操作细化迁移（§7.1–7.4）

每个 POSIR 操作落到四级树 + 帧池的细化迁移：

### 5.1 `memseg_create(page_count, allowed_access)`

- 细化：从 `free_frames` 取 `page_count` 个帧（W6：确保不与任何 segment 交），建 `Segment {frames, allowed_access}`，铸造根 SegCap（`Derive|SegDestroy|SegMapR/W/X`）。
- **不建树**：页帧此时未映射进任何 vspace。
- α 投影：`segments` 出现新 Segment，`free_frames` 减少。
- 错误：`ZeroSize`、`InvalidAccess`、`NoMemory`。

### 5.2 `memseg_map(seg_cap, task_cap, seg_offset, target_base, page_count, access)`

- 前置校验：SegMap 授权、`access ⊆ allowed_access` 且非空、区间在段内、`[target_base, target_base+page_count)` 在目标 Task 全空。
- 对每页在目标 Task 的 CR3 树**走到底**插入叶 `Pte{frame_phys = seg.frames[seg_offset+i], writable/nx = encode(access), user=true, mapping = {seg, seg_offset+i, access}}`；同时 `task.vspace[target_base+i] = mapping`。
- α 投影：vspace 出现新映射、树出现新叶。**W7 验证点**。
- 错误：`InvalidSegment`、`InvalidTask`、`MissingMapRight`、`RightsExceeded`、`OutOfBounds`、`Overlap`、`ZeroSize`。

### 5.3 `memseg_unmap(task_cap, target_base, page_count)`

- 前置校验：TaskManageVSpace、区间全映射、**区间不包含 target 的 `entry` 所在页**（W8）。
- 清四级树叶（摘叶并向上回收空表结点），同时从 `task.vspace` 删除对应 `PageMapping`。
- α 投影：vspace 与树叶同时减少。**W8 验证点**。
- 错误：`InvalidTask`、`MissingRight(TaskManageVSpace)`、`NotMapped`、`EntryMapping`、`ZeroSize`。

### 5.4 `memseg_destroy(seg_cap)`

- 前置校验：SegDestroy、无活动 vspace 引用 target（`StillMapped`）。
- 从 `segments` 删除，`frames` 全部归还 `free_frames`（W6），追加事件。树在此无叶残留（否则 StillMapped）。
- α 投影：`segments` 消失、`free_frames` 增加。错误：`InvalidSegment`、`MissingRight(SegDestroy)`、`StillMapped`。

## 6. 抽象函数 α：树 ↔ vspace 投影

`memseg_alpha(state) -> bool`（或返回验证结果）检查：

1. 对每个 Task `t`：细化侧 vspace 视图（`Map<TaskId, Vec<(VirtAddr, SegmentRef)>>`，非 `state.rs` 简化 tuple）的映射集 == CR3 树全部叶 `Pte.mapping` 的投影集（双视图一致）；每 `VirtPage` 单映射（W7）。
2. W6：所有活动 segment `frames` 两两不交、与 `free_frames` 不交、并集 = `frame_universe`。
3. 每个 `Pte.mapping` 指向活动 Segment 且 access ≤ allowed_access（W7）。

## 7. 错误处理

沿用规范错误集，**不新增对 POSIR 可见的错误原语**。页表遍历中"路径深表不存在"等机制内部状态不外泄，统一折叠为对应 POSIR 错误（`Overlap`/`NotMapped`/`OutOfBounds`/`ZeroSize`）。

## 8. 测试计划（TDD，三件套 + 错误，对接 BLK-002 §5.2 模式）

1. **静态 `test_vspace_matches_page_tree`**：构造一棵 CR3 树叶集，α 投影 vspace（细化侧视图），断言与细化 vspace 视图一致；覆盖 W7 约束。
2. **动态 `test_memseg_ops_map_to_page_walk`**：四操作各一步 → 树断言：`memseg_map` 后 `translate(target_base)` 返回叶、unmap 后返回 `None`、destroy 后帧归还；帧 alloc/dealloc 配对，W6 全程成立。
3. **错误/收尾**：destroy 映射引用 → `StillMapped` 帧不归还；map 撞已映射区 → `Overlap` 树不变；unmap 含 entry 页 → `EntryMapping`（W8）；create 缺帧 → `NoMemory` 且 `free_frames` 不变。
4. **幂等/双视图**：重复 map 同一页 → 状态不变且返回错误；树↔vspace 双视图同步在每操作后由 α 验证成立。

## 9. 验收

- `cargo test -p cosir` 全绿（既有 exc_resume + bootstrap + 本切片新增）；
- `cargo clippy --workspace --all-targets --no-deps -- -D warnings` 零警告；
- `cargo xtask test` 六条 PASS 不破坏启动闭环；
- `main.rs` 验收线束追加一条 `PASS memseg page-walk refinement` 摘要；
- 实施文档登记 SPEC-014。

## 10. 证据登记

- `COSIR 原语规范.md`：§20 扩展 MemorySegment 细化小节（四级表、页表位编码表、四操作细化对应、W6/W7/W8 承接）。
- `Prima OS 实现阶段.md`：新增 SPEC-014 证据，更新 BLK-002 描述为「草案已放宽为覆盖 MemorySegment 页表↔vspace 细化」，退出条件 S0-D4 维持「细化草案已定义」（不虚报到已证明）。
- `Phase 0` 退出条件 S0-X1/X2/X3 与 BLK-001 均保持原状（本切片不关闭它们）。

## 11. 范围边界（证伪边界）

延续 BLK-002 §6，区分「已定义」与「待证明」：

- **已定义**：四级页表结构、页表位编码表、α 树↔vspace 投影、四个 memseg 操作的树映射、W6/W7/W8 的探针可执行验证。
- **待证明**：α 对全部 POSIR 状态的完整性；真实内核 S1-D4 页表实现满足 α；形式化验证（BLK-001）。真机 trap 时叶 `mapping` 额外字段（POSIR 引用）如何落地（真实 PTE 不含 SegId）属 S1-D4——本切片仅在细化层探针建模，不解决真机 PTE 布局。

若实现中发现「需在 POSIR 层新增机制才能完成 MemorySegment 页表细化」，构成对 H3/H1 的潜在反例，须记录在案（见 COSIR §18.0 末尾规则）。
