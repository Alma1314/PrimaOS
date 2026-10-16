# SPEC-128 / EXO-3I：原始 trace 调试通道

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第九小片）
**基线**：SPEC-127 / EXO-3H

## 目标

在 EXO-3D..3H 的通用 trace 记录基础上，增加一条**内核不解释内容**的原始调试通道：

- 内核只保存 `Record { id, args[5] }`；
- 启动收尾时把全部记录原样输出为非 PRIMA 的 `TRACE|...` 行；
- 现有 PRIMA 事件序列/校验保持完全不变；
- 调试工具或后续 xtask 可直接解析 `TRACE|` 行。

本片仍使用内核静态 trace 缓冲 + `SYS_TRACE_EMIT`；**共享 trace ring / 用户态字节流**仍是后续工作。

## 设计

- `kernel/src/syscall.rs` 已有 `trace_record(index)`。
- `kernel/src/protocol.rs` 新增 `trace_dump()`：
  ```text
  TRACE|id=0x..;a=0x..;b=0x..;c=0x..;d=0x..;e=0x..
  ```
  只做原始字段十六进制输出，不按事件名解释。
- `kernel/src/resume.rs::run_and_exit()` 在 `boot_complete` 后、`serial::flush` 前调用
  `protocol::trace_dump()`。
- `xtask` 只解析 `PRIMA|` 行，因此 `TRACE|` 行不进入 43 条事件序列。

## 证据

- `cargo xtask test`：**6/6 PASS**；BIOS 两次退出码 **33**、**43 条 PRIMA 事件**。
- 原始 BIOS 串口包含 14 条 `TRACE|` 记录，例如：
  ```text
  TRACE|id=0x1;a=0x0;b=0x14d495250;c=0x300000001;d=0x100000018;e=0x100000010001
  TRACE|id=0x2;a=0x9;b=0x38001474e4f50;c=0x0;d=0x0;e=0x0
  ```
- 原始串口：`frames_ready.image=1128`、`self_root_ready.pages=363`。
- `cargo test --workspace`：**796 passed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=20143 total=21405 ratio=5.90%`，
  机制层 `unsafe=0`；耦合面 `service_name_lines=1 protocol_lines=92`。
- `cargo xtask compat`：PASS。

## 未决

1. 当前仍是**静态缓冲 + syscall**，不是设计中的共享 trace ring / 用户态可写字节流。
2. `TRACE|` dump 只在 boot 收尾输出一次；后续可扩展为流式串口调试通道。
3. 完整 EXO-3 的“内核只提供共享段/串口资源、用户态写任意字节流、xtask 直接解析”尚未落地。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
