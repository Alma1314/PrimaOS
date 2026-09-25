const EXIT_PORT: u16 = 0xf4;
pub(crate) const SUCCESS: u8 = 0x10;
pub(crate) const PANIC: u8 = 0x11;

pub(crate) fn exit(value: u8) -> ! {
    prima_ostd::arch::outb(EXIT_PORT, value);

    loop {
        prima_ostd::arch::hlt();
    }
}
