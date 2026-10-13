# SPEC-104（前置片）报告：对端 task id 的入口 ABI

> 状态：探索实现；不构成 S4-D2/D3 验收或稳定 ABI
>
> 设计依据：[把服务管理器迁到 ring3 Rust](../specs/2026-10-13-stage4-ring3-service-manager-design.md) §2/§3/§6
>
> 前置：[SPEC-103 ring3 服务管理器任务](2026-10-14-stage4-ring3-servicemgr-task-report.md)
>
> 基线：SPEC-103 HEAD `1d88f30`

## 1. 本片要回答的问题

SPEC-104 要把**运行期注册表 / 按名解析 / 请求路由转发**从 `init` 的手写机器码迁入
`prima-servicemgr`。迁移后端点与客户端要**直接以 servicemgr 为对端**发送，但 `send` 有一处硬约束：

> `kernel/src/syscall.rs:1430-1431`：`authorize(sender, send_cap, TaskSend)` 之外还要求
> `task_of_cap(send_cap) == receiver`——**接收者必须等于发送凭据所指向的 task id**。

任务能查自身 id（`SYS_TASK_ID`），却**无法**得知自己持有的发送凭据指向哪个任务。迁移后 `init`
把「指向 servicemgr 的发送凭据」作为端点的 `initial_caps[0]` 移交，端点必须据此**寻址**对端。
本片提供这一寻址原语。

## 2. 判决性设计：入口 `r8` = 发送凭据所指向的 task id

`init` 建任务时凭据指向谁，新任务就应能寻址谁。故在 `register_created_task` 里把
`initial_caps[0]` 的目标 task id 写入新任务初始帧的 **`r8`**（第 5 个入口参数）：

```
frame.r8 = decode_handle(initial_caps[0])?.task_of_cap()  // 非 Task cap 时为 0
```

- `r8` 在任务恢复路径上被真实加载（`prima-ostd/src/entry.rs:978` `mov r8, [r10+0x40]`），
  故与既有 `rdi/rsi/rdx/rcx` 一样是可信的运行期入口参数。
- 三个 ring3 二进制把原先硬编码的 `TaskId::from_raw(1)` 改为入口 `peer`（第 5 参数）。

**本片不改变任何行为**：迁移前所有 `initial_caps[0]` 皆指向 task 1，故 `r8 == 1`，
二进制寻址的对端与硬编码的 task 1 一致——这是有意的**零事件变化**前置，用它把 ABI 先钉死、
再在 SPEC-104 主体里把 `initial_caps[0]` 改指 servicemgr。若 `r8` 注入有误（0 或错值），
发送会被内核拒绝、QEMU 事件立即崩坏，故本片由既有逐字段事件校验**实证**了 ABI。

## 3. 实现要点

- `kernel/src/user.rs::register_created_task`：新增 `frame.r8`（decode + `task_of_cap`）。
- `prima-user/src/{main.rs,beta.rs,servicemgr.rs}`：`_start` 增第 5 参数 `peer_raw`；
  `peer = TaskId::from_raw(peer_raw as u32)`；三处（自报 id / done / 客户端 Hello）由
  `TaskId::from_raw(1)` 改为 `peer`。

## 4. 验证（2026-10-14）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 146、cosir_state 42、lean_backtrace 3、kernel 369、prima-ostd 21、prima-runtime 1、xtask 206 |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=21292 total_lines=22554 ratio=5.60%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**43 条事件逐字段不变**——`r8` 注入行为保持） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

**判定的不变**：全部 43 条事件逐字节不变（xtask 逐字段期望值无一改动即通过）。

## 5. 未决与登记

1. 本片**只提供寻址原语**；注册表/按名解析/路由/转发的迁移（SPEC-104 主体）与 servicemgr
   创建顺序重排（其必须先于端点/客户端创建，方能移交指向它的发送凭据）留后续片。
2. 迁移主体将**重排 task 创建顺序**（servicemgr 11→更早），端点/客户端 id 随之变化，
   须逐条登记事件值变化。
3. **不勾选** S4-D2/D3 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
