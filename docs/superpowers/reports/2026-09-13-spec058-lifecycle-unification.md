# SPEC-058 统一修订记录：对象生命周期 19 项 Open ISSUE

> 文档类型：规范修订记录（非设计、非规范、非证明）
>
> 日期：2026-09-13
>
> 范围：处置 SPEC-057 判定 / 登记的 **19 项 Open 生命周期 ISSUE**（7 项重开 + 12 项新发现），
> 全部改动位于权威语义规范 `COSIR 原语规范.md`；本切片不改任何代码。
>
> 结论速览：19 项均已给出规范性处置，改动共 36 处。**是否真正闭合仍需 SPEC-059 独立复核**；
> 在 SPEC-059 通过前不得把 ISSUE 标为 Fixed，S0-X1 维持不满足。

## 1. 已确认的规范性取舍

| 取舍 | 决定 | 依据 |
|---|---|---|
| IOMMU/停 DMA 义务 | 显式划出 POSIR，作为细化层义务 | §2.2 已排除 IOMMU/MMU 正确性 |
| node_destroy 在途传输 | 加前置 + 新错误 `TransfersInProgress` | 避免无界阻塞，同时不违反 W3 |
| 永久 Faulted Task 回收 | 补 `KernelActor` 回收路径 | 使“只能经 task_destroy 回收”可达 |
| Task 状态迁移事件 | 统一补 `TaskStateChanged`，§13.1 补 `TaskResumed` | 闭合 W13 |

## 2. 逐 ISSUE → 改动映射

| ISSUE | 规范位置 | 改动 |
|---|---|---|
| LIFE-001（重开） | §4.6 L267、§16 W4 L1894 | 跨节点超时改为条件回退/丢弃；W4 移除“原子中止本节点 `to`”的措辞窗口，明确 `to` 不做活动性要求、由目标确认时拒绝 |
| LIFE-002（重开） | §16 W14 L1979、§17.2 L2008 | W14 增加代理根（`origin≠None`）来源；§17.2 改为沿 parent 链到达代理根后经 origin 溯源，覆盖代理根后代 |
| LIFE-003（回归） | §8.5 L813/L817、§6.2.1 L531 | node_destroy 前置“在途均满足 §8.5 安全条件”，否则 `TransfersInProgress` 且状态不变；删除强制回退与“不依赖对端确认”的不安全语义；就地定义新错误 |
| FAULT-001（重开） | §12.5 L1361/L1371/L1374 | 增加 Running 前置；成功动作新增 Faulted 迁移步骤（置状态、写 `pending_exc`、`TaskStateChanged`）；描述句改为指向该步骤 |
| TIMER-001（回归） | §8.2 L718、§5.5 L475、§10.6 L1033 | task_destroy 删除 recipient 的全部 Timer（Pending 先追加 `TimerCancelled(RecipientDestroyed)`），满足 W11，活动表不再残留死接收者条目 |
| DEV-001（重开） | §15.6 L1769、§15.8 L1826 | IOMMU domain/assignment/`io_addr` 显式划为细化层资源；POSIR 承诺收窄为“帧 pin 于活动期、归还时清零并回 `free_frames`” |
| DEV-002（重开） | §7.2 L603/L617、§15.4 L1699/L1703 | `memseg_map` 与 `dev_mmio_map` 前置要求两表均未映射，`Overlap` 明确任一表；强制 W16.4 |
| FAULT-003 | §12.5 同上 | 状态迁移、`pending_exc` 写入与事件进入操作成功动作，状态机在操作层闭合 |
| FAULT-004 | §8.4 L759-761、§12.10 L1514 | `schedule` 成功动作清空 `kernel_resume_frame`；`exc_resume` 增加前置 8（帧必为 None，含由 W9 保证的说明） |
| LIFE-004 | §16 W14 L1979 | 同 LIFE-002 |
| LIFE-005 | §4.6 L267 | 同 LIFE-001 |
| LIFE-006 | §17.2 L2008 | 同 LIFE-002 |
| LIFE-007 | §16 W4 L1894 | 同 LIFE-001 |
| DEV-004 | §7.2/§15.4 | 同 DEV-002 |
| DEV-005 | §16 W6 L1911/L1913 | W6 改为三类不相交划分：`free_frames` ⊎ 活动 Segment 帧 ⊎ 保留帧；保留帧含活动 `DMABuffer` pin 帧与细化层页表帧 |
| DEV-006 | §15.6/§15.8 | 同 DEV-001 |
| DEV-007 | §15.8 L1824、§17.3 | 显式声明 dev_destroy 对他 Task `dev_vspace` 的拆除是内核授权隐式 unmap 例外，并给出与 `memseg_destroy` `StillMapped` 差异的理由 |
| FAULT-005 | §8.2 L730 | 新增内核回收路径：`KernelActor` 可清理不可恢复终态 Faulted Task，无需 `task_cap`/`TaskDestroy` |
| FAULT-006 | §16 W9 L1936 | 增加 `Faulted` Task `pending_return=None`；邮箱内容不受故障影响 |

