---
id: report-stage6-gate-control-rejections
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# S6-M1 原生 Gate 控制失败尝试审计

## 结论与范围

基于 `d0608de`，补齐[协议 §5](../specs/2026-10-29-stage6-m1-protocol.md#5-审计耗尽与失败关闭)
在[接线报告](2026-10-31-stage6-gate-dispatch.md)和[Ready 准入后继](2026-10-31-stage6-gate-admission.md)
明确保留的普通控制失败尝试审计。原生 73/74/77/78 共用入口在返回错误前追加
`GateControlRejected`，记录尝试者 Task id、含代次 cookie、原始控制号、六个原始参数及返回错误编码。
参数中的本地句柄按收到的值保留；非法或失效句柄不被断言为某个有效 Gate/Request 身份。

这是上述四个原生控制入口的有界实现证据。对象层拒绝仍只读；原生失败只追加审计，
不修改业务表、客户端保存帧、等待或输出。75/76 尚未登记或实现，完整 G2/G3、Linux LibOS
与可信停车装载入口仍未完成。阶段 6 退出保持 0/4，没有发布稳定 ABI。

## 基线与实现

输入为[阶段6–7细化](../plans/2026-10-27-stage6-7-refinement.md)、
[M1 协议 §1–5](../specs/2026-10-29-stage6-m1-protocol.md)、
[D17–D22](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)、
[审计停止控制器](2026-10-30-stage6-audit-fail-stop.md)及其
[七边界裸机证据](2026-10-30-stage6-audit-stop-qemu.md)。保留前序失败和限制，不扩大其证据范围。
宿主 x86_64，rustc `1.99.0-nightly (1a833e165 2026-07-29)`，QEMU 8.2.2；
QEMU 沿用单核 `qemu64`、128 MiB、TCG 和 30 秒门限，普通镜像不包含 `gate-probe`。
源码身份记录于[SHA-256](artifacts/stage6-gate-control-rejections/source-sha256.txt)。

`syscall::gate_control_call` 在参数及句柄解码前预检一条失败事件的容量，且在继续验证之前
执行全局停止检查；成功事务另按已有路径预检完整审计成本。普通错误在用户返回前记录一次，
内部 AuditFull 先进入停止器，不写成用户错误事件。无额外持久预留；服务空队列阻塞后重试时
重新检查容量，不把预留跨调度保留。单核内核不可抢占区间内完成验证和提交。

审计池满时封存 `Operation::Control` 首次诊断，拒绝继续控制；释放普通预留也不能恢复。
未引入日志回收、扩大日志容量或改变审计耗尽策略。Gate helper 保持不内联，避免将其临时
审计对象加入普通 exec 分派的深栈路径。COSIR 抽象操作的失败语义不变；这是 Gate 原生适配层
的尝试事件，不能推广为所有既有原生 syscall 均审计失败。

## 反例与验证范围

新增 host 用例在一个真实已排队请求存在时，仅留下一个可追加事件：预检不占用事件或
持久预留，最后一条拒绝事件逐字段相等，下一次预检封存 Control 诊断；释放压力后仍拒绝追加。
比较 Task、cap slots/位置、Gate/Request 投影、WaitObject 和客户端保存帧保持，检查 Gate/等待不变量。
该用例调用可信适配器辅助函数，不模拟硬件入口；使用 Task16 和显式非零保存帧，
避开另一个调度器测试对 Task3 的写入，不宣称所有共享静态 host 测试均已隔离。

G1 专用镜像新增非法句柄、错误权限及快速 syscall 重复 abort，连同已有反例逐字段核对
恰好九条拒绝事件：保留参数 EINVAL、零句柄 EINVAL、错误权利 EACCES、未决恢复帧 EAGAIN、
重复 reply ENOENT、三个输出页 EFAULT、重复 abort ENOENT。前四条在准入检查点验证无 Gate、
无绑定、能力/等待计数不变；其余与最终请求/审计链核对。合法重试、原有全部寄存器、信号保留、
孤儿返回及一次性终结判据保留。唯一成功记录增加 `rejected=9`；harness 拒绝缺失或错误计数。

这是普通错误实际经过 int80/syscall 的证据；容量极限是 host 注入，未新增“原生控制入口
日志恰满”的独立 QEMU 停止演练，七边界旧证据不冒充本轮该入口实跑。

## 命令与结果

| 命令 | 本次结果与工件 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test -p kernel rejected_controls_record_last_available_event_then_fail_stop_without_mutation -- --exact bootstrap::gate::tests::rejected_controls_record_last_available_event_then_fail_stop_without_mutation` | PASS：新增容量极限用例；[聚焦日志](artifacts/stage6-gate-control-rejections/focused.txt) |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1090 passed、0 failed、1 既有外部 ELF ignored；[日志](artifacts/stage6-gate-control-rejections/workspace.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机 deny unsafe 编译、kernel/src 38 文件零 unsafe；[日志](artifacts/stage6-gate-control-rejections/lint.txt) |
| `CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings` | PASS：探针特性；[日志](artifacts/stage6-gate-control-rejections/probe-lint.txt) |
| `CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-probe -- test-gate` | PASS：G1 两轮，含九条拒绝事件；[harness](artifacts/stage6-gate-control-rejections/G1.txt)、[串口1](artifacts/stage6-gate-control-rejections/G1-1.log)、[串口2](artifacts/stage6-gate-control-rejections/G1-2.log) |

`CARGO_NET_OFFLINE=true cargo xtask test`（经授权沙箱外）**退出1，未整套通过**：
72轮中68轮PASS，默认启动第1轮、V1第1轮和W7双轮均30秒超时，其余轮次及所有Linux参考（含R3）通过。
[完整命令日志](artifacts/stage6-gate-control-rejections/full-suite.txt)；失败细节见下节。

整套运行期间只加强新增 `cfg(test)` 用例的显式保存帧断言和 Task id 隔离，生产与探针路径不变；
表中的 workspace、聚焦用例、lint、探针特性Clippy和G1均在最终用例上重跑。

文档治理及差异检查记录于[校验日志](artifacts/stage6-gate-control-rejections/docs.txt)。
归档日志只统一 LF、去行尾空白和末尾空行，不删除记录或改字段。

## 失败、限制与后继

完整回归在原 30 秒门限下保留以下超时，根因未定位；未放宽门限或重复整套追求通过：

| 失败轮次 | 串口范围与限制 |
|---|---|
| default 第1轮 | 最后记录为 `cow_ready`（seq=11）；第2轮通过。不能由另一轮通过解释首轮超时。 |
| V1 第1轮 | 最后协议记录为 `authority_ready`（seq=36），无完整验收收尾；第2轮通过。 |
| W7 两轮 | 均只有 `EXEC\|exec=1;wait=1;bad=1;stream=1` 前缀，缺少 `large=1\|ok=1;err=0`；与准入轮相同的未完成范围，不证明526页装载或再次exec成功，也不能区分慢执行、阻塞或其它原因。 |

[完整失败串口](artifacts/stage6-gate-control-rejections/full-suite-errors.txt)保留实际输出。
前序准入轮70/72、接线轮71/72和旧聚焦W7通过仍按各自报告解释，不能覆盖本轮失败。

只关闭四个已接线控制入口的普通失败尝试审计缺口；未来接入 75/76 或其它 Gate 控制必须
继续使用公共入口并补充方向/地址/长度及双端身份的复制审计，不能从本轮控制事件推出 G3。
新错误路径若在返回前改变映射或业务状态，须重新评审事务原子性和审计预检。
失败事件中的原始句柄只是尝试证据，不给予权限，也不保证失败时能解析到活动对象。
