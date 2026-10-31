//! Opt-in G1 hardware witness. Reuses the two platform Tasks and their private
//! code/data pages; ordinary images contain neither these programs nor hooks.
use super::*;
use crate::{sched, syscall, user, vspace};
use prima_abi::syscall::*;

const REPLY: u64 = 0x1234_5678_9abc_def0;
const ABORT: u64 = 0xfedc_ba98_7654_3210;
const ORPHAN: u64 = 0xaabb_ccdd_eeff_1122;
const ARGS: [u64; 6] = [11, 22, 33, 44, 55, 66];
const NUMBERS: [u64; 2] = [SYS_TASK_ID, SYS_GATE_ATTACH];
static EXPECTED: StaticCell<[[u64; 17]; 2]> = StaticCell::new([[0; 17]; 2]);
static ADMISSION: StaticCell<Option<(u64, u64, [usize; 3], extern "C" fn() -> !)>> =
    StaticCell::new(None);

// Tiny byte emitter, matching user.rs's existing platform witnesses. No toolchain
// assembler or executable writable user mapping is introduced.
struct Code {
    bytes: [u8; 2048],
    len: usize,
}
impl Code {
    fn new() -> Self {
        Self {
            bytes: [0; 2048],
            len: 0,
        }
    }
    fn emit(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.bytes[self.len] = *byte;
            self.len += 1;
        }
    }
    fn mov(&mut self, reg: usize, value: u64) {
        self.emit(&[0x48 | (reg >> 3) as u8, 0xb8 | (reg & 7) as u8]);
        self.emit(&value.to_le_bytes());
    }
    fn store(&mut self, reg: usize, at: u32) {
        self.emit(&[
            0x48 | ((reg >> 3) << 2) as u8,
            0x89,
            0x83 | ((reg & 7) << 3) as u8,
        ]);
        self.emit(&at.to_le_bytes());
    }
    fn load(&mut self, reg: usize, at: u32) {
        self.emit(&[
            0x48 | ((reg >> 3) << 2) as u8,
            0x8b,
            0x83 | ((reg & 7) << 3) as u8,
        ]);
        self.emit(&at.to_le_bytes());
    }
    fn call(&mut self, number: u64, args: [u64; 6]) {
        for (reg, value) in [7, 6, 2, 10, 8, 9].into_iter().zip(args) {
            self.mov(reg, value);
        }
        self.mov(0, number);
        self.emit(&[0xcd, 0x80]);
    }
    fn gate_wait(&mut self, pointer: u64) {
        self.call(SYS_GATE_WAIT, [0, pointer, gate::OUTPUT_BYTES, 0, 0, 0]);
        // rdi = r12 (Gate handle), replacing the zero just emitted.
        let at = self.len - 2;
        self.len = at;
        self.emit(&[0x4c, 0x89, 0xe7, 0xcd, 0x80]);
    }
    fn finish_request(&mut self, number: u64, offset: u32, value: u64) {
        self.call(number, [0, value, 0, 0, 0, 0]);
        self.len -= 2;
        self.load(7, offset + 16);
        self.emit(&[0xcd, 0x80]);
    }
    fn snapshot(&mut self, at: u32) {
        // Architectural register number is also its output slot. The receipt
        // base (rbx) is captured too; these mov/push/pop instructions preserve flags.
        for reg in 0..16 {
            if reg != 4 {
                self.store(reg, at + reg as u32 * 8);
            }
        }
        self.emit(&[0x48, 0x89, 0xe0]);
        self.store(0, at + 4 * 8); // rsp
        self.emit(&[0x9c, 0x58]);
        self.store(0, at + 16 * 8); // rflags
    }
}

fn install(task: u32, va: u64, code: &Code, offset: u64) {
    let root = sched::root(task).expect("probe Task root");
    let leaf = vspace::read_user_leaf(root, va, |p| crate::mem::phys_to_virt(p, offset)).unwrap();
    let physical = leaf & 0x000f_ffff_ffff_f000;
    for (index, byte) in code.bytes[..code.len].iter().enumerate() {
        prima_ostd::mm::write_u8_at(
            crate::mem::phys_to_virt(physical, offset),
            index as u64,
            *byte,
        );
    }
}

