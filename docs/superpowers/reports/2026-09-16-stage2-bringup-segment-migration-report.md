# 切片 R 报告：bring-up 13 区域迁移为真实 Segment 对象

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-16
>
> 对象：`kernel/src/{frames,bootstrap,vspace,cow,user,resume,protocol,main}.rs`、
> `xtask/src/protocol.rs`
>
> 目的：交付「平台 bring-up 13 区域 → 真实 `SegmentObject` + 可授权根 Capability + 帧标签
> 统一」，回收 `ISSUE-AUTH-002` 与 `USER_SEGMENT_FRAME_LABEL_BASE` 临时命名空间

## 0. 结论速览

- **对象化**：13 个区域全部成为真实 `SegmentObject`（id 2..14，`creator`/位置持有者 =
  `KERNEL_ACTOR`），每个铸一条可授权根 Capability（槽 32..44，句柄
  `0x100000020`..`0x10000002c`）。QEMU 实测 `segments=13;roots=13`。
- **帧标签统一**：归属标签与 Segment id **恒等**（`100+id` 命名空间删除）。这不是构造性声明：
  事件里的 `unified=1` 由逐帧 `frames::owner_of(frame) == FrameOwner::Segment(id)` 计算。
- **授权中介映射**：11 个用户可见区域经 `memseg_map` 装入任务根，`TaskVspaceTable` 留下
  11 条平台逻辑映射；仅 VSPACE（换叶重映射见证）与 COW（fault-time 换映射、内核私有页）
  保留内核直映射（`direct=2;direct_ids=2,3`）。
- **位置模型**：新增 `CapabilityLocation::HeldByKernel`。`validate_authority` 的
  `HeldByTask` 存活检查**一字未改**；内核持有是独立位置类，不是检查豁免。
- **init 的初始 Authority 未受影响**：`authority_ready` 仍报 `caps=3;segments=1`，
  ring3 内嵌句柄槽 6/7 全部保持原值。
- **未完成/边界**：平台 cap 尚未移交给用户 Task（属 S2-D4/D5）；VSPACE/COW 的 W7 视图未建模；
  COW 私有 X 帧存在逻辑/物理归属分叉。`ISSUE-FRAME-001`（image 仍为 `(base,count)`）保持 Open。
- **不勾选任何阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛，见 §6）。

## 1. 方法与独立性声明

- 全部为真实内核实现 + QEMU 实测 + host 单测；无机器证明。
- 本报告复核独立性为 **harness 内会话级别**，**不是组织级第三方审查**，也不构成
  `REFINEMENT-PROOF`；不改变阶段 0/1/2 的独立性门槛。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| 帧标签 | `SEG_VSPACE`..`SEG_SPIN2_DATA` 1..13 → **2..14**；`frame_label`/`USER_SEGMENT_FRAME_LABEL_BASE` 删除（标签即 id） |
| 对象表 | `MAX_SEGMENT_OBJECTS` 16 → 32（`memseg_destroy` 不回收槽位）；`alloc_segment_slot` 下界 `USER_SEGMENT_ID_FIRST=15` |
| 位置 | 新增 `CapabilityLocation::HeldByKernel` + `holder_id`/`is_held_by`；`register_location`；`move_capability` 的审计记录**实际**源位置 |
| 根 cap | 新增 `mint_root_capability_at`（显式槽、不重置 generation）；`CapabilitySlotTable::slot_free`；`reserve` 改为保留既有 generation |
| 构造核 | `memseg_create` 主体抽成 `create_segment_object`；新增 `platform_create_segment`（id 固定、允许空访问集、内核持有 cap） |
| 平台入口 | `platform_segment_frame`/`platform_append_frame`/`platform_prepare_task_caps`/`platform_segments_record` + 模块级 wrapper |
| Task cap | `Task(1)`/`Task(2)` 的 `TaskManageVSpace` 根 cap 铸在内核持有槽 45/46 |
| COW | 共享帧由对象化创建提供，私有帧经 `platform_append_frame` 追加（COW 段终态 3 帧） |
| 见证 | 新事件 `platform_segments_ready`（seq 25）；`AuthorityReady`/`FramesReady`/`BootComplete` 25/26/27 → 26/27/28 |

## 3. 实测证据（2026-09-16）

