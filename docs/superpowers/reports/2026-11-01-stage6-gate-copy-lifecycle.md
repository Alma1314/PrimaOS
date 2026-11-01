---
id: report-stage6-gate-copy-lifecycle
kind: report
authority: scoped-implementation-evidence
status: verified-scoped-with-failures
lifecycle: historical
updated: 2026-11-01
---

# S6-M1 / M1c–M1d：代理复制与 G2/G3 生命周期验收

## 结论与范围

基线为主分支 `76fc397b0c572d511522b005fff5757b2a90b62a`。本轮实现原生75/76代理复制，
修正容量等待/已交付请求销毁后的错误晋级，并补充G2、G2-death、G3专用裸机矩阵。
采用[协议](../specs/2026-10-29-stage6-m1-protocol.md) §1–6与
[递送设计](../specs/2026-10-29-stage6-m1-delivery-design.md)的验收范围；这是单核技术证据，
不签署阶段6、不实现Linux LibOS、可信停车/装载协议、共享root、可中断门控等待或整帧答复。
M1e计量与S6-D2整体验收仍另行跟踪。退出条件保持0/4。

最终统一验证结果见下表；[W7分段诊断](2026-11-01-stage6-w7-timing-diagnosis.md)单独记录，
不把QEMU诊断替代双轮验收，也不覆盖前序60/72等失败。

## 实现

### 75/76与复制事务

`prima-abi`追加`SYS_GATE_COPY_IN=75`、`SYS_GATE_COPY_OUT=76`，1–72及52保留不变。
参数为`(request_cap, client_va, len, service_va, 0, 0)`，成功返回`len`，单次≤4096字节。
两方向复用reply的实时授权：活动token、实际持有位置、固定服务者cookie、请求Delivered状态、
客户端Task代次。attach记录root物理地址和单调代次；同物理地址重装也失效。

客户端绑定期间拒绝map/unmap/protect/access-enable、exec、fork与新增共享线程。
每次复制重新核对独占root。映射身份由绑定期间的映射冻结及单核不可调度事务保证，
不另造可跨阻塞保存的PTE计划；只有受控COW在该事务内变更PTE。
服务缓冲使用服务root，客户端使用绑定root。双端逐页要求用户可访问的普通Segment RAM、
逻辑读/写权限、实际物理帧归属与Segment一致；拒绝内核、设备、隐藏页、溢出及跨未映射边界。
零长不解引用指针，仍核对请求、root与两端加法。

先把整个输入读到独立4KiB scratch，再调用既有`prepare_task_output`完成目标全范围权限检查、
全部COW帧预留，最后无失败写入。普通失败不改目标字节；预留失败释放全部新帧并保持PTE。
成功COW的PTE变化属于提交。既有真实帧分配失败回滚host反例继续覆盖底层；G3补齐代理调用接线，
不以host结果声称发生过QEMU分配耗尽。

入口在任何指针验证前预检失败日志；成功复制先预检完整成功审计成本。
`GateCopied`含Gate/Request、客户端/服务者cookie、双端地址、长度、方向；原始trap参数由同一请求链关联。
失败共用`GateControlRejected`，记录原始控制号/六参数/尝试者cookie/原生错误。
审计不足仍全局fail-stop，不返回伪造客户端结果，不继续复制。

### 队列清理

前序`gate_on_task_destroy`在有容量等待者时无条件晋级。销毁WaitingCapacity请求、或销毁Delivered
请求而队列仍满时，都可能把8项队列加到9项并覆盖已有项。现在仅当删除后队列确实有空位才晋级；
总审计成本预检同步使用相同条件。host反例保持多个容量等待者，依次销毁容量等待、已交付、排队请求，
逐次核对容量和W17投影。G2通过真实陷入及真实Task销毁入口验证容量等待与排队清理。

### 有界裸机矩阵

普通镜像无探针。`gate-probe`配合编译时`PRIMA_GATE_CASE`选择矩阵，xtask拒绝运行时与编译时不一致。
创建的客户端用真实Task builder、独立root/Segment；逻辑序号映射实际分配id，保留普通boot故障见证。
可信检查点只编排调度、排队信号或注入页权限反例；所有attach/wait/copy/reply/abort/destroy操作在ring3
走生产入口，监督者持有自己的管理能力。检查点不替代授权或请求提交。

| 用例 | 核对范围 |
|---|---|
| G1 | 既有syscall/int80、全部寄存器、空队列重试、wait输出失败、Ready准入、9条逐字段拒绝审计及孤儿trap原始上下文 |
| G2 | 10个真实trap形成8项队列及2项容量等待；另销毁一个Idle绑定；销毁WaitingCapacity后队列仍满，销毁Queued后准确晋级；销毁Delivered后迟到reply/copy失败；reply/abort各一次并拒绝重复；服务交付下一请求后不答复，监督者显式销毁服务，剩余请求以配置值终结；绑定后续trap仍fail-closed |
| G2-death | Delivered、Queued、WaitingCapacity同时存在时，服务经原生自毁入口死亡；所有未决客户端恢复配置值，后续trap不退回原生 |
| G3 | 两任务同VA不同root；4096字节非对齐跨两页copy-in/out及双端COW，逐8字节检查完整内容和边界；零长/过长/溢出/用户地址上界/未映射/只读/跨未映射边界/隐藏页；失败不改字节和预检前PTE；错误持有者、终结后请求拒绝；4条成功复制、12条拒绝及身份/返回值对账 |

