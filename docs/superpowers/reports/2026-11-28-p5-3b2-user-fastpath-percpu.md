---
id: report-p5-3b2-user-fastpath-percpu
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-28
---

# P5-3b-2：CPL3 入口、`syscall` 快路径与恢复跳板的每 CPU 分区

## 结论与范围

本切片是 [S7-B.1](../specs/2026-11-27-stage7-smp-state-partition.md) P5-3b 的第二步：把
**CPL3 来源**的 `int 0x80`、`#PF`（含 `USER_FRAME`）、`syscall` 快路径（每 CPU 栈、
`USER_RSP`、保存块基址）与 `resume` 跳板（每 CPU 恢复帧指针）全部改为每 CPU 槽，索引
取自本 CPU GS 区（GS-3 契约）。每个 CPU 构造本 CPU 的最小用户根，经每 CPU 恢复跳板进入
CPL3，用户代码依次触发写未映射页 `#PF`、`int 0x80`、`syscall` 快路径，并以一次
`int 0x80` 收尾；收尾回调按 CPU 汇总。BSP 在 AP 停放后执行同样流程并汇总两面见证。

**不是运行期启用。** 普通镜像不编译本 feature；AP 上线后即停放，不进入调度、不接收
中断；可运行队列、创建时放置与唯一运行者协议仍未实现。7-B 与任何阶段退出条件均未
勾选，P5 未签署。

## 基线与输入

- 代码基线：`73a81f8`（P5-3b-1 提交）。
- 上位：[S7-B.1 §5.1/§8](../specs/2026-11-27-stage7-smp-state-partition.md)、
  [P5-3b-1 报告](2026-11-27-p5-3b1-entry-percpu.md)、
  [GS-3 报告](2026-11-27-p5-gs3-percpu-gs-area.md)。
- QEMU：既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、128 MiB；仅 `-smp 1` 换 2/4。

## 实现内容

| 路径 | 内容 |
|---|---|
| `prima-ostd/src/entry.rs` | `PerCpuEntry` 增加 `resume_frame`（0xC0）与 `user_frame`（0xC8）；每 CPU 裸入口：`syscall_entry_percpu`/`page_fault_entry_percpu` 支持 CPL3（GS-2 判据 + `USER_FRAME`），新增 `syscall_fast_entry_percpu`（GS 区 `USER_RSP`/栈顶/槽基址直接寻址）与 `resume_percpu`/`exc_resume_entry_percpu`（每 CPU 恢复帧 + `iretq` 前把两根 GS MSR 切到用户侧）；注册式收尾回调 `set_finish_handler`/`finish_now` |
| `prima-ostd/src/entry.rs` | GS 区偏移常量 `GS_SYSCALL_STACK_TOP_OFF=0x10`、`GS_USER_RSP_OFF=0x18`、`GS_ENTRY_SLOT_OFF=0x20` |
| `kernel/src/cpu.rs` | `CpuLocal` 增加 `syscall_stack_top`/`user_rsp`/`entry_slot`；`install_gs` 写入；访问器 `set_syscall_stack_top`/`user_rsp_on`/`syscall_stack_top_on` |
| `kernel/src/entry_probe.rs` | 用户代码生成器（每 CPU 独立代码页）；最小用户根（代码/栈/数据页；`K_STACK_VA` 与用户页同 PT）；CPL3 回调（`int80`/`fast`/`fault`/`finish`）；回执读取；两面见证 |
| `kernel/src/smp.rs` | AP 在 CPL0 自检后进入 CPL3 探针；收尾置在线位；`AP_PHYS_OFFSET` |
| `kernel/src/main.rs` | BSP CPL0 自检后进入 CPL3；注册收尾回调并在收尾时输出两面见证 |
| `kernel/src/protocol.rs` | `SMP|entry-user=...` 见证、用户诊断与根构建失败步骤行 |
| `xtask/src/protocol.rs` | `validate_entry` 接受 6 行见证（增加 `entry-user`）与正反例 |

**发现并修复的缺陷**：

1. **静态 `.bss` 每 CPU 快路径栈在启动器根下不可靠**：BSP 的快路径栈顶页在启动器根中
   可读但 `push` 失败（启动器对 `.bss` 尾部的映射不保证可写），导致 BSP 在 `syscall`
   入口切栈时三重故障。改为在探针根内**显式映射**每 CPU 快路径内核栈（`K_STACK_VA`，
   16 KiB/CPU），并在构建后把真实栈顶写入 GS 区。
2. **AP 共享跳板根**：所有 AP 使用同一 `ap_root`，第二个 AP 的槽位检查失败。改为 AP 复用
   探针自己在上一次 AP 进入时建立的用户页节点并覆盖叶 PTE（BSP 仍拒绝覆盖启动器映射）。
3. **跨 CPU 根的 VA 不可互读**：BSP 汇总时读取 AP 快路径栈的哨兵会访问 BSP 根下未映射的
   地址。改为每个 CPU 在自身根下校验哨兵并记录标志，BSP 只读标志。
