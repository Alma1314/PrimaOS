---
id: report-stage0-6-review-remediation
kind: report
authority: scoped-implementation-and-validation
status: verified-with-bounded-remediation
lifecycle: historical
updated: 2026-11-06
---

# 阶段0–6实现审查与跨阶段整改

## 结论

本轮修复8类跨阶段问题：共享能力持有者、旧代次容器清理、IPC直投审计预算、Task创建完整预检、
Linux共享信号清理与writev故障/进度语义，以及Namespace路径/局部计数。
工作区1132项、Linux特性1133项通过，默认忽略的静态ELF样本测试另行通过；
lint/特性Clippy、Lean主模型/有限表检查、Linux11项双轮、Gate全部专项及最终单次完整BIOS72/72均通过。
源码细化因缺少锁定Aeneas未复跑；Namespace既有槽保留等边界不扩大。以下按实际证据范围解释。

## 范围与依据

按作者“审查并完善7阶段以前的所有实现”的指令，范围为阶段0–6现有实现，基线
`b70abf3`（RV-1审计承载后继），开始时工作树干净。以[阶段跟踪器](../../../Prima%20OS%20实现阶段.md)、
[白皮书](../../../Prima%20OS.md)、[原语规范](../../../COSIR%20原语规范.md)与
[Lean映照](../../../lean/README.md)确定职责和已有证据边界。

本轮是跨阶段源码审查与回归补强，不重复组织级出口签署，不启动阶段7，也不扩大冻结的
[阶段5 profile v2](../specs/2026-10-25-stage5-profile-v2.md)或
[阶段6 Linux profile v1](../compat/stage6-linux-profile-v1.md)。
原生线程共享能力空间已经由[进程/线程设计](../specs/2026-10-23-stage5-m15-process-thread-wait-design.md)规定；
修复提交路径使其遵守已有授权语义。Gate仍绑定固定服务Task，消息仍以真实Task为端点，
能力的holder采用Process leader；本轮没有将这三种身份合并。

审查按主题阅读了设计、执行计划、发现及后继整改链，包括
[阶段0–5补强](2026-10-25-stage0-5-gap-followup.md)、
[阶段1–5后继](2026-10-31-stage1-5-regression-remediation.md)、
[阶段5出口](2026-10-27-stage5-exit-signoff.md)、
[阶段6统一验收](2026-11-02-stage6-process-and-differential.md)、
[阶段6独立审查](2026-11-06-stage6-implementation-review.md)、
[首轮整改](2026-11-06-stage6-review-remediation.md)及
[RV-1后继](2026-11-06-gate-request-audit-history.md)。历史失败和局限保留。

## 审查覆盖

| 阶段 | 本轮核对的实现与契约边界 | 验证/处置 |
|---|---|---|
| 0 | cosir操作、CDT/位置与事件模型；Lean字段映照与显式遗漏；SPEC-145有界冻结出口 | workspace模型/映照回归及Lean重建；没有把模型定理扩张到完整内核 |
| 1 | OSTD同步/一次发布/POD与恢复帧、帧归属、页表、异常入口和内核零unsafe；SPEC-146 | host、lint、BIOS基础启动；未发现需新增架构的实现变更 |
| 2 | 能力槽/代次/历史、权限、Task创建/销毁、IPC队列与直投、失败前置；SPEC-147及IPC整改链 | 修复R1–R4，保留共享能力与真实Task端点的区别 |
| 3 | Path构造、Namespace绑定/挂载/解析/清理、Timer接收者与审计预算；SPEC-148 | 修复R1/R2/R7/R8；未改变Timer接收Task身份或命名空间抽象契约 |
| 4 | runtime能力/IPC包装、服务握手和分片、VFS会话与请求顺序、manifest及用户驱动边界；SPEC-149 | host与既有裸机服务程序回归；保持单客户端/单在途等已登记限制 |
| 5 | VFS内容别名/句柄/路径/打开/重命名、进程线程共享空间、fork/exec/COW、信号、原生/posix std与musl接口 | R1实际ring3探针；完整BIOS及musl/std构建护栏；保持SPEC-152有界签署范围 |
| 6 | Gate授权/请求历史/一次性完成/容器撤销、管理装载、上下文与FPU、Linux线程/信号/futex及向量I/O | R2/R5/R6；Gate、停止边界、D14与同ELF差分按下表实际结果解释 |

这是围绕权限、事务、内存和生命周期的跨层审查；不声称逐条穷举所有机器状态、证明所有Rust函数或排除所有缺陷。

## 发现与修复

### R1 / P1：非主线程授权与能力提交使用不同holder

