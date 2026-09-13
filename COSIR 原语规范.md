
> 状态：抽象语义规范
>
> 修订日期：2026-08-26

本文按 **11 类 Bootstrap 后常规机制 + 1 个 Bootstrap 初始化迁移** 统计接口类别。§19 总览因此列出 12 项；其中 Bootstrap 是初始 Authority 的唯一来源，不是系统引导完成后的常规可调用操作。H1 中的“11 类”仅指 Bootstrap 后常规机制，不包含该唯一初始化迁移。

## 1. 文档定位

本文定义 Prima 内核面向形式化验证的**平台无关抽象状态机**。文件名沿用《COSIR 原语规范》，但按 Prima 的术语划分：
- 本文描述的是 HexArc / POSIR 层的抽象语义；
- COSIR 是该抽象语义在具体硬件上的实例化与细化；
- 页表、寄存器、中断入口、驱动和具体 ABI 不属于本文状态机。
Capability 的派生、丢弃以及对象生命周期操作可以作为独立操作存在；可信计算基应以语义最小、边界清晰和可证明为准，而不是以操作数量为准。

本文使用以下规范词：

- **必须**：任何符合实现都要满足；
- **不得**：任何符合实现都不能出现；
- **可以**：实现可自由选择，但不得破坏不变量；
- **原子地**：外部观察不到中间状态；操作要么全部成功，要么不改变抽象状态。

## 2. 验证目标与非目标

### 2.1 第一阶段目标

状态机应足以建立以下性质：

1. Capability 不可由用户态伪造；
2. 派生 Capability 的权限不会提升；
3. 每个现存 Capability token 只有一个位置；
4. Capability 的创建、派生和移动具有完整溯源；
5. 未经授权的 Task 不能修改其他 Task 的地址空间或生命周期；
6. 跨 Task 的消息传递、共享内存建立和 Capability 获取都有显式授权路径；
7. 对象、引用、消息、映射、Timer 和 Namespace 的生命周期保持一致；
8. 每个失败操作保持抽象状态不变。

### 2.2 明确不在本规范中证明

- 调度公平性、无饥饿、终止性和实时性；
- 缓存、分支预测、总线竞争、页故障时延等隐蔽信道；
- CPU、IOMMU、MMU 或 DMA 是否正确执行映射与权限；
- 物理内存中字节的具体内容及程序读写行为；
- SMP 并发、弱内存模型和锁实现；
- 资源配额、审计日志容量、Capability 数量和消息队列的全局上限；
- Rust 实现对本状态机的细化证明；
- 选择性撤销某棵 Capability 派生子树。

因此，本文建立的是**显式授权安全**，不是包含时序与资源隐蔽信道的完全非干扰。

## 3. 基础域与记号

所有 ID 都是内核分配的 opaque 自然数。每类 ID 单调递增且永久不复用：

```text
SegId, TaskId, NsId, TimerId, CapId, MsgId, EventId : Nat
FrameId, VirtPage, PageOffset, VirtAddr, Instant     : Nat
Byte                                                   = Fin 256
Bytes                                                  = List<Byte>
```

这些名称在数学上都可由自然数实现，但在规范中是互不混用的类型。固定常量 `PAGE_SIZE > 0`、`MAX_PATH_COMPONENTS > 0`、`MAX_MSG_SIZE ≥ 0` 和 `MAX_CAPS_PER_MSG ≥ 0`。这些常量的具体值由 COSIR 细化层根据目标平台的缓存行大小、内核内存预算和性能目标确定；本抽象规范只要求它们为正（或非负）。定义：

```text
page_of(addr)   = addr / PAGE_SIZE
offset_of(addr) = addr % PAGE_SIZE
```

`VirtPage` 参数均表示页号而不是字节地址；`VirtAddr` 仅用于入口等需要页内偏移的值。

对象引用是带类型且开放的和类型，不能把一种 ID 解释成另一种对象。本文在后续各节中逐步扩展此类型——此处列出的是在第 4 节 Capability 讨论之前已引入的对象；ExcSource 和 Device 在各自定义处加入。完整定义见各对象类型的定义处汇总：

```text
ObjectRef = Segment(SegId)
          | Task(TaskId)
          | Namespace(NsId)
          | Timer(TimerId)
          | ExcSource(ExcSourceId)   // 见 §12.2
          | Device(DevId)            // 见 §15.2
```

本文使用：

- `x ∈ dom(m)`：有限映射 `m` 中存在键 `x`；
- `m[x]`：键 `x` 对应的值；
- `A ⊆ B`：集合包含；
- `[base, base + count)`：半开区间；
- `s —[actor, op/result]→ s'`：一次原子状态迁移；
- `live(s, obj)`：对象 `obj` 当前存在于相应活动对象表中。

抽象模型使用任意精度自然数。具体机器上的地址加法、页数乘法和 ID 计数器溢出，必须在实现细化层显式检查，不能依赖回绕语义。

### 3.1 Path

路径在进入内核状态机前先解析成分量列表：

```text
Component = 非空 ASCII 字符串，长度 ≤ 255 字节，且不含 '/'
Path      = List<Component>
```

约束：

- `.` 和 `..` 不能作为分量；
- 外部 Namespace 操作的 `Path` 必须非空；
- 不存在空分量，因此原始文本不得包含 `//` 或首尾 `/`；
- 分量数不得超过 `MAX_PATH_COMPONENTS`；
- Namespace 内部使用规范化后的 `Path`，不再重复处理字符串语法。

空列表只在规范内部表示 Namespace 根。

## 4. 权限与 Capability 核心

### 4.1 权限

```text
Right = Derive

      | SegMapR | SegMapW | SegMapX | SegDestroy

      | TaskSend | TaskManageVSpace | TaskDestroy

      | NsResolve | NsBind | NsUnbind
      | NsMount | NsUnmount | NsDestroy

      | TimerCancel | TimerDestroy

      | ExcSubscribe | ExcSourceDestroy

      | NodeConnect | NodeDestroy
```

`well_typed_rights(target, rights)` 要求权限与目标对象类型匹配；`Derive` 可以用于任何对象类型，其他权限只能用于对应对象：

| 对象类型 | 合法非 Derive 权限 |
|---|---|
| Segment | `SegMapR`, `SegMapW`, `SegMapX`, `SegDestroy` |
| Task | `TaskSend`, `TaskManageVSpace`, `TaskDestroy` |
| Namespace | `NsResolve`, `NsBind`, `NsUnbind`, `NsMount`, `NsUnmount`, `NsDestroy` |
| Timer | `TimerCancel`, `TimerDestroy` |
| ExcSource | `ExcSubscribe`, `ExcSourceDestroy` |
| Node | `NodeConnect`, `NodeDestroy` |

`Derive` 表示可以从该 Capability 派生指向同一对象、权限不更高的新 Capability。它不表示可以修改既有 Capability，也不表示可以撤销已经派生的子 Capability。

### 4.2 Capability 描述

```text
CapRecord {
    target:     ObjectRef,
    rights:     FiniteSet<Right>,
    parent:     Option<CapId>,
    created_at: EventId,
}
```

`CapRecord` 一经创建永久不可修改。`caps: CapId ⇀ CapRecord` 是追加式历史表；丢弃 Capability 或销毁目标对象都不会删除记录。

- `parent = None`：该 Capability 是随对象创建或 Bootstrap 铸造的根 Capability；
- `parent = Some(p)`：该 Capability 由 `p` 派生；
- 子 Capability 必须与父 Capability 指向同一对象；
- 子权限必须是父权限的子集；
- 父 `CapId` 必须小于子 `CapId`，因此派生关系天然无环。

### 4.3 Capability 位置

```text
CapLocation = HeldBy(TaskId)
            | InNamespace(NsId, Path)
            | InMessage(MsgId)
            | InTransit(from: CapLocation, to: CapLocation, started_at: Instant)
```

```text
locations: CapId ⇀ CapLocation
```

`locations` 是函数，所以一个 `CapId` 在任意时刻至多有一个位置。共享 Authority 时不得复制同一个 `CapId`；必须派生一个新的子 Capability。

`InTransit` 表示 Capability 正在跨节点传输——源节点已从本地位置移除，但目标节点尚未确认接收。在传输期间 token 不能用于授权。传输完成时 `InTransit` 被替换为目标位置；传输超时或失败时回退到源位置。该状态仅在多节点分布式场景中出现；单机场景下所有 Capability 移动是同步的（即不存在可观察的 `InTransit` 中间状态）。

区分以下概念：

```text
present(s, c) := c ∈ dom(s.locations)

held(s, t, c) := s.locations[c] = HeldBy(t)

usable(s, c) :=
    present(s, c)
    ∧ c ∈ dom(s.caps)
    ∧ live(s, s.caps[c].target)
    ∧ ¬ (s.locations[c] instanceof InTransit)   // 传输中的 token 不可用
```

`InTransit` 的 token 不算 `usable`——正在传输不能授权新操作。`InTransit` 不影响 `present`（因为位置仍然被记录，只是类型不同），也不能被 `cap_drop`（必须等传输完成后才能丢弃）。

目标对象销毁后，指向它的 token 可以继续存在于 Task、Namespace 或 Message 中，但它是 **stale token**：

- 可被持有者 `cap_drop`；
- 可被持有者查询 Audit；
- 可由 `ns_unbind` 取回或由 `ns_unmount` 清理；
- 已经在途的 stale token 仍会随原消息交付；
- 不得用于授权、派生、新发送或新绑定；
- `InTransit` 的 token 在目标节点确认接收时成为一个 stale token（若目标对象在传输期间被销毁）；
- `pending_return` 中的 token 在 `schedule` 恢复 Task 后由用户态取走时，若目标对象在发送后被销毁，则成为 stale token——接收方收到的是原来的 token 身份，但不能用于授权。这与"消息和 token 整体到达"一致：`pending_return` 是直接交付的等价物，stale token 规则对直接交付路径同样适用。

这一规则保留消息内容与审计历史，同时避免对象销毁必须扫描并重写所有 Capability 容器。

### 4.4 `cap_derive`

```text
cap_derive(parent, requested_rights) -> CapId
```

前置条件：

1. 调用者是当前 Running Task；
2. `parent` 由调用者持有且 `usable`；
3. 父 Capability 包含 `Derive`；
4. `requested_rights ⊆ parent.rights`；
5. 请求权限对目标对象类型合法。

成功动作：

1. 分配全新 `CapId`；
2. 创建不可变 `CapRecord`，`parent = Some(parent)`；
3. 将新 Capability 放在 `HeldBy(caller)`；
4. 追加 `CapDerived` 审计事件。

父 Capability 保持不变。请求空权限集合是合法的，但该 token 只能用于身份关联、Audit 或丢弃。

错误包括：`InvalidCap`、`NotHeld`、`TargetDead`、`MissingRight(Derive)`、`RightsExceeded`、`WrongRightType`。

### 4.5 `cap_drop`

```text
cap_drop(cap) -> ()
```

前置条件：

- `cap` 当前位于 `HeldBy(caller)`；
- `cap` 不是任何活动 Task 的结构性根 Namespace Capability。

成功动作：删除 `locations[cap]` 并追加 `CapDropped`。`CapRecord` 及其子 Capability 均不受影响。

`cap_drop` 不要求目标对象仍然存活，因此可用于清理 stale token。

错误：`InvalidCap`（`cap` 不是有效 CapId）、`NotHeld`（不在 `HeldBy(caller)`）、`StructuralCap`（是活动 Task 的结构性根 Namespace Capability）。目标对象已销毁不构成失败，stale token 仍可成功丢弃。

### 4.6 Capability 移动

Capability 没有用户可直接调用的任意 `move`。位置变化只发生在规范列出的原子操作内：

- 排队 `send`：`HeldBy(sender) → InMessage(msg)`；
- 直接满足阻塞 `recv` 的 `send`：`HeldBy(sender) → HeldBy(receiver)`，不存在可观察的中间位置；
- 从邮箱 `recv`：`InMessage(msg) → HeldBy(receiver)`；
- `task_create` 注入：`HeldBy(creator) → HeldBy(new_task)`；
- `ns_bind`：`HeldBy(caller) → InNamespace(ns, path)`；
- `ns_unbind`：`InNamespace(ns, path) → HeldBy(caller)`；
- `ns_mount`：派生后直接置于 `InNamespace(ns, path)`；
- `cap_drop`、`task_destroy`、`ns_unmount`、容器销毁或消息丢弃：删除位置；
- 跨节点 `send`：`HeldBy(sender)` → `InTransit(from=HeldBy(sender), to=HeldBy(receiver), ...)` → （接收确认后）`HeldBy(receiver)`；
- 跨节点 `send` 超时：`InTransit(...)` → `HeldBy(sender)`（回退到源位置）。

每次位置变化必须记录原位置和新位置；删除位置时新位置为 `None`。直接交付必须记录关联的 `MsgId`。移动不改变 `CapRecord`，且一个成功迁移中不得出现 Capability 暂时没有位置的可观察中间状态——`InTransit` 本身就是一个位置，不是"没有位置"。

Capability 移动不是用户可直接调用的操作，因此没有独立的成功/失败语义；其失败语义继承自触发它的外层操作（如 `send`、`recv`、`task_create`、`ns_*` 的错误清单），外层操作失败时移动不发生。

### 4.7 撤销边界

第一版不提供 `cap_revoke`：

- 丢弃父 Capability 不影响已经派生的子 Capability；
- 删除 Namespace 绑定不影响已经通过该绑定派生出去的 Capability；
- 只有销毁目标对象会令所有指向该对象的 Capability 同时失去授权能力。

如果未来加入选择性撤销，必须单独定义撤销域、并发语义、在途消息处理和派生树一致性，不能把它隐含在 `cap_drop` 中。

> **撤销域（2026-08）：** 探索阶段已在 Bootstrap 宿主实现中先行落地选择性撤销（子树传播，见实施证据 SPEC-022/023），其撤销域定义见 [[Prima OS]] §6.2 实现注记；本节保持第一版不提供 `cap_revoke` 的基线，不将撤销隐含进 `cap_drop` 或新增一等原语。在途（`InMessage`/`InNamespace`/`InTransit`）位置仍不纳入撤销域，留待后续阶段。

## 5. 内核状态

```text
KernelState {
    segments:     SegId       ⇀ Segment,
    tasks:        TaskId      ⇀ Task,
    namespaces:   NsId        ⇀ Namespace,
    timers:       TimerId     ⇀ Timer,
    exc_sources:  ExcSourceId ⇀ ExceptionSource,
    devices:      DevId       ⇀ Device,
    nodes:        NodeId      ⇀ Node,

    caps:       CapId   ⇀ CapRecord,       // 追加式
    locations:  CapId   ⇀ CapLocation,
    messages:   MsgId   ⇀ Message,         // 仅活动消息
    audit:      List<AuditEvent>,          // 追加式

    frame_universe: FiniteSet<FrameId>,    // 启动后不变（CPU 可寻址内存）
    free_frames:    FiniteSet<FrameId>,

    gpu_frame_universe: Map<GPUArch, FiniteSet<FrameId>>,  // 每个 GPU 架构的显存池
    gpu_free_frames:    Map<GPUArch, FiniteSet<FrameId>>,

    now:        Instant,
    booted:     Bool,

    next_seg:       SegId,
    next_task:      TaskId,
    next_ns:        NsId,
    next_timer:     TimerId,
    next_exc_src:   ExcSourceId,
    next_dev:       DevId,
    next_node:      NodeId,
    next_cap:       CapId,
    next_msg:       MsgId,
    next_event:     EventId,
}
```

活动对象表只包含存活对象。对象销毁时从活动表删除；相应 ID 永不复用，历史由 Audit 和 `caps` 保留。

### 5.1 Segment

```text
Access = FiniteSet<{R, W, X}>

Segment {
    creator:       Actor,
    frames:        Vector<FrameId>,
    allowed_access: Access,
}
```

`frames` 非空、无重复。Segment 的页数等于 `frames.length`。

### 5.2 Task 与虚拟地址空间

