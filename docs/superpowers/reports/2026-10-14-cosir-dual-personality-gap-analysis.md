# COSIR/POSIR 双人格底座差距分析

> 文档类型：差距分析报告（非设计、非规范、非证据）
>
> 日期：2026-10-14
>
> 分析对象：[COSIR 原语规范](../../../COSIR%20原语规范.md) 当前版本、[Prima OS 白皮书](../../../Prima%20OS.md) 的兼容性路线与开放问题
>
> 输入：用户提交的《对这份 COSIR/POSIR 规范的评估》
>
> 结论边界：本报告只核验评估中的事实判断、修正优先级与机制边界，不把任何候选机制写成冻结语义，不新增 POSIR 原语、不变量或证明义务。正式采纳任一候选扩展前，必须走“设计 → 评审 → 规范修订 → 验证”的流程。
>
> **后续状态（2026-10-14）：** 本报告的候选已进入逐项采纳审核。`WaitObject`/定向 upcall 与 `memseg_protect` 审核通过并写入 [[COSIR 原语规范]]；Process/Thread/ShareGroup 暂缓，具体 Mutex/Event、内核 CompletionQueue、FileObject 不采纳。采纳结论见 [[docs/superpowers/reports/2026-10-14-cosir-dual-personality-adoption-review|COSIR/POSIR 双人格扩展候选采纳审核]]。本报告保留审核前分析口径。

## 1. 结论摘要

原评估的主结论成立：当前《COSIR 原语规范》作为**对象能力微内核的抽象状态机**质量高，Capability、Namespace、Exception、Send/Recv、Device 与形式化边界都扎实；但作为**同时支撑 Linux 与 Windows NT 语义的双人格底座**，它缺少若干内核必须参与的关键机制。

不过，原评估把“缺口”与“建议补法”绑得略紧，需要做四点修正：

1. **通用等待/唤醒机制是硬缺口，同步对象是否一等化则是设计选择。** NT 与 Linux 都要求“原子地检查状态并阻塞/唤醒”，当前规范只有 `recv` 的 `BlockedRecv` 一条阻塞路径。最小内核机制应是通用 `Waitable`/等待队列协议，`Mutex`、`Semaphore`、`Event` 可以在其上由 LibOS 管理层语义；但如果要保留 NT 的 owner、递归锁、abandonment、可警报等待和 `WAIT_ALL` 原子性，部分状态需要进入内核。
2. **Exception 的“投递通道”已经存在，缺的是线程定向队列和可警报等待状态。** `raise_exception` + `enqueue_or_deliver` + `FromKernel(ExceptionEvent)` 已经是一条复用消息通道的投递路径；APC 真正缺的是 per-Thread pending upcall 队列、`alertable` 阻塞状态、以及“下次可警报等待时投递”的线性化语义，而不是又一条消息机制。
3. **I/O 完成抽象是共同需求，但 `CompletionQueue` 未必需要成为内核对象。** IRQ + `ExcSource` + 共享 Segment + 等待队列可以定义标准用户态完成协议；在性能数据证明用户态协议无法满足前，不宜直接把 IOCP/io_uring 的提交语义放进内核对象。
4. **FileObject 不宜轻率进入内核。** 规范与白皮书都明确文件系统在用户态；跨 Task 文件语义需要的是 OS 中立的 `Stream`/`IOSession` 协议或明确的用户态服务方案，而不是语义上偏向单一 personality 的 `FileObject`。

优先级建议如下：

| 优先级 | 扩展族 | 依赖关系 | 主要风险 |
|---|---|---|---|
| P0 | 通用等待/唤醒 + 可警报阻塞状态 | 被同步、I/O、APC 共同依赖 | 丢失唤醒、原子 test-and-block、W9 状态机扩展 |
| P0 | Process/Thread/ShareGroup 共享模型 | 决定等待结果与 Capability 持有者身份 | W3/W4 位置模型、GPU/NPU Task 兼容 |
| P1 | 定向 upcall / APC | 依赖 P0 的可警报等待 | fault 状态与 pending queue 共存、W15 |
| P1 | I/O 完成队列或标准完成协议 | 依赖 P0 的等待队列 | 内核对象 vs 用户态服务边界 |
| P2 | `memseg_protect` + 按需分页标准协议 | 依赖较低的独立内存机制 | W7/W8、W^X、COW 并发 |
| P3 | 文件/流边界决策 | 影响兼容性上限与性能模型 | H1/H3、OS 中立性 |