`authorize`已按`space_of(thread)`检查，但派生、Namespace创建/绑定/取出/挂载/解析、带能力IPC、
Task创建部分检查和提交仍使用线程Task ID。非主线程派生实际复现`NotHolder`，
带能力发送复现`CapabilityHolderMismatch`；某些操作还可能在写入对象后失败。

将这些能力持有者统一到共享能力空间，并同步铸造/派生/移动审计中的holder。
Gate/Timer/设备/WaitObject的铸造事件也遵守同一口径；对象creator、Gate固定服务者和消息端点仍保留实际Task身份。
增加host非主线程派生/Namespace、排队发送/反向直投及Task创建检查；扩展既有W5/SHARE探针，
从真实ring3线程派生共享WaitObject能力并据此唤醒主线程；
Namespace创建/绑定/挂载/解析/解绑与多Namespace局部计数保留为host覆盖。
新增`cargo xtask test-process W5`聚焦入口，复用完整套件的双轮运行、协议和IPC断言。

### R2 / P1：旧代次Namespace条目能删除复用槽的新位置

`CapabilityPositionTable`查找/移动/删除以及活动记录删除仅按index，未比较完整handle。
Gate请求终结会撤销Namespace中的别名，但容器条目按既有协议保留。
随后复用该槽并销毁Namespace，旧条目会删除新能力的位置。
新增反例在修复前明确失败：`holder_of(replacement)`为`None`，应为`Some(2)`。

在表层统一比较完整句柄，覆盖查找、可变查找与删除；活动记录删除也校验代次。
回归构造真实Gate交付→派生别名→绑定→答复撤销→槽复用→旧条目清理，验证旧token不能移动/删除新记录，
新位置保留且`validate_authority()`通过。不复活旧token，不更改保留历史或只追加审计。

### R3 / P1：阻塞IPC直投少预留唤醒审计

带能力直投原预检只计算`MessageSent`/`MessageReceived`及能力移动，漏掉`BlockedRecv→Ready`。
仅剩精确余量时，已移动能力、写pending并唤醒后，追加审计触发断言。
修复前测试实际触发`audit append`容量panic；修复后将唤醒事件计入预算，返回`AuditFull`且完整状态不变。
不依赖增大审计容量掩盖边界。

### R4 / P1：Task创建没有一次预检完整能力与审计预算

成功路径需manager根、self派生、root_ns派生三条新能力，以及四条基础事件（含TaskCreated），
原先活动槽数估算忽略保留/永久退役槽，审计只预留两条，也未预检通用历史和CapId后继。
初始能力逐条移动还可能先触发Gate孤儿化，撤销下一条尚未转移的请求能力。

预检三条普通可用槽、通用历史、CapId和四条基础事件/全部转移/可能的Gate终结预算，再分配页表。
所有initial_caps先完成位置转移与审计，再一次提交已预检的Gate权限丢失计划。
host测试覆盖审计恰差一条、历史不足、CapId不足、所有空槽永久退役及非主线程的合法前置；
各容量拒绝均在分配页表前且状态不变。该测试不等于全部分配失败路径的形式证明。

### R5 / P2：Linux共享SIG_IGN未清除兄弟线程的旧pending

依据[Linux信号契约](../specs/2026-11-01-stage6-linux-signals.md)与
[线程契约](../specs/2026-11-01-stage6-gate-threads.md)，处置共享而pending/mask按线程区分。
原实现只清当前线程pending；其他线程阻塞的旧信号在IGNORE→重新安装handler后可能复活。

成功提交信号操作后，清除进程及全部线程中显式SIG_IGN的pending，再路由。
保留失败操作的提交顺序。host同时验证旧pending消失、其他信号保留和重新安装后新信号仍可入队；
扩展M4真实pthread程序，在兄弟线程阻塞/排队SIGUSR2，主线程切换IGNORE再恢复handler，随后解阻塞并join，要求不交付旧信号。

### R6 / P2：Linux writev逐条导入描述符导致错误时已经输出

原实现读一个iovec就输出，后续描述符跨不可读页时返回EFAULT却已输出前缀；
后续payload或输出失败还会丢失已完成的字节计数。

先一次导入有界iovec表并检查总长度，再逐项读取payload/输出；后续失败保留已提交进度。
保留profile原有最多16项、每项最多4096字节等边界，没有新增Linux调用或放宽白名单。
host测试覆盖描述符失败零输出、后续payload/输出失败保留进度、长度溢出和零项空指针；
M3差分夹具增加描述符跨PROT_NONE页的EFAULT与后续payload故障的短写。
Linux参考和Prima执行完全相同的新ELF，差分断言保留。

### R7 / P2：Path::extend可构造parse拒绝的路径

