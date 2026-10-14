//! SPEC-106（S4-D3）：**基础存储服务端点**的独立二进制映像（`prima-store`）。
//!
//! 与 `prima-log`/`prima-ns` 共用 [`service`] 骨架，仅注册名/行为码/存在标记相异。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

#[path = "service.rs"]
mod service;

use prima_runtime::protocol::{SERVICE_BEHAVIOR_STORE, SERVICE_MARKER_STORE, SERVICE_NAME_STORE};

/// 基础存储服务入口。装载器提供 ring3 栈；入口参数由 ABI 保留（见 `service::run`）。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    self_cap_raw: u64,
    send_cap_raw: u64,
    _initial_cap1_raw: u64,
    name_raw: u64,
    peer_raw: u64,
) -> ! {
    let _ = name_raw;
    service::run(
        SERVICE_NAME_STORE,
        SERVICE_BEHAVIOR_STORE,
        SERVICE_MARKER_STORE,
        self_cap_raw,
        send_cap_raw,
        peer_raw,
    )
}
