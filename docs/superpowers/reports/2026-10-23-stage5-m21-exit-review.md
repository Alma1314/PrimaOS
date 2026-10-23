# 阶段 5 / M21：出口评审（未签署）

**日期**：2026-10-23
**方法**：按路线图 [M21](../plans/2026-10-22-stage5-completion-roadmap.md) 的
“按证据判定”，逐项核对 S5-D1–D7/X1–X3 与退出条件。**本报告是技术复核，
不是组织级签署**。

## 1. 全量重跑

| 命令 | 结果 |
|---|---|
| `cargo test --workspace` | 全绿（host 模型用例 427） |
| `cargo xtask lint` | fmt/clippy/bare-metal/零 unsafe 全 PASS |
| `cargo xtask test` | **114 条 PASS，退出码 0**（W1–W9 + 既有轮次双轮） |
| `cargo xtask compat` | **PASS compat cow-write-fault** |

## 2. 条件核对

| 条件 | 结论 | 依据/缺口 |
|---|---|---|
| S5-D1 冻结且版本化的 POSIX profile | **未满足（待组织级）** | profile v0 已冻结、v1 delta 待评审；**profile v2 未编写** |
| S5-D2 libc 适配 | 部分满足 | musl 补丁/适配层/pthread/futex/wait 可用；信号掩码、EINTR、完整 POSIX 未做 |
| S5-D3 fd/路径/线程/网络子集 | 部分满足 | VFS、线程、loopback UDP 有双轮证据；边界已登记 |
| S5-D4 逐 API 语义映射矩阵 | 满足（当前子集） | compat-state §6 + symbol-trace §5（M20） |
| S5-D5 与 Linux 差分矩阵 | 部分满足 | W4/W6/W8/W9 等有 Linux 同源；W7/W7b（静态 ELF）无 Linux 参考 |
| S5-D6 Rust target 与 core/std | **未满足** | 本阶段未实现 `x86_64-prima` 自定义 target 与 core/alloc/std 子集（M20 只完成矩阵部分） |
| S5-D7 兼容性护栏 | 部分满足 | ABI 1.0 握手/台账；persona/符号版本未见 |
| S5-X1 每项 API 有映射和可重复测试 | 部分满足 | 已实现子集均有测试；未实现项见 §3 |
| S5-X2 每项 API 明确 supported/partial/unsupported | 满足（当前子集） | compat-state §6 状态列 |
| S5-X3 选定开源程序通过冻结功能集 | **证据就绪、待组织级复核** | sbase echo E1–E6、cat C1、ls L1–L3 双轮 + Linux 参考 |

## 3. 未满足项与已知限制

- **S5-D6**：Rust target/core-std 未实现（M20 未覆盖）；
- **profile v2** 未编写；
- `ISSUE-FORK-CONTINUATION-001`：`syscall(57)` 的 fork 子进程返回值损坏（W6 用原生 51）；
- fork 子任务的 rootfs 按任务授权限制（子任务内 `execve(path)` 阻塞，W7b 改为父任务预读）；
- 信号掩码/`EINTR`/`SA_RESTART`/`SIGCHLD` 自动通知未做；
- pthread 取消/分离/属性、TTY 字间定时/流控、VFS 持久化未做；
- S5-X3/S5-D1 的组织级复核未进行。

## 4. 结论

- **阶段 5 未签署**：技术证据矩阵基本就绪（114 条双轮 PASS、compat PASS），
  但 S5-D1/D6、profile v2、S5-X3 组织级复核与上述限制未满足；
- **阶段 6 入口（D10）不启动**：S6-A（静态 ELF 装载器）所需的内核原语
  （`exec::plan`/`build_stack`、`SYS_EXEC_REPLACE`）已具备，但按路线图应在
  M21 签署后启动；
- 建议的下一步顺序：S5-D6 Rust target → profile v2 → 组织级复核 →
  修复 `ISSUE-FORK-CONTINUATION-001`/信号掩码等 → S6-A。

## 5. 本报告不代表

- 不表示阶段 5 退出条件满足；
- 不构成组织级签署。
