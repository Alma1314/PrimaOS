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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    sequence: u32,
    kind: EventKind,
    data: Option<String>,
}

const EXPECTED_EVENTS: [(u32, EventKind); 11] = [
    (1, EventKind::BootStart),
    (2, EventKind::SerialReady),
    (3, EventKind::MemoryReady),
    (4, EventKind::PhysicalMapReady),
    (5, EventKind::PageTableReady),
    (6, EventKind::GdtTssReady),
    (7, EventKind::TrapReady),
    (8, EventKind::PageFaultReady),
    (9, EventKind::BootstrapReady),
    (10, EventKind::ExcResumeReady),
    (11, EventKind::BootComplete),
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

    if let Some(resume) = events
        .iter()
        .find(|event| event.kind == EventKind::ExcResumeReady)
    {
        validate_exc_resume_data(resume.data.as_deref())?;
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
        "PRIMA|v=1|seq=10|event=exc_resume_ready|status=ok|data=sentinel=0x123456789abcdef0;sentinel2=0xfedcba987654321;resumed=1;rsp_ok=1;flags_ok=1\n",
        "PRIMA|v=1|seq=11|event=boot_complete|status=ok\r\n",
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
