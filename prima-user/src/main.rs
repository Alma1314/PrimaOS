#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, TaskId,
    protocol::{
        MESSAGE_HEADER_LEN, MessageHeader, OFFER_LEN, ProtocolOffer, SERVICE_BEHAVIOR_NONE,
        SERVICE_NAME_BETA, SERVICE_NAME_MASK, SERVICE_NAME_OFFSET, SERVICE_WAKE_TAG,
        negotiation_negatives, negotiation_word, service_offer,
    },
    trace::EVENT_RUNTIME_HELLO,
};

const USER_HELLO_VA: u64 = 0x0000_0A00_0000_7000;
/// SPEC-087：把 init 应答 payload 写回共享段内的偏移（与内核 `SERVICE_REPLY_OFFSET` 一致）。
const SERVICE_REPLY_OFFSET: u64 = 64;
/// SPEC-088：服务端点把协商结果写回共享段内的偏移（与内核 `SERVICE_NEGOTIATED_OFFSET` 一致）。
const SERVICE_NEGOTIATED_OFFSET: u64 = 128;
/// SPEC-088：服务端点把两个负例码写回共享段内的偏移。
const SERVICE_NEGATIVES_OFFSET: u64 = 136;
/// SPEC-092：服务端点把**自己的 task id** 写回共享段内的偏移（`160 + (behavior-1)*8`）——
/// 使「哪个端点产出了该协商结果」成为可核验的绑定，而非仅凭行为码。
const SERVICE_ENDPOINT_OFFSET: u64 = 160;
/// SPEC-093：服务端点把**编译进自身二进制的标记**写回共享段的偏移（`176 + (behavior-1)*8`）——
/// 两个端点由**不同二进制**创建，标记相异即证明「结果来自不同程序」而非同一映像的两次实例。
const SERVICE_BINARY_MARKER_OFFSET: u64 = 176;
/// SPEC-093：`prima-smoke`（`ALPHA` 端点）的二进制标记。`prima-beta` 用**不同**常量。
const BINARY_MARKER: u64 = 0x414C_5048_4100_0001;
/// SPEC-088/101：端点自报的消息 tag（高 16 位）；payload 布局 `tag(48..63) | name(8..47) | id(0..7)`。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// SPEC-088：服务端点完成协商的消息 tag。
const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
const HELLO_OPCODE: u16 = 1;
const HELLO_REQUEST_ID: u32 = 3;
const HELLO_FEATURES: u64 = 1;
const HELLO_MAX_MESSAGE_SIZE: u32 = 4096;
const HELLO_RW_ACCESS: Rights = Rights::from_bits((1 << 1) | (1 << 2));
/// SPEC-104：移交给服务管理器的发送权（仅 `TaskSend`）——派生副本，端点自己保留 self cap。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);
/// SPEC-104：引导任务（init）的 task id。对端是它时**不**发无 cap 唤醒（其握手块按单条消息接收）。
const BOOTSTRAP_TASK: u32 = 1;

