#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EventKind {
    BootStart,
    SerialReady,
    BootComplete,
    Panic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Event {
    sequence: u32,
    kind: EventKind,
}

const EXPECTED_EVENTS: [Event; 3] = [
    Event {
        sequence: 1,
        kind: EventKind::BootStart,
    },
    Event {
        sequence: 2,
        kind: EventKind::SerialReady,
    },
    Event {
        sequence: 3,
        kind: EventKind::BootComplete,
    },
];

pub(crate) fn validate_output(output: &str) -> Result<(), String> {
    let mut events = Vec::new();

    for raw_line in output.lines() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.starts_with("PRIMA|") {
            events.push(parse_line(line)?);
        }
    }

    if events.iter().any(|event| event.kind == EventKind::Panic) {
        return Err("kernel emitted panic event".to_owned());
    }

    if events.as_slice() != EXPECTED_EVENTS.as_slice() {
        return Err(format!(
            "unexpected PRIMA event sequence: expected {EXPECTED_EVENTS:?}, got {events:?}"
        ));
    }

    Ok(())
}

fn parse_line(line: &str) -> Result<Event, String> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() != 5 {
        return Err(format!(
            "PRIMA line must contain five fields, got {}: {line}",
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

    let kind = match (event, status) {
        ("boot_start", "ok") => EventKind::BootStart,
        ("serial_ready", "ok") => EventKind::SerialReady,
        ("boot_complete", "ok") => EventKind::BootComplete,
        ("panic", "error") => EventKind::Panic,
        _ => return Err(format!("unsupported event/status pair: {event}/{status}")),
    };

    Ok(Event { sequence, kind })
}

#[cfg(test)]
mod tests {
    use super::validate_output;

    const VALID: &str = concat!(
        "SeaBIOS boot text\r\n",
        "PRIMA|v=1|seq=1|event=boot_start|status=ok\r\n",
        "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
        "PRIMA|v=1|seq=3|event=boot_complete|status=ok\r\n",
    );

    #[test]
    fn accepts_expected_sequence_and_ignores_firmware_output() {
        assert_eq!(validate_output(VALID), Ok(()));
    }

    #[test]
    fn rejects_missing_event() {
        let output = concat!(
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=3|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_out_of_order_event() {
        let output = concat!(
            "PRIMA|v=1|seq=2|event=serial_ready|status=ok\n",
            "PRIMA|v=1|seq=1|event=boot_start|status=ok\n",
            "PRIMA|v=1|seq=3|event=boot_complete|status=ok\n",
        );
        assert!(validate_output(output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_duplicate_event() {
        let output = format!(
            "{}{}",
            VALID, "PRIMA|v=1|seq=3|event=boot_complete|status=ok\n"
        );
        assert!(validate_output(&output).unwrap_err().contains("sequence"));
    }

    #[test]
    fn rejects_malformed_prima_line() {
        let error = validate_output("PRIMA|v=1|seq=1|event=boot_start\n").unwrap_err();
        assert!(error.contains("five fields"));
    }

    #[test]
    fn rejects_unsupported_version() {
        let error = validate_output("PRIMA|v=2|seq=1|event=boot_start|status=ok\n").unwrap_err();
        assert!(error.contains("version"));
    }

    #[test]
    fn rejects_invalid_status() {
        let error = validate_output("PRIMA|v=1|seq=1|event=boot_start|status=error\n").unwrap_err();
        assert!(error.contains("event/status"));
    }

    #[test]
    fn rejects_invalid_sequence_number() {
        let error = validate_output("PRIMA|v=1|seq=x|event=boot_start|status=ok\n").unwrap_err();
        assert!(error.contains("sequence number"));
    }

    #[test]
    fn rejects_panic_event() {
        let output = format!("{}{}", VALID, "PRIMA|v=1|seq=4|event=panic|status=error\n");
        assert!(validate_output(&output).unwrap_err().contains("panic"));
    }
}