G2在Idle、Queued/WaitingCapacity、Delivered和终结后排队信号，核对仍保留且不取消/重放。
每请求终结或取消恰一次；真实Gate/Request/Wait表始终well-formed，结束无残留未决请求。
G2-death仍不证明服务无人监督且不答复时有时限；协议明确允许无限等待。

## 命令与结果

宿主x86_64，pinned nightly-2026-07-30；QEMU 8.2.2，pc-i440fx-8.2、TCG单线程、qemu64、
1 CPU、128MiB。全部验收保持30秒门限；专用矩阵不计入普通72轮。

| 命令 | 最终本次结果 |
|---|---|
| `CARGO_NET_OFFLINE=true cargo test --workspace` | PASS：1095 passed、0 failed、1既有外部ELF ignored |
| `CARGO_NET_OFFLINE=true cargo xtask lint` | PASS：fmt、Clippy -D warnings、裸机deny unsafe编译；39个内核文件零unsafe |
| `CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G3 cargo clippy --workspace --all-targets --features kernel/gate-probe,xtask/gate-probe -- -D warnings` | PASS：包含专用矩阵的全部targets，-D warnings |
| `CARGO_NET_OFFLINE=true cargo run -p xtask --features gate-probe -- test-gate` | G1双轮PASS |
| `CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G2 cargo run -p xtask --features gate-probe -- test-gate` | PASS：两轮，10个请求恰一次终结/取消、4条拒绝及7次孤儿trap原始审计 |
| `CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G2-death cargo run -p xtask --features gate-probe -- test-gate` | PASS：两轮，Delivered/Queued/WaitingCapacity统一孤儿化及10次孤儿trap原始审计 |
| `CARGO_NET_OFFLINE=true PRIMA_GATE_CASE=G3 cargo run -p xtask --features gate-probe -- test-gate` | PASS：两轮，4条成功复制、12条拒绝审计、不同字节哨兵及1次孤儿trap原始审计 |
| `CARGO_NET_OFFLINE=true cargo xtask test-process W7` | PASS：Linux参考及QEMU两轮，保持30秒门限 |
| `CARGO_NET_OFFLINE=true cargo xtask test` | 首次71/72，I1跨轮消息总数差异，退出1；最终72/72（36程序各两轮），退出0；全部Linux参考通过、30秒门限保持 |

原始日志留在`/tmp/stage6-gate-*.log`及ignored `target/prima-os/`；不提交日志或镜像。
文档self-test/build/check/check-base均退出0；源码身份见下节。

## 首次完整回归与最后审计补强

首次普通完整命令退出1，71/72轮通过，唯一失败组I1。第2轮退出33且有boot_complete，
单轮阻塞归账成立，但跨轮用户消息总数423→420不一致：

```text
run1 paths=(84,339,353) receives=[3,3,2,1,0,8]
run2 paths=(84,336,351) receives=[4,4,2,1,0,8]
```

两轮非UART内核完成均为0，interrupted/retired/faults/live保持2/1/0/8；不是未归账阻塞，
也不是W7超时。第2轮在严格跨轮比较处返回，后续该轮其它validator未执行，不能拼成全部PASS。
本轮没有修改I1协议或判据，没有据此推断Gate改动造成差异。初轮证据保留在
`/tmp/stage6-gate-full-suite-final.log`与`/tmp/stage6-gate-full-first-I1-run{1,2}.log`。
这轮正常内核ELF长8,070,128字节、entry=0x2b7f0；当时Gate源码SHA-256：

```text
b19189fedf9c2dc455655859fdafffa47e1e8a21594e49035cf98b2129371a40  kernel/src/gate.rs
```

套件末段复核发现：前序孤儿绑定后续trap只有结果事件，多个客户端无法从该事件恢复原始调用身份。
最后补强改为先记录`GateTrapped(request=0, client_cookie, number, args, flags)`，再记录
`GateTerminated(request=0, value, Orphaned)`，两条容量在变更前一并预检；不分配请求或恢复原生分派。
新增host反例验证仅剩一条审计空间时全局停止且不留下半条事务。G1核对这条原始尝试，
G2/G2-death/G3逐个核对孤儿客户端cookie、原始调用与相邻终结事件，分别为7/10/1次。

