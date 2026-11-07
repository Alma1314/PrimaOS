//! D12 input pinning, native launcher and strict eleven-case differential.
use crate::{image, linux, qemu};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const INPUT: &[u8] = include_bytes!(env!("PRIMA_LINUX_ELF"));
const HASH: &str = "dbac288c29ba568459550a2da9e7ae0ded6b1fc728ee9fad3044c44e62d6ac14";
const CASES: [&str; 11] = prima_user::linux::CASES;

pub(crate) fn build() -> Result<(image::Images, PathBuf), String> {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("target/prima-os");
    let input = verify_input(&directory)?;
    for (name, bytes) in [
        ("p1", prima_user::linux::P1),
        ("p2", prima_user::linux::P2),
        ("pthread", prima_user::linux::PTHREAD),
        ("process", prima_user::linux::PROCESS),
        ("process-child", prima_user::linux::PROCESS_CHILD),
        ("matrix", prima_user::linux::MATRIX),
    ] {
        fs::write(directory.join(format!("stage6-{name}.elf")), bytes)
            .map_err(|e| e.to_string())?;
    }
    let service_path = directory.join("stage6-native-service.elf");
    image::strip_debug(
        Path::new(env!("CARGO_BIN_FILE_PRIMA_USER_prima-linux")),
        &service_path,
    )?;
    let service = fs::read(service_path).map_err(|e| e.to_string())?;
    if !service.windows(INPUT.len()).any(|bytes| bytes == INPUT) {
        return Err("native service does not embed the exact pinned ELF bytes".into());
    }
    for bytes in [
        prima_user::linux::P1,
        prima_user::linux::P2,
        prima_user::linux::PTHREAD,
        prima_user::linux::PROCESS,
        prima_user::linux::PROCESS_CHILD,
        prima_user::linux::MATRIX,
    ] {
        if !service.windows(bytes.len()).any(|window| window == bytes) {
            return Err("native service does not embed the exact probe ELF bytes".into());
        }
    }
    let images = image::package_images(|path| {
        image::write_manifest_plain(
            path,
            vec![
                (prima_abi::boot_image::KIND_INIT, launcher(0)),
                (1, service),
            ],
        )
    })?;
    Ok((images, input))
}

pub(crate) fn test() -> Result<(), String> {
    let (images, input) = build()?;
    let directory = input.parent().unwrap();
    let reference = linux::stage6_reference(
        &input,
        &[
            directory.join("stage6-p1.elf"),
            directory.join("stage6-p2.elf"),
            directory.join("stage6-pthread.elf"),
            directory.join("stage6-process.elf"),
            directory.join("stage6-process-child.elf"),
            directory.join("stage6-matrix.elf"),
        ],
    )?;
    let expected = parse_reference(&reference)?;
    println!("PASS S6REF cases=11 pinned_guest=1 same_elf=1");
    let _ = images;
    let service =
        fs::read(directory.join("stage6-native-service.elf")).map_err(|e| e.to_string())?;
    for round in 1..=2 {
        let mut observed = Vec::new();
        for (index, case) in CASES.iter().enumerate() {
            let images = image::package_images(|path| {
                image::write_manifest_plain(
                    path,
                    vec![
                        (prima_abi::boot_image::KIND_INIT, launcher(index as u64 + 1)),
                        (1, service.clone()),
                    ],
                )
            })?;
            let output = qemu::run_bios_linux_capture(&images.bios)?;
            fs::write(
                directory.join(format!("S6LINUX-{case}-{round}.log")),
                &output,
            )
            .map_err(|e| e.to_string())?;
            observed.extend(parse_prima_cases(&output, &[*case])?);
            if observed.last() != expected.get(index) {
                return Err(format!(
                    "Linux differential failed round={round} case={case}: expected={:?} observed={:?}",
                    expected.get(index),
                    observed.last()
                ));
            }
            println!("PASS S6LINUX round={round} case={case} stdout=1 stderr=1 exit=1 audit=1");
        }
        if observed != expected {
            return Err(format!(
                "Linux applet differential failed round={round}: expected={expected:?} observed={observed:?}"
            ));
        }
        println!(
            "PASS S6LINUX round={round} pinned=1 applets=6 probes=2 pthread=1 fork_exec=1 syscall_matrix=1 stdout=1 stderr=1 exit=1"
        );
    }
    Ok(())
}

