---
id: report-p3-frontend-v2-data-plane
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-24
---

# P3-2：C 适配层数据面 v2（fork 反例修复）

## 结论与范围

本报告补齐 P3-2 的 C 适配层数据面：musl/C 程序的 `read`/`write`/`pread64`/
`pwrite64` 从 v1 单页同步会话切到直连 5 页 v2 会话，并修复
[上一片报告](2026-11-19-p3-frontend-v2.md)保留的 W6/W7b fork 反例。C 进程级
文件锁拆分（P3-2 工作项 3）仍未开始；**P3-2 不关闭**，P3 不收口。
v1 页、`PRIMA_BULK_PROBE` 与既有事件/回执保持不变。

## 基线与输入

- 基线：`5d61084`（P3-2 Rust 侧已落盘）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md) 的 P3-2。
- 失败保留：[2026-11-19 P3-2 报告](2026-11-19-p3-frontend-v2.md) 的
  "C 数据面 v2 未交付"一节。

## 根因与修复

### 反例一：fork 前只关 v1 页，v2 会话与映射被子进程继承

`prima_bulk_close()` 外层按 v1 页判断：`if (!session || !session->bulk_cap)
return 0;`。C 数据面切到 v2 后 `bulk_cap` 恒为 0，因此 fork/exec/线程退出路径
既不解除 5 页映射，也不发送 `ASYNC_CLOSE`；子进程继承父进程控制页的映射与
`prima_session` 表，父进程在 fork 后的 doorbell 与 Store 会话 `generation`
不一致，Store 以 `EACCES` 拒绝（即上一片"父进程 `read(/dev/null)` 失败"的
可复现形态）。修复：只要任一 v1 页或 v2 会话存在，就分别清理；v2 关闭顺序为
解除映射 → 丢弃控制段能力 → `ASYNC_CLOSE` → 等 `ASYNC_ACK`，再清空本地状态。

### 反例二：per-op 失败时未推进生产者序号

首个分页批返回非零操作状态（如只读 fd 上的 `pwrite`）时，适配层直接返回错误
而未推进本地 `async_tail`。Store 已消费该序号并推进 `head`，下一批重复发布
同一序号后等待合并 `COMPLETE` 悬挂（V4 的 `pread(wo, ...)` 暴露）。修复：
收到越过该序号的 `COMPLETE` 后先写回 `async_tail = ordinal + 1`，再报告操作
错误；只有 doorbell 协议拒绝（未消费）才不推进。

### 实现要点

- 直连 v2 tag `0x5f30..0x5f35`，slot 取 Task 槽（bits 32..39）；控制页
  `"PBQ2"`、`K=4`、数据页 `1 + ordinal % 4`，数据页 v1 头 `sequence` 为
  `ordinal + 1`。
- `ASYNC_OPEN` 递增 generation；门铃携带 generation 低 8 位与 `tail` 低 32 位；
  客户端收到任何消息后重读控制页 `head`，不把单条消息当作全部完成。
- 控制页保留位/魔数/`tail`/`head` 初始值在客户端映射后校验；失败即退图并报
  `EINVAL`。
- `prima_session_accept` 在 Task cookie 变化时清空 v1 与 v2 本地状态；
  `prima_bulk_close()` 由 fork、exec、线程清理共用。
- host 可执行检查：`test_adapter.c` 断言 `prima_async_header_ok` 的接受/拒绝
  路径与按 Task 槽的固定 VA 计算；不执行任何 IPC。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test --workspace` | 本次 | PASS；1181 passed、0 failed、1 ignored | host |
| `python3 prima-user/musl/test_port.py` | 本次 | PASS；7 tests（含新增 v2 控制页断言） | host（C 适配层） |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy `-D warnings`、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `cargo xtask test-process` | 本次 | PASS；W4/W5/W6/W7/W7b/W8/W9 各 2 轮 | QEMU（C 适配层 v2） |
| `cargo xtask test-vfs` | 本次 | PASS；V1/V2/V3/V4 各 2 轮（V4 含 v1 bulk 探针） | QEMU（v1+v2 并存） |
| `cargo xtask test` | 本次 | PASS；`full BIOS suite: 36 programs, 72 runs`、`stage1 boot slice` | QEMU（完整套件） |

## 失败与限制

- **C 进程级文件锁未拆分**：fd 表仍由进程级锁串行；数据面每个 syscall 仍然
  同步等待 `COMPLETE`，未使用满 `K=4` 在途或 park/唤醒。P3-2 工作项 3 未完成。
- **只测单核单会话**：无 SMP、恢复/去重、性能门槛；v2 会话每 Task 5 页，直连
  槽上限仍为 `MAX_CLIENTS=18`。
- 失败路径保留：上一片的 W6/W7b 反例与诊断仍在
  [2026-11-19 报告](2026-11-19-p3-frontend-v2.md)，本报告只登记修复后的结果。
- 未改变 v1 客户端、`PRIMA_BULK_PROBE`、profile v1/v2 或既有 72 轮事件。

## 阶段/profile 影响

不关闭 P3-2 或 P3，不改变 P2 状态、阶段 7 九轨勾选或 7-A；不改变 profile。

## 后继关系

- 下一片：C 进程级锁拆分——短临界区保护 fd 表，按 wire fd 在途引用延迟
  `CLOSE`，不放宽 append、共享游标与 errno 语义。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md)。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节。
- 上一片：[P3-2 Rust 侧交付与失败保留](2026-11-19-p3-frontend-v2.md)。
