# SPEC-088 报告：服务端解析与协商（阶段 4 S4-D2 前置）

> 状态：探索实现；不构成 S4-D2 验收或稳定 ABI
>
> 设计依据：[服务端解析与协商设计](../specs/2026-10-10-stage4-service-negotiation-design.md)
>
> 前置：[SPEC-087 报告](2026-10-10-stage4-init-client-handshake-report.md)（双向握手）

## 1. 本片要回答的问题

SPEC-085/087 里 **init 从不解析**客户端 Hello，只回一个固定字。本片让一个**独立 ring3
Rust 服务端点**真实解析客户端 Hello 的 `MessageHeader` + `ProtocolOffer`，调用
`prima_runtime::protocol::negotiate`，把协商结果与两个负例写回共享帧；内核从同一物理帧读回。

## 2. 判决性见证（`service_negotiated_ready`，本片新增，seq 27）

```
token=0x10000001c;service=0x10000001d;stack=0x0;svc_id=0x9;
forward=0x0;done=0x5ec2000000000000;negotiated=0x1000008000001;negatives=0x100000000
```

- `negotiated = 0x0001_0000_0800_0001`：`major=1, minor=0, features=0b1, max=2048`。
  **判决性在 `max=2048`**——客户端 offer 是 4096，只有把**服务端 offer**
  （`features 0b101, required 0b001, max 2048`）纳入协商才能得到 2048；回显或常量都排除。
- `negatives = 0x0000_0001_0000_0000`：`MajorMismatch(1)<<32 | BadMagic(0)`，由服务端
  **ring3 Rust** 计算，证明 `decode`/`negotiate` 真跑过（逻辑没跑会给 `0xffff` 哨兵）。
- 两字由 ring3 服务写入共享帧 `offset 128/136`，内核从 init 的同一物理帧读回。

## 3. 定位并修复的缺陷

### 3.1 带 cap 消息不能投给 `BlockedRecv` 接收者（`kernel/src/user.rs` broker 块）

内核 `send_call` 对 `was_blocked && cap_count > 0` 返回 `EINVAL`（SPEC-087 注释即此约束）。
服务自报 id 后阻塞在 `recv`，init 直接带 cap 转发会被拒。**修复**：init 先发一条**无 cap**
唤醒（`SERVICE_PREPARE_TAG`），再带 cap 转发；服务端 `receive` 循环**跳过无 cap 消息**，
直到取到带共享 Segment cap 的那条。

### 3.2 S2-D4「入口越界」负例的硬编码坏值（`kernel/src/user.rs`）

负例 1 原用硬编码 `USER3_BASE_VA + 0x9000` 作为越界入口。嵌入 ELF 增长到 **10 个代码页**
（`code_end = +0xA000`）后该值落入代码段内，负例改由后置的 `initial_cap` 检查以 `EACCES`
拒绝（实测 `bad_entry=0x…fff3`），而非预期的 `InvalidEntry`（`EINVAL`，`0x…ffea`）。
**修复**：坏入口改为**代码段末尾**（one-past-end，`USER3_CODE_VA + RUNTIME_APP_CODE_PAGES*4K`），
`bad_entry` 恢复为 `EINVAL`。

## 4. 发现的未修问题（登记为反例）

**畸形句柄触发内核 panic**：把 `task_create` 的错误码（负值 errno）误当句柄传给
`task_create` 的 `initial_caps` 时，`Handle::decode` 只查格式（index/gen 非零）不查范围，
`CapabilityRecordTable::find` 以 `entries[handle.index()]` 直接索引 → 越界 panic
（实测：`kernel/src/bootstrap.rs:1133`，`index ≥ MAX_CAPABILITY_SLOTS`）。该路径对
**不可信用户输入可达**（ring3 可构造 `0xffff_ffea_0000_0001`），本片按最小改动**未修**，
登记为待办（见 tracker `ISSUE-CAP-001`）。

## 5. 容量与布局变化（逐条登记）

| 项 | 变化 | 原因 |
|---|---|---|
| `USER_TASK_KSTACK_COUNT` | 6 → 7（覆盖 id 3..9） | 服务端点拿到 id 9 |
| `MAX_VSPACE_ENTRIES` | 32 → 44 | 服务代码段 10 页 + 栈 1 页的映射把 32 挤满 |
| `VspaceEntry.task_id` | `u32` → `NonZeroU32` | 给 `Option<VspaceEntry>` 提供 niche，使每槽 40→32 B；容量 44 时 `BootstrapState` 仍在 13 KiB 守卫内（不放宽守卫） |
| `MAX_CAPABILITY_SLOTS` / `MAX_AUDIT_EVENTS` / `MAX_SEGMENT_OBJECTS` | 不变（64 / 256 / 32） | 实测足够 |

## 6. 回归面（逐条登记）

| 事件 | 变化 |
|---|---|
| 新增 `service_negotiated_ready` | seq 27；其后事件顺移 1 位，`boot_complete` 40→41 |
| `task_ready` | `tasks`/`tasks_after` 4→5；`caps`/`caps_after` 49→54（服务端点 5 条 cap） |
| `ipc_ready` | `queued/direct/blocks` 4/5/6 → **6/7/9**（服务 id 上报 + 转发请求/无 cap 唤醒 + 服务与 init 的阻塞） |
| `memory_ready`/`bootstrap_ready`/`frames_ready`/`self_root_ready`/`preempt_ready`/`state_audit_ready` | 布局/计数类字段（KSTACK BSS +16 KiB、嵌入 ELF 增长、多一个任务） |

**未重建 pre-merge 事件流 diff**（镜像变化与 stage4 纠缠，方法边界如实登记）；当前 **41 条**事件
基线为冻结真值。

## 7. 验证（2026-10-10）

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿：cosir 145、cosir_state 40、lean_backtrace 2、kernel 365、prima-ostd 21、xtask **195** |
| `cargo xtask lint` | 四条全 PASS（fmt / clippy `-D warnings` / 内核裸机构建 / `kernel/src` 零 unsafe·27 文件） |
| `cargo xtask tcb` | `trusted_lines=1262 mechanism_lines=20219 total_lines=21481 ratio=5.87%`、机制层 `unsafe=0`，PASS |
| `cargo xtask test` | BIOS run=1/2 六条 PASS（**41 条事件**，含 seq 27 `service_negotiated_ready`） |
| `cargo xtask compat` | `PASS compat cow-write-fault` |

反例：xtask 单测 `rejects_service_negotiated_when_result_is_echoed`（把 `negotiated` 换成客户端
offer 原值即失败）与 `rejects_service_negotiated_without_negative_cases`。

## 8. 未决与登记

1. **仅一次服务端解析+协商**：不构成服务互通、调度或安全证明；**S4-D2 与阶段 4 交付项/退出
   条件一律不勾选**。
2. 端点选择仍是内核侧 init 写死的单一端点，未引入多服务名称解析。
3. `ISSUE-CAP-001`（畸形句柄越界 panic）未修。
4. 快路径保存块与中断帧一致性、PCID/嵌套/多核仍未涉及。
