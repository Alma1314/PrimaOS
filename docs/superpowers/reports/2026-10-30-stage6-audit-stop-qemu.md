---
id: report-stage6-audit-stop-qemu
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-30
---

# S6-M1 审计停止控制器：裸机耗尽与停止边界验收

## 结论与范围

基线 `38ca35f`（工作区干净）上，为[审计停止控制器](2026-10-30-stage6-audit-fail-stop.md)
补充显式启用的 QEMU 故障注入。验收针对既有原生启动中的七个真实执行位置：int80 入口、
共同分派、int80 返回、fast syscall 返回、pick_next、resume、IRQ0。
这补强[协议 §5/§9](../specs/2026-10-29-stage6-m1-protocol.md)的裸机停止证据；
没有创建 Gate/Request 能力或拦截客户端调用，不能计作 G1–G3 或 S6-D2 完成。

## 实现与可复现入口

`kernel`/`xtask` 新增非默认特性 `gate-audit-probe`。探针与串口观察者仅在该特性下编译，
没有新 syscall、用户可触发的注入参数、普通镜像中的压力注入或日志容量扩大。
`PRIMA_GATE_AUDIT_SITE` 是编译期选择；runner 要求运行时环境与编译值一致，且拒绝
用探针构建运行普通 test/run/lint 命令，防止将专用镜像计入常规成功验收。

```sh
for site in int80-entry dispatch int80-return fast-return pick-next resume irq; do
    PRIMA_GATE_AUDIT_SITE="$site" CARGO_NET_OFFLINE=true \
        cargo run -p xtask --features gate-audit-probe -- test-gate-audit || exit "$?"
done
```

每种配置从头启动两次，使用仓库既有 BIOS / `pc-i440fx-8.2` / TCG single / qemu64 /
1 vCPU / 128 MiB 与 30 秒限制，不修改 timeout。工具链为 `nightly-2026-07-30`。
boot image、依赖快照与中间 ELF 不入库。

进入指定位置时，探针选择当前 Running Task，通过真实 reserve/block 接口建立不可警报
私有等待。随后用普通 AuditLog 的实际 reservation 账目保留尾部容量，只剩一条可追加事件；
完成需要两条，真实 `complete_completion_wait` 必须在修改前返回 AuditFull，并封存
operation=Complete、完整 owner cookie、required=2、used/reserved 的首次诊断。
这是**预留压力导致的可用容量耗尽**，不是把普通日志实际写满，也不是伪造错误返回。

探针在真实表中比较目标 TaskRecord、该 WaitObject、审计 count 与调度器保存帧；检查
私有等待双向登记。解除本次压力后再次完成、释放、schedule/yield 仍须失败，首次诊断不变。
探针在 State 锁外发出 `armed`，随后由该位置已有的停止检查走 terminal panic；panic handler
再次比较上述记录，只有保持时才发出相同的 `stopped`。保存帧检查指 sched 的保存槽，
不等同于已实现 Gate trap 返回帧事务，也不声称验证了不存在的请求/能力表。

## 验收器与反向验证

专用 runner 要求 QEMU exit=35（既有 panic 退出值），恰好一对相同的 armed/stopped，
正确 site、完整 cookie、required=2、used+reserved=16383、unchanged=1，随后必须是
`gate_audit.rs` 的 panic 源位置且无后续进展。单纯 panic、错误位置、缺字段、状态变化、
容量不符、重复记录、escaped 或之后继续输出都失败。普通套件仍要求 exit=33。
两组 host 校验器测试覆盖七种正例及上述损坏/漏报输入。

另做实际源码变异：临时将 `enforce_diagnostic(diagnostic)` 替换为空操作，运行 resume 配置。
探针在停止检查返回后发出 `escaped`，终止于 probe 的断言；QEMU 即使同样 exit=35，
runner 仍返回 1。随后恢复原调用并重跑最终七种配置。该变异未提交，
[变异命令输出](artifacts/stage6-audit-qemu/mutation.txt)与
[串口](artifacts/stage6-audit-qemu/mutation-serial.txt)保留为预期失败证据。

## 命令与结果

七种专用构建均双轮 PASS（共 14 轮），每轮实际 exit=35。两轮的诊断计数一致：

