---
id: spec-stage7-gate-user-benchmark
kind: specification
authority: scoped-measurement-contract
status: reviewed-for-user-roundtrip-slice
lifecycle: current
updated: 2026-11-07
---

# 7-0 G5用户态Gate调用往返测量

## 范围与实现前复核

承接[五项基准与G6](2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[G4内核路径方法](2026-11-07-stage7-gate-baseline.md)及其
[实际基线与失败链](../reports/2026-11-07-stage7-ci-green-gate-baseline.md)。
本片增加独立的G5 selector，复用生产Gate路径、两个平台Task与既有安全代码发射器，
不增加ABI或改变G4的记录格式/1000样本边界。G5只扩展可选测量探针，不构成五项基准全部交付或性能门槛。

## 测量方法v0.1

- 平台沿用pc-i440fx-8.2、TCG单线程、qemu64、1 vCPU、128M、BIOS；默认dev构建，内核未启用release优化；记录实际宿主与QEMU工具版本。
- 每次独立启动两轮，每轮128次串行调用。两点各u64、每样本16字节，共2048字节，
  放在客户端现有4096字节可写页USER2_Y_VA内；不扩大映射、缩窄时间戳或循环覆盖样本。
- 用户态执行LFENCE/RDTSC，组合完整u64计数并保存起点，然后设置六个参数和不透明调用号并int80；
  答复返回用户态后先把rax保存到r15，再LFENCE/RDTSC取终点，保存后核对答复值。
  区间含起点保存、参数准备、完整陷入/Gate/wait/reply/iret、rax保存及终点前LFENCE，
  不含终点保存和答复检查；称为user-wrapper往返，不冒称仅int80指令本体成本。
- 不丢弃冷样本，不扣除用户端点和五点内核插桩的开销。服务逐次核对wait/reply成功并让出，与G4复用生产路径。
  用现有PIT/TSC校准解释时间戳，不新增可信熵、跨核或固定频率主张。
- 保留五个内核时间点；每条必须满足user_begin≤kernel五点≤user_end，跨样本有序，
  审计trap/delivered/replied各128条，答复逐次正确，Gate/完成等待及退休身份历史良构。
- 每轮独立计算nearest-rank p50/p99，排名64/127；整数ns=floor(cycles×10^9/hz)。
  除四段和kernel-total外，报告user-entry、user-return和独立user-total；不把分位数相加。
  与G4的样本数/边界不同，分别列出，不混合成同一统计总体。

## 输出与接受条件

G5输出独立的S7HISTORY（deliveries/audit_roots/dedicated_slots_saved均128）、
S7BENCH（samples=128、非零hz、boundary=user-wrapper、verified=1）及128行
S7TIME|index;user_begin;kernel0;kernel1;kernel2;kernel3;kernel4;user_end。
记录在全部验证后导出，串口输出不进入采样区间。
宿主必须拒绝缺失/重复/错误头、错误审计计数、错误样本数/列数/索引、零起点/频率、
组内或跨组时钟回退、用户区间未覆盖内核区间及panic；不能把G4记录当作G5通过。
CI Gate组在原五配置之后增加G5，仍每配置双轮、单轮30秒；现有逐步原始串口归档和探针制品身份继续适用。

## 验证与限制

实现前已核对gate_bench.rs、gate_probe.rs、sched.rs、syscall.rs、main.rs和xtask/qemu.rs的完整调用链。
直接用户RDTSC是既有QEMU配置上的探针要求；若被平台禁止，应失败并记录，不能伪造用户端点。
新增host反例检查G5格式与区间覆盖；运行Gate六配置双轮、workspace/host、lint、普通BIOS回归及docs门禁，
再从归档原始样本独立重算。其它四类基准、2/4核扩展性、KVM/实机与基线后数值门槛仍待对应工作包。
测量边界、样本数、平台或负载变化时修订方法；本片运行不扩大任何兼容profile或形式化证明范围。

实际结果、faf551a远端归档修复证据与本地完整回归见
[G5基线报告](../reports/2026-11-07-stage7-gate-user-benchmark.md)。
