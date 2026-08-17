use crate::serial;
use core::sync::atomic::{AtomicU32, Ordering};
use kernel::mem::{FrameInventory, MemError};

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

pub(crate) fn memory_ready(inventory: &FrameInventory) {
    write_prefix("memory_ready", "ok");
    serial::write_bytes(b"|data=usable=");
    write_u64(inventory.usable_frames as u64);
    serial::write_bytes(b";image=");
    write_u64(inventory.image_frames as u64);
    serial::write_bytes(b";free=");
    write_u64(inventory.free_frames as u64);
    serial::write_bytes(b";regions=");
    write_u64(inventory.usable_regions as u64);
    serial::write_bytes(b";kstart=");
    write_hex_u64(inventory.kernel_phys_start);
    serial::write_bytes(b";kend=");
    write_hex_u64(inventory.kernel_phys_end);
    serial::write_bytes(b"\r\n");
}

pub(crate) fn memory_error(error: MemError) {
    write_prefix("memory_error", "error");
    serial::write_bytes(b"|data=err=");
    serial::write_bytes(mem_error_name(error).as_bytes());
    serial::write_bytes(b"\r\n");
}

pub(crate) fn physical_map_ready(offset: u64, va: u64) {
    write_prefix("physical_map_ready", "ok");
    serial::write_bytes(b"|data=offset=");
    write_hex_u64(offset);
    serial::write_bytes(b";va=");
    write_hex_u64(va);
    serial::write_bytes(b"\r\n");
}

pub(crate) fn physical_map_error() {
    write_prefix("physical_map_error", "error");
    serial::write_bytes(b"\r\n");
}

fn mem_error_name(error: MemError) -> &'static str {
    match error {
        MemError::EmptyImage => "EmptyImage",
        MemError::ImageOverlapsUsable => "ImageOverlapsUsable",
        MemError::RegionsNotSortedOrOverlapping => "RegionsNotSortedOrOverlapping",
    }
}

fn write_event(event: &str, status: &str) {
    write_prefix(event, status);
    serial::write_bytes(b"\r\n");
}

fn write_prefix(event: &str, status: &str) {
    let sequence = NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    serial::write_bytes(b"PRIMA|v=1|seq=");
    write_u32(sequence);
    serial::write_bytes(b"|event=");
    serial::write_bytes(event.as_bytes());
    serial::write_bytes(b"|status=");
    serial::write_bytes(status.as_bytes());
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

fn write_u64(mut value: u64) {
    let mut buffer = [0_u8; 20];
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

fn write_hex_u64(value: u64) {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";

    serial::write_bytes(b"0x");
    if value == 0 {
        serial::write_bytes(b"0");
        return;
    }

    let mut buffer = [0_u8; 16];
    let mut index = buffer.len();
    let mut remaining = value;
    while remaining != 0 {
        index -= 1;
        buffer[index] = DIGITS[(remaining & 0xf) as usize];
        remaining >>= 4;
    }

    serial::write_bytes(&buffer[index..]);
}
