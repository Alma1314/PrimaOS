# 双人格 Holder 模型设计草案：AddressSpace / CapTable / ShareGroup / Thread

> 文档类型：设计草案（非冻结规范、非实现证据）
>
> 日期：2026-10-14
>
> 输入：[[docs/superpowers/reports/2026-10-14-cosir-dual-personality-adoption-review|双人格扩展候选采纳审核]]、[[docs/superpowers/reports/2026-10-14-cosir-dual-personality-gap-analysis|COSIR/POSIR 双人格底座差距分析]]、[[COSIR 原语规范]] §5.2/§8.1/§16/§17/§21/§22
>
> 状态：待评审；本草案不修改 [[COSIR 原语规范]] 的冻结语义。

## 0. 结论先行

当前 `Task` 同时承担执行上下文、`vspace`、Capability holder、邮箱、根 Namespace 和计算介质，无法表达 Windows Process/Thread 或 Linux `clone` 的共享组合。

**推荐目标模型：**

```text
Thread / Task  = 执行上下文 + 调度状态 + 邮箱 + 等待/upcall/fault 状态
ShareGroup     = 进程级共享域：AddressSpace + CapTable + root_ns_cap + signal_state
AddressSpace   = 独立一等对象：VirtPage ⇀ PageMapping / DeviceMapping
CapTable       = 独立一等对象：Capability 的持有者位置
```

其中：

- Windows Process ↔ `ShareGroup`；Windows Thread ↔ CPU `Task`/`Thread`；
- Linux pthread ↔ 新 `Task`/`Thread` 加入同一 `ShareGroup`；
- Linux `fork` ↔ 新 `ShareGroup` + 复制 `AddressSpace` + 复制 `CapTable`；
- `CLONE_VM` ↔ 新 `ShareGroup` 引用同一 `AddressSpace`；
- `CLONE_FILES` ↔ 新 `ShareGroup` 引用同一 `CapTable`；
- `CLONE_THREAD`/`CLONE_SIGHAND` ↔ 新 `Task`/`Thread` 加入同一 `ShareGroup`。

**关键结论：** 不能只给 `task_create` 加几个共享参数；必须先把 Capability holder 从 `TaskId` 一般化为 `CapTableId`（或 `ShareGroupId`），否则 W3/W4 无法表达共享句柄表。

## 1. 问题定义

### 1.1 当前模型

```text
Task {
  kind, node, status,
  vspace, dev_vspace, entry,
  root_ns_cap, mailbox,
  pending_return, wait_request, pending_wait_result, pending_upcalls,
  pending_exc, kernel_resume_frame
}
```

Capability 位置：`HeldBy(TaskId)`。

### 1.2 缺口

| 需求 | 当前模型能否表达 |
|---|---|
| Windows 同进程多线程共享地址空间和句柄表 | 不能：`vspace` 和 Capability holder 都绑定单个 Task |
| Windows `CreateProcess` / `CreateThread` | 不能：`task_create` 总是创建空地址空间并移动 `initial_caps` |
| Linux `pthread_create` | 不能：没有“线程加入同一共享域”的概念 |
| Linux `fork` | 不能：没有复制/共享地址空间与句柄表的显式模型 |
| Linux `CLONE_VM` / `CLONE_FILES` 任意组合 | 不能：`vspace` 与 Capability 表被绑定在同一 Task 上 |
| Windows 进程终止 → 全部线程终止 | 不能：Task 生命周期彼此独立 |
| `wait`/`waitid` 等进程级退出语义 | 只能部分用 `TaskExit` 订阅组合 |

### 1.3 为什么不能只扩展 `task_create`

如果两个 Task 共享 Capability 表，则同一个 `CapId` 会被两个 `HeldBy(Task)` 同时持有，直接违反 W3（单一位置）与 §4.3/§17.2。因此 holder 模型必须先于 Process/Thread 操作冻结。

## 2. 需求映射

### 2.1 Windows NT

- **Process：** 私有地址空间、进程级句柄表、访问令牌、线程集合、进程级退出状态。
- **Thread：** 执行上下文、内核栈/用户栈、调度状态、TEB、可警报等待状态、APC 队列。
- 句柄进程内共享：一个线程关闭句柄影响同进程其他线程。
- `CreateProcess` 创建 Process + 主 Thread；`CreateThread` 在同一 Process 中新增 Thread。
- Thread 是句柄，可被等待；Process 也是句柄，可被等待。
- 线程终止独立于进程；主线程退出不自动杀死进程。
- 终止进程会使全部线程退出。

### 2.2 Linux / POSIX

`clone` 的共享标志必须能独立组合：

