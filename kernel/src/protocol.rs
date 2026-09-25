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
    write_handle(snapshot.image_cap);
    serial::write_bytes(b";namespace_cap=");
    write_handle(snapshot.namespace_cap);
    serial::write_bytes(b";task_cap=");
    write_handle(snapshot.task_cap);
    serial::write_bytes(b";image_base=");
    write_hex_u64(snapshot.image_base);
    serial::write_bytes(b";image_len=");
    write_u64(snapshot.image_len);
    serial::write_bytes(b";image_frames=");
    write_u64(snapshot.image_frames);
    serial::write_bytes(b";free_frames=");
    write_u64(snapshot.free_frames);
    serial::write_bytes(b";image_rights=");
    write_rights(snapshot.image_rights);
    serial::write_bytes(b";namespace_rights=");
    write_rights(snapshot.namespace_rights);
    serial::write_bytes(b";task_rights=");
    write_rights(snapshot.task_rights);
    serial::write_bytes(b"\r\n");
}

/// S2-D1：按 `(generation << 32) | index` 渲染句柄（hex，与 xtask 侧镜像一致）。
fn write_handle(handle: kernel::handle::Handle) {
    write_hex_u64(handle.encode());
}

/// 按 §4.1 的规范顺序渲染权限有限集合（缓冲在栈上，无分配）。
fn write_rights(rights: kernel::rights::Rights) {
    let mut buffer = [0u8; 256];
    match rights.render(&mut buffer) {
        Some(text) => serial::write_bytes(text.as_bytes()),
        None => serial::write_bytes(b"<rights-buffer-too-small>"),
    }
}

/// 切片 P：初始 Authority 见证（`bootstrap`/`activate_init` 之后只读全局记录）。
pub(crate) fn authority_ready(record: &kernel::bootstrap::BootstrapRecord) {
    write_prefix("authority_ready", "ok");
    serial::write_bytes(b"|data=caps=");
    write_u32(record.caps);
    serial::write_bytes(b";segments=");
    write_u32(record.segments);
    serial::write_bytes(b";namespaces=");
    write_u32(record.namespaces);
    serial::write_bytes(b";booted=");
    write_u32(record.booted as u32);
    serial::write_bytes(b";initial_status=");
    serial::write_bytes(if record.initial_status_ready {
        b"ready"
    } else {
        b"other"
    });
    serial::write_bytes(b";entry_in_image=");
    write_u32(record.entry_in_image as u32);
    serial::write_bytes(b";root_ns=");
    write_handle(record.root_ns_cap);
    serial::write_bytes(b";init_running=");
    write_u32(record.init_running as u32);
    serial::write_bytes(b"\r\n");
}

/// 切片 P：帧唯一归属（W6）守恒见证。
pub(crate) fn frames_ready(witness: &kernel::frames::FrameWitness) {
    write_prefix("frames_ready", "ok");
    serial::write_bytes(b"|data=universe=");
    write_u64(witness.universe);
    serial::write_bytes(b";free=");
    write_u64(witness.free);
    serial::write_bytes(b";segment=");
    write_u64(witness.segment_frames);
    serial::write_bytes(b";table=");
    write_u64(witness.table_frames);
    serial::write_bytes(b";image=");
    write_u64(witness.image_frames);
    serial::write_bytes(b";conserved=");
    write_u32(witness.conserved as u32);
    serial::write_bytes(b";disjoint=");
    write_u32(witness.disjoint as u32);
    serial::write_bytes(b"\r\n");
}

/// SPEC-069：自建 CR3 见证（独立事件，落在 `frames_ready` 之后，只顺移 `boot_complete` 的 seq）。
pub(crate) fn self_root_ready(witness: &kernel::user::SelfRootWitness) {
    write_prefix("self_root_ready", "ok");
    serial::write_bytes(b"|data=root=");
    write_hex_u64(witness.root);
    serial::write_bytes(b";kernel_root=");
    write_hex_u64(witness.kernel_root);
    serial::write_bytes(b";image_table=");
    write_hex_u64(witness.image_table);
    serial::write_bytes(b";owned=");
    write_u32(witness.owned as u32);
    serial::write_bytes(b";pages=");
    write_u64(witness.pages);
    serial::write_bytes(b";translated=");
    write_hex_u64(witness.translated);
    serial::write_bytes(b"\r\n");
}