```text
PageMapping {
    segment: SegId,
    offset:  PageOffset,
    access:  Access,
}

TaskStatus = Ready | Running | BlockedRecv

ComputeMedium = CPU
              | GPU(GPUArch)
              | NPU(NPUArch)
              | FPGA(FPGAVendor)

GPUArch  = Vulkan | CUDA | Metal | ROCm
NPUArch  = CoreML | OpenVINO | QualcommHexagon
FPGAVendor = Xilinx | IntelFPGA

TaskKind = ComputeTask(ComputeMedium)

Task {
    creator:       Actor,
    kind:          TaskKind,
    status:        TaskStatus,
    vspace:        VirtPage ⇀ PageMapping,       // CPU Task：MMU 页表；非 CPU：空（GPU/NPU 用显存池）
    entry:         VirtAddr,                       // CPU Task：指令地址；非 CPU：kernel 句柄，语义由 COSIR 细化
    root_ns_cap:   CapId,
    mailbox:       List<MsgId>,
    pending_return: Option<RecvResult>,
}
```

CPU Task 拥有完整的 `vspace` 映射，`entry` 是虚拟地址。GPU/NPU/FPGA Task 的 `vspace` 为空，`entry` 在细化层解释为特定计算介质的 kernel 句柄（如 CUDA kernel 指针或 OpenCL kernel 名称）。所有介质上的 Task 共享同一个 Capability 位置空间——GPU Task 通过 `task_create` 注入的 `initial_caps` 获得对显存 Segment 的访问权。

**SMP 扩展。** 当前 W9 要求"至多一个 Task 为 Running"。在多核系统中，此约束放松为：

```text
|{t ∈ dom(tasks) | tasks[t].status = Running ∧ tasks[t].kind = ComputeTask(CPU)}| ≤ cpu_core_count
```

GPU/NPU Task 的 `Running` 语义由 COSIR 细化定义——CUDA stream 上的 kernel 提交视为 `Running`，直到完成通知返回。

**物理内存池。** `frame_universe` 和 `free_frames` 仅管理 CPU 可寻址的通用内存页。GPU/NPU 显存通过 `gpu_frame_universe` 和 `gpu_free_frames` 管理，按 `GPUArch` 区分不同设备的独立显存池。所有隔离不变量（W6 物理页唯一归属、W7 虚拟映射有效）对 CPU 池和 GPU 池分别适用。

`pending_return` 表示阻塞的 `recv` 已由内核完成、但 Task 尚未重新进入用户态领取的结果。

### 5.3 Message

```text
KernelEvent  = TimerFired(TimerId)
MessageOrigin = FromTask(TaskId)
              | FromKernel(KernelEvent)

Message {
    origin: MessageOrigin,
    to:     TaskId,
    data:   Bytes,
    caps:   List<CapId>,
}

RecvResult {
    msg_id: MsgId,
    origin: MessageOrigin,
    data:   Bytes,
    caps:   List<CapId>,
}
```

`MessageOrigin` 只能由内核填写。普通 Task 无法伪造 `FromKernel`，也无法指定其他 Task 作为来源。

`FromTask(t)` 中的 `t` 若在消息发送后被销毁，消息及其 `origin` 字段不受影响——接收方收到的 `FromTask(t)` 可能引用一个已不存在的 Task ID。这是务实的设计：销毁 Task 时不回溯扫描所有消息队列修改 `origin`。接收方可以通过 Audit 查询 `t` 的创建和销毁事件以判断该 Task 是否仍然存活，或直接尝试向 `t` 发送回复消息（此时操作返回 `InvalidTask`）。该行为已在全局不变量 W10 中显式允许。

### 5.4 Namespace

```text
NsEntry = Bound(CapId)
        | Mount(CapId)   // 目标必须是 Namespace

Namespace {
    creator: Actor,
    entries: Path ⇀ NsEntry,
}
```

目录节点是隐式的：若存在 `a/b/c`，则 `a` 和 `a/b` 可作为遍历前缀，无需单独的 Directory 对象。

### 5.5 Timer

```text
CancelReason = ByTask(TaskId) | RecipientDestroyed

TimerState = Pending {
                 deadline:  Instant,
                 recipient: TaskId
             }
           | Fired {
                 deadline:     Instant,
                 recipient:    TaskId,
                 fired_at:     Instant,
                 notification: MsgId
             }
           | Cancelled {
                 deadline:  Instant,
                 recipient: TaskId,
                 reason:    CancelReason
             }

Timer {
    creator: TaskId,
    state:   TimerState,
}
```

Timer 是一次性的。`Fired` 和 `Cancelled` 都不会回到 `Pending`。

### 5.6 唯一预启动状态

形式化模型的唯一起点 `EmptyState` 定义为：

- 所有对象表（含 `nodes`）、`caps`、`locations`、`messages` 和 `audit` 为空；
- `frame_universe = free_frames = ∅`；
- `gpu_frame_universe = gpu_free_frames = empty map`；
- `now = 0`，`booted = false`；
- 所有 `next_* = 0`。

预启动状态没有用户迁移；它唯一可执行的成功迁移是 `bootstrap`。具体平台的单调时钟可以用固定偏移细化抽象的零起点。

## 6. 状态迁移通则

### 6.1 Actor

```text
Actor = TaskActor(TaskId) | KernelActor
```

- 用户可调用操作要求 `TaskActor(t)` 且 `tasks[t].status = Running`；
- `KernelActor` 只用于 `bootstrap`、调度、时间推进和 Timer 到期等内部迁移；
- 用户输入不能构造 `KernelActor`。

### 6.2 环境操作与原子验证

以下操作是任何 Running Task 都可使用的环境机制，不要求额外 Capability：`cap_drop`（仅限自己持有的 token）、`memseg_create`、`ns_create`、`timer_set`、`time_now`、`recv` 和 `task_yield`。它们只会创建由调用者掌握的新对象、观察公开内核状态、改变调用者自身状态或放弃调用者已有资源；其中创建操作会消耗全局 ID、内存或日志空间。资源可用性、配额、拒绝服务及由此产生的观察信道明确不属于当前安全结论。除此之外，影响既有对象或其他 Task 的用户操作必须通过 Capability 授权。

每项操作先验证所有输入，再一次性提交状态变化：

1. Actor 与启动状态；
2. ID、Capability 位置、目标类型和目标存活性；
3. 所需权限；
4. 参数范围、列表去重、路径和地址合法性；
5. 对象状态、冲突和资源可用性。

任何阶段失败：

```text
s —[actor, op/Error(e)]→ s
```

即 `segments`、`tasks`、`locations`、消息、ID 计数器和 Audit 均不得变化。拒绝事件日志可由具体实现另行维护，但不属于本抽象状态。

如果多个错误同时成立，抽象规范允许返回任一适用错误。需要稳定 ABI 的实现层必须另行固定错误优先级。

#### 6.2.1 默认失败语义（覆盖全部操作的兜底规则）

为保证每个操作都有明确的失败语义，规定：

- **显式错误清单优先：** 若操作小节给出“错误：”清单，该清单即其失败域；失败时按 §6.2 保持 `State` 不变。
- **默认规则兜底：** 若操作小节未给出“错误：”清单，则其失败语义为——失败当且仅当该操作列出的任一前置条件不满足；失败时返回 §6.2 第 1–5 阶段中适用阶段的标准错误标识，并保持 `State` 不变；实现可返回任一适用错误。
- **显式无失败分支：** 若操作小节声明“无失败分支”，则在 §6.1 的 Actor 约束成立且其前置条件由内核保证成立时，该操作不产生失败结果。

据此，本规范中每个操作都具有明确的成功语义（“成功动作”或返回值）与失败语义（显式清单、默认规则或“无失败分支”声明）。本规范新引入的错误标识 `InvalidTime`、`InvalidTimer`、`MountCycle`、`InvalidDevice`、`InvalidNode` 在首次出现的小节内就地定义。

### 6.3 内部辅助操作 `enqueue_or_deliver`

内核发送消息时统一执行。对于 Task 发起的 `send`，输入 Capability 在操作开始时都位于 `HeldBy(sender)`；内核事件当前不携带 Capability：

1. 分配 `MsgId` 并构造不可伪造的 `origin`；
2. 追加 `MessageSent`；这是发送动作的线性化点，并且总是先于同一消息的 `MessageReceived`；
3. 若接收 Task 是 `BlockedRecv`：
   - 将每个 Capability 原子地从 `HeldBy(sender)` 直接移到 `HeldBy(receiver)`；
   - 为每个被移动的 Capability 追加 `CapMoved` 事件，记录从 `HeldBy(sender)` 到 `HeldBy(receiver)` 的位置变化；
   - 把 `RecvResult` 写入 `pending_return`；
   - 将接收 Task 的状态从 `BlockedRecv` 原子地改为 `Ready`；（此状态转换与 `pending_return` 写入在同一原子步骤中完成；外部观察者看不到 `pending_return` 非空而状态仍为 `BlockedRecv` 的中间状态）
   - 追加 `MessageReceived`，此处是该阻塞 `recv` 的线性化点；
   - 消息不进入活动 `messages` 表；
4. 否则：
   - 把消息放入 `messages`；
   - 按 FIFO 追加 `MsgId` 到 `mailbox`；
   - 将每个 Capability 原子地从 `HeldBy(sender)` 移到 `InMessage(msg)`；
   - 为每个被移动的 Capability 追加 `CapMoved` 事件，记录从 `HeldBy(sender)` 到 `InMessage(msg)` 的位置变化；

直接交付和排队交付都消耗一个永久不复用的 `MsgId`，并使用相同格式的 `MessageSent`。

**失败语义：无失败分支。** `enqueue_or_deliver` 是 `KernelActor` 内部辅助，其输入（发送方/接收方位置、目标 Task 存活、非结构性 Capability）已由触发它的外层操作按 §6.2 预先验证；本步骤不引入新的失败域，也不产生用户可观察的错误返回。

## 7. MemorySegment 操作

### 7.1 `memseg_create`

```text
memseg_create(page_count, allowed_access) -> CapId
```

前置条件：

- `page_count > 0`；
- `allowed_access` 非空且只含 `R/W/X`；
- `free_frames` 至少包含 `page_count` 个页帧。

成功动作：

1. 从 `free_frames` 选择并移除恰好 `page_count` 个页帧；抽象状态把这些页视为已清零，不保留其前一归属的字节信息；
2. 创建 Segment；
3. 铸造一个根 Segment Capability，作为成功结果返回并由调用者持有；该根 Capability 不是创建前置条件；
4. 根权限为 `Derive | SegDestroy`，并按 `allowed_access` 加入 `SegMapR/W/X`；
5. 追加对象创建、页帧分配和 Capability 铸造事件。

错误：`ZeroSize`、`InvalidAccess`、`NoMemory`。

对象创建在本抽象模型中是 Running Task 的环境权限。资源配额或内存分配授权应在后续资源域模型中增加，不属于当前安全结论。具体实现必须在页帧首次对新 Segment 可见之前完成清零；仅把页帧加入空闲链表不满足该抽象语义。

### 7.2 `memseg_map`

```text
memseg_map(
    seg_cap,
    task_cap,
    seg_offset,
    target_base,
    page_count,
    access
) -> ()
```

前置条件：

1. `seg_cap` 由调用者持有、可用且指向 Segment；
2. 每项请求访问都得到相应 `SegMapR/W/X` 授权；
3. `access ⊆ segment.allowed_access` 且非空；
4. `task_cap` 授权目标 Task 的 `TaskManageVSpace`；
5. `page_count > 0`；
6. `[seg_offset, seg_offset + page_count)` 位于 Segment 内；
7. 目标 Task 的 `[target_base, target_base + page_count)` 全部未映射。

成功动作：对每一页 `i < page_count` 插入：

```text
task.vspace[target_base + i] = {
    segment = seg_cap.target,
    offset  = seg_offset + i,
    access  = access
}
```

该操作同时需要 Segment 的映射授权和目标 Task 的地址空间管理授权。`TaskSend` 不得替代 `TaskManageVSpace`。

错误：`InvalidSegment`、`InvalidTask`、`MissingMapRight`、`RightsExceeded`、`OutOfBounds`、`Overlap`、`ZeroSize`。

### 7.3 `memseg_unmap`

```text
memseg_unmap(task_cap, target_base, page_count) -> ()
```

前置条件：

- `task_cap` 授权 `TaskManageVSpace`；
- `page_count > 0`；
- 目标区间中的每一页当前都有映射；
- 目标区间不包含该 Task 的 `entry` 所在虚拟页。

成功动作：原子删除区间中的全部页映射。该操作不需要 Segment Capability，因为它只修改获得授权的目标地址空间，不销毁 Segment。

当前模型不记录指令指针或入口迁移，因此要求初始入口映射在 Task 整个生命周期内保持有效。将来若加入执行上下文状态，可用“当前指令指针仍可执行”等更精确条件替代此限制。

错误：`InvalidTask`、`MissingRight(TaskManageVSpace)`、`NotMapped`、`EntryMapping`、`ZeroSize`。

### 7.4 `memseg_destroy`

```text
memseg_destroy(seg_cap) -> ()
```

前置条件：

- `seg_cap` 授权 `SegDestroy`；
- 任何活动 Task 的 `vspace` 都没有引用目标 Segment。

成功动作：

1. 从 `segments` 删除目标；
2. 将其全部页帧归还 `free_frames`。归还的页帧内容仍保留前一归属的字节，但已不再被任何 `vspace` 映射——页帧内容只能通过后续 `memseg_create` 分配并映射后才对用户态可见，而 `memseg_create` 对新分配的页清零（§7.1）。因此归还页帧的残留数据不构成授权信息流；
3. 追加销毁事件。

所有指向该 Segment 的 Capability 记录与位置保留，但因目标不再存活而成为 stale token。

错误：`InvalidSegment`、`MissingRight(SegDestroy)`、`StillMapped`。

## 8. Task 与调度操作

### 8.1 `task_create`

```text
task_create(
    code_seg_cap : CapId,
    code_offset  : PageOffset,
    code_pages   : Nat,
    load_base    : VirtPage,
    entry        : VirtAddr,          // CPU Task：虚拟地址；非 CPU Task（GPU/NPU/FPGA）：由 COSIR 细化层解释为特定计算介质的 kernel 句柄
    parent_ns_cap: CapId,
    root_rights  : FiniteSet<Right>,
    initial_caps : List<CapId>,
    kind         : TaskKind           // 确定 Task 的计算介质类型；默认为 ComputeTask(CPU)
) -> manager_task_cap : CapId
```

前置条件：

1. 代码 Segment Capability 由调用者持有且授权 `SegMapR` 与 `SegMapX`，初始代码映射权限恰为 `{R, X}`；
2. `code_pages > 0` 且 `code_offset + code_pages ≤ segment.frames.length`；
3. `load_base ≤ page_of(entry) < load_base + code_pages`；
4. `parent_ns_cap` 由调用者持有、可用并指向 Namespace；
5. `parent_ns_cap` 包含 `NsResolve` 和 `Derive`；
6. `NsResolve ∈ root_rights ⊆ parent_ns_cap.rights`；
7. `initial_caps` 无重复、全部由调用者持有且可用；
8. `initial_caps` 不包含 `code_seg_cap` 或 `parent_ns_cap`，也不包含任何活动 Task 的结构性根 Namespace Capability。

成功动作是一个整体：

1. 创建空地址空间的新 Task；
2. 对每个 `i < code_pages` 建立 `vspace[load_base + i] = {segment = code_seg_cap.target, offset = code_offset + i, access = {R, X}}`；
3. 铸造新 Task 的根管理 Capability，权限恰为 `{Derive, TaskSend, TaskManageVSpace, TaskDestroy}`，放在 `HeldBy(caller)`；
4. 从管理 Capability 派生权限相同的 self Capability，放在 `HeldBy(new_task)`；
5. 调用者或新 Task 可在创建后另行派生权限衰减的 Task Capability；
6. 从 `parent_ns_cap` 派生 `root_rights` 子 Capability，放在 `HeldBy(new_task)` 并把该子 Capability 写入新 Task 的 `root_ns_cap` 字段；
7. 将 `initial_caps` 从调用者移动到新 Task；
8. 将新 Task 设为 `Ready`，邮箱为空，`pending_return = None`；
9. 返回调用者持有的管理 Capability。

