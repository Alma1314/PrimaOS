---
id: project-status
kind: reference
authority: current-status
status: maintained
lifecycle: current
updated: 2026-10-27
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
| 当前阶段 5 | 进行中；统一技术验收完成，最终范围/组织级签署待确认 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)、[D1–D10评审工件](docs/superpowers/specs/2026-10-26-stage5-release-decisions.md)；D6 A已选定并验证，不重复裁决 |
| 兼容范围 | 逐项读取当前台账，不宣称完整 POSIX/Linux 兼容 | [compat-state](docs/superpowers/compat/compat-state.md)、[symbol-trace](docs/superpowers/compat/symbol-trace.md)、[M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md) |
| 最近验证记录 | POSIX std文件I/O与接收归账后继：workspace1020、lint、三模式Clippy、musl/compat/TCB通过；单次BIOS36组72/72轮PASS，含V4/I1/R1/R2/R3 | [最新后继报告](docs/superpowers/reports/2026-10-27-posix-std-file-io-report.md)；[初始69/70失败](docs/superpowers/reports/2026-10-27-posix-std-backend-report.md)保留，不倒写PASS；无新Lean证明 |
| 阶段 6 | 未开始；D10 入口未启动 | 已有 exec_replace 机制不等于未修改 Linux ELF 路线已完成 |
| 阶段 7 | 未开始；范围未冻结 | SMP、动态链接、第二架构等仍为候选 |
| TCB/容量 | 以对应版本测量为准 | 当前值运行 `cargo xtask tcb`；内核零 unsafe 规则不变 |

## 证据索引

| 范围 | 入口 | 如何解释 |
|---|---|---|
| 抽象状态与研究目标 | [规范](COSIR%20原语规范.md)、[白皮书](Prima%20OS.md) | 预期语义与研究假设，不是实现证明 |
| 阶段0–4签署 | 上方 SPEC-145–149 | 以各自退出条件及证据范围为限 |
| 签署后整改 | [SPEC-150](docs/superpowers/reports/2026-10-17-stage1-review-remediation-spec150.md)、[151](docs/superpowers/reports/2026-10-18-stage1-open-issues-spec151.md)、[阶段2 IPC](docs/superpowers/reports/2026-10-18-stage2-ipc-review-remediation.md)、[阶段3](docs/superpowers/reports/2026-10-18-stage3-review-remediation.md)、[1–4复核](docs/superpowers/reports/2026-10-18-stage1-4-implementation-review.md) | 旧签署后的局部修订，不抹去旧报告 |
| OSTD 跨项目复用 | [契约](docs/superpowers/specs/2026-10-24-asterinas-reuse-design.md)、[验证](docs/superpowers/reports/2026-10-24-asterinas-reuse-report.md)、[来源与许可](docs/architecture/asterinas-provenance.md) | 对齐/临界区、[POD/缓冲](docs/superpowers/specs/2026-10-24-asterinas-pod-buffer-design.md)及[一次发布](docs/superpowers/specs/2026-10-24-asterinas-once-publication-design.md)后继；不等于完整框架 soundness 或 SMP |
| 原生运行时/服务 | [SPEC-130](docs/superpowers/reports/2026-10-16-stage4-closure-report.md)、[握手/路径后继](docs/superpowers/reports/2026-10-24-native-runtime-service-report.md)、[主题包](docs/architecture/topics/runtime.md) | 原生应用/服务收口不等于 POSIX 完整实现 |
| 阶段5当前子集 | [M20](docs/superpowers/reports/2026-10-23-stage5-m20-compat-final.md)、[M21](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)、下方里程碑表 | 分别记录已覆盖、限制与未签署条件 |
| 阶段5余项/阶段6入口 | [收尾路线图](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md) | 计划；其中旧基线的“未实现”由后继报告限定，不当成当前摘要 |
| 历史切片与反例 | [全量目录](docs/architecture/catalog.md)、[主题阅读链](docs/architecture/topics/README.md) | 文件不因未进入精选表而失效；必须保留失败与后继 |

