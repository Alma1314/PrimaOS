# COSIR 原语规范修复计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 修复 COSIR 原语规范中的 3 个语义问题、4 个文档一致性问题和 1 个架构风险记录

**Architecture:** 所有修改集中在单个文件 `COSIR 原语规范.md`，按修改区域分组为独立任务，每个任务修改一个区域且不依赖其他任务

**Tech Stack:** Markdown 文档编辑

## Global Constraints

- 所有修改在 `C:\Project\PrimaOS\COSIR 原语规范.md` 一个文件中
- 每个任务独立可验证（读取修改区域确认语义正确）
- 不引入新的规范词或记号
- 保持与现有术语和风格一致

---

### Task 1: 修复 `TaskExit` 触发路径不明确问题

**Files:**
- Modify: `C:\Project\PrimaOS\COSIR 原语规范.md` — §12.2 中 `TaskExit` 的说明段落

**Interfaces:**
- Consumes: 当前 `TaskExit` 定义（§12.2，约 1147 行附近）
- Produces: 明确的三段式触发路径说明

- [ ] **Step 1: 在 `TaskExit` 定义后增加触发路径说明**

在 §12.2 中，找到 `TaskExit` 的说明段落（当前为："`TaskExit` 不等同于 `task_destroy`..."），替换为以下内容：

**原文：**
```markdown
`TaskExit` 不等同于 `task_destroy`；它是 Task 执行终止（例如 `exit` 系统调用或致命异常无法恢复）产生的事件。COW fork 的子进程可以通过 Task 入口和 `TaskExit` 订阅实现 wait 语义。
```

**替换为：**
```markdown
`TaskExit` 不等同于 `task_destroy`；它是 Task 执行终止产生的事件。`TaskExit` 不是硬件直接触发的异常，而是软件行为经以下路径产生：

1. 用户态 Task 通过 `SyscallTrap` 发起终止请求（例如 `exit` 系统调用）；
2. 订阅 `syscall_trap_source` 的 LibOS 处理该请求，通过某个 *COSIR 细化层定义的操作*（当前抽象规范未覆盖）告知内核标记该 Task 为已终止；
3. 内核随后调用 `raise_exception(task_exit_source, TaskExit{exiting_task = t})` 通知所有订阅者。

如果 Task 遇到不可恢复的致命异常（如高特权级发生的 PageFault 或 DoubleFault），内核可以直接触发 `TaskExit` 而不通过 SyscallTrap 路径——这是 COSIR 细化层的实现选择。COW fork 的子进程可以通过订阅 `task_exit_source` 实现 wait 语义。
```

- [ ] **Step 2: 验证修改**

读取 `COSIR 原语规范.md` 中修改区域，确认 `TaskExit` 段落语义完整、与 Exception/SyscallTrap 的关系链路清晰。

- [ ] **Step 3: 同步更新 §12.9 页故障中 TaskExit 引用的潜在交叉引用**

检查 §12.9（第 12.9 节）中是否引用了 `TaskExit`——如果 §12.9 提到"若 T 未订阅该源，则行为由平台定义：COISR 可以实现 T 直接被销毁，或通知 `task_exit_source` 的所有订阅者"——确保此引用与 Task 1 的修改一致。此段落当前已正确引用 `task_exit_source`，无需修改。

---

### Task 2: 修复页故障后 Task 状态归属问题

**Files:**
- Modify: `C:\Project\PrimaOS\COSIR 原语规范.md` — §12.9 页故障处理期间的状态变化描述

**Interfaces:**
- Consumes: 当前 §12.9 的内容（约 1337-1363 行）
- Produces: 对 Task 在页故障后的状态归属的精确说明

- [ ] **Step 1: 替换"页故障处理期间的抽象状态变化"段落**

在 §12.9 中，找到"页故障处理期间的抽象状态变化"段落（以"**页故障处理期间的抽象状态变化：**"开头），将整个项目符号列表替换为：

**原文：**
```markdown
**页故障处理期间的抽象状态变化：**

- 如果 `T` 订阅了 `page_fault_source`，异常通知通过 `enqueue_or_deliver` 送达 `T` 的邮箱（或直接交付，如果 `T` 当前正在 `BlockedRecv`）；
- 如果 `T` 未订阅该源，则行为由平台定义：COISR 可以实现 `T` 直接被销毁，或通知 `task_exit_source` 的所有订阅者，或通过预配置的默认处理路径。**抽象状态机不强制具体行为**，因为这是一种不可恢复错误；
- `T` 在 `raise_exception` 完成时不再是 `Running`（因为指令流无法继续）。`T` 的状态可以建模为 `Ready`（异常通知已投递，`T` 可以通过 `recv` 获取并处理，然后通过某种恢复路径继续执行），或平台可将 `T` 直接终止；
```

