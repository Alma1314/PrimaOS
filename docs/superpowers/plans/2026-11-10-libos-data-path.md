---
id: plan-libos-data-path
kind: plan
authority: execution-plan
status: in-progress
lifecycle: current
updated: 2026-11-27
---

# LibOS 数据通路完整改造计划

## 目标与基线

作者要求完整改造；基线63c555e2661b371454521a2afe70f5b1010b6765，初始工作树干净。
遵循[工作契约](../specs/2026-11-10-libos-data-path.md)和
[阶段7计划](2026-11-06-stage7-execution.md)。复用已有域内C适配层、能力/Segment、
认证服务会话与OpenFiles，不重写中立内核。全范围按依赖分片，未完成项保留而不记完成。

## 工作项与依赖

| 步骤 | 依赖 | 实现范围 | 验收观察 |
|---|---|---|---|
| P1 共享batch首片 | 本片契约 | runtime显式wire编解码；服务私有快照与批处理；会话映射生命周期；C整块读写 | host负例，真实VFS/进程QEMU；消除2/3B分片的主读写路径 |
| P2 双personality共同服务 | P1、G1/G3 | Linux数据面访问Store；管理器授权/root视图；初始化fixture和镜像接线 | 两套真实白名单、显式共享、未授权拒绝，原Linux差分保持 |
| P3 异步批量与背压 | P1/P2、并发契约 | 有界提交/完成队列；通知合并；公平资源归属；取消 | 队满/重放/撤销/丢唤醒、恶意客户端及持续进展 |
| P4 Gate/IPC路径收窄 | 分段测量 | 在相同抽象迁移下优化递送/恢复、减少复制 | 同负载原始样本及安全/失败原子性回归 |
| P5 多核服务扩展 | P2/P3、D15 | per-CPU状态、AP/IPI/TLB、服务对象协调 | 2/4vCPU并发fork/exec/I/O及扩展曲线 |
| P6 撤销/恢复与总验收 | P2/P3/P5、7-D/G契约 | 在途结算、epoch、去重/结果查询、配额对账 | 崩溃窗口无重复副作用、旧权不复活、无关域进展 |

P1不替代P2–P6。完整阶段7其它义务保持原计划；本计划不把硬件/SMP、恢复和证明降级为host模型。

2026-11-11：P1已实现并通过host、lint及完整36组72轮BIOS，详见
[本片报告](../reports/2026-11-10-libos-data-path.md)。P2–P6尚未实现，完整计划仍进行中。

2026-11-16：P2拆为P2a/P2b。P2a（代理会话签发、Store绑定与受限根视图）已实现并取得
host/lint证据，详见[P2a报告](../reports/2026-11-16-stage7-proxy-session.md)；这只是授权与
视图原语，**Linux数据面迁移、fork生命周期、镜像接线和QEMU正反例仍未实现**，P2b继续。
P2b 与其后 holder 模型的依赖切片见[P2b与holder模型依赖切片计划](2026-11-16-p2b-holder-model-slices.md)；该计划不冻结 holder 语义，也不关闭 7-A。

2026-11-18：P2b-0 至 P2b-6 已落盘，Linux 数据面、管理器签发、镜像接线与代理
正反例矩阵均有 QEMU 证据；完整 36 组 72 轮 BIOS 套件通过。P2b 作为证据包收口，
**P2 本身未签署**（组织级复核待做），P3–P6 未开始。见
[P2b 收口报告](../reports/2026-11-18-p2b-closeout.md)。

2026-11-27：P5-0 已落盘——按 D15 先做 SMP 状态所有权设计与 CPU 本地基座（调度当前
任务从全局静态迁到每 CPU 槽），host/lint 与完整 72 轮 BIOS 通过。**未启动 AP、未做
`-smp 2/4`、未分区 `BOOTSTRAP_STATE`**；P5 未签署，P6 未开始。见
[S7-B.1 设计](../specs/2026-11-27-stage7-smp-state-partition.md)与
[P5-0 报告](../reports/2026-11-27-p5-0-state-partition.md)。

2026-11-27：P5-1 完成——`kernel/src/acpi.rs` 的 RSDP/RSDT/XSDT 与 MADT 纯解析，加
`smp_probe` 从 `BootInfo.rsdp_addr` 经物理直映射读表、CPUID 与 LAPIC 自识别；专用
`test-smp` 在 `-smp 2/4` 各两轮通过并复用既有启动校验器。**AP 未启动**（`online=1`），
默认 `-smp 1` 与 72 轮 BIOS 不变。见
[P5-1 报告](../reports/2026-11-27-p5-1-acpi-madt-host.md)。