新 Task 的根 Namespace 是共享对象，不做深拷贝。创建者、所有者和授权者是不同概念：`creator` 只用于审计，所有后续操作只看 Capability。

错误：`InvalidEntry`、`InvalidCodeRange`、`InvalidNamespace`、`InvalidInitialCap`、`DuplicateCap`、`MissingRight`、`RightsExceeded`。

### 8.2 `task_destroy`

```text
task_destroy(task_cap) -> ()
```

前置条件：`task_cap` 授权目标 Task 的 `TaskDestroy`。

成功动作：

1. 删除目标 Task 的全部页映射；
2. 删除其邮箱中的活动消息；
3. 删除这些消息内 Capability 的位置；
4. 删除该 Task 持有的全部 Capability 位置，包括根 Namespace Capability 和待返回结果中的 Capability；
5. 将所有以该 Task 为接收者的 `Pending` Timer 变为 `Cancelled(RecipientDestroyed)`；
6. 从 `tasks` 删除目标 Task；
7. 追加 Task 销毁、消息丢弃、Capability 丢弃和 Timer 取消事件。

下列对象不会被隐式销毁：Segment、Namespace、其他 Task，以及已经发送到其他 Task 邮箱中的消息和 Capability。

删除自身是合法的；成功后没有 Running Task，内核必须通过后续 `schedule` 选择新的 Task。

指向被销毁 Task 的 Capability 变为 stale token。

错误：`InvalidTask`（`task_cap` 不指向有效 Task）、`NotHeld`（不在 `HeldBy(caller)`）、`MissingRight(TaskDestroy)`。

### 8.3 `task_yield`

```text
task_yield() -> ()
```

当前 Task 从 `Running` 变为 `Ready`。它不是 Capability 操作。`task_yield` 成功后当前没有 Running Task，因此必须在返回用户态之前通过 `schedule` 选择新 Task——延迟到下一次定时器中断才调度会令 CPU 在 `Ready` Task 存在时空闲，不符合 W9 的调度状态一致性。

**失败语义：无失败分支。** 调用者由 §6.1 保证是 `Running` Task；本操作不检查 Capability、不接受参数、不访问其它对象，因此不存在失败结果。

### 8.4 `schedule`

```text
schedule(chosen_task) -> Option<RecvResult>
```

这是 `KernelActor` 内部迁移。前置条件：

- 当前没有 `Running` Task；
- `chosen_task` 当前为 `Ready`。

成功动作：

1. 把 `chosen_task` 设为 `Running`；
2. 将其 `pending_return` 作为恢复结果交给用户上下文并清空该字段。

选择可以是非确定的；本规范不要求公平。任意时刻至多有一个 Running Task。

**失败语义：无失败分支。** `schedule` 是 `KernelActor` 内部迁移；其两条前置条件（当前无 Running Task、`chosen_task` 为 `Ready`）由调度器本身维护，用户输入不能触发或绕过，故不产生可返回的错误。

### 8.5 跨节点与异地 Task

多机集群中，每个节点运行独立的 HexArc 内核实例，拥有本地 `KernelState`。跨节点通信通过扩展的 `send` 原语完成。

**节点对象：**

```text
NodeId : Nat

Node {
    creator:  Actor,
    transport: NodeTransport,
    status:   NodeStatus,
}

NodeTransport = Direct(NodeId)
              | Mesh(FiniteSet<NodeId>)
              | Forwarded(NodeId)

NodeStatus = Connected | Disconnected | PartiallyConnected
```

每个 Task 属于一个 Node：

```text
Task {
    ...
    node: NodeId,
    ...
}
```

**节点创建：**

```text
node_create(transport) → node_cap
```

铸造完整根 Node Capability，权限为 `{Derive, NodeConnect, NodeDestroy}`。`NodeConnect` 授权在与该节点建立传输连接时使用此 Capability；`NodeDestroy` 授权销毁该 Node 对象。

**失败语义：无失败分支。** 除 §6.1 的 Actor 约束外本操作无前置条件；`transport` 是内核提供的传输实现，其可用性属资源问题（§6.2 排除），不作为抽象层失败。

```text
node_destroy(node_cap) -> ()
```

前置条件：`node_cap` 授权 `NodeDestroy`。成功时从活动 Node 表删除目标，并关闭所有通过此 Node 建立的连接。任何使用此 Node 的 `InTransit` Capability 在目标节点不可达时回退到源位置，否则阻塞至传输完成。销毁后的 Node Capability 变为 stale token。

错误：`InvalidNode`（`node_cap` 未指向有效 Node；本规范中该标识统一表示“Node Capability 参数无效”）、`NotHeld`、`MissingRight(NodeDestroy)`。

Node 对象的权限集合纳入 §4.1 的权限表：

| 对象类型 | 合法非 Derive 权限 |
|---|---|
| Node | `NodeConnect`, `NodeDestroy` |

Node 对象的 `KernelState` 字段：`nodes: NodeId ⇀ Node`，`next_node: NodeId`，初始状态均为空/0。

**跨节点通信模型：**

当发送方 Task `t_s` 和接收方 Task `t_r` 位于不同 Node 时，`send` 扩展为异步两阶段协议：

1. **验证阶段（本地）：** 发送方节点验证 Capability 和权限——与单机 `send` 完全相同；
2. **InTransit 阶段：** 源节点标记 Capability 为 `InTransit`，将消息序列化并通过 `NodeTransport` 发送到目标节点。跨节点传输使用"携带源节点 ID + 本地 CapId 的全局引用"格式；目标节点在接收时创建本地代理 CapRecord，其 `parent` 指向传输操作本身的审计事件（记录源节点 ID 和源 CapId），从而实现跨节点的完整溯源链；
3. **确认阶段（远端）：** 目标节点验证后可接受消息。若接收方当前 `BlockedRecv`，直接交付 Capability；否则进入消息队列；
4. **回退：** 若传输超时或网络不可达，Capability 从 `InTransit` 回退到 `HeldBy(t_s)`。

`InTransit` 是 CapLocation 的第四种状态。在单机同步通信中不存在可观察的 `InTransit`——所有 Capability 移动是即时的。只有在网络通信中，中间状态才在抽象层面暴露。

**超时回退的安全条件：** `InTransit` 从 `InTransit(...)` 回退到 `HeldBy(t_s)` 仅在以下条件成立时合法：

- 源节点已确认目标节点**未**接收该 Capability（目标节点未发送接收确认，或确认在网络分区后无法到达源节点）；
- 换言之，源节点必须遵守一个分布式安全协议：*回退和确认接收不能同时成功*。这要求目标节点在将 Capability 从 `InTransit` 写入本地 `HeldBy(t_r)` 或 `InMessage(msg)` 之前，必须先向源节点发送确认回执。如果源节点在超时窗口内未收到此回执，则可以安全回退。

如果平台无法满足此分布式安全协议（例如使用了无状态传输协议），则 `send` 在跨节点场景中必须退化为**阻塞同步**——源节点阻塞直到目标节点确认接收——此时不存在可观察的 `InTransit` 中间状态。实现选择（异步+共识 vs 阻塞同步）取决于 COSIR 细化层对网络传输的抽象，但两种选择都必须保证 W3（Capability 单一位置）。

违背此安全条件将导致同一个 `CapId` 同时出现在源节点和目标节点——直接违反 W3。

**分布式不变量：**

- **W3 扩展：** 每个 Capability token 在任意时刻至多有一个非 `InTransit` 位置。`InTransit` 状态内部记录源位置和目标位置，但 token 在传输期间不可授权；
- **Namespace 约束：** Namespace 不跨节点挂载——跨节点命名通过分布式服务实现，而非内核机制。这一限制避免分布式 Mount 环检测的 CAP 问题；
- **Timer 约束：** 每个节点拥有独立的 `now` 时钟。跨节点 Timer 语义不在本规范中定义。

### 8.6 GPU / NPU / FPGA Task

非 CPU 计算介质上的 Task 拥有独立的执行模型和内存池，但共享同一个 Capability 位置空间。

**计算介质的执行语义差异：**

|               | CPU Task                       | GPU Task                          | NPU Task                      | FPGA Task                  |
| ------------- | ------------------------------ | --------------------------------- | ----------------------------- | -------------------------- |
| vspace        | MMU 页表（VirtPage → PageMapping） | 空                                 | 空                             | 空                          |
| entry 含义      | 虚拟地址                           | CUDA kernel 指针 / OpenCL kernel 句柄 | 模型文件路径 + 入口函数名                | 位流 ID + 入口信号               |
| 内存池           | `frame_universe`（通用内存）         | `gpu_frame_universe[CUDA]`（显存）    | `gpu_frame_universe[NPUArch]` | `gpu_frame_universe[FPGA]` |
| Running 语义    | CPU 核执行指令流                     | GPU 流上 kernel 正在执行                | NPU 推理引擎活跃                    | FPGA 电路正在运行                |
| 阻塞类型          | BlockedRecv                    | 不支持 `recv`（GPU 不能独立发起 recv）       | 同 GPU                         | 同 GPU                      |
| Capability 持有 | 全面                             | 仅 `initial_caps` 注入的 SegCap       | 同 GPU                         | 同 GPU                      |

**GPU 内存操作：**

GPU 显存通过 `gpu_frame_universe` 管理，逻辑上与 CPU `frame_universe` 隔离。显存 Segment 的创建类似于 CPU Segment，但页帧来自对应 GPU 架构的帧池：

```text
gpu_memseg_create(arch, page_count, allowed_access) → CapId
```

GPU 显存映射到 CPU Task 的 `vspace`（用于 CPU 端读写显存内容）走常规 `memseg_map`。底层通过 COSIR 细化保证映射的一致性（PCIe BAR 映射或 unified memory 页迁移）。

**GPU kernel 启动：**

GPU Task 通过 `task_create` 创建，注入 `initial_caps` 包含其可访问的显存 Segment Capability。GPU kernel 在 COSIR 细化层通过特定驱动提交（CUDA stream、Vulkan command buffer 等）：

1. CPU Task 持有 GPU Task 的管理 Capability；
2. CPU Task 通过消息协议向 GPU 驱动服务提交 kernel 启动请求；
3. GPU 驱动验证参数后将 kernel 调度到 GPU 执行；
4. GPU Task 在执行期间只有 `initial_caps` 注入的 Segment 访问权；
5. 执行完毕时 GPU 发出完成中断 → `ExternalInterrupt` 异常通知 → CPU 端驱动通知调用者。

GPU Task 不持有对其他 Task 的 `TaskSend` Capability（不能发送消息），也不订阅异常源（不能处理 page fault）。所有与 GPU 的交互走共享 Segment + 中断通知路径。

## 9. Send / Recv

### 9.1 `send`

```text
send(to_task_cap, data, transferred_caps) -> ()
```

前置条件：

1. `to_task_cap` 由调用者持有、可用并授权 `TaskSend`；
2. `data.length ≤ MAX_MSG_SIZE`；
3. `transferred_caps.length ≤ MAX_CAPS_PER_MSG`；
4. `transferred_caps` 无重复；
5. 每个待传 Capability 都由调用者持有且 `usable`；
6. 任何待传 Capability 都不是活动 Task 的结构性根 Namespace Capability。

全部验证完成后，内核原子地：

1. 构造来源为 `FromTask(caller)` 的 Message 输入；
2. 使用 `enqueue_or_deliver` 分配 `MsgId`、记录 `MessageSent`，并把全部 Capability 从调用者的位置原子地移动到 Message 或接收者。`MessageSent` 包含来源、目标、payload 长度与摘要以及 Capability ID 列表。

`to_task_cap` 可以同时出现在 `transferred_caps` 中；授权先完成验证，再发生移动。

错误：`InvalidTask`、`MissingRight(TaskSend)`、`MsgTooLarge`、`TooManyCaps`、`DuplicateCap`、`InvalidCap`、`StructuralCap`。

### 9.2 `recv`

```text
recv() -> RecvResult | Blocked
```

若邮箱非空：

1. 取 FIFO 队首消息；
2. 将消息中的全部 Capability 从 `InMessage(msg)` 移到 `HeldBy(caller)`；
3. 从活动消息表和邮箱移除该消息；
4. 追加 `MessageReceived`，此处是本次 `recv` 的线性化点；
5. 返回内核记录的 `origin`、数据和 Capability ID；
6. Task 保持 `Running`。

若邮箱为空：

1. 当前 Task 变为 `BlockedRecv`；
2. 该调用尚未向用户态返回；
3. 后续消息通过 `enqueue_or_deliver` 完成此调用，把结果写入 `pending_return` 并将 Task 设为 `Ready`。

已经进入消息的 Capability 可能因目标对象在发送后被销毁而变成 stale token。`recv` 仍原子交付原来的 token 身份，但接收方不能用它授权。这与“消息和 token 整体到达”一致，不承诺目标对象在网络时间窗内继续存活。

`recv` 不需要 Capability；安全边界位于发送方必须持有目标 Task 的 `TaskSend`。

**失败语义：无失败分支。** 空邮箱返回 `Blocked` 是正常成功路径而非错误；调用者由 §6.1 保证是 `Running` Task，本操作不接受参数、不检查 Capability。

## 10. Timer

### 10.1 时间模型

`now` 是单调不减的抽象绝对时间。状态机不规定时间单位与时钟精度。

```text
time_now() -> Instant
```

`time_now` 是纯查询，不改变状态。

**失败语义：无失败分支。** 无参数、无前置条件、不检查 Capability。

### 10.2 `timer_set`

```text
timer_set(deadline) -> timer_cap
```

调用者只能为自己创建 Timer，接收者固定为调用者。成功动作：

1. 创建 Timer；
2. 铸造由调用者持有的根 Timer Capability，权限为 `Derive | TimerCancel | TimerDestroy`；
3. 追加 `TimerSet(timer_id, caller, deadline)`；
4. 若 `deadline > now`，Timer 进入 `Pending`；
5. 若 `deadline ≤ now`，在同一原子操作中：
   - 追加唯一的 `TimerFired(timer_id, deadline, now)`；
   - 创建一次 `FromKernel(TimerFired(timer_id))` 通知；
   - 通过 `enqueue_or_deliver` 发送给调用者并取得 `MsgId`；
   - Timer 以该 `MsgId` 作为 `notification` 直接进入 `Fired`。

过去或当前的 deadline 不是错误。该语义避免“检查当前时间后再设置 Timer”产生竞态，也消除了 `deadline == now` 究竟何时触发的歧义。

Timer 通知不携带或移动 Timer Capability；接收方通过不可伪造的 `KernelEvent` 中的 `TimerId` 识别来源。

**失败语义：无失败分支。** 除 §6.1 的 Actor 约束外本操作无前置条件，不接受 Capability 参数；过去/当前 deadline 显式非错误。实现层 ID 或日志空间耗尽属 §6.2 明确排除的资源可用性问题，不作为抽象层失败。

### 10.3 `advance_time`

```text
advance_time(new_now) -> ()
```

这是 `KernelActor` 内部迁移，要求 `new_now ≥ now`。操作原子地：

1. 将 `now` 更新为 `new_now`；
2. 找出所有 `Pending` 且 `deadline ≤ new_now` 的 Timer；
3. 按 `(deadline, TimerId)` 的稳定顺序逐个处理；
4. 对每个 Timer 追加唯一的 `TimerFired(timer_id, deadline, new_now)`；
5. 通过 `enqueue_or_deliver` 发送一次内核通知、取得 `MsgId`，并让 Timer 以该 `MsgId` 作为 `notification` 进入 `Fired`。

迁移完成后必须满足：所有 `Pending` Timer 都有 `deadline > now`。

错误：`InvalidTime`（`new_now < now`，违反时间单调性）。`InvalidTime` 定义为本规范中“时间参数违反 `now` 单调不减”的失败标识。失败时 `now` 与全部 Timer 状态不变。

### 10.4 `timer_cancel`

```text
timer_cancel(timer_cap) -> ()
```

前置条件：`timer_cap` 授权 `TimerCancel`。

- `Pending`：进入 `Cancelled(ByTask(caller))`，追加 `TimerCancelled`，不产生通知；
- `Fired`：返回 `AlreadyFired`，状态不变；
- `Cancelled`：返回 `AlreadyCancelled`，状态不变。

