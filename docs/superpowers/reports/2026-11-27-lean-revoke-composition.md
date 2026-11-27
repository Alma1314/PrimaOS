---
id: report-lean-revoke-composition
kind: report
authority: scoped-model-evidence
status: validated-with-limits
lifecycle: historical
updated: 2026-11-27
---

# Lean 撤销不变量的连续操作闭合修复

## 结论与范围

修复 `Formal.Revoke` 的证明衔接缺口：原定理要求 `WellFormed`，但撤销后仅能得到
`ActiveWellFormed`，不能直接用于下一次撤销。新增 `RevokeWellFormed` 并证明任意
有限 `revoke` 序列保持它。执行定义 `revoked`/`revoke` 不变，原定理名称及类型保留。
这是 Lean 参考模型的证明补强，不是 Rust 细化、内核撤销修复或阶段签署。

## 基线与输入

- 基线：`ce33f03`；开始时工作区干净。
- 工具链：`elan run leanprover/lean4:v4.35.0`，纯 Lean 自带库。
- 契约：[COSIR §4.2/§4.7](../../../COSIR%20原语规范.md)、[白皮书 §6.2](../../../Prima%20OS.md)。
- 设计/执行背景：[BLK-001 设计](../specs/2026-08-18-blk001-lean-formal-model-design.md)、[历史计划](../plans/2026-08-18-blk001-lean-formal-model.md)。
- 前序证据：[11-25 撤销模型](2026-11-25-lean-revoke.md)。本报告只补强连续撤销证明。

## 根因与修复

`WellFormed` 要求所有派生记录的父活动；撤销父子后，这个条件不成立。
`ActiveWellFormed` 只约束活动记录，无法单独保证非活动父链的燃料完备性。

- `ParentOrdered` 单独要求所有记录的父 ID 严格递减，包括非活动记录。
- `RevokeWellFormed = ParentOrdered ∧ ActiveWellFormed`；初始 `WellFormed` 蕴含它。
- 子树完备性新增 `_ordered` 入口，只依赖 `ParentOrdered`；原入口仍接受 `WellFormed`。
- `revoked_preserves_ordered`、`revoked_preserves_active_wf` 给出组合保持性；
  `revoke_preserves_wf` 同时覆盖成功/失败，`revoke_sequence_preserves_wf` 对列表归纳。
- 可检查回归：`revState_wellformed`；撤销后不满足原 `WellFormed` 的反例；
  根 1、无关根 4、重复根 1 的连续撤销仍满足组合不变量。

## 命令与结果

原始输出保留在 `/tmp/lean-fix-*.log`，不入库。

