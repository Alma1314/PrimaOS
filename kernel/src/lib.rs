#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
// framekernel 边界（SPEC-076/079）：本 crate 为上层机制，**零 unsafe**（含测试代码），全部
// 硬件触面收敛进唯一信任面 `prima-ostd`。本 crate 内不存在任何 `#[allow(unsafe_code)]`。
#![deny(unsafe_code)]

pub mod arena;
pub mod bootstrap;
pub mod cow;
pub mod cpu;
pub mod exc;
#[doc(hidden)]
pub mod exec;
pub mod fault;
pub mod frames;
pub mod gdt;
pub mod handle;
pub mod idt;
pub mod mem;
pub mod msr;
pub mod namespace;
pub mod preempt;
pub mod rights;
pub mod sched;
#[doc(hidden)]
pub mod signal;
pub mod syscall;
#[cfg(test)]
pub mod test_support;
pub mod user;
pub mod vspace;
