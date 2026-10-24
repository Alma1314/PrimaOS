---
id: spec-kernel-handle-model
kind: spec
authority: implementation-contract
status: working-design
lifecycle: current
updated: 2026-10-24
---

# 内核句柄耗尽与 Lean 迁移模型

## 范围与依据

本切片落实[原语规范](../../../COSIR%20原语规范.md)§3 的机器计数器溢出显式检查、§4.1 的有限权限与类型匹配、§18 的索引/generation 表示，以及[阶段 2 设计](2026-09-16-stage2-handles-caps-segments-design.md)和[SPEC-147](../reports/2026-10-17-stage2-exit-review-spec147.md)的 S2-X4 旧句柄不复活要求。它是用户授权的实现补强工作设计，不是新的阶段或 profile 签署。

基线 ff7f99e 存在三个具体缺口：`next_generation` 在 `u32::MAX` 回绕到 1；`Rights::well_typed` 遍历已知位时漏过 bit31；分配失败前已递增逻辑 CapId。既有常规复用/QEMU 证据不能覆盖 generation 耗尽。保留历史报告，不将其通过结论扩大至该边界。

## 成功与失败契约

- 句柄编码、合法位宽、generation 从 1 起、槽 0 无效均保持既有 ABI。
- 释放有效活动句柄后：generation 未耗尽则加一；已达 `u32::MAX` 则槽永久退役，内部表示为 `generation=0, in_use=false`。0 不会编码成可返回的合法句柄。
- 普通分配跳过退役槽；固定槽保留也必须通过相同可用性检查。可用槽耗尽返回既有容量错误，不回绕、不复活、不扩大表容量。
- 错误句柄释放、重复释放、槽耗尽、逻辑 CapId 耗尽都不得修改槽表或逻辑 CapId 游标。共用分配入口先检查计数器，再分配槽，最后提交游标。历史容量与权限检查仍在调用方、提交前完成。
- `well_typed` 通过对象允许的权限集合加 Derive 的子集判定，拒绝所有未知位。空权限的既有语义不在本切片变更。
- 内核保持零 unsafe；不新增 syscall 或修改 errno、协议事件、保留槽区间、并发模型。

## Lean 模型与映照

新增独立 `Formal.KernelHandle` 命名空间，与旧 `Formal.State` 的抽象 CDT 分开。用自然数表示单个固定物理槽的 generation，以参数 `limit` 表示机器上限（生产实例 `2^32-1`）；0 是吸收的退役状态。

模型实现可执行 allocate/release/check，显式失败返回 none，状态不提交。证明合法范围保持、成功释放严格前进或退役、退役不再分配，以及任意后续 allocate/release/失败停留序列中旧 generation 不再成为 Live。Rust 测试覆盖实际槽表、动态/固定分配和上界，证据为模型证明与 host 见证的映照；不是从 Rust 源码提取的 refinement proof。模型不证明 holder、对象存活、权限、完整内核执行或硬件。

Lean 工具链固定为本机已安装的 v4.35.0，删除未使用的外部 std4 Git 依赖，保留 Lean 自带库。对全部既有 CDT/Bootstrap/ExcResume 定理重新构建，不用 sorry/admit；库构建禁用 autoImplicit，并将 warning 视为 error。

## 验证与重审

先在旧实现运行耗尽/未知位反例，再验证修复；Rust 覆盖正常加一、末代可用、释放退役、重复释放、全退役动态与固定拒绝、逻辑 CapId 耗尽和类型掩码。Lean 用一般定理证明无复活，并加相同边界的可执行锚。运行 workspace tests、lint、完整 BIOS 双轮验收以及文档校验。所有结果与失败记录在[后继报告](../reports/2026-10-24-kernel-handle-model-report.md)。

generation 位宽/编码、槽分配或预留策略、CapId 提交顺序、权限集合、Lean 版本或上述映照发生变化时重审本契约与测试。能力历史表的 512 条累计上限仍存在，本轮不扩容；耗尽策略不能替代资源回收设计。
