# SPEC-095 设计：运行期服务注册表（闭合 SPEC-093 §5.2）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 前置：[SPEC-090 服务管理器按名解析](2026-10-11-stage4-service-manager-design.md)、
> [SPEC-094 `ALPHA` 端点独立栈段](2026-10-12-stage4-alpha-independent-stack-design.md)

## 1. 本片要回答的问题

SPEC-093 §5.2 登记：服务注册表仍为**编译期内联 `cmp`**（`kernel/src/user.rs` 里
`r8 = (name == SERVICE_NAME_BETA) ? BETA : NONE`），名字是硬编码常量而非端点**自报**。

本片把解析升级为**运行期注册表**：两个端点在启动时把 `(注册名, task id)` **自报**给 init，
init 据此在 Y 页建表，解析请求名时**按名扫描**该表——而非内联常量。

## 2. 机制

### 2.1 端点自报 (name, id)

自报消息 payload 由 SPEC-088 的 `tag ‖ id` 扩为 `tag ‖ name(16..31) ‖ id(0..15)`：

- `prima-smoke`（`ALPHA` 端点）注册名 `1`（`SERVICE_SELF_NAME`）；
- `prima-beta`（`BETA` 端点）注册名 `2`。

名字编译进各自二进制（与 §5.2「名字为 64 位码而非字符串路径」相符；改为字符串路径是更远的一步，
见 §5）。

### 2.2 init 建表

init 的两次自报收集循环（`ALPHA` 的 `svc_id_loop`、`BETA` 的 `svc2_id_loop`）在取出 id 的同时，
从 payload 的 16..31 位取出注册名，存入 Y 页注册表槽
`SVC_RECEIPT_REG_A_NAME=768` / `SVC_RECEIPT_REG_B_NAME=776`（Y 页空隙，无任何见证读取；避开
`runtime_hello` 的 `map_result=Y+736`）。

### 2.3 按名扫描解析

解析块（原内联 `cmp`）改为**扫描注册表**：`r8 = 0`；`rax == 注册表 B 名 → r8 = BETA(2)`；
`rax == 注册表 A 名 → r8 = ALPHA(1)`；未命中 → `NONE(0)`。正例（客户端请求 `BETA`）与未注册名
负例都走同一扫描。

## 3. 判决性见证（`service_selected_ready`，seq 28，扩展）

追加 `reg_a_name` / `reg_b_name`：

```
...;client_ep=0x8;client_stack_seg=0x16;reg_a_name=0x1;reg_b_name=0x2
```

- `reg_a_name=0x1`、`reg_b_name=0x2`：init 从两个端点**自报消息**读出的注册名——证明解析用的
  注册表**由运行期自报建成**，而非编译期常量（反例：任一不符即拒绝）。
- `resolved=0x2`（客户端请求名 `0x2` 经扫描命中 B 项 → `BETA`）、`unknown=0x0`（未注册名扫描
  未命中 → `NONE`）沿用既有语义。

## 4. 回归面

见报告 §4（基线 = SPEC-094 HEAD；逐条登记）。

## 5. 未决与登记

1. **名字仍是 64 位码**（`1`/`2`），非字符串路径；注册仍是**编译进二进制**而非运行期动态注册
   （端点不能改名）。更远的一步（字符串服务名 / 运行期动态注册表）留待后续。
2. 注册表与解析仍是 `init` 内的 Y 页表（2 项）+ 线性扫描；未引入内核侧注册对象。
3. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
