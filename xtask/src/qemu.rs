use crate::protocol;
use std::sync::mpsc;
use std::{
    env,
    ffi::OsString,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SUCCESS_EXIT_CODE: i32 = 33;
const PANIC_EXIT_CODE: i32 = 35;
const TEST_TIMEOUT: Duration = Duration::from_secs(30);
const QEMU_INSTALL_COMMAND: &str =
    "winget install --id SoftwareFreedomConservancy.QEMU --exact --version 11.0.50";

pub(crate) struct CapturedOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

#[derive(Debug)]
struct CapturedStreams {
    stdout: String,
    stderr: String,
}

/// Minimal seam for the timeout cleanup state machine.
trait ChildControl {
    fn kill(&mut self) -> io::Result<()>;
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>>;
    fn wait(&mut self) -> io::Result<ExitStatus>;
}

impl ChildControl for Child {
    fn kill(&mut self) -> io::Result<()> {
        Child::kill(self)
    }

    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        Child::try_wait(self)
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        Child::wait(self)
    }
}

pub(crate) fn run_bios_interactive(image: &Path) -> Result<(), String> {
    let qemu = locate_qemu()?;
    let status = Command::new(&qemu)
        .args(qemu_args(image))
        .status()
        .map_err(|error| format!("failed to start {}: {error}", qemu.display()))?;
    validate_exit_code(status.code(), SUCCESS_EXIT_CODE)
}

pub(crate) fn run_bios_test(image: &Path) -> Result<(), String> {
    let output = run_captured(image)?;
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_output(&output.stdout).map_err(|error| with_output(error, &output))?;
    protocol::validate_service_traces(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_native_app_trace(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_stage5_capacity(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_stage5_tty(&output.stdout).map_err(|error| with_output(error, &output))?;
    protocol::validate_user_fault_trace(&output.stdout).map_err(|error| with_output(error, &output))
}

/// D14 has its own scoped witness; the supervisor intentionally terminates
/// ALPHA and exits through a capability-authorized debug port before init's
/// ordinary acceptance. It must not be counted as a normal boot-suite round.
pub(crate) fn run_bios_domains(image: &Path, round: u32) -> Result<(), String> {
    let output = run_captured(image)?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    fs_write_domain_log(root, round, &output.stdout)?;
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|e| with_output(e, &output))?;
    protocol::validate_domains(&output.stdout).map_err(|e| with_output(e, &output))?;
    println!("PASS S6DOM round={round} roots=2 shared_log=3 denied=3 survivor=1");
    Ok(())
}
fn fs_write_domain_log(root: &Path, round: u32, text: &str) -> Result<(), String> {
    std::fs::write(
        root.join(format!("target/prima-os/S6DOM-{round}.log")),
        text,
    )
    .map_err(|e| format!("could not preserve domain witness: {e}"))
}

pub(crate) fn run_bios_gate(image: &Path, round: u32) -> Result<(), String> {
    let output = run_captured(image)?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::write(
        root.join(format!("target/prima-os/S6GATE-{round}.log")),
        &output.stdout,
    )
    .map_err(|e| format!("could not preserve gate witness: {e}"))?;
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|e| with_output(e, &output))?;
    if option_env!("PRIMA_GATE_CASE") == Some("C1") {
        let summary = validate_cow_bench(&output.stdout).map_err(|e| with_output(e, &output))?;
        println!("PASS S7COW round={round} {summary}");
        return Ok(());
    }
    if option_env!("PRIMA_GATE_CASE") == Some("I1") {
        let summary = validate_ipc_bench(&output.stdout).map_err(|e| with_output(e, &output))?;
        println!("PASS S7IPC round={round} {summary}");
        return Ok(());
    }
    if option_env!("PRIMA_GATE_CASE") == Some("N1") {
        let summary = validate_native_bench(&output.stdout).map_err(|e| with_output(e, &output))?;
        println!("PASS S7NATIVE round={round} {summary}");
        return Ok(());
    }
    if matches!(option_env!("PRIMA_GATE_CASE"), Some("G4" | "G5")) {
        let user_mode = option_env!("PRIMA_GATE_CASE") == Some("G5");
        let summary =
            validate_gate_bench(&output.stdout, user_mode).map_err(|e| with_output(e, &output))?;
        let prefix = if user_mode { "S7" } else { "S6" };
        println!("PASS {prefix}BENCH round={round} {summary}");
        return Ok(());
    }
    if let Some(case) = option_env!("PRIMA_GATE_CASE") {
        validate_gate_matrix(&output.stdout, case).map_err(|e| with_output(e, &output))?;
        println!("PASS S6MATRIX round={round} case={case} verified=1 state=1 audit=1");
        return Ok(());
    }
    validate_gate_output(&output.stdout).map_err(|e| with_output(e, &output))?;
    println!(
        "PASS S6GATE round={round} int80=1 fast=1 registers=1 signals=1 retry=1 output_failures=3 duplicate=1 orphan=1 admission=1 rejected=9 audit=1"
    );
    Ok(())
}

fn validate_gate_bench(output: &str, user_mode: bool) -> Result<String, String> {
    let count: usize = if user_mode { 128 } else { 1000 };
    let prefix = if user_mode { "S7" } else { "S6" };
    let boundary = if user_mode {
        "user-wrapper"
    } else {
        "pre-iret"
    };
    if output.contains("|event=panic|") {
        return Err("benchmark panicked".into());
    }
    let history: Vec<_> = output
        .lines()
        .filter(|s| s.starts_with(&format!("{prefix}HISTORY|")))
        .collect();
    let expected_history = format!(
        "{prefix}HISTORY|deliveries={count};audit_roots={count};dedicated_slots_saved={count};verified=1"
    );
    if history != [expected_history.as_str()] {
        return Err("benchmark requires one verified audit-backed history record".into());
    }
    let headers: Vec<_> = output
        .lines()
        .filter(|s| s.starts_with(&format!("{prefix}BENCH|")))
        .collect();
    let Some(header) = headers.first().filter(|_| headers.len() == 1) else {
        return Err("benchmark needs one calibration record".into());
    };
    let hz = header
        .strip_prefix(&format!("{prefix}BENCH|samples={count};hz="))
        .and_then(|s| s.strip_suffix(&format!(";boundary={boundary};verified=1")))
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|hz| *hz > 0)
        .ok_or("invalid benchmark calibration")?;
    let names: &[&str] = if user_mode {
        &[
            "trap-queue",
            "queue-deliver",
            "deliver-reply",
            "reply-resume",
            "kernel-total",
            "user-entry",
            "user-return",
            "user-total",
        ]
    } else {
        &[
            "trap-queue",
            "queue-deliver",
            "deliver-reply",
            "reply-resume",
            "total",
        ]
    };
    let mut durations = vec![Vec::new(); names.len()];
    let mut previous = 0;
    for (index, line) in output
        .lines()
        .filter_map(|s| s.strip_prefix(&format!("{prefix}TIME|")))
        .enumerate()
    {
        let values: Vec<u64> = line
            .split(';')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| "invalid benchmark timestamp")?;
        if values.len() != if user_mode { 8 } else { 6 }
            || values[0] != index as u64
            || values[1] == 0
            || values[1] < previous
            || !values[1..].windows(2).all(|w| w[0] <= w[1])
        {
            return Err("unordered or incomplete benchmark sample".into());
        }
        let kernel = if user_mode { 2 } else { 1 };
        for phase in 0..4 {
            durations[phase].push(values[kernel + phase + 1] - values[kernel + phase]);
        }
        durations[4].push(values[kernel + 4] - values[kernel]);
        if user_mode {
            durations[5].push(values[2] - values[1]);
            durations[6].push(values[7] - values[6]);
            durations[7].push(values[7] - values[1]);
        }
        previous = *values.last().unwrap();
    }
    if durations[0].len() != count {
        return Err(format!("benchmark requires exactly {count} samples"));
    }
    Ok(benchmark_summary(
        count,
        hz,
        boundary,
        names,
        &mut durations,
    ))
}

