---
id: report-stage7-ipc-audit-order
kind: report
authority: scoped-remediation-and-validation
status: locally-validated
lifecycle: historical
updated: 2026-11-08
---

# 7-0 IPC 审计排序整改与 I1 远端后继

## 结论与范围

整改 ISSUE-IPC-AUDIT-ORDER-001：Task 发送、异常通知、Timer 通知均在能力迁移及唤醒前追加
MessageSent，直投遵循 Sent → CapabilityMoved（每个携带能力）→ Ready → Received，
排队发送遵循 Sent → CapabilityMoved。依据是抽象规范§6.3/§9，而非将既有实现提升为契约。
I1 测后校验加强为逐条精确检查 Sent → Ready → Received；测量端点、样本数、用户协议、
输出格式、授权、队列与审计容量保持原范围。本报告是同一会话的实现/测试复核，不是第三方审查或形式化细化证明。

2e3b95f 自身远端九组及92份归档核验通过；本次新增代码的本地回归见后文，仍待自身远端结果。
五类基准仍只有 Gate、原生 syscall、IPC 三类有界单核基线；页故障/COW、端到端LibOS、
多核、数值门槛及发布许可检查继续待完成，九轨退出均不勾选。

## 基线与审查链

基线2e3b95f8d0416c7f7e658ab7c6261e1bd6f49b3d，开始时工作树干净。
输入：[阶段跟踪](../../../Prima%20OS%20实现阶段.md)、
[执行计划](../plans/2026-11-06-stage7-execution.md)、[抽象规范§6.3](../../../COSIR%20原语规范.md#63-内部辅助操作-enqueue_or_deliver)、
[IPC设计](../specs/2026-09-16-stage2-ipc-user-surface-design.md)、
[位置审计首片](2026-10-13-ipc-capability-move-audit-report.md)、
[携cap直投整改](2026-10-18-stage2-ipc-review-remediation.md)、
[精确预算R3](2026-11-06-stage0-6-review-remediation.md#r3--p1阻塞ipc直投少预留唤醒审计)、
[I1发现报告](2026-11-07-stage7-ipc-benchmark.md#issue-ipc-audit-order-001直投审计排序偏差)与
[测量方法](../specs/2026-11-07-stage7-ipc-benchmark.md)。

宿主AMD Ryzen5 3500U/8逻辑CPU、Linux7.0.0-42；使用仓库固定 nightly-2026-07-30、QEMU8.2.2，I1仍是pc-i440fx-8.2/TCG单线程/qemu64/1vCPU/128M/BIOS/dev。
无KVM、真实硬件、多核、冷样本丢弃或计时开销扣除；本次不据前后延迟差异推断优化收益。

## 根因、调用者与失败原子性

全仓库检索 set_pending_task：三个调用者是 send_message_with_capabilities、exc_raise、enqueue_kernel_event；
后者只由 timer_set 立即触发与 advance_time 到期触发调用。set_pending_task 同时写 pending、
将 BlockedRecv 改为 Ready 并追加状态事件，不能删除它的状态审计来掩盖排序。

既有三条路径都在调用后才记 Sent；进一步复核发现 Task 的排队及直投携cap路径也先记 CapabilityMoved。
将每个生产者的重复 Sent 分支合并到投递前，保留所有前置/预算与后置 Gate 权限丢失提交。
不增加容量、不增加新的原语/ABI/生产采样，不改 pending 与 Ready 原子写入。

| 路径 | Sent之前的检查与提交保证 |
|---|---|
| Task send | 双方存活、接收者绑定TaskSend、cap数量/重复/live/位置/结构根限制、消息ID、队列、完整Gate丢权及审计预算；位置迁移所需的HeldBy已验证，随后无新正常失败域 |
| 异常 | 活动订阅者由登记/Task销毁维护；预检全部订阅者的ID、队列和ExceptionRaised+直投三条/排队一条预算；硬件形态另预留fault状态事件 |
| Timer | 活动接收者由timer_set及Task销毁清理维护；timer_set立即触发保守预留六条，advance_time按每个接收者仅第一条可直投计算4/2条预算；时间/ID/队列均在写入前检查 |

新增三个host测试覆盖Task发送直投/排队×0/1/2个cap、异常直投/排队、Timer立即/到期×直投/排队。
每种场景先以审计预算差一条断言AuditFull且整个BootstrapState不变，再给足预留预算成功，检查精确事件位置与消息数据。
Timer立即排队仍保守预留六条、实际使用四条，未将预留上界误写为实际消耗。
原R3精确余量测试保留，已有带cap直投测试更新为完整四事件顺序。
I1在128条真实ring3请求/答复的每条Sent和Received之间核对唯一相邻Ready事件；只覆盖无cap直投，不是完整审计轨迹导出或全内核序列证明。

## 2e3b95f 干净远端后继

本次 gh run view 42374364021 --json status,conclusion,headSha,jobs,url 与
 gh run download 42374364021 --dir /tmp/prima-stage7-i1-remote 均退出0。
[Actions 42374364021](https://github.com/Alma1314/PrimaOS/actions/runs/42374364021)整体和九job均success。
九份manifest均revision=2e3b95f8d0416c7f7e658ab7c6261e1bd6f49b3d、dirty=false，
files_sha256=48f7f520d1d49f43156604d2e786b0821a7e344e9501230c8f456cab5c67fc38。
逐个校验所有步骤passed/exit0、九组集合及92份归档长度/SHA-256，并独立重算N1/I1两轮分位数、
核对I1全部认证身份/载荷/cap元数据/消息ID，与对应步骤摘要精确匹配，退出0。

| 组 | manifest运行ID | 秒数 | 原始归档 / 最终制品身份数 |
|---|---|---|---|
| host | 4392e2a255b04b3abfa6ee77a9b3d221 | 198.916 | 0 / 0 |
| lint | 1600404372cc4e30b6ea692a94bbfee6 | 133.587 | 0 / 0 |
| docs | b7267a1dd149472392767e638752635c | 11.632 | 0 / 0 |
| bios | 32cde2ec8cde4dd4b221f560c38c50e5 | 781.295 | 38 / 38 |
| linux | e5c74ebaccb24e52a1956645fd670b0c | 245.038 | 22 / 12 |
| gate | 39c016bde04d48098ef511f9b10211ab | 350.237 | 16 / 5 |
| gate-audit | 56ade8f1e79948fdb3637fe616afa491 | 284.420 | 14 / 5 |
| domains | fff96613bfdb48eab30f668285959ded | 163.891 | 2 / 5 |
| formal | 088878d48af342a581b4a0cd60862a32 | 373.907 | 0 / 0 |

远端BIOS36组72轮已含默认启动双轮，归档含空错误汇总；Gate八配置16轮。
domains只覆盖D14人格桩，formal只覆盖既有模型。Actions归档串口和清单，不上传镜像字节；
制品身份是每组最终文件，不能冒充每步旧镜像。

| I1轮次 | Hz | p50/p99 ns | 原始traces/08串口SHA-256（各8882字节） |
|---|---|---|---|
| 1 | 2437802624 | 855820 / 4588815 | f06b82c727dfbae680f88fbff625d1dd42c6199a4055f0b7962e9272d79f97f3 |
| 2 | 2437022335 | 884525 / 4312945 | b9155abc23f5b469ca136dc047d9136db41ef7dca38f29133c04cd85858b954a |

最终Gate ELF：3013736字节，dd61855c5bd25caa459c7c85d2d04564f06ddf0db61e98ced165dc601b0e53bf；
BIOS：12026880字节，d3348b69cd9faf3e6448b90ea63dacb75a66991a040146207d9decd47a5c4b07。
这些是v0.1旧审计校验的干净远端样本，不能反称已通过本次加强后的检查；UEFI未执行。

## 本次命令与结果

本地host/lint/Gate/BIOS的代码采样基线均为2e3b95f dirty，源摘要
fabd06cb8af9dd25ec78a6796291610426509f4799828a5308a5d3bd5304ab0a；
仅kernel/src/bootstrap.rs与kernel/src/ipc_bench.rs改变，随后补录本文及索引/阶段/方法/目录。
不将本地dirty采样改称新提交的干净远端结果。

| 命令 | 本次结果 | 范围/身份 |
|---|---|---|
| CARGO_NET_OFFLINE=true cargo test -p kernel | 退出0，565通过、1项既有忽略 | 新增三个排序/精确预算测试及原R3回归 |
| python3 scripts/ci.py host | 退出0，1141通过、0失败、1项既有忽略；musl7、POSIX std3、CI运行器11通过 | 482745637b094fb69e631f1a61b7c9bf，13.679秒 |
| python3 scripts/ci.py lint | 退出0，fmt/Clippy/裸机kernel/43文件零unsafe/personality护栏通过 | 6253d7f70fa048fba190a18771e41957，26.652秒；含tcb |
| CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=I1 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings | 退出0 | 生产特性及I1测后断言编译检查 |
| python3 scripts/ci.py gate | 第二次命令退出0，八配置16轮、16份归档、5项最终制品身份 | aa92c046ed674b899548b145f6be864b，341.324秒；首次锁冲突见下节 |
| python3 scripts/ci.py bios（宿主权限） | 最终命令退出0，36组72轮、38份归档、38项最终制品身份 | 3b7dd143cd4545919494426023e11465，1413.219秒（验收1412.378秒）；含默认启动双轮、W4/W7/W7b/W9、普通I1及R3 Linux参考 |
| python3 scripts/docs.py self-test | 退出0 | Markdown/metadata/search/determinism/lifecycle/Git recovery/change guards |
| python3 scripts/docs.py build / check / check --base 2e3b95f | 三项均退出0，389文档、3715节 | 文档与新增元数据/基线检查；补录结果后再次生成/检查 |

TCB实际输出trusted_lines=2087、mechanism_lines=39386、total_lines=41473、ratio=5.03%；
unsafe=79/0，include_bytes=0、prima_user_dep=false、protocol_lines=56。
这是该计数脚本对本版源码（含测试）的统计，不是缩减生产可信基的证明。

四组manifest的status均passed、所有步骤exit0、源身份精确一致；逐个重算54份原始归档的长度/SHA-256，
逐个读取43项当前最终制品重算长度/SHA-256。BIOS解析为36种case、72个唯一(case,round)，无遗漏/重复。
Gate八配置选择器与16份归档逐步匹配；从I1两轮全部64行回执重算nearest-rank排名32/64，
ns=floor(cycles×10^9/hz)，与07.log摘要一致。原始输出留在ignored target/及/tmp/。

| I1整改后本地轮次 | Hz | p50/p99 ns | traces/07串口SHA-256（各8882字节） |
|---|---|---|---|
| 1 | 2087723405 | 2019365 / 8097910 | 859e7aeecd494f31733f1d006dc9f88411ec51edb88b2fe928d32224679a7b45 |
| 2 | 2071150455 | 2174462 / 9639678 | ac7ee4c977bc5b43da2869e35a453eaccc88563476cde3fdbaf7b0fe93b4ad13 |

最终I1 ELF：3017832字节，6656e56a64c271a1544f973e04a5fd3a673ad4cbd8e75f838bb6883d9a878f7e；
BIOS：12026880字节，578ac620a224e5d4d0fa93619a487d32f16b90b73285554fc9aea36237341d81。UEFI未执行。

## 失败、反例与限制

- 修复前 cargo test -p kernel audit_order 退出101，三个新增测试均失败：排队携cap时首事件为CapabilityMoved，异常/Timer直投时首事件为Ready，预期均为Sent。修复后通过；首次编译因测试常量缺少super::限定而失败，补限定后才取得上述排序反例。
- Gate初次启动与尚未结束的lint竞争共享锁，运行370550cae3ae427ca8ee948ae07af39d在0.001秒退出1，source=null、steps=[]、Resource temporarily unavailable；无guest验收开始。等待lint完成后显式重新运行，保留首次失败记录。
- 普通BIOS首次在沙箱执行，Linux N1本机UDP参考得到全部零且exit1；运行3a8c8e2c54784e8c8a0a886efea51dd2经显式SIGTERM收尾，129.669秒、status=interrupted/step exit130，不计完整通过。首次中断只结束了启动shell，子运行器仍持锁；宿主重跑623f52362cdf4b72b8af0f3e5bb67e21因锁未释放在0.001秒退出1、steps=[]。确认具体PID后向运行器发SIGTERM，运行器保存中断并清理整个子进程组；确认无残留后再次宿主重跑，Linux UDP参考通过。未改测试或错误码。
- 远端归档辅助校验初版未过滤prepare-inputs步骤，配置数量断言拒绝；改为筛选test-gate步骤后重新逐份核对通过。不是guest或归档哈希失败。
- 历史超时、原I1计数偏差及CI输入/归档失败按前序报告保留；本轮通过不解释其全部根因。
- 无新增Lean证明、跨节点审计排序、SMP或完整COSIR细化主张。ISSUE在所列本地Task/异常/Timer事件排序范围技术收口；阶段和兼容profile不扩大。

## 后继关系与待办

本文在事件排序及I1自身远端证据范围补强[I1首片报告](2026-11-07-stage7-ipc-benchmark.md)，
取代其“排序仍待整改/新I1尚无自身远端”的当前结论，保留当时观察、方法与数据。
[阶段跟踪](../../../Prima%20OS%20实现阶段.md)继续维护7-0其余两类基准、多核、性能门槛与发布检查。

后继：[C1基准与a40c487自身远端核验](2026-11-07-stage7-cow-benchmark.md)补充九组/92份归档，
在自身干净远端范围关闭本文“整改仍待远端”的当前待办，保留本文原本地数据及失败记录。