| flag | 共享/复制对象 | 目标模型对应 |
|---|---|---|
| `CLONE_VM` | 地址空间 | 共享 `AddressSpace` |
| `CLONE_FILES` | 文件描述符表 | 共享 `CapTable`（或 LibOS fd table 映射） |
| `CLONE_FS` | root/cwd/umask | LibOS/`ShareGroup` 扩展 |
| `CLONE_SIGHAND` | 信号处理表 | `ShareGroup.signal_state`（需与 LibOS 协议配合） |
| `CLONE_THREAD` | 线程组 / tgid | 同一个 `ShareGroup` |
| 无上述标志 | fork 语义 | 新 `ShareGroup` + 复制引用对象 |

`fork` 的精确语义是：地址空间复制、fd 表复制但打开文件描述共享；`vfork`/`CLONE_VM` 共享地址空间；`pthread_create` 共享全部。目标模型应至少能表达这些常见组合，再评估是否支持完整 `clone` 位图。

### 2.3 Prima 约束

1. **OS 中立：** 不引入 `Process`/`Thread`/`Windows` 专属语义到 POSIR 对象名；使用 `ShareGroup`、`AddressSpace`、`CapTable`、`Task`/`Thread`。
2. **不破坏 W3/W4：** holder 必须支持“同一 token 只在一个位置”的一般化表达。
3. **不破坏 W5：** 结构性根 Namespace 应可由同组线程共享。
4. **不破坏 W9/W17：** `Task` 仍是调度/阻塞/等待/upcall 的单位。
5. **兼容 GPU/NPU：** 非 CPU Task 不能强行套用“线程组 + 用户地址空间”语义；需要保留 `ComputeTask` 分支或为它们创建退化 `ShareGroup`。
6. **可迁移性：** 让单线程任务和现有实现先以私有 `AddressSpace`/`CapTable` 运行，再逐步打开共享。

## 3. 候选模型对比

### 模型 A：直接拆 `Process` 与 `Thread`

```text
Process { vspace, cap_table, root_ns_cap, threads }
Thread  { process, execution, mailbox, ... }
```

- 优点：Windows 映射直观。
- 缺点：`HeldBy(Process)` 与当前 `HeldBy(Task)` 差异大；Linux `CLONE_VM`/`CLONE_FILES` 独立组合无法只靠一个 `Process` 表达；需要同时重构两个对象。

### 模型 B：`ShareGroup` + `Thread`（推荐起点）

```text
ShareGroup { address_space, cap_table, root_ns_cap, signal_state }
Thread     { share_group, execution, status, mailbox, wait/upcall/fault }
```

- 优点：中立；Windows/Linux 常见组合可映射；`Thread` 保持调度/阻塞单位。
- 缺点：若 `address_space` 与 `cap_table` 内联在 `ShareGroup`，仍无法表达 `CLONE_VM` 与 `CLONE_FILES` 的独立组合。

### 模型 C：`ShareGroup` + 独立 `AddressSpace` / `CapTable`（推荐目标）

```text
AddressSpace { mappings, dev_mappings, creator, ... }
CapTable     { creator, ... }
ShareGroup   { address_space_ref, cap_table_ref, root_ns_cap, signal_state, ... }
Thread/Task  { share_group, execution, status, mailbox, wait/upcall/fault }
```

- 优点：`CLONE_VM` 只共享 `AddressSpace`，`CLONE_FILES` 只共享 `CapTable`；Windows Process 可拥有私有二者；`fork` 可复制二者；`pthread_create` 共享二者。
- 缺点：改动最大，必须重写 W3/W4/W7/W9 的引用位置与大量操作的前置检查。

**推荐路线：** 以 C 为目标，但分阶段实现；先引入 `ShareGroup` + `CapTable`，再引入独立 `AddressSpace`，最后开放任意 clone 标志组合。

## 4. 推荐目标模型

### 4.1 对象与所有权

```text
AddressSpaceId, CapTableId, ShareGroupId : Nat

AddressSpace {
    creator:       Actor,
    mappings:      VirtPage ⇀ PageMapping,
    dev_mappings:  VirtPage ⇀ DeviceMapping,
}

CapTable {
    creator: Actor,
    // Capability 的位置由全局 locations 表指向 HeldByTable(CapTableId)
}

ShareGroup {
    creator:          Actor,
    address_space:    AddressSpaceId,
    cap_table:        CapTableId,
    root_ns_cap:      CapId,
    signal_state:     Option<...>,   // LibOS 解释；POSIR 可先仅保留引用
    exiting:          Bool,
}

Task {   // CPU Task 同时承担 Thread 语义；非 CPU Task 保留 ComputeTask 分支
    creator:       Actor,
    kind:          TaskKind,
    node:          NodeId,
    share_group:   Option<ShareGroupId>,   // CPU Task 必须非 None；非 CPU 见 §7
    status:        TaskStatus,
    entry:         VirtAddr,
    mailbox:       List<MsgId>,
    pending_return:      Option<RecvResult>,
    wait_request:        Option<WaitRequest>,
    pending_wait_result: Option<WaitResult>,
    pending_upcalls:     List<Upcall>,
    pending_exc:         Option<PendingExc>,
    kernel_resume_frame: Option<Bytes>,
}
```

