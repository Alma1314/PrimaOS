# Capability 派生 / 位置 / 溯源建模缺口备忘

> 文档类型：研究备忘（非设计、非规范、非证据）
>
> 日期：2026-08-17
>
> 来源：Prima OS 全量一致性审计（实现↔规范维度，发现 ③-A）。
> 本备忘记录一个已知模型缺口及其背后的研究决策，不修改权威规范，不为任一机制新增证据。

## 1. 缺口一句话

host 可执行模型（`cosir/`）把 `Capability` 建模为只有 **owner / object / rights / active** 四字段的浅层记录，**没有**规范 §17.2 依赖的派生链（`parent`）、位置表（`locations` / `HeldBy`）与溯源事件（`CapMinted`/`CapDerived`/`CapMoved` / `CapRecord`），也因此**没有 `cap_derive` 操作**。

## 2. 对 §2.1 第一阶段目标的影响

COSIR §2.1 列出 8 条第一阶段目标。当前模型的实际覆盖：

| §2.1 目标 | 模型覆盖 | 说明 |
|---|---|---|
| (8) 失败操作保持抽象状态不变 | **覆盖充分** | 大量 `assert_eq!(before)` + 内联不变量自检 |
| (6) 显式授权通道 | **部分覆盖** | `authorize_mapping`/`authorize_task_management` 忠实；跨 Task 消息 `send`/`recv` 未建模 |
| (1) Capability 用户态不可伪造 | **弱覆盖** | 仅靠 active/owner 自证，无回溯源头的不可伪造证明 |
| (5) 未授权 Task 不能改动他 Task | **间接覆盖** | 经 TaskManageVSpace 授权要求，但无独立证明 |
| (2) 派生权限不提升 | **无测试抓手** | 无 `cap_derive`，无从验证 |
| (3) 每个 token 单一位置 | **无测试抓手** | 无位置表 |
| (4) 完整溯源 | **无测试抓手** | 无 CapRecord / 审计链 |
| (7) 对象/引用/消息/映射/Timer/Ns 生命周期一致 | **无测试抓手** | 仅映射/段有 W6/W7；Timer/Ns/消息生命周期未建 |

**结论**：模型把"内存与调用细化层"做得很深（失败原子性、W6/W7 双视图、checked 溢出），把"对象能力抽象层"做得很浅——而后者恰是 COSIR 的核心卖点。§2.1 目标 2/3/4（以及 7 的大部分）在模型中**没有任何抓手可测**，任何据此泛称"模型验证了 Capability 安全"的表述都属越界。

## 3. 为什么现在不补实现（决策与理由）

本次审计**选择记录缺口而非补实现**，理由：

- `cap_derive` + `locations` + 完整溯源事件是规范核心卖点，落地需新状态面（每 Cap 的 parent、位置上下文、审计迁移），覆盖 W2/W3/W4（§17.2 不可伪造/权限单调/位置唯一）的多组测试。这是一项独立的垂直切片级工作量，远超一次一致性审计的修复范围。
- 模型当前也无 `send`/`recv` 消息机制（同样属 ③-A 下层缺失）。此时单独补 capability 溯源会与尚未建模的位置载体（Message/Namespace 中的 token）脱节，容易产出一个不与整体自洽的半截实现。
- 补实现与正在推进的方向相关：framekernel 差距报告 G9 已把 untyped memory 建模列为下层切片的"设计输入"；Capability 派生/位置建模若要做，宜作为独立切片、与消息机制同批设计，而非缝进本次修复。

**判定**：维持现状 + 下文文档标注，防止未来误读。把"补 `cap_derive` + `locations` + 溯源事件"列为后续独立切片候选。

## 4. 文档标注（本次审计已落）

为避免"模型验证了 Capability 安全"被误读，本次审计在下列位置如实标注了该缺口：

- `cosir/src/ops.rs` `raise_exception` 注释：明确模型只建模同步侧，规范 §12.9 的"订阅者通知投递"异步语义未建模（与 Send/Recv 未建模一致）。
- `Prima OS 实现阶段.md` SPEC-012 行：补入了通知投递未建模的范围边界。

## 5. 关联与后续

- **关联**：①-A 同属"抽象层另一半未建模"：模型的 `Capability` 无 `parent`/`locations`，`cow_refine.rs` 中出现的 `parent`/`CapObject` 只是页表/COW 映射概念（`CowMapping.parent`），**不是 capability 溯源**，不得混淆。
- **建议后续切片（Open）**：在消息机制下切片中，一并设计 `Capability { owner, rights, active, parent: Option<CapId> }` + `locations: CapId ↦ Location` + `CapDerived/CapMoved` 审计事件，并以 §17.2 的不可伪造/权限单调/位置唯一为验收不变量（对应 §2.1 目标 2/3/4）。
- **不属于本备忘**：不判定现有已通过测试（如 COW/TLB 组合授权）是否过度声称——§4 已核对，那些模型确实覆盖且命名已收敛。

## 6. 证据边界

本备忘不产生新证据；全部判断基于对 `cosir/src/state.rs`、`cosir/src/ops.rs`、`cosir/src/*_refine.rs` 的静态搜证（grep 确认无 `cap_derive`、无 `locations`/`HeldBy`/`CapRecord`、无 `CapMinted`/`CapDerived`/`CapMoved`）与 COSIR §2.1 / §17.2 的对照。


## 7. 2026-10-25 后继代码（未统一验收）

历史发现与当时选择保留不变。parent/CDT 已由 SPEC-099 补强；位置、消息/Bound载体、
drop和溯源的本次代码见[后继记录](2026-10-25-stage0-5-gap-followup.md)，尚未统一测试。
本地载体扩展不代表 Timer、Mount、远端与完整异常通知模型已经具备，也不扩充 Lean 证明。
