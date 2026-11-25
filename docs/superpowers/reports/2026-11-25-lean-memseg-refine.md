---
id: report-lean-memseg-refine
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# Lean `Formal.Memseg`：双视图与帧归属最小参考模型（BLK-001 后续切片）

## 结论与范围

本报告记录 [BLK-001 计划](../plans/2026-08-18-blk001-lean-formal-model.md) 中
“Lean 建模 memseg_refine”后续任务的落地：新增 `lean/Formal/Memseg.lean`，把 cosir
`memseg_refine.rs` 的最小参考语义纳入 `Formal.Memseg`，并新增
`lean/Checks/Memseg.lean` 公理审计。29 条普适定理通过 `lake build`，公理依赖仅
`propext`/`Quot.sound`。

范围是主机 Lean 模型与 cosir host 测试的并列证据，不是 Rust 源码细化、内核机器
证明、四级页表证明或 SMP 证据。未修改 Rust 源码、既有定理、阶段状态、profile 或
任何退出勾选。

## 基线与输入

- 基线：`5583cfc`（本切片另加 `fd844ac` 的 TLB 模型）。本切片只新增
  `lean/Formal/Memseg.lean`、`lean/Checks/Memseg.lean`，修改 `lean/Formal.lean`、
  `lean/README.md` 与本报告；并行工作区的 `prima-user/c` 改动不属于本报告范围。
- 计划：[BLK-001 计划](../plans/2026-08-18-blk001-lean-formal-model.md)的
  “Task 后续：Lean 建模 memseg_refine（W6/W7 双视图）”。
- Rust 参考语义：`cosir/src/memseg_refine.rs`、`cosir/src/pagewalk.rs`、
  `cosir/src/memseg_refine_tests.rs`。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 Memseg 节。

## 实现

- `TaskCtx.table`/`vspace`：页表叶子与视图均为函数；`ViewsAgree` 是逐地址的
  投影一致。`mapEntry`/`unmapEntry` 同时更新两者。
- `Pte`：保留 W7 检查的 present/user/writable/nx/frame/mapping；`makePte` 从
  段帧与 access 构造 PTE，`PteOk` 断言派生位、帧偏移与 `access ⊆ allowed`。
- `Segment`/`Access`：帧为顺序列表（`nth?` 索引），权限为三个 Bool，
  `Access.valid`/`Access.sub` 为 Bool 版本。
- `State`：`frameUniverse`、`free`、单值 `owner : Frame → Option Owner`（free/
  segment/table）表达 W6 的唯一归属；段帧与表帧互斥由函数单值性自动成立。
- `mapEntry`/`unmapEntry`：单页原语，失败原样返回状态；entry 页先于未映射检查。
- `createSegment`/`destroySegment`：帧集由调用方选择（过近似分配器）；销毁时
  `StillMapped` 是调用方 Bool 守卫；成功时帧在 `Owner.segment` 与 `Owner.free`
  之间转移。
- `destroyTask`/`spawnTask`：表帧在 free 与 `Owner.table` 之间转移。

已证性质（29 条普适定理，均非 `native_decide`）：

- 双视图：`viewsAgree_map`、`viewsAgree_unmap`；`mapEntry_success_views`、
  `unmapEntry_success_views` 接到成功返回的新 Task。
- 权限：`makePte_ok` 证明 PTE 派生位、帧匹配与权限子集。
- W6 转移：`createSegment_owned`/`createSegment_other_frame`、
  `destroySegment_freed`/`destroySegment_removed`、`destroyTask_freed`、
  `spawnTask_ok`。
- 守卫与原子性：`mapEntry_no_task`/`_invalid_access`/`_invalid_segment`/
  `_overlap`、`unmapEntry_entry`/`_not_mapped`、`createSegment_fail_*`、
  `destroySegment_still`；成功方程 `*_ok` 钉住精确状态。

`#eval` 锚覆盖：创建/销毁的 owner 转移、map 成功与 invalidAccess/rightsExceeded/
outOfBounds/invalidSegment/overlap、unmap 的 entryMapping/notMapped、spawn 表帧
转移与 destroyTask 归还。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (13 jobs)`，exit 0 | Lean 模型构建 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Memseg.lean`（`lean/`） | PASS；29 条定理仅 `propext`/`Quot.sound`，无 `sorryAx`/native | 公理审计 |
| `cargo test -p cosir memseg_refine_tests` | PASS；36 passed、0 failed | Rust host 见证（既有测试复核） |

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- 抽象边界：不建模四级页表树、`check_frame_ownership`/表帧计数、多页原子
  map/unmap、地址 canonical/对齐、`next_seg` 溢出、分配器顺序、帧清零或真实
  CR3 操作。
- `StillMapped` 视图枚举与任务集合不在模型中，作为调用方 Bool 守卫；模型证明
  的是“干净守卫下销毁后帧归还与段移除”，不证明扫描本身。
- 证据等级：Lean 侧为参考语义（MODEL-TEST / REFINEMENT-DESIGN 通道）；Rust
  测试是独立 host 见证。没有自动抽取、双向差分或 Rust refinement proof。
- 本次未改 Rust，未重跑 workspace 全量、QEMU 或 lint；这些不在本次改动范围内。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile、ABI 或既有 COW/Gate 边界。
`memseg_refine` 从 [lean/README.md](../../../lean/README.md) 的“已知不覆盖”移入
最小已覆盖；`cow_refine` 仍待后续切片。

## 后继关系

- 计划：[BLK-001](../plans/2026-08-18-blk001-lean-formal-model.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- Rust 语义：[memseg_refine.rs](../../../cosir/src/memseg_refine.rs)。