/// SPEC-070：`Faulted` 状态机见证（独立事件，落在 `self_root_ready` 之后）。
pub(crate) fn fault_ready(witness: &kernel::bootstrap::FaultWitness) {
    write_prefix("fault_ready", "ok");
    serial::write_bytes(b"|data=task=");
    write_u32(witness.task_id);
    serial::write_bytes(b";before=");
    write_u32(status_code(witness.before));
    serial::write_bytes(b";after=");
    write_u32(status_code(witness.after));
    serial::write_bytes(b";raised=");
    write_u32(witness.raised);
    serial::write_bytes(b";pending_source=");
    write_u32(witness.pending_source);
    serial::write_bytes(b";pending_kind=");
    write_u32(witness.pending_kind);
    serial::write_bytes(b";precond_rejected=");
    write_u32(witness.precond_rejected);
    serial::write_bytes(b";invariant_ok=");
    write_u32(witness.invariant_ok);
    serial::write_bytes(b";unschedulable=");
    write_u32(witness.unschedulable);
    serial::write_bytes(b"\r\n");
}

/// SPEC-070：Task 状态的稳定编码（协议/镜像常量共用）。
fn status_code(status: kernel::bootstrap::TaskStatus) -> u32 {
    use kernel::bootstrap::TaskStatus;
    match status {
        TaskStatus::Ready => 0,
        TaskStatus::Running => 1,
        TaskStatus::BlockedRecv => 2,
        TaskStatus::Faulted => 3,
        TaskStatus::Dead => 4,
    }
}

/// SPEC-071：§13.1/W13 Task 状态迁移审计汇总（独立事件，落在 `fault_ready` 之后）。
pub(crate) fn state_audit_ready(audit: &kernel::bootstrap::TaskStateAudit) {
    write_prefix("state_audit_ready", "ok");
    serial::write_bytes(b"|data=total=");
    write_u32(audit.total());
    serial::write_bytes(b";to_running=");
    write_u32(audit.to_running);
    serial::write_bytes(b";to_ready=");
    write_u32(audit.to_ready);
    serial::write_bytes(b";to_blocked=");
    write_u32(audit.to_blocked);
    serial::write_bytes(b";to_faulted=");
    write_u32(audit.to_faulted);
    serial::write_bytes(b";used=");
    write_u32(audit.used);
    serial::write_bytes(b";cap=");
    write_u32(audit.cap);
    serial::write_bytes(b"\r\n");
}

