---
id: plan-p3-async-slices
kind: plan
authority: execution-plan
status: in-progress
lifecycle: current
updated: 2026-11-24
---

# P3 有界异步批量切片计划

## 目标与基线

把[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md)新增的 P3 草案拆成
可独立验收的切片：先冻结并实现共享控制页的有界环，再把 Store 与 Linux 前端接到
v2 会话，最后用 QEMU 正反例矩阵取证。本计划不改变 v1 同步会话、不关闭 P2、
不签署 7-A，也不包含 SMP 与恢复。

- 基线：`f8922b3`（P2b-6 已提交，工作树干净）。
- 上位契约：契约 P3 节；[LibOS 数据通路完整改造计划](2026-11-10-libos-data-path.md)。
- 容量前提：`ASYNC_CREDITS=4`、`ASYNC_FAIR_QUOTA=2`、每会话 5 页；
  `MAX_PROXIES=8`；不新增 syscall。
- 前序限制保持：`MAX_CLIENTS=18` / `MAX_OWNERS=32` / `MAX_PROXIES=8` /
  Linux fd 表 16 / `SERVICE_FILE_HANDLES=8`。

> 2026-11-18 进度：P3-0 已落盘——契约 P3 节改为 IPC 顺序模型（拒绝裸共享原子），
> `async_batch` 状态机与有界穷举交错 host 证据通过。P3-1 Store v2 会话已落盘——
> ASYNC_* tag、控制页编解码、5 页段、公平排空、取消/重置/关闭与死亡回收，host
> 1181 与既有 72 轮通过；Store 历史栈 4→8 页已登记。
>
> 2026-11-19：P3-2 的 Rust 侧已落盘——Store 直连 v2、`Remote` v2 管线、exec
> 重开、独立代理 tag 族与服务栈预算修复；host 1181、lint、72 轮与
> `test-linux-proxy` 通过。C 数据面 v2 在 W6/W7b 暴露 fork 后父进程 fd
> EACCES 反例，已回退未交付；C 文件锁拆分未开始，P3-2 不关闭。见
> [P3-0 报告](../reports/2026-11-18-p3-async-state-machine.md)、
> [P3-1 报告](../reports/2026-11-18-p3-store-v2.md) 与
> [P3-2 报告](../reports/2026-11-19-p3-frontend-v2.md)。
>
> 2026-11-24：P3-2 的 C 数据面 v2 已落盘——`prima_bulk_close` 同时清理 v1 页与
> v2 会话（修复 fork 继承控制页映射的 W6/W7b 反例），per-op 失败也推进生产者
> 序号（修复 V4 悬挂）；workspace 1181、musl 7、lint、test-process W4–W9、
> test-vfs V1–V4 与完整 72 轮通过。C 进程级文件锁拆分仍未开始，P3-2 不关闭。见
> [P3-2 数据面报告](../reports/2026-11-24-p3-frontend-v2-data-plane.md)。

## 依赖总览

```text
P3-0 ─→ P3-1 ─→ P3-2 ─→ P3-3 ─→ P3-4
```

| 切片 | 依赖 | 类型 | 关闭时提供 |
|---|---|---|---|
| P3-0 契约与环状态机 | P2b-6 | 实现+host | v2 状态机与有界穷举交错证据 |
| P3-1 Store v2 会话 | P3-0 | 实现+host | 排空、公平、取消、重置、死亡回收 |
| P3-2 前端 v2 接线 | P3-1 | 实现 | Linux 前端异步管线、在途 fd、exec 围栏 |
| P3-3 QEMU 正反例矩阵 | P3-2 | 验收 | 队列/取消/唤醒/公平/恶意客户端机器证据 |
| P3-4 P3 收口 | P3-3 | 证据 | 报告与组织级复核输入 |

## P3-0 契约与环状态机

- 依赖：P2b-6。
- 交付路径：契约 P3 节；`prima-runtime/src/async_batch.rs`（新模块）；
  `docs/superpowers/reports/` 的 P3-0 报告。
- 工作：
  1. 按契约冻结控制页布局、`tail`/`head`/`generation`、`K=4`、数据页序号的
     映射，以及由 IPC 边建立的所有权/顺序规则。
  2. 实现可被 Store 与客户端共用的纯状态机：`publish`（含满队列拒绝）、
     `claim`（含快照边界）、`complete`、`cancel`（认领前/后）、`reset`、
     `close` 判定。状态机只消费消息事件，不直接访问映射内存或内核。
  3. host 证据必须是**有界穷举交错**：对生产者/Store/死亡回收三方在各消息边
     上的交错做系统性探索，断言每个可达状态满足不变量（未完成数 ≤ K、序号
     单调、取消只作用于未认领批、关闭只在空队列成功）；另配确定性回归用例。
     单纯单线程顺序回放不构成证据。
- 验收：`cargo test --workspace` 与新增异步测试通过；契约文本与实现命名一致。
- 停止条件：状态机无法同时服务 Store 与客户端，或穷举模型的状态空间无法界定；
  停止并修订契约，不得用线性回放冒充交错证据。
- 不包含：Store/前端接线、QEMU。
- 2026-11-18：`prima-runtime/src/async_batch.rs` 已实现 `Session`/`Producer`、
  满窗口、取消、重置、关闭与 `u64` 回绕；host 6 项测试与有界交错通过。见
  [报告](../reports/2026-11-18-p3-async-state-machine.md)。

## P3-1 Store v2 会话

