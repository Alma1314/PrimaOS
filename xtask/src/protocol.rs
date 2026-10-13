#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    BootStart,
    SerialReady,
    MemoryReady,
    PhysicalMapReady,
    PageTableReady,
    GdtTssReady,
    TrapReady,
    PageFaultReady,
    BootstrapReady,
    VspaceReady,
    CowReady,
    UserCowReady,
    SyscallReady,
    SyscallAuthReady,
    ExcReady,
    ExcPendingReady,
    SyscallFastReady,
    MultitaskReady,
    ExcWaitReady,
    HandleReady,
    CapLifecycleReady,
    MemsegReady,
    IpcReady,
    TaskReady,
    RuntimeHelloReady,
    ServiceReplyReady,
    ServiceNegotiatedReady,
    ServiceSelectedReady,
    NamespaceReady,
    PreemptReady,
    TimerReady,
    AuditReady,
    ExcResumeReady,
    PlatformSegmentsReady,
    AuthorityReady,
    FramesReady,
    SelfRootReady,
    FaultReady,
    StateAuditReady,
    ReclaimReady,
    ResumeReady,
    BootComplete,
    Panic,
    MemoryError,
    PhysicalMapError,
    PageTableError,
    GdtTssError,
    IdtError,
}

/// 切片 C：#PF 捕获闭环的探测地址，与 `kernel/src/main.rs` 的 `PROBE_FAULT_VA` 一致。
const PROBE_FAULT_VA: u64 = 0x0000_0000_0004_0000;

/// 切片 D：恢复落点应读回的哨兵寄存器值，与 `kernel/src/exc.rs` 的
/// `RESUME_SENTINEL_A`/`RESUME_SENTINEL_B` 镜像一致（跨 crate 常量需人工同步）。
const RESUME_SENTINEL_A: u64 = 0x1234_5678_9abc_def0;
const RESUME_SENTINEL_B: u64 = 0x0fed_cba9_8765_4321;

/// 切片 E：自建映射见证的镜像常量，与 `kernel/src/vspace.rs` 的
/// `VSPACE_VA`/`MAGIC_A`/`MAGIC_B` 一致（跨 crate 需人工同步）。
const VSPACE_VA: u64 = 0x0000_0080_0000_0000;
const MAGIC_A: u64 = 0xaaaa_0000_1111_2222;
const MAGIC_B: u64 = 0xbbbb_0000_3333_4444;

/// 切片 F：COW 见证的镜像常量，与 `kernel/src/cow.rs` 的
/// `COW_VA`/`pattern` 一致（跨 crate 需人工同步）。
const COW_VA: u64 = 0x0000_0800_0000_0000;
const COW_PATTERN_WORDS: u64 = 512;

pub(crate) fn cow_pattern(index: u64) -> u64 {
    0xC0FF_EE00_0000_0000 | index
}

/// 切片 G：ring3 用户任务的镜像常量，与 `kernel/src/user.rs` 一致（跨 crate 需人工同步）。
const USER_BASE_VA: u64 = 0x0000_0A00_0000_0000;
const USER_X_VA: u64 = USER_BASE_VA + 0x2000;
pub(crate) const USER_STORE_VALUE: u64 = 0x5A5A_1234_DEAD_BEEF;
/// #PF error code 的 US 位（bit 2）：置位表示故障来自 CPL3。
const PF_US: u64 = 1 << 2;

/// 切片 H/I：syscall ABI 的镜像常量，与 `kernel/src/syscall.rs` 一致（跨 crate 需人工同步）。
const SYS_ECHO: u64 = 1;
const SYS_BAD: u64 = 0xDEAD;
const ECHO_MASK: u64 = 0xFFFF_0000_FFFF_0000;
const SYS_ECHO_ARG: u64 = 0x0123_4567_89AB_CDEF;
const SUM_EXPECTED: u64 = 0x3F3F_3F3F_3F3F_3F3F;
/// 负 errno 二补码（EINVAL=-22、EFAULT=-14、ENOSYS=-38）。
const EFAULT: u64 = 0xFFFF_FFFF_FFFF_FFF2;
const EINVAL: u64 = 0xFFFF_FFFF_FFFF_FFEA;
const ENOSYS: u64 = 0xFFFF_FFFF_FFFF_FFDA;
const USER_CS: u64 = 0x33;

/// 切片 K：能力授权查询的镜像常量，与 `kernel/src/{syscall,bootstrap}.rs` 一致。
/// S2-D1：句柄 = ((generation << 32) | index)；初始根 cap 为槽 1/2/3 的 generation 1。
const IMAGE_CAP_HANDLE: u64 = 0x0000_0001_0000_0001;
const NAMESPACE_CAP_HANDLE: u64 = 0x0000_0001_0000_0002;
const TASK_CAP_HANDLE: u64 = 0x0000_0001_0000_0003;
const AUTH_GRANTED: u64 = IMAGE_CAP_HANDLE;
const EXC_PAGE_FAULT_HANDLE: u64 = 0x0000_0001_0000_0004;
const EXC_SYSCALL_TRAP_HANDLE: u64 = 0x0000_0001_0000_0005;

/// S2-D2：能力生命周期见证的镜像常量（槽 6 的 gen1 / gen2 句柄）。
const CAP_CHILD1_HANDLE: u64 = 0x0000_0001_0000_0006;
const CAP_CHILD2_HANDLE: u64 = 0x0000_0002_0000_0006;

/// S2-D3：Segment 原语见证的镜像常量。
const USER_SEG_VA: u64 = 0x0000_0A00_0000_5000;
const MEMSEG_CAP1_HANDLE: u64 = 0x0000_0003_0000_0006;
const MEMSEG_CAP2_HANDLE: u64 = 0x0000_0001_0000_0007;
const MEMSEG_PATTERN: u64 = 0xDEC0_DE00_1234_5678;
/// 负 errno 二补码（ENOENT=-2、EACCES=-13）。
const ENOENT: u64 = 0xFFFF_FFFF_FFFF_FFFE;
const EACCES: u64 = 0xFFFF_FFFF_FFFF_FFF3;

/// S2-D4：`task_create` 选定的新 Task id（init 与 task 2 之后的最小空闲 id）。
const TASK_CREATE_CHILD_ID: usize = 3;
/// S2-D4：`task_create` 返回的 manager cap 句柄（槽 10 gen1），与 `kernel/src/syscall.rs` 一致。
const TASK_MANAGER_CAP: u64 = 0x0000_0001_0000_000A;
/// `task_ready` 时刻的活动 cap 槽位数：既有 43 个，加上 Rust 客户端移交的共享 Segment cap
/// 与 SPEC-087 存活的客户端持有的 4 条 cap，以及 SPEC-088 服务端点的端点令牌/回发凭据/
/// manager/self/root-ns 共 5 条（49 → 54）。
const TASK_READY_LIVE_CAPS: usize = 63;
/// SPEC-067：`task_create` 第二次调用选定的 task id（自毁见证载体，id 不复用已销毁的 3）。
const TASK_SELF_DESTROY_ID: usize = 4;
/// SPEC-066：切片 U 的 ring3 块读回自持段映射首 qword 的期望值（X 段共享帧的 COW 模式）。
const TASK_MAP_VALUE: u64 = 0xC0FF_EE00_0000_0000;

/// 切片 R：平台 bring-up 区域对象化的镜像常量，与 `kernel/src/{bootstrap,frames}.rs` 一致。
const PLATFORM_SEGMENT_COUNT: usize = 13;
const PLATFORM_SEGMENT_FIRST_ID: u64 = 2;
const PLATFORM_SEGMENT_LAST_ID: u64 = 14;
const PLATFORM_SEGMENT_CAP_SLOT_BASE: u64 = 32;
const PLATFORM_TASK1_CAP_HANDLE: u64 = 0x0000_0001_0000_002d;
const PLATFORM_TASK2_CAP_HANDLE: u64 = 0x0000_0001_0000_002e;
const PLATFORM_ACTOR: u64 = 0;
const PLATFORM_MAPPED: u64 = 11;
const PLATFORM_DIRECT: u64 = 2;
/// SPEC-066：平台段根 cap 按属主移交后，由用户 Task 持有的段根数。
const PLATFORM_USER_ROOTS: u64 = 11;
/// SPEC-066：仍由内核持有的段根数（VSPACE、COW）。
const PLATFORM_KERNEL_ROOTS: u64 = 2;
/// SPEC-066：逐槽属主（按 Segment id 2..14 顺序；0 = `KERNEL_ACTOR`）。
const PLATFORM_SEGMENT_HOLDERS: [u64; PLATFORM_SEGMENT_COUNT] = [
    PLATFORM_ACTOR,
    PLATFORM_ACTOR,
    1,
    1,
    1,
    1,
    2,
    2,
    2,
    1,
    1,
    2,
    2,
];
/// SPEC-066：平台 Task Capability 的逐槽属主（槽 45 → task 1，槽 46 → task 2）。
const PLATFORM_TASK_HOLDERS: [u64; 2] = [1, 2];
/// 切片 S：IPC 用户化与共享 Segment 的镜像常量，与 `kernel/src/{syscall,user}.rs` 一致。
/// task 1 首个 `SYS_MEMSEG_CREATE` 的句柄：槽 8 gen1（槽 6/7 已被既有块占用且不释放）。
const IPC_SHARED_SEG_CAP: u64 = 0x0000_0001_0000_0008;
/// task 1 写进共享段、task 2 读回的魔数（证明是同一物理帧）。
const IPC_SHARED_MAGIC: u64 = 0x51A5_0000_0000_0001;
/// 直投（阻塞唤醒）路径的 payload。
const IPC_DIRECT_PAYLOAD: u64 = 0xD1EC_7000_0000_0002;

/// 每个平台区域的帧数（VSPACE 2、COW 共享+2 私有、task1 代码 2、其余各 1）。
///
/// SPEC-087：task 1 的代码段由 1 页扩到 2 页（追加 SPEC-087 握手块），故 id 4 由 1 增至 2。
const PLATFORM_FRAME_COUNTS: [u64; PLATFORM_SEGMENT_COUNT] =
    [2, 3, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1];
/// `TaskVspaceTable` 中平台区域的逻辑映射条目数。
///
/// SPEC-066：切片 U 的 ring3 块用**自持**的 X 段 cap 追加了一条映射（`USER_MAP_WITNESS_VA`），
/// 故由 11 增至 12。SPEC-087：init 把客户端移交的共享 Segment 映射进自己的根
/// （`USER_HELLO2_VA`），由 12 增至 13。
const PLATFORM_VSPACE_ENTRIES: u64 = 13;

/// 切片 L：异常订阅的镜像常量，与 `kernel/src/{syscall,bootstrap}.rs` 一致。
const EXC_PAGE_FAULT_SOURCE: u64 = 1;
const EXC_SYSCALL_TRAP_SOURCE: u64 = 2;
/// 平台初始化那次 page_fault 通知 payload = source_id(1) | kind(1)<<16。
const EXC_PAGE_FAULT_PAYLOAD: u64 = 0x1_0001;

/// 切片 M：双任务协作调度的镜像常量，与 `kernel/src/{sched,user}.rs` 一致。
const MT_TASK1: u64 = 1;
const MT_TASK2: u64 = 2;
const MT_SWITCHES: u64 = 2;

/// 切片 N：`syscall_trap_source`（id=2）的 `SyscallTrap`（kind=2）通知 payload。
const EXC_SYSCALL_TRAP_PAYLOAD: u64 = 0x2_0002;

/// 切片 O：per-task 内核栈大小，与 `kernel/src/user.rs::KSTACK_SIZE` 镜像一致。
const KSTACK_SIZE: u64 = 16 * 1024;
/// 每个任务需累计的最小被抢占次数（与 `kernel/src/preempt.rs` 的阈值镜像一致）。
const PREEMPT_THRESHOLD: u64 = 2;

/// 切片 J：`syscall`/`sysret` 快路径的镜像常量，与 `kernel/src/{syscall,msr}.rs` 一致。
const FAST_ARG: u64 = 0x0FED_CBA9_8765_4321;
const FAST_STAR: u64 = 0x0020_0008_0000_0000;
const FAST_FMASK: u64 = 0x700;

fn echo_of(arg: u64) -> u64 {
    arg ^ ECHO_MASK
}

fn echo_expected() -> u64 {
    echo_of(SYS_ECHO_ARG)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    sequence: u32,
    kind: EventKind,
    data: Option<String>,
}

const EXPECTED_EVENTS: [(u32, EventKind); 42] = [
    (1, EventKind::BootStart),
    (2, EventKind::SerialReady),
    (3, EventKind::MemoryReady),
    (4, EventKind::PhysicalMapReady),
    (5, EventKind::PageTableReady),
    (6, EventKind::GdtTssReady),
    (7, EventKind::TrapReady),
    (8, EventKind::PageFaultReady),
    (9, EventKind::BootstrapReady),
    (10, EventKind::VspaceReady),
    (11, EventKind::CowReady),
    (12, EventKind::UserCowReady),
    (13, EventKind::SyscallReady),
    (14, EventKind::SyscallAuthReady),
    (15, EventKind::ExcReady),
    (16, EventKind::ExcPendingReady),
    (17, EventKind::SyscallFastReady),
    (18, EventKind::MultitaskReady),
    (19, EventKind::ExcWaitReady),
    (20, EventKind::HandleReady),
    (21, EventKind::CapLifecycleReady),
    (22, EventKind::MemsegReady),
    (23, EventKind::IpcReady),
    (24, EventKind::TaskReady),
    (25, EventKind::RuntimeHelloReady),
    (26, EventKind::ServiceReplyReady),
    (27, EventKind::ServiceNegotiatedReady),
    (28, EventKind::ServiceSelectedReady),
    (29, EventKind::NamespaceReady),
    (30, EventKind::PreemptReady),
    (31, EventKind::TimerReady),
    (32, EventKind::AuditReady),
    (33, EventKind::ExcResumeReady),
    (34, EventKind::PlatformSegmentsReady),
    (35, EventKind::AuthorityReady),
    (36, EventKind::FramesReady),
    (37, EventKind::SelfRootReady),
    (38, EventKind::FaultReady),
    (39, EventKind::StateAuditReady),
    (40, EventKind::ReclaimReady),
    (41, EventKind::ResumeReady),
    (42, EventKind::BootComplete),
];

const LEGACY_EXPECTED_EVENTS: [(u32, EventKind); 8] = [
    (1, EventKind::BootStart),
    (2, EventKind::SerialReady),
    (3, EventKind::MemoryReady),
    (4, EventKind::PhysicalMapReady),
    (5, EventKind::PageTableReady),
    (6, EventKind::GdtTssReady),
    (7, EventKind::TrapReady),
    (8, EventKind::BootComplete),
];

pub(crate) fn validate_output(output: &str) -> Result<(), String> {
    let mut events = Vec::new();

    for raw_line in output.split_inclusive('\n') {
        let Some(line) = raw_line.strip_suffix('\n') else {
            if raw_line.starts_with("PRIMA|") {
                return Err("PRIMA record must end with LF or CRLF".to_owned());
            }
            continue;
        };
        if line.starts_with("PRIMA|") {
            let line = match line.strip_suffix('\r') {
                Some(line) if line.ends_with('\r') => {
                    return Err("PRIMA record must end with LF or CRLF".to_owned());
                }
                Some(line) => line,
                None => line,
            };
            events.push(parse_line(line)?);
        }
    }

    if events.iter().any(|event| {
        matches!(
            event.kind,
            EventKind::Panic
                | EventKind::MemoryError
                | EventKind::PhysicalMapError
                | EventKind::PageTableError
                | EventKind::GdtTssError
                | EventKind::IdtError
        )
    }) {
        return Err(
            "kernel emitted panic, memory_error, physical_map_error, page_table_error, gdt_tss_error, or idt_error event"
                .to_owned(),
        );
    }

    let observed: Vec<(u32, EventKind)> = events
        .iter()
        .map(|event| (event.sequence, event.kind))
        .collect();
    if observed != EXPECTED_EVENTS && observed != LEGACY_EXPECTED_EVENTS {
        return Err(format!(
            "unexpected PRIMA event sequence: expected {EXPECTED_EVENTS:?}, got {observed:?}"
        ));
    }

    let memory = events
        .iter()
        .find(|event| event.kind == EventKind::MemoryReady)
        .expect("memory_ready is in the expected sequence");
    let memory_kstart = validate_memory_data(memory.data.as_deref())?;

    let phys = events
        .iter()
        .find(|event| event.kind == EventKind::PhysicalMapReady)
        .expect("physical_map_ready is in the expected sequence");
    validate_physical_map_data(phys.data.as_deref())?;

    let page_table = events
        .iter()
        .find(|event| event.kind == EventKind::PageTableReady)
        .expect("page_table_ready is in the expected sequence");
    let page_table_translated = validate_page_table_data(page_table.data.as_deref())?;

    // 跨事件不变式：translate(kernel_image_offset) 必须解析回内核镜像实际装载的物理基址
    // （即 memory_ready.kstart）。这是“页表 translate 恢复内核地址”的诚实证明。
    if memory_kstart != page_table_translated {
        return Err(format!(
            "page_table_ready.translated (0x{page_table_translated:x}) must equal memory_ready.kstart (0x{memory_kstart:x}) — translate of kernel_image_offset should recover kernel_addr"
        ));
    }

    let trap = events
        .iter()
        .find(|event| event.kind == EventKind::TrapReady)
        .expect("trap_ready is in the expected sequence");
    validate_trap_data(trap.data.as_deref())?;

    if let Some(page_fault) = events
        .iter()
        .find(|event| event.kind == EventKind::PageFaultReady)
    {
        validate_page_fault_data(page_fault.data.as_deref())?;
    }

    if let Some(bootstrap) = events
        .iter()
        .find(|event| event.kind == EventKind::BootstrapReady)
    {
        validate_bootstrap_data(bootstrap.data.as_deref())?;
    }

    if let Some(vspace) = events
        .iter()
        .find(|event| event.kind == EventKind::VspaceReady)
    {
        validate_vspace_data(vspace.data.as_deref())?;
    }

    if let Some(cow) = events
        .iter()
        .find(|event| event.kind == EventKind::CowReady)
    {
        validate_cow_data(cow.data.as_deref())?;
    }

    if let Some(user_cow) = events
        .iter()
        .find(|event| event.kind == EventKind::UserCowReady)
    {
        validate_user_cow_data(user_cow.data.as_deref())?;
    }

    if let Some(syscall) = events
        .iter()
        .find(|event| event.kind == EventKind::SyscallReady)
    {
        validate_syscall_data(syscall.data.as_deref())?;
    }

    if let Some(auth) = events
        .iter()
        .find(|event| event.kind == EventKind::SyscallAuthReady)
    {
        validate_syscall_auth_data(auth.data.as_deref())?;
    }

    if let Some(exc) = events
        .iter()
        .find(|event| event.kind == EventKind::ExcReady)
    {
        validate_exc_data(exc.data.as_deref())?;
    }

    if let Some(pending) = events
        .iter()
        .find(|event| event.kind == EventKind::ExcPendingReady)
    {
        validate_exc_pending_data(pending.data.as_deref())?;
    }

    if let Some(fast) = events
        .iter()
        .find(|event| event.kind == EventKind::SyscallFastReady)
    {
        validate_syscall_fast_data(fast.data.as_deref())?;
    }

    if let Some(multitask) = events
        .iter()
        .find(|event| event.kind == EventKind::MultitaskReady)
    {
        validate_multitask_data(multitask.data.as_deref())?;
    }

    if let Some(exc_wait) = events
        .iter()
        .find(|event| event.kind == EventKind::ExcWaitReady)
    {
        validate_exc_wait_data(exc_wait.data.as_deref())?;
    }

    if let Some(handle) = events
        .iter()
        .find(|event| event.kind == EventKind::HandleReady)
    {
        validate_handle_data(handle.data.as_deref())?;
    }

    if let Some(caps) = events
        .iter()
        .find(|event| event.kind == EventKind::CapLifecycleReady)
    {
        validate_cap_lifecycle_data(caps.data.as_deref())?;
    }

    if let Some(memseg) = events
        .iter()
        .find(|event| event.kind == EventKind::MemsegReady)
    {
        validate_memseg_data(memseg.data.as_deref())?;
    }

    if let Some(ipc) = events
        .iter()
        .find(|event| event.kind == EventKind::IpcReady)
    {
        validate_ipc_data(ipc.data.as_deref())?;
    }

    if let Some(task) = events
        .iter()
        .find(|event| event.kind == EventKind::TaskReady)
    {
        validate_task_data(task.data.as_deref())?;
    }

    if let Some(hello) = events
        .iter()
        .find(|event| event.kind == EventKind::RuntimeHelloReady)
    {
        validate_runtime_hello_data(hello.data.as_deref())?;
    }

    if let Some(reply) = events
        .iter()
        .find(|event| event.kind == EventKind::ServiceReplyReady)
    {
        validate_service_reply_data(reply.data.as_deref())?;
    }

    if let Some(negotiated) = events
        .iter()
        .find(|event| event.kind == EventKind::ServiceNegotiatedReady)
    {
        validate_service_negotiated_data(negotiated.data.as_deref())?;
    }

    if let Some(selected) = events
        .iter()
        .find(|event| event.kind == EventKind::ServiceSelectedReady)
    {
        validate_service_selected_data(selected.data.as_deref())?;
    }

    if let Some(audit) = events
        .iter()
        .find(|event| event.kind == EventKind::AuditReady)
    {
        validate_audit_data(audit.data.as_deref())?;
    }

    if let Some(resume) = events
        .iter()
        .find(|event| event.kind == EventKind::ExcResumeReady)
    {
        validate_exc_resume_data(resume.data.as_deref())?;
    }

    if let Some(preempt) = events
        .iter()
        .find(|event| event.kind == EventKind::PreemptReady)
    {
        validate_preempt_data(preempt.data.as_deref())?;
    }

    if let Some(platform) = events
        .iter()
        .find(|event| event.kind == EventKind::PlatformSegmentsReady)
    {
        validate_platform_segments_data(platform.data.as_deref())?;
    }

    if let Some(authority) = events
        .iter()
        .find(|event| event.kind == EventKind::AuthorityReady)
    {
        validate_authority_data(authority.data.as_deref())?;
    }

    if let Some(frames) = events
        .iter()
        .find(|event| event.kind == EventKind::FramesReady)
    {
        validate_frames_data(frames.data.as_deref())?;
    }
    if let Some(fault) = events
        .iter()
        .find(|event| event.kind == EventKind::FaultReady)
    {
        validate_fault_data(fault.data.as_deref())?;
    }
    if let Some(self_root) = events
        .iter()
        .find(|event| event.kind == EventKind::SelfRootReady)
    {
        let translated = events
            .iter()
            .find(|event| event.kind == EventKind::PageTableReady)
            .and_then(|event| event.data.as_deref())
            .and_then(|data| {
                data.split(';')
                    .find_map(|field| field.strip_prefix("translated="))
            })
            .map(|value| value.to_owned());
        validate_self_root_data(self_root.data.as_deref(), translated.as_deref())?;
    }
    if let Some(audit) = events
        .iter()
        .find(|event| event.kind == EventKind::StateAuditReady)
    {
        validate_state_audit_data(audit.data.as_deref())?;
    }
    if let Some(reclaim) = events
        .iter()
        .find(|event| event.kind == EventKind::ReclaimReady)
    {
        validate_reclaim_data(reclaim.data.as_deref())?;
    }
    if let Some(resume) = events
        .iter()
        .find(|event| event.kind == EventKind::ResumeReady)
    {
        validate_resume_data(resume.data.as_deref())?;
    }
    if let Some(namespace) = events
        .iter()
        .find(|event| event.kind == EventKind::NamespaceReady)
    {
        validate_namespace_data(namespace.data.as_deref())?;
    }
    if let Some(timer) = events
        .iter()
        .find(|event| event.kind == EventKind::TimerReady)
    {
        validate_timer_data(timer.data.as_deref())?;
    }
    Ok(())
}

/// 切片 AB：校验 §12.10 `exc_resume`（KernelActor 代故障任务调用）的全部成功动作。
///
/// 断言：恢复成功、成功动作 5 后状态为 `Ready`、动作 1/2/3 都有实测结果、§8.4 步骤 3 的
/// 帧消费清空成立、前置 6（不可恢复种类）被拒且保持 `Faulted`、随后经 §8.2 内核路径回收，
/// 且 W9 双向不变量全程成立。
fn validate_resume_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "resume_ready event is missing its data field".to_owned())?;
    let mut fields = std::collections::BTreeMap::new();
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed resume_ready field: {field}"))?;
        fields.insert(key, value);
    }
    let get = |key: &str| -> Result<usize, String> {
        fields
            .get(key)
            .ok_or_else(|| format!("resume_ready is missing {key}"))
            .and_then(|value| parse_count(value))
    };
    if get("task")? == 0 {
        return Err("resume_ready task must be a non-zero task id".to_owned());
    }
    // 成功动作 5：`Faulted → Ready`（状态编码 0 = Ready）。
    if get("status_after")? != 0 {
        return Err(format!(
            "resume_ready status_after must be Ready (0), got {}",
            get("status_after")?
        ));
    }
    for key in [
        "resumed",
        "pending_cleared",
        "frame_saved",
        "frame_bytes_zeroed",
        "frame_consumed",
        "unrecoverable_rejected",
        "reclaimed_after",
        "invariant_ok",
    ] {
        if get(key)? != 1 {
            return Err(format!(
                "resume_ready {key} must be 1 — a successful exc_resume must clear pending_exc, save the kernel_resume_frame, zero the consumed frame bytes, be consumed on the next schedule, reject an unrecoverable kind, and leave the W9 invariant intact"
            ));
        }
    }
    Ok(())
}

/// SPEC-081：校验 §11 Namespace 路径操作的判决性见证（S3-D1/D2 与 S3-X1）。
///
/// 判决性条件：三个 Namespace 被真实创建、绑定/挂载/解析各按 §11 的成功动作留下审计事件、
/// **成环的挂载被拒绝**（`MountCycle`）、路径冲突与非法路径文本各被拒绝一次，且最终活动对象
/// 与存活条目数与整条操作链一致（`c` 经 `ns_destroy` 回收后只剩 `a` 的挂载项）。
fn validate_namespace_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "namespace_ready event is missing its data field".to_owned())?;
    let mut fields = std::collections::BTreeMap::new();
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed namespace_ready field: {field}"))?;
        fields.insert(key, value);
    }
    let get = |key: &str| -> Result<usize, String> {
        fields
            .get(key)
            .ok_or_else(|| format!("namespace_ready is missing {key}"))
            .and_then(|value| parse_count(value))
    };

    // 活动 Namespace：init + a + b（c 已被 ns_destroy 从活动表删除）。
    if get("namespaces")? != 3 {
        return Err(
            "namespace_ready namespaces must be 3 — after ns_create ×3 and ns_destroy ×1 only init, a and b stay live"
                .to_owned(),
        );
    }
    // 存活条目：只有 a 的 `x`（Mount b）留下；init 的 `lib`/`mnt` 分别被 unbind/unmount 清掉。
    if get("entries")? != 1 {
        return Err(
            "namespace_ready entries must be 1 — after unbind/unmount/destroy only a's mount entry survives"
                .to_owned(),
        );
    }
    // 审计事件计数：每类与 ring3 程序实际执行的成功操作一一对应（§13.1 要求路径操作追加 Ns* 事件）。
    for (key, expected) in [
        ("binds", 2usize),
        ("unbinds", 1),
        ("mounts", 2),
        ("unmounts", 1),
        ("resolves", 2),
    ] {
        if get(key)? != expected {
            return Err(format!(
                "namespace_ready {key} must be {expected} — the audit log must record exactly one Ns* event per successful path operation"
            ));
        }
    }
    // 拒绝路径：环 / 路径冲突 / 非法路径文本各恰好一次（失败不追加审计事件，故用实现层计数）。
    for key in [
        "cycles_rejected",
        "overlaps_rejected",
        "invalid_path_rejected",
    ] {
        if get(key)? != 1 {
            return Err(format!(
                "namespace_ready {key} must be 1 — the rejected path operation must be observed exactly once"
            ));
        }
    }
    // 环检测：每次成功的 ns_mount 都在提交前判无环，故执行次数等于成功挂载次数。
    if get("acyclic_ok")? != 2 {
        return Err(
            "namespace_ready acyclic_ok must be 2 — every successful ns_mount must have run the cycle DFS and found no cycle"
                .to_owned(),
        );
    }
    for key in [
        "create_ok",
        "resolve_ok",
        "overlap_rejected",
        "cycle_rejected",
        "evidence_ok",
    ] {
        if get(key)? != 1 {
            return Err(format!(
                "namespace_ready {key} must be 1 — the ring3 receipts must show three ns_create handles, two resolve handles, an EEXIST overlap rejection, an EINVAL cycle rejection, and an overall pass"
            ));
        }
    }
    Ok(())
}

