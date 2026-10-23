# S0-X2 逐操作覆盖报告（修订后复核）

> 文档类型：覆盖报告（非设计、非规范、非证明）
>
> 日期：2026-09-13（含同日规范修订与复核）
>
> 范围：对 [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md) 中全部常规操作逐项核对「成功语义」与「失败/错误语义」是否明确。
>
> 结论速览：经同日规范修订，**S0-X2 满足**。全部常规操作现在都具有明确失败语义——具体为
> 显式 `错误：` 清单、§6.2.1 全局默认失败语义规则，或显式“无失败分支”声明三者之一。

## 1. 方法

逐读 `COSIR 原语规范.md` §4~§15，对每个常规操作摘录成功迁移与失败/错误语义。判定口径：

- **显式清单**：操作小节给出 `错误：` 枚举，并说明失败时按 §6.2 保持 `State` 不变；
- **默认规则**：操作小节未单列清单，由新增 §6.2.1 规定失败域 = 其前置条件违反，返回 §6.2 阶段
  标准错误并保持 `State` 不变；
- **无失败分支**：操作小节显式声明，且其前置条件由 Actor 约束或内核保证成立。

§6.2.1 为本次修订新增的规范性兜底规则，保证“未单列清单”不再等于“失败语义未定义”。

## 2. 修订内容（2026-09-13）

针对修订前的覆盖空洞（见 §5 复核记录），做了三类修改：

1. **新增 §6.2.1 默认失败语义**：显式清单优先 / 默认规则兜底 / 显式无失败分支，并定义新错误标识
   `InvalidTime`、`InvalidTimer`、`MountCycle`、`InvalidDevice`、`InvalidNode`。
2. **补齐显式 `错误：` 清单**：`cap_drop`、`task_destroy`、`timer_cancel`、`timer_destroy`
   （新增 `InvalidTimer`）、`advance_time`（新增 `InvalidTime`）、`ns_bind`、`ns_unbind`、`ns_mount`
   （新增 `MountCycle`）、`ns_unmount`、`ns_destroy`、`exc_unsubscribe`、`exc_source_destroy`、
   `audit`、`node_destroy`（新增 `InvalidNode`）、`dev_mmio_map`、`dev_irq_mask`/`dev_irq_unmask`、
   `dev_destroy`（新增 `InvalidDevice`）。
3. **声明“无失败分支”**：`enqueue_or_deliver`、`task_yield`、`schedule`、`recv`、`time_now`、
   `timer_set`、`ns_create`、`raise_exception`、`node_create`、`dev_ioport_create`、`dev_irq_create`；
   §4.6 明确 Capability 移动不是可调用原语、失败语义继承外层操作。

## 3. 逐操作判定（修订后）

| 类别 | 操作 | 失败语义类型 |
|---|---|---|
| Capability | `cap_derive` | 显式清单 |
| Capability | `cap_drop` | 显式清单（本次补齐） |
| Capability | 移动 | 继承外层操作（非可调用原语） |
| 内部辅助 | `enqueue_or_deliver` | 无失败分支（本次声明） |
| MemorySegment | `memseg_create` / `map` / `unmap` / `destroy` | 显式清单 |
| Task | `task_create` | 显式清单 |
| Task | `task_destroy` | 显式清单（本次补齐） |
| Task | `task_yield` | 无失败分支（本次声明） |
| 调度 | `schedule` | 无失败分支（本次声明；`KernelActor` 内部） |
| Send/Recv | `send` | 显式清单 |
| Send/Recv | `recv` | 无失败分支（本次声明；空邮箱 `Blocked` 非错误） |
| Timer | `time_now` | 无失败分支（本次声明） |
| Timer | `timer_set` | 无失败分支（本次声明） |
| Timer | `advance_time` | 显式清单 `InvalidTime`（本次补齐） |
| Timer | `timer_cancel` | 显式清单（本次补齐，含 `InvalidTimer`） |
| Timer | `timer_destroy` | 显式清单（本次补齐） |
| Namespace | `ns_create` | 无失败分支（本次声明） |
| Namespace | `ns_bind` | 显式清单（本次补齐） |
| Namespace | `ns_unbind` | 显式清单（本次补齐） |
| Namespace | `ns_mount` | 显式清单（本次补齐，含 `MountCycle`） |
| Namespace | `ns_unmount` | 显式清单（本次补齐） |
| Namespace | `ns_resolve` | 显式清单 |
| Namespace | `ns_destroy` | 显式清单（本次补齐） |
| Audit | `audit` | 显式清单（本次补齐） |
| Exception | 源创建 `exc_source_create` | 显式清单 |
| Exception | 源销毁 `exc_source_destroy` | 显式清单（本次补齐） |
| Exception | `exc_subscribe` | 显式清单 |
| Exception | `exc_unsubscribe` | 显式清单（本次补齐） |
| Exception | `raise_exception` | 无失败分支（本次声明；内部 KernelActor） |
| Exception | `exc_resume` | 显式清单（含失败原子性） |
| Node | `node_create` | 无失败分支（本次声明） |
| Node | `node_destroy` | 显式清单（本次补齐，含 `InvalidNode`） |
| Device | `dev_mmio_create` | 显式清单 |
| Device | `dev_mmio_map` | 显式清单（本次补齐） |
| Device | `dev_mmio_unmap` | 显式清单（引用 §7.3） |
| Device | `dev_ioport_create` | 无失败分支（本次声明） |
| Device | `dev_dma_create` | 显式清单 |
| Device | `dev_irq_create` | 无失败分支（本次声明） |
| Device | `dev_irq_mask` / `dev_irq_unmask` | 显式清单（本次补齐） |
| Device | `dev_destroy` | 显式清单（本次补齐，含 `InvalidDevice`） |
| Bootstrap | `bootstrap` | 一次性迁移，不属常规操作；成功/失败由 §14 引导状态定义 |

