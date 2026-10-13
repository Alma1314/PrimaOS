//! SPEC-103：**ring3 服务管理器的独立二进制映像**。
//!
//! 决策见 `docs/superpowers/specs/2026-10-13-stage4-ring3-service-manager-design.md`：把服务发现
//! 与路由从 `init` 的手写 x86 机器码迁到一个独立的 ring3 Rust 任务。本片先证明「服务管理器是一个
//! 被 `init` 建起的独立 ring3 任务、并收到 init 的移交」；运行期注册表、按名解析与请求转发的迁移
//! 见后续 SPEC-104。
//!
//! 本映像**不含任何服务名字面量**——它只向 init 自报存活（携带自身 task id）。

#![no_std]
#![no_main]
#![deny(unsafe_code)]

use core::panic::PanicInfo;
use prima_runtime::{Capability, Runtime, TaskId, protocol::SERVICEMGR_HELLO_TAG};

/// 服务管理器入口。装载器提供 ring3 栈；入口参数由 ABI 保留。
///
/// 第 2 参数（`rsi` = `initial_caps[0]`）是 `init` 移交的**回发凭据**（目标 task 1 + `TaskSend`），
/// 服务管理器用它向 init 自报存活。
#[unsafe(no_mangle)]
#[allow(unsafe_code)]
pub extern "C" fn _start(
    _self_cap_raw: u64,
    send_cap_raw: u64,
    _initial_cap1_raw: u64,
    _name_raw: u64,
) -> ! {
    let runtime = Runtime::x86_64();
    if let (Ok(send_cap), Ok(id)) = (Capability::from_raw(send_cap_raw), runtime.task_id()) {
        // 自报存活：payload = `SERVICEMGR_HELLO_TAG` 高 16 位 ‖ 自身 task id 低 8 位。
        let _ = runtime.send(
            TaskId::from_raw(1),
            send_cap,
            SERVICEMGR_HELLO_TAG | (id.as_raw() as u64 & 0xff),
            &[],
        );
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
