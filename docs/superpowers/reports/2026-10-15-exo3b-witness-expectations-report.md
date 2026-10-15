# SPEC-119 / EXO-3B：witness 期望值移出内核

**日期**：2026-10-15
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第二小片）
**基线**：SPEC-118 / EXO-3A

## 目标

继续把“服务语义期望值”从内核移出：内核 witness 只收集/输出原始 qword，具体期望由
`xtask/src/protocol.rs` 校验。本片移出的是 marker、reply 状态、协商结果和部分行为码。

## 改动

`kernel/src/user.rs` 删除：

- `SERVICE_REPLY_WORD`；
- `SERVICE_EXPECTED_NEGOTIATED` / `SERVICE_EXPECTED_NEGOTIATED_B`；
- `SERVICE_MARKER_DRIVER/LOG/NS/STORE`；
- `SERVICE_REPLY_LOG_*` / `SERVICE_REPLY_NS_*` / `SERVICE_REPLY_STORE_*`；
- `SERVICE_ALPHA_BINARY_MARKER` / `SERVICE_BETA_BINARY_MARKER`；
- `SERVICE_BEHAVIOR_NONE/ALPHA/BETA`。

对应 witness 只保留结构/关系断言（例如 field 非零、两个 endpoint 不同、两个 marker 不同、
两个 stack segment 不同、状态数组长度/形状），不再比较服务专属期望值。

`xtask/src/protocol.rs` 仍按原值严格校验这些原始字段；事件流中的 `evidence_ok=1`
继续表示内核侧结构断言通过，而服务语义期望由外部 harness 负责。

## 证据

- `cargo xtask test`：**6/6 PASS**，BIOS 两次退出码 33、43 条事件。
- `cargo test --workspace`：**795 passed, 0 failed**（kernel 377）。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=21699 total=22961 ratio=5.50%`，机制层 `unsafe=0`；
  耦合面 `include_bytes=0 prima_user_dep=false service_name_lines=2 protocol_lines=213→165`。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 未决

1. 协议 tag（`SERVICE_*_TAG`）、共享段 witness 偏移、init 机器码仍在内核；
2. 完整 EXO-3 应改为通用 trace/调试通道，而不是固定偏移读回；
3. 完整 EXO-2 用户态 loader 未完成。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