fn benchmark_summary(
    count: usize,
    hz: u64,
    boundary: &str,
    names: &[&str],
    durations: &mut [Vec<u64>],
) -> String {
    let mut summary = format!("samples={count} hz={hz} boundary={boundary}");
    for (name, samples) in names.iter().zip(durations) {
        samples.sort_unstable();
        // Nearest-rank median/p99; integer nanoseconds from the measured PIT/TSC ratio.
        let ns = |cycles: u64| u128::from(cycles) * 1_000_000_000 / u128::from(hz);
        summary.push_str(&format!(
            " {name}_ns={}/{}",
            ns(samples[(count * 50).div_ceil(100) - 1]),
            ns(samples[(count * 99).div_ceil(100) - 1])
        ));
    }
    summary
}

fn validate_native_bench(output: &str) -> Result<String, String> {
    if output.contains("|event=panic|") {
        return Err("native benchmark panicked".into());
    }
    let headers: Vec<_> = output
        .lines()
        .filter(|s| s.starts_with("S7NATIVE|"))
        .collect();
    let Some(header) = headers.first().filter(|_| headers.len() == 1) else {
        return Err("native benchmark requires one verified header".into());
    };
    let hz = header
        .strip_prefix("S7NATIVE|samples=128;hz=")
        .and_then(|s| s.strip_suffix(";boundary=user-wrapper;number=7;task=2;gate=0;verified=1"))
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|hz| *hz > 0)
        .ok_or("invalid native benchmark calibration or identity")?;
    let mut durations = Vec::new();
    let mut previous = 0;
    for (index, line) in output
        .lines()
        .filter_map(|s| s.strip_prefix("S7NTIME|"))
        .enumerate()
    {
        let values: Vec<u64> = line
            .split(';')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| "invalid native benchmark receipt")?;
        if values.len() != 4
            || values[0] != index as u64
            || values[1] == 0
            || values[1] < previous
            || values[2] < values[1]
            || values[3] != 2
        {
            return Err("unordered, incomplete or incorrect native benchmark receipt".into());
        }
        durations.push(values[2] - values[1]);
        previous = values[2];
    }
    if durations.len() != 128 {
        return Err("native benchmark requires exactly 128 samples".into());
    }
    Ok(benchmark_summary(
        128,
        hz,
        "user-wrapper",
        &["user-total"],
        &mut [durations],
    ))
}

fn validate_cow_bench(output: &str) -> Result<String, String> {
    if output.contains("|event=panic|") {
        return Err("COW benchmark panicked".into());
    }
    let headers: Vec<_> = output.lines().filter(|s| s.starts_with("S7COW|")).collect();
    let Some(header) = headers.first().filter(|_| headers.len() == 1) else {
        return Err("COW benchmark requires one verified header".into());
    };
    let hz = header
        .strip_prefix("S7COW|samples=64;hz=")
        .and_then(|s| {
            s.strip_suffix(";boundary=user-store;task=2;faults=64;observer=checked;verified=1")
        })
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|hz| *hz > 0)
        .ok_or("invalid COW benchmark calibration or boundary")?;
    let mut durations = Vec::new();
    let mut frames = Vec::new();
    let mut previous = 0;
    for (index, line) in output
        .lines()
        .filter_map(|s| s.strip_prefix("S7CTIME|"))
        .enumerate()
    {
        let v: Vec<u64> = line
            .split(';')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| "invalid COW benchmark receipt")?;
        if index >= 64
            || v.len() != 8
            || v[0] != index as u64
            || v[1] == 0
            || v[1] < previous
            || v[2] < v[1]
            || v[3] != 65536 + index as u64
            || v[4] != 0x0000_0b00_0040_0000 + index as u64 * 4096
            || v[5] != 7
        {
            return Err("unordered, incomplete or incorrect COW benchmark receipt".into());
        }
        for frame in &v[6..8] {
            if *frame == 0 || *frame & !0x000f_ffff_ffff_f000 != 0 || frames.contains(frame) {
                return Err("invalid or aliased COW benchmark frame".into());
            }
            frames.push(*frame);
        }
        durations.push(v[2] - v[1]);
        previous = v[2];
    }
    if durations.len() != 64 {
        return Err("COW benchmark requires exactly 64 samples".into());
    }
    Ok(benchmark_summary(
        64,
        hz,
        "user-store",
        &["user-total"],
        &mut [durations],
    ))
}

#[test]
fn cow_benchmark_requires_fresh_frames_and_user_write_fault_receipts() {
    let header = "S7COW|samples=64;hz=1000000000;boundary=user-store;task=2;faults=64;observer=checked;verified=1\n";
    let rows: Vec<_> = (0..64u64)
        .map(|i| {
            format!(
                "S7CTIME|{i};{};{};{};{};7;{};{}\n",
                1000 + i * 100,
                1012 + i * 101,
                65536 + i,
                0x0000_0b00_0040_0000 + i * 4096,
                4096 + i * 8192,
                8192 + i * 8192
            )
        })
        .collect();
    let good = format!("{header}{}", rows.concat());
    assert!(
        validate_cow_bench(&good)
            .unwrap()
            .contains("user-total_ns=43/75")
    );
    assert!(validate_native_bench(&good).is_err());
    assert!(validate_ipc_bench(&good).is_err());
    assert!(validate_gate_bench(&good, true).is_err());
    for (old, new) in [
        ("samples=64", "samples=63"),
        ("hz=1000000000", "hz=0"),
        ("user-store", "user-wrapper"),
        ("task=2", "task=1"),
        ("faults=64", "faults=63"),
        ("observer=checked", "observer=none"),
        ("verified=1", "verified=0"),
    ] {
        assert!(validate_cow_bench(&good.replace(old, new)).is_err());
    }
    let fields: Vec<_> = rows[1]
        .trim()
        .strip_prefix("S7CTIME|")
        .unwrap()
        .split(';')
        .collect();
    for (field, bad) in [
        (0, "0"),
        (1, "0"),
        (1, "999"),
        (2, "1099"),
        (3, "65536"),
        (4, "0"),
        (5, "3"),
        (5, "0"),
        (6, "0"),
        (6, "4097"),
        (6, "4503599627370496"),
        (6, "4096"),
        (6, "8192"),
        (7, "12288"),
        (7, "8192"),
        (7, "no-number"),
    ] {
        let mut f = fields.clone();
        f[field] = bad;
        assert!(
            validate_cow_bench(&good.replace(&rows[1], &format!("S7CTIME|{}\n", f.join(";"))))
                .is_err()
        );
    }
    for bad in [
        good.replace(header, ""),
        format!("{header}{good}"),
        good.replace(&rows[1], ""),
        format!("{good}{}", rows[0]),
        good.replace(&rows[1], &rows[0]),
        good.replace(&rows[1], "S7CTIME|1;2\n"),
        format!("{good}|event=panic|\n"),
    ] {
        assert!(validate_cow_bench(&bad).is_err());
    }
}

