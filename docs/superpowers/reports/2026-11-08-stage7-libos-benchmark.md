---
id: report-stage7-libos-benchmark
kind: report
authority: scoped-implementation-and-validation
status: libos-read-and-clean-ci-verified
lifecycle: historical
updated: 2026-11-08
---

# 7-0 L1 端到端 LibOS 基准与 C1 自身远端后继

## 结论与范围

L1在真实受管Linux ELF中测量64次17字节文件读取，完整经过Gate、用户态Linux LibOS、
OpenFiles/VFS、请求授权复制与答复恢复，逐次核对全部字节/两侧哨兵，并测后读取原始审计。
固定Linux6.8.0-31 guest运行同一ELF通过语义参考；其TSC只报告cycles，不套用Prima的校准频率。
新增显式可选通道，不扩展D12白名单、冻结profile v1或生产ABI。

五类已有各自有界单核方法/本地基线：原生N1、IPC I1、Gate G5（G4独立保留）、COW C1、LibOS L1。
本次还核验8933a5c自身干净远端九组/94份归档，关闭前篇“C1待自身远端”待办。
实现提交e6dce41已取得自身干净远端九组/97份归档及完整36组72轮通过证据；本机两轮中断按失败记录保留。
多核、基线后数值门槛/稳定性、发布许可与镜像包仍待完成。
阶段7九轨退出均不勾选。这是同一会话的实现/运行复核，不是第三方审查、形式化证明或组织签署。

## 基线与实现

