---
id: spec-capability-location-model
kind: specification
authority: scoped-implementation-contract
status: working-contract
lifecycle: current
updated: 2026-10-27
---

# Capability 位置与溯源 host 模型补齐

承接 [ISSUE-CAP-001 发现记录](../reports/2026-08-17-capability-derivation-model-gap.md)、
[Lean 映照](../../../lean/README.md)以及原语规范 §4、§6.3、§9、§11、W2–W5/W13。
适用于本次 cosir host 模型；不改变原生 ABI、阶段退出条件或 Lean 定理的覆盖范围。

`State.locations` 是授权持有关系的唯一来源，缺失位置不得回退到旧 `owner`。
旧 Capability.owner 仅保留创建时拥有者，供既有 CDT 投影使用；移动不修改记录。
所有模型创建路径同时登记 HeldBy；记录永久保留，drop 只删除位置。
派生校验持有、对象存活、Derive、权限子集和权限类型，使用单调 ID，不复用已丢弃记录。
既有显式 ID 的 derive 函数仍是 Lean CDT 对拍用内部原语，不作为用户授权接口。

实现 Task 持有、FIFO 消息和 Namespace Bound 三类位置载体；消息排队、阻塞直投、
接收、bind/unbind、drop 和容器回收在同一步完成位置和容器更新并追加完整位置事件。
没有公开任意 move。结构性根拒绝 send/bind/drop；失去最后一个 ExcSubscribe token
时删除订阅；stale token 可接收、unbind、audit、drop。
Namespace 路径沿用规范的非空分量及前缀不重叠约束；本片不扩充 mount/跨节点。

增加独立位置/载体良构检查和溯源查询。检查描述权限单调、父链序、位置与载体双向一致、
结构性根、消息唯一队列归属及 pending_return 约束；审计查询只读，返回祖先记录和相关事件。
故障注入应覆盖 owner 仍相同但 token 已移走时的旧授权入口，避免只验证新函数。
失败必须逐字段状态相等；资源 ID 耗尽也先拒绝再修改。

统一验收运行 workspace tests、lint、compat、文档四项检查；host 模型修改不新增 QEMU
机制主张。暂不建模 Timer、Namespace Mount、远端传输及所有对象生命周期；这些不以
位置模型通过冒充完整 COSIR 模型。扩大载体或改 Lean 时须重审本契约。

## 本地 Mount 与 task_destroy 根因整改

本次继续纳入 §11 的 Mount：挂载只派生 `{NsResolve, Derive}`，直接登记 Namespace 位置；
图搜索拒绝环，解析逐步消费路径分量，stale Mount 可卸载。路径严格限制 ASCII/255字节分量与平台分量数。
派生事件记录初始位置，解析事件记录起始cap、挂载链和最终父项，审计重放覆盖直接置入容器的创建。

对照现行 §8.2 发现内核保留了旧实现偏差：销毁发送者时也删已发送消息，且清理 held caps
使用递归 revoke。两者均违反“已经发送到其他 Task 邮箱中的消息和 Capability 不隐式销毁”。
整改统一只删除目标邮箱与目标持有位置，保留消息 origin 和外部后代；父历史不删除。
结构根拒绝、整个 held 集合及整个销毁动作的审计容量必须预检，失败不得先摘 Process 或清 cap。
历史设计/失败报告保留；前批托管退出交接继续保留，但不再充当消息安全性的必要前提。
此内核修复需最终 workspace/lint/完整BIOS双轮验收，不能以host模型代替机器证据。


## 2026-10-26 事件组合后继

Timer、异常fan-out、WaitObject/upcall和远端纯数据通路已写入[组合契约](2026-10-26-event-authority-model-design.md)，
共同使用位置模型，尚未统一测试。上文“不建模”的范围记录仅属于前片；本后继也不表示完整COSIR或机器证明。
Lean位置保持性草稿及扩展的精确映照仅查[唯一映照契约](../../../lean/README.md)。


## 2026-10-27 当前分层证据

本契约及事件组合的cosir host回归已通过；Lean位置保持性与既有CDT模块lake build通过。
内核对应边界的host及完整单次BIOS68轮通过。精确版本、失败链与未覆盖项见
[统一报告](../reports/2026-10-25-stage0-5-gap-followup.md)，Lean映照仍以README为准。
这不证明完整Rust迁移、远端机器实现或COSIR到机器的完整细化，不更改既有阶段签署范围。
