---
id: report-p5-4-1-directed-ipi
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: historical
updated: 2026-11-28
---

# P5-4-1：AP LAPIC 窗口与有界定向 IPI 往返

## 结论与范围

修复 AP 根缺少 LAPIC MMIO 窗口的前置问题；独立 `ipi-probe` 在每个 AP 上启用
LAPIC，经 IRQ15/vector `0x2f` 完成 BSP→AP→BSP 定向往返。`-smp 2/4` 各两轮
通过，包括主动抑制 pong 后的确认超时反例。同 vCPU 数两轮完整串口输出逐字节相同。
本报告是有界 QEMU 技术证据，不是 P5/7-B 签署。

本片不修改调度器，不刷新 TLB，不开放远端活动 root 修改或帧回收。
generation/ticket、active-CPU 集合与运行期超时失败策略仍须后续冻结。

## 基线与输入

- 代码基线：`e10cf66`，开始时工作区干净；本次改动未提交。
- 上位：[S7-B.1 §5.3](../specs/2026-11-27-stage7-smp-state-partition.md#53-p5-4-1-定向-ipi-传输探针边界)、
  [阶段 7 执行计划 S7-B.1](../plans/2026-11-06-stage7-execution.md)。
- 前序：[AP 启动失败链与 WB 映射限制](2026-11-27-p5-2-ap-bringup.md)、
  [每 CPU GDT/TSS](2026-11-27-p5-3a-percpu-gdt-tss.md)、
  [P5-3b-6 与 exec 后 fork 失败](2026-11-28-p5-3b6-program-fork-exec-post-wakeup.md)。
- 工具链：`nightly-2026-07-30`；QEMU 8.2.2、`pc-i440fx-8.2`、`qemu64`、128 MiB、
  TCG 单线程，沿用 `qemu_args_smp`；只有 `test-sched` 回归沿用 MTTCG。
- 改前先实跑 `test-smp-ap`，保留四份基线日志于 `/tmp/p5-4-baseline/`；运行日志不入库。

## 实现内容

| 路径 | 行为与边界 |
|---|---|
| `kernel/src/smp.rs` | BSP 在既有 `lapic-map` 后把 LAPIC 的 PML4 项复制进尚未活动的 AP 根，并校验其叶解析到 MADT 的 LAPIC 物理页；不增加页表帧分配，不重排启动事件 |
| `kernel/src/smp.rs` | 保留原有 ICR 前后 10,000,000 次轮询界限，抽取 `poll_bounded` 给确认路径复用；host 验证耗尽、最后一次成功、零预算和 ICR busy 时不覆写命令 |
| `kernel/src/ipi_probe.rs` | AP 使能自身 LAPIC 后才发布在线位，以 `sti/hlt/cli` 等待；BSP 每次只向一个 MADT APIC ID 发固定向量 ping，目标回送 BSP；全程无共享锁 |
| `kernel/src/irq.rs` | 仅 `ipi-probe` 借用现有 IRQ15 DPL0 入口；handler 记录本 CPU 接收数、LAPIC EOI、有界回送和回送结束标志；物理 PIC 设备线保持屏蔽 |
| `kernel/src/main.rs`、`protocol.rs` | GS 自识别见证通过后运行传输探针；IPI 往返的 ICR/确认失败立即输出原因并以失败码退出；成功才打印 IPI 行与最终 PASS |
| Cargo features、`xtask` | 新增 `ipi-probe`/`test-ipi`；校验 CPU 数、往返数、定向检查与超时反例，拒绝缺失、重复、乱序、失败或错误字段 |

每轮往返核对所有 CPU 的累计接收数，非目标 CPU 必须保持不变；BSP 同时等待 pong
与目标 AP 的发送结束标志。正例完成后，主动抑制 AP1 的 pong，再通过同一发送/确认
路径要求 `ipi-ack-timeout`，并核对 AP1 确实收到额外 ping、BSP 没有收到额外 pong。
ICR 和确认均有迭代上限，不承诺物理时间上限。

## 命令与结果

以下 Cargo 命令均以 `CARGO_NET_OFFLINE=true` 运行。默认套件首轮因沙箱宿主 UDP 权限失败；沙箱外完整重跑解决该权限失败，但 V2/N1 各有一轮 QEMU 超时。两轮命令都 exit=1，不能计为完整套件通过。聚焦 V2 双轮通过，聚焦 N1 第一轮再次超时、第二轮通过。未取得完整套件连续 PASS。

| 命令 | 本次结果 | 范围 |
|---|---|---|
| `cargo run -q -p xtask --features ipi-probe -- test-ipi` | PASS，2/4 vCPU 各两轮，exit=0；含回送结束检查后的重跑 | QEMU BIOS，CPL0 传输与缺确认反例 |
| `cargo test -p kernel --features ipi-probe smp::tests` | PASS，5 项，exit=0 | host；含 ICR 持续 busy 与有界轮询 |
| `cargo test -p xtask --features ipi-probe smp_tests` | PASS，6 项，exit=0 | host 见证校验正反例；新增 2 vCPU 正例已由 workspace 回归覆盖 |
| `cargo clippy -p kernel --features ipi-probe --target x86_64-unknown-none -- -D warnings` | PASS，exit=0 | 探针裸机构建/静态 |
| `cargo run -q -p xtask --features smp-ap -- test-smp-ap` | PASS，修改前/后均各四轮，exit=0；差分范围见下节 | QEMU BIOS，AP 启动回归 |
| `cargo run -q -p xtask --features sched-percpu-probe -- test-sched` | PASS，2/4 vCPU 各两轮，exit=0；同 vCPU 双轮逐字节一致 | QEMU BIOS，MTTCG；既有双核负载回归 |
| `cargo run -q -p xtask --features entry-percpu-probe -- test-entry` | PASS，2/4 vCPU 各两轮，exit=0；同 vCPU 双轮逐字节一致 | QEMU BIOS，CPL0/CPL3 入口回归 |
| `cargo run -q -p xtask --features gs3-probe -- test-gs3` | PASS，2/4 vCPU 各两轮，exit=0；同 vCPU 双轮逐字节一致 | QEMU BIOS，GS 自识别回归 |
| `cargo test --workspace` | PASS，1258 passed、0 failed、1 ignored，exit=0 | host |
| `cargo xtask lint` | PASS，fmt、clippy、裸机构建、零 unsafe（51 文件）、S6-X3，exit=0 | host/静态 |
| `cargo xtask test`（沙箱内） | FAIL，exit=1：Linux N1 与 R3 宿主 UDP 均 EPERM；70 个 BIOS 轮次通过，R3 两轮未执行 | 环境失败；不计为完整套件通过 |
| `cargo xtask test`（沙箱外） | FAIL，exit=1：70 个 BIOS 轮次通过，V2 第一轮/N1 第二轮各 30 秒超时；N1/R3 宿主参考通过，R3 两个 QEMU 轮次通过 | 完整重跑，保留两个 QEMU 失败 |
| `cargo xtask test-vfs V2` | PASS，exit=0，原参数/30 秒门限下双轮通过 | 聚焦复核；不消除完整套件 V2 超时记录 |
| `cargo xtask test-net`（沙箱外） | FAIL，exit=1：宿主参考通过，QEMU 第一轮同样超时、第二轮通过 | N1 未取得稳定双轮；不消除完整套件 N1 超时记录 |
| `python3 scripts/docs.py self-test / build / check / check --base e10cf66`（分别运行） | PASS，各 exit=0，456 文档 | 文档结构、目录新鲜度与基线变更守卫 |
| `env -i target/prima-os/net-linux`（沙箱外） | PASS，所有 NET 字段为 1、`err=0`，exit=0 | 宿主 UDP 权限定位；不替代完整套件 |
| `PRIMA_BOOT='std env' target/linux-ref/rust-probe '' '中文 std'`（沙箱外） | PASS，`posix/fs/env/udp/thread=1`，exit=0 | R3 宿主 UDP 权限定位；不替代完整套件 |

关键见证：

```text
SMP|ipi=cpus=2;roundtrips=1;directed=1;ack_timeout=1;PASS
SMP|ipi=cpus=4;roundtrips=3;directed=1;ack_timeout=1;PASS
```

最终 IPI 日志 SHA-256：`IPI-2-{1,2}` 均为
`4385c3ce04dc130c807e1798ddecfbc2124385089d5b6e87d18f481773d5754e`；
`IPI-4-{1,2}` 均为
`d9c7b33729bf7ea7315d758c1990e326038b4b9a2bad796453ab88e78eebabb6`。

### AP 启动差分范围

修改前后的 `test-smp-ap` 均为 2/4 vCPU 各两轮、命令 exit=0。四份日志的全部
`SMP|` 行（含 `stage` 事件）逐字节一致；`vspace_ready`、`cow_ready`、
`platform_segments_ready`、`self_root_ready` 行也逐字节一致，页表探针帧地址和表帧数不变。
同版本、同 vCPU 两轮整份日志逐字节一致。

**完整串口基线比较不通过**：新代码改变 ELF 长度 `2841136→2841480`、段布局及
bootloader 建 GDT 的物理页 `0x177e000→0x177d000`，可用帧 `29679→29680`；
`memory_ready`、`bootstrap_ready` 与 `frames_ready` 的相关动态计数因此变化。
`frames_ready` 的 segment=292、table=157、image=694 保持不变。
这不是完整 BIOS 输出不变的证据；不得把启动 `SMP|` 序列一致外推为整份日志一致。
本次未改动 trampoline 模板和补丁字段。

改前 `SMPAP-2-{1,2}` SHA-256 均为
`2d73d28a9d01240813eb284ffa5157a592f33bd8fd57afabde0ffa56960366bd`，
`SMPAP-4-{1,2}` 均为
`0e62a110722928f022e2d5859a465e5d5cb0b8a53f4fc85850d92cb135e048a2`；
改后分别为
`ad37b7f3178f9b23108744fc179cb3c16eeae03c3b556771398c234b667cd395`、
`0a4cf827aecb927f1fe8695ba63e7f76e90ef6ed0e475c0aecc6a8f90e0db5b1`。

## 失败、反例与限制

1. **前次回退试验**：用户交接记录指出 AP 根没有 LAPIC 窗口，物理直映射只覆盖 RAM，
   AP 写 SVR 的 #PF 被跳过，随后发送方无界等 ICR。该试验已回退且未留下原始日志；
   本次不重放该挂起。代码检查独立确认：`build_ap_root` 在 BSP 建 LAPIC 窗口前复制根。
   另须区分：`e10cf66` 的 `wait_icr` 本已有 10,000,000 次上限；无界等待属于已回退试验，
   本片保留其基线有界语义并复用于确认。
2. **负例范围**：QEMU 注入的是“收到 ping 但不回 pong”；ICR 永久 busy 用 host 寄存器
   替身验证，不声称已实跑“关闭目标 LAPIC”的硬件反例。
3. **CPL0 only**：未与用户态、调度、GS 交换窗口或共享 root 并行混跑；不与
   `entry-percpu-probe`/`sched-percpu-probe` 组合。本探针在设备/用户阶段前退出。
4. **单个在途目标**：计数与回送标志不是 generation/ticket；没有重试、晚到确认或重复
   投递恢复语义，不得作为运行期 shootdown API 使用。
5. **MMIO WB**：沿用 P5-2 的 WB 映射，未设置 PCD/PWT，不宣称真机缓存语义；xAPIC
   only，最多 4 CPU，无热插拔、重启、APIC ID 超出 xAPIC 范围的支持。
6. AP 启动阶段自身的超时仍沿用前序返回错误、输出失败并继续启动的策略，由专用
   校验器拒绝；本片立即失败退出只覆盖进入 IPI 往返探针后的传输失败，不是运行期 fail-stop。
7. 文档初次 `check` 拒绝未登记的 `status: partial`；改用已有 `verified-scoped`（仅上述
   已验证范围）后通过，未为此新增状态分类。
8. **默认套件的宿主权限失败**：沙箱中 Linux N1 参考探针输出 `NET|pair=0;...|ok=0;err=1`；
   另用 Python 创建 `AF_INET/SOCK_DGRAM` socket 立即得到 `EPERM`，尚未执行 bind。
   同一二进制 `env -i target/prima-os/net-linux` 在沙箱外运行，所有字段为 1、`err=0`、exit=0。
   R3 Linux 参考也在 `posix.rs:88` 的 `UdpSocket::bind` 得到 EPERM，导致它的两个 QEMU
   轮次未执行。该 R3 二进制在沙箱外按原参数/环境运行输出 `posix/fs/env/udp/thread=1`，
   exit=0。首轮命令 exit=1，共两个失败检查，70 个已执行 BIOS 轮次全部通过。
   未修改参考源码或验收断言；首轮失败保留，沙箱外完整重跑的 N1/R3 宿主参考均通过，
   R3 两个 QEMU 轮次也通过；但重跑仍有下述两个 QEMU 超时。
9. **沙箱外完整重跑的 QEMU 超时**：V2 第一轮在原 30 秒门限超时并回收 QEMU 子进程，
   第二轮通过；超时输出停在启动期 `preempt_ready`（seq=31），尚未进入 V2 用户负载，
   未见 panic。同期宿主 load average 约 13.7、可见 CPU 为 8；高负载是可能因素，
   尚未证明为根因。首轮沙箱中 V2 两轮均通过。原超时保留，完整重跑最终 exit=1、70 轮通过、2 轮超时。
   N1 第一轮通过、第二轮也触及 30 秒超时，最后用户输出为 `NET|...;interrupt=0`，
   未完成完整 NET 结果；其根因未定，不能仅凭负载将其归为宿主环境问题。
   聚焦 `test-vfs V2` 在原参数/30 秒门限下双轮通过。聚焦 `test-net` 的第一轮再次
   超时，最后仍为 `NET|...;interrupt=0`，第二轮通过，命令 exit=1。N1 的等待/信号
   时序尚未定位，不能计为已修复或稳定通过；未取得连续完整套件 PASS。
10. 未实现 TLB 失效、确认前帧不复用、运行期 fail-stop、AP timer 或统一时间域；exec 后
   fork 的取指缺页、Gate 端到端、服务管理器 held-launch 仍未解决。

默认回归缺口：V2 的连续完整套件超时记录仍保留；N1 在完整与聚焦运行均出现
`interrupt=0` 后挂起，需要另行定位。上述聚焦复核未改变任何验收断言或时间门限。

## 阶段/profile 影响与后继

P5-4 只取得第一项传输探针证据。普通镜像不编译 `ipi-probe`；默认 QEMU 参数、profile
和阶段退出勾选不变。下一步为冻结 active-CPU、generation/ticket、确认和超时失败协议，
再接 TLB shootdown；仍须保留 [S7-B.1 §5.1](../specs/2026-11-27-stage7-smp-state-partition.md#51-ap-用户态调度准入与初次放置)
对远端活动映射修改及帧回收的限制。当前结论以[阶段跟踪](../../../Prima%20OS%20实现阶段.md)为准。
若向量用途、并发发送模型、缓存属性或 root 生命周期变化，须重审本片证据。
