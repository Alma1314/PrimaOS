#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(feature = "std", feature(restricted_std))]
#![no_main]

extern crate alloc;

#[cfg(feature = "std")]
mod pal;

use alloc::{boxed::Box, format, vec};
use core::alloc::{GlobalAlloc, Layout};
use core::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(feature = "std"))]
use prima_runtime::protocol::STAGE5_TTY_WRITE_TAG;
use prima_runtime::{Capability, Runtime, TaskId, protocol::STAGE5_C_PROBE_TAG};

#[used]
#[unsafe(link_section = ".note.prima.abi")]
static PROGRAM_CONTRACT: [u32; 11] =
    prima_abi::program_contract::note(prima_abi::program_contract::NATIVE, 0);

// Probe-only allocator: the manager maps a private 96-page arena here. The
// monotonic allocation budget lasts for this one-shot process; no general
// allocator or leak-free long-running Rust process is claimed by this probe.
// ponytail: bounded monotonic arena; use a reclaiming allocator for long-lived apps.
struct ProbeArena(AtomicUsize);
const HEAP_BASE: usize = 0x0000_0c00_0000_0000;
const HEAP_BYTES: usize = 96 * 4096;

// SAFETY: every successful CAS reserves a disjoint, aligned range in the
// process's writable arena. No range exceeds the manager-provided mapping.
unsafe impl GlobalAlloc for ProbeArena {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut cursor = self.0.load(Ordering::Relaxed);
        loop {
            let Some(aligned) = cursor
                .checked_add(layout.align() - 1)
                .map(|value| value & !(layout.align() - 1))
            else {
                return core::ptr::null_mut();
            };
            let Some(end) = aligned
                .checked_add(layout.size())
                .filter(|end| *end <= HEAP_BASE + HEAP_BYTES)
            else {
                return core::ptr::null_mut();
            };
            match self
                .0
                .compare_exchange_weak(cursor, end, Ordering::Relaxed, Ordering::Relaxed)
            {
                Ok(_) => return aligned as *mut u8,
                Err(next) => cursor = next,
            }
        }
    }
    unsafe fn dealloc(&self, _: *mut u8, _: Layout) {}
}

#[global_allocator]
static ARENA: ProbeArena = ProbeArena(AtomicUsize::new(HEAP_BASE));

#[cfg(not(feature = "std"))]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
#[unsafe(naked)]
pub extern "C" fn _start(_: u64, _: u64, _: u64, _: u64, _: u64) -> ! {
    // Native process entry RSP is 16-aligned; a C ABI callee expects the return
    // address at RSP%16=8. Preserve the five entry registers through this call.
    core::arch::naked_asm!("call {start}", "ud2", start = sym rust_start);
}

