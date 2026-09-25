use core::{
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

    prima_ostd::arch::outb(COM1 + 1, 0x00);
    prima_ostd::arch::outb(COM1 + 3, 0x80);
    prima_ostd::arch::outb(COM1, 0x03);
    prima_ostd::arch::outb(COM1 + 1, 0x00);
    prima_ostd::arch::outb(COM1 + 3, 0x03);
    prima_ostd::arch::outb(COM1 + 2, 0xc7);
    prima_ostd::arch::outb(COM1 + 4, 0x0b);

    INITIALIZED.store(true, Ordering::Release);
}

pub(crate) fn write_bytes(bytes: &[u8]) {
    init();
    for &byte in bytes {
        while prima_ostd::arch::inb(LINE_STATUS) & TRANSMITTER_EMPTY == 0 {
            spin_loop();
        }
        prima_ostd::arch::outb(COM1, byte);
    }
}

pub(crate) fn flush() {
    init();
    while prima_ostd::arch::inb(LINE_STATUS) & TRANSMITTER_IDLE == 0 {
        spin_loop();
    }
}
