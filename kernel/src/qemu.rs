use core::arch::asm;

const EXIT_PORT: u16 = 0xf4;
pub(crate) const SUCCESS: u8 = 0x10;
pub(crate) const PANIC: u8 = 0x11;

pub(crate) fn exit(value: u8) -> ! {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") EXIT_PORT,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }

    loop {
        unsafe {
            asm!("hlt", options(nomem, nostack));
        }
    }
}
