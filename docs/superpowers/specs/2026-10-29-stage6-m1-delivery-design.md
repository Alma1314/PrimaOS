---
id: spec-stage6-m1-delivery
kind: specification
authority: proposed-mechanism-design
status: design-proposal
lifecycle: current
updated: 2026-10-30
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
  确定了六条路线选择。后续[六项决定复审](../reports/2026-10-29-stage6-m1-decision-review.md)
  发现 R1–R6 接线前缺口，本文按复审修订；[协议收口](2026-10-29-stage6-m1-protocol.md)现已确定 M1a 协议，
  [整改评审](../reports/2026-10-29-stage6-m1-blocker-closure.md)包含模型与 7-A QEMU 双轮探索。
  机器 ABI 与内核接线尚未发布，不把模型通过当作 S6-D2 完成。
- **决定（本提案主张，需评审确认）**：
  1. 门控（gate）是**能力对象**，不是任务的隐藏属性；挂接、服务、答复都由能力授权。
  2. 门控期间**分派被取代**：被门控任务的一切 `syscall`/`int 0x80` 陷阱原样交给门控，
     内核不解释调用号与参数（S6-X3 的操作化定义）。
  3. 请求者内存由内核**代理复制**（`copy_in`/`copy_out`），v1 不做跨任务映射授权。
  4. 答复**至多一次**：`reply`/`abort` 消费请求，所有请求能力别名随即失效；不保证外部副作用 exactly-once。
  5. 不存在内核超时；服务存活但不答复时可能无限等待。中断、任务终止与服务死亡
     提供退出路径，不证明完成时限（见[失败与终止](#失败终止与确定性)）。

## 成功与失败契约

### 对象与能力

| 对象 | 内容 | 生命周期 |
|---|---|---|
| `Gate` | 固定服务 Task、被门控任务集合、队列（有界 FIFO）、等待者、状态 | 由 `gate_attach` 创建；管理/服务授权消失时孤儿化，绑定失败关闭，不回退原生分派 |
| `GateRequest` | 请求任务 id、保存的返回帧、原始 `number`/`args`、状态 | 每次被拦下的 trap 一个；由 `reply`/`abort` 或取消路径消费 |
| 请求能力 | 指向单个 `GateRequest` 的派生能力 | `gate_wait` 返回；答复后撤销；stale 句柄确定性失败 |

新增权利（保持既有位序；位集已扩宽到 u64，追加位 31/32 在实际接线时才加入枚举）：

- `GateAttach`：把一个 Task 挂到门控上（还需持有该 Task 且含 `TaskManageVSpace` 的能力；调用者即固定服务者，详见收口协议）。
- `GateServe`：等待/答复/代理复制，只能由被授权的服务任务持有。

不变式：

- **I1（一次性）**：一个请求最多一次有效答复；`reply`/`abort` 都撤销请求能力并释放槽位，
  第二次调用得到 `ENOENT`（stale）；请求对象终结状态约束全部派生/移动别名。
- **I2（分派被取代）**：任务处于门控挂接状态时，原生分派表**不被查询**；被门控任务不能
  自行调用任何原生 syscall（这是权限收窄，不是便利性限制）。
- **I3（零 Linux 语义）**：`number`、6 个参数、答复值都按不透明 `u64` 搬运；内核不比较
  调用号、不选择 Linux errno、不读参数指向的内容（只有 `gate_copy_*` 才读）；孤儿返回值由可信挂接者配置。
- **I4（退出与清理完整性）**：`reply`、`abort`、请求者终止及服务孤儿化
  均有确定清理路径；审计耗尽则按收口协议全局 fail-stop，不继续调度门控域。服务存活但不答复时无完成
  时限保证。活性依赖服务/监督者进展，不由此安全性质推出。

### ABI（候选号段，未分配；M1a 参数已定义、机器接口未冻结）

下表使用[协议收口](2026-10-29-stage6-m1-protocol.md)的布局；机器接口仍须由 M1b 实现和验证后登记。

| 候选号 | 名字 | 签名（语义） |
|---|---|---|
| 73 | `SYS_GATE_ATTACH` | `(task_cap, gate_or_zero, orphan_value, 0, 0, 0) -> gate_cap` |
| 74 | `SYS_GATE_WAIT` | `(gate_cap, output_ptr, bytes=96, 0, 0, 0) -> 0`：输出版本、句柄、task_cookie、原始 number/args 和 flags |
| 75 | `SYS_GATE_COPY_IN` | `(request_cap, client_va, len, service_dst, 0, 0) -> len` |
| 76 | `SYS_GATE_COPY_OUT` | `(request_cap, client_va, len, service_src, 0, 0) -> len` |
| 77 | `SYS_GATE_REPLY` | `(request_cap, value) -> 0`：终结并仅改写客户端 rax |
| 78 | `SYS_GATE_ABORT` | `(request_cap, value) -> 0`：同样搬运不透明值，审计原因不同 |

约束：号段 1–72 已由 SPEC-152 冻结，本文**不重编号**；新增号只在实现时写入 `prima-abi`
并登记 `compat` 台账。`args[6]` 用 6 个 u64 搬运；输出固定 96 字节布局、版本和 trap 来源标志见收口协议。
记录 int 0x80 不自动构成 Linux i386 ABI 支持，阶段 6 仍限 x86_64。

### 请求状态机

```text
                 trap(被门控任务)
                        │
                        ▼
   (none) ──────► Queued ──gate_wait──► Delivered ──reply──► Replied ──► 请求者 rax=value
                    │                      │
                    │                      ├──abort────────► Aborted ──► 请求者 rax=reason
                    │                      ├──请求者死亡───► Cancelled ► 不恢复帧
                    │                      └──服务死亡─────► Orphaned ─► 请求者 rax=配置值
                    └──门控孤儿化/任务死亡──► Aborted
```

- 队列满时先进入 `WaitingCapacity`，按 FIFO 晋级到 `Queued`；两者均占已预留的请求槽。
- `Queued → Delivered` 只能发生一次；同一请求不会被 `gate_wait` 返回两次。
- `Delivered → *` 只能一次；之后请求能力失效（I1）。
- `Replied` 之前请求者一直处于"被阻塞的 trap"状态：任务已保存返回帧，不在 CPU 上运行。

### 时序（同步路径）

1. 被门控任务执行 `syscall`（或 `int 0x80`）→ 内核入口。
2. 入口在所有原生分派前检查绑定：命中则**冻结**返回帧，使用预留槽构造请求，
   进入 Queued 或 WaitingCapacity；孤儿绑定直接用配置值终结。
3. 若门控有阻塞中的服务者 → 直接唤醒（复用现有直投模式），否则仅入队。
4. 服务者 `gate_wait` 取到请求能力（可能重试唤醒）。
5. 服务者按需 `gate_copy_in/out` 读参数、写结果缓冲。
6. 服务者 `gate_reply(req, value)`：内核校验 Delivered、完成授权、句柄/审计容量，
   再将写 `rax=value`、等待完成/Ready、请求别名失效、槽位释放和审计作为一次原子提交。
7. 请求者从 trap 返回（`sysret`/`iretq` 走既有路径），如同一次普通系统调用返回。

### 阻塞、唤醒与中断

- 可复用保存帧和 `sched::block_current()` 的底层组件；现有 `interrupt_for_upcall`
  没有 GateRequest 清理逻辑，`prepare_signal_resume` 的原生号特例不能用于不透明客户号。
- v1 所有客户端门控等待均设为 `alertable=false`、无 deadline；信号排队，任何阶段均不自动
  取消、返回 EINTR 或重放。服务可 reply/abort；请求者销毁不恢复帧。Linux 信号语义留 M4。
- 队列满用预留槽记录 WaitingCapacity，按 FIFO 晋级；请求者销毁清理并归还绑定槽。
  明确 EAGAIN 并审计也是可审计拒绝，v1 选择阻塞是背压策略。

### 失败、终止与确定性

| 情形 | 请求者观察 | 门控侧 |
|---|---|---|
| reply/abort | rax 为不透明 value | 一次终结，别名全部失效 |
| 服务死亡/最后服务授权消失 | rax 为 attach 配置值 | Gate 孤儿化，全部请求终结；绑定不回退原生 |
| 孤儿绑定再次 trap | rax 为配置值 | 不查询原生调用表 |
| 服务存活但不答复 | 可无限阻塞 | 监督者可显式终止服务 |
| 信号/upcall | 保留排队 | 不取消，不重放；M4 再定义信号语义 |
| 请求者销毁 | 不恢复帧 | 清理等待、请求别名与绑定预留 |
| 审计耗尽 | 门控任务不再被调度 | 全局 fail-stop；预留首次故障诊断，不伪造正常失败返回 |

Linux ENOSYS 由 LibOS 经普通 reply/abort 搬运；内核不比较 errno 或客户端调用号。
具体能力交付、最后授权丢失、输出原子性与资源耗尽策略见[收口协议](2026-10-29-stage6-m1-protocol.md)。

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
  需要真实专属 WaitObject 与 W17 双向注册，答复以受控的帧写入、等待完成和审计原子提交。
  普通 WaitSignal 不得绕过请求终结授权；不能仅更改 Task 状态或依赖状态机外约定。
- **候选 B**：新增 POSIR 状态 `BlockedCall`。语义更直白，但要改状态机与不变量
  （`status = Faulted ⇔ pending_exc ≠ None` 一类），Lean 证明与既有 trace 断言需同步返工。

选择 A 的理由：既有 `Faulted`/`BlockedWait`/`Ready` 已能表达"不在 CPU 上、等待被唤醒、
由他人决定返回帧"，硬加状态只会让阶段 6 同时改机制与模型。A 已通过[本次 COSIR 技术设计评审](../reports/2026-10-29-stage6-m1-blocker-closure.md)，
仅认可 M1a 的零 Linux 语义与 W17 投影；真实内核审计/调度的细化验证仍是 M1b 义务。

容量与拓扑已由收口协议选定：单服务、多客户端，门控 16、绑定 64、队列 8、单次 copy≤4096。
attach 实际预留每绑定 Task 的请求槽/私有等待对象，不以“当前空闲”代替预留；host 模型验证
64 请求满载及请求者销毁后的 FIFO 晋级。数值不是机器 ABI 保证。

复制明确由可信 TaskManageVSpace 委托整个目标用户地址空间；client_va 支持嵌套指针。
请求与目标 Task/root 身份绑定，双端范围/权限/COW 全预检后提交；v1 不准入共享 root。
原来的 args+offset 不构成授权收窄，已显式改为直接地址。硬件页表/分配/并发验证仍属 M1c。

并发：单核（与现有全局状态一致）。D15 的按 CPU 分区在 7-B 处理；本文的门控表届时按域
迁移，不预留伪 SMP 接口。

## 验证与反例

| 层 | 输入 | 观察 | 通过标准 |
|---|---|---|---|
| host 单测 | 状态机序列（含重复 reply、abort 后 reply、请求者销毁后 reply、孤儿化、队列满、stale 句柄） | 返回码与审计序列 | 每序列确定性一致；I1–I4 每条至少一个用例 |
| host 单测 | 容量预检 | attach 失败不改变既有状态 | 失败原子 |
| QEMU `G1` | ring3 客户端（被门控）+ ring3 服务：服务改写 `rax` | 客户端读到被改写值，审计链完整 | 计划验收项"截获并改写 `rax`" |
| QEMU `G2` | 服务死亡／服务不答复后监督者显式终止服务／重复答复／任意阶段信号排队及请求者销毁 | 孤儿路径得到配置值；首次答复生效；信号不取消、不自动重放，不发生终结后迟到写回 | 清理完整、无重复写回；不宣称无监督动作也能有界完成 |
| QEMU `G3` | `gate_copy_in/out` 的越界、跨页、未映射、只读目标 | 确定性 errno | 失败不产生部分写入 |
| 计量 | 1000 次往返，TCG 与 KVM 分开记录 | 中位数/p99，分段（trap→入队、入队→服务、服务→答复、答复→恢复） | **只记录实测值**；计划的"≤5 µs"标为 PROVISIONAL（先例：D9 拒绝把未测量的 250 ns 写成出口门槛） |

反例必须保留，重点覆盖复审 R1–R6：服务不进展、交付后副作用/取消、两任务同 VA、
跨页 COW、派生请求别名、权限未知高位、容量/审计耗尽及失败关闭。退出路径不证明时限。

## 增量与估算

下表是原草案估算；复审 R1–R6 的协议及权限表示修订未估时，不能当作已确认的交付承诺。

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
3–5，可将计量推到 S6-M5 并记录范围变化；取消/孤儿清理与原子审计属于安全必需，
不得从首个接线增量中省略。

## 路线选择（D17–D22；复审后仍有冻结前义务）

| # | 问题 | 裁决 |
|---|---|---|
| 1 | ABI 号段 73–78 可用吗 | **D17**：批准追加，逐项在实现时分配；1–72 不重编号；未实现的号不预留 |
| 2 | 答复只写 `rax` 还是整帧 | **D18**：v1 只写 `rax`；整帧答复另立机制，随 S6-M4 |
| 3 | 代理复制还是跨任务映射 | **D19**：v1 代理复制（≤4 KiB，整个目标用户地址空间委托，直接 client_va）；映射需先有 7-D 撤销 |
| 4 | 门控任务的 `fork`/`exec` | **D20**：v1 由 LibOS 答复 `ENOSYS`，行为留 S6-M4；不得静默执行原生调用 |
| 5 | 模型候选 A 还是 B | **D21**：优先 A；须满足 W17、专属完成授权、别名失效与原子审计，已通过 M1a 技术设计评审，内核细化待 G1–G3 |
| 6 | 队列满阻塞还是丢弃 | **D22**：阻塞背压；需实际预留与容量等待账目；v1 不可警报等待、无自动取消/重放，无时限保证 |

裁决全文、依据与重裁触发见[S6-M1 递送机制的六项裁决](2026-10-29-stage6-m1-mechanism-decisions.md)。

## 兼容偏差与后继

- 与[细化计划](../plans/2026-10-27-stage6-7-refinement.md) S6-M1 第 2 条的偏差：计划写
  "LibOS 经 vspace 访问能力读写请求者内存"，本文改为**内核代理复制**。理由：跨任务映射
  需要撤销语义（7-D）与额外的页表权限面，v1 用一次复制换取"不新增授权面"。
- 不影响 profile v2/SPEC-152；不改阶段 6 的退出条件措辞。
- 后继：M1b 完成后补 `reports/` 报告并更新阶段跟踪 S6-D2；若评审选候选 B，本设计转
  `historical` 并另出状态机设计。

## 冻结前义务（2026-10-29 复审）

前序 [R1–R6](../reports/2026-10-29-stage6-m1-decision-review.md)的定义阻塞由
[收口协议](2026-10-29-stage6-m1-protocol.md)与
[整改评审](../reports/2026-10-29-stage6-m1-blocker-closure.md)逐项响应，7-A 有界 QEMU 探索已取得双轮见证。
M1a 可以按已确定协议推进；提案仍不等于机器 ABI 发布、COSIR 冻结对象扩展或 S6-D2 完成。
M1b/M1c 必须实现协议列出的能力丢失钩子、私有等待、审计 fail-stop、跨任务页表与 G1–G3；
发现模型无法细化时重开对应 R 项，不能把本报告当作豁免。D17–D22 原文保留历史依据与显式后继。