/// 切片 AA（ISSUE-FAULT-005 的实现侧）：校验 §8.2 内核回收路径（KernelActor，无 `task_cap`）。
///
/// 判决性条件：目标必须是「不可恢复的 Faulted」、回收成功、回收后不再存活且 `pending_exc`
/// 已按 §8.2 步骤 6 清空；**可恢复**的 Faulted 必须被拒绝（它是 §12.10 的候选，回收会绕过
/// 未来的恢复路径）。
/// SPEC-082：校验 §10 Timer 的判决性见证（S3-D3/D4 与 S3-X2）。
///
/// 判决性条件：抽象时钟被 PIT tick 推进（`now > 0`）、`deadline ≤ now` 的 Timer 在**同一原子
/// 操作内**触发（S3-D4）、显式取消得到 `Cancelled` 而重复取消是 `AlreadyCancelled` 状态返回、
/// `timer_destroy` 不撤回已排队通知，且每个 `Fired` Timer 恰有一条 `TimerFired` 与恰一条同
/// `MsgId` 的 `MessageSent`（S3-X2）。
fn validate_timer_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "timer_ready event is missing its data field".to_owned())?;
    let mut fields = std::collections::BTreeMap::new();
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed timer_ready field: {field}"))?;
        fields.insert(key, value);
    }
    let get = |key: &str| -> Result<usize, String> {
        fields
            .get(key)
            .ok_or_else(|| format!("timer_ready is missing {key}"))
            .and_then(|value| parse_count(value))
    };

    // §10.3：`now` 由 CPL3 抢占的 PIT tick 推进——启动期为 0，收尾时必已 > 0。
    if get("now")? == 0 {
        return Err(
            "timer_ready now must be > 0 — the PIT tick must have advanced the abstract clock"
                .to_owned(),
        );
    }
    // 三个 `timer_set`，其中 t2 立即触发并被销毁、t3 被取消，故活动数 2。
    if get("timers")? != 2 {
        return Err(
            "timer_ready timers must be 2 — t1 (fired by advance_time) and t3 (cancelled) stay in the active table"
                .to_owned(),
        );
    }
    for (key, expected) in [
        ("sets", 3usize),
        ("fired", 2),
        ("cancelled", 1),
        ("destroyed", 1),
        ("immediate_fired", 1),
        ("already_cancelled", 1),
    ] {
        if get(key)? != expected {
            return Err(format!(
                "timer_ready {key} must be {expected} — timer_set ×3, exactly one immediate fire (S3-D4), one cancel, one destroy, one AlreadyCancelled state return"
            ));
        }
    }
    // S3-X2（exactly-once）：每个 Fired Timer 恰一条 TimerFired 且恰一条同 MsgId 的 MessageSent。
    if get("exactly_once_ok")? != 1 {
        return Err(
            "timer_ready exactly_once_ok must be 1 — each Fired timer must own exactly one TimerFired event and exactly one MessageSent carrying its MsgId"
                .to_owned(),
        );
    }
    for key in [
        "sets_ok",
        "cancel_ok",
        "already_cancelled_ok",
        "evidence_ok",
    ] {
        if get(key)? != 1 {
            return Err(format!(
                "timer_ready {key} must be 1 — the ring3 receipts must show three timer handles, a Cancelled outcome, an AlreadyCancelled state return, and an overall pass"
            ));
        }
    }
    Ok(())
}

fn validate_audit_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "audit_ready event is missing its data field".to_owned())?;
    let mut fields = std::collections::BTreeMap::new();
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed audit_ready field: {field}"))?;
        fields.insert(key, value);
    }
    for (key, expected) in [
        ("chain_len", 2usize),
        ("derived", 1),
        ("moves", 2),
        ("created", 1),
        ("destroyed", 0),
        ("stale_ok", 1),
        ("purity_ok", 1),
        ("evidence_ok", 1),
    ] {
        let actual = fields
            .get(key)
            .ok_or_else(|| format!("audit_ready is missing {key}"))
            .and_then(|value| parse_count(value))?;
        if actual != expected {
            return Err(format!(
                "audit_ready {key}: expected {expected}, got {actual}"
            ));
        }
    }
    Ok(())
}

fn validate_reclaim_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "reclaim_ready event is missing its data field".to_owned())?;
    let mut fields = std::collections::BTreeMap::new();
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed reclaim_ready field: {field}"))?;
        fields.insert(key, value);
    }
    let get = |key: &str| -> Result<usize, String> {
        fields
            .get(key)
            .ok_or_else(|| format!("reclaim_ready is missing {key}"))
            .and_then(|value| parse_count(value))
    };
    let task = get("task")?;
    let negative_task = get("negative_task")?;
    if task == negative_task || task == 0 {
        return Err(format!(
            "reclaim_ready task ({task}) and negative_task ({negative_task}) must be distinct non-zero task ids"
        ));
    }
    for key in [
        "unrecoverable",
        "reclaimed",
        "negative_rejected",
        "after_dead",
        "pending_cleared",
        "invariant_ok",
    ] {
        if get(key)? != 1 {
            return Err(format!(
                "reclaim_ready {key} must be 1 — the kernel reclaim path must destroy only an unrecoverable Faulted task, clear its pending_exc, and reject a recoverable one"
            ));
        }
    }
    Ok(())
}

/// 切片 Z（ISSUE-TASK-003）：校验 §13.1/W13 Task 状态迁移审计汇总。
///
/// 断言：四类目标状态**都**至少有 1 条真实迁移事件（证明 yield/抢占、block/wake、schedule
/// 与 §12.5 故障四条路径都接上了事件）、`total` 等于分项之和、审计日志**未被打满**
/// （`used <= cap` 且 `cap >= total`——余量枯竭会让后续受审计操作失败）。
fn validate_state_audit_data(data: Option<&str>) -> Result<(), String> {
    let data =
        data.ok_or_else(|| "state_audit_ready event is missing its data field".to_owned())?;
    let mut fields = std::collections::BTreeMap::new();
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed state_audit_ready field: {field}"))?;
        fields.insert(key, value);
    }
    let get = |key: &str| -> Result<usize, String> {
        fields
            .get(key)
            .ok_or_else(|| format!("state_audit_ready is missing {key}"))
            .and_then(|value| parse_count(value))
    };
    let total = get("total")?;
    let to_running = get("to_running")?;
    let to_ready = get("to_ready")?;
    let to_blocked = get("to_blocked")?;
    let to_faulted = get("to_faulted")?;
    let used = get("used")?;
    let cap = get("cap")?;
    for (name, value) in [
        ("to_running", to_running),
        ("to_ready", to_ready),
        ("to_blocked", to_blocked),
        ("to_faulted", to_faulted),
    ] {
        if value == 0 {
            return Err(format!(
                "state_audit_ready {name} must be non-zero — every POSIR Task transition path must append TaskStateChanged"
            ));
        }
    }
    if total != to_running + to_ready + to_blocked + to_faulted {
        return Err(format!(
            "state_audit_ready total ({total}) must equal the per-status sum ({} )",
            to_running + to_ready + to_blocked + to_faulted
        ));
    }
    if cap < total || used < total {
        return Err(format!(
            "state_audit_ready capacity too small: cap={cap}, used={used}, transitions={total}"
        ));
    }
    if used > cap {
        return Err(format!(
            "state_audit_ready audit log overflowed: used={used} > cap={cap}"
        ));
    }
    Ok(())
}

/// 切片 Y（ISSUE-TASK-001）：校验 `Faulted` 状态机见证。
///
/// §12.5：`faulting_task` 必须是 `Running`（前置 4），成功动作 3 置 `Faulted` 并写唯一
/// `pending_exc`；W9：`status == Faulted ⇔ pending_exc != None`；调度器不得隐式恢复。
fn validate_fault_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "fault_ready event is missing its data field".to_owned())?;
    let mut fields = std::collections::BTreeMap::new();
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed fault_ready field: {field}"))?;
        fields.insert(key, value);
    }
    let get = |key: &str| -> Result<usize, String> {
        fields
            .get(key)
            .ok_or_else(|| format!("fault_ready is missing {key}"))
            .and_then(|value| parse_count(value))
    };
    // §12.5 前置 4 通过：见证 Task 被调度为 Running（1）。
    if get("before")? != 1 {
        return Err(
            "fault_ready before must be Running (1) — the faulting task must satisfy §12.5 前置 4"
                .to_owned(),
        );
    }
    // 成功动作 3：置 Faulted（3）。
    if get("after")? != 3 {
        return Err("fault_ready after must be Faulted (3)".to_owned());
    }
    if get("raised")? != 1 {
        return Err("fault_ready raised must be 1".to_owned());
    }
    // `pending_exc` 必须写入且指向 page_fault 源与 PageFault 种类。
    if get("pending_source")? != 1 || get("pending_kind")? != 1 {
        return Err(
            "fault_ready must report pending_exc = {source=1(page_fault), kind=1(PageFault)}"
                .to_owned(),
        );
    }
    // 三条反例/前提：前置 4 拒绝 + W9 双向不变量 + 调度器不隐式恢复。
    for key in ["precond_rejected", "invariant_ok", "unschedulable"] {
        if get(key)? != 1 {
            return Err(format!("fault_ready {key} must be 1"));
        }
    }
    Ok(())
}

/// 切片 X（ISSUE-AUTH-001）：校验自建 CR3 见证。
///
/// 判决性条件：**运行中的地址空间**解析 `kernel_image_offset` 得到的物理地址必须等于
/// `page_table_ready.translated`（= 内核镜像物理基址），且其 PML4[2] 指向的表帧在帧注册表
/// 中的归属是「保留页表帧」——即内核镜像是经**本内核自有**的页表映射的，不再是 bootloader 页表。
fn validate_self_root_data(
    data: Option<&str>,
    page_table_translated: Option<&str>,
) -> Result<(), String> {
    let data = data.ok_or_else(|| "self_root_ready event is missing its data field".to_owned())?;
    let mut root = None;
    let mut kernel_root = None;
    let mut image_table = None;
    let mut owned = None;
    let mut pages = None;
    let mut translated = None;
    for field in data.split(';') {
        let (key, value) = field
            .split_once('=')
            .ok_or_else(|| format!("malformed self_root_ready field: {field}"))?;
        match key {
            "root" => root = Some(parse_addr(value)?),
            "kernel_root" => kernel_root = Some(parse_addr(value)?),
            "image_table" => image_table = Some(parse_addr(value)?),
            "owned" => owned = Some(parse_count(value)?),
            "pages" => pages = Some(parse_count(value)?),
            "translated" => translated = Some(parse_addr(value)?),
            other => return Err(format!("unexpected self_root_ready field: {other}")),
        }
    }
    let root = root.ok_or_else(|| "self_root_ready is missing root".to_owned())?;
    let kernel_root =
        kernel_root.ok_or_else(|| "self_root_ready is missing kernel_root".to_owned())?;
    let image_table =
        image_table.ok_or_else(|| "self_root_ready is missing image_table".to_owned())?;
    let owned = owned.ok_or_else(|| "self_root_ready is missing owned".to_owned())?;
    let pages = pages.ok_or_else(|| "self_root_ready is missing pages".to_owned())?;
    let translated =
        translated.ok_or_else(|| "self_root_ready is missing translated".to_owned())?;
    for (name, value) in [
        ("root", root),
        ("kernel_root", kernel_root),
        ("image_table", image_table),
    ] {
        if value == 0 || value % 0x1000 != 0 {
            return Err(format!(
                "self_root_ready {name} (0x{value:x}) must be a non-zero 4KiB-aligned physical frame"
            ));
        }
    }
    if owned != 1 {
        return Err(format!(
            "self_root_ready owned must be 1 (the image chain's table frame must be a registered reserved page-table frame), got {owned}"
        ));
    }
    if pages == 0 {
        return Err(
            "self_root_ready must report a non-zero materialized image page count".to_owned(),
        );
    }
    match page_table_translated {
        Some(expected) => {
            let expected = parse_addr(expected)?;
            if translated != expected {
                return Err(format!(
                    "self_root_ready translated (0x{translated:x}) must equal page_table_ready.translated (0x{expected:x}) — the running address space must resolve kernel_image_offset to the kernel image physical base"
                ));
            }
        }
        None => {
            return Err(
                "self_root_ready requires page_table_ready.translated for cross-check".to_owned(),
            );
        }
    }
    Ok(())
}

/// 切片 P：校验初始 Authority 见证（§14）。
fn validate_authority_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "authority_ready event is missing its data field".to_owned())?;
    let mut caps = None;
    let mut segments = None;
    let mut namespaces = None;
    let mut booted = None;
    let mut initial_status = None;
    let mut entry_in_image = None;
    let mut root_ns = None;
    let mut init_running = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "caps" => caps = Some(parse_count(value)?),
            "segments" => segments = Some(parse_count(value)?),
            "namespaces" => namespaces = Some(parse_count(value)?),
            "booted" => booted = Some(parse_count(value)?),
            "initial_status" => initial_status = Some(value),
            "entry_in_image" => entry_in_image = Some(parse_count(value)?),
            "root_ns" => root_ns = Some(parse_addr(value)?),
            "init_running" => init_running = Some(parse_count(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    if caps != Some(3) || segments != Some(1) || namespaces != Some(1) {
        return Err(format!(
            "authority_ready must report 3 caps / 1 segment / 1 namespace, got {caps:?}/{segments:?}/{namespaces:?}"
        ));
    }
    if booted != Some(1) || entry_in_image != Some(1) || init_running != Some(1) {
        return Err(format!(
            "authority_ready must report booted/entry_in_image/init_running = 1, got {booted:?}/{entry_in_image:?}/{init_running:?}"
        ));
    }
    if initial_status != Some("ready") {
        return Err(format!(
            "authority_ready initial_status must be ready, got {initial_status:?}"
        ));
    }
    if root_ns != Some(NAMESPACE_CAP_HANDLE) {
        return Err(format!(
            "authority_ready root_ns must be the namespace cap handle 0x{NAMESPACE_CAP_HANDLE:x}, got {root_ns:?}"
        ));
    }
    Ok(())
}

/// 切片 P：校验帧唯一归属（W6）守恒见证。
fn validate_frames_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "frames_ready event is missing its data field".to_owned())?;
    let mut universe = None;
    let mut free = None;
    let mut segment = None;
    let mut table = None;
    let mut image = None;
    let mut conserved = None;
    let mut disjoint = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "universe" => universe = Some(parse_count(value)?),
            "free" => free = Some(parse_count(value)?),
            "segment" => segment = Some(parse_count(value)?),
            "table" => table = Some(parse_count(value)?),
            "image" => image = Some(parse_count(value)?),
            "conserved" => conserved = Some(parse_count(value)?),
            "disjoint" => disjoint = Some(parse_count(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let (universe, free, segment, table, image) = (
        universe.ok_or_else(|| "frames_ready is missing universe".to_owned())?,
        free.ok_or_else(|| "frames_ready is missing free".to_owned())?,
        segment.ok_or_else(|| "frames_ready is missing segment".to_owned())?,
        table.ok_or_else(|| "frames_ready is missing table".to_owned())?,
        image.ok_or_else(|| "frames_ready is missing image".to_owned())?,
    );
    if conserved != Some(1) {
        return Err(format!(
            "frames_ready conserved must be 1 (W6 conservation holds), got {conserved:?}"
        ));
    }
    if free + segment + table + image != universe {
        return Err(format!(
            "frames_ready violates W6: free({free}) + segment({segment}) + table({table}) + image({image}) != universe({universe})"
        ));
    }
    if image == 0 || table == 0 || segment == 0 {
        return Err(format!(
            "frames_ready must report non-zero image/table/segment ownership, got image={image}, table={table}, segment={segment}"
        ));
    }
    if disjoint != Some(1) {
        return Err(format!(
            "frames_ready disjoint must be 1 (W6 per-frame disjointness holds), got {disjoint:?}"
        ));
    }
    Ok(())
}

/// 校验切片 G 的用户态 COW 证据：故障来自 CPL3（US 位）、共享帧未被改、整页已复制、
/// 重试成功、用户代码跑到 int3 写入回执。
fn validate_user_cow_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "user_cow_ready event is missing its data field".to_owned())?;
    let mut va = None;
    let mut err = None;
    let mut class = None;
    let mut shared0 = None;
    let mut private0 = None;
    let mut private1 = None;
    let mut receipt = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "va" => va = Some(parse_addr(value)?),
            "err" => err = Some(parse_addr(value)?),
            "class" => class = Some(value.to_owned()),
            "shared0" => shared0 = Some(parse_addr(value)?),
            "private0" => private0 = Some(parse_addr(value)?),
            "private1" => private1 = Some(parse_addr(value)?),
            "receipt" => receipt = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let va = va.ok_or_else(|| "missing va in user_cow_ready data".to_owned())?;
    let err = err.ok_or_else(|| "missing err in user_cow_ready data".to_owned())?;
    let class = class.ok_or_else(|| "missing class in user_cow_ready data".to_owned())?;
    let shared0 = shared0.ok_or_else(|| "missing shared0 in user_cow_ready data".to_owned())?;
    let private0 = private0.ok_or_else(|| "missing private0 in user_cow_ready data".to_owned())?;
    let private1 = private1.ok_or_else(|| "missing private1 in user_cow_ready data".to_owned())?;
    let receipt = receipt.ok_or_else(|| "missing receipt in user_cow_ready data".to_owned())?;
    if va != USER_X_VA {
        return Err(format!(
            "user_cow_ready va (0x{va:x}) must equal user page 0x{USER_X_VA:x}"
        ));
    }
    if err & PF_US == 0 {
        return Err(format!(
            "user_cow_ready err (0x{err:x}) must have the US bit set — the fault must originate from CPL3"
        ));
    }
    if err & 0x3 != 0x3 {
        return Err(format!(
            "user_cow_ready err must be present|write (0x3 low bits) for a user read-only write, got 0x{err:x}"
        ));
    }
    if class != "write_protection" {
        return Err(format!(
            "user_cow_ready class must be write_protection, got {class}"
        ));
    }
    if shared0 != cow_pattern(0) {
        return Err(format!(
            "user_cow_ready shared0 (0x{shared0:x}) must be unchanged 0x{:x} — copy-on-write must not modify the shared frame",
            cow_pattern(0)
        ));
    }
    if private0 != USER_STORE_VALUE {
        return Err(format!(
            "user_cow_ready private0 (0x{private0:x}) must equal 0x{USER_STORE_VALUE:x} — the retried user store must have executed"
        ));
    }
    if private1 != cow_pattern(1) {
        return Err(format!(
            "user_cow_ready private1 (0x{private1:x}) must equal 0x{:x} — user frame bytes not fully copied",
            cow_pattern(1)
        ));
    }
    if receipt != USER_STORE_VALUE {
        return Err(format!(
            "user_cow_ready receipt (0x{receipt:x}) must equal 0x{USER_STORE_VALUE:x} — user code did not reach its receipt store / int3"
        ));
    }
    Ok(())
}

/// 校验切片 H/I 的 syscall ABI v1 证据：6 次 `int 0x80` 均来自 CPL3（CS=0x33），
/// 覆盖多寄存器参数（SUM4）、用户缓冲读入（COPY_IN）与三类 errno 编码（EFAULT/EINVAL/ENOSYS），
/// 且用户把每个返回值都写回了回执页。
fn validate_syscall_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "syscall_ready event is missing its data field".to_owned())?;
    let mut calls = None;
    let mut cs = None;
    let mut cs_all = None;
    let mut echo_ret = None;
    let mut sum_ret = None;
    let mut copy_ret = None;
    let mut fault_ret = None;
    let mut invalid_ret = None;
    let mut bad_ret = None;
    let mut receipt0 = None;
    let mut receipt1 = None;
    let mut receipt2 = None;
    let mut receipt3 = None;
    let mut receipt4 = None;
    let mut receipt5 = None;
    let mut receipt6 = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "calls" => calls = Some(parse_decimal(value, "calls")?),
            "cs" => cs = Some(parse_addr(value)?),
            "cs_all" => cs_all = Some(parse_decimal(value, "cs_all")?),
            "echo_ret" => echo_ret = Some(parse_addr(value)?),
            "sum_ret" => sum_ret = Some(parse_addr(value)?),
            "copy_ret" => copy_ret = Some(parse_addr(value)?),
            "fault_ret" => fault_ret = Some(parse_addr(value)?),
            "invalid_ret" => invalid_ret = Some(parse_addr(value)?),
            "bad_ret" => bad_ret = Some(parse_addr(value)?),
            "receipt0" => receipt0 = Some(parse_addr(value)?),
            "receipt1" => receipt1 = Some(parse_addr(value)?),
            "receipt2" => receipt2 = Some(parse_addr(value)?),
            "receipt3" => receipt3 = Some(parse_addr(value)?),
            "receipt4" => receipt4 = Some(parse_addr(value)?),
            "receipt5" => receipt5 = Some(parse_addr(value)?),
            "receipt6" => receipt6 = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let calls = calls.ok_or_else(|| "missing calls in syscall_ready data".to_owned())?;
    let cs = cs.ok_or_else(|| "missing cs in syscall_ready data".to_owned())?;
    let cs_all = cs_all.ok_or_else(|| "missing cs_all in syscall_ready data".to_owned())?;
    let echo_ret = echo_ret.ok_or_else(|| "missing echo_ret in syscall_ready data".to_owned())?;
    let sum_ret = sum_ret.ok_or_else(|| "missing sum_ret in syscall_ready data".to_owned())?;
    let copy_ret = copy_ret.ok_or_else(|| "missing copy_ret in syscall_ready data".to_owned())?;
    let fault_ret =
        fault_ret.ok_or_else(|| "missing fault_ret in syscall_ready data".to_owned())?;
    let invalid_ret =
        invalid_ret.ok_or_else(|| "missing invalid_ret in syscall_ready data".to_owned())?;
    let bad_ret = bad_ret.ok_or_else(|| "missing bad_ret in syscall_ready data".to_owned())?;
    let receipt0 = receipt0.ok_or_else(|| "missing receipt0 in syscall_ready data".to_owned())?;
    let receipt1 = receipt1.ok_or_else(|| "missing receipt1 in syscall_ready data".to_owned())?;
    let receipt2 = receipt2.ok_or_else(|| "missing receipt2 in syscall_ready data".to_owned())?;
    let receipt3 = receipt3.ok_or_else(|| "missing receipt3 in syscall_ready data".to_owned())?;
    let receipt4 = receipt4.ok_or_else(|| "missing receipt4 in syscall_ready data".to_owned())?;
    let receipt5 = receipt5.ok_or_else(|| "missing receipt5 in syscall_ready data".to_owned())?;
    let receipt6 = receipt6.ok_or_else(|| "missing receipt6 in syscall_ready data".to_owned())?;

    if calls != 15 {
        return Err(format!(
            "syscall_ready calls ({calls}) must be 15 — 6 ABI battery + 3 authorization probes + 6 exception subscription probes"
        ));
    }
    if cs != USER_CS || cs & 3 != 3 {
        return Err(format!(
            "syscall_ready cs (0x{cs:x}) must equal CPL3 user code selector 0x{USER_CS:x} — the syscall must originate from CPL3"
        ));
    }
    if cs_all != 1 {
        return Err(format!(
            "syscall_ready cs_all ({cs_all}) must be 1 — every syscall must originate from CPL3"
        ));
    }
    if echo_ret != echo_expected() {
        return Err(format!(
            "syscall_ready echo_ret (0x{echo_ret:x}) must equal 0x{:x} — kernel echo result was not produced",
            echo_expected()
        ));
    }
    if sum_ret != SUM_EXPECTED {
        return Err(format!(
            "syscall_ready sum_ret (0x{sum_ret:x}) must equal 0x{SUM_EXPECTED:x} — multi-register arguments were not passed"
        ));
    }
    if copy_ret != USER_STORE_VALUE {
        return Err(format!(
            "syscall_ready copy_ret (0x{copy_ret:x}) must equal 0x{USER_STORE_VALUE:x} — COPY_IN did not read the post-COW private user page"
        ));
    }
    if fault_ret != EFAULT {
        return Err(format!(
            "syscall_ready fault_ret (0x{fault_ret:x}) must be -EFAULT 0x{EFAULT:x} for the bad user pointer"
        ));
    }
    if invalid_ret != EINVAL {
        return Err(format!(
            "syscall_ready invalid_ret (0x{invalid_ret:x}) must be -EINVAL 0x{EINVAL:x} for the reserved number 0"
        ));
    }
    if bad_ret != ENOSYS {
        return Err(format!(
            "syscall_ready bad_ret (0x{bad_ret:x}) must be -ENOSYS 0x{ENOSYS:x} for number 0x{SYS_BAD:x}"
        ));
    }
    if receipt0 != USER_STORE_VALUE {
        return Err(format!(
            "syscall_ready receipt0 (0x{receipt0:x}) must equal 0x{USER_STORE_VALUE:x} — COW receipt missing before syscalls"
        ));
    }
    if receipt1 != echo_expected() {
        return Err(format!(
            "syscall_ready receipt1 (0x{receipt1:x}) must equal 0x{:x} — user did not store the echo result",
            echo_expected()
        ));
    }
    if receipt2 != SUM_EXPECTED {
        return Err(format!(
            "syscall_ready receipt2 (0x{receipt2:x}) must equal 0x{SUM_EXPECTED:x} — user did not store the sum result"
        ));
    }
    if receipt3 != USER_STORE_VALUE {
        return Err(format!(
            "syscall_ready receipt3 (0x{receipt3:x}) must equal 0x{USER_STORE_VALUE:x} — user did not store the COPY_IN result"
        ));
    }
    if receipt4 != EFAULT {
        return Err(format!(
            "syscall_ready receipt4 (0x{receipt4:x}) must equal 0x{EFAULT:x} — user did not store the EFAULT result"
        ));
    }
    if receipt5 != EINVAL {
        return Err(format!(
            "syscall_ready receipt5 (0x{receipt5:x}) must equal 0x{EINVAL:x} — user did not store the EINVAL result"
        ));
    }
    if receipt6 != ENOSYS {
        return Err(format!(
            "syscall_ready receipt6 (0x{receipt6:x}) must equal 0x{ENOSYS:x} — user did not store the ENOSYS result"
        ));
    }
    Ok(())
}

/// 解析十进制计数（`calls`/`cs_all`）。
fn parse_decimal(value: &str, key: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("invalid {key} count {value}: {error}"))
}

