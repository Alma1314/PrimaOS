# SPEC-127 / EXO-3H：`servicemgr_ready` 迁移到通用 trace

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第八小片）
**基线**：SPEC-126 / EXO-3G

## 目标

把最后一个仍按**服务共享段固定偏移**读回的内核 witness——`servicemgr_ready`——迁移到
通用 trace。迁移字段包括：

- `self_id` / `marker` / `created` / `entry_a` / `entry_b`；
- `services` / `log_id` / `ns_id` / `store_id` / `log_mark` / `ns_mark` / `store_mark`；
- 三组定序状态各 3 个 qword；
- `ns_resolved` / `log_seg` / `store_seg`；
- `driver_id` / `driver_mark` / `driver_dev` / `driver_io[2]`。

`task_id` 与 `hello` 仍来自 Y 页通用机制回执，保持内核读取。

## 设计

- `prima-abi::trace` 增加 `EVENT_SERVICEMGR = 5`。
- `prima-servicemgr` 在 finish 分支发六条 trace 记录：
  ```text
  r0: self_id, marker, created, entry_a, entry_b
  r1: services, log_id, ns_id, store_id, log_mark
  r2: ns_mark, store_mark, log_state[0..2]
  r3: ns_state[0..2], store_state[0..1]
  r4: store_state[2], ns_resolved, log_seg, store_seg, driver_id
  r5: driver_mark, driver_dev, driver_io[0], driver_io[1], 0
  ```
- 为避免 1 页 servicemgr 栈被六条记录的 `[u64;5]` 数组压爆，`trace_emit` 从
  `trace_emit(id, [u64;5])` 改为**标量参数** `trace_emit(id, a, b, c, d, e)`；内核 ABI 不变。
- `kernel/src/user.rs::servicemgr_witness`：
  - 通过 `trace_find(EVENT_SERVICEMGR, 0..5)` 组合字段；
  - 不再读 `USER_HELLO2_VA` 的 servicemgr 固定偏移。

## 证据

- `cargo xtask test`：**6/6 PASS**；BIOS 两次退出码 **33**、**43 条事件**。
- 原始串口 seq29（`entry_*`、段句柄等为运行期分配值，xtask 继续校验结构与关系）：
  ```text
  PRIMA|v=1|seq=29|event=servicemgr_ready|status=ok|data=task_id=0x8;hello=0x5e56000000000008;self_id=0x8;marker=0x534552564d475203;created=0x6;entry_a=0x30b0;entry_b=0x2390;services=0x3;log_id=0xc;ns_id=0xd;store_id=0xe;log_mark=0x4c4f470001;ns_mark=0x4e53560002;store_mark=0x5354520003;log_state0=0x5ec9000300000005;log_state1=0x5ec9000300000008;log_state2=0x5ec9000300000008;ns_state0=0x5ec9000400000005;ns_state1=0x5ec9000400000053;ns_state2=0x5ec9000400000053;store_state0=0x5ec9000500000005;store_state1=0x5ec9000500000503;store_state2=0x5ec9000500000503;ns_resolved=0x5ec9000400000068;log_seg=0x100000066;store_seg=0x100000067;driver_id=0xf;driver_mark=0x494f500004;driver_dev=0x10000006a;driver_io0=0x5ec900060000005a;driver_io1=0x5ec9000600000000
  ```
- 原始串口：`frames_ready.image=1128`、`self_root_ready.pages=363`。
- `cargo test --workspace`：**796 passed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=20120 total=21382 ratio=5.90%`，
  机制层 `unsafe=0`；耦合面 `include_bytes=0 prima_user_dep=false
  service_name_lines=1 protocol_lines=115→92`。
- `cargo xtask compat`：PASS。

## 未决

1. `task_id` / `hello` 仍读 Y 页通用机制回执；这是通用 IPC/管理 cap 不变量，不属于服务共享段协议。
2. 当前 trace 仍是内核静态缓冲 + syscall；尚未实现共享 trace ring / 串口通用调试通道。
3. `service_negotiated.forward` 等少量 Y 页回执仍由内核读取；它们不是服务共享段固定偏移。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