fn verify_input(directory: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let input = directory.join("stage6-pinned-busybox.elf");
    fs::write(&input, INPUT).map_err(|e| e.to_string())?;
    let result = Command::new("sha256sum")
        .arg(&input)
        .output()
        .map_err(|e| e.to_string())?;
    if !result.status.success()
        || String::from_utf8_lossy(&result.stdout)
            .split_whitespace()
            .next()
            != Some(HASH)
    {
        return Err("D12 frozen busybox SHA-256 mismatch".into());
    }
    Ok(input)
}

struct Code(Vec<u8>);
impl Code {
    fn bytes(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }
    fn mov(&mut self, reg: u8, value: u64) {
        self.bytes(&[0x48 | (reg >> 3), 0xb8 | (reg & 7)]);
        self.bytes(&value.to_le_bytes());
    }
    fn boot(&mut self, offset: u32) {
        self.bytes(&[0x49, 0x8b, 0x87]);
        self.bytes(&offset.to_le_bytes());
    }
    fn store(&mut self, offset: u32) {
        self.bytes(&[0x48, 0x89, 0x83]);
        self.bytes(&offset.to_le_bytes());
    }
    fn call(&mut self, number: u64, args: [u64; 6], overrides: &[u8]) {
        for (reg, value) in [7, 6, 2, 10, 8, 9].into_iter().zip(args) {
            self.mov(reg, value);
        }
        self.bytes(overrides);
        self.mov(0, number);
        self.bytes(&[0xcd, 0x80]);
        // Accept a real token or zero, reject the complete native errno range.
        self.bytes(&[0x48, 0x3d, 0x01, 0xf0, 0xff, 0xff, 0x72, 0x02, 0x0f, 0x0b]);
    }
}