fn validate_ipc_bench(output: &str) -> Result<String, String> {
    if output.contains("|event=panic|") {
        return Err("IPC benchmark panicked".into());
    }
    let headers: Vec<_> = output.lines().filter(|s| s.starts_with("S7IPC|")).collect();
    let Some(header) = headers.first().filter(|_| headers.len() == 1) else {
        return Err("IPC benchmark requires one verified header".into());
    };
    let (hz, identities) = header
        .strip_prefix("S7IPC|samples=64;hz=")
        .and_then(|s| s.strip_suffix(";direct=128;queued=0;blocks=128;verified=1"))
        .and_then(|s| s.split_once(";boundary=user-wrapper;client="))
        .ok_or("invalid IPC benchmark calibration or paths")?;
    let hz = hz
        .parse::<u64>()
        .ok()
        .filter(|hz| *hz > 0)
        .ok_or("invalid IPC frequency")?;
    let (client, service) = identities
        .split_once(";service=")
        .ok_or("missing IPC identities")?;
    let identity = |text: &str, id: u32| {
        text.parse::<u64>()
            .ok()
            .filter(|value| *value as u32 == id && *value >> 32 != 0)
    };
    let client = identity(client, 2).ok_or("invalid IPC client identity")?;
    let service = identity(service, 1).ok_or("invalid IPC service identity")?;
    let mut durations = Vec::new();
    let mut previous_time = 0;
    let mut previous_message = 0;
    for (index, line) in output
        .lines()
        .filter_map(|s| s.strip_prefix("S7ITIME|"))
        .enumerate()
    {
        let v: Vec<u64> = line
            .split(';')
            .map(str::parse)
            .collect::<Result<_, _>>()
            .map_err(|_| "invalid IPC benchmark receipt")?;
        if index >= 64
            || v.len() != 15
            || v[0] != index as u64
            || v[1] == 0
            || v[1] < previous_time
            || v[2] < v[1]
            || v[3] != 65536 + (64 - index) as u64
            || v[9] != (64 - index) as u64
            || v[4..7] != [0; 3]
            || v[10..13] != [0; 3]
            || v[7] != service
            || v[13] != client
            || v[14] <= previous_message
            || v[8] <= v[14]
            || v[8] > u32::MAX as u64
        {
            return Err("unordered, incomplete or incorrect IPC benchmark receipt".into());
        }
        durations.push(v[2] - v[1]);
        previous_time = v[2];
        previous_message = v[8];
    }
    if durations.len() != 64 {
        return Err("IPC benchmark requires exactly 64 samples".into());
    }
    Ok(benchmark_summary(
        64,
        hz,
        "user-wrapper",
        &["user-total"],
        &mut [durations],
    ))
}

#[cfg(test)]
#[test]
fn ipc_benchmark_requires_sequenced_authenticated_roundtrips() {
    let header = "S7IPC|samples=64;hz=1000000000;boundary=user-wrapper;client=8589934594;service=4294967297;direct=128;queued=0;blocks=128;verified=1\n";
    let rows: Vec<_> = (0..64)
        .map(|i| {
            format!(
                "S7ITIME|{i};{};{};{};0;0;0;4294967297;{};{};0;0;0;8589934594;{}\n",
                i * 1000 + 1,
                i * 1000 + 13 + i,
                65600 - i,
                2 * i + 2,
                64 - i,
                2 * i + 1
            )
        })
        .collect();
    let good = header.to_owned() + &rows.concat();
    assert!(
        validate_ipc_bench(&good)
            .unwrap()
            .ends_with("user-total_ns=43/75")
    );
    assert!(validate_native_bench(&good).is_err());
    assert!(validate_gate_bench(&good, true).is_err());
    for (old, new) in [
        ("samples=64", "samples=63"),
        ("hz=1000000000", "hz=0"),
        ("boundary=user-wrapper", "boundary=pre-iret"),
        ("client=8589934594", "client=8589934595"),
        ("client=8589934594", "client=2"),
        ("service=4294967297", "service=4294967298"),
        ("service=4294967297", "service=1"),
        ("direct=128", "direct=127"),
        ("queued=0", "queued=1"),
        ("blocks=128", "blocks=127"),
        ("verified=1", "verified=0"),
    ] {
        assert!(validate_ipc_bench(&good.replace(old, new)).is_err());
    }
    let values: Vec<_> = rows[0]
        .trim()
        .strip_prefix("S7ITIME|")
        .unwrap()
        .split(';')
        .collect();
    for (field, bad_value) in [
        (0, "1"),
        (1, "0"),
        (1, "14"),
        (2, "0"),
        (3, "65601"),
        (4, "1"),
        (5, "1"),
        (6, "1"),
        (7, "8589934593"),
        (8, "1"),
        (8, "4294967296"),
        (9, "63"),
        (10, "1"),
        (11, "1"),
        (12, "1"),
        (13, "4294967298"),
        (14, "0"),
        (14, "bad"),
    ] {
        let mut bad = values.clone();
        bad[field] = bad_value;
        let bad_row = format!("S7ITIME|{}\n", bad.join(";"));
        assert!(validate_ipc_bench(&good.replace(&rows[0], &bad_row)).is_err());
    }
    for bad in [
        good.replace(header, ""),
        good.clone() + header,
        good.replace(&rows[0], ""),
        good.clone() + &rows[0],
        good.replace("S7ITIME|1;1001", "S7ITIME|1;12"),
        good.replace(
            "S7ITIME|1;1001;1014;65599;0;0;0;4294967297;4;63;0;0;0;8589934594;3",
            "S7ITIME|1;1001;1014;65599;0;0;0;4294967297;4;63;0;0;0;8589934594;2",
        ),
        good.replace(&rows[0], "S7ITIME|0;1\n"),
        good.replace(&rows[0], &(rows[0].trim().to_owned() + ";0\n")),
        good + "PRIMA|event=panic|status=error\n",
        "S7NATIVE|samples=128;hz=1;boundary=user-wrapper;number=7;task=2;gate=0;verified=1".into(),
    ] {
        assert!(validate_ipc_bench(&bad).is_err());
    }
}

