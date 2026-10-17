# 切片 P 设计：阶段 1 退出条件——帧唯一归属与初始 Authority 物化

> **SPEC-150 更正（2026-10-17）**：本文保留 2026-09-16 的原始设计快照。后续已发生：
> 初始创建事件改为 `TaskCreated`（非 `TaskStateChanged`）；bootstrap 追加三条初始
> `CapMinted`（§14 步骤 9）；`authority_ready.root_ns` 为严格句柄 `0x100000002`；
> `frames_ready` 增 `disjoint=1`（`ISSUE-FRAME-002` 已关闭）。现行结论见
> [SPEC-150 报告](../reports/2026-10-17-stage1-review-remediation-spec150.md)。

> 文档类型：真实内核实现设计（阶段 1 退出条件）
>
> 日期：2026-09-16
>
> 项目：Prima OS（`kernel/` 真实内核）
>
> 状态：已获用户确认（2026-09-16）

## 0. 定位与前置

阶段 1 两个退出条件仍未勾选：

- **S1-X2**「init 只获得规范定义的初始 Authority」；
- **S1-X3**「活动 Segment 与空闲页帧满足唯一归属不变量」。

冻结的垂直切片（COW→PF→`exc_resume`→TLB→Linux trace）已收口，阶段 2 未开始。本切片按
[[COSIR 原语规范]] §14（Bootstrap 初始 Authority）、§4.1（权限）、§5.1/§5.4（对象）、
§7（Segment）、§16 W5/W6/W7（不变量）、§20.5（页表帧守恒式）补齐实现。

### 0.1 规范与实现的差距（动手前核对）

| 项 | 规范要求 | 实现现状（改前） |
|---|---|---|
| `frame_universe` | §14 L1602：`free_frames = frame_universe \ image_frames`（image **属** universe） | `mem::build_inventory` 把 image 排除在 universe 外 |
| W6 三类划分 | free ⊎ 活动 Segment 帧 ⊎ 保留帧（含页表帧）= universe | **无 Segment 对象**；`vspace::map_4k` 取的页表帧无归属记账 |
| 单一分配源 | — | `cow.rs` 把 `FreeFrameAllocator` 的**副本**存入 `COW_CTX`（L134-135 注释自承可状态分叉） |
| image Segment | §14 步骤 2：`frames=image_frames`、只允许 `{R,X}`、`pool=CPU` | 无对象实体 |
| init Namespace | §14 步骤 3：空 Namespace；Namespace cap 为结构性根（W5） | 无对象实体；无结构性根字段 |
| init 初始状态 | §14 步骤 8：**必须** `Ready` | `TaskStatus::Running` |
| 权限 | §4.1：`FiniteSet<Right>`，类型匹配 | 逗号字符串；`NAMESPACE_RIGHTS="all"` 通配符 |
| `booted`/Audit | §14 步骤 9 + W14：`booted=true`、第一组 Audit、重复调用 `AlreadyBooted` | 无 |
| 入口前置 | §14 前置 3：入口必须在 image 范围 | 无校验 |

### 0.2 已确认的关键决策（用户逐项确认）

1. **权限重构为 bitset**（`Rights` 位集 + `Right` 枚举 + 类型匹配），去掉 `"all"` 通配符；
2. **init 改为 `Ready → Running`**：bootstrap 置 `Ready`，首次调度（进入用户态前）迁 `Running`
   并追加 `TaskStateChanged`；
3. **image Segment 只物化对象 + 逻辑映射记录**，不替换 bootloader CR3；
4. **沿用组织级独立性门槛**：即使证据齐全也**不勾选** S1-X2/S1-X3、阶段 1 维持“进行中”。

## 1. 帧唯一归属（S1-X3）

新增 `kernel/src/frames.rs`：`FrameRegistry` 是 W6 的单一归属点。

- `universe = usable_frames + image_frames`（§14 步骤 1）；分配器仍只从 Usable 取帧，
  image 帧由 `image_frames` 类单独计数，不与 `segment_frames` 重复计。
- 取帧只有两个带标签入口：`allocate_for_segment(id)`（归入某活动 Segment）与
  `allocate_table_frame()`（归入保留页表帧）。**不存在不带归属的取帧 API**。
- `check_conservation()`：`free + segment + table + image == universe`。
  **不交性由构造保证**（单一分配器 + 带标签入口，每个帧只能被取走一次；image 与 Usable 的
  不相交已由 `mem::build_inventory` 与 `FreeFrameAllocator::from_regions` 校验），
  **守恒性由计数校验**——任何绕过标签入口的分配都会让守恒式失衡（host 单测覆盖）。
- 迁移全部取帧路径：`main.rs` 的局部 allocator → `frames::install`；`vspace::run_witness`
  的 A/B 数据帧与页表帧分开标签；`cow.rs` **删除分配器副本**，COW 共享/私有帧经
  `allocate_for_segment(SEG_COW)`；`user.rs` 的 code/stack/X/Y/第二任务/空转页各自先
  `open_segment` 再取帧，页表帧走 `allocate_table_frame`。