extern "C" fn rust_start(_: u64, send_raw: u64, _: u64, _: u64, manager: u64) -> ! {
    core::hint::black_box(&PROGRAM_CONTRACT);
    let runtime = Runtime::x86_64();
    let peer = TaskId::from_raw(manager as u32);
    if let (Ok(send), Ok(id)) = (Capability::from_raw(send_raw), runtime.task_id()) {
        #[cfg(feature = "std")]
        if pal::initialize(send, peer).is_err() {
            let _ = runtime.send(
                peer,
                send,
                STAGE5_C_PROBE_TAG | (1 << 16) | u64::from(id.as_raw()),
                &[],
            );
            loop {
                let _ = runtime.receive();
            }
        }
        let oversized_alignment = Layout::from_size_align(1, 1usize << 45).unwrap();
        // SAFETY: the layout is valid, but no aligned address fits this arena.
        let rejected = unsafe { ARENA.alloc(oversized_alignment) }.is_null();
        let core_ok = 123u64.checked_mul(7) == Some(861) && rejected;
        #[repr(align(4096))]
        struct Aligned([u8; 4096]);
        let aligned = Box::new(Aligned([0x5a; 4096]));
        let mut data = vec![3u32, 1, 2];
        data.sort_unstable();
        let alloc_ok = data == [1, 2, 3]
            && format!("{}:{}", data.len(), "Prima") == "3:Prima"
            && (&*aligned as *const Aligned as usize) & 4095 == 0
            && aligned.0[0] == 0x5a
            && aligned.0[4095] == 0x5a;
        #[cfg(feature = "std")]
        let std_ok = {
            use std::io::Read;
            let mut map = std::collections::BTreeMap::new();
            map.insert("value", data.iter().sum::<u32>());
            let mut bytes = [0; 3];
            let read = (&b"abc"[..]).read_exact(&mut bytes);
            // SAFETY: a valid nonzero layout; this unsupported System backend
            // must report OOM rather than invent a usable pointer.
            let system_oom = unsafe { std::alloc::System.alloc(Layout::new::<u64>()) }.is_null();
            let before = std::time::Instant::now();
            std::thread::yield_now();
            std::thread::sleep(std::time::Duration::from_millis(1));
            let elapsed = before.elapsed();
            // A clock can still read zero immediately after initialization.
            // MAX is representable from zero; one further nanosecond is not.
            let overflow = before
                .checked_add(std::time::Duration::MAX)
                .is_none_or(|limit| {
                    limit
                        .checked_add(std::time::Duration::from_nanos(1))
                        .is_none()
                });
            let spawned = std::thread::Builder::new().spawn(|| ()).unwrap_err().kind();
            let input = std::io::stdin().read(&mut bytes).unwrap_err().kind();
            let os_ok = elapsed >= std::time::Duration::from_millis(1)
                && overflow
                && spawned == std::io::ErrorKind::Unsupported
                && input == std::io::ErrorKind::Unsupported;
            system_oom
                && os_ok
                && map.get("value") == Some(&6)
                && read.is_ok()
                && &bytes == b"abc"
                && std::fs::metadata("/").unwrap_err().kind() == std::io::ErrorKind::Unsupported
        };
        #[cfg(not(feature = "std"))]
        let std_ok = true;
        let ok = core_ok && alloc_ok && std_ok;
        let output = format!(
            "RUST|core={};alloc={};std={};native=1;pal={}|ok={}\n",
            u8::from(core_ok),
            u8::from(alloc_ok),
            u8::from(cfg!(feature = "std") && std_ok),
            u8::from(cfg!(feature = "std") && std_ok),
            u8::from(ok)
        );
        #[cfg(feature = "std")]
        {
            use std::io::Write;
            // Split the marker across stdout/stderr to witness both PAL paths.
            let midpoint = output.find("|ok=").unwrap();
            if std::io::stdout()
                .write_all(&output.as_bytes()[..midpoint])
                .is_err()
                || std::io::stdout().flush().is_err()
                || std::io::stderr()
                    .write_all(&output.as_bytes()[midpoint..])
                    .is_err()
                || std::io::stderr().flush().is_err()
            {
                let _ = runtime.send(
                    peer,
                    send,
                    STAGE5_C_PROBE_TAG | (1 << 16) | u64::from(id.as_raw()),
                    &[],
                );
                loop {
                    let _ = runtime.receive();
                }
            }
        }
        #[cfg(not(feature = "std"))]
        for byte in output.bytes() {
            let payload = STAGE5_TTY_WRITE_TAG
                | (1 << 16)
                | ((byte as u64) << 24)
                | ((id.as_raw() as u64 & 0xff) << 32);
            if runtime.send(peer, send, payload, &[]).is_err() {
                break;
            }
            let _ = runtime.yield_task();
        }
        let status = STAGE5_C_PROBE_TAG | ((u64::from(!ok)) << 16) | (id.as_raw() as u64 & 0xff);
        let _ = runtime.send(peer, send, status, &[]);
    }
    // The manager receives the status, destroys this parked task, then frees
    // its memory. Never discard a still-queued outgoing status by self-destroy.
    loop {
        let _ = runtime.receive();
    }
}
