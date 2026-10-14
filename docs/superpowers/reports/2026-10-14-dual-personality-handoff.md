# 双人格工作流交接与上下文压缩

> 文档类型：交接快照（非规范、非设计、非证据）
>
> 日期：2026-10-14
>
> 基线提交：`5838630`（`docs(spec): adopt wait objects, directed upcalls, and memseg_protect (DOC-007)`）
>
> 用途：为后续 session / agent 提供压缩上下文；精确语义始终以 [[COSIR 原语规范]] 为准。

## 1. 当前冻结语义

[[COSIR 原语规范]] 已于 2026-10-14 正式采纳：

| 项 | 规范位置 | 状态 |
|---|---|---|
| 通用 `WaitObject` 与等待/唤醒 | §5.6、§21.1–21.2、W17 | 冻结；第 12 类常规机制 |
| 定向 upcall / 可警报等待 | §21.3 | 冻结；Task 机制异步扩展 |
| `memseg_protect` | §7.4 | 冻结；MemorySegment 操作 |

计数与不变量：

- Bootstrap 后常规机制：**12 类**；
- 加 Bootstrap：**13 项**；
- 良构不变量：**W1–W17**；
- 证明义务：**§17.1–§17.9**。

## 2. 仍未采纳 / 暂缓

| 项 | 状态 | 下一步条件 |
|---|---|---|
| Mutex / Semaphore / Event / ConditionVariable | 不采纳为一等 POSIR 对象 | 由 `WaitObject` + LibOS 映射；`COMPAT-TEST` 证明不足再复议 |
| Process / Thread / ShareGroup / holder 模型 | **暂缓，当前 P0 设计** | 先冻结 AddressSpace/CapTable/ShareGroup 模型与 W3/W4 修订 |
| 内核 `CompletionQueue` | 不采纳 | 先用 `WaitObject` + 共享环原型测量 |
| 标准 I/O 完成协议 | 暂缓 | 共享环 ABI 与性能数据 |
| 内核 `FileObject` | 不采纳 | 保持用户态文件系统边界 |
| OS 中立 `Stream` / `IOSession` | 暂缓 | 若性能证据要求内核快速路径再设计 |

## 3. 关键约束

1. 不引入 Linux/NT 专属对象名到 POSIR；
2. 不破坏 W3/W4（Capability 单一位置与容器一致）；
3. 不破坏 W5（结构性根 Namespace）、W9（调度状态）、W17（等待一致性）；
4. CPU Task、GPU/NPU Task 的既有介质语义必须兼容；
5. `WaitObject`/upcall 的冻结语义不能被 holder 重构无声破坏。

## 4. 下一阶段入口

下一阶段为 **Process / Thread / ShareGroup holder 模型设计**：

- 设计草案：[[docs/superpowers/specs/2026-10-14-dual-personality-holder-model-design|双人格 Holder 模型设计草案]];
- 审核输入：[[docs/superpowers/reports/2026-10-14-cosir-dual-personality-adoption-review|双人格扩展候选采纳审核]];
- 差距分析：[[docs/superpowers/reports/2026-10-14-cosir-dual-personality-gap-analysis|COSIR/POSIR 双人格底座差距分析]]。

目标不是立刻改 POSIR，而是先冻结候选模型、迁移顺序和 W3/W4/W9/W17 的影响面；该设计通过后再进入规范修订与实现切片。

**H0 评审状态（2026-10-14）：** 设计草案**条件通过**，评审报告见 [[docs/superpowers/reports/2026-10-14-dual-personality-holder-model-h0-review|Holder 模型 H0 评审报告]]。

H1 前必须完成：

- C1：`HeldByCapTable` + `HeldByShareGroup` 位置模型冻结；
- C2：`CapTable`/`AddressSpace` 内部对象引用计数规则；
- C3：`fork` 的 `cap_table_dup` 权限与 W2/Audit 论证；
- C4：Thread/ShareGroup 生命周期、zombie/wait 状态机；
- C5：第一版 Linux/Windows profile 映射表；
- C6：现有 `task_create`/`task_destroy` 迁移路径。

## 5. 证据边界

- 本快照只记录当前状态与入口链接，不产生新证据；
- 规范变更的证据仍以 `DOC-007` 与后续设计/实现评审记录为准。
