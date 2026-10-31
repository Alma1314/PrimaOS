---
id: report-stage6-gate-admission
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# S6-M1 Ready 目标挂接准入整改

## 结论与范围

基于 `46715bb`，补齐[接线报告发现](2026-10-31-stage6-gate-dispatch.md#阶段影响与待办)的
Ready 目标未决返回准入缺口。`gate_attach` 在新建或复用 Gate、铸能力及预留私有等待之前，
拒绝带原生返回／恢复续体的目标，返回 `Busy`，原生入口映为 EAGAIN。
检查只读，保留旧结果给原生路径消费；排队的信号可以保留，旧 wait4 输出地址本身不构成续体。

这是[协议 §1](../specs/2026-10-29-stage6-m1-protocol.md#1-拓扑授权与准入)的有界整改，
不是完整 G2、G3 或可信停车装载入口。75/76 代理复制、普通控制失败尝试审计、Linux LibOS、
未修改白名单与其余阶段交付仍未完成；阶段 6 退出保持 0/4，不发布稳定 ABI。
源码身份见[SHA-256](#关键输出摘录)。

## 基线、契约与后继关系

输入为[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)、
[M1 协议 §1–2](../specs/2026-10-29-stage6-m1-protocol.md)、
[D17–D22](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)与
[COSIR §5.2、§8.4、W9/W17](../../../COSIR%20原语规范.md)。Ready 并不意味着
`pending_return`／`pending_wait_result` 为空；细化层还在任务表以外保存返回状态。
本报告只补齐前序接线报告明确保留的这项偏差，不改写其 71/72、栈回归、限制及失败记录。

宿主 x86_64，rustc `1.99.0-nightly (1a833e165 2026-07-29)`；QEMU 8.2.2，
`pc-i440fx-8.2`、`tcg,thread=single`、`qemu64`、1 CPU、128 MiB，维持 30 秒门限，
未扩栈、改验收输入或增加依赖。
普通镜像不含 `gate-probe` 探针。没有改 syscall 号、权利位或控制错误编码。

## 实现与反例

`kernel/src/gate.rs::gate_attach` 共用准入检查，覆盖新 Gate 和已有 Gate 两条路径：

| 状态所在层 | 拒绝条件 | 对应原生返回路径 |
|---|---|---|
| TaskRecord | 非 Ready、非本地节点、pending IPC、wait_request、三种 pending_wait_result、pending_exc、waiting_process | IPC 直投、公开等待、异常或进程等待 |
| 调度器 | kernel_resume_frame、接收输出指针、pending wait4 status（含空输出指针） | 异常恢复、接收写回、wait4 写回 |
| syscall | BLOCKED_CALL 或 UPCALL_INTERRUPTED | 原生等待结束／信号重启处理 |
| signal | 活动恢复栈或 suspended_mask | handler 返回、sigsuspend 原始掩码恢复 |

当前 TaskKind 只有 Cpu；硬件 root 独占与 Process 单线程的既有检查继续适用。保存的普通
用户现场、已完成 wait4 留下的地址、信号 handler 配置及尚未投递的 pending 信号不属于未决返回。
只读查询放在各自状态的所有者模块，不复制或消费帧，也不清理旧续体来强行准入。

新增 3 个 host 测试：一个在新建／已有 Gate 下遍历 12 种状态（24 个拒绝组合），比较任务、
能力槽／位置、审计计数、等待对象和 Gate 投影不变，并检查清除续体后成功、pending 信号保留；
另两个核对原生重启／中断标记和信号恢复栈／suspend 掩码的只读识别。矩阵部分为防御性注入，
包括在有效状态机中通常不与 Ready 共存的登记，不能当作所有可达状态的证明。

扩展 G1：可信探针先给 Ready Task 2 注入一个待消费的 kernel_resume_frame；ring3 服务者
经原生 73 挂接，必须得到 EAGAIN。int3 检查点比较目标现场／恢复帧不变、仍 Ready、
能力／等待／审计计数不变且无 Gate/Binding。检查点随后清除所注入的续体，在已知程序地址恢复
服务者，再执行正常挂接及原有全部 G1 检查。此处是探针注入和可信续执行，不声称实跑异常后
完整恢复或通用停车协议。最终唯一记录增加 `admission=1`；harness 拒绝缺失或失败字段。

## 命令与结果

| 命令 | 本次结果与证据范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1089 passed、0 failed、1 既有外部 ELF ignored |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机 deny unsafe 编译、kernel/src 38 文件零 unsafe |
| `CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings` | PASS：含专用探针 |
| `CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-probe -- test-gate` | PASS：两轮，含 ring3 准入拒绝及原有 G1 |
| `CARGO_NET_OFFLINE=true cargo xtask test`（经授权沙箱外） | **退出 1，未整套通过**：72 轮中 70 轮 PASS，W7 两轮在 30 秒超时；其余 QEMU（含 W7b、R3）及所有 Linux 参考通过 |

该轮文档 self-test/build/check/check --base 46715bb 与 diff 检查均通过，
关键结果内嵌于下文。

## 初轮失败与整改

首轮 `cargo test --workspace` 退出 101：kernel 532 passed、2 failed、1 ignored，
失败为两个调度器 deferred-frame 测试；[原始输出摘录](#关键输出摘录)。
它们并发操作同一全局队列，均在用例开头 flush，但整个测试过程没有互斥；一个测试在期待
2 帧时读到 3，另一个满队列断言失效。新增准入测试不调用 defer/flush。
在这两个既有测试之间增加测试用 Mutex，保留原有容量／延迟释放断言；生产队列未改。
随后完整工作区命令通过，初轮失败不改记为 PASS。不是调度器生产路径或全局并发正确性的证明。

## W7 完整套件时限余项

本轮完整命令经授权在沙箱外运行，以便执行 Linux UDP 参考，维持原有 QEMU 配置与 30 秒门限。
W7 两轮均超时，串口均只到 `EXEC|exec=1;wait=1;bad=1;stream=1`；
缺少 `large=1|ok=1;err=0`，不能据前缀认定 526 页大映像及再次 exec 成功。
源码 `prima-user/c/stage5_exec.c` 在 stream 字段之后启动 large 子项，因此记录只把未完成范围
缩至后续大映像演练，不能区分慢执行、阻塞或其它原因。没有将时间门限提高或用独立重试覆盖失败。
此切片没有修复 W7 根因；前序接线报告的聚焦通过及整套 71/72 仍按原范围保留。

## 阶段影响与限制

关闭当前已识别的 Ready 未决返回挂接偏差，更新阶段跟踪、协议检查点和机器入口错误范围。
普通失败仍未逐次审计；本轮只检查拒绝保持，不能替代协议 §5。代理复制双端 root／Task／映射
身份、COW 与 G3、完整 G2 清理矩阵、可信停车入口仍是后继工作。以后新增原生阻塞／恢复状态、
开放共享 root、多核或可警报 Gate 时，须扩充准入查询与反例，不能沿用本次清单宣称穷尽。

本报告保留的四个原生控制入口失败尝试审计，随后由[独立后继](2026-10-31-stage6-gate-control-rejections.md)补齐；本报告原始计数和完整套件失败范围不改写。

## 关键输出摘录

以下片段来自本报告原验证记录，仅内嵌支撑结论的输出；本次文档整理未重跑这些验证。
完整旧附件的恢复版本及目录见[日志整理记录](2026-10-31-documentation-log-inline.md)。

原 `workspace-initial.txt` 的关键行：

```text
thread 'sched::tests::deferred_frames_stay_pending_until_flush' (1171) panicked at kernel/src/sched.rs:681:9:
assertion `left == right` failed: 入队后仍未归还
  left: 3
 right: 2
thread 'sched::tests::deferred_queue_rejects_overflow' (1172) panicked at kernel/src/sched.rs:696:9:
满队列必须确定性失败
test result: FAILED. 532 passed; 2 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.54s
```

两轮相同的最终串口判据（共同记录只列一次）：

```text
S6GATE|int80=1;fast=1;registers=1;signals=1;retry=1;output_failures=3;duplicate=1;orphan=1;admission=1;audit=1
```

原 `G1.txt` 的关键行：

```text
PASS S6GATE round=1 int80=1 fast=1 registers=1 signals=1 retry=1 output_failures=3 duplicate=1 orphan=1 admission=1 audit=1
PASS S6GATE round=2 int80=1 fast=1 registers=1 signals=1 retry=1 output_failures=3 duplicate=1 orphan=1 admission=1 audit=1
```

原 `full-suite-errors.txt` 的关键行：

```text
W7: BIOS W7 run 1 failed: QEMU timed out after 30s; QEMU child terminated
EXEC|exec=1;wait=1;bad=1;stream=1
BIOS W7 run 2 failed: QEMU timed out after 30s; QEMU child terminated
```

原 `full-suite.txt` 的关键行：

```text
ERROR full BIOS suite: 1 failed groups/checks; details in target/prima-os/full-suite-errors.log
```

该轮最终源码 SHA-256（历史身份，不表示当前文件仍有相同哈希）：

```text
6b8e3286f98704195bdf0f0f1d1459ce92e591ace70cba8ddfbaa29354205ee3  kernel/src/gate.rs
215a4c6d0117abeaee5356afa0bfef4f93aa24a38f1531ba4b90e891bad8bd07  kernel/src/sched.rs
050c075995d810a4002306d2a3c8d3815716866be61a9b430ef34770d42a9086  kernel/src/syscall.rs
0dc595a18dc257191b188f0952ba9b10108f7a61d32f40b9f2ba5a6bfacb4d57  kernel/src/signal.rs
b3fc9cd61541469f4bc86cf02eb63366114d5b9fdfd20bf84c4e36d869fc5d77  kernel/src/gate_probe.rs
44258a7dcaf7691410826e8045572b02d0876e6b10dcf8d814ba01e343c1bceb  kernel/src/main.rs
f8665d62974efeef17dccfabf45d764439b3be6420ca8ebf2634e8f419ceffa3  xtask/src/qemu.rs
```

该轮文档／状态检查关键行：

```text
PASS docs: 351 documents; catalog, IDs, topics, local links and Markdown anchors
```
