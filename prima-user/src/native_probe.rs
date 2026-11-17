//! P2b-4/P2b-5：原生/POSIX ABI 的代理场景应用。
//!
//! 本二进制由服务管理器在代理场景镜像里创建（manifest kind `NATIVE_APP`），
//! 经管理器签发的**原生策略视图**（`SERVICE_ROOT_NATIVE`：`/native` + `/shared`）
//! 读取 Store，把结果写到串口并向管理器报告完成。
//!
//! P2b-5 把它扩展成正反例矩阵：同一 `/shared` 资源两侧可读，私有根互不可见，
//! `..` 与绝对/相对符号链接都被夹在视图根内；伪造槽位与重放序号由 Store 拒绝。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use prima_runtime::{
    Capability, Runtime, TaskId, X86_64Transport,
    bulk::{self, DIRFD_ROOT},
    protocol::{SERVICE_PROXY_DONE_TAG, SERVICE_ROOT_NATIVE},
};
use prima_user::linux_store::{Remote, Session};

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

fn hex(rt: &Native, serial: Capability, bytes: &[u8]) {
    for byte in bytes {
        emit(
            rt,
            serial,
            &[
                b"0123456789abcdef"[(*byte >> 4) as usize],
                b"0123456789abcdef"[(*byte & 15) as usize],
            ],
        );
    }
}

/// Record one negative case: `expected` is the errno the store must return.
fn negative(
    rt: &Native,
    serial: Capability,
    case: &[u8],
    expected: i64,
    got: i64,
    failures: &mut u32,
) {
    let ok = got == -expected;
    emit(rt, serial, b"S7NEG|case=");
    emit(rt, serial, case);
    emit(rt, serial, b";errno=");
    number(rt, serial, expected as u64);
    emit(rt, serial, b";status=");
    number(rt, serial, u64::from(!ok));
    emit(rt, serial, b"\n");
    if !ok {
        *failures += 1;
    }
}

fn open_errno(session: &mut Session, remote: &mut Remote<'_, X86_64Transport>, path: &[u8]) -> i64 {
    match session.open(remote, DIRFD_ROOT, path, 0) {
        Ok(fd) => {
            let _ = session.close(remote, fd);
            0
        }
        Err(error) => error,
    }
}

#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    _self_cap: u64,
    send_cap_raw: u64,
    _initial_cap1: u64,
    _name: u64,
    _peer: u64,
) -> ! {
    let rt = Runtime::x86_64();
    let serial = rt.dev_ioport_create(0x3f8, 8).unwrap();
    let manager = Capability::from_raw(send_cap_raw).expect("manager send capability");
    let mut read = 0usize;
    let mut ok = false;
    let mut failures = 0u32;
    // The native scenario registers its own policy view: private `/native` plus
    // the explicit `/shared` mount. Data never comes from the service root.
    if let Ok(mut remote) = Remote::connect(&rt, manager, SERVICE_ROOT_NATIVE)
        && let Ok(self_cap) = rt.self_capability()
        && remote.register_owner(1, self_cap).is_ok()
    {
        let mut session = Session::new();
        session.set_owner(1);

        if let Ok(fd) = session.open(&mut remote, DIRFD_ROOT, b"/README", 0) {
            let mut bytes = [0u8; 32];
            if let Ok(count) = session.read(&mut remote, fd, &mut bytes) {
                read = count;
                ok = &bytes[..count] == b"prima rootfs\n";
                emit(&rt, serial, b"S7NA|abi=native;path=/README;hex=");
                hex(&rt, serial, &bytes[..count]);
            }
            let _ = session.close(&mut remote, fd);
        }
        emit(&rt, serial, b";read=");
        number(&rt, serial, read as u64);
        emit(&rt, serial, b";status=");
        number(&rt, serial, u64::from(!ok));
        emit(&rt, serial, b"\n");
        if !ok {
            failures += 1;
        }

        // Positive: the same `/shared` resource is explicitly mounted in both
        // policy views.
        let mut note = [0u8; 16];
        let shared_ok = session
            .open(&mut remote, DIRFD_ROOT, b"/shared/note", 0)
            .and_then(|fd| {
                let count = session.read(&mut remote, fd, &mut note)?;
                let _ = session.close(&mut remote, fd);
                Ok(count)
            })
            .is_ok_and(|count| &note[..count] == b"shared\n");
        emit(&rt, serial, b"S7NPOS|case=shared;hex=");
        hex(&rt, serial, &note[..if shared_ok { 7 } else { 0 }]);
        emit(&rt, serial, b";status=");
        number(&rt, serial, u64::from(!shared_ok));
        emit(&rt, serial, b"\n");
        if !shared_ok {
            failures += 1;
        }

        // Negative matrix: private root, `..` clamp, absolute/relative symlink
        // containment, guessed fd, replayed sequence and an unregistered slot.
        negative(
            &rt,
            serial,
            b"private",
            2,
            open_errno(&mut session, &mut remote, b"/linux/fixture.txt"),
            &mut failures,
        );
        negative(
            &rt,
            serial,
            b"dotdot",
            2,
            open_errno(&mut session, &mut remote, b"/../linux/fixture.txt"),
            &mut failures,
        );
        negative(
            &rt,
            serial,
            b"symlink",
            2,
            open_errno(&mut session, &mut remote, b"/shared/escape"),
            &mut failures,
        );
        negative(
            &rt,
            serial,
            b"symlinkup",
            2,
            open_errno(&mut session, &mut remote, b"/shared/up"),
            &mut failures,
        );
        let mut scratch = [0u8; 8];
        negative(
            &rt,
            serial,
            b"guessfd",
            9,
            session
                .read(&mut remote, 31, &mut scratch)
                .map_or_else(|error| error, |_| 0),
            &mut failures,
        );
        let last = session.next().saturating_sub(1);
        let slot = remote.slot(1).unwrap_or(0);
        let replayed = remote
            .probe(
                1,
                bulk::PROXY_SUBMIT | (u64::from(slot) << bulk::PROXY_SLOT_SHIFT) | last,
            )
            .map_or(0, |status| status as i64);
        negative(&rt, serial, b"replay", 22, -replayed, &mut failures);
        let foreign = (prima_user::proxy::MAX_PROXIES - 1) as u64;
        let status = remote
            .probe(
                1,
                bulk::PROXY_SUBMIT | (foreign << bulk::PROXY_SLOT_SHIFT) | 1,
            )
            .map_or(0, |status| status as i64);
        negative(&rt, serial, b"foreign", 13, -status, &mut failures);
    } else {
        failures += 1;
        emit(
            &rt,
            serial,
            b"S7NA|abi=native;path=/README;hex=;read=0;status=1\n",
        );
    }
    emit(&rt, serial, b"S7NEGDONE|fail=");
    number(&rt, serial, u64::from(failures));
    emit(&rt, serial, b"\n");
    let failed = !ok || failures != 0;
    if let Ok(cookie) = rt.send_target_cookie(manager) {
        let _ = rt.send(
            TaskId::from_raw(cookie as u32),
            manager,
            SERVICE_PROXY_DONE_TAG | u64::from(failed),
            &[],
        );
    }
    // Park on recv: a spinning task would starve lower-priority Ready tasks
    // (the scheduler is not preemptive here).
    loop {
        let _ = rt.receive();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