**替换为：**
```markdown
**页故障处理期间的抽象状态变化：**

- 如果 `T` 订阅了 `page_fault_source`，异常通知通过 `enqueue_or_deliver` 送达 `T` 的邮箱（或直接交付，如果 `T` 当前正在 `BlockedRecv`）；
- 如果 `T` 未订阅该源，则行为由平台定义：COSIR 可以实现 `T` 直接被销毁，或通知 `task_exit_source` 的所有订阅者，或通过预配置的默认处理路径。**抽象状态机不强制具体行为**，因为这是一种不可恢复错误；
- `T` 在 `raise_exception` 完成时不再是 `Running`（因为指令流无法继续）。`T` 的状态被设置为 `Ready`，但 **COSIR 细化层必须保证**：当 `schedule` 选中 `T` 时，`T` 不会重新执行触发故障的指令，而是进入 COSIR 定义的"异常返回"路径（具体恢复机制见下文"恢复路径"段）。POSIR 抽象层将 faulted Task 建模为 `Ready` 是安全且充分的，因为：
  - `Ready` 状态的 Task 不可执行任何操作（只有 `Running` Task 可调用内核操作）；
  - `recv` 对 `Ready` Task 不可用（`recv` 要求 `Running`）；
  - `schedule` 选中一个 faulted `Ready` Task 的行为由细化层定义——细化层在恢复执行上下文后交付异常帧，而不是交付 Task 的原始指令流；
- 细化层可为 `TaskStatus` 增加 `Faulted` 等细化状态以管理异常帧和执行上下文保存，但这些状态不对 POSIR 层可见；
```

- [ ] **Step 2: 验证修改**

读取修改区域，确认：
- "POSIR 抽象层将 faulted Task 建模为 `Ready`"的理由充分
- 与 W9（调度状态一致）不冲突——W9 只约束 Running CPU Task 数量
- 与 §12.9 末尾的"恢复路径"段衔接自然

---

### Task 3: 修复 `InTransit` 超时回退的 double-holding 安全问题

**Files:**
- Modify: `C:\Project\PrimaOS\COSIR 原语规范.md` — §8.5 跨节点通信模型段落

**Interfaces:**
- Consumes: 当前 §8.5 的内容（约 700-755 行）
- Produces: `InTransit` 超时回退的安全条件说明

- [ ] **Step 1: 在 §8.5 "跨节点通信模型"的最后增加安全条件段落**

在 §8.5 "跨节点通信模型"段落之后、"分布式不变量"段落之前，插入以下新段落：

**在"4. **回退：** 若传输超时或网络不可达，Capability 从 `InTransit` 回退到 `HeldBy(t_s)`。"** 之后、**"`InTransit` 是 CapLocation 的第四种状态..."** 之前，将后一句扩展为：

**原文：**
```markdown
`InTransit` 是 CapLocation 的第四种状态。在单机同步通信中不存在可观察的 `InTransit`——所有 Capability 移动是即时的。只有在网络通信中，中间状态才在抽象层面暴露。
```

**替换为：**
```markdown
`InTransit` 是 CapLocation 的第四种状态。在单机同步通信中不存在可观察的 `InTransit`——所有 Capability 移动是即时的。只有在网络通信中，中间状态才在抽象层面暴露。

**超时回退的安全条件：** `InTransit` 从 `InTransit(...)` 回退到 `HeldBy(t_s)` 仅在以下条件成立时合法：

- 源节点已确认目标节点**未**接收该 Capability（目标节点未发送接收确认，或确认在网络分区后无法到达源节点）；
- 换言之，源节点必须遵守一个分布式安全协议：*回退和确认接收不能同时成功*。这要求目标节点在将 Capability 从 `InTransit` 写入本地 `HeldBy(t_r)` 或 `InMessage(msg)` 之前，必须先向源节点发送并等待确认回执。如果源节点在超时窗口内未收到此回执，则可以安全回退。

如果平台无法满足此分布式安全协议（例如使用了无状态传输协议），则 `send` 在跨节点场景中必须退化为**阻塞同步**——源节点阻塞直到目标节点确认接收——此时不存在可观察的 `InTransit` 中间状态。实现选择（异步+共识 vs 阻塞同步）取决于 COSIR 细化层对网络传输的抽象，但两种选择都必须保证 W3（Capability 单一位置）。

违背此安全条件将导致同一个 `CapID` 同时出现在源节点和目标节点——直接违反 W3。
```