#[cfg(test)]
#[test]
fn native_benchmark_requires_correct_return_values_and_complete_ordered_samples() {
    let header = "S7NATIVE|samples=128;hz=1000000000;boundary=user-wrapper;number=7;task=2;gate=0;verified=1\n";
    let mut good = header.to_owned();
    for i in 0..128 {
        let t = i * 1000 + 1;
        good.push_str(&format!("S7NTIME|{i};{t};{};2\n", t + 12 + i));
    }
    assert!(
        validate_native_bench(&good)
            .unwrap()
            .ends_with("user-total_ns=75/138")
    );
    assert!(validate_gate_bench(&good, true).is_err());
    assert!(
        validate_native_bench("S7BENCH|samples=128;hz=1;boundary=user-wrapper;verified=1").is_err()
    );
    for bad in [
        good.replace(header, ""),
        good.clone() + header,
        good.replace("samples=128", "samples=127"),
        good.replace("hz=1000000000", "hz=0"),
        good.replace("number=7", "number=8"),
        good.replace("task=2", "task=1"),
        good.replace("gate=0", "gate=1"),
        good.replace("verified=1", "verified=0"),
        good.replace("boundary=user-wrapper", "boundary=pre-iret"),
        good.replace("S7NTIME|0;1;13;2\n", ""),
        good.replace("S7NTIME|1;1001", "S7NTIME|2;1001"),
        good.replace("S7NTIME|0;1;13;2", "S7NTIME|0;1;13;3"),
        good.replace("S7NTIME|0;1;13;2", "S7NTIME|0;1;13"),
        good.replace("S7NTIME|0;1;13;2", "S7NTIME|0;1;13;2;2"),
        good.replace("S7NTIME|0;1;13;2", "S7NTIME|0;0;13;2"),
        good.replace("S7NTIME|0;1;13;2", "S7NTIME|0;14;13;2"),
        good.replace("S7NTIME|1;1001", "S7NTIME|1;12"),
        good.replace("S7NTIME|0;1;13;2", "S7NTIME|0;bad;13;2"),
        good.clone() + "S7NTIME|128;128001;128141;2\n",
        good + "PRIMA|event=panic|status=error\n",
    ] {
        assert!(validate_native_bench(&bad).is_err());
    }
}

#[cfg(test)]
#[test]
fn benchmark_rejects_missing_reordered_regressed_and_duplicate_samples() {
    let header = "S6BENCH|samples=1000;hz=1000000000;boundary=pre-iret;verified=1\n";
    let history =
        "S6HISTORY|deliveries=1000;audit_roots=1000;dedicated_slots_saved=1000;verified=1\n";
    let mut good = history.to_owned() + header;
    for i in 0..1000 {
        let t = i * 10 + 1;
        good.push_str(&format!(
            "S6TIME|{i};{t};{};{};{};{}\n",
            t + 1,
            t + 3,
            t + 6,
            t + 9
        ));
    }
    assert!(
        validate_gate_bench(&good, false)
            .unwrap()
            .contains("total_ns=9/9")
    );
    assert!(validate_gate_bench(&good, true).is_err());
    for bad in [
        good.replace(history, ""),
        good.clone() + history,
        good.replace("dedicated_slots_saved=1000", "dedicated_slots_saved=0"),
        good.replace(header, ""),
        good.clone() + header,
        good.replace("hz=1000000000", "hz=0"),
        good.replace("S6TIME|0;1;2;4;7;10\n", ""),
        good.replace("S6TIME|1;11", "S6TIME|2;11"),
        good.replace("S6TIME|0;1;2", "S6TIME|0;3;2"),
        good.clone() + "S6TIME|1000;10001;10002;10004;10007;10010\n",
        good + "PRIMA|event=panic|status=error\n",
    ] {
        assert!(validate_gate_bench(&bad, false).is_err());
    }
}

#[cfg(test)]
#[test]
fn user_benchmark_requires_complete_ordered_outer_and_kernel_intervals() {
    let history = "S7HISTORY|deliveries=128;audit_roots=128;dedicated_slots_saved=128;verified=1\n";
    let header = "S7BENCH|samples=128;hz=1000000000;boundary=user-wrapper;verified=1\n";
    let mut good = history.to_owned() + header;
    for i in 0..128 {
        let t = i * 1000 + 1;
        good.push_str(&format!(
            "S7TIME|{i};{t};{};{};{};{};{};{}\n",
            t + 1,
            t + 2,
            t + 4,
            t + 7,
            t + 10,
            t + 12 + i
        ));
    }
    let summary = validate_gate_bench(&good, true).unwrap();
    assert!(summary.contains("kernel-total_ns=9/9"));
    assert!(summary.contains("user-entry_ns=1/1 user-return_ns=65/128 user-total_ns=75/138"));
    assert!(validate_gate_bench(&good, false).is_err());
    for bad in [
        good.replace(history, ""),
        good.clone() + history,
        good.replace("audit_roots=128", "audit_roots=127"),
        good.replace(header, ""),
        good.clone() + header,
        good.replace("hz=1000000000", "hz=0"),
        good.replace("boundary=user-wrapper", "boundary=pre-iret"),
        good.replace("S7TIME|0;1;2;3;5;8;11;13\n", ""),
        good.replace("S7TIME|1;1001", "S7TIME|2;1001"),
        good.replace("S7TIME|0;1;2", "S7TIME|0;3;2"), // begin after kernel entry
        good.replace(";11;13\n", ";11;10\n"),         // end before kernel resume
        good.replace("S7TIME|1;1001", "S7TIME|1;12"), // next begin before previous end
        good.replace("S7TIME|0;1;2", "S7TIME|0;0;2"),
        good.replace("S7TIME|0;1;2;", "S7TIME|0;1;"),
        good.clone() + "S7TIME|128;128001;128002;128003;128005;128008;128011;128141\n",
        good + "PRIMA|event=panic|status=error\n",
    ] {
        assert!(validate_gate_bench(&bad, true).is_err());
    }
}

