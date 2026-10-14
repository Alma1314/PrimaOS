//! SPEC-106（S4-D3）：**日志服务端点**的独立二进制映像（`prima-log`）。
//!
//! 与 `prima-ns`/`prima-store` 共用 [`service`] 骨架，仅注册名/行为码/存在标记相异。
//!
//! SPEC-112（S4-D3 收口）：本服务经 `SERVICE_SEG_TAG` 收到管理器移交的一个内核
//! `MemorySegment`，映射进自身根后**落日志**——状态写入段、查询从段读回。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

#[path = "service.rs"]
mod service;

use prima_runtime::protocol::{SERVICE_BEHAVIOR_LOG, SERVICE_MARKER_LOG, SERVICE_NAME_LOG};

/// 日志服务入口。装载器提供 ring3 栈；入口参数由 ABI 保留（见 `service::run`）。
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
        SERVICE_NAME_LOG,
        SERVICE_BEHAVIOR_LOG,
        SERVICE_MARKER_LOG,
        self_cap_raw,
        send_cap_raw,
        peer_raw,
    )
}