## 2. 核验依据

- `COSIR 原语规范.md`，重点核验章节：§2.1/§2.2（目标与非目标）、§4（Capability）、§5.2（Task 与 vspace）、§6.3（`enqueue_or_deliver`）、§7（MemorySegment）、§8（Task 与调度）、§9（Send/Recv）、§12（Exception/Upcall）、§15（Device）、§17（证明义务）、§18.0（POSIR/COSIR 边界）、§19（原语总览）。
- `Prima OS.md`，重点核验：§3（可证伪假设）、§4（分层）、§6（安全模型）、§8（兼容性路线）、§20（开放问题）。
- 规范文件最近一次相关提交为 `70e1ce7`（2026-10-12，§13.1 audit envelope）；白皮书相关提交为 `2a9f051`（2026-10-11）。
- 原评估中对当前原语状态的事实性描述，逐条对照规范原文核验；下文中“成立”“部分成立”“需修正”即指该核验结果。

## 3. 逐项事实核验

### 3.1 通用等待队列：成立，且是最致命的缺口

**规范现状：**

```text
TaskStatus = Ready | Running | BlockedRecv | Faulted
```

- `recv` 在邮箱为空时把 Task 置为 `BlockedRecv`（§9.2）；
- `enqueue_or_deliver` 只对 `BlockedRecv` 做直接交付（§6.3）；
- Timer 通知只能入队消息，不能让 Task 直接阻塞等待定时器；
- Task 没有“等待多个对象”“等待超时”“可警报等待”“等待任一/全部”的抽象状态。

**结论：** 原评估成立。`NtWaitForMultipleObjects`、`futex`、`epoll_wait`、`signalfd`、`timerfd` 的**共同机制**是“原子地检查状态并进入等待，稍后由一个线性化唤醒操作使其可运行”。当前规范没有这个机制。用户态只能通过反复 `recv` + 定时器轮询来模拟，会同时损失原子性、公平性、正确性和性能。

**修正：** 不要把它表述为“缺一个 `wait_objects` 系统调用”这么简单。它需要定义：

- 等待对象的身份与生命周期；
- test-and-block 的线性化点；
- 唤醒 `one`/`all`/`count` 的语义；
- 超时、取消、对象销毁、Task 销毁与可警报中断的交错；
- 等待注册是否影响 Capability 位置与 W3/W4。建议等待注册是临时关系，不占用 Capability 位置，但必须在规范中明确。

### 3.2 同步原语对象：事实成立，但“必须一等对象”是设计选择

**规范现状：** `ObjectRef` 当前包含 Segment、Task、Namespace、Timer、ExcSource、Device、Node；没有 Mutex、Semaphore、Event、ConditionVariable、WaitQueue。

**结论：** 原评估说“没有这些对象”成立；但“必须把它们作为 POSIR 一等对象”不是唯一方案，也不是当前证据能直接推出的结论。

可以分三层看：

| 层 | 内容 | 可能的归属 |
|---|---|---|
| A. 阻塞/唤醒机制 | 等待队列、原子 test-and-block、超时、取消、唤醒一到多 | 内核机制（否则无法正确实现） |
| B. 可等待对象协议 | `signaled` 状态、等待者集合、一次性/自动重置、计数 | 可内核可 LibOS；NT 语义下部分必须内核 |
| C. personality 对象语义 | pthread mutex、NT mutant、semaphore owner、event type | LibOS / personality 层 |

