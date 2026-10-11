# SPEC-090 报告：服务管理器（init 按名解析并路由，阶段 4 S4-D2 前置）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[服务管理器设计](../specs/2026-10-11-stage4-service-manager-design.md)
>
> 前置：[SPEC-088 报告](2026-10-10-stage4-service-negotiation-report.md)

## 1. 本片要回答的问题

SPEC-088 让 init 作为 broker 把客户端 Hello 转发给**一个写死的** ring3 Rust 服务端点，并在其
报告 §8.2 登记「端点选择仍是内核侧 init 写死的单一端点，未引入多服务名称解析」。本片落地 S4-D2
「服务管理器」的第一条实质能力：**init 持有服务注册表，按客户端写入共享段的名字解析端点并路由，
未注册名被拒绝**。

## 2. 判决性见证（`service_selected_ready`，本片新增，seq 28）

```
name=0x2;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001
```

- `negotiated_beta = 0x0001_0000_0400_0001`：**判决性在 `max=1024`**——只有服务按 init 路由到的
  `BETA` 行为选择 `SERVICE_OFFER_BETA`（`max 1024`，客户端的 4096 与 `ALPHA` 的 2048 都不是）
  才可能得到。若 init 忽略名字硬编码行为，服务会用 `ALPHA` offer 得 `max=2048`。
- `name=2`/`resolved=2` **绑定**：`name` 由**客户端写入共享段 offset 48**（`HELLO_SERVICE_NAME_OFFSET`），
  init 从同一物理帧 `USER_HELLO2_VA+48` 读回；解耦即失败。
- `unknown=0`：init 解析未注册名 `0xDEAD` 得 `NONE`，证明**真的查表**而非对任意输入都成功。

## 3. 定位并修复的缺陷（手写机器码的跳转补丁）

第一版实测 `resolved=0x2;unknown=0x2`——**未知名也解析成 2**，即「按名解析」实际退化为**恒为 2
的硬编码**（判决性因此失效）。dump 字节定位：解析块的 `cmp rax, imm32` 之后 `jne rel32` 恒为 `0`
（落穿），而 `cmp` 的立即数被写成了 `0x10`。

**根因**：`let name_jne = at;` 被捕获在 **`cmp` 的位置**，但补丁 `put(code, name_jne + 2, rel)` 写的是
`cmp` 的**立即数字段**；`jne` 的 `rel32` 从未被写（保持 0 → 永远落穿 → `mov r8, 2` 恒执行）。
**修复**：把捕获点移到 `jne` 指令处（`let name_jne = at` 置于写出 `0F 85` 之后）。两处解析器
（请求名与未知名）同修。修复后 `unknown=0x0`、`name/resolved` 仍正确，判决性恢复。

该缺陷印证了「仅靠合成 golden 字符串的 host 反例不足」：三个 xtask 反例对**合成输入**成立，
但**真实机器码**的解析恒为 2 需 QEMU 实测才暴露。修复由 QEMU 门禁的 seq 28 真实事件钉住。

## 4. 回归面（逐条登记）

| 事件 | 变化 | 说明 |
|---|---|---|
| 新增 `service_selected_ready` | seq 28 | 其后事件顺移 1 位，`boot_complete` 41→42；`EXPECTED_EVENTS` 41→42 |
| `ipc_ready` | `queued/direct/blocks` 6/7/9 → **6/9/11** | 二次路由的无 cap `prepare` 直投（direct +2）+ 服务/init 额外的阻塞 `recv`（blocks +2）；校验器文案同步扩写 |

**其余事件逐字节不变**：本片未改镜像布局、任务数、cap 计数与帧归属，首次转发仍走行为 `ALPHA`
（`service_negotiated_ready` 的 `negotiated=0x1000008000001` 不变）。QEMU 42 条事件终点冻结。

TCB 机制层行数 20267 → **20446**（+179：解析/路由块与见证；`unsafe` 仍 0）。

## 5. 验证（2026-10-11）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel **366**、prima-ostd 21、xtask **198**（+3 反例） |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20446 total_lines=21708 ratio=5.81%`、机制层 `unsafe=0`、PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**42 条事件**，含 seq 28 `service_selected_ready`） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例（xtask 单位测试，对合成输入）：
`rejects_service_selected_when_routed_to_wrong_behavior`（把 `BETA` 结果换成 `ALPHA` 的 `max=2048` 即失败）、
`rejects_service_selected_when_name_is_hardcoded`（名字与解析结果脱钩即失败）、
`rejects_service_selected_when_unknown_name_is_not_refused`（未知名解析非 `NONE` 即失败）。

## 6. 未决与登记

1. **单一宿主任务**：两个注册服务由**同一 ring3 任务**承载（按 init 路由的行为码选择 offer）；
   **独立多端点任务**待后续切片。原因见设计 §4.1（`MAX_VSPACE_ENTRIES` 逐页记账、`BootstrapState`
   13 KiB 守卫、客户端/服务已共享物理栈帧）——这是**如实登记的简化**。
2. 注册表为编译期常数（无动态注册 API），名字为 64 位码而非字符串路径；namespace 的名字→cap
   解析是另一条线，本片未接入。
3. **S4-D2 与阶段 4 交付项/退出条件一律不勾选**；闭合判断交复核者（组织级独立性门槛）。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