- [ ] **Step 2: 验证修改**

读取修改区域，确认：
- 安全条件的表述与 W3（Capability 单一位置）保持一致
- 两种实现选择（异步+共识 vs 阻塞同步）提供了操作指导而不限制细化层

---

### Task 4: `enqueue_or_deliver` 中 Task 状态转换显式化

**Files:**
- Modify: `C:\Project\PrimaOS\COSIR 原语规范.md` — §6.3 `enqueue_or_deliver` 的步骤 3

**Interfaces:**
- Consumes: 当前第 6.3 节第 3 步文本
- Produces: 显式的状态转换描述

- [ ] **Step 1: 修改第 6.3 节第 3 步**

找到 `enqueue_or_deliver` 的第 3 步（当前为"若接收 Task 是 `BlockedRecv`：..."），修改状态转换的描述：

**原文：**
```markdown
3. 若接收 Task 是 `BlockedRecv`：
   - 将每个 Capability 原子地从 `HeldBy(sender)` 直接移到 `HeldBy(receiver)`；
   - 把 `RecvResult` 写入 `pending_return`；
   - 将 Task 设为 `Ready`；
   - 追加 `MessageReceived`，此处是该阻塞 `recv` 的线性化点；
   - 消息不进入活动 `messages` 表；
```

**替换为：**
```markdown
3. 若接收 Task 是 `BlockedRecv`：
   - 将每个 Capability 原子地从 `HeldBy(sender)` 直接移到 `HeldBy(receiver)`；
   - 把 `RecvResult` 写入 `pending_return`；
   - 将接收 Task 的状态从 `BlockedRecv` 原子地改为 `Ready`；（此状态转换与 `pending_return` 写入在同一原子步骤中完成；外部观察者看不到 `pending_return` 非空而状态仍为 `BlockedRecv` 的中间状态）
   - 追加 `MessageReceived`，此处是该阻塞 `recv` 的线性化点；
   - 消息不进入活动 `messages` 表；
```

- [ ] **Step 2: 验证修改**

确认修改后的描述满足：
- 显式写出了从 `BlockedRecv` 到 `Ready` 的状态转换
- 强调了原子性（"同一原子步骤中完成"）
- 与 W9 的要求一致（`pending_return ≠ None` 的 Task 必须为 `Ready`）

---

### Task 5: Stale token 规则覆盖 `pending_return`

**Files:**
- Modify: `C:\Project\PrimaOS\COSIR 原语规范.md` — §4.3 stale token 段落

**Interfaces:**
- Consumes: 当前第 4.3 节末尾的 stale token 规则（约 191-201 行）
- Produces: 扩展的 stale token 规则，覆盖 `pending_return` 路径

- [ ] **Step 1: 在 stale token 规则中增加 `pending_return` 条目**

在 §4.3 的 stale token 规则列表中，在现有条目之后增加一条：

**原文（stale token 规则列表末尾）：**
```markdown
- `InTransit` 的 token 在目标节点确认接收时成为一个 stale token（若目标对象在传输期间被销毁）。
```

**替换为：**
```markdown
- `InTransit` 的 token 在目标节点确认接收时成为一个 stale token（若目标对象在传输期间被销毁）；
- `pending_return` 中的 token 在 `schedule` 恢复 Task 后由用户态取走时，若目标对象在发送后被销毁，则成为 stale token——接收方收到的是原来的 token 身份，但不能用于授权。这与"消息和 token 整体到达"一致：`pending_return` 是直接交付的等价物，stale token 规则对直接交付路径同样适用。
```

- [ ] **Step 2: 验证修改**

确认：
- 新增条目在列表中的位置合理（在 `InTransit` 条目之后）
- 语义准确（`pending_return` 是直接交付的结果，stale token 规则应统一适用）
- 与 §9.2 `recv` 中已有的 stale token 说明（"已经进入消息的 Capability 可能因目标对象在发送后被销毁而变成 stale token"）呼应

