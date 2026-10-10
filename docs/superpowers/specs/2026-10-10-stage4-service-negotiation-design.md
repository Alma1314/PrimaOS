# SPEC-088 设计：服务端解析与协商（阶段 4 S4-D2 前置）

> 状态：探索设计 + 实现；尚未验收，不构成 S4-D2 交付
>
> 前置：[SPEC-087](2026-10-10-stage4-init-client-handshake-design.md)（init ↔ 存活 ring3 客户端双向握手）、
> [SPEC-085](2026-10-10-stage4-ring3-runtime-hello-design.md)（单向 Hello）、
> [SPEC-084](2026-09-25-stage4-runtime-api-design.md)（Runtime API 原型）
>
> 未决来源：SPEC-085 报告 §6.1「init 尚未解析该 Hello；服务端解析、`minor`/feature 协商与端点选择留后续切片」、
> SPEC-087 设计 §8.2「`_start` 第三个参数仍未使用；服务管理器切片再引入多服务与端点选择」。

## 1. 目标与边界

SPEC-085/087 里 **init 从不解析**客户端 Hello，只回一个固定字。本切片让一个**独立 ring3
Rust 服务端点**真实解析客户端 Hello 的 wire `MessageHeader` + `ProtocolOffer`，调用
`prima_runtime::protocol::negotiate`，把**协商结果**与两个**负例**（`MajorMismatch`、`BadMagic`）
写回共享帧；内核从同一物理帧读回并逐位比对。

本切片不实现完整服务管理器、日志/存储服务或驱动，不冻结 syscall 编号，不勾选 S4-D1/S4-D2
或阶段 4 任何交付项/退出条件。

## 2. 拓扑：init 作为 broker

Capability 模型下 sibling 任务无法互相寻址（`SYS_SEND` 要求 `send_cap` 的目标恰为
`receiver`，且新任务 id 由 `next_free_id` 动态分配），故 **init 必须作为 broker 中转**——
这正是 S4-D2「服务管理器」的本意。流程：

1. init 在**既有页面 1 握手块末尾**追加：先 `SYS_CAP_DERIVE` 从
   `TASK_CREATE_CODE_CAP` 派生一条只读（`{SegMapR}`）**端点令牌** cap；
2. init 第 4 次 `SYS_TASK_CREATE` 建 **Rust 服务任务**：复用 `TASK_REQ_VA` 块，改写
   `entry`（Rust ELF 入口）、`initial_cap0 = TASK_CREATE_SEND_CAP`（可向 task 1 发送）、
   `initial_cap1 = 端点令牌`；再把栈段映射进服务根；
3. 服务启动：`task_id()` → `send(1, cap0, SVC_ID_TAG|id, [])` → `receive()` 阻塞；
4. init `recv` 循环直到带 `SVC_ID_TAG` 的消息，得到 `svc_id`；
5. init 把**客户端已递交的 Hello 段 cap 转发**给服务：
   `send(svc_id, svc_mgr_cap, client_id, [seg_cap])`（seg cap 是 SPEC-087 里客户端经 IPC
   移交、init 已映射进自己根的那条）；
6. 服务收到 → `memseg_map(seg, self_cap, 0, SVC_SHARED_VA, 1, R|W)` → 读回客户端写的
   Hello wire 字节 → `MessageHeader::decode` + `ProtocolOffer::decode` → `negotiate` →
   把结果两字写到段内 offset 128/136 → `send(1, cap0, SVC_DONE_TAG, [])`；
7. init `recv` 直到 `SVC_DONE_TAG`，随后内核见证从 init 自己的 `USER_HELLO2_VA` 映射读回
   offset 128/136。

**角色选择**：`_start` 第 3 参数（`rdx` = `initial_caps[1]`）**非零 = 服务端点**，零 = 客户端
（SPEC-085/087 的客户端行为逐字节不变）。端点令牌是派生的只读代码段 cap（**不含
`SegMapX`/`SegDestroy`**，符合最小权限；S4-X2 关注的正是这点）。

