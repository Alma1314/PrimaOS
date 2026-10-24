---
id: spec-asterinas-reuse
kind: specification
authority: implementation-contract
status: current-contract
lifecycle: current
updated: 2026-10-24
---

# Asterinas 代码复用与 OSTD 同步契约

## 状态、来源与范围

用户授权参考本地 Asterinas、允许迁入代码与设计，并直接修改 master。本契约限定本轮已迁入的组件，
不将参考项目的代理说明、README 指令或 Linux 专属策略作为 PrimaOS 的需求。
上游读取版本是 `ceb65e3f850096d30283137845ac01ad51dfcdaf`，来源文件与许可证记录于
[来源与许可](../../architecture/asterinas-provenance.md)。不依赖该 checkout 在其他机器上存在。

上位边界仍是[白皮书](../../../Prima%20OS.md)的多 personality 与机制/策略分离，
[原语规范](../../../COSIR%20原语规范.md)的帧所有权/授权，及[当前阶段](../../../Prima%20OS%20实现阶段.md)。
前序 [framekernel 差距分析](../reports/2026-08-16-framekernel-gap-analysis.md)、
[AE 设计](2026-09-25-framekernel-skeleton-design.md)、
[AG 设计](2026-09-25-framekernel-global-state-design.md)、
[AG 报告](../reports/2026-09-25-framekernel-global-state-report.md)和
[AI 收口报告](../reports/2026-09-25-framekernel-closure-report.md)保留历史判断。
本契约只后继修订“StaticCell 闭包仍仅靠单核顺序访问”的假设；不追认整个 OSTD soundness。

## 复用选择

| Asterinas 组件/设计 | PrimaOS 决定与理由 |
|---|---|
| `ostd/libs/align_ext` | 迁入实际源码、原测试；补充 checked_align_up，并接入物理帧发现、空闲帧构造、镜像边界计算，删除旧私有重复实现 |
| `ostd/src/sync/spin.rs`、IRQ guard、x86 IRQ ops | 移植最小原子锁和 IRQ 保存/恢复，不带入抢占/task 框架；用于全部既有 StaticCell 闭包 |
| framekernel 的 unsafe 收敛、所有权与临界区 RAII | 采纳：内核仍零 unsafe，底层原语留在 prima-ostd；锁 guard 不跨不返回的调度跳转 |
| `ostd-pod`、`VmReader`/`VmWriter` | 后续候选，未迁入。本地 mm 裸地址 API 仍有映射/别名契约缺口；仅复制 trait 名称不能建立类型安全。迁移须覆盖现有 TSS、帧和读写调用链 |
| Frame/Segment/PageTableCursor、RangeAllocator、IdAlloc | 后续候选，未迁入。上游依赖 alloc、CPU-local、页表元数据与资源释放；直接替换会绕过 PrimaOS 的 W6/Segment/Capability 归属与固定容量失败语义 |
| WaitQueue/Waker、CPU-local、RCU、SMP TLB shootdown | 后续候选，未迁入。需要与 Prima Task/WaitObject/generation 及调度契约对接；局部锁升级不等于 SMP 支持 |
| Linux syscall、inode/dentry/VFS、socket/process 层 | 保留为 personality 实现参考，未复制进通用内核；Linux errno/对象/策略不替代 PrimaOS 运行时和跨 OS 边界 |
| OSDK、驱动整体与平台启动代码 | 本轮不迁入；现有 bootloader/manifest/QEMU 门禁继续使用。引入设备时再按实际硬件范围适配 |

POD 候选的局部后继已由[POD/恢复帧契约](2026-10-24-asterinas-pod-buffer-design.md)覆盖：
有限类型的人工审计和自有缓冲借用，完整 VmReader/VmWriter、用户缺页恢复及剩余 VA 契约仍未迁入。

## 对齐契约

`AlignExt` 支持上游相同的 `u8/u16/u32/u64/usize`。align_up/down 要求 alignment 为 ≥2 的二次幂，
非法 alignment 显式 panic；align_up 溢出也 panic。新增 checked_align_up 对非法 alignment/溢出返回 None，
不绕回、不修改输入。帧路径只使用合法常量 4096；对 boot 区间的向上取整沿用原有 Option 失败处理，
不使用上游 panic 路径处理外部内存图。没有增加堆分配或新依赖。

## StaticCell 临界区契约

1. `with` 和 `with_mut` 均独占同一原子锁；Acquire 成功后才构造引用，闭包结束后 Release。
   不同 host 测试实例即使占不同 arena 区段，也不再同时构造整个 backing array 的可变引用。