2026-10-18 压缩前记录保存在[历史版本](https://github.com/Alma1314/PrimaOS/blob/a434a2f0c515ac3421e999be3c436b7d7c6c5313/Prima%20OS%20%E5%AE%9E%E7%8E%B0%E9%98%B6%E6%AE%B5.md)。本次重构前版本可用 `git show d48d5ef:Prima\ OS\ 实现阶段.md` 查看；[整理报告](docs/superpowers/reports/2026-10-23-documentation-restructure-report.md)记录陈旧摘要的修正依据。

VFS 后继实现与验证见[LibOS 基础重构报告](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)：原生打开对象、普通文件延迟回收和数据边界修复；不改变阶段出口勾选。

musl 后继实现见[调用边界与 fd/stdio 报告](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)：fcntl、fdopen/已有 append、向量 I/O、汇编边界与锁定构建；不改变阶段出口勾选。

内核句柄与 Lean 后继补强见[验证报告](docs/superpowers/reports/2026-10-24-kernel-handle-model-report.md)：generation 耗尽永久退役、权限未知位拒绝、分配失败不推进逻辑 CapId，以及单槽任意后续迁移中的旧句柄不复活证明；证据分层及未覆盖项以报告/Lean 映照为准，不改变阶段签署。

原生运行时与服务后继见[握手/路径重构报告](docs/superpowers/reports/2026-10-24-native-runtime-service-report.md)：Hello 一致性验证、顺序分片、双路径事务重置与单次消费；仍限单客户端/单在途，未改变阶段签署。

Asterinas OSTD 复用后继见[移植报告](docs/superpowers/reports/2026-10-24-asterinas-reuse-report.md)：
对齐工具、StaticCell 原子互斥/IRQ guard、裸入口布局与不返回调度边界；不引入完整上游内核或修改阶段签署。

Asterinas 参考续篇见[POD 与恢复帧报告](docs/superpowers/reports/2026-10-24-asterinas-pod-buffer-report.md)：
字节视图的类型审计、TSS 下沉与恢复帧的溢出/借用/锁边界。验证范围以该报告为准，不改变阶段勾选或前轮超时结论。

Asterinas 一次发布后继见[报告](docs/superpowers/reports/2026-10-24-asterinas-once-publication-report.md)：Once错误/unwind重试、所有权析构及根/镜像描述整体发布；完整验收超时仍未收口，未改变阶段签署。

fork 续执行与进程边界后继见[报告](docs/superpowers/reports/2026-10-24-fork-continuation-report.md)：
COW写保护后同步TLB、分配失败回收、musl子退出与四字节跨页/COW等待输出；没有修改阶段签署。

本批增量见[2026-10-25 缺口补强记录](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)：启动栈空间、syscall保护页、托管退出交接、fork FPU继承及Rust最小工作目标；本次验证范围按报告解释。
[profile v2](docs/superpowers/specs/2026-10-25-stage5-profile-v2.md)保留提案；D6用户选定的原生单线程std A已验证，整体阶段签署仍待最终评审。

本次后继代码及待办见[阶段0–5工作记录](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)：
本地能力位置/事件组合、Task栈guard、独立fork能力视图、认证VFS/OFD/socket holder、exec新crt、信号/线程/IRQ及D6 A均已取得本轮有界证据。
遵照用户要求先完成实现再统一测试；最终完整68轮通过，历史失败保留。下表按实际范围更新，组织级退出勾选仍待确认。

POSIX std后继（2026-10-27）见[工作契约](docs/superpowers/specs/2026-10-27-posix-std-backend-design.md)与
[本次报告](docs/superpowers/reports/2026-10-27-posix-std-backend-report.md)：独立Prima Unix/musl目标与完整std模块接入；
原A子集和组织级签署边界保留，具体支持范围以R3及台账为准。
[文件I/O与接收归账续做](docs/superpowers/reports/2026-10-27-posix-std-file-io-report.md)已通过单次完整36组72/72轮、workspace1020、lint、三种Rust模式Clippy与compat；V4扩展和I1新归账按契约范围解释。

## 已知问题与当前阻塞

| ID / 事项 | 当前口径与限制 | 证据 / 下一步 |
|---|---|---|
| QEMU 启动超时记录 | 2026-10-23 VFS 多次超时；后继双轮通过。2026-10-24 原生服务首次 W9 无输出超时；OSTD首轮复用的 V1 停于 bootloader 物理映射、W1 完全无输出；当时仅分段覆盖62轮，完整命令未单次全绿。POD后继完整62轮通过，旧超时根因仍未定位；一次发布后继三次完整命令超时及多次后缀超时，最终成功覆盖60/62轮，保留该版本验证缺口；本轮fork后继分段覆盖62/62，完整命令在35轮后W8 run2超时，根因仍未定位；2026-10-25前批及2026-10-27本轮均分别取得单次68轮PASS；本轮另保留I1/N1计数失败和显式验收修订，见后继报告 | [一次发布超时](docs/superpowers/reports/2026-10-24-asterinas-once-publication-report.md)、[OSTD首轮 V1/W1](docs/superpowers/reports/2026-10-24-asterinas-reuse-report.md)、[W9](docs/superpowers/reports/2026-10-24-native-runtime-service-report.md)、[VFS 历史](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)、[musl 后继](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)；[本批完整验收](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)；保留门限/断言/失败证据，不以栈保护或退出整改自动解释全部历史超时 |
| I1接收归账验收（有界技术收口） | 初始69/70失败保留；后继新增所有阻塞接收出口及UART子集的逐轮严格归账，跨轮用户总消息数及非UART出口固定；本组双轮及完整72轮PASS | [归账修订与实测](docs/superpowers/reports/2026-10-27-posix-std-file-io-report.md)；旧354次阻塞没有分类快照，不能倒推逐条来源；其余协议/输出/清理断言及30秒门限保留 |
| 系统调用栈保护边界 | syscall和全部Task中断栈下界guard通过host与本轮启动页表检查；32KiB可用容量、哨兵保留；exec准备缓冲在提交前退栈 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；stack_guard=1，不构成所有路径峰值证明 |
| BLK-003 | 待最终范围确认：Prima号段/命名/错误映射、静态链接、线程及静态护栏已有具体选择和技术证据 | [D1–D10评审工件](docs/superpowers/specs/2026-10-26-stage5-release-decisions.md)；不以实现或测试自动代替组织级采纳 |
| S5-D1 / S5-D6 / S5-X3 | v2保留提案、待冻结；D6原A已完成双轮和Clippy；本任务扩展POSIX std B后端，R3已双轮通过，保留底层API限制；文件I/O与接收归账后继完整72轮PASS，初始I1失败留作历史；冻结开源测试集待最终复核 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)、[D6裁决](docs/superpowers/specs/2026-10-26-rust-d6-scope-decision.md)；阶段5未签署、阶段6未开始 |
| fork 子任务 VFS 授权 | 认证会话/直接IPC/OFD holder、libc execve/envp/CLOEXEC、新C crt与连续exec本轮W6/W7/W7b双轮通过 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；有界静态映像及RAM VFS，不宣称完整execve |
| ISSUE-FORK-CAPS-001 | 用户选定A的独立视图、受控继承、认证服务交接/发布、OFD/socket holder在本轮host及W6/N1中通过；有界技术范围收口 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；多线程fork、每LibOS根视图不在范围；共享线程视图待最终语义确认 |
| ISSUE-PTHREAD-CAPS-001（工作决定） | 同 Process 线程共享 leader 能力空间，组织级语义确认仍待完成 | [M15设计](docs/superpowers/specs/2026-10-23-stage5-m15-process-thread-wait-design.md)、[M15b-2a](docs/superpowers/reports/2026-10-23-stage5-m15b2a-thread-create.md) |
| 信号/线程未覆盖项 | mask/siginfo/EINTR/RESTART/SIGCHLD、硬件故障及取消/分离出口的W4/W9/I1/N1双轮通过 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；无altstack/实时排队/完整属性及任意异步取消保证；#AC仅host路由 |
| ISSUE-TTYIN-001（有界技术收口） | IRQ4驱动的输入、等待/取消/确认、EINTR/重启、handler嵌套回复本轮I1双轮通过；不再以轮询工作实现概括当前路径 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)；无VTIME、流控或job control，IRQ时序验收边界见报告 |
| ISSUE-START-ARGS-001（有界子集收口） | 已定位256字节预留不足，改为按参数实际尺寸分配；A2独立版本化载体16/8项、长串、空值及UTF-8的Linux参考和QEMU双轮通过 | [本批后继](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)；不外推任意用户程序，无AT_RANDOM等边界保留 |
| ISSUE-CAP-001 | 本地Task/消息/Bound/Mount及Timer/异常/等待/远端组合的位置与溯源host回归通过；Lean位置保持性构建通过；有界模型范围收口 | [本轮报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)、[Lean映照](lean/README.md)；不是完整COSIR状态机或机器证明 |
| ISSUE-AUTH-001 | SPEC-146 接受的 image/vspace 细化分叉，不阻断 S1-X2 | 保留与§14完整细化的区别；新装载路径另按阶段5报告取证 |
| ISSUE-COW-001/002 | 抽象 COW 的映射/并发责任澄清，不是自动阻断项 | [SPEC-015](docs/superpowers/specs/2026-08-14-cow-refinement-design.md)；扩大语义时再验证 |

