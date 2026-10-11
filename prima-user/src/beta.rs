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
        MESSAGE_HEADER_LEN, OFFER_LEN, negotiation_negatives, negotiation_word, service_offer,
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
/// SPEC-088：服务端点自报 task id 的消息 tag（高 16 位；低 16 位为 id）。
const SERVICE_ID_TAG: u64 = 0x5EC1_0000_0000_0000;
/// SPEC-088：服务端点完成协商的消息 tag。
const SERVICE_DONE_TAG: u64 = 0x5EC2_0000_0000_0000;
const HELLO_RW_ACCESS: Rights = Rights::from_bits((1 << 1) | (1 << 2));

/// `BETA` 服务端点入口。装载器提供 ring3 栈；入口参数由 ABI 保留。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(self_cap_raw: u64, send_cap_raw: u64, _initial_cap1_raw: u64) -> ! {
    let runtime = Runtime::x86_64();
    if let (Ok(self_cap), Ok(send_cap), Ok(id)) = (
        Capability::from_raw(self_cap_raw),
        Capability::from_raw(send_cap_raw),
        runtime.task_id(),
    ) {
        // 自报 id：manager 用第二个 `SERVICE_ID_TAG` 收集本端点 id。
        let _ = runtime.send(
            TaskId::from_raw(1),
            send_cap,
            SERVICE_ID_TAG | id.as_raw() as u64,
            &[],
        );
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
                let _ = runtime.send(TaskId::from_raw(1), send_cap, SERVICE_DONE_TAG, &[]);
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
