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
    expected_stdout: &[u8],
    expected_ipc: Option<(u64, u64, u64)>,
    script: &[(&'static str, &'static [u8])],
) -> Result<(u64, u64, u64), String> {
    let output = run_captured_input(image, script)?;
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask must be directly under the workspace root");
    let log = workspace
        .join("target/prima-os")
        .join(format!("{name}.log"));
    let _ = std::fs::write(log, &output.stdout);
    validate_exit_code(output.status.code(), SUCCESS_EXIT_CODE)
        .map_err(|error| with_output(error, &output))?;
    let ipc = protocol::validate_output_echo(&output.stdout, None)
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
