//! Link hooks for the explicitly single-thread Prima std PAL. No libc/TLS ABI.
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use prima_runtime::{Capability, Error, Result, Runtime, TaskId, protocol::STAGE5_TTY_WRITE_TAG};

static SEND: AtomicU64 = AtomicU64::new(0);
static PEER: AtomicU32 = AtomicU32::new(0);
static TASK: AtomicU32 = AtomicU32::new(0);

pub fn initialize(send: Capability, peer: TaskId) -> Result<()> {
    let runtime = Runtime::x86_64();
    runtime.physical_time_enable()?;
    TASK.store(runtime.task_id()?.as_raw(), Ordering::Relaxed);
    PEER.store(peer.as_raw(), Ordering::Relaxed);
    SEND.store(send.as_raw(), Ordering::Release);
    Ok(())
}

#[unsafe(no_mangle)]
extern "C" fn __prima_std_write_byte(byte: u8) -> bool {
    let Ok(send) = Capability::from_raw(SEND.load(Ordering::Acquire)) else {
        return false;
    };
    let runtime = Runtime::x86_64();
    let peer = TaskId::from_raw(PEER.load(Ordering::Relaxed));
    let payload = STAGE5_TTY_WRITE_TAG
        | (1 << 16)
        | (u64::from(byte) << 24)
        | ((u64::from(TASK.load(Ordering::Relaxed)) & 0xff) << 32);
    loop {
        match runtime.send(peer, send, payload, &[]) {
            Ok(()) => {
                let _ = runtime.yield_task();
                return true;
            }
            Err(Error::Again) => {
                if runtime.yield_task().is_err() {
                    return false;
                }
            }
            Err(_) => return false,
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn __prima_std_ticks() -> u64 {
    Runtime::x86_64()
        .physical_ticks()
        .expect("Prima physical clock query")
}

#[unsafe(no_mangle)]
extern "C" fn __prima_std_sleep(seconds: u64, nanos: u32) {
    if seconds == 0 && nanos == 0 {
        return;
    }
    let ticks = seconds
        .checked_mul(100)
        .and_then(|value| value.checked_add(u64::from(nanos).div_ceil(10_000_000)))
        // The reading is rounded down. One additional tick prevents returning
        // before the requested duration when starting between tick boundaries.
        .and_then(|value| value.checked_add(1))
        .expect("Prima sleep duration overflow");
    let deadline = __prima_std_ticks()
        .checked_add(ticks)
        .expect("Prima sleep deadline overflow");
    loop {
        match Runtime::x86_64().wait_physical_deadline(deadline) {
            Ok(()) => return,
            Err(Error::Interrupted) => {}
            Err(_) => panic!("Prima physical deadline wait failed"),
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn __prima_std_yield() {
    Runtime::x86_64().yield_task().expect("Prima yield failed");
}
