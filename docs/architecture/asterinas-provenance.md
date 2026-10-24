---
id: ostd-third-party
kind: reference
authority: source-provenance
status: maintained
lifecycle: current
updated: 2026-10-24
---

# Asterinas 来源与修改

上游：[Asterinas](https://github.com/asterinas/asterinas)，锁定提交
`ceb65e3f850096d30283137845ac01ad51dfcdaf`。本轮读取本地 Git checkout，未抓取上游最新版本。
版权归原贡献者，作者列表见上游对应版本 Git 历史；保留上游文件的 SPDX 标识。
许可证全文见 [LICENSE-MPL](../../prima-ostd/LICENSE-MPL)（从该提交逐字复制）。本表只登记列出的移植文件，
不改变其他 PrimaOS 文件的许可归属。

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
