# COSIR/POSIR 双人格扩展候选采纳审核

> 文档类型：采纳审核报告（非规范；规范变更见 [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)）
>
> 日期：2026-10-14
>
> 审核输入：[COSIR/POSIR 双人格底座差距分析](2026-10-14-cosir-dual-personality-gap-analysis.md) 中的候选机制族
>
> 审核目标：对每一项候选逐条判断“是否正式纳入 POSIR 冻结语义”；审核通过项已同步写入 [COSIR 原语规范](../../../COSIR%20%E5%8E%9F%E8%AF%AD%E8%A7%84%E8%8C%83.md)，未通过或暂缓项保留为后续设计输入。

## 1. 审核准则

每项候选按以下顺序审核：

| # | 准则 | 判定问题 |
|---|---|---|
| R1 | 必要性 | 现有 11 类机制 + 用户态协议是否无法正确表达该语义？ |
| R2 | 原子性 | 错误语义是否依赖内核级 test-and-block、线性化点或生命周期清理？ |
| R3 | OS 中立性 | 是否引入 Linux/NT 专属对象名或语义？ |
| R4 | 最小性 | 是否存在更小的通用机制可覆盖多个 personality 需求？ |
| R5 | 可验证性 | 能否定义有限状态、不变量、失败原子性与证明义务？ |
| R6 | 集成风险 | 是否要求重构 Capability 位置、Task 身份或 GPU/NPU 计算介质模型？ |

结论分为：

- **采纳**：写入冻结语义，更新相关章节、权限、不变量、证明义务与总览。
- **不采纳**：保持现有边界，记录理由；不进入规范。
- **暂缓**：方向成立，但当前缺少独立设计所需的关键决策，先登记为后续设计切片。

## 2. 逐项审核

### 2.1 通用等待/唤醒（`WaitObject`）

**候选：** 内核级 `WaitObject`、`wait_objects`、`signal_object`、超时、等待任一/全部。

| 准则 | 判断 |
|---|---|
| R1 | **通过。** `recv` 的 `BlockedRecv` 不能等待任意对象集合；用户态轮询无法保证原子 test-and-block 和丢失唤醒边界。 |
| R2 | **通过。** 多对象等待、超时、对象销毁和唤醒竞争都需要内核线性化点。 |
| R3 | **通过。** `WaitObject` 只承载单调 generation 与等待者集合，不解释 Mutex/Event/Semaphore。 |
| R4 | **通过。** 单类 `WaitObject` 可覆盖多对象等待、可警报等待、完成通知与 futex 类重试逻辑，避免把多种 OS 对象搬进内核。 |
| R5 | **通过。** 可定义 W17、超时/销毁/取消规则，并加入 §17 证明义务。 |
| R6 | **低。** 不改变 Capability 位置；只扩展 `TaskStatus`、Task 阻塞字段和调度返回值。 |

**决定：采纳。**  
新增第 12 类常规机制 `WaitObject`；新增 `task_alert` / `task_upcall_recv` 作为 Task 机制的定向异步扩展；新增 W17，并更新 §4、§5、§6、§8、§10、§13、§17、§19。

### 2.2 具体同步对象（Mutex / Semaphore / Event / ConditionVariable）

**候选：** 将 NT/POSIX 同步对象直接纳入 `ObjectRef`。

| 准则 | 判断 |
|---|---|
| R1 | **不通过（作为一等对象不必要）。** 通用 `WaitObject` + LibOS 原子状态可表达事件、计数信号量与互斥的状态机；`WAIT_ALL` 由 `wait_objects` 的多对象模式承担。 |
| R2 | **部分通过。** NT mutex abandonment、可警报等待需要内核协作，但这由 `WaitObject` + upcall 提供。 |
| R3 | **不通过。** `Mutex`、`Semaphore`、`Event` 是 personality 对象名；直接进入 POSIR 会把 OS 语义固化进内核。 |
| R4 | **通过。** `WaitObject` 是更小机制。 |
| R5 | **可验证，但无必要。** 具体对象的 owner/abandonment 状态机应由 LibOS profile 证明。 |
| R6 | **高。** 会引入新的 Capability 目标类型和生命周期。 |

