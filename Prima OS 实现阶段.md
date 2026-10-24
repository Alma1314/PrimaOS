---
id: project-status
kind: reference
authority: current-status
status: maintained
updated: 2026-10-24
---

# Prima OS 实现阶段

> 文档整理更新：2026-10-23。当前阶段结论依据已有报告；本次文档/Rust回归见[整理报告](docs/superpowers/reports/2026-10-23-documentation-restructure-report.md)，不改变组织级签署。
> 架构见[白皮书](Prima%20OS.md)，抽象语义见[原语规范](COSIR%20原语规范.md)。
> [文档中心](docs/README.md) · [主题地图](docs/architecture/topics/README.md) · [证据规则](docs/architecture/evidence.md)

## 维护规则

- 本文是当前进度、退出条件、活跃阻塞与证据指针的唯一汇总来源；计划记录当时步骤，报告保存当时结果。
- 阶段状态为未开始、进行中、阻塞或已满足退出条件；设计/技术验收不自动构成组织级签署。
- 勾选前必须有可访问工件及相应范围证据。host、QEMU、Lean、差分不能互相替代；失败与反例留在报告。
- 阶段定义先改白皮书，语义先改原语规范。文档整理不代替 ABI/profile 裁决。

## 当前快照

| 项目 | 当前结论 | 依据与范围 |
|---|---|---|
| 阶段 0–4 | 已满足各自退出条件 | [SPEC-145](docs/superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md)、[146](docs/superpowers/reports/2026-10-17-stage1-exit-review-spec146.md)、[147](docs/superpowers/reports/2026-10-17-stage2-exit-review-spec147.md)、[148](docs/superpowers/reports/2026-10-17-stage3-exit-review-spec148.md)、[149](docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md)；阶段0限规范文本，各实现/证明范围分别解释 |
| 当前阶段 5 | 进行中；M21 技术评审完成，未签署 | [M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)：S5-D1/D6、profile v2、组织级复核与已知限制未收口 |
| 兼容范围 | 逐项读取当前台账，不宣称完整 POSIX/Linux 兼容 | [compat-state](docs/superpowers/compat/compat-state.md)、[symbol-trace](docs/superpowers/compat/symbol-trace.md)、[M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md) |
| 最近验证记录 | musl 后继 Rust 876 项、lint、4 个 host 检查、F1/W4 定向双轮和沙箱外全量 62 轮 BIOS PASS；沙箱内 UDP EPERM 单独保留 | [musl 后继报告](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)；[前次 VFS 超时记录](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)保留；历史 M21 的 427/114 PASS 只属于当时版本 |
| 阶段 6 | 未开始；D10 入口未启动 | 已有 exec_replace 机制不等于未修改 Linux ELF 路线已完成 |
| 阶段 7 | 未开始；范围未冻结 | SMP、动态链接、第二架构等仍为候选 |
| TCB/容量 | 以对应版本测量为准 | 当前值运行 `cargo xtask tcb`；内核零 unsafe 规则不变 |

## 证据索引

| 范围 | 入口 | 如何解释 |
|---|---|---|
| 抽象状态与研究目标 | [规范](COSIR%20原语规范.md)、[白皮书](Prima%20OS.md) | 预期语义与研究假设，不是实现证明 |
| 阶段0–4签署 | 上方 SPEC-145–149 | 以各自退出条件及证据范围为限 |
| 签署后整改 | [SPEC-150](docs/superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md)、[151](docs/superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)、[阶段2 IPC](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)、[阶段3](docs/superpowers/reports/2026-10-18-stage3-review-remediation.md)、[1–4复核](docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md) | 旧签署后的局部修订，不抹去旧报告 |
| 原生运行时/服务 | [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md)、[主题包](docs/architecture/topics/runtime.md) | 原生应用/服务收口不等于 POSIX 完整实现 |
| 阶段5当前子集 | [M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md)、[M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)、下方里程碑表 | 分别记录已覆盖、限制与未签署条件 |
| 阶段5余项/阶段6入口 | [收尾路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md) | 计划；其中旧基线的“未实现”由后继报告限定，不当成当前摘要 |
| 历史切片与反例 | [全量目录](docs/architecture/catalog.md)、[主题阅读链](docs/architecture/topics/README.md) | 文件不因未进入精选表而失效；必须保留失败与后继 |

