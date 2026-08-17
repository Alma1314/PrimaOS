#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod protocol;
mod qemu;
mod serial;

use bootloader_api::{BootInfo, entry_point};
use core::panic::PanicInfo;
use kernel::mem;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    protocol::boot_start();
    protocol::serial_ready();

    let regions: &[bootloader_api::info::MemoryRegion] = &boot_info.memory_regions;
    match mem::build_inventory(regions, boot_info.kernel_addr, boot_info.kernel_len) {
        Ok(inventory) => {
            protocol::memory_ready(&inventory);
            protocol::boot_complete();
            serial::flush();
            qemu::exit(qemu::SUCCESS)
        }
        Err(error) => {
            protocol::memory_error(error);
            serial::flush();
            qemu::exit(qemu::PANIC)
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    serial::init();
    protocol::panic_event();
    serial::flush();
    qemu::exit(qemu::PANIC)
}
