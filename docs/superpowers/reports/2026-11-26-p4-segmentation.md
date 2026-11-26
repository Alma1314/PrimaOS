---
id: report-p4-segmentation
kind: report
authority: scoped-review
status: reviewed-with-limits
lifecycle: current
updated: 2026-11-26
---

# P4-1：Gate 递送/恢复分段确认与候选

## 结论与范围

本报告把 [P4-0 基线](2026-11-26-p4-baseline.md) 的 G5 阶段映射到 Kernel 代码路径，
确认 `queue-deliver`（p50 约 1.40 ms）与 `deliver-reply`（约 0.83 ms）主要是
**两次任务切换与用户态恢复**（FXSAVE/FXRSTOR、CR3、iretq），而不是单一的查表或
拷贝；现有 5 个阶段不足以把这两段再归因。因此 P4-2 不应在没有细分证据时直接
选择“减少复制”实现。本片**不改代码**，交付候选排序、所需证据与停止条件。

## 基线与输入

- 基线提交：`4f4fc49`（P4-0 已提交）。
- 基线数据：[P4-0](2026-11-26-p4-baseline.md)：G5 两轮 128 samples，`hz≈2.08 GHz`。
- 计划：[P4 Gate/IPC 路径收窄执行切片](../plans/2026-11-26-p4-path-narrowing.md) P4-1。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P4。

## 阶段边界（现有 5 点）

| 段 | 起点 | 终点 | 包含 |
|---|---|---|---|
| trap-queue | `syscall.rs:4592`（客户端 trap 入口） | `syscall.rs:4599`（入队/阻塞后） | `gate_trap` 绑定查找、审计预算、请求记录、客户端阻塞与唤醒服务 |
| queue-deliver | 入队后 | `syscall.rs:4290`（服务 `GATE_WAIT` 返回） | 调度选择、客户端→服务切换、`gate_wait` 输出准备 |
| deliver-reply | 服务拿到请求 | `syscall.rs:4312`（`GATE_REPLY` 后） | 服务端处理、`gate_reply` 写回、唤醒客户端 |
| reply-resume | reply 后 | `sched.rs:568`（客户端 resume） | 调度回到客户端 |
| user-entry/return | 客户端 RDTSC 端点 | — | 用户 wrapper 边界 |

P4-0 的占比：`queue-deliver` 48%、`deliver-reply` 29%、`trap-queue` 7%、
`reply-resume` 15%（p50，按 kernel-total）。

## 代码路径事实

- 每次 Gate 往返至少两次任务切换（client→service→client）；`resume` 每次执行
  `write_cr3`、`activate_fs`（MSR）、`restore_fpu`（FXRSTOR 512 B）、信号检查、
  `flush_deferred_frames`、`stamp(4)` 与 `iretq`（`kernel/src/sched.rs:544`）。
  切换离开时对应 `FXSAVE` 512 B（`sched.rs:340`）。
- `gate_trap` 的查表是 64 槽线性绑定扫描（`gate.rs:461`）加两次 `gate_tables`
  小描述符读取（`Tables` 是 3 个 `ArenaSlice` + 4 个 u32，`gate.rs:158`），不是
  整表拷贝；审计 append 与 completion ticket 是小对象与位图操作。
- `gate_wait` 交付时安装 request root capability、更新队列/容量并写回请求记录
  （`gate.rs:1236`）；输出 96 B `GateRequest` 由用户包装解码
  （`prima-runtime/src/gate.rs:155`）。
- `gate_copy_call` 对每次 ≤4 KiB 的数据面复制使用 4 KiB 栈 scratch 做
  “用户→scratch→用户”两次页遍历与写（`kernel/src/syscall.rs:4322`）；该路径不在
  G5 往返内，Linux 数据面已由 P3 v2 共享页绕开大部分复制。
- `SYS_SEND` 直投写回 24 B 接收缓冲与最多 2 个 capability
  （`kernel/src/syscall.rs:3173`）；I1 的 p50 远高于 N1，说明递送/调度占主导。

## 候选排序

| 顺序 | 候选 | 预期收益 | 风险/所需证据 |
|---|---|---|---|
| 1 | 细分 `queue-deliver` 与 `deliver-reply`：追加可选阶段（唤醒后、resume 前、`gate_wait` 输出前） | 把 1.4 ms 拆成调度等待、切换/FPU/CR3、查表与输出 | 只追加字段、旧 G5 字段逐字节冻结；需更新解析/断言 |
| 2 | FPU 惰性切换（CR0.TS + #NM 延迟 FXSAVE/FXRSTOR） | 每往返省 4×512 B 保存/恢复，可能最大 | 高：改变 FPU 状态语义与异常路径；需独立设计与反例 |
| 3 | 绑定槽索引（Task→slot 直接数组替代 64 槽扫描） | trap-queue 小段 | 低；收益可能在噪声内，需细分证据 |
| 4 | `gate_copy` 直接页间复制（去掉 4 KiB scratch） | 数据面 v1/信号路径 | 需新微基准；不在 G5 往返内 |
| 5 | `SYS_SEND` 直投写回合并 | 小 | 需保持能力位置/审计顺序 |

## 未做与限制

- 未新增细分探针；上述归因是代码审查加现有阶段占比，不是新的机器分段。
- 未实现任何候选；未测 FPU 惰性、绑定索引或 `gate_copy` 微基准。
- 宿主争用与 TCG 噪声仍如 P4-0；p50 占比只用于排序，不作门槛。
- 本片不改变 P3（未签署）、阶段 7、7-A、profile 或既有 QEMU 事件。

## 阶段/profile 影响

P4-1 审查完成，未改行为；P4-2 需先补齐细分证据或选一个可证伪的低风险项。

## 后继关系

- 下一片：P4-2 单项收窄；若选细分探针，必须先登记追加字段与旧字段冻结。
- 计划：[P4 Gate/IPC 路径收窄执行切片](../plans/2026-11-26-p4-path-narrowing.md)。
- 基线：[P4-0](2026-11-26-p4-baseline.md)。
