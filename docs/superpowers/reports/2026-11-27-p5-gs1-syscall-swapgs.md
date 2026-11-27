---
id: report-p5-gs1-syscall-swapgs
kind: report
authority: scoped-qemu-evidence
status: verified-scoped
lifecycle: current
updated: 2026-11-27
---

# GS-1：`syscall` 路径 `swapgs` 与每任务用户 GS（受控探针）

## 结论与范围

本切片实现 [Windows 阻断处置](../specs/2026-11-27-windows-blocker-decisions.md) §3.3 的
**GS-1**，即作者既定顺序 `P5-3a → GS-1 → GS-2` 的第二步：

1. `prima-ostd::arch` 增加 `swapgs` 原语；`kernel::msr` 增加 `IA32_GS_BASE=0xC0000101`、
   `IA32_KERNEL_GS_BASE=0xC0000102` 常量。
2. `prima-ostd::entry::syscall_fast_entry` 在**运行期开关**下于入口/出口各执行一次
   `swapgs`；开关默认关闭，普通镜像完全不触碰 GS 基址，且不依赖 `%gs`。
3. `sched` 增加每任务用户 GS 槽（与 `FS_BASES` 对称）：`set_gs_base`/`gs_base`/清零；
   `sched::resume` 在开关开启时把 `GS_BASE` 强制写成该任务的值、`KERNEL_GS_BASE` 置 0。
4. 新增 `SYS_TASK_SET_GS_BASE=90`：只写调度槽（不立即改硬件），非规范值返回 `EINVAL`，
   探针关闭时返回 `ENOSYS`。
5. 新增 `gs-probe` feature 与 `cargo xtask test-gs`：专用镜像把两个用户任务替换为手写
   探针机码，ring3 覆盖三项验收。

**这不是运行期启用。** 普通镜像、AP 调度与 Windows 运行期都不使用 GS；`int 0x80` 与
异常/IRQ 入口**未**交换（属 GS-2）；内核每 CPU GS 区**未**建立（属 GS-3）。7-B 与任何阶段
退出条件均未勾选，M3 运行期 TEB/TLS 仍被 GS-2…GS-4 阻塞。

## 基线与输入

- 代码基线：`46e7ae6`（P5-3a 之后的工作树）。
- 上位：[阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 设计 §7 执行顺序](../specs/2026-11-27-stage7-smp-state-partition.md)。
- QEMU：既有 `pc-i440fx-8.2`、TCG 单线程、`qemu64`、128 MiB 单核，参数冻结；`test-gs`
  不改成多核。

## 实现内容

| 路径 | 内容 |
|---|---|
| `prima-ostd/src/arch.rs` | `swapgs()`（CPL0 原语，无内存副作用）；调用契约入库文档 |
| `kernel/src/msr.rs` | `IA32_GS_BASE`、`IA32_KERNEL_GS_BASE` 常量 |
| `prima-ostd/src/entry.rs` | `GS_SWAP` 运行期开关 + `enable_gs_swap()`/`gs_swap_enabled()`；`syscall_fast_entry` 入口先 `swapgs`、出口 `sysretq` 前再 `swapgs`（`cmp byte [rip+GS_SWAP],0` 条件执行，只改 RFLAGS，不动 GPR） |
| `kernel/src/sched.rs` | `GS_BASES` 槽、`set_gs_base`/`gs_base`、`clear_task` 清零、`activate_gs`（开关下写两 MSR）、`resume` 调用 |
| `kernel/src/syscall.rs` | `SYS_TASK_SET_GS_BASE` 分派、`validate_gs_base`（复用低半区规范地址约束）、只写槽 |
| `prima-abi/src/lib.rs` | 追加号 90，更新 D17 冻结台账（88→89 项） |
| `kernel/src/user.rs` | `gs_task1_code`/`gs_task2_code` 探针机码、`GsWitness`、按物理读 Y2；`gs-probe` 下替换两任务代码、预置 GS 槽与 Y 页魔数 |
| `kernel/src/protocol.rs`、`resume.rs` | `GS|...;PASS/FAIL` 见证；`gs-probe` 下 `int3` 收尾走精简分支 |
| `kernel/Cargo.toml`、`xtask` | `gs-probe` feature、`test-gs` 命令、`validate_gs` 校验器与 host 用例 |

配对不变量（关键）：

- 用户 GS 只能经 `SYS_TASK_SET_GS_BASE` 写入**槽**（CPL3 不能写 MSR）；值在下一个
  `sched::resume` 由 `activate_gs` 一次性装入。因此无需从半交换的硬件状态回读，也回避了
  `syscall`（已交换）与 `int 0x80`（未交换）混用时“该写哪根 MSR”的分歧——统一在恢复点处理。
- 普通镜像 `GS_SWAP=0`、两 MSR 恒 0，入/出口整段跳过，既有路径零变化。
- 探针镜像 `GS_SWAP=1`：入用户态前 `GS_BASE=task_gs, KERNEL_GS_BASE=0`；`syscall` 入口
  交换得 `GS=0, KERNEL_GS=task_gs`，出口换回；内核在 GS-3 前不读 `%gs`，故内核态 GS 值
  不影响正确性。

