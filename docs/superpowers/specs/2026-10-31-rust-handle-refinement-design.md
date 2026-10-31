---
id: spec-rust-handle-source-refinement
kind: specification
authority: implementation-contract
status: implemented-with-limits
lifecycle: current
updated: 2026-10-31
---

# Rust 句柄源码到 Lean 的首个细化切片

## 目标与范围

承接[有限槽表证明](../reports/2026-10-31-kernel-handle-table-proof.md)的明确缺口，建立生产 Rust 源码 → 编译器 MIR/Charon LLBC → Aeneas Lean → 已有 `Formal.KernelHandle` 模型的检查链。

本切片只覆盖 `kernel/src/handle.rs` 中 `Handle::new` 与 `Handle::next_generation`，以及后者投影到单槽 release 的 generation 更新。不扩大为整个 Rust `CapabilitySlotTable::free/alloc/reserve/check`、编解码、Authority、内存写入或机器码的细化证明。后续模型序列仍由既有 `KernelHandle.Reachable` 给出，尚未证明所有真实 Rust 后续操作细化这些序列。

## 源码与工具约束

- `lean/refinement/source.rs` 用 Rust `#[path]` 直接引用生产 `kernel/src/handle.rs`；两个入口只调用原方法，不复制其实现。
- Charon 使用 `--preset=aeneas --sysroot default`，仅从这两个入口提取可达代码；启用 `--error-on-warnings`，不跳过 borrow checking，不提供用户编写的外部函数桩。
- Aeneas 使用 Lean 后端、split-files、warnings-as-errors；仅允许两个生成文件 `Types.lean`/`Funs.lean`，额外 external template 单元会使检查失败。生成文件不得手改。
- 工具版本和官方发布包 URL/SHA256、三个可执行文件 SHA256 固定在 `lean/refinement/toolchain.lock.json`。Charon 对应独立 `nightly-2026-09-17`，不修改生产构建用的 `nightly-2026-07-30`。
- 源码细化放在独立 Lake 包；原 `lean/` 的无外部依赖模型仍保持独立。Aeneas 依赖的 mathlib 由独立 manifest 固定，不写入原模型的依赖图。

## 细化关系与证明义务

Aeneas 的 U32 用有界 BitVec 表示，投影为 `.val : Nat`。`generationAfter` 将 `Some g` 投影为 `g.val`，将 `None` 投影为永久退役标记 0。证明：

1. 对任意 U32，checked_add(1) 的投影恰为 `< 2^32-1` 时加一，否则 0。
2. 生产方法的提取结果为上述 checked_add；这是对自动生成定义的检查，不是手写另一份 Rust 实现。
3. 若既有单槽模型一次 release 成功，生成方法返回值的投影与其目标 generation 相等。
4. 从实际生成方法的成功返回值建立释放后状态，任意后续允许的模型序列都不复活旧非零 generation。
5. 生成的 new 对任意两个 U32，恰在 index 或 generation 为 0 时返回 Malformed，否则返回原值构造的句柄。

不得用有限样例、source hash、Rust 单测或生成器退出成功替代以上普适 Lean 定理。

## 重放与失败门禁

`python3 scripts/check_handle_refinement.py` 必须每次从当前生产源码重新提取，并将输入 hash、工具锁 hash、生成物 hash 与 `source-manifest.json` 对照，再逐字节比较生成文件，随后构建证明和审计全部定理的公理依赖。默认不下载、不改生成物；源码变化后须评审并用 `--regenerate` 更新，证明仍必须通过。

`--mutation-test` 在 target 临时目录将真实 Rust 的 checked_add 改成 `Some(1)`，重新提取并构建同一组证明，要求因 `Refinement/Handle.lean` 的证明错误失败。生产源码/正式生成文件不受 mutation 修改。不得把依赖缺失、编译失败或 hash 不符当成证明发现该语义反例。

公理审计需覆盖文件中的全部一般定理，依赖只允许 Lean 标准 `propext`、`Classical.choice`、`Quot.sound`；拒绝 sorryAx、原生求值或自定义公理依赖。

## 信任边界与后继

这是**以 rustc/Charon/Aeneas 和 Rust core 标准模型为可信翻译边界的源码级条件性细化证明**。没有形式证明翻译器本身、两个 Rust 编译器版本的语义等价、LLVM、链接、ABI 或实际内核二进制。Aeneas 的 checked_add 模型属于受信任的标准库语义，不等于本仓库已验证 Rust core 的源码。

任何生产源码、入口、工具/后端/标准库模型、Rust/Lean/依赖版本、generation 表示或抽取选项变化都应重放该链。下一步需扩大自动提取范围至真实槽表读写/选择过程，再证明相应状态投影；本切片不能先行把整表手写模型结果标为已落实到 Rust。

本次实测与失败见[源码细化报告](../reports/2026-10-31-rust-handle-source-refinement.md)，逐字段边界由 [Lean 映照契约](../../../lean/README.md)维护。
