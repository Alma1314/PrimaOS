# SPEC-092 设计：独立第二服务端点（init 按名路由到不同任务）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 前置：[SPEC-090 服务管理器](2026-10-11-stage4-service-manager-design.md)、
> [SPEC-091 vspace 存储移出](../reports/2026-10-11-stage4-vspace-arena-report.md)、
> [SPEC-088 服务端解析与协商](2026-10-10-stage4-service-negotiation-design.md)

## 1. 本片要回答的问题

SPEC-090 让 init 按客户端写入的名字解析端点并路由，但报告 §6.1 如实登记：两个注册服务由
**同一宿主 ring3 任务**承载，**独立多端点任务**待后续切片。SPEC-091 把逻辑 vspace 存储移出
`BootstrapState`，解除了「第二个完整 Rust 服务任务需 +11 条 vspace 条目」的存储墙（当时
41/44 仅 3 槽余量、13 KiB 软守卫仅余 72 B）。

本片落地该待办：**新增一个与第一个服务**完全独立**的 ring3 Rust 服务任务，init 按名字把请求
路由到不同任务**，判决性见证据此区分**端点身份**（task id / 独立根），而非仅行为码。

## 2. 机制

### 2.1 第二个服务任务

在 init 的机器码里，于第一个服务端点（`ALPHA`，id 9）完成协商之后，**第 5 次 `SYS_TASK_CREATE`**
建第二个端点（`BETA`，id 10），复用同一嵌入 ELF：

- 从代码段根 cap 派生**第二个端点令牌**（`{SegMapR}`）；
- `initial_cap0` = 新铸的 `SERVICE2_SEND_CAP`（`Task(1)`+`TaskSend`，槽 `USER_IPC_CAP_SLOT_BASE+7`）；
- 把栈段映射进 B 的独立根；
- B 启动后自报 `SERVICE_ID_TAG | id`，init 用**第二个** `recv` 循环收集其 id。

`USER_TASK_KSTACK_COUNT` 7→8（覆盖 id 3..10；id 10 若无内核栈，`register_created_task` 确定性失败）。

### 2.2 段 cap 的分发（本片最易错处）

客户端 Hello 段 cap 由 init 收到后：**先派生**一条 `{SegMapR, SegMapW, **Derive**}` 子集 cap 供
B 用，**再**把原始 cap 转发（移动）给 A。两条硬约束：

1. `memseg_map` 的前置是 `authorize(caller, seg_cap, Right::Derive)`——**段 cap 必须含 `Derive`**，
   故派生 cap 不能只给 `{R,W}`；
2. 转发会**移动**原 cap，移动后 init 不再持有，故派生必须在转发**之前**。

### 2.3 按名路由到不同端点

SPEC-090 的二次路由块把解析到的行为发给**端点 id**；本片改为发给**第二个端点 id**
（`SVC_RECEIPT_ID2`）。`ALPHA` 首转仍走 A（SPEC-088 逐字节不变），`BETA` 按名路由到 B：
先发无 cap `prepare(behavior)` 唤醒 B（内核拒绝向 `BlockedRecv` 投递带 cap 消息），再转发 Hello 段 cap。

### 2.4 端点身份的绑定

两端点运行**同一 ELF 代码**，按收到的 `prepare` payload 低 8 位选 offer，并在写回协商结果的同时
把**自己的 task id** 写进共享段：`160 + (behavior-1)*8`（`ALPHA`→160、`BETA`→168）。
这把「结果」绑定到「产出它的端点身份」。

## 3. 判决性见证（`service_selected_ready`，seq 28，扩展）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;
alpha_ep=0x9;beta_ep=0xa;beta_self=0xa
```

- `alpha_ep=0x9`、`beta_ep=0xa`：两个**相异**的 task id——`ALPHA` 与 `BETA` 由**两个独立任务**
  （独立根、独立内核栈）承载，而非 SPEC-090 的「单宿主多行为」。
- `beta_self=0xa`：init 从**第二个** `SERVICE_ID_TAG` 收集到的端点 id，须等于写回结果的 `beta_ep`
  ——证明请求确实送到了被记录的那个端点，而非任意任务。
- `negotiated_beta` 的 `max=1024` 仍只可能来自 `BETA` offer（B 在自己根下映射段、独立协商）。
- `alpha_ep != beta_ep` 单独成条拒绝（见反例 `rejects_service_selected_when_beta_ep_is_not_the_second_task`）。

## 4. 回归面（逐条登记）

| 事件 | 变化 |
|---|---|
| `service_selected_ready`（seq 28） | 新增 `alpha_ep`/`beta_ep`/`beta_self` 三字段 |
| `ipc_ready`（seq 23） | queued/direct/blocks 6/9/11 → **8/9/12**（第二端点自报 + Hello 转发排队、额外阻塞 recv） |
| `task_ready`（seq 24） | `tasks`/`tasks_after` 5→**6**、`caps`/`caps_after` 54→**60** |
| 布局/计数类 | `memory_ready`（usable/image/kend）、`bootstrap_ready`（image_len/frames）、`preempt_ready`（rsp/kstack 地址）、`frames_ready`（universe/free/table/image）、`self_root_ready.pages`、`state_audit_ready`（total/used…）随镜像与 kstack BSS 增大而变 |
| 其余事件 | **逐字节不变**（含 seq 27 `service_negotiated_ready`：`token=0x1c`/`service=0x1d` 未顺移——见 §5） |

`cargo xtask test` 的 `validate_*_data` 对布局字段只断言结构不变式、不钉死数值，故布局类变化由
**手工事件流 diff** 逐条登记。

## 5. 一个被规避的回归：句柄槽顺移

`memseg_map` 前置要求段 cap 含 `Derive`，故 B 的派生 cap 必须在转发前做。若把该派生放在
SPEC-088 令牌派生**之前**，它会先领到最低空闲槽，使第一个端点的 `token`/`service` 句柄各顺移一格
（`0x1c/0x1d` → `0x1d/0x1e`），改变既有 `service_negotiated_ready` 证据。故把派生**移到令牌派生
之后、转发之前**——既满足移动前派生，又不动既有句柄。

## 6. 未决与登记

1. **同一 ELF、共享物理栈帧**：两个端点由**同一嵌入 ELF** 创建，且沿用 SPEC-088 的共享栈帧；
   真正的「不同二进制的多服务」与独立栈帧待后续切片。
2. 注册表仍为编译期常数；名字为 64 位码而非字符串路径。
3. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