| 命令 | 结果 | 范围 |
|---|---|---|
| `elan run leanprover/lean4:v4.35.0 lake build`（`lean/`） | PASS，exit 0，21 jobs | 全部主模型与回归证明 |
| `elan run leanprover/lean4:v4.35.0 lake env lean Checks/Revoke.lean`（`lean/`） | PASS，exit 0；36 条定理，仅标准 `propext`/`Quot.sound` 或无公理 | 覆盖所有命名撤销定理，无 sorry/native 依赖 |
| 全部 `lean/Checks/*.lean` 逐文件执行同一 `lake env lean` 命令 | PASS，11 个文件，exit 0 | 主模型公理检查文件 |
| `cargo test --workspace` | PASS，exit 0 | Rust host 回归 |
| `cargo xtask lint` | PASS，exit 0 | 格式、Clippy、裸机构建、unsafe/边界检查 |
| `python3 scripts/check_handle_refinement.py` | FAIL，exit 1：`missing or unpinned tool: target/aeneas/aeneas` | 独立源码细化工具前置条件未满足，未完成该通道复跑 |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base ce33f03` | PASS，exit 0 | 文档治理 |

## 失败与限制

修复前主模型与 11 个 Checks 文件均能构建，问题是定理的前后置条件不闭合，
不是原有构建失败。回归明确证明撤销后不满足原前提，避免用编译通过替代组合证明。
开发中使用 Lean 自带库没有的 `split_ifs` 导致局部构建失败，改为显式分支后通过。

仍不覆盖并发、跨节点、容器回收、审计或 Rust 源码细化。独立 Aeneas 检查因本地
工具缺失或哈希不匹配而停止；未下载或改写工具快照。未运行 QEMU：没有 Rust/启动路径改动。
`ISSUE-REVOKE-CONTAINER-001` 仍开放，见[架构复核](2026-11-27-architecture-isa-re-review.md)。

## 阶段与后继关系

[阶段跟踪](../../../Prima%20OS%20实现阶段.md)仅增加本次证据指针；阶段、7-D 与其它
退出勾选不变。[Lean 映照](../../../lean/README.md)登记新增不变量及边界。
本报告不取代 11-25 的运行语义或 11-27 架构复核的容器差异结论。

## 同日后继：本地细化工具恢复与离线验收

用户要求修复前置条件后，确认根因是整个 `target/aeneas/` 和独立 Lake 依赖缓存缺失，
而非已安装二进制哈希不匹配。既有提取 Rust `nightly-2026-09-17`（含 rustc-dev/rust-src）
和 Lean 4.35.0 已安装；沿用锁定版本，未改工具锁、生成文件或生产 Rust 源码。

执行仓库现有 `python3 scripts/ci.py formal --prepare-inputs`，从锁定官方 URL 恢复
发布包。归档 SHA256 为 `0963dc77cab890de16b213b903091721961afca6303b9b0ea05ff8d76013f624`，
三个二进制分别通过 `toolchain.lock.json` 的 SHA256 检查；九个 Git 依赖均匹配
`lake-manifest.json` 的提交且工作树干净。准备并解压 1478 个已有 mathlib 缓存文件，
`lean/refinement/.lake/packages` 链接到本地后端缓存。所有工具、归档与依赖留在
ignored `target/` / `.lake/`；README 补上准备和验收命令。

| 命令 | 本次结果 | 范围 |
|---|---|---|
| `python3 scripts/ci.py formal --prepare-inputs` | 重试 PASS，exit 0 | 锁定工具、提取 Rust/Lean 与后端依赖准备 |
| `python3 scripts/ci.py formal` | 最终 PASS，exit 0；三个步骤均 exit 0 | 离线主模型、句柄表公理检查、生产源码重新提取/生成物逐字节新鲜度、五条细化定理及公理审计、语义 mutation |
| `python3 scripts/check_handle_refinement.py` | PASS，exit 0；`Build completed successfully (1802 jobs)` | 最初失败的直接检查命令已恢复，无需额外指定工具链环境 |

关键输出：`PASS source mutation (generation reset to 1) rejected by Lean proof` 与
`PASS Rust source extraction, generated-code freshness, refinement and axiom audit`。
mutation 仅修改临时副本，真实源码不变；五条细化定理仅依赖标准
`propext`/`Classical.choice`/`Quot.sound`。

保留过程失败：首次沙箱内准备因 rustup 的受限环境退出；联网准备在归档尾部读取超时。
断点恢复剩余字节并通过完整归档哈希后，重试准备成功，没有跳过校验。首次 formal
的三个检查步骤全部通过，但诊断性 Python 导入并行生成了未跟踪的
`scripts/__pycache__/ci.cpython-312.pyc`，触发 `source changed during validation`；
清理该诊断产物并停止并行修改后，同一命令完整通过。原始输出保留于
`/tmp/lean-tools-*.log` 与 ignored `target/ci/formal/`，不入库。

本后继解除本报告前文的本地细化工具阻塞，保留原失败历史；不扩大两个 Rust 方法的
源码细化范围，不改变阶段或退出勾选，`ISSUE-REVOKE-CONTAINER-001` 仍开放。