/// Called after ordinary bring-up, before either Task executes user code.
pub fn enter(offset: u64, terminal: extern "C" fn() -> !) -> ! {
    let task_cap = BOOTSTRAP_STATE.with_mut(|slot| {
        let state = slot.as_mut().unwrap();
        assert!(state.audit.can_append(1));
        let cap = state
            .mint_root_capability(
                1,
                Rights::from_rights(&[Right::TaskManageVSpace]),
                ObjectRef::Task(2),
            )
            .unwrap();
        state
            .audit
            .append(AuditEvent::CapabilityMinted { cap, holder: 1 });
        cap.encode()
    });
    let mut service = Code::new();
    service.mov(3, user::USER_Y_VA);
    service.call(SYS_GATE_ATTACH, [task_cap, 0, ORPHAN, 1, 0, 0]);
    service.store(0, 512);
    service.call(
        SYS_GATE_WAIT,
        [0, user::USER_Y_VA, gate::OUTPUT_BYTES, 0, 0, 0],
    );
    service.store(0, 624); // malformed local handle, before capability lookup
    service.call(
        SYS_GATE_WAIT,
        [task_cap, user::USER_Y_VA, gate::OUTPUT_BYTES, 0, 0, 0],
    );
    service.store(0, 616); // wrong object/right, despite a valid held handle
    // A Ready target with an exception-resume continuation must be rejected.
    service.call(SYS_GATE_ATTACH, [task_cap, 0, ORPHAN, 0, 0, 0]);
    service.store(0, 600);
    service.emit(&[0xcc]);
    let continuation = user::USER_CODE_VA + service.len as u64;
    service.call(SYS_GATE_ATTACH, [task_cap, 0, ORPHAN, 0, 0, 0]);
    service.emit(&[0x49, 0x89, 0xc4]); // r12 = Gate handle
    service.gate_wait(user::USER_Y_VA);
    service.store(0, 520); // empty -> retry
    service.finish_request(SYS_GATE_REPLY, 0, REPLY);
    service.store(0, 528);
    service.finish_request(SYS_GATE_REPLY, 0, REPLY);
    service.store(0, 536); // stale
    service.call(SYS_YIELD, [0; 6]);
    service.gate_wait(0x4000);
    service.store(0, 544); // unmapped; queued request stays
    service.gate_wait(user::USER_Y_VA + 4096 - 48);
    service.store(0, 552); // partial page
    service.gate_wait(user::USER_CODE_VA);
    service.store(0, 560); // read-only
    service.gate_wait(user::USER_Y_VA + 128);
    service.store(0, 568);
    service.finish_request(SYS_GATE_ABORT, 128, ABORT);
    service.store(0, 576);
    service.finish_request(SYS_GATE_ABORT, 128, ABORT);
    service.len -= 2;
    service.emit(&[0x0f, 0x05]); // native fast entry rejection shares auditing
    service.store(0, 608);
    service.call(SYS_CAP_DROP, [0; 6]);
    service.len -= 2;
    service.emit(&[0x4c, 0x89, 0xe7, 0xcd, 0x80]);
    service.store(0, 584);
    service.call(SYS_YIELD, [0; 6]);
    service.emit(&[0x0f, 0x0b]); // must not resume before terminal client int3

    let mut client = Code::new();
    let mut expected = [[0; 17]; 2];
    for round in 0..2 {
        let mut regs = [0u64; 17];
        for (reg, value) in regs[..16].iter_mut().enumerate() {
            *value = 0x1000 + reg as u64;
        }
        regs[3] = user::USER2_Y_VA;
        regs[4] = user::USER2_STACK_VA + 4096;
        for (reg, value) in [7, 6, 2, 10, 8, 9].into_iter().zip(ARGS) {
            regs[reg] = value;
        }
        regs[0] = NUMBERS[round];
        for (reg, value) in regs[..16].iter().copied().enumerate() {
            if reg != 4 {
                client.mov(reg, value);
            }
        }
        client.emit(if round == 0 {
            &[0xcd, 0x80]
        } else {
            &[0x0f, 0x05]
        });
        regs[0] = if round == 0 { REPLY } else { ABORT };
        regs[16] = 2; // initial IF=0 and all preceding instructions are mov
        if round == 1 {
            regs[1] = user::USER2_CODE_VA + client.len as u64;
            regs[11] = 2;
        }
        expected[round] = regs;
        client.snapshot(round as u32 * 160);
    }
    client.call(SYS_TASK_ID, [0; 6]);
    client.store(0, 320); // orphan must never return native id=2
    client.emit(&[0xcc, 0x0f, 0x0b]);
    EXPECTED.with_mut(|slot| *slot = expected);
    install(1, user::USER_CODE_VA, &service, offset);
    install(2, user::USER2_CODE_VA, &client, offset);
    let counts = BOOTSTRAP_STATE.with(|slot| {
        let state = slot.as_ref().unwrap();
        [
            state.audit.count,
            state.slots.live_count(),
            state.wait_objects.count,
        ]
    });
    ADMISSION.with_mut(|slot| *slot = Some((continuation, task_cap, counts, terminal)));
    sched::set_kernel_resume_frame(2, user::task2_frame());
    crate::idt::set_user_trap_handler(admission_checkpoint);
    crate::signal::set_pending(2, 10);
    sched::init();
    sched::set_frame(1, user::task1_frame());
    sched::set_frame(2, user::task2_frame());
    sched::resume(1)
}

