# SPEC-087 设计：init ↔ 存活 ring3 客户端双向握手

> 状态：探索设计 + 实现；尚未验收，不构成 S4-D2 交付
>
> 前置：[SPEC-085](2026-10-10-stage4-ring3-runtime-hello-design.md)（单向 Hello 握手）、
> [SPEC-084](2026-09-25-stage4-runtime-api-design.md)（Runtime API 原型）

## 1. 目标与边界

SPEC-085 的握手是**单向**的：独立 ring3 Rust 客户端经 `prima-runtime` 把 Hello 连同共享
Segment Capability 移交给 init，内核从共享帧读回 wire 字节；但 **init 没有解析也没有回复**。
本切片让 init 与一个**存活的** Rust 客户端完成一次真实的双向握手：客户端发出 Hello →
init 收到、把共享 Segment 映射进自己的根、应答客户端 → 客户端收下应答并把 payload 写回
共享帧 → init 侧读回该 payload 逐位比对。

本切片不实现服务管理器（S4-D2）、日志/存储服务、驱动或示例应用，不冻结 syscall 编号，
不勾选 S4-D1 或阶段 4 任何交付项/退出条件。

## 2. Ring3 客户端（存活任务）

- `prima-user/src/main.rs` 的 `_start` 保持既有流程（`task_id` → `time_now` →
  `memseg_create` → `memseg_map` → `write_user_bytes` → `send`），随后按 id 分流：
  - **id 3**（SPEC-085 的历史见证载体）行为**逐字节冻结**——发完 Hello 即进入
    `loop { yield_task() }`（它在 init 的 S2-D4 块中被销毁）。
  - **其它 id**（本切片新建的存活客户端）在 `send` 之后 `receive()` 等待 init 的应答，
    把应答 payload 写回共享段 `USER_HELLO_VA + SERVICE_REPLY_OFFSET`，然后进入
    `loop { receive() }` **阻塞等待**（不再是 yield 循环）。
- 结束后阻塞而非持续 Ready 是必须的：收尾的 PIT 抢占阶段只轮转 task 1/2，一个持续
  Ready 的客户端会被 `pick_next` 选中并打乱轮转。

## 3. init 侧握手块（代码页第 1 页）

- `USER_CODE_LEN` 由 4096 提升为 **8192**（2 页），页 0 尾部 `jmp rel32` 跳到页 1
  （`USER_CODE2_VA = USER_BASE_VA + 0x9000`，避开既有的 `USER_SEG_VA`）。
- init 在页 1：复用 task 3 的请求块，只改写 `entry`（Rust ELF 入口）与 `initial_cap0`
  （新铸的 `CLIENT_SEND_CAP`，槽 52）→ `SYS_TASK_CREATE` 建**存活**客户端 →
  把栈段映射进客户端根 → 让出两次（排空 task 2 后轮到客户端执行发送）→ 循环
  `SYS_RECV` 直到取到**带 cap** 的 Hello（payload = 客户端 id，cap0 = 共享段）→
  把共享段映射进 init 根（`USER_HELLO2_VA`）→ `SYS_SEND` 应答 `SERVICE_REPLY_WORD` →
  让出一次等客户端写回执 → **再直投唤醒 task 2**（见下）→ `int3` 收尾。

## 4. 本切片定位并修复的两个真实缺陷

### 4.1 `syscall` 快路径的阻塞/让出上下文（`prima-ostd/src/entry.rs`）

`syscall` 指令**不换栈、不压帧**，故快路径入口原先把 15 个 GPR 压栈后即把 `rbp` 当保存块，
但**既不写 `CONTEXT_BASE`，也没有模拟中断路径的 5-qword 硬件帧**。后果：
`prima_ostd::entry::SaveBlock::to_sret_frame` 在快路径上从保存块的 `0x78..0x98` 读到的是
`SYSCALL_STACK` 上的陈旧字节（不是 `rip/cs/rflags/rsp/ss`）。于是**任何经快路径发起的
阻塞/让出**（本切片的客户端 `receive()`）会把**上一次 `int 0x80` 的外来上下文**存进本任务
的调度帧；恢复时 `iretq` 落到一个未映射地址，触发 `#PF` 死循环（QEMU 实测：`t=8`
`cr2==rip==0xa0000009093`，即 init 的 recv 循环地址）。

