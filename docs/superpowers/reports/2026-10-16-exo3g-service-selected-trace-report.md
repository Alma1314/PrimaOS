# SPEC-126 / EXO-3G：`service_selected_ready` 迁移到通用 trace

**日期**：2026-10-16
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 第七小片）
**基线**：SPEC-125 / EXO-3F

## 目标

把 `service_selected_ready` 中来自**服务共享段固定偏移**的字段迁移到通用 trace：

- `name` / `resolved` / `unknown` / `done` / `negotiated_beta`；
- `alpha_ep` / `beta_ep` / `beta_self` / `alpha_marker` / `beta_marker`；
- `client_ep` / `reg_a_name` / `reg_b_name`。

内核不再为这些字段读取 `USER_HELLO2_VA +` 固定偏移。以下字段仍由内核按任务映射查询：

- `alpha_stack_seg` / `beta_stack_seg` / `client_stack_seg`（`mapped_segment_at`）。

## 设计

- `prima-abi::trace` 增加 `EVENT_SERVICE_SELECTED = 4`。
- `prima-servicemgr` 在 Hello/自报/done/选择阶段收集字段，并在收尾发**三条** trace：
  ```text
  record 0: [name, resolved, unknown, done, negotiated_beta]
  record 1: [alpha_ep, beta_ep, beta_self, alpha_marker, beta_marker]
  record 2: [client_ep, reg_a_name, reg_b_name, 0, 0]
  ```
- `kernel/src/user.rs::service_selected_witness`：
  - 通过 `trace_find(EVENT_SERVICE_SELECTED, 0/1/2)` 组合原始字段；
  - 仅用 `alpha_ep`/`beta_ep`/`client_ep` 继续查询任务栈 Segment id；
  - 事件 seq28 的字段 shape 与 xtask 校验保持不变。

## 证据

- `cargo xtask test`：**6/6 PASS**；BIOS 两次退出码 **33**、**43 条事件**。
- 原始串口 seq28：
  ```text
  PRIMA|v=1|seq=28|event=service_selected_ready|status=ok|data=name=0x61746562;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;alpha_ep=0xa;beta_ep=0xb;beta_self=0xb;alpha_marker=0x414c504841000001;beta_marker=0x4245544100000002;alpha_stack_seg=0x17;beta_stack_seg=0x18;client_ep=0x9;client_stack_seg=0x14;reg_a_name=0x6168706c61;reg_b_name=0x61746562
  ```
  其中 `*_stack_seg` 是运行期分配相关值，xtask 继续校验其结构/相异关系；协议/身份/名字字段不变。
- 原始串口：`frames_ready.image=1126`、`self_root_ready.pages=362`。
- `cargo test --workspace`：**796 passed**。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=20118 total=21380 ratio=5.90%`，
  机制层 `unsafe=0`；耦合面 `include_bytes=0 prima_user_dep=false
  service_name_lines=2→1 protocol_lines=127→115`。
- `cargo xtask compat`：PASS。

## 未决

1. `servicemgr_ready` 仍按共享段固定偏移读回，是 EXO-3 最后一个服务 witness。
2. `alpha_stack_seg` / `beta_stack_seg` / `client_stack_seg` 仍由内核查询任务映射；它们是通用
   机制不变量，不是服务协议字段。
3. trace 仍是内核静态缓冲 + syscall，尚未实现共享 trace ring / 串口通用调试通道。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