如果只实现 A 和 B，LibOS 可以用共享 Segment + 内核等待机制实现大部分 POSIX 互斥/条件变量；若要在 Windows 侧精确支持 `NtWaitForMultipleObjects(WAIT_ALL)`、mutex abandonment、可警报等待和 APC，通常需要让 B 进入内核或获得等价的内核协作。**建议把“机制 A+B 的最小内核化”作为候选，把“C 的对象命名与 ABI 语义”留在 LibOS；只有证据表明 NT 语义无法安全下沉时才把具体对象提升为一等对象。**

### 3.3 APC / 信号 / SEH：结论基本成立，但 Exception 通道可复用

**规范现状：**

- Exception 是**事件源订阅**模型：`exc_subscribe` + `raise_exception` fan-out（§12.4/§12.5）；
- 硬件异常会把 `faulting_task` 置为 `Faulted`，并写唯一 `pending_exc`（§5.2/§12.5）；
- `FromKernel(ExceptionEvent)` 通过 `enqueue_or_deliver` 投递，不引入新通道（§12.1）；
- `exc_resume` 只恢复一个被暂停的异常帧（§12.10）。

**结论：** 原评估成立：这不等于 APC、完整 Linux 信号或 SEH。需要修正的地方是：

1. **APC 缺的不是通道，而是“线程定向 + 可警报等待 + pending queue”。** 当前 ExceptionSource 是 fan-out 订阅；硬件异常天然绑定 `faulting_task` 是特例，不是通用的 per-Thread 异步队列。
2. `pending_exc` 是单一、排他的 fault 状态，不能拿来当 APC/信号 pending 队列。需要独立的 `pending_upcalls` 或 `pending_signals` 状态，并定义它和 `Faulted` 的优先级。
3. Linux 信号的 blocked/pending/handler/sigreturn 生命周期仍应由 LibOS 实现；内核需要提供的是“可被中断的等待”和“定向投递”。`exc_resume` 只恢复 fault frame，不能自动承担 signal trampoline / `rt_sigreturn` 的语义。
4. SEH 的结构化展开和帧链完全是 personality/运行时责任，不应进入 POSIR；但若 APC 要求在内核态排队，排队和投递点需要内核机制。

**建议：** 增加“线程定向 upcall”候选，而不是泛化为“Exception 补 APC”。语义上可以是：

```text
thread_alert(thread_cap, upcall) -> ()
```

其中 `upcall` 是内核不解释的 payload；投递时机为“目标 Thread 下次进入 `alertable` 等待”或“下次从内核返回用户态前”，具体由细化层与 LibOS 约定。`wait_objects(..., alertable = true)` 必须在等待线性化点上原子地检查 pending upcall；有 upcall 时返回 `Alerted` 而不是继续阻塞。

### 3.4 I/O 完成模型：事实成立，但先定义协议再决定对象化

**规范现状：** §15 把 MMIO、IOPort、DMA、IRQ 封装为 Device Capability；IRQ 被路由为 `ExternalInterrupt` 事件源。但没有统一的完成队列、`submit` 语义、批量等待或完成记录格式。

**结论：** 原评估成立。`IOCP`、`io_uring`、`epoll`、`aio` 的共同需求是“提交异步操作 → 稍后批量取完成事件 → 可等待”。当前规范只能由用户态服务用消息和共享 Segment 自定义协议，缺少标准抽象。

**修正：**

- `epoll` 的 readiness 和 IOCP 的 completion 不是同一语义；公共部分主要是“等待队列 + 完成记录 + 事件源”。
- `cq_submit(cq, op)` 不应轻易进入内核，因为 `op` 的设备/文件语义是用户态服务责任。内核最多需要提供 OS 中立的 `CompletionQueue` 等待对象，或定义一个“服务端提交、客户端等待”的共享环协议。
- 在缺少延迟数据前，不能断言用户态协议一定性能不足；但 `WAIT_ALL` 和批量等待的正确性依赖 P0 的等待机制。

### 3.5 Task 模型与进程/线程分离：成立，且是 P0 依赖项

