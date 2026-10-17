# SPEC-144：SPEC-143 复核残修（3 项）

> 文档类型：复核残修补丁记录（非设计、非规范、非证明）
>
> 日期：2026-10-17
>
> 基线：`b108bf5`（SPEC-143 整改）
>
> 结论速览：SPEC-143 提交后的独立复核在 `§8.5` 发现 3 项残修问题；均已修复并提交。由于复核过程中工作区规范文件被本次修复改动，复核器按“发现文件被改即停止”的约定中止，**未完成 29 项/47 项的完整重审**；S0-X1/S0-X2 仍不勾选，需在新冻结提交上重做完整复核。

## 1. 发现的 3 项残修

| # | 位置（`b108bf5`） | 问题 | 修复 |
|---|---|---|---|
| 1 | §8.5 `node_attach` 成功动作 1 | 查找条件写成 `kind = RemoteProxy(peer)`，但 `node_attach(node_cap, remote_task_id)` 作用域内没有绑定 `peer`（只有 `node_cap.target`），条件不可判定 | 改为 `kind = RemoteProxy(node.peer)` |
| 2 | §8.5 `node_attach` 成功动作 2 | `node_attach(...) -> task_cap` 需要 CapId，而内部迁移 `attach_remote_task(...) -> TaskId` 不回传 CapId；“直接返回它”没有定义 CapId 的取得方式，首次跨节点发送仍不可达 | 明确“创建后扫描 `HeldBy(caller)` 中指向新代理的 `{Derive, TaskSend}` 根 Capability 并返回”；禁止用 `next_cap-1` 等实现假设 |
| 3 | §8.5 跨节点投递目标端步骤 | 目标端先创建/复用 `RemoteProxy`（`attach_remote_task(..., holder=接收方)`），之后才检查“接收方不是 CPU Task 或已销毁”；接收方已销毁时会以死 holder 创建代理并留下不可达代理 | 把“接收方是活动 CPU Task”检查提前；失败时只追加 `MessageDropped`，不创建代理、不投递 |

## 2. 修复后的文本

- `node_attach` 成功动作 1 使用 `node.peer`；动作 2 定义“扫描 `HeldBy(caller)`”为 CapId 的唯一取回路径；动作 3 保留“已存在代理时返回调用者持有的可用 `TaskSend` Cap，否则 `NotHeld`”。
- 目标端步骤改为“先验证接收方是活动 CPU Task；失败则 `MessageDropped`，不创建代理；成功才 `attach_remote_task` 并投递”。

## 3. 复核完整性与边界

- 独立复核器以 `b108bf5` 为基线开始；在读取 §8.5 后检测到工作区文件被本次残修改动，按约定中止，未给出 29 项/47 项的完整结论。其停止前确认的 3 项即上表。
- 本记录只修复这 3 项；**不构成对 SPEC-143 全部 29 项或 §19 47 项的重新签署**。
- 由于工作区在复核期间移动，S0-X1/S0-X2 仍保持未勾选；下一轮必须以本提交（或更新的冻结提交）为唯一只读基线，重做完整复核。
- 本轮只改规范文本；未改 Rust/Lean/QEMU 代码。`cargo test --workspace`（786 passed）与 `lake build`（8 jobs）在本提交前已通过；本次仅 §8.5 文本，无代码回归面。

## 4. 证据边界

- 不构成 `MODEL-TEST`、`REFINEMENT-PROOF`、QEMU 证据或组织级独立性审查。
- `node_attach` 的 CapId 取回、目标端代理创建顺序、`NodeIdentity`/`NodeId`、`reserved_frames`/GPU 池等仍无 `cosir`/Lean 实现；文本闭合不等于实现闭合。
- 下一轮复核应同时检查：`node_attach` 的首次发送可达性、`node_destroy` 清理、W14 无父根来源、W6 保留帧三划分、§19 47 项错误清单。
