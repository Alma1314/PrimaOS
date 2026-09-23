# 切片 W 设计：image Segment 的真实逐帧向量（ISSUE-FRAME-001）

> 文档类型：真实内核实现设计（阶段 2 收尾，第一节）
>
> 日期：2026-09-23
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-23，四项路线决策逐项确认）

## 0. 定位与前置

本切片是「闭合阶段 2 剩余登记项」的第一节，另两节为切片 X（`ISSUE-AUTH-001`，自建 CR3）
与切片 Y（`ISSUE-TASK-001`，`Faulted` 状态机）。用户已确认顺序 **W → X → Y**：X 需要
image 帧的逐帧枚举与 vspace 记账，故 W 先做；Y 与二者无代码依赖但共享同一条 QEMU 事件
基线，串行推进才能各自登记回归面。

跟踪表 `ISSUE-FRAME-001` 原文大意：

> image Segment 的 `frames` 以内核 `(base, count)` 区间表达，规范 §5.1 要求 `Vector<FrameId>`；
> 尚无逐帧归属表（不交性靠构造，非逐帧映射）。解除条件：在 `memseg_create` 落地时引入真实
> 帧向量与逐帧 owner 查询。状态：Open（部分修复——`memseg_create` 已用 `SegmentFrames::Vector`
> 真实帧向量；image 仍为 `(base, count)` 区间表达）。

| 面 | 规范要求 | 改前现状 |
|---|---|---|
| Segment 帧集合 | §5.1 `frames: Vector<FrameId>`，非空、无重复，页数 = `frames.length`（`COSIR 原语规范.md:333-341`） | `SegmentFrames::Contiguous { base, count }`（`bootstrap.rs:1407`），image 走区间 |
| image 帧来源 | §14 前置 2 `image_frames` 非空、无重复；动作 2 以它建 image Segment（`:1584-1603`） | image Segment 存区间（`bootstrap.rs:3869`） |
| 逐帧归属 | W6 三类两两不交、并集 = universe（`:1920-1924`） | 注册表 `owner_of` 对 image 走**区间判断**（`frames.rs:264`），非逐帧查表 |

侦察（只读）确认：§5.1/§14 均为**硬性逐帧枚举**，措辞检索未找到任何"区间/连续"豁免条款；
`Prima OS.md` 无帧级条款。

### 0.1 已确认的关键决策（用户逐项确认，2026-09-23）

1. **image 帧集合落地形态**：新增**结构体外的真实逐帧静态表**——字面满足 §5.1 的
   `Vector<FrameId>`，不占用 `BootstrapState` 的 16 KiB 结构体守卫额度；被否决的两项是
   「确立『区间即有限帧集合』等价谓词」（不字面满足 §5.1）与「修订 §5.1 承认区间表达」
   （为实现方便改规范）。
2. **切片划分**：W / X / Y 各为**独立 SPEC**，即使回归面重叠也不合并；每片内部完整交付该领域。

### 0.2 容量事实（决定了本切片不是"改个类型"）

image 帧数随内核镜像体积漂移（历史实测 663→778），而既有两处定长数组都装不下：

- `MAX_SEGMENT_FRAMES = 16`（`bootstrap.rs:1392`）——`SegmentFrames::Vector` 的内联数组容量；
  调大到 ~800 会让 `SegmentObject` 的 `[_; MAX_SEGMENT_OBJECTS=32]` 撑爆 `BootstrapState`
  的 16 KiB 守卫（该守卫按项目纪律不得放宽）。
- `MAX_FRAME_OWNERS = 192`（`frames.rs:43`）——且它**只记非 image 帧**。

故向量必须落在 `BootstrapState` 之外。

## 1. 设计

### 1.1 `SegmentFrames`：区间变体删除，新增 `Image`

（`kernel/src/bootstrap.rs`）

```rust
pub enum SegmentFrames {
    Vector { frames: [u64; MAX_SEGMENT_FRAMES], count: usize },  // 用户/平台 Segment（内联）
    Image  { count: usize },                                     // image Segment（逐帧委托注册表）
}
```

