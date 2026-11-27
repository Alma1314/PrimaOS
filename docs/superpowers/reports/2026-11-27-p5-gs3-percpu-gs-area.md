---
id: report-p5-gs3-percpu-gs-area
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-27
---

# GS-3：每 CPU GS 区与经 `%gs` 的 CPU 索引

## 结论与范围

本切片实现 [Windows 阻断处置](../specs/2026-11-27-windows-blocker-decisions.md) §3.3 的
**GS-3**，承接作者既定顺序 `P5-3a → GS-1 → GS-2 → GS-3`：

1. 每 CPU 一个 `cpu::local::CpuLocal` 区，字段顺序即 GS 偏移：`%gs:0` = CPU 索引、
   `%gs:4` = APIC ID、`%gs:8` = 当前 Task id。`install_gs(cpu, apic_id)` 把
   `IA32_GS_BASE` 指向本槽并清 `IA32_KERNEL_GS_BASE`。
2. `cpu::local::current_index()` 在 `gs3-probe` 镜像中改为读 `%gs:0`（带越界拒绝），
   不再返回常量 BSP。普通镜像与 host 测试仍返回 BSP——GS 区不安装时无权读 `%gs`。
3. AP 在长模式入口先安装本 CPU 区，再用 `%gs` 读出自身索引、`IA32_GS_BASE`、
   `%gs:4` APIC ID，并记录 CPUID 自识别的 APIC ID；BSP 启动前安装自己的区，事后补测
   并逐 CPU 核对四项。
4. 专用 `cargo xtask test-gs3` 在 `-smp 2/4` 各两轮启动同一镜像，校验
   `SMP|gs=cpus=N;index_ok=N;base_ok=N;apic_ok=N;cpuid_ok=N;PASS` 后立即退出。

**不是运行期启用。** 默认镜像不安装 GS 区、`current_index()` 仍为 BSP；AP 仍只 `hlt`
停放、不参与调度；`sched::resume` 的 `activate_gs` 仍把 `KERNEL_GS_BASE` 置 0，因此
进入用户态后的内核 `%gs` 尚不指向 CPU 区。7-B 与任何阶段退出条件均未勾选，P5 未签署。

## 基线与输入

- 代码基线：`480053e`（GS-2 提交）。
- 上位：[阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 设计](../specs/2026-11-27-stage7-smp-state-partition.md)、
  [GS-2 报告](2026-11-27-p5-gs2-trap-swapgs-nmi.md)。
- QEMU：既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、128 MiB；仅把 `-smp 1` 换成 2/4。

## 实现内容

| 路径 | 内容 |
|---|---|
| `prima-ostd/src/arch.rs` | `read_gs_u32::<OFF>()`：编译期偏移的 `mov ..., gs:[OFF]` 只读原语 |
| `kernel/src/cpu.rs` | `CpuLocal` 改为 `#[repr(C)] { index, apic_id, current_task }`；`area_ptr`/`apic_id_on`、`install_gs`（写 `IA32_GS_BASE`、清 `KERNEL_GS_BASE`）、`local_apic_id`（`%gs:4`）；`gs3-probe` 下 `current_index()` 读 `%gs:0` |
| `kernel/src/smp.rs` | AP 入口安装本 CPU 区并记录 `GS_IDX_SEEN`/`GS_BASE_SEEN`/`GS_APIC_SEEN`/`GS_CPUID_SEEN`；`start_aps` 记录 MADT 期望 APIC ID；`gs_witness` 汇总 BSP 自测与每 AP 记录 |
| `kernel/src/protocol.rs`、`main.rs` | `SMP|gs=...;PASS`、失败诊断 `SMP|stage=gs-cpu=...`；BSP 在启动 AP 前安装 GS 区，见证后 `qemu::exit(SUCCESS)`（不进入用户态） |
| `xtask` | feature `gs3-probe`、命令 `test-gs3`、runner `run_bios_gs3`、校验器 `validate_gs3` 与 host 正反例 |

GS 偏移契约：`index` 在 0、`apic_id` 在 4，由 `offset_of!` host 测试钉住；任何字段调整
必须同步 `read_gs_u32` 的使用点。`install_gs` 只在 `gs3-probe` 且非 host 测试时编译，
普通镜像不触碰 GS MSR。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features gs3-probe -- test-gs3` | PASS：`-smp 2/4` × 2 轮，exit=33 | QEMU BIOS，2/4 vCPU |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1254 passed、0 failed、1 ignored | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（48 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p kernel --features gs3-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：完整 BIOS 套件 36 程序/72 轮、stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |
| `cargo run -p xtask --features gs-probe -- test-gs` | PASS：GS-1/GS-2 见证行不变，两轮日志逐字节相同 | QEMU BIOS，单核 |
| `cargo run -p xtask --features smp-ap -- test-smp-ap` | PASS：`-smp 2/4` × 2 轮 | QEMU BIOS，2/4 vCPU |

关键见证（`GS3-2-1/2.log` 与 `GS3-4-1/2.log`）：

```text
SMP|aps=1;online=2;percpu=1;tss=distinct
SMP|gs=cpus=2;index_ok=2;base_ok=2;apic_ok=2;cpuid_ok=2;PASS
SMP|PASS

