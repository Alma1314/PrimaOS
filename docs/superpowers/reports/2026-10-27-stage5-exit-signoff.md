---
id: report-stage5-exit-signoff
kind: report
authority: stage-signoff-record
status: signed-with-limits
lifecycle: current
updated: 2026-10-27
---

# 阶段 5 出口签署（SPEC-152）

> **签署记录**：2026-10-27，由项目所有者授权执行（用户指示"签署掉"）。
> 本报告是阶段 5 的签署工件与证据包；签署范围严格限于下文"签署范围"。
> 不表示完整 POSIX/Linux 兼容、内存安全证明或性能对齐，不启动阶段 6。

## 结论与范围

阶段 5 在**有界 POSIX profile v2** 范围内满足退出条件：

- [POSIX profile v2](../specs/2026-10-25-stage5-profile-v2.md) 于 2026-10-27 冻结，
  30 行全部为 `partial` 并逐行带限制；冻结 v0 的 `supported` 行（原生 ABI 握手、
  8 个 stdio 符号）继续有效；
- 交付项 S5-D1–D7 与退出条件 S5-X1–X3 按本报告范围勾选（见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)）；
- BLK-003 按"阶段 5 号段与命名范围"关闭，剩余问题转入阶段 6/7；
- 阶段 6 仍未开始；本签署不改变白皮书的阶段定义。

**本次签署不是**：不是"兼容一切 POSIX"、不是"阶段 6 入口就绪"、不是组织外第三方认证。

## 基线与输入

| 项 | 值 |
|---|---|
| 基线 | `6f0dfea`（master，本报告写作时未提交的工作树含本次签署文档与两处 C 夹具修改） |
| 工具链 | nightly-2026-07-30（rustc 1.99.0-nightly），targets `x86_64-unknown-none`/`uefi` |
| host | Linux x86_64；GNU binutils 2.42；QEMU 8.2.2 |
| guest | BIOS profile、qemu64、单 CPU、TCG、128 MiB |
| 输入工件 | 冻结 profile v0/v2、[兼容台账](../compat/compat-state.md)、[逐入口表](../compat/stage5-working-api.md)、[musl 快照锁](../compat/musl-snapshot.lock.toml) |
| 前序证据 | [统一验收报告](2026-10-25-stage0-5-gap-followup.md)、[POSIX std 文件 I/O](2026-10-27-posix-std-file-io-report.md)、[M21 未签署评审](2026-10-23-stage5-m21-exit-review.md) |
| 审查记录 | [实现质量独立审查](2026-10-27-implementation-quality-review.md)（记录了两处环境依赖缺陷） |

## 签署范围

### 1. 冻结 profile

[POSIX profile v2](../specs/2026-10-25-stage5-profile-v2.md) 自 2026-10-27 起为阶段 5 的
冻结契约；变更须发布 v3 并重新评审。v0/v1 保留为历史契约，不删除、不重编号。

### 2. 明确不接受（unsupported）

以下内容在本阶段不承诺，且不因本签署获得任何默认支持：

- 动态链接、`ld.so`、`PT_INTERP`、共享库与 GNU 运行时符号版本解析；
- 非环回网络：NIC、TCP、DNS；持久化存储、硬链接；
- `sigaltstack`、实时信号排队、完整 job control、`VTIME`/流控；
- 多线程 `fork`、`pthread_atfork`；每 LibOS 根视图与多 personality 并存；
- SMP、真机、性能保证（不设 250 ns 目标）、全包 Miri、`#AC` 机器见证；
- 任意 POSIX 参数组合；profile 行按 `partial` 解释，`supported` 仅限已冻结测试。

### 3. 决策门处置（D1–D10）

| 门 | 处置 | 边界 |
|---|---|---|
| D1 | 接受 v2 冻结为阶段 5 profile | v0/v1 为历史；修改须发 v3 |
| D2 | 接受：Prima 号段 1–72、52 保留、Linux 号仅在适配层 | 不承诺稳定外部 ABI |
| D3 | 接受：单 Process 多 Task、线程共享能力视图、fork 独立视图 | 多线程 fork 仍不支持 |
| D4 | 接受：阶段 5 静态链接；动态链接保留阶段 7 候选，不采用 DOC-002 的 ld.so P0 | 阶段归属冲突以此为准 |
| D5 | 接受：用户态 loopback IPv4/UDP 子集 | 不提供 TCP/DNS/NIC |
| D6 | 接受：用户 2026-10-26 选择 A（原生单线程 std 子集） | B 不在本轮 |
| D7 | 接受：有界信号集合与 8 层现场 | 无 altstack/实时排队；`#AC` 仅 host |
| D8 | 接受：RAM VFS 权限/umask/rename/符号链接/stdio | 无硬链接/持久化 |
| D9 | 接受：性能不设出口门槛，只记录实测 | 队列 IPC 通过≠直接快路径完成 |
| D10 | 接受：阶段 6 继续未开始；Linux persona 留用户态 | 本签署不构成 Linux ELF 证据 |

