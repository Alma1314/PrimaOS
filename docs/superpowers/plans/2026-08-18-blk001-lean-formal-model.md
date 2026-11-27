---
id: docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md
kind: plan
authority: historical
status: execution-record
lifecycle: historical
updated: 2026-11-27
---

# BLK-001 Lean 4 形式化模型工件 Implementation Plan

本页保留初建 Lean 工件的目标、依赖、选择与局限；不是当前证明清单或安装指南。原 803 行草稿可由 `git show 5ebf2791fcf01f21cf6e396227483e74ef1b2dd9:docs/superpowers/plans/2026-08-18-blk001-lean-formal-model.md` 恢复；哈希见[整理记录](../reports/2026-11-27-documentation-consolidation.md)。上位来源是 [BLK-001 设计](../specs/2026-08-18-blk001-lean-formal-model-design.md)，模型字段、现行固定工具链、构建与不覆盖项统一由 [Lean 映照契约](../../../lean/README.md)维护。

## Global Constraints

- 原目标是独立 Lake 包中可执行的 bootstrap/exc_resume，加 Rust host 夹具对拍，支持 BLK-001/S0-X3；不以此代替独立规格审查 S0-X1。
- Lean 以 `CapId → Option Capability` 表示能力表、`Option State` 表示成功/失败，none 不返回新态；Rust 对拍只属 MODEL-TEST，Lean 证明为独立通道，不是完整 Rust/内核细化、KERNEL-TRACE、真实硬件或全量 §17。
- 初始候选 Lean 4.33 + elan/stable、默认不引入 mathlib；旧模板又写 std4 main 与无网络回退。现行版本/依赖按锁文件与映照契约复现，不继续使用浮动 stable/main 或 Windows 安装路径。
- Lake 包独立于 Rust workspace，不新增 Rust 第三方 crate。映照漂移须登记，计划中的“关闭阻塞/勾选”必须以实际验收和阶段规则为前提。

## File Map

当时创建 `lean/lakefile.lean`、toolchain、`Formal.lean` 入口、`Formal/State.lean`、Bootstrap/ExcResume 模块和映照 README；Rust 侧先给 cosir 增 lib，再加 `cosir/tests/lean_backtrace.rs`。模型子模块必须属于 `lean_lib Formal`，由入口 import，不能把未覆盖的根目录文件当作 `lake build` 已检查。

### Task 1: 安装 Lean 工具链并搭建 lake 项目脚手架

先验证 elan/Lean/Lake 和最小构建，再扩模块；原计划的 Windows winget、stable、std4 网络回退只属于当时操作方案。现行主模型只依赖 Lean 自带库；独立源码细化包的工具准备另见映照契约，不混淆两个包。

### Task 2: 定义 Lean `State` 与能力模型（Bootstrap 所需子集）

最初草稿只含能力表与 booted、owner/obj/rights/active，**将 `WellFormed := True` 作为占位**；不等于有限能力域或安全性质已证。现行 CDT 非平凡良构谓词与派生树字段另有后继实现，不能复用该占位模板作为证明。

### Task 3: Lean 实现 `bootstrap` 并证明三性质（对拍样板）

当时最小常量为空态与单条 init cap 的 booted 态；目标是成功、重复启动拒绝、booted/能力查找投影和良构。为避开函数字段整体相等，草稿选投影断言，建议 rfl/simp（部分可判定锚可 native_decide）。`WellFormed := True` 使该草稿良构结论平凡，不能推出当前 bootstrap Authority 完整性；现行三条根 cap 与 CDT 证明按映照契约解释。

### Task 4: 给 cosir 增加 lib 目标（集成测试前置）

保留当时用户选择：新增 lib + `src/lib.rs`，bin 通过 `cosir::…` 复用，使集成测试可 import；不是复制第二套模型。最初八个功能模块外提，内联测试仍随编译单元；草稿中的第九个 `lean_backtrace_tests` 声明明确应删除，对拍放集成测试。先验证 build/test 不破坏原语义。

### Task 5: Rust 对拍 `lean_backtrace.rs`（bootstrap 样板）

复用 pre-boot 机器夹具：Rust bootstrap 成功、init Task 存在、alpha/booted 改变；二次启动预期 AlreadyBooted 对应 Lean none。Lean #eval 是人工核对参考，Rust 硬编码同义断言不是自动双向差分或源码细化。模板期 assert(false) 只观察测试入口，不能充当语义 mutation。

### Task 6: Lean 实现 `exc_resume` 并证明失败原子性

旧草稿只检查 source cap 存在/active/owner，成功返回原态、失败 none；缺少真实 Task/订阅/帧/寄存器语义。所谓失败原子性首先来自无新态返回的接口，不等于整个 Rust 恢复流程细化。成功仅做可执行/投影锚，避免整体函数相等；旧目标中的完整状态定理不能据此视为已完成。

### Task 7: Rust 对拍扩展（exc_resume 失败原子性）

计划复用 Rust 夹具保存 before，触发无源能力的 InvalidExcSource，再断言完整 State 相等；与 Lean none 作有限语义对应。该用例不枚举全部失败路径，也不证明成功恢复、订阅、异常帧或内核 iretq。

### Task 8: 文档化映照契约并登记证据（关闭 BLK-001 / 支持 S0-X3）

先运行 cosir/xtask 单测、fmt、workspace Clippy 与 Lake 构建，再按实测维护唯一映照契约和 SPEC-019/BLK-001/S0-X3 记录；S0-X1 仍需独立推进。这里保留的是计划要求，不补记原文件未记载的运行成功，也不据旧步骤改写现行阶段状态。

## Follow-up（后续切片，不属本计划）

原计划列出的 tlb_refine、memseg_refine、cow_refine 及可选自动差分是后续任务；不能把当时“未覆盖”读作永久缺失。2026-11-25 已有 [TLB](../reports/2026-11-25-lean-tlb-refine.md)、[Memseg](../reports/2026-11-25-lean-memseg-refine.md)、[Cow](../reports/2026-11-25-lean-cow-refine.md)最小模型报告；其范围按报告/映照解释，不变成完整内核细化。当前源码细化只覆盖单独登记的方法，仍有翻译器可信边界。