**规范现状（§5.2/§8.1）：**

```text
Task {
    creator, kind, node, status,
    vspace, dev_vspace, entry,
    root_ns_cap, mailbox, pending_return,
    pending_exc, kernel_resume_frame
}
```

`task_create` 会创建**空地址空间**的新 Task，映射代码页，铸造管理 Capability，把 `initial_caps` 移动到新 Task，并派生共享的根 Namespace Capability。

**结论：** 原评估成立。当前 Task 同时承担：

- 执行上下文（入口、栈隐含在 vspace 中）；
- 地址空间（`vspace`）；
- Capability 持有者位置；
- 邮箱与阻塞状态；
- GPU/NPU 计算介质抽象（`kind`）。

它无法自然表达：

- Windows 的 Process（对象表 + 地址空间 + 线程集合）与 Thread（执行上下文）；
- Linux `clone` 的 `CLONE_VM`、`CLONE_FILES`、`CLONE_THREAD`、`CLONE_SIGHAND` 等共享组合；
- 两个线程共享 vspace 和句柄表但独立调度；
- fork 后共享或复制文件描述符表等混合语义。

**修正与方案：**

- 直接“拆成 Process + Thread”是一个候选，但必须先解决 Capability 位置问题：如果多个线程共享 Capability 表，`HeldBy(task)` 不再能表达唯一位置。
- 另一个更中立的方案是引入 `ShareGroup`（或 `ThreadGroup`）：它持有 vspace、Capability 表、root Namespace 和进程级状态；`Thread`/`Task` 是执行上下文。这样 Windows Process/Thread 和 Linux clone 都可以映射为“新建 Thread + 选择共享哪些 ShareGroup 组件”。
- 还必须兼容现有 `TaskKind = ComputeTask(CPU|GPU|NPU|FPGA)` 抽象；GPU/NPU Task 不调用 send/recv，也不是完整 CPU 执行上下文。重构不能破坏这些已经形式化的介质语义。
- 该决策直接影响等待结果的身份：`wait_objects` 是等待“线程”还是“进程”？句柄在 `fork` 后如何派生？Capability 位置如何审计？建议先决定 ShareGroup 模型，再冻结等待/同步语义。

### 3.6 内存保护变更与按需分页：成立，且有独立的小修可先做

**规范现状：**

- `memseg_map` 要求目标区间完全未映射（§7.2）；
- `memseg_unmap` 只能删除映射，不能只改 `access`；
- 没有 `memseg_protect` 或等价操作；
- PageFault 的异常语义、`exc_resume`、用户态 COW 示例都已经存在（§12.8–12.10），但没有“填充页、更新 PTE、重播指令”的标准协议。

**结论：**

- `mprotect` / `VirtualProtect` 需要独立操作，原评估成立；用 `unmap + map` 模拟会引入窗口、丢失 COW/共享映射状态，并可能违反失败原子性。
- 按需分页已有足够机制表达用户态路径，但“并发 fault 去重、填充完成与映射建立之间的原子性、COW 协调”仍缺标准协议；这部分至少在规范中应作为细化义务或标准用户态协议固定下来。

**建议：** 先加入一个低风险的 `memseg_protect` 候选：

```text
memseg_protect(task_cap, target_base, page_count, new_access) -> ()
```

前置语义至少需要定义原始映射存在、`new_access` 非空且不超过 `segment.allowed_access`、调用者持有 `TaskManageVSpace`、变更权限所需授权、入口页 W^X 约束，以及 W7 的一致性更新。

### 3.7 文件/流抽象：事实成立，但这是边界决策问题

**规范现状：**

- Namespace 是 `Path ⇀ Bound(cap) | Mount(cap)`，不是文件系统；
- 白皮书 §8、§15 明确文件系统和协议栈是用户态服务；
- 没有打开实例、偏移、文件锁、`read`/`write`、`mmap` 文件的一等对象。

**结论：** 原评估指出的能力缺口成立，但不能简单地说“应该把 FileObject 纳入 POSIR”。这触及项目最核心的 H1/H3：

