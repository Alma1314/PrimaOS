# SPEC-122 / EXO-3C：协议 tag 不再进入内核 witness

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第三小片）
**基线**：SPEC-121 / EXO-2D

## 目标

继续把服务协议语义移出内核：删除 `kernel/src/user.rs` 中的协议 tag 常量，以及 witness
`evidence_ok()` 对 tag 的相等断言。协议 tag 的具体期望值继续由 `xtask/src/protocol.rs`
严格校验。

## 改动

`kernel/src/user.rs` 删除：

- `SERVICE_ID_TAG`；
- `SERVICE_DONE_TAG`；
- `SERVICE_PREPARE_TAG`；
- `SERVICE_WAKE_TAG`；
- `SERVICE_SETUP_TAG`；`SERVICE_SETUP_CODE_TAG`；
- `SERVICEMGR_HELLO_TAG`；`SERVICEMGR_WAKE_TAG`；`SERVICEMGR_SEG_TAG`；
  `SERVICEMGR_FINISH_TAG`。

以及 `ServiceNegotiatedWitness` / `ServiceSelectedWitness` / `ServicemgrWitness` 中对应
的 tag 相等断言；内核 witness 只保留结构性/关系断言，原始 qword 继续输出，期望值由
xtask 校验。

## 证据

- `cargo xtask test`：**6/6 PASS**，43 条事件，BIOS 退出码 33。
- `cargo test --workspace`：**796 passed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=20113 total=21375 ratio=5.90%`，
  机制层 `unsafe=0`；耦合面 `service_name_lines=2`、`protocol_lines=153→133`。
- `cargo xtask compat`：PASS。

## 未决

1. **共享段 witness 偏移仍在 `kernel/src/user.rs`**，内核 witness 仍按固定偏移读取用户
   共享段；完整 EXO-3 应改为通用 trace/调试通道。
2. 通用 trace ABI 尚未定义与接入；`runtime_hello`、`service_reply`、`service_negotiated`、
   `service_selected`、`servicemgr` 等事件仍由内核读回固定偏移后输出。
3. 服务协议 tag 现已完全由用户态/xtask 持有，但完整 trace 迁移仍会改变事件流形态，需按
   维护规则逐条登记。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
