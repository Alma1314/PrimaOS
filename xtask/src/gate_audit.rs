//! Scoped validator for the opt-in audit-stop image. Ordinary suites still
//! require success exit=33; this runner requires the exact exhausted-stop trace.
pub const SITES: [&str; 7] = [
    "int80-entry",
    "dispatch",
    "int80-return",
    "fast-return",
    "pick-next",
    "resume",
    "irq",
];

pub fn selected_site() -> Result<&'static str, String> {
    if !cfg!(feature = "gate-audit-probe") {
        return Err("use cargo run -p xtask --features gate-audit-probe -- test-gate-audit".into());
    }
    let site = option_env!("PRIMA_GATE_AUDIT_SITE")
        .filter(|site| SITES.contains(site))
        .ok_or_else(|| format!("set PRIMA_GATE_AUDIT_SITE at build time to one of {SITES:?}"))?;
    if std::env::var("PRIMA_GATE_AUDIT_SITE").ok().as_deref() != Some(site) {
        return Err(
            "runtime probe site differs from the compiled image; rebuild with cargo".into(),
        );
    }
    Ok(site)
}

pub fn validate_output(output: &str, site: &str) -> Result<(), String> {
    if !SITES.contains(&site) {
        return Err("unknown audit-stop site".into());
    }
    let lines: Vec<_> = output.lines().map(str::trim_end).collect();
    let records: Vec<_> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.starts_with("S6AUDIT|"))
        .collect();
    if records.len() != 2 {
        return Err("audit-stop requires exactly armed and stopped records".into());
    }
    let (armed_index, armed) = records[0];
    let (stopped_index, stopped) = records[1];
    let armed = armed
        .strip_prefix("S6AUDIT|armed ")
        .ok_or("missing audit-stop armed record")?;
    if stopped.strip_prefix("S6AUDIT|stopped ") != Some(armed) {
        return Err("audit-stop diagnostic changed or boundary escaped".into());
    }
    let mut fields = armed.split(';');
    if fields.next() != Some(format!("site={site}").as_str()) {
        return Err("audit-stop reached the wrong boundary".into());
    }
    let mut values = [0u64; 5];
    for (value, key) in
        values
            .iter_mut()
            .zip(["owner=", "required=", "used=", "reserved=", "unchanged="])
    {
        *value = fields
            .next()
            .and_then(|field| field.strip_prefix(key))
            .and_then(|text| text.parse().ok())
            .ok_or("malformed audit-stop diagnostic")?;
    }
    let [owner, required, used, reserved, unchanged] = values;
    if fields.next().is_some()
        || owner >> 32 == 0
        || owner as u32 == 0
        || required != 2
        || used == 0
        || used.checked_add(reserved) != Some(16383)
        || unchanged != 1
    {
        return Err("invalid audit-stop identity, capacity or preserved state".into());
    }
    // Nothing can execute past this guard. Reject other panics, later progress,
    // duplicate terminal events and missing/wrong panic source locations.
    let tail = &lines[armed_index..];
    if stopped_index != armed_index + 2
        || tail.len() != 4
        || !tail[1].starts_with("PRIMA|")
        || !tail[1].ends_with("|event=panic|status=error")
        || !tail[3]
            .strip_prefix("[panic] kernel/src/gate_audit.rs:")
            .is_some_and(|line| line.parse::<u32>().is_ok_and(|line| line > 0))
        || lines[..armed_index]
            .iter()
            .any(|line| line.contains("|event=panic|") || line.contains("|event=boot_complete|"))
    {
        return Err("audit-stop did not take the nominated terminal guard path".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(site: &str) -> String {
        let record =
            format!("site={site};owner=4294967297;required=2;used=100;reserved=16283;unchanged=1");
        format!(
            "boot\nS6AUDIT|armed {record}\nPRIMA|v=1|seq=9|event=panic|status=error\nS6AUDIT|stopped {record}\n[panic] kernel/src/gate_audit.rs:107\n"
        )
    }

    #[test]
    fn accepts_each_nominated_guard_with_matching_preserved_diagnostic() {
        for site in SITES {
            assert!(validate_output(&trace(site), site).is_ok());
        }
    }

    #[test]
    fn rejects_escape_unrelated_panic_stale_identity_and_partial_evidence() {
        let good = trace("irq");
        for bad in [
            good.replace("S6AUDIT|stopped", "S6AUDIT|escaped"),
            good.replace("gate_audit.rs:107", "syscall.rs:107"),
            good.replace(
                "|event=panic|status=error",
                "|event=boot_complete|status=ok",
            ),
            good.replace("owner=4294967297", "owner=1"),
            good.replace("required=2", "required=1"),
            good.replace("reserved=16283", "reserved=16282"),
            good.replace("unchanged=1", "unchanged=0"),
            good.replace("site=irq", "site=resume"),
            good.replacen("used=100", "used=101", 1),
            good.replace("PRIMA|v=1|seq=9|event=panic|status=error\n", ""),
            good.clone() + "S6AUDIT|escaped\n",
            good.clone() + "user resumed\n",
            "PRIMA|v=1|seq=8|event=panic|status=error\n".to_owned() + &good,
            good.lines().take(3).collect::<Vec<_>>().join("\n"),
        ] {
            assert!(validate_output(&bad, "irq").is_err(), "accepted {bad}");
        }
        assert!(validate_output(&good, "unknown").is_err());
    }
}