- 如果内核引入 `FileObject`，语义会天然偏向某一 OS，并有把文件系统策略吸进 TCB 的风险；
- 如果全部留在 LibOS，则必须面对每次 `read/write` 走 IPC/共享环的性能代价，以及 `O_APPEND`、文件锁、异步 I/O、`mmap` 文件等语义的复杂度。

**建议：** 把选项写清楚，而不是预设结论：

1. **保持用户态服务 + 标准会话协议**（当前路线）；
2. **定义 OS 中立的 `Stream`/`IOSession` Capability**：表示可读/可写、有完成通知的字节流，不命名为文件；
3. **内核 `FileObject`**：只有在明确接受 H1/H3 代价并完成跨 personality 语义映射后才考虑。

在测量出 IPC/共享环路径的实际代价之前，不建议直接跳到 3。

## 4. 对原评估的逐条修正

| 原评估表述 | 核验结果 | 建议修正 |
|---|---|---|
| “通用等待队列是最致命的缺口” | 成立 | 保持，并把它放在 P0；同步、APC、I/O 都依赖它 |
| “同步原语对象必须纳入 POSIR” | 部分成立 | 机制最小内核化即可；对象语义可下沉 LibOS，NT 关键语义可再提升 |
| “Exception 不足以表达 APC/信号/SEH” | 基本成立 | 补 per-Thread pending upcall 与 alertable wait；SEH 仍属 LibOS |
| “缺 I/O 完成队列抽象” | 成立 | 先定义完成协议与等待关系；内核 CQ 需性能证据 |
| “Task 绑定太死，应拆 Process/Thread” | 成立 | 可拆 Process/Thread，也可引入 ShareGroup；先解决 Capability 位置模型 |
| “内存保护变更缺一个操作” | 成立 | `memseg_protect` 是低风险候选，但与 W7/W8 同步设计 |
| “文件/流要么 kernel 对象要么明确用户态” | 成立 | 增加第三选项：OS 中立的 Stream/IOSession |
| “不补这些，Windows 侧会退化成用户态模拟” | 成立 | 补充：是否退化的前提是 H4 性能测量；先建立 P0 机制再避免误判 |

## 5. 候选机制画像（非规范、非承诺）

本节只给出设计讨论所需的形状，不构成 POSIR 语义。

### 5.1 通用等待/唤醒机制

候选原语族：

```text
WaitableState      = Signaled | NonSignaled | Counted(Nat) | ...
Waiter             = { thread: ThreadId, mode: Any | All, alertable: Bool,
                       deadline: Option<Instant>, result_slot: ... }

wait_objects(objs: List<CapId>, mode: Any | All,
             alertable: Bool, timeout: Option<Instant>) -> WaitResult

signal_object(obj_cap, wake: One | All | Count(Nat)) -> ()
```

语义要求：

- test-and-block 必须在单一线性化点上完成；不能在“测试状态”和“注册等待者”之间被唤醒交错；
- 唤醒必须消除所有相应的等待者，或使计数/状态转移保持可解释；
- 超时、取消、对象销毁、Task 销毁、可警报 upcall 必须与唤醒竞争并只有一个胜出；
- 等待注册不消耗 Capability，不改变 `HeldBy`，但被等待对象销毁时应使请求返回 `TargetDead` 或等价状态；
- 若对象是 LibOS 管理状态，内核需要能验证“期望状态”而不破坏抽象。

替代的极简机制是 futex 式：

```text
wait_on(seg_cap, offset, expected, timeout) -> WaitResult
wake(seg_cap, offset, count) -> Nat
```

它更小，但 `WAIT_ALL`、可警报等待和 NT 对象身份仍需要对象协议或额外协作；因此双人格底座建议优先设计 `Waitable` 协议，而把 futex 作为可选细化实现。

### 5.2 Process / Thread / ShareGroup

候选结构：

