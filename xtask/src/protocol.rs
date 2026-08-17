#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    BootStart,
    SerialReady,
    MemoryReady,
    PhysicalMapReady,
    PageTableReady,
    BootComplete,
    Panic,
    MemoryError,
    PhysicalMapError,
    PageTableError,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Event {
    sequence: u32,
    kind: EventKind,
    data: Option<String>,
}

const EXPECTED_EVENTS: [(u32, EventKind); 6] = [
    (1, EventKind::BootStart),
    (2, EventKind::SerialReady),
    (3, EventKind::MemoryReady),
    (4, EventKind::PhysicalMapReady),
    (5, EventKind::PageTableReady),
    (6, EventKind::BootComplete),
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
        )
    }) {
        return Err(
            "kernel emitted panic, memory_error, physical_map_error, or page_table_error event"
                .to_owned(),
        );
    }

    let observed: Vec<(u32, EventKind)> = events
        .iter()
        .map(|event| (event.sequence, event.kind))
        .collect();
    if observed != EXPECTED_EVENTS {
        return Err(format!(
            "unexpected PRIMA event sequence: expected {EXPECTED_EVENTS:?}, got {observed:?}"
        ));
    }

    let memory = events
        .iter()
        .find(|event| event.kind == EventKind::MemoryReady)
        .expect("memory_ready is in the expected sequence");
    validate_memory_data(memory.data.as_deref())?;

    let phys = events
        .iter()
        .find(|event| event.kind == EventKind::PhysicalMapReady)
        .expect("physical_map_ready is in the expected sequence");
    validate_physical_map_data(phys.data.as_deref())?;

    let page_table = events
        .iter()
        .find(|event| event.kind == EventKind::PageTableReady)
        .expect("page_table_ready is in the expected sequence");
    validate_page_table_data(page_table.data.as_deref())
}

fn validate_page_table_data(data: Option<&str>) -> Result<(), String> {
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
    Ok(())
}

fn validate_memory_data(data: Option<&str>) -> Result<(), String> {
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
        ("boot_complete", "ok") => EventKind::BootComplete,
        ("memory_error", "error") => EventKind::MemoryError,
        ("physical_map_error", "error") => EventKind::PhysicalMapError,
        ("page_table_error", "error") => EventKind::PageTableError,
        ("panic", "error") => EventKind::Panic,
        _ => return Err(format!("unsupported event/status pair: {event}/{status}")),
    };

    match (kind, &data) {
        (
            EventKind::MemoryReady
            | EventKind::MemoryError
            | EventKind::PhysicalMapReady
            | EventKind::PageTableReady,
            None,
        ) => {
            return Err(format!("{event} event is missing its data field"));
        }
        (
            EventKind::MemoryReady
            | EventKind::MemoryError
            | EventKind::PhysicalMapReady
            | EventKind::PageTableReady,
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
        "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\r\n",
        "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\r\n",
        "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0xf0000000\r\n",
        "PRIMA|v=1|seq=6|event=boot_complete|status=ok\r\n",
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
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
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
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
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
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
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
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
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
    fn rejects_page_table_ready_zero() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=3|event=memory_ready|status=ok|data=usable=16;image=1;free=16;regions=1;kstart=0x0;kend=0x1000\n",
            "PRIMA|v=1|seq=4|event=physical_map_ready|status=ok|data=offset=0xffff800000000000;va=0xffff800000001000\n",
            "PRIMA|v=1|seq=5|event=page_table_ready|status=ok|data=translated=0x0\n",
            "PRIMA|v=1|seq=6|event=boot_complete|status=ok\n",
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
}