**决定：不采纳为一等 POSIR 对象。** 保留 `WaitObject` 为通用机制；LibOS 负责把 NT/POSIX 语义映射到 `WaitObject` + 共享内存原子状态。若未来 `COMPAT-TEST` 证明无法安全映射，再以独立设计重新审核。

### 2.3 Process / Thread / ShareGroup

**候选：** 拆分 Process 与 Thread，或引入 ShareGroup/ThreadGroup 显式共享 vspace、Capability 表和根 Namespace。

| 准则 | 判断 |
|---|---|
| R1 | **通过。** 当前 `Task` 同时绑执行上下文、`vspace`、Capability 位置、邮箱和计算介质，无法表达 Linux `clone` 或 Windows Process/Thread。 |
| R2 | **通过。** 共享句柄表、vspace 与 fork 生命周期必须在内核状态中建模。 |
| R3 | **需要设计。** Process/Thread 名可以映射为中立 `ShareGroup` 对象，但执行上下文与 Capability holder 的语义必须重新定义。 |
| R4 | **待定。** `ShareGroup` 是候选最小解，但尚未证明能同时覆盖 POSIX fd 表、NT 句柄表和 GPU/NPU Task。 |
| R5 | **暂不可验证。** `HeldBy(TaskId)` 目前是 W3/W4 的基础；若 Capability 表变为共享，位置必须改为新的 holder 对象或引入引用计数语义。 |
| R6 | **极高。** 影响 `Task`、`HeldBy`、W3/W4、W5、W9、fork/exec 生命周期、Audit 和 GPU/NPU 介质分支。 |

**决定：暂缓。** 采纳它需要先完成独立的 holder 模型设计，不能作为 `task_create` 参数扩展直接塞入。登记为后续 P0 设计切片；在完成前，双人格 Windows 侧的进程/线程语义仍不完整。

### 2.4 定向 upcall / 可警报等待 / APC

**候选：** `task_alert`、per-Task pending upcall 队列、`alertable` 等待中断。

| 准则 | 判断 |
|---|---|
| R1 | **通过。** `send` 只能投递消息，不能原子中断 `BlockedWait`；APC、Linux 信号和可警报等待都需要定向投递 + 等待中断。 |
| R2 | **通过。** “下次可警报等待时投递”需要内核等待状态与 pending 队列的原子交错。 |
| R3 | **通过。** upcall payload 为不透明 bytes，内核不解释 APC/信号语义。 |
| R4 | **通过。** 复用 `TaskSend` 权限与 `WaitObject` 等待机制，不新增独立对象类。 |
| R5 | **通过。** 可定义 pending_upcalls FIFO、`WaitAlerted` 不消费队列、`task_upcall_recv` 清空队列、销毁清理。 |
| R6 | **低。** 只增加 Task 字段与状态转换；不改变 Capability 位置。 |

**决定：采纳。**  
作为 `WaitObject` 机制的配套子机制正式写入 §21，并在 §5、§8、§13、§16（W17）、§17 同步更新。**边界：** `task_alert` 只提供定向队列与可警报等待；kernel APC 的执行时机、特权级切换和内核栈上下文仍属 COSIR/LibOS 细化，不在 POSIR。

### 2.5 I/O 完成队列（IOCP / io_uring / epoll）

**候选：** 内核 `CompletionQueue` 对象及 `cq_submit` / `cq_wait`。

| 准则 | 判断 |
|---|---|
| R1 | **部分通过。** 标准完成协议缺失，但 correctness 所需的部分已由 `WaitObject` + 共享 Segment 覆盖；`submit` 语义属于设备/文件服务，不是通用机制。 |
| R2 | **不必要内核化。** 完成记录的排队与唤醒可由 `WaitObject` + 共享环协议承担。 |
| R3 | **不通过。** `submit`/`Completion` 若命名和语义绑定 IOCP 或 io_uring，会违反 OS 中立性。 |
| R4 | **通过。** `WaitObject` 是更小机制；标准完成环可作为用户态协议。 |
| R5 | **可协议化。** 需要定义共享环的 ABI/协议，但不进入 POSIR 对象模型。 |
| R6 | **中。** 若不内核化，依靠共享内存；性能需测量。 |

