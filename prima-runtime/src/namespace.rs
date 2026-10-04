//! Namespace 路径解析与 Capability 绑定操作。

use crate::{Capability, Error, Result, Rights, SyscallTransport, syscall, transport::Runtime};

/// 当前 x86_64 原型允许的 Namespace 路径文本最大长度。
pub const MAX_PATH_BYTES: usize = 64;

fn path_args(path: &str) -> Result<[u64; 2]> {
    let bytes = path.as_bytes();
    if bytes.is_empty() || bytes.len() > MAX_PATH_BYTES {
        return Err(Error::InvalidInput);
    }
    Ok([bytes.as_ptr() as usize as u64, bytes.len() as u64])
}

impl<T: SyscallTransport> Runtime<T> {
    /// 将调用者持有的 Capability 绑定到 Namespace 路径。
    ///
    /// 成功后该 token 的内核位置转入 Namespace；只有显式 `namespace_unbind` 才会取回 token。
    pub fn namespace_bind(
        &self,
        namespace: Capability,
        path: &str,
        capability: Capability,
    ) -> Result<()> {
        let [path_ptr, path_len] = path_args(path)?;
        self.call_unit(
            syscall::NS_BIND,
            [
                namespace.as_raw(),
                path_ptr,
                path_len,
                capability.as_raw(),
                0,
                0,
            ],
        )
    }

    /// 从 Namespace 中移除路径并取回绑定的 Capability。
    pub fn namespace_unbind(&self, namespace: Capability, path: &str) -> Result<Capability> {
        let [path_ptr, path_len] = path_args(path)?;
        let raw = self.call(
            syscall::NS_UNBIND,
            [namespace.as_raw(), path_ptr, path_len, 0, 0, 0],
        )?;
        Capability::from_raw(raw)
    }

    /// 在 Namespace 路径挂载另一个 Namespace。
    pub fn namespace_mount(
        &self,
        destination: Capability,
        path: &str,
        source: Capability,
    ) -> Result<()> {
        let [path_ptr, path_len] = path_args(path)?;
        self.call_unit(
            syscall::NS_MOUNT,
            [
                destination.as_raw(),
                path_ptr,
                path_len,
                source.as_raw(),
                0,
                0,
            ],
        )
    }

    /// 移除 Namespace 挂载。
    pub fn namespace_unmount(&self, namespace: Capability, path: &str) -> Result<()> {
        let [path_ptr, path_len] = path_args(path)?;
        self.call_unit(
            syscall::NS_UNMOUNT,
            [namespace.as_raw(), path_ptr, path_len, 0, 0, 0],
        )
    }

    /// 按路径解析并请求一组不超过父 Capability 权限的权限。
    pub fn namespace_resolve(
        &self,
        namespace: Capability,
        path: &str,
        requested: Rights,
    ) -> Result<Capability> {
        let [path_ptr, path_len] = path_args(path)?;
        let raw = self.call(
            syscall::NS_RESOLVE,
            [
                namespace.as_raw(),
                path_ptr,
                path_len,
                requested.bits() as u64,
                0,
                0,
            ],
        )?;
        Capability::from_raw(raw)
    }

    /// 销毁 Namespace 对象。
    pub fn namespace_destroy(&self, namespace: Capability) -> Result<()> {
        self.call_unit(syscall::NS_DESTROY, [namespace.as_raw(), 0, 0, 0, 0, 0])
    }
}