## 命令与结果

| 命令 | 本次结果 | 证据范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo run -q -p xtask --features gs-probe -- test-gs` | PASS：`-smp 1` × 2 轮，exit=33 | QEMU BIOS，单核 |
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1251 passed、0 failed | host |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、clippy `-D warnings`、bare-metal kernel、零 unsafe（47 文件）、S6-X3 | host/静态 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | PASS：完整 BIOS 套件 36 程序/72 轮、stage1 boot slice（默认镜像事件不变） | QEMU BIOS，单核 |

关键见证（`GS-1.log` 与 `GS-2.log` 逐字节相同；SHA-256 均为
`6682e9932a6f6656f50349eeaa046797119b8b58694d358561918bf02eb747ad`）：

```text
GS|read1=0x47535f54315f4d41;read2=0x47535f54325f4d42;invalid=0xffffffffffffffea;post_yield=0x47535f54315f4d41;post_syscall=0x47535f54315f4d41;echo=0xffff0000ffff1234;ok=1
GS|PASS
```

逐项对应三项验收：

- **ring3 用户 GS 往返**：`read1==0x47535f54315f4d41`（task 1 `mov gs:[0]` 读到自己 Y 页的
  魔数），`syscall` ECHO 往返后 `post_syscall` 仍为该魔数 → 入口/出口 `swapgs` 配对，
  往返不改用户 GS。
- **两任务 GS 不串扰**：task 2 在其恢复点读到 `read2==0x47535f54325f4d42`（自己的 Y2 魔数，
  非 task 1 的值）；task 1 在 task 2 让回后 `post_yield` 仍为自己魔数 → 双向各自正确。
- **错误 GS 值被拒绝**：`SYS_TASK_SET_GS_BASE(2^47)` 返回 `EINVAL`
  （`invalid=0xffffffffffffffea`），槽与硬件均不被改写。

`echo=0xffff0000ffff1234` 即 `0x1234 ^ ECHO_MASK`，是既有的 `SYS_ECHO` 语义，证明
`syscall` 快路径本身在 GS 探针镜像下仍正确。

新增 host 用例：`gs_bases_are_per_task_and_cleared_with_task`（槽独立与清理）、
`gs_base_validation_rejects_non_canonical_and_kernel_addresses`、
`gs_witness_requires_all_fields_and_one_pass`（校验器正反例）。

## 失败、反例与限制

- 本切片没有失败运行。首轮实现曾把 `GS_ECHO_RET` 误写为 `arg+1`，QEMU 报 `ok=0`
  （`echo=0xffff0000ffff1234`）；诊断确认为 `SYS_ECHO` 实为 `arg ^ ECHO_MASK`，修正期望值
  （非机制缺陷），重跑双轮通过。
- **仅 `syscall` 指令路径交换**：`int 0x80`、异常、IRQ、NMI 入口都不 `swapgs`（GS-2）。
- **NMI 半交换窗口**：GS-1 探针内核不读 `%gs`，NMI 命中也不致错，但没有 NMI/IST 负例矩阵；
  该矩阵属 GS-2 的退出条件。
- **未启用用户 `wrgsbase`**：CR4.FSGSBASE 保持关闭，用户不能自行改 GS 基址；所有设置经内核
  槽进行。Windows TEB/TLS（GS-4）与 APC/SEH（GS-5）未触及。
- **探针镜像替换了用户程序**：`test-gs` 只校验退出码与 `GS|` 见证，不跑完整用户-phase
  校验器；默认镜像的完整回归仍由 `cargo xtask test` 覆盖。
- 内核 GS 基址仍为 0（无每 CPU GS 区）；GS-3 之前热路径 CPU 索引继续用 CPUID/APIC ID。
- `SYS_TASK_SET_GS_BASE` 在探针关闭时返回 `ENOSYS`；该号不构成对外稳定 ABI 承诺。

## 阶段/profile 影响

不勾选 7-B 或任何阶段 7 退出条件；不改变 profile、默认 boot 事件、默认 QEMU `-smp 1`。
GS-1 完成，GS-2（异常/IRQ 入口交换与 NMI/IST 策略）为下一片；Windows M3 运行期退出仍被
GS-2…GS-4 阻塞。

## 后继关系

- 前序：[P5-3a 每 CPU GDT/TSS 与共享 IDT](2026-11-27-p5-3a-percpu-gdt-tss.md)。
- 上位：[Windows 阻断处置 §3](../specs/2026-11-27-windows-blocker-decisions.md)、
  [S7-B.1 状态分区设计](../specs/2026-11-27-stage7-smp-state-partition.md)。
- 后继：GS-2 异常/中断/IRQ 入口出口交换与嵌套/NMI 策略；P5-3b 每 CPU 可运行队列与固定放置。
