use crate::serial;
use core::sync::atomic::{AtomicU32, Ordering};
use kernel::bootstrap::BootstrapSnapshot;
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

pub(crate) fn bootstrap_ready(snapshot: &BootstrapSnapshot) {
    write_prefix("bootstrap_ready", "ok");
    serial::write_bytes(b"|data=task=");
    write_u32(snapshot.task_id);
    serial::write_bytes(b";caps=3;image_cap=");
    write_u32(snapshot.image_cap_id);
    serial::write_bytes(b";namespace_cap=");
    write_u32(snapshot.namespace_cap_id);
    serial::write_bytes(b";task_cap=");
    write_u32(snapshot.task_cap_id);
    serial::write_bytes(b";image_base=");
    write_hex_u64(snapshot.image_base);
    serial::write_bytes(b";image_len=");
    write_u64(snapshot.image_len);
    serial::write_bytes(b";image_frames=");
    write_u64(snapshot.image_frames);
    serial::write_bytes(b";free_frames=");
    write_u64(snapshot.free_frames);
    serial::write_bytes(b";image_rights=");
    serial::write_bytes(snapshot.image_rights.as_bytes());
    serial::write_bytes(b";namespace_rights=");
    serial::write_bytes(snapshot.namespace_rights.as_bytes());
    serial::write_bytes(b";task_rights=");
    serial::write_bytes(snapshot.task_rights.as_bytes());
    serial::write_bytes(b"\r\n");
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

pub(crate) fn page_table_ready(translated: u64) {
    write_prefix("page_table_ready", "ok");
    serial::write_bytes(b"|data=translated=");
    write_hex_u64(translated);
    serial::write_bytes(b"\r\n");
}

pub(crate) fn page_table_error() {
    write_prefix("page_table_error", "error");
    serial::write_bytes(b"\r\n");
}

pub(crate) fn gdt_tss_ready() {
    write_event("gdt_tss_ready", "ok");
}

pub(crate) fn gdt_tss_error() {
    write_prefix("gdt_tss_error", "error");
    serial::write_bytes(b"\r\n");
}

pub(crate) fn trap_ready(seen: u8) {
    write_prefix("trap_ready", "ok");
    serial::write_bytes(b"|data=seen=");
    write_u32(seen as u32);
    serial::write_bytes(b"\r\n");
}

pub(crate) fn page_fault_ready(fault_addr: u64, fault_err: u64, class: &str) {
    write_prefix("page_fault_ready", "ok");
    serial::write_bytes(b"|data=addr=");
    write_hex_u64(fault_addr);
    serial::write_bytes(b";err=");
    write_hex_u64(fault_err);
    serial::write_bytes(b";class=");
    serial::write_bytes(class.as_bytes());
    serial::write_bytes(b"\r\n");
}

pub(crate) fn idt_error() {
    write_prefix("idt_error", "error");
    serial::write_bytes(b"\r\n");
}

/// 切片 D：经真实 `iretq` 恢复进一帧后，从落点上报的恢复证据。
///
/// `sentinel`/`sentinel2` 是落点实际读回的 `r12`/`r13`；`resumed` 为两个哨兵均
/// 匹配的布尔、`rsp_ok` 为栈已切到恢复栈顶、`flags_ok` 为 RFLAGS 车道恢复。
pub(crate) fn exc_resume_ready(
    sentinel: u64,
    sentinel2: u64,
    resumed: u32,
    rsp_ok: u32,
    flags_ok: u32,
) {
    write_prefix("exc_resume_ready", "ok");
    serial::write_bytes(b"|data=sentinel=");
    write_hex_u64(sentinel);
    serial::write_bytes(b";sentinel2=");
    write_hex_u64(sentinel2);
    serial::write_bytes(b";resumed=");
    write_u32(resumed);
    serial::write_bytes(b";rsp_ok=");
    write_u32(rsp_ok);
    serial::write_bytes(b";flags_ok=");
    write_u32(flags_ok);
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