错误：`InvalidTimer`（`timer_cap` 不指向有效 Timer）、`NotHeld`（不在 `HeldBy(caller)`）、`MissingRight(TimerCancel)`。`InvalidTimer` 定义为本规范中“Capability 参数未指向有效 Timer 对象”的失败标识。`AlreadyFired`/`AlreadyCancelled` 是状态返回而非失败，且保持状态不变。

### 10.5 `timer_destroy`

```text
timer_destroy(timer_cap) -> ()
```

前置条件：`timer_cap` 授权 `TimerDestroy`。

成功时从活动 Timer 表删除目标：

- 若仍为 `Pending`，先追加 `TimerCancelled`，不产生通知；
- 若已 `Fired`，已经排队或交付的通知不撤回；
- 若已 `Cancelled`，不产生新的取消事件；
- 最后追加对象销毁事件并删除活动 Timer；
- 指向该 Timer 的 Capability 全部成为 stale token。

错误：`InvalidTimer`、`NotHeld`、`MissingRight(TimerDestroy)`。

### 10.6 Exactly-once 边界

“恰好一次”指：每个进入 `Fired` 的 Timer 恰有一个 `TimerFired` 审计事件，以及恰一个来源为 `FromKernel(TimerFired(timer_id))` 的 `MessageSent`；两者指向同一 `notification : MsgId`。它不保证接收 Task 最终运行，也不保证 Task 在通知到达前不会被销毁。

## 11. Namespace

### 11.1 结构约束

一个 Namespace 内的活动条目必须满足：

1. 每个条目的 Capability 位置恰为 `InNamespace(ns, path)`；
2. `Bound` 和 `Mount` 都是本地终端条目；
3. 任何终端条目的路径都不能是另一条本地条目路径的严格前缀；
4. 若 `Mount(cap)` 的 Capability 可用，则其目标必须是活动 Namespace，且至少具有 `NsResolve`；这种条目称为**活动 Mount**；
5. 若挂载目标已销毁，则条目可以暂时保留为 **stale Mount**，但只能被 `ns_unmount` 清理，不能参与解析；
6. 由活动 Mount 建立的 Namespace 有向图必须无环。

第三条意味着挂载点以下不能同时存在本地条目，从而避免本地树与挂载树的遮蔽歧义。stale Mount 仍占用原路径，直至显式卸载。

### 11.2 `ns_create`

```text
ns_create() -> ns_cap
```

创建空 Namespace，并铸造由调用者持有的完整根 Capability。抽象模型中 Namespace 作为纯数学映射（`Path ⇀ NsEntry`）不消耗页帧；COSIR 细化层中 Namespace 的存储必然消耗内核内存，该消耗属于内核元数据开销，不在本规范中建模：

```text
Derive | NsResolve | NsBind | NsUnbind |
NsMount | NsUnmount | NsDestroy
```

**失败语义：无失败分支。** 除 §6.1 的 Actor 约束外本操作无前置条件、不接受 Capability 参数；实现层元数据空间耗尽属 §6.2 明确排除的资源可用性问题，不作为抽象层失败。

### 11.3 `ns_bind`

```text
ns_bind(ns_cap, path, cap) -> ()
```

前置条件：

1. `ns_cap` 授权目标 Namespace 的 `NsBind`；
2. `path` 合法且非空；
3. `cap` 由调用者持有且 `usable`；
4. `cap` 不是结构性根 Namespace Capability；
5. `path` 没有现有终端祖先、同名条目或后代条目。

成功动作：将 `cap` 移到 `InNamespace(ns, path)` 并建立 `Bound(cap)`。

绑定项只有在 Capability 含 `Derive` 时才能通过 `ns_resolve` 发放子 Capability。不含 `Derive` 的绑定仍可作为不可导出的保管项，并可由 `ns_unbind` 取回。

错误：`InvalidNamespace`、`MissingRight(NsBind)`、`InvalidPath`（`path` 非法或为空）、`Overlap`（存在终端祖先、同名条目或后代条目）、`InvalidCap`（`cap` 不是有效 CapId）、`NotHeld`、`StructuralCap`。

### 11.4 `ns_unbind`

```text
ns_unbind(ns_cap, path) -> CapId
```

前置条件：

- `ns_cap` 授权 `NsUnbind`；
- `path` 精确指向 `Bound(cap)`。

成功动作：删除条目，并把 `cap` 从 Namespace 移到 `HeldBy(caller)`。

被绑定 Capability 的目标可以已经销毁；`ns_unbind` 仍允许取回 stale token 以便 Audit 或清理。

错误：`InvalidNamespace`、`MissingRight(NsUnbind)`、`InvalidPath`、`NotFound`（`path` 未精确指向 `Bound(cap)`）。目标已销毁不构成失败。

### 11.5 `ns_mount`

```text
ns_mount(destination_ns_cap, path, source_ns_cap) -> ()
```

前置条件：

1. `destination_ns_cap` 授权 `NsMount`；
2. `source_ns_cap` 由调用者持有、可用且指向 Namespace；
3. `source_ns_cap` 具有 `NsResolve` 和 `Derive`；
4. `path` 没有终端祖先、同名条目或后代条目；
5. 添加 `destination → source` 边不会形成 Mount 环；
6. `destination_ns_cap` 和 `source_ns_cap` 的目标 Namespace 必须位于同一节点（单机场景）。跨节点 Namespace 挂载不属于内核机制——跨节点命名通过用户态分布式服务实现（§8.5 Namespace 约束）。

成功动作：

1. 从 `source_ns_cap` 派生权限恰为 `{NsResolve, Derive}` 的子 Capability；
2. 子 Capability 直接放入 `InNamespace(destination, path)`；
3. 建立 `Mount(child_cap)`；
4. `source_ns_cap` 仍由调用者持有。

挂载明确使用 Capability，而不是裸 `NsId`。

错误：`InvalidNamespace`、`MissingRight(NsMount)`、`InvalidCap`（`source_ns_cap` 不是有效 CapId）、`NotHeld`、`NotDelegable`（`source_ns_cap` 缺 `NsResolve`/`Derive`）、`InvalidPath`、`Overlap`（`path` 冲突）、`MountCycle`（新增 `destination → source` 边会形成 Mount 环）。`MountCycle` 定义为本规范中“挂载边引入环”的失败标识。跨节点挂载由 `InvalidNamespace`（目标不在同一节点）拒绝。

### 11.6 `ns_unmount`

```text
ns_unmount(ns_cap, path) -> ()
```

前置条件：

- `ns_cap` 授权 `NsUnmount`；
- `path` 精确指向 `Mount(cap)`。

成功动作：删除挂载条目并删除挂载子 Capability 的位置；源 Namespace、父 Capability 和已经解析出去的子 Capability 均不受影响。即使源 Namespace 已销毁、挂载 Capability 已 stale，也允许清理。

错误：`InvalidNamespace`、`MissingRight(NsUnmount)`、`InvalidPath`、`NotFound`（`path` 未精确指向 `Mount(cap)`）。源 Namespace 已销毁、挂载 Capability 已 stale 均不构成失败。

### 11.7 `ns_resolve`

```text
ns_resolve(ns_cap, path, requested_rights) -> CapId
```

前置条件：

1. 调用者持有可用的 `ns_cap`，且其目标是 Namespace；
2. `ns_cap` 包含 `NsResolve`；
3. `path` 合法且非空。

解析算法：

1. 从 `ns_cap` 指向的 Namespace 根开始逐分量消费路径；
2. 若当前前缀无条目，继续检查更长的本地前缀；
3. 遇到 `Bound(cap)`：
   - 若仍有剩余分量，返回 `NotDirectory`；
   - 若已到路径末端，以该 `cap` 为派生父项；
4. 遇到 `Mount(cap)`：
   - 验证挂载 Capability 可用、目标是 Namespace 且有 `NsResolve`；
   - 切换到源 Namespace 根，并继续消费剩余分量；
   - 若挂载点恰为路径末端，以挂载 Capability 为派生父项；
5. 最终父 Capability 必须可用并含 `Derive`；
6. `requested_rights` 必须是父权限的合法子集；
7. 派生新 Capability，放到 `HeldBy(caller)`。

返回的是新 `CapId`，Namespace 中的父 Capability 保持不变。新 Capability 的 `parent` 指向最终 Bound 或 Mount Capability。成功时追加 `NsResolved`，记录调用者使用的起始 Namespace Capability、规范化路径、依次经过的 Mount Capability、最终父 Capability 和新 Capability；因此既能追溯派生关系，也能重建路径授权证据。

错误：`NotFound`、`NotDirectory`、`TargetDead`、`MountTargetDead`、`NotDelegable`、`RightsExceeded`、`InvalidPath`。

### 11.8 `ns_destroy`

```text
ns_destroy(ns_cap) -> ()
```

前置条件：

- `ns_cap` 授权 `NsDestroy`；
- 该 Namespace 不是任何活动 Task 的结构性根 Namespace。

成功动作：

1. 删除 Namespace 内全部条目；
2. 删除这些 Bound/Mount Capability 的位置；
3. 从活动 Namespace 表删除目标；
4. 指向该 Namespace、但位于其他容器中的 Capability 变为 stale token。

其他 Namespace 中指向它的挂载项不会被全局扫描删除；解析这些项会得到 `MountTargetDead`，管理者可随后 `ns_unmount`。这一选择避免销毁操作隐式改写无授权的其他 Namespace。

错误：`InvalidNamespace`、`NotHeld`、`MissingRight(NsDestroy)`、`StructuralCap`（是活动 Task 的结构性根 Namespace）。

## 12. Exception / Upcall

Exception / Upcall 是让 Task 以 OS 中立方式接收异步硬件和内核事件的机制。它与 Send/Recv 的区别是：Send 由另一个 Task 的 Capability 授权，而 Exception 由 Task 显式订阅的事件源产生。每个事件源是一个独立的内核对象；异常通知不可伪造来源，接收方不能伪造订阅。

### 12.1 设计原则

Exception 原语遵循以下约束：

1. **内核不解释异常语义。** 内核只负责把事件从源路由到订阅 Task。硬件 page fault、除零、syscall trap 等的**含义**完全由接收方的 LibOS 或运行时解释；
2. **异常源是一等对象。** 每个事件源有独立 ID、生命周期和授权边界；
3. **订阅显式、可撤销。** 订阅本身是 Capability 操作；订阅方可主动取消订阅，订阅源可被销毁；
4. **通知走已有消息通道。** 异常通知复用 `FromKernel(ExceptionEvent)` 和 `enqueue_or_deliver`，不引入新的通知机制；
5. **一个源可被多个 Task 订阅（fan-out）。** 不同 Task 可以为不同目的订阅同一事件源；但每个 Task 对同一源至多有一个活跃订阅；
6. **订阅不传递 Capability。** 订阅操作只创建事件源 → 接收方的通知关系；接收方不能通过订阅获得事件源的修改或销毁权限。

### 12.2 ExceptionSource 对象与事件类型

```text
ExcSourceId : Nat
```

```text
ObjectRef = Segment(SegId)
          | Task(TaskId)
          | Namespace(NsId)
          | Timer(TimerId)
          | ExcSource(ExcSourceId)
```

```text
ExceptionKind = PageFault {
                    faulting_task: TaskId,
                    faulting_vaddr: VirtAddr,
                    cause: FaultCause
                }
              | GeneralProtectionFault {
                    faulting_task: TaskId
                }
              | DivideError {
                    faulting_task: TaskId
                }
              | InvalidOpcode {
                    faulting_task: TaskId
                }
              | DoubleFault {
                    faulting_task: TaskId
                }
              | SyscallTrap {
                    calling_task: TaskId,
                    syscall_number: Nat
                }
              | ExternalInterrupt {
                    interrupt_vector: Nat   // 硬件 IRQ 号（审计用）；实际路由通过 ExcSourceId 决定（见 §15.7）
                }
              | TaskExit {
                    exiting_task: TaskId
                }

FaultCause = PageNotPresent
           | PageProtectionViolation
           | PageReservedBitSet

ExceptionEvent = ExceptionRaised(ExcSourceId, ExceptionKind)

KernelEvent = TimerFired(TimerId)
            | ExceptionRaised(ExcSourceId, ExceptionKind)
```

`ExceptionKind` 的语义划分为三类：

- **硬件执行异常**（PageFault、GeneralProtectionFault、DivideError、InvalidOpcode、DoubleFault）：由 CPU 在指令执行期间同步触发，`faulting_task` 是正在执行的 Task；
- **Syscall trap**（SyscallTrap）：用户态主动发起的系统调用，`calling_task` 是发起调用的 Task。当前抽象模型尚未定义 syscall 的调用/返回协议；本对象只定义事件通知通道，语义适配由 LibOS 和 COSIR ABI 细化层完成；
- **异步事件**（ExternalInterrupt、TaskExit）：硬件中断（IRQ 路由到此）或 Task 生命周期事件；

`TaskExit` 不等同于 `task_destroy`；它是 Task 执行终止产生的事件。`TaskExit` 不是硬件直接触发的异常，而是软件行为经以下路径产生：

1. 用户态 Task 通过 `SyscallTrap` 发起终止请求（例如 `exit` 系统调用）；
2. 订阅 `syscall_trap_source` 的 LibOS 处理该请求，通过某个 *COSIR 细化层定义的操作*（当前抽象规范未覆盖）告知内核标记该 Task 为已终止；
3. 内核随后调用 `raise_exception(task_exit_source, TaskExit{exiting_task = t})` 通知所有订阅者。

如果 Task 遇到不可恢复的致命异常（如高特权级发生的 PageFault 或 DoubleFault），内核可以直接触发 `TaskExit` 而不通过 SyscallTrap 路径——这是 COSIR 细化层的实现选择。COW fork 的子进程可以通过订阅 `task_exit_source` 实现 wait 语义。

### 12.3 ExceptionSource 状态

```text
Subscriber {
    task:       TaskId,
    subscribed_at: Instant,
}

ExcSourceState = Active {
                     kind:        ExceptionKind → Type     // 描述此源可产生的异常类别约束
                     subscribers: FiniteSet<Subscriber>
                 }
               | Destroyed

ExceptionSource {
    creator: TaskId,
    state:   ExcSourceState,
}
```

`kind` 是此源的"类型签名"——不是具体的异常实例，而是一个约束，限定此源只能产生哪类 `ExceptionKind`。例如：

- 一个订阅了 `PageFault` 的源不能产生 `SyscallTrap`；
- 一个订阅了 `ExternalInterrupt` 的源不能产生 `DivideError`。

`kind` 在创建时固定，不可更改。这种类型安全性由内核在异常路由时强制——尝试在一个不匹配的源上触发异常是一项 panic/内核 bug，不应到达抽象状态机层面。

### 12.4 订阅生命周期

订阅是异常源的内部状态，不是独立的 Capability。订阅由 `exc_subscribe` 创建，由 `exc_unsubscribe` 或事件源销毁终止。接收方 Task 不需要持有异常源的 Capability 来接收通知——它只需要在订阅期间保持存活。

```text
exc_subscribe(exc_source_cap, exception_kind) -> ()
```

前置条件：

1. 调用者是当前 Running Task；
2. `exc_source_cap` 由调用者持有且指向 ExcSource；
3. `exc_source_cap` 包含 `ExcSubscribe`；
4. `exception_kind` 与此源的 `kind` 类型兼容；
5. 调用者尚未订阅此源的同一 `exception_kind`。

成功动作：

1. 添加 `{task = caller, subscribed_at = now}` 到该源的 `subscribers`；
2. 追加 `ExcSubscribed(exc_source_id, caller, exception_kind)` Audit 事件。

`exc_subscribe` 消耗的 Capability 仍由调用者持有；订阅成功不代表调用者获得了对此源的操作权限。

错误：`InvalidExcSource`、`MissingRight(ExcSubscribe)`、`KindMismatch`、`AlreadySubscribed`。

```text
exc_unsubscribe(exc_source_cap) -> ()
```

前置条件：

