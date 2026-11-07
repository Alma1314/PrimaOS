---
id: report-stage7-ipc-benchmark
kind: report
authority: scoped-implementation-and-validation
status: ipc-roundtrip-locally-verified
lifecycle: historical
updated: 2026-11-07
---

# 7-0 I1授权IPC往返基线与N1远端后继

## 结论与范围

新增I1：两个未绑定Gate的真实ring3 Task通过各自TaskSend授权，用SYS_SEND/SYS_RECV_META
完成带序号的请求/答复，64次往返一轮、两次独立启动。全部用户端点、两侧payload、
认证发送者及msg-id保留在既有私有页；每条消息匹配唯一发送/接收审计，路径计数为
128次直投、128次阻塞、0次排队。无新生产ABI、映射、快路径或内核采样钩子。
[实现前方法](../specs/2026-11-07-stage7-ipc-benchmark.md)限定user-wrapper/dev/单核TCG范围。

本轮八配置16次启动、host、lint/特性Clippy与普通BIOS回归结果见下表；原始样本/哈希/分位数独立核对。
上一提交38f92bb已取得自身干净远端九组及90份归档核验，补强
[N1此前仅本地的范围](2026-11-07-stage7-native-syscall-benchmark.md)。
五类基准中Gate、原生syscall及IPC各有有界单核基线；页故障/COW、端到端LibOS、
多核、数值门槛与发布许可检查待独立完成，九轨退出不改变。

## N1干净远端后继

本次gh run view 42369265416 --json status,conclusion,headSha,jobs,url及
 gh run download 42369265416 --dir /tmp/prima-stage7-n1-remote，均退出0。
