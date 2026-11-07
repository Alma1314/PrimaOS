//! I1: two authorized, ungated Tasks exchanging sequenced native messages.
use super::*;

const COUNT: usize = 64;
const CLIENT_STRIDE: u64 = 64;
const SERVICE_STRIDE: u64 = 48;
const REPLY_BASE: u64 = 65536;
const _: () = assert!(COUNT as u64 * CLIENT_STRIDE <= 4096);
const _: () = assert!(COUNT as u64 * SERVICE_STRIDE <= 4096);
static IDENTITIES: StaticCell<[u64; 2]> = StaticCell::new([0; 2]);
static AUDIT_START: StaticCell<usize> = StaticCell::new(0);
static PATH_START: StaticCell<[u64; 3]> = StaticCell::new([0; 3]);
static RECV_START: StaticCell<[u64; 6]> = StaticCell::new([0; 6]);

pub(super) fn enter(offset: u64, terminal: extern "C" fn() -> !) -> ! {
    sched::clear_kernel_resume_frame(2);
    let request_cap = BOOTSTRAP_STATE.with_mut(|slot| {
        let s = slot.as_mut().unwrap();
        IDENTITIES.with_mut(|ids| {
            for (index, id) in ids.iter_mut().enumerate() {
                let task = s.tasks.find(index as u32 + 1).unwrap();
                assert!(!s.task_is_gated(task.id));
                *id = ((task.generation as u64) << 32) | task.id as u64;
            }
        });
        let cap = s
            .mint_root_capability(
                2,
                Rights::from_rights(&[Right::TaskSend]),
                ObjectRef::Task(1),
            )
            .unwrap();
        s.audit
            .append(AuditEvent::CapabilityMinted { cap, holder: 2 });
        AUDIT_START.with_mut(|start| *start = s.audit.count);
        s.expose_user_handle(2, cap).encode()
    });
    let paths = syscall::ipc_witness();
    PATH_START.with_mut(|p| *p = [paths.direct, paths.queued, paths.blocks]);
    RECV_START.with_mut(|p| {
        *p = syscall::ipc_receive_counts();
        assert_eq!(p[5], 0);
    });

    let mut service = Code::new();
    service.mov(3, user::USER_Y_VA);
    service.mov(13, COUNT as u64);
    let service_loop = service.len;
    receive(&mut service, 8);
    service.store(0, 0);
    service.emit(&[0x4c, 0x39, 0xe8, 0x74, 0x02, 0x0f, 0x0b]); // rax == r13
    service.emit(&[0x49, 0x89, 0xc7]); // r15 = request sequence
    service.mov(14, REPLY_BASE);
    service.emit(&[0x4d, 0x01, 0xf7]); // r15 += r14
    service.call(SYS_SEND, [2, TASK2_SEND_CAP_HANDLE.encode(), 0, 0, 0, 0]);
    service.len -= 2;
    service.emit(&[0x4c, 0x89, 0xfa, 0xcd, 0x80]); // rdx = r15; int80
    send_ok(&mut service);
    service.emit(&[0x48, 0x83, 0xc3, SERVICE_STRIDE as u8]);
    repeat(&mut service, 13, service_loop);
    service.call(SYS_YIELD, [0; 6]);
    service.emit(&[0x0f, 0x0b]);

    let mut client = Code::new();
    client.mov(3, user::USER2_Y_VA);
    client.mov(13, COUNT as u64);
    let client_loop = client.len;
    user_timestamp(&mut client, 0);
    client.call(SYS_SEND, [1, request_cap, 0, 0, 0, 0]);
    client.len -= 2;
    client.emit(&[0x4c, 0x89, 0xea, 0xcd, 0x80]); // rdx = r13; int80
    send_ok(&mut client);
    receive(&mut client, 24);
    client.emit(&[0x49, 0x89, 0xc7]); // preserve reply across RDTSC
    user_timestamp(&mut client, 8);
    client.store(15, 16);
    client.mov(14, REPLY_BASE);
    client.emit(&[0x4d, 0x01, 0xee]); // r14 += r13
    client.emit(&[0x4d, 0x39, 0xf7, 0x74, 0x02, 0x0f, 0x0b]); // r15 == r14
    client.emit(&[0x48, 0x83, 0xc3, CLIENT_STRIDE as u8]);
    repeat(&mut client, 13, client_loop);
    client.emit(&[0xcc, 0x0f, 0x0b]);
    install(1, user::USER_CODE_VA, &service, offset);
    install(2, user::USER2_CODE_VA, &client, offset);
    assert!(crate::preempt::start_time_counter());
    crate::idt::set_user_trap_handler(terminal);
    sched::init();
    assert_eq!(sched::current(), 1);
    sched::set_frame(1, user::task1_frame());
    sched::set_frame(2, user::task2_frame());
    sched::resume(1)
}