说明：

- `Task` 不再直接持有 `vspace`/`dev_vspace`；对 CPU Task，映射位于 `ShareGroup.address_space`。
- `root_ns_cap` 移到 `ShareGroup`；结构性根由同组线程共享。
- `CapTable` 是 Capability 的 holder；一个 `CapTable` 可被多个 `Task` 通过 `ShareGroup` 共享。
- `Task` 仍是 `Running`/`Ready`/`BlockedRecv`/`BlockedWait`/`Faulted`、`recv`、`wait_objects`、`task_alert` 的单位。

### 4.2 Capability 位置一般化

```text
CapLocation =
    HeldByTable(CapTableId)
  | InMessage(MsgId)
  | InNamespace(NsId, Path)
  | InTransit(...)
```

- W3：每个 `CapId` 至多一个位置；
- W4：`HeldByTable(ct)` 要求 `ct` 是活动 CapTable；容器与位置双向一致；
- 单线程兼容：新 Task 创建时自动铸一个私有 `CapTable`，因此旧 `HeldBy(task)` 语义可投影为 `HeldByTable(cap_table_of(task))`；
- `cap_derive`/`cap_drop`/`send`/`recv`/`ns_bind` 等操作把“调用者”解析为 `HeldByTable(caller_share_group.cap_table)`；
- `task_create`/`thread_create` 的 `initial_caps` 从调用者所在 CapTable 移动到新 Task 的 CapTable 或共享 CapTable。

**开放问题：** 是保留 `HeldBy(Task)` 作为单线程特例，还是全量迁移到 `HeldByTable`？建议正式规范采用 `HeldByTable`，实现内部可保留 Task 快捷投影。

### 4.3 AddressSpace 一般化

- `memseg_map` / `memseg_unmap` / `memseg_protect` 的目标从 `task_cap` 改为 `share_group_cap` 或独立的 `AddressSpace` Capability；
- `TaskManageVSpace` 权限可以放在 `ShareGroup` 或 `AddressSpace` 对象上；
- W7 改为约束每个活动 `AddressSpace.mappings`；
- W8 改为约束每个活动 CPU Task 的 `entry` 在其 `ShareGroup.address_space` 中可执行；
- `task_destroy` 不再删除 `vspace`，而由 `ShareGroup` 生命周期管理；
- GPU/NPU 的显存池模型暂不由该 AddressSpace 承载。

### 4.4 操作草案

```text
address_space_create() -> address_space_cap
cap_table_create() -> cap_table_cap

share_group_create(
    address_space_cap,
    cap_table_cap,
    parent_ns_cap,
    root_rights
) -> share_group_cap

thread_create(
    share_group_cap,
    code_seg_cap,
    code_offset,
    code_pages,
    load_base,
    entry,
    initial_caps
) -> task_cap

thread_exit()
share_group_terminate(share_group_cap)
share_group_wait(share_group_cap, timeout) -> ExitStatus
```

Linux profile 映射：

```text
pthread_create   → thread_create(同 share_group_cap)
fork             → address_space_dup + cap_table_dup + share_group_create + thread_create
CLONE_VM         → 共享 address_space_cap
CLONE_FILES      → 共享 cap_table_cap
CLONE_THREAD     → 同 share_group_cap
vfork            → 共享 address_space_cap 的受限形式；等待/恢复协议需单独设计
```

Windows profile 映射：

```text
CreateProcess    → address_space_create + cap_table_create + share_group_create + thread_create
CreateThread     → thread_create(同 share_group_cap)
TerminateProcess → share_group_terminate
TerminateThread  → task_destroy 的线程级变体
```

## 5. 对现有不变量的影响

| 不变量 | 影响 |
|---|---|
| W3 Capability 单一位置 | `HeldBy` 改为 `HeldByTable`；共享 CapTable 是合法 holder |
| W4 容器与位置一致 | 新增 CapTable 活动性约束；`InMessage`/`InNamespace` 不变 |
| W5 结构性根 Namespace | 从 Task 移到 ShareGroup；同组线程共享 |
| W6/W7/W8 | `vspace` 移到 AddressSpace；W7/W8 引用更新 |
| W9 调度状态 | 仍按 CPU Task 计数；新增“一个 ShareGroup 至少一个活动 Task？”等组级规则 |
| W13 Audit | 新增 ShareGroup/AddressSpace/CapTable 生命周期与位置变更事件 |
| W16 | `dev_vspace` 跟随 AddressSpace；设备映射仍按 AddressSpace 约束 |
| W17 | `Task` 等待/upcall 仍在 Task；`task_destroy` 不能直接清空共享 CapTable |

