---
id: plan-stage6-7-refinement
kind: plan
authority: execution-plan
status: proposed
lifecycle: current
updated: 2026-10-27
---

# 阶段 6–7 细化提案：从 Linux 静态 ELF 到多 LibOS 并存

> 性质：**执行计划提案**，不是阶段定义，也不产生运行证据。阶段定义以
> [架构白皮书](../../../Prima%20OS.md)为准；本计划把它拆成可审阅的工作项与判据，
> 供组织级评审取舍。计划通过不自动签署阶段。
>
> 约束：阶段 5 已于 2026-10-27 签署（[SPEC-152](../reports/2026-10-27-stage5-exit-signoff.md)），
> 阶段 6 具备合法入口，但按 D10 不自动启动；本计划的 S6-M1 仍待 D12–D16 裁决后再开工。
> 阶段 7 的候选范围仍按白皮书保持未冻结。

## 目标、范围与基线

- **目标**：把阶段 6（未修改 Linux x86_64 静态 ELF）与阶段 7（扩展与硬化）拆成带退出判据的子阶段，
  并回答一个白皮书未回答的问题：**多 personality 并存**应归入哪一阶段、排在哪一步。
- **基线**：commit `6f0dfea`；上位契约是[白皮书第 15 节](../../../Prima%20OS.md)、
  [阶段跟踪](../../../Prima%20OS%20实现阶段.md)与[静态程序契约](../specs/2026-10-26-static-program-contract-design.md)。
- **相关问题**：`BLK-003`（ABI 归属）仍 Open；`ld.so` 归属在 DOC-002 与白皮书之间冲突
  （阶段跟踪已标注）；[质量审查](../reports/2026-10-27-implementation-quality-review.md)
  记录的门禁缺陷（musl 审计 locale 依赖）不影响本计划，但会阻塞验收命令的跨环境复现。
- **排除项**：本计划不改白皮书的阶段定义、不发布 profile、不承诺任何估算为测量值；
  工作量估算按"单人 + 现有 QEMU 双轮验收 + AI 辅助"口径给出，属判断。

## 现状与缺口

| 已具备（可复用） | 缺口 |
|---|---|
| `ET_EXEC/EM_X86_64/无 PT_INTERP` 的 ELF64 解析、`argc/argv/envp/auxv` 构造（[exec.rs](../../../kernel/src/exec.rs)，459 行，`MAX_EXEC_SEGMENTS=4`） | 段上限与 `PT_GNU_STACK`/`PT_GNU_RELRO` 处理；auxv 完整性（`AT_PHDR`/`AT_ENTRY`/`AT_RANDOM` 等） |
| 任务帧保存/恢复、`iretq` 跳板、信号阻塞与 `RESTART` 重放（[syscall.rs](../../../kernel/src/syscall.rs) `prepare_signal_resume`） | **同步 syscall 转发**：`SYS_EXC_*` 只是异步通知；`SYS_UPCALL_CONTROL` 实为信号管道（sigaction/mask/pending/kill） |
| 异常订阅/等待与用户态恢复帧（[exc.rs](../../../kernel/src/exc.rs)） | 请求-回复语义：参数内存授权、回复一次、超时/取消、LibOS 崩溃时请求者的确定性状态 |
| musl 适配层（[prima_syscall.c](../../../prima-user/c/prima_syscall.c)）把 Linux 号段映射到原生 syscall | 该适配层**编译期链接进程序**；未修改二进制不能链接它，必须改为**运行时 LibOS** |
| Linux 参考差分脚手架（[linux.rs](../../../xtask/src/linux.rs)，pinned 6.8 内核 + busybox initramfs，目前仅 `cow-write-fault` tracepoint） | 逐 syscall 差分矩阵、白名单程序集、性能基线 |
| TLS（`SYS_TASK_SET_FS_BASE`）、futex 式（`SYS_WAIT_WORD`）、fork/clone/线程基础 | Linux 细节：`CLONE_*` flags、`set_tid_address`、`sigaltstack`/`siginfo`、`wait4`/rusage |
| persona 声明在用户态 `.note.prima.abi`（内核不认识 OS） | 决策：Linux LibOS 是**常驻服务**还是**每程序加载器** |
| 单核全局状态（[bootstrap.rs](../../../kernel/src/bootstrap.rs) 单个 `BOOTSTRAP_STATE`）与按 task 索引的全局调度数组 | SMP 所需的 per-CPU 状态、IPI、TLB shootdown、AP 启动 |