#[cfg(test)]
#[test]
fn gate_witness_rejects_missing_duplicate_failed_or_panicked_records() {
    let good = "S6GATE|int80=1;fast=1;registers=1;signals=1;retry=1;output_failures=3;duplicate=1;orphan=1;admission=1;rejected=9;audit=1\n";
    assert!(validate_gate_output(good).is_ok());
    for bad in [
        String::new(),
        good.repeat(2),
        good.replace("fast=1", "fast=0"),
        good.replace("output_failures=3", "output_failures=2"),
        good.replace(";admission=1", ""),
        good.replace("admission=1", "admission=0"),
        good.replace(";rejected=9", ""),
        good.replace("rejected=9", "rejected=8"),
        good.to_owned() + "PRIMA|v=1|seq=9|event=panic|status=error\n",
    ] {
        assert!(validate_gate_output(&bad).is_err());
    }
}

fn validate_gate_output(output: &str) -> Result<(), String> {
    let records: Vec<_> = output
        .lines()
        .filter(|line| line.starts_with("S6GATE|"))
        .collect();
    if records
        != [
            "S6GATE|int80=1;fast=1;registers=1;signals=1;retry=1;output_failures=3;duplicate=1;orphan=1;admission=1;rejected=9;audit=1",
        ]
        || output.contains("|event=panic|")
    {
        return Err("missing or invalid G1 gate witness".into());
    }
    Ok(())
}

