//! SPEC-093：**第二个服务端点的独立二进制映像**（`BETA` 端点）。
//!
//! 与 `prima-smoke`（客户端 + `ALPHA` 端点）是**不同的 ELF**：内核按名把 `BETA` 请求路由到一个
//! 由本映像创建的任务。两者共用 `prima-runtime::protocol` 的协商助手，但 `_start` 与内嵌的
//! [`BINARY_MARKER`] 相异——内核见证据此区分「不同程序」，而非同一映像的两次实例。
//!
//! 服务端协议与内核 broker 约定见 `kernel/src/user.rs`（偏移/标记同款常量）。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{
    Capability, Rights, Runtime, TaskId,
    protocol::{
        MESSAGE_HEADER_LEN, OFFER_LEN, SERVICE_NAME_MASK, SERVICE_WAKE_TAG, negotiation_negatives,
        negotiation_word, service_offer,
    },
};

const USER_HELLO_VA: u64 = 0x0000_0A00_0000_7000;
/// SPEC-088：服务端点把协商结果写回共享段内的偏移（`128 + (behavior-1)*16`）。
const SERVICE_NEGOTIATED_OFFSET: u64 = 128;
/// SPEC-088：服务端点把两个负例码写回共享段内的偏移。
const SERVICE_NEGATIVES_OFFSET: u64 = 136;
/// SPEC-092：服务端点把**自己的 task id** 写回共享段内的偏移（`160 + (behavior-1)*8`）。
const SERVICE_ENDPOINT_OFFSET: u64 = 160;
/// SPEC-093：服务端点把**编译进自身二进制的标记**写回共享段内的偏移（`176 + (behavior-1)*8`）。
const SERVICE_BINARY_MARKER_OFFSET: u64 = 176;
/// SPEC-093：`prima-beta`（`BETA` 端点）的二进制标记。**必须与 `prima-smoke` 的相异**。
const BINARY_MARKER: u64 = 0x4245_5441_0000_0002;
/// SPEC-088/101：端点自报的消息 tag（高 16 位）；payload 布局 `tag(48..63) | name(8..47) | id(0..7)`。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// SPEC-088：服务端点完成协商的消息 tag。
const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
const HELLO_RW_ACCESS: Rights = Rights::from_bits((1 << 1) | (1 << 2));
/// SPEC-104：移交给服务管理器的发送权（仅 `TaskSend`）——派生副本，端点自己保留 self cap。
const TASK_SEND_RIGHTS: Rights = Rights::from_bits(1 << 5);
/// SPEC-104：引导任务（init）的 task id。对端是它时**不**发无 cap 唤醒。
const BOOTSTRAP_TASK: u32 = 1;

/// `BETA` 服务端点入口。装载器提供 ring3 栈；入口参数由 ABI 保留。
///
/// SPEC-101：第 4 参数（`rcx`）是服务管理器在创建时**运行期指派**的注册名；本映像不含名字字面量。
///
/// SPEC-104：第 5 参数（`r8`）是 `initial_caps[0]`（发送凭据）所指向的**对端 task id**，
/// 由 `task_create` 运行期指派——`send` 的接收者必须等于凭据目标，故据此寻址而非硬编码 task 1。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    self_cap_raw: u64,
    send_cap_raw: u64,
    _initial_cap1_raw: u64,
    name_raw: u64,
    peer_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    let peer = TaskId::from_raw(peer_raw as u32);
    if let (Ok(self_cap), Ok(send_cap), Ok(id)) = (
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(send_cap_raw),
        runtime.task_id(),
    ) {
        // SPEC-104：向管理器移交**自身自管理凭据的派生副本**（仅 `TaskSend`）——端点自己仍须
        // 持有原 self cap（随后 `memseg_map` 需要它）。保留 SPEC-104 的**无 cap 唤醒**
        // 与带 cap 自报两消息协议；阶段 2 的直投修复已解除内核限制。
        if let Ok(authority) = runtime.cap_derive(self_cap, TASK_SEND_RIGHTS) {
            if peer.as_raw() != BOOTSTRAP_TASK {
                let _ = runtime.send(peer, send_cap, SERVICE_WAKE_TAG, &[]);
            }
            // 自报 id + **运行期指派的注册名**：manager 用第二个 `SERVICE_ID_TAG` 收集本端点 (name, id)。
            let _ = runtime.send(
                peer,
                send_cap,
                SERVICE_ID_TAG
                    | ((name_raw & SERVICE_NAME_MASK) << 8)
                    | (id.as_raw() as u64 & 0xff),
                &[authority],
            );
        }
        let mut segment_mapped = false;
        let mut behavior = 0u64;
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
                // `prepare` 的 payload 低 8 位是管理器路由到的行为码（本端点应为 `BETA`）。
                behavior = message.payload & 0xff;
            }
            if !segment_mapped || behavior == 0 {
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
                let _ =
                    runtime.write_user_bytes(USER_HELLO_VA + word_off, &negotiated.to_le_bytes());
                let ep_off = SERVICE_ENDPOINT_OFFSET + (behavior - 1) * 8;
                let _ = runtime
                    .write_user_bytes(USER_HELLO_VA + ep_off, &(id.as_raw() as u64).to_le_bytes());
                let marker_off = SERVICE_BINARY_MARKER_OFFSET + (behavior - 1) * 8;
                let _ = runtime
                    .write_user_bytes(USER_HELLO_VA + marker_off, &BINARY_MARKER.to_le_bytes());
                let _ = runtime.write_user_bytes(
                    USER_HELLO_VA + SERVICE_NEGATIVES_OFFSET,
                    &negatives.to_le_bytes(),
                );
                let _ = runtime.send(peer, send_cap, SERVICE_DONE_TAG, &[]);
            }
            behavior = 0;
        }
    }
    loop {
        let _ = runtime.receive();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
