---
id: ostd-third-party
kind: reference
authority: source-provenance
status: maintained
lifecycle: current
updated: 2026-10-25
---

# Asterinas 来源与修改

上游：[Asterinas](https://github.com/asterinas/asterinas)，锁定提交
`ceb65e3f850096d30283137845ac01ad51dfcdaf`。本轮读取本地 Git checkout，未抓取上游最新版本。
版权归原贡献者，作者列表见上游对应版本 Git 历史；保留上游文件的 SPDX 标识。
许可证全文见 [LICENSE-MPL](../../prima-ostd/LICENSE-MPL)（从该提交逐字复制）。本表只登记列出的移植文件，
不改变其他 PrimaOS 文件的许可归属。 全仓库的许可分布（GPL-3.0-only 与这三个 MPL-2.0 文件的组合依据）见
[许可证](licensing.md)。

| PrimaOS 文件 | 上游源文件（均为 MPL-2.0） | 修改 |
|---|---|---|
| [align.rs](../../prima-ostd/src/align.rs) | `ostd/libs/align_ext/src/lib.rs` | 从 crate 改为模块；修复 rustdoc 路径；增加 checked_align_up；保留原 align_up/down 与两项测试；补充边界测试 |
| [sync/lock.rs](../../prima-ostd/src/sync/lock.rs) | `ostd/src/sync/spin.rs`、`ostd/src/irq/guard.rs`、`ostd/src/arch/x86/irq/ops.rs` | 提取原子加锁/释放与 IRQ guard；去除调度/抢占/CPU-local 依赖；保留 Acquire/Release、先屏蔽 IRQ 后取锁、先释放锁后恢复 IF；使用本地 asm 与 host IF 模型 |

原理、取舍与后续候选见[移植契约](../superpowers/specs/2026-10-24-asterinas-reuse-design.md)。

## 上游输入 SHA-256

| 上游路径 | SHA-256 |
|---|---|
| `ostd/libs/align_ext/src/lib.rs` | `961fcb4bde867f1a9cb026a16af5e835b45e199c684df4bcad921876ba1bc9cd` |
| `ostd/src/sync/spin.rs` | `8d0b331b7ad31a61ba41fb838103d75c606d4709027d13782afd61eade294207` |
| `ostd/src/irq/guard.rs` | `d314d4217b1d694b2ff1e2f193a8059fc3b5e7763d5c88329ce94b06cdf11622` |
| `ostd/src/arch/x86/irq/ops.rs` | `2eaac6896c7ae83a51dff7fcc320c62c197b9329a5a762f8804685b45132c755` |
| `LICENSE-MPL` | `86cf9656479f1edb82245b985f4a2cc0d503b945766ebeda4269c60b307699ae` |
| `ostd/src/task/kernel_stack.rs`（设计参考，未移植源码） | `cfa6de805080350735e092dbdd439c608091494badc5e7834cee6c3656500ca8` |

## POD 与借用访问设计参考（后继）

本轮 [pod.rs](../../prima-ostd/src/pod.rs) 参考 ostd-pod 的类型边界重新实现，保留 MPL-2.0 标识；
不是复制上游 zerocopy trait 栈/derive 宏，也不具有上游自动布局检查。
恢复缓冲采用 VmReader/VmWriter 对已借用内核切片的生命周期原则，直接使用本地 Rust slices，
不复制用户缺页恢复、copy 汇编或读写器类。Tss 是 PrimaOS 原类型的层次迁移。
来源版本与许可证沿用上表；适配契约见[POD 与恢复帧设计](../superpowers/specs/2026-10-24-asterinas-pod-buffer-design.md)。

| 设计输入（逐个核对锁定 Git blob） | SHA-256 |
|---|---|
| `ostd/libs/ostd-pod/src/lib.rs` | `55c9c8cff15f66ed464535c80dd78f0a02ccaacf08405ed593f0656b7b080c6b` |
| `ostd/libs/ostd-pod/README.md` | `f66d28d773320b7763184fa3473148cb0cbab47d0c45882a7781f3f2eb73b684` |
| `ostd/src/mm/io/mod.rs` | `6b374ca6c1a066ec11df8ff1fcc6b3cd7c5a2f5a032fbd85c050876ff641450a` |

## 一次发布的设计参考（后继）

[Once 与内核根描述](../superpowers/specs/2026-10-24-asterinas-once-publication-design.md)借鉴上游用 Once
整体发布 boot/平台只读信息的用法；本地复用已迁入的 IRQ-safe 锁，独立修订既有 Once，增加错误重试与析构。
没有复制 Asterinas 的 boot/ACPI/logger 源码或其 spin crate 依赖，没有移植依赖 crate 的完整状态机。
本次没有新的逐字源码移植或许可证文件；前轮 MPL 标识保留，新增 lock::try_lock 属既有适配文件的本地修改。

| 设计输入（锁定提交 Git blob 已核对） | SHA-256 |
|---|---|
| `ostd/src/boot/mod.rs` | `8d5adf5603324edf3ffbdfe22b11c9436db74246843df3927926e54d28a4b76e` |
| `ostd/src/arch/x86/kernel/acpi/mod.rs` | `4b5f77e77e261ef2ceb9d48ef56c2dd00dbea87970c58eaee4afc898c4b69ab8` |
| `ostd/src/log/logger.rs` | `93a4a18caa4c37a69b54767c7a4072e60c4f742c60bef1e65f527e97f974d4f6` |

## fork COW 提交顺序的设计参考（后继）

[fork 续执行契约](../superpowers/specs/2026-10-24-fork-continuation-design.md)参考 VMAR fork 的
“父写保护后、返回前同步 TLB”责任以及 VmSpace protect 不自动刷新的契约。
本地独立实现两阶段子表构造、失败归还及既有 CR3 重装；没有复制上游函数、导入 VmSpace/RCU 或新增许可文件。
现有 MPL 文件标识保持不变；以下仅为设计输入，不表示 API/并发模型等价。

| 设计输入（逐字核对锁定提交 Git blob） | SHA-256 |
|---|---|
| `kernel/core/src/vm/vmar/vmar_impls/fork.rs` | `ce7e624d5333513a7e0fddafaacaf69b144917e25d1ae2a3607a9c5a2fca98e4` |
| `ostd/src/mm/vm_space.rs` | `6baec94428aa053d3cfb32f402f021db0b3d28e8cc4dce4b01ab8d3dfef64a84` |