- `Contiguous` **删除**（不再有区间表达这条旁路）；`frame_at` 对 `Image` 逐帧委托
  `frames::image_frame_at(index)`，与 `Vector` 同属 §5.1 的 `Vector<FrameId>`。
- `memseg_map` 的容量前置与 `platform_append_frame` 的分支按变体改名：
  image 段不允许追加帧（返回确定性 `CapacityExceeded`，语义与改前一致）。
- `create_state` 建 image Segment 时记 `SegmentFrames::Image { count: inventory.image_frames }`。

### 1.2 `FrameRegistry`：image 逐帧向量是唯一真值源

（`kernel/src/frames.rs`）

- 新增 `image_frame_ids: [u64; MAX_IMAGE_FRAMES]` + `image_count`，**取代** `image_start`/
  `image_end`/`image_frames` 三个字段；`MAX_IMAGE_FRAMES = 2048`（对应内核镜像 8 MiB 上限，
  超出返回 `CapacityExceeded`，显式失败而非静默截断）。
- 向量在 `init_at` 中由 `kernel_phys_start + i * FRAME_SIZE`（i ∈ 0..`image_frames`）**一次性物化**。
  `mem::build_inventory` 的 `image_frames` 由帧对齐的 `[start, end)` 导出（`mem.rs:330`），
  故逐帧物化与旧区间判断是**同一帧集合**——由 host 单测钉住（含区间端点 `kernel_phys_end`
  不属于 image 的边界断言）。
- `owner_of` / `check_disjoint` / `check_conservation` / `witness` 全部改读向量：
  - `owner_of`：`image_frame_ids[..image_count].contains(&frame)` → `FrameOwner::Image`；
  - `check_disjoint`：**新增** image 向量自身的重复判定（区间表达下"向量内有重复"根本不可
    表达，逐帧向量必须自证无重复），并保留与 Segment/页表帧的逐帧不交判定；
  - 守恒式与 `frames_ready` 的输出口径**不变**（image 帧仍计入 `image` 类，不并入 `segment`）。
- 新增 `image_frame_count()` / `image_frame_at(index)`（含模块级全局包装）。

### 1.3 就地构造：本切片实施中暴露的真实缺陷

首版实现把向量内联进 `FrameRegistry` 后，`install()` 仍沿用
「`FrameRegistry::new(...)` 构造 → 按值放入 `static mut REGISTRY: Option<FrameRegistry>`」。
注册表因此膨胀到 ~27 KiB，boot 早期在该路径上叠出数份临时量，**实测溢出 256 KiB 启动栈并
三重故障**（QEMU 无 `memory_ready` 输出、退出码 0 = reset）。

处置按"扩展模型 / 不改既有约束"的纪律：

- **不**调大 `config.kernel_stack_size`（那是把约束放宽来迁就实现）。
- 改为**就地构造**：`FrameRegistry::empty()`（`const` 初值）+ `init_at(&mut self, ...)`
  （全部前置校验先于任何字段写入，失败时注册表保持调用前状态）；`install()` 直接在
  `static mut REGISTRY: FrameRegistry` 上调用它，`INSTALLED` 单独表达安装状态。
  `new()` 保留为 `empty() + init_at()` 的按值包装，供 host 单测使用。
- `FreeFrameAllocator::empty()`（`mem.rs`）为 `const`，`from_regions` 复用它消除重复字面量。

### 1.4 容量与守卫（不改任何既有守卫）

| 守卫 | 阈值 | 本切片 |
|---|---|---|
| `BootstrapState` 尺寸 | 16 KiB | 不动（向量在 `BootstrapState` 之外） |
| `MAX_SEGMENT_FRAMES` | 16 | 不动（image 不走内联向量） |
| `MAX_FRAME_OWNERS` | 192 | 不动（image 不再需要逐帧 owner 条目：向量本身即归属表） |
| 启动栈 | 256 KiB | 不动（改为就地构造，不靠加栈） |
| **新增** `MAX_IMAGE_FRAMES` | 2048 | 新增（注册表内的独立数组，~16 KiB BSS） |

