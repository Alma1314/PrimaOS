use core::{
    arch::asm,
    hint::spin_loop,
    sync::atomic::{AtomicBool, Ordering},
};

const COM1: u16 = 0x3f8;
const LINE_STATUS: u16 = COM1 + 5;
const TRANSMITTER_EMPTY: u8 = 1 << 5;
const TRANSMITTER_IDLE: u8 = 1 << 6;

static INITIALIZED: AtomicBool = AtomicBool::new(false);

pub(crate) fn init() {
    if INITIALIZED.load(Ordering::Acquire) {
        return;
    }

    unsafe {
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x80);
        outb(COM1, 0x03);
        outb(COM1 + 1, 0x00);
        outb(COM1 + 3, 0x03);
        outb(COM1 + 2, 0xc7);
        outb(COM1 + 4, 0x0b);
    }

    INITIALIZED.store(true, Ordering::Release);
}

pub(crate) fn write_bytes(bytes: &[u8]) {
    init();
    for &byte in bytes {
        while unsafe { inb(LINE_STATUS) } & TRANSMITTER_EMPTY == 0 {
            spin_loop();
        }
        unsafe {
            outb(COM1, byte);
        }
    }
}

pub(crate) fn flush() {
    init();
    while unsafe { inb(LINE_STATUS) } & TRANSMITTER_IDLE == 0 {
        spin_loop();
    }
}

unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}