SMP|aps=3;online=4;percpu=3;tss=distinct
SMP|gs=cpus=4;index_ok=4;base_ok=4;apic_ok=4;cpuid_ok=4;PASS
SMP|PASS
```

日志 SHA-256：`GS3-2-{1,2}` 均为
`1c3a1e3fcbc9c38b900ef20c8533c2377148d22e9a7588a6240985be0e8ab182`；
`GS3-4-{1,2}` 均为
`10e3e02424b46ff888083e0d053c0e28436bd1cdd872249db18f31e6854e8692`。
同 vCPU 数两轮逐字节相同。

逐项对应 GS-3 验收：

- **索引来自 `%gs` 而非常量/CPUID**：每个 CPU（含 BSP）在 `install_gs` 后经
  `current_index()` 读 `%gs:0`，`index_ok` 要求等于自己的索引。
- **每 CPU 区互异**：`base_ok` 要求 `IA32_GS_BASE` 等于 `area_ptr(cpu)`；host 测试另行
  钉住四个区地址互异。
- **APIC ID 与 MADT 一致**：`apic_ok` 比较 `%gs:4` 与 `start_aps` 记录的 MADT APIC ID；
  `cpuid_ok` 另行比较 CPUID leaf 1 的 APIC ID，证明同一索引由同一物理 CPU 执行、
  未发生同一 AP 以不同索引重复启动。
- **AP 并发上下文**：各 AP 的测量发生在 `-smp 2/4` 启动过程中，BSP 同时执行启动循环；
  `start_aps` 逐个等待在线后才启动下一个 AP。这证明每 CPU GS 区在 AP 首次执行时可用，
  **不是**锁步同时采样（见限制）。

与 P5-4 的衔接：每 CPU 区同时保存 MADT APIC ID，后续 IPI 目标可由本地区直接取得；
本切片不发 IPI、不做 TLB shootdown，锁顺序与非中断上下文约束未改变。

## 失败、反例与限制

1. **首轮使用“BSP 置 GO、AP 在线后回读”的会合设计，`-smp 4` 超时**：`-smp 2` 通过，
   `-smp 4` 只有最后启动的 AP 观察到旗标；诊断（每 AP 阶段与自旋计数）显示先启动的 AP
   在单线程 TCG 下未再获得调度。该失败保留在实现历史中；改为各 AP 在自己的启动入口
   测量（入口执行是确定的），不再依赖让忙等 AP 观察后置旗标。此现象不否定 P5-2/P5-3a
   已登记的 2/4 vCPU 在线与每 CPU TSS 证据，但说明本 QEMU 配置下“所有 vCPU 同时自旋”
   的验证协议不可靠。
2. **不是运行期启用**：默认镜像不安装 GS 区，`current_index()` 仍返回 BSP；`gs3-probe`
   镜像在见证后立即退出，不进入用户态。任务切换（`sched::resume`）仍只写 BSP 的
   `TSS.rsp0`，`activate_gs` 仍清 `KERNEL_GS_BASE`；因此本切片不证明用户边界、
   调度迁移或跨 CPU 唤醒下的 GS 正确性。
3. **AP 仍不参与调度**：AP 安装 GDT/TSS、加载共享 IDT 后 `hlt`，IF=0；无 IPI、无 TLB
   shootdown、无并行 fork/exec，7-B 四项退出条件均未勾选。
4. **xAPIC、单线程 TCG**：未测 x2APIC；实测调度观察来自 `-accel tcg,thread=single`，
   真机/MTTCG 的并发时序未覆盖。
5. **诊断行不属验收**：失败时输出的 `SMP|stage=gs-cpu=...` 仅用于定位；校验器只接受
   完整 `SMP|gs=` 行与单个 `SMP|PASS`。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`；
不改变 GS-1/GS-2 的默认镜像行为（探针日志的 GS 见证行与上一提交相同）。GS-3 落盘为
受控探针范围；运行期启用仍须在 P5-3b 的入口状态分区、任务放置与统一期限评审后决定。

## 后继关系

- 前序：[GS-2 陷阱 `swapgs` 与 NMI/IST](2026-11-27-p5-gs2-trap-swapgs-nmi.md)。
- 上位：[阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：P5-3b 入口状态分区、每 CPU 可运行队列与创建时放置；P5-4 IPI/TLB shootdown
  可直接使用每 CPU 区的索引与 APIC ID（本切片未实现）；GS-4 运行期 TEB/TLS。