---

### Task 6: I/O Port per-Task 授权粒度架构警告

**Files:**
- Modify: `C:\Project\PrimaOS\COSIR 原语规范.md` — §15.5 I/O Port 段落末尾

**Interfaces:**
- Consumes: 当前 §15.5 内容（约 1539-1556 行）
- Produces: 架构警告段落

- [ ] **Step 1: 在 §15.5 末尾增加架构警告**

在 §15.5 最后一段（"I/O port 授权在 Task 切换时..."）之后，增加警告：

**在现有段落之后追加：**
```markdown

**架构警告：** 在 x86_64 细化中，IOPL（I/O Privilege Level）和 TSS（Task State Segment）I/O bitmap 的授权粒度是 **per-Task** 而非 **per-port**。持有任一 `IOPortRange` Capability 的 Task 在获得 I/O 特权（IOPL=3 或 bitmap 清零对应位）后，硬件层面可以访问其 bitmap 覆盖范围内的所有端口。COSIR 细化层可以将 bitmap 精确配置到每个 Task 实际持有 Capability 的端口范围，确保"超出授权范围的端口指令触发 #GP 异常"。但这一精确性取决于 bitmap 的每端口位控制机制——如果平台在单个 bitmap 中覆盖了 Task 未授权的端口范围，则 Task 在硬件层面可能获得超出 Capability 授权的端口访问能力。

抽象状态机的安全声明（§17.8）依赖 COSIR 细化层正确地实现了 per-port bitmap 隔离。具体实现必须确保每个 Task 的 TSS I/O bitmap 或等效机制严格限制到仅其持有 `IOPortRange` Capability 覆盖的端口范围。
```

- [ ] **Step 2: 验证修改**

确认架构警告与 §17.8 设备隔离声明一致，且给出了细化要求。

---

### Task 7: `MessageOrigin` 对已销毁 Task 的显式说明

**Files:**
- Modify: `C:\Project\PrimaOS\COSIR 原语规范.md` — §5.3 Message 段落

**Interfaces:**
- Consumes: 当前 §5.3 内容（约 370-392 行）
- Produces: 对 `FromTask(t)` 引用已销毁 Task 的显式声明

- [ ] **Step 1: 在 §5.3 Message 定义后增加引用声明**

在 `MessageOrigin` 和 `Message` 的定义之后、`RecvResult` 之前，增加说明：

**在"`MessageOrigin` 只能由内核填写。普通 Task 无法伪造 `FromKernel`，也无法指定其他 Task 作为来源。"** 之后增加：

```markdown

`FromTask(t)` 中的 `t` 若在消息发送后被销毁，消息及其 `origin` 字段不受影响——接收方收到的 `FromTask(t)` 可能引用一个已不存在的 Task ID。这是务实的设计：销毁 Task 时不回溯扫描所有消息队列修改 `origin`。接收方可以通过 Audit 查询 `t` 的创建和销毁事件以判断该 Task 是否仍然存活，或直接尝试向 `t` 发送回复消息（此时操作返回 `InvalidTask`）。该行为已在全局不变量 W10 中显式允许。
```

- [ ] **Step 2: 验证修改**

确认：
- 新增说明与 W10 中的"`FromTask(t)` 可以引用已销毁 Task，但必须存在该 Task 的创建事件"一致
- 给出了接收方辨别已销毁 Task 的实用方法

---

## 验证清单

所有任务完成后，进行一次整体验证：

- [ ] Task 1: `TaskExit` 触发路径明确，三段式路径覆盖用户态 exit 和致命异常两个场景
- [ ] Task 2: 页故障后 Task 状态为 `Ready`，细化层负责恢复语义，与 W9 一致
- [ ] Task 3: `InTransit` 超时回退安全条件完整，覆盖异步+共识和阻塞同步两种选择
- [ ] Task 4: `enqueue_or_deliver` 显式写出 `BlockedRecv → Ready` 状态转换，强调原子性
- [ ] Task 5: Stale token 规则覆盖 `pending_return` 路径，与直接交付路径一致
- [ ] Task 6: I/O port per-Task 授权粒度警告，与 §17.8 设备隔离声明一致
- [ ] Task 7: `MessageOrigin` 对已销毁 Task 的显式说明，与 W10 一致
- [ ] 全文通读检查：各修改之间无矛盾，编号和交叉引用无断裂
