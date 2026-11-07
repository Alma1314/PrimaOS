//! N1: native int80 user-wrapper, reusing the opt-in probe's emitter and pages.
use super::*;

const COUNT: usize = 128;
const STRIDE: u64 = 24;
const _: () = assert!(COUNT as u64 * STRIDE <= 4096);
static AUDIT_START: StaticCell<usize> = StaticCell::new(0);

pub(super) fn enter(offset: u64, terminal: extern "C" fn() -> !) -> ! {
    sched::clear_kernel_resume_frame(2);
    BOOTSTRAP_STATE.with(|slot| {
        let state = slot.as_ref().unwrap();
        assert!(!state.task_is_gated(2));
        AUDIT_START.with_mut(|start| *start = state.audit.count);
    });
    let mut client = Code::new();
    client.mov(3, user::USER2_Y_VA);
    client.mov(13, COUNT as u64);
    let start = client.len;
    user_timestamp(&mut client, 0);
    client.call(SYS_TASK_ID, [0; 6]);
    client.emit(&[0x49, 0x89, 0xc7]); // r15 = return value before RDTSC clobbers rax
    user_timestamp(&mut client, 8);
    client.store(15, 16);
    client.mov(14, 2);
    client.emit(&[0x4d, 0x39, 0xf7, 0x74, 0x02, 0x0f, 0x0b]); // cmp r15, r14
    client.emit(&[0x48, 0x83, 0xc3, STRIDE as u8]);
    repeat(&mut client, 13, start);
    client.emit(&[0xcc, 0x0f, 0x0b]);
    install(2, user::USER2_CODE_VA, &client, offset);
    assert!(crate::preempt::start_time_counter());
    crate::idt::set_user_trap_handler(terminal);
    sched::init();
    sched::set_frame(2, user::task2_frame());
    sched::switch_to(2)
}

fn receipt(index: usize, field: u64) -> u64 {
    read_word(2, user::USER2_Y_VA + index as u64 * STRIDE + field * 8)
}

pub(super) fn verify() {
    assert_eq!(sched::current(), 2);
    let mut previous = 0;
    for index in 0..COUNT {
        let begin = receipt(index, 0);
        let end = receipt(index, 1);
        assert!(begin > 0 && begin >= previous && end >= begin);
        assert_eq!(receipt(index, 2), 2);
        previous = end;
    }
    BOOTSTRAP_STATE.with(|slot| {
        let state = slot.as_ref().unwrap();
        assert!(!state.task_is_gated(2));
        assert!(state.gate_well_formed());
        assert!(state.completion_waits_well_formed());
        for index in AUDIT_START.with(|start| *start)..state.audit.count {
            assert!(!matches!(
                state.audit.entry(index).unwrap(),
                AuditEvent::GateTrapped { .. }
                    | AuditEvent::GateDelivered { .. }
                    | AuditEvent::GateTerminated { .. }
                    | AuditEvent::GateControlRejected { .. }
            ));
        }
    });
}

pub(super) fn emit(mut write: impl FnMut(&[u8])) {
    write(b"S7NATIVE|samples=128;hz=");
    decimal(crate::preempt::timestamp_hz().unwrap(), &mut write);
    write(b";boundary=user-wrapper;number=7;task=2;gate=0;verified=1\n");
    for index in 0..COUNT {
        write(b"S7NTIME|");
        decimal(index as u64, &mut write);
        for field in 0..3 {
            write(b";");
            decimal(receipt(index, field), &mut write);
        }
        write(b"\n");
    }
}
