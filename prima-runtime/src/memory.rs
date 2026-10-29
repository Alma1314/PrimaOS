//! 用户地址空间与 MemorySegment 操作。

use crate::{Capability, Error, Result, Rights, SyscallTransport, syscall, transport::Runtime};

impl<T: SyscallTransport> Runtime<T> {
    /// 创建 Segment，返回其根 Capability。
    pub fn memseg_create(&self, pages: u64, allowed_access: Rights) -> Result<Capability> {
        let raw = self.call(
            syscall::MEMSEG_CREATE,
            [pages, allowed_access.bits(), 0, 0, 0, 0],
        )?;
        Capability::from_raw(raw)
    }

    /// 将 Segment 的一段映射到目标 Task 的地址空间。
    pub fn memseg_map(
        &self,
        segment: Capability,
        task: Capability,
        segment_offset: u64,
        target_address: u64,
        pages: u64,
        access: Rights,
    ) -> Result<()> {
        self.call_unit(
            syscall::MEMSEG_MAP,
            [
                segment.as_raw(),
                task.as_raw(),
                segment_offset,
                target_address,
                pages,
                access.bits(),
            ],
        )
    }

    /// 解除目标 Task 地址空间中的映射。
    pub fn memseg_unmap(&self, task: Capability, target_address: u64, pages: u64) -> Result<()> {
        self.call_unit(
            syscall::MEMSEG_UNMAP,
            [task.as_raw(), target_address, pages, 0, 0, 0],
        )
    }

    /// 销毁 Segment 对象。
    pub fn memseg_destroy(&self, segment: Capability) -> Result<()> {
        self.call_unit(syscall::MEMSEG_DESTROY, [segment.as_raw(), 0, 0, 0, 0, 0])
    }

    /// 将字节写入当前 Task 已映射且可写的用户地址区间。
    ///
    /// 调用方必须先通过 `memseg_map` 建立覆盖整个区间的可写映射；无效或只读地址仍由
    /// CPU 页表权限保护并可能触发用户态 Page Fault。地址加法溢出会在触碰内存前拒绝。
    pub fn write_user_bytes(&self, address: u64, bytes: &[u8]) -> Result<()> {
        address
            .checked_add(bytes.len() as u64)
            .ok_or(Error::InvalidInput)?;
        for (index, byte) in bytes.iter().copied().enumerate() {
            prima_ostd::mm::write_u8_at(address, index as u64, byte);
        }
        Ok(())
    }

    /// 从当前 Task 已映射且可读的用户地址区间读回字节。
    ///
    /// SPEC-088：服务端解析对端写入共享 Segment 的 wire 消息时需要读回。调用方必须先通过
    /// `memseg_map` 建立覆盖整个区间的可读映射；地址加法溢出会在触碰内存前拒绝。
    pub fn read_user_bytes(&self, address: u64, out: &mut [u8]) -> Result<()> {
        address
            .checked_add(out.len() as u64)
            .ok_or(Error::InvalidInput)?;
        for (index, byte) in out.iter_mut().enumerate() {
            *byte = prima_ostd::mm::read_u8_at(address, index as u64);
        }
        Ok(())
    }

    /// 读回一个以 little-endian 编码的 u64。
    pub fn read_user_u64(&self, address: u64) -> Result<u64> {
        let mut bytes = [0u8; 8];
        self.read_user_bytes(address, &mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }
}
