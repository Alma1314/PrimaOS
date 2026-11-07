---
id: report-stage7-cow-benchmark
kind: report
authority: scoped-implementation-and-validation
status: cow-fault-locally-verified
lifecycle: historical
updated: 2026-11-08
---

# 7-0 C1 COW 基准与 IPC 整改自身远端后继

## 结论与范围

新增五类基准中的第四类：C1在ring3对64个新共享页分别触发真实写保护故障，经现有
cow::dispatch → resolve_cow完成4KiB复制、换PTE、invlpg、iret与原写指令重试。
测后逐页核对双根、全页字节、硬件地址/error=7、帧归属/不交性/守恒及用户读回。
计时包含C1专用故障观察器的断言与记录成本；不把它称为纯生产handler成本。
这不是完整fork基准、非present缺页或抽象§12.8用户态LibOS异常处理协议的验收。

本次核验a40c487自身干净远端九组及92份归档，在前篇限定范围关闭“IPC整改待自身远端”待办。
新C1仍待自身干净远端结果。端到端LibOS操作、多核基线、性能门槛及发布许可/镜像包继续待完成，
阶段7九轨退出均不勾选。本文是同一会话的实现/运行复核，不是第三方审查、形式化证明或组织签署。

## 基线、输入与实现

本片2026-11-07启动，2026-11-08完成收尾验证。起点a40c48788eff89ad05483f785a570f3401e35c2c，工作树干净。
依据[五项裁决](../specs/2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[执行计划](../plans/2026-11-06-stage7-execution.md)、[C1测量契约](../specs/2026-11-07-stage7-cow-benchmark.md)
及[IPC整改发现/失败/后继链](2026-11-07-stage7-ipc-audit-order.md)。
COW相关输入包括[历史模型设计](../specs/2026-08-14-cow-refinement-design.md)、
[页表首片](2026-10-23-stage5-m17b2-cow-primitives.md)、
[当时fork失败](2026-10-23-stage5-m17b2-cow-fork.md)、
[W6后继](2026-10-23-stage5-m17b4-fork-wait.md)与
[阶段0–6整改](2026-11-06-stage0-6-review-remediation.md)。历史报告的未接线/失败不覆盖其后继，
本片也不据既有回归推导完整POSIX或COW细化。

宿主AMD Ryzen5 3500U、8逻辑CPU、Linux7.0.0-42；固定nightly-2026-07-30，QEMU8.2.2。
C1平台pc-i440fx-8.2/TCG单线程/qemu64/1vCPU/128M/BIOS/dev，Task 2的IF=0。
没有独占宿主负载控制、冷样本丢弃或计时开销扣除，不比较前后数据推断优化收益。

- kernel/src/cow_bench.rs为可选夹具：真实memseg_create/map授权建立64页双根共享，使用fork同一cow_leaf变换预置COW；代码/栈/回执保持私有。新VA使用槽22，预置不纳入计时，不宣称执行了fork。
- 复用Code、LFENCE/RDTSC与PIT/TSC校准，回执1536字节；观察器核对每次#PF的Task/CS/CR2/error/RIP和指针、序号、写值、r11哨兵，委托生产分派器后要求原RIP重试。硬件地址/error保存在独立C1静态中，不用合成值冒充原始捕获。
- 测后每个原帧512字不变，私有帧仅首字改变，其余511字全部复制；原帧保持只读SHARED，私有帧可写且COW/SHARED清除，两侧均NX。128个物理帧逐对排除别名，恰64个新Segment归属帧，页表帧不增加；没有新增Gate事件。
- xtask拒绝错误头/字段/样本数、时钟回退、错误读回/故障码及任何帧别名，复用nearest-rank宽整数分位数；host测试同时覆盖有变动时长的正例与字段/集合/重放反例。
- CI Gate组追加C1，九配置18轮；原单轮30秒与1200秒组预算、既有配置断言及归档规则保留。生产ABI、分派器、页表算法和所有容量不变。

## a40c487 干净远端核验

本次gh run view 42379541225 --json status,conclusion,headSha,jobs,url及
 gh run download 42379541225 --dir /tmp/prima-stage7-audit-remote均退出0。
[Actions 42379541225](https://github.com/Alma1314/PrimaOS/actions/runs/42379541225)整体及九job均success；
九manifest均revision=a40c48788eff89ad05483f785a570f3401e35c2c、dirty=false，源摘要
 a5034e1eff6e0873c91794018246c5aa885435a16dd962e1a2bd23d2af9c41b9。
全部步骤passed/exit0，92份串口归档逐份长度与SHA-256一致；独立重算N1/I1所有样本、回执及分位数，
与步骤日志摘要精确一致。I1执行a40c487加强后的逐条Sent→Ready→Received检查，非旧v0.1结果。

| 组 | manifest运行ID | 秒数 | 串口归档 / 最终制品身份 |
|---|---|---|---|
| bios | c25ddc424689423793ddef01ff1badc7 | 584.292 | 38 / 38 |
| docs | e8c2786d79684fada96697957dcdbc54 | 14.349 | 0 / 0 |
| domains | b7c86a2117fd499faaed9c582a67a033 | 139.254 | 2 / 5 |
| formal | 62966833e3274e6292745e53c16125f3 | 396.962 | 0 / 0 |
| gate | 9ba129729bd448d2948991d9ea3fb17e | 355.93 | 16 / 5 |
| gate-audit | a74f8c073cf34fc6a3a119fc1d06b118 | 293.953 | 14 / 5 |
| host | 17cf277afda648c7941f2ac6cf81e71d | 136.344 | 0 / 0 |
| lint | 06fe5277ff014c808d56db363602fd6d | 118.219 | 0 / 0 |
| linux | 2d17d03ca38d4a9d9d988a2701f3c6a9 | 196.436 | 22 / 12 |

普通BIOS为36组72个唯一(case,round)，已含默认启动双轮；Gate八配置16轮。
92份中包含空错误汇总；domains仅D14人格桩，formal仅既有模型证明范围。
Actions保留串口与清单，不上传镜像字节；制品身份是每组最终文件，不代表每一步旧镜像。

| I1远端轮次 | Hz | p50/p99 ns | traces/08串口SHA-256（各8882字节） |
|---|---|---|---|
| 1 | 2437354230 | 854118 / 4671813 | d5d81a2005149f9f33fe7b22eac544c3318a499f0c407c5a9822e135c5d4123e |
| 2 | 2437351830 | 866845 / 4585894 | a1ebad0cf762a5ba7490fea6eccd34f9a8e1722c2beeb3d9ef8329cc1185380a |

最终Gate ELF3013736字节，4ed6a06e8527ce357e435180637c7f0fd460c457a1d372b38c3e04f6235a3785；
BIOS12026880字节，c5de43a6e901692cb5a5f0d94fdc6f2abf0afef3976ebdf86973a3fd1bf5bab8。
未执行UEFI，不将身份清单冒充制品字节复核。

## C1 本地命令与结果

本地四组均采样a40c487 dirty，同一源摘要
7128cf5d1928c29e6079491b476a7d30ea7108ae51ce8181f32b4963bdca8b13；
当时修改范围是C1探针、bench路由、xtask解析/测试、CI矩阵/测试及实现前C1方法，
随后只补录报告/索引/阶段跟踪/方法链接和生成目录。不将dirty结果改称新提交的干净远端。

| 命令 | 本次结果 | manifest运行ID / 秒数 |
|---|---|---|
| CARGO_NET_OFFLINE=true cargo test -p xtask benchmark | 退出0，5通过 | 有变动时长正例及格式/时钟/故障/帧别名反例 |
| CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=C1 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings | 退出0 | 可选特性检查 |
| CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=C1 cargo run --locked -p xtask --features gate-probe -- test-gate | 退出0，聚焦双轮通过 | 不纳入正式基线 |
| python3 scripts/ci.py gate | 退出0，九配置18轮，18份串口归档，5项最终制品身份 | fb8a2c9e98e04dfbbbd270afbfc53a5e / 365.882 |
| python3 scripts/ci.py host | 退出0，1142通过、0失败、1项既有忽略；musl7、POSIX std3、CI运行器11通过 | 26fc439773e5418a8d80e3dc05ed1731 / 8.459 |
| python3 scripts/ci.py lint | 退出0，fmt/Clippy/裸机编译/44文件零unsafe/personality护栏通过 | 8ae7273bd662477388abd25d5e8151a1 / 6.731 |
| python3 scripts/ci.py bios（宿主权限） | 退出0，36组72轮，38份串口归档、38项最终制品身份；含默认双轮/W4/W7/W7b/W9及Linux UDP参考 | 1f0fb1f9c6e341169beef761dd35d932 / 1624.357 |

四份manifest均passed、所有步骤exit0且源身份精确一致；逐份核对56份原始串口归档的长度/SHA-256，
逐项读取43项当前最终制品重算长度/SHA-256。BIOS解析为36种case、72个唯一(case,round)。
Gate九配置选择器和18份归档逐步对应，C1两轮全部回执、128个帧身份及分位数与08.log摘要一致。
普通BIOS中的C1/I1是既有启动case，与PRIMA_GATE_CASE=C1/I1测量选择器互不等同。
原始输出保留在ignored target/及/tmp/。

TCB实际输出trusted_lines=2087、mechanism_lines=39642、total_lines=41729、ratio=5.00%；
unsafe=79/0、include_bytes=0、prima_user_dep=false、protocol_lines=56。此脚本统计包含测试及可选探针，
不是生产可信基缩减或完整安全证明。

文档 python3 scripts/docs.py self-test、build、check及check --base a40c487均退出0，
391文档、3727节；补录结果后再次生成并检查，git diff --check通过。

| C1轮次 | Hz | p50/p99 ns | traces/08串口SHA-256（各8193字节） |
|---|---|---|---|
| 1 | 2085319941 | 460448 / 2907746 | cfd1572c9ab7570537115e33a9d9d9f770dd57bf0a0283e5947376b46417c68e |
| 2 | 2090681760 | 424493 / 2897436 | b36bd026b1471ca512ca6870763a7ebc80445a25ce5e2ee7d87d9f5bd9c18ed4 |

最终C1制品 target/prima-os/gate-probe/prima-kernel-boot.elf：3051256字节，SHA-256=82967e2d84283c403e147065ad8fa54e8f70c186b8fac6effcecc33cb25171c6。

最终C1制品 target/prima-os/gate-probe/prima-os-bios.img：12026880字节，SHA-256=22c657cf8466caf6a08fd7eca4d15faafc5321392e25158e2ace98fed9bdd028。

UEFI未执行。

统计每轮独立nearest-rank排名32/64，ns=floor(cycles×10^9/hz)。样本不跨boot混合，
64样本的p99就是该轮最大值，不据此声称尾延迟统计稳定或通过尚未冻结的性能门槛。

## 失败、反例与边界

- 首次九配置矩阵0eccde3d81c6421c8a3fc7b8a4326a0a在268.507秒退出1；前五配置双轮通过，G5首轮在启动至cow_ready后触发30秒超时，尚未进入基准，未输出panic，G5步骤43.387秒且未形成新串口文件。未改变源码或门限，显式整组重跑；首次失败保留在target/及本记录。超时根因未定位，不据后续通过声称已修复。
- 第二次矩阵9651939c737747079543bc736080b3d4在259.033秒退出1；前四配置通过，G4首轮1000样本通过、第二轮同样在cow_ready后30秒超时，G4步骤57.437秒。随后以PRIMA_GATE_CASE=G4、PRIMA_QEMU_DEBUG_LOG=/tmp/prima-stage7-c1-g4-cpu.log运行原test-gate双轮退出0，最终第二轮异常日志只见三次预期ring0 #PF（error 0/3/3）及用户int80/int3；日志路径每boot覆盖，不能冒充两个完整异常轨迹。未改源码/断言/时限；仍未定位前两次超时因果，不能称为修复。
- 聚焦C1双轮在完整矩阵前已通过；正式基线只取完整通过矩阵的C1归档，不混用聚焦样本或失败作业数据。
- 原先IPC排序、CI嵌套输入/归档、锁竞争与沙箱UDP参考失败均按前篇保留；本次通过不扩大其整改范围。
- 没有KVM、实机、SMP、非present缺页、完整fork成本、用户态LibOS异常处理或新Lean证明。现有内核general COW只是本片机器路径，与抽象§12.8的用户态协议边界已显式区分。
- 原共享帧与私有帧延续既有Segment归属；本片不建立新的生命周期回收/额度转移结论。重审触发见C1方法。

## 后继关系与剩余工作

本文在a40c487自身远端范围补强[IPC整改报告](2026-11-07-stage7-ipc-audit-order.md)，
保留其原本地数据和失败记录。五类基准已有四类有界单核方法与基线；
[阶段跟踪](../../../Prima%20OS%20实现阶段.md)继续维护端到端LibOS、多核、门槛和发布检查，
新C1须在合入后取得自身远端证据，各轨技术与组织签署独立保留。


## C1 自身远端后继（2026-11-08）

[L1推进报告](2026-11-08-stage7-libos-benchmark.md)核验8933a5c自身干净远端九组/94份归档、
C1全部原始样本/帧身份及分位数，在该限定范围补齐本文当时的“C1待自身远端”。
本文历史失败与未覆盖项保留；后继不证明超时因果或阶段7退出。
