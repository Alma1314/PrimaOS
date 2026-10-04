//! 不透明 Capability 句柄的用户态包装。

use crate::Error;

/// 已通过句柄编码格式检查的 Capability token。
///
/// 该类型不证明 token 仍然有效，也不证明它拥有某项权限。复制 token 不会复制内核中的
/// Authority；具体操作仍由内核依据 Capability 的当前位置和权限判定。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Capability(u64);

impl Capability {
    /// 从 syscall 返回值或 IPC payload 解码句柄。
    ///
    /// 格式使用低 32 位槽索引和高 32 位 generation；二者均不得为零。该检查仅排除
    /// 格式错误，不执行内核查询。
    pub const fn from_raw(raw: u64) -> Result<Self, Error> {
        let index = raw as u32;
        let generation = (raw >> 32) as u32;
        if index == 0 || generation == 0 {
            Err(Error::MalformedCapability)
        } else {
            Ok(Self(raw))
        }
    }

    /// 返回传给传输层的 opaque token。
    pub const fn as_raw(self) -> u64 {
        self.0
    }
}

/// Runtime 权限位集。位号由内核规范定义；构造该值本身不会授予对应权限。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct Rights(u32);

impl Rights {
    /// 从规范定义的位集构造权限请求。
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// 返回权限位集，供内核原语和服务协议使用。
    pub const fn bits(self) -> u32 {
        self.0
    }
}

impl<T: crate::SyscallTransport> crate::Runtime<T> {
    /// 从父 Capability 派生权限不高于父项的新 Capability。
    pub fn cap_derive(&self, parent: Capability, requested: Rights) -> crate::Result<Capability> {
        let raw = self.call(
            crate::syscall::CAP_DERIVE,
            [parent.as_raw(), requested.bits() as u64, 0, 0, 0, 0],
        )?;
        Capability::from_raw(raw)
    }
}
