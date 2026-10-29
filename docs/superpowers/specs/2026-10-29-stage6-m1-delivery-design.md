---
id: spec-stage6-m1-delivery
kind: specification
authority: proposed-mechanism-design
status: design-proposal
lifecycle: current
updated: 2026-10-29
---

# S6-M1 递送设计：OS 中立的同步 syscall 门控（proposal）

## 问题与范围

阶段 6 要求"实现在**不改二进制**的前提下运行 Linux 静态 ELF"，而内核必须保持
"没有 Linux 专属对象或 syscall 语义"（S6-X3）。两条约束的交点只有一个：内核必须能把
一个 ring3 进程的 `syscall` 陷阱**同步**交给另一个 ring3 进程（LibOS），并把它的答复
写回请求者的返回帧。当前内核没有这条路径：

| 现有机制 | 它实际是什么 | 为什么不够 |
|---|---|---|
| `SYS_EXC_SUBSCRIBE`(5)/`SYS_EXC_WAIT`(9) + `exc.rs` | 异步异常订阅与恢复帧 | 通知与恢复是两件事，没有"请求-答复"与一次性语义 |
| `SYS_UPCALL_CONTROL`(65) | 信号管道（sigaction/mask/pending/kill） | 名字像 upcall，语义是 POSIX 信号，不能承载 syscall |
| `SYS_SEND`(17)/`SYS_RECV`(18) | IPC 消息 | 没有"冻结请求者返回帧并最终改写 `rax`"的概念 |
| `SYS_VSPACE_ACCESS_ENABLE`(68) | 切换**自己**页表叶的 user/hidden 位 | 不提供跨任务内存访问，不能拿给 LibOS 读请求者参数 |

本文只定义 **S6-M1**：递送机制本身。它不定义 Linux syscall 表（S6-M3）、不定义
profile、不改白皮书阶段定义。上位契约：[白皮书 §15](../../../Prima%20OS.md)、
[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)的 S6-M1 详细范围、
[阶段 6–7 裁决](../specs/2026-10-28-stage6-7-release-decisions.md)（D10/D13）、
[准备性切片报告](../reports/2026-10-28-stage6-loader-slice.md)。

