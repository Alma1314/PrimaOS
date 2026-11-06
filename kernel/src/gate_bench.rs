//! Opt-in, single-client 1000-call measurement through the production Gate path.
//! Samples stop immediately before the final iret trampoline, not in userspace.
use super::*;

pub const SAMPLES: usize = 1000;
static TIMES: StaticCell<[[u64; 5]; SAMPLES]> = StaticCell::new([[0; 5]; SAMPLES]);
static PROGRESS: StaticCell<Option<(usize, usize)>> = StaticCell::new(None);

/// Phase 0/1 run on the client; 2/3 on its fixed service; 4 on client resume.
/// This feature contains no sampling in ordinary images. Phase assertions also
/// reject a benchmark that silently skips a call or measures an empty wait.
pub fn stamp(phase: usize, task: u32) {
    if option_env!("PRIMA_GATE_CASE") != Some("G4") {
        return;
    }
    PROGRESS.with_mut(|progress| {
        let Some((index, expected)) = *progress else {
            return;
        };
        if phase == 4 && (task != 2 || expected != 4) {
            return; // initial client entry or service scheduling
        }
        assert_eq!(task, if phase == 2 || phase == 3 { 1 } else { 2 });
        assert_eq!(phase, expected);
        assert!(index < SAMPLES);
        let now = prima_ostd::arch::timestamp_counter();
        TIMES.with_mut(|times| times[index][phase] = now);
        *progress = Some(if phase == 4 {
            (index + 1, 0)
        } else {
            (index, expected + 1)
        });
    });
}

fn repeat(code: &mut Code, reg: usize, start: usize) {
    code.emit(&[0x48 | (reg >> 3) as u8, 0xff, 0xc8 | (reg & 7) as u8]);
    code.emit(&[0x0f, 0x85]);
    let delta = start as i32 - (code.len + 4) as i32;
    code.emit(&delta.to_le_bytes());
}

pub fn enter(offset: u64, terminal: extern "C" fn() -> !) -> ! {
    // Bring-up's exc_resume witness leaves a continuation in Task 2. G1 tests
    // its rejection explicitly; the measurement starts after that fixture.
    sched::clear_kernel_resume_frame(2);
    let task_cap = BOOTSTRAP_STATE.with_mut(|slot| {
        let s = slot.as_mut().unwrap();
        let cap = s
            .mint_root_capability(
                1,
                Rights::from_rights(&[Right::TaskManageVSpace]),
                ObjectRef::Task(2),
            )
            .unwrap();
        s.audit
            .append(AuditEvent::CapabilityMinted { cap, holder: 1 });
        cap.encode()
    });
    let mut service = Code::new();
    service.mov(3, user::USER_Y_VA);
    service.call(SYS_GATE_ATTACH, [task_cap, 0, ORPHAN, 0, 0, 0]);
    service.emit(&[0x48, 0x85, 0xc0, 0x79, 0x02, 0x0f, 0x0b]);
    service.emit(&[0x49, 0x89, 0xc4]); // r12 = Gate
    // Start with a queued client call, excluding initial empty-wait setup.
    service.call(SYS_YIELD, [0; 6]);
    service.mov(13, SAMPLES as u64);
    let service_loop = service.len;
    service.gate_wait(user::USER_Y_VA);
    service.emit(&[0x48, 0x85, 0xc0, 0x74, 0x02, 0x0f, 0x0b]);
    service.finish_request(SYS_GATE_REPLY, 0, REPLY);
    service.emit(&[0x48, 0x85, 0xc0, 0x74, 0x02, 0x0f, 0x0b]);
    service.call(SYS_YIELD, [0; 6]);
    repeat(&mut service, 13, service_loop);
    service.emit(&[0x0f, 0x0b]);

    let mut client = Code::new();
    client.mov(13, SAMPLES as u64);
    let client_loop = client.len;
    client.call(0x1234_5678, ARGS);
    client.mov(3, REPLY);
    client.emit(&[0x48, 0x39, 0xd8, 0x74, 0x02, 0x0f, 0x0b]);
    repeat(&mut client, 13, client_loop);
    client.emit(&[0xcc, 0x0f, 0x0b]);
    install(1, user::USER_CODE_VA, &service, offset);
    install(2, user::USER2_CODE_VA, &client, offset);
    assert!(crate::preempt::start_time_counter());
    PROGRESS.with_mut(|p| *p = Some((0, 0)));
    crate::idt::set_user_trap_handler(terminal);
    sched::init();
    sched::set_frame(1, user::task1_frame());
    sched::set_frame(2, user::task2_frame());
    sched::resume(1)
}