/// 校验切片 J 的 `syscall`/`sysret` 快路径证据：MSR（EFER.SCE/STAR/LSTAR/FMASK）实测正确，
/// 且一次真实 `syscall` 指令往返来自 CPL3、返回值被用户写回。
fn validate_syscall_fast_data(data: Option<&str>) -> Result<(), String> {
    let data =
        data.ok_or_else(|| "syscall_fast_ready event is missing its data field".to_owned())?;
    let mut cs = None;
    let mut num = None;
    let mut arg = None;
    let mut ret = None;
    let mut receipt = None;
    let mut efer_sce = None;
    let mut star = None;
    let mut fmask = None;
    let mut lstar_ok = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "cs" => cs = Some(parse_addr(value)?),
            "num" => num = Some(parse_addr(value)?),
            "arg" => arg = Some(parse_addr(value)?),
            "ret" => ret = Some(parse_addr(value)?),
            "receipt" => receipt = Some(parse_addr(value)?),
            "efer_sce" => efer_sce = Some(parse_decimal(value, "efer_sce")?),
            "star" => star = Some(parse_addr(value)?),
            "fmask" => fmask = Some(parse_addr(value)?),
            "lstar_ok" => lstar_ok = Some(parse_decimal(value, "lstar_ok")?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let cs = cs.ok_or_else(|| "missing cs in syscall_fast_ready data".to_owned())?;
    let num = num.ok_or_else(|| "missing num in syscall_fast_ready data".to_owned())?;
    let arg = arg.ok_or_else(|| "missing arg in syscall_fast_ready data".to_owned())?;
    let ret = ret.ok_or_else(|| "missing ret in syscall_fast_ready data".to_owned())?;
    let receipt = receipt.ok_or_else(|| "missing receipt in syscall_fast_ready data".to_owned())?;
    let efer_sce =
        efer_sce.ok_or_else(|| "missing efer_sce in syscall_fast_ready data".to_owned())?;
    let star = star.ok_or_else(|| "missing star in syscall_fast_ready data".to_owned())?;
    let fmask = fmask.ok_or_else(|| "missing fmask in syscall_fast_ready data".to_owned())?;
    let lstar_ok =
        lstar_ok.ok_or_else(|| "missing lstar_ok in syscall_fast_ready data".to_owned())?;

    if cs != USER_CS || cs & 3 != 3 {
        return Err(format!(
            "syscall_fast_ready cs (0x{cs:x}) must equal CPL3 user code selector 0x{USER_CS:x} — the fast syscall must originate from CPL3"
        ));
    }
    if num != SYS_ECHO {
        return Err(format!(
            "syscall_fast_ready num (0x{num:x}) must be the echo syscall 0x{SYS_ECHO:x}"
        ));
    }
    if arg != FAST_ARG {
        return Err(format!(
            "syscall_fast_ready arg (0x{arg:x}) must equal 0x{FAST_ARG:x} — argument not passed through registers"
        ));
    }
    if ret != echo_of(FAST_ARG) {
        return Err(format!(
            "syscall_fast_ready ret (0x{ret:x}) must equal 0x{:x} — fast syscall did not complete the kernel round trip",
            echo_of(FAST_ARG)
        ));
    }
    if receipt != ret {
        return Err(format!(
            "syscall_fast_ready receipt (0x{receipt:x}) must equal ret 0x{ret:x} — user did not store the fast syscall result"
        ));
    }
    if efer_sce != 1 {
        return Err(format!(
            "syscall_fast_ready efer_sce ({efer_sce}) must be 1 — EFER.SCE must enable syscall/sysret"
        ));
    }
    if star != FAST_STAR {
        return Err(format!(
            "syscall_fast_ready star (0x{star:x}) must equal 0x{FAST_STAR:x} — STAR selectors are wrong"
        ));
    }
    if fmask != FAST_FMASK {
        return Err(format!(
            "syscall_fast_ready fmask (0x{fmask:x}) must equal 0x{FAST_FMASK:x} — FMASK must clear IF/TF/DF"
        ));
    }
    if lstar_ok != 1 {
        return Err(format!(
            "syscall_fast_ready lstar_ok ({lstar_ok}) must be 1 — IA32_LSTAR must point at the fast entry"
        ));
    }
    Ok(())
}

/// 校验切片 K 的能力授权查询证据：全部来自 CPL3；持有 `SegMapR` 授予成功（返回 cap id）、
/// 缺 `SegMapW` 返回 `EACCES`、不存在 Capability 返回 `ENOENT`，且用户写回回执。
fn validate_syscall_auth_data(data: Option<&str>) -> Result<(), String> {
    let data =
        data.ok_or_else(|| "syscall_auth_ready event is missing its data field".to_owned())?;
    let mut cs_all = None;
    let mut granted_ret = None;
    let mut denied_ret = None;
    let mut unknown_ret = None;
    let mut receipt_granted = None;
    let mut receipt_denied = None;
    let mut receipt_unknown = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "cs_all" => cs_all = Some(parse_decimal(value, "cs_all")?),
            "granted_ret" => granted_ret = Some(parse_addr(value)?),
            "denied_ret" => denied_ret = Some(parse_addr(value)?),
            "unknown_ret" => unknown_ret = Some(parse_addr(value)?),
            "receipt_granted" => receipt_granted = Some(parse_addr(value)?),
            "receipt_denied" => receipt_denied = Some(parse_addr(value)?),
            "receipt_unknown" => receipt_unknown = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let cs_all = cs_all.ok_or_else(|| "missing cs_all in syscall_auth_ready data".to_owned())?;
    let granted_ret =
        granted_ret.ok_or_else(|| "missing granted_ret in syscall_auth_ready data".to_owned())?;
    let denied_ret =
        denied_ret.ok_or_else(|| "missing denied_ret in syscall_auth_ready data".to_owned())?;
    let unknown_ret =
        unknown_ret.ok_or_else(|| "missing unknown_ret in syscall_auth_ready data".to_owned())?;
    let receipt_granted = receipt_granted
        .ok_or_else(|| "missing receipt_granted in syscall_auth_ready data".to_owned())?;
    let receipt_denied = receipt_denied
        .ok_or_else(|| "missing receipt_denied in syscall_auth_ready data".to_owned())?;
    let receipt_unknown = receipt_unknown
        .ok_or_else(|| "missing receipt_unknown in syscall_auth_ready data".to_owned())?;

    if cs_all != 1 {
        return Err(format!(
            "syscall_auth_ready cs_all ({cs_all}) must be 1 — every authorization probe must originate from CPL3"
        ));
    }
    if granted_ret != AUTH_GRANTED {
        return Err(format!(
            "syscall_auth_ready granted_ret (0x{granted_ret:x}) must equal granted cap id 0x{AUTH_GRANTED:x}"
        ));
    }
    if denied_ret != EACCES {
        return Err(format!(
            "syscall_auth_ready denied_ret (0x{denied_ret:x}) must be -EACCES 0x{EACCES:x} — missing SegMapW must be denied"
        ));
    }
    if unknown_ret != ENOENT {
        return Err(format!(
            "syscall_auth_ready unknown_ret (0x{unknown_ret:x}) must be -ENOENT 0x{ENOENT:x} — unknown capability id must be rejected"
        ));
    }
    if receipt_granted != granted_ret {
        return Err(format!(
            "syscall_auth_ready receipt_granted (0x{receipt_granted:x}) must equal granted_ret 0x{granted_ret:x}"
        ));
    }
    if receipt_denied != denied_ret {
        return Err(format!(
            "syscall_auth_ready receipt_denied (0x{receipt_denied:x}) must equal denied_ret 0x{denied_ret:x}"
        ));
    }
    if receipt_unknown != unknown_ret {
        return Err(format!(
            "syscall_auth_ready receipt_unknown (0x{receipt_unknown:x}) must equal unknown_ret 0x{unknown_ret:x}"
        ));
    }
    Ok(())
}

/// 校验切片 L 的异常订阅证据：全部来自 CPL3；订阅成功（返回 source_id）、重复订阅
/// `EACCES`、kind 不符 `EINVAL`、取消成功、再次取消 `EACCES`，回执与返回值一致，
/// 且 Audit 计数符合成功状态修改的次数。
fn validate_exc_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "exc_ready event is missing its data field".to_owned())?;
    let mut cs_all = None;
    let mut sub_ret = None;
    let mut dup_ret = None;
    let mut kind_ret = None;
    let mut sub2_ret = None;
    let mut unsub_ret = None;
    let mut unsub2_ret = None;
    let mut receipt_sub = None;
    let mut receipt_dup = None;
    let mut receipt_kind = None;
    let mut receipt_sub2 = None;
    let mut receipt_unsub = None;
    let mut receipt_unsub2 = None;
    let mut audit_created = None;
    let mut audit_subscribed = None;
    let mut audit_unsubscribed = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "cs_all" => cs_all = Some(parse_decimal(value, "cs_all")?),
            "sub_ret" => sub_ret = Some(parse_addr(value)?),
            "dup_ret" => dup_ret = Some(parse_addr(value)?),
            "kind_ret" => kind_ret = Some(parse_addr(value)?),
            "sub2_ret" => sub2_ret = Some(parse_addr(value)?),
            "unsub_ret" => unsub_ret = Some(parse_addr(value)?),
            "unsub2_ret" => unsub2_ret = Some(parse_addr(value)?),
            "receipt_sub" => receipt_sub = Some(parse_addr(value)?),
            "receipt_dup" => receipt_dup = Some(parse_addr(value)?),
            "receipt_kind" => receipt_kind = Some(parse_addr(value)?),
            "receipt_sub2" => receipt_sub2 = Some(parse_addr(value)?),
            "receipt_unsub" => receipt_unsub = Some(parse_addr(value)?),
            "receipt_unsub2" => receipt_unsub2 = Some(parse_addr(value)?),
            "audit_created" => audit_created = Some(parse_decimal(value, "audit_created")?),
            "audit_subscribed" => {
                audit_subscribed = Some(parse_decimal(value, "audit_subscribed")?)
            }
            "audit_unsubscribed" => {
                audit_unsubscribed = Some(parse_decimal(value, "audit_unsubscribed")?)
            }
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let cs_all = cs_all.ok_or_else(|| "missing cs_all in exc_ready data".to_owned())?;
    let sub_ret = sub_ret.ok_or_else(|| "missing sub_ret in exc_ready data".to_owned())?;
    let dup_ret = dup_ret.ok_or_else(|| "missing dup_ret in exc_ready data".to_owned())?;
    let kind_ret = kind_ret.ok_or_else(|| "missing kind_ret in exc_ready data".to_owned())?;
    let sub2_ret = sub2_ret.ok_or_else(|| "missing sub2_ret in exc_ready data".to_owned())?;
    let unsub_ret = unsub_ret.ok_or_else(|| "missing unsub_ret in exc_ready data".to_owned())?;
    let unsub2_ret = unsub2_ret.ok_or_else(|| "missing unsub2_ret in exc_ready data".to_owned())?;
    let receipt_sub =
        receipt_sub.ok_or_else(|| "missing receipt_sub in exc_ready data".to_owned())?;
    let receipt_dup =
        receipt_dup.ok_or_else(|| "missing receipt_dup in exc_ready data".to_owned())?;
    let receipt_kind =
        receipt_kind.ok_or_else(|| "missing receipt_kind in exc_ready data".to_owned())?;
    let receipt_sub2 =
        receipt_sub2.ok_or_else(|| "missing receipt_sub2 in exc_ready data".to_owned())?;
    let receipt_unsub =
        receipt_unsub.ok_or_else(|| "missing receipt_unsub in exc_ready data".to_owned())?;
    let receipt_unsub2 =
        receipt_unsub2.ok_or_else(|| "missing receipt_unsub2 in exc_ready data".to_owned())?;
    let audit_created =
        audit_created.ok_or_else(|| "missing audit_created in exc_ready data".to_owned())?;
    let audit_subscribed =
        audit_subscribed.ok_or_else(|| "missing audit_subscribed in exc_ready data".to_owned())?;
    let audit_unsubscribed = audit_unsubscribed
        .ok_or_else(|| "missing audit_unsubscribed in exc_ready data".to_owned())?;

    if cs_all != 1 {
        return Err(format!(
            "exc_ready cs_all ({cs_all}) must be 1 — every subscription probe must originate from CPL3"
        ));
    }
    if sub_ret != EXC_PAGE_FAULT_SOURCE {
        return Err(format!(
            "exc_ready sub_ret (0x{sub_ret:x}) must equal page_fault source id 0x{EXC_PAGE_FAULT_SOURCE:x}"
        ));
    }
    if dup_ret != EACCES {
        return Err(format!(
            "exc_ready dup_ret (0x{dup_ret:x}) must be -EACCES 0x{EACCES:x} — duplicate subscription must be denied"
        ));
    }
    if kind_ret != EINVAL {
        return Err(format!(
            "exc_ready kind_ret (0x{kind_ret:x}) must be -EINVAL 0x{EINVAL:x} — kind mismatch must be rejected"
        ));
    }
    if sub2_ret != EXC_SYSCALL_TRAP_SOURCE {
        return Err(format!(
            "exc_ready sub2_ret (0x{sub2_ret:x}) must equal syscall_trap source id 0x{EXC_SYSCALL_TRAP_SOURCE:x}"
        ));
    }
    if unsub_ret != EXC_PAGE_FAULT_SOURCE {
        return Err(format!(
            "exc_ready unsub_ret (0x{unsub_ret:x}) must equal page_fault source id 0x{EXC_PAGE_FAULT_SOURCE:x}"
        ));
    }
    if unsub2_ret != EACCES {
        return Err(format!(
            "exc_ready unsub2_ret (0x{unsub2_ret:x}) must be -EACCES 0x{EACCES:x} — unsubscribing twice must be denied"
        ));
    }
    if receipt_sub != sub_ret
        || receipt_dup != dup_ret
        || receipt_kind != kind_ret
        || receipt_sub2 != sub2_ret
        || receipt_unsub != unsub_ret
        || receipt_unsub2 != unsub2_ret
    {
        return Err(format!(
            "exc_ready receipts must equal their syscall returns (sub 0x{receipt_sub:x}/0x{sub_ret:x}, dup 0x{receipt_dup:x}/0x{dup_ret:x}, kind 0x{receipt_kind:x}/0x{kind_ret:x}, sub2 0x{receipt_sub2:x}/0x{sub2_ret:x}, unsub 0x{receipt_unsub:x}/0x{unsub_ret:x}, unsub2 0x{receipt_unsub2:x}/0x{unsub2_ret:x})"
        ));
    }
    if audit_created != 2 || audit_subscribed != 3 || audit_unsubscribed != 1 {
        return Err(format!(
            "exc_ready audit counts must be created=2 subscribed=3 unsubscribed=1, got created={audit_created} subscribed={audit_subscribed} unsubscribed={audit_unsubscribed}"
        ));
    }
    Ok(())
}

/// 校验切片 L 的 pending 直投证据：一次投递、经 BlockedRecv 写 pending、目标回到 Ready、
/// payload 为 page_fault 通知编码，且追加了 `ExceptionRaised` Audit。
fn validate_exc_pending_data(data: Option<&str>) -> Result<(), String> {
    let data =
        data.ok_or_else(|| "exc_pending_ready event is missing its data field".to_owned())?;
    let mut delivered = None;
    let mut pending = None;
    let mut message_id = None;
    let mut payload = None;
    let mut task_ready_after = None;
    let mut audit_raised = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "delivered" => delivered = Some(parse_decimal(value, "delivered")?),
            "pending" => pending = Some(parse_decimal(value, "pending")?),
            "message_id" => message_id = Some(parse_decimal(value, "message_id")?),
            "payload" => payload = Some(parse_addr(value)?),
            "task_ready_after" => {
                task_ready_after = Some(parse_decimal(value, "task_ready_after")?)
            }
            "audit_raised" => audit_raised = Some(parse_decimal(value, "audit_raised")?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let delivered =
        delivered.ok_or_else(|| "missing delivered in exc_pending_ready data".to_owned())?;
    let pending = pending.ok_or_else(|| "missing pending in exc_pending_ready data".to_owned())?;
    let message_id =
        message_id.ok_or_else(|| "missing message_id in exc_pending_ready data".to_owned())?;
    let payload = payload.ok_or_else(|| "missing payload in exc_pending_ready data".to_owned())?;
    let task_ready_after = task_ready_after
        .ok_or_else(|| "missing task_ready_after in exc_pending_ready data".to_owned())?;
    let audit_raised =
        audit_raised.ok_or_else(|| "missing audit_raised in exc_pending_ready data".to_owned())?;

    if delivered != 1 {
        return Err(format!(
            "exc_pending_ready delivered ({delivered}) must be 1 — exactly one notification"
        ));
    }
    if pending != 1 {
        return Err(format!(
            "exc_pending_ready pending ({pending}) must be 1 — delivery must take the pending_return path"
        ));
    }
    if message_id == 0 {
        return Err("exc_pending_ready message_id must be non-zero".to_owned());
    }
    if payload != EXC_PAGE_FAULT_PAYLOAD {
        return Err(format!(
            "exc_pending_ready payload (0x{payload:x}) must equal page_fault notification 0x{EXC_PAGE_FAULT_PAYLOAD:x}"
        ));
    }
    if task_ready_after != 1 {
        return Err(format!(
            "exc_pending_ready task_ready_after ({task_ready_after}) must be 1 — BlockedRecv must be woken to Ready"
        ));
    }
    if audit_raised != 1 {
        return Err(format!(
            "exc_pending_ready audit_raised ({audit_raised}) must be 1 — one ExceptionRaised audit event"
        ));
    }
    Ok(())
}

/// 校验切片 M 的双任务协作证据：三次进入均来自 CPL3；task id 与 yield 往返次序正确
/// （task1→task2→task1）；task 2 的授权/订阅因非持有者被拒；所有回执与返回值一致。
fn validate_multitask_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "multitask_ready event is missing its data field".to_owned())?;
    let mut cs1_before = None;
    let mut cs1_after = None;
    let mut cs2 = None;
    let mut task1_before = None;
    let mut task1_after = None;
    let mut task2 = None;
    let mut switches = None;
    let mut task2_auth = None;
    let mut task2_sub = None;
    let mut receipt1_before = None;
    let mut receipt1_yield = None;
    let mut receipt1_after = None;
    let mut receipt2_id = None;
    let mut receipt2_auth = None;
    let mut receipt2_sub = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "cs1_before" => cs1_before = Some(parse_addr(value)?),
            "cs1_after" => cs1_after = Some(parse_addr(value)?),
            "cs2" => cs2 = Some(parse_addr(value)?),
            "task1_before" => task1_before = Some(parse_addr(value)?),
            "task1_after" => task1_after = Some(parse_addr(value)?),
            "task2" => task2 = Some(parse_addr(value)?),
            "switches" => switches = Some(parse_addr(value)?),
            "task2_auth" => task2_auth = Some(parse_addr(value)?),
            "task2_sub" => task2_sub = Some(parse_addr(value)?),
            "receipt1_before" => receipt1_before = Some(parse_addr(value)?),
            "receipt1_yield" => receipt1_yield = Some(parse_addr(value)?),
            "receipt1_after" => receipt1_after = Some(parse_addr(value)?),
            "receipt2_id" => receipt2_id = Some(parse_addr(value)?),
            "receipt2_auth" => receipt2_auth = Some(parse_addr(value)?),
            "receipt2_sub" => receipt2_sub = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let cs1_before =
        cs1_before.ok_or_else(|| "missing cs1_before in multitask_ready data".to_owned())?;
    let cs1_after =
        cs1_after.ok_or_else(|| "missing cs1_after in multitask_ready data".to_owned())?;
    let cs2 = cs2.ok_or_else(|| "missing cs2 in multitask_ready data".to_owned())?;
    let task1_before =
        task1_before.ok_or_else(|| "missing task1_before in multitask_ready data".to_owned())?;
    let task1_after =
        task1_after.ok_or_else(|| "missing task1_after in multitask_ready data".to_owned())?;
    let task2 = task2.ok_or_else(|| "missing task2 in multitask_ready data".to_owned())?;
    let switches = switches.ok_or_else(|| "missing switches in multitask_ready data".to_owned())?;
    let task2_auth =
        task2_auth.ok_or_else(|| "missing task2_auth in multitask_ready data".to_owned())?;
    let task2_sub =
        task2_sub.ok_or_else(|| "missing task2_sub in multitask_ready data".to_owned())?;
    let receipt1_before = receipt1_before
        .ok_or_else(|| "missing receipt1_before in multitask_ready data".to_owned())?;
    let receipt1_yield = receipt1_yield
        .ok_or_else(|| "missing receipt1_yield in multitask_ready data".to_owned())?;
    let receipt1_after = receipt1_after
        .ok_or_else(|| "missing receipt1_after in multitask_ready data".to_owned())?;
    let receipt2_id =
        receipt2_id.ok_or_else(|| "missing receipt2_id in multitask_ready data".to_owned())?;
    let receipt2_auth =
        receipt2_auth.ok_or_else(|| "missing receipt2_auth in multitask_ready data".to_owned())?;
    let receipt2_sub =
        receipt2_sub.ok_or_else(|| "missing receipt2_sub in multitask_ready data".to_owned())?;

    if cs1_before != USER_CS || cs1_after != USER_CS || cs2 != USER_CS {
        return Err(format!(
            "multitask_ready cs values (0x{cs1_before:x}/0x{cs1_after:x}/0x{cs2:x}) must all be 0x{USER_CS:x} — every entry must originate from CPL3"
        ));
    }
    if task1_before != MT_TASK1 || task1_after != MT_TASK1 || task2 != MT_TASK2 {
        return Err(format!(
            "multitask_ready task ids (task1_before=0x{task1_before:x} task1_after=0x{task1_after:x} task2=0x{task2:x}) must be {MT_TASK1}/{MT_TASK1}/{MT_TASK2}"
        ));
    }
    if switches != MT_SWITCHES {
        return Err(format!(
            "multitask_ready switches (0x{switches:x}) must be {MT_SWITCHES} — task1→task2→task1"
        ));
    }
    if task2_auth != EACCES || task2_sub != EACCES {
        return Err(format!(
            "multitask_ready task2_auth (0x{task2_auth:x}) / task2_sub (0x{task2_sub:x}) must both be -EACCES 0x{EACCES:x} — task 2 must not hold task 1's capabilities"
        ));
    }
    if receipt1_before != task1_before
        || receipt1_yield != 0
        || receipt1_after != task1_after
        || receipt2_id != task2
        || receipt2_auth != task2_auth
        || receipt2_sub != task2_sub
    {
        return Err(format!(
            "multitask_ready receipts must equal their returns (r1_before=0x{receipt1_before:x}/0x{task1_before:x}, r1_yield=0x{receipt1_yield:x}/0x0, r1_after=0x{receipt1_after:x}/0x{task1_after:x}, r2_id=0x{receipt2_id:x}/0x{task2:x}, r2_auth=0x{receipt2_auth:x}/0x{task2_auth:x}, r2_sub=0x{receipt2_sub:x}/0x{task2_sub:x})"
        ));
    }
    Ok(())
}

/// 校验切片 N 的跨任务阻塞 raise/wait 证据：wait 与 trap 均来自 CPL3；一次 raise 投递、
/// 经 pending 直投唤醒 task 1；payload 为 syscall_trap 通知编码；阻塞一次；回执一致。
fn validate_handle_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "handle_ready event is missing its data field".to_owned())?;
    let mut index_bits = None;
    let mut generation_bits = None;
    let mut image = None;
    let mut namespace = None;
    let mut task = None;
    let mut page_fault = None;
    let mut syscall_trap = None;
    let mut malformed_rejected = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "index_bits" => index_bits = Some(parse_decimal(value, "index_bits")?),
            "generation_bits" => generation_bits = Some(parse_decimal(value, "generation_bits")?),
            "image" => image = Some(parse_addr(value)?),
            "namespace" => namespace = Some(parse_addr(value)?),
            "task" => task = Some(parse_addr(value)?),
            "page_fault" => page_fault = Some(parse_addr(value)?),
            "syscall_trap" => syscall_trap = Some(parse_addr(value)?),
            "malformed_rejected" => {
                malformed_rejected = Some(parse_decimal(value, "malformed_rejected")?)
            }
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let index_bits =
        index_bits.ok_or_else(|| "missing index_bits in handle_ready data".to_owned())?;
    let generation_bits =
        generation_bits.ok_or_else(|| "missing generation_bits in handle_ready data".to_owned())?;
    let image = image.ok_or_else(|| "missing image in handle_ready data".to_owned())?;
    let namespace = namespace.ok_or_else(|| "missing namespace in handle_ready data".to_owned())?;
    let task = task.ok_or_else(|| "missing task in handle_ready data".to_owned())?;
    let page_fault =
        page_fault.ok_or_else(|| "missing page_fault in handle_ready data".to_owned())?;
    let syscall_trap =
        syscall_trap.ok_or_else(|| "missing syscall_trap in handle_ready data".to_owned())?;
    let malformed_rejected = malformed_rejected
        .ok_or_else(|| "missing malformed_rejected in handle_ready data".to_owned())?;

    if index_bits != 32 || generation_bits != 32 {
        return Err(format!(
            "handle_ready must use 32-bit index + 32-bit generation, got {index_bits}/{generation_bits}"
        ));
    }
    if image != IMAGE_CAP_HANDLE
        || namespace != NAMESPACE_CAP_HANDLE
        || task != TASK_CAP_HANDLE
        || page_fault != EXC_PAGE_FAULT_HANDLE
        || syscall_trap != EXC_SYSCALL_TRAP_HANDLE
    {
        return Err(format!(
            "handle_ready initial handles mismatch: image=0x{image:x}, namespace=0x{namespace:x}, task=0x{task:x}, page_fault=0x{page_fault:x}, syscall_trap=0x{syscall_trap:x}"
        ));
    }
    if malformed_rejected != 1 {
        return Err(
            "handle_ready malformed_rejected must be 1 — raw integers (generation 0) must not decode"
                .to_owned(),
        );
    }
    Ok(())
}

fn validate_cap_lifecycle_data(data: Option<&str>) -> Result<(), String> {
    let data =
        data.ok_or_else(|| "cap_lifecycle_ready event is missing its data field".to_owned())?;
    let mut derive = None;
    let mut escalate = None;
    let mut drop = None;
    let mut drop_again = None;
    let mut reuse = None;
    let mut stale = None;
    let mut grant = None;
    let mut drop_new = None;
    let mut structural = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "derive" => derive = Some(parse_addr(value)?),
            "escalate" => escalate = Some(parse_addr(value)?),
            "drop" => drop = Some(parse_addr(value)?),
            "drop_again" => drop_again = Some(parse_addr(value)?),
            "reuse" => reuse = Some(parse_addr(value)?),
            "stale" => stale = Some(parse_addr(value)?),
            "grant" => grant = Some(parse_addr(value)?),
            "drop_new" => drop_new = Some(parse_addr(value)?),
            "structural" => structural = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let derive = derive.ok_or_else(|| "missing derive in cap_lifecycle_ready data".to_owned())?;
    let escalate =
        escalate.ok_or_else(|| "missing escalate in cap_lifecycle_ready data".to_owned())?;
    let drop = drop.ok_or_else(|| "missing drop in cap_lifecycle_ready data".to_owned())?;
    let drop_again =
        drop_again.ok_or_else(|| "missing drop_again in cap_lifecycle_ready data".to_owned())?;
    let reuse = reuse.ok_or_else(|| "missing reuse in cap_lifecycle_ready data".to_owned())?;
    let stale = stale.ok_or_else(|| "missing stale in cap_lifecycle_ready data".to_owned())?;
    let grant = grant.ok_or_else(|| "missing grant in cap_lifecycle_ready data".to_owned())?;
    let drop_new =
        drop_new.ok_or_else(|| "missing drop_new in cap_lifecycle_ready data".to_owned())?;
    let structural =
        structural.ok_or_else(|| "missing structural in cap_lifecycle_ready data".to_owned())?;

    if derive != CAP_CHILD1_HANDLE {
        return Err(format!(
            "cap_lifecycle_ready derive (0x{derive:x}) must be the first derived handle 0x{CAP_CHILD1_HANDLE:x}"
        ));
    }
    if escalate != EACCES {
        return Err(format!(
            "cap_lifecycle_ready escalate (0x{escalate:x}) must be EACCES — rights escalation must be denied"
        ));
    }
    if drop != 0 {
        return Err(format!(
            "cap_lifecycle_ready drop (0x{drop:x}) must be 0 — dropping a held capability succeeds"
        ));
    }
    if drop_again != ENOENT {
        return Err(format!(
            "cap_lifecycle_ready drop_again (0x{drop_again:x}) must be ENOENT — a dropped handle is gone"
        ));
    }
    if reuse != CAP_CHILD2_HANDLE || reuse == derive {
        return Err(format!(
            "cap_lifecycle_ready reuse (0x{reuse:x}) must be 0x{CAP_CHILD2_HANDLE:x} (same slot, next generation)"
        ));
    }
    if stale != ENOENT {
        return Err(format!(
            "cap_lifecycle_ready stale (0x{stale:x}) must be ENOENT — S2-X4: slot reuse must not re-grant the old handle"
        ));
    }
    if grant != CAP_CHILD2_HANDLE {
        return Err(format!(
            "cap_lifecycle_ready grant (0x{grant:x}) must be the live handle 0x{CAP_CHILD2_HANDLE:x}"
        ));
    }
    if drop_new != 0 {
        return Err(format!(
            "cap_lifecycle_ready drop_new (0x{drop_new:x}) must be 0 — cleanup drop succeeds"
        ));
    }
    if structural != EACCES {
        return Err(format!(
            "cap_lifecycle_ready structural (0x{structural:x}) must be EACCES — the structural root namespace cap is protected"
        ));
    }
    Ok(())
}

