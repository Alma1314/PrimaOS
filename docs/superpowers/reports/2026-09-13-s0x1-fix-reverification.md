# S0-X1 修复独立复核报告：SPEC-056 九项生命周期修复的对抗式再验证

> 文档类型：独立复核报告（非设计、非规范、非证明）
>
> 日期：2026-09-13
>
> 复核对象：提交 `cd442cf` 中 SPEC-056 对 [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md) 的九项生命周期修复
> （对应 SPEC-055 登记并修复的 ISSUE-EXC-005 / LIFE-001~003 / FAULT-001~002 / TIMER-001 / DEV-001~003）。
>
> 目的：在宣告 S0-X1 之前，独立验证“9 项全部 Fixed、无遗留空洞”的断言是否成立。
>
> 结论速览：**断言不成立。** 复核确认 2 项真正闭合、5 项仅部分闭合、**2 项修复自身回归**，
> 并另外发现 12 项新空洞/矛盾（含 2 项高严重度）。因此 S0-X1“没有已知且未记录的对象生命周期空洞”
> **不能达成**；阶段 0 维持“进行中”。本次复核为 harness 内独立 agent 会话，非组织级第三方审查。

## 1. 方法与独立性声明

派 3 个**独立 reviewer agent**（各自全新上下文、不共享主会话历史、不预设修复正确、被要求以证伪为目标），
按视角分工对抗式复核：

| 视角 | 覆盖范围 |
|---|---|
| R1 Capability 与位置 | §4.1/§4.2/§4.3/§4.6/§8.2/§8.5/§12.6/§13.2/§16 W2/W4/W14/W15/§17.2 |
| R2 Task 状态机与异常恢复 | §5.2/§8.2/§8.4/§12.5/§12.6/§12.9/§12.10/§16 W9 |
| R3 Timer 与 Device | §5.2/§5.4/§5.5/§8.2/§10/§15.4/§15.6/§15.8/§15.9/§16 W6/W7/W11/W16 |

主机侧复核者对 reviewer 报告的全部高严重度反例逐条回读当前规范原文核对，结论均已复现。

**独立性边界（如实声明）：** 与 SPEC-055 同等级——harness 内独立会话，非组织级第三方；
不构成机器检查或 `REFINEMENT-PROOF`。新增/变更 ISSUE 以 [Prima OS 实现阶段](../../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md) 为准。

## 2. 九项修复复核判定