同时为G3失败写入设置不同内容的双端哨兵，检查跨未映射边界的可写前缀和隐藏页前的字节，
避免“错误写入与原内容相同”导致误通过。最终源码重新运行host/lint、全部专用矩阵和一次普通完整回归；
首次I1失败保留，不因后继PASS改写，也没有反复运行同一整套版本追求PASS。

最终同一命令在最后审计补强源码上退出0，普通72/72、所有Linux参考通过。I1最终两轮：

```text
run1 paths=(84,339,353) receives=[3,3,2,1,0,8]
run2 paths=(83,340,353) receives=[2,2,2,1,0,8]
PASS full BIOS suite: 36 programs, 72 runs
PASS stage1 boot slice
```

最终I1两轮总消息数均423，UART完成可变、非UART完成仍0；这是原判据允许的变化。
未改变I1实现/validator，也不将这次PASS归因于孤儿审计；首次差异根因仍未唯一确认。
最终原始回归在`/tmp/stage6-gate-full-suite-after-audit.log`，I1两轮另保留于同前缀的临时文件。
最终普通内核ELF长8,070,144字节、entry=0x2b7f0，SHA-256：

```text
c08fd91ca9818028f46db6b8e44e234ad1bfa28471a741e613549aa60b3a3da5  final-normal-kernel.elf
```

## 开发失败与整改链

1. 特性Clippy拒绝编译时可缺省变量直接unwrap，改为默认G1；case选择仍在入口校验。
2. G3首轮debug ELF为8,593,968字节，超过2048页的image归属向量，引导确定性拒绝。
   将该**内核镜像**向量上限扩到2304页（9MiB），保留超额拒绝测试；用户数据帧2048、普通段149、
   私有exec640和32KiB内核栈预算不变。未放宽任何验收时间或错误判据。
3. G3探针初始化在持有BootstrapState时调用需重新解析能力的调度登记，造成同一锁重入。
   30秒超时后另一次60秒诊断持续采样到`prima_ostd::sync::Lock::lock`；诊断不计验收。
   按生产syscall适配器模式把登记移到State访问之外，随后G3双轮通过。此为探针初始化错误，
   与W7历史超时无因果结论。
4. G2首版假设Task id连续，碰到保留boot故障见证而失败；改为保存真实Task builder返回的id，
   未移除这些见证。第二版监督者错误使用服务者持有的cap，授权拒绝并被结果断言发现；
   改用Task builder交给监督者的独立管理能力。生产授权检查保持，随后G2双轮通过。

## 后继与限制

本报告补强[对象生命周期](2026-10-31-stage6-gate-lifecycle.md)、
[授权丢失](2026-10-31-stage6-gate-authority-loss.md)、
[trap接线](2026-10-31-stage6-gate-dispatch.md)、
[普通失败审计](2026-10-31-stage6-gate-control-rejections.md)各自明确未覆盖的75/76及G2/G3。
前序失败和证据边界仍有效。该矩阵不是所有可达状态证明、SMP/共享root支持、Linux兼容证明或阶段签署。
当前交付与退出结论见[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。

## 源码身份与必要输出

下列SHA-256对应本轮最终源码（基线76fc397上的工作区改动）；不提交生成镜像。

```text
bac9f469c0b688dc159b1e0c5d92ee05800b495ddf4758727ce3696c5d85db82  kernel/src/bootstrap.rs
5225ff676dbfe02189c1135cc1d88265b847d18c4aa6a4749c3da51f5eccd204  kernel/src/frames.rs
941ddceddaf876aaf6e3ed6502db0d75f1edc3b37b76a0247712ab8d1c3a2035  kernel/src/gate.rs
afef2e4e7e9a2c56083ca9c2873418e5c74a8d2b71c76d37c19a8be38be50285  kernel/src/gate_matrix_probe.rs
245ae37b4d0055e098792edfd5f85240d096b8e558ba425ca24a36446dc7a181  kernel/src/sched.rs
135e553130ed6e07c433c7d56011378009b04074998330ed7f71396818dd77a2  kernel/src/syscall.rs
51db939c1404a5d153cb5f51c57c48a17fa173946ec99945ce3d563383f057c2  prima-abi/src/lib.rs
3304f7e0b3189238b0fd0f117c3c71c447cb1a8844ef9cc7e25f7b02078a46b1  xtask/src/qemu.rs
```

最终专用矩阵输出摘录（每个case各两轮、命令均退出0）：

```text
PASS S6MATRIX round=1 case=G2 verified=1 state=1 audit=1
PASS S6MATRIX round=2 case=G2 verified=1 state=1 audit=1
PASS S6MATRIX round=1 case=G2-death verified=1 state=1 audit=1
PASS S6MATRIX round=2 case=G2-death verified=1 state=1 audit=1
PASS S6MATRIX round=1 case=G3 verified=1 state=1 audit=1
PASS S6MATRIX round=2 case=G3 verified=1 state=1 audit=1
PASS qemu bios W7 run=1
PASS qemu bios W7 run=2
```