fn validate_memseg_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "memseg_ready event is missing its data field".to_owned())?;
    let mut create = None;
    let mut map = None;
    let mut zero = None;
    let mut write = None;
    let mut unmap = None;
    let mut destroy = None;
    let mut stale = None;
    let mut create2 = None;
    let mut map2 = None;
    let mut zero2 = None;
    let mut unmap2 = None;
    let mut destroy2 = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "create" => create = Some(parse_addr(value)?),
            "map" => map = Some(parse_addr(value)?),
            "zero" => zero = Some(parse_addr(value)?),
            "write" => write = Some(parse_addr(value)?),
            "unmap" => unmap = Some(parse_addr(value)?),
            "destroy" => destroy = Some(parse_addr(value)?),
            "stale" => stale = Some(parse_addr(value)?),
            "create2" => create2 = Some(parse_addr(value)?),
            "map2" => map2 = Some(parse_addr(value)?),
            "zero2" => zero2 = Some(parse_addr(value)?),
            "unmap2" => unmap2 = Some(parse_addr(value)?),
            "destroy2" => destroy2 = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let create = create.ok_or_else(|| "missing create in memseg_ready data".to_owned())?;
    let map = map.ok_or_else(|| "missing map in memseg_ready data".to_owned())?;
    let zero = zero.ok_or_else(|| "missing zero in memseg_ready data".to_owned())?;
    let write = write.ok_or_else(|| "missing write in memseg_ready data".to_owned())?;
    let unmap = unmap.ok_or_else(|| "missing unmap in memseg_ready data".to_owned())?;
    let destroy = destroy.ok_or_else(|| "missing destroy in memseg_ready data".to_owned())?;
    let stale = stale.ok_or_else(|| "missing stale in memseg_ready data".to_owned())?;
    let create2 = create2.ok_or_else(|| "missing create2 in memseg_ready data".to_owned())?;
    let map2 = map2.ok_or_else(|| "missing map2 in memseg_ready data".to_owned())?;
    let zero2 = zero2.ok_or_else(|| "missing zero2 in memseg_ready data".to_owned())?;
    let unmap2 = unmap2.ok_or_else(|| "missing unmap2 in memseg_ready data".to_owned())?;
    let destroy2 = destroy2.ok_or_else(|| "missing destroy2 in memseg_ready data".to_owned())?;

    if create != MEMSEG_CAP1_HANDLE {
        return Err(format!(
            "memseg_ready create (0x{create:x}) must be the first segment cap 0x{MEMSEG_CAP1_HANDLE:x}"
        ));
    }
    if map != 0 || unmap != 0 || destroy != 0 || map2 != 0 || unmap2 != 0 || destroy2 != 0 {
        return Err(format!(
            "memseg_ready map/unmap/destroy must all succeed (0), got map=0x{map:x}, unmap=0x{unmap:x}, destroy=0x{destroy:x}, map2=0x{map2:x}, unmap2=0x{unmap2:x}, destroy2=0x{destroy2:x}"
        ));
    }
    if zero != 0 {
        return Err(format!(
            "memseg_ready zero (0x{zero:x}) must be 0 — S2-X3: frames must be zeroed before first visibility"
        ));
    }
    if write != MEMSEG_PATTERN {
        return Err(format!(
            "memseg_ready write (0x{write:x}) must read back 0x{MEMSEG_PATTERN:x} — the mapping must be a real writable PTE at 0x{USER_SEG_VA:x}"
        ));
    }
    if stale != EACCES {
        return Err(format!(
            "memseg_ready stale (0x{stale:x}) must be EACCES — mapping after memseg_destroy must be denied"
        ));
    }
    if create2 != MEMSEG_CAP2_HANDLE {
        return Err(format!(
            "memseg_ready create2 (0x{create2:x}) must be a fresh segment cap 0x{MEMSEG_CAP2_HANDLE:x}"
        ));
    }
    if zero2 != 0 {
        return Err(format!(
            "memseg_ready zero2 (0x{zero2:x}) must be 0 — S2-X3: a recycled frame must be zeroed before reuse"
        ));
    }
    Ok(())
}

/// 切片 R：校验平台 bring-up 13 区域对象化见证（真实 Segment 对象 + 内核持有的根 Capability）。
fn validate_platform_segments_data(data: Option<&str>) -> Result<(), String> {
    let data =
        data.ok_or_else(|| "platform_segments_ready event is missing its data field".to_owned())?;
    let mut segments = None;
    let mut first_id = None;
    let mut last_id = None;
    let mut roots = None;
    let mut user_roots = None;
    let mut kernel_roots = None;
    let mut holders: Option<Vec<u64>> = None;
    let mut task_caps = None;
    let mut task_holders: Option<Vec<u64>> = None;
    let mut mapped = None;
    let mut direct = None;
    let mut direct_ids: Option<Vec<u64>> = None;
    let mut vspace = None;
    let mut slot_base = None;
    let mut task1_cap = None;
    let mut task2_cap = None;
    let mut caps: Option<Vec<u64>> = None;
    let mut frames: Option<Vec<u64>> = None;
    let mut unified = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "segments" => segments = Some(parse_decimal(value, "segments")?),
            "first_id" => first_id = Some(parse_decimal(value, "first_id")?),
            "last_id" => last_id = Some(parse_decimal(value, "last_id")?),
            "roots" => roots = Some(parse_decimal(value, "roots")?),
            "user_roots" => user_roots = Some(parse_decimal(value, "user_roots")?),
            "kernel_roots" => kernel_roots = Some(parse_decimal(value, "kernel_roots")?),
            "holders" => holders = Some(parse_decimal_list(value, "holders")?),
            "task_caps" => task_caps = Some(parse_decimal(value, "task_caps")?),
            "task_holders" => task_holders = Some(parse_decimal_list(value, "task_holders")?),
            "mapped" => mapped = Some(parse_decimal(value, "mapped")?),
            "direct" => direct = Some(parse_decimal(value, "direct")?),
            "direct_ids" => direct_ids = Some(parse_decimal_list(value, "direct_ids")?),
            "vspace" => vspace = Some(parse_decimal(value, "vspace")?),
            "slot_base" => slot_base = Some(parse_decimal(value, "slot_base")?),
            "task1_cap" => task1_cap = Some(parse_addr(value)?),
            "task2_cap" => task2_cap = Some(parse_addr(value)?),
            "caps" => caps = Some(parse_addr_list(value, "caps")?),
            "frames" => frames = Some(parse_decimal_list(value, "frames")?),
            "unified" => unified = Some(parse_decimal(value, "unified")?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let missing = |key: &str| format!("missing {key} in platform_segments_ready data");
    let segments = segments.ok_or_else(|| missing("segments"))?;
    let first_id = first_id.ok_or_else(|| missing("first_id"))?;
    let last_id = last_id.ok_or_else(|| missing("last_id"))?;
    let roots = roots.ok_or_else(|| missing("roots"))?;
    let user_roots = user_roots.ok_or_else(|| missing("user_roots"))?;
    let kernel_roots = kernel_roots.ok_or_else(|| missing("kernel_roots"))?;
    let holders = holders.ok_or_else(|| missing("holders"))?;
    let task_caps = task_caps.ok_or_else(|| missing("task_caps"))?;
    let task_holders = task_holders.ok_or_else(|| missing("task_holders"))?;
    let mapped = mapped.ok_or_else(|| missing("mapped"))?;
    let direct = direct.ok_or_else(|| missing("direct"))?;
    let direct_ids = direct_ids.ok_or_else(|| missing("direct_ids"))?;
    let vspace = vspace.ok_or_else(|| missing("vspace"))?;
    let slot_base = slot_base.ok_or_else(|| missing("slot_base"))?;
    let task1_cap = task1_cap.ok_or_else(|| missing("task1_cap"))?;
    let task2_cap = task2_cap.ok_or_else(|| missing("task2_cap"))?;
    let caps = caps.ok_or_else(|| missing("caps"))?;
    let frames = frames.ok_or_else(|| missing("frames"))?;
    let unified = unified.ok_or_else(|| missing("unified"))?;

    for (key, actual, expected) in [
        ("segments", segments, PLATFORM_SEGMENT_COUNT as u64),
        ("first_id", first_id, PLATFORM_SEGMENT_FIRST_ID),
        ("last_id", last_id, PLATFORM_SEGMENT_LAST_ID),
        ("roots", roots, PLATFORM_SEGMENT_COUNT as u64),
        ("user_roots", user_roots, PLATFORM_USER_ROOTS),
        ("kernel_roots", kernel_roots, PLATFORM_KERNEL_ROOTS),
        ("task_caps", task_caps, 2),
        ("mapped", mapped, PLATFORM_MAPPED),
        ("direct", direct, PLATFORM_DIRECT),
        ("vspace", vspace, PLATFORM_VSPACE_ENTRIES),
        ("slot_base", slot_base, PLATFORM_SEGMENT_CAP_SLOT_BASE),
    ] {
        if actual != expected {
            return Err(format!(
                "platform_segments_ready {key} is {actual}, expected {expected}"
            ));
        }
    }
    if unified != 1 {
        return Err(
            "platform_segments_ready unified must be 1 — frame labels must equal segment ids"
                .to_owned(),
        );
    }
    if direct_ids.as_slice() != [2, 3] {
        return Err(format!(
            "platform_segments_ready direct_ids is {direct_ids:?}, expected [2, 3] (VSPACE, COW)"
        ));
    }
    // SPEC-066：逐槽属主——11 条用户可见区域按属主移交，VSPACE/COW 仍归内核（0）。
    if holders.as_slice() != PLATFORM_SEGMENT_HOLDERS {
        return Err(format!(
            "platform_segments_ready holders is {holders:?}, expected {PLATFORM_SEGMENT_HOLDERS:?}"
        ));
    }
    if task_holders.as_slice() != PLATFORM_TASK_HOLDERS {
        return Err(format!(
            "platform_segments_ready task_holders is {task_holders:?}, expected {PLATFORM_TASK_HOLDERS:?}"
        ));
    }
    if task1_cap != PLATFORM_TASK1_CAP_HANDLE || task2_cap != PLATFORM_TASK2_CAP_HANDLE {
        return Err(format!(
            "platform_segments_ready task caps are 0x{task1_cap:x}/0x{task2_cap:x}, expected 0x{PLATFORM_TASK1_CAP_HANDLE:x}/0x{PLATFORM_TASK2_CAP_HANDLE:x}"
        ));
    }
    if caps.len() != PLATFORM_SEGMENT_COUNT {
        return Err(format!(
            "platform_segments_ready caps has {} entries, expected {PLATFORM_SEGMENT_COUNT}",
            caps.len()
        ));
    }
    for (index, cap) in caps.iter().enumerate() {
        let expected = (1u64 << 32) | (PLATFORM_SEGMENT_CAP_SLOT_BASE + index as u64);
        if *cap != expected {
            return Err(format!(
                "platform_segments_ready caps[{index}] is 0x{cap:x}, expected 0x{expected:x} (slot {})",
                PLATFORM_SEGMENT_CAP_SLOT_BASE + index as u64
            ));
        }
    }
    if frames.as_slice() != PLATFORM_FRAME_COUNTS {
        return Err(format!(
            "platform_segments_ready frames is {frames:?}, expected {PLATFORM_FRAME_COUNTS:?}"
        ));
    }
    Ok(())
}

/// 切片 S：校验 IPC 用户化与两任务共享 Segment 见证（S2-D5/S2-X2）。
fn validate_ipc_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "ipc_ready event is missing its data field".to_owned())?;
    let mut create = None;
    let mut map1 = None;
    let mut send1 = None;
    let mut send2 = None;
    let mut recv1 = None;
    let mut cap0 = None;
    let mut magic = None;
    let mut map2 = None;
    let mut cross = None;
    let mut send_denied = None;
    let mut recv2 = None;
    let mut cap_count = None;
    let mut queued = None;
    let mut direct = None;
    let mut blocks = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "create" => create = Some(parse_addr(value)?),
            "map1" => map1 = Some(parse_addr(value)?),
            "send1" => send1 = Some(parse_addr(value)?),
            "send2" => send2 = Some(parse_addr(value)?),
            "recv1" => recv1 = Some(parse_addr(value)?),
            "cap0" => cap0 = Some(parse_addr(value)?),
            "magic" => magic = Some(parse_addr(value)?),
            "map2" => map2 = Some(parse_addr(value)?),
            "cross" => cross = Some(parse_addr(value)?),
            "send_denied" => send_denied = Some(parse_addr(value)?),
            "recv2" => recv2 = Some(parse_addr(value)?),
            "cap_count" => cap_count = Some(parse_decimal(value, "cap_count")?),
            "queued" => queued = Some(parse_decimal(value, "queued")?),
            "direct" => direct = Some(parse_decimal(value, "direct")?),
            "blocks" => blocks = Some(parse_decimal(value, "blocks")?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let missing = |key: &str| format!("missing {key} in ipc_ready data");
    let create = create.ok_or_else(|| missing("create"))?;
    let map1 = map1.ok_or_else(|| missing("map1"))?;
    let send1 = send1.ok_or_else(|| missing("send1"))?;
    let send2 = send2.ok_or_else(|| missing("send2"))?;
    let recv1 = recv1.ok_or_else(|| missing("recv1"))?;
    let cap0 = cap0.ok_or_else(|| missing("cap0"))?;
    let magic = magic.ok_or_else(|| missing("magic"))?;
    let map2 = map2.ok_or_else(|| missing("map2"))?;
    let cross = cross.ok_or_else(|| missing("cross"))?;
    let send_denied = send_denied.ok_or_else(|| missing("send_denied"))?;
    let recv2 = recv2.ok_or_else(|| missing("recv2"))?;
    let cap_count = cap_count.ok_or_else(|| missing("cap_count"))?;
    let queued = queued.ok_or_else(|| missing("queued"))?;
    let direct = direct.ok_or_else(|| missing("direct"))?;
    let blocks = blocks.ok_or_else(|| missing("blocks"))?;

    if create != IPC_SHARED_SEG_CAP {
        return Err(format!(
            "ipc_ready create (0x{create:x}) must be the shared segment cap 0x{IPC_SHARED_SEG_CAP:x}"
        ));
    }
    if map1 != 0 || send1 != 0 || send2 != 0 || map2 != 0 {
        return Err(format!(
            "ipc_ready map/send must all succeed (0), got map1=0x{map1:x}, send1=0x{send1:x}, send2=0x{send2:x}, map2=0x{map2:x}"
        ));
    }
    if recv1 != IPC_SHARED_MAGIC || magic != IPC_SHARED_MAGIC {
        return Err(format!(
            "ipc_ready recv1/magic (0x{recv1:x}/0x{magic:x}) must both equal the shared magic 0x{IPC_SHARED_MAGIC:x} — task 2 must read back the same physical frame"
        ));
    }
    if cap0 != IPC_SHARED_SEG_CAP {
        return Err(format!(
            "ipc_ready cap0 (0x{cap0:x}) must be the segment cap 0x{IPC_SHARED_SEG_CAP:x} received through the message"
        ));
    }
    if cap_count != 1 {
        return Err(format!(
            "ipc_ready cap_count is {cap_count}, expected 1 (the shared segment cap travelled with the message)"
        ));
    }
    if cross != EACCES {
        return Err(format!(
            "ipc_ready cross (0x{cross:x}) must be EACCES — mapping into another task's vspace without holding its TaskManageVSpace cap must be denied"
        ));
    }
    if send_denied != EACCES {
        return Err(format!(
            "ipc_ready send_denied (0x{send_denied:x}) must be EACCES — sending without an authorized cap on the receiver must be denied"
        ));
    }
    if recv2 != IPC_DIRECT_PAYLOAD {
        return Err(format!(
            "ipc_ready recv2 (0x{recv2:x}) must be the direct-delivered payload 0x{IPC_DIRECT_PAYLOAD:x}"
        ));
    }
    if queued != 8 || direct != 9 || blocks != 12 {
        return Err(format!(
            "ipc_ready queued/direct/blocks are {queued}/{direct}/{blocks}, expected 8/9/12 — slice S's pair, S2-D4's new-task message plus the end-of-run wake of task 2, slice V's self-destroying task 4 report plus the re-wake of task 2 before the preemption phase, SPEC-087's client Hello (queued) / client reply + re-wake of task 2 (direct), SPEC-088's service id report + forwarded request (queued) / no-cap wake of the service (direct) plus the service/inits blocking recvs, SPEC-090's second no-cap prepare route to the service (direct) plus the extra service/init blocking recvs, and SPEC-092's second endpoint self-report + hello forward (queued) plus its extra blocking recvs"
        ));
    }
    Ok(())
}

/// S2-D4：校验 `task_ready` —— `task_create`（完整 §8.1）/`task_destroy`（§8.2）三个用户原语
/// 的成功与失败路径。
///
/// 钉住的是**事实**而非推断：`create` 必须是 `task_create` 返回的 manager cap 句柄；
/// `recv` 必须等于内核选定的子任务 id（子任务经 `SYS_TASK_ID` 自报后回发）；`destroy` 成功而
/// `again`/`map_dead`/`deny` 均被拒；`root_cleared`/`tasks`/`caps` 由内核在事件发射时读取，
/// 证明目标已从任务表与调度登记移除、manager cap 已不再可授权。
fn validate_task_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "task_ready event is missing its data field".to_owned())?;
    let mut stack_seg = None;
    let mut create = None;
    let mut bad_entry = None;
    let mut no_x = None;
    let mut stack_map = None;
    let mut recv = None;
    let mut deny = None;
    let mut destroy = None;
    let mut again = None;
    let mut map_dead = None;
    let mut map_own = None;
    let mut map_value = None;
    let mut map_other = None;
    let mut task4_cap = None;
    let mut task4_stack = None;
    let mut task4_recv = None;
    let mut task4_map_dead = None;
    let mut child = None;
    let mut tasks = None;
    let mut caps = None;
    let mut root_cleared = None;
    let mut grandchild = None;
    let mut tasks_after = None;
    let mut caps_after = None;
    let mut root4_cleared = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "stack_seg" => stack_seg = Some(parse_addr(value)?),
            "create" => create = Some(parse_addr(value)?),
            "bad_entry" => bad_entry = Some(parse_addr(value)?),
            "no_x" => no_x = Some(parse_addr(value)?),
            "stack_map" => stack_map = Some(parse_addr(value)?),
            "recv" => recv = Some(parse_addr(value)?),
            "deny" => deny = Some(parse_addr(value)?),
            "destroy" => destroy = Some(parse_addr(value)?),
            "again" => again = Some(parse_addr(value)?),
            "map_dead" => map_dead = Some(parse_addr(value)?),
            "map_own" => map_own = Some(parse_addr(value)?),
            "map_value" => map_value = Some(parse_addr(value)?),
            "map_other" => map_other = Some(parse_addr(value)?),
            "task4_cap" => task4_cap = Some(parse_addr(value)?),
            "task4_stack" => task4_stack = Some(parse_addr(value)?),
            "task4_recv" => task4_recv = Some(parse_addr(value)?),
            "task4_map_dead" => task4_map_dead = Some(parse_addr(value)?),
            "child" => child = Some(parse_count(value)?),
            "tasks" => tasks = Some(parse_count(value)?),
            "caps" => caps = Some(parse_count(value)?),
            "root_cleared" => root_cleared = Some(parse_count(value)?),
            "grandchild" => grandchild = Some(parse_count(value)?),
            "tasks_after" => tasks_after = Some(parse_count(value)?),
            "caps_after" => caps_after = Some(parse_count(value)?),
            "root4_cleared" => root4_cleared = Some(parse_count(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let missing = |key: &str| format!("missing {key} in task_ready data");
    let stack_seg = stack_seg.ok_or_else(|| missing("stack_seg"))?;
    let create = create.ok_or_else(|| missing("create"))?;
    let bad_entry = bad_entry.ok_or_else(|| missing("bad_entry"))?;
    let no_x = no_x.ok_or_else(|| missing("no_x"))?;
    let stack_map = stack_map.ok_or_else(|| missing("stack_map"))?;
    let recv = recv.ok_or_else(|| missing("recv"))?;
    let deny = deny.ok_or_else(|| missing("deny"))?;
    let destroy = destroy.ok_or_else(|| missing("destroy"))?;
    let again = again.ok_or_else(|| missing("again"))?;
    let map_dead = map_dead.ok_or_else(|| missing("map_dead"))?;
    let map_own = map_own.ok_or_else(|| missing("map_own"))?;
    let map_value = map_value.ok_or_else(|| missing("map_value"))?;
    let map_other = map_other.ok_or_else(|| missing("map_other"))?;
    let task4_cap = task4_cap.ok_or_else(|| missing("task4_cap"))?;
    let task4_stack = task4_stack.ok_or_else(|| missing("task4_stack"))?;
    let task4_recv = task4_recv.ok_or_else(|| missing("task4_recv"))?;
    let task4_map_dead = task4_map_dead.ok_or_else(|| missing("task4_map_dead"))?;
    let child = child.ok_or_else(|| missing("child"))?;
    let tasks = tasks.ok_or_else(|| missing("tasks"))?;
    let caps = caps.ok_or_else(|| missing("caps"))?;
    let root_cleared = root_cleared.ok_or_else(|| missing("root_cleared"))?;
    let grandchild = grandchild.ok_or_else(|| missing("grandchild"))?;
    let tasks_after = tasks_after.ok_or_else(|| missing("tasks_after"))?;
    let caps_after = caps_after.ok_or_else(|| missing("caps_after"))?;
    let root4_cleared = root4_cleared.ok_or_else(|| missing("root4_cleared"))?;

    if child != TASK_CREATE_CHILD_ID {
        return Err(format!(
            "task_ready child is {child}, expected {TASK_CREATE_CHILD_ID} — task_create must pick the smallest free id after init and task 2"
        ));
    }
    if stack_seg == 0 || stack_seg == create {
        return Err(format!(
            "task_ready stack_seg (0x{stack_seg:x}) must be a distinct live handle minted by the new task segment"
        ));
    }
    if create != TASK_MANAGER_CAP {
        return Err(format!(
            "task_ready create (0x{create:x}) must be the manager cap 0x{TASK_MANAGER_CAP:x} returned by §8.1 step 3"
        ));
    }
    if bad_entry != EINVAL {
        return Err(format!(
            "task_ready bad_entry (0x{bad_entry:x}) must be EINVAL (-22) — an entry outside [load_base, load_base + code_pages*4KiB) must be rejected before any state change"
        ));
    }
    if no_x != EACCES {
        return Err(format!(
            "task_ready no_x (0x{no_x:x}) must be EACCES (-13) — a code cap without SegMapX must be rejected"
        ));
    }
    if stack_map != 0 {
        return Err(format!(
            "task_ready stack_map (0x{stack_map:x}) must be 0 — the caller must map the new task's stack through the manager cap after create"
        ));
    }
    if recv != child as u64 {
        return Err(format!(
            "task_ready recv ({recv}) must equal child ({child}) — the child reports its own SYS_TASK_ID back through an authorized message"
        ));
    }
    if deny != EACCES {
        return Err(format!(
            "task_ready deny (0x{deny:x}) must be EACCES — destroying a task without a TaskDestroy cap must be denied"
        ));
    }
    if destroy != 0 {
        return Err(format!(
            "task_ready destroy (0x{destroy:x}) must be 0 — §8.2 task_destroy on the created child must succeed"
        ));
    }
    if again != EACCES {
        return Err(format!(
            "task_ready again (0x{again:x}) must be EACCES — the manager cap's target is destroyed, so a second destroy must be denied (stale token, not silently successful)"
        ));
    }
    if map_dead != ENOENT {
        return Err(format!(
            "task_ready map_dead (0x{map_dead:x}) must be ENOENT — mapping into a destroyed task's address space must be denied"
        ));
    }
    // SPEC-066：自持平台段 cap 的映射——用户可达的判决性证据。
    if map_own != 0 {
        return Err(format!(
            "task_ready map_own (0x{map_own:x}) must be 0 — a task holding its own platform segment cap must pass memseg_map's holder_of(cap) == caller check"
        ));
    }
    if map_value != TASK_MAP_VALUE {
        return Err(format!(
            "task_ready map_value (0x{map_value:x}) must be 0x{TASK_MAP_VALUE:x} — the fresh mapping must read back the segment frame's COW pattern"
        ));
    }
    if map_other != EACCES {
        return Err(format!(
            "task_ready map_other (0x{map_other:x}) must be EACCES — mapping a segment owned by another task must be denied"
        ));
    }
    // SPEC-070/072/073：存活 Task = init + task 2 + **`fault_ready` 的保留槽位任务 7**。
    // 回收见证的任务 6 与 `resume_ready` 生命周期收尾的保留槽位任务 5 都已由内核路径销毁
    // （Dead 记录保留但不计入存活）。Faulted 是"存活"（非 Dead）的常规状态，故任务 7 计入。
    // SPEC-087：再加 **存活的 Rust 客户端**（id 8，握手后阻塞等待）。
    // SPEC-088：再加 **Rust 服务端点**（id 9，自报 id 后阻塞等待协商请求）。
    // SPEC-092：再加**第二个独立服务端点**（id 10，各自独立任务与独立根）。
    if tasks != 6 {
        return Err(format!(
            "task_ready tasks is {tasks}, expected 6 (init + task 2 + the reserved Faulted witness task + the surviving Rust client + two independent Rust service endpoints) — the created child must be gone from the task table"
        ));
    }
    if root_cleared != 1 {
        return Err(format!(
            "task_ready root_cleared is {root_cleared}, expected 1 — task_destroy must clear the destroyed task's scheduler root registration"
        ));
    }
    // SPEC-067：task 4 承载自毁——自报 id（id 不复用）、自毁后不可映射、调度登记已清。
    if task4_cap == 0 || task4_cap == create {
        return Err(format!(
            "task_ready task4_cap (0x{task4_cap:x}) must be a distinct live handle returned by the second task_create"
        ));
    }
    if task4_stack != 0 {
        return Err(format!(
            "task_ready task4_stack (0x{task4_stack:x}) must be 0 — the caller must map the stack into task 4's root"
        ));
    }
    if grandchild != TASK_SELF_DESTROY_ID {
        return Err(format!(
            "task_ready grandchild is {grandchild}, expected {TASK_SELF_DESTROY_ID} — ids are not reused after destroy"
        ));
    }
    if task4_recv != TASK_SELF_DESTROY_ID as u64 {
        return Err(format!(
            "task_ready task4_recv ({task4_recv}) must equal grandchild ({TASK_SELF_DESTROY_ID}) — task 4 reports its own SYS_TASK_ID before destroying itself"
        ));
    }
    if task4_map_dead != ENOENT {
        return Err(format!(
            "task_ready task4_map_dead (0x{task4_map_dead:x}) must be ENOENT — task 4 destroyed itself, so mapping into its address space must be denied"
        ));
    }
    if tasks_after != 6 {
        return Err(format!(
            "task_ready tasks_after is {tasks_after}, expected 6 (init + task 2 + the reserved Faulted witness task + the surviving Rust client + two independent Rust service endpoints) — the self-destroyed task must be gone from the task table"
        ));
    }
    if root4_cleared != 1 {
        return Err(format!(
            "task_ready root4_cleared is {root4_cleared}, expected 1 — the self-destroyed task's scheduler root registration must be cleared"
        ));
    }
    if caps_after != TASK_READY_LIVE_CAPS {
        return Err(format!(
            "task_ready caps_after is {caps_after}, expected {TASK_READY_LIVE_CAPS}"
        ));
    }
    if caps != TASK_READY_LIVE_CAPS {
        return Err(format!(
            "task_ready caps is {caps}, expected {TASK_READY_LIVE_CAPS} — the destroyed child's self/root-ns caps must be revoked while the caller's manager cap stays live"
        ));
    }
    Ok(())
}

fn parse_decimal_list(value: &str, key: &str) -> Result<Vec<u64>, String> {
    value
        .split(',')
        .map(|item| parse_decimal(item, key))
        .collect()
}

fn parse_addr_list(value: &str, key: &str) -> Result<Vec<u64>, String> {
    value
        .split(',')
        .map(|item| {
            parse_addr(item).map_err(|error| format!("invalid {key} entry {item}: {error}"))
        })
        .collect()
}

fn validate_exc_wait_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "exc_wait_ready event is missing its data field".to_owned())?;
    let mut cs_wait = None;
    let mut cs_trap = None;
    let mut delivered = None;
    let mut pending = None;
    let mut woken = None;
    let mut payload = None;
    let mut blocks = None;
    let mut wait_receipt = None;
    let mut trap_receipt = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "cs_wait" => cs_wait = Some(parse_addr(value)?),
            "cs_trap" => cs_trap = Some(parse_addr(value)?),
            "delivered" => delivered = Some(parse_decimal(value, "delivered")?),
            "pending" => pending = Some(parse_decimal(value, "pending")?),
            "woken" => woken = Some(parse_decimal(value, "woken")?),
            "payload" => payload = Some(parse_addr(value)?),
            "blocks" => blocks = Some(parse_decimal(value, "blocks")?),
            "wait_receipt" => wait_receipt = Some(parse_addr(value)?),
            "trap_receipt" => trap_receipt = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let cs_wait = cs_wait.ok_or_else(|| "missing cs_wait in exc_wait_ready data".to_owned())?;
    let cs_trap = cs_trap.ok_or_else(|| "missing cs_trap in exc_wait_ready data".to_owned())?;
    let delivered =
        delivered.ok_or_else(|| "missing delivered in exc_wait_ready data".to_owned())?;
    let pending = pending.ok_or_else(|| "missing pending in exc_wait_ready data".to_owned())?;
    let woken = woken.ok_or_else(|| "missing woken in exc_wait_ready data".to_owned())?;
    let payload = payload.ok_or_else(|| "missing payload in exc_wait_ready data".to_owned())?;
    let blocks = blocks.ok_or_else(|| "missing blocks in exc_wait_ready data".to_owned())?;
    let wait_receipt =
        wait_receipt.ok_or_else(|| "missing wait_receipt in exc_wait_ready data".to_owned())?;
    let trap_receipt =
        trap_receipt.ok_or_else(|| "missing trap_receipt in exc_wait_ready data".to_owned())?;

    if cs_wait != USER_CS || cs_trap != USER_CS {
        return Err(format!(
            "exc_wait_ready cs_wait/cs_trap (0x{cs_wait:x}/0x{cs_trap:x}) must both be 0x{USER_CS:x} — wait and trap must originate from CPL3"
        ));
    }
    if delivered != 1 {
        return Err(format!(
            "exc_wait_ready delivered ({delivered}) must be 1 — exactly one subscriber notified"
        ));
    }
    if pending != 1 {
        return Err(format!(
            "exc_wait_ready pending ({pending}) must be 1 — delivery must take the pending_return path"
        ));
    }
    if woken != MT_TASK1 {
        return Err(format!(
            "exc_wait_ready woken ({woken}) must be {MT_TASK1} — the blocked waiter must be task 1"
        ));
    }
    if payload != EXC_SYSCALL_TRAP_PAYLOAD {
        return Err(format!(
            "exc_wait_ready payload (0x{payload:x}) must equal syscall_trap notification 0x{EXC_SYSCALL_TRAP_PAYLOAD:x}"
        ));
    }
    if blocks != 1 {
        return Err(format!(
            "exc_wait_ready blocks ({blocks}) must be 1 — exactly one blocking wait"
        ));
    }
    if wait_receipt != payload || trap_receipt != delivered {
        return Err(format!(
            "exc_wait_ready receipts must equal their returns (wait 0x{wait_receipt:x}/0x{payload:x}, trap 0x{trap_receipt:x}/{delivered})"
        ));
    }
    Ok(())
}