/// Only an ordinary native service is runnable here. Linux clients are created
/// held by that service and published by the managed Gate-start transaction.
pub(super) fn launcher(selection: u64) -> Vec<u8> {
    use prima_abi::{boot_abi as boot, syscall::*};
    const Y: u64 = prima_initgen::USER_Y_VA;
    const BASE: u64 = 0x0000_0b00_0000_0000;
    const STACK: u64 = BASE + 0x80_0000;
    let mut code = Code(Vec::new());
    code.mov(3, Y);
    for (slot, field) in [
        (0, boot::OFF_SERVICE_IMAGE_CAP),
        (1, boot::entry_field(1, boot::ENTRY_OFF_OFFSET_PAGES)),
        (2, boot::entry_field(1, boot::ENTRY_OFF_PAGES)),
        (4, boot::entry_field(1, boot::ENTRY_OFF_ENTRY_VA)),
        (6, boot::OFF_NAMESPACE_CAP),
    ] {
        code.boot(field);
        code.store(slot * 8);
    }
    for (slot, value) in [
        (3, BASE),
        (5, STACK + 64 * 4096 - 8),
        (7, 257),
        (8, 0),
        (9, 0),
        (10, selection),
    ] {
        code.mov(0, value);
        code.store(slot * 8);
    }
    code.call(SYS_TASK_CREATE, [Y, 0, 0, 0, 0, 0], &[]);
    code.bytes(&[0x49, 0x89, 0xc4]); // r12=service manager
    code.call(SYS_MEMSEG_CREATE, [64, 6, 0, 0, 0, 0], &[]);
    code.bytes(&[0x49, 0x89, 0xc6]); // r14=stack Segment
    code.call(
        SYS_MEMSEG_MAP,
        [0, 0, 0, STACK, 64, 6],
        &[0x4c, 0x89, 0xf7, 0x4c, 0x89, 0xe6],
    );
    let again = code.0.len();
    code.call(SYS_RECV, [Y + 256, 0, 0, 0, 0, 0], &[]);
    code.bytes(&[0xe9]);
    let delta = again as i32 - (code.0.len() + 4) as i32;
    code.bytes(&delta.to_le_bytes());
    assert!(code.0.len() <= 4096);
    let mut pages = [0u8; prima_initgen::USER_CODE_LEN];
    pages[..code.0.len()].copy_from_slice(&code.0);
    image::encode_init_elf(&pages)
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Observation {
    case: String,
    pub(super) stdout: Vec<u8>,
    pub(super) stderr: Vec<u8>,
    status: u32,
}
fn unhex(text: &str) -> Result<Vec<u8>, String> {
    if text.len() & 1 != 0 {
        return Err("odd hex stream".into());
    }
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|b| {
            let digit = |v: u8| match v {
                b'0'..=b'9' => Some(v - b'0'),
                b'a'..=b'f' => Some(v - b'a' + 10),
                _ => None,
            };
            Ok(digit(b[0]).ok_or("bad hex")? * 16 + digit(b[1]).ok_or("bad hex")?)
        })
        .collect()
}
pub(super) fn parse_prima_cases(output: &str, cases: &[&str]) -> Result<Vec<Observation>, String> {
    if output.contains("|event=panic|") || output.contains("S6PANIC|") {
        return Err("Prima panicked".into());
    }
    let mut results = Vec::new();
    let mut active: Option<Observation> = None;
    let mut completed = 0;
    let mut nosys = 0u64;
    let mut cookies = std::collections::BTreeSet::new();
    let mut failures = std::collections::BTreeMap::<(u64, u64), u64>::new();
    let mut audited = failures.clone();
    let mut counts = None;
    let terminal = format!("S6LINUX|cases={};verified=1", cases.len());
    for line in output.lines() {
        if let Some(case) = line.strip_prefix("S6BEGIN|case=") {
            if active.is_some() || cases.get(results.len()) != Some(&case) {
                return Err("invalid applet order".into());
            }
            nosys = 0;
            cookies.clear();
            failures.clear();
            audited.clear();
            counts = None;
            active = Some(Observation {
                case: case.into(),
                stdout: Vec::new(),
                stderr: Vec::new(),
                status: 0,
            });
        } else if let Some(data) = line.strip_prefix("S6WRITE|fd=") {
            let (fd, hex) = data.split_once(";hex=").ok_or("bad write record")?;
            let observation = active.as_mut().ok_or("write outside applet")?;
            match fd {
                "1" => observation.stdout.extend(unhex(hex)?),
                "2" => observation.stderr.extend(unhex(hex)?),
                _ => return Err("bad output fd".into()),
            }
        } else if let Some(data) = line.strip_prefix("S6TASK|cookie=") {
            let cookie: u64 = data.parse().map_err(|_| "bad Task cookie")?;
            if active.is_none()
                || cookie as u32 == 0
                || cookie >> 32 == 0
                || cookies.len() == 32
                || !cookies.insert(cookie)
            {
                return Err("invalid Task registration".into());
            }
        } else if let Some(data) = line.strip_prefix("S6NOSYS|cookie=") {
            if active.is_none() {
                return Err("unsupported call outside applet".into());
            }
            let fields: Vec<_> = data.split(';').collect();
            if fields.len() != 3 || fields[2] != "value=-38" {
                return Err("bad unsupported result".into());
            }
            let observed: u64 = fields[0].parse().map_err(|_| "bad client cookie")?;
            let number: u64 = fields[1]
                .strip_prefix("number=")
                .ok_or("missing raw number")?
                .parse()
                .map_err(|_| "bad raw number")?;
            if !cookies.contains(&observed) {
                return Err("inconsistent client cookie".into());
            }
            *failures.entry((observed, number)).or_default() += 1;
            nosys += 1;
        } else if let Some(data) = line.strip_prefix("S6AUDIT|cookie=") {
            if active.is_none() {
                return Err("audit outside case".into());
            }
            let fields: Vec<_> = data.split(';').collect();
            if fields.len() != 4 {
                return Err("bad raw audit".into());
            }
            let cookie = fields[0].parse::<u64>().map_err(|_| "bad audit cookie")?;
            let number = fields[1]
                .strip_prefix("number=")
                .ok_or("missing audit number")?
                .parse::<u64>()
                .map_err(|_| "bad audit number")?;
            let value = fields[2]
                .strip_prefix("value=")
                .ok_or("missing audit value")?
                .parse::<u64>()
                .map_err(|_| "bad audit value")?;
            let args = fields[3]
                .strip_prefix("args=")
                .ok_or("missing raw args")?
                .split(',')
                .map(str::parse::<u64>)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "bad raw args")?;
            if !cookies.contains(&cookie) || value != (-38i64) as u64 || args.len() != 6 {
                return Err("invalid unsupported audit".into());
            }
            *audited.entry((cookie, number)).or_default() += 1;
        } else if let Some(data) = line.strip_prefix("S6COUNTS|") {
            if active.is_none() || counts.is_some() {
                return Err("duplicate/outside audit counts".into());
            }
            let fields: Vec<_> = data.split(';').collect();
            if fields.len() != 6 {
                return Err("bad audit counts".into());
            }
            let mut values = [0u64; 6];
            for ((field, name), value) in fields
                .iter()
                .zip([
                    "trapped=",
                    "delivered=",
                    "replied=",
                    "aborted=",
                    "orphaned=",
                    "cancelled=",
                ])
                .zip(&mut values)
            {
                *value = field
                    .strip_prefix(name)
                    .ok_or("missing audit count")?
                    .parse()
                    .map_err(|_| "bad audit count")?;
            }
            counts = Some(values);
        } else if let Some(data) = line.strip_prefix("S6EXIT|case=") {
            let fields: Vec<_> = data.split(';').collect();
            let mut observation = active.take().ok_or("duplicate exit")?;
            if fields.len() != 4 || fields[0] != observation.case {
                return Err("bad exit record".into());
            }
            observation.status = fields[1]
                .strip_prefix("status=")
                .ok_or("missing status")?
                .parse()
                .map_err(|_| "bad status")?;
            let requests: u64 = fields[2]
                .strip_prefix("requests=")
                .ok_or("missing count")?
                .parse()
                .map_err(|_| "bad count")?;
            let unsupported: u64 = fields[3]
                .strip_prefix("unsupported=")
                .ok_or("missing unsupported")?
                .parse()
                .map_err(|_| "bad unsupported count")?;
            let values = counts.take().ok_or("missing kernel audit counts")?;
            if observation.status != 0
                || unsupported != nosys
                || failures != audited
                || values[1] != requests
                || values[3] != unsupported
                || values[4] != 0
                || values[0] < values[1]
                || values[0] != values[2..].iter().sum::<u64>()
            {
                return Err("failed case or kernel audit mismatch".into());
            }
            if requests == 0 || cookies.is_empty() {
                return Err("no real client traps".into());
            }
            results.push(observation);
        } else if line == terminal {
            completed += 1;
        } else if line.starts_with("S6BEGIN|")
            || line.starts_with("S6WRITE|")
            || line.starts_with("S6EXIT|")
            || line.starts_with("S6LINUX|")
            || line.starts_with("S6TASK|")
            || line.starts_with("S6NOSYS|")
            || line.starts_with("S6AUDIT|")
            || line.starts_with("S6COUNTS|")
        {
            return Err("malformed witness".into());
        }
    }
    if results.len() != cases.len() || active.is_some() || completed != 1 {
        return Err("incomplete applet matrix".into());
    }
    Ok(results)
}
fn parse_reference(output: &str) -> Result<Vec<Observation>, String> {
    let mut results = Vec::new();
    for data in output
        .lines()
        .filter_map(|line| line.strip_prefix("S6REF|case="))
    {
        let fields: Vec<_> = data.split(';').collect();
        if fields.len() != 4 || CASES.get(results.len()) != Some(&fields[0]) {
            return Err("bad Linux reference order".into());
        }
        results.push(Observation {
            case: fields[0].into(),
            status: fields[1]
                .strip_prefix("status=")
                .ok_or("bad reference status")?
                .parse()
                .map_err(|_| "bad reference status")?,
            stdout: unhex(
                fields[2]
                    .strip_prefix("stdout=")
                    .ok_or("bad reference stdout")?,
            )?,
            stderr: unhex(
                fields[3]
                    .strip_prefix("stderr=")
                    .ok_or("bad reference stderr")?,
            )?,
        });
    }
    if results.len() != CASES.len() || results.iter().any(|o| o.status != 0) {
        return Err("missing or failed Linux reference case".into());
    }
    Ok(results)
}