2026-10-18 压缩前记录保存在[历史版本](https://github.com/Alma1314/PrimaOS/blob/a434a2f0c515ac3421e999be3c436b7d7c6c5313/Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md)。本次重构前版本可用 `git show d48d5ef:Prima\ OS\ 实现阶段.md` 查看；[整理报告](docs/superpowers/reports/2026-10-23-documentation-restructure-report.md)记录陈旧摘要的修正依据。

VFS 后继实现与验证见[LibOS 基础重构报告](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)：原生打开对象、普通文件延迟回收和数据边界修复；不改变阶段出口勾选。

musl 后继实现见[调用边界与 fd/stdio 报告](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)：fcntl、fdopen/已有 append、向量 I/O、汇编边界与锁定构建；不改变阶段出口勾选。

## 已知问题与当前阻塞

| ID / 事项 | 当前口径与限制 | 证据 / 下一步 |
|---|---|---|
| VFS 历史启动超时 | 2026-10-23 多次出现 30 秒超时；2026-10-24 musl 后继版本的 V1/V2/V3 均已双轮通过，未宣称历史超时根因已修复 | [历史记录](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)、[musl 后继验证](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)；保留 30 秒门限及所有断言 |
| BLK-003 | 仍 Open：候选基座库的 ABI/profile 假设未全部裁决；工作实现不等于组织级采纳 | [修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md)、[路线图 D1–D10](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)、M21；保留号段/命名/错误映射与阶段归属待决项 |
| S5-D1 / S5-D6 / S5-X3 | profile v0 待组织级确认、v1 delta 待评审、v2 未编写；Rust target/core-std 未实现；开源程序证据就绪但待组织级复核 | [M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)；未签署、不启动阶段6 |
| ISSUE-FORK-CONTINUATION-001 | Linux 风格 `syscall(57)` 包装的 fork 子返回值损坏；W6 用原生 51 绕过 | [M17b-4](docs/superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)；修复包装续执行并补对应探针 |
| fork 子任务 VFS 授权 | 子任务内直接 execve(path) 受按任务授权限制；W7b 使用父任务预读镜像 | [M17c-4](docs/superpowers/reports/2026-10-23-stage5-m17c4-execve-vfs.md)；不能据此宣称完整 execve |
| ISSUE-FORK-CAPS-001 | fork 父子共享能力空间，子 close 服务句柄影响父，存在 POSIX close 隔离偏差 | [M17设计](docs/superpowers/specs/2026-10-23-stage5-m17-fork-exec-design.md)；需评估 per-Process 空间/多 holder 方案 |
| ISSUE-PTHREAD-CAPS-001（工作决定） | 同 Process 线程共享 leader 能力空间，组织级语义确认仍待完成 | [M15设计](docs/superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)、[M15b-2a](docs/superpowers/reports/2026-10-23-stage5-m15b2a-thread-create.md) |
| 信号/线程未覆盖项 | 无掩码、siginfo、EINTR、SA_RESTART、SIGCHLD 自动通知；pthread 取消/分离/属性未验证 | [M18b](docs/superpowers/reports/2026-10-23-stage5-m18b-sigaction.md)、[M16b-3](docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)、M21 |
| ISSUE-TTYIN-001（部分关闭） | 输入子集已通过；有读者时仍轮询 COM1，尚未由 WaitObject/IRQ 替换 | [M14](docs/superpowers/reports/2026-10-23-stage5-m14-tty-input.md)；WaitObject 存在不自动关闭此偏差 |
| ISSUE-START-ARGS-001 | M12 独立参数载体组合的启动回归未定位；固定探针证据不能代替任意 argv 闭环 | [M12](docs/superpowers/reports/2026-10-22-stage5-m12-c-startup.md)；保留调度断言，单独复现 |
| ISSUE-CAP-001 | host 模型仍缺完整位置/溯源；不能当成当前内核缺陷 | [模型缺口](docs/superpowers/reports/2026-08-17-capability-derivation-model-gap.md)、[Lean映照契约](lean/README.md)；parent/CDT 后续补强不消除 HeldBy 边界 |
| ISSUE-AUTH-001 | SPEC-146 接受的 image/vspace 细化分叉，不阻断 S1-X2 | 保留与§14完整细化的区别；新装载路径另按阶段5报告取证 |
| ISSUE-COW-001/002 | 抽象 COW 的映射/并发责任澄清，不是自动阻断项 | [SPEC-015](docs/superpowers/specs/2026-08-14-cow-refinement-design.md)；扩大语义时再验证 |

### 已收口问题的后继入口

- BLK-001 已关闭；Lean 证明范围见 lean/README。BLK-002 仅形成细化设计，不能外推为机器证明。
- BLK-004 容量门槛已关闭：[C ELF 见证](docs/superpowers/reports/2026-10-18-stage5-c-elf-qemu.md)。
- ISSUE-IPC-001 已由[阶段2整改](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)关闭。
- ISSUE-THREAD-CAPACITY-001 已由[M15b-2b](docs/superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md)关闭；保留 task_create 满容量负例。
- ISSUE-PTHREAD-CLONE-001 的 clone/join 阻塞由[M16b-3](docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)后继收口；M16b-1/2 的失败是历史记录。
- ISSUE-FORK-COW-001 的 COW 机制路径已由[M17b-4](docs/superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)取得 W6 见证；包装续执行与能力隔离问题继续单独跟踪，不将它们一并关闭。

## 决策日志

| 日期 | 决定与当前影响 | 来源 |
|---|---|---|
| 2026-10-23 | M21 技术出口评审未签署；阶段6 D10 不启动 | [M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md) |
| 2026-10-22/23 | M12–M21 与 S6-A–E 按路线图推进；D2/D3/D5/D8 等工作决定不等于组织级关闭；号段从既有接口追加、Prima 命名 | [路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)、M15/M17/M19设计 |
| 2026-10-18 | 阶段5先冻结 profile 与 ABI 归属，再扩展 libc；DOC-002 仍属候选、非已评审权威约束 | [修订计划](docs/superpowers/plans/2026-10-18-stage5-revised-plan.md) |
| 2026-10-17 | 阶段0–4按 SPEC-145–149 各自范围签署，阶段0限文本 | 各退出复核报告 |
| 2026-10-23 文档整理 | 移除重复的过程堆叠，按后继证据修正陈旧摘要；不改退出勾选、不新增技术结论 | [文档整理报告](docs/superpowers/reports/2026-10-23-documentation-restructure-report.md) |

## 阶段 0 — 语义基线

**状态：已满足退出条件（2026-10-17，SPEC-145；规范文本范围）**

### 交付检查

| ID | 交付物 | 状态 | 证据 |
|---|---|---|---|
| S0-D1 | 一致的对象能力状态模型 | [x] 已定义 | [原语规范](COSIR%20原语规范.md) |
| S0-D2 | 操作、错误、全局不变量和非目标 | [x] 已定义 | 规范及 SPEC-145 文本复核 |
| S0-D3 | 研究假设与反例标准 | [x] 已定义 | [白皮书](Prima%20OS.md)§3/§14 |
| S0-D4 | x86_64 状态到 POSIR 的细化草案 | [x] 设计，未完成内核细化证明 | 规范§20与[细化设计](docs/superpowers/specs/2026-08-07-blk002-refinement-design.md) |

### 退出条件

| ID | 条件 | 满足 | 证据与限制 |
|---|---|---|---|
| S0-X1 | 无已知且未记录的生命周期空洞 | [x] | SPEC-142 未通过 → [SPEC-143](docs/superpowers/reports/2026-10-17-stage0-review-remediation-spec143.md)/[144](docs/superpowers/reports/2026-10-17-stage0-re-review-spec144.md)整改 → [SPEC-145](docs/superpowers/reports/2026-10-17-stage0-frozen-re-review-spec145.md)文本签署；旧“暂不勾选”是前序状态 |
| S0-X2 | 每项操作有明确成功与失败语义 | [x] | SPEC-145 对规范§19清单的文本复核；不等于所有原语已实现或证明 |
| S0-X3 | 形式化工具能够表达并执行状态与迁移 | [x] | [Lean设计](docs/superpowers/specs/2026-08-18-blk001-lean-formal-model-design.md)、[映照契约](lean/README.md)；只覆盖明确子集 |

前序未通过、整改与 CDT 证明链见[语义主题](docs/architecture/topics/semantics.md)，不将签署外推为完整机器细化。

## 阶段 1 — 启动与最小平台层

**状态：已满足退出条件（2026-10-17，SPEC-146）**

### 交付检查

- [x] S1-D1：QEMU 可重复启动；证据：TEST-001、TEST-002、TEST-003（固定 BIOS profile 连续两次）。
- [x] S1-D2：串口结构化输出与确定性退出；证据：IMPL-001、TEST-001、TEST-002、TEST-003。
- [x] S1-D3：物理内存发现；证据：页帧清单与保留区域测试（SPEC-017）。
- [x] S1-D4：初始页表、GDT/IDT 与 trap 框架；证据：异常和权限测试（SPEC-018）。
- [x] S1-D5：Bootstrap 创建 init Task；证据：SPEC-020 初始对象和 Capability 快照。

### 退出条件

- [x] S1-X1：脚本可无人值守启动并退出 QEMU；证据：TEST-001、TEST-002、TEST-003。
- [x] S1-X2：init 只获得规范定义的初始 Authority。证据：SPEC-061/TEST-008（image Segment、空 Namespace、结构性根、init `Ready→Running`、初始 Audit、`booted`/`AlreadyBooted`、入口范围前置、权限 bitset 去通配符；QEMU 实测 `authority_ready`）。**已勾选（SPEC-146，2026-10-17）**：`0b63bc1` 上复核 `create_state` 的 3 条根/权限/root_ns/Ready→Running/Audit/AlreadyBooted/入口前置；`ISSUE-AUTH-001` 的 §14 image/vspace 偏差按显式细化分叉接受，不阻塞 Authority。
- [x] S1-X3：活动 Segment 与空闲页帧满足唯一归属不变量。证据：SPEC-061/TEST-008（`FrameRegistry` 三分类、带标签取帧、守恒等式 QEMU 实测 `conserved=1`）。**已勾选（SPEC-146，2026-10-17）**：`FrameRegistry` 逐帧 image 向量 + owner 表 + 保留页表帧；`check_conservation`/`check_disjoint`；QEMU `frames_ready` 断言 `conserved=1/disjoint=1`；`ISSUE-FRAME-001/002` 关闭。

**阶段 1 关闭依据**：SPEC-061/TEST-008 的实现与 QEMU 证据 + SPEC-146 复核签署。S1-X2/S1-X3 已勾选；`ISSUE-AUTH-001` 的两条 §14 image/vspace 细化分叉保留为阶段 2/4 缺口，不阻塞 Authority/帧归属条件。

补充整改见 [SPEC-150](docs/superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md) 和 [SPEC-151](docs/superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)；后者关闭 4 项遗留问题。

## 阶段 2 — Capability、Segment、Task 与 IPC

**状态：已满足退出条件（2026-10-17，SPEC-147）**


阶段 2 的实现过程及历史缺口见 SPEC-062–075 报告；签署依据为 [SPEC-147](docs/superpowers/reports/2026-10-17-stage2-exit-review-spec147.md)。带 Capability 的阻塞直投已按[签署后整改](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)复验。

### 交付检查

- [x] S2-D1：不可伪造且不复用的句柄表示。**已勾选（SPEC-147，2026-10-17）**：`Handle` encode/decode + generation + 槽索引上界校验；stale/畸形句柄 host 单测。
- [x] S2-D2：Capability 派生、移动、drop 与 stale 检测。**已勾选（SPEC-147，2026-10-17）**：`SYS_CAP_DERIVE`/`SYS_CAP_DROP`、权限收窄、结构性根保护、stale token 可 drop；host 单测。
- [x] S2-D3：Segment create/map/unmap/destroy。**已勾选（SPEC-147，2026-10-17）**：`SYS_MEMSEG_*`、真实 PTE 装/卸、空表帧回收、帧归还、新段清零；`memseg_ready` QEMU。
- [x] S2-D4：Task create/yield/destroy 与单核调度。证据：SPEC-065/TEST-012（完整 §8.1 `SYS_TASK_CREATE`、§8.2 `SYS_TASK_DESTROY`、`SYS_YIELD`；QEMU 实测 `task_ready`）。**已勾选（SPEC-147，2026-10-17）**：`task_ready` 的 create/bad_entry/no_x/deny/destroy/again/map_dead/root_cleared 全部被 xtask 断言。
- [x] S2-D5：Send/Recv、FIFO 邮箱与阻塞恢复。**已勾选（SPEC-147，2026-10-17）**：`SYS_SEND`/`SYS_RECV`、FIFO/直投、接收者绑定授权；`ipc_ready` QEMU。 **2026-10-18 签署后整改**：ISSUE-IPC-001 已修复，ring3 `ipc_ready` 新增带 Cap 的阻塞直投见证；详见[阶段 2 复核报告](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)。

### 退出条件

- [x] S2-X1：每项成功和失败路径具有状态转换测试。证据（实现侧）：SPEC-062/063/064/065 的 host 单测 + QEMU 负例注入，调度迁移事件由 SPEC-071/TEST-018 补齐；**已勾选（SPEC-147，2026-10-17）**：`state_audit_ready`（`total=50;to_running=22;to_ready=22;to_blocked=5;to_faulted=1`）。
- [x] S2-X2：两个不可信 Task 只能经授权消息或共享 Segment 交互。**已勾选（SPEC-147，2026-10-17）**：`ipc_ready` 的 `cross=EACCES;send_denied=EACCES` 与 `task_ready` 的 `map_other=EACCES`。
- [x] S2-X3：页帧在对新 Segment 可见前清零。**已勾选（SPEC-147，2026-10-17）**：`memseg_ready.zero=0/zero2=0`；`create_segment_object` 在映射前调用 `mem::zero_frame`。
- [x] S2-X4：stale handle 不会因 ID 复用重新获得 Authority。**已勾选（SPEC-147，2026-10-17）**：generation 释放递增；`stale_handle_is_rejected_after_slot_reuse`；越界解码不 panic。

## 阶段 3 — Namespace、Timer 与 Audit

**状态：已满足退出条件（2026-10-17，SPEC-148）**

**2026-10-18 实现复核整改**：[阶段 3 复核报告](docs/superpowers/reports/2026-10-18-stage3-review-remediation.md)修复 4 项：Mount 图 DFS 栈按边数覆盖，`ns_destroy` 补齐逐条 `NsUnbound`/`NsUnmounted`，对象生命周期 Audit 按准确类型关联，派生链查询上界改为追加式历史容量。新增反例 host 测试；workspace tests、lint、BIOS QEMU 双跑通过。现行 `namespace_ready.unbinds=2`、`state_audit_ready.used=606`；SPEC-148 与 SPEC-151 报告中的旧数值保留为其当时基线。阶段 3 交付/退出条件维持已完成，其证据范围仍为 host/QEMU 有界见证。

### 交付检查

- [x] S3-D1：Namespace bind/unbind/mount/unmount/resolve。**已勾选（SPEC-148，2026-10-17）**：七个操作 + `namespace_ready` 的 binds/unbinds/mounts/unmounts/resolves 计数。
- [x] S3-D2：Mount 环检测与 stale Mount 清理。**已勾选（SPEC-148，2026-10-17）**：DFS + visited 位集；`cycles_rejected=1`/`acyclic_ok=2`；stale Mount 由 `ns_unmount` 清理。
- [x] S3-D3：绝对单调 Timer。**已勾选（SPEC-148，2026-10-17）**：`now` 由 PIT tick 推进；`InvalidTime` 保持状态不变；`timer_ready.now=4`。
- [x] S3-D4：`deadline ≤ now` 时原子立即触发。**已勾选（SPEC-148，2026-10-17）**：`timer_ready.immediate_fired=1`。
- [x] S3-D5：Capability 派生、位置变化与对象生命周期 Audit。**已勾选（SPEC-148，2026-10-17）**：`audit_ready.chain_len=2;derived=1;moves=2;created=1;stale_ok=1`；`SYS_AUDIT` 纯查询。

### 退出条件

- [x] S3-X1：所有 Namespace 解析均终止，活动 Mount 图无环。**已勾选（SPEC-148，2026-10-17）**：路径分量缩短 + DFS/visited；`cycles_rejected=1`/`acyclic_ok=2`。
- [x] S3-X2：每个 Fired Timer 恰有一次对应内核通知。**已勾选（SPEC-148，2026-10-17）**：`timer_ready.exactly_once_ok=1`。
- [x] S3-X3：Audit 能重建 Capability 派生链和位置历史。**已勾选（SPEC-148，2026-10-17）**：`audit_ready.chain_len=2;derived=1;moves=2`；查询不追加事件。
- [x] S3-X4：Task、Namespace、Message 或 Timer 容器销毁不破坏其他对象生命周期。**已勾选（SPEC-148，2026-10-17）**：按 recipient/条目清理；host 单测与 `namespace_ready` 的 c 销毁路径。

## 阶段 4 — Prima 原生运行时与服务

**状态：已满足退出条件（2026-10-17，SPEC-149）** S4-D1 Runtime API/版本协商（SPEC-084/085），S4-D2 ring3 服务管理器（SPEC-103–105、109），S4-D3 三个有状态基础服务（SPEC-106、110–112），S4-D4 用户态驱动/受控 I/O（SPEC-113/114），S4-D5 及 X1–X4 的收口证据见 [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md)。


后续硬化与边界见 [SPEC-149](docs/superpowers/reports/2026-10-17-stage4-exit-review-spec149.md) 和[阶段 1–4 实现复核](docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md)。

### 交付检查

- [x] S4-D1：原生运行时 API 与版本规则；SPEC-084/085 + SPEC-130。
- [x] S4-D2：init / 服务管理器；SPEC-103–105、109 + SPEC-130。
- [x] S4-D3：日志、Namespace 和基本存储服务；SPEC-106、110–112 + SPEC-130。
- [x] S4-D4：第一条用户态驱动或受控 I/O 路径；SPEC-113/114 + SPEC-130。
- [x] S4-D5：使用消息与共享 Segment 的原生示例应用；[SPEC-129](docs/superpowers/reports/2026-10-16-stage4-native-shared-segment-app-report.md) 的一页起点扩为 [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md) 的 8 KiB 跨页路径，QEMU 两次运行。

### 退出条件

- [x] S4-X1：不依赖 POSIX LibOS 即可自举并运行应用；SPEC-130 的独立 ring3 Rust 原生客户端和 BIOS QEMU 双次启动。
- [x] S4-X2：服务按现行 COSIR §8.1 的最小权限拆分；[SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md) 列出初始 Capability、结构性空 Namespace 与运行期额外授予。
- [x] S4-X3：共享 Segment 传 8 KiB 跨页数据；SPEC-130 的执行前取消、越界长度拒绝与恢复重试由客户端/管理器双侧和 QEMU trace 校验。
- [x] S4-X4：驱动从 ring3 向无权映射的客户端数据 VA 写入产生真实 `#PF`，驱动变 `Faulted`，无关客户端随后完成两次校验并到达 `boot_complete`；SPEC-130。

**下游义务登记**：阶段 5 的基座库栈（DOC-002）消费一批内核 ABI 语义，清单与归属见「阶段 5 · 上游前置」表与 BLK-003；这些**不属本阶段交付项**，进入阶段 5 前须逐项评审确认。
## 阶段 5 — POSIX 源码兼容

**状态：进行中，M21 未签署。** [M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md)汇总当前子集，[M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)逐项判定出口；本节不复制完整兼容矩阵。

### 里程碑证据与边界

| 里程碑 | 已取得的技术证据 | 当前限制 / 后继 |
|---|---|---|
| M0–M4 | [M0盘点](docs/superpowers/reports/2026-10-18-stage5-musl-m0-inventory.md)、[M2边界](docs/superpowers/reports/2026-10-18-stage5-musl-m2-boundary.md)、[M3 musl](docs/superpowers/reports/2026-10-18-stage5-musl-m3-base.md)、[M4文件](docs/superpowers/reports/2026-10-18-stage5-musl-m4-file.md) | 首切片不等于完整 libc；固定初始栈/单文件后端由后继扩展 |
| M5–M7 | [sbase echo](docs/superpowers/reports/2026-10-19-stage5-m5-sbase-echo.md)、[时间](docs/superpowers/reports/2026-10-19-stage5-m6-time.md)、[fd/dup](docs/superpowers/reports/2026-10-19-stage5-m7-fd-table.md) | 冻结用例与 partial 边界；S5-X3 待复核 |
| M8–M11 | [VFS](docs/superpowers/reports/2026-10-20-stage5-m8-user-vfs.md)、[metadata/cwd](docs/superpowers/reports/2026-10-21-stage5-m9-metadata-cwd.md)、[匿名内存](docs/superpowers/reports/2026-10-21-stage5-m10-anon-memory.md)、[树变更](docs/superpowers/reports/2026-10-22-stage5-m11-file-mutation.md) | 固定容量、匿名 arena 与有限文件语义 |
| M12–M13 | [启动/退出](docs/superpowers/reports/2026-10-22-stage5-m12-c-startup.md)、[链接/rename](docs/superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md)、[权限/stdio](docs/superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md) | 参数载体回归、无硬链接/持久化；exec CLOEXEC 未收口 |
| M14 | [TTY 输入 I1](docs/superpowers/reports/2026-10-23-stage5-m14-tty-input.md) | canonical/raw/EOF 子集；有读者时轮询 |
| M15–M16 | [WaitObject](docs/superpowers/reports/2026-10-23-stage5-m15a-waitobject.md)、[阻塞模型](docs/superpowers/reports/2026-10-23-stage5-m15a2-waitobject-blocking.md)、[W2](docs/superpowers/reports/2026-10-23-stage5-m15b2b-thread-slots.md)、[futex](docs/superpowers/reports/2026-10-23-stage5-m16a-futex.md)、[pthread W4/W5](docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md) | WAIT/WAKE、create/join 最小集；共享能力空间待确认 |
| M17 | [fork/COW/wait W6](docs/superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)、[exec W7](docs/superpowers/reports/2026-10-23-stage5-m17c3-exec.md)、[父预读 VFS 镜像 W7b](docs/superpowers/reports/2026-10-23-stage5-m17c4-execve-vfs.md) | 包装续执行、子任务 VFS 授权、fd close 隔离；W7/W7b 无 Linux 参考 |
| M18 | [默认终止 W8](docs/superpowers/reports/2026-10-23-stage5-m18a-signal-kill.md)、[handler/sigreturn W9](docs/superpowers/reports/2026-10-23-stage5-m18b-sigaction.md) | 无掩码/EINTR 等；前序“handler未做”已被后继限定 |
| M19 | [loopback UDP N1](docs/superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md) | 仅环回 IPv4/UDP，无 NIC/TCP/DNS |
| VFS / LibOS 基础 | [设计](docs/superpowers/specs/2026-10-23-vfs-libos-foundation-design.md)、[验证](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md) | 打开对象独立于 ABI；普通文件延迟回收；每 LibOS 根视图/会话授权、多 OS personality 仍未实现 |
| musl 后继补强 | [设计](docs/superpowers/specs/2026-10-24-musl-port-boundary-design.md)、[验证](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md) | 汇编陷入边界、fd 控制/stdio/向量 I/O、锁定构建；取消/分离、信号、fork 包装与 exec CLOEXEC 义务继续保留 |
| M20–M21 | [兼容两表汇总](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md)、[技术出口评审](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md) | 未完成 Rust target/profile v2/组织级确认，未签署 |

### 交付检查

以下保留未签署的勾选状态；“当前子集满足”不自动代签整体交付条件。

| ID | 交付物 | 勾选 | M21 当前判定 |
|---|---|---|---|
| S5-D1 | 冻结且版本化 POSIX profile | [ ] | v0已冻结、待组织级；v1提案；v2未编写 |
| S5-D2 | libc适配 | [ ] | 部分满足；musl/线程/等待已有证据，信号等仍有缺口 |
| S5-D3 | fd、路径、线程和基础网络子集 | [ ] | 部分满足；线程与网络已有最小集证据，不能再写“均未实现” |
| S5-D4 | 逐API语义映射矩阵 | [ ] | 当前子集满足：compat-state §6–7 / symbol-trace §5–6 |
| S5-D5 | Linux差分矩阵 | [ ] | 部分满足；W7/W7b无Linux参考 |
| S5-D6 | Rust target/core/std | [ ] | 未满足；M20只完成矩阵部分 |
| S5-D7 | persona、符号版本、台账与ABI护栏 | [ ] | 部分满足；ABI握手/台账已有，persona/符号版本未见 |

### 上游 ABI 与候选规格归属

BLK-003 保持 Open。原生号段以 [prima-abi](prima-abi/src/lib.rs)为定义源；42–58 的实现状态见[台账](docs/superpowers/compat/compat-state.md)和[接口说明](docs/architecture/interfaces.md)，52未分配。候选 DOC-002 的 `0x0xx`、RUXOS 命名与时延目标不自动进入 Prima 契约。

WaitObject/futex、信号、fork/exec、TTY 输入、匿名内存已经有各自的后继技术证据，不能继续以候选阶段的“未实现”表代替当前状态。直接 IPC ≤250ns、动态链接归属、Rust 范围、persona/符号版本及未收口语义仍按[路线图 D1–D10](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)与 M21 裁决；同步 call/reply 快路径和性能指标不因队列 IPC 通过而完成。

候选规格与风险走查保留在[基座库规格](docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md)和[走查记录](docs/superpowers/specs/2026-10-11-baselib-stack-walkthrough.md)。这些是设计输入，不是本仓库已接受的排期或ABI承诺。

### 退出条件

| ID | 条件 | 勾选 | 当前依据 |
|---|---|---|---|
| S5-X1 | profile每项API均有映射与可重复测试 | [ ] | 部分满足；未实现项与profile缺口见M21 |
| S5-X2 | 每项API标记supported/partial/unsupported | [ ] | M21判定当前子集满足；整体签署待收口 |
| S5-X3 | 选定开源程序通过冻结测试集 | [ ] | echo E1–E6、cat C1、ls L1–L3已有双轮/参考证据，待组织级复核 |

## 阶段 6 — Linux x86_64 静态 ELF

**状态：未开始**

### 交付检查

- [ ] S6-D1：静态 ELF 加载器与 Linux 初始栈。
- [ ] S6-D2：OS 中立的 syscall / exception 递送机制。
- [ ] S6-D3：版本化的 Linux LibOS syscall profile。
- [ ] S6-D4：所选 profile 必需的 futex、TLS 和进程环境。
- [ ] S6-D5：未修改静态 ELF 白名单与测试镜像。

### 退出条件

- [ ] S6-X1：白名单中的静态 ELF 无需修改即可运行。
- [ ] S6-X2：支持的 syscall 与参考 Linux 通过差分测试。
- [ ] S6-X3：HexArc / COSIR 不包含 Linux 专属对象或 syscall 语义。
- [ ] S6-X4：未支持 syscall 得到确定且可审计的失败。

> 注：候选规格（DOC-002）把 `ld.so` 最小动态链接闭环列为 P0、随阶段 5 交付，而白皮书把“动态链接”列为阶段 7 候选。两者的阶段归属须一并裁决，不得两处各写一套。

## 阶段 7 — 扩展与硬化

**状态：未开始；范围未冻结**

候选工作不预先视为承诺：

- [ ] SMP；
- [ ] IOMMU 与更完整的用户态驱动；
- [ ] 动态链接；
- [ ] 资源计量与配额；
- [ ] 选择性撤销；
- [ ] 更强的故障恢复；
- [ ] 第二硬件架构。

只有在阶段 0–6 的证据揭示实际优先级后，才把候选项拆成带退出条件的独立里程碑。

## 阶段晋级规则

1. 阶段状态由该阶段的退出条件决定，不由提交数量、代码行数或主观完成度决定；
2. 前一阶段未满足退出条件时，后一阶段可以做探索原型，但不能成为“当前已完成阶段”；
3. 失败测试、反例和实现偏差应写入对应报告；仍影响当前阶段的事项进入上方问题表；
4. 关闭阶段时，在签署报告中记录完整命令、工件版本和结果，并从证据索引链接该报告；
5. 架构或语义变化必须先更新权威文档，再重新评估受影响的已关闭检查项。