/// Probe-only checkpoint: verify rejection before consuming the injected native
/// continuation, then resume the service at the next known program location.
extern "C" fn admission_checkpoint() -> ! {
    assert_eq!(sched::current(), 1);
    let (rip, task_cap, counts, terminal) = ADMISSION.with_mut(|slot| slot.take().unwrap());
    assert_eq!(
        read_word(1, user::USER_Y_VA + 600),
        syscall::encode_errno(syscall::Errno::Again)
    );
    assert_eq!(sched::kernel_resume_frame(2), Some(user::task2_frame()));
    assert_eq!(sched::frame(2), Some(user::task2_frame()));
    BOOTSTRAP_STATE.with(|slot| {
        let state = slot.as_ref().unwrap();
        assert_eq!(state.tasks.status_of(2), Some(TaskStatus::Ready));
        for (index, (number, args, error)) in [
            (
                SYS_GATE_ATTACH,
                [task_cap, 0, ORPHAN, 1, 0, 0],
                syscall::Errno::Invalid,
            ),
            (
                SYS_GATE_WAIT,
                [0, user::USER_Y_VA, gate::OUTPUT_BYTES, 0, 0, 0],
                syscall::Errno::Invalid,
            ),
            (
                SYS_GATE_WAIT,
                [task_cap, user::USER_Y_VA, gate::OUTPUT_BYTES, 0, 0, 0],
                syscall::Errno::Access,
            ),
            (
                SYS_GATE_ATTACH,
                [task_cap, 0, ORPHAN, 0, 0, 0],
                syscall::Errno::Again,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                state.audit.entry(counts[0] + index),
                Some(AuditEvent::GateControlRejected {
                    caller: 1,
                    caller_cookie: state.task_cookie(1).unwrap(),
                    number,
                    args,
                    result: syscall::encode_errno(error),
                })
            );
        }
        assert_eq!(
            [
                state.audit.count,
                state.slots.live_count(),
                state.wait_objects.count
            ],
            [counts[0] + 4, counts[1], counts[2]]
        );
        assert_eq!(state.gate_tables().gate_count, 0);
        assert_eq!(state.gate_tables().binding_count, 0);
    });
    sched::clear_kernel_resume_frame(2);
    let mut frame = user::task1_frame();
    frame.rip = rip;
    frame.rbx = user::USER_Y_VA;
    sched::set_frame(1, frame);
    crate::idt::set_user_trap_handler(terminal);
    sched::resume(1)
}

fn read_word(task: u32, pointer: u64) -> u64 {
    let (_, offset) = crate::cow::root_and_offset().unwrap();
    let mut bytes = [0; 8];
    vspace::read_user_bytes(sched::root(task).unwrap(), pointer, &mut bytes, |p| {
        crate::mem::phys_to_virt(p, offset)
    })
    .unwrap();
    u64::from_le_bytes(bytes)
}

