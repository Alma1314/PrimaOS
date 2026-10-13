# SPEC-104（主体）报告：注册表/按名解析/路由转发迁入 ring3 servicemgr

> 状态：探索实现；不构成 S4-D2/D3 验收或稳定 ABI
>
> 设计依据：[把服务管理器迁到 ring3 Rust](../specs/2026-10-13-stage4-ring3-service-manager-design.md) §2–§4
>
> 前置：[SPEC-104 前置片：对端 task id 的入口 ABI](2026-10-14-stage4-peer-id-entry-report.md)
>
> 基线：前置片 HEAD `c835b20`

## 1. 本片做了什么

把**运行期注册表 / 按名解析 / 请求路由与转发**从 `init` 的手写 x86 机器码
（`kernel/src/user.rs` 的 SPEC-088/090/092/093/095 块）迁入独立的 ring3 Rust 任务
`prima-servicemgr`（SPEC-103 建起）。迁移后：

- `init` 退化为**引导 + 载入**：建 `servicemgr`（**最先**）、从它的管理 cap 派生三条指向它的
  发送凭据（`M_send`，作客户端/两端点的 `initial_cap0`）、建客户端与两端点、并在收尾收下
  `servicemgr` 交回的共享段只读副本。
- **端点/客户端以 `servicemgr` 为对端**（对端 id 由创建时经入口 `r8` 运行期指派）；自报/done
  直达管理器，`prepare` 与共享段 cap 由管理器转发。
- `prima-servicemgr` 持有运行期注册表，收到两端点自报（`(name,id)` + 自管理凭据的派生副本）
  后：按 `ALPHA` 行为首次转发、读客户端写入共享段的**请求名**并解析、按解析结果二次转发、
  应答客户端 `PONG`、把共享段只读副本交回 `init`。消息处理是**顺序无关的事件循环**。

## 2. 判决性见证（尽量冻结既有事件）

既有事件 `service_reply/negotiated/selected` 的**值**基本不变，但**产出方与 task id** 改变。
内核见证函数改从**共享段固定偏移**读回由管理器产出的值
（`192`=管理器 id、`200`=二进制标记、`208`=resolved、`216`=unknown、`224`=beta done、
`232/240`=两端点注册名、`248`=beta 自报 id、`256`=alpha done、`264`=客户端 id）。

新增区分性字段（见证据此证明注册表/解析由**独立 ring3 映像**产出）：
`servicemgr_ready` 增 `self_id`（=管理器自身 task id，须等于从管理 cap 解出的 id）与
`marker`（=`0x5345_5256_4D47_5203`，编译进 `prima-servicemgr` 的标记）。

## 3. 逐条登记的既有事件变化

任务创建顺序重排（`servicemgr` 最先）使后续 id 顺移：`servicemgr` 8、客户端 9、`ALPHA` 10、
`BETA` 11（原为 客户端 8、`ALPHA` 9、`BETA` 10、`servicemgr` 11）。据此变化的事件：

| seq | 事件 | 变化 |
|---|---|---|
| 23 | `ipc_ready` | `queued/direct/blocks` `9/9/13` → **`10/14/18`**（迁移新增的 Hello/自报/wake/PONG/段交回等 IPC） |
| 24 | `task_ready` | `caps`/`caps_after` `69` → **`77`**（迁移新增的存活 cap：三条 `M_send`、两端点发送权副本、客户端发送权副本、管理器派生的段副本等）；其余字段不变 |
| 26 | `service_reply_ready` | `client` `0x100000018` → **`0x10000001c`**；`recv`（客户端 id）`8` → **`9`**；`echo` 不变（仍为 `SERVICE_REPLY_WORD`） |
| 27 | `service_negotiated_ready` | `token` `0x10000001c` → **`0x200000033`**；`service` `0x10000001d` → **`0x10000003e`**；`svc_id`（`ALPHA` id）`9` → **`10`**；`forward`/`done`/`negotiated`/`negatives` 不变 |
| 28 | `service_selected_ready` | `alpha_ep` `9` → **`10`**；`beta_ep` `10` → **`11`**；`beta_self` `10` → **`11`**；`client_ep` `8` → **`9`**；`client_stack_seg` `0x10` → **`0x18`**；`name`/`resolved`/`unknown`/`done`/`negotiated_beta`/两 marker/`alpha_stack_seg`/`beta_stack_seg`/两 reg name 不变 |
| 29 | `servicemgr_ready` | `task_id` `0xb` → **`0x8`**；`hello` `0x…0b` → **`0x5e56_0000_0000_0008`**；**新增** `self_id=0x8`、`marker=0x534552564d475203` |

**其余事件（seq 1–22、25、30–43）逐字节不变。**

## 4. 其他登记

- `MAX_CAPABILITY_SLOTS` `72` → **`80`**：迁移后峰值越过 72（三条运行期派生的 `M_send` 常驻子任务；
  原 bootstrap 发送凭据仍铸留、不再移动；客户端 Hello 与两端点自报各多携一条自权副本；管理器
  另派生三份段 cap）。按 SPEC-074/091/094 先例扩展并**照实重测** `BootstrapState` 守卫（≤13 KiB，
  仍未放宽）与 TCB 指标。
- 机制层行数 **减少**（`mechanism_lines` 21292 → 21029）：`init` 手写的注册表/解析/转发汇编被删除，
  换为 ring3 Rust——这是迁移的直接目标。TCB `trusted_lines=1262` 不变，ratio 5.60% → 5.66%。

## 5. 迁移中发现的三处硬约束（已在实现中处理）

1. **向 `BlockedRecv` 接收者投递带 cap 消息被拒**（`syscall.rs::send_call`）。管理器常态阻塞在
   `recv`，故端点/客户端在发带 cap 消息前先发**无 cap 唤醒**（`SERVICE_WAKE_TAG`，发送方连发、
   中间不让出）。对引导任务（id 1）**不**发唤醒——其握手块按**单条**消息接收。
2. **端点的 `self_cap` 是 `memseg_map` 的目标 cap，不能移动**。端点自报移交的是 `self_cap` 的
   **派生副本**（仅 `TaskSend`），原 cap 保留在端点自己手里。
3. **客户端写回应答的时序**：管理器在把段交回 `init` **之前**自旋等待客户端把 `PONG` 写回
   段内 offset 64，否则 `init` 可能先 `int3`，内核见证读到未写入的 0。

## 6. 验证（2026-10-14）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 146、cosir_state 42、lean_backtrace 3、kernel 369、prima-ostd 21、prima-runtime 1、xtask 206 |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=21029 total_lines=22291 ratio=5.66%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（同镜像连跑两次逐字节相同） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

## 7. 未决与登记

1. `prima-servicemgr` 的**端点创建**仍由 `init` 手写（本片只迁注册表/解析/路由/转发）；表驱动
   创建（自解 ELF 入口）留 SPEC-105。
2. 本片注册表为编译期已知两项；新增服务仍需一个二进制 + 表项扩展（SPEC-105 一并处理）。
3. **不勾选** S4-D2/D3 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
