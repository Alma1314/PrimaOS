---
id: report-lean-tlb-refine
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# Lean `Formal.Tlb`：TLB 缓存最小参考模型（BLK-001 后续切片）

## 结论与范围

本报告记录 [BLK-001 计划](../plans/2026-08-18-blk001-lean-formal-model.md) 中
“Lean 建模 tlb_refine”后续任务的落地：新增 `lean/Formal/Tlb.lean`，把 cosir
`tlb_refine.rs` 的最小参考语义纳入 `Formal.Tlb`，并新增 `lean/Checks/Tlb.lean`
公理审计。31 条普适定理通过 `lake build`，公理依赖仅 `propext`/`Quot.sound`。

范围是主机 Lean 模型与 cosir host 测试的并列证据，不是 Rust 源码细化、内核机器
证明、四级页表证明或 SMP/TLB shootdown 证据。未修改 Rust 源码、既有定理、阶段
状态、profile 或任何退出勾选。

## 基线与输入

- 基线：`5583cfc`。本切片只新增 `lean/Formal/Tlb.lean`、`lean/Checks/Tlb.lean`，
  修改 `lean/Formal.lean`、`lean/README.md` 与本报告；并行工作区中
  `prima-user/c` 的改动不属于本报告范围。
- 计划：[BLK-001 计划](../plans/2026-08-18-blk001-lean-formal-model.md)的
  “Task 后续：Lean 建模 tlb_refine”。
- Rust 参考语义：`cosir/src/tlb_refine.rs`、`cosir/src/pagewalk.rs`、
  `cosir/src/tlb_refine_tests.rs`。
- 映照契约：[lean/README.md](../../../lean/README.md) 的 2026-11-25 节。

## 实现

- `CachedXlat`：物理帧 + 三个独立权限布尔；不表达 8 位掩码或 u64 ABI 位值。
- `TlbState := TaskId → VAddr → Option CachedXlat`：只做点查询/更新，对应
  Rust 的每任务 `BTreeMap`。
- `insert`/`invalidateVa`/`flushAll`/`removeTask`：填表、单地址失效、全量刷新、
  任务移除；后两者对不存在的缓存幂等，`removeTask` 无存在性前置。
- `translate`：命中直接返回缓存（可有陈旧值），未命中读抽象页表并填充，未映射
  不填充；失败路径原样返回输入 TLB。
- `invalidate`/`flush`：与 `translate` 相同的守卫顺序，失败不修改 TLB。
- `FreshFor`：缓存项与页表一致的证明谓词；陈旧命中正是该谓词失败的缓存项。
- 地址 canonical/对齐与任务存在被抽象为 `validAddr`/`taskExists` 守卫；页表被
  抽象为 `pt : TaskId → VAddr → Option CachedXlat`。

已证性质（31 条普适定理，均非 `native_decide`）：

- 点更新隔离：`insert_same`/`insert_other`；`*_other_task`/`*_other_va` 表明
  失效、刷新、移除只影响目标 Task 或地址。
- 幂等：`invalidate_idempotent`、`flush_idempotent`、`removeTask_idempotent`。
- 错误面：`invalidate_*`、`flush_*`、`translate_*` 覆盖成功、非法地址、未知任务、
  未映射；失败返回输入 TLB。
- 命中与填充：`translate_hit`、`translate_miss`、`translate_not_mapped`。
- 陈旧性边界：`translate_after_invalidate`——有效地址与存活任务下，单地址失效
  后的下一次翻译完全由当前页表决定。
- 新鲜性保持：`invalidate_fresh_for`、`flush_fresh_for`、`removeTask_fresh_for`、
  `translate_miss_fresh`。
- 任务隔离：`insert_preserves_other_task`；`translate` 的唯一状态变化是 `insert`。

`#eval` 锚覆盖：未命中填充与只读权限、陈旧命中（旧帧 `0x9000`）、失效后新帧
（`0xA000`）、单地址/单任务作用域、刷新与移除清空、三类错误面与权限位矩阵。

## 命令与结果

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS；`Build completed successfully (12 jobs)`，exit 0 | Lean 模型构建 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Tlb.lean`（`lean/`） | PASS；31 条定理仅 `propext`/`Quot.sound`，无 `sorryAx`/native | 公理审计 |
| `cargo test -p cosir tlb_refine_tests` | PASS；16 passed、0 failed | Rust host 见证（既有测试复核） |

原始输出保存在 `/tmp/`；本报告只保留命令、退出码与必要摘录。

## 失败与限制

- 抽象边界：不建模四级页表、位级 canonical/对齐、帧所有权与
  `check_invariants`、TLB 替换策略或容量、真实 CR3/INVLPG/INVPCID。
- 并发/平台：不覆盖 TLB shootdown、多核顺序、弱内存模型或硬件页表遍历；不构成
  7-B/7-H 证据或前置完成。
- 证据等级：Lean 侧为参考语义（MODEL-TEST / REFINEMENT-DESIGN 通道）；Rust
  测试是独立 host 见证。没有自动抽取、双向差分或 Rust refinement proof。
- 本次未改 Rust，未重跑 workspace 全量、QEMU 或 lint；这些不在本次改动范围内。

## 阶段/契约影响

不改变阶段 7 状态、九轨退出勾选、profile、ABI 或既有 COW/Gate 边界。
`tlb_refine` 从 [lean/README.md](../../../lean/README.md) 的“已知不覆盖”移入最小
已覆盖；`memseg_refine` 与 `cow_refine` 仍待后续切片。

## 后继关系

- 计划：[BLK-001](../plans/2026-08-18-blk001-lean-formal-model.md)。
- 映照：[lean/README.md](../../../lean/README.md)。
- Rust 语义：[tlb_refine.rs](../../../cosir/src/tlb_refine.rs)。
