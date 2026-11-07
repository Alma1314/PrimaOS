//! C1: fresh shared pages, real user write faults and the production COW handler.
use super::*;
use crate::frames::{self, FrameOwner, FrameWitness};

const COUNT: usize = 64;
const BASE: u64 = 0x0000_0b00_0040_0000;
const STRIDE: u64 = 24;
const SENTINEL: u64 = 0x1234_5678_9abc_def0;
const _: () = assert!(COUNT as u64 * STRIDE <= 4096);
static FAULTS: StaticCell<usize> = StaticCell::new(0);
static FAULT_RIP: StaticCell<u64> = StaticCell::new(0);
static HARDWARE: StaticCell<[[u64; 2]; COUNT]> = StaticCell::new([[0; 2]; COUNT]);
static START: StaticCell<Option<(FrameWitness, usize, u32)>> = StaticCell::new(None);

fn pattern(page: usize, word: u64) -> u64 {
    crate::cow::pattern(word) | ((page as u64) << 16)
}

fn leaf(task: u32, index: usize, offset: u64) -> u64 {
    vspace::read_user_leaf(
        sched::root(task).unwrap(),
        BASE + index as u64 * 4096,
        |p| crate::mem::phys_to_virt(p, offset),
    )
    .unwrap()
}

pub(super) fn enter(offset: u64, terminal: extern "C" fn() -> !) -> ! {
    sched::clear_kernel_resume_frame(2);
    let access = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
    let seg = crate::bootstrap::memseg_create(1, COUNT, access, offset).unwrap();
    let segment = crate::bootstrap::segment_of_cap(seg).unwrap();
    let caps = BOOTSTRAP_STATE.with_mut(|slot| {
        let s = slot.as_mut().unwrap();
        [1, 2].map(|task| {
            assert!(!s.task_is_gated(task));
            let cap = s
                .mint_root_capability(
                    1,
                    Rights::from_rights(&[Right::TaskManageVSpace]),
                    ObjectRef::Task(task),
                )
                .unwrap();
            s.audit
                .append(AuditEvent::CapabilityMinted { cap, holder: 1 });
            cap
        })
    });
    for (task, task_cap) in [1, 2].into_iter().zip(caps) {
        crate::bootstrap::memseg_map(
            1,
            seg,
            task_cap,
            0,
            BASE,
            COUNT,
            if task == 1 {
                Rights::from_rights(&[Right::SegMapR])
            } else {
                access
            },
            sched::root(task).unwrap(),
            offset,
        )
        .unwrap();
    }
    let map = |p| crate::mem::phys_to_virt(p, offset);
    for index in 0..COUNT {
        let original = leaf(1, index, offset) & vspace::PT_ADDR_MASK;
        assert_eq!(leaf(2, index, offset) & vspace::PT_ADDR_MASK, original);
        for word in 0..512 {
            prima_ostd::mm::write_u64(map(original) + word * 8, pattern(index, word));
        }
        for task in [1, 2] {
            // Fixture only: same transform used by fork, without cloning the
            // receipt/stack pages or including fork setup in the timed interval.
            let protected = vspace::cow_leaf(leaf(task, index, offset));
            vspace::write_leaf_raw(
                sched::root(task).unwrap(),
                BASE + index as u64 * 4096,
                protected,
                map,
            )
            .unwrap();
            assert_eq!(
                protected
                    & (vspace::PT_WRITABLE
                        | vspace::PT_SHARED
                        | vspace::PT_COW
                        | vspace::PT_NO_EXECUTE),
                vspace::PT_SHARED
                    | vspace::PT_NO_EXECUTE
                    | if task == 2 { vspace::PT_COW } else { 0 }
            );
        }
    }
    let mut client = Code::new();
    client.mov(3, user::USER2_Y_VA);
    client.mov(12, BASE);
    client.mov(13, COUNT as u64);
    client.mov(14, 65536);
    client.mov(11, SENTINEL);
    let start = client.len;
    user_timestamp(&mut client, 0);
    FAULT_RIP.with_mut(|rip| *rip = user::USER2_CODE_VA + client.len as u64);
    client.emit(&[0x4d, 0x89, 0x34, 0x24]); // mov [r12], r14: fault, copy, retry
    user_timestamp(&mut client, 8);
    client.emit(&[0x49, 0x8b, 0x04, 0x24]); // read back [r12]
    client.store(0, 16);
    client.emit(&[0x4c, 0x39, 0xf0, 0x74, 0x02, 0x0f, 0x0b]); // rax == r14
    client.mov(15, SENTINEL);
    client.emit(&[0x4d, 0x39, 0xfb, 0x74, 0x02, 0x0f, 0x0b]); // r11 preserved
    client.emit(&[0x49, 0x81, 0xc4, 0x00, 0x10, 0x00, 0x00]); // r12 += 4096
    client.emit(&[0x49, 0xff, 0xc6]); // r14++
    client.emit(&[0x48, 0x83, 0xc3, STRIDE as u8]);
    repeat(&mut client, 13, start);
    client.emit(&[0xcc, 0x0f, 0x0b]);
    install(2, user::USER2_CODE_VA, &client, offset);
    let audit = BOOTSTRAP_STATE.with(|slot| slot.as_ref().unwrap().audit.count);
    START.with_mut(|s| *s = Some((frames::witness().unwrap(), audit, segment)));
    assert!(crate::preempt::start_time_counter());
    prima_ostd::entry::set_page_fault_handler(observe_fault);
    crate::idt::set_user_trap_handler(terminal);
    sched::init();
    sched::set_frame(2, user::task2_frame());
    sched::switch_to(2)
}

