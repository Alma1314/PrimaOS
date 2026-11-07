---
id: spec-stage7-native-syscall-benchmark
kind: specification
authority: scoped-measurement-contract
status: reviewed-for-native-syscall-slice
lifecycle: current
updated: 2026-11-07
---

# 7-0 N1原生syscall用户往返测量

## 范围与实现前复核

承接[五项基准裁决](2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[G5用户端点方法](2026-11-07-stage7-gate-user-benchmark.md)和
[G5本地证据及前序失败链](../reports/2026-11-07-stage7-gate-user-benchmark.md)。
已核对gate_probe/bench、syscall::handle/serve、sched::resume及xtask的生成、返回和验证路径。
本片只增加可选N1测量程序，不修改生产ABI、SYS_TASK_ID语义或原Gate探针断言。
抽象规范尚未定义完整syscall调用返回协议；本片明确是现有Prima机器ABI上的测量。

## 测量方法v0.1

复用gate-probe构建通道，PRIMA_GATE_CASE=N1；它是原生测量selector，不是Gate请求。
在未绑定Gate的Task 2执行128次SYS_TASK_ID（7）int80，每次返回必须为2。
不启动服务、附加Gate或人为增加内核采样钩子。启动前清除bring-up留下的Task 2续体，
校准后经switch_to迁移Running/Ready状态及当前身份，再恢复Task 2；记录已有审计计数，结束检查未新增GateTrapped、GateDelivered、GateTerminated或GateControlRejected事件且Task仍未绑定Gate。

平台为pc-i440fx-8.2、TCG单线程、qemu64、1 vCPU、128M、BIOS、默认dev构建。
每次验收独立启动两轮；每轮128个完整u64起止对和返回值，占现有私有RW页3072字节，
编译期检查4096字节上界，不新建映射或缩窄时间戳。
使用G5相同LFENCE/RDTSC发射器；起点后保存时间戳、准备六个零参数和调用号，int80返回后
保存rax到r15，再取终点，最后保存返回值并比较。区间含起点保存、参数准备、完整原生陷入/分派/iret、
返回值暂存和终点前LFENCE；不含终点保存、回执保存及比较。称user-wrapper，不声称仅int80指令成本。
保留冷样本及计时开销，不扣除基线；用现有PIT/TSC校准，不作跨核/固定频率主张。

## 输出、校验与统计

验证128条非零有序起止、跨样本有序、逐条返回2及未绑定Gate后，输出：

- 唯一S7NATIVE头：samples=128;hz=非零整数;boundary=user-wrapper;number=7;task=2;gate=0;verified=1。
- 128行S7NTIME|index;user_begin;user_end;return_value，索引0至127。

串口导出在全部测量结束后；宿主严格拒绝错误/重复/缺失头、错误字段、零频率/起点、缺失/多余样本、
错误列数/索引、时钟回退、错误回执及panic；不能将G4/G5格式当作N1通过。
每轮独立计算nearest-rank p50/p99，排名64/127；ns=floor(cycles×10^9/hz)，使用宽整数防乘法溢出。
CI在原六种Gate配置之后追加N1双轮，复用逐步日志归档、探针制品身份与单轮30秒上限。
名称沿用Gate工程通道，不把N1记为Gate性能总体。

## 验证与限制

运行包含变动样本时长/非法回执/格式/时钟反例的host测试、七配置双轮、host、lint/特性Clippy、
普通BIOS回归及文档门禁；从归档独立核对哈希与重算分位数。
无新生产机制或形式化主张；既有证明范围不扩大。仅覆盖SYS_TASK_ID这一条原生int80路径，
不代表所有原语、fast syscall、多核、KVM或实机性能。IPC、页故障/COW、端到端LibOS、
2/4核、统计稳定性和数值门槛仍需独立完成；平台、负载或边界变化须修订方法。

实际结果与G5远端后继见[N1基线报告](../reports/2026-11-07-stage7-native-syscall-benchmark.md)。