/// Verify the real user receipts, saved frames, queues/waits and audit chain.
/// Any mismatch panics instead of emitting the success record.
pub fn verify() {
    assert_eq!(sched::current(), 2);
    assert_eq!(crate::signal::pending(2), 1 << 9, "Gate v1 retains signals");
    let read = read_word;
    let expected = EXPECTED.with(|slot| *slot);
    for (round, regs) in expected.iter().enumerate() {
        for (reg, value) in regs.iter().enumerate() {
            assert_eq!(
                read(2, user::USER2_Y_VA + (round * 160 + reg * 8) as u64),
                *value,
                "round {round} register {reg}"
            );
        }
    }
    assert_eq!(read(2, user::USER2_Y_VA + 320), ORPHAN);
    assert_eq!(
        read(1, user::USER_Y_VA + 624),
        syscall::encode_errno(syscall::Errno::Invalid)
    );
    assert_eq!(
        read(1, user::USER_Y_VA + 608),
        syscall::encode_errno(syscall::Errno::NoEnt)
    );
    assert_eq!(
        read(1, user::USER_Y_VA + 616),
        syscall::encode_errno(syscall::Errno::Access)
    );
    for (index, expected) in [
        syscall::encode_errno(syscall::Errno::Invalid),
        0,
        0,
        syscall::encode_errno(syscall::Errno::NoEnt),
        syscall::encode_errno(syscall::Errno::Fault),
        syscall::encode_errno(syscall::Errno::Fault),
        syscall::encode_errno(syscall::Errno::Fault),
        0,
        0,
        0,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            read(1, user::USER_Y_VA + 512 + index as u64 * 8),
            expected,
            "service result {index}"
        );
    }
    for (round, number) in NUMBERS.into_iter().enumerate() {
        let pointer = user::USER_Y_VA + round as u64 * 128;
        assert_eq!(read(1, pointer), 1);
        assert_eq!(read(1, pointer + 8), 96);
        assert_ne!(read(1, pointer + 16), 0);
        assert_eq!(read(1, pointer + 32), number);
        for (index, arg) in ARGS.into_iter().enumerate() {
            assert_eq!(read(1, pointer + 40 + index as u64 * 8), arg);
        }
        assert_eq!(read(1, pointer + 88), if round == 0 { 1 } else { 0 });
    }
    BOOTSTRAP_STATE.with(|slot| {
        let state = slot.as_ref().unwrap();
        assert!(state.gate_well_formed());
        assert!(state.completion_waits_well_formed());
        let cookie = state.task_cookie(2).unwrap();
        for round in 0..2 {
            assert_eq!(read(1, user::USER_Y_VA + round * 128 + 24), cookie);
        }
        let mut trapped = 0;
        let mut delivered = 0;
        let mut terminal = 0;
        let mut blocked_service = 0;
        let mut rejected = 0;
        let gate_cap = state.gate_tables().find(1).unwrap().root_cap.encode();
        let expected_rejections = [
            (
                SYS_GATE_REPLY,
                [read(1, user::USER_Y_VA + 16), REPLY, 0, 0, 0, 0],
                syscall::Errno::NoEnt,
            ),
            (
                SYS_GATE_WAIT,
                [gate_cap, 0x4000, gate::OUTPUT_BYTES, 0, 0, 0],
                syscall::Errno::Fault,
            ),
            (
                SYS_GATE_WAIT,
                [
                    gate_cap,
                    user::USER_Y_VA + 4096 - 48,
                    gate::OUTPUT_BYTES,
                    0,
                    0,
                    0,
                ],
                syscall::Errno::Fault,
            ),
            (
                SYS_GATE_WAIT,
                [gate_cap, user::USER_CODE_VA, gate::OUTPUT_BYTES, 0, 0, 0],
                syscall::Errno::Fault,
            ),
            (
                SYS_GATE_ABORT,
                [read(1, user::USER_Y_VA + 128 + 16), ABORT, 0, 0, 0, 0],
                syscall::Errno::NoEnt,
            ),
        ];
        for index in 0..state.audit.count {
            let event = state.audit.entry(index).unwrap();
            match event {
                AuditEvent::GateTrapped { .. } => trapped += 1,
                AuditEvent::GateDelivered { .. } => delivered += 1,
                AuditEvent::GateTerminated { .. } => terminal += 1,
                AuditEvent::GateControlRejected { .. } => {
                    // The first four were checked at the admission checkpoint.
                    if rejected >= 4 {
                        let (number, args, error) = expected_rejections[rejected - 4];
                        assert_eq!(
                            event,
                            AuditEvent::GateControlRejected {
                                caller: 1,
                                caller_cookie: state.task_cookie(1).unwrap(),
                                number,
                                args,
                                result: syscall::encode_errno(error),
                            }
                        );
                    }
                    rejected += 1;
                }
                AuditEvent::TaskStateChanged {
                    task_id: 1,
                    status: TaskStatus::BlockedWait,
                    ..
                } => blocked_service += 1,
                _ => {}
            }
        }
        assert_eq!((trapped, delivered, terminal), (2, 2, 3));
        assert_eq!(blocked_service, 1);
        assert_eq!(rejected, 9);
        assert!(state.task_is_gated(2));
        assert_eq!(state.gate_tables().request(1), None);
        assert_eq!(state.gate_tables().request(2), None);
    });
}
