# 阶段 5 / M15b-2a：共享能力空间的线程创建实现报告

**日期**：2026-10-23
**范围**：M15b-2 的内核部分（用户态 `thread_create` 已接线，QEMU 端到端
被任务表容量阻塞）。设计见
[M15 设计](../specs/2026-10-23-stage5-m15-process-thread-wait-design.md)。
**后续**：W2 已在 M15b-2b（线程惰性槽位回收）通过；本报告保留当时的失败记录。
**结论**：
- `ISSUE-PTHREAD-CAPS-001` 采用**工作决定：同 Process 的线程共享 leader 的
  Capability 位置空间**；`space_of(task)` 把 holder 归一化为 Process leader，
  单线程 Task 退化为自身 id，既有 413 个 host 用例全绿。
- 新增 syscall **47** `thread_create(entry, stack_top, tls)`、自毁 cap 注入、
  `user::register_thread_task`（共享 root、独立内核栈/FS base/初始帧）。
- host 用例验证线程与 leader 共享 Process 与能力空间。
- **QEMU 端到端（W2 探针）未通过**：boot 结束时 `TaskTable` 已满（18 个槽位，
  含 Dead 记录），`next_free_id()` 返回 `None`，`thread_create` 返回 `ENOSPC`。
  已登记 `ISSUE-THREAD-CAPACITY-001`；W2 未接入套件。

## 1. 变更

| 位置 | 变更 |
|---|---|
| `kernel/src/bootstrap.rs` | `Process.leader`；`space_of`；`authorize`/`mint_root_capability`/`derive_*`/`move_capability`/直接 holder 检查统一按能力空间；`thread_create`/`mint_thread_self_cap`/`thread_abort`；`ThreadError` |
| `kernel/src/user.rs` | `register_thread_task`（共享 root、kstack、FS base、初始帧 rdi=self cap） |
| `kernel/src/syscall.rs` | `SYS_THREAD_CREATE`（47）胶水：建 Task/Process 归属、铸自毁 cap、登记调度器 |
| `prima-abi/src/lib.rs` | `SYS_THREAD_CREATE = 47` |

## 2. 能力空间语义

- `space_of(task)`：Task 有 Process → `process.leader`；否则自身 id。
- 单线程 Task 的 leader 就是自身，因此既有 holder 值、审计与授权结果不变
  （413 个 host 用例原样通过）。
- 同 Process 的线程共享 leader 的 holder：leader 铸造的 Wait/文件/命名空间 cap
  可被任一线程授权使用；线程自毁 cap 落在同一空间。
- `task_create` 的 Process leader 固定为新 Task（修正了 M15b-1 以调用者为
  leader 的缺陷；该缺陷在 `space_of` 接线后会使新 Task 的既有 cap 失配）。

## 3. 线程创建

- 前置：调用者存活、entry 非零、栈 16 对齐且位于低半区、审计容量；
- 新 Task 挂到调用者 Process（无则新建、leader=调用者），共享同一 root
  （`sched::root(caller)`）、独立内核栈（id 3..18）与 FS base（`tls=0` 继承）；
- 初始帧 `rdi = self cap`（自毁用），`rip=entry`、`rsp=stack_top`；
- 自毁走既有 `SYS_TASK_DESTROY`（20）自毁路径。

## 4. 证据

### 4.1 host

```text
cargo test --workspace  → 413 passed
```

新用例 `thread_create_shares_process_and_capability_space`：同 Process、线程数 2、
`space_of(thread)==leader`、leader 铸造的 WaitObject cap 可被线程授权、非法
入口/栈被拒。

### 4.2 QEMU（未通过，登记）

W2 探针（`WAIT2`：主线程阻塞等待、线程 signal + 自毁）在 boot 后失败：

```text
WAIT2|create=1[TDBG]T1[TDBG]4;thread=0 ...
[panic] kernel/src/syscall.rs:1097   // 无其他 Ready 任务，等待无法切走
```

`T1` 表示 `TaskTable::next_free_id()` 返回 `None`：boot 的 services/manager/
探针已占满 18 个槽位，且销毁的 Task 记录（Dead）不回收。探针未接入套件。
既有 83 条套件轮次全量重跑通过（EXIT 0），确认 `space_of` 重构未改变单线程
Task 的用户可见行为。

## 5. ISSUE-THREAD-CAPACITY-001（新登记）

`MAX_TASKS=18` 的槽位在阶段 5 的常规 boot 配置下已被占满（含 Dead 记录），
用户线程无法获得 Task id。可选修复：
1. 回收 Dead 槽位：销毁时移除记录，并**撤销**所有 target 为该 Task 的 cap
   （避免 id 复用复活旧句柄）；
2. 扩大 `MAX_TASKS`/内核栈数组，同时压缩 `TaskRecord`/`Process` 以守住
   `BootstrapState` 栈预算（当前 13,984 B，余量 <3 KiB 测试）；
3. 把 Process/等待请求表移出 `BootstrapState` 的静态 arena。

未决定前，M15b-2 的 QEMU 线程证据与 M16 的 pthread 仍被阻塞。

## 6. 边界

- 无线程 join/分离/取消、无 per-thread TLS（`tls=0` 继承 leader 的 FS base）；
- 线程创建要求有空闲 Task 槽位（当前 boot 配置下不满足）；
- 无 W2 套件轮次；`M16` 未开始。

## 7. 本报告不代表

- 不表示共享线程的 QEMU 端到端已验证；
- 不表示 M16 pthread/futex 可开始；
- 不表示阶段 5 退出条件已满足。
