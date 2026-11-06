//! One workspace image builder/QEMU owner at a time, including `compat`.
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    path::Path,
};

/// Keep the returned handle alive until the complete command has returned.
/// The persistent file is only a lock anchor; its existence is not ownership.
pub(crate) fn acquire(workspace: &Path, command: &str) -> Result<Option<File>, String> {
    let uses_images = matches!(
        command,
        "build" | "build-linux" | "run" | "test" | "compat" | "linux-trace"
    ) || command.starts_with("test-");
    if !uses_images {
        return Ok(None);
    }
    let directory = workspace.join("target");
    fs::create_dir_all(&directory).map_err(|error| {
        format!(
            "cannot create artifact lock directory {}: {error}",
            directory.display()
        )
    })?;
    let path = directory.join("prima-os-artifacts.lock");
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&path)
        .map_err(|error| format!("cannot open artifact lock {}: {error}", path.display()))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Err(format!(
            "workspace artifacts busy: {command} cannot acquire {}; wait for the owning xtask to finish (do not delete the lock file)",
            path.display()
        )),
        Err(TryLockError::Error(error)) => {
            Err(format!("cannot lock artifacts {}: {error}", path.display()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        path::PathBuf,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    fn scratch() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        env::temp_dir().join(format!(
            "prima-artifact-lock-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn competing_process_is_rejected_and_drop_releases_the_lock() {
        const ROOT: &str = "PRIMA_XTASK_LOCK_TEST_ROOT";
        const BUSY: &str = "PRIMA_XTASK_LOCK_TEST_BUSY";
        if let Some(root) = env::var_os(ROOT) {
            let result = acquire(Path::new(&root), "compat");
            if env::var(BUSY).unwrap() == "1" {
                assert!(result.unwrap_err().contains("workspace artifacts busy"));
            } else {
                assert!(result.unwrap().is_some());
            }
            return;
        }
        let root = scratch();
        let guard = acquire(&root, "test").unwrap().unwrap();
        let probe = |busy: &str| {
            let output = Command::new(env::current_exe().unwrap())
                .args(["--exact", "artifact_lock::tests::competing_process_is_rejected_and_drop_releases_the_lock"])
                .env(ROOT, &root)
                .env(BUSY, busy)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        };
        probe("1");
        drop(guard);
        assert!(root.join("target/prima-os-artifacts.lock").is_file());
        probe("0");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn image_routes_share_the_lock_and_read_only_routes_do_not_create_it() {
        let root = scratch();
        for command in ["lint", "tcb", "not-a-command"] {
            assert!(acquire(&root, command).unwrap().is_none());
        }
        assert!(!root.exists());
        let guard = acquire(&root, "test").unwrap();
        for command in [
            "build",
            "build-linux",
            "run",
            "test",
            "test-vfs",
            "test-process",
            "test-musl",
            "test-rust",
            "test-tty",
            "test-net",
            "test-linux",
            "test-gate",
            "test-gate-audit",
            "test-domains",
            "compat",
            "linux-trace",
        ] {
            assert!(
                acquire(&root, command)
                    .unwrap_err()
                    .contains("workspace artifacts busy"),
                "{command}"
            );
        }
        let independent = scratch();
        drop(acquire(&independent, "build").unwrap());
        drop(guard);
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(independent).unwrap();
    }

    #[test]
    fn lock_setup_errors_fail_closed() {
        let root = scratch();
        fs::write(&root, b"not a directory").unwrap();
        assert!(
            acquire(&root, "build")
                .unwrap_err()
                .contains("cannot create artifact lock directory")
        );
        fs::remove_file(root).unwrap();
    }
}
