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

/// 切片 E：自建映射见证（PTE 翻译 / 重映射 + invlpg / 真实写保护 #PF）。
pub(crate) fn vspace_ready(witness: &kernel::vspace::Witness) {
    write_prefix("vspace_ready", "ok");
    serial::write_bytes(b"|data=va=");
    write_hex_u64(witness.va);
    serial::write_bytes(b";frame_a=");
    write_hex_u64(witness.frame_a);
    serial::write_bytes(b";frame_b=");
    write_hex_u64(witness.frame_b);
    serial::write_bytes(b";read_before=");
    write_hex_u64(witness.read_before);
    serial::write_bytes(b";stale_read=");
    write_hex_u64(witness.stale_read);
    serial::write_bytes(b";read_after=");
    write_hex_u64(witness.read_after);
    serial::write_bytes(b";translated=");
    write_hex_u64(witness.translated);
    serial::write_bytes(b";wp_addr=");
    write_hex_u64(witness.wp_addr);
    serial::write_bytes(b";wp_err=");
    write_hex_u64(witness.wp_err);
    serial::write_bytes(b";class=");
    serial::write_bytes(kernel::fault::classify(witness.wp_err).as_str().as_bytes());
    serial::write_bytes(b"\r\n");
}

/// 切片 F：真实 COW（写故障 → 帧字节复制 → 换映射 → 重试原指令）的证据。
pub(crate) fn cow_ready(witness: &kernel::cow::CowWitness) {
    write_prefix("cow_ready", "ok");
    serial::write_bytes(b"|data=va=");
    write_hex_u64(witness.va);
    serial::write_bytes(b";shared=");
    write_hex_u64(witness.shared);
    serial::write_bytes(b";private=");
    write_hex_u64(witness.private);
    serial::write_bytes(b";err=");
    write_hex_u64(witness.err);
    serial::write_bytes(b";class=");
    serial::write_bytes(witness.class.as_bytes());
    serial::write_bytes(b";shared0=");
    write_hex_u64(witness.shared0);
    serial::write_bytes(b";private0=");
    write_hex_u64(witness.private0);
    serial::write_bytes(b";private1=");
    write_hex_u64(witness.private1);
    serial::write_bytes(b";private_last=");
    write_hex_u64(witness.private_last);
    serial::write_bytes(b";read_back=");
    write_hex_u64(witness.read_back);
    serial::write_bytes(b"\r\n");
}

/// 切片 G：ring3 用户任务的用户态 COW 证据。
pub(crate) fn user_cow_ready(witness: &kernel::user::UserWitness) {
    write_prefix("user_cow_ready", "ok");
    serial::write_bytes(b"|data=va=");
    write_hex_u64(witness.va);
    serial::write_bytes(b";err=");
    write_hex_u64(witness.err);
    serial::write_bytes(b";class=");
    serial::write_bytes(witness.class.as_bytes());
    serial::write_bytes(b";shared0=");
    write_hex_u64(witness.shared0);
    serial::write_bytes(b";private0=");
    write_hex_u64(witness.private0);
    serial::write_bytes(b";private1=");
    write_hex_u64(witness.private1);
    serial::write_bytes(b";receipt=");
    write_hex_u64(witness.receipt);
    serial::write_bytes(b"\r\n");
}

/// 切片 H：最小 syscall ABI 的见证（CPL3 `int 0x80` → 内核分派 → 返回 CPL3）。
pub(crate) fn syscall_ready(witness: &kernel::syscall::SyscallWitness) {
    write_prefix("syscall_ready", "ok");
    serial::write_bytes(b"|data=calls=");
    write_u64(witness.calls);
    serial::write_bytes(b";cs=");
    write_hex_u64(witness.cs);
    serial::write_bytes(b";cs_all=");
    write_u32(witness.cs_all);
    serial::write_bytes(b";echo_ret=");
    write_hex_u64(witness.echo_ret);
    serial::write_bytes(b";sum_ret=");
    write_hex_u64(witness.sum_ret);
    serial::write_bytes(b";copy_ret=");
    write_hex_u64(witness.copy_ret);
    serial::write_bytes(b";fault_ret=");
    write_hex_u64(witness.fault_ret);
    serial::write_bytes(b";invalid_ret=");
    write_hex_u64(witness.invalid_ret);
    serial::write_bytes(b";bad_ret=");
    write_hex_u64(witness.bad_ret);
    serial::write_bytes(b";receipt0=");
    write_hex_u64(witness.receipt0);
    serial::write_bytes(b";receipt1=");
    write_hex_u64(witness.receipt1);
    serial::write_bytes(b";receipt2=");
    write_hex_u64(witness.receipt2);
    serial::write_bytes(b";receipt3=");
    write_hex_u64(witness.receipt3);
    serial::write_bytes(b";receipt4=");
    write_hex_u64(witness.receipt4);
    serial::write_bytes(b";receipt5=");
    write_hex_u64(witness.receipt5);
    serial::write_bytes(b";receipt6=");
    write_hex_u64(witness.receipt6);
    serial::write_bytes(b"\r\n");
}

