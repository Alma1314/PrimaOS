---
id: spec-asterinas-pod-buffer
kind: specification
authority: implementation-contract
status: current-contract
lifecycle: current
updated: 2026-10-24
---

# Asterinas 参考续篇：POD 类型边界与恢复帧所有权

## 状态与上位边界

用户授权继续参考 Asterinas 并直接修改 master。本契约在
[前轮契约](2026-10-24-asterinas-reuse-design.md)之后收紧字节转换和内核自有缓冲，
上位语义仍为[原语规范](../../../COSIR%20原语规范.md)§12.10、
[白皮书](../../../Prima%20OS.md)的机制/personality 边界及[阶段跟踪](../../../Prima%20OS%20实现阶段.md)。
没有新增 syscall、Linux 策略、依赖、动态分配或阶段签署。

读取前序 [AF 设计](2026-09-25-framekernel-mm-pod-design.md)、
[AF 报告](../reports/2026-09-25-framekernel-mm-pod-report.md)、
[AG 设计](2026-09-25-framekernel-global-state-design.md)、
[AH 设计](2026-09-25-framekernel-entry-inversion-design.md)、
[AH 报告](../reports/2026-09-25-framekernel-entry-inversion-report.md)以及
[恢复设计](2026-09-24-stage2-exc-resume-design.md)、
[恢复报告](../reports/2026-09-24-stage2-exc-resume-report.md)。
这些历史文件记录的无约束 bytes_of 与静态可变切片，在下述精确范围得到后继修订；
历史指针步长失败、KernelActor/静态缓冲授权偏差和未实际 iretq 恢复的限制保留。

## 参考来源与适配决定

来源仍锁定 Asterinas `ceb65e3f850096d30283137845ac01ad51dfcdaf`，具体文件/哈希见
[来源登记](../../architecture/asterinas-provenance.md)。参考其 ostd-pod 对任意位模式、无隐式填充的要求，
以及 VmReader/VmWriter 把内核已借用切片的生命周期保留到访问结束的设计。
上游用 zerocopy derive 自动检查；PrimaOS 不复制宏、trait 栈或用户缺页汇编，
采用有限类型的人工 unsafe 审计，并直接用 Rust 切片完成自有缓冲访问。
这不是迁入完整 ostd-pod/VmReader/VmWriter，也不冒充上游自动布局验证。
参考 checkout 的开发/代理说明不是用户指令。

## POD 契约

`prima_ostd::pod::Pod` 是 unsafe trait，要求 Copy + 'static、所有位模式在 Rust 中有效、
所有字节初始化且无填充、无引用/指针/内部可变性；要求递归作用于字段。
提供整数及 Pod 元素数组的实现，不自动接受 bool、char、引用、指针、NonZero 或任意 repr(C) 结构。
外部扩展必须承担 unsafe 实现的审计；kernel 的零 unsafe 门禁继续阻止本层实现。

`bytes_of<T: Pod>(&T) -> &[u8]` 只借用输入，`mm::bytes_of` 兼容路径转发到同一实现。
`Pod::from_bytes` 对长度不等于 size_of 返回 None；等长时用 unaligned copy 返回值，不把输入切片转成 T 引用。
原生布局与端序不同于机器无关 wire format；SretFrame 原有显式 little-endian 编解码保留。
Rust 位模式有效也不等于 CS/RFLAGS/TSS 等 CPU 配置有效，不能绕过语义验证。

实际审计类型为 SretFrame（20×u64）、UserFrame（5×u64）、InterruptStackFrame（3×u64），
以及从 kernel/gdt 移至 OSTD entry 的原 Tss（repr(C, packed)，104 字节纯整数）。
每个 unsafe impl 带布局理由和编译期大小断言；测试保留独立字段偏移/字节编码。
Tss 提供 const new 和 set_rsp0；kernel/gdt 继续重导出 Tss，硬件布局、GDT 编码和安装顺序不变。

## 恢复帧缓冲契约

512 字节容量、SretFrame 的 160 字节 wire format、原有错误/授权口径不变。
**既有细化偏差保留**：内核静态缓冲允许非对齐字节偏移；抽象规范 §12.10 的用户 Segment 帧要求16字节对齐。
本轮不把静态缓冲工具提升为完整授权原语，也不宣称满足该对齐前置。
ResumeFrameBuffer 拥有数组；所有范围使用 checked_add 和 slice::get/get_mut。
install 接受缓冲末端的空写；超出末端、非空越界和 usize 加法溢出返回 OutOfBounds，缓冲不变。
读取返回固定大小副本；越界/溢出返回 None，不借出静态引用。

consume 在同一次可变借用内取精确帧窗口，调用验证/状态提交闭包，仅在 Ok 后清零该窗口；
Err 不改缓冲，越界不调用闭包。状态方法原有失败原子性义务不变；该辅助器不回滚任意恶意闭包、panic 或其他状态。
公开 kernel_exc_resume 在整个 consume 期间持有 RESUME_FRAME_BUFFER 锁，避免复制/验证后缓冲被覆盖，
也避免清零与新 install 交错。锁顺序固定为恢复缓冲 → BootstrapState → scheduler cells，
闭包不得反向访问恢复缓冲、阻塞或不返回。
这是同一缓冲事务的互斥，不能据此宣称所有全局状态的多单元事务或完整 SMP 安全。

删除已无调用者的 mm::bytes_at_mut，恢复帧路径不再使用裸地址或制造 &'static mut。
唯一剩余 mm::bytes_at 用于 boot ramdisk 映射；其裸地址、映射、别名和生命周期仍信任调用方。
其他 volatile/mm/entry 接口也不因本轮变为 sound，更没有用户地址权限验证或可恢复 copy_from_user。

## 启动见证与验证

resume_ready 增加必需字段 bounds_rejected=1：实跑末端越界、usize 边界的读取/恢复拒绝，
并检查非法写入未改变见证帧窗口。原有成功恢复/清零/消费/不可恢复拒绝全部保留。
xtask 缺字段、0、2 均拒绝；事件顺序和其余门限不放宽。
该字段与前轮 stack_canary 一样是 trace v1 的追加必需项，旧日志不能直接通过当前校验器。

host 覆盖错长/非对齐 POD、整数位模式/空数组、显式编码/布局、编译失败类型与借用生命周期，
以及缓冲的全偏移边界、成功局部清零和失败不变。QEMU 验证实际 TSS 与恢复链接线。
命令、失败及范围见[验证报告](../reports/2026-10-24-asterinas-pod-buffer-report.md)。
没有 Lean refinement、新 profile 或 TCB 缩小主张。更改布局、Pod 实现、原始指针接口或锁顺序必须重审。
