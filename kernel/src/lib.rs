#![cfg_attr(not(test), no_std)]
#![deny(unsafe_op_in_unsafe_fn)]
#![feature(abi_x86_interrupt)]

pub mod bootstrap;
pub mod cpu;
pub mod fault;
pub mod gdt;
pub mod idt;
pub mod mem;
pub(crate) mod safety;
