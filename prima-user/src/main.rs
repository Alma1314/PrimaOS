#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, TaskId,
    protocol::{MESSAGE_HEADER_LEN, MessageHeader, ProtocolOffer},
};

const USER_HELLO_VA: u64 = 0x0000_0A00_0000_7000;
/// SPEC-087：把 init 应答 payload 写回共享段内的偏移（与内核 `SERVICE_REPLY_OFFSET` 一致）。
const SERVICE_REPLY_OFFSET: u64 = 64;
const HELLO_OPCODE: u16 = 1;
const HELLO_REQUEST_ID: u32 = 3;
const HELLO_FEATURES: u64 = 1;
const HELLO_MAX_MESSAGE_SIZE: u32 = 4096;
const HELLO_RW_ACCESS: Rights = Rights::from_bits((1 << 1) | (1 << 2));

/// Rust 用户映像入口。装载器将提供 ring3 栈，入口参数由 ABI 保留。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(self_cap_raw: u64, send_cap_raw: u64, _initial_cap1_raw: u64) -> ! {
    let runtime = Runtime::x86_64();
    // 通过 Runtime 创建共享 Segment、映射到自己的地址空间并写入版本化 PRIM Hello。
    if let (Ok(id), Ok(self_cap), Ok(send_cap), Ok(_now)) = (
        runtime.task_id(),
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(send_cap_raw),
        runtime.time_now(),
    ) {
        let offer = ProtocolOffer {
            major: 1,
            minor: 0,
            features: HELLO_FEATURES,
            required_features: HELLO_FEATURES,
            max_message_size: HELLO_MAX_MESSAGE_SIZE,
        };
        let header = MessageHeader {
            major: offer.major,
            minor: offer.minor,
            opcode: HELLO_OPCODE,
            flags: 0,
            request_id: HELLO_REQUEST_ID,
            payload_len: prima_runtime::protocol::OFFER_LEN as u32,
        };
        let header_bytes = header.encode();
        let offer_bytes = offer.encode();
        let mut message = [0u8; MESSAGE_HEADER_LEN + prima_runtime::protocol::OFFER_LEN];
        message[..MESSAGE_HEADER_LEN].copy_from_slice(&header_bytes);
        message[MESSAGE_HEADER_LEN..].copy_from_slice(&offer_bytes);
        if let Ok(segment) = runtime.memseg_create(1, HELLO_RW_ACCESS) {
            let mapped =
                runtime.memseg_map(segment, self_cap, 0, USER_HELLO_VA, 1, HELLO_RW_ACCESS);
            if mapped.is_ok() && runtime.write_user_bytes(USER_HELLO_VA, &message).is_ok() {
                let _ = runtime.send(
                    TaskId::from_raw(1),
                    send_cap,
                    id.as_raw() as u64,
                    &[segment],
                );
                // SPEC-087：只有**存活的**新客户端（非 SPEC-085 的历史见证载体 task 3）等 init
                // 的应答，并把应答 payload 写回共享段供内核核验。task 3 的行为保持不变，
                // 以冻结既有证据（它在发完 Hello 后即被 init 销毁）。
                if id.as_raw() != 3 {
                    if let Ok(reply) = runtime.receive() {
                        let _ = runtime.write_user_bytes(
                            USER_HELLO_VA + SERVICE_REPLY_OFFSET,
                            &reply.payload.to_le_bytes(),
                        );
                    }
                    // 握手完成后**阻塞等待**（不再 Ready）：抢占阶段只轮转 task 1/2，
                    // 一个持续 Ready 的客户端会被 `pick_next` 选中并打乱轮转。
                    loop {
                        let _ = runtime.receive();
                    }
                }
            }
        }
    }
    loop {
        let _ = runtime.yield_task();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
