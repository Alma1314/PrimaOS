//! L1: actual Linux service, VFS and authorized copy, with user TSC receipts.
use crate::{image, linux, qemu, stage6};
use std::fs;

const HEADER: &str =
    "S7LIBOS|samples=64;operation=read;fd=3;bytes=17;boundary=user-wrapper;verified=1";

fn samples(output: &str) -> Result<Vec<u64>, String> {
    if output.contains("PANIC") || output.contains("|event=panic|") {
        return Err("LibOS benchmark panicked".into());
    }
    let headers: Vec<_> = output
        .lines()
        .filter(|s| s.starts_with("S7LIBOS|"))
        .collect();
    if headers != [HEADER] {
        return Err("invalid LibOS benchmark header".into());
    }
    let mut durations = Vec::new();
    let mut previous = 0;
    for (index, row) in output
        .lines()
        .filter_map(|s| s.strip_prefix("S7LTIME|"))
        .enumerate()
    {
        let fields: Vec<_> = row.split(';').collect();
        if index >= 64
            || fields.len() != 7
            || fields[0] != index.to_string()
            || fields[3] != "17"
            || fields[4] != "68656c6c6f2066726f6d207072696d610a"
            || fields[5..] != ["165", "165"]
        {
            return Err("invalid LibOS read receipt".into());
        }
        let begin = fields[1].parse::<u64>().map_err(|_| "invalid read begin")?;
        let end = fields[2].parse::<u64>().map_err(|_| "invalid read end")?;
        if begin == 0 || begin < previous || end < begin {
            return Err("LibOS clock regressed".into());
        }
        durations.push(end - begin);
        previous = end;
    }
    if durations.len() != 64 {
        return Err("LibOS benchmark needs 64 samples".into());
    }
    Ok(durations)
}

fn validate_prima(output: &str) -> Result<String, String> {
    let observed = stage6::parse_prima_cases(output, &["L1"])?;
    if !observed[0].stderr.is_empty() {
        return Err("LibOS benchmark wrote stderr".into());
    }
    let stdout = std::str::from_utf8(&observed[0].stdout).map_err(|_| "invalid benchmark UTF8")?;
    let durations = samples(stdout)?;
    let clocks: Vec<_> = output
        .lines()
        .filter(|s| s.starts_with("S7CLOCK|"))
        .collect();
    let hz = clocks
        .first()
        .filter(|_| clocks.len() == 1)
        .and_then(|s| s.strip_prefix("S7CLOCK|hz="))
        .and_then(|s| s.strip_suffix(";source=pit-tsc;verified=1"))
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|hz| *hz > 0)
        .ok_or("invalid LibOS clock calibration")?;
    let tasks: Vec<_> = output
        .lines()
        .filter_map(|s| s.strip_prefix("S6TASK|cookie="))
        .collect();
    if tasks.len() != 1 {
        return Err("LibOS benchmark requires one Task".into());
    }
    let operations: Vec<_> = output
        .lines()
        .filter(|s| s.starts_with("S7LOPS|"))
        .collect();
    if operations
        != [format!(
            "S7LOPS|read=64;seek=64;cookie={};verified=1",
            tasks[0]
        )]
    {
        return Err("LibOS benchmark operations lack raw audit".into());
    }
    // open, 64 seeks/reads, close, bounded stdout writes, exit_group.
    let requests = 131 + stdout.len().div_ceil(4096);
    let counts: Vec<_> = output
        .lines()
        .filter(|s| s.starts_with("S6COUNTS|"))
        .collect();
    if counts
        != [format!(
            "S6COUNTS|trapped={requests};delivered={requests};replied={};aborted=0;orphaned=0;cancelled=1",
            requests - 1
        )]
    {
        return Err("LibOS benchmark request count mismatch".into());
    }
    Ok(qemu::benchmark_summary(
        64,
        hz,
        "user-wrapper",
        &["user-total"],
        &mut [durations],
    ))
}