| ISSUE | 判定 | 依据 |
|---|---|---|
| ISSUE-EXC-005 | **CLOSED** | §8.2 步骤 7 从所有 ExceptionSource `subscribers` 删除目标 Task，对齐 §16 W15（L1974）与 §17.7；subscribers 仅由 §12.4 创建，无漏删路径。 |
| ISSUE-LIFE-001 | **PARTIAL** | 源侧闭合（§8.2 步骤 4 删 `from=HeldBy`；§4.3 L179/§8.5 步骤 4 改条件回退）。残留：§4.6 L267 仍无条件写“超时→回退到源”，与 §4.3/§8.5 矛盾；§8.2 L728 对 `to=HeldBy(目标)` 为确认时拒绝的延迟处置，与 W4 L1886“原子中止”措辞不一致（→ ISSUE-LIFE-005/007）。 |
| ISSUE-LIFE-002 | **PARTIAL** | §4.2 `origin`/W2 L1877/§13.2 项 5/§8.5 步骤 2 已自洽。残留：§16 W14 L1970“所有无父 Capability 都来自 Bootstrap 或对象创建”未随代理根 `parent=None` 修订（→ ISSUE-LIFE-004）；§17.2 L1999 的 `origin` 例外只覆盖代理根本身，未覆盖 `origin=None` 但父链止于代理根的后代（→ ISSUE-LIFE-006）。 |
| ISSUE-LIFE-003 | **REGRESSED** | §8.5 L810 改为“原子迁移/不依赖对端确认”并强制回退全部在途，直接违反同节 L833-840 的安全条件：回退合法性要求“源已确认目标未接收”，否则同一 CapId 同时出现在两节点、违反 W3。修复引入自相矛盾（→ ISSUE-LIFE-003 重开）。 |
| ISSUE-FAULT-001 | **PARTIAL** | §5.2 已加 `Faulted`/`PendingExc`/`pending_exc`/`kernel_resume_frame`，W9 L1927-1928 已加对应条款。但**进入 Faulted 的迁移没有进入任何操作的成功动作**：§12.5 `raise_exception` 成功动作（L1358-1365）只写 Audit + 广播，不置 `status`/`pending_exc`，也未规定 `faulting_task` 非 Running 时的处置（→ ISSUE-FAULT-003）。 |
| ISSUE-FAULT-002 | **CLOSED（有残留）** | §8.2 步骤 6 清 `pending_exc`/`kernel_resume_frame`；§12.6 新段定义源销毁后不可恢复、只能 `task_destroy` 回收；与 §12.10 前置严格一致。残留见 ISSUE-FAULT-005（回收可达性）。 |
| ISSUE-TIMER-001 | **REGRESSED** | §8.2 步骤 5 对 Pending 只做 `Pending → Cancelled(RecipientDestroyed)`，**未删除**；步骤 4 已删接收者的 Timer Cap，无人能再 `timer_destroy`；步骤 8 删 Task 后这些新 Cancelled Timer 永久残留，直接违反 SPEC-056 新增的 W11 L1944“每个活动 Timer 的接收 Task 活动”。修复把残留从 Fired/Cancelled 搬到了新 Cancelled，未消除（→ ISSUE-TIMER-001 重开）。 |
| ISSUE-DEV-001 | **PARTIAL** | §15.6 L1761 与 §15.8 L1818 已统一为“先停 DMA/拆 IOMMU → 清零 → 归还”，与 W16.2 字面冲突消除。但 §15.9 KernelState 无 IOMMU domain/assignment 对象或表，`io_addr` 无状态迁移，该前置仍是细化层散文义务，POSIR 不可验证（→ ISSUE-DEV-006）。 |
| ISSUE-DEV-002 | **PARTIAL** | `DeviceMapping`/`dev_vspace`（§5.2 L378）、W16.4 入全局不变量（L1984）已建立。但跨表互斥无迁移前置强制：§15.4 前置 4“全部未映射”未指明同时查 `vspace`，§7 `memseg_map` 亦不提 `dev_vspace`，而 §15.4 L1701 明言 Overlap 按 `dev_vspace` 判定 → 存在同时落两表的可达路径，W16.4 可被破坏（→ ISSUE-DEV-004）。 |
| ISSUE-DEV-003 | **CLOSED** | §15.8 L1819 InterruptLine 分支就地声明关联 ExcSource 的 Cap 全变 stale（同 §12.6）。 |

## 3. 新发现问题（未记录空洞/矛盾）

