---
id: report-kernel-handle-table-proof-20261031
kind: report
authority: scoped-evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-31
---

# 内核有限能力槽表的 Lean 组合证明

## 结论与范围

将既有单物理槽证明推进到有限槽表：任意长度的串行表操作序列均可逐槽投影为合法生命周期，从而证明整表良构保持、跨槽隔离、永久退役，以及释放后的旧完整句柄永不再次 Live。

这是**手写有限模型的组合证明 + 真实 Rust 实现的有界 host 回归**，不构成从 Rust 自动抽取的 refinement proof，不宣称完整内核安全已证明。不修改生产内核行为、ABI、阶段 6 机制或阶段签署。

## 基线与契约链

- 基线：`59f4d8aa0c3087607b3ffc3638f49b6996abc42b`；开始时工作树干净。
- Lean 固定 `leanprover/lean4:v4.35.0`，只用 Lean 自带库；`autoImplicit=false`、`warningAsError=true`。
- Rust 使用仓库 pinned nightly；本次行为证据是 host 测试与裸机编译，未运行 QEMU/真机。
- 依据：[COSIR §18](../../../COSIR%20原语规范.md#18-从抽象规范到实现的细化要求)、[S2-X4/阶段跟踪](../../../Prima%20OS%20实现阶段.md)、[原单槽设计与本次增量](../specs/2026-10-24-kernel-handle-model-design.md)、[前序报告](2026-10-24-kernel-handle-model-report.md)、[权威映照](../../../lean/README.md)。

前序报告明确未覆盖整表。本报告只补强有限表组合层，不追溯改变其当时结论，也不关闭其余遗漏。

## 实现与证明

新增 `Formal.KernelHandleTable`，通过 `Formal.lean` 纳入默认构建。`Table size` 为 `Fin size → Slot`，句柄携带已解码的范围内索引与 generation；零槽仍拒绝。成功的 `allocateAt`/`releaseAt` 只更新目标槽，失败为 none、状态无提交。`Step` 包括分配、释放和未变；`Reachable` 表示任意有限长度序列。

分配允许任意空闲且未退役的非零槽，覆盖普通 `alloc` 选槽及 `slot_free` 守卫后的 `reserve`。未守卫地对不可用槽调用 Rust `reserve` 会断言失败；模型只对应其守卫后的成功路径，不把该断言改述为返回 none。

这是一种**安全性质的过近似**：证明不依赖选槽顺序，但没有证明最低槽选择和内部预留区间策略。表校验只对应 Rust `SlotState::Live`，不区分 Free 与 Stale。

| 一般定理 | 结论 |
|---|---|
| `allocateAt_effect` / `releaseAt_effect` | 成功操作具有目标槽单步语义，全部其他槽记录保持原值 |
| `step_projects` / `reachable_projects` | 每个表单步/序列投影成每个槽的合法单槽迁移/序列 |
| `reachable_preserves_wellformed` | 任意序列保持逐槽 generation 上界和退役不活动约束 |
| `allocate_returns_live` | 成功分配返回的句柄通过当前表校验 |
| `allocate_preserves_other_handle` / `release_preserves_other_handle` | 不同物理槽句柄的校验结果不受此次操作影响 |
| `retired_slot_stays_retired` | 任意后续整表操作均不恢复已退役槽 |
| `freed_handle_never_live` | 成功释放后，任意后续整表操作均不能让旧完整句柄再次有效 |

这 10 条是一般证明，不依赖 `native_decide`。另有三条求值锚，仅验证零槽分配/释放拒绝、其他槽保持 Live。`lean/Checks/KernelHandleTable.lean` 对全部一般定理执行 `#print axioms`，供后续重审。

Rust 增加 `kernel/src/handle_table_tests.rs`，由 `bootstrap.rs` 的仅测试模块载入，不改生产路径。直接使用真实 `CapabilitySlotTable::alloc/reserve/free/check`，枚举三槽、深度六的操作分支：普通最低槽分配、逐个可用槽固定保留、已发出句柄释放/重复释放、零和越界拒绝。注入末两代/退休状态避免执行 2^32 次循环；每次分配/释放核对非目标记录，每层检查所有已释放句柄不再 Live，失败核对整个 Rust 表未变化。

这是对实际实现的有界见证，不是所有 Rust 输入/序列穷举，也不是 Lean/Rust 自动双向差分。Lean 定理的序列长度和表容量均为参数，不受三槽六步限制。

## 命令与结果

原始日志位于运行环境 `/tmp/prima-table-*.log`；本报告保留输入、关键结论与范围，不提交依赖或生成物。

| 命令/输入 | 本次结果 | 范围 |
|---|---|---|
| 在 `lean/` 执行 `elan run leanprover/lean4:v4.35.0 lake clean` 后同版本 `lake build` | PASS：11 jobs，全部既有模块与新模块，零 warning | Lean 模型检查 |
| 同版本 `lake env lean Checks/KernelHandleTable.lean` | PASS：全部 10 条一般定理仅依赖 `propext`，部分另依赖 `Quot.sound`；无 `sorryAx` 或原生求值公理 | 公理依赖审计 |
| 临时副本删除求值锚，`replace` 改成对全表返回新 Slot | 预期 FAIL：一般 `allocateAt_effect` / `releaseAt_effect` 的非目标槽结论无法证明 | 非干扰反例 |
| 临时副本删除求值锚，`releaseAt` 改成把 generation 重置为 1 | 预期 FAIL：一般 `releaseAt_effect` 与单槽合法释放结果不一致 | 生命周期组合反例 |
| `CARGO_NET_OFFLINE=true cargo test -p kernel --lib table_traces_preserve_other_slots_and_never_revive_freed_handles` | PASS：真实槽表有界遍历 | host |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1045 项测试 + 7 doctest，共 1052；0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy `-D warnings`、裸机内核构建、36 文件内核零 unsafe | 编译/静态检查 |
| docs self-test / build / check / `check --base 59f4d8a` | PASS：338 篇文档；自检、目录、链接及基线变更检查通过 | 文档 |

两个 mutation 的正式源码始终不变。复现方法：复制新模块到临时文件，截去 `-- Concrete anchors;` 后的实例部分并保留 namespace 的 `end`；分别替换 `fun j => if j = i then s else t j` 为 `fun _ => s`，或将 `releaseAt` 的成功分支换成 `some (replace t h.index ⟨1, false⟩)`，在 `lean/` 用同版本 `lake env lean <临时文件>` 编译。必须检查一般定理的证明错误，不能仅凭 unused-variable warning 判定反例成立。

## 失败与未覆盖项

开发首轮证明未通过：替换函数的非目标分支需要显式使用索引不等式，`cases` 分支需正确绑定索引化构造子的隐式变量。修正证明脚本后通过；没有增加公理、sorry、修改模型语义或减弱目标。首次在仓库根误运行 Lake 因没有 lakefile 退出，随后均在 `lean/` 运行，不将其记为逻辑反例。

仍不覆盖：u64 编解码及越界输入的形式证明、选择策略、`peak_live`、权限/holder/location/目标存活、CapId/历史/审计提交、并发/中断线性化、任意直接改表或旧快照恢复、Rust 编译器语义及硬件。已证明的是允许迁移关系内的性质；不能将外层完整 Authority 检查省略为槽匹配。

未重跑 QEMU、Lean 之外的其他证明器或性能测试；新增 Rust 代码仅在 cfg(test) 下编译，生产行为不变。

## 阶段与重审

本报告补强 S2-X4 的模型证据，不改任何阶段退出勾选或 profile，不影响主分支阶段 6 的推进。若选槽/更新路径、generation 规则、重置/恢复、句柄表示或映照发生变化，须重新审查逐槽投影与实际 Rust 见证。下一条明确缺口仍是 Rust 源码到模型的语义对应/自动提取，而非把本次模型命名为完整内核证明。


同日[源码细化后继](2026-10-31-rust-handle-source-refinement.md)在两个生产句柄方法及 generation 核心上建立自动提取与细化定理；这只局部补强本报告的“源码映照”缺口，不将本报告的整表定理升级为真实 Rust 整表已证明。