## 4. 与实现的对照（证据索引）

实现侧证据只覆盖规范操作的一个子集，且绝大多数为 `MODEL-TEST`；本报告不据此宣称语义已被机器验证。

| 规范操作 | 实现/模型证据 |
|---|---|
| `cap_derive`/撤销/位置移动 | `kernel/src/bootstrap.rs`（`derive_capability`/`revoke_capability`/`validate_authority`）、`cosir` MODEL-TEST（SPEC-021~023、039） |
| Task 表/schedule/yield | `kernel/src/bootstrap.rs`、`cosir` MODEL-TEST（SPEC-024~031）；真实切换 SPEC-051/053 |
| `send`/`recv`/`enqueue_or_deliver` | `kernel/src/bootstrap.rs`、`cosir` MODEL-TEST（SPEC-024~031） |
| 异常源/订阅/递送 | `kernel/src/bootstrap.rs`（`exc_subscribe`/`exc_unsubscribe`/`exc_raise`、Audit）、QEMU `exc_ready`/`exc_pending_ready`/`exc_wait_ready`（SPEC-050/052） |
| 授权查询 | QEMU `syscall_auth_ready`（SPEC-049） |
| `exc_resume` | `cosir` MODEL-TEST（SPEC-012）、`kernel/src/exc.rs` 验证式恢复（SPEC-042）、QEMU `exc_resume_ready` |
| MemSegment | 仅 `cosir` 细化模型（SPEC-013/014/033~036），**无真实内核 memseg 实现** |
| Timer 全族 | **仅规范定义，无实现/模型证据** |
| Namespace 全族 | **仅规范定义，无实现/模型证据** |
| Device / Node 全族 | **仅规范定义，无实现/模型证据** |

**边界：** 本列的空白是“实现/验证证据覆盖”的缺口，不是“失败语义定义”的缺口；S0-X2 只要求后者。
前者属于阶段 2/3 的实现与 `REFINEMENT-PROOF` 范畴，登记为后续切片。

## 5. 复核记录

- **修订前（同日早先快照）：** 26 项常规操作中，失败语义完整者约 11 项，`timer_cancel`/`exc_unsubscribe`
  仅部分，15 项（`cap_drop`、`enqueue_or_deliver`、`task_destroy`、`task_yield`、`schedule`、`recv`、
  `timer_set`、`advance_time`、`timer_destroy`、`ns_create`、`ns_bind`、`ns_unbind`、`ns_mount`、
  `ns_unmount`、`ns_destroy`、异常源销毁、`raise_exception`）无逐操作失败语义。该快照即本报告最初的
  “S0-X2 不满足”依据。
- **修订后（本报告当前状态）：** 上述 17 项全部获得显式清单或“无失败分支”声明；其余操作由 §6.2.1
  默认规则覆盖；`timer_cancel`/`exc_unsubscribe` 补为完整清单。**S0-X2 满足。**

## 6. 证据边界

本报告是对规范文本的覆盖核对，证据等级为文档审查；不构成机器检查，不声称实现与规范一致
（`REFINEMENT-PROOF`），也不覆盖 §2.2 列出的非目标（调度公平性/无饥饿、隐蔽信道、硬件 MMU/IOMMU、
SMP、资源配额）。计时器/命名空间/设备的“成功/失败语义”目前只有规范文本，无实现或模型证据。
S0-X1（独立规格审查）仍需**范畴外**的独立审查者推进，本报告不替代之。