## 3. 自检

- `git diff "COSIR 原语规范.md"`：36 处改动，均落在生命周期相关小节。
- 旧措辞残留检查：`from 或本节点 to`、旧 W14“无父 Capability 来自 Bootstrap 或对象创建”、
  旧 W6 并集等式、`Pending → Cancelled(RecipientDestroyed)`（不删除）均已消除。
- 新错误标识 `TransfersInProgress` 已纳入 §6.2.1 的“就地定义”清单，并在 §8.5 给出语义。

## 4. 证据边界与下一步

- 本记录是**规范修订**，不构成机器检查、`REFINEMENT-PROOF` 或实现验证；不改代码与既有 QEMU 证据。
- **必须由 SPEC-059 独立对抗复核**逐项判定本节 19 项是否真正闭合、有无新引入矛盾；全部 CLOSED
  且无新空洞后，才在 [Prima OS 实现阶段](../../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md) 把 ISSUE 标 Fixed。
- S0-X1 仍不满足；组织级第三方复核门槛不变，阶段 0 维持“进行中”。

## 5. 复核后补正（第二轮，2026-09-13）

SPEC-059 独立复核发现第一轮修订仍有残留，已补正：

| 复核发现 | 严重级 | 补正 |
|---|---|---|
| §6.1 要求用户操作调用者 `Running`，与 §12.10 前置 5（调用者 `Faulted`）冲突，`exc_resume` 不可达 | 高 | §6.1 增唯一例外：`exc_resume` 调用者为 `Faulted` Task |
| 未订阅源的 Faulted Task 既不能 `exc_resume`、又不满足内核回收条件，成永久僵尸 | 高 | §8.2 内核回收条件扩为三选一（不可恢复 kind / 源已销毁 / **不在源的 active subscription 中**） |
| node_destroy 前置“所有使用此 Node 的 InTransit”不可判定 | 中 | `InTransit` 增 `via: Option<NodeId>`；§8.5 步骤 2 把 `via` 绑定到接收方 Task 的 `node` |
| §8.5 安全协议“目标先 ack 后写”不健全，ack 丢失会双份位置违反 W3 | 高 | 改写为：回退仅当源**确定目标未提交**；只接受“显式放弃”或“原子提交协议”；不确定时不得回退 |
| 悬空 `to` / 永久分区下无终态 | 中 | 新增“传输有界性”；永久分区明确划为活性问题（§2.2 非目标），只保证不违反 W3 |
| §4.3 与 §8.5“不得回退”冲突 | 中 | §4.3 按源活动/销毁/安全未定三分支改写 |
| §5.2/§12.6/§12.9 回收措辞与内核路径矛盾 | 中 | 四处统一引用 §8.2 内核回收路径 |
| GPU W6 条件句不可判定；GPU 帧可能被 `memseg_destroy` 还入 CPU 池 | 中 | Segment 增 `pool: PoolRef`；`memseg_destroy` 归还原池；W6 GPU 无条件三类划分 |
| `recv` 的 `InMessage→HeldBy` 未显式追加 `CapMoved` | 中 | §9.2 步骤 2 显式追加 |
| §19 原语总览漏 `exc_resume` | 低 | 补入 Exception 行 |

第二轮复核（针对上述补正的对抗验证）结论：Faulted 链、事件统一、Device 各项 FIXED；跨节点 `via` 绑定、§4.3 冲突、有界性与永久分区已按上表补正。19 项是否全部 CLOSED 以最终复核为准。
