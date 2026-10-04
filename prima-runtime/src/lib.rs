#![no_std]
#![forbid(unsafe_code)]

//! Prima 原生用户态运行时的初始 API 层。
//!
//! 该 crate 把 syscall 传输细节与应用接口分开，并提供服务协议版本协商。
//! 当前 x86_64 syscall 编号只是内核原型映射，不是稳定 ABI；应用只调用本 crate 的方法。
//! Capability 包装只做格式检查，所有授权仍由内核验证。关闭包装值不会自动释放内核
//! Capability；调用 [`Runtime::cap_drop`] 才会明确请求释放当前位置。

mod syscall;

pub mod capability;
pub mod error;
pub mod ipc;
pub mod memory;
pub mod namespace;
pub mod protocol;
pub mod task;
pub mod transport;

pub use capability::{Capability, Rights};
pub use error::{Error, Result, TransportError};
pub use ipc::Message;
pub use task::{TaskId, TaskSpec};
pub use transport::{Runtime, SyscallTransport, TimerCancelOutcome};

/// 当前 crate 的源码 API 主版本。仍处于 `0.x` 阶段，兼容性按仓库的 S4-D1 约定管理。
pub const API_MAJOR: u16 = 0;
/// 当前 crate 的源码 API 次版本。
pub const API_MINOR: u16 = 1;

/// 当前 x86_64 Prima 用户态 syscall 入口的传输适配器。
#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug, Default)]
pub struct X86_64Transport;

#[cfg(target_arch = "x86_64")]
impl transport::SyscallTransport for X86_64Transport {
    fn invoke(&self, number: u64, args: [u64; 6]) -> core::result::Result<u64, TransportError> {
        prima_ostd::entry::syscall_from_user(number, args).map_err(|error| match error {
            prima_ostd::entry::UserSyscallError::NotUserMode => TransportError::NotUserMode,
        })
    }
}

#[cfg(target_arch = "x86_64")]
impl Runtime<X86_64Transport> {
    /// 创建使用 Prima x86_64 ring3 syscall 入口的 Runtime。
    pub const fn x86_64() -> Self {
        Self::new(X86_64Transport)
    }
}
