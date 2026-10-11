# SPEC-090 设计：服务管理器（init 按名解析并路由）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 前置：[SPEC-088 服务端解析与协商](2026-10-10-stage4-service-negotiation-design.md)、[SPEC-087 双向握手](2026-10-10-stage4-init-client-handshake-design.md)

## 1. 本片要回答的问题

S4-D2 的意图是 **init 作为「服务管理器」**：在 capability 模型下 sibling 任务无法互相寻址，
客户端只能把请求交给 init，由 init 决定转发给哪个服务。SPEC-088 已让 init 作为 broker 把
客户端 Hello 转发给**一个写死的** ring3 Rust 服务端点（`service_negotiated_ready`），但
SPEC-088 报告 §8.2 如实登记：**端点选择仍是内核侧 init 写死的单一端点，未引入按名解析**。

本片判定并落地「服务管理器」的第一条实质能力：**init 持有服务注册表，按客户端写入的名字
解析端点并路由；未注册的名字被拒绝**。

## 2. 机制

### 2.1 名字来源与 wire 约束

`SYS_RECV` 返回的是消息 **payload**（`kernel/src/syscall.rs:1519`），不是 sender id；现有
「回执目标 = `recv` 返回值」只是因为客户端恰好把自身 id 放进 payload。因此**服务名不能复用
payload**，必须走**共享段内的固定偏移**：客户端把请求服务名写到段内 `SERVICE_NAME_OFFSET=48`
（20 B 头 + 24 B offer 之后），init 从它对同一物理帧的映射 `USER_HELLO2_VA+48` 读回。

### 2.2 注册表与行为码

注册表把**名字**映射到**行为码**：`name==BETA(2)` → 行为 `BETA(2)`，其余 → `NONE(0)`（拒绝）。
init 的首次转发仍走行为 `ALPHA(1)`（保持 SPEC-088 既有证据**逐字节不变**），随后对客户端写入
的名字做一次解析并二次转发；另外解析一个未注册名 `0xDEAD` 作为拒绝负例。

### 2.3 路由的载体

路由决策经**无 cap 的 `prepare` 消息 payload 低 8 位**传给服务：`payload = SERVICE_PREPARE_TAG
| behavior`。服务端按行为码选择自己的 offer：`ALPHA`→`max 2048`（`SERVICE_OFFER`）、
`BETA`→`max 1024`（`SERVICE_OFFER_BETA`），把结果写到 `128 + (behavior-1)*16`
（`ALPHA`→128，`BETA`→144），每周期回 `SERVICE_DONE_TAG`。

## 3. 判决性见证（`service_selected_ready`，本片新增，seq 28）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001
```

- `negotiated_beta = 0x0001_0000_0400_0001`：`max=1024` **只可能来自被按名选中的 `BETA`
  offer**（`ALPHA` 是 2048）。若 init 忽略名字而硬编码行为，服务会用 `ALPHA` offer 得到
  `max=2048`，此字不符。
- `name=2` 与 `resolved=2` **绑定**：`name` 读自客户端写入的共享段，解析结果必须与之一致；
  两者解耦（如名字未写入）即失败。
- `unknown=0`：未注册名解析为 `NONE`，证明解析器**真的查表**而非对任何输入都成功。

## 4. 回归面与容量约束

### 4.1 为什么用「单宿主多行为」而非第二个服务任务

本轮**不新增第二个 ring3 服务任务**，原因是硬容量约束：`MAX_VSPACE_ENTRIES` 按**逐页**记账
（`VspaceEntry` 24 B/条，`kernel/src/bootstrap.rs:2021`），一个完整 Rust 服务任务占用嵌入 ELF
的 10 个代码页 + 1 栈页 = 11 条；`BootstrapState` 受 13 KiB 软守卫（实测 ~12240 B）。新增 11 条
≈ +264 B 尚可，但随之而来的 kstack 表、cap 槽、以及客户端/服务三任务**共享同一物理栈帧**
（SPEC-088 已如此）使风险显著上升。故本片规模收窄为「单一宿主任务承载两个注册服务 + 未注册名
拒绝」，把**独立多端点任务**留给后续切片（见 §5）。这是**如实登记的简化**，不是等价实现。

### 4.2 回归面（逐条登记）

| 事件 | 变化 |
|---|---|
| 新增 `service_selected_ready` | seq 28；其后事件顺移 1 位，`boot_complete` 41→42 |
| `ipc_ready` | `queued/direct/blocks` 6/7/9 → **6/9/11**（二次路由的无 cap `prepare` 直投 + 服务/init 额外的阻塞 `recv`） |
| 其余事件 | **逐字节不变**（未改布局、任务数、cap 计数、帧归属；首次转发仍走行为 `ALPHA`） |

## 5. 未决与登记

1. **单一宿主任务**：两个「服务」由同一 ring3 任务承载（按 init 路由的行为码选择）；**独立
   多端点任务**（第二个 ring3 服务）待后续切片，须先解决 §4.1 的容量/共享栈问题。
2. 注册表为**编译期常数**（无动态注册 API）；名字为 64 位码而非字符串路径（namespace 的名字→cap
   解析是另一条线，本片未接入）。
3. **不勾选** S4-D2 或任何阶段 4 交付项/退出条件；闭合判断交复核者。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