1. `exc_source_cap` 由调用者持有且指向 ExcSource；
2. `exc_source_cap` 包含 `ExcSubscribe`；
3. 调用者当前订阅了此源（至少一个 `exception_kind`）。

成功动作：

1. 从该源的 `subscribers` 删除所有 `task = caller` 的条目；
2. 追加 `ExcUnsubscribed(exc_source_id, caller)` Audit 事件。

`exc_unsubscribe` 不消耗 Capability。调用者可以在任何时候取消订阅，包括异常源已销毁后（此时操作返回 `TargetDead`）。

错误：`InvalidExcSource`（`exc_source_cap` 不是有效 CapId 或未指向 ExcSource）、`NotHeld`、`MissingRight(ExcSubscribe)`、`NotSubscribed`（调用者当前未订阅此源）、`TargetDead`（源已销毁）。`TargetDead` 是终态清理路径的明确失败返回，失败时订阅表不变。

### 12.5 异常递送

异常在具体硬件事件发生时由 `KernelActor` 触发。内核执行：

```text
raise_exception(exc_source_id, kind) -> ()
```

前置条件：

1. `exc_source_id` 指向活跃的 ExceptionSource；
2. `kind` 与此源的 `kind` 类型兼容；
3. 由 COSIR 细化层在合适的线性化点调用（例如页故障在 `iretq` 返回前、syscall trap 在 `syscall` 指令后）。

成功动作：

1. 追加 `ExceptionRaised(exc_source_id, kind)` Audit 事件；
2. 对 `subscribers` 中每个 `{task, _}`：
   - 构造 `FromKernel(ExceptionRaised(exc_source_id, kind))` 的 KernelEvent；
   - 通过 `enqueue_or_deliver` 向 `task` 发送通知；
   - 该通知不携带 Capability；
3. `raise_exception` 的调用上下文（中断栈、寄存器保存等）由 COSIR 细化定义，不在本抽象状态机中。

如果 `faulting_task`（见 `ExceptionKind` 中的 task 字段）是 Running Task 且异常不可恢复（例如 DoubleFault），则该 Task 被标记为不可继续执行。当前抽象模型把这种行为建模为：Task 在 `raise_exception` 完成后不再 Running，必须通过 `schedule` 选择新 Task。具体恢复路径（如 `iretq` 到异常返回入口）由 COSIR 细化定义。

**重复的 ExceptionRaised 事件具有相同的 `ExcSourceId` 和 `ExceptionKind` 类型，但 `faulting_vaddr`（对页故障）或其他字段值可以不同。** 每个事件都有独立的 `EventId`。

**失败语义：无失败分支。** `raise_exception` 是 `KernelActor` 内部迁移；`exc_source_id` 由内核保证指向活跃源，`kind` 与源声明不一致属内核缺陷（§12.3），不作为抽象层可返回的错误。本步骤不引入用户可触发的失败。

### 12.6 异常源创建与销毁

```text
exc_source_create(supported_kinds) -> exc_source_cap
```

前置条件：

- `supported_kinds` 非空，且是单一、互不重叠的 `ExceptionKind` 构造子集合（不能同时包含 `PageFault` 和 `SyscallTrap` 等无关类型）。单个源的 `kind` 约束在创建后不可变。

成功动作：

1. 分配 `ExcSourceId` 并创建 `ExceptionSource`；
2. `subscribers` 初始为空；
3. 铸造完整根 `ExcSource Capability`，权限为 `{Derive, ExcSubscribe, ExcSourceDestroy}`；
4. 追加对象创建和 Capability 铸造事件。

错误：`InvalidKind`、`KindConflict`。

Bootstrap 后至少需要创建以下预定义异常源以支持基本硬件事件：

| 源 | kind 约束 | 用途 |
|---|---|---|
| `page_fault_source` | `PageFault` | 按需分页、COW、保护页 |
| `syscall_trap_source` | `SyscallTrap` | syscall 转发到 LibOS |
| `general_fault_source` | `GeneralProtectionFault \| DivideError \| InvalidOpcode \| DoubleFault` | LibOS 信号模拟 |
| `hardware_interrupt_source` | `ExternalInterrupt` | 设备中断路由 |
| `task_exit_source` | `TaskExit` | 进程退出通知 |

这些预定义源不是 Bootstrap 的隐式产物。init Task 或平台初始化代码必须在启动后显式创建它们，并将相应的 `ExcSubscribe` Capability 委托给需要接收异常的 Task。

```text
exc_source_destroy(exc_source_cap) -> ()
```

前置条件：

- `exc_source_cap` 授权 `ExcSourceDestroy`。

成功动作：

1. 删除所有 `subscribers` 条目；
2. 状态变为 `Destroyed`；
3. 从活动 ExcSource 表删除目标；
4. 追加销毁事件；
5. 指向该源的 Capability 变为 stale token。

销毁异常源不会向订阅者发送通知。已经排队但未交付的通知不受影响。

错误：`InvalidExcSource`、`NotHeld`、`MissingRight(ExcSourceDestroy)`。

### 12.7 与 Send/Recv 的关系

Exception 和 Send 是两个正交的通信维度：

| | Send | Exception |
|---|---|---|
| 触发方 | 持有 `TaskSend` 的用户 Task | 硬件/内核事件 |
| 发送方身份 | `FromTask(sender)` | `FromKernel(ExceptionRaised(...))` |
| 携带 Capability | 是 | 否 |
| 携带数据 | 是（bytes） | 否（仅事件元数据） |
| 接收方保护 | 发送方需要目标 Task 的 `TaskSend` | 接收方显式订阅 |
| 阻塞语义 | 发送方不阻塞 | 接收方的阻塞 recv 可被异常通知满足 |
| 通知 destination | 必须是目标 Task | 广播给所有订阅者 |

**设计要点：** 异常通知复用 `enqueue_or_deliver`，因此：
- 正在 `BlockedRecv` 的 Task 会被异常通知唤醒并收到 `RecvResult`；
- 异常通知与其他消息混合在同一邮箱 FIFO 中；
- 接收方通过 `origin = FromKernel(ExceptionRaised(...))` 区分异常和普通消息；
- 如果接收方需要快速区分异常和普通消息（而不扫描邮箱），可以通过在独立 Task 中订阅异常源实现——该 Task 只接收异常通知，通过共享 Segment 或消息向主 Task 转发元数据。

### 12.8 COW fork 的建模示例

使用 Exception 原语，COW fork 可以在 POSIR 层面被分解为以下原语组合（不需新增内核对象）：

1. **父 Task** 持有可写的私有匿名 Segment S；
2. **子 Task** 通过 `task_create` 创建，`code_seg_cap` 等参数与 fork 语义对应；
3. **父** 对 S 建立子 Task 只读映射（`SegMapR`）：`memseg_map(seg_cap, child_task_cap, ... access={R})`；
4. **父** 将自己的 S 映射也改为只读（这是用户态 `memseg_unmap` + `memseg_map` 序列，不是内核 fork 一步完成）；
5. **父** 和 **子** 都订阅 `page_fault_source`（若尚未订阅）；
6. 当任一 Task 写入被标记为只读的页时，硬件触发 page fault → `raise_exception(page_fault_source, PageFault{..., cause = PageProtectionViolation})`；
7. 接收方 LibOS 识别到写入请求发生在 COW 页上 → LibOS 执行：
   - 分配新 Segment；
   - 复制原页内容；
   - 更换映射为私有可写；
   - 通过消息通知另一方"你的映射已更新"。

**这不是内核提供的 `fork`。** 它是用通用原语构造的 `fork` 语义。内核只知道"page fault 发生了"和"订阅了这个源的 Task 收到通知"。所有 COW 逻辑都在用户态。

**证据边界：** 在明确限定的 host 组合场景中，MODEL-TEST 未发现为“共享只读映射 → 写保护 fault → 用户态显式分配相离新 Segment 并换映射 → exc_resume → 显式调度”新增 POSIR 一等原语的需求。该结论不覆盖帧字节复制、真实 trap/PTE/TLB、多核协议、KERNEL-TRACE 或 REFINEMENT-PROOF。反例 R4（不换映射直接恢复则该页仍只读，内核式自动 COW 复制不可从既存原语演绎）为对本节“用户态 COW”的边界澄清，非 H1 反例；反例 R1（两个 Task 并发写同一共享只读页时各 fault、无合并原语）表明并发 COW 协调是用户态显式责任。

### 12.9 Page fault 的异常源语义细节

`PageFault` 是最复杂的异常类型，因为它涉及"Task 当前无法继续执行"的同步语义。以下详细定义页故障在抽象状态机中的行为：

**故障发生时的抽象状态：**

1. 当前 Running Task `T` 尝试访问虚拟地址 `va`，但 `va` 在 `T.vspace` 中没有映射，或映射的 `access` 不允许当前访问类型（R/W/X）；
2. COSIR 细化层在 trap 处理中识别此条件，将 `T` 的指令指针、栈指针等执行上下文保存在 per-Task 的"异常帧"中。**抽象状态机不建模指令指针和寄存器的精确值**，只建模"T 被暂停，异常帧已保存"这一事实；
3. `KernelActor` 调用 `raise_exception(page_fault_source, PageFault{...})`；

**页故障处理期间的抽象状态变化：**

- 如果 `T` 订阅了 `page_fault_source`，异常通知通过 `enqueue_or_deliver` 送达 `T` 的邮箱（或直接交付，如果 `T` 当前正在 `BlockedRecv`）；
- 如果 `T` 未订阅该源，则行为由平台定义：COSIR 可以实现 `T` 直接被销毁，或通知 `task_exit_source` 的所有订阅者，或通过预配置的默认处理路径。**抽象状态机不强制具体行为**，因为这是一种不可恢复错误；
- `T` 在 `raise_exception` 完成时不再是 `Running`（因为指令流无法继续），而进入细化层可见的 `Faulted` 状态并关联唯一的 `pending_exc`；`Faulted` 不对 POSIR 层可见，且调度器不会把它隐式恢复或重播原始指令流；
- 只有 §12.10 定义的 `exc_resume` 成功提交后，`T` 才从 `Faulted` 变为 `Ready`；随后 `schedule` 仅执行通常的 `Ready → Running`。

**恢复路径（`exc_resume`）：**

- 页故障处理方检查 `faulting_vaddr` 与 `cause`，在用户态决定映射新页、复制 COW 页、重播故障指令、跳转处理程序或终止进程；内核只原子恢复一份经验证的异常上下文帧，不解释该策略。
- 恢复入口绑定原异常源、调用者的活跃订阅和调用者自身的待恢复异常。授权与帧语义完整定义于 §12.10，不存在另一个未定义的恢复 ABI。

**当前抽象模型的显式范围：**

- **定义了：** 事件源对象、订阅与通知递送、`Faulted`/`pending_exc` 绑定、`exc_resume` 参数与授权、x86_64 帧编码、验证与原子提交语义；
- **不声称：** host 模型已经接入真实 trap、真实 `iretq`，或提供 `KERNEL-TRACE` / `REFINEMENT-PROOF`。当前证据仅为 host `MODEL-TEST`。

### 12.10 `exc_resume` — 恢复被暂停的异常帧

```text
exc_resume(exc_source_cap, sret_frame_seg_cap, frame_offset) -> ()
```

前置条件与纯验证阶段：

1. `exc_source_cap` 必须由调用者持有、处于 active、指向 active `ExceptionSource`，并含 `ExcSubscribe` 权限；调用者还必须位于该 source 的 active subscription 集合；
2. `sret_frame_seg_cap` 必须由调用者持有、处于 active、指向调用者拥有 `SegMapW` Authority 且内核可读取 bytes 的 Segment；内核读取仅用于受保护的 `exc_resume` 实现，不新增用户可见 `SegWrite` 权限。
3. `frame_offset` 为 16-byte aligned，`frame_offset + 0xA0` 不溢出且不超过 Segment bytes；x86_64 `SretFrame` 是固定 `0xA0` 字节 little-endian 编码：`rax`、`rbx`、`rcx`、`rdx`、`rsi`、`rdi`、`rbp`、`rsp`、`r8`–`r15`、`rflags`、`rip`、`cs`、`ss` 各占连续的 8 字节槽；
4. 帧中的 `RIP` 与 `RSP` 必须是 canonical 用户地址；`RIP` 位于调用者可执行用户映射，`RSP` 位于调用者可写用户映射；`CS`、`SS` 满足用户态 selector 策略，`RFLAGS` 不含禁止的特权或保留位；
5. 调用者 Task 必须处于 `Faulted`，且有 `pending_exc`；pending 的 `source_id` 必须匹配 capability 指向的 source，pending `kind` 必须匹配 source 支持的 kind；
6. pending `ExceptionKind` 必须可恢复：`PageFault` / `GeneralProtectionFault` / `DivideError` / `InvalidOpcode`。`DoubleFault` 等不可恢复异常返回 `UnrecoverableFault` 并保持 `Faulted`；
7. 上述 capability、subscription、Segment bytes、frame、Task、pending/source/kind 检查全部是纯验证；任何检查失败前不得修改任何 State 字段。

成功动作按以下严格顺序作为一个原子提交。该提交段在模型内不产生可恢复的 `ResumeError`；其无界 host `Vec` 的 allocator abort/OOM 不在该模型内：

1. 清零 Segment 中 `[frame_offset, frame_offset + 0xA0)` 的已消费 frame bytes；
2. 清除调用者的 `pending_exc`；
3. 将已验证的完整 `SretFrame` 保存为内核持有的 `kernel_resume_frame`；
4. 追加 `TaskResumed` Audit，记录 actor、source ID、两个 Capability ID、Segment ID、frame offset 与帧中 RIP；
5. 将 Task 从 `Faulted` 置为 `Ready`。

错误包括：`InvalidExcSource`、`MissingRight`、`NotSubscribed`、`InvalidFrameAddr`、`InvalidFrame`、`NoPendingException`、`UnrecoverableFault`。任一失败都保持**完整 State 原子不变**，包括 Task 状态、`pending_exc`、capabilities、subscriptions、Segment bytes、`kernel_resume_frame` 与 Audit log。

恢复与调度的交互：`exc_resume` 只提交一帧并执行 `Faulted → Ready`；`schedule` 之后只负责 `Ready → Running`，不隐式重播故障指令。当前 Rust 工件仅以 host `MODEL-TEST` 检查这些状态语义，不是实际 kernel/trap 返回实现。

## 13. Audit

### 13.1 事件模型

每个成功的状态修改操作追加一个或多个具有唯一 `EventId` 的事件。至少包括：

```text
Bootstrapped
ObjectCreated / ObjectDestroyed
CapMinted / CapDerived / CapMoved / CapDropped
SegmentMapped / SegmentUnmapped
TaskCreated / TaskStateChanged / TaskDestroyed
MessageSent / MessageReceived / MessageDropped
TimerSet / TimerFired / TimerCancelled
NsBound / NsUnbound / NsMounted / NsUnmounted / NsResolved
```

每条事件至少包含 `{id : EventId, at : Instant, actor : Actor, kind : AuditKind}`。`kind` 必须记录该操作涉及的对象 ID 和 Capability ID；Capability 位置变化必须记录旧位置与新位置（删除时为 `None`）；受保护操作必须记录用于授权的 Capability ID、被检查的权限和目标对象；创建类环境操作必须记录调用者；`NsResolved` 必须记录起始 Namespace Capability、规范化路径、按顺序经过的 Mount Capability、最终父 Capability和新 Capability。`MessageSent` 固定记录 MsgId、来源、目标、payload 长度和 Capability ID 列表，不记录明文 payload；内容摘要不属于抽象状态。

`audit` 与 `caps` 都是追加式的；活动对象销毁、Capability 丢弃或消息收取不得删除历史。

### 13.2 `audit`

```text
audit(cap) -> AuditTrace
```

前置条件：`cap` 当前由调用者持有。目标对象可以已经销毁。

结果包含：

1. 从根 Capability 到 `cap` 的不可变 `parent` 链；
2. 每个链上 Capability 的铸造或派生事件；
3. `cap` 自创建以来的全部位置变化；
4. 目标对象的创建与销毁事件（若存在）。

`audit` 是纯查询，不参与权限判定，也不改变状态。权限判定只依赖当前活动对象、Capability 描述和位置。

