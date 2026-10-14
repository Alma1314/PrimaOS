# 双人格 Holder 模型 H0 评审报告

> 文档类型：设计评审报告（非冻结规范、非实现证据）
>
> 日期：2026-10-14
>
> 评审对象：[[docs/superpowers/specs/2026-10-14-dual-personality-holder-model-design|双人格 Holder 模型设计草案]]
>
> 输入：[[COSIR 原语规范]] §5.2/§8.1/§16/§17/§21/§22、[[docs/superpowers/reports/2026-10-14-cosir-dual-personality-adoption-review|双人格扩展候选采纳审核]]、[[docs/superpowers/reports/2026-10-14-cosir-dual-personality-gap-analysis|双人格底座差距分析]]
>
> 结论：**条件通过**。目标架构方向成立；在进入 H1 规范细化前，必须先完成下方“通过条件”中的 6 项设计修订。

## 1. 评审准则

| # | 准则 | 判定问题 |
|---|---|---|
| J1 | 双人格表达能力 | 是否同时覆盖 Windows Process/Thread 与 Linux fork/pthread/clone 常见组合 |
| J2 | POSIR 不变量 | 是否保持 W3/W4/W5/W7/W9/W13/W17 的可表达性与可证明性 |
| J3 | OS 中立性 | 是否避免把 Windows/Linux 专属对象名与语义写入 POSIR |
| J4 | 机制最小性 | 是否以最小对象/权限集合表达共享与复制 |
| J5 | 生命周期闭合 | 对象销毁、线程退出、进程终止、zombie/wait 是否有清晰归宿 |
| J6 | 可迁移性 | 单线程 Task、GPU/NPU Task 与现有实现是否可渐进迁移 |

## 2. H0 决策

### H0-1 Holder 采用 `HeldByCapTable(CapTableId)`

**决定：通过。**

- 普通 Capability 的 holder 从 `HeldBy(TaskId)` 一般化为 `HeldByCapTable(CapTableId)`；
- `CapTable` 是独立内部结构对象；
- 一个 `ShareGroup` 引用一个 `CapTable`；
- 一个 `CapTable` 可被多个 `ShareGroup` 共享，以表达 Linux `CLONE_FILES`；
- `HeldBy(Task)` 不再是冻结语义的一部分；单线程实现可保留兼容投影，但规范必须全量迁移到 `HeldByCapTable`。

理由：只有 CapTable 与 AddressSpace 解耦，才能独立表达 `CLONE_FILES` 与 `CLONE_VM`。

### H0-2 `AddressSpace` 与 `CapTable` 独立但内部化

**决定：通过（修正设计草案）。**

- `AddressSpace` 与 `CapTable` 都必须是独立一等**内部结构对象**，拥有独立生命周期，可被多个 `ShareGroup` 引用；
- 第一版二者**不直接作为用户 Capability 目标**；
- 公开的进程/线程对象是 `ShareGroup`；`memseg_map`/`memseg_unmap`/`memseg_protect` 的目标改为 `share_group_cap`（具体权限名 H1 决定）；
- `CLONE_VM` 映射为共享同一 `AddressSpaceId`；`CLONE_FILES` 映射为共享同一 `CapTableId`。

理由：独立对象身份是表达 clone 标志组合的必要条件；内部化可避免 H1/H3 无谓扩大用户可见对象与权限。

### H0-3 每个 Task 强制拥有一个 ShareGroup

**决定：通过（修正设计草案）。**

- `Task.share_group: ShareGroupId` 为必填，不再使用 `Option`；
- CPU Task 的 ShareGroup 承载 `AddressSpace`；
- GPU/NPU/FPGA Task 使用私有退化 ShareGroup：`AddressSpace` 可为空或不参与 CPU vspace 语义，`CapTable` 私有；
- 第一版不允许非 CPU Task 与 CPU Task 共享同一个 ShareGroup。

理由：统一 Capability holder 解析路径，避免每个操作再分 `Task` 直接持有与 `ShareGroup` 持有两条路径。

