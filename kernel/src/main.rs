#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

use bootloader_api::{BootInfo, entry_point};
use core::{arch::asm, panic::PanicInfo};

entry_point!(kernel_main);

fn kernel_main(_boot_info: &'static mut BootInfo) -> ! {
    halt()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    halt()
}

fn halt() -> ! {
    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}