错误：`InvalidCap`（`cap` 不是有效 CapId）、`NotHeld`（不在 `HeldBy(caller)`）。目标对象已销毁不构成失败——审计链仍可返回其创建与销毁事件。

## 14. Bootstrap

```text
bootstrap(
    frame_universe,
    image_frames,
    image_virtual_base,
    entry
) -> ()
```

Bootstrap 是唯一允许在没有 Task Actor 时铸造初始 Authority 的内部迁移。

前置条件：

1. 当前状态严格等于 `EmptyState`；
2. `image_frames` 非空、无重复且是输入 `frame_universe` 的子集；
3. `image_virtual_base ≤ page_of(entry) < image_virtual_base + image_frames.length`。

成功动作：

1. 初始化 `frame_universe`，并设置 `free_frames = frame_universe \ image_frames`；
2. 使用 `image_frames` 创建只允许 `{R, X}` 的 image Segment；
3. 创建空的 init Namespace；
4. 创建 init Task，并对每个 image 页 `i` 建立 `vspace[image_virtual_base + i] = {segment = image_segment, offset = i, access = {R, X}}`；
5. 铸造三个无父根 Capability：image Segment 权限为 `{Derive, SegMapR, SegMapX, SegDestroy}`，init Namespace 权限为完整 Namespace 权限集，init Task 权限为 `{Derive, TaskSend, TaskManageVSpace, TaskDestroy}`；
6. 将这些 Capability 放入 `HeldBy(init)`；
7. 把 Namespace Capability 指定为 init 的结构性根；
8. init Task 初始状态为 `Ready`；
9. 设置 `booted = true` 并写入第一组 Audit 事件。

Bootstrap 不向“不存在的调用者”返回 Capability。其创建的初始 Capability 直接位于 init Task。Bootstrap 后再次调用必须返回 `AlreadyBooted` 且状态不变。

系统随后通过 `schedule(init)` 首次进入用户态。

## 15. 设备与 I/O 对象

设备原语将硬件 I/O 资源封装为对象能力化的抽象，使用户态驱动和协议栈可以安全访问设备而无需内核理解设备语义。内核只提供建立安全硬件访问所需的机制。

### 15.1 设计原则

1. **设备是独立 Task 中驱动的私有资源。** 设备 Capability 授权对 MMIO/IRQ/DMA 的访问，不授权内核理解 USB、NVMe 或以太网协议；
2. **MMIO 区域是 Segment 的变体，有额外约束。** MMIO 页不可 swap、不可 fork、不可被普通 `memseg_create` 分配；
3. **IRQ 看作事件源。** 硬件 IRQ 路由通过 `ExternalInterrupt` ExceptionKind 递送给订阅者。设备驱动订阅特定 IRQ 号的事件源以接收中断；
4. **DMA 缓冲区是特殊 Segment。** DMA Segment 的物理页被 pinned 且受限的 IOMMU 域隔离；
5. **第一个 QEMU 原型先取最小设备集。** 串口、时钟/中断控制器、一种块设备或 ramdisk、可选 virtio 设备。

### 15.2 I/O 对象类型

```text
DeviceKind = MMIORegion | IOPortRange | DMABuffer | InterruptLine

DevId : Nat
```

```text
ObjectRef = Segment(SegId)
          | Task(TaskId)
          | Namespace(NsId)
          | Timer(TimerId)
          | ExcSource(ExcSourceId)   // 见 §12.2
          | Device(DevId)            // 见 §15.2
          | Node(NodeId)             // 见 §8.5
```

Node 是一等对象，定义于 §8.5；此处将其并入 ObjectRef 汇总，与 §19 总览列出的机制类别一致。`NodeId : Nat` 的定义见 §8.5。

### 15.3 权限

```text
DeviceRight = DevMMIOR  | DevMMIOW      // MMIO 区域读/写映射
            | DevIORead  | DevIOWrite   // I/O port 访问
            | DevDMAR      | DevDMAW    // DMA 缓冲区的总线读写方向
            | DevInterruptSubscribe     // IRQ 线订阅
            | DevDestroy
```

### 15.4 MMIO 区域对象

```text
MMIORegion {
    base_paddr:   PhysAddr,
    size_pages:   Nat,                      // > 0
    allowed_access: Access,                 // {R}, {W}, {R,W}
}

Device {
    creator: TaskId,
    kind:    DeviceKind,
}
```

```text
PhysAddr : Nat
```

`MMIORegion` 的 `allowed_access` 限制设备页的 R/W 类型（MMIO 不支持 X）。`size_pages` 是区域大小（以页为单位）。

**创建：**

```text
dev_mmio_create(base_paddr, size_pages, allowed_access) -> dev_cap
```

前置条件：

1. `base_paddr` 和 `size_pages` 由平台的内存发现/设备枚举信息提供；
2. 区间 `[base_paddr, base_paddr + size_pages * PAGE_SIZE)` 不在任何现有 MMIO 区域或 Segment 物理页的范围内；
3. `allowed_access` 非空且只含 `{R, W}`。

成功动作：

1. 创建 `Device` 对象，`kind = MMIORegion{...}`；
2. 铸造完整根 Capability，权限为 `{Derive, DevMMIOR/W (按 allowed_access), DevDestroy}`；
3. 追加设备创建 Audit 事件。

错误：`InvalidPhysRange`、`Overlap`、`InvalidAccess`。

**映射 MMIO 区域到 Task 虚拟地址空间：**

```text
dev_mmio_map(dev_cap, task_cap, target_base, access) -> ()
```

前置条件：

1. `dev_cap` 指向 `MMIORegion` 且授权对应 `access` 的 `DevMMIOR`/`DevMMIOW`；
2. `access ⊆ region.allowed_access` 且非空；
3. `task_cap` 授权目标 Task 的 `TaskManageVSpace`；
4. Task 的 `[target_base, target_base + size_pages)` 全部未映射。

成功动作：对 region 中每个页建立 `vspace` 映射（类似 `memseg_map`，但映射的是设备物理地址，不是 Segment）。

错误：`InvalidDevice`（`dev_cap` 未指向有效设备；本规范中该标识统一表示“设备 Capability 参数无效”）、`MissingRight(DevMMIOR)`/`MissingRight(DevMMIOW)`（按 `access`）、`InvalidAccess`（`access` 为空或超出 `region.allowed_access`）、`InvalidTask`、`MissingRight(TaskManageVSpace)`、`Overlap`（`[target_base, target_base + size_pages)` 已被映射）。

```text
dev_mmio_unmap(task_cap, target_base, size_pages) -> ()
```

语义同 `memseg_unmap`，作用于通过 `dev_mmio_map` 建立的映射；错误集合同 §7.3 `memseg_unmap`。

### 15.5 I/O Port（仅 x86_64）

```text
IOPortRange {
    base_port: Nat,
    port_count: Nat,            // > 0
}
```

`IOPortRange` 只对支持 I/O port 空间的架构（x86_64 通过 QEMU 的 `inb`/`outb` 等指令访问）有意义。在抽象模型中，它是一组受保护端口号。

```text
dev_ioport_create(base_port, port_count) -> dev_cap
```

铸造根 Capability，权限为 `{Derive, DevIORead, DevIOWrite, DevDestroy}`。

I/O port 授权在 Task 切换时由 COSIR 细化层通过 IOPL 或 TSS I/O bitmap 实现。抽象模型只建模"持有 Capability 的 Task 有权执行 I/O 指令"这一事实。

**架构警告：** 在 x86_64 细化中，IOPL（I/O Privilege Level）和 TSS（Task State Segment）I/O bitmap 的授权粒度是 **per-Task** 而非 **per-port**。持有任一 `IOPortRange` Capability 的 Task 在获得 I/O 特权（IOPL=3 或 bitmap 清零对应位）后，硬件层面可以访问其 bitmap 覆盖范围内的所有端口。COSIR 细化层可以将 bitmap 精确配置到每个 Task 实际持有 Capability 的端口范围，确保"超出授权范围的端口指令触发 #GP 异常"。但这一精确性取决于 bitmap 的每端口位控制机制——如果平台在单个 bitmap 中覆盖了 Task 未授权的端口范围，则 Task 在硬件层面可能获得超出 Capability 授权的端口访问能力。

抽象状态机的安全声明（§17.8）依赖 COSIR 细化层正确地实现了 per-port bitmap 隔离。具体实现必须确保每个 Task 的 TSS I/O bitmap 或等效机制严格限制到仅其持有 `IOPortRange` Capability 覆盖的端口范围。

**失败语义：无失败分支。** 除 §6.1 的 Actor 约束外本操作无前置条件（`base_port`/`port_count` 由平台枚举提供，`port_count > 0` 为构造约束）；资源耗尽按 §6.2 排除。

### 15.6 DMA 缓冲区

```text
DMABuffer {
    frames:      Vector<FrameId>,    // 非空、无重复、物理连续
    direction:   DMADirection,
    io_addr:     PhysAddr,           // 设备可见地址（可能 == 物理地址，也可能来自 IOMMU 翻译）
}

DMADirection = DMABidir | DMAToDevice | DMAFromDevice
```

`DMABuffer` 的页帧来自 `free_frames`，创建后从通用页池中移除。这些页被 pinned，不参与 swap 或被普通 `memseg_create` 分配。

```text
dev_dma_create(page_count, direction) -> dev_cap
```

前置条件：

- `page_count > 0`；
- `free_frames` 至少包含 `page_count` 个物理连续的页帧；
- 平台有能力确保 DMA 一致性（如通过 IOMMU 映射或 coherent memory）。

成功动作：

1. 从 `free_frames` 移除选定的连续页帧；
2. 创建 `DMABuffer` 对象，分配其页帧和 IO 地址；
3. 铸造根 Capability，权限为 `{Derive, DevDMAR (if DMAFromDevice/双向), DevDMAW (if DMAToDevice/双向), DevDestroy}`；
4. 页帧被清零（由平台负责通过 coherent 手段完成）。

错误：`NoMemory`、`NotContiguous`、`NoIOMMUDomain`。

DMA 缓冲区不能直接映射到用户态——驱动必须通过服务协议（共享 Segment 包含缓冲描述符、或拷贝）与之交互。这个设计选择避免了用户态进程直接访问设备物理页的隔离风险。

### 15.7 IRQ 对象

硬件中断线被建模为 `InterruptLine` 设备对象。IRQ 的递送复用 Exception 机制：每个 IRQ 设备有对应的 `ExcSource`，其 `kind` 约束为 `ExternalInterrupt`。

```text
InterruptLine {
    irq_number:    Nat,
    exc_source:    ExcSourceId,  // 该中断线绑定的事件源
}
```

```text
dev_irq_create(irq_number) -> dev_cap
```

成功动作：

1. 创建一个 `ExcSource`，`kind` 约束为 `ExternalInterrupt`；
2. 创建 `Device` 对象，`kind = InterruptLine{irq_number, exc_source_id}`；
3. 铸造根 Capability，权限为 `{Derive, DevInterruptSubscribe, DevDestroy}`；
4. 将 `ExcSubscribe` Capability 放在 `HeldBy(caller)`，使创建者可以自行委托中断订阅。

**失败语义：无失败分支。** 除 §6.1 的 Actor 约束外本操作无前置条件；`irq_number` 由平台设备枚举提供，其合法性属细化层职责，不作为抽象层失败。

硬件中断到来时，COSIR 细化层：

1. 识别 `irq_number` 对应的 `InterruptLine` 设备；
2. 调用 `raise_exception(exc_source_id, ExternalInterrupt{interrupt_vector: irq_number})`；
3. 所有订阅该 `ExcSource` 的 Task 收到通知；
4. 驱动在收到通知后通过 MMIO/I/O port 操作处理设备。

```text
dev_irq_mask(dev_cap) / dev_irq_unmask(dev_cap) -> ()
```

前置条件：持有 `DevInterruptSubscribe`。

成功动作：在 COSIR 细化层，这控制中断控制器的屏蔽位。抽象状态机不建模具体的屏蔽状态——它只建模"订阅者仍然可以接收通知"这一事实。

错误：`InvalidDevice`、`NotHeld`（不在 `HeldBy(caller)`）、`MissingRight(DevInterruptSubscribe)`。

### 15.8 设备销毁

```text
dev_destroy(dev_cap) -> ()
```

前置条件：`dev_cap` 授权 `DevDestroy`。

成功动作取决于设备类型：

- **MMIORegion：** 删除所有已有映射（同 `memseg_destroy` 对地址空间的影响）；删除设备对象；
- **IOPortRange：** 释放端口范围；删除设备对象；
- **DMABuffer：** 归还全部页帧到 `free_frames`；删除设备对象。归还的 DMA 帧由 DMA 设备写入过，其内容在物理页中保留前一 I/O 操作的字节，但归还后不再被任何 `vspace` 映射——页帧内容只能通过后续 `memseg_create` 分配并映射后才对用户态可见，而 `memseg_create` 对新分配的页清零（§7.1），因此不存在旧 DMA 数据的授权读取路径；
- **InterruptLine：** 同时销毁关联的 `ExcSource`（所有订阅被终止，但排队的通知保留）；删除设备对象。

销毁后的设备 Capability 变为 stale token。

错误：`InvalidDevice`、`NotHeld`（不在 `HeldBy(caller)`）、`MissingRight(DevDestroy)`。

### 15.9 现有对象模型的扩展

设备对象的引入需要在以下现有部分中添加对应字段和条件：

**KernelState (§5):**

```
KernelState {
    ...
    devices:    DevId ⇀ Device,
    ...
    next_dev:   DevId,
}
```

**Device 表的初始状态**在 `EmptyState` 中为空，Bootstrap 后无预设设备。所有设备由 init Task 或平台初始化代码在阶段 1 中创建。

**全局不变量 (§15，现在是 §16):**

新增不变量（对应 §16 全局良构不变量中的 W16，以下为 W16 的三个子条件）：

**W16.1 — 设备物理资源唯一：** 所有活动设备的物理资源（MMIO 区域、I/O port 范围、DMA 帧、IRQ 号）两两不交，且不与通用页池或 Segment 帧重叠。

**W16.2 — DMA 帧隔离：** DMA 缓冲区的帧不在 `free_frames` 中，也不在任何活动 Segment 中。销毁时必须归还这些帧并清零。

**W16.3 — IRQ 事件源有效：** 每个 `InterruptLine` 的 `exc_source` 指向活动 `ExcSource`，且其 `kind` 约束为 `ExternalInterrupt`。

**细化要求 (§16，现在是 §17) 中增加：**

11. MMIO 映射与普通 Segment 映射的区分，以及页表级别的缓存策略设置（PAT/MTTR）；
12. I/O port bitmap 的 per-Task 切换和特权级隔离；
13. IOMMU domain 的创建、设备 assignment、DMA 地址翻译和故障处理；
14. 设备枚举信息的传递与平台抽象；
15. Cache coherence 保证（coherent vs non-coherent DMA）和 fence 指令放置。

## 16. 全局良构不变量

所有 Bootstrap 后可达状态都必须满足以下 `WellFormed` 条件。

### W1 — ID 新鲜且不复用

所有活动 ID 和历史 ID 都小于对应 `next_*`；分配只使用当前 `next_*`，随后严格递增。

### W2 — Capability 描述良构

- 每个 `CapRecord` 权限类型正确；
- `parent = Some(p)` 时父记录存在；
- 父子目标相同；
- `child.rights ⊆ parent.rights`；
- `p < child`。

### W3 — Capability 单一位置

`locations` 对每个 `CapId` 至多给出一个位置。每个有位置的 ID 都必须存在 `CapRecord`。

### W4 — 容器与位置双向一致

- `HeldBy(t)` 要求 Task `t` 活动；
- `InMessage(m)` 当且仅当活动 Message `m` 的 `caps` 恰好包含该 ID 一次；
- `InNamespace(n,p)` 当且仅当 Namespace `n` 的 `entries[p]` 引用该 ID；
- 一个 Message 的 Capability 列表无重复；
- 一个 Namespace 路径至多引用一个 Capability。

### W5 — 结构性根 Namespace 有效

每个活动 Task 的 `root_ns_cap`：