**决定：不采纳内核 `CompletionQueue` 原语；暂缓“标准完成协议”。** 先在 `WaitObject` + 共享 Segment 上实现用户态完成环并测量，之后再决定是否需要 POSIR 级对象或协议文本。

### 2.6 `memseg_protect`

**候选：** 原地改变映射 `access`。

| 准则 | 判断 |
|---|---|
| R1 | **通过。** `unmap + map` 会破坏映射身份和 COW/共享语义，并引入中间窗口。 |
| R2 | **通过。** 权限切换必须是对现有映射的原子更新。 |
| R3 | **通过。** 仅改变 `PageMapping.access`，不引入 OS 专属语义。 |
| R4 | **通过。** 一个独立操作即可覆盖 `mprotect` / `VirtualProtect` 的机制部分。 |
| R5 | **通过。** 可并入 W7/W8，定义 widened 权限检查规则。 |
| R6 | **低。** 不影响 Capability 位置或 Task 身份。 |

**决定：采纳。**  
新增 §7.5 `memseg_protect`，更新 §13.1 Audit、§17.3 地址空间完整性、§19 总览和 W7/W8 的关联约束。

### 2.7 文件/流对象

**候选：** 内核 `FileObject` 或 `file_open/read/write` 原语。

| 准则 | 判断 |
|---|---|
| R1 | **不通过。** 文件语义可以由用户态文件系统服务 + 会话 Capability + 共享 Segment 表达。 |
| R2 | **非必要。** 文件锁、`O_APPEND`、mmap 文件等是服务协议和 LibOS 职责。 |
| R3 | **不通过。** 内核 `FileObject` 会固化单一 personality 的命名与语义。 |
| R4 | **不通过。** 若确实需要内核快速路径，更中立的候选是 `Stream`/`IOSession`，但尚未设计。 |
| R5 | **暂不可验证。** 必须先定义跨 personality 的流语义和性能模型。 |
| R6 | **极高。** 会将文件系统策略吸入 TCB。 |

**决定：不采纳。** 保持白皮书既有“文件系统在用户态”边界；未来若性能证据要求，可重新审核 OS 中立的 `Stream`/`IOSession`，但不使用 `FileObject` 这一人格化命名。

## 3. 审核结果汇总

| 候选 | 结论 | 规范动作 |
|---|---|---|
| 通用 `WaitObject` + 等待/唤醒 | 采纳 | 新增第 12 类机制；§5.6、§21、W17、§17.9、§19 |
| 具体 Mutex/Semaphore/Event | 不采纳为一等对象 | 保留 LibOS 映射边界 |
| Process/Thread/ShareGroup | 暂缓 | 登记为 holder 模型设计切片 |
| 定向 upcall / alertable wait | 采纳 | §21.3；Task 字段；W17；Audit |
| 内核 CompletionQueue | 不采纳 | 暂缓标准完成协议；先共享环 + WaitObject |
| `memseg_protect` | 采纳 | §7.5；W7/W8；Audit；§19 |
| 内核 FileObject | 不采纳 | 保持用户态 FS；未来评估 OS 中立 Stream |

本次审核后，Bootstrap 后常规机制从 **11 类** 变为 **12 类**；加 Bootstrap 共 **13 项**。这是 H1“机制表达力”假设的正式修订：不是新增具体 OS 对象，而是补一个通用的等待/唤醒机制类。

## 4. 证据边界与后续

- 本审核与随之进行的规范修订是**设计/规范级**变更，不产生 `MODEL-TEST`、QEMU、Lean 或 `COMPAT-TEST` 证据。
- `WaitObject`、定向 upcall 和 `memseg_protect` 已写入抽象语义，但仍需后续独立切片补齐：Rust host 模型、Lean 不变量、细化层实现与失败原子性测试。
- Process/Thread/ShareGroup 暂缓不等于否决；它是双人格 Windows 侧进程模型的必要条件，但必须先解决 Capability holder 与 fork 共享语义。
- 标准 I/O 完成协议暂缓不等于不必要；它需要在 `WaitObject` 落地后以共享环原型评估性能和 ABI。