Path组件约束为无`/`的ASCII；extend原允许`b"b/c"`及非ASCII字节，破坏组件和render/parse一致性。
复用既有错误枚举拒绝这两类输入，扩展测试覆盖合法单字节追加的render/parse往返。
不改变既有parse规则或路径容量。

### R8 / P3：Namespace局部条目计数被写成全局计数

bind/unbind/mount/unmount更新`NamespaceObject.entries`时使用全表条目数。
改为目标Namespace的`count_in`；在同一测试建立多个Namespace并验证各自局部计数。
实际查找仍使用原条目表，本项修复上报/见证信息。

## 命令与结果

原始输出保存在`/tmp/prima-pre7-*.log`与ignored target，不提交日志附件。

| 命令 | 结果 | 范围 |
|---|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | 基线1125；整改后1132通过，0失败、1默认忽略；退出0 | host；新增7个测试，另扩展Path既有测试 |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | 退出0；fmt、Clippy、裸机构建、41文件零unsafe及personality守卫PASS | 编译/源码门禁 |
| `PRIMA_LINUX_ELF=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo test --workspace --features xtask/linux-probe` | 1133通过，0失败、1默认忽略；退出0 | Linux特性host及差分harness负例 |
| 上述Linux特性执行`cargo clippy --workspace --all-targets --features xtask/linux-probe -- -D warnings` | 退出0 | 特性编译 |
| `PRIMA_GATE_CASE=G4 CARGO_NET_OFFLINE=true cargo clippy --workspace --all-targets --features xtask/gate-probe -- -D warnings` | 退出0 | Gate特性编译 |
| `elan run leanprover/lean4:v4.35.0 lake build`（lean目录） | 退出0；11 jobs | 主模型，未新增定理 |
| 同目录`elan run leanprover/lean4:v4.35.0 lake env lean Checks/KernelHandleTable.lean` | 退出0；10条有限表定理/公理清单输出 | 原有限表模型，不是位置表新修复的源码证明 |
| `python3 prima-user/musl/test_port.py` / `python3 scripts/test_posix_std.py` | 退出0；分别7/3项 | 构建、接口与调用flags护栏 |
| `CARGO_NET_OFFLINE=true cargo xtask tcb` | 退出0；trusted=2087、mechanism=38784、ratio=5.11%，unsafe=79/0，耦合0/false/0/52 | 源码度量，不是安全证明 |
| `CARGO_NET_OFFLINE=true cargo xtask test-process W5` | 退出0；最终共享能力派生/使用夹具双轮通过，原能力/Namespace精确断言保留 | ring3；Namespace新修复仅host覆盖 |
| `PRIMA_ELF_SAMPLE=/usr/bin/busybox CARGO_NET_OFFLINE=true cargo test -p kernel --lib plan_accepts_operator_supplied_static_linux_elf -- --ignored` | 退出0，1项通过；补跑默认忽略的制品输入测试 | host；按需读取与完整文件解析一致，不等于实际运行 |
| `python3 scripts/check_handle_refinement.py` | 退出1，缺锁定翻译器；输入/生成物与manifest哈希核对相符 | 本次未重跑源码细化，不计PASS |
| `python3 scripts/docs.py self-test` / `build` / `check` / `check --base b70abf3` | 全部退出0；374文档，元数据、目录、链接和基线变更检查PASS | 文档元数据/目录/链接/基线 |

| 裸机/差分命令（均`CARGO_NET_OFFLINE=true`） | 结果 | 范围 |
|---|---|---|
| `PRIMA_LINUX_ELF=/usr/bin/busybox cargo run -p xtask --features linux-probe -- test-linux` | 退出0；pinned guest参考11项、Prima每项双轮；stdout/stderr/status/audit全匹配 | 同一ELF；包括M4兄弟线程SIG_IGN与M3描述符故障/短写增量 |
| `cargo run -p xtask --features gate-probe -- test-gate`，G1不设case；分别设`PRIMA_GATE_CASE=G2/G2-death/G3/G4` | 五条命令均退出0、各双轮；G4每轮1000请求、历史标记/物理槽排除/时间戳校验通过 | 固定服务/一次性答复、队列/销毁、复制/COW、历史承载 |
| `PRIMA_GATE_AUDIT_SITE=<site> cargo run -p xtask --features gate-audit-probe -- test-gate-audit`，site为`int80-entry/dispatch/int80-return/fast-return/pick-next/resume/irq` | 七条命令均退出0；共14轮预期QEMU退出35且`unchanged=1` | 真实审计fail-stop；不是普通成功退出 |
| `PRIMA_STAGE6_DOMAINS=1 cargo xtask test-domains` | 退出0；双轮`roots=2;shared_log=3;denied=3;survivor=1` | D14有界双域人格桩，不是阶段7多核或完整personality出口 |
| `cargo xtask compat` | 最终串行退出0，`PASS compat cow-write-fault` | 同场景COW语义与已提交Linux基线；与上面的同ELF差分区分 |
| `cargo xtask test`（支持本机UDP回环的执行环境） | 退出0；最终单次36程序×2轮=72/72，`PASS full BIOS suite: 36 programs, 72 runs`；失败清单为空 | 普通完整启动/阶段5回归；不拼接前两轮失败套件 |

