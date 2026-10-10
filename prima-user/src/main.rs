#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, TaskId,
    protocol::{
        CodecError, MESSAGE_HEADER_LEN, MessageHeader, NegotiatedProtocol, NegotiationError,
        OFFER_LEN, ProtocolOffer, negotiate,
    },
};

const USER_HELLO_VA: u64 = 0x0000_0A00_0000_7000;
/// SPEC-087：把 init 应答 payload 写回共享段内的偏移（与内核 `SERVICE_REPLY_OFFSET` 一致）。
const SERVICE_REPLY_OFFSET: u64 = 64;
/// SPEC-088：服务端点把协商结果写回共享段内的偏移（与内核 `SERVICE_NEGOTIATED_OFFSET` 一致）。
const SERVICE_NEGOTIATED_OFFSET: u64 = 128;
/// SPEC-088：服务端点把两个负例码写回共享段内的偏移。
const SERVICE_NEGATIVES_OFFSET: u64 = 136;
/// SPEC-088：服务端点自报 task id 的消息 tag（高 16 位；低 16 位为 id）。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// SPEC-088：服务端点完成协商的消息 tag。
const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
const HELLO_OPCODE: u16 = 1;
const HELLO_REQUEST_ID: u32 = 3;
const HELLO_FEATURES: u64 = 1;
const HELLO_MAX_MESSAGE_SIZE: u32 = 4096;
const HELLO_RW_ACCESS: Rights = Rights::from_bits((1 << 1) | (1 << 2));

/// SPEC-088：服务端点自己的协议 offer。与客户端 offer 取交后 `max` 由 4096 降到 2048，
/// 是「结果只可能来自服务端」的判决性字段。
const SERVICE_OFFER: ProtocolOffer = ProtocolOffer {
    major: 1,
    minor: 0,
    features: 0b101,
    required_features: 0b001,
    max_message_size: 2048,
};

/// SPEC-088：把协商结果打包成一个字（`major<<48 | minor<<32 | max<<16 | features 低 16 位`）。
fn pack_negotiated(negotiated: NegotiatedProtocol) -> u64 {
    ((negotiated.major as u64) << 48)
        | ((negotiated.minor as u64) << 32)
        | ((negotiated.max_message_size as u64) << 16)
        | (negotiated.features & 0xffff)
}

/// SPEC-088：解析对端 Hello 并协商；成功返回打包结果，失败返回 0。
fn negotiation_word(header_bytes: &[u8; MESSAGE_HEADER_LEN], offer_bytes: &[u8; OFFER_LEN]) -> u64 {
    if MessageHeader::decode(*header_bytes).is_err() {
        return 0;
    }
    let Ok(offer) = ProtocolOffer::decode(*offer_bytes) else {
        return 0;
    };
    match negotiate(offer, SERVICE_OFFER) {
        Ok(negotiated) => pack_negotiated(negotiated),
        Err(_) => 0,
    }
}

/// SPEC-088：两个负例——`major` 不匹配的协商与魔数错误的解码；命中给显式码，否则给 `0xffff`
/// 哨兵（使「逻辑没跑」不可能伪装成成功）。
fn negotiation_negatives() -> u64 {
    let client = ProtocolOffer {
        major: 1,
        minor: 0,
        features: 1,
        required_features: 1,
        max_message_size: 4096,
    };
    let mismatched = ProtocolOffer {
        major: 2,
        ..SERVICE_OFFER
    };
    let major_mismatch = match negotiate(client, mismatched) {
        Err(NegotiationError::MajorMismatch) => 1u64,
        _ => 0xffff,
    };
    let bad_magic = match MessageHeader::decode([0u8; MESSAGE_HEADER_LEN]) {
        Err(CodecError::BadMagic) => 0u64,
        _ => 0xffff,
    };
    (major_mismatch << 32) | bad_magic
}

/// Rust 用户映像入口。装载器将提供 ring3 栈，入口参数由 ABI 保留。
///
/// SPEC-088：第 3 参数（`rdx` = `initial_caps[1]`）**非零 = 服务端点角色**，零 = 客户端角色。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(self_cap_raw: u64, send_cap_raw: u64, initial_cap1_raw: u64) -> ! {
    let runtime = Runtime::x86_64();
    if initial_cap1_raw != 0 {
        // ===== SPEC-088：Rust 服务端点 =====
        // 自报 id → 等 init 转发的协商请求（带客户端共享段 cap）→ 映射 → 解析 Hello →
        // 协商 → 写结果与负例码 → 回执 init → 阻塞。
        if let (Ok(self_cap), Ok(send_cap), Ok(id)) = (
            Capability::from_raw(self_cap_raw),
            Capability::from_raw(send_cap_raw),
            runtime.task_id(),
        ) {
            let _ = runtime.send(
                TaskId::from_raw(1),
                send_cap,
                SERVICE_ID_TAG | id.as_raw() as u64,
                &[],
            );
            // SPEC-088：init 先发一条无 cap 唤醒（内核拒绝向 `BlockedRecv` 接收者投递带 cap
            // 消息），故循环跳过无 cap 消息，直到取到带共享 Segment cap 的协商请求。
            let mut request_segment = None;
            for _ in 0..8 {
                match runtime.receive() {
                    Ok(message) => {
                        if let Some(segment) = message.capabilities[0] {
                            request_segment = Some(segment);
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            if let Some(segment) = request_segment
                && runtime
                    .memseg_map(segment, self_cap, 0, USER_HELLO_VA, 1, HELLO_RW_ACCESS)
                    .is_ok()
            {
                let mut header_bytes = [0u8; MESSAGE_HEADER_LEN];
                let mut offer_bytes = [0u8; OFFER_LEN];
                let header_ok = runtime
                    .read_user_bytes(USER_HELLO_VA, &mut header_bytes)
                    .is_ok();
                let offer_ok = runtime
                    .read_user_bytes(USER_HELLO_VA + MESSAGE_HEADER_LEN as u64, &mut offer_bytes)
                    .is_ok();
                if header_ok && offer_ok {
                    let negotiated = negotiation_word(&header_bytes, &offer_bytes);
                    let negatives = negotiation_negatives();
                    let _ = runtime.write_user_bytes(
                        USER_HELLO_VA + SERVICE_NEGOTIATED_OFFSET,
                        &negotiated.to_le_bytes(),
                    );
                    let _ = runtime.write_user_bytes(
                        USER_HELLO_VA + SERVICE_NEGATIVES_OFFSET,
                        &negatives.to_le_bytes(),
                    );
                }
            }
            let _ = runtime.send(TaskId::from_raw(1), send_cap, SERVICE_DONE_TAG, &[]);
        }
        loop {
            let _ = runtime.receive();
        }
    }
    // ===== 客户端角色（SPEC-085/087 行为逐字节不变）=====
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