/// Rust 用户映像入口。装载器将提供 ring3 栈，入口参数由 ABI 保留。
///
/// SPEC-088：第 3 参数（`rdx` = `initial_caps[1]`）**非零 = 服务端点角色**，零 = 客户端角色。
/// SPEC-101：第 4 参数（`rcx` = 服务名）由服务管理器在 `task_create` 时**运行期指派**——端点
/// 角色用它作自注册名（客户端角色忽略它：客户端用众所周知的常量 `"beta"` 请求服务名）。
///
/// SPEC-104：第 5 参数（`r8` = 对端 task id）同样由 `task_create` **运行期指派**——它是
/// `initial_caps[0]`（发送凭据）所指向的 task id。`send` 的接收者必须等于凭据目标
/// （`kernel/src/syscall.rs::send_call`），故本映像据此寻址其设计对端，而非硬编码 task 1。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    self_cap_raw: u64,
    send_cap_raw: u64,
    initial_cap1_raw: u64,
    name_raw: u64,
    peer_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    let peer = TaskId::from_raw(peer_raw as u32);
    if initial_cap1_raw != 0 {
        // ===== SPEC-088/090：Rust 服务端点 =====
        // 自报 id → 循环：解析 init 每条路由消息。SPEC-090 的服务管理器把**路由到的行为码**
        // 编码进无 cap 的 `prepare` payload 低 8 位，故本端按行为码选择自己的 offer
        // （`ALPHA`→max 2048、`BETA`→max 1024），把协商结果写到 `128 + (behavior-1)*16`。
        if let (Ok(self_cap), Ok(send_cap), Ok(id)) = (
            Capability::from_raw(self_cap_raw),
            Capability::from_raw(send_cap_raw),
            runtime.task_id(),
        ) {
            // SPEC-104：向管理器移交**自身自管理凭据的派生副本**（仅 `TaskSend`）——端点自己仍须
            // 持有原 self cap（随后 `memseg_map` 需要它），故不能直接移交原 cap。
            if let Ok(authority) = runtime.cap_derive(self_cap, TASK_SEND_RIGHTS) {
                // 仅当对端是**服务管理器**（id ≠ 引导任务 1、常态阻塞在 `recv`）时先发无 cap 唤醒：
                // 内核拒绝向 `BlockedRecv` 接收者投递带 cap 消息。对引导任务（id 1）其握手块按**单条**
                // 消息接收，不预期额外唤醒，故直发。
                if peer.as_raw() != BOOTSTRAP_TASK {
                    let _ = runtime.send(peer, send_cap, SERVICE_WAKE_TAG, &[]);
                }
                let _ = runtime.send(
                    peer,
                    send_cap,
                    // SPEC-101：自报 payload = tag(48..63) ‖ 运行期指派的注册名(8..47) ‖ task id(0..7)
                    // ——管理器据此建立 `name → id` 注册表并按名扫描解析。名字来自入口 `rcx`，非字面量。
                    SERVICE_ID_TAG
                        | ((name_raw & SERVICE_NAME_MASK) << 8)
                        | (id.as_raw() as u64 & 0xff),
                    &[authority],
                );
            }
            let mut segment_mapped = false;
            let mut behavior = SERVICE_BEHAVIOR_NONE;
            for _ in 0..6 {
                let Ok(message) = runtime.receive() else {
                    break;
                };
                if let Some(segment) = message.capabilities[0] {
                    if runtime
                        .memseg_map(segment, self_cap, 0, USER_HELLO_VA, 1, HELLO_RW_ACCESS)
                        .is_ok()
                    {
                        segment_mapped = true;
                    }
                } else {
                    // SPEC-090：`prepare` 的 payload 低 8 位是管理器路由到的行为码。
                    behavior = message.payload & 0xff;
                }
                if !segment_mapped || behavior == SERVICE_BEHAVIOR_NONE {
                    continue;
                }
                let mut header_bytes = [0u8; MESSAGE_HEADER_LEN];
                let mut offer_bytes = [0u8; OFFER_LEN];
                let header_ok = runtime
                    .read_user_bytes(USER_HELLO_VA, &mut header_bytes)
                    .is_ok();
                let offer_ok = runtime
                    .read_user_bytes(USER_HELLO_VA + MESSAGE_HEADER_LEN as u64, &mut offer_bytes)
                    .is_ok();
                if header_ok && offer_ok {
                    let negotiated =
                        negotiation_word(&header_bytes, &offer_bytes, service_offer(behavior));
                    let negatives = negotiation_negatives();
                    let word_off = SERVICE_NEGOTIATED_OFFSET + (behavior - 1) * 16;
                    let _ = runtime
                        .write_user_bytes(USER_HELLO_VA + word_off, &negotiated.to_le_bytes());
                    // SPEC-092：把自己的 task id 写在结果旁——判决性见证据此把结果绑定到**端点身份**。
                    let ep_off = SERVICE_ENDPOINT_OFFSET + (behavior - 1) * 8;
                    let _ = runtime.write_user_bytes(
                        USER_HELLO_VA + ep_off,
                        &(id.as_raw() as u64).to_le_bytes(),
                    );
                    // SPEC-093：把**自身二进制标记**写在结果旁——两个端点由不同二进制承载，
                    // 标记相异即证明结果来自不同程序（而非同一映像的两次实例）。
                    let marker_off = SERVICE_BINARY_MARKER_OFFSET + (behavior - 1) * 8;
                    let _ = runtime
                        .write_user_bytes(USER_HELLO_VA + marker_off, &BINARY_MARKER.to_le_bytes());
                    let _ = runtime.write_user_bytes(
                        USER_HELLO_VA + SERVICE_NEGATIVES_OFFSET,
                        &negatives.to_le_bytes(),
                    );
                    let _ = runtime.send(peer, send_cap, SERVICE_DONE_TAG, &[]);
                }
                behavior = SERVICE_BEHAVIOR_NONE;
            }
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
                // EXO-3D：把 runtime-hello 的结果/字段作为**通用 trace 记录**交给内核透传，
                // 不再依赖内核读共享段固定偏移。字段打包成 5 个 u64：
                // a=map_result、b=magic_version、c=opcode_request、d=length_version、
                // e=features | required<<16 | max<<32。
                let magic_version = u64::from_le_bytes(message[0..8].try_into().unwrap());
                let opcode_request = u64::from_le_bytes(message[8..16].try_into().unwrap());
                let length_version = u64::from_le_bytes(message[16..24].try_into().unwrap());
                let features = offer.features;
                let required = offer.required_features;
                let max_message_size = offer.max_message_size as u64;
                let packed = features | (required << 16) | (max_message_size << 32);
                let _ = runtime.trace_emit(
                    EVENT_RUNTIME_HELLO,
                    [0, magic_version, opcode_request, length_version, packed],
                );
                // SPEC-090：把**请求服务名**写入共享段（众所周知的常量 `"beta"`），供服务管理器
                // 按名解析端点。客户端是**请求方**，用固定名寻址是常态；运行期指派的是**端点**的
                // 注册名（见下）。
                let _ = runtime.write_user_bytes(
                    USER_HELLO_VA + SERVICE_NAME_OFFSET,
                    &SERVICE_NAME_BETA.to_le_bytes(),
                );
                // SPEC-104：把 Hello 随消息移交给**服务管理器**：payload = 客户端 id，cap = 共享段 +
                // **自管理凭据的派生副本**（仅 `TaskSend`，供管理器回送 `PONG`）。对端 = 入口 `r8`
                // （发送凭据所指 task id）。仅对**服务管理器**（id ≠ 引导任务 1）先发无 cap 唤醒——
                // 它常态阻塞在 `recv`，而内核拒绝向 `BlockedRecv` 接收者投递带 cap 消息；对引导任务
                // 其握手块按**单条**消息接收，故直发。
                if let Ok(authority) = runtime.cap_derive(self_cap, TASK_SEND_RIGHTS) {
                    if peer.as_raw() != BOOTSTRAP_TASK {
                        let _ = runtime.send(peer, send_cap, SERVICE_WAKE_TAG, &[]);
                    }
                    let _ = runtime.send(peer, send_cap, id.as_raw() as u64, &[segment, authority]);
                }
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