## 2. 回归面（逐条登记，实测自 `cargo xtask run` 前后事件流 diff）

镜像因新增 16 KiB 静态数组而增大 4 帧（778→782），**仅 4 条事件**变化，无 seq 顺移：

| 事件 | 改前（HEAD `70b080f`） | 改后 | 原因 |
|---|---|---|---|
| `memory_ready` | `usable=30698;image=778;free=30698;kend=0x130a000` | `usable=30693;image=782;free=30693;kend=0x130e000` | 镜像增大 4 帧，Usable 相应减少 5 帧 |
| `bootstrap_ready` | `image_len=3183464;image_frames=778;free_frames=30698` | `image_len=3202656;image_frames=782;free_frames=30693` | 同上 |
| `preempt_ready` | `rsp1=0x1000007f328;rsp2=0x10000083328;kstack1_top=0x1000007f350;kstack2_top=0x10000083350` | `rsp1=0x10000083698;rsp2=0x10000087698;kstack1_top=0x100000836c0;kstack2_top=0x100000876c0` | 镜像增大后内核栈整体后移（两任务内核栈仍各自独立） |
| `frames_ready` | `universe=31476;free=30653;segment=19;table=26;image=778` | `universe=31475;free=30648;segment=19;table=26;image=782` | 守恒式仍成立（30648+19+26+782 = 31475）；`segment`/`table` **不变** |

**其余 26 条事件逐字段不变。** 其中 `segment=19`/`table=26` 未变是关键不变量证据：
本切片没有改变任何取帧路径的调用次数与顺序，只改了 image 帧集合的表达与构造方式。

（另记：`physical_map_ready.offset` 在本切片前后均为 `0x28000000000`——跟踪表/旧报告里写的
`0xffff800000000000` 是过时值，与本切片无关，此处一并更正。）

## 3. host 单测（新增 5 条）

- `image_frames_are_materialized_as_a_real_vector`：向量逐项等于区间枚举、越界为 `None`、
  逐帧 `owner_of` 为 `Image`、区间端点 `kernel_phys_end` 属 `Free`。
- `image_vector_self_overlap_is_detected`：向量内重复被 `check_disjoint` 检出；守恒只看计数
  故不受影响（两条判据独立）。
- `image_vector_capacity_is_enforced`：`image_frames > MAX_IMAGE_FRAMES` → `CapacityExceeded`。
- `init_at_failure_leaves_the_registry_untouched`：失败前置先于任何字段写入。
- `registry_stays_within_the_value_size_budget`：`size_of::<FrameRegistry>() ≤ 32 KiB`。

## 4. 验证

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel **283**、xtask 149 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，30 条事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。
- 事件流前后 diff（`/tmp` 基线对比）确认改动面恰为上表 4 条。

## 5. 边界（不夸大）

- 证据为真实内核 + QEMU，独立性为 **harness 内**，不构成 `REFINEMENT-PROOF`；
  **不勾选**任何交付项与退出条件，阶段 2 维持「进行中」（组织级门槛，与阶段 0/1 一致）。
- `SegmentFrames::Image::frame_at` 依赖**全局帧注册表已安装**（细化层约束）：未安装时返回
  `None`。内核路径在 `install` 之后才建 image Segment，故成立；登记为一条隐含前置。
- 逐帧向量与旧区间是同一帧集合由 host 单测与端点断言支持，但**规范措辞**（§5.1 是否允许
  "物理连续帧的紧凑表达"）的解读分歧仍**交复核者**；本切片选择字面物化以回避该分歧。
- `MAX_IMAGE_FRAMES=2048` 对应内核镜像 8 MiB 上限；超出时启动显式失败（`CapacityExceeded`
  → `panic_event`），不静默截断。
- 本切片**不**触碰 image 的 vspace 映射（§14 步骤 4）——那是切片 X（`ISSUE-AUTH-001`）的范围。
- `ISSUE-FRAME-001` 是否算闭合（"真实帧向量"已落地，但"逐帧 owner 查询"是否要求 image 也走
  逐帧 owner 条目）**交复核者裁定**，本切片不自行宣告。
