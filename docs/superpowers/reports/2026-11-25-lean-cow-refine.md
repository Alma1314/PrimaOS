---
id: report-lean-cow-refine
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# Lean `Formal.Cow`：COW 组合最小参考模型（BLK-001 后续切片）

## 结论与范围

本报告记录 [BLK-001 计划](../plans/2026-08-18-blk001-lean-formal-model.md) 中
“Lean 建模 cow_refine（组合序列，H1 承载体）”后续任务的落地：新增
`lean/Formal/Cow.lean`，把 cosir `cow_refine.rs` 的最小参考语义纳入
`Formal.Cow`，并新增 `lean/Checks/Cow.lean` 公理审计。25 条普适定理通过
`lake build`，公理依赖仅 `propext`（两条无公理）。

范围是主机 Lean 模型与 cosir host 测试的并列证据，不是 Rust 源码细化、内核机器
证明或 capability/异常恢复证明。未修改 Rust 源码、既有定理、阶段状态、profile 或
任何退出勾选。

## 基线与输入

- 基线：`5583cfc`（本切片另加 `fd844ac` 的 TLB 模型与 `01dc0fe` 的 Memseg
  模型）。本切片只新增 `lean/Formal/Cow.lean`、`lean/Checks/Cow.lean`，修改
  `lean/Formal.lean`、`lean/README.md` 与本报告；并行工作区的 `prima-user/c`
  改动不属于本报告范围。
- 计划：[BLK-001 计划](../plans/2026-08-18-blk001-lean-formal-model.md)的
  “Task 后续：Lean 建模 cow_refine”。
- Rust 参考语义：`cosir/src/cow_refine.rs`、`cosir/src/cow_refine_tests.rs`，
  以及其依赖的 `memseg_refine`/`tlb_refine`/`ops`。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 Cow 节。

## 实现

- 复用 `Formal.Memseg` 的 `Access`/`Segment`/`Mapping`/`setAt`；`Task` 只保留
  `status`（ready/running/faulted）与 `vspace` 函数。
- `State`：`segments`、`tasks`、`current`、`pending`、`cows`（显式 COW 对列表）。
- `establishCow`：父 RW 映射变只读、子新增只读别名、追加 COW 对；父缺失映射、
  子已映射、父非 RW、自指等返回原状态。
- `access`：仅在 current 且 Running 时翻译；未映射→`present=false` 故障；无权限
  →`present=true` 故障；故障置任务 Faulted、清 `current`、记录 pending；允许路径
  不改状态。
- `privatize`：要求 Faulted、非 current、同 va 的可写故障、旧映射只读；换成私有段
  的 RW 映射、任务回 Ready、pending 清除；COW 对列表不变。
- `schedule`/`yieldTask`/`destroyTask`/`destroySegment`：单运行任务、CPU 忙拒绝、
  非 current/非 Faulted 才可销毁；段销毁的 `StillMapped` 仍是调用方守卫。
- 不变量：`RunningOk`（current 与 Running 一致且至多一个）、`FaultedOk`
  （Faulted ⇔ pending 且非 current）、`CowPairOk`（同段同偏移⇒双方只读；否则
  映射帧不同）、`CowsOk`。

已证性质（25 条普适定理，均非 `native_decide`）：

- 建立：`establishCow_ok`、`establishCow_cows`、`readOnly_write`；失败面
  `establishCow_missing`/`_conflict_child_mapped`/`_parent_not_rw` 返回原状态。
- 访问：`access_read_allowed`、`access_write_fault`、`access_unmapped_fault`、
  `access_rejects_not_current`/`_no_task`。
- 私有化：`privatize_ok`、`privatized_task`（Ready/pending 清/COW 列表不变）、
  `privatize_wrong_va`/`_not_faulted`。
- 调度与销毁：`schedule_ok`/`_busy`、`yieldTask_ok`、`destroyTask_ok`/
  `_removed`/`_keeps_other`/`_rejects_faulted`、`destroySegment_still`/`_ok`。

`#eval` 锚覆盖：建立成功与 COW 长度、父缺失/自指冲突、子映射只读、读允许、
写故障（present=true）、未映射故障、非当前拒绝、私有化回 Ready 且 pending 清除、
调度忙/空闲、销毁任务与段守卫。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (14 jobs)`，exit 0 | Lean 模型构建 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Cow.lean`（`lean/`） | PASS；25 条定理仅 `propext`（两条无公理），无 `sorryAx`/native | 公理审计 |
| `cargo test -p cosir cow` | PASS；71 passed、0 failed | Rust host 见证（既有测试复核） |

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- 抽象边界：不建模 capability 权限/持有者校验、异常源与订阅、SRET 恢复帧、
  `exc_resume` 校验、帧级全不交检查、TLB 失效、Dead 任务/wait，或真实 CR3 操作。
- `destroyTask` 对 COW 对的 `filterCows` 实现未证其成员性质（只由 `#eval` 锚
  覆盖）；`destroySegment` 的 `StillMapped` 仍是调用方 Bool 守卫。
- 证据等级：Lean 侧为参考语义（MODEL-TEST / REFINEMENT-DESIGN 通道）；Rust
  测试是独立 host 见证。没有自动抽取、双向差分或 Rust refinement proof。
- 本次未改 Rust，未重跑 workspace 全量、QEMU 或 lint；这些不在本次改动范围内。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile、ABI 或既有 COW/Gate 边界。
`cow_refine` 从 [lean/README.md](../../../lean/README.md) 的“已知不覆盖”移入
最小已覆盖；细化层三个宿主（TLB/Memseg/COW）至此都有最小参考模型，但都不构成
7-B/7-C/7-D/7-H 证据。

## 后继关系

- 计划：[BLK-001](../plans/2026-08-18-blk001-lean-formal-model.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- Rust 语义：[cow_refine.rs](../../../cosir/src/cow_refine.rs)。