```text
ShareGroup {
    vspace, cap_table, root_ns_cap,
    signal_state?, child_collection?, ...
}

Thread {
    share_group, entry, stack, registers,
    status, alert_state, pending_upcalls,
    mailbox, pending_return,
    pending_exc, kernel_resume_frame,
}
```

设计问题：

1. Capability 位置是否变成 `HeldBy(ShareGroup)`？如果是，W3/W4 与审计事件需要 generalized holder；Capability 的移动语义会变化。
2. 线程本地凭据与进程共享凭据如何分离？句柄表 `fork` 的复制/共享如何建模？
3. GPU/NPU Task 是否也有 ShareGroup，还是保留为独立 `ComputeTask` 分支？
4. 等待结果如何指定目标线程，唤醒如何从共享对象选择等待者？

### 5.3 定向 upcall / 可警报等待

候选形状：

```text
thread_alert(thread_cap, upcall) -> ()
Thread.pending_upcalls: Queue<Upcall>
```

投递规则候选：

- 目标 Thread 处于 `alertable` 等待：原子地中断等待，返回 `Alerted`，不消费 upcall payload？或消费一条？
- 目标 Thread 处于用户态运行：回到内核后优先检查 pending upcall；
- 目标 Thread 被抢占/Ready：保持排队；
- 与 `Faulted`/`pending_exc` 的优先级要显式定义；
- 不应违反“不可伪造来源”：upcall 的来源由内核记录，但内核不解释 payload。

Linux 信号和 SEH 的完整语义仍在 LibOS；本机制只提供定向投递和可中断等待。

### 5.4 I/O 完成

候选形状 A（用户态协议）：IRQ → `ExcSource` → 共享环；等待端使用通用 `wait_objects`。

候选形状 B（内核对象）：

```text
CompletionQueue {
    pending:   List<Completion>,
    waiters:   List<Waiter>,
}
Completion = { op_id, status, result }
```

关键边界：`submit` 的 `op` 如何由用户态服务解释；内核 CQ 是否只负责完成记录和唤醒。若没有性能证据，A 是更符合 H1/H3 的起点。

### 5.5 `memseg_protect` 与按需分页

```text
memseg_protect(task_cap, target_base, page_count, new_access) -> ()
```

需要明确的规则：

- `new_access` 非空，且每页新权限不超过 `segment.allowed_access`；
- 原区间必须已映射；不能通过 protect 建立新映射；
- 调用者需要 `TaskManageVSpace`，是否还需要 Segment Capability 取决于“增大权限”是否被允许；
- 入口页可执行权限、W^X、COW 只读映射的语义必须更新；
- 新增 Audit 事件，例如 `VspaceProtChanged`；
- W7 要覆盖同一 VirtPage 的 access 从旧值到新值的原子切换。

按需分页标准协议可以在这个候选包中一并定义，而不是依赖每个 LibOS 各自发明。

### 5.6 文件/流

建议将决策记录为三选一：

| 方案 | 优点 | 代价 |
|---|---|---|
| 用户态服务 + 会话协议 | 最符合当前 H1/H3 | 性能与复杂语义全部由协议承担 |
| OS 中立 Stream/IOSession | 可做内核级快速路径和统一等待 | 需要抽象出跨 personality 的流语义 |
| 内核 FileObject | 最接近传统 OS ABI | 最可能固化单一 personality，风险最高 |

## 6. 采纳顺序与门禁

建议按以下顺序推进：

1. **P0-a：通用等待/唤醒机制设计。** 先冻结 `Waitable` 的状态、等待者、线性化点、超时/取消/销毁规则。
2. **P0-b：Process/Thread/ShareGroup 设计。** 解决 Capability 位置、等待者身份和 fork 共享语义；与 P0-a 联合评审。
3. **P1-a：`alertable` 等待 + 定向 upcall。** 以 P0 为基础定义 APC；不变式 W15 与 W9 同步扩展。
4. **P1-b：完成协议/CQ。** 先用共享环 + `wait_objects` 做原型，测量后再决定是否内核化 `CompletionQueue`。
5. **P2-a：`memseg_protect` + 按需分页协议。** 相对独立，可并行设计，但必须在 W7/W8 层面收口。
6. **P3：文件/流边界决策。** 基于 H4 性能和兼容性目标做正式选择。

