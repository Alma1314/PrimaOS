//! P5-4-1: CPL0-only directed ping/pong, one outstanding target, no locks.
//! IRQ15/vector 0x2f is reserved only in this image; no device IRQ is unmasked.
//! This is transport evidence, not a shootdown or scheduler notification API.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

const VECTOR: u32 = 0x2f; // Fixed, physical destination, no destination shorthand.
const EOI: u64 = 0xb0;
const ACK_POLL_LIMIT: u32 = 10_000_000;
static LAPIC: AtomicU64 = AtomicU64::new(0);
static BSP_APIC: AtomicU32 = AtomicU32::new(0);
static FAILED: AtomicBool = AtomicBool::new(false);
static SILENT: AtomicBool = AtomicBool::new(false);
static RECEIVED: [AtomicU32; crate::cpu::local::MAX_CPUS] =
    [const { AtomicU32::new(0) }; crate::cpu::local::MAX_CPUS];
static REPLIED: [AtomicBool; crate::cpu::local::MAX_CPUS] =
    [const { AtomicBool::new(false) }; crate::cpu::local::MAX_CPUS];

pub(crate) fn prepare(lapic: u64, bsp_apic: u32) {
    LAPIC.store(lapic, Ordering::Relaxed);
    BSP_APIC.store(bsp_apic, Ordering::Relaxed);
}

pub(crate) fn enable_local() {
    crate::smp::enable_lapic(LAPIC.load(Ordering::Relaxed));
}

pub(crate) fn interrupt() {
    let cpu = crate::cpu::local::current_index();
    let lapic = LAPIC.load(Ordering::Relaxed);
    RECEIVED[cpu].fetch_add(1, Ordering::SeqCst);
    prima_ostd::mm::write_u32(lapic + EOI, 0);
    if cpu != crate::cpu::local::BSP
        && !SILENT.load(Ordering::SeqCst)
        && crate::smp::send_ipi(lapic, BSP_APIC.load(Ordering::Relaxed), VECTOR).is_err()
    {
        FAILED.store(true, Ordering::SeqCst);
    }
    REPLIED[cpu].store(true, Ordering::SeqCst);
}

/// Called by BSP with IF=0, after all APs publish online with LAPIC enabled.
/// Always returns with IF=0; neither ICR completion nor acknowledgement may spin forever.
pub fn run(report: &crate::smp_probe::Report, aps: u32) -> Result<(), &'static str> {
    if cfg!(feature = "entry-percpu-probe") {
        return Err("ipi-feature-conflict");
    }
    let lapic = LAPIC.load(Ordering::Relaxed);
    let mut index = 0usize;
    for order in 0..report.topology.count() {
        let apic = report.topology.apic_id(order).ok_or("ipi-topology")?;
        if apic == report.bsp_apic_id {
            continue;
        }
        index += 1;
        // BSP sends with IF=0; its pong handler never writes the ICR pair.
        crate::smp::send_ipi(lapic, apic, VECTOR).map_err(|_| "ipi-icr-timeout")?;
        wait_ack(index as u32, index)?;
        // Check both target and non-target counts after each round trip.
        if !counts_match(aps, index) {
            return Err("ipi-target-mismatch");
        }
    }
    if index as u32 != aps {
        return Err("ipi-topology");
    }
    // Same transport, target and wait path; suppress only the target's reply.
    // Require actual receipt on AP1 as well, so a lost ping cannot pass this test.
    SILENT.store(true, Ordering::SeqCst);
    let first_ap = (0..report.topology.count())
        .filter_map(|order| report.topology.apic_id(order))
        .find(|apic| *apic != report.bsp_apic_id)
        .ok_or("ipi-no-target")?;
    crate::smp::send_ipi(lapic, first_ap, VECTOR).map_err(|_| "ipi-icr-timeout")?;
    if wait_ack(aps + 1, 1) != Err("ipi-ack-timeout")
        || RECEIVED[1].load(Ordering::SeqCst) != 2
        || RECEIVED[0].load(Ordering::SeqCst) != aps
    {
        return Err("ipi-timeout-negative");
    }
    Ok(())
}

fn wait_ack(expected: u32, target: usize) -> Result<(), &'static str> {
    prima_ostd::arch::sti();
    let acknowledged = crate::smp::poll_bounded(ACK_POLL_LIMIT, || {
        FAILED.load(Ordering::SeqCst)
            || (RECEIVED[0].load(Ordering::SeqCst) >= expected
                && REPLIED[target].load(Ordering::SeqCst))
    });
    prima_ostd::arch::cli();
    if FAILED.load(Ordering::SeqCst) {
        Err("ipi-pong-icr-timeout")
    } else if !acknowledged {
        Err("ipi-ack-timeout")
    } else {
        Ok(())
    }
}

fn counts_match(aps: u32, completed: usize) -> bool {
    RECEIVED.iter().enumerate().all(|(cpu, count)| {
        let expected = if cpu == 0 {
            completed as u32
        } else {
            u32::from(cpu <= completed && cpu <= aps as usize)
        };
        count.load(Ordering::SeqCst) == expected
    })
}
