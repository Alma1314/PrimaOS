# SPEC-132：从 manifest 顺序确定 init 服务索引

**日期**：2026-10-17
**阶段**：阶段 4 / EXO-2 后续硬化
**基线**：SPEC-131（`301c602`）

`xtask` 曾在生成 init ELF 前写死 `alpha_index = 1`、`servicemgr_index = 3`，同时在另一段代码按固定顺序填充 manifest。两处必须同步修改，顺序变更时容易让 init 从错误条目读取 BootInfo。

现在构建工具把服务 ELF 路径与 opaque `kind` 放在同一张表内，先构造实际 manifest，再按 `kind` 查找两个索引并生成 init ELF。缺少所需服务种类时构建报错。内核仍只复制 opaque `kind`，不解释服务角色。

验证范围：`xtask` 单元测试覆盖条目位置与缺失种类；QEMU BIOS 双次启动验证当前顺序的引导行为。该修改尚不提供真正的 ring3 Rust loader，也不改变阶段 4 退出条件判定。
