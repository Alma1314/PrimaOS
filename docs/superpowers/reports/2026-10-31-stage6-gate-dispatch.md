---
id: report-stage6-gate-dispatch
kind: report
authority: scoped-implementation-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-10-31
---

# S6-M1/M1b trap 接线与 G1 裸机见证

## 结论与范围

在 `a6c4944` 干净基线上接入 Gate 的 syscall/int80 截获及原生 73/74/77/78
（attach/wait/reply/abort）。服务者在 ring3 经真实控制入口挂接另一个 ring3 Task，
截获两个不同来源的调用、原样获得 number/六参数/cookie/flags，reply/abort 修改客户端 rax。
Gate v1 的信号保留、私有等待、空队列唤醒重试、重复答复和孤儿绑定不回退原生均有有界探针。

最终源码身份见[源码SHA-256清单](#关键输出摘录)，不提交启动镜像。
关键输出及源码身份内嵌于文末；CPU故障只列用于定位的字段。

这是 G1 和部分失败路径的实现证据，不是 S6-D2 完成或阶段签署。75/76 的代理复制、完整
G2（服务死亡、监督终止、各阶段销毁/信号）、G3（跨任务双端/COW）、可信停车入口、Linux
LibOS 和未修改白名单仍未完成；阶段退出条件保持 0/4。

## 契约与后继关系

输入为[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)、
[递送提案](../specs/2026-10-29-stage6-m1-delivery-design.md)、
[D17–D22](../specs/2026-10-29-stage6-m1-mechanism-decisions.md)、
[R1–R6 复审](2026-10-29-stage6-m1-decision-review.md)、
[M1a 收口](2026-10-29-stage6-m1-blocker-closure.md)与
[现行协议](../specs/2026-10-29-stage6-m1-protocol.md)。
按 COSIR W9/W17/§21，继续使用真实私有完成等待；没有新增 Linux 对象或解释不透明调用号。

本报告补强[对象层生命周期](2026-10-31-stage6-gate-lifecycle.md)与
[最后服务授权丢失](2026-10-31-stage6-gate-authority-loss.md)未覆盖的机器入口、输出事务和
G1。它不抹去旧报告的全套超时；[内存事务](2026-10-30-stage6-user-copy-transactions.md)仍只
是代理复制前置；[七边界审计停止](2026-10-30-stage6-audit-stop-qemu.md)的故障注入范围不扩大。

## 实现边界

- `syscall::serve` 在任何原生号判断前检查绑定；保存真正的硬件返回帧，提交 gate_trap 后
  BlockedWait 切走。int80 flags=1，syscall flags=0；客户端没有原生控制调用逃逸口。
- 控制调用检查所有保留参数；attach 同时核对 Process 单线程和实际硬件 root 无其它 Task
  共享。句柄按服务者的本地视图解码/返回，能力铸造前也检查本地槽位（含代次耗尽）。
- wait 共用只读授权/队列/能力/审计预检，先于输出 COW；再预检整个 96 字节的页权限与
  COW 帧，最后出队、安装能力、晋级容量等待、写输出。单核内核不可抢占期间无调度或
  VM 修改；物理输出计划绝不跨阻塞保存。空队列时只把服务者原生 wait 返回帧后退两字节，
  以原参数重新陷入并验证映射；不对客户端不透明号做指令重放。
- reply/abort 继续调用既有对象层一次性提交。服务者返回 0，客户端拿到不透明 u64；
  除 rax 外的保存寄存器、栈、RFLAGS 和返回 RIP 保持硬件陷入语义。
- 原生信号恢复入口对所有 Gate 绑定（含孤儿）直接保留 pending，避免原生信号处理器、
  默认终止或 syscall restart 修改门控帧。此项不承诺 Linux 信号可用。
- 所有门控控制与 trap 的 AuditFull 在用户返回/调度前进入现有全局停止器；wait 在任何
  页表/COW 变更前就检查审计预算。没有把内部 AuditFull 当客户端返回值。

新入口台账见[原生 Gate ABI](../compat/stage6-gate-native-abi.md)；仍为实验性接口。
普通镜像不含 `gate-probe` 特性。专用镜像重用两个平台 Task 的独立 code/data/root，
不改变生产 syscall helper、不跳过授权；仅探针启动时给服务者铸造合法TaskManageVSpace并记录CapabilityMinted；
最终复核补齐了这条探针准备事件，不改普通镜像，随后重跑特性Clippy与G1。

## 探针与反例

`cargo run -p xtask --features gate-probe -- test-gate` 独立运行两轮、保持 30 秒门限。
服务者先以非零保留参数 attach，必须 EINVAL，再成功 attach；空队列 wait 阻塞，客户端
的首个 int80 唤醒服务者。客户端使用原生 TASK_ID 号，随后使用 GATE_ATTACH 号走 syscall，
证明两类号都先截获。两次返回分别为 `0x123456789abcdef0`、`0xfedcba9876543210`。

客户端把全部 16 个 GPR（含 RSP）和 RFLAGS 写入自身页；探针逐字核对，syscall 的 RCX/R11
按 CPU 返回地址/flags 语义核对。顺序执行到 int3 证明返回 RIP 为陷入后指令。信号 10 在
首次执行前已排队，整个序列后仍 pending。重复 reply 得 ENOENT。第二请求排队时 wait 的
未映射地址、跨未映射页、只读代码页均 EFAULT，随后合法 wait 仍能交付同一请求。
服务者丢弃最后 GateServe 后，客户端再用 TASK_ID，结果必须是配置值
`0xaabbccddeeff1122`，不会原生返回 Task id。

终止回调同时核对真实 Gate/Wait 表投影和审计 2 次 trap、2 次交付、3 次终结、1 次服务者
BlockedWait；harness 要求唯一完整 S6GATE 记录与成功退出，拒绝缺失/重复/失败字段及 panic。
这不是所有可达状态证明，也没有运行用户 Linux 程序。

## 命令与结果

宿主 x86_64；pinned rustc `1.99.0-nightly (1a833e165 2026-07-29)`；QEMU 8.2.2，
`pc-i440fx-8.2`、`tcg,thread=single`、`qemu64`、1 CPU、128 MiB。无 KVM 或延迟性能主张。

| 命令 | 结果与范围 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1086 passed、0 failed、1 既有外部 ELF ignored；Gate 共18组（本轮新增预检保持测试），另增本地句柄容量和harness拒绝测试 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机 deny unsafe 编译、38个kernel文件零unsafe |
| `CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings` | PASS：含专用探针 |
| `CARGO_NET_OFFLINE=true cargo xtask test-process W7b` / `W7` | PASS：整改后各两轮，维持30秒门限 |
| `CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-probe -- test-gate` | PASS：G1两轮 |
| `CARGO_NET_OFFLINE=true cargo xtask test`（栈整改后、经授权沙箱外） | **退出1，未整套通过**：72轮中71轮QEMU通过，W7 run1在30秒超时、run2通过；全部Linux参考及R3参考通过 |

该轮文档`self-test`、`build`、`check`、`check --base a6c4944`及`git diff --check`均通过；
文档结果的关键行也内嵌于下文。

最终整套没有再出现W7b的guard故障；W7时限问题仍未闭合。独立W7两轮PASS只属于聚焦范围，
不能覆盖整套中的超时，也没有提高门限或重试整套直到通过。前序[门控对象报告](2026-10-31-stage6-gate-lifecycle.md)
已保留30秒临界时序观察，但不能由它直接归因本轮全部超时。下面保留整改前失败链。

## 开发失败与环境

[开发失败记录](#关键输出摘录)保留：探针最初误用
AuditLog::iter 与 TaskStateChanged::to，改用实际 entry/status；首次构建缺本地锁定 musl
快照，从主工作区复制同一源码快照到 ignored target 后由既有脚本重新校验并构建；
Clippy 拒绝 chunks_exact_mut 常量分块及探针的下标循环，按建议改为数组块与 enumerate。
没有放宽门限或证据判据，没有提交依赖快照或启动镜像。

完整套件的 Linux N1 参考探针在沙箱内返回 `err=1`（EPERM），所有功能位为0；经用户授权，
独立执行同一个已构建的 `target/prima-os/net-linux` 于沙箱外，退出0且11个功能位全部为1，
输出与既有 `NET_EXPECTED_STDOUT` 一致（[原始输出](#关键输出摘录)）。
Rust R3的Linux参考也在`UdpSocket::bind`返回EPERM，其QEMU两轮因此被跳过。
这次独立UDP执行不接触Prima镜像、不计作QEMU轮次；原始完整套件失败仍保留，不拼接成命令级PASS。
最终完整回归经授权改在沙箱外执行，仍使用同一命令、门限、参考程序和QEMU判据。

## 原生分派栈回归与整改

首版完整套件实跑66个QEMU成功轮次，W7两轮30秒超时（已输出exec成功记录但未结束），
W7b两轮退出0；另有Linux N1与Rust R3参考测试因沙箱UDP权限失败，R3没有进入QEMU。
原始命令退出1、共4个失败组/检查，不能记作整套通过：
初轮汇总及关键故障见[输出摘录](#关键输出摘录)。

随后不重编译、用保留的首版xtask/内核产物执行
`PRIMA_QEMU_DEBUG_LOG=/tmp/prima-stage6-gate-dispatch-w7b-before-cpu.txt target/debug/xtask test-process W7b`，
再次退出0（[复现](#关键输出摘录)）。
[CPU输出摘录尾部](#关键输出摘录)显示supervisor写故障：
CR2=`0x10000189ed8`，落在SYSCALL_STACK guard
`[0x10000189000,0x1000018a000)`内，随后double/triple fault。
相对RIP=`0x73fb7`定位于`BootstrapState::authorize_ioport`；这是本轮引入的原生路径栈回归，
不能归为宿主争用或提高超时来消除。

Gate客户端帧快照曾放在公用serve局部变量中，即使执行原生号也支付该栈帧；
将它移入独立的`#[inline(never)] gate_client_call`后，公用serve栈帧从744降到296字节
（[反汇编与地址映照](#关键输出摘录)）。没有扩32KiB栈、
取消guard、缩减exec输入或放宽门限。无绑定的入口另按binding_count快速返回，避免每次原生
trap及信号恢复都扫描64槽；已有测试扩展核对无绑定/活动/孤儿/销毁后的谓词，尤其保留孤儿
绑定fail-closed，不以gate.live代替绑定存续。上述不等于所有路径的峰值栈证明或延迟保证。

## 阶段影响与待办

明确后继：[Ready 准入整改](2026-10-31-stage6-gate-admission.md)补齐下文记录的未决返回／恢复续体拒绝，
并扩展 G1 取得一个 ring3 注入反例；不改写本报告当时的失败、证据范围与其它待办。

关闭本切片的“trap 未接、四个控制号未分配、G1 尚无裸机证据”阻塞；S6-D2 与阶段退出
仍不勾选；W7的30秒时限回归尚未闭合。下一步为 M1c 双端显式 root/Task/映射身份代理复制与 G3，以及 G2 完整清理矩阵。
准入核对另发现既有对象层attach仅凭Ready与独占根条件，尚未完整拒绝Ready但仍有待领取IPC/等待结果或恢复续体的目标；
这是协议§1“无其它未决内核等待或返回”的未闭合项，后继真实装载/可信停车接入前必须补齐。
G1使用尚未执行、没有这些续体的两个平台Task，不覆盖该反例，也不把现有准入自动提升为规范。
控制调用普通失败目前仅返回机制errno，尚无独立的失败尝试审计事件（协议§5的未闭合项）；
本轮仅对成功请求链和审计耗尽预检取证。fork后服务者的本地视图有host容量反例，尚无对应
ring3 fork服务探针。开放共享 root、多核提交、可警报门控等待、整帧答复或改变重试策略时必须重审协议与证据。

## 关键输出摘录

以下片段来自本报告原验证记录，仅内嵌支撑结论的输出；本次文档整理未重跑这些验证。
完整旧附件的恢复版本及目录见[日志整理记录](2026-10-31-documentation-log-inline.md)。

两轮相同的最终串口判据（共同记录只列一次）：

```text
S6GATE|int80=1;fast=1;registers=1;signals=1;retry=1;output_failures=3;duplicate=1;orphan=1;audit=1
```

原 `G1.txt` 的关键行：

```text
PASS S6GATE round=1 int80=1 fast=1 registers=1 signals=1 retry=1 output_failures=3 duplicate=1 orphan=1 audit=1
PASS S6GATE round=2 int80=1 fast=1 registers=1 signals=1 retry=1 output_failures=3 duplicate=1 orphan=1 audit=1
```

原 `development-failures.txt` 的关键行：

```text
error[E0599]: no method named `iter` found for struct `AuditLog` in the current scope
error[E0026]: variant `bootstrap::AuditEvent::TaskStateChanged` does not have a field named `to`
error: could not compile `kernel` (lib) due to 2 previous errors
ERROR musl: musl snapshot missing: /home/alma/.codex/worktrees/05b3/PrimaOS/target/musl-1.2.5; restore from musl-snapshot.lock.toml
error: using `chunks_exact_mut` with a constant chunk size
error: could not compile `kernel` (lib) due to 1 previous error
error: could not compile `kernel` (lib test) due to 1 previous error
error: the loop variable `reg` is used to index `regs`
```

原 `W7b-before.txt` 的关键行：

```text
ERROR BIOS W7b run 1 failed: unexpected QEMU exit code: expected 33, got 0; panic exit code is 35
BIOS W7b run 2 failed: unexpected QEMU exit code: expected 33, got 0; panic exit code is 35
```

原 `W7b-guard-tail.txt` 的关键行：

```text
   143: v=0e e=0002 i=0 cpl=0 IP=0008:0000010000073fb7 pc=0000010000073fb7 SP=0010:0000010000189ed0 CR2=0000010000189ed8
Triple fault
```

同一公用 serve 的反汇编栈分配：整改前／后：

```text
Before correction (serve frame 0x2e8 = 744 bytes):
   b5790:	48 81 ec e8 02 00 00 	sub    $0x2e8,%rsp
After correction (serve frame 0x128 = 296 bytes):
   b5990:	48 81 ec 28 01 00 00 	sub    $0x128,%rsp
```

原 `full-suite-initial.txt` 的关键行：

```text
ERROR full BIOS suite: 4 failed groups/checks; details in target/prima-os/full-suite-errors.log
```

原 `linux-net-unrestricted.txt` 的关键行：

```text
NET|pair=1;zero=1;large=1;drop=1;reuse=1;badfam=1;fork=1;interrupt=1;cancel=1;restart=1;closed=1|ok=1;err=0
```

原 `W7b-after.txt` 的关键行：

```text
PASS qemu bios W7b run=1
PASS qemu bios W7b run=2
```

原 `W7-after.txt` 的关键行：

```text
PASS qemu bios W7 run=1
PASS qemu bios W7 run=2
```

原 `full-suite-errors-final.txt` 的关键行：

```text
W7: BIOS W7 run 1 failed: QEMU timed out after 30s; QEMU child terminated
EXEC|exec=1;wait=1;bad=1;stream=1
```

原 `full-suite-final.txt` 的关键行：

```text
ERROR full BIOS suite: 1 failed groups/checks; details in target/prima-os/full-suite-errors.log
```

该轮最终源码 SHA-256（历史身份，不表示当前文件仍有相同哈希）：

```text
d3ca0518699946408128ce41b9e05bc7bc613554d039696092511117036d0c6c  kernel/Cargo.toml
19abdd804740858ed7d2b552dcb1192dbb28655330a4c46ee1c46c57e252be9a  kernel/src/bootstrap.rs
766980eabf009bfb343a7ca56bc1f7cd0e2fc2c4bea92c61e902e6c402f1ec4f  kernel/src/cap_view.rs
18f7a032661cf0f72133b9c1f0bbf0e50c012c19435210cb4f5ccc8dd3c9d746  kernel/src/gate.rs
7f77884ca2ff7a73a258cb7bb27f1879ae683040f4b0667b1804861404b69117  kernel/src/gate_probe.rs
4e4961b82784cfa28835e51b5a6934d63bacf9da113b77ebd6f93ca26ba958c4  kernel/src/main.rs
83ffab92ffee274384266bf12e864dedc8253fbbf13aac54fa3667b11cd3973b  kernel/src/syscall.rs
3a5f91f2640eb8d4635bd75ad3453223ea46ec20198d1bcc258c8410394ed54f  prima-abi/src/lib.rs
9cab6f94f9295d0b9cdf86f5dd8e4a5553981b3168edf6aa3d3a475731f3e6d1  xtask/Cargo.toml
53e70e07ea0be75a85a8df80506fcc9398e8cecb0554f0db9f9521fb9d5231a5  xtask/src/main.rs
dc3ad19a70dfb5730eabf3493b58f0435e8b0937ca786d309a246a2216fc4517  xtask/src/qemu.rs
```

该轮文档／状态检查关键行：

```text
PASS docs: 350 documents; catalog, IDs, topics, local links and Markdown anchors
```