extern "C" fn observe_fault(frame: &crate::exc::SretFrame) -> u64 {
    let index = FAULTS.with(|n| *n);
    assert!(index < COUNT);
    assert_eq!(sched::current(), 2);
    assert_eq!(frame.cs & 3, 3);
    assert_eq!(crate::idt::fault_addr(), BASE + index as u64 * 4096);
    assert_eq!(crate::idt::last_fault_err(), 7);
    HARDWARE
        .with_mut(|rows| rows[index] = [crate::idt::fault_addr(), crate::idt::last_fault_err()]);
    assert_eq!(frame.rip, FAULT_RIP.with(|rip| *rip));
    assert_eq!(frame.rbx, user::USER2_Y_VA + index as u64 * STRIDE);
    assert_eq!(frame.r12, BASE + index as u64 * 4096);
    assert_eq!(frame.r13, (COUNT - index) as u64);
    assert_eq!(frame.r14, 65536 + index as u64);
    assert_eq!(frame.r11, SENTINEL);
    let rip = crate::cow::dispatch(frame);
    assert_eq!(rip, frame.rip);
    FAULTS.with_mut(|n| *n += 1);
    rip
}

fn receipt(index: usize, field: u64) -> u64 {
    read_word(2, user::USER2_Y_VA + index as u64 * STRIDE + field * 8)
}

pub(super) fn verify() {
    assert_eq!(sched::current(), 2);
    assert_eq!(FAULTS.with(|n| *n), COUNT);
    let (start, audit, segment) = START.with(|s| s.unwrap());
    let (_, offset) = crate::cow::root_and_offset().unwrap();
    let mut previous = 0;
    for index in 0..COUNT {
        let begin = receipt(index, 0);
        let end = receipt(index, 1);
        assert!(begin > 0 && begin >= previous && end >= begin);
        assert_eq!(receipt(index, 2), 65536 + index as u64);
        previous = end;
        let shared = leaf(1, index, offset);
        let private = leaf(2, index, offset);
        let mask = vspace::PT_WRITABLE | vspace::PT_COW | vspace::PT_SHARED | vspace::PT_NO_EXECUTE;
        assert_eq!(shared & mask, vspace::PT_SHARED | vspace::PT_NO_EXECUTE);
        assert_eq!(private & mask, vspace::PT_WRITABLE | vspace::PT_NO_EXECUTE);
        let shared = shared & vspace::PT_ADDR_MASK;
        let private = private & vspace::PT_ADDR_MASK;
        assert_ne!(shared, private);
        assert_eq!(
            frames::owner_of(shared).unwrap(),
            FrameOwner::Segment(segment)
        );
        assert_eq!(
            frames::owner_of(private).unwrap(),
            FrameOwner::Segment(segment)
        );
        for other in 0..COUNT {
            assert_ne!(private, leaf(1, other, offset) & vspace::PT_ADDR_MASK);
            if other != index {
                assert_ne!(private, leaf(2, other, offset) & vspace::PT_ADDR_MASK);
                assert_ne!(shared, leaf(1, other, offset) & vspace::PT_ADDR_MASK);
            }
        }
        for word in 0..512 {
            let read =
                |phys| prima_ostd::mm::read_u64(crate::mem::phys_to_virt(phys, offset) + word * 8);
            assert_eq!(read(shared), pattern(index, word));
            assert_eq!(
                read(private),
                if word == 0 {
                    65536 + index as u64
                } else {
                    pattern(index, word)
                }
            );
        }
    }
    let finish = frames::witness().unwrap();
    assert!(start.conserved && start.disjoint && finish.conserved && finish.disjoint);
    assert_eq!(
        finish,
        FrameWitness {
            free: start.free - COUNT as u64,
            segment_frames: start.segment_frames + COUNT as u64,
            ..start
        }
    );
    BOOTSTRAP_STATE.with(|slot| {
        let s = slot.as_ref().unwrap();
        assert!(!s.task_is_gated(1) && !s.task_is_gated(2));
        assert!(s.gate_well_formed() && s.completion_waits_well_formed());
        for i in audit..s.audit.count {
            assert!(!matches!(
                s.audit.entry(i).unwrap(),
                AuditEvent::GateTrapped { .. }
                    | AuditEvent::GateDelivered { .. }
                    | AuditEvent::GateTerminated { .. }
                    | AuditEvent::GateControlRejected { .. }
            ));
        }
    });
}

pub(super) fn emit(mut write: impl FnMut(&[u8])) {
    write(b"S7COW|samples=64;hz=");
    decimal(crate::preempt::timestamp_hz().unwrap(), &mut write);
    write(b";boundary=user-store;task=2;faults=64;observer=checked;verified=1\n");
    let (_, offset) = crate::cow::root_and_offset().unwrap();
    for index in 0..COUNT {
        write(b"S7CTIME|");
        decimal(index as u64, &mut write);
        for value in [
            receipt(index, 0),
            receipt(index, 1),
            receipt(index, 2),
            HARDWARE.with(|rows| rows[index][0]),
            HARDWARE.with(|rows| rows[index][1]),
            leaf(1, index, offset) & vspace::PT_ADDR_MASK,
            leaf(2, index, offset) & vspace::PT_ADDR_MASK,
        ] {
            write(b";");
            decimal(value, &mut write);
        }
        write(b"\n");
    }
}
