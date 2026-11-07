---
id: spec-stage7-ipc-benchmark
kind: specification
authority: scoped-measurement-contract
status: reviewed-for-ipc-roundtrip-slice
lifecycle: current
updated: 2026-11-07
---

# 7-0 I1 授权 IPC 用户往返测量

## 范围与实现前复核

承接[五项基准裁决](2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[N1方法](2026-11-07-stage7-native-syscall-benchmark.md)及其
[证据链](../reports/2026-11-07-stage7-native-syscall-benchmark.md)。已核对抽象规范§6.3/§9、
[IPC用户化设计](2026-09-16-stage2-ipc-user-surface-design.md)、
[直投整改](../reports/2026-10-18-stage2-ipc-review-remediation.md)、
[直投唤醒审计预算后继R3](../reports/2026-11-06-stage0-6-review-remediation.md#r3--p1阻塞ipc直投少预留唤醒审计)、现行send/recv_meta、调度与审计路径。
旧设计中携cap直投限制已经由该整改关闭；首片不改生产语义、ABI或发送授权。
[审计排序后继](../reports/2026-11-07-stage7-ipc-audit-order.md)按既有§6.3修正事件顺序，并加强测后检查；ABI与发送授权保持。

## 测量方法v0.1与审计验收v0.2

复用gate-probe工程通道，PRIMA_GATE_CASE=I1。Task 2客户端与Task 1服务端均未绑定Gate，
各持指向对方且仅含TaskSend的授权，消息不携cap。服务先阻塞接收；客户端每次发送
payload=64-index的请求，再阻塞接收；服务核对序号后发送payload=65536+请求序号的答复。
服务下一次接收阻塞，使客户端恢复；最后一次答复后服务让出。每轮64次串行真实双向IPC，
共128次直投、128次阻塞接收，排队投递增量为0。不是同步call/reply快路径或≤250ns指标验收。

两端使用现有SYS_SEND（17）/SYS_RECV_META（60）int80及独立地址空间；每次send必须返回0。
客户端接收使用现有五qword输出：cap0/cap1/count均0、认证sender/incarnation为Task 1、
非零msg-id；服务端同样保留Task 2身份及请求msg-id。全部消息ID严格递增，
每条均须有且只有一对匹配的MessageSent/MessageReceived审计，发送先于接收。
身份代次从实际Task记录捕获，不把Task ID当作incarnation。结束无未决接收，Gate/完成等待良构。
v0.1只检查Sent先于Received，曾按[首片报告](../reports/2026-11-07-stage7-ipc-benchmark.md)登记
ISSUE-IPC-AUDIT-ORDER-001。v0.2在每对Sent/Received间要求恰一条相邻且接收者匹配的Ready事件，
对应无cap直投Sent→Ready→Received；历史v0.1样本不升级为加强校验后的结果。
测量端点、用户协议、样本数及输出格式仍为v0.1，不宣称完整抽象审计序列等价。

客户端每条64字节：begin/end/答复payload/五qword元数据，共4096字节；
服务端每条48字节：请求payload/五qword元数据，共3072字节，均放现有各自RW页，编译期检查页容量。
用既有LFENCE/RDTSC发射器在客户端send准备前及recv返回后暂存rax之后取完整u64端点。
区间包含起点保存、参数准备、两次send/recv与地址空间切换、服务序号核对/答复准备，
以及客户端send结果检查、rax暂存和终点前LFENCE；不含终点保存、客户端答复检查及测后内核元数据核对。
串口在全部测量与验证结束后导出，无新内核采样钩子。

平台沿用pc-i440fx-8.2、TCG单线程、qemu64、1vCPU、128M、BIOS、默认dev构建；
两Task IF=0，使用现有PIT/TSC校准，独立启动两轮。不丢冷样本、不扣计时/协议开销。

## 输出与验收

唯一头S7IPC|samples=64;hz=非零;boundary=user-wrapper;client=认证身份;service=认证身份;
direct=128;queued=0;blocks=128;verified=1。认证身份为generation左移32位或Task ID。
每行S7ITIME|index;客户端八qword;服务端六qword，共15列；保留全部原始回执。
宿主严格检查唯一头、身份低32位/非零代次、路径计数、样本数/列数/索引、
非零且组内/跨组有序端点、逐条请求/答复/元数据及严格递增消息ID，拒绝panic与错类格式。
每轮独立计算nearest-rank p50/p99，排名32/64；ns=floor(cycles×10^9/hz)，宽整数乘法。
CI在既有七配置后追加I1双轮，保留单轮30秒与组预算、逐步原始归档及最终制品身份。

运行解析正反例、八配置双轮、host、lint/特性Clippy、普通BIOS及文档检查；独立重算归档哈希与分位数。
本片仅为单核无cap直投的有界往返基线，不扩大兼容或形式化主张；
页故障/COW、端到端LibOS、多核、数值门槛与发布检查仍待独立完成。

实际基线、回归及N1干净远端后继见[验收报告](../reports/2026-11-07-stage7-ipc-benchmark.md)。