pub fn verify() {
    assert_eq!(sched::current(), 2);
    assert_eq!(PROGRESS.with(|p| *p), Some((SAMPLES, 0)));
    TIMES.with(|times| {
        for row in times {
            assert!(row[0] > 0);
            assert!(row.windows(2).all(|w| w[0] <= w[1]));
        }
    });
    BOOTSTRAP_STATE.with(|slot| {
        let s = slot.as_ref().unwrap();
        assert!(s.gate_well_formed());
        assert!(s.completion_waits_well_formed());
        let mut counts = [0; 3];
        let mut first = None;
        let mut last = None;
        for i in 0..s.audit.count {
            match s.audit.entry(i).unwrap() {
                AuditEvent::GateTrapped { .. } => counts[0] += 1,
                AuditEvent::GateDelivered { request, root, .. } => {
                    counts[1] += 1;
                    assert_eq!(root.target, ObjectRef::GateRequest(request));
                    assert_eq!(root.parent, None);
                    assert_eq!(root.rights, gate::REQUEST_ROOT_RIGHTS);
                    assert!(matches!(s.audit.entry(root.created_at as usize),
                        Some(AuditEvent::CapabilityMinted { cap, holder })
                            if cap == root.handle && holder == root.holder));
                    first.get_or_insert(root);
                    last = Some(root);
                }
                AuditEvent::GateTerminated {
                    reason: gate::Reason::Replied,
                    ..
                } => counts[2] += 1,
                AuditEvent::GateControlRejected { .. } => panic!("benchmark control rejected"),
                _ => {}
            }
        }
        assert_eq!(counts, [SAMPLES; 3]);
        // G4 never derives request aliases: no delivery root may also occupy
        // the general arena, even if a faulty length query double-counts it.
        for index in 0..s.cap_history_count {
            let record = CAPABILITY_HISTORY_ARENA.get(s.cap_history, index).unwrap();
            assert!(!matches!(record.target, ObjectRef::GateRequest(_)));
        }
        assert_eq!(s.capability_history_len(), s.cap_history_count + SAMPLES);
        for root in [first.unwrap(), last.unwrap()] {
            assert_eq!(s.capability_history_record(root.handle), Some(root));
            assert!(s.retired_gate_request_cap(root.handle));
        }
    });
}

/// Stream raw, ordered timestamps for independent host-side quantiles and sums.
pub fn emit(mut write: impl FnMut(&[u8])) {
    fn decimal(mut n: u64, write: &mut impl FnMut(&[u8])) {
        let mut bytes = [0; 20];
        let mut at = bytes.len();
        loop {
            at -= 1;
            bytes[at] = b'0' + (n % 10) as u8;
            n /= 10;
            if n == 0 {
                break;
            }
        }
        write(&bytes[at..]);
    }
    write(b"S6HISTORY|deliveries=1000;audit_roots=1000;dedicated_slots_saved=1000;verified=1\n");
    write(b"S6BENCH|samples=1000;hz=");
    decimal(crate::preempt::timestamp_hz().unwrap(), &mut write);
    write(b";boundary=pre-iret;verified=1\n");
    TIMES.with(|times| {
        for (index, row) in times.iter().enumerate() {
            write(b"S6TIME|");
            decimal(index as u64, &mut write);
            for stamp in row {
                write(b";");
                decimal(*stamp, &mut write);
            }
            write(b"\n");
        }
    });
}
