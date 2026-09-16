# 切片 Q 报告：阶段 2 启动——句柄编码、Capability 生命周期、Segment 原语

> 文档类型：实施报告（非设计、非规范、非证明）
>
> 日期：2026-09-16
>
> 对象：`kernel/src/{handle,bootstrap,rights,frames,mem,vspace,syscall,user,resume,protocol,config}.rs`、
> `xtask/src/protocol.rs`
>
> 目的：交付 S2-D1（不可伪造且不复用的句柄表示）、S2-D2（Capability 派生/移动/drop 与 stale
> 检测用户化）、S2-D3（Segment create/map/unmap/destroy），并回收帧归属/空表结点

## 0. 结论速览

- **S2-D1**：`Handle = (generation << 32) | index`，generation 从 1 起；Capability 槽位可复用，
  释放时 generation 递增 → 旧句柄永久失效。QEMU 实测：初始根 cap 为 `0x100000001/2/3`，
  异常源 cap 为 `0x100000004/5`，裸整数不是合法句柄（`malformed_rejected=1`）。
- **S2-D2**：`SYS_CAP_DERIVE`/`SYS_CAP_DROP` 用户可调；QEMU 实测
  `derive=0x100000006`、越权 `EACCES`、`drop=0`、重复 drop `ENOENT`、
  **同槽复用后旧句柄 `ENOENT`（S2-X4）**、live 句柄授予、结构性根 drop `EACCES`。
- **S2-D3**：`memseg_create/map/unmap/destroy` 用户可调 + 真实 PTE 装/卸 + 空表结点回收 +
  帧归还；QEMU 实测 create/map/读零/写回/unmap/destroy 全部成功，销毁后 map 被拒（stale），
  再次 create 复用回收帧并读回 0（S2-X3）。
- `frames_ready` 新增 `disjoint=1`；守恒式在取帧/归还/页表回收后仍成立
  （`free 30779 + segment 16 + table 24 + image 682 = universe 31501`）。
- **未完成**：bring-up 13 区域未迁移为真实 Segment 对象 → `ISSUE-AUTH-002` 保持 Open
  （与计划的一处偏差，见 §5）。
  > 后续：切片 R（SPEC-063，2026-09-16）已完成该迁移——13 区域成为真实 `SegmentObject`
  > （id 2..14，内核持有的可授权根 cap），帧标签统一为 Segment id，11 个用户可见区域改经
  > `memseg_map`。详见 `2026-09-16-stage2-bringup-segment-migration-report.md`。
- **不勾选任何阶段退出条件、不宣告阶段 2 关闭**（组织级独立性门槛，见 §6）。

## 1. 方法与独立性声明

- 全部为真实内核实现 + QEMU 实测 + host 单测；无机器证明。
- 本报告复核独立性为 **harness 内会话级别**，**不是组织级第三方审查**，也不构成
  `REFINEMENT-PROOF`；不改变阶段 0/1/2 的独立性门槛。

## 2. 实现要点

| 面 | 变化 |
|---|---|
| 句柄 | 新增 `handle.rs`：严格 `(gen << 32) \| index`、`Malformed` 拒绝、`ZERO` 哨兵 |
| 槽位 | `CapabilitySlotTable`：最小空闲槽分配、释放递增 generation、`Free/Stale/Live` |
| CapRecord | 逐槽索引的 `CapabilityRecordTable` / `CapabilityPositionTable`；`parent: Option<Handle>`（铸造根 vs 派生） |
| 目标存活 | `ObjectRef` + `target_live`；`AuthError::{StaleHandle, TargetDead, MalformedHandle}` |
| 派生/丢弃 | `derive_capability(caller, parent, rights)` 内核分配子槽；`drop_capability` 不检查目标存活、保护结构性根 |
| 授权 | `authorize` 检查序：槽位 → 目标存活 → 持有者 → 权限 |
| 帧归属 | owner 表 + `owner_of`/`release_frame`/`release_table_frame`/`close_segment`/`check_disjoint` |
| 清零 | `mem::zero_frame`（S2-X3，对**回收复用帧**同样生效） |
| 表回收 | `vspace::unmap_4k`：清叶 + 自底向上回收变空表帧；`VspaceError::NotMapped` |
| Segment | `SegmentFrames::{Contiguous,Vector}`；`SegmentObject` 增 generation/active；`memseg_*` 四原语 |
| vspace 视图 | `TaskVspaceTable`（overlap / `StillMapped` / W7） |
| syscall | `SYS_CAP_DERIVE=11`、`SYS_CAP_DROP=12`、`SYS_MEMSEG_{CREATE=13,MAP=14,UNMAP=15,DESTROY=16}` |
| 启动栈 | `config.rs` 显式 256KiB（默认 80KiB 在扩容后溢出，QEMU 实测三重故障） |

