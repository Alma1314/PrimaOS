---
id: report-kernel-handle-model
kind: report
authority: scoped-evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-24
---

# 内核句柄、权限与 Lean 模型补强

## 结论与范围

本切片修复三项实现边界：generation 不再回绕，耗尽槽永久退役；权限类型检查拒绝未知位；槽与单调 CapId 的分配在共同入口提交，容量失败不提前消耗游标。新增独立 Lean 单槽模型，证明释放后的旧 generation 在任意允许的后续操作序列中不会再次 Live。

这是实现、host 回归、独立 Lean 模型证明与 BIOS 集成验证报告，**不构成 Rust 源码细化证明或完整内核安全证明，不改变阶段签署**。对应[设计](../specs/2026-10-24-kernel-handle-model-design.md)、[映照契约](../../../lean/README.md)、[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

## 基线与输入

- 基线：`ff7f99e71167678f449261d4bdcea53c99064d7f`。
- Rust：`nightly-2026-07-30`，实跑 `rustc 1.99.0-nightly (1a833e165 2026-07-29)`。
- Lean：基线 PATH 直接指向 `stable` 的 4.33.1；本次固定并显式运行 `leanprover/lean4:v4.35.0`（Lake 5.0.0）。全部旧模块也重新构建。
- QEMU：8.2.2；BIOS 验收采用 xtask 现有 x86_64 `qemu64`、1 vCPU、128M、`-nic none` guest 参数、30 秒门限、同一事件/输出/容量检查。没有添加人为 guest 退役注入或修改既有期望。
- 构建并行度：`CARGO_BUILD_JOBS=2`，musl 默认 `make -j2`。QEMU 仍可能占用一个完整 CPU 核。
- 前序：[阶段 2 设计](../specs/2026-09-16-stage2-handles-caps-segments-design.md)、[SPEC-147](2026-10-17-stage2-exit-review-spec147.md)、[CDT 森林报告](2026-10-13-lean-cdt-forest-report.md)与[cosir parent 补齐报告](2026-10-13-cosir-cdt-parent-report.md)。这些旧结论按其范围保留。

## 实现

### 句柄与分配事务

`kernel/src/handle.rs` 的 `next_generation` 改为 `Option<u32>`，使用 `checked_add`。`CapabilitySlotTable::free` 在耗尽时将槽写成 `generation=0, in_use=false`。`slot_free`、动态分配和固定保留共同排除退役槽；重复释放与错误 generation 释放不改变状态。内部 `reserve` 增加可用性断言，避免未来调用方跳过约束。

`BootstrapState::allocate_capability_slot` 先预检逻辑 CapId 的后继值，再尝试分配槽，成功后才提交游标。普通铸造、固定铸造、普通派生、指定持有者/固定槽派生、Namespace 派生都使用它。现有历史容量、审计和授权检查仍在提交前执行。没有新增表或依赖，没有改变句柄编码、保留区间、错误映射或表容量。

CapId 游标达到 `u32::MAX` 后拒绝分配，沿用原来的 checked-add 边界；不试图复用逻辑 ID。此处的失败原子性覆盖有效内核状态上的资源耗尽，不声称能对任意已损坏记录/位置表恢复事务。

### 权限校验

`Rights::well_typed` 改成请求位集是否属于“对象合法权限 ∪ Derive”的子集。旧算法只遍历 `ALL_RIGHTS`，会漏掉 bit31；新算法在单一入口同时拒绝跨类型位与未知位。11 种对象/设备类别上测试全部 32 个单个位及与合法全集混合后的结果，并测试两种 root mint 入口的未知位拒绝与状态不变。

此项是内核 `u32` 权限位集校验；不声称完成所有机器字参数的 ABI 解码审计。空权限、跨进程能力共享、目标生命周期等语义不在本次扩展。

### Lean

新增 `Formal.KernelHandle`：`Slot`、有界良构谓词、`allocate`、`release`、`check`、单步 `Step` 与任意长度 `Reachable`。12 条一般性定理覆盖：

- 分配与释放保持范围/退役约束，任意序列保持良构。
- 退役不能分配或释放，且沿任意序列保持退役。
- 成功释放严格越过旧 generation 或进入退役；失效谓词在任意序列中保持。
- `freed_handle_never_live`：一次有效释放后，任意允许的后续操作都不能使旧 generation 再次 Live。

六条 `native_decide` 仅为末两代、退役、重复释放、错误 generation 的求值锚。12 条一般性定理不靠这些实例证明。`#print axioms` 审计只出现 Lean 标准的 `propext`、`Quot.sound`（部分定理无公理依赖），没有 `sorryAx` 或原生求值公理。构建与证明检查仍信任 Lean 工具链及其内核。

工具链固定为 v4.35.0，移除未使用的 std4 Git package 和锁记录；只使用 Lean 自带库。Lake 库构建启用 `autoImplicit=false` 和 `warningAsError=true`。更新旧证明中的弃用定理名；修正 State/Derive/Forest 注释中已过时的“cosir 尚无 parent”，并保留 cosir / 内核 / 新模型字段之间的真实边界。

## 命令与结果

原始本机日志保存在 gitignored `target/kernel-lean-validation/`，不作为永久发布工件；下表记录本次可复现输入与结果。

| 命令/输入 | 本次结果 | 证据范围 |
|---|---|---|
| 旧实现 + `cargo test -p kernel --lib generation_retires -- --nocapture` | 预期 FAIL：末代释放后 `slot_free(1)` 仍为真 | 注入末两代状态的反例 |
| 旧实现 + `cargo test -p kernel --lib mint_rejects_unknown_right_bits_without_mutation -- --nocapture` | 预期 FAIL：返回 Ok 而非 WrongRightType | bit31 混合法定权限 |
| 旧实现 + `cargo test -p kernel --lib retired_slots_fail_dynamic_and_explicit_allocation_without_mutation -- --nocapture` | 预期 FAIL：容量失败后状态已变 | 退役表示注入；旧代码不认识 0，仍占槽并推进 CapId |
| `CARGO_BUILD_JOBS=2 cargo test --workspace` | PASS，882 项、0 失败，其中 kernel 434 | host 单元/集成测试 |
| `CARGO_BUILD_JOBS=2 cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机编译、29 个内核源文件零 unsafe | 静态/编译门禁 |
| `elan run leanprover/lean4:v4.35.0 lake clean` 后 `lake build`（同一显式版本） | PASS，9 jobs，零 warning | 全部既有模型 + 新增模块 |
| 显式 v4.35.0 `lake env lean ../target/kernel-lean-validation/Axioms.lean` | PASS：对新模块全部 12 条一般性 theorem 执行 `#print axioms` | 证明依赖审计 |
| 删除六条求值锚后，将 release 的耗尽分支临时改为回绕 1，显式 v4.35.0 编译 | 预期 FAIL：一般性 release_invalidates 等证明被拒绝；未修改正式源码 | 证明能区分原始回绕缺陷 |
| `CARGO_BUILD_JOBS=2 cargo xtask test`（沙箱外含 UDP 对照） | PASS，62 轮 BIOS、115 条 PASS，退出 0；含 4 项 musl host 检查与 Linux 参考 | BIOS 双轮集成；不执行 generation 耗尽反例 |
| docs self-test / build / check / check --base ff7f99e | PASS：293 个源文档，目录/ID/主题/链接/生命周期/基线变更检查全部通过 | 文档链接、生命周期、目录一致性 |

`Axioms.lean` 的输入由 `import Formal` 加上新模块每条 theorem 的 `#print axioms Formal.KernelHandle.<name>` 组成；不是只检查最终定理的一部分依赖。

## 失败、反例与限制

- 基线第一次 `lake build` 在沙箱内下载 std4 失败（DNS），批准网络后基线构建通过。最终移除依赖后干净构建通过。网络失败不记为逻辑模型错误。
- 新模型开发中第一次 omega 证明未通过：需先化简结构投影。补齐化简后一般定理通过；没有引入额外假设、sorry 或减弱无复活结论。固定 4.35.0 的首轮构建提示旧弃用名，已替换并干净重建。
- 当前每个 BootstrapState 的追加式能力历史上限仍是 512，正常运行会先耗尽历史容量，不能到达 2^32 代循环。host 测试直接注入末两代/退役状态；这是边界契约修正，**不是已证明可在当前正常可达状态利用的漏洞**。QEMU 验证普通路径，不能替代上述边界测试。
- 新模型是固定单物理槽的自然数有界模型；没有证明整张 Rust 表、位编码/解码、最小槽选择、CapId 提交、权限、holder/location、对象存活、并发、硬件。具体映照与遗漏以 lean/README 为准。
- 既有 CDT 模型仍只覆盖其明列子集：cap_drop、位置、完整 Audit、vspace/TLB/COW 的 Lean 建模未在本次实现。不将新槽模型当作这些工作已经完成。
- `CapabilitySlotTable.entries` 仍可由可信内核代码访问；定理假设遵循 allocate/release/失败停留迁移，不覆盖任意内存破坏、直接写表或旧快照恢复。公开字段的全面封装不在本次范围。

## 阶段与后继

本报告补强 S2-X4 的机器上界语义与 Lean 证据范围，不修改旧阶段签署、任何退出勾选、BLK-003、阶段 5/6 状态或兼容 profile。后继若更改 generation 宽度、句柄编码、重置/恢复、分配策略、历史容量/回收或 Lean 版本，应重审设计、映照与反例；若要宣称 Rust refinement，仍需建立自动抽取/语义对应和整表证明链。