### H0-4 结构性根 Namespace 使用 `HeldByShareGroup`

**决定：通过（新增位置类型）。**

设计草案把 `root_ns_cap` 简单移入 ShareGroup，但没有解决一个关键问题：`CLONE_FILES` 只应共享普通句柄表，不应意外共享 root namespace。

因此 H0 决定：

```text
CapLocation =
    HeldByCapTable(CapTableId)
  | HeldByShareGroup(ShareGroupId)    // 仅结构性根等不可转移 token
  | InMessage(MsgId)
  | InNamespace(NsId, Path)
  | InTransit(...)
```

- `ShareGroup.root_ns_cap` 位于 `HeldByShareGroup(sg)`，不进入共享 `CapTable`；
- W4 增加 `HeldByShareGroup(sg)` 要求 `sg` 活动；
- W5 改为：每个活动 ShareGroup 的 `root_ns_cap` 位于该 ShareGroup 的结构位置，目标和权限约束保持；
- `cap_drop`/`send`/`ns_bind` 等操作禁止移动结构性根 token 的规则从 Task 迁移到 ShareGroup。

理由：不把 root/cwd-like 状态与 fd/handle 表共享语义混在一起。

### H0-5 `fork` 的 CapTable 复制采用显式派生

**决定：通过，但需在 H1 细化权限与 Audit。**

- `fork` 的 CapTable 复制不是共享同一个 `CapTableId`，而是创建新 CapTable；
- 对原表中每个普通 Capability，内核创建新的 `CapId`：
  - `parent = 原 CapId`；
  - `rights = 原 rights`；
  - 目标对象不变；
- 该机制是 KernelActor/ShareGroup 的显式操作 `cap_table_dup`，不是用户 `cap_derive`；
- 其授权来自 ShareGroup 级 `GroupFork`（名称 H1 定）权限，而不是逐 Cap 的 `Derive`；
- W2 仍要求 `child.rights ⊆ parent.rights`（此处为相等），W13 增加复制/派生 Audit 事件。

**注意：** 这是一项需要 H1 明确证明的语义扩展。若坚持每 Cap `Derive`，POSIX fork 对不可派生句柄的兼容性会退化；H0 选择保留 fork 语义。

### H0-6 `TaskSend`/`send` 保持 Task/Thread 定向

**决定：通过。**

- 第一版不把 `send`/`task_alert` 改成组级广播；
- `send` 仍以 `Task` 为接收者，`task_alert` 仍按线程/Task 定向；
- Process/ShareGroup 的等待与终止通过独立操作表达，例如：

```text
share_group_wait(share_group_cap, timeout) -> ExitStatus
share_group_terminate(share_group_cap) -> ()
```

- 后续若 Windows 句柄语义需要通用 wait，可在 H3 将 Task/ShareGroup 投影为 WaitObject 目标，而不改 `send` 基本语义。

### H0-7 第一版 profile 范围冻结

**决定：通过。**

Linux 第一版覆盖：

- `fork`
- `pthread_create`
- `CLONE_VM`
- `CLONE_FILES`
- `CLONE_THREAD`

Windows 第一版覆盖：

- `CreateProcess`
- `CreateThread`
- `TerminateProcess`
- `TerminateThread`
- process/thread 句柄等待

以下项明确推迟，不阻塞 H1：

- 完整任意 `clone` flag 矩阵；
- `CLONE_FS`/`CLONE_SIGHAND`/`CLONE_NEW*` 的精确建模；
- pid namespace / pidfd / process group / session；
- 完整 Windows token、job、completion port 语义。

### H0-8 生命周期必须拆分 Thread 与 ShareGroup

**决定：通过，列为 H1 第一优先。**

当前 `task_create`/`task_destroy` 把线程与进程生命周期捆在一起。目标模型必须拆分：

