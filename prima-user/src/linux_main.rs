#![no_std]
#![no_main]
#![deny(unsafe_code)]

use prima_runtime::{Capability, Error, Rights, Runtime, TaskId, TaskSpec, X86_64Transport};
use prima_user::linux::{
    CASES, MAX_FAMILY_TASKS, P1, P2, PTHREAD, PendingTraps, Process,
    process::Family,
    signal::{Completion, Signals},
    thread::{Call, ThreadGroup},
};

// The input path is supplied explicitly by the harness, which verifies D12's
// SHA-256 before packaging. The bytes are never relinked or patched.
#[cfg(feature = "linux-probe")]
static IMAGE: &[u8] = include_bytes!(env!("PRIMA_LINUX_ELF"));
// Artifact dependencies may compile every binary even without its feature.
// No ordinary manifest selects this binary; only linux-probe registers its
// harness, and an unconfigured image fails ELF validation rather than running.
#[cfg(not(feature = "linux-probe"))]
static IMAGE: &[u8] = &[];
type Native = Runtime<X86_64Transport>;

fn emit(rt: &Native, serial: Capability, bytes: &[u8]) {
    for byte in bytes {
        while rt.dev_ioport_in(serial, 0x3fd).unwrap() & 0x20 == 0 {
            core::hint::spin_loop();
        }
        rt.dev_ioport_out(serial, 0x3f8, *byte).unwrap();
    }
}
fn number(rt: &Native, serial: Capability, mut value: u64) {
    let mut digits = [0; 20];
    let mut start = digits.len();
    loop {
        start -= 1;
        digits[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    emit(rt, serial, &digits[start..]);
}
fn audit(rt: &Native, serial: Capability, gate: Capability, cursor: &mut u64, benchmark: bool) {
    let mut pending = PendingTraps::default();
    let mut counts = [0u64; 6];
    let mut operations = [0u64; 2];
    let mut identity = None;
    while let Some(record) = rt.gate_audit_read(gate, *cursor).unwrap() {
        *cursor = record.cursor;
        match record.kind {
            1 => {
                counts[0] += 1;
                pending.insert(record);
            }
            2 => counts[1] += 1,
            3 => {
                assert!(record.flags < 4);
                counts[2 + record.flags as usize] += 1;
                let original = pending.remove(record.request);
                if benchmark && matches!(original.number, 0 | 8) {
                    assert_eq!(*identity.get_or_insert(original.cookie), original.cookie);
                    assert_eq!(record.flags, 0);
                    if original.number == 0 {
                        assert_eq!(original.args[0], 3);
                        assert!(original.args[1] > 0);
                        assert_eq!(original.args[2..], [17, 0, 0, 0]);
                        assert_eq!(record.value, 17);
                        operations[0] += 1;
                    } else {
                        assert_eq!(original.args, [3, 0, 0, 0, 0, 0]);
                        assert_eq!(record.value, 0);
                        operations[1] += 1;
                    }
                }
                if record.flags != 1 {
                    continue;
                }
                assert_eq!(record.value, (-38i64) as u64);
                emit(rt, serial, b"S6AUDIT|cookie=");
                number(rt, serial, original.cookie);
                emit(rt, serial, b";number=");
                number(rt, serial, original.number);
                emit(rt, serial, b";value=");
                number(rt, serial, record.value);
                emit(rt, serial, b";args=");
                for (index, arg) in original.args.into_iter().enumerate() {
                    if index != 0 {
                        emit(rt, serial, b",");
                    }
                    number(rt, serial, arg);
                }
                emit(rt, serial, b"\n");
            }
            _ => unreachable!(),
        }
    }
    assert!(pending.is_empty());
    emit(rt, serial, b"S6COUNTS|");
    for (index, label) in [
        b"trapped=".as_slice(),
        b"delivered=",
        b"replied=",
        b"aborted=",
        b"orphaned=",
        b"cancelled=",
    ]
    .into_iter()
    .enumerate()
    {
        if index != 0 {
            emit(rt, serial, b";");
        }
        emit(rt, serial, label);
        number(rt, serial, counts[index]);
    }
    emit(rt, serial, b"\n");
    if benchmark {
        assert_eq!(operations, [64, 64]);
        emit(rt, serial, b"S7LOPS|read=64;seek=64;cookie=");
        number(rt, serial, identity.unwrap());
        emit(rt, serial, b";verified=1\n");
    }
}

fn quit(rt: &Native, value: u8) -> ! {
    if let Ok(cap) = rt.dev_ioport_create(0xf4, 1) {
        let _ = rt.dev_ioport_out(cap, 0xf4, value);
    }
    loop {
        core::hint::spin_loop();
    }
}

#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(_self_cap: u64, _: u64, _: u64, selection: u64, _: u64) -> ! {
    let rt = Runtime::x86_64();
    let serial = rt.dev_ioport_create(0x3f8, 8).unwrap();
    let ns = rt.root_namespace().unwrap();
    let mut shared_gate = None;
    let mut audit_cursor = 0;
    let cases = if selection == 0 {
        &CASES[..]
    } else if cfg!(feature = "libos-bench") && selection == 12 {
        &["L1"][..]
    } else {
        let index = selection as usize - 1;
        &CASES[index..index + 1]
    };
    for &case in cases {
        emit(&rt, serial, b"S6BEGIN|case=");
        emit(&rt, serial, case.as_bytes());
        emit(&rt, serial, b"\n");
        let stub = rt.memseg_create(1, Rights::from_bits(10)).unwrap();
        let manager = rt
            .task_create_held(&TaskSpec {
                code_segment: stub,
                code_offset: 0,
                code_pages: 1,
                load_base: 0x400000,
                entry: 0x400000,
                stack_top: 0x70000000,
                parent_namespace: Some(ns),
                root_rights: Rights::from_bits(256),
                initial_capabilities: [None; 2],
                name: 0,
            })
            .unwrap();
        let cookie = rt.send_target_cookie(manager).unwrap();
        assert!(rt.task_prepared(TaskId::from_raw(cookie as u32)).unwrap());
        assert_eq!(rt.task_start_gated(manager, None, 0), Err(Error::Fault));
        assert_eq!(
            rt.task_load_image(manager, b"bad", &[], &[]),
            Err(Error::UnknownErrno(-8))
        );
        assert!(rt.task_prepared(TaskId::from_raw(cookie as u32)).unwrap());
        let applet: &core::ffi::CStr = match case {
            "true" => c"true",
            "echo" => c"echo",
            "cat" => c"cat",
            "ls" => c"ls",
            "uname" => c"uname",
            "sort" => c"sort",
            "P1" => c"P1",
            "P2" => c"P2",
            "M4" => c"M4",
            "M4-fork" => c"M4-fork",
            "M3" => c"M3",
            #[cfg(feature = "libos-bench")]
            "L1" => c"L1",
            _ => unreachable!(),
        };
        let mut argv = [0; 4];
        argv[0] = c"busybox".as_ptr() as u64;
        argv[1] = applet.as_ptr() as u64;
        let argc = match case {
            "echo" => {
                argv[2] = c"hello".as_ptr() as u64;
                3
            }
            "cat" => {
                argv[2] = c"/fixture.txt".as_ptr() as u64;
                3
            }
            "ls" => {
                argv[2] = c"/fixturedir".as_ptr() as u64;
                3
            }
            _ => 2,
        };
        let env = [c"LC_ALL=C".as_ptr() as u64, c"LANG=C".as_ptr() as u64];
        rt.task_load_image(
            manager,
            match case {
                "P1" => P1,
                "P2" => P2,
                "M4" => PTHREAD,
                "M4-fork" => prima_user::linux::PROCESS,
                "M3" => prima_user::linux::MATRIX,
                #[cfg(feature = "libos-bench")]
                "L1" => prima_user::linux::LIBOS_BENCH,
                _ => IMAGE,
            },
            &argv[..argc],
            &env,
        )
        .unwrap();
        rt.memseg_destroy(stub).unwrap();
        rt.cap_drop(stub).unwrap();
        let mut process = Process::new(manager, cookie).unwrap();
        let context = rt.task_context_authority(manager).unwrap();
        process.threads.register_leader(manager, context, cookie);
        let gate = rt
            .task_start_gated(manager, shared_gate, (-38i64) as u64)
            .unwrap();
        shared_gate = Some(gate);
        let mut requests = 0;
        let mut seen_tasks = [0; MAX_FAMILY_TASKS];
        let mut family = Family::new(process).unwrap();
        let mut unsupported = 0;
        loop {
            let req = rt.gate_wait(gate).unwrap();
            let owner = family
                .owner(req.task_cookie)
                .expect("authenticated family member");
            if !seen_tasks.contains(&req.task_cookie) {
                *seen_tasks
                    .iter_mut()
                    .find(|cookie| **cookie == 0)
                    .expect("bounded thread count") = req.task_cookie;
                emit(&rt, serial, b"S6TASK|cookie=");
                number(&rt, serial, req.task_cookie);
                emit(&rt, serial, b"\n");
            }
            requests += 1;
            if requests == 1 {
                assert_eq!(
                    rt.gate_read_frame(req.capability, manager),
                    Err(Error::Denied)
                );
                let saved = rt.gate_read_context(req.capability, context).unwrap();
                let mut invalid = saved;
                invalid.frame.cs = 8;
                assert_eq!(
                    rt.gate_reply_context(req.capability, context, &invalid),
                    Err(Error::InvalidInput)
                );
                invalid = saved;
                invalid.frame.rflags |= 3 << 12;
                assert_eq!(
                    rt.gate_reply_context(req.capability, context, &invalid),
                    Err(Error::InvalidInput)
                );
                invalid = saved;
                invalid.frame.rip = saved.frame.rsp - 8;
                assert_eq!(
                    rt.gate_reply_context(req.capability, context, &invalid),
                    Err(Error::Fault)
                );
                invalid = saved;
                invalid.fpu[24..28].copy_from_slice(&u32::MAX.to_le_bytes());
                assert_eq!(
                    rt.gate_reply_context(req.capability, context, &invalid),
                    Err(Error::InvalidInput)
                );
                assert_eq!(
                    rt.gate_read_context(req.capability, context).unwrap(),
                    saved
                );
            }
            let status = if req.number == 60 {
                match family
                    .process(owner)
                    .threads
                    .exit_thread(&rt, &req)
                    .unwrap()
                {
                    Some(status) => (status as u32) << 8,
                    None => continue,
                }
            } else if req.number == 231 {
                ((req.args[0] & 255) as u32) << 8
            } else {
                let mut terminated = None;
                let result = if req.number == 57 {
                    Some(family.fork(&rt, &req, gate))
                } else if req.number == 59 {
                    match family.exec(&rt, &req, IMAGE, prima_user::linux::PROCESS_CHILD) {
                        Ok(()) => continue,
                        Err(error) => Some(error),
                    }
                } else if req.number == 61 {
                    match family.wait(&rt, &req).unwrap() {
                        Some(value) => Some(value),
                        None => continue,
                    }
                } else if req.number == 110 {
                    Some(family.parent(owner))
                } else if ThreadGroup::is_call(req.number) {
                    match family.process(owner).threads.call(&rt, &req, gate) {
                        Call::Value(value) => Some(value),
                        Call::Pending => continue,
                        Call::Terminated(signal) => {
                            terminated = Some(signal);
                            Some(0)
                        }
                    }
                } else if Signals::is_call(req.number) {
                    if matches!(req.number, 62 | 234) {
                        Some(family.process(owner).threads.send_signal(&req))
                    } else {
                        None
                    }
                } else {
                    Some(family.dispatch(owner, &rt, &req, |fd, bytes| {
                        emit(&rt, serial, b"S6WRITE|fd=");
                        number(&rt, serial, fd);
                        emit(&rt, serial, b";hex=");
                        for byte in bytes {
                            emit(
                                &rt,
                                serial,
                                &[
                                    b"0123456789abcdef"[(*byte >> 4) as usize],
                                    b"0123456789abcdef"[(*byte & 15) as usize],
                                ],
                            );
                        }
                        emit(&rt, serial, b"\n");
                        Ok(())
                    }))
                };
                // A signal sender still owns its Delivered request during target interruption.
                let completion = if let Some(signal) = terminated.or_else(|| {
                    family
                        .process(owner)
                        .threads
                        .interrupt_signals(&rt)
                        .unwrap()
                }) {
                    Completion::Terminated(signal)
                } else {
                    family
                        .process(owner)
                        .threads
                        .signal_service(&rt, &req, result)
                        .unwrap()
                };
                if completion == Completion::Replied(-38) {
                    unsupported += 1;
                    emit(&rt, serial, b"S6NOSYS|cookie=");
                    number(&rt, serial, req.task_cookie);
                    emit(&rt, serial, b";number=");
                    number(&rt, serial, req.number);
                    emit(&rt, serial, b";value=-38\n");
                }
                match completion {
                    Completion::Replied(_) | Completion::Suspended => continue,
                    Completion::Terminated(signal) => u32::from(signal),
                }
            };
            let Some(status) = family.exit(&rt, owner, &req, status).unwrap() else {
                continue;
            };
            audit(&rt, serial, gate, &mut audit_cursor, case == "L1");
            emit(&rt, serial, b"S6EXIT|case=");
            emit(&rt, serial, case.as_bytes());
            emit(&rt, serial, b";status=");
            number(&rt, serial, status);
            emit(&rt, serial, b";requests=");
            number(&rt, serial, requests);
            emit(&rt, serial, b";unsupported=");
            number(&rt, serial, unsupported);
            emit(&rt, serial, b"\n");
            if status != 0 {
                quit(&rt, 0x11);
            }
            break;
        }
    }
    emit(&rt, serial, b"S6LINUX|cases=");
    number(&rt, serial, cases.len() as u64);
    emit(&rt, serial, b";verified=1\n");
    quit(&rt, 0x10)
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use core::fmt::Write;
    struct Serial<'a>(&'a Native, Capability);
    impl core::fmt::Write for Serial<'_> {
        fn write_str(&mut self, value: &str) -> core::fmt::Result {
            emit(self.0, self.1, value.as_bytes());
            Ok(())
        }
    }
    let rt = Runtime::x86_64();
    if let Ok(serial) = rt.dev_ioport_create(0x3f8, 8) {
        let _ = writeln!(Serial(&rt, serial), "S6PANIC|{info}");
    }
    quit(&rt, 0x11)
}