/// 切片 K：能力授权查询见证（授予 / 缺权限拒绝 / 不存在 Capability）。
pub(crate) fn syscall_auth_ready(witness: &kernel::syscall::AuthWitness) {
    write_prefix("syscall_auth_ready", "ok");
    serial::write_bytes(b"|data=cs_all=");
    write_u32(witness.cs_all);
    serial::write_bytes(b";granted_ret=");
    write_hex_u64(witness.granted_ret);
    serial::write_bytes(b";denied_ret=");
    write_hex_u64(witness.denied_ret);
    serial::write_bytes(b";unknown_ret=");
    write_hex_u64(witness.unknown_ret);
    serial::write_bytes(b";receipt_granted=");
    write_hex_u64(witness.receipt_granted);
    serial::write_bytes(b";receipt_denied=");
    write_hex_u64(witness.receipt_denied);
    serial::write_bytes(b";receipt_unknown=");
    write_hex_u64(witness.receipt_unknown);
    serial::write_bytes(b"\r\n");
}

/// 切片 L：异常订阅/取消订阅见证（真实 Capability 门控 + Audit 计数）。
pub(crate) fn exc_ready(witness: &kernel::syscall::ExcWitness) {
    write_prefix("exc_ready", "ok");
    serial::write_bytes(b"|data=cs_all=");
    write_u32(witness.cs_all);
    serial::write_bytes(b";sub_ret=");
    write_hex_u64(witness.sub_ret);
    serial::write_bytes(b";dup_ret=");
    write_hex_u64(witness.dup_ret);
    serial::write_bytes(b";kind_ret=");
    write_hex_u64(witness.kind_ret);
    serial::write_bytes(b";sub2_ret=");
    write_hex_u64(witness.sub2_ret);
    serial::write_bytes(b";unsub_ret=");
    write_hex_u64(witness.unsub_ret);
    serial::write_bytes(b";unsub2_ret=");
    write_hex_u64(witness.unsub2_ret);
    serial::write_bytes(b";receipt_sub=");
    write_hex_u64(witness.receipt_sub);
    serial::write_bytes(b";receipt_dup=");
    write_hex_u64(witness.receipt_dup);
    serial::write_bytes(b";receipt_kind=");
    write_hex_u64(witness.receipt_kind);
    serial::write_bytes(b";receipt_sub2=");
    write_hex_u64(witness.receipt_sub2);
    serial::write_bytes(b";receipt_unsub=");
    write_hex_u64(witness.receipt_unsub);
    serial::write_bytes(b";receipt_unsub2=");
    write_hex_u64(witness.receipt_unsub2);
    serial::write_bytes(b";audit_created=");
    write_u32(witness.audit_created as u32);
    serial::write_bytes(b";audit_subscribed=");
    write_u32(witness.audit_subscribed as u32);
    serial::write_bytes(b";audit_unsubscribed=");
    write_u32(witness.audit_unsubscribed as u32);
    serial::write_bytes(b"\r\n");
}

/// 切片 L：平台初始化那次 pending 直投见证（BlockedRecv → pending → Ready）。
pub(crate) fn exc_pending_ready(witness: &kernel::syscall::PendingWitness) {
    write_prefix("exc_pending_ready", "ok");
    serial::write_bytes(b"|data=delivered=");
    write_u32(witness.delivered as u32);
    serial::write_bytes(b";pending=");
    write_u32(witness.pending as u32);
    serial::write_bytes(b";message_id=");
    write_u32(witness.message_id);
    serial::write_bytes(b";payload=");
    write_hex_u64(witness.payload);
    serial::write_bytes(b";task_ready_after=");
    write_u32(witness.task_ready_after as u32);
    serial::write_bytes(b";audit_raised=");
    write_u32(witness.audit_raised as u32);
    serial::write_bytes(b"\r\n");
}

/// 切片 J：x86-64 `syscall`/`sysret` 快路径见证（MSR 配置 + 一次真实快路径往返）。
pub(crate) fn syscall_fast_ready(witness: &kernel::syscall::FastWitness) {
    write_prefix("syscall_fast_ready", "ok");
    serial::write_bytes(b"|data=cs=");
    write_hex_u64(witness.cs);
    serial::write_bytes(b";num=");
    write_hex_u64(witness.num);
    serial::write_bytes(b";arg=");
    write_hex_u64(witness.arg);
    serial::write_bytes(b";ret=");
    write_hex_u64(witness.ret);
    serial::write_bytes(b";receipt=");
    write_hex_u64(witness.receipt);
    serial::write_bytes(b";efer_sce=");
    write_u32(witness.efer_sce);
    serial::write_bytes(b";star=");
    write_hex_u64(witness.star);
    serial::write_bytes(b";fmask=");
    write_hex_u64(witness.fmask);
    serial::write_bytes(b";lstar_ok=");
    write_u32(witness.lstar_ok);
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
