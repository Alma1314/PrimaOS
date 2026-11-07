---
id: spec-stage7-libos-benchmark
kind: specification
authority: scoped-measurement-contract
status: reviewed-for-libos-read-slice
lifecycle: current
updated: 2026-11-08
---

# 7-0 L1 端到端 LibOS read 基准

## 范围与实现前复核

承接[五项裁决](2026-10-31-stage7-scope-decisions.md#72-先基线后门槛)、
[执行计划](../plans/2026-11-06-stage7-execution.md)及
[C1与前序证据链](../reports/2026-11-07-stage7-cow-benchmark.md)。
已复核受管装载、linux_main的服务循环、Family/Process分派、VFS读取及Gate复制/答复路径，
依据[停车装载契约](2026-11-01-stage6-managed-launch.md)、
[Gate协议](2026-10-29-stage6-m1-protocol.md)和
[冻结profile v1](../compat/stage6-linux-profile-v1.md)。
只增加显式可选测量通道；不新增Linux号/变体，不扩展D12白名单、profile或阶段退出。

## 方法 v0.1

`--features libos-bench -- test-libos-bench`复用linux-probe的真实原生服务与受管ELF装载。
L1是独立选择项，不加入原11案例矩阵。一个无libc、静态Linux x86_64 ELF执行64次
`read(3, buffer, 17)`，文件为既有只读RAM fixture `hello from prima\n`。
open和每次lseek(3,0,SEEK_SET)在计时外；每次先填充缓冲区及两侧哨兵，测后核对17字节、
返回17和两侧0xa5。保留每个完整u64起止、返回、全部字节及哨兵，全部结束后才导出。

用户端点为LFENCE/RDTSC；起点后编译器保存计数/准备调用，真实Linux syscall陷入、
Gate等待/交付、用户态Family/Process分派、OpenFiles/VFS游标读取、授权gate_copy_out、
信号服务和答复/恢复，保存返回值，再取终点。区间包含编译器生成的包装成本；
不称纯指令或纯服务函数成本。lseek及输出不计入单次read，不代表完整open/read/close事务。
不丢弃冷样本、不扣除端点成本；不添加计时区间内的观测钩子。
测后读取既有原始Gate审计，逐条核对64次read返回/参数、64次seek及同一含代次cookie，
同时保留现有全链终结计数与失败检查。服务首请求的上下文负例落在open，位于采样前。

Prima平台pc-i440fx-8.2、TCG单线程、qemu64、1vCPU、128M、BIOS、dev构建，独立启动双轮。
可选kernel timestamp-probe只在进入用户态前用既有PIT/TSC方法校准并输出唯一非零Hz，
不改变生产ABI；普通镜像没有此输出。用户任务的既有IF/调度规则保持，不等同G5的IF=0夹具。
固定Linux6.8.0-31 guest使用同一ELF、相同fixture及32KiB栈；作为操作语义参考，
输出原始TSC分位数，不用Prima频率解释Linux计数，不比较两侧速度或声称有统计显著性。

## 接受条件与统计

用户输出唯一`S7LIBOS|samples=64;operation=read;fd=3;bytes=17;boundary=user-wrapper;verified=1`，
随后64行`S7LTIME|index;begin;end;return;hex17bytes;left;right`，索引0..63。
宿主拒绝panic、头缺失/重复/错字段、行数/列数/索引错误、零起点、时间回退、返回/字节/哨兵错误。
Prima还必须通过既有S6解析器的stdout/stderr/status/Task身份/原始审计闭合检查，
要求唯一校准和64次read/seek的原始审计回执；参考guest必须退出0并提供完整同格式样本。
nearest-rank p50/p99排名32/64；Prima以u128执行floor(cycles×10^9/hz)，Linux只报告cycles。
两次Prima启动分别统计，不混合总体。CI linux组在原差分后追加该显式命令，
逐步保留参考和Prima原始串口与最终制品身份；单次Prima30秒、参考180秒和原组预算不变。

## 验证、限制与重审

运行有变动时长正例及格式/时钟/数据/审计反例、可选特性Clippy、host、lint、
原Linux11案例双轮与新基准、完整普通BIOS及文档四项检查，从归档独立重算分位数/哈希。
本片仅为有限RAM文件的17字节读取，无磁盘、设备、动态链接、多核、KVM/实机或新证明主张。
五类单核基线不关闭2/4核、性能数值冻结/稳定性、发布许可和镜像包义务；九轨退出保持独立。
平台、负载、端点、编译器或profile变化时重审本方法，审计满仍全局fail-stop。


实现、C1自身远端核验、本地专项与普通BIOS失败/后继见
[L1报告](../reports/2026-11-08-stage7-libos-benchmark.md)。