- **保持原分配顺序**，故既有事件中的帧号（`vspace_ready.frame_a/frame_b`、`cow_ready.shared/private`、
  `user_cow_ready.*`、`preempt_ready.root*/cr3*`）逐字节不变（QEMU 实测确认）。

## 2. 初始 Authority（S1-X2）

`kernel/src/bootstrap.rs`：

- `create_state` 按 §14 步骤 1-9 重写：入口范围前置（新错误 `EntryOutsideImage`）→ 物化
  image Segment（`{R,X}`、CPU、frames=(kernel_phys_start, image_frames)）→ 空 init Namespace →
  3 条根 Cap（bitset 有限权限）→ 结构性根 `root_ns_cap` → 初始 Audit（`Bootstrapped` +
  两个 `ObjectCreated` + `TaskStateChanged(Ready)`）→ `booted = true`；重复调用（已安装时）
  返回新的 `AlreadyBooted`（W14）。
- 新增 `TaskRecord.root_ns_cap`；`TaskTable::with_init` 初始状态改 `Ready`。
- `BootstrapState::activate_init()`：`Ready → Running` + `TaskStateChanged(Running)`；
  main 在进入用户态前调用。
- `validate_authority` 增：初始 Cap 权限必须 `well_typed`（§4.1 类型匹配）；image Segment 与
  Namespace 对象必须存在；W5 结构性根（由 init 持有、位置 active、含 `NsResolve`）。
- `BootstrapRecord`（见证记录）与 `authority_record()` 供协议事件读取。

## 3. 见证事件

两条事件**追加在 `exc_resume_ready` 之后、`boot_complete` 之前**，使既有 seq 1..21 与 data
完全不变（沿用 SPEC-053 先例；`boot_complete` seq 22→24）：

- `authority_ready`：`caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=2;init_running=1`
- `frames_ready`：`universe=31546;free=30843;segment=16;table=24;image=663;conserved=1`（实测值）

## 4. 证据边界（不夸大，且含一处**有意修正**）

- **既有事件的一处数据变化**：`bootstrap_ready.namespace_rights` 由通配符 `all` 改为规范
  有限集 `Derive,NsResolve,NsBind,NsUnbind,NsMount,NsUnmount,NsDestroy`。这是去通配符的
  **有意修正**（§4.1 要求 `FiniteSet<Right>`），按维护规则 5 显式记录，**不隐藏**；
  其余既有事件的 data 不变。
- image Segment 的 `frames` 在本实现中以 `(base, count)` 连续区间表达，规范是
  `Vector<FrameId>`；本实现另有 `build_inventory` 校验 image 与 Usable 不相交，故区间表达
  不损失本切片所需语义，但**不是** `Vector<FrameId>` 的完整细化。
- §14 步骤 4 的逐页 vspace 映射**未物化到真实 CR3**：真实内核仍运行在 bootloader 提供的
  页表上；本切片只建立对象与归属，不替换映射。
- bring-up 的 Segment（code/stack/X/Y/spin/COW/vspace 见证）以 ID 标签 + 帧计数表达，
  待阶段 2 的 `memseg_create` 接管为真实用户可见 Segment。
- **不交性由构造保证、守恒性由计数校验**：本切片不保存 frame→owner 的逐帧映射，因此无法
  检测"同一帧被同时计入两个 Segment"这类构造外错误（该路径在本实现中不可达）。
- 未实现：`memseg_create/map/unmap/destroy`、`ns_*` 作为用户可调用原语（阶段 2）；
  `TaskStatus::Faulted`（规范已定义，内核状态机仍缺）；`REFINEMENT-PROOF`。

## 5. 测试面

- `cargo test --workspace`：kernel 225（含 `rights` 9 条、`frames` 13 条、entry/activate 等）、
  xtask 125（含新见证字段篡改反例 4 条）。
- `cargo xtask test`：BIOS run=1/2 六条 PASS；新事件被断言（`conserved=1` 且
  `free+segment+table+image == universe`）。
- `cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`。
- `cargo xtask compat` 仍 PASS（Linux 有界差分不受影响）。

## 6. 范围未含

- 阶段 2 的 Capability/Segment/Task/IPC 用户可见原语；
- 自建 CR3 承载 image、TLB/PCID/shootdown、多核；
- 用户可调 `SYS_EXIT`/任务回收；
- 阶段 1 的关闭宣告（见 §7）。

## 7. 关闭口径

本切片产出可复查证据，但**不勾选 S1-X2/S1-X3、不宣告阶段 1 关闭**：按用户决定沿用与
阶段 0 / S0-X1 相同的组织级独立性门槛，阶段 1 维持“进行中”，唯一剩余前置是范畴外第三方复核。
