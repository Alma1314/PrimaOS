---
id: spec-stage7-gate-baseline
kind: specification
authority: scoped-measurement-and-evidence-contract
status: reviewed-for-gate-baseline-slice
lifecycle: current
updated: 2026-11-07
---

# 7-0原始串口证据与Gate内核路径基线

## 范围与实现前复核

承接[五类基准清单与G6](2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[CI证据契约](2026-11-06-stage7-ci-evidence.md)和[阶段7执行计划](../plans/2026-11-06-stage7-execution.md)。
本片复用现有G4，不修改OS机制、样本挂钩、双轮或QEMU时限，不新增性能门槛。
实现前核对[阶段6测量边界](../reports/2026-11-01-stage6-measurement-and-boot-span.md#m1e-计量探针)
与kernel/src/gate_bench.rs、xtask/src/qemu.rs：G4是Gate内核路径测量，**不是完整用户态往返**。
本片只冻结该已有测量的复现和归档方法，不能据此关闭G6或五项基准交付。

## 逐命令原始证据保留

九组原运行器只保留子命令stdout/stderr和最终制品身份；成功的Gate原始时间戳写在
target/prima-os/S6GATE-{1,2}.log，下一配置会覆盖同名文件，原Actions上传目录没有这些原始采样。
同时旧制品扫描仅匹配根目录，遗漏package_images实际使用的gate-probe、gate-audit-probe、
linux-probe一层子目录；首次九组通过的Gate/审计manifest因此没有镜像身份，Linux也缺其探针镜像。
成功制品身份扫描须覆盖根目录及这一层子目录中的*.img/*.elf，仍排除运行前未变化的旧文件。
只保存最终制品哈希，不宣称保留所有中间配置的二进制。

运行器须在每个子命令前快照target/prima-os下直接的*.log的大小和mtime_ns；
命令成功、非零、超时或中断后，在下一条命令开始前，将新增/改写的普通日志文件复制到
本次运行目录traces/步骤编号/，保存原工作区路径、归档相对路径、大小和归档字节SHA-256。
不复制未变化的旧日志或符号链接，不把失败诊断标为通过制品；日志归档独立于既有artifacts字段。
同名日志的不同步骤必须分别保留。复制失败使组非通过，不允许缺证据却标PASS。
本片只覆盖合作子命令产生并留在磁盘的直接*.log；不保证其内部每次覆盖前的中间版本、
未落盘的超时串口流、任意外部改写或mtime/大小被人为恢复的变化。
原始文件只留ignored target和现有短期Actions附件，不提交日志或单独hash附件。

## Gate内核路径基线v0.1

| 项 | 冻结方法与边界 |
|---|---|
| 入口 | python3 scripts/ci.py gate --prepare；已有缓存可省略--prepare；选择该组G4步骤 |
| 输入 | 仓库pinned Rust、bootloader及已有musl/sbase输入；每次记录源摘要、实际工具版本、镜像/ELF身份 |
| 平台 | 现有pc-i440fx-8.2、TCG单线程、qemu64、1 vCPU、128M、BIOS；记录实际宿主与QEMU版本 |
| 工作负载 | 独立启动两轮，每轮单客户端/单服务，1000次生产int80→Gate/wait/reply；逐次验证答复 |
| 区间 | trap→queue、queue→deliver、deliver→reply、reply→resume及首尾total；末点在最终iret之前 |
| 开销 | 包含现有时间戳、状态/审计处理与调度；没有扣除采样器开销；包含首个请求，不丢弃冷样本 |
| 导出 | 每轮结束后输出1000组原始有序五点时间戳及PIT/TSC校准；串口导出不进入采样区间 |
| 验证 | 恰1000样本、连续索引、组内/跨组单调、唯一非零校准、无panic；三类审计各1000条与历史记录验证 |
| 统计 | 每轮分别计算nearest-rank p50/p99，排名500/990；整数ns=floor(cycles×10^9/hz)；total独立求分位数 |
| 参考侧 | 同一原始序列由xtask验证/统计，并在报告中用Python独立重算；不拿Linux或KVM数据作等价参考 |
| 解释 | 两轮分别列出，不混合校准、宿主、TCG/KVM或核数；单次观测不是稳定性/扩展性或性能门槛证明 |

该基线不包含客户端陷入前指令、硬件陷入入口、最终iret和用户态续执行；不能把它写成完整Gate往返，
更不能把四段p99之和当作total p99。完整用户态Gate往返及其余四类基准的具体方法和探针仍待完成；
多核基线依赖7-B，数值门槛仍按作者选择在基线后确认。

## 验证与重审

host反例须验证旧日志排除、同名覆盖前的逐步副本、失败日志保留及归档I/O错误不能PASS。
实际运行一次Gate组，核对五配置各双轮、G4两份原始采样和独立分位数重算，并执行host/lint和docs门禁。
历史远端九组通过属于其提交；新增归档器的本地通过不自动成为远端通过。
改变采样边界、负载、校准、平台或归档范围时修订本契约，不能跨方法沿用数值比较或退出结论。

实际远端九组结果、归档发现/修复与本地基线见
[九组CI及Gate基线报告](../reports/2026-11-07-stage7-ci-green-gate-baseline.md)。