## 6. 迁移阶段

### Phase H0 — 设计冻结

- 评审本文档；
- 决定 holder 是 `HeldByTable` 还是 `HeldByShareGroup`；
- 决定 AddressSpace 与 CapTable 是否都必须独立一等对象；
- 冻结 `fork`/`pthread_create`/`CreateProcess`/`CreateThread` 的目标 profile。

### Phase H1 — AddressSpace / CapTable 私有化

- 新增 `AddressSpace`、`CapTable` 对象；
- 每个现有 CPU Task 自动获得私有 `AddressSpace` 与 `CapTable`，旧 `vspace`/`HeldBy(Task)` 作为投影；
- 先保持 W3/W4/W7 原语义不变，只换内部载体；
- 更新 §20 细化设计与 host model。

### Phase H2 — ShareGroup 与 Thread

- 新增 `ShareGroup`；
- `task_create` 仍可用于单线程；新增 `thread_create` 加入已有 ShareGroup；
- `root_ns_cap` 移到 ShareGroup；
- 更新 W5/W9/W13/W17。

### Phase H3 — 共享与复制操作

- `address_space_dup`、`cap_table_dup`、`share_group_create`；
- 打开 `CLONE_VM`/`CLONE_FILES`/`CLONE_THREAD` 组合；
- 增加 `share_group_terminate`/`share_group_wait`；
- LibOS 层实现 fork/pthread/CreateThread 映射。

### Phase H4 — 双人格 profile 验证

- Linux `COMPAT-TEST`：多线程共享地址空间、futex/wait、fork 复制；
- Windows 侧机制验证：同句柄表、线程 APC/alertable wait、进程终止；
- 不宣称完整 Windows/Linux ABI 兼容。

## 7. GPU / NPU / FPGA 兼容

非 CPU Task 当前不进入 `BlockedRecv`/`BlockedWait`，也不处理 page fault；它们不能被强行当作线程。

候选方案：

1. **私有退化 ShareGroup：** 为每个非 CPU Task 创建只含 AddressSpace=∅ 和私有 CapTable 的 ShareGroup；
2. **保留直接 Capability holder：** 非 CPU Task 仍可 `HeldByTable` 指向私有 CapTable，不要求 vspace；
3. **ComputeTask 分支：** `Task.kind` 决定是否解释 `share_group`，非 CPU 只使用 `cap_table`。

建议采用方案 1+2 的组合：所有 Task 都通过私有或共享 CapTable 持有 Capability；`AddressSpace` 对非 CPU Task 可以为空或仅显存映射，由细化层解释。

## 8. 开放问题

1. `CapTable` 与 `ShareGroup` 的关系：CapTable 是否可脱离 ShareGroup 独立共享？推荐是，以支持 `CLONE_FILES`。
2. `AddressSpace` 是否可脱离 ShareGroup 独立共享？推荐是，以支持 `CLONE_VM`。
3. `root_ns_cap` 属于 ShareGroup 还是 CapTable？推荐 ShareGroup；若 `CLONE_FILES` 共享 CapTable 但 root 不同，需要两者分离。
4. `fork` 时 CapTable 复制语义：是批量派生新 Capability，还是复制 `CapRecord`/位置表？必须保持 W2/W3 且不隐式扩大权限。
5. `TaskSend` 的目标是 Task 还是 ShareGroup？Windows 句柄表为进程级，可能需要在 holder 重构后增加“向组内任意线程投递”的语义。
6. `task_alert` 仍按线程/Task 定向；不移到 ShareGroup。
7. `wait_objects` 是否支持等待 `ShareGroup`/`Thread` 对象？Windows waitable 包括 process/thread 句柄，需要新增 WaitObject 目标类型或映射到 WaitObject。
8. GPU/NPU Task 是否参与 `ShareGroup`/`CapTable` 或保留私有退化物。
9. 是否保持 `TaskId` 作为对外任务 ID，还是引入 `ThreadId`；建议先保持 `Task` 名，profile 层再暴露 `Thread`/`Process` 语义。
10. 迁移期间 `HeldBy(Task)` 与 `HeldByTable(CapTable)` 能否共存；若共存，必须证明两者不产生双位置窗口。

## 9. 证据与边界

- 本文是设计草案，不改变 [[COSIR 原语规范]] 冻结语义；
- 不产生实现、QEMU、Lean 或 `COMPAT-TEST` 证据；
- Phase H0 评审通过前，不应把本文当作双人格完整方案；
- 一旦确定目标模型，应先做规范修订（W3/W4/W5/W7/W9/W13/W17 与操作集合），再进入 host 模型和实现。
