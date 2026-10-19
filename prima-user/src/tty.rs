//! 阶段 5 片 C：**用户态 tty 字节流服务**的独立二进制映像（`prima-tty`）。
//!
//! 服务管理器运行期把覆盖 COM1 的 `Device(IOPortRange)` Capability 移交本服务；本服务把
//! C 探针 `write(1/2)` 的每个字节经内核受控端口路径写到串口数据寄存器。收到两个字节后，
//! 再对同一设备做「Scratch 写回读」与「区间外写被拒」两条自检，并把 `TTYIN|`/`TTYOUT|`
//! 见证行写进串口，最后向管理器回 `STAGE5_TTY_ACK_TAG`。
//!
//! 本服务不解释 POSIX fd 表：fd 1/2 的合法性由调用方与管理器先行检查；这里只做字节流
//! 与设备授权。没有 libc、没有文件系统，也不宣称通用 tty 语义。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, TaskId,
    protocol::{
        IOPORT_BASE, IOPORT_MAGIC, IOPORT_OUT_OF_RANGE, IOPORT_SCRATCH, SERVICE_DEV_CAP_TAG,
        STAGE5_ECHO_WRITE_TAG, STAGE5_TTY_ACK_TAG, STAGE5_TTY_READY_TAG, STAGE5_TTY_SYNC_ACK_TAG,
        STAGE5_TTY_SYNC_TAG, STAGE5_TTY_WRITE_TAG,
    },
};

/// 派生 `TaskSend` 用（tty→管理器）。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);

#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    self_cap_raw: u64,
    send_cap_raw: u64,
    _initial_cap1_raw: u64,
    _name_raw: u64,
    peer_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    let peer = TaskId::from_raw(peer_raw as u32);
    let (Ok(self_cap), Ok(send_cap)) = (
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(send_cap_raw),
    ) else {
        idle();
    };
    let Ok(id) = runtime.task_id() else {
        idle();
    };
    let Ok(authority) = runtime.cap_derive(self_cap, TASK_SEND_RIGHTS) else {
        idle();
    };
    let _ = runtime.send(
        peer,
        send_cap,
        STAGE5_TTY_READY_TAG | (id.as_raw() as u64 & 0xff),
        &[authority],
    );

    let mut dev: Option<Capability> = None;
    let mut received = 0u64;
    let mut wire = [(0u8, 0u8); 2];
    loop {
        let Ok(message) = runtime.receive() else {
            continue;
        };
        let tag = message.payload >> 48;
        if tag == SERVICE_DEV_CAP_TAG >> 48 {
            // 只接受一块设备权威；重复移交时归还新 cap，避免泄漏或静默替换。
            if let Some(cap) = message.capabilities[0] {
                if dev.is_none() {
                    dev = Some(cap);
                } else {
                    let _ = runtime.cap_drop(cap);
                }
            }
        } else if tag == STAGE5_ECHO_WRITE_TAG >> 48 {
            // M5：echo 的字节只落到串口，不做探针专用的自检/见证行。
            let Some(cap) = dev else {
                continue;
            };
            let fd = ((message.payload >> 16) & 0xff) as u8;
            let byte = ((message.payload >> 24) & 0xff) as u8;
            if (fd == 1 || fd == 2) && runtime.dev_ioport_out(cap, IOPORT_BASE, byte).is_err() {
                continue;
            }
        } else if tag == STAGE5_TTY_SYNC_TAG >> 48 {
            // 管理器在 echo 退出后发送；此前的字节消息已按序处理完。
            let _ = runtime.send(peer, send_cap, STAGE5_TTY_SYNC_ACK_TAG, &[]);
        } else if tag == STAGE5_TTY_WRITE_TAG >> 48 {
            let Some(cap) = dev else {
                continue;
            };
            let fd = ((message.payload >> 16) & 0xff) as u8;
            let byte = ((message.payload >> 24) & 0xff) as u8;
            let sender = (message.payload >> 32) & 0xff;
            if fd != 1 && fd != 2 {
                continue;
            }
            if runtime.dev_ioport_out(cap, IOPORT_BASE, byte).is_err() {
                continue;
            }
            // 字节流不设上限；只有前两条进入固定探针的 TTYIN 见证。
            if received < 2 {
                wire[received as usize] = (fd, byte);
            }
            received += 1;
            if received == 2 {
                let out_ok = runtime
                    .dev_ioport_out(cap, IOPORT_SCRATCH, IOPORT_MAGIC)
                    .is_ok();
                let in_magic = out_ok
                    && matches!(runtime.dev_ioport_in(cap, IOPORT_SCRATCH), Ok(byte) if byte == IOPORT_MAGIC);
                let out_of_range = runtime.dev_ioport_out(cap, IOPORT_OUT_OF_RANGE, 0).is_err();
                // 数据字节本身已写进串口；先换行，避免 'h'/'i' 粘到见证行首。
                let _ = puts(&runtime, cap, b"\r\nTTYIN|task=");
                let _ = write_dec(&runtime, cap, sender);
                for (fd, byte) in wire {
                    let _ = puts(&runtime, cap, b";fd=");
                    let _ = write_dec(&runtime, cap, fd as u64);
                    let _ = puts(&runtime, cap, b";ch=");
                    let _ = write_dec(&runtime, cap, byte as u64);
                }
                let _ = puts(&runtime, cap, b"\r\nTTYOUT|task=");
                let _ = write_dec(&runtime, cap, sender);
                let _ = puts(&runtime, cap, b";in_magic=");
                let _ = write_dec(&runtime, cap, u64::from(in_magic));
                let _ = puts(&runtime, cap, b";out_of_range=");
                let _ = write_dec(&runtime, cap, u64::from(out_of_range));
                let _ = puts(&runtime, cap, b";out_ok=");
                let _ = write_dec(&runtime, cap, u64::from(out_ok));
                let _ = puts(&runtime, cap, b"\r\n");
                let _ = runtime.send(peer, send_cap, STAGE5_TTY_ACK_TAG, &[]);
            }
        }
    }
}

/// 经受控端口路径把字节串写到 COM1 数据寄存器。
fn puts(
    runtime: &Runtime<impl prima_runtime::SyscallTransport>,
    cap: Capability,
    bytes: &[u8],
) -> prima_runtime::Result<()> {
    for &byte in bytes {
        runtime.dev_ioport_out(cap, IOPORT_BASE, byte)?;
    }
    Ok(())
}

/// 十进制输出；避免为见证行引入格式化设施。
fn write_dec(
    runtime: &Runtime<impl prima_runtime::SyscallTransport>,
    cap: Capability,
    mut value: u64,
) -> prima_runtime::Result<()> {
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    loop {
        index -= 1;
        buf[index] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    puts(runtime, cap, &buf[index..])
}

fn idle() -> ! {
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    idle()
}
