# SPEC-118 / EXO-3A：内核不再解释服务名

**日期**：2026-10-15
**阶段**：阶段 4（Exokernel 对齐系列，EXO-3 的第一小片）
**基线**：SPEC-117

## 目标

把服务名的“期望值判断”从内核 witness 移到外部 `xtask` 校验器：

- 内核只从共享段读回**原始 qword**，不定义 `"alpha"`/`"beta"` 名字常量，也不调用
  `service_name()` 打包；
- `xtask/src/protocol.rs` 继续用 `SERVICE_SELECTED_*` 期望值核验 `name`/`reg_a_name`/
  `reg_b_name`；
- 内核 witness 仍验证结构/绑定字段（`resolved`、`unknown`、端点 id、marker、stack seg 等），
  但不再比较服务名字面量。

这是 EXO-3 的局部实现：内核不再“认识”服务名，服务名语义由用户态/Runtime 与外部 harness
共同持有。

## 改动

| 文件 | 内容 |
|---|---|
| `kernel/src/user.rs` | 删除 `SERVICE_NAME_MAX/MASK`、`service_name()`、`SERVICE_NAME_ALPHA/BETA`；`ServiceSelectedWitness::evidence_ok` 删除名字比较；删除对应 host 单测 |
| `xtask/src/protocol.rs` | 不改变期望值，继续校验原始 `name`/`reg_a_name`/`reg_b_name` |
| `xtask/src/lint.rs` | `kernel_service_coupling` 的 `service_name_lines` 作为递减指标保留 |

## 证据

- `cargo xtask test`：**6/6 PASS**，BIOS 两次退出码 33、43 条事件。
  - 内核侧不再检查名字，但 `xtask` 仍拒绝错误的 `name`/`reg_a_name`/`reg_b_name`；
  - 若内核不再输出这些原始字段或 xtask 期望被绕过，事件校验会失败。
- `cargo test --workspace`：**795 passed, 0 failed**（kernel 378→**377**，删除一条 service name host 测试）。
- `cargo xtask lint`：**4/4 PASS**。
- `cargo xtask tcb`：`trusted=1262 mechanism=21750 total=23012 ratio=5.48%`，机制层 `unsafe=0`；
  耦合面 `include_bytes=0 prima_user_dep=false service_name_lines=2 protocol_lines=213`。
  `service_name_lines` 由 17 降到 2（剩余为 `HELLO_SERVICE_NAME_OFFSET` 常量名与注释）。
- `cargo xtask compat`：`PASS compat cow-write-fault`。

## 未决

1. 内核仍定义协议 tag、服务 marker、witness 偏移等 213 条 `SERVICE_*`/`SERVICEMGR_*` 命中；
   服务协议解释仍未下沉。
2. 内核仍读取固定共享段偏移形成 witness；完整 EXO-3 应改为通用 trace/调试通道。
3. `user_code()` init 机器码与完整用户态 loader（EXO-2）仍未完成。

## 不勾选

本片不勾选阶段 4 任何交付项/退出条件。
