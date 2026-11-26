---
id: report-p4-segmentation-probe
kind: report
authority: scoped-implementation-and-validation
status: validated-with-limits
lifecycle: current
updated: 2026-11-26
---

# P4-2a：G5 细分采样探针与阶段分解

## 结论与范围

按 P4-1 候选 1，为 G5 增加三个可选采样点：服务 resume 入口、`SYS_GATE_WAIT`
内核入口、客户端 resume 入口，把原 `queue-deliver` 拆成 `queue-wait`、
`switch-service`、`wait-output`，把 `reply-resume` 拆成 `reply-dispatch`、
`resume-client`。旧 G4/G5 字段与事件保持原值，G5 另输出每样本 `S7PHASE` 行，
`S7BENCH` 摘要末尾追加 5 个派生字段。两轮 G5 与两轮 G4 通过；本片只提供测量，
未实现优化、未冻结门槛。

## 基线与输入

- 基线：`2ad966e`（P4-1 已提交）。
- 计划：[P4 Gate/IPC 路径收窄执行切片](../plans/2026-11-26-p4-path-narrowing.md)。
- 前序：[P4-0 基线](2026-11-26-p4-baseline.md)、
  [P4-1 分段确认](2026-11-26-p4-segmentation.md)。

## 实现

- `kernel/src/gate_bench.rs`：新增 G5 专用 `USER_TIMES[[u64;3];128]`；`stamp`
  相位序列改为 0→1→5→6→2→3→7→4（G5），G4 仍为 0→1→2→3→4；G4 下 phase≥5
  的钩子直接返回；`verify` 检查新时间戳顺序；`emit` 输出 `S7PHASE|index;t5;t6;t7`。
- `kernel/src/sched.rs`：`resume` 入口按 Task 记录 5（服务）或 7（客户端）。
- `kernel/src/syscall.rs`：`SYS_GATE_WAIT` 分支入口记录 6。
- `xtask/src/qemu.rs`：解析 `S7PHASE`，校验与非零/顺序，派生 5 个新字段；反例
  覆盖缺行、t5 早于 t1、t5 晚于 t6、t7 早于 t3。
- 普通镜像不编译这些调用；G4 不产生 `S7PHASE`。

### 事件冻结登记

- 旧 `S6TIME`/`S7TIME` 行格式与字段值不变；G4 的 1000 样本与 `S6BENCH` 字段
  不变（实测两轮）。
- G5 的 `S7BENCH` 行在原有 8 个字段后追加 `queue-wait`、`switch-service`、
  `wait-output`、`reply-dispatch`、`resume-client`；旧字段前缀与值语义不变。
- 新增 `S7PHASE` 行每样本一行，仅在 G5 输出；既有解析器不把它当作 `S7TIME`。

## 命令与结果

| 命令/输入 | 本次实跑或引用 | 退出结果与观察 | 证据范围 |
|---|---|---|---|
| `PRIMA_GATE_CASE=G5 … cargo run -q -p xtask --features gate-probe -- test-gate` | 本次 | PASS；两轮各 128 样本，新字段输出 | QEMU（G5） |
| `PRIMA_GATE_CASE=G4 … 同上` | 本次 | PASS；两轮各 1000 样本，无 `S7PHASE` | QEMU（G4 回归） |
| `cargo test -p xtask user_benchmark` | 本次 | PASS；含新字段与 4 个反例 | host |
| `cargo test --workspace` | 本次 | PASS；1205 passed、0 failed | host |
| `cargo xtask lint` | 本次 | PASS | host/静态 |
| `… --features linux-probe -- test` | 本次 | PASS；完整 36 组 72 轮 | QEMU（既有套件） |

G5 两轮摘要（nearest-rank median/p99，单位 ns）：

```text
round1 hz=2082597382: trap-queue 200995/372174; queue-deliver 1428598/3322846;
  deliver-reply 773842/1716655; reply-resume 431717/931630; kernel-total 2894103/6355558;
  user-entry 103376/270047; user-return 11132/24936; user-total 3015065/6573655;
  queue-wait 70837/182099; switch-service 424396/909295; wait-output 910051/2261349;
  reply-dispatch 311844/753605; resume-client 118219/214890
round2 hz=2080506714: trap-queue 464481/2406579; queue-deliver 3125341/9356562;
  deliver-reply 1587909/5954573; reply-resume 947010/2690040; kernel-total 6327969/13620542;
  user-entry 251656/512073; user-return 22286/79871; user-total 6616255/13907496;
  queue-wait 191739/1963173; switch-service 1021784/1811444; wait-output 1881323/6184457;
  reply-dispatch 723596/2354303; resume-client 224170/684796
```

## 有界观察

- round1 的 `queue-deliver`（1.429 ms）≈ `queue-wait` 0.071 + `switch-service`
  0.424 + `wait-output` 0.910；`wait-output` 是最大子段，超过任务切换本身。
- `reply-resume`（0.432 ms）= `reply-dispatch` 0.312 + `resume-client` 0.118。
- round2 整体显著更高（user-total 6.62 ms），宿主/TCG 噪声大；两轮方向一致，
  但本报告不把单轮数值当作收益依据。

## 失败与限制

- 首次 G4 运行在 `gate_bench.rs` 的相位断言 panic：状态机把 G4 推进到新的 5/6/7
  序列。修复为 G4 保持 0..4、phase≥5 钩子直接返回后，两轮 G4 通过；失败保留。
- 新阶段只覆盖内核采样点之间的区间；用户态 wrapper 准备、服务用户态执行与
  output 内容解析仍包含在 `switch-service`/`wait-output`/`reply-dispatch` 内。
- RDTSC 采样本身有少量开销；两轮差异说明不能作门槛，只能作候选排序。
- 未实现任何收窄；未测 FPU 惰性、绑定索引或 `gate_copy` 微基准。
- 不改 P3 状态（未签署）、7-A、profile 或普通镜像事件。

## 阶段/profile 影响

P4-2 的测量输入完成；不关闭 P4、不勾选 7-A、不宣称性能达标。

## 后继关系

- 下一步 P4-2 单项收窄：可按 `wait-output` 与 `switch-service` 的占比选择候选，
  先做不改变故障原子性的实现，再同负载两轮对照。
- 计划：[P4 切片计划](../plans/2026-11-26-p4-path-narrowing.md)。