pub(super) fn test() -> Result<(), String> {
    let (_, input) = stage6::build()?;
    let directory = input.parent().unwrap();
    let probe = directory.join("stage7-libos-bench.elf");
    let bytes = prima_user::linux::LIBOS_BENCH;
    fs::write(&probe, bytes).map_err(|e| e.to_string())?;
    let service =
        fs::read(directory.join("stage6-native-service.elf")).map_err(|e| e.to_string())?;
    if !service.windows(bytes.len()).any(|b| b == bytes) {
        return Err("LibOS service lacks exact benchmark ELF".into());
    }
    let reference = linux::libos_reference(&input, &probe)?;
    fs::write(directory.join("S7LIBOS-reference.log"), &reference).map_err(|e| e.to_string())?;
    let status: Vec<_> = reference
        .lines()
        .filter(|s| s.starts_with("S7LREF|"))
        .collect();
    if status != ["S7LREF|status=0"] {
        return Err("Linux benchmark failed".into());
    }
    let mut cycles = samples(&reference)?;
    cycles.sort_unstable();
    println!(
        "PASS S7LIBOS reference=linux samples=64 same_elf=1 user-total_cycles={}/{}",
        cycles[31], cycles[63]
    );
    let images = image::package_images(|path| {
        image::write_manifest_plain(
            path,
            vec![
                (prima_abi::boot_image::KIND_INIT, stage6::launcher(12)),
                (1, service.clone()),
            ],
        )
    })?;
    for round in 1..=2 {
        let output = qemu::run_bios_linux_capture(&images.bios)?;
        fs::write(directory.join(format!("S7LIBOS-{round}.log")), &output)
            .map_err(|e| e.to_string())?;
        println!("PASS S7LIBOS round={round} {}", validate_prima(&output)?);
    }
    Ok(())
}

#[test]
fn libos_benchmark_validates_variable_samples_and_rejects_missing_data_and_audit() {
    let rows: Vec<_> = (0..64)
        .map(|i| {
            format!(
                "S7LTIME|{i};{};{};17;68656c6c6f2066726f6d207072696d610a;165;165\n",
                1000 + i * 100,
                1012 + i * 101
            )
        })
        .collect();
    let stdout = format!("{HEADER}\n{}", rows.concat());
    let make = |stdout: &str| {
        let mut hex = String::new();
        use std::fmt::Write;
        for byte in stdout.bytes() {
            write!(hex, "{byte:02x}").unwrap();
        }
        let n = 131 + stdout.len().div_ceil(4096);
        format!(
            "S7CLOCK|hz=1000000000;source=pit-tsc;verified=1\nS6BEGIN|case=L1\nS6TASK|cookie=4294967299\nS6WRITE|fd=1;hex={hex}\nS6COUNTS|trapped={n};delivered={n};replied={};aborted=0;orphaned=0;cancelled=1\nS7LOPS|read=64;seek=64;cookie=4294967299;verified=1\nS6EXIT|case=L1;status=0;requests={n};unsupported=0\nS6LINUX|cases=1;verified=1\n",
            n - 1
        )
    };
    let good = make(&stdout);
    assert!(
        validate_prima(&good)
            .unwrap()
            .contains("user-total_ns=43/75")
    );
    for bad in [
        stdout.replace(HEADER, ""),
        format!("{HEADER}\n{stdout}"),
        stdout.replace(&rows[1], ""),
        format!("{stdout}{}", rows[0]),
        stdout.replace(&rows[1], &rows[0]),
        stdout.replace(";68656c", ";00656c"),
        stdout.replace(";165", ";164"),
        stdout.replace(";17;", ";16;"),
        stdout.replace("1000;1012", "0;1012"),
        stdout.replace("1100;1113", "999;1113"),
        stdout.replace("1100;1113", "1100;1099"),
        stdout.replace(";17;68656c", ";17;extra;68656c"),
    ] {
        assert!(validate_prima(&make(&bad)).is_err());
    }
    for (old, new) in [
        ("hz=1000000000", "hz=0"),
        ("source=pit-tsc", "source=none"),
        ("S7CLOCK|", "S7MISSING|"),
        ("read=64", "read=63"),
        ("seek=64", "seek=63"),
        (
            "cookie=4294967299;verified=1",
            "cookie=8589934595;verified=1",
        ),
        ("aborted=0", "aborted=1"),
        ("cancelled=1", "cancelled=0"),
        (";status=0;", ";status=1;"),
        ("unsupported=0", "unsupported=1"),
    ] {
        assert!(validate_prima(&good.replace(old, new)).is_err());
    }
    assert!(validate_prima(&format!("S6PANIC|test\n{good}")).is_err());
    assert!(validate_prima(&format!("S7CLOCK|hz=1;source=pit-tsc;verified=1\n{good}")).is_err());
    assert!(
        validate_prima(&format!(
            "S7LOPS|read=64;seek=64;cookie=4294967299;verified=1\n{good}"
        ))
        .is_err()
    );
}