fn validate_gate_matrix(output: &str, case: &str) -> Result<(), String> {
    let expected = format!("S6MATRIX|case={case};verified=1;state=1;audit=1");
    if !["G2", "G2-death", "G3"].contains(&case)
        || output
            .lines()
            .filter(|line| line.starts_with("S6MATRIX|"))
            .collect::<Vec<_>>()
            != [expected]
        || output.contains("|event=panic|")
    {
        return Err("missing or invalid G2/G3 gate witness".into());
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn matrix_requires_the_selected_complete_unique_success_record() {
    for case in ["G2", "G2-death", "G3"] {
        let good = format!("S6MATRIX|case={case};verified=1;state=1;audit=1\n");
        assert!(validate_gate_matrix(&good, case).is_ok());
        for bad in [
            good.repeat(2),
            good.replace("verified=1", "verified=0"),
            good.replace(";audit=1", ""),
            good.clone() + "PRIMA|event=panic|status=error",
        ] {
            assert!(validate_gate_matrix(&bad, case).is_err());
        }
        assert!(validate_gate_matrix(&good, "other").is_err());
    }
}

/// Expected terminal failure, distinct from every ordinary success suite. The
/// panic status alone is insufficient: require the nominated guard's evidence.
pub(crate) fn run_bios_gate_audit(image: &Path, site: &str, round: u32) -> Result<(), String> {
    let output = run_captured(image)?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::write(
        root.join(format!("target/prima-os/S6AUDIT-{site}-{round}.log")),
        &output.stdout,
    )
    .map_err(|e| format!("could not preserve audit-stop witness: {e}"))?;
    validate_exit_code(output.status.code(), PANIC_EXIT_CODE)
        .map_err(|e| with_output(e, &output))?;
    crate::gate_audit::validate_output(&output.stdout, site)
        .map_err(|e| with_output(e, &output))?;
    println!("PASS S6AUDIT site={site} round={round} exit=35 unchanged=1");
    Ok(())
}

/// M5–M9：托管程序轮（echo、时间/fd/vfs/meta 探针与 sbase cat/ls 共用同一证据通道）。
pub(crate) fn run_bios_test_program(
    image: &Path,
    name: &str,
    expected_stdout: &[u8],
    expected_ipc: Option<(u64, u64, u64)>,
) -> Result<(u64, u64, u64), String> {
    let output = run_captured(image)?;
    // 证据落盘：每个托管程序的原始串口流写到 `target/prima-os/<name>.log`，报告引用
    // 计数/输出时不必依赖终端滚动；失败时 `with_output` 仍带完整输出。
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root");
    let log = workspace
        .join("target/prima-os")
        .join(format!("{name}.log"));
    let _ = std::fs::write(log, &output.stdout);
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|error| with_output(error, &output))?;
    let ipc = protocol::validate_output_echo(
        &output.stdout,
        if name == "N1" { None } else { expected_ipc },
    )
    .map_err(|error| with_output(error, &output))?;
    if name == "N1" {
        protocol::validate_timed_ipc(ipc, expected_ipc)
            .map_err(|error| with_output(error, &output))?;
    }
    protocol::validate_service_traces(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_native_app_trace(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_user_fault_trace(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_stage5_echo(&output.stdout, name, expected_stdout)
        .map_err(|error| with_output(error, &output))?;
    Ok(ipc)
}

/// M14：带 marker 驱动输入注入的 QEMU 捕获运行器。
///
/// `script` 的每一对 `(marker, bytes)` 在串口输出首次出现 `marker` 时写入 QEMU
/// stdin（即 guest 串口输入）。这样输入不会早于 guest 的读请求被丢弃。
fn run_captured_input(
    image: &Path,
    script: &[(&'static str, &'static [u8])],
) -> Result<CapturedOutput, String> {
    let qemu = locate_qemu()?;
    let mut child = Command::new(&qemu)
        .args(qemu_args(image))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", qemu.display()))?;
    let mut stdin = child.stdin.take().expect("stdin is piped");
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let (sender, receiver) = mpsc::channel::<Vec<u8>>();
    let writer = thread::Builder::new()
        .name("prima-qemu-input".to_owned())
        .spawn(move || {
            for chunk in receiver {
                if stdin.write_all(&chunk).is_err() {
                    break;
                }
                let _ = stdin.flush();
            }
        })
        .map_err(|error| format!("failed to start input writer: {error}"))?;
    let script_owned: Vec<(String, Vec<u8>)> = script
        .iter()
        .map(|(marker, bytes)| ((*marker).to_owned(), bytes.to_vec()))
        .collect();
    let reader = thread::Builder::new()
        .name("prima-qemu-stdout".to_owned())
        .spawn(move || {
            let mut out = Vec::new();
            let mut buffer = [0u8; 4096];
            let mut used = vec![false; script_owned.len()];
            let mut stream = stdout;
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        out.extend_from_slice(&buffer[..count]);
                        for (index, (marker, bytes)) in script_owned.iter().enumerate() {
                            if used[index] || marker.is_empty() {
                                continue;
                            }
                            if contains_bytes(&out, marker.as_bytes()) {
                                used[index] = true;
                                let _ = sender.send(bytes.to_vec());
                            }
                        }
                    }
                    Err(_) => break,
                }
            }
            out
        })
        .map_err(|error| format!("failed to start QEMU stdout reader: {error}"))?;
    let stderr_reader = thread::Builder::new()
        .name("prima-qemu-stderr".to_owned())
        .spawn(move || {
            let mut out = Vec::new();
            let mut stream = stderr;
            let _ = stream.read_to_end(&mut out);
            out
        })
        .map_err(|error| format!("failed to start QEMU stderr reader: {error}"))?;
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < TEST_TIMEOUT => {
                thread::sleep(Duration::from_millis(10))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let stdout = reader.join().unwrap_or_default();
                let stderr = stderr_reader.join().unwrap_or_default();
                return Err(format!(
                    "QEMU timed out after {}s with input script\nstdout:\n{}\nstderr:\n{}",
                    TEST_TIMEOUT.as_secs(),
                    String::from_utf8_lossy(&stdout),
                    String::from_utf8_lossy(&stderr)
                ));
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("failed while waiting for QEMU: {error}"));
            }
        }
    };
    let stdout = reader.join().unwrap_or_default();
    let stderr = stderr_reader.join().unwrap_or_default();
    let _ = writer.join();
    Ok(CapturedOutput {
        status,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// M14：托管程序轮 + marker 驱动串口输入。
pub(crate) fn run_bios_test_input(
    image: &Path,
    name: &str,
    run: usize,
    expected_stdout: &[u8],
    expected_ipc: Option<protocol::InteractiveIpc>,
    script: &[(&'static str, &'static [u8])],
) -> Result<protocol::InteractiveIpc, String> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root");
    let directory = workspace.join("target/prima-os");
    let run_log = directory.join(format!("{name}-run{run}.log"));
    let output = run_captured_input(image, script).inspect_err(|error| {
        let _ = std::fs::write(&run_log, error);
    })?;
    let _ = std::fs::write(run_log, &output.stdout);
    let log = directory.join(format!("{name}.log"));
    let _ = std::fs::write(log, &output.stdout);
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|error| with_output(error, &output))?;
    let ipc = protocol::validate_output_echo(&output.stdout, None)
        .map_err(|error| with_output(error, &output))?;
    let ipc = protocol::interactive_ipc(&output.stdout, ipc)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_interactive_ipc(ipc, expected_ipc)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_service_traces(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_native_app_trace(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_user_fault_trace(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_stage5_echo(&output.stdout, name, expected_stdout)
        .map_err(|error| with_output(error, &output))?;
    Ok(ipc)
}

/// 用固定 BIOS 参数捕获一次运行（切片 A/B 的既有路径）。
fn run_captured(image: &Path) -> Result<CapturedOutput, String> {
    run_captured_with(qemu_args(image), TEST_TIMEOUT)
}

/// 切片 P：跑一次 BIOS 镜像并返回已通过退出码与事件序列校验的串口输出，
/// 供差分比较器提取 `user_cow_ready` 观测。校验语义与 `run_bios_test` 完全一致。
pub(crate) fn run_bios_capture(image: &Path) -> Result<String, String> {
    let output = run_captured(image)?;
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_output(&output.stdout).map_err(|error| with_output(error, &output))?;
    protocol::validate_service_traces(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_native_app_trace(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_stage5_capacity(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    protocol::validate_stage5_tty(&output.stdout).map_err(|error| with_output(error, &output))?;
    protocol::validate_user_fault_trace(&output.stdout)
        .map_err(|error| with_output(error, &output))?;
    Ok(output.stdout)
}

/// The Linux image has its own strict protocol in stage6, rather than the
/// ordinary native-service acceptance traces checked by run_bios_capture.
#[cfg(feature = "linux-probe")]
pub(crate) fn run_bios_linux_capture(image: &Path) -> Result<String, String> {
    let output = run_captured(image)?;
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|error| with_output(error, &output))?;
    Ok(output.stdout)
}

/// 通用 QEMU 捕获运行器：给定完整参数与超时，返回退出状态与双流输出。
///
/// 切片 P 的参考 Linux guest 复用此入口（`-kernel`/`-initrd` 参数、独立 180s 超时）。
pub(crate) fn run_captured_with(
    args: Vec<OsString>,
    timeout: Duration,
) -> Result<CapturedOutput, String> {
    let qemu = locate_qemu()?;
    let mut child = Command::new(&qemu)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", qemu.display()))?;

    let stdout = child
        .stdout
        .take()
        .expect("stdout is piped when the child is created");
    let stderr = child
        .stderr
        .take()
        .expect("stderr is piped when the child is created");
    let (stdout_reader, stderr_reader) =
        start_readers_with(&mut child, stdout, stderr, spawn_reader)?;
    let started = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let cleanup = terminate_child_with(&mut child);
                return match cleanup {
                    Ok(cleanup) => {
                        let streams = join_readers(stdout_reader, stderr_reader);
                        Err(format!(
                            "QEMU timed out after {}s; {cleanup}\n{}",
                            timeout.as_secs(),
                            streams.diagnostics()
                        ))
                    }
                    Err(cleanup) => Err(format!(
                        "QEMU timed out after {}s; {cleanup}; reader threads were not joined because QEMU may still be running",
                        timeout.as_secs()
                    )),
                };
            }
            Err(error) => {
                let cleanup = terminate_child_with(&mut child);
                return match cleanup {
                    Ok(cleanup) => {
                        let streams = join_readers(stdout_reader, stderr_reader);
                        Err(format!(
                            "failed while waiting for QEMU: {error}; {cleanup}\n{}",
                            streams.diagnostics()
                        ))
                    }
                    Err(cleanup) => Err(format!(
                        "failed while waiting for QEMU: {error}; {cleanup}; reader threads were not joined because QEMU may still be running"
                    )),
                };
            }
        }
    };

    join_readers(stdout_reader, stderr_reader)
        .into_output()
        .map(|streams| CapturedOutput {
            status,
            stdout: streams.stdout,
            stderr: streams.stderr,
        })
}

pub(crate) fn locate_qemu() -> Result<PathBuf, String> {
    let executable = if cfg!(windows) {
        "qemu-system-x86_64.exe"
    } else {
        "qemu-system-x86_64"
    };

    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            let candidate = directory.join(executable);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    #[cfg(windows)]
    {
        let winget_path = PathBuf::from(r"C:\Program Files\qemu\qemu-system-x86_64.exe");
        if winget_path.is_file() {
            return Ok(winget_path);
        }
    }

    Err(format!(
        "qemu-system-x86_64 not found; install it with: {QEMU_INSTALL_COMMAND}"
    ))
}

fn qemu_args(image: &Path) -> Vec<OsString> {
    let mut args = vec![
        "-machine".into(),
        "pc-i440fx-8.2".into(),
        "-accel".into(),
        "tcg,thread=single".into(),
        "-cpu".into(),
        "qemu64".into(),
        "-smp".into(),
        "1".into(),
        "-m".into(),
        "128M".into(),
        "-display".into(),
        "none".into(),
        "-monitor".into(),
        "none".into(),
        "-serial".into(),
        "stdio".into(),
        "-no-reboot".into(),
        "-nic".into(),
        "none".into(),
        "-device".into(),
        "isa-debug-exit,iobase=0xf4,iosize=0x04".into(),
        "-drive".into(),
        format!("format=raw,file={}", image.display()).into(),
        "-boot".into(),
        "order=c,strict=on".into(),
    ];
    if let Some(path) = env::var_os("PRIMA_QEMU_DEBUG_LOG") {
        args.extend(["-d".into(), "int,cpu_reset".into(), "-D".into(), path]);
    }
    args
}

fn validate_exit_code(actual: Option<i32>, expected: i32) -> Result<(), String> {
    match actual {
        Some(actual) if actual == expected => Ok(()),
        Some(actual) => Err(format!(
            "unexpected QEMU exit code: expected {expected}, got {actual}; panic exit code is {PANIC_EXIT_CODE}"
        )),
        None => Err(format!(
            "QEMU returned no numeric exit code; expected {expected}"
        )),
    }
}

type ReaderResult = io::Result<Vec<u8>>;
type ReaderHandle = JoinHandle<ReaderResult>;
type ReaderJob = Box<dyn FnOnce() -> ReaderResult + Send>;

fn spawn_reader(name: &'static str, read: ReaderJob) -> io::Result<ReaderHandle> {
    thread::Builder::new().name(name.to_owned()).spawn(read)
}

fn start_readers_with<Stdout, Stderr, Spawn>(
    child: &mut impl ChildControl,
    stdout: Stdout,
    stderr: Stderr,
    mut spawn: Spawn,
) -> Result<(ReaderHandle, ReaderHandle), String>
where
    Stdout: Read + Send + 'static,
    Stderr: Read + Send + 'static,
    Spawn: FnMut(&'static str, ReaderJob) -> io::Result<ReaderHandle>,
{
    let stdout_reader = match spawn("prima-qemu-stdout", read_job(stdout)) {
        Ok(reader) => reader,
        Err(error) => {
            let startup = format!("failed to start QEMU stdout reader: {error}");
            let cleanup = terminate_child_with(child);
            return Err(startup_cleanup_error(startup, cleanup, false));
        }
    };

    match spawn("prima-qemu-stderr", read_job(stderr)) {
        Ok(stderr_reader) => Ok((stdout_reader, stderr_reader)),
        Err(error) => {
            let startup = format!("failed to start QEMU stderr reader: {error}");
            let cleanup = terminate_child_with(child);
            Err(match cleanup {
                Ok(cleanup) => {
                    let stdout = join_reader(stdout_reader, "stdout").unwrap_or_else(|error| error);
                    format!("{startup}; {cleanup}\nstdout:\n{stdout}")
                }
                Err(cleanup) => startup_cleanup_error(startup, Err(cleanup), true),
            })
        }
    }
}

fn read_job<R>(mut reader: R) -> ReaderJob
where
    R: Read + Send + 'static,
{
    Box::new(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
}

fn startup_cleanup_error(
    startup: String,
    cleanup: Result<String, String>,
    reader_started: bool,
) -> String {
    match cleanup {
        Ok(cleanup) => format!("{startup}; {cleanup}"),
        Err(cleanup) if reader_started => format!(
            "{startup}; {cleanup}; reader thread was not joined because QEMU may still be running"
        ),
        Err(cleanup) => format!("{startup}; {cleanup}"),
    }
}

struct ReaderResults {
    stdout: Result<String, String>,
    stderr: Result<String, String>,
}

impl ReaderResults {
    fn diagnostics(&self) -> String {
        format!(
            "stdout:\n{}\nstderr:\n{}",
            self.stdout.as_deref().unwrap_or_else(|error| error),
            self.stderr.as_deref().unwrap_or_else(|error| error)
        )
    }

    fn into_output(self) -> Result<CapturedStreams, String> {
        match (self.stdout, self.stderr) {
            (Ok(stdout), Ok(stderr)) => Ok(CapturedStreams { stdout, stderr }),
            (stdout, stderr) => Err(ReaderResults { stdout, stderr }.diagnostics()),
        }
    }
}

fn join_readers(
    stdout_reader: JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: JoinHandle<io::Result<Vec<u8>>>,
) -> ReaderResults {
    ReaderResults {
        stdout: join_reader(stdout_reader, "stdout"),
        stderr: join_reader(stderr_reader, "stderr"),
    }
}

fn join_reader(
    reader: JoinHandle<io::Result<Vec<u8>>>,
    stream_name: &str,
) -> Result<String, String> {
    let bytes = reader
        .join()
        .map_err(|_| format!("{stream_name} reader thread panicked"))?
        .map_err(|error| format!("failed to read QEMU {stream_name}: {error}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn terminate_child_with(child: &mut impl ChildControl) -> Result<String, String> {
    match child.kill() {
        Ok(()) => child
            .wait()
            .map(|_| "QEMU child terminated".to_owned())
            .map_err(|error| format!("QEMU cleanup failed after kill: wait failed: {error}")),
        Err(kill_error) => match child.try_wait() {
            Ok(Some(_)) => Ok("QEMU child already exited".to_owned()),
            Ok(None) => Err(format!(
                "QEMU cleanup failed: kill failed: {kill_error}; child is still running"
            )),
            Err(wait_error) => Err(format!(
                "QEMU cleanup failed: kill failed: {kill_error}; try_wait failed: {wait_error}"
            )),
        },
    }
}

fn with_output(error: String, output: &CapturedOutput) -> String {
    format!(
        "{error}\nstdout:\n{}\nstderr:\n{}",
        output.stdout, output.stderr
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ChildControl, PANIC_EXIT_CODE, SUCCESS_EXIT_CODE, TEST_TIMEOUT, join_readers, qemu_args,
        start_readers_with, terminate_child_with, validate_exit_code,
    };
    use std::{
        io::{self, Cursor},
        path::Path,
        process::{Command, ExitStatus},
        sync::mpsc,
        thread,
        time::Duration,
    };

    struct FakeChild {
        kill_error: Option<io::Error>,
        try_wait_result: Option<io::Result<Option<ExitStatus>>>,
        wait_result: Option<io::Result<ExitStatus>>,
        wait_signal: Option<mpsc::Sender<()>>,
        calls: Vec<&'static str>,
    }

    impl ChildControl for FakeChild {
        fn kill(&mut self) -> io::Result<()> {
            self.calls.push("kill");
            self.kill_error.take().map_or(Ok(()), Err)
        }

        fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
            self.calls.push("try_wait");
            self.try_wait_result
                .take()
                .expect("test configures one try_wait result")
        }

        fn wait(&mut self) -> io::Result<ExitStatus> {
            self.calls.push("wait");
            if let Some(signal) = self.wait_signal.take() {
                signal.send(()).expect("test receiver remains open");
            }
            self.wait_result
                .take()
                .expect("test configures one wait result")
        }
    }

    #[test]
    fn first_reader_startup_failure_terminates_and_reaps_child() {
        let mut child = FakeChild {
            kill_error: None,
            try_wait_result: None,
            wait_result: Some(Ok(success_status())),
            wait_signal: None,
            calls: Vec::new(),
        };
        let mut spawn_calls = 0;
        let mut reader_names = Vec::new();
        let error = start_readers_with(
            &mut child,
            Cursor::new(Vec::<u8>::new()),
            Cursor::new(Vec::<u8>::new()),
            |name, _| {
                spawn_calls += 1;
                reader_names.push(name);
                Err(io::Error::other("stdout spawn denied"))
            },
        )
        .unwrap_err();

        assert_eq!(spawn_calls, 1);
        assert_eq!(reader_names, ["prima-qemu-stdout"]);
        assert_eq!(child.calls, ["kill", "wait"]);
        assert!(error.contains("failed to start QEMU stdout reader: stdout spawn denied"));
        assert!(error.contains("QEMU child terminated"));
    }

    #[test]
    fn second_reader_startup_failure_cleans_child_then_joins_first_reader() {
        let (cleanup_confirmed, cleanup_observed) = mpsc::channel();
        let mut cleanup_observed = Some(cleanup_observed);
        let mut child = FakeChild {
            kill_error: None,
            try_wait_result: None,
            wait_result: Some(Ok(success_status())),
            wait_signal: Some(cleanup_confirmed),
            calls: Vec::new(),
        };
        let mut spawn_calls = 0;
        let mut reader_names = Vec::new();
        let error = start_readers_with(
            &mut child,
            Cursor::new(b"stdout preserved".to_vec()),
            Cursor::new(Vec::<u8>::new()),
            |name, read| {
                spawn_calls += 1;
                reader_names.push(name);
                if spawn_calls == 1 {
                    let cleanup_observed = cleanup_observed
                        .take()
                        .expect("test starts the stdout reader once");
                    Ok(thread::spawn(move || {
                        cleanup_observed
                            .recv_timeout(Duration::from_secs(1))
                            .map_err(io::Error::other)?;
                        read()
                    }))
                } else {
                    Err(io::Error::other("stderr spawn denied"))
                }
            },
        )
        .unwrap_err();

        assert_eq!(spawn_calls, 2);
        assert_eq!(reader_names, ["prima-qemu-stdout", "prima-qemu-stderr"]);
        assert_eq!(child.calls, ["kill", "wait"]);
        assert!(error.contains("failed to start QEMU stderr reader: stderr spawn denied"));
        assert!(error.contains("QEMU child terminated"));
        assert!(error.contains("stdout preserved"));
    }

    #[test]
    fn failed_reader_startup_cleanup_does_not_join_live_pipe_reader() {
        let mut child = FakeChild {
            kill_error: Some(io::Error::other("access denied")),
            try_wait_result: Some(Ok(None)),
            wait_result: None,
            wait_signal: None,
            calls: Vec::new(),
        };
        let (reader_gate, reader_waiting) = mpsc::channel::<()>();
        let mut reader_waiting = Some(reader_waiting);
        let mut spawn_calls = 0;
        let error = start_readers_with(
            &mut child,
            Cursor::new(Vec::<u8>::new()),
            Cursor::new(Vec::<u8>::new()),
            |_, read| {
                spawn_calls += 1;
                if spawn_calls == 1 {
                    let reader_waiting = reader_waiting
                        .take()
                        .expect("test starts the stdout reader once");
                    Ok(thread::spawn(move || {
                        reader_waiting.recv().map_err(io::Error::other)?;
                        read()
                    }))
                } else {
                    Err(io::Error::other("stderr spawn denied"))
                }
            },
        )
        .unwrap_err();

        assert_eq!(child.calls, ["kill", "try_wait"]);
        assert!(error.contains("kill failed: access denied"));
        assert!(error.contains("reader thread was not joined because QEMU may still be running"));
        drop(reader_gate);
    }

    fn success_status() -> ExitStatus {
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "exit", "0"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = Command::new("true");
        command
            .status()
            .expect("test harness should provide a successful status")
    }

    #[test]
    fn failed_kill_of_running_child_never_calls_blocking_wait() {
        let mut child = FakeChild {
            kill_error: Some(io::Error::other("access denied")),
            try_wait_result: Some(Ok(None)),
            wait_result: None,
            wait_signal: None,
            calls: Vec::new(),
        };

        let error = terminate_child_with(&mut child).unwrap_err();

        assert!(error.contains("kill failed: access denied"));
        assert_eq!(child.calls, ["kill", "try_wait"]);
    }

    #[test]
    fn reader_error_still_collects_other_stream_diagnostic() {
        let stdout_reader = thread::spawn(|| -> io::Result<Vec<u8>> {
            Err(io::Error::other("stdout read failure"))
        });
        let stderr_reader = thread::spawn(|| Ok(b"stderr preserved".to_vec()));

        let error = join_readers(stdout_reader, stderr_reader)
            .into_output()
            .unwrap_err();

        assert!(error.contains("failed to read QEMU stdout: stdout read failure"));
        assert!(error.contains("stderr preserved"));
    }

    #[test]
    fn reader_panic_still_collects_other_stream_diagnostic() {
        let stdout_reader = thread::spawn(|| Ok(b"stdout preserved".to_vec()));
        let stderr_reader = thread::spawn(|| -> io::Result<Vec<u8>> { panic!("stderr panic") });

        let error = join_readers(stdout_reader, stderr_reader)
            .into_output()
            .unwrap_err();

        assert!(error.contains("stdout preserved"));
        assert!(error.contains("stderr reader thread panicked"));
    }

    #[test]
    fn arguments_are_fixed_single_core_and_headless() {
        let image = Path::new("C:/prima/prima-os-bios.img");
        let actual: Vec<String> = qemu_args(image)
            .into_iter()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            actual,
            vec![
                "-machine",
                "pc-i440fx-8.2",
                "-accel",
                "tcg,thread=single",
                "-cpu",
                "qemu64",
                "-smp",
                "1",
                "-m",
                "128M",
                "-display",
                "none",
                "-monitor",
                "none",
                "-serial",
                "stdio",
                "-no-reboot",
                "-nic",
                "none",
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
                "-drive",
                "format=raw,file=C:/prima/prima-os-bios.img",
                "-boot",
                "order=c,strict=on",
            ]
        );
        assert_eq!(TEST_TIMEOUT, Duration::from_secs(30));
    }

    #[test]
    fn accepts_success_exit_code() {
        assert_eq!(
            validate_exit_code(Some(SUCCESS_EXIT_CODE), SUCCESS_EXIT_CODE),
            Ok(())
        );
    }

    #[test]
    fn rejects_panic_exit_code_when_success_is_expected() {
        let error = validate_exit_code(Some(PANIC_EXIT_CODE), SUCCESS_EXIT_CODE).unwrap_err();
        assert!(error.contains("expected 33"));
        assert!(error.contains("got 35"));
    }

    #[test]
    fn rejects_missing_process_exit_code() {
        let error = validate_exit_code(None, SUCCESS_EXIT_CODE).unwrap_err();
        assert!(error.contains("no numeric exit code"));
    }
}