## 3. 实测证据（2026-09-16）

```
PRIMA|v=1|seq=20|event=handle_ready|status=ok|data=index_bits=32;generation_bits=32;image=0x100000001;namespace=0x100000002;task=0x100000003;page_fault=0x100000004;syscall_trap=0x100000005;malformed_rejected=1
PRIMA|v=1|seq=21|event=cap_lifecycle_ready|status=ok|data=derive=0x100000006;escalate=0xfffffffffffffff3;drop=0x0;drop_again=0xfffffffffffffffe;reuse=0x200000006;stale=0xfffffffffffffffe;grant=0x200000006;drop_new=0x0;structural=0xfffffffffffffff3
PRIMA|v=1|seq=22|event=memseg_ready|status=ok|data=create=0x300000006;map=0x0;zero=0x0;write=0xdec0de0012345678;unmap=0x0;destroy=0x0;stale=0xfffffffffffffff3;create2=0x100000007;map2=0x0;zero2=0x0;unmap2=0x0;destroy2=0x0
PRIMA|v=1|seq=26|event=frames_ready|status=ok|data=universe=31501;free=30779;segment=16;table=24;image=682;conserved=1;disjoint=1
```

解读：

- `handle_ready.image=0x100000001`：槽 1 的 gen1 句柄 ≠ 裸索引 `1`；`malformed_rejected=1`
  由真实 `Handle::decode(999)` 计算（generation 为 0 被拒）。
- `cap_lifecycle_ready.reuse=0x200000006`：槽 6 释放后复用，generation 1→2；
  `stale=ENOENT` 证明**旧句柄 `0x100000006` 未因复用重获 Authority**（S2-X4）；
  `structural=EACCES` 证明活动 Task 的结构性根 Namespace cap 不可丢弃。
- `memseg_ready.zero=0`：新 Segment 首帧在首次可见前已清零；
  `write=0xdec0de0012345678` 读回一致，证明装的是**真实可写 PTE**；
  `stale=EACCES` 证明 `memseg_destroy` 后原 seg cap 变 stale token；
  `create2=0x100000007` + `zero2=0` 证明**归还帧被复用且复用前已清零**（S2-X3）。
- `frames_ready`：`free + segment + table + image = 31501 = universe` 且 `disjoint=1`，
  说明 create→destroy 与 map→unmap 的取帧/页表帧回收净零，W6 守恒在归还路径下成立。
  `segment=16`/`table=24` 与阶段 1 相同；`image=682` 随内核镜像增大而变化。

验证命令与结果：

- `cargo test --workspace`：cosir 145、cosir_state 40、lean_backtrace 2、kernel 240、xtask 130 全绿。
- `cargo clippy --workspace --all-targets -- -D warnings` 零告警；`cargo fmt --all --check` 无差异。
- `cargo xtask test`：BIOS run=1/2 六条 PASS，27 条事件按新序列与 data 断言。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 4. 既有事件的变化（显式登记，不隐藏）

| 事件/回执 | 变化与原因 |
|---|---|
| `bootstrap_ready` | `image_cap/namespace_cap/task_cap` 由裸索引改为严格句柄（§18 第 1 条） |
| `syscall_auth_ready` | `granted_ret` 由 `1` 改为 `0x100000001`；`unknown_ret` 仍是 `ENOENT`（未分配槽的合法格式句柄） |
| `authority_ready` | `root_ns` 由 `2` 改为 `0x100000002` |
| `frames_ready` | 新增 `disjoint=1`；守恒字段不变 |
| 事件序列 | 新增 `handle_ready`(20)/`cap_lifecycle_ready`(21)/`memseg_ready`(22)，既有 `preempt_ready` 起顺移 |
| `user_code` | 622 → 1478 字节；阶段二序列完整保留（偏移 0..932 不变），新序列追加在 NOP 对齐之后 |
| 启动栈 | `config.rs` 由默认 80KiB 改 256KiB；直接原因：`BootstrapState` 扩容后
  `create_state`（按值返回）+ `install` 的组合栈帧在 QEMU 上触发 #PF→双重故障→三重故障，
  且 `xtask` 只看到 QEMU 退出码 0。已在 `bootstrap::tests::bootstrap_state_stays_within_the_stack_budget`
  钉住结构大小上限（16KiB）防止再次悄悄超栈 |
