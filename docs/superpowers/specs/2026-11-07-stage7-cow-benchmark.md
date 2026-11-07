---
id: spec-stage7-cow-benchmark
kind: specification
authority: scoped-measurement-contract
status: reviewed-for-cow-fault-slice
lifecycle: current
updated: 2026-11-08
---

# 7-0 C1 用户写保护故障与 COW 测量

## 范围与实现前复核

承接[五项基准裁决](2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[执行计划](../plans/2026-11-06-stage7-execution.md)及
[I1 审计整改后继](../reports/2026-11-07-stage7-ipc-audit-order.md)。
已读[抽象 COW 组合设计](2026-08-14-cow-refinement-design.md)、其历史计划、
[页表原语](../reports/2026-10-23-stage5-m17b2-cow-primitives.md)、
[general COW 接线及当时失败](../reports/2026-10-23-stage5-m17b2-cow-fork.md)与
[阶段0–6整改](../reports/2026-11-06-stage0-6-review-remediation.md)。
复核 cow::dispatch、resolve_cow、cow_leaf、memseg_map、帧归属、裸 #PF 入口及探针发射器。

本片测量现有机器实现的内核 general COW 路径，不改变抽象§12.8的用户态LibOS责任，
也不将此内核复制路径宣称为该抽象协议的实现或细化证明。不测非present缺页、完整fork、
用户态异常递送/授权恢复或端到端LibOS操作；这些边界不能由 C1 的通过关闭。

## 方法 v0.1

selector 为 PRIMA_GATE_CASE=C1，复用 gate-probe 构建通道，但不绑定 Gate。
准备阶段由Task 1创建64页RW Segment，经真实授权 memseg_map 映射到Task 1只读、Task 2读写。
每页填充带页号与字偏移的512个u64；仅在该可选夹具中，对Task 2叶应用生产fork同一cow_leaf变换，
清W、置COW/SHARED，对Task 1只读叶置SHARED。双根逐页验证同帧、用户可读、NX及只读状态。
这是预构造共享快照，不测fork创建。现有私有回执/代码/栈页不标COW，不将夹具建立计入区间。

Task 2在ring3依序写64个不同页面，每页只触发一次写保护 #PF。
每轮保存LFENCE/RDTSC起点，执行一条写u64指令，再取终点；随后保存读回并检查写值及r11哨兵。
区间包括起点拼接/保存、真实异常入口、探针观察器、生产cow::dispatch/resolve_cow的4KiB复制、
PTE替换、invlpg、iret、重试写及终点前LFENCE；不含终点拼接/保存、读回和测后完整检查。
观察器只存在于C1镜像：逐次核对当前Task、硬件CR2/error=7、故障RIP和寄存器，委托生产分派器，
要求原RIP返回，记下硬件地址/error和成功次数。观察器成本保留，不声称纯生产handler成本。

平台延续pc-i440fx-8.2/TCG单线程/qemu64/1vCPU/128M/BIOS/dev；两次独立启动，每次64样本。
现有私有回执页使用1536字节，编译期检查容量。PIT/TSC校准、保留冷样本与计时开销、不扣基线。
不引入新ABI、生产采样钩子或容量扩大。

## 测后验证、输出与统计

必须确认恰64次顺序写保护故障、逐条非零有序时间与读回；Task 2每个新帧非原帧且彼此不同、
W=1/COW=SHARED=0，Task 1仍指原只读共享帧。逐页核对全部512个u64：原帧全模式不变，
私有帧仅第0字为写值，其余511字完全复制；检查新旧帧Segment归属、不交性及帧守恒，
空闲帧恰减少64、段帧恰增加64，页表帧不增加。确认没有新增Gate事件及未绑定Gate。

全部通过后输出唯一头 S7COW|samples=64;hz=非零;boundary=user-store;task=2;faults=64;observer=checked;verified=1。
64行 S7CTIME|index;begin;end;readback;fault_va;error;shared_phys;private_phys。
写值为65536+index，VA为0x00000b0000400000+4096×index；物理帧非零、4KiB对齐，
所有原帧及私有帧在两集合内及跨集合均不同。完整内容/PTE/归属检查在guest执行，串口仅导出其已验证身份。
宿主拒绝错误/重复/缺失头、校准、字段、次序、回执、帧别名、缺失/多余行和panic。
每轮独立nearest-rank p50/p99，排名32/64；ns=floor(cycles×10^9/hz)，宽整数转换。
CI在八配置后追加C1双轮，保留逐步串口和制品身份，单轮30秒上限不变。

## 验证与重审

运行解析正反例、九配置18轮、host、lint/特性Clippy、普通BIOS及文档门禁，
独立重算原始样本分位数与归档哈希。端到端LibOS、2/4核、数值门槛、发布许可仍待完成。
无新增Lean证明，不将单核TCG/dev结果推广到KVM、实机、其他页面尺寸或完整异常协议。
页面数量、平台、负载或计时边界变化需修订本方法。

实际验证及整改自身远端后继见[C1报告](../reports/2026-11-07-stage7-cow-benchmark.md)。
