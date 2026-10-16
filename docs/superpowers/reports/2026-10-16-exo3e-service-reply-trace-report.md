# SPEC-124 / EXO-3E：`service_reply_ready` 迁移到通用 trace

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第五小片）
**基线**：SPEC-123 / EXO-3D

## 目标

把 `service_reply_ready` 中来自**服务共享段固定偏移**的字段迁移到 EXO-3D 通用 trace：

- `recv`：原读 `USER_HELLO2_VA + SERVICE_CLIENT_ID_OFFSET`；
- `echo`：原读 `USER_HELLO2_VA + SERVICE_REPLY_OFFSET`。

`client` / `stack` / `map` / `reply` 属于 init 在 Y 页的通用机制回执，本片暂保留内核读取；
内核不再为 service-reply 语义读取服务共享段固定偏移。

## 设计

- `prima-abi::trace` 增加 `EVENT_SERVICE_REPLY = 2`。
- `prima-servicemgr` 在等客户端把 `echo` 写回共享段后、三段控制消息之前调用：
  ```text
  trace_emit(EVENT_SERVICE_REPLY, [client_id, echo, 0, 0, 0])
  ```
- 内核 `kernel/src/syscall.rs` 增加 `trace_find(id, occurrence)`，按 opaque id 查找记录，不解释语义。
- `kernel/src/user.rs` 的 `service_reply_witness`：
  - `recv` / `echo` 从 `trace_find(EVENT_SERVICE_REPLY, 0)` 读取；
  - 不再读 `USER_HELLO2_VA` 的两个服务偏移。
- `xtask/src/protocol.rs` 无需改动：seq26 事件名与 data 形状保持稳定，字段值不变。

## 证据

- `cargo xtask test`：**6/6 PASS**；BIOS 两次退出码 **33**、**43 条事件**。
- 原始串口 seq26：
  ```text
  PRIMA|v=1|seq=26|event=service_reply_ready|status=ok|data=client=0x10000001c;stack=0x0;recv=0x9;map=0x0;reply=0x0;echo=0x38001474e4f50
  ```
  与迁移前字段逐字节一致。
- 原始串口：`frames_ready.image=1119`、`self_root_ready.pages=360`。
- `cargo test --workspace`：**796 passed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=20107 total=21369 ratio=5.91%`，
  机制层 `unsafe=0`；耦合面 `include_bytes=0 prima_user_dep=false
  service_name_lines=2 protocol_lines=133→132`。
- `cargo xtask compat`：PASS。

## 未决

1. `service_negotiated_ready`、`service_selected_ready`、`servicemgr_ready` 仍按共享段固定
   偏移读回；下一步逐事件迁移。
2. `service_reply` 的 `client/stack/map/reply` 仍从 task 1 的 Y 页通用回执读取；它们不是
   服务共享段语义，但完整“通用 trace”目标下也可后续下沉。
3. trace 仍是内核静态缓冲 + syscall，尚未实现共享 trace ring / 串口通用调试通道。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
