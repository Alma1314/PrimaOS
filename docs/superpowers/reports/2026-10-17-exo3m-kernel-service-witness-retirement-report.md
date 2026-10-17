# SPEC-138 / EXO-3M：内核服务 witness 组装退役

**日期**：2026-10-17
**阶段**：阶段 4 后续硬化（不改变阶段退出条件判定）
**基线**：SPEC-136 / EXO-3L

## 改动

内核不再为 Stage 4 的服务测试组装任何 `PRIMA` 事件，也不再读取 Y 页的任何服务回执偏移：

- 删除 `ServiceReplyWitness`、`ServiceNegotiatedWitness`、`ServiceSelectedWitness`、
  `ServicemgrWitness` 及其采集函数和 `protocol::*_ready` 发射器。内核不再持有
  `CLIENT_RECEIPT_*`、`SVC_RECEIPT_*`、`SERVICEMGR_RECEIPT_*` 等 Y 页偏移常量。
- `seq 25–29`（`runtime_hello_ready`、`service_reply_ready`、`service_negotiated_ready`、
  `service_selected_ready`、`servicemgr_ready`）退役。`protocol::skip_event()` 只消耗序号，
  因此 `seq 30–43` 的既有事件**逐字节不变**，事件总数 43→38。
- 新增三类通用原始证据，内核均不解释字段语义：
  - `TRACEBYTES|slot=0/1;...`：两页共享 trace 的原始记录字节（沿用 SPEC-135/136）。
  - `YBYTES|data=...`：init 证据页（Y）4096 字节原样十六进制。
  - `LIVE|...`：对两页 trace 的全部原始参数和 Y 页全部 qword 做通用句柄存活扫描；
    只报告“该原始值当前指向活 Segment/Device”，不判断它属于哪个服务字段。
- servicemgr 在第三条 `EVENT_SERVICE_SELECTED` 记录的空闲参数位上报 alpha/beta 端点栈段
  句柄；客户端栈段句柄由 init 写入 `Y[368]`。因此栈段证据仍可判定，但偏移解释移入 `xtask`。

`xtask` 直接解码上述原始字节完成原来的全部语义断言：runtime-hello 字段、reply 的
client/stack/map/reply 回执、协商 tag/负例、按名解析与多端点路由、服务管理器六个分片、
基础服务状态机、`ns_resolved`、日志/存储段互异与驱动 `IOPortRange` 往返。原
`validate_*_data` 中“内核侧活对象”的硬断言由 `LIVE` 扫描保留：`log_seg`/`store_seg`
必须作为活 Segment 出现，`driver_dev` 必须作为活 Device 出现，三个栈段句柄必须非零、
互异且均为活 Segment。

## 验证

| 命令 | 结果 |
|---|---|
| `cargo test --workspace --offline --quiet` | 786 passed；服务事件旧负例删除/迁移后仍覆盖原始字节负例 |
| `cargo xtask lint` | fmt、Clippy `-D warnings`、裸机内核构建、`kernel/src` 零 `unsafe` 四项 PASS |
| `cargo xtask test` | BIOS QEMU run 1/2 与 stage1 boot slice 共 6 项 PASS |
| `cargo xtask compat` | `PASS compat cow-write-fault` |
| `cargo xtask tcb` | trusted 1262、mechanism 19769（SPEC-136 为 20127）、ratio 6.00%；机制层 unsafe 0；`protocol_lines=48→38`、`service_name_lines=0` |
| `git diff --check` | PASS |

QEMU 实际输出显示：`seq 24` 后直接是 `seq 30`，`seq 30–43` 与原基线一致；新增
`TRACEBYTES`、`LIVE`、`YBYTES` 行均由 `xtask` 解码并通过全部断言。

## 边界

这是**有界定长记录流 + 定长页转储**，仍不是循环 ring 或任意长度消息，也尚未支持多消费者。
内核仍知道“Y 页是用户证据页”这一通用布局，但不再知道其中任何服务字段偏移。阶段 4 的正式
晋级仍受 S0-X1 组织级第三方复核约束；本切片只关闭 EXO-3 的“内核组装兼容 PRIMA 服务事件”
残留。
