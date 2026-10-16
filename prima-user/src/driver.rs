//! SPEC-114（S4-D4）：**第一个 ring3 用户态驱动**的独立二进制映像（`prima-driver`）。
//!
//! 与三个基础服务共用同一「向管理器自报注册名 + id + 存在标记」的接入协议，但不参与 §15 的
//! 定序状态探针——它是**设备持有者**：服务管理器 `SYS_DEV_IOPORT_CREATE` 铸出一个覆盖 COM1
//! （`0x3F8..0x400`）的 `Device(IOPortRange)` Capability，经 [`SERVICE_DEV_CAP_TAG`] **运行期
//! 移交**本驱动。驱动据此走内核**受控端口路径**：
//!
//! - `op=IOPORT_RW`：向区间内的 Scratch 寄存器（`0x3FF`）写魔数再读回，应答低 32 位是**读回的
//!   字节**——等于魔数即证明「写→读」的设备往返成立（Scratch 寄存器无输出副作用，不污染串口流）。
//! - `op=IOPORT_OOR`：尝试写区间**外**的端口（`0x2F8`）；内核须拒绝，应答低 32 位 = 0 表示被拒、
//!   = 1 表示竟被放行（即受控路径失效）。
//!
//! 最小权限：初始只持 `TaskSend`（指向管理器）+ 角色令牌；唯一的设备权威是运行期收到的那一份
//! `IOPortRange` 副本。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, TaskId,
    protocol::{
        IOPORT_MAGIC, IOPORT_OUT_OF_RANGE, IOPORT_SCRATCH, NATIVE_APP_DATA_VA,
        SERVICE_BEHAVIOR_DRIVER, SERVICE_DEV_CAP_TAG, SERVICE_MARKER_DRIVER, SERVICE_NAME_DRIVER,
        SERVICE_NAME_MASK, SERVICE_SERVE_OP_IOPORT_OOR, SERVICE_SERVE_OP_IOPORT_RW,
        SERVICE_SERVE_OP_SHIFT, SERVICE_SERVE_TAG, SERVICE_WAKE_TAG, pack_service_marker,
        pack_service_reply,
    },
};

/// 端点/驱动自报的消息 tag（高 16 位；与 `prima-runtime::protocol`/内核同款）。
const SERVICE_ID_TAG_HI: u64 = 0x5EC1_0000_0000_0000;
/// 派生 `TaskSend` 用（驱动→管理器）。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);
/// 引导任务（init）的 task id。对端是它时**不**发无 cap 唤醒。
const BOOTSTRAP_TASK: u32 = 1;
/// 受控 I/O 失败的哨兵（不在任何合法应答值域内）。
const IOPORT_FAILED: u64 = 0xffff_ffff;

/// 用户态驱动入口。装载器提供 ring3 栈；入口参数由 ABI 保留。
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
    // 与基础服务同款接入：无 cap 唤醒 → 自报注册名/id（带自管理凭据）→ 自报存在标记。
    if let Ok(id) = runtime.task_id()
        && let Ok(authority) = runtime.cap_derive(self_cap, TASK_SEND_RIGHTS)
    {
        if peer.as_raw() != BOOTSTRAP_TASK {
            let _ = runtime.send(peer, send_cap, SERVICE_WAKE_TAG, &[]);
        }
        let _ = runtime.send(
            peer,
            send_cap,
            SERVICE_ID_TAG_HI
                | ((SERVICE_NAME_DRIVER & SERVICE_NAME_MASK) << 8)
                | (id.as_raw() as u64 & 0xff),
            &[authority],
        );
        let _ = runtime.send(
            peer,
            send_cap,
            pack_service_marker(SERVICE_BEHAVIOR_DRIVER, SERVICE_MARKER_DRIVER),
            &[],
        );
    }
    // SPEC-114：管理器运行期移交的 `Device(IOPortRange)` Capability（驱动唯一的设备权威）。
    let mut dev_cap: Option<Capability> = None;
    let mut replies = 0;
    loop {
        let Ok(message) = runtime.receive() else {
            continue;
        };
        let tag = message.payload >> 48;
        if tag == SERVICE_DEV_CAP_TAG >> 48 {
            if let Some(cap) = message.capabilities[0] {
                dev_cap = Some(cap);
            }
        } else if tag == SERVICE_SERVE_TAG >> 48 {
            let op = (message.payload >> SERVICE_SERVE_OP_SHIFT) & 0xff;
            let result = match (op, dev_cap) {
                (SERVICE_SERVE_OP_IOPORT_RW, Some(cap)) => {
                    // 区间内：写 Scratch 寄存器再读回。任一失败给哨兵（读回值绝不等于哨兵）。
                    match runtime.dev_ioport_out(cap, IOPORT_SCRATCH, IOPORT_MAGIC) {
                        Ok(()) => runtime
                            .dev_ioport_in(cap, IOPORT_SCRATCH)
                            .map_or(IOPORT_FAILED, |byte| byte as u64),
                        Err(_) => IOPORT_FAILED,
                    }
                }
                (SERVICE_SERVE_OP_IOPORT_OOR, Some(cap)) => {
                    // 区间外：内核**必须**拒绝。0 = 被拒（期望）；1 = 竟被放行（受控路径失效）。
                    if runtime.dev_ioport_out(cap, IOPORT_OUT_OF_RANGE, 0).is_ok() {
                        1
                    } else {
                        0
                    }
                }
                _ => IOPORT_FAILED,
            };
            let _ = runtime.send(
                peer,
                send_cap,
                pack_service_reply(SERVICE_BEHAVIOR_DRIVER, result),
                &[],
            );
            replies += 1;
            if replies == 2 {
                // 隔离见证：驱动无客户端共享段映射，向同 VA 写入须真实触发 #PF。
                prima_ostd::entry::trigger_page_fault_write(NATIVE_APP_DATA_VA);
            }
        }
    }
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