| # | ID | 严重级 | 规范位置 | 描述 |
|---|---|---|---|---|
| 1 | ISSUE-LIFE-004 | 高 | §16 W14 L1970 vs §4.2 L160/§8.5 L827/§17.2 L1999 | W14 断言“所有无父 Capability 都来自 Bootstrap 或对象创建”，与跨节点代理根 `parent=None` 直接冲突，W14 需加 `origin` 例外。 |
| 2 | ISSUE-FAULT-003 | 高 | §12.5 L1358-1367 | 进入 `Faulted` 的状态迁移、`pending_exc` 写入与 `TaskStateChanged` 事件均未出现在任何操作的成功动作中；`faulting_task` 非 Running（Ready/BlockedRecv）时无前置与字段处置定义，§17.1 保持性无法证明。 |
| 3 | ISSUE-FAULT-004 | 中高 | §5.2 L389/§8.4 L755-758/§16 W9 L1928 | `kernel_resume_frame` 的“细化层在 `schedule` 进 Running 时消费并清空”无对应 POSIR 迁移；§8.4 不清帧，故成功的 `schedule` 即可令 W9 被违反；§12.10 步骤 3 对“已为 Some”也无守卫，二次恢复可覆盖旧帧。 |
| 4 | ISSUE-LIFE-003（重开） | 高 | §8.5 L810 vs L833-840 | 见 §2；node_destroy 的无确认回退与 W3 安全条件冲突。 |
| 5 | ISSUE-TIMER-001（重开） | 中高 | §8.2 步骤 5 vs W11 L1944 | 见 §2；新 Cancelled Timer 永久残留。 |
| 6 | ISSUE-LIFE-005 | 中 | §4.6 L267 vs §4.3 L179/§8.5 步骤 4 | §4.6 仍无条件“跨节点 send 超时 → 回退到源”，未含“源已销毁则丢弃”的条件分支。 |
| 7 | ISSUE-LIFE-006 | 中 | §17.2 L1999 | `origin` 溯源例外未覆盖由代理根派生、`origin=None` 的后代；与 §13.2 项 5“链上存在”的正确措辞不一致。 |
| 8 | ISSUE-DEV-004 | 中 | §15.4 L1691/§7.2/§16 W16.4 L1984 | `vspace` 与 `dev_vspace` 跨表互斥无前置强制，存在同时建立两类映射的可达路径。 |
| 9 | ISSUE-FAULT-005 | 中 | §5.2/§12.6/§8.2 | Faulted Task 的回收可达性无保证：其自身 Cap 含 `TaskDestroy` 但 Task 非 Running、无法作为 §6.1 的调用者；creator 已销毁且无人持派生销毁 Cap 时形成永久僵尸。 |
| 10 | ISSUE-LIFE-007 | 低 | W4 L1886 vs §8.2 L728 | W4 称本地 Task 销毁“原子中止以 t 为本节点 `to` 的在途传输”，与 §8.2 的远端确认时拒绝/超时处置之间存在不变量窗口。 |
| 11 | ISSUE-DEV-005 | 中 | W6 L1903 vs §15.6 L1740 | DMA 缓冲帧在创建时移出 `free_frames` 且不属于任何 Segment，使 W6“Segment 帧 ∪ free_frames = frame_universe”不成立；W16.2 只补不交性，未补记账例外（SPEC-056 之前的潜在空洞，本轮暴露）。 |
| 12 | ISSUE-DEV-006 | 低 | §15.6 L1761/§15.9 | 停 DMA/拆 IOMMU 为细化层义务，POSIR 无 IOMMU domain 对象与状态迁移，不可验证。 |
| 13 | ISSUE-FAULT-006 | 低 | §16 W9 L1926-1931 | W9 未显式声明 `Faulted` 时 mailbox/`pending_return` 的处置不变量（实际由“故障要求 Running + schedule 已清 pending_return”推导，缺显式条款）。 |
| 14 | ISSUE-DEV-007 | 低 | §15.8 L1816 vs §7.4 | `dev_destroy` 静默强拆他 Task 的 `DeviceMapping`（§17.3 L2009 明许无需 `TaskManageVSpace`），而 `memseg_destroy` 要求先 unmap/StillMapped，两条销毁路径不一致。 |

## 4. 主机验证套件（无回归）

| 命令 | 日期 | 结果 |
|---|---|---|
| `cargo test --workspace` | 2026-09-13 | cosir 145 + kernel 206 + xtask 106（另有 40/2 host 用例）全通过，0 失败 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 2026-09-13 | 零告警 |
| `cargo xtask test` | 2026-09-13 | QEMU BIOS run=1/2 六条 PASS，退出 0 |

以上证明本复核为纯文档改动，未影响既有实现证据（SPEC-040~053 不变）。

## 5. 判定与对阶段 0 的影响

- SPEC-056 声称的“九项全部 Fixed”**不成立**：2 项 CLOSED、5 项 PARTIAL、2 项 REGRESSED，另有 12 项新空洞/矛盾。
- 因此 S0-X1“规范没有已知且未记录的对象生命周期空洞”**不满足**：既存在未记录空洞，也存在已登记但未真正闭合的 ISSUE。
- 按维护规则 5（实验失败与反例必须保留、不得改写措辞隐藏），S0-X1 退出条件应撤销勾选，全部新发现问题登记为 ISSUE。
- 建议后续以独立规范修订切片（SPEC-058）统一处置上表 14 项（12 项新发现 + 2 项重开），并在修订后再次独立复核；阶段 0 维持“进行中”。

## 6. 证据边界

本报告是**文档审查**，不构成机器检查、`REFINEMENT-PROOF` 或真实实现验证；不覆盖 §2.2 非目标。
独立性为 harness 内独立 agent 会话，非组织级第三方审查。新增/变更的 ISSUE 以 [Prima OS 实现阶段](../../../Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md) 为准。
