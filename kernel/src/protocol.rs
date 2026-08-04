use crate::serial;
use core::sync::atomic::{AtomicU32, Ordering};

static NEXT_SEQUENCE: AtomicU32 = AtomicU32::new(1);

pub(crate) fn boot_start() {
    write_event("boot_start", "ok");
}

pub(crate) fn serial_ready() {
    write_event("serial_ready", "ok");
}

pub(crate) fn boot_complete() {
    write_event("boot_complete", "ok");
}

pub(crate) fn panic_event() {
    write_event("panic", "error");
}

fn write_event(event: &str, status: &str) {
    let sequence = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    serial::write_bytes(b"PRIMA|v=1|seq=");
    write_u32(sequence);
    serial::write_bytes(b"|event=");
    serial::write_bytes(event.as_bytes());
    serial::write_bytes(b"|status=");
    serial::write_bytes(status.as_bytes());
    serial::write_bytes(b"\r\n");
}

fn write_u32(mut value: u32) {
    let mut buffer = [0_u8; 10];
    let mut index = buffer.len();

    loop {
        index -= 1;
        buffer[index] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    serial::write_bytes(&buffer[index..]);
}