起点8933a5cc56c1068eb30597144d935621bee55fa7，工作树干净；依据
[五项裁决](../specs/2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[执行计划](../plans/2026-11-06-stage7-execution.md)、
[L1实现前方法](../specs/2026-11-08-stage7-libos-benchmark.md)、
[Gate协议](../specs/2026-10-29-stage6-m1-protocol.md)、
[受管装载](../specs/2026-11-01-stage6-managed-launch.md)、
[冻结Linux profile](../compat/stage6-linux-profile-v1.md)和
[阶段6实际差分/失败链](2026-11-02-stage6-process-and-differential.md)。
[C1前篇](2026-11-07-stage7-cow-benchmark.md)及前序失败保留原范围。

Prima平台pc-i440fx-8.2/TCG单线程/qemu64/1vCPU/128M/BIOS/dev，服务沿用现有调度/IF规则，
可选启动校准启用既有物理期限时钟，不是G5的IF=0夹具。参考使用现有pc/TCG/1vCPU/512M的
Linux6.8.0-31启动器，32KiB栈、同fixture，未将两侧平台或计量认作等价。
宿主AMD Ryzen5 3500U、8逻辑CPU、Linux7.0.0-42，固定nightly-2026-07-30，QEMU8.2.2。
没有宿主负载独占、冷样本丢弃、开销扣除或显著性/优化收益主张。

- prima-user/linux-probes/libos-bench.c用既有cc裸ABI构建选项产生静态ELF，无libc或Prima专有调用；普通11案例/D12成员表不变。
- 先open只读fixture，再64次计时外seek/填充、计时read、计时外校验及保存回执，最后close并分块write。64条完整时间戳、17字节、返回和哨兵在全部测量结束后导出；stdout上界12000字节。
- 实际反汇编确认0x401143 RDTSC→0x401164 syscall→r15保存rax→0x40116c RDTSC；编译器产生的合并计数/准备参数/起点保存位于区间内。数据比较与输出均在区间外；不是仅syscall本体成本。
- libos-bench特性显式选择L1，复用linux_main实际服务循环、Family/Process/read与signals答复。首请求上下文负例发生于采样前的open。服务收尾通过89读取原始Gate审计，验证64次read返回17/参数及64次seek返回0/参数、同一完整cookie和原有请求终结闭合。
- kernel timestamp-probe只在用户态入口前调用现有PIT/TSC校准并输出频率，生产ABI及普通镜像的校准行为保持原状；没有测量区间内新插桩、容量扩大或新的unsafe。
- xtask复用受管launcher、S6解析器与宽整数nearest-rank摘要，严格拒绝格式、时间、数据、身份、校准、审计不一致。原始S6PANIC显式拒绝；新增反例覆盖该拒绝。CI linux组保留原11项双轮并追加可选解析测试与L1参考/双轮，原30/180秒和组预算不变。

## C1 自身干净远端核验

本次gh run view 42395901438 --json status,conclusion,headSha,jobs,url与
 gh run download 42395901438 --dir /tmp/prima-stage7-c1-remote均退出0。
[Actions 42395901438](https://github.com/Alma1314/PrimaOS/actions/runs/42395901438)整体及九job均success。
全部manifest revision=8933a5cc56c1068eb30597144d935621bee55fa7、dirty=false，源摘要
13ea6d693075a290bca55d5a62dca149fc0d0b8cf02455072f5a100be0e9837c，所有步骤passed/exit0。
94份归档逐份核对长度/SHA-256；C1两轮128条回执/256个帧身份及分位数独立重算，与09.log精确相符。
两轮各自128个帧逐对不交；没有作跨独立启动的物理地址不交主张。

| 组 | manifest运行ID | 秒数 | 串口归档 / 最终制品身份 |
|---|---|---|---|
| bios | fa5526aab80c40d9b0e25f2feeb4dbbb | 962.304 | 38 / 38 |
| docs | 3cc23d4906aa4fb6b9d53e925ef3c3a1 | 9.195 | 0 / 0 |
| domains | ece5b8678b6a4784b224c2e0ede8f814 | 112.469 | 2 / 5 |
| formal | bacf3e98fb75488b88491f708d350cb3 | 379.921 | 0 / 0 |
| gate | 21d21b5b2f5e43bcb5df924d167807ca | 290.472 | 18 / 5 |
| gate-audit | 90e371ef264946e89e7bb3301e136a0c | 190.288 | 14 / 5 |
| host | 873c1625791a4ceda9373d48d5e9ab82 | 198.287 | 0 / 0 |
| lint | 89ffa0dc872747cd8417e2dc0c141168 | 128.724 | 0 / 0 |
| linux | b8340a1b5f284d34b5a105de2e257a61 | 258.551 | 22 / 12 |

普通BIOS为36组72轮，已含默认双轮；Gate九配置18轮。归档包含空错误汇总，
D14仍仅人格桩，formal只覆盖既有模型。Actions未上传镜像字节，制品身份只代表每组最终文件，
不代表每一步旧镜像，也没有将清单当作镜像字节复核或UEFI运行证据。

| C1远端轮次 | Hz | p50/p99 ns | 串口SHA-256（各8193字节） |
|---|---|---|---|
| 1 | 2291711927 | 171777 / 1363705 | 97a3190f7566d67de41edf457ae6144ab5036b0fb7c106219884466385d493be |
| 2 | 2291138336 | 171806 / 1361517 | 7d0af1f4406e76e4c64e198b91e74e599254f2e74f910342cf63673e00cdf206 |

最终Gate ELF3051256字节，ee1a0a1351c25b5ad999946eda447993a7622531d8619655e020f411e59de08a；
BIOS12026880字节，ee08d03dc30b810a1e1041a6fe08f757a97e9524444083513db3fd46fe3f79b8。
后续通过不定位C1前篇记录的启动超时因果，不宣称稳定性问题已修复。

## L1 本地命令与结果

以下本地四组采样8933a5c dirty，同一源摘要
1eab1510898bf74b6d382b27b73df824838345df226b17ad0148ed2ac0aa3f53；当时包括代码、解析/运行器测试及实现前L1方法。
随后仅补录本报告、索引/阶段跟踪/方法后继链接及生成目录。新提交的干净CI须独立登记，
不把dirty本地运行改称干净远端。

| 命令 | 本次结果 | manifest运行ID / 秒数 |
|---|---|---|
| CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo test --locked -p xtask --features libos-bench libos_benchmark | 退出0，1项有变动样本正例及格式/时间/数据/哨兵/身份/审计反例 | 聚焦检查 |
| CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo test --locked --workspace --features xtask/libos-bench | 退出0，1144通过、0失败、1默认忽略 | 含可选Linux/L1测试 |
| CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo clippy --workspace --all-targets --features xtask/libos-bench -- -D warnings | 退出0 | 可选特性检查 |
| CARGO_NET_OFFLINE=true PRIMA_LINUX_ELF=/usr/bin/busybox cargo run --locked -p xtask --features libos-bench -- test-libos-bench（宿主权限） | 退出0，参考64样本、Prima两轮各64样本 | 聚焦，不纳入正式基线 |
| python3 scripts/ci.py host | 退出0，Rust1142通过/0失败/1忽略；musl7、POSIX std3、CI运行器11通过 | 5be007e3b05a4907b55b5017305c0691 / 12.052 |
| python3 scripts/ci.py lint | 退出0，fmt/Clippy/裸机构建/44文件零unsafe及人格护栏通过 | 6051035a8792496f9a7e5995e5144ae8 / 16.992 |
| python3 scripts/ci.py linux | 退出0，原11案例各双轮、L1解析反例、同ELF参考及Prima双轮；25份原始串口、13项最终制品 | afebb0d710d341cc955bcfa76dffbf47 / 352.361 |
| python3 scripts/ci.py bios（宿主权限） | **中断，非PASS**；共17轮通过、4组超时；保存8份串口，step exit130，运行器退出1 | 8c53bbe09d9b47e99ac942c42eaf42bd / 586.511 |

host/lint/linux三份manifest及所有步骤passed/exit0；25份正式原始串口的长度与SHA-256逐份通过，
13项实际最终制品逐项读取复核。中断轮8份串口也逐份核对，不混入通过样本。
从正式L1归档独立解码stdout，核对192条样本（参考64、Prima128）的所有数据及时间，
Prima每轮133次trap/delivered、132 replied、1 cancelled，read/seek各64、cookie=30064771076，
全部分位数与02.log精确一致。参考只承担本操作的语义检查，不宣称整体Linux兼容。

| L1正式轮次 | Hz | p50/p99 ns | 串口SHA-256（各14218字节） |
|---|---|---|---|
| 1 | 2086743919 | 11491982 / 17500661 | 20646021928e7c3978858f56524304d196379b83bcc62f3fe223beea8feff457 |
| 2 | 2086737620 | 11613403 / 17194067 | fe65ca6db1bd512aac7a289f05416e75a5be493a72514cbbc1956719f34f6f1d |

参考p50/p99=5859/451563 cycles，5372字节原始串口SHA-256=
50c60995854550d9070e57ca25efe1849f507edd4b11aa2817d4eae816fcd04f。
不从这两个不同计量总体计算倍率。

最终探针stage7-libos-bench.elf为9120字节，SHA-256=
de5fb57055f86dfc7ef7b8f57e4b3f04c62195f68df1749d973b3911b8351478；
原生服务3202680字节，1e7f3ce3ca8ec99a4cbafd19bd1f724b4e51fb30afaa933696ffe2d68ca17eac；
linux-probe/prima-kernel-boot.elf为2709280字节，dd88a7d56431f5f37dc0a62e7aabed57ea3682db0308b1b44d4764468e7e33e6；
BIOS7832576字节，18b2fd102dccb8476fc1e62b221c2153dcce24f6a329b0c81e68bfb9be2aa31f。
没有UEFI执行证据。

TCB=2087 trusted / 39657 mechanism / 41744 total，5.00%；unsafe=79/0，
include_bytes=0、prima_user_dep=false、service_name_lines=0、protocol_lines=56。
文档self-test/build/check/check --base 8933a5c及git diff --check均退出0，393份文档/3740节；补录结果后重新生成并复核。实现提交的干净远端结果见下一节；其后只补录本文/阶段跟踪/索引和生成目录，代码保持e6dce41。

## L1 实现提交的干净远端后继

实现提交e6dce4176c546f7bc3c466d72c9b5e1e2a9e2f3d推送工作分支后，
通过gh workflow run ci.yml --ref codex/stage7-libos-benchmark触发既有九组矩阵。
本次gh run view 42405116578和gh run download 42405116578 --dir /tmp/prima-stage7-l1-remote均退出0。
[Actions 42405116578](https://github.com/Alma1314/PrimaOS/actions/runs/42405116578)整体及九job均success，
九份manifest均revision=e6dce4176c546f7bc3c466d72c9b5e1e2a9e2f3d、dirty=false、源摘要
25b3b14390089e9cd0065f44e953c9f0ba176740eb5554d38e307dd58a45fa72，所有步骤passed/exit0。
97份原始归档逐份核对长度/SHA-256；L1参考及两轮Prima全部192条时间/返回/字节/哨兵、
各轮133次请求闭合及read/seek各64的原始审计回执通过，分位数与03.log精确一致。

| 组 | manifest运行ID | 秒数 | 串口归档 / 最终制品身份 |
|---|---|---|---|
| bios | 723b1d950bfa4006ad6b9a413cbf869a | 542.405 | 38 / 38 |
| docs | c7733deac0bb4baabb56514d3876e1cf | 13.404 | 0 / 0 |
| domains | 8ac691aea34949a983e1b1a437391dff | 112.899 | 2 / 5 |
| formal | d47e6f4e44a945348013b70354b7c2a9 | 380.953 | 0 / 0 |
| gate | 902b794b584d4888b093f0cba6e77902 | 238.624 | 18 / 5 |
| gate-audit | 9653fc42975a45279e0df1e430c5e1a3 | 188.685 | 14 / 5 |
| host | 5113da350d0446c08d42434f6225d806 | 196.324 | 0 / 0 |
| lint | 1c936c05c788433889dbb0757e5fea9e | 121.951 | 0 / 0 |
| linux | 3951d5fc5be54a09a107f40d78fbd058 | 418.666 | 25 / 13 |

BIOS确有36组72个唯一(case,run)，默认双轮已含其中；Gate九配置18轮，审计停止七site双轮。
普通host1142通过、1默认忽略；linux组另执行L1解析正反例。D14仍仅人格桩，formal重跑既有模型/细化检查，
没有新增定理或扩张实现映照。此结果补齐本片完整回归的干净远端证据，不证明本机超时已修复。

逐步归档只保留每一步结束时仍存在的串口文件；普通BIOS不宣称72轮各有独立原始文件。
L1参考和两轮Prima各有独立归档。Actions未上传镜像字节，下面是manifest记录的最终制品身份，
没有将其当作镜像字节独立重哈希、每一步旧镜像保留或UEFI执行证据。

| L1干净远端轮次 | Hz | p50/p99 ns | 串口SHA-256（各14218字节） |
|---|---|---|---|
| 1 | 2433958682 | 6425229 / 6780696 | 0cb6bf9cd592ea42ef67a96661e0074e029808b57b2f031ca14f952def6241c1 |
| 2 | 2435609957 | 6649133 / 9030540 | 605b7d6f7d3f990457ea1df4247519b3ca4e2e4eb5a79081779e24d61f5f1de9 |

Linux参考5611/154815 cycles，5244字节原始串口SHA-256=
081909a3b28317412c273a89568d69e0c7fb7162f67ae5e6272f3d34455808e5。
新探针9120字节及de5fb57055f86dfc7ef7b8f57e4b3f04c62195f68df1749d973b3911b8351478的身份与本地记录相同。
服务3195616字节，78f44bbb6d911dfce8d3b8006f8f2ed9a1ef5b6ad3383988e1dd245c3987e9a7；
最终Linux探针kernel ELF2705184字节，0ccfbc84ca7606720649a40ddf576e068727ff1c30c0b9cff22ed1fde7cfe73f；
BIOS7832576字节，77c12a2deb9344a7adf0e8e4e377358ef0ebf4259433945b3f8bfd95f4ad8189。
本地与远端的宿主、构建路径及产物分别登记，不主张跨环境位级复现或从耗时差异推导优化收益。

## 失败与限制

- 两次起步分别因PRIMA_LINUX_ELF采用相对路径及不存在的本工作树缓存路径而编译退出101；改用/usr/bin/busybox后通过，并核对其为D12固定ELF哈希，再复制到CI预期缓存路径。未替换或修改ELF字节。
- 首次聚焦运行因沙箱DNS限制下载Linux参考内核失败，退出1；宿主权限重跑下载后强制固定SHA512，参考及Prima双轮通过。聚焦结果不混入正式基线。
- 普通BIOS首轮共7次30秒超时：T1第2轮、V1双轮及V2第1轮输出为空；F1第1轮最后到vspace_ready，F1第2轮及V2第2轮最后到cow_ready，无panic证据。原step完整输出保留在ignored target/ci/bios/8c53bbe09d9b47e99ac942c42eaf42bd/00.log。本次只记录已报告的4组，V2后正在构建下一组时主动SIGTERM，由运行器杀子进程、保存8份串口并标interrupted；不是36组完成，也不把17轮拼成完整通过。
- 诊断时宿主loadavg一度16.29/10.85/7.05（8逻辑CPU），CPU PSI some avg10=51.76%；观察到多项外部CPU密集进程，仅有本工作区一个QEMU，无其它本工作区测试并行。资源争用是实测环境，尚无超时因果证明，未停止他人进程、修改源码或放宽门限。后继改用干净CI验收；其通过也不能宣称已修复本机超时。
- 宿主压力短时回落（CPU PSI some avg10=1.54%）后，对干净e6dce41执行python3 scripts/ci.py bios --require-clean。运行ID bcd6620dc7a647d68b68d1b5efb15fc3、184.614秒，6轮通过、E1第1轮30秒空串口超时；E3第2轮时再次主动中断。manifest仍为interrupted/step exit130，保存3份串口且长度/哈希核对通过，源身份与远端实现提交完全一致。两次中断不合并成完整通过；后继远端结果独立计量，不能倒推本机超时根因。
- 仅17字节有限RAM文件read；seek/open/close/输出不在单次区间，不代表完整文件事务、磁盘、网络、动态链接、所有LibOS操作或其它personality。无SMP、KVM、实机或新Lean证明。
- 五类基线的调用、样本数、平台和计量边界分别定义，不横向比较为同一总体；性能门槛与稳定性仍须后继冻结/验证。容量和全局审计满fail-stop规则保持。

## 后继关系与阶段影响

本文只在C1自身远端结果和L1有界单核测量两项上补强前篇；发现、失败及IPC排序整改链均保留。
[阶段跟踪](../../../Prima%20OS%20实现阶段.md)维护多核、数值门槛、发布许可/镜像包与九轨剩余机制。
实现提交e6dce41自身干净远端九组及原始证据已取得；本机超时仍是未定位的稳定性限制。
本文不签署7-0或其它子轨，也不扩大已有profile或证明范围。

本次后继文档的self-test/build/check/check --base 8933a5c/check --base e6dce41及git diff --check均退出0；393份文档/3741节，补录后重新生成并核对。
