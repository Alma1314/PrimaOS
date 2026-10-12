# SPEC-095 报告：运行期服务注册表（闭合 SPEC-093 §5.2）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[运行期服务注册表设计](../specs/2026-10-12-stage4-runtime-service-registry-design.md)
>
> 前置：[SPEC-094 `ALPHA` 端点独立栈段](2026-10-12-stage4-alpha-independent-stack-report.md)

## 1. 本片要回答的问题

闭合 SPEC-093 §5.2 登记的「注册表仍为**编译期内联 `cmp`**」：两个端点在启动时把
`(注册名, task id)` **自报**给 init，init 建 Y 页注册表，解析请求名时**按名扫描**——取代
`r8 = (name == SERVICE_NAME_BETA) ? BETA : NONE` 的硬编码。

## 2. 判决性见证（`service_selected_ready`，seq 28 扩展）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;
alpha_ep=0x9;beta_ep=0xa;beta_self=0xa;
alpha_marker=0x414c504841000001;beta_marker=0x4245544100000002;
alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x8;client_stack_seg=0x16;
reg_a_name=0x1;reg_b_name=0x2
```

- `reg_a_name=0x1`、`reg_b_name=0x2`：init 从两端点**自报消息**（payload 16..31 位）读出的注册名
  ——证明解析用的注册表**由运行期自报建成**。`resolved=0x2` 即客户端请求名 `0x2` 经**扫描**命中
  B 项（其自报名正是 `0x2`）；`unknown=0x0` 即未注册名扫描未命中。
- 若注册表不是自报所建（任一项与自报名不符），见反例。

## 3. 实现要点

- **端点自报**：`prima-smoke`/`prima-beta` 各自把 `SERVICE_SELF_NAME`（`1`/`2`）编进自报 payload
  `tag ‖ name(16..31) ‖ id(0..15)`。
- **init 建表**：两次自报收集循环在取 id 的同时取注册名，存入 `SVC_RECEIPT_REG_A_NAME=768` /
  `SVC_RECEIPT_REG_B_NAME=776`（Y 页空隙；**避开** `runtime_hello` 的 `map_result=Y+736`——
  首版误用 736 曾致 `runtime_hello_ready` 的 `map` 被覆盖为 `0x2`，已修正）。
- **按名扫描**：解析块由内联 `cmp` 改为「与注册表 A/B 项比较 → 命中赋行为码、未命中 `NONE`」，
  正例与未注册名负例共用同一扫描。

## 4. 回归面（手工事件流 diff，逐条登记；基线 = SPEC-094 HEAD）

42 条事件、seq **无增删**；rel diff：

| 事件 | 字段 | 前 → 后 |
|---|---|---|
| `memory_ready`（3） | `usable`/`free`/`image`/`kend` | 29944/29944/1516/0x15ec000 → **29942/29942/1517/0x15ed000** |
| `bootstrap_ready`（9） | `image_len`/`image_frames`/`free_frames` | 6207960/1516/29944 → **6210272/1517/29942** |
| `preempt_ready`（30） | `rsp*`/`kstack*_top` | 前移（`root*`/`cr3*`/`data*`/`mark*`/`ticks` 不变） |
| `frames_ready`（36） | `universe`/`free`/`image` | 31460/29790/1516 → **31459/29788/1517** |
| `service_selected_ready`（28） | 新增 `reg_a_name`/`reg_b_name` | **0x1 / 0x2** |

**判定的不变**：`ipc_ready`（23）仍 8/9/12；seq 25/26/27（`runtime_hello` 的 `map=0x0` 与 wire 数据、
`service_reply`、`service_negotiated`）逐字节不变；`namespace_ready`（29）仍 `evidence_ok=1`；
`platform_segments_ready`（34）逐字段不变；`self_root_ready`（37）`pages=686`、`state_audit_ready`
（39）`used=251` 不变。

事件镜像增长（`image_frames` 1516→1517、`kend` `0x15ec000`→`0x15ed000`）源于 init ELF 页 1 代码
（+≈180 B）与两个端点 ELF（新增 `SERVICE_SELF_NAME` 常量）随内核嵌入。

同镜像连跑两次 42 条事件逐字节相同。

## 5. 验证（2026-10-12）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 366、prima-ostd 21、xtask **204**（+1 反例） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20874 total_lines=22136 ratio=5.70%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**42 条事件**，含 seq 28 扩展见证） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例（xtask 单位测试，对合成输入）：
`rejects_service_selected_when_the_registry_is_not_built_from_self_reports`（注册表项与端点自报
不符即失败；即退化回编译期常量），连同 SPEC-090/092/093/094 的八条。

## 6. 未决与登记

1. **名字仍是 64 位码**（`1`/`2`），非字符串路径；注册仍是**编译进二进制**（端点不能运行期改名）。
   字符串服务名 / 运行期动态注册留待后续。
2. 注册表与解析仍是 `init` 内 Y 页 2 项表 + 线性扫描；未引入内核侧注册对象。
3. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