```
PRIMA|v=1|seq=25|event=platform_segments_ready|status=ok|data=segments=13;first_id=2;last_id=14;roots=13;task_caps=2;mapped=11;direct=2;direct_ids=2,3;vspace=11;holder=0;slot_base=32;task1_cap=0x10000002d;task2_cap=0x10000002e;caps=0x100000020,0x100000021,0x100000022,0x100000023,0x100000024,0x100000025,0x100000026,0x100000027,0x100000028,0x100000029,0x10000002a,0x10000002b,0x10000002c;frames=2,3,1,1,1,1,1,1,1,1,1,1,1;unified=1
PRIMA|v=1|seq=26|event=authority_ready|status=ok|data=caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=0x100000002;init_running=1
PRIMA|v=1|seq=27|event=frames_ready|status=ok|data=universe=31501;free=30758;segment=16;table=24;image=703;conserved=1;disjoint=1
PRIMA|v=1|seq=28|event=boot_complete|status=ok
```

解读：

- `roots=13` 是**校验后的计数**（每条都要槽 live、目标 `Segment(id)`、位置 `HeldByKernel`、
  权限非空且类型正确），不是「铸了 13 条」的重述。
- `unified=1` 是逐帧 `owner_of` 查询的结果：13 个区域的每一帧在帧注册表里的归属标签都等于其
  Segment id。迁移前这些标签是独立的 1..13 命名空间、用户段是 `100+id`。
- `frames=2,3,1×11`：VSPACE 2 帧、COW 共享 + 2 个私有帧、其余各 1 帧，**和为 16**，
  与 `frames_ready.segment=16` 完全一致 —— 说明对象化没有改变任何一帧的取用数量与顺序。
- `vspace=11`：11 个区域在 `TaskVspaceTable` 留下逻辑映射（ring3 已把自己创建的两个段完全
  `memseg_unmap`，故最终恰为平台侧的 11 条）。
- `frames_ready`：`30758 + 16 + 24 + 703 = 31501 = universe`，`conserved=1`、`disjoint=1`。
  `segment=16`/`table=24` 与切片 Q 相同；`image` 682 → 703（内核镜像增大，见 §4）。
- **init 的初始 Authority 与 ring3 ABI 未变**：`bootstrap_ready.caps=3`、
  `authority_ready.caps=3;segments=1`、`handle_ready` 1..5、`cap_lifecycle_ready` 槽 6 两代、
  `memseg_ready` `create=0x300000006`/`create2=0x100000007` 全部与切片 Q 一致。

其余关键既有事件（地址已随镜像移动，语义不变）：

```
seq=10 vspace_ready frame_a=0x0;frame_b=0x1000;wp_err=0x3
seq=11 cow_ready shared=0x6000;private=0xb000;shared0=0xc0ffee0000000000
seq=12 user_cow_ready err=0x7;class=write_protection   ← X 页仍是只读，用户态写保护 #PF 仍在
seq=23 preempt_ready root1=0xc000;root2=0x15000;data1=0x1e000;data2=0x22000
```

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel 245、xtask 134 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，**28 条**事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| 事件序列 | 新增 `platform_segments_ready`(25)；`authority_ready` 25→26、`frames_ready` 26→27、`boot_complete` 27→28；`EXPECTED_EVENTS` 27 → 28 条 |
| `memory_ready` | `image` 682→703、`free` 30779→30758、`kend` 0x12bf000：内核镜像增大 21 帧（84 KiB）。原因：新增平台段 API/位置类代码 + `BootstrapState` 由 12552 → **15240** 字节（`MAX_SEGMENT_OBJECTS` 16→32）。`usable` 不变 |
| `bootstrap_ready` | `image_frames`/`free_frames` 随上；`image_len` 2879176。三个初始 cap 句柄与三组权限**不变** |
| `vspace_ready` | 帧地址随镜像移动（`frame_a=0x0`、`frame_b=0x1000`）；`wp_err=0x3`、魔数读回语义不变 |
| `cow_ready` | `shared=0x6000`、`private=0xb000`（原为其它帧号）；`shared0`/`private0`/`private1`/`private_last` 语义不变 |
| `user_cow_ready` | 仅地址相关字段变化；`err=0x7`、`class=write_protection`、回执值不变 |
| `preempt_ready` | `root1/root2/data1/data2/rsp*/kstack*` 随镜像与帧分配移动；`ticks=4`、`pre1=pre2=2`、`mark1/mark2` 不变 |
| `frames_ready` | `image` 682→703、`free` 30779→30758；`segment=16`/`table=24`/`universe=31501`/`conserved`/`disjoint` 不变 |
| `handle_ready`/`cap_lifecycle_ready`/`memseg_ready`/`syscall_*`/`exc_*` | **逐字段不变**（槽 1..7 与全部回执保持原值）——平台 cap 落在预留槽 32..46，未侵入 ring3 内嵌句柄 |
| `user_code` | 不变（1478 字节；`user_code_bytes_are_fixed` 通过） |

