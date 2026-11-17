//! P2b-4：原生/POSIX ABI 的代理场景应用。
//!
//! 本二进制由服务管理器在代理场景镜像里创建（manifest kind `NATIVE_APP`），
//! 经管理器签发的直接文件会话读取 Store 服务根里的 `/README`，把结果写到串口
//! 并向管理器报告完成。它是“两 ABI 程序”中的原生一侧；Linux 一侧由
//! `prima-linux` 的代理模式承担。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use prima_runtime::{
    Capability, Runtime, TaskId, X86_64Transport, protocol::SERVICE_PROXY_DONE_TAG,
};
use prima_user::linux_store::Direct;

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
    if let Ok(mut direct) = Direct::connect(&rt, manager)
        && let Ok(fd) = direct.open(b"/README")
    {
        let mut bytes = [0u8; 32];
        if let Ok(count) = direct.read(fd, &mut bytes) {
            read = count;
            ok = &bytes[..count] == b"prima rootfs\n";
            emit(&rt, serial, b"S7NA|abi=native;path=/README;hex=");
            hex(&rt, serial, &bytes[..count]);
        }
        direct.shutdown();
    }
    emit(&rt, serial, b";read=");
    number(&rt, serial, read as u64);
    emit(&rt, serial, b";status=");
    number(&rt, serial, u64::from(!ok));
    emit(&rt, serial, b"\n");
    if let Ok(cookie) = rt.send_target_cookie(manager) {
        let _ = rt.send(
            TaskId::from_raw(cookie as u32),
            manager,
            SERVICE_PROXY_DONE_TAG | u64::from(!ok),
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
