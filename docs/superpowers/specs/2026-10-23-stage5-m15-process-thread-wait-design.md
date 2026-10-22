# 阶段 5 / M15：Process/Thread/ShareGroup 与 WaitObject 设计（D3 提案）

**日期**：2026-10-23
**状态**：设计提案（未实现）。D3/D2 未组织级关闭；本文件按路线图的建议给出
**工作决定**，作为 M15a（WaitObject）与 M15b（Process/Thread）的实现契约，但不
宣称 COSIR 已修订或阶段条件已满足。
**上位计划**：[路线图](../plans/2026-10-22-stage5-completion-roadmap.md) M15；
[COSIR §5.2/§5.6/§21](../../../COSIR%20原语规范.md) 已有 WaitObject 冻结语义。

## 1. 决策（工作决定，未关闭）

| 决策 | 工作决定 | 影响 |
|---|---|---|
| D2 | 保留 1–41，从 **42** 起追加；Prima 命名；只登记已实现错误码 | M15 的 `wait_*` 与 M16 的 `futex` 用 42+ |
| D3 | 先在 COSIR 建立 Process/Thread/ShareGroup 语义，再实现；Task 仍是唯一调度单元，Thread 是 Task 的角色 | fork/exec 的归属待 M17 |
| D7 | 本文件不实现 signal；`BlockedWait` 的 `alertable` 留 M18 | — |

这些工作决定与 D5/D8 一样登记为“未关闭”；实现只按证据标 `partial`。

## 2. COSIR 增量（提案）

现有 COSIR 只有 `Task`。M15 在**不删除/不改写既有 Task 语义**的前提下追加：

```text
ProcessId : Nat
ThreadId  : Nat

Process {
    creator:  Actor,
    root:     SegId,                 // 地址空间根（与 Task.vspace 同源）
    threads:  FiniteSet<ThreadId>,
    exit:     Option<ExitStatus>,
    children: List<ProcessId>,
}

Thread {
    process: ProcessId,
    task:    TaskId,                 // 1:1 映射到 Task
    fs_base: Option<VirtAddr>,       // TLS
    status:  TaskStatus,             // 与 Task.status 同步
}
```

不变量：

- 每个 Thread 恰有一个 Task，反之亦然；Process 至少一个 Thread 时存活；
- 同一 Process 的 Threads 共享 `root`/vspace、fd 表、cwd；每个 Thread 有独立
  kernel stack、TLS（FS base）、执行上下文与 signal mask（M18）；
- `Task` 的 `BlockedWait` 语义沿用 §5.2/§21.2；Process 不是调度单元。

Phase 5 只落地**单线程 Process + Thread 视图**；`fork`/`execve`/`wait4` 留 M17，
多线程 `pthread` 由 M16 在共享 root 上创建 Threads。

**实现进展（M15b-1）**：内核 bootstrap 已有 Process 表与 Task→Process 归属；
`task_create` 建立单线程 Process，销毁摘除。用户态线程创建暂缓：COSIR 的
Capability 位置空间是每 Task 的，线程共享地址空间后无法直接使用既有 fd/cap，
见 `ISSUE-PTHREAD-CAPS-001`；需先决定 Process 共享 cap 空间或一致的 cap
复制/迁移语义，才能继续 M15b-2 与 M16。

## 3. WaitObject（M15a，按 COSIR §21 实现）

### 3.1 对象与权限

- 新内核对象 `WaitObject { id, creator, generation, waiters: [TaskId; N] }`；
- 根 Capability 权限 `{Derive, WaitWait, WaitSignal, WaitDestroy}`；
- 目标销毁后 Capability 变 stale；`observe`/`wait`/`signal`/`destroy` 对 stale
  返回 `TargetDead`；
- 与 Timer/ExcSource 同一套 Capability 位置模型，不新增旁路授权。

### 3.2 Syscall（42–46，工作决定）