- 由该 Task 持有；
- 目标是活动 Namespace；
- 至少具有 `NsResolve`；
- 不得被直接移动或丢弃。

### W6 — 物理页唯一归属

所有活动 CPU Segment 的 frame 集合两两不交；它们与 `free_frames` 不交；二者并集等于 `frame_universe`。

对每个 GPU 架构 `arch`：所有属于该架构的活动 GPU 显存 Segment 的 frame 集合两两不交；它们与 `gpu_free_frames[arch]` 不交；二者并集等于 `gpu_frame_universe[arch]`。不同 `arch` 和 CPU 池之间页帧完全隔离。

### W7 — 虚拟映射有效

每个 `PageMapping`：

- 指向活动 Segment；
- `offset` 位于 Segment 页范围内；
- `access` 非空且不超过 Segment 的 `allowed_access`；
- 同一 Task 的每个 `VirtPage` 只有一个映射。

### W8 — 可执行入口显式授权

每个活动 Task 的 `entry` 所在虚拟页存在且映射权限含 `X`。

### W9 — 调度状态一致

- `Running` 的 CPU Task 数量 ≤ CPU 核数；每个 CPU 核至多运行一个 Task；
- GPU/NPU/FPGA Task 的 `Running` 语义由 COSIR 细化定义——同一物理 GPU 上可同时有多个 kernel 在多个 stream 上执行；
- `BlockedRecv` Task 的邮箱为空且 `pending_return = None`；非 CPU Task 不能进入 `BlockedRecv`；
- `pending_return ≠ None` 的 Task 必须为 `Ready`；
- 每个 `pending_return` 的 Capability 列表无重复，且其中每个 Capability 都位于 `HeldBy(该 Task)`；
- `pending_return.msg_id` 不属于活动 `messages`，并恰有对应的 `MessageSent` 与 `MessageReceived` 事件，且发送事件先于接收事件。

### W10 — Message 一致性

- 每个活动 Message 的目标 Task 活动；
- 每个 Task 邮箱中的 MsgId 唯一且指向以该 Task 为目标的活动 Message；
- 每个活动 Message 恰好出现在目标 Task 邮箱一次；
- Message 中所有 Capability 的位置均为该 Message；
- 每个活动 Message 恰有一个 `MessageSent` 且尚无 `MessageReceived`，同一 MsgId 的发送事件必须先于接收或丢弃事件；
- `FromTask(t)` 可以引用已销毁 Task，但必须存在该 Task 的创建事件；`FromKernel` 只能包含规范定义的 `KernelEvent`。

### W11 — Timer 一致性

- `Pending` Timer 的接收 Task 活动且 `deadline > now`；
- 每个 `Fired` Timer 恰有一个 `TimerFired` 事件和一个 origin 为 `FromKernel(TimerFired(timer_id))` 的 `MessageSent`；其 `notification` 等于该发送事件的 MsgId；
- `Cancelled` Timer 没有在取消后产生 Timer 通知；
- Timer 状态不会从 `Fired/Cancelled` 回到 `Pending`。

### W12 — Namespace 一致性

- 路径全部规范化；
- 终端条目之间不存在严格前缀关系；
- Bound/Mount 条目与 Capability 位置双向一致；
- 活动 Mount 的目标是活动 Namespace 且含 `NsResolve`；
- 活动 Mount 图无环。

允许 stale Bound 和 stale Mount：前者不能作为派生父项，后者不能参与解析或活动 Mount 图；`ns_unbind` 与 `ns_unmount` 仍可分别清理它们。

### W13 — Audit 完整性

- 每个 `CapRecord.created_at` 指向对应铸造或派生事件；
- 每次 Capability 位置变化都有事件；
- 每个活动对象都有创建事件；
- Audit EventId 严格递增且事件序列只追加不改写。

### W14 — Bootstrap 唯一

- `booted = false` 时不存在用户可执行迁移；
- `booted = true` 后不能再次成功 Bootstrap；
- 所有无父 Capability 都来自 Bootstrap 或某个对象创建操作。

### W15 — ExceptionSource 一致性

- 每个活动 ExceptionSource 的 `subscribers` 中所有 `task` 都指向活动 Task；
- 同一 ExceptionSource 的 `subscribers` 中，每个 `task` 最多出现一次；
- `KindMismatch` 错误保证：内核不会在类型不兼容的源上触发异常；
- 每个 `InterruptLine` 设备的 `exc_source` 指向活动 ExcSource，且其 `kind` 约束为 `ExternalInterrupt`。

### W16 — Device 物理资源唯一

- 所有活动设备的物理资源（MMIO 区域、I/O port 范围、DMA 帧、IRQ 号）两两不交，且不与通用 `free_frames` 或任何活动 Segment 的帧重叠；
- DMA 缓冲区的帧不在 `free_frames` 中，也不在任何活动 Segment 中；
- 设备销毁时，其占用的所有物理资源必须被释放（MMIO 区域移除映射、DMA 帧归还并清零、IRQ 号释放）。

## 17. 预期安全结论与证明义务

本文不附带 Lean 4 证明；形式化工作应至少覆盖以下义务。

### 17.1 基础保持性

1. 初始空状态满足预启动良构条件；
2. `bootstrap` 成功结果满足 `WellFormed`；
3. 每个成功迁移保持 W1–W16。对不涉及 ExceptionSource 或 Device 的迁移，可以通过未修改投影保持与受影响投影保持分解证明，但成功结果仍须满足完整 WellFormed。
4. 每个失败迁移保持状态完全相同。

### 17.2 Capability 安全

**不可伪造：** 任意可达状态中，所有 `present` Capability 都存在 `CapRecord`，并可沿 `parent` 链到达 Bootstrap 或对象创建铸造事件。

**权限单调：** 任意派生链上，后代权限始终是祖先权限的子集。

**位置唯一：** 一个 `CapId` 不会同时被两个 Task 持有，也不会同时位于 Task、Message 或 Namespace。

**无隐式复制：** 除 `cap_derive`、`ns_resolve`、`ns_mount` 和对象创建中的显式派生/铸造外，任何操作都不会增加 `CapRecord` 数量。

### 17.3 Task 与内存隔离

**地址空间完整性：** 除 Bootstrap、新 Task 创建、目标 Task 销毁和持有该目标 `TaskManageVSpace` 的调用外，任何迁移都不能改变目标 Task 的 `vspace`。

**映射授权：** 除 Bootstrap 与新 Task 的初始代码映射外，每个新映射同时具有 Segment 访问权限和目标 Task 地址空间管理权限的成功检查记录；初始映射分别由 Bootstrap 可信迁移和 `task_create` 的代码 Segment Capability 授权。

**条件式硬件隔离：** 若具体机器保证 Task 只能通过其 `vspace` 访问物理页，则没有相应映射的 Task 不能访问该 Segment。该结论依赖 MMU/IOMMU 细化假设，不由本状态机单独推出。

### 17.4 显式授权信息流

抽象状态机承认以下显式跨 Task 通道：

1. 持有目标 Task 的 `TaskSend` 后发送的消息；
2. 经 `SegMap* + TaskManageVSpace` 建立的共享 Segment 映射；
3. 经 Namespace Resolve 路径获得的派生 Capability；
4. 获授权 Capability 后执行的对象操作；
5. 明确定义的 `time_now`、Timer 和 Audit 观察；
6. 经 `ExcSubscribe` + 硬件事件触发的异常通知；
7. 经 `DevInterruptSubscribe` + IRQ 路由的硬件中断通知。

应证明：在把全局 ID、内存余量、日志容量、调度与时间排除出观察后，任何改变其他 Task 局部可观察内核状态的用户迁移，都属于上述通道或持有相应管理/销毁 Capability。Task 的局部观察至少包括自身状态、地址空间、邮箱、`pending_return`、所持 Capability 及其可查询 Audit。

这不是字节级非干扰定理。若要证明数据机密性，必须扩展 Segment 字节、read/write、程序执行和观察等价关系。

### 17.5 消息原子性

- 发送成功时，payload 与全部 Capability token 作为一个整体进入队列或直接交付；
- 发送失败时，调用者仍持有全部 token，且没有 Message 被创建；
- 接收成功时，Message 删除与全部 token 移入接收方是同一原子迁移；
- token 的目标可在传输期间销毁，因此“原子交付”不等于“接收时仍可用”。

### 17.6 Timer 安全

- `deadline ≤ now` 的新 Timer 在 `timer_set` 完成时已处于 `Fired`；
- 任意可达状态中不存在已过期的 `Pending` Timer；
- 一个 Timer 最多产生一次内核通知；
- `Cancelled` Timer 之后不会产生通知；
- Exactly-once 不蕴含最终调度或最终消费。

### 17.7 Exception 安全

- 每个 ExceptionRaised 事件的 `exc_source_id` 指向活动 ExceptionSource；
- 异常通知的来源不可伪造——`MessageOrigin = FromKernel(ExceptionRaised(...))` 只能由 `raise_exception` 产生；
- `exc_subscribe` 只在调用者订阅异常源时产生，订阅不授予订阅者对异常源的操作权限；
- 未订阅异常源的 Task 不会收到该源的异常通知；
- 已订阅异常源的 Task 销毁时，该 Task 自动从所有异常源的 `subscribers` 中移除；
- 异常源销毁不产生额外的异常通知——已排队的通知保留，但不会再产生新通知。

### 17.8 设备隔离

- 只有持有 MMIO 区域 Capability 的 Task 可以映射该区域到自己的地址空间；
- I/O port 访问在 Task 切换时由 COSIR 细化层基于持有的 IOPortRange Capability 授权；
- DMA 缓冲区页帧被隔离于通用页池——不能通过错误的 `memseg_create` 或 Segment 操作意外访问；
- IRQ 订阅通过 ExcSource 机制保护——只有持有 `DevInterruptSubscribe` 的 Task 可通过 `exc_subscribe` 订阅对应 IRQ 源；
- 设备销毁时，所有与该设备相关的权威资源被释放且不可被原 Capability 重新激活。

## 18. 从抽象规范到实现的细化要求

具体 Rust / COSIR 实现至少要补充并验证：

1. opaque ID 的不可伪造表示，例如内核表索引加 generation 或受保护句柄；
2. 有限整数溢出、地址回绕和页对齐检查；
3. 页表更新、TLB shootdown、R/W/X 与用户/内核权限位，以及页帧复用前的清零；
4. 中断与系统调用边界上的原子性；
5. SMP 下锁、线性化点和内存顺序；
6. DMA 设备受 IOMMU 或驱动能力约束；
7. 消息、Audit、对象表和 Capability 表的资源上限；
8. 崩溃、断电和持久化是否属于状态机；
9. 错误码优先级和稳定 ABI；
10. 对本状态机的 refinement relation；
11. MMIO 映射与普通 Segment 映射的区分，以及页表级别的缓存策略设置（PAT/MTTR）；
12. I/O port bitmap 的 per-Task 切换和特权级隔离；
13. IOMMU domain 的创建、设备 assignment、DMA 地址翻译和故障处理；
14. 设备枚举信息的传递与平台抽象；
15. Cache coherence 保证（coherent vs non-coherent DMA）和 fence 指令放置；
16. 异常帧格式和 `exc_resume` 的参数与语义：已由 §12.10 定义并有 host `MODEL-TEST`；真实 trap 返回接入与形式化验证仍待完成；
17. 异常源与中断控制器的集成（APIC/LAPIC 屏蔽、EOI、优先级）；
18. SMP 核间同步：spinlock/qspinlock 实现、per-CPU 数据结构、RCU 或等效的无锁读路径、核间 TLB shootdown 协议（IPI 发送与确认）；
19. 多物理内存池管理：GPU 显存帧池 (`gpu_frame_universe`) 与 CPU 帧池的隔离、PCIe BAR 映射一致性、GPU unified memory 页迁移（HMM）与 CPU-GPU 缓存一致性协议；
20. GPU kernel 调度：CUDA stream / Vulkan command buffer 的接口抽象、kernel 提交与完成中断的集成、GPU 多 stream 并发的安全隔离；
21. 跨节点网络传输：Capability 序列化/反序列化格式、网络协议（TCP/RDMA/自定义 L4）、`InTransit` 的超时与回退策略、节点发现与健康检查协议；
22. 物理页帧分配器的数据结构选择（buddy allocator / bitmap / free list）及其对碎片化和分配延迟的影响；
23. ID 计数器溢出后的内核行为（panic / 返回 `ResourceExhausted` / 其他），必须在 COSIR 细化层定义，不能依赖回绕语义；
24. `MAX_MSG_SIZE`、`MAX_CAPS_PER_MSG`、`MAX_PATH_COMPONENTS` 等常量的具体值，根据目标平台的缓存行大小、内核内存预算和性能目标确定；
25. Audit 日志的配额、保留期、丢弃证明和查询权限——抽象审计模型无界且消息不记明文 payload，具体实现必须定义容量边界。

在完成这些细化前，不得根据本文直接声称”实际内核已形式化证明安全”。准确表述应是：抽象对象能力状态机具有待证明的安全不变量，具体实现需要另行证明其正确细化。

### 18.0 POSIR/COSIR 边界清单

以下是 POSIR 抽象规范中提及但**故意不定义**、留待 COSIR 细化层实现的具体项目汇总：

| #   | 项目                              | POSIR 中的引用    | COSIR 细化层职责                                                                                     |
| --- | ------------------------------- | ------------- | ----------------------------------------------------------------------------------------------- |
| 1   | 异常帧格式与寄存器保存/恢复                  | §12.9, §12.10 | x86_64 `0xA0` little-endian 帧由 Segment bytes 提供，恢复目标取帧内 RIP；已有 host `MODEL-TEST`，真实 trap 返回仍待实现 |
| 2   | `exc_resume` / 异常返回原语           | §12.9, §12.10 | 已定义为 §12.10 三参数原语并有 host `MODEL-TEST`；`KERNEL-TRACE` 与 `REFINEMENT-PROOF` 尚无                    |
| 3   | syscall 调用/返回协议                 | §12.2         | syscall number 编码、参数寄存器、返回寄存器、错误码编码                                                             |
| 4   | ID 的不可伪造表示                      | §3, §18       | opaque 句柄编码（索引 + generation 或受保护指针）                                                             |
| 5   | 页表格式（PML4/TTBR/Sv39）            | §7            | 多级页表遍历、权限位编码、W^X 策略                                                                             |
| 6   | TLB 维护（INVLPG/TLBI/DSB）         | §7            | 单页或全 TLB shootdown、ASID 管理                                                                      |
| 7   | 中断控制器编程（APIC/GICv3）             | §12, §15.7    | 中断路由、屏蔽、EOI、优先级                                                                                 |
| 8   | Timer 时钟硬件（LAPIC/Generic Timer） | §10           | 一次性和周期性模式、校准、频率                                                                                 |
| 9   | MMIO 的缓存策略（PAT/MTTR/MAIR）       | §15.4         | WC/UC/WB 内存类型、fence 指令放置                                                                        |
| 10  | I/O port 隔离（IOPL/TSS bitmap）    | §15.5         | per-Task bitmap 配置、#GP 触发                                                                       |
| 11  | IOMMU domain 和设备 assignment     | §15.6         | DMA 地址翻译、故障处理、设备隔离                                                                              |
| 12  | 页帧分配器数据结构                       | §7.1          | buddy/bitmap/free list 选择                                                                       |
| 13  | Audit 持久化与容量                    | §13           | 日志配额、保留期、丢弃证明、查询权限                                                                              |
| 14  | GPU kernel 提交接口                 | §8.6          | CUDA stream / Vulkan command buffer 抽象                                                          |
| 15  | 跨节点网络传输协议                       | §8.5          | 序列化格式、传输协议、超时策略、节点发现                                                                            |
| 16  | Task 入口的 COSIR 细化处理             | §5.2          | 对 GPU Task 的 entry 解释为 kernel 句柄、对 CPU Task 解释为虚拟地址                                             |

若在实现过程中发现需要在 POSIR 层面新增机制才能完成以上细化项中的某项，则该发现本身构成对 H3（小型可信机制层）或 H1（机制表达力）的潜在反例，必须记录在案。

### 18.1 跨架构可移植性