/// SPEC-072：§8.2 内核回收路径见证（独立事件，落在 `state_audit_ready` 之后）。
pub(crate) fn reclaim_ready(witness: &kernel::bootstrap::ReclaimWitness) {
    write_prefix("reclaim_ready", "ok");
    serial::write_bytes(b"|data=task=");
    write_u32(witness.task_id);
    serial::write_bytes(b";negative_task=");
    write_u32(witness.negative_task);
    serial::write_bytes(b";unrecoverable=");
    write_u32(witness.unrecoverable);
    serial::write_bytes(b";reclaimed=");
    write_u32(witness.reclaimed);
    serial::write_bytes(b";negative_rejected=");
    write_u32(witness.negative_rejected);
    serial::write_bytes(b";after_dead=");
    write_u32(witness.after_dead);
    serial::write_bytes(b";pending_cleared=");
    write_u32(witness.pending_cleared);
    serial::write_bytes(b";invariant_ok=");
    write_u32(witness.invariant_ok);
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

/// 切片 M：双 ring3 任务协作切换见证（真实 Task 身份 + 两次 yield 往返）。
pub(crate) fn multitask_ready(witness: &kernel::syscall::MtWitness) {
    write_prefix("multitask_ready", "ok");
    serial::write_bytes(b"|data=cs1_before=");
    write_hex_u64(witness.task1_cs_before);
    serial::write_bytes(b";cs1_after=");
    write_hex_u64(witness.task1_cs_after);
    serial::write_bytes(b";cs2=");
    write_hex_u64(witness.task2_cs);
    serial::write_bytes(b";task1_before=");
    write_hex_u64(witness.task1_id_before);
    serial::write_bytes(b";task1_after=");
    write_hex_u64(witness.task1_id_after);
    serial::write_bytes(b";task2=");
    write_hex_u64(witness.task2_id);
    serial::write_bytes(b";switches=");
    write_hex_u64(witness.switches);
    serial::write_bytes(b";task2_auth=");
    write_hex_u64(witness.task2_auth);
    serial::write_bytes(b";task2_sub=");
    write_hex_u64(witness.task2_sub);
    serial::write_bytes(b";receipt1_before=");
    write_hex_u64(witness.receipt_task1_before);
    serial::write_bytes(b";receipt1_yield=");
    write_hex_u64(witness.receipt_task1_yield);
    serial::write_bytes(b";receipt1_after=");
    write_hex_u64(witness.receipt_task1_after);
    serial::write_bytes(b";receipt2_id=");
    write_hex_u64(witness.receipt_task2_id);
    serial::write_bytes(b";receipt2_auth=");
    write_hex_u64(witness.receipt_task2_auth);
    serial::write_bytes(b";receipt2_sub=");
    write_hex_u64(witness.receipt_task2_sub);
    serial::write_bytes(b"\r\n");
}

/// 切片 N：跨任务阻塞 raise/wait 见证（task 2 trap → 内核 actor raise → task 1 pending 唤醒）。
pub(crate) fn exc_wait_ready(witness: &kernel::syscall::ExcWaitWitness) {
    write_prefix("exc_wait_ready", "ok");
    serial::write_bytes(b"|data=cs_wait=");
    write_hex_u64(witness.cs_wait);
    serial::write_bytes(b";cs_trap=");
    write_hex_u64(witness.cs_trap);
    serial::write_bytes(b";delivered=");
    write_u32(witness.delivered as u32);
    serial::write_bytes(b";pending=");
    write_u32(witness.pending as u32);
    serial::write_bytes(b";woken=");
    write_u32(witness.woken as u32);
    serial::write_bytes(b";payload=");
    write_hex_u64(witness.payload);
    serial::write_bytes(b";blocks=");
    write_u32(witness.blocks as u32);
    serial::write_bytes(b";wait_receipt=");
    write_hex_u64(witness.wait_receipt);
    serial::write_bytes(b";trap_receipt=");
    write_hex_u64(witness.trap_receipt);
    serial::write_bytes(b"\r\n");
}

/// S2-D1：句柄编码见证（严格 `(generation << 32) | index` + 初始/异常源句柄）。
pub(crate) fn handle_ready() {
    let malformed_rejected = kernel::handle::Handle::decode(999).is_err();
    write_prefix("handle_ready", "ok");
    serial::write_bytes(b"|data=index_bits=32;generation_bits=32;image=");
    write_hex_u64(kernel::bootstrap::IMAGE_CAP_HANDLE.encode());
    serial::write_bytes(b";namespace=");
    write_hex_u64(kernel::bootstrap::NAMESPACE_CAP_HANDLE.encode());
    serial::write_bytes(b";task=");
    write_hex_u64(kernel::bootstrap::TASK_CAP_HANDLE.encode());
    serial::write_bytes(b";page_fault=");
    write_hex_u64(kernel::bootstrap::EXC_PAGE_FAULT_HANDLE.encode());
    serial::write_bytes(b";syscall_trap=");
    write_hex_u64(kernel::bootstrap::EXC_SYSCALL_TRAP_HANDLE.encode());
    serial::write_bytes(b";malformed_rejected=");
    write_u32(malformed_rejected as u32);
    serial::write_bytes(b"\r\n");
}

/// S2-D2：Capability 生命周期用户化见证（派生/drop/stale/结构性根保护）。
pub(crate) fn cap_lifecycle_ready(witness: &kernel::syscall::CapWitness) {
    write_prefix("cap_lifecycle_ready", "ok");
    serial::write_bytes(b"|data=derive=");
    write_hex_u64(witness.child1);
    serial::write_bytes(b";escalate=");
    write_hex_u64(witness.escalate);
    serial::write_bytes(b";drop=");
    write_hex_u64(witness.drop);
    serial::write_bytes(b";drop_again=");
    write_hex_u64(witness.drop_again);
    serial::write_bytes(b";reuse=");
    write_hex_u64(witness.child2);
    serial::write_bytes(b";stale=");
    write_hex_u64(witness.stale);
    serial::write_bytes(b";grant=");
    write_hex_u64(witness.grant);
    serial::write_bytes(b";drop_new=");
    write_hex_u64(witness.drop2);
    serial::write_bytes(b";structural=");
    write_hex_u64(witness.structural);
    serial::write_bytes(b"\r\n");
}

/// S2-D3：MemorySegment 原语见证（create/map/清零/写回/unmap/destroy/stale/复用帧清零）。
pub(crate) fn memseg_ready(witness: &kernel::syscall::MemsegWitness) {
    write_prefix("memseg_ready", "ok");
    serial::write_bytes(b"|data=create=");
    write_hex_u64(witness.create);
    serial::write_bytes(b";map=");
    write_hex_u64(witness.map);
    serial::write_bytes(b";zero=");
    write_hex_u64(witness.zero);
    serial::write_bytes(b";write=");
    write_hex_u64(witness.write);
    serial::write_bytes(b";unmap=");
    write_hex_u64(witness.unmap);
    serial::write_bytes(b";destroy=");
    write_hex_u64(witness.destroy);
    serial::write_bytes(b";stale=");
    write_hex_u64(witness.stale);
    serial::write_bytes(b";create2=");
    write_hex_u64(witness.create2);
    serial::write_bytes(b";map2=");
    write_hex_u64(witness.map2);
    serial::write_bytes(b";zero2=");
    write_hex_u64(witness.zero2);
    serial::write_bytes(b";unmap2=");
    write_hex_u64(witness.unmap2);
    serial::write_bytes(b";destroy2=");
    write_hex_u64(witness.destroy2);
    serial::write_bytes(b"\r\n");
}

/// 切片 R：平台 bring-up 13 区域对象化见证。
///
/// 断言的是"这些区域不再是裸归属标签"：每个区域都有真实 `SegmentObject`（`segments`）、
/// 一条内核持有的可授权根 Capability（`roots`）、`TaskVspaceTable` 中的逻辑映射（`vspace`），
/// 且逐帧归属标签与 Segment id **恒等**（`unified`，即 `100+id` 命名空间已消除）。
pub(crate) fn platform_segments_ready(record: &kernel::bootstrap::PlatformSegmentsRecord) {
    write_prefix("platform_segments_ready", "ok");
    serial::write_bytes(b"|data=segments=");
    write_u32(record.segments);
    serial::write_bytes(b";first_id=");
    write_u32(record.first_id);
    serial::write_bytes(b";last_id=");
    write_u32(record.last_id);
    serial::write_bytes(b";roots=");
    write_u32(record.roots);
    serial::write_bytes(b";user_roots=");
    write_u32(record.user_roots);
    serial::write_bytes(b";kernel_roots=");
    write_u32(record.kernel_roots);
    // SPEC-066：逐槽属主（按 id 2..14 顺序；内核持有记 0 = `KERNEL_ACTOR`）。
    serial::write_bytes(b";holders=");
    for (index, holder) in record.holders.iter().enumerate() {
        if index != 0 {
            serial::write_bytes(b",");
        }
        write_u32(*holder);
    }
    serial::write_bytes(b";task_caps=");
    write_u32(record.task_caps);
    serial::write_bytes(b";task_holders=");
    for (index, holder) in record.task_holders.iter().enumerate() {
        if index != 0 {
            serial::write_bytes(b",");
        }
        write_u32(*holder);
    }
    serial::write_bytes(b";mapped=");
    write_u32(record.mapped);
    serial::write_bytes(b";direct=");
    write_u32(record.direct);
    serial::write_bytes(b";direct_ids=");
    write_u32(record.direct_ids[0]);
    serial::write_bytes(b",");
    write_u32(record.direct_ids[1]);
    serial::write_bytes(b";vspace=");
    write_u32(record.vspace);
    serial::write_bytes(b";slot_base=");
    write_u32(kernel::bootstrap::PLATFORM_SEGMENT_CAP_SLOT_BASE);
    serial::write_bytes(b";task1_cap=");
    write_hex_u64(record.task_cap_handles[0].encode());
    serial::write_bytes(b";task2_cap=");
    write_hex_u64(record.task_cap_handles[1].encode());
    serial::write_bytes(b";caps=");
    for (index, handle) in record.cap_handles.iter().enumerate() {
        if index != 0 {
            serial::write_bytes(b",");
        }
        write_hex_u64(handle.encode());
    }
    serial::write_bytes(b";frames=");
    for (index, count) in record.frame_counts.iter().enumerate() {
        if index != 0 {
            serial::write_bytes(b",");
        }
        write_u32(*count);
    }
    serial::write_bytes(b";unified=");
    write_u32(record.unified as u32);
    serial::write_bytes(b"\r\n");
}

/// 切片 S：IPC 用户化与两任务共享 Segment 见证。
///
/// 断言两个不可信 Task 的交互只能走「授权消息」与「共享 Segment」：段 cap 经**授权消息**
/// 转交（排队路径，位置经历 `InMessage`），task 2 把它映射进**自己的**根并读回 task 1 写的
/// 魔数；跨任务映射与未授权发送都被 `EACCES` 拒绝；直投路径唤醒阻塞的接收者。
pub(crate) fn ipc_ready(witness: &kernel::syscall::IpcWitness) {
    write_prefix("ipc_ready", "ok");
    serial::write_bytes(b"|data=create=");
    write_hex_u64(witness.create);
    serial::write_bytes(b";map1=");
    write_hex_u64(witness.map1);
    serial::write_bytes(b";send1=");
    write_hex_u64(witness.send1);
    serial::write_bytes(b";send2=");
    write_hex_u64(witness.send2);
    serial::write_bytes(b";recv1=");
    write_hex_u64(witness.recv1);
    serial::write_bytes(b";cap0=");
    write_hex_u64(witness.cap0);
    serial::write_bytes(b";magic=");
    write_hex_u64(witness.magic);
    serial::write_bytes(b";map2=");
    write_hex_u64(witness.map2);
    serial::write_bytes(b";cross=");
    write_hex_u64(witness.cross);
    serial::write_bytes(b";send_denied=");
    write_hex_u64(witness.send_denied);
    serial::write_bytes(b";recv2=");
    write_hex_u64(witness.recv2);
    serial::write_bytes(b";cap_count=");
    write_u32(witness.cap_count as u32);
    serial::write_bytes(b";queued=");
    write_u32(witness.queued as u32);
    serial::write_bytes(b";direct=");
    write_u32(witness.direct as u32);
    serial::write_bytes(b";blocks=");
    write_u32(witness.blocks as u32);
    serial::write_bytes(b"\r\n");
}

/// S2-D4：Task create/yield/destroy 用户原语（单核调度）见证。
pub(crate) fn task_ready(witness: &kernel::syscall::TaskWitness) {
    write_prefix("task_ready", "ok");
    serial::write_bytes(b"|data=stack_seg=");
    write_hex_u64(witness.stack_seg);
    serial::write_bytes(b";create=");
    write_hex_u64(witness.create);
    serial::write_bytes(b";bad_entry=");
    write_hex_u64(witness.bad_entry);
    serial::write_bytes(b";no_x=");
    write_hex_u64(witness.no_x);
    serial::write_bytes(b";stack_map=");
    write_hex_u64(witness.stack_map);
    serial::write_bytes(b";recv=");
    write_hex_u64(witness.recv);
    serial::write_bytes(b";deny=");
    write_hex_u64(witness.deny);
    serial::write_bytes(b";destroy=");
    write_hex_u64(witness.destroy);
    serial::write_bytes(b";again=");
    write_hex_u64(witness.again);
    serial::write_bytes(b";map_dead=");
    write_hex_u64(witness.map_dead);
    // SPEC-066：自持平台段 cap 的映射见证（正例 + 读回值 + 非属主负例）。
    serial::write_bytes(b";map_own=");
    write_hex_u64(witness.map_own);
    serial::write_bytes(b";map_value=");
    write_hex_u64(witness.map_value);
    serial::write_bytes(b";map_other=");
    write_hex_u64(witness.map_other);
    // SPEC-067：task4 承载自毁见证（栈映射、自报 id、自毁后不可映射）。
    serial::write_bytes(b";task4_cap=");
    write_hex_u64(witness.task4_cap);
    serial::write_bytes(b";task4_stack=");
    write_hex_u64(witness.task4_stack);
    serial::write_bytes(b";task4_recv=");
    write_hex_u64(witness.task4_recv);
    serial::write_bytes(b";task4_map_dead=");
    write_hex_u64(witness.task4_map_dead);
    serial::write_bytes(b";child=");
    write_u32(witness.child);
    serial::write_bytes(b";tasks=");
    write_u32(witness.tasks as u32);
    serial::write_bytes(b";caps=");
    write_u32(witness.caps as u32);
    serial::write_bytes(b";root_cleared=");
    write_u32(witness.root_cleared as u32);
    // SPEC-067：task4 自毁的内核侧事实。
    serial::write_bytes(b";grandchild=");
    write_u32(witness.grandchild);
    serial::write_bytes(b";tasks_after=");
    write_u32(witness.tasks_after as u32);
    serial::write_bytes(b";caps_after=");
    write_u32(witness.caps_after as u32);
    serial::write_bytes(b";root4_cleared=");
    write_u32(witness.root4_cleared as u32);
    serial::write_bytes(b"\r\n");
}

/// 切片 O：PIT 时钟抢占见证（per-task CR3/内核栈 + 时钟轮转空转任务）。
pub(crate) fn preempt_ready(witness: &kernel::preempt::PreemptWitness) {
    write_prefix("preempt_ready", "ok");
    serial::write_bytes(b"|data=ticks=");
    write_u32(witness.ticks as u32);
    serial::write_bytes(b";pre1=");
    write_u32(witness.preempt1 as u32);
    serial::write_bytes(b";pre2=");
    write_u32(witness.preempt2 as u32);
    serial::write_bytes(b";root1=");
    write_hex_u64(witness.root1);
    serial::write_bytes(b";root2=");
    write_hex_u64(witness.root2);
    serial::write_bytes(b";cr31=");
    write_hex_u64(witness.cr31);
    serial::write_bytes(b";cr32=");
    write_hex_u64(witness.cr32);
    serial::write_bytes(b";rsp1=");
    write_hex_u64(witness.rsp1);
    serial::write_bytes(b";rsp2=");
    write_hex_u64(witness.rsp2);
    serial::write_bytes(b";kstack1_top=");
    write_hex_u64(witness.kstack1_top);
    serial::write_bytes(b";kstack2_top=");
    write_hex_u64(witness.kstack2_top);
    serial::write_bytes(b";data1=");
    write_hex_u64(witness.data1);
    serial::write_bytes(b";data2=");
    write_hex_u64(witness.data2);
    serial::write_bytes(b";mark1=");
    write_hex_u64(witness.mark1);
    serial::write_bytes(b";mark2=");
    write_hex_u64(witness.mark2);
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