**非目标**：动态链接（D11/7-E）、Linux 号到语义的映射、`fork`/`exec` 的门控语义（S6-M4）、
SMP 下的门控表分片（D15/7-B）、性能出口门槛（见[验证](#验证与反例)）。

## 状态与决定

- **状态：提案（design-proposal）**。未实现、未分配任何 syscall 号。六个开放问题已由
  [D17–D22 机制裁决](2026-10-29-stage6-m1-mechanism-decisions.md)关闭，本文按裁决结果
  固定了 ABI 形态、答复宽度、内存模型、`fork`/`exec` 立场、模型路线与队列策略。
- **决定（本提案主张，需评审确认）**：
  1. 门控（gate）是**能力对象**，不是任务的隐藏属性；挂接、服务、答复都由能力授权。
  2. 门控期间**分派被取代**：被门控任务的一切 `syscall`/`int 0x80` 陷阱原样交给门控，
     内核不解释调用号与参数（S6-X3 的操作化定义）。
  3. 请求者内存由内核**代理复制**（`copy_in`/`copy_out`），v1 不做跨任务映射授权。
  4. 答复**一次性**（exactly-once）：`reply`/`abort` 消费请求，句柄随即失效。
  5. 不存在内核超时；"不永久阻塞"由中断、任务终止与 LibOS 死亡三条路径保证（见
     [失败与终止](#失败终止与确定性)），这是**明确限制**而不是遗漏。

## 成功与失败契约

### 对象与能力

| 对象 | 内容 | 生命周期 |
|---|---|---|
| `Gate` | 归属者、被门控任务、队列（有界 FIFO）、等待者、状态 | 由 `gate_attach` 创建；最后一个含 `GateAttach` 的能力被 drop/撤销时自动解绑并销毁 |
| `GateRequest` | 请求任务 id、保存的返回帧、原始 `number`/`args`、状态 | 每次被拦下的 trap 一个；由 `reply`/`abort` 或取消路径消费 |
| 请求能力 | 指向单个 `GateRequest` 的派生能力 | `gate_wait` 返回；答复后撤销；stale 句柄确定性失败 |

新增权利（追加到 `kernel/src/rights.rs` 的枚举**末尾**，保持既有位序）：

- `GateAttach`：把一个 Task 挂到门控上（还需持有该 Task 且含 `TaskManageVSpace` 的能力）。
- `GateServe`：等待/答复/代理复制，只能由被授权的服务任务持有。

不变式：

- **I1（一次性）**：一个请求最多一次有效答复；`reply`/`abort` 都撤销请求能力并释放槽位，
  第二次调用得到 `ENOENT`（stale）。
- **I2（分派被取代）**：任务处于门控挂接状态时，原生分派表**不被查询**；被门控任务不能
  自行调用任何原生 syscall（这是权限收窄，不是便利性限制）。
- **I3（零 Linux 语义）**：`number`、6 个参数、答复值都按不透明 `u64` 搬运；内核不比较
  调用号、不产生 Linux errno、不读参数指向的内容（只有 `gate_copy_*` 才读）。
- **I4（有界等待）**：任何未决请求最终以 `reply`、`abort`、请求者被中断（`EINTR`/`RESTART`）
  或请求者终止之一结束；不存在"无人持有能力却仍阻塞"的请求（由 I5 与能力生命周期闭合）。

### ABI（候选号段，未分配）

| 候选号 | 名字 | 签名（语义） |
|---|---|---|
| 73 | `SYS_GATE_ATTACH` | `(task_cap) -> gate_cap`：创建门控并挂到该任务；能力含 `GateAttach` |
| 74 | `SYS_GATE_WAIT` | `(gate_cap) -> (request_cap, task_id, number, args[6], flags)`：阻塞直到有请求或门控孤儿化 |
| 75 | `SYS_GATE_COPY_IN` | `(request_cap, arg_index, offset, len, dst_ptr) -> len`：从请求者的第 `arg_index` 个参数地址复制 |
| 76 | `SYS_GATE_COPY_OUT` | `(request_cap, arg_index, offset, len, src_ptr) -> len`：写回请求者缓冲 |
| 77 | `SYS_GATE_REPLY` | `(request_cap, value) -> 0`：把 `value` 写入请求者 `rax` 并恢复它 |
| 78 | `SYS_GATE_ABORT` | `(request_cap, reason) -> 0`：未决请求以确定性失败结束（v1 只允许 `ENOSYS`） |

约束：号段 1–72 已由 SPEC-152 冻结，本文**不重编号**；新增号只在实现时写入 `prima-abi`
并登记 `compat` 台账。`args[6]` 用 6 个 u64 搬运，不做结构体布局承诺。

### 请求状态机

```text
                 trap(被门控任务)
                        │
                        ▼
   (none) ──────► Queued ──gate_wait──► Delivered ──reply──► Replied ──► 请求者 rax=value
                    │                      │
                    │                      ├──abort────────► Aborted ──► 请求者 rax=-ENOSYS
                    │                      ├──中断/取消────► Cancelled ► 请求者 EINTR 或 RESTART 重放
                    │                      └──LibOS 死亡───► Orphaned ─► 请求者 rax=-ENOSYS
                    └──门控孤儿化/任务死亡──► Aborted
```

- `Queued → Delivered` 只能发生一次；同一请求不会被 `gate_wait` 返回两次。
- `Delivered → *` 只能一次；之后请求能力失效（I1）。
- `Replied` 之前请求者一直处于"被阻塞的 trap"状态：任务已保存返回帧，不在 CPU 上运行。

### 时序（同步路径）

1. 被门控任务执行 `syscall`（或 `int 0x80`）→ 内核入口。
2. 入口检查任务的门控绑定：命中则**冻结**当前返回帧、构造请求、入队。
3. 若门控有阻塞中的服务者 → 直接唤醒（复用现有直投模式），否则仅入队。
4. 服务者 `gate_wait` 取到请求能力（可能重试唤醒）。
5. 服务者按需 `gate_copy_in/out` 读参数、写结果缓冲。
6. 服务者 `gate_reply(req, value)`：内核校验请求仍为 `Delivered` → 写 `rax=value` →
   请求者置 Ready → 撤销请求能力 → 释放槽位 → 审计。
7. 请求者从 trap 返回（`sysret`/`iretq` 走既有路径），如同一次普通系统调用返回。

### 阻塞、唤醒与中断

- 请求者阻塞沿用既有模式（`mark_blocked_call` + 保存帧 + `sched::block_current()`），
  因此信号路径无需新机制：`interrupt_for_upcall` + `prepare_signal_resume` 已能把被阻塞
  调用变成 `EINTR` 或在 `RESTART` 标志下把 `rip` 回退 2 字节重放 `syscall` 指令。
- **取消**：请求者被中断时，其未决请求转 `Cancelled` 并撤销能力；服务者此后的 `reply`
  得到 `ENOENT`（审计记 `GateCancelled` + `GateReplyStale`）。`RESTART` 重放会重新 trap
  成**新**请求——同一系统调用被服务两次是 POSIX 允许的语义，不是重复投递。
- **队列满**：新 trap 阻塞（可被信号打断），不丢弃、不返回 `EAGAIN`——丢弃会让请求者的
  系统调用凭空消失，比阻塞更难审计。

### 失败、终止与确定性

| 情形 | 请求者观察 | 门控侧 | 审计 |
|---|---|---|---|
| 服务者 `abort(req, ENOSYS)` | `rax = -ENOSYS` | 请求被消费 | `GateAborted` |
| 服务者崩溃/被销毁 | `rax = -ENOSYS`（确定性恢复，不是挂起） | 未决请求全部 `Orphaned`；门控转 `Orphaned` | `GateOrphaned` + 每请求 `GateAborted` |
| 门控孤儿化后仍有被门控任务 trap | `rax = -ENOSYS`（直接失败，不阻塞） | — | `GateTrapOrphaned` |
| 答复两次 / 答复已取消的请求 | 无变化（第一次答复已生效） | `ENOENT` | `GateReplyStale` |
| 请求者被销毁 | 不适用 | 请求 `Cancelled` | `GateCancelled` |
| 容量不足（attach 时预检失败） | `ENOSPC` | — | — |

**为什么 LibOS 死亡用 `ENOSYS` 而不是新 errno**：内核不该知道"Linux 服务崩溃"意味着什么。
`ENOSYS` 在内核里已经表示"该调用不受支持"，是唯一不虚构语义的选择；把它映射成
`EIO`/`ECONNRESET` 之类属于 LibOS/服务管理器的策略（7-G）。这条限制写入 v1 文档。

## 实现与跨层影响

| 层 | 文件/符号（预计） | 内容 |
|---|---|---|
| 内核纯逻辑 | `kernel/src/gate.rs`（新） | Gate/Request 状态机、容量、审计顺序；host 单测 |
| 内核接线 | `kernel/src/syscall.rs` | 入口门控检查、6 个新入口、`Errno` 映射 |
| 内核状态 | `kernel/src/bootstrap.rs` | Gate/Request 容量 arena、任务销毁时的孤儿化、审计事件 |
| 权利/能力 | `kernel/src/rights.rs`、`handle.rs` | `GateAttach`/`GateServe`；`ObjectRef::Gate`/`Request` |
| ABI | `prima-abi/src/lib.rs` | 候选 73–78 与不透明参数约定（实现时登记） |
| 模型 | `cosir/`（POSIR/Lean 映射） | 见下 |

**模型影响（S6-X3 的关键）**：本机制在抽象层需要回答"被门控任务的 syscall trap 是什么
状态"。两个候选：

- **候选 A（推荐）**：请求者在 trap 期间处于既有 `BlockedWait`（等待对象＝请求），
  答复是一次 `wait_signal` + 帧写入。POSIR/COSIR 只需追加"trap 递送"操作与请求对象，
  审计事件沿用 `BlockedWait`/`Ready` 迁移，模型改动最小。
- **候选 B**：新增 POSIR 状态 `BlockedCall`。语义更直白，但要改状态机与不变量
  （`status = Faulted ⇔ pending_exc ≠ None` 一类），Lean 证明与既有 trace 断言需同步返工。

选择 A 的理由：既有 `Faulted`/`BlockedWait`/`Ready` 已能表达"不在 CPU 上、等待被唤醒、
由他人决定返回帧"，硬加状态只会让阶段 6 同时改机制与模型。**A 仍需一次 COSIR 评审**：
它必须证明"门控不引入 Linux 语义"且"审计链可重建"，否则退回 B。

容量（v1，全部预检、不动态增长）：门控 16 个、未决请求 64 个、每门控队列 8、单次
`gate_copy_*` ≤ 4096 字节。容量数值是实现期可调常量，不写入 ABI。

并发：单核（与现有全局状态一致）。D15 的按 CPU 分区在 7-B 处理；本文的门控表届时按域
迁移，不预留伪 SMP 接口。

## 验证与反例

| 层 | 输入 | 观察 | 通过标准 |
|---|---|---|---|
| host 单测 | 状态机序列（含重复 reply、abort 后 reply、取消后 reply、孤儿化、队列满、stale 句柄） | 返回码与审计序列 | 每序列确定性一致；I1–I4 每条至少一个用例 |
| host 单测 | 容量预检 | attach 失败不改变既有状态 | 失败原子 |
| QEMU `G1` | ring3 客户端（被门控）+ ring3 服务：服务改写 `rax` | 客户端读到被改写值，审计链完整 | 计划验收项"截获并改写 `rax`" |
| QEMU `G2` | 服务崩溃／服务在 `Delivered` 后不发答复／重复答复／客户端在等待中被信号打断 | 客户端分别得到 `-ENOSYS`、`-ENOSYS`、首次答复生效、`EINTR` 或 `RESTART` 重放 | 无永久阻塞、无重复写回 |
| QEMU `G3` | `gate_copy_in/out` 的越界、跨页、未映射、只读目标 | 确定性 errno | 失败不产生部分写入 |
| 计量 | 1000 次往返，TCG 与 KVM 分开记录 | 中位数/p99，分段（trap→入队、入队→服务、服务→答复、答复→恢复） | **只记录实测值**；计划的"≤5 µs"标为 PROVISIONAL（先例：D9 拒绝把未测量的 250 ns 写成出口门槛） |

反例必须在报告里保留：任何"加了超时所以不挂起"的说法都要替换成 I4 的三条终止路径。

## 增量与估算

| 增量 | 内容 | 估算（人周） |
|---|---|---|
| M1a | `gate.rs` 状态机 + 容量 + 审计序列（host 全测，不接线） | 1–1.5 |
| M1b | trap 入口拦截、冻结/恢复、`reply` 写 `rax`；QEMU `G1` | 1.5–2 |
| M1c | `gate_copy_in/out` 与边界负例；QEMU `G3` | 1 |
| M1d | 中断/取消/孤儿化/审计补齐；QEMU `G2` | 1 |
| M1e | 计量探针与报告、容量调参 | 0.5 |
| 合计 | | **5–6** |

**估算修订**：细化计划给 S6-M1 的估算是 3–5 人周。本设计细化了取消、孤儿化、审计与
代理复制后认为 5–6 更现实；差异来源是"可审计的失败路径"而不是新功能。若评审只接受
3–5，建议压缩 M1d/M1e（把审计与计量推到 S6-M5），但要明确记录这是范围削减。

## 开放问题（已裁决：D17–D22）

| # | 问题 | 裁决 |
|---|---|---|
| 1 | ABI 号段 73–78 可用吗 | **D17**：批准追加，逐项在实现时分配；1–72 不重编号；未实现的号不预留 |
| 2 | 答复只写 `rax` 还是整帧 | **D18**：v1 只写 `rax`；整帧答复另立机制，随 S6-M4 |
| 3 | 代理复制还是跨任务映射 | **D19**：v1 代理复制（≤4 KiB，仅用记录下来的参数地址）；映射需先有 7-D 撤销 |
| 4 | 门控任务的 `fork`/`exec` | **D20**：v1 由 LibOS 答复 `ENOSYS`，行为留 S6-M4；不得静默执行原生调用 |
| 5 | 模型候选 A 还是 B | **D21**：候选 A（复用 `BlockedWait`＋请求对象），附三条前置条件；不满足则退回 B |
| 6 | 队列满阻塞还是丢弃 | **D22**：阻塞（可被信号打断），不丢弃；队列 8/未决 64/门控 16，attach 时预检 |

裁决全文、依据与重裁触发见[S6-M1 递送机制的六项裁决](2026-10-29-stage6-m1-mechanism-decisions.md)。

## 兼容偏差与后继

- 与[细化计划](../plans/2026-10-27-stage6-7-refinement.md) S6-M1 第 2 条的偏差：计划写
  "LibOS 经 vspace 访问能力读写请求者内存"，本文改为**内核代理复制**。理由：跨任务映射
  需要撤销语义（7-D）与额外的页表权限面，v1 用一次复制换取"不新增授权面"。
- 不影响 profile v2/SPEC-152；不改阶段 6 的退出条件措辞。
- 后继：M1b 完成后补 `reports/` 报告并更新阶段跟踪 S6-D2；若评审选候选 B，本设计转
  `historical` 并另出状态机设计。
