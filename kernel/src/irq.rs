//! 8259 IRQ delivery. Hardware entries mask one-shot device lines and retain a
//! coalesced pending bit until the ordinary exception mailbox can accept it.
use crate::{bootstrap, syscall};
use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};
static INITIALIZED: AtomicBool = AtomicBool::new(false);
static PENDING: AtomicU16 = AtomicU16::new(0);
fn initialize() {
    if INITIALIZED.swap(true, Ordering::Relaxed) {
        return;
    }
    for (port, value) in [
        (0x20, 0x11),
        (0xa0, 0x11),
        (0x21, 0x20),
        (0xa1, 0x28),
        (0x21, 4),
        (0xa1, 2),
        (0x21, 1),
        (0xa1, 1),
        (0x21, 0xff),
        (0xa1, 0xff),
    ] {
        prima_ostd::arch::outb(port, value);
    }
}
pub fn set_mask(line: u8, masked: bool) {
    initialize();
    let port = if line < 8 { 0x21 } else { 0xa1 };
    let current = prima_ostd::arch::inb(port);
    let bit = 1 << (line & 7);
    prima_ostd::arch::outb(
        port,
        if masked {
            current | bit
        } else {
            current & !bit
        },
    );
    if line >= 8 {
        let master = prima_ostd::arch::inb(0x21);
        let slave = prima_ostd::arch::inb(0xa1);
        prima_ostd::arch::outb(
            0x21,
            if slave == 0xff {
                master | 4
            } else {
                master & !4
            },
        );
    }
}
pub fn disable(line: u8) {
    set_mask(line, true);
    PENDING.fetch_and(!(1 << line), Ordering::Relaxed);
}
pub extern "C" fn dispatch(raw_line: u64) {
    let line = raw_line as u8;
    // Spurious IRQ7 sends no EOI; spurious IRQ15 acknowledges only the cascade.
    if line == 7 || line == 15 {
        let command = if line == 7 { 0x20 } else { 0xa0 };
        prima_ostd::arch::outb(command, 0x0b);
        if prima_ostd::arch::inb(command) & 0x80 == 0 {
            if line == 15 {
                prima_ostd::arch::outb(0x20, 0x20);
            }
            return;
        }
    }
    set_mask(line, true);
    PENDING.fetch_or(1 << line, Ordering::Relaxed);
    if line >= 8 {
        prima_ostd::arch::outb(0xa0, 0x20);
    }
    prima_ostd::arch::outb(0x20, 0x20);
    retry_pending();
}
pub fn retry_pending() {
    let pending = PENDING.load(Ordering::Relaxed);
    for line in 1..16 {
        if pending & (1 << line) == 0 {
            continue;
        }
        let Some(source) = bootstrap::irq_source(line) else {
            PENDING.fetch_and(!(1 << line), Ordering::Relaxed);
            continue;
        };
        if let Ok(outcome) = bootstrap::exc_raise(source, bootstrap::ExcKind::ExternalInterrupt) {
            PENDING.fetch_and(!(1 << line), Ordering::Relaxed);
            syscall::complete_irq_wakes(&outcome);
        }
    }
}
