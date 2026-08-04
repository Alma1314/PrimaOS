use crate::protocol;
use std::{
    env,
    ffi::OsString,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const SUCCESS_EXIT_CODE: i32 = 33;
const PANIC_EXIT_CODE: i32 = 35;
const TEST_TIMEOUT: Duration = Duration::from_secs(10);
const QEMU_INSTALL_COMMAND: &str =
    "winget install --id SoftwareFreedomConservancy.QEMU --exact --version 11.0.50";

struct CapturedOutput {
    status: ExitStatus,
    stdout: String,
    stderr: String,
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
    protocol::validate_output(&output.stdout).map_err(|error| with_output(error, &output))
}

const _: fn(&Path) -> Result<(), String> = run_bios_interactive;
const _: fn(&Path) -> Result<(), String> = run_bios_test;

fn run_captured(image: &Path) -> Result<CapturedOutput, String> {
    let qemu = locate_qemu()?;
    let mut child = Command::new(&qemu)
        .args(qemu_args(image))
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
    let stdout_reader = read_in_background(stdout);
    let stderr_reader = read_in_background(stderr);
    let started = Instant::now();

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < TEST_TIMEOUT => {
                thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let cleanup = terminate_child(&mut child);
                let stdout = join_reader(stdout_reader, "stdout")?;
                let stderr = join_reader(stderr_reader, "stderr")?;
                return Err(format!(
                    "QEMU timed out after {}s; {cleanup}\nstdout:\n{stdout}\nstderr:\n{stderr}",
                    TEST_TIMEOUT.as_secs()
                ));
            }
            Err(error) => {
                let cleanup = terminate_child(&mut child);
                let stdout = join_reader(stdout_reader, "stdout")?;
                let stderr = join_reader(stderr_reader, "stderr")?;
                return Err(format!(
                    "failed while waiting for QEMU: {error}; {cleanup}\nstdout:\n{stdout}\nstderr:\n{stderr}"
                ));
            }
        }
    };

    Ok(CapturedOutput {
        status,
        stdout: join_reader(stdout_reader, "stdout")?,
        stderr: join_reader(stderr_reader, "stderr")?,
    })
}

fn locate_qemu() -> Result<PathBuf, String> {
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
    vec![
        "-machine".into(),
        "pc-i440fx-10.0".into(),
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
        "-no-shutdown".into(),
        "-nic".into(),
        "none".into(),
        "-device".into(),
        "isa-debug-exit,iobase=0xf4,iosize=0x04".into(),
        "-drive".into(),
        format!("format=raw,readonly=on,file={}", image.display()).into(),
        "-boot".into(),
        "order=c,strict=on".into(),
    ]
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

fn read_in_background<R>(mut reader: R) -> JoinHandle<io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Ok(bytes)
    })
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

fn terminate_child(child: &mut Child) -> String {
    let kill_result = child.kill();
    let wait_result = child.wait();
    match (kill_result, wait_result) {
        (Ok(()), Ok(_)) => "QEMU child terminated".to_owned(),
        (kill, wait) => format!("QEMU cleanup result: kill={kill:?}, wait={wait:?}"),
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
    use super::{PANIC_EXIT_CODE, SUCCESS_EXIT_CODE, TEST_TIMEOUT, qemu_args, validate_exit_code};
    use std::{path::Path, time::Duration};

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
                "pc-i440fx-10.0",
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
                "-no-shutdown",
                "-nic",
                "none",
                "-device",
                "isa-debug-exit,iobase=0xf4,iosize=0x04",
                "-drive",
                "format=raw,readonly=on,file=C:/prima/prima-os-bios.img",
                "-boot",
                "order=c,strict=on",
            ]
        );
        assert_eq!(TEST_TIMEOUT, Duration::from_secs(10));
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