[Actions 42369265416](https://github.com/Alma1314/PrimaOS/actions/runs/42369265416)整体及九job均success，
九份manifest均revision=38f92bb58a084651fcc2140ab9e3dd2c4aa68625、dirty=false，源摘要：
0c7accb46e10194560c43c1ae85b3b1c1727af1b171d0f7fdbf9a2d2c2386ec4。
Python逐份检查所有步骤passed/exit0、九组集合、90份归档长度/SHA-256；
从N1两轮各128行端点/返回值独立计算nearest-rank分位数，与07.log摘要逐字匹配，退出0。

| 组 | manifest运行ID | 秒数 | 原始归档 / 最终制品身份数 |
|---|---|---|---|
| host | c5ff18e759d64428a4b57b0db1d8b474 | 160.990 | 0 / 0 |
| lint | dd1f91b5069d4114b2d7d0b49fa9fe2f | 139.445 | 0 / 0 |
| docs | 70ec67fa5a234e4296f4ed5f777ab122 | 10.718 | 0 / 0 |
| bios | 800a6df8731c4dc99085a811f63f3f6e | 764.179 | 38 / 38 |
| linux | 241f48ead4fd4c42b96374deabe8ca8d | 178.900 | 22 / 12 |
| gate | fda54ce241214db38b1d160ca825aa3c | 289.108 | 14 / 5 |
| gate-audit | 74df173cc8ea4dc08398a4e6dbeb4505 | 310.429 | 14 / 5 |
| domains | 584415b66f4a4f099faead89b809fa82 | 127.450 | 2 / 5 |
| formal | bd7bbf55c1974c0f85019f7a866b7b76 | 379.802 | 0 / 0 |

BIOS仍36组72轮，已包含默认启动双轮；归档包含空错误汇总。
远端Gate七配置含N1，尚不含本轮I1；domains仅D14人格桩，formal维持既有模型范围。
Actions保留清单及串口，不上传镜像字节；制品栏是最终文件身份而非每步旧镜像。

| N1远端轮次 | PIT/TSC Hz | user-total p50 / p99（ns） | 原始串口SHA-256（各8216字节） |
|---|---|---|---|
| 1 | 2784139199 | 24733 / 42507 | b318ad6edf2b1335afe12dd3eb4e9afc8cc2b6917dffe67b3d8997578d417c8a |
| 2 | 2783516908 | 24742 / 41628 | 53097642f3240d0918f82461f83baee0b63a8573bebd5450cf22598e829ddf55 |

远端最终N1 BIOS身份9da2db3871342ff12b22d3dd7d6b0c7963578c4f4c737f6b4fee423dc1830db4，
boot内核ELF为87d850dd62e9627ff1b803a3b93f7f2d5eef5a866b9f3ed5480f6f51b8e6f393。
这些数据与先前本地128样本分别报告，不混合宿主或声称跨宿主性能可复现。

## 实现与验证边界

kernel/src/ipc_bench.rs复用既有字节发射、安装、用户时间戳及十进制导出。
Task 1先阻塞接收，Task 2发送倒序序号并阻塞等答复，服务逐次核对序号并加65536回发，
最后让出使客户端到达int3。每次send返回0，服务收到请求及客户端收到答复均在用户态检查。
元数据在测量完成后由内核逐条核对实际Task代次与不可重复msg-id；审计从初始化后检查点扫描，
每条恰一发一收且顺序正确，总数均128。复用现有ipc_witness路径计数快照作增量检查；
ipc_receive_counts无额外结束/未决接收，两个Task无pending消息。
未附加Gate并拒绝新增GateTrapped/Delivered/Terminated/ControlRejected四类事件，
不声称此枚举覆盖所有Gate事件。完成等待/Gate良构断言保留。

客户端64字节×64=4096，服务48字节×64=3072；页容量编译期检查。
每轮独立nearest-rank排名32/64；不丢冷样本或扣除计时/参数/协议检查成本。
边界内包含两次发送、两次接收、服务序号检查与调度/CR3切换；
末次服务让出和普通轮次服务再次阻塞都在该轮区间内，不将其冒称仅内核IPC成本。
原G4/G5/N1格式、样本数及断言不改；CI只追加I1双轮，保留30秒单轮上限与1200秒组预算。
xtask解析器拒绝错误身份/代次、payload、元数据、消息重放、样本/时间/格式及panic反例，复用既有统计函数。

## 本地命令、输入与结果

基线38f92bb；四组运行均dirty=true，源摘要be36248907621aa7aded976777df7c3496028e2873a3aeb224b2c25e4c288a53。
这些是未提交增量的本地结果，后续只补充文档与生成目录，不能标成新提交的干净远端运行。

| 命令 | 退出结果 / 秒数 / manifest运行ID | 范围 |
|---|---|---|
| CARGO_NET_OFFLINE=true cargo test -p xtask benchmark | 0；4项通过 | G4/G5/N1/I1解析、变动时长及身份/回放/格式反例 |
| CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=I1 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings | 0 | 最终I1特性检查 |
| python3 scripts/ci.py gate | 0；307.449秒；bfdca78dfa1c46d787da093c118fd7d7 | 八配置各双轮、16份原始串口、5个最终I1制品身份 |
| python3 scripts/ci.py host | 0；9.480秒；d884840d65c545009c5fa2035cfb8e37 | Rust1138通过/0失败/默认忽略1；musl7、POSIX std3、CI运行器11 |
| python3 scripts/ci.py lint | 0；13.515秒；dc4fe5861df34a80aee4cb60d20fd420 | fmt/Clippy/裸机编译/43文件零unsafe；TCB2087/39170/41257=5.06%，unsafe79/0，include_bytes0、prima_user_dep=false、protocol_lines56 |
| python3 scripts/ci.py bios --prepare | 0；1608.210秒；8b7c7ef88fcd4e7f8c271ea3f80440db | 完整36组72轮，含默认启动双轮；38份归档、38个最终制品身份 |
| Python独立核验 | 0 | 四组同源、54份原始长度/哈希、43个最终制品文件身份；I1两轮全部回执及分位数精确匹配 |
| python3 scripts/docs.py self-test；build；check；check --base 38f92bb | 0；四项通过，388文档 | 文档元数据、目录、引用、锚点及新增文件基线门禁 |

BIOS输入准备22.040秒，完整验收步骤1585.301秒；表中1608.210秒包含组级准备及记录开销。

首次聚焦命令CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=I1 cargo run --locked -p xtask --features gate-probe -- test-gate亦退出0，
两轮各64条；正式基线以下述完整八配置归档为准，不混合聚焦样本。

## I1单核基线v0.1

AMD Ryzen 5 3500U、8逻辑CPU、Linux 7.0.0-42、glibc 2.39、QEMU 8.2.2；
镜像pc-i440fx-8.2/TCG单线程/qemu64/1vCPU/128M/BIOS，默认dev内核未release优化，两个Task IF=0。
未控制宿主独占负载，不作KVM、实机、跨核、固定频率或统计稳定性主张。

| 轮次 | PIT/TSC Hz | user-total p50 / p99（ns） |
|---|---|---|
| 1 | 2071965542 | 1974387 / 8857322 |
| 2 | 2086938417 | 1502163 / 8218437 |


| 本地I1项 | SHA-256 |
|---|---|
| traces/07/S6GATE-1.log（8882字节） | aa9448be075e104869b4540826e28523eba15063b754109c58a21957928514ec |
| traces/07/S6GATE-2.log（8882字节） | 731c7be79a16068af3b4a260c8384726349770d28cf1388ee56bbad3ed4207e6 |
| 最终prima-kernel-boot.elf（3017832字节） | 53348a05f473a6090ecf757ec286b197e8b7288a491f4d04615d0cab650a0644 |
| 最终prima-os-bios.img（12026880字节） | 8f5bb395bec9b8245664c5005dd017596a2c5a62952c1eea25d9150b09db948a |


### 独立重算

从本地gate manifest定位env.PRIMA_GATE_CASE=I1的两份traces，先校验size/SHA-256。
严格解析头中的Hz/双方身份及每行15列，核对全部序号、零cap、认证身份和递增消息ID，
再以d=sorted(user_end-user_begin)，p50=d[31]*10**9//hz，p99=d[63]*10**9//hz重算；
精确匹配07.log两条PASS S7IPC摘要。原始日志保持ignored target/或/tmp/，不新增日志/哈希附件。
本地五个最终I1制品亦逐个读文件复算长度/SHA-256；未执行UEFI。

### ISSUE-IPC-AUDIT-ORDER-001：直投审计排序偏差

源码复核发现既有偏差：抽象规范§6.3按MessageSent→TaskStateChanged(Ready)→MessageReceived列出直投事件，
38f92bb的send_message_with_capabilities却先调用set_pending_task，后者写pending、唤醒并追加Ready审计，
然后才追加MessageSent与MessageReceived。此处是源码观察，未另行导出整段原始审计作动态排序证明。
本轮只检查每条MessageSent先于匹配MessageReceived，不验证三者的完整序列等价。
偏差登记在阶段跟踪器，未修改生产排序，也未将现行实现提升为抽象规范；
后续须单独审查所有set_pending_task调用者、审计失败原子性与对应正反例后整改。

## 失败、限制与阶段影响

本轮未观察到I1执行失败，四个host解析测试及完整回归结果按上表记录；
前序CI的bootloader输入缺口、归档遗漏、历史BIOS超时及N1初始化复核仍按各原报告保留，
本轮PASS不解释所有历史根因。普通BIOS已有I1标签，与本轮PRIMA_GATE_CASE=I1不同。
既有[直投唤醒审计预算R3](2026-11-06-stage0-6-review-remediation.md#r3--p1阻塞ipc直投少预留唤醒审计)
修复及其精确余量失败反例继续适用，本片不改预算或扩大容量。
只覆盖无cap直投往返，不代表排队/带cap性能、同步call/reply快路径或≤250ns目标；
没有新增形式化证明或扩大兼容profile。页故障/COW、端到端LibOS、多核、数值门槛及发布检查仍待完成。
新I1提交仍须自己的远端CI，不能用父提交九组通过替代；九轨退出均不勾选。
