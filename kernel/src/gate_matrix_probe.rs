//! Opt-in G2/G3 machine witnesses. Set PRIMA_GATE_CASE to G2, G2-death or G3.
//! Checkpoints only orchestrate scheduling/inject faults; all controls and
//! client traps execute in ring3 through the production entry paths.
use super::*;
use crate::bootstrap;
const BASE: u64 = user::USER3_STACK_VA;
const DATA: u64 = user::USER2_Y_VA;
const VALUE: u64 = 0x8877_6655_4433_2211;
static RUN: StaticCell<Option<Run>> = StaticCell::new(None);
#[derive(Clone, Copy)]
struct Run {
    offset: u64,
    terminal: extern "C" fn() -> !,
    phase: u32,
    next: u32,
    end: u32,
    service_pc: [u64; 3],
    supervisor_pc: u64,
    caps: [u64; sched::MAX_TASKS],
    ids: [u32; sched::MAX_TASKS],
    results: [u64; 24],
    result_count: usize,
    leaves: [u64; 2],
}
fn mode() -> &'static str {
    option_env!("PRIMA_GATE_CASE").unwrap_or("G1")
}
// Logical clients are contiguous for the test matrix; real Task allocation
// skips the ordinary boot fault/recovery witnesses, which remain untouched.
fn actual(task: u32) -> u32 {
    if task < 3 {
        task
    } else {
        RUN.with(|r| r.unwrap().ids[task as usize])
    }
}
fn read_word(task: u32, pointer: u64) -> u64 {
    super::read_word(actual(task), pointer)
}
fn copy(code: &mut Code, number: u64, client: u64, len: u64, service: u64, slot: u32) {
    code.call(number, [0, client, len, service, 0, 0]);
    code.len -= 2;
    code.load(7, 16);
    code.emit(&[0xcd, 0x80]);
    code.store(0, 512 + slot * 8);
}
fn leaf(task: u32, va: u64) -> u64 {
    let r = RUN.with(|r| r.unwrap());
    vspace::read_leaf(sched::root(actual(task)).unwrap(), va, |p| {
        crate::mem::phys_to_virt(p, r.offset)
    })
    .unwrap()
}
fn set_leaf(task: u32, va: u64, value: u64) {
    let r = RUN.with(|r| r.unwrap());
    vspace::write_leaf_raw(sched::root(actual(task)).unwrap(), va, value, |p| {
        crate::mem::phys_to_virt(p, r.offset)
    })
    .unwrap();
}
fn fill(task: u32, va: u64, byte: u8, offset: u64) {
    let p = vspace::read_user_leaf(sched::root(actual(task)).unwrap(), va, |p| {
        crate::mem::phys_to_virt(p, offset)
    })
    .unwrap()
        & 0x000f_ffff_ffff_f000;
    for i in 0..512 {
        prima_ostd::mm::write_u64_at(
            crate::mem::phys_to_virt(p, offset),
            i,
            u64::from_le_bytes([byte; 8]),
        );
    }
}
fn frame(task: u32, pc: u64, base: u64) {
    let mut f = sched::frame(task).unwrap();
    f.rip = pc;
    f.rbx = base;
    if task == 2 {
        f.r12 = BOOTSTRAP_STATE.with(|s| {
            s.as_ref()
                .unwrap()
                .gate_tables()
                .find(1)
                .unwrap()
                .root_cap
                .encode()
        });
    }
    sched::set_frame(task, f);
}
fn resume(task: u32) -> ! {
    let task = actual(task);
    if task == sched::current() {
        sched::resume(task)
    }
    sched::switch_to(task)
}
fn check_state() {
    BOOTSTRAP_STATE.with(|slot| {
        let s = slot.as_ref().unwrap();
        assert!(s.gate_well_formed());
        assert!(s.completion_waits_well_formed());
    });
}
pub fn enter(offset: u64, terminal: extern "C" fn() -> !) -> ! {
    assert!(["G2", "G2-death", "G3"].contains(&mode()));
    let end = if mode() == "G3" { 3 } else { 12 };
    let created_end = if mode() == "G2" { 13 } else { end };
    let mut ids = [0; sched::MAX_TASKS];
    let mut caps = [0; sched::MAX_TASKS];
    let mut made_tasks = [None; sched::MAX_TASKS];
    let mut managers = [0; sched::MAX_TASKS];
    // Legitimate independent roots/Segments, created by the ordinary Task builder.
    BOOTSTRAP_STATE.with_mut(|slot| {
        let s = slot.as_mut().unwrap();
        let rx = Rights::from_rights(&[Right::SegMapR, Right::SegMapX]);
        let rw = Rights::from_rights(&[Right::SegMapR, Right::SegMapW]);
        let code = s.memseg_create(1, 1, rx, offset).unwrap();
        for task in 3..=created_end {
            let req = TaskCreateRequest {
                code_seg_cap: code,
                code_offset: 0,
                code_pages: 1,
                load_base: user::USER3_CODE_VA,
                entry: user::USER3_CODE_VA,
                stack_top: BASE + 8192,
                parent_ns_cap: NAMESPACE_CAP_HANDLE,
                root_rights: Rights::from_rights(&[Right::NsResolve]),
                initial_caps: [Handle::ZERO; 2],
                name: 0,
            };
            let made = s
                .task_create(1, &req, sched::root(1).unwrap(), offset)
                .unwrap();
            ids[task as usize] = made.task_id;
            managers[task as usize] = made.manager_cap.encode();
            let data = s.memseg_create(1, 2, rw, offset).unwrap();
            s.memseg_map(
                1,
                data,
                made.manager_cap,
                0,
                BASE,
                2,
                rw,
                made.root_phys,
                offset,
            )
            .unwrap();
            made_tasks[task as usize] = Some(made);
            let cap = s
                .mint_root_capability(2, MANAGER_TASK_RIGHTS, ObjectRef::Task(made.task_id))
                .unwrap();
            s.audit
                .append(AuditEvent::CapabilityMinted { cap, holder: 2 });
            caps[task as usize] = cap.encode();
        }
        let cap = s
            .mint_root_capability(1, MANAGER_TASK_RIGHTS, ObjectRef::Task(2))
            .unwrap();
        s.audit
            .append(AuditEvent::CapabilityMinted { cap, holder: 1 });
        caps[2] = cap.encode();
        managers[2] = cap.encode();
        if mode() == "G3" {
            let data = s.memseg_create(1, 2, rw, offset).unwrap();
            s.memseg_map(
                1,
                data,
                cap,
                0,
                BASE,
                2,
                rw,
                sched::root(2).unwrap(),
                offset,
            )
            .unwrap();
        }
    });
    // Registration resolves Task capabilities through the global State; keep
    // it outside the preparation borrow, just as the syscall adapter does.
    for made in made_tasks.into_iter().flatten() {
        user::register_created_task(
            made.task_id,
            made.root_phys,
            user::USER3_CODE_VA,
            BASE + 8192,
            made.self_cap.encode(),
            [0; 2],
            0,
        )
        .unwrap();
    }
    let mut service = Code::new();
    service.mov(3, DATA);
    for task in 3..=created_end {
        service.call(SYS_GATE_ATTACH, [caps[task as usize], 0, ORPHAN, 0, 0, 0]);
        if task != 3 {
            // all clients share the first Gate, never a fresh one
            let at = service.len - 2;
            service.len = at;
            service.emit(&[0x4c, 0x89, 0xe6, 0xcd, 0x80]); // rsi=r12
        }
        if task == 3 {
            service.emit(&[0x49, 0x89, 0xc4]);
        }
    }
    if mode() == "G2" {
        service.call(SYS_TASK_DESTROY, [caps[13], 0, 0, 0, 0, 0]);
        service.store(0, 800); // Idle binding teardown, before any client trap.
    }
    service.emit(&[0xcc]);
    let mut service_pc = [user::USER2_CODE_VA + service.len as u64, 0, 0];
    service.gate_wait(DATA);
    service.emit(&[0xcc]);
    service_pc[1] = user::USER2_CODE_VA + service.len as u64;
    let mut results = [0; 24];
    let mut n = 0;
    if mode() == "G3" {
        for (num, client, len, local, expected) in [
            (SYS_GATE_COPY_IN, BASE + 8, 4096, BASE + 8, 4096),
            (SYS_GATE_COPY_OUT, BASE + 8, 4096, BASE + 8, 4096),
            (SYS_GATE_COPY_IN, u64::MAX, 0, u64::MAX, 0),
            (SYS_GATE_COPY_OUT, u64::MAX, 0, u64::MAX, 0),
            (
                SYS_GATE_COPY_IN,
                BASE,
                4097,
                BASE,
                syscall::encode_errno(syscall::Errno::Invalid),
            ),
            (
                SYS_GATE_COPY_OUT,
                u64::MAX - 1,
                8,
                BASE,
                syscall::encode_errno(syscall::Errno::Fault),
            ),
            (
                SYS_GATE_COPY_IN,
                1 << 47,
                8,
                BASE,
                syscall::encode_errno(syscall::Errno::Fault),
            ),
            (
                SYS_GATE_COPY_IN,
                0x4000,
                8,
                BASE,
                syscall::encode_errno(syscall::Errno::Fault),
            ),
            (
                SYS_GATE_COPY_OUT,
                user::USER3_CODE_VA,
                8,
                BASE,
                syscall::encode_errno(syscall::Errno::Fault),
            ),
            (
                SYS_GATE_COPY_IN,
                BASE,
                8,
                user::USER2_CODE_VA,
                syscall::encode_errno(syscall::Errno::Fault),
            ),
            (
                SYS_GATE_COPY_IN,
                BASE,
                16,
                BASE + 8184,
                syscall::encode_errno(syscall::Errno::Fault),
            ),
            (
                SYS_GATE_COPY_OUT,
                BASE + 8184,
                16,
                BASE,
                syscall::encode_errno(syscall::Errno::Fault),
            ),
        ] {
            if n == 1 {
                // Modify copied service bytes before the copy-out.
                service.mov(3, BASE);
                service.mov(0, VALUE);
                service.store(0, 8);
                service.mov(3, DATA);
            }
            copy(&mut service, num, client, len, local, n as u32);
            results[n] = expected;
            n += 1;
        }
        service.emit(&[0xcc]);
        service_pc[2] = user::USER2_CODE_VA + service.len as u64;
        for num in [SYS_GATE_COPY_IN, SYS_GATE_COPY_OUT] {
            copy(&mut service, num, BASE + 4088, 16, BASE + 4088, n as u32);
            results[n] = syscall::encode_errno(syscall::Errno::Fault);
            n += 1;
        }
        service.finish_request(SYS_GATE_REPLY, 0, REPLY);
        service.store(0, 512 + n as u32 * 8);
        results[n] = 0;
        n += 1;
        copy(&mut service, SYS_GATE_COPY_OUT, BASE, 8, BASE, n as u32);
        results[n] = syscall::encode_errno(syscall::Errno::NoEnt);
        n += 1;
    } else if mode() == "G2-death" {
        // Self-death in ring3 with Delivered + Queued + WaitingCapacity requests.
        let selfcap = BOOTSTRAP_STATE.with_mut(|slot| {
            let s = slot.as_mut().unwrap();
            let cap = s
                .mint_root_capability(2, MANAGER_TASK_RIGHTS, ObjectRef::Task(2))
                .unwrap();
            s.audit
                .append(AuditEvent::CapabilityMinted { cap, holder: 2 });
            cap.encode()
        });
        service.call(SYS_TASK_DESTROY, [selfcap, 0, 0, 0, 0, 0]);
        service.emit(&[0x0f, 0x0b]);
    } else {
        service.call(SYS_TASK_DESTROY, [caps[3], 0, 0, 0, 0, 0]);
        service.store(0, 512);
        results[n] = 0;
        n += 1;
        service.finish_request(SYS_GATE_REPLY, 0, REPLY);
        service.store(0, 520);
        results[n] = syscall::encode_errno(syscall::Errno::NoEnt);
        n += 1;
        copy(&mut service, SYS_GATE_COPY_IN, BASE, 0, BASE, 2);
        results[n] = syscall::encode_errno(syscall::Errno::NoEnt);
        n += 1;
        for (num, value) in [(SYS_GATE_REPLY, REPLY), (SYS_GATE_ABORT, ABORT)] {
            service.gate_wait(DATA);
            service.finish_request(num, 0, value);
            service.store(0, 512 + n as u32 * 8);
            results[n] = 0;
            n += 1;
            service.finish_request(num, 0, value);
            service.store(0, 512 + n as u32 * 8);
            results[n] = syscall::encode_errno(syscall::Errno::NoEnt);
            n += 1;
        }
        service.gate_wait(DATA); // intentionally never answers this Delivered request
    }
    service.emit(&[0xcc, 0x0f, 0x0b]);
    let mut client = Code::new();
    client.mov(3, BASE);
    client.call(SYS_TASK_ID, [1, 2, 3, 4, 5, 6]);
    client.store(0, 128);
    client.call(SYS_GATE_ATTACH, [6, 5, 4, 3, 2, 1]);
    client.store(0, 136);
    client.emit(&[0xcc, 0x0f, 0x0b]);
    let mut supervisor = Code::new();
    supervisor.mov(3, user::USER_Y_VA);
    supervisor.emit(&[0xcc]);
    let supervisor_pc = user::USER_CODE_VA + supervisor.len as u64;
    if mode() == "G3" {
        // Filled after delivery with a real service-owned handle: wrong holder.
        supervisor.call(SYS_GATE_COPY_IN, [0, BASE, 0, BASE, 0, 0]);
        supervisor.store(0, 512);
    } else {
        for (i, task) in [11usize, 5, 2].into_iter().enumerate() {
            supervisor.call(SYS_TASK_DESTROY, [managers[task], 0, 0, 0, 0, 0]);
            supervisor.store(0, 512 + i as u32 * 8);
            if task == 5 {
                supervisor.emit(&[0xcc]);
            }
        }
    }
    supervisor.emit(&[0xcc, 0x0f, 0x0b]);
    install(2, user::USER2_CODE_VA, &service, offset);
    install(3, user::USER3_CODE_VA, &client, offset);
    install(1, user::USER_CODE_VA, &supervisor, offset);
    for task in 3..=created_end {
        let id = ids[task as usize];
        let mut f = sched::frame(id).unwrap();
        f.rflags = 2;
        sched::set_frame(id, f);
    }
    RUN.with_mut(|r| {
        *r = Some(Run {
            offset,
            terminal,
            phase: 0,
            next: 3,
            end,
            service_pc,
            supervisor_pc,
            caps,
            ids,
            results,
            result_count: n,
            leaves: [0; 2],
        })
    });
    if mode() == "G3" {
        for task in [2, 3] {
            for page in [BASE, BASE + 4096] {
                fill(task, page, if task == 2 { 0x22 } else { 0x11 }, offset);
                set_leaf(
                    task,
                    page,
                    (leaf(task, page) & !vspace::PT_WRITABLE) | vspace::PT_COW,
                );
            }
        }
    }
    sched::init();
    sched::set_frame(1, user::task1_frame());
    sched::set_frame(2, user::task2_frame());
    crate::idt::set_user_trap_handler(checkpoint);
    sched::switch_to(2)
}
extern "C" fn checkpoint() -> ! {
    check_state();
    let mut r = RUN.with(|r| r.unwrap());
    match r.phase {
        0 => {
            // All attach controls completed before any client executes.
            assert_eq!(sched::current(), 2);
            BOOTSTRAP_STATE
                .with(|s| assert_eq!(s.as_ref().unwrap().gate_tables().binding_count, r.end - 2));
            for task in 3..=r.end {
                crate::signal::set_pending(actual(task), 10);
            }
            frame(2, r.service_pc[0], DATA);
            r.phase = 1;
            RUN.with_mut(|s| *s = Some(r));
            resume(3)
        }
        1 => {
            // Supervisor observes every actual trap before dequeue.
            assert_eq!(sched::current(), 1);
            BOOTSTRAP_STATE.with(|s| {
                let s = s.as_ref().unwrap();
                let req = s.gate_tables().request(r.next - 2).unwrap();
                assert_eq!(req.client, actual(r.next));
                assert_eq!(
                    req.phase,
                    if r.next <= 10 {
                        gate::Phase::Queued
                    } else {
                        gate::Phase::WaitingCapacity
                    }
                );
                assert_eq!(
                    s.tasks.status_of(actual(r.next)),
                    Some(TaskStatus::BlockedWait)
                );
            });
            crate::signal::set_pending(actual(r.next), 12);
            r.next += 1;
            if r.next <= r.end {
                let next = r.next;
                RUN.with_mut(|s| *s = Some(r));
                resume(next)
            }
            r.phase = if mode() == "G2" { 2 } else { 3 };
            RUN.with_mut(|s| *s = Some(r));
            if mode() == "G2" {
                frame(1, r.supervisor_pc, user::USER_Y_VA);
                resume(1)
            } else {
                resume(2)
            }
        }
        2 => {
            // WaitingCapacity cancellation must not overflow a still-full queue.
            assert_eq!(read_word(1, user::USER_Y_VA + 512), 0);
            assert_eq!(read_word(1, user::USER_Y_VA + 520), 0);
            assert!(bootstrap::task_status(actual(11)).is_some_and(|s| s == TaskStatus::Dead));
            BOOTSTRAP_STATE.with(|s| {
                let s = s.as_ref().unwrap();
                let g = s.gate_tables().find(1).unwrap();
                assert_eq!((g.queue_len, g.capacity_len), (8, 0));
                assert!(
                    s.gate_tables()
                        .request(10)
                        .is_some_and(|r| r.phase == gate::Phase::Queued)
                );
            });
            // The next supervisor continuation explicitly terminates service 2.
            // Skip the already executed destroy controls (each is 70+store bytes).
            let mut tail = Code::new();
            tail.mov(3, user::USER_Y_VA);
            tail.call(SYS_TASK_DESTROY, [r.caps[2], 0, 0, 0, 0, 0]);
            tail.store(0, 528);
            tail.emit(&[0xcc]);
            install(1, user::USER_CODE_VA, &tail, r.offset);
            frame(1, user::USER_CODE_VA, user::USER_Y_VA);
            r.phase = 3;
            RUN.with_mut(|s| *s = Some(r));
            resume(2)
        }
        3 => {
            // First Delivered request: signals must remain pending and not cancel.
            assert_eq!(sched::current(), 2);
            assert_eq!(crate::signal::pending(actual(3)), (1 << 9) | (1 << 11));
            crate::signal::set_pending(actual(3), 14);
            if mode() == "G3" {
                let cap = read_word(2, DATA + 16);
                let mut wrong = Code::new();
                wrong.mov(3, user::USER_Y_VA);
                wrong.call(SYS_GATE_COPY_IN, [cap, BASE, 0, BASE, 0, 0]);
                wrong.store(0, 512);
                wrong.emit(&[0xcc]);
                install(1, user::USER_CODE_VA, &wrong, r.offset);
                frame(1, user::USER_CODE_VA, user::USER_Y_VA);
                r.phase = 4;
                frame(2, r.service_pc[1], DATA);
                RUN.with_mut(|s| *s = Some(r));
                resume(1)
            }
            frame(2, r.service_pc[1], DATA);
            r.phase = 6;
            RUN.with_mut(|s| *s = Some(r));
            resume(2)
        }
        4 => {
            assert_eq!(
                read_word(1, user::USER_Y_VA + 512),
                syscall::encode_errno(syscall::Errno::Access)
            );
            r.phase = 5;
            RUN.with_mut(|s| *s = Some(r));
            resume(2)
        }
        5 => {
            // Full 4096-byte two-root COW success followed by permission faults.
            for i in 0..12 {
                assert_eq!(read_word(2, DATA + 512 + i * 8), r.results[i as usize]);
            }
            for task in [2, 3] {
                for offset in (8..4104).step_by(8) {
                    assert_eq!(
                        read_word(task, BASE + offset),
                        if offset == 8 {
                            VALUE
                        } else {
                            0x1111_1111_1111_1111
                        }
                    );
                }
            }
            assert_eq!(read_word(3, BASE + 8), VALUE);
            assert_eq!(read_word(2, BASE + 8), VALUE);
            for (task, guard) in [(2, 0x2222_2222_2222_2222), (3, 0x1111_1111_1111_1111)] {
                assert_eq!(read_word(task, BASE), guard);
                assert_eq!(read_word(task, BASE + 4104), guard);
                assert_eq!(
                    read_word(task, BASE + 8184),
                    guard,
                    "failed output crossing the unmapped boundary preserves its prefix"
                );
                for page in [BASE, BASE + 4096] {
                    assert_ne!(leaf(task, page) & vspace::PT_WRITABLE, 0);
                    assert_eq!(leaf(task, page) & vspace::PT_COW, 0);
                }
            }
            // Distinct input/output sentinels expose partial byte writes as well
            // as partial COW. The second client page will be made inaccessible.
            let physical = leaf(2, BASE) & 0x000f_ffff_ffff_f000;
            prima_ostd::mm::write_u64(
                crate::mem::phys_to_virt(physical + 4088, r.offset),
                0x3333_3333_3333_3333,
            );
            set_leaf(
                3,
                BASE,
                (leaf(3, BASE) & !vspace::PT_WRITABLE) | vspace::PT_COW,
            );
            set_leaf(
                3,
                BASE + 4096,
                (leaf(3, BASE + 4096) & !vspace::PT_USER) | vspace::PT_HIDDEN,
            );
            r.leaves = [leaf(3, BASE), leaf(3, BASE + 4096)];
            frame(2, r.service_pc[2], DATA);
            r.phase = 7;
            RUN.with_mut(|s| *s = Some(r));
            resume(2)
        }
        6 => {
            if mode() == "G2-death" {
                assert_eq!(sched::current(), 1);
                assert!(sched::root(2).is_none());
            } else {
                assert_eq!(sched::current(), 2);
                for i in 0..r.result_count {
                    assert_eq!(read_word(2, DATA + 512 + i as u64 * 8), r.results[i]);
                }
                for task in [4, 6] {
                    crate::signal::set_pending(actual(task), 15);
                }
                // Request 7 is Delivered and unanswered; a supervisor must act.
                BOOTSTRAP_STATE.with(|s| {
                    assert!(
                        s.as_ref()
                            .unwrap()
                            .gate_tables()
                            .request(5)
                            .is_some_and(|r| r.phase == gate::Phase::Delivered)
                    )
                });
                r.phase = 8;
                RUN.with_mut(|s| *s = Some(r));
                resume(1)
            }
            r.phase = 8;
            RUN.with_mut(|s| *s = Some(r));
            checkpoint()
        }
        7 => {
            assert_eq!(
                [leaf(3, BASE), leaf(3, BASE + 4096)],
                r.leaves,
                "failed cross-page copy preserves PTEs"
            );
            set_leaf(
                3,
                BASE + 4096,
                (r.leaves[1] | vspace::PT_USER) & !vspace::PT_HIDDEN,
            );
            for i in 0..r.result_count {
                assert_eq!(read_word(2, DATA + 512 + i as u64 * 8), r.results[i]);
            }
            assert_eq!(read_word(2, BASE + 4088), 0x3333_3333_3333_3333);
            assert_eq!(read_word(3, BASE + 4088), 0x1111_1111_1111_1111);
            crate::signal::set_pending(actual(3), 15); // after reply, before client resumes
            // Retire service authorization so the client's second trap is fail-closed.
            BOOTSTRAP_STATE.with_mut(|s| {
                let s = s.as_mut().unwrap();
                let g = s.gate_tables().find(1).unwrap();
                s.drop_capability(2, g.root_cap).unwrap();
            });
            r.phase = 9;
            r.next = 3;
            RUN.with_mut(|s| *s = Some(r));
            resume(3)
        }
        8 => {
            assert_eq!(sched::current(), 1);
            assert!(sched::root(2).is_none());
            BOOTSTRAP_STATE.with(|s| {
                let s = s.as_ref().unwrap();
                let g = s.gate_tables().find(1).unwrap();
                assert!(!g.live);
                assert_eq!((g.queue_len, g.capacity_len), (0, 0));
            });
            r.phase = 9;
            r.next = if mode() == "G2" { 4 } else { 3 };
            RUN.with_mut(|s| *s = Some(r));
            resume(r.next)
        }
        9 => {
            let task = r.next;
            assert_eq!(sched::current(), actual(task));
            let first = if mode() == "G3" || mode() == "G2" && task == 4 {
                REPLY
            } else if mode() == "G2" && task == 6 {
                ABORT
            } else {
                ORPHAN
            };
            assert_eq!(read_word(task, BASE + 128), first);
            assert_eq!(read_word(task, BASE + 136), ORPHAN);
            assert_eq!(
                crate::signal::pending(actual(task)) & ((1 << 9) | (1 << 11)),
                (1 << 9) | (1 << 11)
            );
            if mode() == "G3" || mode() == "G2" && [4, 6].contains(&task) {
                assert_ne!(crate::signal::pending(actual(task)) & (1 << 14), 0);
            }
            r.next += 1;
            while r.next <= r.end && mode() == "G2" && [5, 11].contains(&r.next) {
                r.next += 1;
            }
            RUN.with_mut(|s| *s = Some(r));
            if r.next <= r.end {
                resume(r.next)
            }
            (r.terminal)()
        }
        _ => panic!("unknown gate checkpoint"),
    }
}
pub fn verify() {
    check_state();
    let r = RUN.with(|r| r.unwrap());
    assert!(r.next > r.end);
    BOOTSTRAP_STATE.with(|slot| {
        let s = slot.as_ref().unwrap();
        let mut trapped = 0;
        let mut cancelled = 0;
        let mut ended = 0;
        let mut copied = 0;
        let mut rejected = 0usize;
        let mut orphan_traps = 0;
        let mut orphan_seen = [false; sched::MAX_TASKS];
        let mut terminal = [false; sched::MAX_TASKS];
        for i in 0..s.audit.count {
            match s.audit.entry(i).unwrap() {
                AuditEvent::GateTrapped {
                    request: 0,
                    gate,
                    client_cookie,
                    number,
                    args,
                    flags,
                } => {
                    assert_eq!(number, SYS_GATE_ATTACH);
                    assert_eq!(args, [6, 5, 4, 3, 2, 1]);
                    assert_eq!(flags, 1);
                    let task = client_cookie as u32;
                    assert_eq!(s.task_cookie(task), Some(client_cookie));
                    assert!(!orphan_seen[task as usize]);
                    orphan_seen[task as usize] = true;
                    assert_eq!(
                        s.audit.entry(i + 1),
                        Some(AuditEvent::GateTerminated {
                            gate,
                            request: 0,
                            value: ORPHAN,
                            reason: gate::Reason::Orphaned,
                        })
                    );
                    orphan_traps += 1;
                }
                AuditEvent::GateTrapped { .. } => trapped += 1,
                AuditEvent::GateCancelled { request, .. } => {
                    assert!(!terminal[request as usize]);
                    terminal[request as usize] = true;
                    cancelled += 1;
                }
                AuditEvent::GateTerminated { request, .. } if request != 0 => {
                    assert!(!terminal[request as usize]);
                    terminal[request as usize] = true;
                    ended += 1;
                }
                AuditEvent::GateControlRejected {
                    caller,
                    caller_cookie,
                    number,
                    args,
                    result,
                } => {
                    let gate = s.gate_tables().find(1).unwrap();
                    assert_eq!(
                        caller_cookie,
                        if caller == 2 {
                            gate.service_cookie
                        } else {
                            s.task_cookie(caller).unwrap()
                        }
                    );
                    if mode() == "G3" {
                        assert!(matches!(number, SYS_GATE_COPY_IN | SYS_GATE_COPY_OUT));
                        assert_eq!(args[0], read_word(2, DATA + 16));
                        assert_eq!(&args[4..], &[0, 0]);
                        if rejected == 0 {
                            assert_eq!(caller, 1);
                            assert_eq!(
                                (number, args[2], result),
                                (
                                    SYS_GATE_COPY_IN,
                                    0,
                                    syscall::encode_errno(syscall::Errno::Access)
                                )
                            );
                        } else {
                            assert_eq!(caller, 2);
                            assert_eq!(
                                result,
                                *r.results[..r.result_count]
                                    .iter()
                                    .filter(|v| (**v as i64) < 0)
                                    .nth(rejected - 1)
                                    .unwrap()
                            );
                        }
                    } else {
                        assert_eq!(mode(), "G2");
                        assert_eq!(caller, 2);
                        assert!(matches!(
                            number,
                            SYS_GATE_REPLY | SYS_GATE_ABORT | SYS_GATE_COPY_IN
                        ));
                        assert_eq!(result, syscall::encode_errno(syscall::Errno::NoEnt));
                    }
                    rejected += 1;
                }
                AuditEvent::GateCopied {
                    client_cookie,
                    service_cookie,
                    client_va,
                    service_va,
                    len,
                    out,
                    ..
                } => {
                    assert_eq!(client_cookie, s.task_cookie(3).unwrap());
                    assert_eq!(service_cookie, s.task_cookie(2).unwrap());
                    assert_eq!(
                        (client_va, service_va),
                        if len == 0 {
                            (u64::MAX, u64::MAX)
                        } else {
                            (BASE + 8, BASE + 8)
                        }
                    );
                    assert_eq!(len, if copied < 2 { 4096 } else { 0 });
                    assert_eq!(out, copied % 2 == 1);
                    copied += 1;
                }
                _ => {}
            }
        }
        assert_eq!(
            orphan_traps,
            match mode() {
                "G2" => 7,
                "G2-death" => 10,
                _ => 1,
            }
        );
        assert_eq!(trapped, r.end - 2);
        assert_eq!(cancelled, if mode() == "G2" { 3 } else { 0 });
        assert_eq!(ended + cancelled, trapped);
        assert_eq!(copied, if mode() == "G3" { 4 } else { 0 });
        assert_eq!(
            rejected,
            match mode() {
                "G3" => 12,
                "G2" => 4,
                _ => 0,
            }
        );
        for id in 1..=trapped {
            assert!(s.gate_tables().request(id).is_none());
        }
    });
}