#[cfg(test)]
#[test]
fn differential_rejects_missing_duplicate_reordered_and_failed_witnesses() {
    let mut good = String::new();
    for case in CASES {
        good.push_str(&format!("S6BEGIN|case={case}\nS6TASK|cookie=4294967299\nS6WRITE|fd=1;hex=6869\nS6COUNTS|trapped=1;delivered=1;replied=0;aborted=0;orphaned=0;cancelled=1\nS6EXIT|case={case};status=0;requests=1;unsupported=0\n"));
    }
    good.push_str("S6LINUX|cases=11;verified=1\n");
    assert_eq!(parse_prima_cases(&good, &CASES).unwrap()[0].stdout, b"hi");
    for bad in [
        good.repeat(2),
        good.replace("cases=11;verified=1", "cases=11;verified=0"),
        good.replace("requests=1", "requests=0"),
        good.replace("cancelled=1", "cancelled=2"),
        good.replace("delivered=1", "delivered=2"),
        good.replace("unsupported=0", "unsupported=1"),
        good.replace("status=0", "status=1"),
        good.replace("hex=6869", "hex=6z"),
        good.replace("case=true", "case=sort"),
        good + "PRIMA|event=panic|status=error\n",
    ] {
        assert!(parse_prima_cases(&bad, &CASES).is_err());
    }
}