### 4. BLK-003 关闭范围

BLK-003 在原表述（号段/命名/错误映射、静态链接、线程、护栏）上按阶段 5 范围关闭：
原生调用号 1–72 以 `prima-abi` 为定义源、52 保留；错误码只登记实际实现；静态链接与
persona/符号护栏按[静态程序契约](../specs/2026-10-26-static-program-contract-design.md)执行。
**剩余部分**（动态装载、Linux 号在用户态 LibOS 的完整映射、第二 personality）不作为阶段 5
缺口，转入阶段 6/7 的计划与裁决（见[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)）。

## 命令与结果

签署前在基线 `6f0dfea` + 本次两处 C 夹具修改上执行；原始日志保存在运行环境临时目录，
失败摘要文件 `target/prima-os/full-suite-errors.log` 在最终通过后为空。命令与观察值如下：

| 命令 | 结果 | 证据范围 |
|---|---|---|
| `cargo test --workspace` | PASS：1018 passed / 0 failed + 7 doctest | host |
| `cargo xtask lint` | PASS：fmt、clippy -D warnings、裸机构建、kernel/src 零 unsafe（32 文件） | host + 裸机编译 |
| `cargo xtask tcb` | PASS：trusted 2072 / mechanism 30223 / 6.42%，unsafe 78/0，耦合 include_bytes=0 | 源码度量 |
| `cargo xtask test` | **PASS**：`full BIOS suite: 36 programs, 72 runs`，全部 Linux 参考与 musl host 通过，退出码 0（修复夹具后的单次完整运行） | QEMU + Linux 参考 + musl host |
| `cargo xtask test-process W7` / `W7b` | PASS：linux 参考 + BIOS 双轮（夹具修复后） | host + QEMU |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base HEAD` | PASS | 文档系统 |

## 失败、反例与限制

### 1. 本次验收中发现并修复的两个环境依赖（保留失败）

**(a) musl 归档审计的 locale 依赖**（见[质量审查](2026-10-27-implementation-quality-review.md)）：
`prima-user/musl/build.py` 用英文 `":     file format "` 解析 `objdump` 输出，非英文 locale
下失败。签署工作未修该脚本；签署验收以 `LC_ALL=C` 执行，并在报告中登记为环境要求。

**(b) Linux 参考 W7/W7b 的 memfd 执行策略依赖**：
`prima-user/c/stage5_exec_fixture.h` 与 `stage5_execve.c` 原用 `memfd_create` + `fexecve`，
在内核/沙箱拒绝 memfd 执行时失败（首次完整运行记录：W7 `exec=0`、W7b `err=24320`）。
本次改为"memfd 失败时回退到未链接临时文件 + `fexecve`"，两处均为 **host 参考夹具**，
不改变 Prima 侧行为；修复后 `test-process W7` / `W7b` 双轮 PASS。
历史失败保留在本节，不改写为 PASS。

### 2. 已知限制（不影响本范围签署，但不外推）

- 历史 QEMU 超时（W8/W9/V1）根因未定位；本报告只记录本轮结果，不解释旧失败；
- `#AC` 仅有 host 路由，无机器见证；Miri 仅覆盖 authority 模型 21 项；
- 未取得 SMP 或真机证据；未测性能；未运行全包 Miri；
- 测试计数在不同批次为 1016/1018/1020：计数口径与基线不同，本报告以本轮 1018 为准；
- 21 项 Linux 参考中存在 1 项为 `default` 基础启动，文档旧称"20 项"是既有计数口径。

## 阶段与 profile 影响

- 阶段 5：状态由"进行中"改为**已满足退出条件（2026-10-27，SPEC-152）**，
  S5-D1–D7 / S5-X1–X3 按本报告范围勾选；BLK-003 关闭（范围见上）。
- 阶段 6：仍未开始；本次签署使阶段 6 具备合法入口，但不启动它（按 D10 与
  [阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)的 D11–D16 另行裁决）。
- profile：v2 冻结；v1 不再单独发布，其变更行由 v2 解释。
- 兼容台账：`supported` 面仍是 v0 的 2 行；v2 的 30 行全部 `partial`。

## 后继关系

- 前序：[M21 未签署评审](2026-10-23-stage5-m21-exit-review.md)、[统一验收](2026-10-25-stage0-5-gap-followup.md)、
  [POSIX std 文件 I/O](2026-10-27-posix-std-file-io-report.md)。
- 本次：[出口评审工件](../specs/2026-10-26-stage5-release-decisions.md)（范围来源）、
  [实现质量审查](2026-10-27-implementation-quality-review.md)（环境缺陷来源）。
- 后续：阶段 6 工作项见[阶段 6–7 细化](../plans/2026-10-27-stage6-7-refinement.md)；
  未完成的组织动作（CI、许可、benchmark）不在本签署范围，列入阶段 7 的 7-0 工程底座。