/// 校验切片 O 的时钟抢占见证：两任务被 PIT 反复轮转、各有独立 CR3/内核栈、同 VA 在不同
/// 根下解析出不同物理帧且各自写入自己的标记。
fn validate_preempt_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "preempt_ready event is missing its data field".to_owned())?;
    let mut ticks = None;
    let mut pre1 = None;
    let mut pre2 = None;
    let mut root1 = None;
    let mut root2 = None;
    let mut cr31 = None;
    let mut cr32 = None;
    let mut rsp1 = None;
    let mut rsp2 = None;
    let mut kstack1_top = None;
    let mut kstack2_top = None;
    let mut data1 = None;
    let mut data2 = None;
    let mut mark1 = None;
    let mut mark2 = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "ticks" => ticks = Some(parse_decimal(value, "ticks")?),
            "pre1" => pre1 = Some(parse_decimal(value, "pre1")?),
            "pre2" => pre2 = Some(parse_decimal(value, "pre2")?),
            "root1" => root1 = Some(parse_addr(value)?),
            "root2" => root2 = Some(parse_addr(value)?),
            "cr31" => cr31 = Some(parse_addr(value)?),
            "cr32" => cr32 = Some(parse_addr(value)?),
            "rsp1" => rsp1 = Some(parse_addr(value)?),
            "rsp2" => rsp2 = Some(parse_addr(value)?),
            "kstack1_top" => kstack1_top = Some(parse_addr(value)?),
            "kstack2_top" => kstack2_top = Some(parse_addr(value)?),
            "data1" => data1 = Some(parse_addr(value)?),
            "data2" => data2 = Some(parse_addr(value)?),
            "mark1" => mark1 = Some(parse_addr(value)?),
            "mark2" => mark2 = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let ticks = ticks.ok_or_else(|| "missing ticks in preempt_ready data".to_owned())?;
    let pre1 = pre1.ok_or_else(|| "missing pre1 in preempt_ready data".to_owned())?;
    let pre2 = pre2.ok_or_else(|| "missing pre2 in preempt_ready data".to_owned())?;
    let root1 = root1.ok_or_else(|| "missing root1 in preempt_ready data".to_owned())?;
    let root2 = root2.ok_or_else(|| "missing root2 in preempt_ready data".to_owned())?;
    let cr31 = cr31.ok_or_else(|| "missing cr31 in preempt_ready data".to_owned())?;
    let cr32 = cr32.ok_or_else(|| "missing cr32 in preempt_ready data".to_owned())?;
    let rsp1 = rsp1.ok_or_else(|| "missing rsp1 in preempt_ready data".to_owned())?;
    let rsp2 = rsp2.ok_or_else(|| "missing rsp2 in preempt_ready data".to_owned())?;
    let kstack1_top =
        kstack1_top.ok_or_else(|| "missing kstack1_top in preempt_ready data".to_owned())?;
    let kstack2_top =
        kstack2_top.ok_or_else(|| "missing kstack2_top in preempt_ready data".to_owned())?;
    let data1 = data1.ok_or_else(|| "missing data1 in preempt_ready data".to_owned())?;
    let data2 = data2.ok_or_else(|| "missing data2 in preempt_ready data".to_owned())?;
    let mark1 = mark1.ok_or_else(|| "missing mark1 in preempt_ready data".to_owned())?;
    let mark2 = mark2.ok_or_else(|| "missing mark2 in preempt_ready data".to_owned())?;

    if ticks == 0 {
        return Err("preempt_ready ticks must be > 0 — the PIT clock must have fired".to_owned());
    }
    if pre1 < PREEMPT_THRESHOLD || pre2 < PREEMPT_THRESHOLD {
        return Err(format!(
            "preempt_ready pre1/pre2 ({pre1}/{pre2}) must both be >= {PREEMPT_THRESHOLD} — the timer must rotate both tasks"
        ));
    }
    if root1 == 0 || root2 == 0 || root1 == root2 {
        return Err(format!(
            "preempt_ready roots (0x{root1:x}/0x{root2:x}) must be nonzero and distinct — tasks need per-task address spaces"
        ));
    }
    if cr31 != root1 || cr32 != root2 {
        return Err(format!(
            "preempt_ready observed CR3 (0x{cr31:x}/0x{cr32:x}) must equal the task roots (0x{root1:x}/0x{root2:x})"
        ));
    }
    if rsp1 == rsp2 {
        return Err(format!(
            "preempt_ready rsp1/rsp2 (0x{rsp1:x}) must differ — tasks need per-task kernel stacks"
        ));
    }
    if rsp1 <= kstack1_top.saturating_sub(KSTACK_SIZE) || rsp1 > kstack1_top {
        return Err(format!(
            "preempt_ready rsp1 (0x{rsp1:x}) must lie in task 1 kernel stack (top 0x{kstack1_top:x}, size {KSTACK_SIZE})"
        ));
    }
    if rsp2 <= kstack2_top.saturating_sub(KSTACK_SIZE) || rsp2 > kstack2_top {
        return Err(format!(
            "preempt_ready rsp2 (0x{rsp2:x}) must lie in task 2 kernel stack (top 0x{kstack2_top:x}, size {KSTACK_SIZE})"
        ));
    }
    if data1 == 0 || data2 == 0 || data1 == data2 {
        return Err(format!(
            "preempt_ready data frames (0x{data1:x}/0x{data2:x}) must be distinct — same VA must map to different frames"
        ));
    }
    if mark1 != MT_TASK1 || mark2 != MT_TASK2 {
        return Err(format!(
            "preempt_ready markers ({mark1}/{mark2}) must be {MT_TASK1}/{MT_TASK2} — each task must run in its own address space"
        ));
    }
    Ok(())
}

/// 校验切片 F 的 COW 见证：写故障分类、帧字节复制完整性、原页未被修改、重试成功。
fn validate_cow_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "cow_ready event is missing its data field".to_owned())?;
    let mut va = None;
    let mut shared = None;
    let mut private = None;
    let mut err = None;
    let mut class = None;
    let mut shared0 = None;
    let mut private0 = None;
    let mut private1 = None;
    let mut private_last = None;
    let mut read_back = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "va" => va = Some(parse_addr(value)?),
            "shared" => shared = Some(parse_addr(value)?),
            "private" => private = Some(parse_addr(value)?),
            "err" => err = Some(parse_addr(value)?),
            "class" => class = Some(value.to_owned()),
            "shared0" => shared0 = Some(parse_addr(value)?),
            "private0" => private0 = Some(parse_addr(value)?),
            "private1" => private1 = Some(parse_addr(value)?),
            "private_last" => private_last = Some(parse_addr(value)?),
            "read_back" => read_back = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let va = va.ok_or_else(|| "missing va in cow_ready data".to_owned())?;
    let shared = shared.ok_or_else(|| "missing shared in cow_ready data".to_owned())?;
    let private = private.ok_or_else(|| "missing private in cow_ready data".to_owned())?;
    let err = err.ok_or_else(|| "missing err in cow_ready data".to_owned())?;
    let class = class.ok_or_else(|| "missing class in cow_ready data".to_owned())?;
    let shared0 = shared0.ok_or_else(|| "missing shared0 in cow_ready data".to_owned())?;
    let private0 = private0.ok_or_else(|| "missing private0 in cow_ready data".to_owned())?;
    let private1 = private1.ok_or_else(|| "missing private1 in cow_ready data".to_owned())?;
    let private_last =
        private_last.ok_or_else(|| "missing private_last in cow_ready data".to_owned())?;
    let read_back = read_back.ok_or_else(|| "missing read_back in cow_ready data".to_owned())?;
    if va != COW_VA {
        return Err(format!(
            "cow_ready va (0x{va:x}) must equal COW_VA 0x{COW_VA:x}"
        ));
    }
    if private == shared {
        return Err(format!(
            "cow_ready private must differ from shared (0x{shared:x}) — no private frame was allocated"
        ));
    }
    if err != 0x3 {
        return Err(format!(
            "cow_ready err must be 0x3 (present|write) for a read-only page write, got 0x{err:x}"
        ));
    }
    if class != "write_protection" {
        return Err(format!(
            "cow_ready class must be write_protection, got {class}"
        ));
    }
    if shared0 != cow_pattern(0) {
        return Err(format!(
            "cow_ready shared0 (0x{shared0:x}) must be unchanged 0x{:x} — copy-on-write must not modify the shared frame",
            cow_pattern(0)
        ));
    }
    if private0 != va {
        return Err(format!(
            "cow_ready private0 (0x{private0:x}) must equal va 0x{va:x} — retried store must have executed"
        ));
    }
    if private1 != cow_pattern(1) {
        return Err(format!(
            "cow_ready private1 (0x{private1:x}) must equal 0x{:x} — frame bytes not fully copied",
            cow_pattern(1)
        ));
    }
    if private_last != cow_pattern(COW_PATTERN_WORDS - 1) {
        return Err(format!(
            "cow_ready private_last (0x{private_last:x}) must equal 0x{:x} — frame tail not copied",
            cow_pattern(COW_PATTERN_WORDS - 1)
        ));
    }
    if read_back != va {
        return Err(format!(
            "cow_ready read_back (0x{read_back:x}) must equal va 0x{va:x} — retry did not take effect"
        ));
    }
    Ok(())
}

/// 校验切片 E 的自建映射见证：PTE 翻译、重映射 + invlpg 刷新、真实写保护 #PF。
fn validate_vspace_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "vspace_ready event is missing its data field".to_owned())?;
    let mut va = None;
    let mut frame_a = None;
    let mut frame_b = None;
    let mut read_before = None;
    let mut stale_read = None;
    let mut read_after = None;
    let mut translated = None;
    let mut wp_addr = None;
    let mut wp_err = None;
    let mut class = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "va" => va = Some(parse_addr(value)?),
            "frame_a" => frame_a = Some(parse_addr(value)?),
            "frame_b" => frame_b = Some(parse_addr(value)?),
            "read_before" => read_before = Some(parse_addr(value)?),
            "stale_read" => stale_read = Some(parse_addr(value)?),
            "read_after" => read_after = Some(parse_addr(value)?),
            "translated" => translated = Some(parse_addr(value)?),
            "wp_addr" => wp_addr = Some(parse_addr(value)?),
            "wp_err" => wp_err = Some(parse_addr(value)?),
            "class" => class = Some(value.to_owned()),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let va = va.ok_or_else(|| "missing va in vspace_ready data".to_owned())?;
    let frame_a = frame_a.ok_or_else(|| "missing frame_a in vspace_ready data".to_owned())?;
    let frame_b = frame_b.ok_or_else(|| "missing frame_b in vspace_ready data".to_owned())?;
    let read_before =
        read_before.ok_or_else(|| "missing read_before in vspace_ready data".to_owned())?;
    let stale_read =
        stale_read.ok_or_else(|| "missing stale_read in vspace_ready data".to_owned())?;
    let read_after =
        read_after.ok_or_else(|| "missing read_after in vspace_ready data".to_owned())?;
    let translated =
        translated.ok_or_else(|| "missing translated in vspace_ready data".to_owned())?;
    let wp_addr = wp_addr.ok_or_else(|| "missing wp_addr in vspace_ready data".to_owned())?;
    let wp_err = wp_err.ok_or_else(|| "missing wp_err in vspace_ready data".to_owned())?;
    let class = class.ok_or_else(|| "missing class in vspace_ready data".to_owned())?;
    if va != VSPACE_VA {
        return Err(format!(
            "vspace_ready va (0x{va:x}) must equal self-built VA 0x{VSPACE_VA:x}"
        ));
    }
    if frame_a == frame_b {
        return Err(format!(
            "vspace_ready frame_a and frame_b must be distinct frames, both 0x{frame_a:x}"
        ));
    }
    if read_before != MAGIC_A {
        return Err(format!(
            "vspace_ready read_before (0x{read_before:x}) must equal MAGIC_A 0x{MAGIC_A:x} — PTE translation failed"
        ));
    }
    if stale_read != MAGIC_A && stale_read != MAGIC_B {
        return Err(format!(
            "vspace_ready stale_read (0x{stale_read:x}) must be MAGIC_A 0x{MAGIC_A:x} (stale TLB) or MAGIC_B 0x{MAGIC_B:x} (already coherent)"
        ));
    }
    if read_after != MAGIC_B {
        return Err(format!(
            "vspace_ready read_after (0x{read_after:x}) must equal MAGIC_B 0x{MAGIC_B:x} — remap + invlpg failed"
        ));
    }
    if translated != frame_b {
        return Err(format!(
            "vspace_ready translated (0x{translated:x}) must equal frame_b 0x{frame_b:x}"
        ));
    }
    if wp_addr != VSPACE_VA {
        return Err(format!(
            "vspace_ready wp_addr (0x{wp_addr:x}) must equal self-built VA 0x{VSPACE_VA:x}"
        ));
    }
    if wp_err != 0x3 {
        return Err(format!(
            "vspace_ready wp_err must be 0x3 (present|write) for a read-only page write, got 0x{wp_err:x}"
        ));
    }
    if class != "write_protection" {
        return Err(format!(
            "vspace_ready class must be write_protection, got {class}"
        ));
    }
    Ok(())
}

/// 校验切片 D 的恢复证据：落点读回的哨兵必须等于已知常量，三条布尔车道均为 1。
fn validate_exc_resume_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "exc_resume_ready event is missing its data field".to_owned())?;
    let mut sentinel = None;
    let mut sentinel2 = None;
    let mut resumed = None;
    let mut rsp_ok = None;
    let mut flags_ok = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "sentinel" => sentinel = Some(parse_addr(value)?),
            "sentinel2" => sentinel2 = Some(parse_addr(value)?),
            "resumed" => resumed = Some(parse_count(value)?),
            "rsp_ok" => rsp_ok = Some(parse_count(value)?),
            "flags_ok" => flags_ok = Some(parse_count(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let sentinel =
        sentinel.ok_or_else(|| "missing sentinel in exc_resume_ready data".to_owned())?;
    let sentinel2 =
        sentinel2.ok_or_else(|| "missing sentinel2 in exc_resume_ready data".to_owned())?;
    let resumed = resumed.ok_or_else(|| "missing resumed in exc_resume_ready data".to_owned())?;
    let rsp_ok = rsp_ok.ok_or_else(|| "missing rsp_ok in exc_resume_ready data".to_owned())?;
    let flags_ok =
        flags_ok.ok_or_else(|| "missing flags_ok in exc_resume_ready data".to_owned())?;
    if sentinel != RESUME_SENTINEL_A {
        return Err(format!(
            "exc_resume_ready sentinel (0x{sentinel:x}) must equal 0x{RESUME_SENTINEL_A:x} — r12 was not restored"
        ));
    }
    if sentinel2 != RESUME_SENTINEL_B {
        return Err(format!(
            "exc_resume_ready sentinel2 (0x{sentinel2:x}) must equal 0x{RESUME_SENTINEL_B:x} — r13 was not restored"
        ));
    }
    if resumed != 1 {
        return Err(format!("exc_resume_ready resumed must be 1, got {resumed}"));
    }
    if rsp_ok != 1 {
        return Err(format!(
            "exc_resume_ready rsp_ok must be 1 (stack switched to resume stack top), got {rsp_ok}"
        ));
    }
    if flags_ok != 1 {
        return Err(format!(
            "exc_resume_ready flags_ok must be 1 (RFLAGS lane restored), got {flags_ok}"
        ));
    }
    Ok(())
}

fn validate_bootstrap_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "bootstrap_ready event is missing its data field".to_owned())?;
    let mut task = None;
    let mut caps = None;
    let mut image_cap = None;
    let mut namespace_cap = None;
    let mut task_cap = None;
    let mut image_base = None;
    let mut image_len = None;
    let mut image_frames = None;
    let mut free_frames = None;
    let mut image_rights = None;
    let mut namespace_rights = None;
    let mut task_rights = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "task" => task = Some(parse_count(value)?),
            "caps" => caps = Some(parse_count(value)?),
            "image_cap" => image_cap = Some(parse_addr(value)?),
            "namespace_cap" => namespace_cap = Some(parse_addr(value)?),
            "task_cap" => task_cap = Some(parse_addr(value)?),
            "image_base" => image_base = Some(parse_addr(value)?),
            "image_len" => image_len = Some(parse_count(value)?),
            "image_frames" => image_frames = Some(parse_count(value)?),
            "free_frames" => free_frames = Some(parse_count(value)?),
            "image_rights" => image_rights = Some(value),
            "namespace_rights" => namespace_rights = Some(value),
            "task_rights" => task_rights = Some(value),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    if task != Some(1)
        || caps != Some(3)
        || image_cap != Some(IMAGE_CAP_HANDLE)
        || namespace_cap != Some(NAMESPACE_CAP_HANDLE)
        || task_cap != Some(TASK_CAP_HANDLE)
    {
        return Err(format!(
            "bootstrap authority mismatch: task={task:?}, caps={caps:?}, image_cap={image_cap:?}, namespace_cap={namespace_cap:?}, task_cap={task_cap:?}"
        ));
    }
    if image_base == Some(0) || image_len.is_none_or(|length| length == 0) {
        return Err("bootstrap image mapping must be non-empty".to_owned());
    }
    if image_frames.is_none_or(|frames| frames == 0) || free_frames.is_none_or(|frames| frames == 0)
    {
        return Err("bootstrap frame ownership must be non-empty".to_owned());
    }
    if image_rights != Some("Derive,SegMapR,SegMapX,SegDestroy")
        || namespace_rights != Some("Derive,NsResolve,NsBind,NsUnbind,NsMount,NsUnmount,NsDestroy")
        || task_rights != Some("Derive,TaskSend,TaskManageVSpace,TaskDestroy")
    {
        return Err(format!(
            "bootstrap rights mismatch: image={image_rights:?}, namespace={namespace_rights:?}, task={task_rights:?}"
        ));
    }
    Ok(())
}

fn validate_trap_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "trap_ready event is missing its data field".to_owned())?;
    let mut seen = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        if key != "seen" {
            return Err(format!("unknown data key: {key}"));
        }
        seen = Some(parse_count(value)?);
    }
    let seen = seen.ok_or_else(|| "missing seen in trap_ready data".to_owned())?;
    if seen != 3 {
        return Err(format!(
            "trap_ready seen must be 3 (breakpoint), got {seen}"
        ));
    }
    Ok(())
}

fn validate_page_fault_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "page_fault_ready event is missing its data field".to_owned())?;
    let mut addr = None;
    let mut err = None;
    let mut class = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "addr" => addr = Some(parse_addr(value)?),
            "err" => err = Some(parse_addr(value)?),
            "class" => class = Some(value.to_owned()),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let addr = addr.ok_or_else(|| "missing addr in page_fault_ready data".to_owned())?;
    let err = err.ok_or_else(|| "missing err in page_fault_ready data".to_owned())?;
    let class = class.ok_or_else(|| "missing class in page_fault_ready data".to_owned())?;
    if addr != PROBE_FAULT_VA {
        return Err(format!(
            "page_fault_ready addr (0x{addr:x}) must equal probe address 0x{PROBE_FAULT_VA:x}"
        ));
    }
    if err != 0 {
        return Err(format!(
            "page_fault_ready err must be 0x0 for absent-read probe, got 0x{err:x}"
        ));
    }
    if class != "not_present" {
        return Err(format!(
            "page_fault_ready class must be not_present for absent-read probe, got {class}"
        ));
    }
    Ok(())
}

fn validate_page_table_data(data: Option<&str>) -> Result<u64, String> {
    let data = data.ok_or_else(|| "page_table_ready event is missing its data field".to_owned())?;
    let mut translated = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        if key != "translated" {
            return Err(format!("unknown data key: {key}"));
        }
        translated = Some(parse_addr(value)?);
    }
    let translated =
        translated.ok_or_else(|| "missing translated in page_table_ready data".to_owned())?;
    if translated == 0 {
        return Err("translated physical address must be > 0".to_owned());
    }
    Ok(translated)
}

fn validate_memory_data(data: Option<&str>) -> Result<u64, String> {
    let data = data.ok_or_else(|| "memory_ready event is missing its data field".to_owned())?;

    let mut usable = None;
    let mut image = None;
    let mut free = None;
    let mut kstart = None;
    let mut kend = None;

    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "usable" => usable = Some(parse_count(value)?),
            "image" => image = Some(parse_count(value)?),
            "free" => free = Some(parse_count(value)?),
            "kstart" => kstart = Some(parse_addr(value)?),
            "kend" => kend = Some(parse_addr(value)?),
            "regions" => {
                parse_count(value)?;
            }
            _ => return Err(format!("unknown data key: {key}")),
        }
    }

    let usable = usable.ok_or_else(|| "missing usable in memory_ready data".to_owned())?;
    let image = image.ok_or_else(|| "missing image in memory_ready data".to_owned())?;
    let free = free.ok_or_else(|| "missing free in memory_ready data".to_owned())?;
    let kstart = kstart.ok_or_else(|| "missing kstart in memory_ready data".to_owned())?;
    let kend = kend.ok_or_else(|| "missing kend in memory_ready data".to_owned())?;

    if image == 0 {
        return Err("image frame count must be > 0".to_owned());
    }
    if free == 0 {
        return Err("free frame count must be > 0".to_owned());
    }
    if free != usable {
        return Err(format!(
            "free ({free}) must equal usable ({usable}) because the kernel image is carried by the bootloader outside Usable"
        ));
    }
    if kstart % 4096 != 0 || kend % 4096 != 0 {
        return Err(format!(
            "kernel image addresses must be page-aligned: kstart=0x{kstart:x}, kend=0x{kend:x}"
        ));
    }
    if kstart >= kend {
        return Err(format!(
            "kernel image start (0x{kstart:x}) must be below end (0x{kend:x})"
        ));
    }

    Ok(kstart)
}

const RUNTIME_HELLO_DATA: &str = "map=0x0;magic=0x14d495250;opcode=0x300000001;length=0x100000018;features=0x1;required=0x1;max=4096";

/// SPEC-087：`service_reply_ready` 的镜像常量，与 `kernel/src/{user,syscall}.rs` 一致
/// （跨 crate 需人工同步）。
const SERVICE_REPLY_CLIENT_CAP: u64 = 0x0000_0001_0000_0018;
const SERVICE_REPLY_CLIENT_ID: u64 = 8;
const SERVICE_REPLY_WORD: u64 = 0x0003_8001_474E_4F50;

fn validate_runtime_hello_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "runtime_hello_ready is missing data".to_owned())?;
    if data != RUNTIME_HELLO_DATA {
        return Err(format!("runtime_hello_ready wire data differs: {data}"));
    }
    Ok(())
}

/// SPEC-087：`service_reply_ready` 的期望字段——init 与存活 ring3 客户端双向握手。
///
/// `client` 是 `SYS_TASK_CREATE` 返回的 manager cap（槽 24 gen1），`recv` 是客户端自报的
/// task id，`echo` 是客户端写回的 `SERVICE_REPLY_WORD`（本块由客户端从共享段读回）。
fn validate_service_reply_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "service_reply_ready is missing data".to_owned())?;
    let mut client = None;
    let mut stack = None;
    let mut recv = None;
    let mut map = None;
    let mut reply = None;
    let mut echo = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "client" => client = Some(parse_addr(value)?),
            "stack" => stack = Some(parse_addr(value)?),
            "recv" => recv = Some(parse_addr(value)?),
            "map" => map = Some(parse_addr(value)?),
            "reply" => reply = Some(parse_addr(value)?),
            "echo" => echo = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let missing = |key: &str| format!("missing {key} in service_reply_ready data");
    let client = client.ok_or_else(|| missing("client"))?;
    let stack = stack.ok_or_else(|| missing("stack"))?;
    let recv = recv.ok_or_else(|| missing("recv"))?;
    let map = map.ok_or_else(|| missing("map"))?;
    let reply = reply.ok_or_else(|| missing("reply"))?;
    let echo = echo.ok_or_else(|| missing("echo"))?;
    if client != SERVICE_REPLY_CLIENT_CAP {
        return Err(format!(
            "service_reply_ready client (0x{client:x}) must be the manager cap 0x{SERVICE_REPLY_CLIENT_CAP:x} returned by SYS_TASK_CREATE"
        ));
    }
    if stack != 0 {
        return Err(format!(
            "service_reply_ready stack (0x{stack:x}) must be 0 — init must map the client stack through the manager cap"
        ));
    }
    if recv != SERVICE_REPLY_CLIENT_ID {
        return Err(format!(
            "service_reply_ready recv ({recv}) must equal the client id {SERVICE_REPLY_CLIENT_ID} — the Hello payload is the client's own SYS_TASK_ID"
        ));
    }
    if map != 0 {
        return Err(format!(
            "service_reply_ready map (0x{map:x}) must be 0 — init must map the received shared Segment into its own root"
        ));
    }
    if reply != 0 {
        return Err(format!(
            "service_reply_ready reply (0x{reply:x}) must be 0 — init's reply SYS_SEND must succeed"
        ));
    }
    if echo != SERVICE_REPLY_WORD {
        return Err(format!(
            "service_reply_ready echo (0x{echo:x}) must be 0x{SERVICE_REPLY_WORD:x} — the client must write init's reply payload back into the shared Segment"
        ));
    }
    Ok(())
}

