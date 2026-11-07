//! Opt-in measurements through the production Gate path: G4 stops before iret;
//! G5 also records the complete user wrapper's endpoints in its private data page.
use super::*;

pub const SAMPLES: usize = 1000;
const USER_SAMPLES: usize = 128;
const _: () = assert!(USER_SAMPLES * 16 <= 4096);
static TIMES: StaticCell<[[u64; 5]; SAMPLES]> = StaticCell::new([[0; 5]; SAMPLES]);
static PROGRESS: StaticCell<Option<(usize, usize)>> = StaticCell::new(None);

pub fn selected() -> bool {
    matches!(option_env!("PRIMA_GATE_CASE"), Some("G4" | "G5"))
}

fn user_mode() -> bool {
    option_env!("PRIMA_GATE_CASE") == Some("G5")
}

fn sample_count() -> usize {
    if user_mode() { USER_SAMPLES } else { SAMPLES }
}

/// Phase 0/1 run on the client; 2/3 on its fixed service; 4 on client resume.
/// This feature contains no sampling in ordinary images. Phase assertions also
/// reject a benchmark that silently skips a call or measures an empty wait.
pub fn stamp(phase: usize, task: u32) {
    if !selected() {
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
        assert!(index < sample_count());
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

fn user_timestamp(code: &mut Code, at: u32) {
    // LFENCE; RDTSC; combine EDX:EAX into RAX, then save to [RBX + at].
    code.emit(&[
        0x0f, 0xae, 0xe8, 0x0f, 0x31, 0x48, 0xc1, 0xe2, 0x20, 0x48, 0x09, 0xd0,
    ]);
    code.store(0, at);
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
    service.mov(13, sample_count() as u64);
    let service_loop = service.len;
    service.gate_wait(user::USER_Y_VA);
    service.emit(&[0x48, 0x85, 0xc0, 0x74, 0x02, 0x0f, 0x0b]);
    service.finish_request(SYS_GATE_REPLY, 0, REPLY);
    service.emit(&[0x48, 0x85, 0xc0, 0x74, 0x02, 0x0f, 0x0b]);
    service.call(SYS_YIELD, [0; 6]);
    repeat(&mut service, 13, service_loop);
    service.emit(&[0x0f, 0x0b]);

    let mut client = Code::new();
    client.mov(13, sample_count() as u64);
    if user_mode() {
        client.mov(3, user::USER2_Y_VA);
    }
    let client_loop = client.len;
    if user_mode() {
        user_timestamp(&mut client, 0);
    }
    client.call(0x1234_5678, ARGS);
    if user_mode() {
        client.emit(&[0x49, 0x89, 0xc7]); // r15 = reply, before the ending RDTSC
        user_timestamp(&mut client, 8);
        client.mov(14, REPLY);
        client.emit(&[0x4d, 0x39, 0xf7, 0x74, 0x02, 0x0f, 0x0b]); // cmp r15, r14
        client.emit(&[0x48, 0x83, 0xc3, 16]); // next pair of u64 timestamps
    } else {
        client.mov(3, REPLY);
        client.emit(&[0x48, 0x39, 0xd8, 0x74, 0x02, 0x0f, 0x0b]);
    }
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
    let count = sample_count();
    assert_eq!(PROGRESS.with(|p| *p), Some((count, 0)));
    TIMES.with(|times| {
        let mut previous = 0;
        for (index, row) in times[..count].iter().enumerate() {
            assert!(row[0] > 0);
            assert!(row.windows(2).all(|w| w[0] <= w[1]));
            if user_mode() {
                let begin = read_word(2, user::USER2_Y_VA + index as u64 * 16);
                let end = read_word(2, user::USER2_Y_VA + index as u64 * 16 + 8);
                assert!(begin > 0 && begin >= previous && begin <= row[0]);
                assert!(row[4] <= end);
                previous = end;
            }
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
        assert_eq!(counts, [count; 3]);
        // G4 never derives request aliases: no delivery root may also occupy
        // the general arena, even if a faulty length query double-counts it.
        for index in 0..s.cap_history_count {
            let record = CAPABILITY_HISTORY_ARENA.get(s.cap_history, index).unwrap();
            assert!(!matches!(record.target, ObjectRef::GateRequest(_)));
        }
        assert_eq!(s.capability_history_len(), s.cap_history_count + count);
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
    let prefix: &[u8] = if user_mode() { b"S7" } else { b"S6" };
    let count = sample_count();
    write(prefix);
    write(b"HISTORY|deliveries=");
    decimal(count as u64, &mut write);
    write(b";audit_roots=");
    decimal(count as u64, &mut write);
    write(b";dedicated_slots_saved=");
    decimal(count as u64, &mut write);
    write(b";verified=1\n");
    write(prefix);
    write(b"BENCH|samples=");
    decimal(count as u64, &mut write);
    write(b";hz=");
    decimal(crate::preempt::timestamp_hz().unwrap(), &mut write);
    write(if user_mode() {
        b";boundary=user-wrapper;verified=1\n"
    } else {
        b";boundary=pre-iret;verified=1\n"
    });
    TIMES.with(|times| {
        for (index, row) in times[..count].iter().enumerate() {
            write(prefix);
            write(b"TIME|");
            decimal(index as u64, &mut write);
            if user_mode() {
                write(b";");
                decimal(
                    read_word(2, user::USER2_Y_VA + index as u64 * 16),
                    &mut write,
                );
            }
            for stamp in row {
                write(b";");
                decimal(*stamp, &mut write);
            }
            if user_mode() {
                write(b";");
                decimal(
                    read_word(2, user::USER2_Y_VA + index as u64 * 16 + 8),
                    &mut write,
                );
            }
            write(b"\n");
        }
    });
}