## 3. 协商期望与判决性

客户端 offer 由 `prima-smoke` 在 ring3 生成并已被 SPEC-085 冻结：
`major=1, minor=0, features=0b1, required=0b1, max=4096`。

服务端 offer（本片固定）：`major=1, minor=0, features=0b101, required=0b001, max=2048`。

`negotiate` 结果：`major=1`，`minor=min(0,0)=0`，`features=0b1 & 0b101 = 0b1`，
`max=min(4096,2048)=2048`。打包成一个字
`word0 = major<<48 | minor<<32 | max<<16 | (features & 0xffff) = 0x0001_0000_0800_0001`。

两个负例（**纯 ring3 Rust 计算**，不需要额外 wire；码由服务端显式映射，不依赖枚举判别式）：
- `negotiate(client, {major: 2, ...})` ⇒ `NegotiationError::MajorMismatch`（显式码 1）；
- `MessageHeader::decode(魔数错误缓冲)` ⇒ `CodecError::BadMagic`（显式码 0）。
打包 `word1 = MajorMismatch<<32 | BadMagic = 0x0000_0001_0000_0000`。

**判决性**：`word0` 里的 `max=2048` 只可能来自**服务端 offer**（客户端是 4096）；
`word1` 的两个负例码只可能来自真实 `decode`/`negotiate`——回显或常量都能被这两个字排除。
两字由 **ring3 Rust 服务**写入共享帧、由内核从 init 同一物理帧读回。

## 4. 已知回归面（逐条登记，非静默改写）

| 事件 | 变化 | 原因 |
|---|---|---|
| 新增 `service_negotiated_ready` | seq 27；其后事件顺移 1 位（`boot_complete` 40→41） | 本切片新事件 |
| `task_ready` | `tasks`/`tasks_after` 4→5；`caps`/`caps_after` 49→54 | 服务任务计入存活任务；服务端点持有端点令牌 + 回发凭据 + manager/self/root-ns 共 5 条 cap |
| `ipc_ready` | queued/direct/blocks 4/5/6→**6/7/9** | 服务自报 id、forward 请求（queued）、无 cap 唤醒（direct）、服务与 init 的阻塞 |
| `memory_ready`/`bootstrap_ready`/`frames_ready`/`self_root_ready`/`preempt_ready`/`state_audit_ready` | 布局/计数类字段 | KStack BSS +16 KiB、嵌入 ELF 增长、多一个任务根 |

**必须解决的既有约束**：`USER_TASK_KSTACK_COUNT` 只覆盖 id 3..8（`bootstrap.rs:75` 明写
「id ≥ 9 会 `NoSpace`」）。SPEC-087 的客户端已占 id 8，服务拿到 id 9，故本片把
`USER_TASK_KSTACK_COUNT` 6→7。另 `MAX_VSPACE_ENTRIES` 32 被服务的 10 页代码 + 1 页栈映射
挤满，本片 32→44 并给 `VspaceEntry.task_id` 换 `NonZeroU32` 提供 niche（每槽 40→32 B），
使容量 44 时 `BootstrapState` 仍在 13 KiB 守卫内——**不放宽守卫**。

## 5. 判据

1. `cargo xtask test` 断言 41 条精确序列事件与 `validate_service_negotiated_data`；
2. 反例单测 `rejects_service_negotiated_when_result_is_echoed`（把 `word0` 改成客户端 offer 的
   原值即测试失败）证明判决性；
3. 同一镜像连跑两次 41 条事件逐字节相同；
4. `cargo test --workspace`、`cargo xtask lint`、`cargo xtask tcb`、`cargo xtask compat` 全绿。

## 6. 未决项

1. **仅一次服务端解析+协商**：不构成服务互通、调度或安全证明，不勾选 S4-D2 或阶段 4。
2. 服务端 offer 与端点选择仍由内核侧 init 写死的单一端点，未引入多服务名称解析。
3. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
