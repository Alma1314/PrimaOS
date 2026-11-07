---
id: plan-stage7-execution
kind: plan
authority: execution-plan
status: executing-foundation
lifecycle: current
updated: 2026-11-08
---

# 阶段7执行计划：工程底座与九轨冻结顺序

## 目标与依据

作者2026-11-06指示“启动阶段7”；基线`d171139`，阶段1–6已有有界技术退出与后继整改证据。
上位范围为[作者九轨裁决](../specs/2026-10-31-stage7-scope-decisions.md)、
[白皮书阶段7](../../../Prima%20OS.md#阶段-7--扩展与硬化)、
[D11–D16](../specs/2026-10-28-stage6-7-release-decisions.md)。
本计划局部承接[旧阶段6–7计划](2026-10-27-stage6-7-refinement.md)的阶段7执行部分；
旧估算、较弱恢复目标与阶段6当时状态保留为历史，不作为本轮范围或工期。
当前状态与退出勾选仍只由[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)维护。

启动不关闭技术冻结门或自动签署阶段6/7。首片是7-0的宿主工件互斥，
具体[实现前契约](../specs/2026-11-06-stage7-artifact-exclusion.md)先于代码；其余机制按依赖逐片评审。
不因为平台、白名单或恢复协议待选而阻塞已可确定的工程底座工作。

## 工作包与依赖

| 包 | 范围/依赖 | 下一项可审阅交付 | 验收观察 |
|---|---|---|---|
| S7-0.1 | 宿主共享工件；无新OS语义 | xtask工作区独占锁、竞争拒绝和释放测试 | 并发竞争不改镜像；释放后同一命令可运行；原断言保留 |
| S7-0.2 | 依赖0.1；CI工程 | host、普通BIOS、Linux、Gate/审计停止、D14、文档/形式化的明确作业矩阵与资源预算 | 清洁环境安装/锁定输入；CI真实记录结果；不足环境不能假PASS |
| S7-0.3 | 依赖可复现构建与G6方法确认 | 五项基准的方法、采样、环境和制品清单；发布身份/许可检查 | 先得到单核基线，再随SMP补2/4核基线；实测后冻结数值门槛 |
| S7-A.1 | G1/G3；复用阶段5/6，不以D14人格桩替代 | 两条真实ABI/profile与程序白名单、独立语义状态/根/能力视图设计 | 双路径应用、显式共享服务、跨域拒绝的正反例矩阵 |
| S7-A/C.1 | A.1与最小资源归属/审计隔离契约 | 单域额度与必要审计全局保留的机制/模型 | 单侧压力和故障后另一侧有可测进展；真实全局故障规则独立保留 |
| S7-B.1 | A的状态归属、G2/G7，遵守D15 | per-CPU状态/所有权设计，随后AP、IPI、TLB shootdown和锁顺序 | x86_64 2/4 vCPU并发反例；禁止给现有单一全局状态简单加锁代替分区 |
| S7-C.2 | 最小配额与G3/G7 | CPU/内存/Task/句柄/队列/审计、磁盘/I/O/带宽的计量与转移 | 各维度单独耗尽与生命周期守恒；机器检查证明及生产映照 |
| S7-D.1 | A/C与G4/G7 | 能力、会话、CPU/DMA映射和未决请求的撤销完成点 | 旧授权失效/不复活、在途结算与无关域持续进展 |
| S7-E.1 | 阶段6装载基线与G1/G2 | Linux及Prima原生/POSIX两条动态链接契约、解释器/库/应用白名单 | 各自真实动态制品正反例；与既有静态回归并存 |
| S7-F.1 | G2/G3/G4，依赖必要隔离机制 | 用户态块设备/网络、DMA/IOMMU、磁盘格式/启动与持久化契约 | 非法DMA拒绝、收发、盘启动、跨重启文件保存分别举证 |
| S7-G.1 | G3–G5/G7；协议设计可先行，完整验收依赖对应E/F | 故障窗口、状态/请求身份、去重与结果查询、重新授权协议 | 用户态组件崩溃后应用续执行；真实副作用不丢失/不重复；守恒证明 |
| S7-H.1 | 抽象观察稳定、G2/G7 | AArch64单核启动与同一POSIR机制映照 | 能力、内存、调度、IPC的对应COMPAT-TEST和架构差异记录 |
| S7-EXIT | 九轨各自完成G8，G6实测门槛通过 | 分轨评审与整体证据包 | 不跳过任一必交付轨，不以运行成功替代证明或组织签署 |

## 验收作业分组

以下保留首片的作业拆分依据。后继[CI契约](../specs/2026-11-06-stage7-ci-evidence.md)已具体化九组矩阵、
预算与输入准备，[实现报告](../reports/2026-11-06-stage7-ci-evidence.md)记录本地实跑；
[远端整改后继](../reports/2026-11-07-stage7-ci-remediation.md)记录首次九组结果；
[九组通过与Gate基线](../reports/2026-11-07-stage7-ci-green-gate-baseline.md)记录1921757干净远端全部通过，
以及逐步原始串口/探针子目录制品身份修复的本地验证。

| 组 | 现有入口 | 资源/边界 |
|---|---|---|
| host | cargo test --workspace；musl/test_port.py；scripts/test_posix_std.py | pinned Rust、C编译器与Python；host模型范围 |
| lint/TCB | cargo xtask lint / tcb | 裸机编译与源码守卫；不是QEMU或完整安全证明 |
| ordinary BIOS | cargo xtask test；test-process W5等聚焦入口 | 同工作区图像互斥；单次完整72轮仍独立计量；Linux N1参考需本机UDP |
| Linux | PRIMA_LINUX_ELF=锁定输入 + linux-probe/test-linux | pinned guest与同ELF、双轮、原有门限和原始审计 |
| Gate | gate-probe/test-gate；gate-audit-probe/test-gate-audit；test-domains | 配置逐个编译并串行打包，不能复用错误case的二进制；D14仅人格桩 |
| docs/formal | docs四项检查；Lean主模型/有限表；锁定源码细化工具 | 缺工具明确失败/登记；模型与源码证明分别记录 |

0.2后继已定义独立工作区、最多2作业及Cargo/musl各2 worker、组预算与验收manifest；
没有主张跨宿主RAM/端口统一保留或完整发布包；首次干净托管runner的2组通过/7组失败与共同根因
见[2026-11-07整改](../reports/2026-11-07-stage7-ci-remediation.md)；1921757的九组远端功能结果已通过，
faf551a后继已取得干净远端九组及86份归档/探针身份核对，见[G5推进报告](../reports/2026-11-07-stage7-gate-user-benchmark.md)。
既有G4的单核内核路径基线方法和原始样本后继见
[测量契约](../specs/2026-11-07-stage7-gate-baseline.md)及[报告](../reports/2026-11-07-stage7-ci-green-gate-baseline.md)。
作者2026-11-07已确认原生syscall、IPC往返、Gate往返、页故障/COW、端到端LibOS操作五类基准，
详见[范围裁决§7.2](../specs/2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)；
0.3已提供[Gate G5 user-wrapper方法](../specs/2026-11-07-stage7-gate-user-benchmark.md)及本地有界单核基线；
后继[N1原生方法](../specs/2026-11-07-stage7-native-syscall-benchmark.md)及
[推进报告](../reports/2026-11-07-stage7-native-syscall-benchmark.md)补齐fe83e59自身远端九组/88份归档与N1本地基线。
后继[I1 IPC方法](../specs/2026-11-07-stage7-ipc-benchmark.md)与
[推进报告](../reports/2026-11-07-stage7-ipc-benchmark.md)补充38f92bb自身远端九组/90份归档、
N1分位数重算及I1本地有界单核往返。该片完成时其余两类方法/探针、多核、数值门槛与发布许可检查仍待完成；
I1的2e3b95f自身远端九组/92份归档及IPC审计排序整改见
[后继报告](../reports/2026-11-07-stage7-ipc-audit-order.md)；
[C1方法](../specs/2026-11-07-stage7-cow-benchmark.md)及[报告](../reports/2026-11-07-stage7-cow-benchmark.md)
补充a40c487自身远端九组/92份归档与第四类本地单核基线；C1新提交仍待自身远端，
端到端LibOS操作、多核、数值门槛与发布许可检查继续待完成。
后继[L1方法](../specs/2026-11-08-stage7-libos-benchmark.md)与
[报告](../reports/2026-11-08-stage7-libos-benchmark.md)补充8933a5c自身干净远端九组/94份归档，
以及第五类端到端LibOS本地单核基线；普通BIOS本机时限失败和后继干净CI分别记录，
不把前述历史待办或中断轮当作当前通过结论。多核、基线后门槛与发布检查仍由阶段跟踪器维护。

## 验证与停止条件

首片执行workspace tests、lint、跨进程竞争反例、W5与compat双轮/既有路径，以及docs四项检查。
本片不改OS启动或协议；不把阶段0–6基线的72/72当成本片新运行。若共享工件在锁前被改写、
已持锁进程不能完整完成命令、竞争被静默重试，先修复或重审本片契约。

后续关键机制在适用G1–G8完成技术评审后实施。证明失败、设备隔离不足、恢复副作用无法闭合、
需要放弃必要审计或引入OS专属内核语义时，保留反例并重审对应契约，不能删改必交付范围制造完成。

## 证据与后继

首片实际结果见[阶段7启动报告](../reports/2026-11-06-stage7-start.md)，
0.2与验收身份后继见[CI报告](../reports/2026-11-06-stage7-ci-evidence.md)。计划不保存滚动实现状态表；
各包完成后链接对应报告，阶段跟踪器维护当前状态与未关闭冻结门。