4. **`resume_percpu` 的 GS 切换**：裸跳板在 `iretq` 前保存原 `IA32_GS_BASE`、置
   `GS_BASE=0`、`KERNEL_GS_BASE=原 GS`，使用户态 `%gs` 与入口 `swapgs` 配对。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features entry-percpu-probe -- test-entry` | PASS：`-smp 2/4` × 2 轮 | QEMU BIOS，2/4 vCPU |
| `CARGO_NET_OFFLINE=true cargo test -p prima-ostd --features entry-percpu-probe` | PASS：47 + 2 + 5 | host |
| `CARGO_NET_OFFLINE=true cargo test -p xtask --features entry-percpu-probe smp_tests` | PASS：4 | host（校验器正反例） |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features smp-ap -- test-smp-ap` | PASS：`-smp 2/4` × 2 | QEMU BIOS，SMP 回归 |
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features gs3-probe -- test-gs3` | PASS：`-smp 2/4` × 2 | QEMU BIOS，GS-3 回归 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1255 passed、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（49 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo clippy -p kernel --features entry-percpu-probe --target x86_64-unknown-none -- -D warnings` | PASS | host/bare-metal 构建 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：36 程序/72 轮 + stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |

关键见证（`ENTRY-2-*` 与 `ENTRY-4-*`）：

```text
SMP|entry=cpus=2;syscall_ok=2;ctx_ok=2;fault_ok=2;frame_ok=2;PASS
SMP|entry-user=cpus=2;int80_ok=2;fast_ok=2;fault_ok=2;frame_ok=2;fast_ctx_ok=2;fast_rsp_ok=2;PASS
SMP|PASS

SMP|entry=cpus=4;syscall_ok=4;ctx_ok=4;fault_ok=4;frame_ok=4;PASS
SMP|entry-user=cpus=4;int80_ok=4;fast_ok=4;fault_ok=4;frame_ok=4;fast_ctx_ok=4;fast_rsp_ok=4;PASS
SMP|PASS
```

日志 SHA-256：`ENTRY-2-{1,2}` 均为
`24f6ead396df0209a5100b222cad9a8362333906d4317999e855d0302a2df719`；
`ENTRY-4-{1,2}` 均为
`8334c4a996e8956df619f95a9005d8b43df293720e390879610610200ec60cb3`。
同 vCPU 数两轮逐字节相同。

逐项对应本片验收：

- **`int80_ok`**：CPL3 `int 0x80` 回调要求调用号/参数含本 CPU 编码、进入 `CS==0x33`、
  保存块基址落在**本 CPU 的内核栈**（BSP 启动栈或 AP 16 KiB 栈）；用户把返回值写回
  本 CPU 数据页，收尾核对回执等于 `RET_USER+cpu`。
- **`fast_ok`/`fast_ctx_ok`**：`syscall` 快路径回调要求同样的 CPU 编码与 `CS`，且保存块
  基址必须落在**本 CPU 显式映射的快路径栈**内；用户写回回执等于 `RET_FAST+cpu`。
- **`fault_ok`/`frame_ok`**：CPL3 写未映射页产生 `err=0x6`（P=0,W=1,US=1），CR2 与每个
  CPU 的专用故障地址一致；入口写入本 CPU 的 `USER_FRAME`，其 `CS==0x33`、`SS==0x2b`、
  `RIP` 等于回调独立记录的故障指令、`RSP` 落在用户栈范围。
- **`fast_rsp_ok`**：本 CPU 快路径入口写入 GS 区的 `USER_RSP` 落在用户栈范围，且快路径
  栈底哨兵在**本 CPU 自己的根**下完好。

## 失败、反例与限制

1. **不是并发锁步**：单线程 TCG 下 AP 在 BSP 忙等期间执行其 CPL3 探针，BSP 在 AP 全部
   停放后执行；两段不同时交错。本片证明每槽只保留本 CPU 的数据（单槽/固定槽 0 会让至少
   一个 CPU 读回零或被覆盖），不证明同时执行无串扰。
2. **AP 共享跳板根**：所有 AP 用户页挂在同一 `ap_root` 的同一组 VA 上，每次 AP 进入前
   覆盖叶 PTE。BSP 使用启动器根并拒绝覆盖已存在映射。本片不证明根隔离或并发访问。
3. **未覆盖的准入反例**：S7-B.1 §5.1 要求的**阻塞恢复**与**嵌套故障**反例未做；AP 上线
   后即停放，不调度、不阻塞、不嵌套。可运行队列、创建时放置、唯一运行者协议、每 CPU
   时基均未实现；阶段 7-B 四项仍未勾选。
4. **快路径栈是探针专用**：每 CPU 16 KiB 显式映射栈（生产预算与 guard 页语义未冻结）；
   探针用户代码不压栈，用户栈与数据页共用一帧。
5. **帧与回收**：用户页/栈/页表帧取自保留页表帧分配器，探针退出前不回收；AP 复用时
   旧帧不归还。探针镜像不跑帧守恒审计。
6. **失败诊断行不属验收**：`SMP|stage=entry-*` 仅用于定位；校验器只接受完整
   `SMP|entry=`、`SMP|entry-user=` 行与单个 `SMP|PASS`。
7. **CPL0 行为与上一片一致**：`ctx_ok`/`fault_ok`/`frame_ok` 的 CPL0 自检未变；本片新增
   的 `SMP|entry-user=` 行使 `ENTRY-*` 日志哈希相对 P5-3b-1 变化（属本命令的事件扩展，
   默认镜像事件不受影响）。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件或默认 QEMU `-smp 1`；
默认镜像不编译本 feature。P5-3b 仍未完成（可运行队列、创建时放置、唯一运行者、阻塞
恢复与嵌套故障反例待做）。

## 后继关系

- 前序：[P5-3b-1 CPL0 入口现场](2026-11-27-p5-3b1-entry-percpu.md)。
- 上位：[S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：P5-3b-3 引入每 CPU 可运行队列、创建时放置与唯一运行者协议，并以双核并行
  fork/exec、阻塞恢复与嵌套故障反例验证；随后 P5-4 IPI/TLB shootdown。
