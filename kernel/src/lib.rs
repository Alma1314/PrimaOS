#![cfg_attr(not(test), no_std)]

pub mod cpu;
pub mod gdt;
pub mod mem;
pub(crate) mod safety;
