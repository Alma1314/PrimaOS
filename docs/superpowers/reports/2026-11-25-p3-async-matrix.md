---
id: report-p3-async-matrix
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-25
---

# P3-3：直连 v2 会话 QEMU 正反例矩阵

## 结论与范围

本报告记录 P3-3：为 P3 的直连（direct-client）版本 2 异步会话新增 V5 验收镜像
与 `cargo xtask test-async`，在 QEMU 中跑两轮 10 个用例，取得队列、拒绝、取消、
关闭、两会话公平与单侧退出后继续进展的机器证据。证据只覆盖 Prima 直连 v2 会话；
Linux v2 前端的读/写/fork/exec 与 park/唤醒由既有 W4–W9、代理矩阵和
[P3-2 报告](2026-11-24-p3-frontend-v2-data-plane.md)覆盖，本报告不重复。P3 仍未
收口（P3-4 与组织级复核待做），7-A 未勾选。

## 基线与输入

- 基线：`0f65768`（P3-2 C 控制面锁拆分已提交，工作树干净）。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md) 的 P3-3。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节。
- 前序：[P3-1 Store v2](2026-11-18-p3-store-v2.md)、
  [P3-2 数据面](2026-11-24-p3-frontend-v2-data-plane.md)、
  [P3-2 控制面锁](2026-11-25-p3-adapter-control-lock.md)。

## 实现

- `prima-user/c/prima_async_probe.h`：仅在 `PRIMA_ASYNC_PROBE` 下编译的探针，
  直接复用生产客户端的 `prima_async_open`/会话函数与页布局常量。它不新增生产
  入口，不改变 v1/v2 客户端语义。
- `prima-user/c/stage5_async.c`：验收程序，先经生产 `write` 打开 v2 会话，再按
  顺序调用探针并打印 `S9ASYNC|case=...;status=...` 与 `S9ASYNCDONE|fail=...`。
- `xtask/src/image.rs`：新增 `Stage5Program::Async` 与 `build_images_for_async`
  （`-DPRIMA_ASYNC_PROBE`），复用现有 C 探针镜像组装与 Store 服务。
- `xtask/src/main.rs`：`test-async` 构建 V5 镜像并跑两轮；逐条断言 10 个 case
  行与 `S9ASYNCDONE|fail=0`，并要求两轮 `S9ASYNC` 行流完全一致；拒绝 panic。
- `xtask/src/qemu.rs`：`run_bios_probe_capture` 只校验启动退出码，因为 V5 的
  IPC 形状与默认 C 探针基线不同；场景断言由 `test-async` 负责。

### 用例与覆盖

| case | 期望与验证点 |
|---|---|
| `open` | 生产首次 `write` 已打开会话；映射与 Store 写出的 `PBQ2` 头有效 |
| `merged` | 一次 doorbell 发布 2 批，单条合并 COMPLETE；两页状态 0、读回内容正确 |
| `full` | `tail = head + K + 1` 的 doorbell 返回 `EAGAIN`；会话仍可用，尾指针恢复 |
| `replay` | 重复 doorbell 幂等；随后对同一 ordinal 的 CANCEL 返回 `EINVAL`（未重复执行） |
| `rewind` | 回退 `tail` 被 Store 以 `EACCES` 重置；客户端解除映射并重新 OPEN |
| `reserved` | 控制页保留字节非零 → `EACCES` + 重置；无批执行；重新 OPEN |
| `cancel` | 已发布未 doorbell 的批取消后以 `ECANCELED` 完成，写入不可见；已完成后取消返回 `EINVAL` |
| `fair` | 两个线程各自 Task 的直连会话发布 2 批后同时 doorbell；两会话都收到自己的合并 COMPLETE |
| `death` | 一个线程发布并 doorbell 后直接退出；存活会话继续发布并完成 2 批 |
| `close` | 全部完成后 `ASYNC_CLOSE` 返回成功 |

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `cargo xtask test-async` | 本次 | PASS；`qemu bios V5 run=1/2`、`P3ASYNC cases=10 runs=2`；两轮 case 流一致 | QEMU（V5 镜像） |
| `cargo test --workspace` | 本次 | PASS；1181 passed、0 failed、1 ignored | host |
| `python3 prima-user/musl/test_port.py` | 本次 | PASS；7 tests | host（C 适配层） |
| `cargo xtask lint` | 本次 | PASS；fmt、clippy、裸机构建、44 文件零 unsafe | host/静态 |
| `CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -q -p xtask --features linux-probe -- test` | 本次 | PASS；`full BIOS suite: 36 programs, 72 runs`、`stage1 boot slice` | QEMU（既有套件回归） |

## 失败与限制

- **认领后取消不在机器证据内**：单线程 Store 在一条 doorbell 消息内认领并完成
  批次，客户端无法稳定让批停在已认领态；`Cancel::Claimed`/`EALREADY` 仍只有
  [P3-1](2026-11-18-p3-store-v2.md) 的状态机 host 证据。
- **发布页改写快照未单独取证**：doorbell 之后客户端改写数据页与 Store 读页是
  竞争关系，无法稳定断言“只按快照执行”。`replay` 用“重复 doorbell 不新增批”
  覆盖重放副作用，但不等于页改写时序证明。
- **公平性是进展证据**：`fair` 证明两个 session 在并发 doorbell 下都能取得合并
  COMPLETE，不测量跨会话吞吐或每轮配额时序；配额本身由 host 调度器用例覆盖。
- **单侧退出只证明另一侧继续**：`death` 不直接观察 Store 对死会话槽位的回收
  时刻；回收由既有死亡回收路径与容量事件间接覆盖。
- V5 是 Prima-only 探针镜像，没有 Linux 参考差分；IPC 基线不复用默认 C 探针的
  固定计数（`run_bios_probe_capture` 只校验退出码）。
- 探针复用同一文件的句柄跨线程；并发 `fair` 两个会话写不同偏移，不覆盖共享
  游标竞争。
- 无 SMP、恢复/去重、性能门槛；未新增 syscall，未改写既有 V1–V4 与代理矩阵事件。

## 阶段/profile 影响

为 P3-3 提供有界机器证据；不关闭 P3（P3-4 收口与组织级复核待做），不勾选 7-A、
不改变 P2 状态、阶段 7 九轨或 profile。

## 后继关系

- 下一片：P3-4 P3 收口（汇总命令、结果、失败与限制，准备组织级复核输入）。
- 计划：[P3 有界异步批量切片计划](../plans/2026-11-18-p3-async-slices.md)。
- 契约：[LibOS 数据通路契约](../specs/2026-11-10-libos-data-path.md) P3 节。