**关键判断**：阶段 6 的难点不在 ELF 解析，而在"未修改二进制"与"内核零 Linux 语义"（S6-X3）
两条约束的交点——内核必须把 `syscall` 指令变成 OS 中立的请求投递给用户态 LibOS，
并保证参数访问、回复一次、可中断、可计量。

## 阶段 6 工作项与依赖

| ID | 工作项 | 依赖 | 验收输入与预期观察 | 估算（人周） |
|---|---|---|---|---|
| S6-M1 | 中立递送（同步 call/reply） | 无 | ring3 服务截获另一 ring3 进程的系统调用并改写 `rax`；重复投递/IPC 中断/服务崩溃负例；TCG 口径往返延迟 ≤ 5 µs | 3–5 |
| S6-M2 | 未修改静态 ELF 加载 | S6-M1 | sha256 固定的未修改静态 busybox 子集启动，`write` 经 LibOS 生效；动态 ELF/坏 ELF/W+X 段负例确定性失败 | 2–3 |
| S6-M3 | Linux LibOS profile v1 | S6-M1、S6-M2 | 逐 syscall 在参考 guest 与 Prima 上同源运行，结果与 errno 一致；未支持 syscall 返回 `ENOSYS` 且有审计记录 | 6–10 |
| S6-M4 | Linux 进程/线程/信号语义 | S6-M1、S6-M3 | 未修改静态 pthread 程序 create/join；fork+exec 子进程；信号打断阻塞调用并 `EINTR` | 4–6 |
| S6-M5 | 差分与验收工程 | S6-M2–M4（并行） | S6-X1–X4 每条映射到具体命令与观察值；完整套件单次通过；验收并行化 | 2–3 |
| S6-M6 | 收口与签署包 | S6-M1–M5 | profile v1 冻结（supported/partial/unsupported）；证据索引与阶段跟踪更新；组织级评审通过 | 1–2 |

**S6-M1 详细范围**（全阶段最易低估的一项）：

1. `syscall_event` 能力对象：订阅者获得 `(task, frame)` 句柄，且只在授权范围内可见；
2. 同步路径：trap → 冻结 task → 唤醒 LibOS → LibOS 经 vspace 访问能力读写请求者内存 →
   提交返回帧 → 恢复 task；
3. 回复一次性（exactly-once）与重复投递负例；
4. 中断与取消：请求者收到信号时按 `RESTART` 语义重放或被 `EINTR` 打断；LibOS 死亡时
   请求者得到确定性失败而不是永久阻塞；
5. 延迟测量与口径记录（TCG 下单独列出，不与实机数据混写）。

**S6-M5 必须与 M3/M4 并行启动**，否则验收工程会吞掉最后 20% 的时间。
现有验收是 36 组 × 2 轮、30 秒门限；阶段 6 会显著增加用例数，需要先做分组并行与确定性 trace。

**阶段 6 总估算：约 18–29 人周（4–7 个月）**，长尾集中在 S6-M3。

## 阶段 6 前置裁决

以下三项不裁决，S6-X1/X2 无法定义，工作项会反复返工：

