# SPEC-087 报告：init ↔ 存活 ring3 客户端双向握手

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[init ↔ 存活 ring3 客户端双向握手设计](../specs/2026-10-10-stage4-init-client-handshake-design.md)
>
> 前置：[SPEC-085 报告](2026-10-10-stage4-ring3-runtime-hello-report.md)（单向握手）

## 1. 本片要回答的问题

SPEC-085 的握手是**单向**的（init 未解析、未回复）。本片让 init 与一个**存活的** ring3
Rust 客户端完成一次真实双向握手：客户端发 Hello → init 映射共享段并应答 → 客户端收下
应答并把 payload 写回共享帧 → init 读回 `SERVICE_REPLY_WORD` 逐位比对。

## 2. 判决性见证（`service_reply_ready`，本片新增，落在 seq 26）

```
client=0x100000018;stack=0x0;recv=0x8;map=0x0;reply=0x0;echo=0x38001474e4f50
```

判决性在于 `echo`：该 word 只可能是**客户端**在收到 init 的应答之后写入它自己映射的共享
Segment、再由 init 从同一物理帧读回的——它同时证明客户端真的等到并被应答唤醒、两端共享同一
wire 编码、以及 init 的应答 `SYS_SEND` 成功。

## 3. 定位并修复的两个真实缺陷

### 3.1 `syscall` 快路径上的阻塞/让出上下文（`prima-ostd/src/entry.rs`，TCB）

`syscall` 不压硬件帧。快路径入口原先既不写 `CONTEXT_BASE`、也不模拟中断路径的 5-qword
硬件帧，导致 `SaveBlock::to_sret_frame` 在快路径上读出垃圾 `rip/rsp/cs/ss`。任何经快路径
发起的阻塞/让出（客户端的 `receive()`）于是把**外来（init 的）上下文**存进本任务调度帧，
恢复时 `iretq` 落到未映射地址触发 `#PF` 死循环。

QEMU 定位证据（临时串口 trace）：切换后 `t=8` 反复 `#PF`，`cr2==rip==0xa0000009093`
（= init recv 循环入口），`cr3=0x76000`（客户端根）——即客户端跑进 init 的代码页且该页在
客户端根下未映射。

修复：入口在 15 个 GPR 之下压入 `ss/rsp/rflags/cs/rip` 模拟硬件帧、并写 `CONTEXT_BASE`；
新增 `set_fast_syscall_ss`，`configure_fast_path` 设用户 CS/SS。

### 3.2 握手让出把 task 2 排空（`kernel/src/user.rs` / `prima-user/src/main.rs`）

- S2-D4 块末尾原有一段直投唤醒，正是因为让出会把 task 2 排空成 `BlockedRecv`；本片握手块
  再让出两次，必须**同样在末尾再直投唤醒一次**，否则收尾抢占阶段无 Ready 任务可轮转
  （`preempt_tick` 早退、`preempt_ready` 缺失、实机挂死）。
- 存活客户端在写完回执后进入 `loop { receive() }` **阻塞**（而非 yield 循环），以免持续
  Ready 打乱抢占阶段的 1↔2 轮转。
- `task_create` 见证记录收紧：只记前两次 create 的 `child`/`grandchild`；第 3 次 create
  （本客户端）不再覆盖 `grandchild`，保持 `task4_recv`/`root4_cleared` 语义不变。

## 4. 回归面（逐条登记）

| 事件 | 变化 |
|---|---|
| 新增 `service_reply_ready` | seq 26；其后事件顺移 1 位，`boot_complete` 39→40 |
| `task_ready` | `tasks`/`tasks_after` 3→4；`caps`/`caps_after` 44→49 |
| `ipc_ready` | `queued/direct/blocks` 3/3/3→4/5/6 |
| `platform_segments_ready` | `vspace` 12→13；`frames[4]`（task 1 代码段）1→2 |
| `memory_ready`/`bootstrap_ready`/`frames_ready`/`self_root_ready`/`preempt_ready` | 布局类字段（镜像/物理帧序移动） |

**未重建 pre-merge 事件流 diff**（与本地 WIP 分支的镜像变化纠缠，方法边界如实登记）。

## 5. 验证（2026-10-10，WIP 分支 `wip/spec-087-init-service-manager`）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 365、prima-ostd 21、xtask **193** |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=19934 total_lines=21196 ratio=5.95%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**40 条事件**，含 seq 26 `service_reply_ready`） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |
| 确定性 | 同镜像连跑两次 40 条事件逐字节相同、退出码 33 |

反例：xtask 单测 `rejects_service_reply_when_echo_is_missing` 把 `echo` 改成 0 后报
`the client must write init's reply payload back`。

## 6. 未决与登记

1. **仅一次双向应用层握手**：不构成服务互通、调度或安全证明；**S4-D2 与阶段 4 交付项/退出
   条件一律不勾选**。
2. 客户端仍是单实例探针；`_start` 第三参数未使用。
3. 快路径保存块与中断帧的一致性只经 `to_sret_frame` 复用；未做 PCID/嵌套/多核。
4. 代码页 2 页已接近用满。
