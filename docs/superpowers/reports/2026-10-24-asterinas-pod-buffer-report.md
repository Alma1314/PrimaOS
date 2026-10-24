---
id: report-asterinas-pod-buffer
kind: report
authority: scoped-evidence
status: verified-with-limits
lifecycle: historical
updated: 2026-10-24
---

# Asterinas 参考续篇：POD 与恢复帧缓冲验证

## 结论与范围

[契约](../specs/2026-10-24-asterinas-pod-buffer-design.md)限定本轮的人工 POD 审计、TSS 底层归属，
以及恢复帧缓冲的溢出拒绝、借用和事务互斥；没有整体移植上游内核或完整内存安全主张。

## 基线与来源

直接修改干净 master，基线 `2ac09b40ad43f336949fb3be0a7a26d8b00feaed`。
Asterinas 读取本地锁定提交 `ceb65e3f850096d30283137845ac01ad51dfcdaf`，参考文件逐个与 git show 比较并记录哈希；
没有改动参考项目、抓取最新版本或执行其代理说明。
[来源登记](../../architecture/asterinas-provenance.md)区分前轮实际源码移植和本轮设计适配。
无新 Cargo 依赖；不复制 zerocopy derive 或完整 fallible 用户内存访问框架。

## 变化与边界

- 原 bytes_of<T> 的“调用者保证没填充”改为 unsafe Pod 类型审计；整数/数组/四种硬件布局显式覆盖。
  Tss 从 kernel 移至 OSTD，保留 kernel 重导出与104字节布局；显式 wire 编码不变。
- 原恢复缓冲绕锁裸地址/静态切片改为固定数组的 checked slice 访问；删除 bytes_at_mut。
  读回副本，验证/状态提交/成功清零持同一缓冲锁，失败不清零；容量和偏移语义不扩大。
- 实际启动见证增加 bounds_rejected 必需字段，原校验继续生效。
  尚存 ramdisk bytes_at 和其他原始 VA 访问契约，不将局部收紧写成整个 OSTD soundness。

## 命令与结果

Rust pinned nightly-2026-07-30，rustc 1.99.0-nightly (1a833e165 2026-07-29)，CARGO_BUILD_JOBS=2，musl默认2 jobs。
QEMU 8.2.2 / pc-i440fx-8.2 / TCG thread=single / qemu64 / 1 vCPU / 128M / 无NIC，
保留原30秒门限和全部原断言；jobs设置不限制QEMU单核利用率。
原始日志在 gitignored target/asterinas-pod-validation/，不提交镜像或临时驱动。

| 本次命令 | 结果 | 范围 |
|---|---|---|
| `CARGO_BUILD_JOBS=2 cargo test --workspace` | PASS，922项、0失败：kernel437、prima-ostd36、xtask206；含原2个示例和新增4个compile_fail | host单元/集成/类型与生命周期门禁；不是机器证明 |
| `CARGO_BUILD_JOBS=2 cargo xtask lint` | PASS：fmt、Clippy -D warnings、x86_64-unknown-none裸机构建、29个内核源文件零unsafe | 静态/构建证据 |
| `CARGO_BUILD_JOBS=2 cargo xtask test` | PASS，完整单次命令退出0；62轮BIOS恰好覆盖31用例各2轮，含4项musl host检查及原Linux参考 | 实际TSS、恢复、权限/资源和业务验收，未使用临时分段驱动 |
| `target/debug/xtask tcb` | trusted1737、mechanism24427、total26164、ratio6.64%，unsafe71/0 | 源码代理度量，包含内联测试；没有TCB缩小主张 |
| docs self-test / build / check / check --base 2ac09b4 | PASS，300份文档 | 元数据、检索、链接、恢复治理与本轮变更 |

新增host用例：长度0..16的u64解码、实际非对齐输入、全部u8/i8位模式、整数/空数组；
独立LE字节编码对照SretFrame/TSS；bool/填充结构/引用/借用越期的编译失败；
恢复缓冲全偏移边界与usize溢出、失败缓冲全量不变、成功仅清指定窗口；trace缺字段/0/2拒绝。
既有恢复状态前置失败原子性测试继续通过。
BIOS串口实际包含 `resume_ready ... bounds_rejected=1`，并保留 `syscall_fast_ready ... stack_canary=1`。
所有原业务输出、容量、IPC双轮一致性、30秒门限和事件顺序保持。
本次完整命令通过不意味着前轮V1/W1或更早W9/VFS启动超时的根因已修复。

## 失败与限制

移动新增测试时脚本误把测试内部 use 当成模块 imports，造成括号位置错误，编译失败；
已修正测试块位置，随后 workspace 922 项通过。未压制告警或弱化断言。
[前轮](2026-10-24-asterinas-reuse-report.md)的16KiB栈溢出、V1/W1超时与62轮分段范围仍保留，
不能把本轮验证用于解释其超时根因。历史恢复的 KernelActor/静态缓冲授权偏差、状态保存与实际 iretq 执行区别保留。
本地静态恢复缓冲仍允许非对齐字节偏移，区别于抽象规范§12.10用户Segment帧的16字节对齐前置；
这是保留的细化偏差，不是新放宽该规范。
Pod 的人工审计不是 zerocopy 自动检查；任意错误 unsafe impl 会破坏保证，布局更改需重审。
没有 Miri、真实 SMP、用户缺页恢复或 Lean 验证，阶段5仍未签署、阶段6未启动、BLK-003仍Open。
