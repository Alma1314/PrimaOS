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
    ExcResumeReady,
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

fn cow_pattern(index: u64) -> u64 {
    0xC0FF_EE00_0000_0000 | index
}

/// 切片 G：ring3 用户任务的镜像常量，与 `kernel/src/user.rs` 一致（跨 crate 需人工同步）。
const USER_BASE_VA: u64 = 0x0000_0A00_0000_0000;
const USER_X_VA: u64 = USER_BASE_VA + 0x2000;
const USER_STORE_VALUE: u64 = 0x5A5A_1234_DEAD_BEEF;
/// #PF error code 的 US 位（bit 2）：置位表示故障来自 CPL3。
const PF_US: u64 = 1 << 2;

/// 切片 H/I：syscall ABI 的镜像常量，与 `kernel/src/syscall.rs` 一致（跨 crate 需人工同步）。
const SYS_BAD: u64 = 0xDEAD;
const ECHO_MASK: u64 = 0xFFFF_0000_FFFF_0000;
const SYS_ECHO_ARG: u64 = 0x0123_4567_89AB_CDEF;
const SUM_EXPECTED: u64 = 0x3F3F_3F3F_3F3F_3F3F;
/// 负 errno 二补码（EINVAL=-22、EFAULT=-14、ENOSYS=-38）。
const EFAULT: u64 = 0xFFFF_FFFF_FFFF_FFF2;
const EINVAL: u64 = 0xFFFF_FFFF_FFFF_FFEA;
const ENOSYS: u64 = 0xFFFF_FFFF_FFFF_FFDA;
const USER_CS: u64 = 0x33;

fn echo_expected() -> u64 {
    SYS_ECHO_ARG ^ ECHO_MASK
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    sequence: u32,
    kind: EventKind,
    data: Option<String>,
}

const EXPECTED_EVENTS: [(u32, EventKind); 15] = [
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
    (14, EventKind::ExcResumeReady),
    (15, EventKind::BootComplete),
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

    if let Some(resume) = events
        .iter()
        .find(|event| event.kind == EventKind::ExcResumeReady)
    {
        validate_exc_resume_data(resume.data.as_deref())?;
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

    if calls != 6 {
        return Err(format!(
            "syscall_ready calls ({calls}) must be 6 — ECHO + SUM4 + COPY_IN ok + COPY_IN fault + reserved + unknown"
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
            "image_cap" => image_cap = Some(parse_count(value)?),
            "namespace_cap" => namespace_cap = Some(parse_count(value)?),
            "task_cap" => task_cap = Some(parse_count(value)?),
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
        || image_cap != Some(1)
        || namespace_cap != Some(2)
        || task_cap != Some(3)
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
        || namespace_rights != Some("all")
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
        ("exc_resume_ready", "ok") => EventKind::ExcResumeReady,
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
            | EventKind::ExcResumeReady,
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
            | EventKind::ExcResumeReady,
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
        "PRIMA|v=1|seq=9|event=bootstrap_ready|status=ok|data=task=1;caps=3;image_cap=1;namespace_cap=2;task_cap=3;image_base=0x200000;image_len=1048576;image_frames=1;free_frames=16;image_rights=Derive,SegMapR,SegMapX,SegDestroy;namespace_rights=all;task_rights=Derive,TaskSend,TaskManageVSpace,TaskDestroy\r\n",
        "PRIMA|v=1|seq=10|event=vspace_ready|status=ok|data=va=0x8000000000;frame_a=0x0;frame_b=0x1000;read_before=0xaaaa000011112222;stale_read=0xaaaa000011112222;read_after=0xbbbb000033334444;translated=0x1000;wp_addr=0x8000000000;wp_err=0x3;class=write_protection\n",
        "PRIMA|v=1|seq=11|event=cow_ready|status=ok|data=va=0x80000000000;shared=0x6000;private=0xb000;err=0x3;class=write_protection;shared0=0xc0ffee0000000000;private0=0x80000000000;private1=0xc0ffee0000000001;private_last=0xc0ffee00000001ff;read_back=0x80000000000\n",
        "PRIMA|v=1|seq=12|event=user_cow_ready|status=ok|data=va=0xa0000002000;err=0x7;class=write_protection;shared0=0xc0ffee0000000000;private0=0x5a5a1234deadbeef;private1=0xc0ffee0000000001;receipt=0x5a5a1234deadbeef\n",
        "PRIMA|v=1|seq=13|event=syscall_ready|status=ok|data=calls=6;cs=0x33;cs_all=1;echo_ret=0xfedc45677654cdef;sum_ret=0x3f3f3f3f3f3f3f3f;copy_ret=0x5a5a1234deadbeef;fault_ret=0xfffffffffffffff2;invalid_ret=0xffffffffffffffea;bad_ret=0xffffffffffffffda;receipt0=0x5a5a1234deadbeef;receipt1=0xfedc45677654cdef;receipt2=0x3f3f3f3f3f3f3f3f;receipt3=0x5a5a1234deadbeef;receipt4=0xfffffffffffffff2;receipt5=0xffffffffffffffea;receipt6=0xffffffffffffffda\n",
        "PRIMA|v=1|seq=14|event=exc_resume_ready|status=ok|data=sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=1\n",
        "PRIMA|v=1|seq=15|event=boot_complete|status=ok\r\n",
    );

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
    const SYSCALL_DATA: &str = "calls=6;cs=0x33;cs_all=1;echo_ret=0xfedc45677654cdef;sum_ret=0x3f3f3f3f3f3f3f3f;copy_ret=0x5a5a1234deadbeef;fault_ret=0xfffffffffffffff2;invalid_ret=0xffffffffffffffea;bad_ret=0xffffffffffffffda;receipt0=0x5a5a1234deadbeef;receipt1=0xfedc45677654cdef;receipt2=0x3f3f3f3f3f3f3f3f;receipt3=0x5a5a1234deadbeef;receipt4=0xfffffffffffffff2;receipt5=0xffffffffffffffea;receipt6=0xffffffffffffffda";

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
        let output = VALID.replace(SYSCALL_DATA, &SYSCALL_DATA.replace("calls=6", "calls=5"));
        let error = validate_output(&output).unwrap_err();
        assert!(error.contains("calls"), "{error}");
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
}