修复：入口在 15 个 GPR 之下压入 `ss/rsp/rflags/cs/rip`（值取自 `FAST_SYSCALL_SS`、
`USER_RSP`、`r11`、`FAST_SYSCALL_CS`、`rcx`），使保存块与中断路径逐槽一致；随后
`mov qword ptr [rip + CONTEXT_BASE], rbp`（与 `int 0x80` 一致）。新增
`set_fast_syscall_ss`，`configure_fast_path` 同时设用户 CS/SS。

### 4.2 握手让出把 task 2 排空（`kernel/src/user.rs`）

S2-D4 块末尾已有一段注释：第一次让出会把 task 2 排空成 `BlockedRecv`，故需再直投唤醒一次，
使收尾抢占阶段有 Ready 任务可轮转。本切片的握手块**再让出两次**，又把它排空——若不处理，
抢占阶段 `pick_next` 返回 `None`，`preempt_tick` 早退，`preempt_ready` 永不发出（实机表现为
挂死）。修复：握手块末尾复制同一次直投唤醒
`send(2, TASK2_SEND_CAP, TASK2_WAKE_PAYLOAD, 0, 0, 0)`。

## 5. 回归与判决性证据

- 新事件 `service_reply_ready`（**seq 26**）：`data=client=0x100000018;stack=0x0;recv=0x8;`
  `map=0x0;reply=0x0;echo=0x38001474e4f50`。
  - `client` = `SYS_TASK_CREATE` 返回的 manager cap（槽 24 gen1）；
  - `stack`/`map`/`reply` = 0：栈映射进客户端根、共享段映射进 init 根、应答发送均成功；
  - `recv=0x8` = 客户端自报的 task id（Hello payload）；
  - `echo=0x38001474e4f50` = `SERVICE_REPLY_WORD`（`"PONG" | opcode 0x8001 | request_id 3`），
    **由客户端在收到应答后写入共享段、由 init 从同一物理帧读回**——判决性在于"谁写的"。
- `task_create` 见证记录收紧：只记前两次 create 的 `child`/`grandchild`；本切片的第 3 次
  create 不再覆盖 `grandchild`，从而保持 `task4_recv`/`root4_cleared` 的语义不变。

## 6. 已知回归面（逐条登记，非静默改写）

| 事件 | 变化 | 原因 |
|---|---|---|
| 新增 `service_reply_ready` | seq 26，其后事件顺移 1 位（`boot_complete` 39→40） | 本切片新事件 |
| `task_ready` | `tasks`/`tasks_after` 3→4、`caps`/`caps_after` 44→49 | 存活客户端计入存活任务、持有 4 条 cap |
| `ipc_ready` | `queued/direct/blocks` 3/3/3→4/5/6 | 客户端 Hello（queued）、应答与 task 2 再唤醒（direct）、客户端阻塞（blocks） |
| `platform_segments_ready` | `vspace` 12→13、`frames[4]` 1→2 | init 把共享段映射进自己根（+1 映射）；task 1 代码 1→2 页 |
| `memory_ready`/`bootstrap_ready`/`frames_ready`/`self_root_ready`/`preempt_ready` | 布局类字段 | 用户 ELF/代码页扩大使镜像与物理帧序移动（构造性后果） |

## 7. 判据

1. `cargo xtask test` 断言 40 条精确序列事件与 `validate_service_reply_data`；
2. 反例单测 `rejects_service_reply_when_echo_is_missing` 证明该判定有效；
3. 同一镜像连跑两次 40 条事件逐字节相同；
4. `cargo test --workspace`、`cargo xtask lint`、`cargo xtask tcb`、`cargo xtask compat` 全绿。

## 8. 未决项

1. **仅证明"一次双向应用层握手"**：不构成服务互通、调度或安全证明，不勾选 S4-D2 或阶段 4。
2. **客户端为单实例探针**：`_start` 第三个参数仍未使用；服务管理器切片再引入多服务与端点选择。
3. **快路径保存块与中断帧的逐槽一致**只经 `to_sret_frame` 复用；未做 PCID/嵌套/多核。
4. 代码页仍是硬约束（2 页已用满，页 1 追加块后仅余少量字节）。
