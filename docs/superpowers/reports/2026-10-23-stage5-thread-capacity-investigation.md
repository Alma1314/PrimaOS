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
- 新增 **id 18（Ready）**：此前因 18 槽位已满而创建失败，扩容后成功并滞留。
- 即扩容会改变 boot 的任务群体，破坏“容量水位”见证的冻结契约；不是安全的
  修复方式。

## 3. 候选修复

| 方案 | 内容 | 风险/成本 |
|---|---|---|
| A | Dead 槽位回收 + 墓碑：销毁时移除记录但保留 `task_status=Dead` 墓碑；新建时可复用 id | 需撤销所有 target 为该 Task 的 cap，否则 id 复用会复活旧句柄；cap 表无按 target 迭代，需新增 `for_each_handle` 并逐条 `retire_capability`；会改变 `caps_live` 见证 |
| B | `ObjectRef::Task(id, generation)`：Task 带代际，复用 id 时旧 cap 目标失活 | 语义最干净；改动面大（ObjectRef 匹配、审计、测试） |
| C | 定位并修复 id 18 的 helper 任务泄漏，再扩容 | 需先解释该任务由谁创建、为何在 C 退出后仍 Ready；可能与 ISSUE-START-ARGS-001 相关 |
| D | 线程不复用 boot 的 Task 槽位，改为独立的线程表/调度槽 | 改动调度器与内核栈分配，工作量最大 |

推荐顺序：C（若只是 helper 泄漏，扩容后一并清理）→ B（长期正确）→ A。

## 4. 下一步

1. 在 manager/main 的 C 任务创建路径定位 id 18 的创建点与归属；
2. 若确认是泄漏：销毁它并保留 12 的见证，再扩容到 20–24；
3. 否则采用 B（Task 代际）实现 Dead 槽位安全复用，再接 W2/M16；
4. 在问题关闭前，W2 不接入套件，M15b-2/M16 不勾选。