以上均按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. 边界与反例保留

- **机制偏差（登记）**：平台区域经 `platform_create_segment` 创建，而非 `memseg_create`
  入口本身。二者共用同一构造核（`create_segment_object`），差异仅在两处：平台入口允许空
  `allowed_access`（VSPACE/COW 内核私有）、并按固定 id 与预留 cap 槽分配。`ISSUE-AUTH-002`
  的字面验收（真实对象 + creator + vspace + 可授权 Capability）已满足，机制措辞的偏差在此登记。
- **cap 持有者是内核 actor（登记）**：13 条段 cap 与 2 条 task cap 由 `KERNEL_ACTOR` 持有。
  这样 init 的初始 Authority 严格保持 3 条（S1-X2 证据不变），且 task2 的段不被其他 Task
  持有（不与 S2-X2 相左）。**但"用户可达"尚未成立**——把平台段 cap 移交给用户 Task 属于
  S2-D4/D5；本切片不宣称用户可见性已达成。
- **VSPACE/COW 的直映射边界（登记）**：二者的 `allowed_access` 为空、`TaskVspaceTable`
  视图为空，故 `memseg_destroy` 的 `StillMapped` 看不到它们，W7 的「逻辑视图 vs 真实 PTE」
  不交性在这两个区域上未建模。它们需要 `set_leaf` 换叶与 fault-time 换映射，超出
  `memseg_map` 语义。
- **COW 私有 X 帧的归属分叉（登记）**：`USER_X_VA` 的 vspace 条目仍指向 `SEG_TASK1_X`
  的共享帧（`seg_id=SEG_TASK1_X, seg_offset=0`），而 COW 换映射后的私有帧归属 `SEG_COW`。
  这是真实的逻辑/物理分叉；不试图改写 `TaskVspaceTable`——那会替换共享映射并破坏
  `cow_ready.shared0` 的语义。内核当前无可观察该分叉的路径（不 `unmap`/`destroy` X）。
- **`unified` 无 host 覆盖（登记）**：host 测试不安装全局帧注册表，故 host 上该字段恒为
  假；权威证据只有 QEMU 的 `unified=1`（xtask 校验器已钉住）。
- **`capability_slot_capacity_is_enforced` 语义不变**：该测试基于 `fresh_state()`（不铸平台
  cap），故仍断言 60 条可分配槽。平台占用的 32..46 不改变该结论。
- **`HeldByKernel` 是规范模型的扩展（登记）**：§4.3 只列 Task 持有与消息携带两种位置；
  内核平台 actor 持有是新增位置类，已在 `CapabilityLocation` 上显式建模并有 host 覆盖。
- `ISSUE-FRAME-001` 保持 **Open**：image Segment 仍为 `(base,count)` 区间表达；本切片闭合的
  是 `memseg_create`/平台创建路径的真实帧向量部分。
- 未实现：S2-D4、S2-D5、`REFINEMENT-PROOF`、`dev_vspace`/W16.4。
- 既有 `cargo xtask test` 的 30s/无网络/精确序列语义不变。

## 6. 关闭口径与下一步

- **不勾选 S2-D1/…/X4 任何退出条件、阶段 2 维持「进行中」**：证据虽为真实内核 + QEMU，
  独立性仍为 harness 内，需范畴外第三方复核——与阶段 0/1 同一门槛，不为「收尾」放宽。
- `ISSUE-AUTH-002` 依据 §3 证据判为 **Fixed**，并在 §5 显式登记「机制偏差」与
  「cap 持有者为内核 actor、用户可达待 S2-D4/D5」两项边界。若把"用户可见"解读为"用户可达"，
  则该 ISSUE 仅为部分修复——此判断留待复核者裁定，证据与偏差均已列明，不做隐藏。
- 下一步候选：
  1. S2-D4（Task create/yield/destroy 用户原语）与 S2-D5（send/recv）——平台段 cap 的用户
     移交依赖它们；
  2. 用户可见的 Capability 移动见证（依赖 D4/D5 的外层操作）；
  3. `ISSUE-FRAME-001`（image Segment 转 `Vector<FrameId>`）；
  4. 阶段 2 各退出条件的范畴外复核。