| 项 | 冲突或缺口 | 影响 |
|---|---|---|
| ~~`ld.so`/动态链接归属~~ | **已裁决**：D4 与 SPEC-152 确认阶段 5 静态链接，动态链接留阶段 7，不采用 DOC-002 的 ld.so P0 | 阶段 6 无需 `PT_INTERP`；若后续要求动态装载须先改阶段定义 |
| 白名单定义 | `S6-X1` 的"白名单中的静态 ELF"未指定成员与版本 | 建议冻结为 busybox 的 6 个子命令 + 2 个自写探针，附 sha256 |
| 无熵条件下的 `AT_RANDOM`/canary 策略 | 项目明确无安全熵，而 `AT_RANDOM`、栈 canary、`HashMap` 都需要 | 需要书面弱化声明（确定性/可审计优先），否则 M2/M4 卡住 |

## 阶段 7：结构问题与子轨

白皮书阶段 7 的候选是**扩展与硬化**（SMP、IOMMU/驱动、动态链接、计量配额、选择性撤销、
故障恢复、第二架构），但**列表中没有项目的核心研究假设——多 personality 并存**；
阶段跟踪也记载"每 LibOS 根及多 personality 仍未实现"。

若照原列表推进，会出现"内核越来越硬、研究主张始终未验证"的局面：SMP 与驱动都很贵，
做完之后若发现多 LibOS 语义设计不对，返工成本最大。因此本提案把阶段 7 拆成
**7-0 与 7-A…7-H 九条子轨**，各自带退出判据。

| ID | 子轨 | 优先级 | 交付物 | 退出判据 | 依赖 | 估算（人周） |
|---|---|---|---|---|---|---|
| 7-0 | 工程底座 | P0 | CI、验收并行化、benchmark、release artifact、开源许可 | `test/lint/tcb/docs check` 在 CI 上绿；5 项基准入库 | 无 | 2–3 |
| 7-A | 多 personality 并存（研究核心） | P0 | 每 LibOS 独立根命名空间/地址空间视图与独立能力空间；两个互不信任 LibOS 同时运行；共享服务经能力授权；跨域访问被拒；生命周期由服务管理器托管 | QEMU demo：两 personality 各自运行、共享日志、跨域请求 `EACCES`、单侧崩溃不影响另一侧；审计链可重建 | 无（单核可验证） | 6–10 |
| 7-B | SMP | P0 | per-CPU 状态、调度分片、IPI、TLB shootdown、AP 启动、锁层次 | 2/4 vCPU 启动；并行 fork/exec 压力无丢唤醒；TLB shootdown 正确性；扩展性曲线 | 7-A 的按域状态划分 | 8–14 |
| 7-C | 资源计量与配额 | P1 | 内存/CPU/句柄计量挂到 capability；超限确定性失败与审计 | 单 LibOS 耗尽自身配额不影响他人 | 7-A | 3–4 |
| 7-D | 选择性撤销 | P1 | 撤销域、capability 子树批量失效 | 服务重启后旧句柄全部 `Stale`，无关域不受影响 | 7-A、7-C | 4–6 |
| 7-E | 动态链接 | P2 | `PT_INTERP`/ld.so、GNU 版本解析、共享库映射 | 未修改动态 ELF 白名单运行（若阶段 6 未裁决归此） | 阶段 6 | 8–12 |
| 7-F | IOMMU 与用户态驱动/持久化 | P2 | virtio-blk/net、DMA 能力、IOMMU 域、磁盘后端 | 从盘启动；文件跨重启持久化；网卡收发 | 阶段 5/6 | 10–16 |
| 7-G | 故障恢复与监督 | P2 | 服务监督、重启协议、崩溃域、配额与审计恢复 | 注入崩溃后服务自愈，配额与审计一致 | 7-A、7-D | 4–6 |
| 7-H | 第二架构 | P2（最后） | RISC-V/AArch64 的 `prima-ostd` + COSIR 细化 | 同一 POSIR 规范在第二架构通过 `COMPAT-TEST`；语义冻结后才做 | 7-A–7-D 语义冻结 | 12–20 |

**推荐顺序**：