2026-11-27：P5-2 AP 启动原型——`prima-ostd::smp` 实模式→长模式跳板与 `kernel::smp`
INIT-SIPI-SIPI，`test-smp-ap` 在 `-smp 2/4` 各两轮通过，AP 置在线位后 `hlt` 停放；
剔除 SMP 行后两种 vCPU 数输出逐字节相同。**AP 不参与调度**，无每 CPU GDT/TSS/IDT、
IPI、TLB shootdown；默认镜像与 72 轮不变。见
[P5-2 报告](../reports/2026-11-27-p5-2-ap-bringup.md)。

2026-11-27：P5-3a 每 CPU GDT/TSS/共享 IDT——GDT/TSS 改为每 CPU 槽，AP 入口装载本槽
并从自身 GDTR/TR 读回描述符基址，BSP 校验 BSP+AP 基址互异；`test-smp-ap` 在 `-smp 2/4`
各两轮通过（`percpu=1/3;tss=distinct`）并复用既有校验器。**AP 仍 `hlt` 且 IF=0**，无
调度/IPI/TLB；P5-3 整体未完成。见
[P5-3a 报告](../reports/2026-11-27-p5-3a-percpu-gdt-tss.md)。

## P2的具体接线与待解决边界

此节是后继实施输入，未实现，不能由P1或P2a的测试签收。P2a只完成第2条中的管理器
签发/Store绑定骨架及第3条的根视图原语；第1、4、5条与第2条的数据面迁移仍待P2b。

1. `prima-user/src/linux.rs::Shared`当前在Linux服务内持有Ram/OpenFiles；
   `prima-user/src/linux_process.rs::Family`在gate_fork前预留OFD owner，失败时释放。
   共同服务接线必须将预留、发布、回滚、退出关闭一起迁移，保持Linux的append、
   errno、stat/dirent布局及共享游标语义；仅替换read/write调用会漏掉生命周期。
2. `prima-user/src/service.rs`与`sessions.rs`目前按实际IPC发送Task及Process cookie授权。
   Linux服务代理多个Gate应用时，IPC发送者仍是服务自己，不能把应用传来的fd、PID、
   owner编号或cookie当授权。需先定义管理器签发的代理会话及其根/资源边界，
   在Store绑定代理身份、应用incarnation和OFD归属，再允许数据提交。
3. 根视图必须由服务约束，包括起点节点、绝对符号链接、`..`、已有fd和跨根rename/link。
   当前客户端能传NodeId的旧路径协议不能直接充当跨personality隔离；禁止只做字符串前缀检查。
   私有状态继续归域内前端，共同协议不要求每个私有操作都绕远访问其它personality。
4. `xtask/src/stage6.rs`当前镜像只装launcher和prima-linux；须加入共同资源服务、
   管理器授权及两种真实应用的启动/根fixture。D14存在桩不是替代验收。
5. QEMU正例必须同时观察两种真实ABI程序、明确授权的同一资源及各自私有资源；
   负例覆盖伪造代理owner、旧incarnation、猜fd/NodeId、根逃逸、未授权共享及fork失败回滚。
   原始ELF差分矩阵、原生C/Rust回归和资源回收必须同时保持。

P3还需从P1的单在途升级为真正的有界异步所有权协议；不能仅把SUBMIT改为不等待。
P3 的契约草案见契约 P3 节，执行切片见
[P3 有界异步批量切片计划](2026-11-18-p3-async-slices.md)（P3-0…P3-4）。
当前C进程级文件锁跨同步调用持有，拆分时须先固定fd→OFD引用，定义在途引用计数、
close完成点及句柄复用代次；否则释放锁后旧请求可能作用于已复用的fd/OFD。
P5不能通过给全局状态加锁或仅把QEMU参数改成`-smp 4`完成；AP启动、CPU本地状态、
IPI和TLB撤销都必须有机器执行证据。上述范围与现有阶段7计划共同约束后继实施。

## 验证与收口

每片先host聚焦检查，再workspace/lint；启动/协议改动运行相关QEMU和完整BIOS。
文档运行self-test/build/check/check --base 63c555e。失败记录命令/退出/必要摘录于报告，
原始日志放ignored target或/tmp。不能因环境缺少输入降低断言或复用其它版本PASS。
各片实际证据链接到阶段跟踪与索引；失败、范围和未完项必须随报告保留。
