# SPEC-094 设计：`ALPHA` 端点独立栈段（闭合 SPEC-093 §5.1）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 前置：[SPEC-093 独立二进制映像 + 独立物理栈段](2026-10-11-stage4-independent-service-images-design.md)、
> [SPEC-092 独立第二服务端点](2026-10-11-stage4-multi-endpoint-service-design.md)

## 1. 本片要回答的问题

SPEC-093 §5.1 如实登记其范围之外的一处共享：**第一个**服务端点（`ALPHA`）的栈仍沿用
**存活客户端**（task 8）经 `SYS_MEMSEG_CREATE` 建的那条 `Y[368]` 段——两个任务在
`USER3_STACK_VA` 处映射**同一物理栈帧**。SPEC-093 只闭合了「**两端点**共享栈帧」。

本片闭合该登记项：`ALPHA` 端点映射**独立栈段**，判决性见证从「两端点栈段相异」升级为
「**客户端 / `ALPHA` / `BETA` 三者栈段两两相异**」。

## 2. 机制

### 2.1 bring-up 另建独立栈段

`prepare_task_create_caps` 现除代码段 A/B、栈段 B 外，再建一条 **1 页栈段**（`{SegMapR,
SegMapW}` 的 `allowed_access`，根 cap 自动含 `Derive`），cap 槽 `SERVICE_STACK_CAP_SLOT`。

### 2.2 `ALPHA` 的栈映射改指向它

SPEC-088 起的第 4 次 `SYS_TASK_CREATE` 之后，init 把栈段映射进服务根的那一步，从
`put_load_reg(..., TASK_RECEIPT_STACK_SEG)`（`Y[368]`，与客户端同一段）改为
`mov rdi, SERVICE_STACK_CAP_HANDLE`（10 字节 `mov r64, imm64`）。其余字段不变。

客户端（task 8）仍映射 `Y[368]`；`BETA`（task 10）仍映射 SPEC-093 的 `SERVICE2_STACK`。
故三者各拥**独立物理栈帧**。

## 3. 新 cap 槽位与 boot 期 cap 计数耦合（本片实质难点）

新增一条 bring-up cap 触发了 boot 期一处**隐性耦合**，定位与修复如下（判决性证据见报告 §5）：

1. **`ns_create`（SPEC-081）把 Namespace 根 cap 铸在固定预留区间**
   `[NS_ROOT_CAP_SLOT_BASE=52, +MAX_NAMESPACE_OBJECTS=8)` = **52..59**。
2. **`USER_IPC_CAP_SLOT_BASE = 47`，SPEC-087..093 的 usercap 已占到 47..56**，与该区间
   **重叠**——`ns_create` 实际只剩 **57/58/59** 三槽，恰好够 boot 的 3 次 `ns_create`。
   任何落在 57..59 的新 cap 都会让第 3 次 `ns_create` 无槽（`ENOSPC`）并级联（namespace
   见证退化、`audit.used` 掉 12）。
3. `SERVICE_STACK_CAP_SLOT` 置于区间**之外**：**61** = `52 + 8 + 1`（60 是 `UNSET_CAP`
   哨兵，不可占用）。
4. 仅挪槽位仍不够：boot 自测的 `task_create` 前置 `live_count + 3 + initial_count ≤
   MAX_CAPABILITY_SLOTS` 在 cap 达 63 时（64 上限下）失败，调度器随后在 init 的 `SYS_YIELD`
   上 `pick_next` 无 Ready 任务而 panic（实测 `syscall.rs::yield_call`）。故把
   `MAX_CAPABILITY_SLOTS` **64 → 72**（余量足够 `63 + 3 + 2 = 68`），随报告**照实重测** 13 KiB
   结构体守卫（不变，见 `bootstrap_state_stays_within_the_stack_budget`）。

> 上述 1–2 是本片**发现的既有隐患**（区间重叠自 SPEC-087 起存在），已登记，暂未重构（重构
> 会平移 SPEC-087..093 的 usercap 槽位、改动既有证据）。

## 4. 判决性见证（`service_selected_ready`，seq 28，扩展）

在 SPEC-093 的四字段之后追加 `client_ep` / `client_stack_seg`：

```
...;alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x8;client_stack_seg=0x16
```

- `client_ep` 取自 init 收到的 Hello payload（客户端自报 id）；`client_stack_seg` 由内核读
  客户端在 `USER3_STACK_VA` 处映射的**所属 Segment id**（`bootstrap::mapped_segment_at`）。
- 判决性：`alpha(0x12) / beta(0x11) / client(0x16)` **两两相异**——证明 `ALPHA` 不再沿用客户端
  `Y[368]` 的共享物理栈帧。

## 5. 回归面

见报告 §4（基线 = SPEC-093 HEAD；逐条登记）。

## 6. 未决与登记

1. **cap 槽位区间重叠隐患**（§3 第 1–2 条）：usercap 47..56 与 Namespace 根 cap 预留区间
   52..59 重叠。本片以「新 cap 放区间外 + 扩上限」绕过，未重构布局。
2. 服务注册表仍为**编译期内联 `cmp`**、名字为 64 位码——由紧随的 SPEC-095 闭合。
3. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