/// SPEC-090/101：`service_selected_ready` 的镜像常量，与 `kernel/src/user.rs` 一致。
/// SPEC-101 起服务名是 ASCII 令牌打包值：`name` = `"beta"`（`0x6174_6562`）。
const SERVICE_SELECTED_NAME: u64 = 0x6174_6562;
const SERVICE_SELECTED_RESOLVED: u64 = 2;
const SERVICE_SELECTED_UNKNOWN: u64 = 0;
/// `BETA` 行为 offer `{features 0b101, required 0b001, max 1024}` 协商得
/// `{major 1, minor 0, features 1, max 1024}`；`max=1024` 只可能来自被按名选中的 `BETA` offer
/// （`ALPHA` 是 2048），是「管理器真按名路由」的判决性字段。
const SERVICE_SELECTED_NEGOTIATED_B: u64 = 0x0001_0000_0400_0001;
/// SPEC-092：两个**独立服务端点**的 task id（客户端 = 8、`ALPHA` 端点 = 9、`BETA` 端点 = 10）。
/// 判决性：`beta_ep` 必须是被按名路由到的**第二个**任务，且与 `alpha_ep` 相异——即多端点，
/// 而非 SPEC-090 的「单宿主多行为」。
const SERVICE_SELECTED_ALPHA_EP: u64 = 9;
const SERVICE_SELECTED_BETA_EP: u64 = 10;
/// SPEC-094：存活客户端（task 8）的 task id。
const SERVICE_SELECTED_CLIENT_EP: u64 = 8;
/// SPEC-095/101：运行期服务注册表里两个端点回报的注册名（ASCII 令牌 `"alpha"`/`"beta"`；
/// 由服务管理器在创建时运行期指派，端点二进制无名字字面量）。
const SERVICE_SELECTED_REG_A_NAME: u64 = 0x0061_6870_6c61;
const SERVICE_SELECTED_REG_B_NAME: u64 = 0x6174_6562;
/// SPEC-093：两个端点映像的**二进制标记**（编译进各自 ELF；相异即证明「不同程序」）。
const SERVICE_SELECTED_ALPHA_MARKER: u64 = 0x414C_5048_4100_0001;
const SERVICE_SELECTED_BETA_MARKER: u64 = 0x4245_5441_0000_0002;

/// SPEC-090：init（服务管理器）按名字解析并二次路由的期望字段。
fn validate_service_selected_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "service_selected_ready is missing data".to_owned())?;
    let mut name = None;
    let mut resolved = None;
    let mut unknown = None;
    let mut done = None;
    let mut negotiated_beta = None;
    let mut alpha_ep = None;
    let mut beta_ep = None;
    let mut beta_self = None;
    let mut alpha_marker = None;
    let mut beta_marker = None;
    let mut alpha_stack_seg = None;
    let mut beta_stack_seg = None;
    let mut client_ep = None;
    let mut client_stack_seg = None;
    let mut reg_a_name = None;
    let mut reg_b_name = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "name" => name = Some(parse_addr(value)?),
            "resolved" => resolved = Some(parse_addr(value)?),
            "unknown" => unknown = Some(parse_addr(value)?),
            "done" => done = Some(parse_addr(value)?),
            "negotiated_beta" => negotiated_beta = Some(parse_addr(value)?),
            "alpha_ep" => alpha_ep = Some(parse_addr(value)?),
            "beta_ep" => beta_ep = Some(parse_addr(value)?),
            "beta_self" => beta_self = Some(parse_addr(value)?),
            "alpha_marker" => alpha_marker = Some(parse_addr(value)?),
            "beta_marker" => beta_marker = Some(parse_addr(value)?),
            "alpha_stack_seg" => alpha_stack_seg = Some(parse_addr(value)?),
            "beta_stack_seg" => beta_stack_seg = Some(parse_addr(value)?),
            "client_ep" => client_ep = Some(parse_addr(value)?),
            "client_stack_seg" => client_stack_seg = Some(parse_addr(value)?),
            "reg_a_name" => reg_a_name = Some(parse_addr(value)?),
            "reg_b_name" => reg_b_name = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let missing = |key: &str| format!("missing {key} in service_selected_ready data");
    let name = name.ok_or_else(|| missing("name"))?;
    let resolved = resolved.ok_or_else(|| missing("resolved"))?;
    let unknown = unknown.ok_or_else(|| missing("unknown"))?;
    let done = done.ok_or_else(|| missing("done"))?;
    let negotiated_beta = negotiated_beta.ok_or_else(|| missing("negotiated_beta"))?;
    let alpha_ep = alpha_ep.ok_or_else(|| missing("alpha_ep"))?;
    let beta_ep = beta_ep.ok_or_else(|| missing("beta_ep"))?;
    let beta_self = beta_self.ok_or_else(|| missing("beta_self"))?;
    let alpha_marker = alpha_marker.ok_or_else(|| missing("alpha_marker"))?;
    let beta_marker = beta_marker.ok_or_else(|| missing("beta_marker"))?;
    let alpha_stack_seg = alpha_stack_seg.ok_or_else(|| missing("alpha_stack_seg"))?;
    let beta_stack_seg = beta_stack_seg.ok_or_else(|| missing("beta_stack_seg"))?;
    let client_ep = client_ep.ok_or_else(|| missing("client_ep"))?;
    let client_stack_seg = client_stack_seg.ok_or_else(|| missing("client_stack_seg"))?;
    let reg_a_name = reg_a_name.ok_or_else(|| missing("reg_a_name"))?;
    let reg_b_name = reg_b_name.ok_or_else(|| missing("reg_b_name"))?;
    if name != SERVICE_SELECTED_NAME {
        return Err(format!(
            "service_selected_ready name (0x{name:x}) must be {SERVICE_SELECTED_NAME} — the manager must read the requested service name from the shared Segment"
        ));
    }
    if resolved != SERVICE_SELECTED_RESOLVED {
        return Err(format!(
            "service_selected_ready resolved (0x{resolved:x}) must be {SERVICE_SELECTED_RESOLVED} — the requested name must resolve to the BETA behavior"
        ));
    }
    if unknown != SERVICE_SELECTED_UNKNOWN {
        return Err(format!(
            "service_selected_ready unknown (0x{unknown:x}) must be {SERVICE_SELECTED_UNKNOWN} — an unregistered name must resolve to NONE (refused)"
        ));
    }
    if done != SERVICE_DONE_TAG {
        return Err(format!(
            "service_selected_ready done (0x{done:x}) must be the service completion tag 0x{SERVICE_DONE_TAG:x}"
        ));
    }
    if negotiated_beta != SERVICE_SELECTED_NEGOTIATED_B {
        return Err(format!(
            "service_selected_ready negotiated_beta (0x{negotiated_beta:x}) must be 0x{SERVICE_SELECTED_NEGOTIATED_B:x} — the routed BETA behavior must yield max=1024, not ALPHA's 2048"
        ));
    }
    if alpha_ep != SERVICE_SELECTED_ALPHA_EP {
        return Err(format!(
            "service_selected_ready alpha_ep ({alpha_ep}) must be {SERVICE_SELECTED_ALPHA_EP} — the ALPHA endpoint (first service task) must report its own task id next to its result"
        ));
    }
    if beta_ep != SERVICE_SELECTED_BETA_EP {
        return Err(format!(
            "service_selected_ready beta_ep ({beta_ep}) must be {SERVICE_SELECTED_BETA_EP} — the BETA request must be routed to a *second, independent* service task (multi-endpoint), not the ALPHA host"
        ));
    }
    if beta_self != SERVICE_SELECTED_BETA_EP {
        return Err(format!(
            "service_selected_ready beta_self ({beta_self}) must be {SERVICE_SELECTED_BETA_EP} — the manager must route BETA to the endpoint it collected from the second SERVICE_ID_TAG self-report"
        ));
    }
    if alpha_ep == beta_ep {
        return Err(
            "service_selected_ready alpha_ep and beta_ep must differ — two names must resolve to two distinct endpoint tasks".to_owned(),
        );
    }
    if alpha_marker != SERVICE_SELECTED_ALPHA_MARKER {
        return Err(format!(
            "service_selected_ready alpha_marker (0x{alpha_marker:x}) must be 0x{SERVICE_SELECTED_ALPHA_MARKER:x} — the ALPHA endpoint must run the prima-smoke image"
        ));
    }
    if beta_marker != SERVICE_SELECTED_BETA_MARKER {
        return Err(format!(
            "service_selected_ready beta_marker (0x{beta_marker:x}) must be 0x{SERVICE_SELECTED_BETA_MARKER:x} — the BETA endpoint must run the *second, distinct* image (prima-beta), not the ALPHA host image"
        ));
    }
    if alpha_marker == beta_marker {
        return Err(
            "service_selected_ready alpha_marker and beta_marker must differ — the two endpoints must be created from two distinct binaries".to_owned(),
        );
    }
    if alpha_stack_seg == 0 || beta_stack_seg == 0 {
        return Err(format!(
            "service_selected_ready stack segments (0x{alpha_stack_seg:x}/0x{beta_stack_seg:x}) must be non-zero — both endpoints must have a mapped stack at USER3_STACK_VA"
        ));
    }
    if alpha_stack_seg == beta_stack_seg {
        return Err(
            "service_selected_ready alpha_stack_seg and beta_stack_seg must differ — the two endpoints must own independent physical stack frames, not share one Segment".to_owned(),
        );
    }
    if client_ep != SERVICE_SELECTED_CLIENT_EP {
        return Err(format!(
            "service_selected_ready client_ep ({client_ep}) must be {SERVICE_SELECTED_CLIENT_EP} — the surviving ring3 client that init collected from the Hello payload"
        ));
    }
    if client_stack_seg == 0 {
        return Err(
            "service_selected_ready client_stack_seg (0x0) must be non-zero — the client must have a mapped stack at USER3_STACK_VA".to_owned(),
        );
    }
    if client_stack_seg == alpha_stack_seg {
        return Err(
            "service_selected_ready client_stack_seg and alpha_stack_seg must differ — the ALPHA endpoint must own its own physical stack frame, not the client's shared Y[368] Segment".to_owned(),
        );
    }
    if client_stack_seg == beta_stack_seg {
        return Err(
            "service_selected_ready client_stack_seg and beta_stack_seg must differ — the client, ALPHA and BETA must all own independent physical stack frames".to_owned(),
        );
    }
    if reg_a_name != SERVICE_SELECTED_REG_A_NAME || reg_b_name != SERVICE_SELECTED_REG_B_NAME {
        return Err(format!(
            "service_selected_ready registry names (reg_a_name={reg_a_name}, reg_b_name={reg_b_name}) must be {SERVICE_SELECTED_REG_A_NAME}/{SERVICE_SELECTED_REG_B_NAME} — the manager must build the name registry from the endpoints' self-reports, not from a compile-time constant"
        ));
    }
    Ok(())
}

/// SPEC-088：`service_negotiated_ready` 的镜像常量，与 `kernel/src/user.rs` 一致。
const SERVICE_NEGOTIATED_TOKEN: u64 = 0x0000_0001_0000_001c;
const SERVICE_NEGOTIATED_CAP: u64 = 0x0000_0001_0000_001d;
const SERVICE_NEGOTIATED_ID: u64 = 9;
const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
/// `major<<48 | minor<<32 | max<<16 | features` —— 客户端 offer 与服务端 offer
/// `{features 0b101, required 0b001, max 2048}` 协商得 `{major 1, minor 0, features 1, max 2048}`。
/// `max=2048` 只可能来自**服务端** offer（客户端是 4096），是「服务端真的参与协商」的判决性字段。
const SERVICE_EXPECTED_NEGOTIATED: u64 = 0x0001_0000_0800_0001;
/// `MajorMismatch(1)<<32 | BadMagic(0)`：两个负例由服务端 ring3 Rust 计算，证明 `decode`/`negotiate` 真跑过。
const SERVICE_EXPECTED_NEGATIVES: u64 = 0x0000_0001_0000_0000;

/// SPEC-088：init（broker）与 ring3 Rust 服务端点解析/协商的期望字段。
///
/// `token` 是 init 从代码段根派生的只读端点令牌，`service` 是第 4 次 `SYS_TASK_CREATE`
/// 返回的 manager cap（槽 28/29 gen1），`svc_id` 是服务自报的 task id，`done` 是服务完成
/// 消息 `SERVICE_DONE_TAG`；`negotiated`/`negatives` 由服务写入共享段、init 从同一物理帧读回。
fn validate_service_negotiated_data(data: Option<&str>) -> Result<(), String> {
    let data = data.ok_or_else(|| "service_negotiated_ready is missing data".to_owned())?;
    let mut token = None;
    let mut service = None;
    let mut stack = None;
    let mut svc_id = None;
    let mut forward = None;
    let mut done = None;
    let mut negotiated = None;
    let mut negatives = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "token" => token = Some(parse_addr(value)?),
            "service" => service = Some(parse_addr(value)?),
            "stack" => stack = Some(parse_addr(value)?),
            "svc_id" => svc_id = Some(parse_addr(value)?),
            "forward" => forward = Some(parse_addr(value)?),
            "done" => done = Some(parse_addr(value)?),
            "negotiated" => negotiated = Some(parse_addr(value)?),
            "negatives" => negatives = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let missing = |key: &str| format!("missing {key} in service_negotiated_ready data");
    let token = token.ok_or_else(|| missing("token"))?;
    let service = service.ok_or_else(|| missing("service"))?;
    let stack = stack.ok_or_else(|| missing("stack"))?;
    let svc_id = svc_id.ok_or_else(|| missing("svc_id"))?;
    let forward = forward.ok_or_else(|| missing("forward"))?;
    let done = done.ok_or_else(|| missing("done"))?;
    let negotiated = negotiated.ok_or_else(|| missing("negotiated"))?;
    let negatives = negatives.ok_or_else(|| missing("negatives"))?;
    if token != SERVICE_NEGOTIATED_TOKEN {
        return Err(format!(
            "service_negotiated_ready token (0x{token:x}) must be the derived endpoint token 0x{SERVICE_NEGOTIATED_TOKEN:x}"
        ));
    }
    if service != SERVICE_NEGOTIATED_CAP {
        return Err(format!(
            "service_negotiated_ready service (0x{service:x}) must be the manager cap 0x{SERVICE_NEGOTIATED_CAP:x} returned by the 4th SYS_TASK_CREATE"
        ));
    }
    if stack != 0 {
        return Err(format!(
            "service_negotiated_ready stack (0x{stack:x}) must be 0 — init must map the service stack through the manager cap"
        ));
    }
    if svc_id != SERVICE_NEGOTIATED_ID {
        return Err(format!(
            "service_negotiated_ready svc_id ({svc_id}) must equal the service id {SERVICE_NEGOTIATED_ID}"
        ));
    }
    if forward != 0 {
        return Err(format!(
            "service_negotiated_ready forward (0x{forward:x}) must be 0 — init's cap-carrying forward SYS_SEND must succeed"
        ));
    }
    if done != SERVICE_DONE_TAG {
        return Err(format!(
            "service_negotiated_ready done (0x{done:x}) must be the service completion tag 0x{SERVICE_DONE_TAG:x}"
        ));
    }
    if negotiated != SERVICE_EXPECTED_NEGOTIATED {
        return Err(format!(
            "service_negotiated_ready negotiated (0x{negotiated:x}) must be 0x{SERVICE_EXPECTED_NEGOTIATED:x} — the server-side negotiation must yield max=2048 from the service offer, not echo the client's 4096"
        ));
    }
    if negatives != SERVICE_EXPECTED_NEGATIVES {
        return Err(format!(
            "service_negotiated_ready negatives (0x{negatives:x}) must be 0x{SERVICE_EXPECTED_NEGATIVES:x} — the ring3 service must observe MajorMismatch (1) and BadMagic (0)"
        ));
    }
    Ok(())
}

fn validate_physical_map_data(data: Option<&str>) -> Result<(), String> {
    let data =
        data.ok_or_else(|| "physical_map_ready event is missing its data field".to_owned())?;
    let mut offset = None;
    let mut va = None;
    for pair in data.split(';') {
        let (key, value) = pair
            .split_once('=')
            .ok_or_else(|| format!("malformed data pair (missing '='): {pair}"))?;
        match key {
            "offset" => offset = Some(parse_addr(value)?),
            "va" => va = Some(parse_addr(value)?),
            _ => return Err(format!("unknown data key: {key}")),
        }
    }
    let offset = offset.ok_or_else(|| "missing offset in physical_map_ready data".to_owned())?;
    let va = va.ok_or_else(|| "missing va in physical_map_ready data".to_owned())?;
    if va < offset {
        return Err(format!("va (0x{va:x}) must be >= offset (0x{offset:x})"));
    }
    Ok(())
}

fn parse_count(value: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|error| format!("invalid count {value}: {error}"))
}

fn parse_addr(value: &str) -> Result<u64, String> {
    let hex = value
        .strip_prefix("0x")
        .ok_or_else(|| format!("invalid address (missing 0x prefix): {value}"))?;
    u64::from_str_radix(hex, 16).map_err(|error| format!("invalid address {value}: {error}"))
}