POSIR 抽象规范把满足以下 7 个条件的架构（von Neumann 架构或类似架构）列为候选适用范围：可寻址线性内存、特权级分离、地址转换硬件、有穷指令集、外部中断/异常、单调时钟、I/O 隔离边界（详见 [[Prima OS]] §17.1）。x86_64、ARMv8-A (AArch64) 和 RISC-V (RV64GC+M+S+U) 是待分别实例化和验证的候选架构；当前证据不能证明跨架构完全适用。

原语在跨架构可移植性上分为两类：

**候选逻辑无关原语**——设计上是纯数据结构，不接触架构特定寄存器或指令。当前假设这些原语可在 x86_64、ARM 和 RISC-V 之间保持状态迁移语义；是否无需重写须由第二架构实例化验证：

| 原语 | 涉及内容 | 可移植原因 |
|---|---|---|
| Capability | opaque ID 和 generation、CapRecord 链表、权限集合 | 纯数据结构；所有架构都有足够字长表达 |
| Task (生命周期/send/recv) | TaskId、mailbox 队列、TaskStatus 状态机、pending_return | 纯数据结构 |
| Send/Recv | 消息队列、HeldBy ↔ InMessage 转移、FIFO 调度 | 纯逻辑 |
| Namespace | Path 表和 Mount 图 | 纯数据结构 |
| Timer (抽象层) | 绝对 Deadline 队列、定时器状态机 | 纯逻辑 |
| Audit | 追加式事件日志 | 纯数据结构 |
| Bootstrap | 初始状态构建 | 纯逻辑 |

**架构特定原语**——需要直接操作硬件寄存器或处理器特定指令。候选设计要求 POSIR 状态迁移保持不变、COSIR 细化实现按架构重写；该要求仍待跨架构证据检验：

| 原语 | x86_64 细化 | ARMv8-A 细化 |
|---|---|---|
| MemorySegment (map/unmap) | 四级/五级页表、CR3 加载、INVLPG/INVPCID TLB invalidation、W^X 通过 NX 位 | 四级页表 (TTBR0/1)、TLBI (VA/ASID)、W^X 通过 PXN/UXN (AP[2:1])、DSB 确保页表可见 |
| Task (执行上下文) | `iretq` 进入/离开用户态、TSS 栈切换、RSP0 | `eret` 切换异常级别、SP_EL0/SP_EL1 自动换栈 |
| Exception (trap 入口) | IDT 注册硬件向量、`int`/`syscall` 指令、异常帧栈格式 | VBAR_EL1 异常向量表、`svc` 指令 (立即数编码 syscall 号)、ESR_EL1 解码异常原因 |
| Exception (page fault) | CR2/FAR 取故障地址、错误码编码 (P/W/U/RSVD/I/D 位) | FAR_EL1 取故障地址、ESR_EL1 的 ISS 字段编码 (DFSC/IFSC 故障码) |
| Timer (时钟硬件) | RDTSC 读时间、LAPIC Timer 编程/校准、One-shot/Periodic 模式 | CNTPCT_EL0 读时间、Generic Timer CTL/CVAL 寄存器、仅 One-shot 模式 |
| Device (MMIO 映射) | PAT/MTTR 内存类型 (WC/UC/WB)、SFENCE/MFENCE、VMX EPT (若虚拟化 I/O) | MAIR_EL1 内存属性、DSB/ISB fence、SMMU 用于 DMA 隔离 (IOMMU) |
| Device (IRQ) | LAPIC 本地中断、I/O APIC 路由、MSI/MSI-X (PCI 配置空间)、EOI 写 | GICv3 ICC 寄存器组 (SGIR/EOIR/CTLR)、ITS 表 (LPI/MSI 翻译) |

**ARM 细化层的主要结构性差异**：

| 差异点 | x86_64 | ARMv8-A |
|---|---|---|
| 页表遍历格式 | PML4 → PDPT → PD → PT → Page | L0 → L1 → L2 → L3 → Page（可选更多级别，按 TCR_EL1 配置） |
| TLB 维护范围 | 可选 INVLPG (单页) 或 INVPCID (带 tag) | 强制按 VA + ASID 逐页 shootdown |
| 中断译码 | 向量号（0-31 reserved、32-255 分配）直接索引 IDT | 异常级别 + 异常类型 (sync/irq/fiq/serror) × 来源 (el1 current/el0 lower) → VBAR_EL1 表 |
| 时间基准 | CPU 本地 TSC（可能跨核漂移） | 系统全局 CNTPCT（ARM Generic Timer 架构保证的一致性） |
| 指令屏障 | SFENCE/MFENCE/LFENCE | DSB (全系统) / DMB (内存域) / ISB (指令流) — 更细粒度控制 |

**ARM 候选实例化预计：**
- **7 类状态迁移候选不变**（Capability / Task 生命周期 / Send/Recv / Namespace / Timer 抽象 / Audit / Bootstrap）
- **4 类预计需要 COSIR 细化重写**（MemorySegment / Exception / Timer 硬件 / Device）
- 总代码量中，逻辑无关代码是否占多数取决于数据结构代码 vs 页表代码的比例，而不是原语类别的比例

第二架构实验将检验“不同架构的 COSIR 实例化能否独立于 POSIR 进行”。若需要修改 POSIR，则构成对 POSIR/COSIR 分层或候选适用范围的反例，而不是仍宣称跨架构完全适用。

## 19. 原语总览

本文按 **11 类 Bootstrap 后常规机制 + 1 个 Bootstrap 初始化迁移** 统计接口类别。§19 总览因此列出 12 项；其中 Bootstrap 是初始 Authority 的唯一来源，不是系统引导完成后的常规可调用操作。H1 中的“11 类”仅指 Bootstrap 后常规机制，不包含该唯一初始化迁移。

| 类别            | 公开操作                                                           | 核心作用                    |
| ------------- | -------------------------------------------------------------- | ----------------------- |
| Capability    | `derive / drop`                                                | 权限衰减、显式共享、token 清理      |
| MemorySegment | `create / map / unmap / destroy`                               | 页帧对象与显式地址空间映射           |
| Task          | `create / destroy / yield`                                     | 执行单元与生命周期               |
| Send          | `send`                                                         | 有授权的消息与 Capability 原子转移 |
| Recv          | `recv`                                                         | FIFO 接收与阻塞恢复            |
| Timer         | `set / cancel / destroy / now`                                 | 单调绝对时间与一次性内核通知          |
| Namespace     | `create / bind / unbind / mount / unmount / resolve / destroy` | 路径化 Capability 存储与委托    |
| Audit         | `audit`                                                        | 纯查询的派生与移动溯源             |
| Exception     | `exc_source_create / exc_subscribe / exc_unsubscribe / exc_source_destroy` | 异步事件订阅与硬件异常路由 |
| Device        | `dev_mmio_create / dev_mmio_map / dev_mmio_unmap / dev_ioport_create / dev_dma_create / dev_irq_create / dev_destroy` | 设备 MMIO、IRQ 和 DMA 的对象能力化封装 |
| Node          | `node_create / node_destroy`                                   | 跨节点通信节点对象              |
| Bootstrap     | `bootstrap`                                                    | 唯一的初始 Authority 来源      |

## 20. 细化层：MachineState 与抽象函数

本章属于 COSIR 细化层，不新增对 POSIR 可见的一等原语。它把 x86_64 机制层与 POSIR 抽象层的对应关系显式化（见 [[Prima OS]] §13.2），为阶段 1 真实内核实现（物理内存、页表/GDT/IDT/trap）提供细化蓝本。当前包含 **Bootstrap 投影**和 **MemorySegment host 四级索引树**两项设计；二者的证据等级均为 `REFINEMENT-DESIGN`，Rust 可执行状态模型的测试仅为 `MODEL-TEST`，不构成真实 CR3/PTE 实现的细化证明。task_create 入口细化和异常/trap 递送仍只登记接口，留待后续细化切片展开。

### 20.1 MachineState

`MachineState` 描述 bootloader 把控制权交给内核那一刻的 x86_64 可观察机器状态（交接点：long mode、分页开、CR3/GDT/TSS/栈有效、已收到 `BootInfo`）。它是可执行状态模型探针（`cosir`）中细化域的起点。

```text
MachineState {
    cr3:             Cr3              // 根页表物理地址
    address_space:   List<MapEntry>   // 简化页表视图；(vaddr, size, access{R,W,X})
    gdt:             GdtTable         // 基址/限长
    tss:             Tss              // 处理器状态段（含 RSP0 中断栈）
    rip:             VirtAddr         // kernel 入口
    rsp:             VirtAddr         // 已设栈顶
    boot_info:       BootInfoHandle   // 交接的 BootInfo 抽象（内存区域）
    idt_ready:       Bool             // IDT 是否已装载（交接点通常尚未）
    interrupt_state: InterruptState   // masked / unmasked
    booted:          Bool             // 细化层可见：bootstrap 是否已执行（W14 唯一性）
}
```

- 页表用简化 `MapEntry` 视图（`(vaddr, size, access)`），**不建模真实多级 PML4 遍历**——真实遍历属于阶段 1 页表/trap 框架工作（S1-D4）。
- 不建模 GPR 全集；只建模对 Bootstrap 有意义的状态（rip/rsp/表/地址空间）。真实寄存器保存/恢复属于异常帧细化（§18.0 边界清单 #1）。
- `MapEntry.access` 复用本规范的 R/W/X 访问位域并在探针中沿用 `cosir::state` 常量。

### 20.2 抽象函数 abstract

```text
abstract : MachineState → POSIRState
```

抽象函数把机器态投影为 POSIR 抽象状态，遵循两段投影：

1. 交接点且 `booted = false` → 投影为 POSIR `EmptyState`（无任务、无能力、未 booted）；
2. Bootstrap 迁移后（`booted = true`）→ 投影为含 init Task 的 booted 态。

抽象函数只做**投影**，不做安全判定：机器内部结构（页表布局细节、GDT 具体槽位等）不进入抽象态；安全判定仍由 POSIR 不变量与后续真实实现保证。

### 20.3 Bootstrap 细化对应

按 §14 的自定义实例化（唯一的无 Task Actor 铸造初始 Authority 的迁移）：

| POSIR 抽象（§14） | x86_64 细化 |
|---|---|
| `EmptyState` | 交接点机器态：`kernel_main` 接管、无用户 Task、无能力表 |
| `bootstrap` 迁移 | 内核固化 bootstrap 路径：铸造初始 Authority |
| 初始 Root Capability 铸造至 init Task | 交接点建立 init 上下文：初始栈、入口、内存视图 |
| `AlreadyBooted` 幂等保护 | 细化层保持 `MachineState.booted`，重复 bootstrap 不重复铸造 |
| init 只获定义初始 Authority | 交接点后 init 的能力快照是 ObjectRef/CapRecord 的机器投影 |

细化层不变量（仅细化层可见，不新增 POSIR 原语）：

- **W14 唯一性细化**：`MachineState.booted` 与 POSIR `booted` 一一对应，交错不违背唯一性；
- **初始 Authority 单调**：α 投影后 init Task 的能力集恰好等于 §14 定义的初始 Authority，无多余对象（对 S1-X2 的 α 层承接）；
- **投影保真**：`α(booted) ≠ α(pre_bootstrap)`，即 bootstrap 确改变抽象态。

### 20.4 范围边界与证伪

本切片已定义 MachineState、抽象函数 α、Bootstrap 细化对应；**待证明**：α 对全部 POSIR 状态成立（后续切片补覆盖）、真实实现满足 α（S1 之后真实 kernel 对照）、形式化验证（BLK-001）。若实现中发现需在 POSIR 层新增机制才能完成 Bootstrap 细化，构成对可信机制层小型性（H3）/机制表达力（H1）的潜在反例，须按 §18.0 末尾规则记录。

### 20.5 MemorySegment host 四级索引树 ↔ vspace refinement design

本小节给出细化层设计：把 MemorySegment 的 host 四级索引树与 vspace 的对应关系显式化（对应 §18.0 边界清单 #5/#6）。不新增对 POSIR 可见的一等原语；`pagewalk` 与 `memseg_refine` 是 host 可执行模型中的 `REFINEMENT-DESIGN` 探针，为阶段 1 页表/trap（S1-D4）与 vspace 管理（S2）提供候选蓝本，不是真实 x86_64 CR3 页表实现。

**host 四级索引树（仅 4KB，四级固定走到底）。** 每 CPU Task 在 host 模型中持有一棵四级、每级 512 槽的索引树，最终指向 4KB 叶。层级形状和叶权限字段借鉴 x86_64，但既不表示真实 CR3，也不编码真实页表页或 PTE。候选字段对应如下：

| POSIR Access | x86_64-inspired candidate encoding |
|---|---|
| R | `present`（候选只读由 `writable=false` 表达） |
| W | `writable` |
| X | `!nx` |
| 用户态可达 | `user` |

**host 叶双视图。** 模型叶 `Pte{frame_phys, mapping}` 是为检查物理地址视图与 POSIR 引用视图一致性而定义的 host 记录；`Pte` 名称及 `present`/`writable`/`nx`/`user` 字段只是 x86_64-inspired candidate encoding，不是真实 CR3/PTE 或其位级布局。`mapping = SegmentRef{segment, offset, access}` 是 §5.2 `PageMapping` 的模型视图。`state` 简化 `vspace`（exc_resume 探针的 `(VirtAddr, u8)` tuple）与本切片无关。

**四操作候选细化对应。**

| POSIR 抽象（§7） | host refinement design |
|---|---|
| `memseg_create` | 从 `free_frames` 取 `page_count` 帧（W6），建 Segment，铸造并返回根 SegCap（成功结果，非创建前置 Capability）；不建树 |
| `memseg_map` | 前置校验后遍历树到底插入叶 `Pte{frame_phys, mapping}`，同步 vspace 视图（W7） |
| `memseg_unmap` | 前置校验（含 W8 不卸 entry 页）后清叶、回收空表结点、同步 vspace |
| `memseg_destroy` | 前置校验（无活动 vspace 引用，StillMapped）后删 Segment，归还帧（W6） |

**不变量承接。** `MODEL-TEST` 检查 W6（物理页唯一归属：活动 segment 帧与页表 backing 帧两两不交、与 free_frames 不交、并集 = frame_universe；页表帧归属由 `FrameAllocator::check_invariants` 统一校验，纳入后守恒为 segment 帧∪页表帧∪空闲帧）以及 W7（vspace host 视图与树叶映射集一致、每 VirtPage 单映射、映射指向活动 Segment 且 access≤allowed_access）。W8 当前只覆盖 `memseg_unmap` 的 entry-page guard；尚未验证 task_create 入口建立、入口权限合成或执行期间入口持续可达，因此不得称为完整 W8 验证。

**错误。** 沿用规范错误集（`MissingMapRight`/`RightsExceeded`/`OutOfBounds`/`Overlap`/`NotMapped`/`EntryMapping`/`StillMapped`/`ZeroSize`/`NoMemory`/`InvalidSegment`/`InvalidTask`/`MissingRight`）；页表遍历内的"路径深表缺失"等机制内部状态折叠为对应 POSIR 错误，不外泄新错误原语。

**未覆盖（留后续切片）。** 当前 host 四级索引树和双视图检查不再覆盖的包括：非叶权限合成、TLB 维护、多核并发与 shootdown、真实 CR3/PTE 编码和访问，以及页表原子更新窗口。**页表页的物理身份与所有权**在细化模型中已覆盖为 `new_owned_task_table`/`map_vaddr_owned`/`unmap_vaddr_owned` 的 backing frame 分配/回收与 `check_frame_ownership`（见 SPEC-035/036），但真实内核沿现有 CR3 可达页表树的审计（SPEC-037/038）仅确认外部根页帧归属，**不证明子页表物理所有权**，故真实内核自建页表的所有权与回收仍留后续切片。大页（2MB/1GB）多粒度、task_create 入口细化（边界清单 #16）、异常/trap 递送（S1-D4）、叶 `mapping` 额外字段在真机 PTE 的落地同样未覆盖。以上边界完成前，证据保持 `REFINEMENT-DESIGN` + `MODEL-TEST`，不得称为真实 MemorySegment refinement PASS。