```text
7-0 ──► 7-A ──► 7-B ──► 7-C/D ──► 7-G ──► 7-E/F ──► 7-H
        (单核可验证)  (可信度)   (安全闭环)  (可用性)   (可移植性)
```

- **7-A 必须最先**：它是唯一无法被"内核直接实现 Linux ABI"路线复制的差异点（参见
  [framekernel 差距分析](../reports/2026-08-16-framekernel-gap-analysis.md)的 H4 反方基线）；
  单核即可验证；越晚做，per-LibOS 根视图、服务发现与崩溃恢复的架构债越大。
- **7-B 是可信度门槛**：白皮书成功标准不含 SMP，但任何评审都会问多核；它依赖 7-A 的状态划分，
  在单个全局 `BOOTSTRAP_STATE` 上直接加锁会先死在性能上。
- **7-E/F 是可用性而非研究价值**：动态链接与真实驱动都很贵，应排在研究主张验证之后。
- **7-H 最后**：语义冻结前做第二架构会返工两遍。

**建议在阶段 6 动工前做 7-A 的探索原型**：用现有 POSIX 子集造两个独立根视图、
一个共享 log 服务与跨域拒绝。理由是它便宜（单核、复用现有能力机制），
且如果做不出来，说明架构假设有问题——那会改变阶段 6 的设计，越早知道越好。

## 需要裁决的决策项

| ID | 问题 | 影响 |
|---|---|---|
| D11 | ~~`ld.so`/动态链接归阶段 5/6/7 哪一阶段~~ | **已由 D4 裁决并写入 SPEC-152**：留阶段 7 |
| D12 | 阶段 6 白名单成员与版本（可验证的"通过"定义） | S6-X1 能否关闭 |
| D13 | 无熵条件下 `AT_RANDOM`/canary 策略与书面弱化声明 | S6-M2/S6-M4 可行性 |
| D14 | 多 LibOS 是否升为 7-A 并前移（含是否先做探索原型） | 研究主张的验证时点 |
| D15 | SMP 前是否先做"状态按 CPU 分区"重构 | 7-B 的成本量级 |
| D16 | 阶段 7 是否拆成 7-0…7-H 子阶段（各自退出条件），而非笼统"扩展与硬化" | 阶段签署的可操作性 |

这些是组织级/作者裁决，不是本计划能自行关闭的事项；裁决后应记入阶段跟踪的决策日志，
并在白皮书修订相应阶段定义。

## 验证与停止条件

本计划本身不产生运行证据。评审时可用的最小检查：

| 检查 | 命令 | 预期 |
|---|---|---|
| 文档治理 | `python3 scripts/docs.py check` | PASS，链接/锚点/元数据完整 |
| 现状核对 | 人工核对表 1 与 `kernel/src/exec.rs`、`kernel/src/syscall.rs`、`xtask/src/linux.rs` | 表 1 的"已具备"不得超出代码事实 |
| 门禁可用 | `cargo xtask lint`、`cargo test --workspace` | 仅证明基线可跑，不证明计划正确 |

**停止条件**：若 D11–D16 中任一裁决把"多 LibOS"移出阶段 7 或大幅改变 `ld.so` 归属，
本计划即作废并指向后继计划，不在旧计划内滚动维护实现事实；若阶段 6 的 S6-M1 无法在
单核 QEMU 上取得"截获系统调用并返回"的见证，则阶段 6 整体应暂停并回到架构假设评审。

## 收口与生命周期

- 实施后：为每个完成的子阶段补 `reports/` 报告并在此链接；本计划转 `historical`。
- 未完成项：在阶段跟踪维护，不在本计划内改写进度。
- 与既有文档的关系：本计划不取代[阶段 5 收尾路线图](2026-10-22-stage5-completion-roadmap.md)
  （后者是 M12–M21 与 S6-A–E 的执行记录），只细化 S6/S7 的拆分与排序；
  白皮书阶段定义在裁决后另行修订。
