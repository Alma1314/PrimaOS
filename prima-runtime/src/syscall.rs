//! 当前 Prima 内核原型的 syscall 号与 errno 编码。
//!
//! 此处映射为 crate 内部实现细节。不得将这些编号作为独立稳定 ABI 暴露给应用。

pub(crate) use prima_abi::syscall::{
    SYS_CAP_DERIVE as CAP_DERIVE, SYS_CAP_DROP as CAP_DROP,
    SYS_DEV_IOPORT_CREATE as DEV_IOPORT_CREATE, SYS_DEV_IOPORT_IN as DEV_IOPORT_IN,
    SYS_DEV_IOPORT_OUT as DEV_IOPORT_OUT, SYS_MEMSEG_CREATE as MEMSEG_CREATE,
    SYS_MEMSEG_DESTROY as MEMSEG_DESTROY, SYS_MEMSEG_MAP as MEMSEG_MAP,
    SYS_MEMSEG_UNMAP as MEMSEG_UNMAP, SYS_NS_BIND as NS_BIND, SYS_NS_CREATE as NAMESPACE_CREATE,
    SYS_NS_DESTROY as NS_DESTROY, SYS_NS_MOUNT as NS_MOUNT, SYS_NS_RESOLVE as NS_RESOLVE,
    SYS_NS_UNBIND as NS_UNBIND, SYS_NS_UNMOUNT as NS_UNMOUNT, SYS_RECV as RECV, SYS_SEND as SEND,
    SYS_TASK_CREATE as TASK_CREATE, SYS_TASK_DESTROY as TASK_DESTROY, SYS_TASK_ID as TASK_ID,
    SYS_TIME_NOW as TIME_NOW, SYS_TIMER_CANCEL as TIMER_CANCEL, SYS_TIMER_DESTROY as TIMER_DESTROY,
    SYS_TIMER_SET as TIMER_SET, SYS_YIELD as YIELD,
};

/// 将当前原型 syscall ABI 的返回值解码为成功值或 errno。
///
/// 只把本内核目前定义的负 errno 编码识别为错误；其他 64 位值（包括最高位为 1 的
/// Capability token）保持为成功结果。
pub(crate) const fn decode(raw: u64) -> crate::Result<u64> {
    match raw {
        0xffff_ffff_ffff_fffe => Err(crate::Error::NotFound),
        0xffff_ffff_ffff_fff5 => Err(crate::Error::Again),
        0xffff_ffff_ffff_fff3 => Err(crate::Error::Denied),
        0xffff_ffff_ffff_fff2 => Err(crate::Error::Fault),
        0xffff_ffff_ffff_ffef => Err(crate::Error::AlreadyExists),
        0xffff_ffff_ffff_ffec => Err(crate::Error::NotDirectory),
        0xffff_ffff_ffff_ffea => Err(crate::Error::InvalidInput),
        0xffff_ffff_ffff_ffe4 => Err(crate::Error::ResourceExhausted),
        0xffff_ffff_ffff_ffda => Err(crate::Error::Unsupported),
        value if value > i64::MAX as u64 => {
            let signed = value as i64;
            if signed >= -4095 {
                Err(crate::Error::UnknownErrno(signed))
            } else {
                Ok(value)
            }
        }
        value => Ok(value),
    }
}
