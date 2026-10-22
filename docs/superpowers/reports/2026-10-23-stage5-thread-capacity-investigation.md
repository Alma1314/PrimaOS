# 阶段 5 / ISSUE-THREAD-CAPACITY-001 调查记录

**日期**：2026-10-23
**范围**：M15b-2 的 QEMU 线程端到端被任务表容量阻塞；本记录是修复前的
定量调查。**未改变实现**（代码保持在 `7930658`）。
**关联**：[M15b-2a 报告](2026-10-23-stage5-m15b2a-thread-create.md)、
[跟踪页 ISSUE-THREAD-CAPACITY-001](../../../Prima%20OS%20实现阶段.md)。

## 1. 现象

W2 探针（主线程阻塞等待 + 共享线程 signal + 自毁）在 boot 后：

```text
WAIT2|create=1[TDBG]T1[TDBG]4;thread=0 ...
[panic] kernel/src/syscall.rs:1097
```

`T1` = `TaskTable::next_free_id()` 返回 `None`；`4` = `CapacityExceeded`。
boot 结束时 18 个 Task 槽位已占满，且销毁的 Task 记录（Dead）保留、id 不复用。

## 2. 扩容实验

把 `MAX_TASKS` 从 18 提到 20（内核栈数组 16→18），`thread_create` 可成功，
但 **boot 自身的 `task_ready` 见证失败**：

```text
task_ready tasks is 13, expected 12
DBGT: 1U 2R 3D 4D 5D 6D 7F 8B 9B 10B 11B 12B 13B 14B 15F 16B 17D 18R
```

- 12 条既有任务：init、task 2、Faulted 见证 7、Rust client 8、六个服务端点
  9–14、tty 15、以及 args/helper 16；
- 新增 **id 18（Ready）**：不是 helper 泄漏，而是 manager 的**容量负例探针**
  （`servicemgr.rs` 中 `capacity_rejected = runtime.task_create(...)`）依赖
  “表已满”才返回 `ENOSPC`；扩容后该请求成功，子任务滞留 Ready。
- 即扩容会破坏容量负例的前提与 `task_ready=12` 的冻结契约；不是安全的修复方式
  （除非同时把负例改成自包含的“填满→拒绝→销毁”）。

## 3. 候选修复

| 方案 | 内容 | 风险/成本 |
|---|---|---|
| A | Dead 槽位回收 + 墓碑：销毁时移除记录但保留 `task_status=Dead` 墓碑；新建时可复用 id | 需撤销所有 target 为该 Task 的 cap，否则 id 复用会复活旧句柄；cap 表无按 target 迭代，需新增 `for_each_handle` 并逐条 `retire_capability`；会改变 `caps_live` 见证 |
| B | `ObjectRef::Task(id, generation)`：Task 带代际，复用 id 时旧 cap 目标失活 | 语义最干净；改动面大（ObjectRef 匹配、审计、测试） |
| C | 定位并修复 id 18 的 helper 任务泄漏，再扩容 | 需先解释该任务由谁创建、为何在 C 退出后仍 Ready；可能与 ISSUE-START-ARGS-001 相关 |
| D | 线程不复用 boot 的 Task 槽位，改为独立的线程表/调度槽 | 改动调度器与内核栈分配，工作量最大 |

推荐顺序：**保持 MAX_TASKS=18 并做 B（Task 代际）或 A（Dead 回收+cap 撤销）**，
让线程复用 Dead 槽位；扩容只有在负例探针改为自包含填充后才可考虑。
C（helper 泄漏）已被排除：id 18 是负例探针自己创建的 Task。

## 4. 下一步

1. id 18 已定位为 manager 容量负例探针的子任务（扩容后成功创建）；
2. 若确认是泄漏：销毁它并保留 12 的见证，再扩容到 20–24；
3. 否则采用 B（Task 代际）实现 Dead 槽位安全复用，再接 W2/M16；
4. 在问题关闭前，W2 不接入套件，M15b-2/M16 不勾选。