每个候选扩展进入规范前必须回答：

1. 能否由现有 11 类机制 + 用户态协议表达？
2. 若不能，新增的是**机制**（安全/原子性/性能所需）还是 **personality 策略**？
3. 是否引入 OS 专属对象或语义？若引入，如何证明仍满足 OS 中立性？
4. 影响哪些不变量（W3/W4/W7/W9/W13/W15 等）与证明义务（§17）？
5. 可用什么证据等级验证：`MODEL-TEST`、QEMU、`COMPAT-TEST`、`KERNEL-TRACE`、`REFINEMENT-PROOF`？

## 7. 对现有不变量与证明义务的影响

| 现有内容 | 候选扩展的影响 |
|---|---|
| W3/W4（Capability 单一位置与容器一致） | ShareGroup/共享 Capability 表会引入新的 holder 形态；等待注册本身建议不占位置，但必须显式声明 |
| W7（虚拟映射有效） | `memseg_protect` 需要把 access 变化纳入同一映射有效性不变量 |
| W8（可执行入口显式授权） | protect 改变 X 权限、入口页 W^X 时必须同步重新校验 |
| W9（调度状态一致） | `BlockedWait`、`Alerted`、pending upcall 会扩展 Task/Thread 状态机；SMP 下“至多一个 Running”转为每核约束 |
| W11/W13（Timer/Audit） | 等待超时、完成事件、upcall、protect 需要新的 AuditKind 与生命周期事件 |
| W15（ExceptionSource 一致性） | 定向 upcall 与订阅 fan-out 是两条路径，需要分别定义来源与生命周期 |
| §2.1 第一阶段目标 | 若采纳 P0，需要新增“无丢失唤醒”“超时/取消原子性”“等待者与对象销毁一致”等目标 |
| §17 证明义务 | 等待线性化点、警报中断、ShareGroup 位置模型都需要新的安全结论与保持性证明 |

## 8. 不建议现在直接做的事

1. **不要把 `Mutex/Semaphore/Event/FileObject` 直接加入 `ObjectRef` 而不先定义 holder、等待队列和权限模型。** 这会把规范从“最小机制”推向“具体 OS 对象仓库”。
2. **不要把 `exc_resume` 当作信号返回的完整机制。** 它恢复的是 fault frame；Linux 信号栈帧、handler 生命周期和 `rt_sigreturn` 仍属 LibOS/ABI profile。
3. **不要仅凭 I/O 性能直觉把提交 API 放进内核。** 先定义 OS 中立的完成协议并测量；IOCP/io_uring 的提交路径各有语义。
4. **不要在用户态模拟 `WAIT_ALL` / APC / 可警报等待。** 这些语义需要原子 test-and-block 与抢占点，用户态忙等或轮询不能提供相同保证。
5. **不要在当前 P0/P1 尚未评审前修改 `Prima OS.md` 的前三阶段兼容承诺。** 白皮书当前明确不承诺 Windows NT ABI；双人格目标若要进入路线图，应先更新架构假设，再同步 COSIR。

## 9. 证据边界与后续

- 本报告是文档层面的差距分析与候选设计梳理，不产生实现、模型测试或形式化证据。
- 所有“规范现状”均来自对当前 Markdown 的静态核对；没有运行 `cosir`、Lean 或 QEMU 来验证候选机制。
- 本报告不改变 `COSIR 原语规范.md` 第 1–20 章的冻结语义；若要在规范中接纳任何候选，应先把对应小节提升为设计要求，再补充模型、测试和证明义务。
- 下一步建议：为 P0-a（通用等待/唤醒）和 P0-b（ShareGroup/Thread）分别编写设计草案，先解决 holder 与等待者身份问题，再进入实现。