| 号 | 名称 | 语义 |
|---:|---|---|
| 42 | `wait_create` | 创建对象，返回根 cap |
| 43 | `wait_observe(cap)` | 返回 generation |
| 44 | `wait_objects(cap0, gen0, cap1, gen1, mode, timeout)` | 立即满足/超时/阻塞 |
| 45 | `wait_signal(cap)` | generation++，按 Any/All 唤醒 |
| 46 | `wait_destroy(cap)` | 销毁对象，唤醒等待者为 `WaitTargetDead` |

`mode`：0=Any、1=All；`timeout = u64::MAX` 表示无限；否则为 PIT tick 绝对
deadline。`WaitResult` 编码为：0=`WaitSatisfied`、1=`WaitTimedOut`、
2=`WaitTargetDead`；`WaitAlerted` 留 M18。

### 3.3 阻塞与唤醒

- `BlockedWait` 加入内核 TaskStatus；调度器只选 `Ready`，与 `BlockedRecv` 并列。
- 阻塞时保存 `wait_request(entries, mode, deadline)`、把 Task 加入各对象的
  waiters、把 captured frame 的返回槽置为 0；`signal` 递增 generation 并：
  - Any：唤醒所有等待该对象且 generation 已超观测的 Task；
  - All：重算每个等待者，全部满足才唤醒；
  - 被唤醒者从所有 waiters 删除，`pending_wait_result` 置为 `Satisfied`，
    frame rax 注入结果码，状态改 `Ready`。
- `wait_destroy` 对每个 waiter 注入 `WaitTargetDead` 并唤醒。
- 超时：每个 syscall 入口调用 `expire_waits(now)`；deadline 到期且仍阻塞者
  唤醒为 `WaitTimedOut`。没有 IRQ 时，“时间推进”仍由 syscall 驱动，QEMU
  超时用例由一个 busy-poll `time_now` 的第二个 Task 驱动；host 状态机用显式
  `advance_time` 覆盖。
- 容量：`MAX_WAIT_OBJECTS=8`、每对象 waiters ≤ 18（MAX_TASKS）、每个等待者
  最多 2 个 entry（Any/All 的最小集合）。

### 3.4 审计

沿用 Capability/Task 状态审计：对象创建/销毁、generation 递增、等待阻塞与
唤醒各追加既有或新增事件；不记录 payload。

## 4. Process/Thread（M15b，后续切片）

- `task_create` 增加共享地址空间模式：新 Thread 复用 Process 的 root，独立
  内核栈/TLS/执行上下文；单线程 Process 由 C 任务创建时隐式建立；
- `Thread` 对象可从用户态观察/管理（名称/状态），调度仍是 Task；
- 容量：Thread/Cap/vspace/audit 预算先测量再扩容；每任务内核栈 32 KiB 的
  上限决定线程数上界；
- 证据：host 状态机 + QEMU 线程创建/join + TLS 隔离 + 退出回收。

## 5. 证据计划

- **host**：`bootstrap` 状态机覆盖 create/observe/signal/wait 立即满足、
  Any/All、阻塞/唤醒、destroy 唤醒、超时、generation 越界、stale cap；
- **QEMU**：`W1` 探针覆盖 create/observe/signal/destroy、立即满足与立即超时、
  非法 generation、目标死亡；阻塞唤醒由 host 状态机覆盖（无第二个用户任务
  时的限制在报告中登记）；
- 既有 QEMU 轮次全量重跑；容量/审计变化逐条登记。

## 6. 边界

- `WaitAlerted`/upcall 留 M18；`futex` 用户映射留 M16；
- fork/exec/wait4 留 M17；多线程 Process 的完整语义留 M15b；
- 无 IRQ 时超时依赖 syscall 推进；不在本设计宣称“无忙等”。

## 7. 本设计不代表

- 不表示 D2/D3 已关闭或 COSIR 已正式修订；
- 不表示 Process/Thread/pthread 已实现；
- 不表示阶段 5 退出条件已满足。