当前M4 ELF SHA-256=`9d2e2508fadd43923d661f302e552af23842be6affc6e908d4f7769bdf36513a`；
M3 ELF SHA-256=`a6e53a6b28385bb55d4561219687a2d27a381dc82829597fcf451452f1851216`。
差分harness验证这些探针原字节同时嵌入Prima服务并复制到pinned Linux guest；
busybox仍为D12原输入`dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14`。
G4本机TCG总路径p50/p99分别为2.067/2.897 ms与2.104/3.269 ms；仅是本次运行计量，不据此主张跨版本性能改进。

## 失败与保留边界

- 修复前的R1/R2/R3反例均有真实非零测试退出；没有将这些失败计为通过。
- BIOS首轮在沙箱中运行，Linux N1参考因本机UDP受限退出1；该轮中止（130），不计完整套件通过。
  第一次沙箱外重跑受到残留QEMU的镜像写锁影响，也已中止并核对/清理本轮进程；随后从头重跑，结果见表。
- 完整BIOS首个沙箱外清理后运行中，W5扩展已输出`authority=1`，但挂载/卸载按原策略留下一个无位置槽，
  `task_ready caps_after=135`不满足原精确值134，W5组双轮均拒绝，命令最终退出1（其余70轮通过），不能计为套件通过。
  第二次完整运行虽然能力计数恢复134，额外Namespace绑定/解析导致全局审计计数5/3/5，而原见证要求4/2/4，
  W5双轮仍被拒绝。这里是新增夹具与全局见证统计范围冲突，不能计PASS。
  最终未放宽任何协议计数：W5只新增非主线程派生与使用共享WaitObject能力；
  Namespace改动留在host反例范围。先用新增聚焦入口验证W5，再从头执行完整套件。
- 初次`cargo xtask compat`与完整套件并发，二者共用输出镜像，compat报告镜像缺失并退出1。
  后续镜像验收全部串行；完整套件仍逐案例核对真实输出，compat最终串行结果见表。
- M4增量初版用`sched_yield`轮询阶段变量，Prima在握手结束前触发既有审计fail-stop，QEMU退出35，
  该次同ELF差分命令退出1。保持审计上限不变，改用已支持的真实futex等待/唤醒同步阶段；
  子线程错误也发布失败阶段，避免主线程无限等待；最终差分另行完整重跑。
- M4增量夹具已在host用相同静态编译参数执行通过；探索性host执行M3因没有guest的`/fixture.txt`退出1，
  该运行不计正式差分证据，正式参考使用原pinned Linux guest及其完整fixture。
- 首轮lint拒绝新增代码的固定分块迭代写法及测试中未读取的赋值；按pinned Clippy改为`as_chunks`并让测试验证新信号可入队，最终门禁结果见表。
- 源码细化脚本本机缺少锁定的`target/aeneas/aeneas`，退出1；没有下载/替换翻译器或把未重跑的证明记为本次通过。
  生产`kernel/src/handle.rs`与其锁定生成物本轮均未修改；Lean主模型及有限表检查与源码翻译验证分开记录。
- 保留[Namespace设计](../specs/2026-09-25-stage3-namespace-design.md)及
  [原实现报告第3项偏差](2026-09-25-stage3-namespace-report.md)“清位置、保留CapRecord/槽”的既有细化边界；
  普通unmount/destroy本轮没有新增历史或槽回收策略。相应不可达token仍占有界资源，不能据本轮声称无限循环可用。
  R2只防止旧代次影响新能力，不把该既有保留策略静默改成新契约。
- 审计池、通用能力历史、进程/线程、文件、描述符、映像和队列保持有界；Stage6仍是单核冻结profile。
  AArch64、多核、配额、持久化、完整撤销和更大兼容面属于阶段7既有待冻结轨道。
- 本轮没有新的完整Rust→Lean状态模拟、二进制证明、SMP验证或无限状态探索。
  OSTD受信任边界、模型遗漏及历史报告中未闭合的组织/研究条件仍按各自权威文档解释。

## 证据衔接

本报告是阶段0–6既有有界实现的跨层整改后继，不替代各阶段出口的原始适用范围。
同步更新阶段跟踪器、reports索引和相关主题路由；生成catalog后运行文档四项检查。
