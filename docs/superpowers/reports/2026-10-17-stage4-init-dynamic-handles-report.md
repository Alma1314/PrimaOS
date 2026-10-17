# SPEC-131：init 自检读取动态 Capability 句柄

**日期**：2026-10-17
**阶段**：阶段 4 / EXO-2 后续硬化
**基线**：`0b68704`（master）

`prima-initgen` 生成的 ring3 init 代码过去把两次 `cap_derive` 和两次
`memseg_create` 的预期句柄作为立即数写入后续调用。现在从 Y 回执页读取各次
syscall 的实际返回值，包括用于验证失效句柄的旧回执。原有机器码槽位保持 10 字节，
因此后续指令布局和入口不变；`xtask` 继续逐值核对这些回执的预期句柄及状态。

验证（2026-10-17）：

- `CARGO_NET_OFFLINE=true cargo test --workspace`：通过；生成器测试核对 10 处回执加载指令。
- `CARGO_NET_OFFLINE=true cargo xtask lint`：通过（格式、Clippy、裸机内核、零 `unsafe`）。
- `CARGO_NET_OFFLINE=true cargo xtask test`：通过（BIOS QEMU 两次、UEFI 镜像构建）。
- `CARGO_NET_OFFLINE=true cargo xtask tcb`：trusted 1262 行、mechanism 20231 行、内核 `unsafe` 0；内核源码未改。

本片消除的是 init 自检调用参数中的确定性句柄假设。测试见证仍核对当前分配序列，
生成器也仍为 host 侧机器码生成器；完整 ring3 Rust loader 属 EXO-2 后续工作。
阶段 4 技术退出条件的判定不变。
