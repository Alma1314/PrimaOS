---
id: report-p3-async-state-machine
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-18
---

# P3-0：异步批量契约与状态机

## 结论与范围

本报告记录 P3-0：把契约 P3 草案落成可执行的 Store/Producer 状态机，并用有界穷举
交错给出 host 证据。**本片没有 wire 标签、没有 Store/前端接线、没有 QEMU 证据**；
P3-1 起才进入真实会话。P3 未收口，P2 状态与 7-A 勾选不变。

## 基线与输入

- 基线：`f8922b3`（P2b-6 已提交）。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节（草案）。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md) 的 P3-0。

## 关键设计决定：IPC 边而非锁自由共享内存

草案最初按 `AtomicU64` + Release/Acquire 描述控制页。实现前复核发现：

- `prima-runtime` 与 `prima-user` 都是 `#![forbid(unsafe_code)]`，把映射页转成
  `&AtomicU64` 需要引入 `unsafe`；这与仓库的零 unsafe 约束冲突，并会新增需要
  独立形式证明的并发模型。
- 本 OS 的同步边本来就是能力化 IPC：生产者写完整页后发送 doorbell，Store 只在
  收到 doorbell 后读取；Store 写回结果后发送 `COMPLETE`。双方都不在对方仍写页时
  读取，因此顺序由消息队列承担，无需页内原子。

契约 P3 节已相应修订为 **IPC 顺序模型**，并显式记录被拒备选（原子环）与重评条件
（P5 跨核测量）。所有权、`tail`/`head`、满队列、取消、关闭与序号回绕语义不变。

## 实现内容

`prima-runtime/src/async_batch.rs`（新模块）：

- `CREDITS=4`、`FAIR_QUOTA=2`；`Session`（Store 侧权威）与 `Producer`（客户端视图）
  都是纯状态机，不访问内核、映射内存或共享原子。
- `Session::publish(generation, tail)`：doorbell 校验；重复 doorbell 幂等，
  倒退 `tail` 触发重置，`tail-head>K` 返回 `Full`。
- `Session::claim/complete`：FIFO 认领与完成；已取消的未认领批返回
  `Claim::Cancelled`，由调用方写 `ECANCELED` 而不执行。
- `Session::cancel`：仅命中未认领批；已认领返回 `Claimed`，范围外返回 `Missing`。
- `Session::reset`：把未认领批标记为取消、递增 `generation`、拒绝后续发布；
  已认领批仍按序完成。
- `Session::close`：仅空队列成功，否则 `Busy`；重置后返回 `Reset`。
- `u64` 计数器全部 `wrapping_add`；倒退判断用有符号模距离，窗口用
  `wrapping_sub`，因此跨 `2^64` 回绕语义一致。
- `Producer::try_publish/observe/drained`：客户端只在自己发布与观察 `COMPLETE`
  时推进，满窗口返回 `None`。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo test -p prima-runtime async_batch` | 本次 | PASS；6 passed（满窗口、取消前后、倒退回绕、重置、回绕窗口、有界交错） | host |
| `cargo test -p prima-runtime` | 本次 | PASS；51 passed | host |
| `cargo clippy -p prima-runtime --all-targets -- -D warnings` | 本次 | PASS | host/静态 |
| `cargo test --workspace` | 本次 | PASS；1176 passed、0 failed、1 ignored | host |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy、裸机构建、44 文件零 unsafe、personality 边界 | host/静态 |
| `python3 scripts/docs.py self-check/build/check` | 本次 | PASS | 文档 |

有界交错模型：状态包含 `generation/published/claimed/head`、生产者
`published/completed` 与两条 FIFO 消息队列；从空会话起在深度 8 内枚举发布、
doorbell、认领、完成与完成观察的交错，每个可达状态都断言
`head <= claimed <= published`、`published - head <= CREDITS`、生产者窗口 ≤ K、
完成消息单调。模型至少探索 20 个唯一状态（测试断言 `>20`）。

## 失败、反例与限制

- 首版交错测试错误地重建 `Producer` 并在 `observe(0)` 上 `unwrap`，已在运行前
  发现并改为直接推进状态数组；该错误未进入最终证据。
- `claim`/`complete`/`try_publish` 初版用 `+= 1`，回绕测试暴露 debug 溢出；
  统一改为 `wrapping_add`，`cancel`/`observe` 的模距离也随之修正。
- **不是形式证明**：有界交错只覆盖深度 ≤8、无取消分支的状态；取消由确定性用例
  覆盖，未纳入穷举。不得把本片称作无锁并发证明——设计本身不含共享内存并发。
- **未接线**：没有 wire tag、控制页布局代码、Store 排空循环、前端管线或 QEMU；
  `Session` 目前只有 host 使用。契约 P3 仍是草案，未由实现签收。
- `FAIR_QUOTA` 只在契约中冻结，状态机未实现轮转调度（属于 P3-1）。
- 未做 `ECANCELED` errno 常量、控制页校验与延迟销毁重试（P3-1）。
- 不改变 v1 会话、既有 72 轮套件与 Linux 差分；未新增 syscall。

## 阶段/profile 影响

不关闭 P3，不改变 P2 状态、阶段 7 九轨勾选或 7-A，不改变 profile v1/v2 与 D12
白名单。契约 P3 节从草案向实现冻结推进了一步，但仍需 P3-1…P3-4 的机器证据。

## 后继关系

- 下一片：P3-1 Store v2 会话（wire tag、多页段、排空与公平、取消/关闭/重置）。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md)。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节。