- 依赖：P3-0。
- 交付路径：`prima-user/src/service.rs`、`prima-user/src/sessions.rs`、
  `prima-runtime/src/bulk.rs`（新 tag 常量）、host 测试。
- 工作：
  1. `ASYNC_OPEN/ACK/DOORBELL/COMPLETE/CANCEL/CLOSE` 与多页段的建立、映射、
     延迟销毁重试（沿用 P2b-5 的槽位复用修复）。
  2. v2 排空循环：快照、v1 编解码复用、每会话 FIFO、`ASYNC_FAIR_QUOTA` 轮转、
     每轮每会话一条合并 `COMPLETE`。
  3. 取消、重置、`EBUSY` 关闭、`generation` 围栏与死亡回收；v1 路径保持不变。
- 验收：host 覆盖满队列、回退 `tail`、保留位非零、认领前/后取消、关闭竞争、
  两会话公平与持续进展；v1 既有测试不改动。
- 停止条件：共享页校验或公平配额需要放宽 v1 语义时停止；不得让 v1 客户端
  承担 v2 的容量或错误语义。
- 不包含：Linux 前端切换。
- 2026-11-18：实现落在 `prima-runtime/src/bulk.rs`、`prima-runtime/src/
  async_batch.rs`（`Store<N>` 调度）与 `prima-user/src/service.rs`（未新开
  sessions 模块）；控制页校验、`generation_low8`、CANCEL/CLOSE 折算 `tail`、
  重置回收与 Store 栈 4→8 页均已落盘。host 1181、lint、既有 72 轮与
  `test-linux-proxy` 通过。见 [报告](../reports/2026-11-18-p3-store-v2.md)。

## P3-2 前端 v2 接线

- 依赖：P3-1。
- 交付路径：`prima-user/src/linux_store.rs`、`prima-user/src/linux_process.rs`、
  Linux 前端的 C 适配层锁、host/QEMU 冒烟。
- 工作：
  1. `Remote` 增加 v2 管线：发布、doorbell、park/唤醒、完成取回、取消与关闭；
     v1 保留给原生/其它客户端。
  2. 在途 fd 计数与延迟 `CLOSE`；应用 fd 复用分配新 wire fd。exec 前排空并
     重新 OPEN；`generation` 不匹配返回 `ESTALE`。
  3. 移除 C 适配层跨同步调用的进程级文件锁，改为按 fd/在途引用；不得放宽
     append、共享游标与 errno 语义。
- 验收：既有 Linux 差分 22 轮通过；单会话 v2 `cat`/`M4-fork` 冒烟通过；host
  覆盖 fd 复用与延迟关闭。
- 停止条件：锁拆分改变 v1/既有 syscall 语义时停止并登记；不得只把 `SUBMIT`
  改成不等待而保留锁。
- 不包含：完整矩阵、SMP。
- 2026-11-19：Rust 侧（`Remote` v2、exec 重开、Store 直连 v2、代理 tag 族与
  栈预算）已落盘并通过 72 轮/代理矩阵；`M4-fork` 冒烟由代理矩阵覆盖。C 数据面
  v2 在 V1 通过、W6/W7b 因 fork 后父进程 `read(/dev/null)` 返回 EACCES 回退；
  C 锁拆分未开始。P3-2 保持未关闭，见
  [报告](../reports/2026-11-19-p3-frontend-v2.md)。
- 2026-11-24：C 数据面 v2 落盘：`prima_bulk.h` 的直连 5 页会话、fork/exec/线程
  退出统一关闭、per-op 失败仍推进生产者序号；W6/W7b 反例根因为 `prima_bulk_close`
  只看 v1 页导致 v2 会话与映射被子进程继承。V4 同时验证 v1 探针与 v2 数据面并存。
  锁拆分仍未开始，P3-2 保持未关闭，见
  [数据面报告](../reports/2026-11-24-p3-frontend-v2-data-plane.md)。

## P3-3 QEMU 正反例矩阵

- 依赖：P3-2。
- 交付路径：`xtask/src/` 新场景、报告。
- 工作：
  1. 正例：Linux v2 会话读/写/fork/exec；两会话公平进展；完成合并下的 park/
     唤醒。
  2. 负例：满队列 `EAGAIN`；回退 `tail`/保留位非零重置；认领前取消无副作用、
     认领后 `EALREADY`；发布页被改写只按快照执行；单侧死亡后另一侧继续。
  3. 保持 v1 `test-linux-proxy`、既有 72 轮与 Linux 差分；不改写冻结事件。
- 验收：命令、退出码、trace 断言与必要摘录进报告；失败保留。
- 停止条件：需要修改 v1 或既有 72 轮事件时停止；逐条登记后才可继续。
- 不包含：SMP、恢复、性能门槛。

## P3-4 P3 收口

- 依赖：P3-3。
- 交付路径：新报告、阶段跟踪、LibOS 计划状态。
- 工作：汇总命令/结果/失败/限制；登记未覆盖项；准备组织级复核输入。
- 验收：文档工具通过；复核记录存在（本步之前 P3 不得称为已复核关闭）。
- 不包含：7-A 勾选、P5/P6。

## 排序与停止规则

1. 契约与状态机先行；Store 与前端不得各自发明所有权语义。
2. v1 路径与既有事件默认冻结；确需变化必须逐条登记并重跑 72 轮与 Linux 差分。
3. 交错证据必须是系统性探索或形式模型；单线程顺序回放不能关闭 P3-0。
4. 公平性以“每轮至少推进一个活会话”为可观察不变量；不得用总吞吐替代。
5. 失败、部分进展与重试窗口随报告保留；不以重跑覆盖。