| `exc_ready` | 不变（返回 source_id，非句柄） |

以上均按维护规则 5 记录，**不视为回归、也不隐藏**。

## 5. 边界与反例保留（含一处未完成项）

- **未完成：bring-up 13 区域未迁移为真实 Segment 对象。** 用户在计划确认时选择「一并迁移」，
  但本轮把预算用于 D1/D2/D3 的真实原语与证据；`ISSUE-AUTH-002` 保持 **Open**，验收条件
  （由 `memseg_create` 接管 bring-up 区域并铸造可授权 Capability）尚未满足。为使 `memseg_create`
  不与被占用的 bring-up 帧标签冲突，用户 Segment 暂用独立标签命名空间（`100 + id`）。
  > 后续：切片 R（SPEC-063，2026-09-16）已完成该迁移——13 区域成为真实 `SegmentObject`
  > （id 2..14，内核持有的可授权根 cap，槽 32..44），`100+id` 命名空间删除、归属标签与
  > Segment id 恒等（QEMU `unified=1`），11 个用户可见区域改经 `memseg_map` 授权中介映射。
  > 两项偏差已登记：平台路径共用构造核而非 `memseg_create` 入口；cap 的用户可达移交待
  > S2-D4/D5。详见 `2026-09-16-stage2-bringup-segment-migration-report.md`。
- image Segment 仍为 `Contiguous` 区间，不是 `Vector<FrameId>`；`ISSUE-FRAME-001` 只在
  `memseg_create` 路径闭合。
- `SegMapX` 只记录权限与逻辑视图，未做 NX 强制（未配置 `EFER.NXE`）。
- `memseg_map/unmap` 只在调用者自身 Task 上实证；跨 Task `TaskManageVSpace` 未覆盖。
- `dev_vspace` 与 W16.4 跨表互斥未建模。
- **`memseg_create` 的失败回滚路径未逐条实测**：`create` 的容量/授权分支由 host 逻辑与 QEMU
  主路径覆盖；QEMU 主路径早先暴露的 `NoMemory`（帧标签冲突）已修复并登记为过程证据。
- **无 host 单测覆盖 `memseg_map/unmap`**：该路径必须真实 CR3/页表，仅 QEMU 证据。
- 一处实现期缺陷值得保留：cap 生命周期块末字节落在 931，而 memseg 块从 933 开始，932 的零字节
  被 CPU 当作 `add [rax], al` 执行 → #PF 跳转错位，导致首个 `MEMSEG_CREATE` 以旧 `rax` 取号、
  返回 `ENOSYS`。修复为显式 NOP 对齐并在黄金测试中断言；该缺陷与修复保留在案。
- 未实现：S2-D4、S2-D5、`REFINEMENT-PROOF`、`memseg_map` 的 `dev_vspace` 互斥。
- 既有 `cargo xtask test` 的 30s/无网络/精确序列语义不变。

## 6. 关闭口径与下一步

- **不勾选 S2-D1/…/X4 任何退出条件、阶段 2 维持「进行中」**：证据虽为真实内核 + QEMU，
  独立性仍为 harness 内，需范畴外第三方复核——与阶段 0/1 同一门槛，不为「收尾」放宽。
- 下一步候选：
  1. bring-up 13 区域迁移为真实 Segment 对象并铸造可授权 Capability（关 `ISSUE-AUTH-002`）；
  2. S2-D4（Task create/yield/destroy）与 S2-D5（send/recv）用户原语；
  3. 用户可见的 Capability 移动见证（依赖 D4/D5 的外层操作）；
  4. 阶段 2 各退出条件的范畴外复核。
