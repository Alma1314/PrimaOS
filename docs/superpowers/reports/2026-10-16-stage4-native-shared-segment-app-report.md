# SPEC-129：原生应用的消息控制与共享 Segment 数据路径

**日期**：2026-10-16
**阶段**：阶段 4（S4-D5、S4-X3 探索证据）
**基线**：`be0e7ef`

> 本报告保留最初的一页 2 KiB 探索结果；当前实现及阶段 4 技术退出条件见 [SPEC-130](2026-10-16-stage4-closure-report.md)。

## 实现

`prima-smoke` 继续作为独立 ring3 Rust 原生应用。它创建并移交一页 `MemorySegment` 后，在偏移 1024 的数据窗口写入 **2048 字节**。消息只携带操作与长度；独立 ring3 `prima-servicemgr` 从自己映射的同一 Segment 读入数据，计算 FNV-1a 校验值，再通过消息回复。客户端核对回复，说明 2 KiB 数据未塞进 64 位消息 payload。

同一会话按序验证四个结果：

1. 首次写入并读回（seed 3，校验值 `0x8641edc5`）；
2. 提交一个待处理请求、取消、再尝试执行；管理器返回 `CANCELLED`，客户端确认共享页另一处哨兵保持原值；
3. 发送越界长度 4096，管理器在读共享页前返回 `BAD_LENGTH`；
4. 重新写入 2 KiB 并成功读回（seed 7，校验值 `0xd7de2dc5`），证明拒绝后会话仍可用。

客户端将四项检查打成 `flags=0xf`，管理器把两次亲自从共享页计算的校验值、成功/排队/取消/跳过/错误计数和该标志作为 `EVENT_NATIVE_APP=6` 原始 trace 上报。内核只透传原始记录；`xtask` 必须找到恰好一条记录并逐字段校验，缺失、重复或篡改均失败。`TRACE|id=0x6` 的期望字段为 `a=0x800;b=0x8641edc5;c=0xd7de2dc5;d=0x201010101;e=0xf`。

## 验证

- `cargo xtask test`：6/6 PASS，BIOS QEMU 两次运行，原有 43 条 PRIMA 事件序列与新 trace 断言均通过。
- `cargo test --workspace --quiet`：797 passed，0 failed；新增 trace 负例测试覆盖缺失、取消计数错误和客户端标志错误。
- `cargo xtask lint`：4/4 PASS。
- `cargo xtask tcb`：`trusted=1262`、`mechanism=20143`、机制层 `unsafe=0`，内核代码未改。
- `ipc_ready` 因六条应用请求、六条回复及完成消息，由 `queued/direct/blocks=44/26/34` 变为 **45/38/46**；`xtask` 金标已同步。其余事件继续通过已有字段和关系断言；未声称全事件逐字节不变。

## 边界

这是**一页内的 2 KiB 示例**，取消发生在请求进入执行前。尚未覆盖多页/持续大流量、执行中的并发取消、超时、驱动或故障服务隔离；因此不据此勾选 S4-X3 或阶段 4 退出条件。S4-D5 已有可复查实现证据，正式验收仍按阶段跟踪规则进行。
