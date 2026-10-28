---
id: decision-stage5-release-review
kind: specification
authority: proposed-stage-scope
status: scoped-accepted-in-spec152
lifecycle: current
updated: 2026-10-27
---

# 阶段 5 出口的具体评审工件

对应[阶段跟踪](../../../Prima%20OS%20实现阶段.md)中的 BLK-003、S5-D1–D7/X1–X3，
承接[路线图 D1–D10](../plans/2026-10-22-stage5-completion-roadmap.md#3-决策门)、
[M21 未签署结论](../reports/2026-10-23-stage5-m21-exit-review.md)。
这份工件说明可批准的具体范围与已取得的技术证据；当前没有发布或组织级签署。

## 待确认的工作选择

| 门 | 本工作树的可审阅选择 | 明确保留的边界/重新裁决触发 |
|---|---|---|
| D1 | 以修订后的 [profile v2](2026-10-25-stage5-profile-v2.md)及[逐入口表](../compat/stage5-working-api.md)提交评审 | 统一测试完成前不发布；每行partial不等于POSIX认证；v0历史证据不删除 |
| D2 | Prima命名，原生1–72中52保留；错误码仅登记实际实现，Linux号留用户态 | 1.0握手不承诺逐功能协商或稳定外部ABI；未来破坏性布局必须另行版本裁决 |
| D3 | 单Process可共享多Task；线程共享能力视图，fork建立独立视图；WaitObject及原子word等待 | 单线程fork；受控继承A已由用户选择；完整多线程fork/pthread_atfork仍不支持 |
| D4 | 阶段5静态链接；动态链接保留白皮书阶段7候选 | 不采用DOC-002的ld.so P0；未来动态装载须先改权威阶段定义和冻结版本策略 |
| D5 | 用户态loopback IPv4/UDP，固定端点/队列/报文预算、认证会话、fork holder及取消 | 不把IPC称为NIC网络；不提供TCP/DNS/NIC；拓展网络需要新协议及容量证据 |
| D6 | [选项A/B与具体模块矩阵](2026-10-26-rust-d6-scope-decision.md)用户已于2026-10-26选择A | A已完成R1/R2双轮和两种Clippy；B不在本轮；不重复请求A/B裁决，最终阶段签署单独确认 |
| D7 | libc最小信号集合、掩码/现场、SIGCHLD、故障、EINTR/SA_RESTART及取消出口 | 有界合并通知、8层现场；#AC仅host路由、无机器见证；无altstack/实时排队/完整job control；不能解释为完整signal |
| D8 | RAM VFS权限/umask/rename/符号链接/stdio；明确硬链接与持久化unsupported | profile逐行列出partial；未来link/linkat或跨启动语义必须补设计和见证 |
| D9 | 性能只记录固定环境实测及PROVISIONAL，不设未经测量的250ns出口门槛 | 同步call/reply直接快路径仍未实现；队列IPC通过不代表该目标完成；候选目标不并入当前语义退出条件 |
| D10 | 阶段6继续未开始；若后续启动，Linux persona留用户态，内核只做OS中立机制 | 本次静态声明不等于Linux persona、双人格根视图或未修改Linux ELF支持 |

## S5-D7 的静态护栏选择

本次[静态程序护栏](2026-10-26-static-program-contract-design.md)包含persona/adapter声明、构建及C exec拒绝、
crt版本化链接锚点、静态符号可见性脚本、ABI握手和逐入口台账。它可作为阶段5有界静态profile的评审对象。
它没有实现候选DOC-002的AS persona字段、AT_RUX_PERSONA、GNU运行时符号版本或NT_RUXOS_ABI。
是否接受上述有界静态选择满足S5-D7须由出口评审明确决定；若评审要求AS persona或动态版本解析，
必须继续实现并重新测试，不能把note或符号表局部化当成替代证据。

## 统一验收与最终签署顺序

用户要求全部缺口处理后统一测试。D6选择确认并完成相应实现后，执行workspace tests、lint、
完整BIOS双轮、Linux同源参考、musl输入锁/归档/host负例、Rust双模式及Clippy、compat、TCB、
Lean与文档self-test/build/check/base。新增模型的测试不替代机器或Lean证据，编译不等于执行。
失败必须记录输入版本、轮次、实际输出和修复重跑范围；历史超时继续保留，不能删除或调弱断言以通过。

统一测试后才能填写“实际结果/未覆盖/容量变化”，再把可审阅结果交用户或组织作最终范围确认和阶段签署。
S5-X3仍仅指冻结的sbase echo/cat/ls功能集，不延伸到任意重新编译程序。
源码探针和完整C内存exec不构成阶段6未修改Linux ELF证据。

## 当前状态与回退

本轮统一技术验收已完成，结果和未覆盖范围见下表。**2026-10-27：本工件的范围已被 [SPEC-152](../reports/2026-10-27-stage5-exit-signoff.md) 接受**，BLK-003 按阶段 5 范围关闭，阶段 5 勾选见阶段跟踪；下表数字保留为签署前批次记录，签署复跑见 SPEC-152。
新出现的证据失败、声明格式破坏性变更、第二persona或动态装载需求都触发重新评审。
仅在用户/组织明确确认后，将选择和依据登记到唯一的阶段决策日志；不另建隐式批准台账。


## 已完成的统一技术验收（2026-10-27）

证据及失败修复完整链见[本轮报告](../reports/2026-10-25-stage0-5-gap-followup.md)。

| 检查 | 实际结果 | 使用边界 |
|---|---|---|
| workspace/lint | 1016 PASS；fmt/Clippy/裸机/kernel零unsafe通过 | host不替代机器或证明 |
| Linux/BIOS | 20项同源参考、单次34组68轮PASS | 固定程序/输入；I1/N1时序不变量修订与旧失败明确保留 |
| Rust/musl | R1/R2双轮、两种Clippy；musl锁定重建/归档、5项host通过 | D6 A；八项补丁，非完整libc/全量std |
| Miri | authority模型21项PASS | 全包尝试手动中止，不记全包通过；不替代机器证据 |
| Lean/compat/TCB | lake build10jobs、cow-write-fault及TCB统计通过 | Lean映照和TCB统计口径不扩大；无完整机器证明 |
| 文档与证据 | self-test/build/check/base及diff检查通过（316篇） | 保留历史失败与未覆盖范围 |

本次可以确认的具体事项是：接受本工件D1–D10的有界选择及S5-D7静态护栏边界，
冻结相应profile v2范围，认可冻结sbase echo/cat/ls测试集的本轮复核，并据此作阶段5出口签署。
fork继承A与D6 A均已有用户明确选择，不再作为待选问题。
这不包含完整POSIX、动态链接、内核persona、NIC/TCP/DNS、硬链接/持久化、全量std、
SMP/完整机器Lean证明或250ns同步快路径。若要求纳入其中任一项，应重新裁决、实施并验收。
在取得明确确认前，profile保持proposed、阶段5保持未签署，阶段6保持未开始。
