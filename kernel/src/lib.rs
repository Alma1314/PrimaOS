#![cfg_attr(not(test), no_std)]
#![feature(abi_x86_interrupt)]

pub mod cpu;
pub mod gdt;
pub mod idt;
pub mod mem;
pub(crate) mod safety;
