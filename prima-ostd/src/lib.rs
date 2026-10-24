//! Prima OS 的特权框架层（framekernel 的 OSTD 对应物）。
//!
//! 项目纪律（差距报告 G2）：**本 crate 是唯一允许 `unsafe` 的 crate**；上层 `kernel/`
//! 与之下的机制代码必须为 safe Rust（由 `#![deny(unsafe_code)]` 编译期强制）。
//!
//! 本层把带不可静态验证前置条件的硬件原语收敛为 **safe API**：前置条件降级为**文档契约**
//! 加类型/边界检查，即信任调用方——误用可产生错误行为，但设计目标是**不产生 UB**。该口径
//! 与"不变量检查不得放宽"的取舍登记在设计文档中，本层**不宣称 sound**。
//!
//! 范围演进：切片 AE（SPEC-076）建 `arch`/`sync`；切片 AF（SPEC-077）增 `mm`；
//! 切片 AH（SPEC-079）增 `entry`（帧类型/裸入口/`iretq` 跳板）。

#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
// 入口 handler（`breakpoint_handler`）用 `extern "x86-interrupt"` ABI；该 feature 只在本
// 框架层打开，上层内核因此不再需要它。
#![feature(abi_x86_interrupt)]

pub mod align;
pub mod arch;
pub mod entry;
pub mod mm;
pub mod pod;
pub mod sync;