fn parse_line(line: &str) -> Result<Event, String> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() != 5 && fields.len() != 6 {
        return Err(format!(
            "PRIMA line must contain five or six fields, got {}: {line}",
            fields.len()
        ));
    }
    if fields[0] != "PRIMA" {
        return Err(format!("invalid PRIMA prefix: {line}"));
    }
    if fields[1] != "v=1" {
        return Err(format!("unsupported PRIMA protocol version: {}", fields[1]));
    }

    let sequence = fields[2]
        .strip_prefix("seq=")
        .ok_or_else(|| format!("invalid sequence field: {}", fields[2]))?
        .parse::<u32>()
        .map_err(|error| format!("invalid sequence number in {}: {error}", fields[2]))?;
    let event = fields[3]
        .strip_prefix("event=")
        .ok_or_else(|| format!("invalid event field: {}", fields[3]))?;
    let status = fields[4]
        .strip_prefix("status=")
        .ok_or_else(|| format!("invalid status field: {}", fields[4]))?;
    let data = if fields.len() == 6 {
        Some(
            fields[5]
                .strip_prefix("data=")
                .ok_or_else(|| format!("invalid data field: {}", fields[5]))?
                .to_owned(),
        )
    } else {
        None
    };

    let kind = match (event, status) {
        ("boot_start", "ok") => EventKind::BootStart,
        ("serial_ready", "ok") => EventKind::SerialReady,
        ("memory_ready", "ok") => EventKind::MemoryReady,
        ("physical_map_ready", "ok") => EventKind::PhysicalMapReady,
        ("page_table_ready", "ok") => EventKind::PageTableReady,
        ("gdt_tss_ready", "ok") => EventKind::GdtTssReady,
        ("trap_ready", "ok") => EventKind::TrapReady,
        ("page_fault_ready", "ok") => EventKind::PageFaultReady,
        ("bootstrap_ready", "ok") => EventKind::BootstrapReady,
        ("vspace_ready", "ok") => EventKind::VspaceReady,
        ("cow_ready", "ok") => EventKind::CowReady,
        ("user_cow_ready", "ok") => EventKind::UserCowReady,
        ("syscall_ready", "ok") => EventKind::SyscallReady,
        ("syscall_auth_ready", "ok") => EventKind::SyscallAuthReady,
        ("exc_ready", "ok") => EventKind::ExcReady,
        ("exc_pending_ready", "ok") => EventKind::ExcPendingReady,
        ("syscall_fast_ready", "ok") => EventKind::SyscallFastReady,
        ("multitask_ready", "ok") => EventKind::MultitaskReady,
        ("exc_wait_ready", "ok") => EventKind::ExcWaitReady,
        ("handle_ready", "ok") => EventKind::HandleReady,
        ("cap_lifecycle_ready", "ok") => EventKind::CapLifecycleReady,
        ("memseg_ready", "ok") => EventKind::MemsegReady,
        ("ipc_ready", "ok") => EventKind::IpcReady,
        ("task_ready", "ok") => EventKind::TaskReady,
        ("runtime_hello_ready", "ok") => EventKind::RuntimeHelloReady,
        ("service_reply_ready", "ok") => EventKind::ServiceReplyReady,
        ("service_negotiated_ready", "ok") => EventKind::ServiceNegotiatedReady,
        ("service_selected_ready", "ok") => EventKind::ServiceSelectedReady,
        ("namespace_ready", "ok") => EventKind::NamespaceReady,
        ("preempt_ready", "ok") => EventKind::PreemptReady,
        ("timer_ready", "ok") => EventKind::TimerReady,
        ("audit_ready", "ok") => EventKind::AuditReady,
        ("exc_resume_ready", "ok") => EventKind::ExcResumeReady,
        ("platform_segments_ready", "ok") => EventKind::PlatformSegmentsReady,
        ("authority_ready", "ok") => EventKind::AuthorityReady,
        ("frames_ready", "ok") => EventKind::FramesReady,
        ("self_root_ready", "ok") => EventKind::SelfRootReady,
        ("fault_ready", "ok") => EventKind::FaultReady,
        ("state_audit_ready", "ok") => EventKind::StateAuditReady,
        ("reclaim_ready", "ok") => EventKind::ReclaimReady,
        ("resume_ready", "ok") => EventKind::ResumeReady,
        ("boot_complete", "ok") => EventKind::BootComplete,
        ("memory_error", "error") => EventKind::MemoryError,
        ("physical_map_error", "error") => EventKind::PhysicalMapError,
        ("page_table_error", "error") => EventKind::PageTableError,
        ("gdt_tss_error", "error") => EventKind::GdtTssError,
        ("idt_error", "error") => EventKind::IdtError,
        ("panic", "error") => EventKind::Panic,
        _ => return Err(format!("unsupported event/status pair: {event}/{status}")),
    };

    match (kind, &data) {
        (
            EventKind::MemoryReady
            | EventKind::MemoryError
            | EventKind::PhysicalMapReady
            | EventKind::PageTableReady
            | EventKind::TrapReady
            | EventKind::PageFaultReady
            | EventKind::BootstrapReady
            | EventKind::VspaceReady
            | EventKind::CowReady
            | EventKind::UserCowReady
            | EventKind::SyscallReady
            | EventKind::SyscallAuthReady
            | EventKind::ExcReady
            | EventKind::ExcPendingReady
            | EventKind::SyscallFastReady
            | EventKind::MultitaskReady
            | EventKind::ExcWaitReady
            | EventKind::HandleReady
            | EventKind::CapLifecycleReady
            | EventKind::MemsegReady
            | EventKind::IpcReady
            | EventKind::TaskReady
            | EventKind::RuntimeHelloReady
            | EventKind::ServiceReplyReady
            | EventKind::ServiceNegotiatedReady
            | EventKind::ServiceSelectedReady
            | EventKind::NamespaceReady
            | EventKind::PreemptReady
            | EventKind::TimerReady
            | EventKind::AuditReady
            | EventKind::ExcResumeReady
            | EventKind::PlatformSegmentsReady
            | EventKind::AuthorityReady
            | EventKind::FramesReady
            | EventKind::SelfRootReady
            | EventKind::FaultReady
            | EventKind::StateAuditReady
            | EventKind::ReclaimReady
            | EventKind::ResumeReady,
            None,
        ) => {
            return Err(format!("{event} event is missing its data field"));
        }
        (
            EventKind::MemoryReady
            | EventKind::MemoryError
            | EventKind::PhysicalMapReady
            | EventKind::PageTableReady
            | EventKind::TrapReady
            | EventKind::PageFaultReady
            | EventKind::BootstrapReady
            | EventKind::VspaceReady
            | EventKind::CowReady
            | EventKind::UserCowReady
            | EventKind::SyscallReady
            | EventKind::SyscallAuthReady
            | EventKind::ExcReady
            | EventKind::ExcPendingReady
            | EventKind::SyscallFastReady
            | EventKind::MultitaskReady
            | EventKind::ExcWaitReady
            | EventKind::HandleReady
            | EventKind::CapLifecycleReady
            | EventKind::MemsegReady
            | EventKind::IpcReady
            | EventKind::TaskReady
            | EventKind::RuntimeHelloReady
            | EventKind::ServiceReplyReady
            | EventKind::ServiceNegotiatedReady
            | EventKind::ServiceSelectedReady
            | EventKind::NamespaceReady
            | EventKind::PreemptReady
            | EventKind::TimerReady
            | EventKind::AuditReady
            | EventKind::ExcResumeReady
            | EventKind::PlatformSegmentsReady
            | EventKind::AuthorityReady
            | EventKind::FramesReady
            | EventKind::SelfRootReady
            | EventKind::FaultReady
            | EventKind::StateAuditReady
            | EventKind::ReclaimReady
            | EventKind::ResumeReady,
            Some(_),
        ) => {}
        (EventKind::PhysicalMapError | EventKind::PageTableError, Some(_))
        | (EventKind::PhysicalMapError | EventKind::PageTableError, None) => {}
        (_, Some(_)) => return Err(format!("{event} event must not carry a data field")),
        (_, None) => {}
    }

    Ok(Event {
        sequence,
        kind,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::validate_output;

    const VALID: &str = concat!(
        "SeaBIOS boot text\r\n",
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
        "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x200000;kend=0x300000\r\n",
        "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\r\n",
        "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0x200000\r\n",
        "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
        "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
        "PRIMA|v=1|seq=8|event=page_fault_ready|status=ok|data=addr=0x40000;err=0x0;class=not_present\n",
        "PRIMA|v=1|seq=9|event=bootstrap_ready|status=ok|data=task=1;caps=3;image_cap=0x100000001;namespace_cap=0x100000002;task_cap=0x100000003;image_base=0x200000;image_len=1048576;image_frames=1;free_frames=16;image_rights=Derive,SegMapR,SegMapX,SegDestroy;namespace_rights=Derive,NsResolve,NsBind,NsUnbind,NsMount,NsUnmount,NsDestroy;task_rights=Derive,TaskSend,TaskManageVSpace,TaskDestroy\r\n",
        "PRIMA|v=1|seq=10|event=vspace_ready|status=ok|data=va=0x8000000000;frame_a=0x0;frame_b=0x1000;read_before=0xaaaa000011112222;stale_read=0xaaaa000011112222;read_after=0xbbbb000033334444;translated=0x1000;wp_addr=0x8000000000;wp_err=0x3;class=write_protection\n",
        "PRIMA|v=1|seq=11|event=cow_ready|status=ok|data=va=0x80000000000;shared=0x6000;private=0xb000;err=0x3;class=write_protection;shared0=0xc0ffee0000000000;private0=0x80000000000;private1=0xc0ffee0000000001;private_last=0xc0ffee00000001ff;read_back=0x80000000000\n",
        "PRIMA|v=1|seq=12|event=user_cow_ready|status=ok|data=va=0xa0000002000;err=0x7;class=write_protection;shared0=0xc0ffee0000000000;private0=0x5a5a1234deadbeef;private1=0xc0ffee0000000001;receipt=0x5a5a1234deadbeef\n",
        "PRIMA|v=1|seq=13|event=syscall_ready|status=ok|data=calls=15;cs=0x33;cs_all=1;echo_ret=0xfedc45677654cdef;sum_ret=0x3f3f3f3f3f3f3f3f;copy_ret=0x5a5a1234deadbeef;fault_ret=0xfffffffffffffff2;invalid_ret=0xffffffffffffffea;bad_ret=0xffffffffffffffda;receipt0=0x5a5a1234deadbeef;receipt1=0xfedc45677654cdef;receipt2=0x3f3f3f3f3f3f3f3f;receipt3=0x5a5a1234deadbeef;receipt4=0xfffffffffffffff2;receipt5=0xffffffffffffffea;receipt6=0xffffffffffffffda\n",
        "PRIMA|v=1|seq=14|event=syscall_auth_ready|status=ok|data=cs_all=1;granted_ret=0x100000001;denied_ret=0xfffffffffffffff3;unknown_ret=0xfffffffffffffffe;receipt_granted=0x100000001;receipt_denied=0xfffffffffffffff3;receipt_unknown=0xfffffffffffffffe\n",
        "PRIMA|v=1|seq=15|event=exc_ready|status=ok|data=cs_all=1;sub_ret=0x1;dup_ret=0xfffffffffffffff3;kind_ret=0xffffffffffffffea;sub2_ret=0x2;unsub_ret=0x1;unsub2_ret=0xfffffffffffffff3;receipt_sub=0x1;receipt_dup=0xfffffffffffffff3;receipt_kind=0xffffffffffffffea;receipt_sub2=0x2;receipt_unsub=0x1;receipt_unsub2=0xfffffffffffffff3;audit_created=2;audit_subscribed=3;audit_unsubscribed=1\n",
        "PRIMA|v=1|seq=16|event=exc_pending_ready|status=ok|data=delivered=1;pending=1;message_id=1;payload=0x10001;task_ready_after=1;audit_raised=1\n",
        "PRIMA|v=1|seq=17|event=syscall_fast_ready|status=ok|data=cs=0x33;num=0x1;arg=0xfedcba987654321;ret=0xf012cba9789a4321;receipt=0xf012cba9789a4321;efer_sce=1;star=0x20000800000000;fmask=0x700;lstar_ok=1\n",
        "PRIMA|v=1|seq=18|event=multitask_ready|status=ok|data=cs1_before=0x33;cs1_after=0x33;cs2=0x33;task1_before=0x1;task1_after=0x1;task2=0x2;switches=0x2;task2_auth=0xfffffffffffffff3;task2_sub=0xfffffffffffffff3;receipt1_before=0x1;receipt1_yield=0x0;receipt1_after=0x1;receipt2_id=0x2;receipt2_auth=0xfffffffffffffff3;receipt2_sub=0xfffffffffffffff3\n",
        "PRIMA|v=1|seq=19|event=exc_wait_ready|status=ok|data=cs_wait=0x33;cs_trap=0x33;delivered=1;pending=1;woken=1;payload=0x20002;blocks=1;wait_receipt=0x20002;trap_receipt=0x1\n",
        "PRIMA|v=1|seq=20|event=handle_ready|status=ok|data=index_bits=32;generation_bits=32;image=0x100000001;namespace=0x100000002;task=0x100000003;page_fault=0x100000004;syscall_trap=0x100000005;malformed_rejected=1\n",
        "PRIMA|v=1|seq=21|event=cap_lifecycle_ready|status=ok|data=derive=0x100000006;escalate=0xfffffffffffffff3;drop=0x0;drop_again=0xfffffffffffffffe;reuse=0x200000006;stale=0xfffffffffffffffe;grant=0x200000006;drop_new=0x0;structural=0xfffffffffffffff3\n",
        "PRIMA|v=1|seq=22|event=memseg_ready|status=ok|data=create=0x300000006;map=0x0;zero=0x0;write=0xdec0de0012345678;unmap=0x0;destroy=0x0;stale=0xfffffffffffffff3;create2=0x100000007;map2=0x0;zero2=0x0;unmap2=0x0;destroy2=0x0\n",
        "PRIMA|v=1|seq=23|event=ipc_ready|status=ok|data=create=0x100000008;map1=0x0;send1=0x0;send2=0x0;recv1=0x51a5000000000001;cap0=0x100000008;magic=0x51a5000000000001;map2=0x0;cross=0xfffffffffffffff3;send_denied=0xfffffffffffffff3;recv2=0xd1ec700000000002;cap_count=1;queued=8;direct=9;blocks=12\n",
        "PRIMA|v=1|seq=24|event=task_ready|status=ok|data=stack_seg=0x100000009;create=0x10000000a;bad_entry=0xffffffffffffffea;no_x=0xfffffffffffffff3;stack_map=0x0;recv=0x3;deny=0xfffffffffffffff3;destroy=0x0;again=0xfffffffffffffff3;map_dead=0xfffffffffffffffe;map_own=0x0;map_value=0xc0ffee0000000000;map_other=0xfffffffffffffff3;task4_cap=0x20000000b;task4_stack=0x0;task4_recv=0x4;task4_map_dead=0xfffffffffffffffe;child=3;tasks=6;caps=63;root_cleared=1;grandchild=4;tasks_after=6;caps_after=63;root4_cleared=1\n",
        "PRIMA|v=1|seq=25|event=runtime_hello_ready|status=ok|data=map=0x0;magic=0x14d495250;opcode=0x300000001;length=0x100000018;features=0x1;required=0x1;max=4096\n",
        "PRIMA|v=1|seq=26|event=service_reply_ready|status=ok|data=client=0x100000018;stack=0x0;recv=0x8;map=0x0;reply=0x0;echo=0x38001474e4f50\n",
        "PRIMA|v=1|seq=27|event=service_negotiated_ready|status=ok|data=token=0x10000001c;service=0x10000001d;stack=0x0;svc_id=0x9;forward=0x0;done=0x5ec2000000000000;negotiated=0x1000008000001;negatives=0x100000000\n",
        "PRIMA|v=1|seq=28|event=service_selected_ready|status=ok|data=name=0x61746562;resolved=0x2;unknown=0x0;done=0x5ec2000000000000;negotiated_beta=0x1000004000001;alpha_ep=0x9;beta_ep=0xa;beta_self=0xa;alpha_marker=0x414c504841000001;beta_marker=0x4245544100000002;alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x8;client_stack_seg=0x10;reg_a_name=0x6168706c61;reg_b_name=0x61746562\n",
        "PRIMA|v=1|seq=29|event=namespace_ready|status=ok|data=namespaces=3;entries=1;binds=2;unbinds=1;mounts=2;unmounts=1;resolves=2;cycles_rejected=1;overlaps_rejected=1;invalid_path_rejected=1;acyclic_ok=2;create_ok=1;resolve_ok=1;overlap_rejected=1;cycle_rejected=1;evidence_ok=1\n",
        "PRIMA|v=1|seq=30|event=preempt_ready|status=ok|data=ticks=4;pre1=2;pre2=2;root1=0x1000;root2=0x2000;cr31=0x1000;cr32=0x2000;rsp1=0xffff800000010400;rsp2=0xffff800000020400;kstack1_top=0xffff800000014000;kstack2_top=0xffff800000024000;data1=0x3000;data2=0x4000;mark1=0x1;mark2=0x2\n",
        "PRIMA|v=1|seq=31|event=timer_ready|status=ok|data=now=4;timers=2;sets=3;fired=2;cancelled=1;destroyed=1;immediate_fired=1;already_cancelled=1;exactly_once_ok=1;sets_ok=1;cancel_ok=1;already_cancelled_ok=1;evidence_ok=1\n",
        "PRIMA|v=1|seq=32|event=audit_ready|status=ok|data=chain_len=2;derived=1;moves=2;created=1;destroyed=0;stale_ok=1;purity_ok=1;evidence_ok=1\n",
        "PRIMA|v=1|seq=33|event=exc_resume_ready|status=ok|data=sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=1\n",
        "PRIMA|v=1|seq=34|event=platform_segments_ready|status=ok|data=segments=13;first_id=2;last_id=14;roots=13;user_roots=11;kernel_roots=2;holders=0,0,1,1,1,1,2,2,2,1,1,2,2;task_caps=2;task_holders=1,2;mapped=11;direct=2;direct_ids=2,3;vspace=13;slot_base=32;task1_cap=0x10000002d;task2_cap=0x10000002e;caps=0x100000020,0x100000021,0x100000022,0x100000023,0x100000024,0x100000025,0x100000026,0x100000027,0x100000028,0x100000029,0x10000002a,0x10000002b,0x10000002c;frames=2,3,2,1,1,1,1,1,1,1,1,1,1;unified=1\n",
        "PRIMA|v=1|seq=35|event=authority_ready|status=ok|data=caps=3;segments=1;namespaces=1;booted=1;initial_status=ready;entry_in_image=1;root_ns=0x100000002;init_running=1\n",
        "PRIMA|v=1|seq=36|event=frames_ready|status=ok|data=universe=9;free=4;segment=2;table=1;image=2;conserved=1;disjoint=1\n",
        "PRIMA|v=1|seq=37|event=self_root_ready|status=ok|data=root=0x5b000;kernel_root=0xa000;image_table=0xb000;owned=1;pages=165;translated=0x200000\n",
        "PRIMA|v=1|seq=38|event=fault_ready|status=ok|data=task=7;before=1;after=3;raised=1;pending_source=1;pending_kind=1;precond_rejected=1;invariant_ok=1;unschedulable=1\n",
        "PRIMA|v=1|seq=39|event=state_audit_ready|status=ok|data=total=51;to_running=22;to_ready=21;to_blocked=5;to_faulted=3;used=131;cap=170\n",
        "PRIMA|v=1|seq=40|event=reclaim_ready|status=ok|data=task=6;negative_task=5;unrecoverable=1;reclaimed=1;negative_rejected=1;after_dead=1;pending_cleared=1;invariant_ok=1\n",
        "PRIMA|v=1|seq=41|event=resume_ready|status=ok|data=task=5;resumed=1;status_after=0;pending_cleared=1;frame_saved=1;frame_bytes_zeroed=1;frame_consumed=1;unrecoverable_rejected=1;reclaimed_after=1;invariant_ok=1\n",
        "PRIMA|v=1|seq=42|event=boot_complete|status=ok\r\n",
    );

    #[test]
    fn rejects_runtime_hello_corruption() {
        let output = VALID.replace("magic=0x14d495250", "magic=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("runtime_hello_ready wire data differs"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_reply_when_echo_is_missing() {
        let output = VALID.replace("echo=0x38001474e4f50", "echo=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("the client must write init's reply payload back"),
            "{error}"
        );
    }

    #[test]
    fn rejects_task_ready_when_create_is_an_errno() {
        let output = VALID.replace("create=0x10000000a", "create=0xfffffffffffffff3");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must be the manager cap"), "{error}");
    }

    #[test]
    fn rejects_task_ready_with_bad_entry_accepted() {
        let output = VALID.replace("bad_entry=0xffffffffffffffea", "bad_entry=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must be EINVAL"), "{error}");
    }

    #[test]
    fn rejects_task_ready_without_missing_segmapx_denial() {
        let output = VALID.replace("no_x=0xfffffffffffffff3", "no_x=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must be EACCES"), "{error}");
    }

    #[test]
    fn rejects_task_ready_when_the_child_reports_a_foreign_id() {
        let output = VALID.replace("recv=0x3", "recv=0x2");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must equal child"), "{error}");
    }

    #[test]
    fn rejects_task_ready_when_destroy_is_denied() {
        let output = VALID.replace("destroy=0x0;again=", "destroy=0xfffffffffffffff3;again=");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must be 0"), "{error}");
    }

    #[test]
    fn rejects_task_ready_when_a_stale_manager_cap_destroys_again() {
        let output = VALID.replace("again=0xfffffffffffffff3", "again=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("second destroy must be denied"), "{error}");
    }

    #[test]
    fn rejects_task_ready_when_the_destroyed_task_is_still_mappable() {
        let output = VALID.replace("map_dead=0xfffffffffffffffe", "map_dead=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("destroyed task's address space"), "{error}");
    }

    #[test]
    fn rejects_task_ready_when_the_child_stays_in_the_task_table() {
        // SPEC-070/072/073 + SPEC-087 + SPEC-088 + SPEC-092：合法值为 6（init + task 2 + 保留
        // 槽位的 Faulted 见证任务 + 存活的 Rust 客户端 + **两个**独立 Rust 服务端点）；7 表示子任务未清除。
        let output = VALID.replace(";tasks=6;", ";tasks=7;");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("must be gone from the task table"),
            "{error}"
        );
    }

    #[test]
    fn rejects_task_ready_without_root_cleanup() {
        let output = VALID.replace("root_cleared=1", "root_cleared=0");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("clear the destroyed task's scheduler root"),
            "{error}"
        );
    }

    #[test]
    fn rejects_task_ready_when_child_caps_stay_live() {
        let output = VALID.replace(";caps=63;root_cleared", ";caps=59;root_cleared");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("self/root-ns caps must be revoked"),
            "{error}"
        );
    }

    #[test]
    fn rejects_handle_ready_with_bare_index_encoding() {
        let output = VALID.replace("image=0x100000001", "image=0x1");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("initial handles mismatch"), "{error}");
    }

    #[test]
    fn rejects_cap_lifecycle_without_generation_bump() {
        // S2-X4：复用后旧句柄未失效（新句柄等于旧句柄）必须被拒。
        let output = VALID.replace("reuse=0x200000006", "reuse=0x100000006");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("same slot, next generation"), "{error}");
    }

    #[test]
    fn rejects_memseg_ready_with_dirty_recycled_frame() {
        // S2-X3：复用帧未清零（读回非 0）必须被拒。
        let output = VALID.replace("zero2=0x0", "zero2=0x1");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("recycled frame must be zeroed"), "{error}");
    }

    #[test]
    fn rejects_memseg_ready_with_stale_mapping_granted() {
        let output = VALID.replace("stale=0xfffffffffffffff3", "stale=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("mapping after memseg_destroy"), "{error}");
    }

    #[test]
    fn rejects_platform_segments_with_non_unified_frame_labels() {
        // 帧归属标签未与 Segment id 统一（仍是独立的 100+id 命名空间）必须被拒。
        let output = VALID.replace(";unified=1", ";unified=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("unified must be 1"), "{error}");
    }

    #[test]
    fn rejects_platform_segments_with_shifted_root_cap_slots() {
        let output = VALID.replace("caps=0x100000020,", "caps=0x100000021,");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("caps[0]"), "{error}");
    }

    #[test]
    fn rejects_platform_segments_with_wrong_frame_counts() {
        let output = VALID.replace(";frames=2,3,", ";frames=1,3,");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("expected"), "{error}");
    }

    #[test]
    fn rejects_platform_segments_without_direct_boundary() {
        let output = VALID.replace("direct_ids=2,3", "direct_ids=2,4");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("direct_ids"), "{error}");
    }

    #[test]
    fn rejects_ipc_ready_with_cross_task_map_granted() {
        // 未持目标 Task 的 ManageVSpace cap 却映射成功 → 必须被拒。
        let output = VALID.replace("cross=0xfffffffffffffff3", "cross=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must be EACCES"), "{error}");
    }

    #[test]
    fn rejects_ipc_ready_with_unauthorized_send_granted() {
        let output = VALID.replace("send_denied=0xfffffffffffffff3", "send_denied=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must be EACCES"), "{error}");
    }

    #[test]
    fn rejects_ipc_ready_without_capability_transfer() {
        let output = VALID.replace(";cap_count=1;", ";cap_count=0;");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("expected 1"), "{error}");
    }

    #[test]
    fn rejects_ipc_ready_without_the_direct_wake_path() {
        let output = VALID.replace(";direct=9;", ";direct=0;");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("expected 8/9/12"), "{error}");
    }

    #[test]
    fn rejects_service_negotiated_when_result_is_echoed() {
        // 把协商结果换成客户端 offer 的原值（max=4096）：服务端若只回显客户端 offer 而非用
        // 自己的 offer 协商，就会得到这个值——判决性检查必须拒绝。
        let output = VALID.replace("negotiated=0x1000008000001", "negotiated=0x1000010000001");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("server-side negotiation must yield max=2048"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_negotiated_without_negative_cases() {
        let output = VALID.replace("negatives=0x100000000", "negatives=0x0");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("MajorMismatch (1) and BadMagic (0)"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_routed_to_wrong_behavior() {
        // 把 `BETA` 的协商结果换成 `ALPHA` 的值（max=2048）：若服务管理器没按名路由到 `BETA`
        // 行为（而是走默认 `ALPHA`），就会得到这个值——判决性检查必须拒绝。
        let output = VALID.replace(
            "negotiated_beta=0x1000004000001",
            "negotiated_beta=0x1000008000001",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("routed BETA behavior must yield max=1024"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_name_is_hardcoded() {
        // 把解析结果与读回的服务名脱钩（resolved=RESOLVED 但 name 为空）：若管理器硬编码行为
        // 而不读共享段里的名字，名字就不会等于客户端写入的 `BETA`——必须拒绝。
        let output = VALID.replace(
            "data=name=0x61746562;resolved=0x2",
            "data=name=0x0;resolved=0x2",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("must read the requested service name"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_unknown_name_is_not_refused() {
        // 未注册名解析结果非 NONE：若管理器对任何名字都解析成功（无注册表），必须拒绝。
        let output = VALID.replace(";unknown=0x0;", ";unknown=0x2;");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("unregistered name must resolve to NONE"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_beta_ep_is_not_the_second_task() {
        // SPEC-092：把 `BETA` 端点 id 改成 `ALPHA` 端点（9）——即单宿主多行为（SPEC-090 的简化）
        // 而非独立第二端点；判决性检查必须拒绝。
        let output = VALID.replace(";alpha_ep=0x9;beta_ep=0xa;", ";alpha_ep=0x9;beta_ep=0x9;");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("must be routed to a *second, independent* service task"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_beta_self_is_not_the_routed_endpoint() {
        // SPEC-092：被记录的端点 id（第二个 `SERVICE_ID_TAG`）与写回结果的端点脱钩——即请求
        // 没送到 init 记录的那个端点；必须拒绝。
        let output = VALID.replace(";beta_ep=0xa;beta_self=0xa", ";beta_ep=0xa;beta_self=0x9");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("must route BETA to the endpoint it collected"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_both_endpoints_run_the_same_binary() {
        // SPEC-093：把 `BETA` 端点标记换成 `ALPHA` 的——即两个端点由**同一二进制**创建
        //（SPEC-092 的简化）；必须拒绝。
        let output = VALID.replace(
            "beta_marker=0x4245544100000002",
            "beta_marker=0x414c504841000001",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("second, distinct* image"), "{error}");
    }

    #[test]
    fn rejects_service_selected_when_the_two_endpoints_share_a_stack_segment() {
        // SPEC-093：两个端点的栈段 id 相同——即共享**物理栈帧**（SPEC-092 的简化）；必须拒绝。
        let output = VALID.replace(
            ";alpha_stack_seg=0x12;beta_stack_seg=0x11",
            ";alpha_stack_seg=0x11;beta_stack_seg=0x11",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("independent physical stack frames"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_alpha_endpoint_shares_the_client_stack_segment() {
        // SPEC-094：`ALPHA` 端点的栈段 id 等于客户端（task 8）的——即它仍沿用客户端 `Y[368]`
        // 的那条**共享物理栈帧**（SPEC-093 §5.1 的登记残留）；必须拒绝。
        let output = VALID.replace(
            ";alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x8;client_stack_seg=0x10",
            ";alpha_stack_seg=0x12;beta_stack_seg=0x11;client_ep=0x8;client_stack_seg=0x12",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("not the client's shared Y[368] Segment"),
            "{error}"
        );
    }

    #[test]
    fn rejects_service_selected_when_the_registry_is_not_built_from_self_reports() {
        // SPEC-095/101：注册表项 B 的名字与端点回报不符——即解析用的注册表不是端点回报所建
        //（退化回编译期常量）；必须拒绝。
        let output = VALID.replace(
            ";reg_a_name=0x6168706c61;reg_b_name=0x61746562",
            ";reg_a_name=0x6168706c61;reg_b_name=0x9",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("build the name registry from the endpoints' self-reports"),
            "{error}"
        );
    }

    #[test]
    fn rejects_ipc_ready_when_task2_reads_a_different_frame() {
        let output = VALID.replace("magic=0x51a5000000000001", "magic=0x51a5000000000002");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("same physical frame"), "{error}");
    }

    #[test]
    fn rejects_authority_ready_with_wrong_initial_status() {
        let output = VALID.replace(
            "initial_status=ready;entry_in_image=1",
            "initial_status=running;entry_in_image=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("initial_status must be ready"), "{error}");
    }

    #[test]
    fn rejects_authority_ready_without_booted_flag() {
        let output = VALID.replace("namespaces=1;booted=1", "namespaces=1;booted=0");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("booted/entry_in_image/init_running"),
            "{error}"
        );
    }

    /// 切片 AB：`exc_resume` 未把状态置为 `Ready`（成功动作 5 缺失）即拒绝。
    #[test]
    fn rejects_resume_without_the_ready_transition() {
        let output = VALID.replace("status_after=0", "status_after=3");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("status_after must be Ready"), "{error}");
    }

    /// 切片 AB：成功动作 1（清零已消费帧字节）缺失即拒绝。
    #[test]
    fn rejects_resume_without_zeroing_the_consumed_frame() {
        let output = VALID.replace("frame_bytes_zeroed=1", "frame_bytes_zeroed=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("frame_bytes_zeroed must be 1"), "{error}");
    }

    /// 切片 AB：§8.4 步骤 3 的帧消费清空缺失即拒绝。
    #[test]
    fn rejects_resume_without_frame_consumption_on_schedule() {
        let output = VALID.replace("frame_consumed=1", "frame_consumed=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("frame_consumed must be 1"), "{error}");
    }

    /// 切片 AB：前置 6（不可恢复种类必须被拒）缺失即拒绝。
    #[test]
    fn rejects_resume_without_the_unrecoverable_rejection() {
        let output = VALID.replace("unrecoverable_rejected=1", "unrecoverable_rejected=0");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("unrecoverable_rejected must be 1"),
            "{error}"
        );
    }

    /// 切片 AA：内核回收必须拒绝**可恢复**的 Faulted 任务。
    #[test]
    fn rejects_reclaim_of_a_recoverable_faulted_task() {
        let output = VALID.replace("negative_rejected=1", "negative_rejected=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("negative_rejected must be 1"), "{error}");
    }

    /// 切片 AA：回收后必须不再存活（否则等于没回收）。
    #[test]
    fn rejects_reclaim_that_leaves_the_task_alive() {
        let output = VALID.replace("after_dead=1", "after_dead=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("after_dead must be 1"), "{error}");
    }

    /// 切片 AA：§8.2 步骤 6 必须清空 `pending_exc`。
    #[test]
    fn rejects_reclaim_without_clearing_pending_exc() {
        let output = VALID.replace("pending_cleared=1", "pending_cleared=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("pending_cleared must be 1"), "{error}");
    }

    /// 切片 Z：任一迁移路径缺事件（分项为 0）即拒绝。
    #[test]
    fn rejects_state_audit_with_a_missing_transition_path() {
        let output = VALID.replace("to_blocked=5", "to_blocked=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must be non-zero"), "{error}");
    }

    /// 切片 Z：审计日志被打满（`used > cap`）即拒绝——余量枯竭会让后续受审计操作失败。
    #[test]
    fn rejects_state_audit_with_an_overflowed_log() {
        let output = VALID.replace("used=131;cap=170", "used=171;cap=170");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("overflowed"), "{error}");
    }

    /// 切片 Z：`total` 必须等于分项之和。
    #[test]
    fn rejects_state_audit_with_inconsistent_total() {
        let output = VALID.replace("total=51;", "total=52;");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must equal the per-status sum"), "{error}");
    }

    /// 切片 Y：§12.5 前置 4 未满足（`before` 不是 Running）即拒绝。
    #[test]
    fn rejects_fault_witness_starting_from_a_non_running_task() {
        let output = VALID.replace("task=7;before=1;after=3", "task=7;before=0;after=3");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("before must be Running"), "{error}");
    }

    /// 切片 Y：成功动作 3 未把 Task 置为 Faulted 即拒绝。
    #[test]
    fn rejects_fault_witness_without_the_faulted_transition() {
        let output = VALID.replace("before=1;after=3", "before=1;after=1");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("after must be Faulted"), "{error}");
    }

    /// 切片 Y：`pending_exc` 必须指向 page_fault 源与 PageFault 种类。
    #[test]
    fn rejects_fault_witness_with_wrong_pending_exc() {
        let output = VALID.replace(
            "pending_source=1;pending_kind=1",
            "pending_source=2;pending_kind=2",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("pending_exc"), "{error}");
    }

    /// 切片 Y：W9 双向不变量与"调度器不隐式恢复"必须为真。
    #[test]
    fn rejects_fault_witness_without_invariant_and_unschedulable_flags() {
        let invariant = VALID.replace("invariant_ok=1", "invariant_ok=0");
        assert!(
            validate_output(&invariant)
                .unwrap_err()
                .contains("invariant_ok must be 1")
        );
        let unschedulable = VALID.replace("unschedulable=1", "unschedulable=0");
        assert!(
            validate_output(&unschedulable)
                .unwrap_err()
                .contains("unschedulable must be 1")
        );
    }

    /// 切片 X：自建根的镜像链表帧必须仍是**本内核的保留页表帧**；归属丢失即拒绝。
    #[test]
    fn rejects_self_root_losing_page_table_ownership() {
        let output = VALID.replace("image_table=0xb000;owned=1", "image_table=0xb000;owned=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("owned must be 1"), "{error}");
    }

    /// 切片 X：运行中的地址空间必须把 `kernel_image_offset` 解析回内核镜像物理基址
    /// （与 `page_table_ready.translated` 的跨事件不变式）。
    #[test]
    fn rejects_self_root_translating_to_a_different_physical_base() {
        let output = VALID.replace(
            "pages=165;translated=0x200000",
            "pages=165;translated=0x300000",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("must equal page_table_ready.translated"),
            "{error}"
        );
    }

    /// 切片 X：必须报告非零的已物化镜像页数（全零等于什么都没映射）。
    #[test]
    fn rejects_self_root_with_no_materialized_image_pages() {
        let output = VALID.replace("owned=1;pages=165", "owned=1;pages=0");
        let error = validate_output(&output).unwrap_err();
        assert!(
            error.contains("non-zero materialized image page count"),
            "{error}"
        );
    }

    /// 切片 X：自建根帧必须是 4 KiB 对齐的非零物理帧。
    #[test]
    fn rejects_self_root_with_misaligned_root_frame() {
        let output = VALID.replace("root=0x5b000;", "root=0x5b008;");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("4KiB-aligned"), "{error}");
    }

    #[test]
    fn rejects_frames_ready_that_violates_w6_conservation() {
        let output = VALID.replace(
            "universe=9;free=4;segment=2;table=1;image=2;conserved=1",
            "universe=9;free=5;segment=2;table=1;image=2;conserved=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("violates W6"), "{error}");
    }

    #[test]
    fn rejects_frames_ready_without_conserved_flag() {
        let output = VALID.replace("image=2;conserved=1", "image=2;conserved=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("conserved must be 1"), "{error}");
    }

    #[test]
    fn rejects_frames_ready_without_disjointness() {
        let output = VALID.replace("conserved=1;disjoint=1", "conserved=1;disjoint=0");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("disjoint must be 1"), "{error}");
    }

    #[test]
    fn rejects_frames_ready_without_page_table_ownership() {
        let output = VALID.replace(
            "universe=9;free=4;segment=2;table=1;image=2;conserved=1",
            "universe=7;free=4;segment=2;table=0;image=1;conserved=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("non-zero image/table/segment"), "{error}");
    }

    #[test]
    fn accepts_expected_sequence_and_ignores_firmware_output() {
        assert_eq!(validate_output(VALID), Ok(()));
    }

    #[test]
    fn accepts_unterminated_non_prima_firmware_tail() {
        let output = format!("{VALID}firmware tail without a line terminator");
        assert_eq!(validate_output(&output), Ok(()));
    }

    #[test]
    fn rejects_prima_record_without_line_feed_terminator() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("LF or CRLF"), "{error}");
    }

    #[test]
    fn rejects_prima_record_with_cr_only_terminator() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\r",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("LF or CRLF"), "{error}");
    }

    #[test]
    fn rejects_prima_record_with_extra_cr_before_line_feed() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\r\r\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("LF or CRLF"), "{error}");
    }

    #[test]
    fn rejects_missing_event() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_out_of_order_event() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_duplicate_event() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n"
        );
        assert!(validate_output(&output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_malformed_prima_line() {
        let error = validate_output("PRIMA|v=1|seq=1|event=boot_start\n").unwrap_err();
        assert!(error.contains("five or six fields"), "{error}");
    }

    #[test]
    fn rejects_unsupported_version() {
        let error = validate_output("PRIMA|v=2|seq=1|event=boot_start|status=ok\n").unwrap_err();
        assert!(error.contains("version"), "{error}");
    }

    #[test]
    fn rejects_invalid_status() {
        let error = validate_output("PRIMA|v=1|seq=1|event=boot_start|status=error\n").unwrap_err();
        assert!(error.contains("event/status"), "{error}");
    }

    #[test]
    fn rejects_invalid_sequence_number() {
        let error = validate_output("PRIMA|v=1|seq=x|event=boot_start|status=ok\n").unwrap_err();
        assert!(error.contains("sequence number"), "{error}");
    }

    #[test]
    fn rejects_panic_event() {
        let output = format!("{}{}", VALID, "PRIMA|v=1|seq=7|event=panic|status=error\n");
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("panic"), "{error}");
    }

    #[test]
    fn rejects_memory_error_event() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=7|event=memory_error|status=error|data=err=EmptyImage\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("memory_error"), "{error}");
    }

    #[test]
    fn rejects_memory_ready_missing_data() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("missing its data field"), "{error}");
    }

    #[test]
    fn rejects_memory_ready_inconsistent_counts() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=5;free=10;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must equal"), "{error}");
    }

    #[test]
    fn rejects_memory_ready_unaligned_address() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x1;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("page-aligned"), "{error}");
    }

    #[test]
    fn rejects_non_data_event_carrying_data() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok|data=foo=1\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must not carry"), "{error}");
    }

    #[test]
    fn rejects_unknown_data_key() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000;bogus=1\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("unknown data key"), "{error}");
    }

    #[test]
    fn rejects_physical_map_error() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=7|event=physical_map_error|status=error\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("physical_map_error"), "{error}");
    }

    #[test]
    fn rejects_physical_map_ready_bad_address() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000001000;va=0xffff800000000000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must be >= offset"), "{error}");
    }

    #[test]
    fn rejects_page_table_error() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=7|event=page_table_error|status=error\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("page_table_error"), "{error}");
    }

    #[test]
    fn rejects_page_table_translated_mismatch() {
        // memory_ready.kstart (0x200000) 与 page_table_ready.translated (0xf0000000) 不一致：
        // translate(kernel_image_offset) 未恢复内核装载物理基址，必须拒绝。
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x200000;kend=0x300000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must equal memory_ready.kstart"), "{error}");
    }

    #[test]
    fn rejects_page_table_ready_zero() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0x0\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=3\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("must be > 0"), "{error}");
    }

    #[test]
    fn rejects_page_table_ready_missing_data() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok\n",
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("missing its data field"), "{error}");
    }

    #[test]
    fn rejects_gdt_tss_error() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=8|event=gdt_tss_error|status=error\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("gdt_tss_error"), "{error}");
    }

    #[test]
    fn rejects_idt_error() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=9|event=idt_error|status=error\n"
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("idt_error"), "{error}");
    }

    #[test]
    fn rejects_trap_wrong_seen() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x200000;kend=0x300000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0x200000\n",
            "PRIMA|v=1|seq=6|event=gdt_tss_ready|status=ok\n",
            "PRIMA|v=1|seq=7|event=trap_ready|status=ok|data=seen=5\n",
            "PRIMA|v=1|seq=8|event=boot_complete|status=ok\n",
        );
        let error = validate_output(output).unwrap_err();
        assert!(error.contains("seen must be 3"), "{error}");
    }

    #[test]
    fn rejects_page_fault_wrong_addr() {
        // addr wrong but err/class consistent
        let output = VALID.replace(
            "data=addr=0x40000;err=0x0;class=not_present",
            "data=addr=0x50000;err=0x0;class=not_present",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must equal probe address"), "{error}");
    }

    #[test]
    fn rejects_page_fault_wrong_class() {
        // class claim inconsistent with hardware absent-read evidence (replaces old "mapped" fiction)
        let output = VALID.replace(
            "data=addr=0x40000;err=0x0;class=not_present",
            "data=addr=0x40000;err=0x0;class=write_protection",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("class must be not_present"), "{error}");
    }

    #[test]
    fn rejects_page_fault_bad_err() {
        // err nonzero contradicts the absent-read supervisor fault signature (err stays 0x0 here)
        let output = VALID.replace(
            "data=addr=0x40000;err=0x0;class=not_present",
            "data=addr=0x40000;err=0x2;class=not_present",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("err must be 0x0"), "{error}");
    }

    #[test]
    fn rejects_page_fault_missing_data() {
        let output = VALID.replace(
            "|event=page_fault_ready|status=ok|data=addr=0x40000;err=0x0;class=not_present",
            "|event=page_fault_ready|status=ok",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("missing"), "{error}");
    }

    /// 切片 D 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const RESUME_DATA: &str =
        "sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=1";

    /// 切片 E 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const VSPACE_DATA: &str = "va=0x8000000000;frame_a=0x0;frame_b=0x1000;read_before=0xaaaa000011112222;stale_read=0xaaaa000011112222;read_after=0xbbbb000033334444;translated=0x1000;wp_addr=0x8000000000;wp_err=0x3;class=write_protection";

    #[test]
    fn rejects_vspace_wrong_va() {
        let output = VALID.replace(
            VSPACE_DATA,
            &VSPACE_DATA.replace("va=0x8000000000", "va=0x8000001000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must equal self-built VA"), "{error}");
    }

    #[test]
    fn rejects_vspace_wrong_read_after() {
        let output = VALID.replace(
            VSPACE_DATA,
            &VSPACE_DATA.replace(
                "read_after=0xbbbb000033334444",
                "read_after=0xaaaa000011112222",
            ),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("read_after"), "{error}");
    }

    #[test]
    fn rejects_vspace_bad_translation() {
        let output = VALID.replace(
            VSPACE_DATA,
            &VSPACE_DATA.replace(
                "read_before=0xaaaa000011112222",
                "read_before=0xDeadBeef00000000",
            ),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("read_before"), "{error}");
    }

    #[test]
    fn rejects_vspace_unknown_stale_value() {
        let output = VALID.replace(
            VSPACE_DATA,
            &VSPACE_DATA.replace("stale_read=0xaaaa000011112222", "stale_read=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("stale_read"), "{error}");
    }

    #[test]
    fn rejects_vspace_translated_frame_mismatch() {
        let output = VALID.replace(
            VSPACE_DATA,
            &VSPACE_DATA.replace("translated=0x1000", "translated=0x2000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("translated"), "{error}");
    }

    #[test]
    fn rejects_vspace_wrong_wp_err() {
        let output = VALID.replace(
            VSPACE_DATA,
            &VSPACE_DATA.replace("wp_err=0x3", "wp_err=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("wp_err must be 0x3"), "{error}");
    }

    #[test]
    fn rejects_vspace_wrong_wp_class() {
        let output = VALID.replace(
            VSPACE_DATA,
            &VSPACE_DATA.replace("class=write_protection", "class=not_present"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("class must be write_protection"), "{error}");
    }

    /// 切片 F 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const COW_DATA: &str = "va=0x80000000000;shared=0x6000;private=0xb000;err=0x3;class=write_protection;shared0=0xc0ffee0000000000;private0=0x80000000000;private1=0xc0ffee0000000001;private_last=0xc0ffee00000001ff;read_back=0x80000000000";

    #[test]
    fn rejects_cow_alias_frame() {
        let output = VALID.replace(
            COW_DATA,
            &COW_DATA.replace("private=0xb000", "private=0x6000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("private must differ from shared"), "{error}");
    }

    #[test]
    fn rejects_cow_shared_modified() {
        let output = VALID.replace(
            COW_DATA,
            &COW_DATA.replace("shared0=0xc0ffee0000000000", "shared0=0x80000000000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("shared0"), "{error}");
    }

    #[test]
    fn rejects_cow_incomplete_copy() {
        let output = VALID.replace(
            COW_DATA,
            &COW_DATA.replace("private1=0xc0ffee0000000001", "private1=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("private1"), "{error}");
    }

    #[test]
    fn rejects_cow_failed_retry() {
        let output = VALID.replace(
            COW_DATA,
            &COW_DATA.replace("read_back=0x80000000000", "read_back=0xc0ffee0000000000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("read_back"), "{error}");
    }

    #[test]
    fn rejects_cow_wrong_err() {
        let output = VALID.replace(COW_DATA, &COW_DATA.replace("err=0x3", "err=0x0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("err must be 0x3"), "{error}");
    }

    #[test]
    fn rejects_cow_wrong_va() {
        let output = VALID.replace(
            COW_DATA,
            &COW_DATA.replace("va=0x80000000000", "va=0x40000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must equal COW_VA"), "{error}");
    }

    /// 切片 G 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const USER_COW_DATA: &str = "va=0xa0000002000;err=0x7;class=write_protection;shared0=0xc0ffee0000000000;private0=0x5a5a1234deadbeef;private1=0xc0ffee0000000001;receipt=0x5a5a1234deadbeef";

    #[test]
    fn rejects_user_cow_not_from_cpl3() {
        // 去掉 US 位（0x3）⇒ 故障并非来自 CPL3，不能作为用户态递送证据。
        let output = VALID.replace(USER_COW_DATA, &USER_COW_DATA.replace("err=0x7", "err=0x3"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("US bit"), "{error}");
    }

    #[test]
    fn rejects_user_cow_shared_modified() {
        let output = VALID.replace(
            USER_COW_DATA,
            &USER_COW_DATA.replace("shared0=0xc0ffee0000000000", "shared0=0x5a5a1234deadbeef"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("shared0"), "{error}");
    }

    #[test]
    fn rejects_user_cow_failed_retry() {
        let output = VALID.replace(
            USER_COW_DATA,
            &USER_COW_DATA.replace("private0=0x5a5a1234deadbeef", "private0=0xc0ffee0000000000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("private0"), "{error}");
    }

    #[test]
    fn rejects_user_cow_missing_receipt() {
        let output = VALID.replace(
            USER_COW_DATA,
            &USER_COW_DATA.replace("receipt=0x5a5a1234deadbeef", "receipt=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("receipt"), "{error}");
    }

    #[test]
    fn rejects_user_cow_wrong_va() {
        let output = VALID.replace(
            USER_COW_DATA,
            &USER_COW_DATA.replace("va=0xa0000002000", "va=0xa0000000000"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("must equal user page"), "{error}");
    }

    /// 切片 H/I 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const SYSCALL_DATA: &str = "calls=15;cs=0x33;cs_all=1;echo_ret=0xfedc45677654cdef;sum_ret=0x3f3f3f3f3f3f3f3f;copy_ret=0x5a5a1234deadbeef;fault_ret=0xfffffffffffffff2;invalid_ret=0xffffffffffffffea;bad_ret=0xffffffffffffffda;receipt0=0x5a5a1234deadbeef;receipt1=0xfedc45677654cdef;receipt2=0x3f3f3f3f3f3f3f3f;receipt3=0x5a5a1234deadbeef;receipt4=0xfffffffffffffff2;receipt5=0xffffffffffffffea;receipt6=0xffffffffffffffda";

    #[test]
    fn rejects_syscall_not_from_cpl3() {
        let output = VALID.replace(SYSCALL_DATA, &SYSCALL_DATA.replace("cs=0x33", "cs=0x8"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("CPL3"), "{error}");
    }

    #[test]
    fn rejects_syscall_not_all_from_cpl3() {
        let output = VALID.replace(SYSCALL_DATA, &SYSCALL_DATA.replace("cs_all=1", "cs_all=0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("cs_all"), "{error}");
    }

    #[test]
    fn rejects_syscall_wrong_sum() {
        let output = VALID.replace(
            SYSCALL_DATA,
            &SYSCALL_DATA.replace("sum_ret=0x3f3f3f3f3f3f3f3f", "sum_ret=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("sum_ret"), "{error}");
    }

    #[test]
    fn rejects_syscall_wrong_copy_result() {
        let output = VALID.replace(
            SYSCALL_DATA,
            &SYSCALL_DATA.replace("copy_ret=0x5a5a1234deadbeef", "copy_ret=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("copy_ret"), "{error}");
    }

    #[test]
    fn rejects_syscall_fault_not_efault() {
        let output = VALID.replace(
            SYSCALL_DATA,
            &SYSCALL_DATA.replace("fault_ret=0xfffffffffffffff2", "fault_ret=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("EFAULT"), "{error}");
    }

    #[test]
    fn rejects_syscall_reserved_not_einval() {
        let output = VALID.replace(
            SYSCALL_DATA,
            &SYSCALL_DATA.replace("invalid_ret=0xffffffffffffffea", "invalid_ret=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("EINVAL"), "{error}");
    }

    #[test]
    fn rejects_syscall_illegal_number_accepted() {
        let output = VALID.replace(
            SYSCALL_DATA,
            &SYSCALL_DATA.replace("bad_ret=0xffffffffffffffda", "bad_ret=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("ENOSYS"), "{error}");
    }

    #[test]
    fn rejects_syscall_missing_receipt() {
        let output = VALID.replace(
            SYSCALL_DATA,
            &SYSCALL_DATA.replace("receipt3=0x5a5a1234deadbeef", "receipt3=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("receipt3"), "{error}");
    }

    #[test]
    fn rejects_syscall_wrong_call_count() {
        let output = VALID.replace(SYSCALL_DATA, &SYSCALL_DATA.replace("calls=15", "calls=14"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("calls"), "{error}");
    }

    /// 切片 K 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const AUTH_DATA: &str = "cs_all=1;granted_ret=0x100000001;denied_ret=0xfffffffffffffff3;unknown_ret=0xfffffffffffffffe;receipt_granted=0x100000001;receipt_denied=0xfffffffffffffff3;receipt_unknown=0xfffffffffffffffe";

    #[test]
    fn rejects_auth_not_all_cpl3() {
        let output = VALID.replace(AUTH_DATA, &AUTH_DATA.replace("cs_all=1", "cs_all=0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("CPL3"), "{error}");
    }

    #[test]
    fn rejects_auth_denied_as_granted() {
        let output = VALID.replace(
            AUTH_DATA,
            &AUTH_DATA.replace("denied_ret=0xfffffffffffffff3", "denied_ret=0x1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("EACCES"), "{error}");
    }

    #[test]
    fn rejects_auth_unknown_cap_granted() {
        let output = VALID.replace(
            AUTH_DATA,
            &AUTH_DATA.replace("unknown_ret=0xfffffffffffffffe", "unknown_ret=0x1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("ENOENT"), "{error}");
    }

    #[test]
    fn rejects_auth_wrong_grant() {
        let output = VALID.replace(
            AUTH_DATA,
            &AUTH_DATA.replace("granted_ret=0x1", "granted_ret=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("granted cap id"), "{error}");
    }

    #[test]
    fn rejects_auth_receipt_mismatch() {
        let output = VALID.replace(
            AUTH_DATA,
            &AUTH_DATA.replace("receipt_denied=0xfffffffffffffff3", "receipt_denied=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("receipt_denied"), "{error}");
    }

    /// 切片 L 的完整证据串（与 `VALID` 中的两行一致），便于替换构造反例。
    const EXC_DATA: &str = "cs_all=1;sub_ret=0x1;dup_ret=0xfffffffffffffff3;kind_ret=0xffffffffffffffea;sub2_ret=0x2;unsub_ret=0x1;unsub2_ret=0xfffffffffffffff3;receipt_sub=0x1;receipt_dup=0xfffffffffffffff3;receipt_kind=0xffffffffffffffea;receipt_sub2=0x2;receipt_unsub=0x1;receipt_unsub2=0xfffffffffffffff3;audit_created=2;audit_subscribed=3;audit_unsubscribed=1";
    const PENDING_DATA: &str =
        "delivered=1;pending=1;message_id=1;payload=0x10001;task_ready_after=1;audit_raised=1";

    #[test]
    fn rejects_exc_not_all_cpl3() {
        let output = VALID.replace(EXC_DATA, &EXC_DATA.replace("cs_all=1", "cs_all=0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("CPL3"), "{error}");
    }

    #[test]
    fn rejects_exc_duplicate_as_granted() {
        let output = VALID.replace(
            EXC_DATA,
            &EXC_DATA.replace("dup_ret=0xfffffffffffffff3", "dup_ret=0x1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("duplicate"), "{error}");
    }

    #[test]
    fn rejects_exc_kind_mismatch_accepted() {
        let output = VALID.replace(
            EXC_DATA,
            &EXC_DATA.replace("kind_ret=0xffffffffffffffea", "kind_ret=0x2"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("kind mismatch"), "{error}");
    }

    #[test]
    fn rejects_exc_wrong_audit_counts() {
        let output = VALID.replace(
            EXC_DATA,
            &EXC_DATA.replace("audit_subscribed=3", "audit_subscribed=2"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("audit counts"), "{error}");
    }

    #[test]
    fn rejects_exc_receipt_mismatch() {
        let output = VALID.replace(
            EXC_DATA,
            &EXC_DATA.replace("receipt_unsub2=0xfffffffffffffff3", "receipt_unsub2=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("receipts"), "{error}");
    }

    #[test]
    fn rejects_pending_without_direct_delivery() {
        let output = VALID.replace(
            PENDING_DATA,
            &PENDING_DATA.replace("pending=1", "pending=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("pending_return"), "{error}");
    }

    #[test]
    fn rejects_pending_wrong_payload() {
        let output = VALID.replace(
            PENDING_DATA,
            &PENDING_DATA.replace("payload=0x10001", "payload=0x2"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("payload"), "{error}");
    }

    #[test]
    fn rejects_pending_task_not_ready() {
        let output = VALID.replace(
            PENDING_DATA,
            &PENDING_DATA.replace("task_ready_after=1", "task_ready_after=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("Ready"), "{error}");
    }

    /// 切片 M 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const MULTITASK_DATA: &str = "cs1_before=0x33;cs1_after=0x33;cs2=0x33;task1_before=0x1;task1_after=0x1;task2=0x2;switches=0x2;task2_auth=0xfffffffffffffff3;task2_sub=0xfffffffffffffff3;receipt1_before=0x1;receipt1_yield=0x0;receipt1_after=0x1;receipt2_id=0x2;receipt2_auth=0xfffffffffffffff3;receipt2_sub=0xfffffffffffffff3";

    #[test]
    fn rejects_multitask_not_all_cpl3() {
        let output = VALID.replace(
            MULTITASK_DATA,
            &MULTITASK_DATA.replace("cs2=0x33", "cs2=0x8"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("CPL3"), "{error}");
    }

    #[test]
    fn rejects_multitask_task2_granted() {
        let output = VALID.replace(
            MULTITASK_DATA,
            &MULTITASK_DATA.replace("task2_auth=0xfffffffffffffff3", "task2_auth=0x1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("EACCES"), "{error}");
    }

    #[test]
    fn rejects_multitask_wrong_task_id() {
        let output = VALID.replace(
            MULTITASK_DATA,
            &MULTITASK_DATA.replace("task2=0x2", "task2=0x1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("task ids"), "{error}");
    }

    #[test]
    fn rejects_multitask_missing_switch_back() {
        let output = VALID.replace(
            MULTITASK_DATA,
            &MULTITASK_DATA.replace("task1_after=0x1", "task1_after=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("task ids"), "{error}");
    }

    #[test]
    fn rejects_multitask_wrong_switch_count() {
        let output = VALID.replace(
            MULTITASK_DATA,
            &MULTITASK_DATA.replace("switches=0x2", "switches=0x1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("switches"), "{error}");
    }

    #[test]
    fn rejects_multitask_receipt_mismatch() {
        let output = VALID.replace(
            MULTITASK_DATA,
            &MULTITASK_DATA.replace("receipt1_yield=0x0", "receipt1_yield=0x9"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("receipts"), "{error}");
    }

    /// 切片 N 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const EXC_WAIT_DATA: &str = "cs_wait=0x33;cs_trap=0x33;delivered=1;pending=1;woken=1;payload=0x20002;blocks=1;wait_receipt=0x20002;trap_receipt=0x1";

    #[test]
    fn rejects_exc_wait_not_from_cpl3() {
        let output = VALID.replace(
            EXC_WAIT_DATA,
            &EXC_WAIT_DATA.replace("cs_trap=0x33", "cs_trap=0x8"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("CPL3"), "{error}");
    }

    #[test]
    fn rejects_exc_wait_no_pending() {
        let output = VALID.replace(
            EXC_WAIT_DATA,
            &EXC_WAIT_DATA.replace("pending=1", "pending=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("pending_return"), "{error}");
    }

    #[test]
    fn rejects_exc_wait_wrong_waker() {
        let output = VALID.replace(EXC_WAIT_DATA, &EXC_WAIT_DATA.replace("woken=1", "woken=2"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("blocked waiter"), "{error}");
    }

    #[test]
    fn rejects_exc_wait_wrong_payload() {
        let output = VALID.replace(
            EXC_WAIT_DATA,
            &EXC_WAIT_DATA.replace("payload=0x20002", "payload=0x10001"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("payload"), "{error}");
    }

    #[test]
    fn rejects_exc_wait_no_block() {
        let output = VALID.replace(
            EXC_WAIT_DATA,
            &EXC_WAIT_DATA.replace("blocks=1", "blocks=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("blocking wait"), "{error}");
    }

    #[test]
    fn rejects_exc_wait_receipt_mismatch() {
        let output = VALID.replace(
            EXC_WAIT_DATA,
            &EXC_WAIT_DATA.replace("wait_receipt=0x20002", "wait_receipt=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("receipts"), "{error}");
    }

    /// 切片 J 的完整证据串（与 `VALID` 中的一行一致），便于替换构造反例。
    const FAST_DATA: &str = "cs=0x33;num=0x1;arg=0xfedcba987654321;ret=0xf012cba9789a4321;receipt=0xf012cba9789a4321;efer_sce=1;star=0x20000800000000;fmask=0x700;lstar_ok=1";

    #[test]
    fn rejects_fast_not_from_cpl3() {
        let output = VALID.replace(FAST_DATA, &FAST_DATA.replace("cs=0x33", "cs=0x8"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("CPL3"), "{error}");
    }

    #[test]
    fn rejects_fast_missing_sce() {
        let output = VALID.replace(FAST_DATA, &FAST_DATA.replace("efer_sce=1", "efer_sce=0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("EFER.SCE"), "{error}");
    }

    #[test]
    fn rejects_fast_wrong_star() {
        let output = VALID.replace(
            FAST_DATA,
            &FAST_DATA.replace("star=0x20000800000000", "star=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("STAR"), "{error}");
    }

    #[test]
    fn rejects_fast_lstar_mismatch() {
        let output = VALID.replace(FAST_DATA, &FAST_DATA.replace("lstar_ok=1", "lstar_ok=0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("LSTAR"), "{error}");
    }

    #[test]
    fn rejects_fast_wrong_ret() {
        let output = VALID.replace(
            FAST_DATA,
            &FAST_DATA.replace("ret=0xf012cba9789a4321", "ret=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("round trip"), "{error}");
    }

    #[test]
    fn rejects_fast_receipt_mismatch() {
        let output = VALID.replace(
            FAST_DATA,
            &FAST_DATA.replace("receipt=0xf012cba9789a4321", "receipt=0x0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("receipt"), "{error}");
    }

    #[test]
    fn rejects_exc_resume_wrong_sentinel() {
        let output = VALID.replace(
            RESUME_DATA,
            "sentinel=0x123456789abcdef1;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("sentinel"), "{error}");
    }

    #[test]
    fn rejects_exc_resume_wrong_sentinel2() {
        let output = VALID.replace(
            RESUME_DATA,
            "sentinel=0x123456789abcdef0;sentinel2=0x0;resumed=1;rsp_ok=1;flags_ok=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("sentinel2"), "{error}");
    }

    #[test]
    fn rejects_exc_resume_not_resumed() {
        let output = VALID.replace(
            RESUME_DATA,
            "sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=0;rsp_ok=1;flags_ok=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("resumed must be 1"), "{error}");
    }

    #[test]
    fn rejects_exc_resume_rsp_not_ok() {
        let output = VALID.replace(
            RESUME_DATA,
            "sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=0;flags_ok=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("rsp_ok must be 1"), "{error}");
    }

    #[test]
    fn rejects_exc_resume_flags_not_ok() {
        let output = VALID.replace(
            RESUME_DATA,
            "sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=0",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("flags_ok must be 1"), "{error}");
    }

    #[test]
    fn rejects_exc_resume_missing_data() {
        let output = VALID.replace(
            "|event=exc_resume_ready|status=ok|data=sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=1",
            "|event=exc_resume_ready|status=ok",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("missing"), "{error}");
    }

    /// SPEC-081：`namespace_ready` 的判决性数据（与 `VALID` 中的一致）。
    const NAMESPACE_DATA: &str = "namespaces=3;entries=1;binds=2;unbinds=1;mounts=2;unmounts=1;resolves=2;cycles_rejected=1;overlaps_rejected=1;invalid_path_rejected=1;acyclic_ok=2;create_ok=1;resolve_ok=1;overlap_rejected=1;cycle_rejected=1;evidence_ok=1";

    #[test]
    fn rejects_namespace_without_a_rejected_cycle() {
        let output = VALID.replace(
            NAMESPACE_DATA,
            &NAMESPACE_DATA
                .replace("cycles_rejected=1", "cycles_rejected=0")
                .replace("cycle_rejected=1", "cycle_rejected=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("cycles_rejected must be 1"), "{error}");
    }

    #[test]
    fn rejects_namespace_when_the_cycle_dfs_did_not_run_per_mount() {
        let output = VALID.replace(
            NAMESPACE_DATA,
            &NAMESPACE_DATA.replace("acyclic_ok=2", "acyclic_ok=1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("acyclic_ok must be 2"), "{error}");
    }

    #[test]
    fn rejects_namespace_when_a_path_audit_event_is_missing() {
        let output = VALID.replace(
            NAMESPACE_DATA,
            &NAMESPACE_DATA.replace("mounts=2", "mounts=1"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("mounts must be 2"), "{error}");
    }

    #[test]
    fn rejects_namespace_with_wrong_live_object_count() {
        let output = VALID.replace(
            NAMESPACE_DATA,
            &NAMESPACE_DATA.replace("namespaces=3", "namespaces=4"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("namespaces must be 3"), "{error}");
    }

    #[test]
    fn rejects_namespace_without_the_ring3_evidence_flag() {
        let output = VALID.replace(
            NAMESPACE_DATA,
            &NAMESPACE_DATA.replace("evidence_ok=1", "evidence_ok=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("evidence_ok must be 1"), "{error}");
    }

    /// SPEC-082：`timer_ready` 的判决性数据（与 `VALID` 中的一致）。
    const TIMER_DATA: &str = "now=4;timers=2;sets=3;fired=2;cancelled=1;destroyed=1;immediate_fired=1;already_cancelled=1;exactly_once_ok=1;sets_ok=1;cancel_ok=1;already_cancelled_ok=1;evidence_ok=1";

    #[test]
    fn rejects_timer_without_a_clock_advanced_by_ticks() {
        let output = VALID.replace(TIMER_DATA, &TIMER_DATA.replace("now=4", "now=0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("now must be > 0"), "{error}");
    }

    #[test]
    fn rejects_timer_without_an_immediate_fire() {
        let output = VALID.replace(
            TIMER_DATA,
            &TIMER_DATA.replace("immediate_fired=1", "immediate_fired=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("immediate_fired must be 1"), "{error}");
    }

    #[test]
    fn rejects_timer_when_exactly_once_is_not_established() {
        let output = VALID.replace(
            TIMER_DATA,
            &TIMER_DATA.replace("exactly_once_ok=1", "exactly_once_ok=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("exactly_once_ok must be 1"), "{error}");
    }

    #[test]
    fn rejects_timer_with_a_missing_fired_count() {
        let output = VALID.replace(TIMER_DATA, &TIMER_DATA.replace("fired=2", "fired=1"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("fired must be 2"), "{error}");
    }

    #[test]
    fn rejects_timer_without_the_state_return_receipt() {
        let output = VALID.replace(
            TIMER_DATA,
            &TIMER_DATA.replace("already_cancelled_ok=1", "already_cancelled_ok=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("already_cancelled_ok must be 1"), "{error}");
    }

    #[test]
    fn rejects_timer_missing_data() {
        let output = VALID.replace(
            "|event=timer_ready|status=ok|data=now=4",
            "|event=timer_ready|status=ok|data=timers=2",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("now"), "{error}");
    }

    #[test]
    fn rejects_namespace_missing_data() {
        let output = VALID.replace(
            "|event=namespace_ready|status=ok|data=namespaces=3",
            "|event=namespace_ready|status=ok|data=entries=1",
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("namespaces"), "{error}");
    }

    const AUDIT_DATA: &str =
        "chain_len=2;derived=1;moves=2;created=1;destroyed=0;stale_ok=1;purity_ok=1;evidence_ok=1";

    #[test]
    fn rejects_audit_without_the_position_history() {
        let output = VALID.replace(AUDIT_DATA, &AUDIT_DATA.replace("moves=2", "moves=1"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("moves: expected 2"), "{error}");
    }

    #[test]
    fn rejects_audit_that_mutates_the_log() {
        let output = VALID.replace(
            AUDIT_DATA,
            &AUDIT_DATA.replace("purity_ok=1", "purity_ok=0"),
        );
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("purity_ok: expected 1"), "{error}");
    }

    #[test]
    fn rejects_audit_that_cannot_query_a_dead_target() {
        let output = VALID.replace(AUDIT_DATA, &AUDIT_DATA.replace("stale_ok=1", "stale_ok=0"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("stale_ok: expected 1"), "{error}");
    }
}
