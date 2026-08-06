#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod protocol;
mod qemu;
mod serial;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;

entry_point!(kernel_main);

fn kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    protocol::boot_start();
    protocol::serial_ready();
    protocol::boot_complete();
    serial::flush();
    qemu::exit(qemu::SUCCESS)
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::init();
    protocol::panic_event();
    serial::flush();
    qemu::exit(qemu::PANIC)
}