```text
task_create / thread_create   // 创建执行上下文
thread_exit / task_destroy    // 终止单个执行上下文
share_group_terminate         // 终止组内全部 Task
share_group_wait              // 等待组退出
share_group_destroy           // 回收已终止且不再被等待的组
cap_table_dup / address_space_dup
```

H1 必须给出：

- 线程退出 vs 进程退出的状态迁移；
- zombie/exit status 的保存与回收；
- 最后一个线程退出时 ShareGroup 仍存活以支持 `wait` 的规则；
- `task_destroy` 与 `share_group_terminate` 的权限边界。

### H0-9 AddressSpace/CapTable 引用计数与回收

**决定：通过，列为 H1 第二优先。**

- 当没有活动 `ShareGroup` 引用 `AddressSpace` 时，回收该 AddressSpace；
- 当没有活动 `ShareGroup` 引用 `CapTable` 时，回收该 CapTable；
- CapTable 回收时，其中所有普通 Capability 必须进入明确的丢弃/失效路径，且生成 Audit；
- AddressSpace 回收时，必须移除映射并处理 `Segment` 引用/`StillMapped` 语义；
- W4 扩展为：`HeldByCapTable(ct)` 要求 `ct` 活动且至少有一个活动 ShareGroup 引用（或由 KernelActor 临时持有）。

### H0-10 Process/Thread 等待与 WaitObject 的关系

**决定：方向通过，细节留 H1/H3。**

- 第一版通过 `share_group_wait` 与 Task 级终止状态表达 wait；
- `WaitObject` 保持通用 generation 机制；
- H3 可将 ShareGroup/Task 绑定到 WaitObject，以支持 Windows `WaitForSingleObject` 和 Linux `waitid` 的统一实现；
- 不在 H1 强行把 ShareGroup/Task 塞进现有 `WaitObject` 对象，以免污染等待语义。

## 3. 设计草案需要修正的点

1. **`Task.share_group` 改为必填**，并写明非 CPU Task 的退化 ShareGroup 规则；
2. **`HeldByTable` 统一改名为 `HeldByCapTable`**；
3. **新增 `HeldByShareGroup`**，修正 `root_ns_cap` 位置与 W5；
4. **区分内部对象与公开 Capability 目标**：`AddressSpace`/`CapTable` 内部；`ShareGroup` 公开；
5. **补 `cap_table_dup` / `address_space_dup` 语义**，尤其是权限、Audit 和 W2/W13；
6. **补线程/进程生命周期拆分**：`thread_exit`、`share_group_terminate`、`share_group_wait`、zombie 状态；
7. **补内部对象引用计数与 GC 规则**；
8. **补第一版 profile 范围**，明确哪些 clone flag 不在 H1。

## 4. 通过条件

进入 H1 前，设计草案必须修订并满足：

| # | 条件 | 产出 |
|---|---|---|
| C1 | `HeldByCapTable` + `HeldByShareGroup` 位置模型冻结 | 修订 §4.2、W3/W4/W5 |
| C2 | `CapTable`/`AddressSpace` 内部对象与引用计数规则冻结 | 修订 §4.1、§4.3、W4/W7/W13 |
| C3 | `fork` 的 `cap_table_dup` 权限、Audit 与 W2 论证 | 新增 H1 设计小节 |
| C4 | Thread/ShareGroup 生命周期、zombie/wait 状态机 | 新增 H1 设计小节 |
| C5 | 第一版 Linux/Windows profile 映射表 | 修订 §4.4 |
| C6 | H1 对现有 `task_create`/`task_destroy` 的迁移路径 | 修订 §6 Phase H1 |

## 5. 评审结论

- **H0：条件通过。**
- 推荐目标架构（`Task`/`Thread` + `ShareGroup` + `AddressSpace` + `CapTable`）成立；
- H0-1～H0-10 决策可以作为 H1 设计输入；
- H1 启动前必须先完成 C1–C6，否则规范修订会在 W3/W4/W5 生命周期处失稳；
- 本评审不修改 [[COSIR 原语规范]] 冻结语义，不产生实现或形式化证据。