### 已收口问题的后继入口

- ISSUE-START-ARGS-001在M12有界参数载体范围技术收口：[A2后继](docs/superpowers/reports/2026-10-25-stage5-gap-closure-report.md)，保留M12原始失败，不外推任意用户程序或AT_RANDOM。

- ISSUE-FORK-CONTINUATION-001 在当前单核/单线程C/musl续执行范围技术收口：[fork后继](docs/superpowers/reports/2026-10-24-fork-continuation-report.md)，W6/W7/W7b/W8聚焦双轮通过，该历史版本完整命令仍超时；本轮后继已取得完整单次68/68，见[统一报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)。M17b-3/4的原生绕行和失败保留；cap/close隔离及VFS授权仍按各自问题跟踪。

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
| 2026-10-27 | 本轮统一技术验收完成：host1016、BIOS单次68/68、Linux参考20项及分层检查通过；profile范围/组织级签署待确认 | [统一报告](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md)、[具体评审工件](docs/superpowers/specs/2026-10-26-stage5-release-decisions.md) |
| 2026-10-26 | 用户选择D6 A：有原生OS接口的单线程std子集；开始统一验收，阶段5仍未签署 | [D6裁决](docs/superpowers/specs/2026-10-26-rust-d6-scope-decision.md) |
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
| M12–M13 | [启动/退出](docs/superpowers/reports/2026-10-22-stage5-m12-c-startup.md)、[链接/rename](docs/superpowers/reports/2026-10-22-stage5-m13a-vfs-links.md)、[权限/stdio](docs/superpowers/reports/2026-10-22-stage5-m13b-permissions-stdio.md) | A2有界载体和W7b CLOEXEC本轮通过；无硬链接/持久化 |
| M14 | [TTY原始范围](docs/superpowers/reports/2026-10-23-stage5-m14-tty-input.md)、[本轮IRQ/取消/重启](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | I1双轮通过；IRQ4/FIFO，有界行规程，无VTIME/流控/job control |
| M15–M16 | [原始线程证据](docs/superpowers/reports/2026-10-23-stage5-m16b3-pthread-join.md)、[本轮线程/futex/取消](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | W2–W5双轮通过；有限属性/取消/分离出口；共享线程视图待最终范围确认，不承诺完整pthread |
| M17 | [fork/COW原始范围](docs/superpowers/reports/2026-10-23-stage5-m17b4-fork-wait.md)、[本轮完整后继](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | W6/W7/W7b双轮和Linux参考通过；有界独立视图/OFD、新crt/连续exec；非多线程fork/动态装载 |
| M18 | [原始signal范围](docs/superpowers/reports/2026-10-23-stage5-m18b-sigaction.md)、[本轮mask/EINTR等](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | W8/W9双轮通过；合并通知/8层现场，无altstack/实时排队，#AC无机器见证 |
| M19 | [loopback UDP N1](docs/superpowers/reports/2026-10-23-stage5-m19-loopback-udp.md) | 仅环回 IPv4/UDP，无 NIC/TCP/DNS |
| VFS / LibOS 基础 | [原始验证](docs/superpowers/reports/2026-10-23-vfs-libos-foundation-report.md)、[本轮会话/OFD](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | 认证Process holder/fork关闭隔离已验证；每LibOS根及多personality仍未实现 |
| musl 后继补强 | [原始边界报告](docs/superpowers/reports/2026-10-24-musl-port-boundary-report.md)、[本轮验证](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | 八补丁锁定构建/归档和host5项通过；信号/取消/exec只覆盖本轮有界见证 |
| M20–M21 | [历史出口评审](docs/superpowers/reports/2026-10-23-stage5-m21-exit-review.md)、[本轮技术验收](docs/superpowers/reports/2026-10-25-stage0-5-gap-followup.md) | D6 A已验证；profile v2与D1–D10可审阅，最终范围及组织级签署待确认 |

### 交付检查

以下保留未签署的勾选状态；“当前子集满足”不自动代签整体交付条件。

| ID | 交付物 | 勾选 | M21 当前判定 |
|---|---|---|---|
| S5-D1 | 冻结且版本化 POSIX profile | [ ] | v0已冻结、待组织级；v1提案；v2提案待评审 |
| S5-D2 | libc适配 | [ ] | 有界信号/线程/退出/exec后继已通过本轮host、Linux参考及机器双轮；范围待最终签署 |
| S5-D3 | fd、路径、线程和基础网络子集 | [ ] | RAM VFS/OFD、线程及环回UDP有界见证通过；无持久化/硬链接或NIC/TCP/DNS；最终范围待确认 |
| S5-D4 | 逐API语义映射矩阵 | [ ] | [当前逐入口表](docs/superpowers/compat/stage5-working-api.md)已与本轮证据对齐；partial不外推完整语义 |
| S5-D5 | Linux差分矩阵 | [ ] | 20项同源参考及BIOS68轮通过，包含W7/W7b；有限见证，不证明全部参数组合 |
| S5-D6 | Rust target/core/std | [ ] | [用户选定A](docs/superpowers/specs/2026-10-26-rust-d6-scope-decision.md)的R1/R2双轮及双模式Clippy通过；[POSIX std文件I/O后继](docs/superpowers/reports/2026-10-27-posix-std-file-io-report.md)的R3双轮与Clippy、完整72轮通过（当前工作范围）；最终出口签署待确认 |
| S5-D7 | persona、符号版本、台账与ABI护栏 | [ ] | 静态声明/版本锚点/可见性脚本、拒绝负例已验证；是否满足有界出口待评审，无动态版本解析/内核persona |

本轮可审阅范围与最终签署门见[具体评审工件](docs/superpowers/specs/2026-10-26-stage5-release-decisions.md)；D6用户已选择A并已验证；本轮技术验收完成，最终范围与组织级签署待确认。

### 上游 ABI 与候选规格归属

BLK-003 保持 Open。原生号段以 [prima-abi](prima-abi/src/lib.rs)为定义源；1–72的工作映射（52保留、新59–72按本轮host和具体验收程序取证）见[台账](docs/superpowers/compat/compat-state.md)和[接口说明](docs/architecture/interfaces.md)，52未分配。候选 DOC-002 的 `0x0xx`、RUXOS 命名与时延目标不自动进入 Prima 契约。

WaitObject/futex、信号、fork/exec、TTY 输入、匿名内存已经有各自的后继技术证据，不能继续以候选阶段的“未实现”表代替当前状态。直接 IPC ≤250ns、动态链接归属、Rust 范围、persona/符号版本及未收口语义仍按[路线图 D1–D10](docs/superpowers/plans/2026-10-22-stage5-completion-roadmap.md)与 M21 裁决；同步 call/reply 快路径和性能指标不因队列 IPC 通过而完成。

候选规格与风险走查保留在[基座库规格](docs/superpowers/specs/2026-10-11-baselib-stack-spec-v1.1.md)和[走查记录](docs/superpowers/specs/2026-10-11-baselib-stack-walkthrough.md)。这些是设计输入，不是本仓库已接受的排期或ABI承诺。

### 退出条件

| ID | 条件 | 勾选 | 当前依据 |
|---|---|---|---|
| S5-X1 | profile每项API均有映射与可重复测试 | [ ] | 当前映射/见证及统一验收已齐备；profile冻结边界待最终确认，不外推逐API全部参数 |
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

文档维护记录：[2026-10-24 生命周期治理与清理](docs/superpowers/reports/2026-10-24-documentation-lifecycle-report.md)仅更新文档与检索工具，不新增 OS 运行证据，不改变上述阶段状态、勾选、profile 或阻塞结论。

## 阶段晋级规则

1. 阶段状态由该阶段的退出条件决定，不由提交数量、代码行数或主观完成度决定；
2. 前一阶段未满足退出条件时，后一阶段可以做探索原型，但不能成为“当前已完成阶段”；
3. 失败测试、反例和实现偏差应写入对应报告；仍影响当前阶段的事项进入上方问题表；
4. 关闭阶段时，在签署报告中记录完整命令、工件版本和结果，并从证据索引链接该报告；
5. 架构或语义变化必须先更新权威文档，再重新评估受影响的已关闭检查项。