fn receive(code: &mut Code, at: u32) {
    code.call(SYS_RECV_META, [0; 6]);
    code.len -= 2;
    code.emit(&[0x48, 0x8d, 0xbb]); // rdi = &this row's metadata
    code.emit(&at.to_le_bytes());
    code.emit(&[0xcd, 0x80]);
}

fn send_ok(code: &mut Code) {
    code.emit(&[0x48, 0x85, 0xc0, 0x74, 0x02, 0x0f, 0x0b]);
}

fn receipt(task: u32, index: usize, field: u64) -> u64 {
    let (base, stride) = if task == 1 {
        (user::USER_Y_VA, SERVICE_STRIDE)
    } else {
        (user::USER2_Y_VA, CLIENT_STRIDE)
    };
    read_word(task, base + index as u64 * stride + field * 8)
}

pub(super) fn verify() {
    assert_eq!(sched::current(), 2);
    let paths = syscall::ipc_witness();
    PATH_START.with(|p| {
        assert_eq!(
            [
                paths.direct - p[0],
                paths.queued - p[1],
                paths.blocks - p[2]
            ],
            [128, 0, 128]
        );
    });
    assert_eq!(syscall::ipc_receive_counts(), RECV_START.with(|p| *p));
    BOOTSTRAP_STATE.with(|slot| {
        let s = slot.as_ref().unwrap();
        assert!(!s.task_is_gated(1) && !s.task_is_gated(2));
        assert!(s.gate_well_formed() && s.completion_waits_well_formed());
        assert!(s.tasks.find(1).unwrap().pending.is_none());
        assert!(s.tasks.find(2).unwrap().pending.is_none());
        let ids = IDENTITIES.with(|ids| *ids);
        let start = AUDIT_START.with(|start| *start);
        let mut previous_time = 0;
        let mut previous_message = 0;
        for index in 0..COUNT {
            let begin = receipt(2, index, 0);
            let end = receipt(2, index, 1);
            assert!(begin > 0 && begin >= previous_time && end >= begin);
            previous_time = end;
            assert_eq!(receipt(1, index, 0), (COUNT - index) as u64);
            assert_eq!(receipt(2, index, 2), REPLY_BASE + (COUNT - index) as u64);
            for (receiver, metadata, sender_id) in [(1, 1, ids[1]), (2, 3, ids[0])] {
                for field in metadata..metadata + 3 {
                    assert_eq!(receipt(receiver, index, field), 0);
                }
                assert_eq!(receipt(receiver, index, metadata + 3), sender_id);
                let id = receipt(receiver, index, metadata + 4);
                assert!(id > previous_message && id <= u32::MAX as u64);
                previous_message = id;
                let mut sent = None;
                let mut received = None;
                for position in start..s.audit.count {
                    match s.audit.entry(position).unwrap() {
                        AuditEvent::MessageSent {
                            message_id,
                            sender,
                            receiver: to,
                        } if message_id as u64 == id => {
                            assert!(sent.replace(position).is_none());
                            assert_eq!((sender, to), (sender_id as u32, receiver));
                        }
                        AuditEvent::MessageReceived {
                            message_id,
                            receiver: to,
                        } if message_id as u64 == id => {
                            assert!(received.replace(position).is_none());
                            assert_eq!(to, receiver);
                        }
                        _ => {}
                    }
                }
                assert!(sent.unwrap() < received.unwrap());
            }
        }
        let mut counts = [0; 2];
        for position in start..s.audit.count {
            match s.audit.entry(position).unwrap() {
                AuditEvent::MessageSent { .. } => counts[0] += 1,
                AuditEvent::MessageReceived { .. } => counts[1] += 1,
                AuditEvent::GateTrapped { .. }
                | AuditEvent::GateDelivered { .. }
                | AuditEvent::GateTerminated { .. }
                | AuditEvent::GateControlRejected { .. } => panic!("IPC benchmark entered Gate"),
                _ => {}
            }
        }
        assert_eq!(counts, [128; 2]);
    });
}

pub(super) fn emit(mut write: impl FnMut(&[u8])) {
    write(b"S7IPC|samples=64;hz=");
    decimal(crate::preempt::timestamp_hz().unwrap(), &mut write);
    write(b";boundary=user-wrapper;client=");
    decimal(IDENTITIES.with(|ids| ids[1]), &mut write);
    write(b";service=");
    decimal(IDENTITIES.with(|ids| ids[0]), &mut write);
    write(b";direct=128;queued=0;blocks=128;verified=1\n");
    for index in 0..COUNT {
        write(b"S7ITIME|");
        decimal(index as u64, &mut write);
        for (task, fields) in [(2, 8), (1, 6)] {
            for field in 0..fields {
                write(b";");
                decimal(receipt(task, index, field), &mut write);
            }
        }
        write(b"\n");
    }
}