2. bare-metal x86_64 CPL0 入口先保存 IF 并 CLI，再获取锁；退出先释放锁，只有进入时 IF=1 才 STI，
   并以 NOP 消化 interrupt shadow。IRQ 屏蔽包含 compiler memory barrier。不同单元的嵌套按闭包 LIFO 恢复原 IF。
3. host 构建只提供原子互斥、不执行特权指令；host 单测用线程局部 IF 模型检查嵌套/异常恢复。
   模型测试不能代替真实 IF=1 中断恢复的硬件证据。
4. 保留 const new、with、with_mut、as_ptr 调用形式。StaticCell 用 repr(C)，value 为首字段，锁位于值之后，
   因而 `sym FAULT_FRAME`、`sym USER_FRAME`、`sym SYSCALL_STACK` 的数据偏移保持 0。尺寸允许增加，
   不是稳定 ABI 类型；as_ptr 返回数据地址、不持锁。汇编和裸指针路径仍须保证独占、有效映射与生命周期。
5. 闭包不得递归锁同一单元、阻塞、调度或不返回；否则会死锁/遗留锁。NMI/异常重入和跨单元锁环不由本锁解决。
   单元锁不使多个全局单元之间的事务自动原子化，也没有引入抢占/SMP/锁公平性/优先级继承。
6. unwind 析构释放锁并恢复 IF，不回滚闭包已完成的值修改。bare-metal panic=abort 不承诺恢复执行。
   Once 的初始化协议保持原样，不纳入此次移植或更强声明。

调度器删除持锁 `SCRATCH.with_mut(... resume ...)`：已有局部帧在当前栈上存活，
exc_resume_entry 先压目标 iret 帧并加载全部寄存器，最后才通过 iretq 换栈；因此可以直接 resume(&frame)，
在跳转前完成所有 StaticCell 临界区，无需新增全局暂存或泄漏锁。

## 调试构建的系统调用栈边界

本轮移植暴露旧 16 KiB 快路径栈不足：实际写监视点捕获 segment 查询链的栈帧落入相邻 GDT。
仅内联短锁原语仍未通过 BIOS。最终将单个共享快路径栈显式设为 32 KiB，与既有每任务内核栈预算一致；
保留短原语内联，底部 64 字节填充固定哨兵 `0x6b737461636ba55a`（little-endian 重复）。
栈在当前单核系统中共享，不创建每任务新栈、动态分配或额外能力；不改变帧池/用户容量门限。

执行后只用 volatile 读取底部 8 个对齐 u64，不借用整块可能承载活动 Rust 栈的缓冲。
`syscall_fast_ready` 增加必需字段 `stack_canary=1`；kernel witness 和 xtask 同时严格校验，
旧日志缺少该字段、0 或其他值均不能通过当前门禁。原有事件序号和其他断言保留。
host 测试逐字节破坏全部 64 字节并验证拒绝；现有 BIOS 每轮覆盖实际栈接线与最终哨兵完整性。

这是一项明确的预算调整和执行后检测；哨兵不能阻止/可靠识别任意越界，也不能给出所有路径的峰值栈用量。
上游 `ostd/src/task/kernel_stack.rs` 采用显式预算与 MMU guard pages；本轮只采用预算/边界验证原则，
没有移植其 KVirtArea、guard pages 或异常栈。引入新深调用链时仍须重新验证。

## 验证、影响与重审触发

保留上游对齐示例/测试，补充全 u8 输入/对齐枚举、机器字边界、非法值，及物理内存既有回归；
host 验证并行加法、panic 后再次访问、IRQ 模型嵌套/恢复、真实裸入口静态地址和对齐。
完整 workspace、lint/零 unsafe、TCB 度量及双轮 BIOS 验收验证接线，结果见
[报告](../reports/2026-10-24-asterinas-reuse-report.md)。原有 QEMU 业务输出/容量/确定性断言不放宽，增加上述栈哨兵要求。

不修改 syscall 号段、能力编码、VFS 镜像格式、profile 支持范围、Lean 模型或阶段出口。
新增锁会增加静态尺寸与执行开销，没有吞吐/延迟提升主张；生成镜像、栈地址与空闲帧计数允许随构建布局变化，
但守恒/不交性与双轮确定性仍须通过。
修改 CPU/IRQ/异常模型、StaticCell 布局、as_ptr/汇编访问、不返回的控制流、对齐合法值或上游版本时必须重审。
