// SPDX-License-Identifier: MPL-2.0
// Adapted from Asterinas ostd/src/sync/spin.rs and ostd/src/irq/guard.rs,
// revision ceb65e3f850096d30283137845ac01ad51dfcdaf. See docs/architecture/asterinas-provenance.md.

//! Minimal IRQ-safe lock for the existing closure-based StaticCell API.
//! Acquisition/release and IRQ restoration follow Asterinas; no scheduler,
//! preemption guardian, CPU-local storage or alloc dependency is imported.

use core::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) struct Lock(AtomicBool);

impl Lock {
    pub(super) const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// A failed attempt restores the IRQ state through the local guard.
    #[inline(always)]
    pub(super) fn try_lock(&self) -> Option<Guard<'_>> {
        let irq = IrqGuard::new();
        self.0
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()?;
        Some(Guard {
            lock: self,
            _irq: irq,
        })
    }

    #[inline(always)]
    pub(super) fn lock(&self) -> Guard<'_> {
        // Disable local IRQs before acquiring: an IRQ on this CPU must not
        // spin on a lock held by the interrupted context.
        let irq = IrqGuard::new();
        while self
            .0
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        Guard {
            lock: self,
            _irq: irq,
        }
    }
}

#[must_use]
pub(super) struct Guard<'a> {
    lock: &'a Lock,
    _irq: IrqGuard,
}

impl Drop for Guard<'_> {
    #[inline(always)]
    fn drop(&mut self) {
        // Fields drop after this method: unlock before restoring local IRQs.
        self.lock.0.store(false, Ordering::Release);
    }
}

struct IrqGuard {
    was_enabled: bool,
    // IRQ state belongs to this CPU. Neither this guard nor Guard may cross
    // threads, even when the protected T is Send + Sync.
    _not_send_sync: PhantomData<*mut ()>,
}

impl IrqGuard {
    #[inline(always)]
    fn new() -> Self {
        Self {
            was_enabled: disable_local(),
            _not_send_sync: PhantomData,
        }
    }
}

impl Drop for IrqGuard {
    #[inline(always)]
    fn drop(&mut self) {
        if self.was_enabled {
            enable_local();
        }
    }
}

#[cfg(all(not(test), target_os = "none"))]
#[inline(always)]
fn disable_local() -> bool {
    let flags: u64;
    // SAFETY: PrimaOS bare-metal code runs at CPL0. The asm has a memory
    // clobber, so accesses cannot move before CLI. PUSHFQ/POP balance the stack.
    unsafe {
        core::arch::asm!("pushfq", "pop {}", "cli", out(reg) flags);
    }
    flags & (1 << 9) != 0
}

#[cfg(all(not(test), target_os = "none"))]
#[inline(always)]
fn enable_local() {
    // SAFETY: restore IF only if this CPU had it enabled on entry. NOP keeps
    // the STI interrupt shadow inside this asm block (as in Asterinas x86).
    unsafe {
        core::arch::asm!("sti", "nop", options(nostack));
    }
}

// Host consumers still get atomic exclusion, without privileged instructions.
#[cfg(all(not(test), not(target_os = "none")))]
#[inline(always)]
fn disable_local() -> bool {
    false
}
#[cfg(all(not(test), not(target_os = "none")))]
#[inline(always)]
fn enable_local() {}

// Model per-CPU IF only for host unit tests; QEMU executes the real asm above.
#[cfg(test)]
std::thread_local! { static IRQ_ENABLED: core::cell::Cell<bool> = const { core::cell::Cell::new(true) }; }
#[cfg(test)]
#[inline(always)]
fn disable_local() -> bool {
    IRQ_ENABLED.with(|flag| flag.replace(false))
}
#[cfg(test)]
#[inline(always)]
fn enable_local() {
    IRQ_ENABLED.with(|flag| flag.set(true));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_guards_restore_only_the_original_interrupt_state() {
        for initially_enabled in [false, true] {
            IRQ_ENABLED.with(|flag| flag.set(initially_enabled));
            let first = Lock::new();
            let second = Lock::new();
            let outer = first.lock();
            assert!(!IRQ_ENABLED.with(|flag| flag.get()));
            let inner = second.lock();
            drop(inner);
            assert!(!IRQ_ENABLED.with(|flag| flag.get()));
            assert!(!second.0.load(Ordering::Relaxed));
            drop(outer);
            assert_eq!(IRQ_ENABLED.with(|flag| flag.get()), initially_enabled);
            assert!(!first.0.load(Ordering::Relaxed));
        }
        IRQ_ENABLED.with(|flag| flag.set(true));
    }

    #[test]
    fn unwinding_unlocks_and_restores_interrupts() {
        let lock = Lock::new();
        let result = std::panic::catch_unwind(|| {
            let _guard = lock.lock();
            assert!(!IRQ_ENABLED.with(|flag| flag.get()));
            panic!("test unwind");
        });
        assert!(result.is_err());
        assert!(!lock.0.load(Ordering::Relaxed));
        assert!(IRQ_ENABLED.with(|flag| flag.get()));
        drop(lock.lock());
    }

    #[test]
    fn once_initialization_and_failed_try_lock_restore_original_irq_state() {
        use super::super::Once;
        for initial in [false, true] {
            IRQ_ENABLED.with(|flag| flag.set(initial));
            let once = Once::new();
            assert_eq!(
                once.get_or_try_init(|| {
                    assert!(!IRQ_ENABLED.with(|flag| flag.get()));
                    assert!(once.get().is_none());
                    assert_eq!(once.set(5), Err(5));
                    assert!(!IRQ_ENABLED.with(|flag| flag.get()));
                    Err::<u8, _>(())
                }),
                Err(())
            );
            assert_eq!(IRQ_ENABLED.with(|flag| flag.get()), initial);
            assert!(
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    once.get_or_init(|| {
                        assert!(!IRQ_ENABLED.with(|flag| flag.get()));
                        panic!("test once unwind");
                    });
                }))
                .is_err()
            );
            assert_eq!(IRQ_ENABLED.with(|flag| flag.get()), initial);
            assert_eq!(
                once.get_or_init(|| {
                    assert!(!IRQ_ENABLED.with(|flag| flag.get()));
                    7
                }),
                &7
            );
            assert_eq!(IRQ_ENABLED.with(|flag| flag.get()), initial);
            assert_eq!(once.get_or_init(|| panic!("already ready")), &7);
            assert_eq!(once.set(9), Err(9));
            assert_eq!(IRQ_ENABLED.with(|flag| flag.get()), initial);
        }
        IRQ_ENABLED.with(|flag| flag.set(true));
    }
}
