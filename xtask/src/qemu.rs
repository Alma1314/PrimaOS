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
                let cleanup = terminate_child_with(&mut child);
                return match cleanup {
                    Ok(cleanup) => {
                        let streams = join_readers(stdout_reader, stderr_reader);
                        Err(format!(
                            "QEMU timed out after {}s; {cleanup}\n{}",
                            TEST_TIMEOUT.as_secs(),
                            streams.diagnostics()
                        ))
                    }
                    Err(cleanup) => Err(format!(
                        "QEMU timed out after {}s; {cleanup}; reader threads were not joined because QEMU may still be running",
                        TEST_TIMEOUT.as_secs()
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
        terminate_child_with, validate_exit_code,
    };
    use std::{io, path::Path, process::ExitStatus, thread, time::Duration};

    struct FakeChild {
        kill_error: Option<io::Error>,
        try_wait_result: Option<io::Result<Option<ExitStatus>>>,
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
            panic!("blocking wait must not run after a failed kill")
        }
    }

    #[test]
    fn failed_kill_of_running_child_never_calls_blocking_wait() {
        let mut child = FakeChild {
            kill_error: Some(io::Error::other("access denied")),
            try_wait_result: Some(Ok(None)),
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