| site | used / reserved | 第 1 / 2 轮串口 |
|---|---|---|
| [int80-entry](artifacts/stage6-audit-qemu/int80-entry-command.txt) | 61 / 16322 | [round1](artifacts/stage6-audit-qemu/int80-entry-round1.txt) / [round2](artifacts/stage6-audit-qemu/int80-entry-round2.txt) |
| [dispatch](artifacts/stage6-audit-qemu/dispatch-command.txt) | 61 / 16322 | [round1](artifacts/stage6-audit-qemu/dispatch-round1.txt) / [round2](artifacts/stage6-audit-qemu/dispatch-round2.txt) |
| [int80-return](artifacts/stage6-audit-qemu/int80-return-command.txt) | 61 / 16322 | [round1](artifacts/stage6-audit-qemu/int80-return-round1.txt) / [round2](artifacts/stage6-audit-qemu/int80-return-round2.txt) |
| [fast-return](artifacts/stage6-audit-qemu/fast-return-command.txt) | 64 / 16319 | [round1](artifacts/stage6-audit-qemu/fast-return-round1.txt) / [round2](artifacts/stage6-audit-qemu/fast-return-round2.txt) |
| [pick-next](artifacts/stage6-audit-qemu/pick-next-command.txt) | 64 / 16319 | [round1](artifacts/stage6-audit-qemu/pick-next-round1.txt) / [round2](artifacts/stage6-audit-qemu/pick-next-round2.txt) |
| [resume](artifacts/stage6-audit-qemu/resume-command.txt) | 61 / 16322 | [round1](artifacts/stage6-audit-qemu/resume-round1.txt) / [round2](artifacts/stage6-audit-qemu/resume-round2.txt) |
| [irq](artifacts/stage6-audit-qemu/irq-command.txt) | 1153 / 15230 | [round1](artifacts/stage6-audit-qemu/irq-round1.txt) / [round2](artifacts/stage6-audit-qemu/irq-round2.txt) |

所有 owner 均为完整 cookie `4294967297`，required=2、unchanged=1；命令日志与各串口
同目录保存，归档只统一 CRLF 为 LF 并去掉末尾额外空行。日志 used 为注入前已有事件数，
不是普通日志填满后的条数。原生 syscall/IRQ 到达指定位置；resume 是首次启动恢复。

| 命令 | 本次结果与范围 |
|---|---|
| 上述七种 `test-gate-audit` 配置 | PASS：14/14；退出值及严格记录校验均通过 |
| 停止调用变异 / resume | 预期失败：runner exit=1；有 escaped、无有效 stopped，未计作成功轮次 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1050 passed、0 failed、1 个既有外部 ELF 样本 ignored；[日志](artifacts/stage6-audit-qemu/workspace.txt) |
| `CARGO_NET_OFFLINE=true cargo clippy -p kernel --features gate-audit-probe --target x86_64-unknown-none -- -D warnings` | PASS：探针 lib/bin 裸机检查；[日志](artifacts/stage6-audit-qemu/feature-lint.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy、裸机编译；kernel/src 35 文件零 unsafe，13 KiB 守卫保留；[日志](artifacts/stage6-audit-qemu/lint.txt) |
| 默认 runner 误用 `test-gate-audit` | 预期拒绝：exit=1，要求显式特性，不构建镜像；[输出](artifacts/stage6-audit-qemu/default-rejected.txt) |
| 默认 standalone ELF 字节检查 | PASS：不含探针标记或逃逸断言；[hash 与范围](artifacts/stage6-audit-qemu/default-probe-absence.txt) |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：最终单次命令 exit=0，36 程序/72 轮及全部 Linux 主机参考；[日志](artifacts/stage6-audit-qemu/full-suite.txt) |
| 文档四项 / Git 空白检查 | PASS：self-test、build、check、check --base 38ca35f；335 文档；[日志](artifacts/stage6-audit-qemu/docs.txt) |

完整默认回归在获准的沙箱外环境运行，因为既有 Linux N1/R3 参考需要主机套接字。
没有修改断言、预期、超时或跳过用例；14 轮专用故障注入与默认 72 轮分别归账。
未重跑 D14 opt-in 域原型、Lean 或性能/TCB 测量。

## 限制与后继关系

本报告只补强前序报告“无 QEMU 耗尽注入”的限制，不改写其 host 证据或历史失败。
resume 配置在首次进入用户态前停止；其它配置来自原生 ring3/IRQ 执行路径，未运行
门控 ring3 服务或 Linux LibOS。未测试服务丢失时的 Gate 批量事务、copy/reply 授权、
SMP、KVM、真实硬件停止、日志回收或恢复。仍须实际 Gate 接线及 G1–G3。
当前阶段与阻塞见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)，不勾选交付或退出条件。
