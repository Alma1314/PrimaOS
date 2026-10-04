//! Runtime 调用的稳定错误表示。

/// `Result` 使用的错误类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Capability 编码的槽索引或 generation 为零。
    MalformedCapability,
    /// 内核返回 `ENOENT`。
    NotFound,
    /// 内核返回 `EAGAIN`。
    Again,
    /// 内核返回 `EACCES`。
    Denied,
    /// 内核返回 `EFAULT`。
    Fault,
    /// 内核返回 `EEXIST`。
    AlreadyExists,
    /// 内核返回 `ENOTDIR`。
    NotDirectory,
    /// 内核返回 `EINVAL`。
    InvalidInput,
    /// 内核返回 `ENOSPC`。
    ResourceExhausted,
    /// 内核返回 `ENOSYS`。
    Unsupported,
    /// 收到当前运行时尚未分类的负 errno。保留原始数值供调用方诊断。
    UnknownErrno(i64),
    /// 传输层拒绝调用或目标入口不可用。
    Transport(TransportError),
    /// 操作成功，但返回值不符合运行时 API 的预期格式。
    InvalidResponse,
}

/// Runtime 传输适配器返回的错误。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportError {
    NotUserMode,
}

/// Runtime 操作的返回类型。
pub type Result<T> = core::result::Result<T, Error>;
