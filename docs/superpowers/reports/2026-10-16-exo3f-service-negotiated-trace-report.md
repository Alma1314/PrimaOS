# SPEC-125 / EXO-3F：`service_negotiated_ready` 迁移到通用 trace

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第六小片）
**基线**：SPEC-124 / EXO-3E

## 目标

把 `service_negotiated_ready` 中来自**服务共享段固定偏移**的字段迁移到通用 trace：

- `token` / `service` / `service_id` / `done` / `negotiated` / `negatives`；
- 内核不再读取：
  - `USER_HELLO2_VA + SERVICEMGR_TOKEN_OFFSET`
  - `USER_HELLO2_VA + SERVICEMGR_MANAGER_OFFSET`
  - `USER_HELLO2_VA + SERVICE_ENDPOINT_ALPHA_OFFSET`
  - `USER_HELLO2_VA + SERVICE_DONE_ALPHA_OFFSET`
  - `USER_HELLO2_VA + SERVICE_NEGOTIATED_OFFSET`
  - `USER_HELLO2_VA + SERVICE_NEGATIVES_OFFSET`

`forward` 仍来自 task 1 的 Y 页机制回执；`stack` 保持 0。

## 设计

- `prima-abi::trace` 增加 `EVENT_SERVICE_NEGOTIATED = 3`。
- `prima-servicemgr` 收集 ALPHA 端点创建/自报/完成协商字段，并在收尾时发两条记录：
  ```text
  record 0: [token, manager, service_id, done, negotiated]
  record 1: [negatives, 0, 0, 0, 0]
  ```
  其中 manager cap/token 由 `create_service` 返回，negotiated/negatives 在 ALPHA done 到达时从
  `USER_SEG_VA` 的同一共享段读回。
- `kernel/src/user.rs::service_negotiated_witness`：
  - 通过 `trace_find(EVENT_SERVICE_NEGOTIATED, 0/1)` 组合字段；
  - 事件 data 形状保持不变，xtask 校验无需修改。

## 证据

- `cargo xtask test`：**6/6 PASS**；BIOS 两次退出码 **33**、**43 条事件**。
- 原始串口 seq27：
  ```text
  PRIMA|v=1|seq=27|event=service_negotiated_ready|status=ok|data=token=0x200000033;service=0x100000038;stack=0x0;svc_id=0xa;forward=0x0;done=0x5ec2000000000000;negotiated=0x1000008000001;negatives=0x100000000
  ```
  与迁移前字段逐字节一致。
- 原始串口：`frames_ready.image=1121`、`self_root_ready.pages=361`。
- `cargo test --workspace`：**796 passed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=20112 total=21374 ratio=5.90%`，
  机制层 `unsafe=0`；耦合面 `include_bytes=0 prima_user_dep=false
  service_name_lines=2 protocol_lines=132→127`。
- `cargo xtask compat`：PASS。

## 未决

1. `service_selected_ready` 与 `servicemgr_ready` 仍按共享段固定偏移读回。
2. `service_negotiated` 的 `forward` 仍读 task 1 Y 页机制回执；完整 trace 目标下可继续下沉。
3. trace 仍是内核静态缓冲 + syscall，尚未实现共享 trace ring / 串口通用调试通道。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
