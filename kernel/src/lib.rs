#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(abi_x86_interrupt)]

pub mod bootstrap;
pub mod cow;
pub mod cpu;
pub mod exc;
pub mod fault;
pub mod gdt;
pub mod idt;
pub mod mem;
pub(crate) mod safety;
pub mod user;
pub mod vspace;
