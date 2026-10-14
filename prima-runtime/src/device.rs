//! SPEC-113 / COSIR §15：设备对象（本片仅 `IOPortRange` 种）与**受控 I/O 端口**路径。
//!
//! `dev_ioport_create(base_port, port_count)` 铸出一个覆盖 `[base_port, base_port + port_count)`
//! 的设备 Capability；持有者只能在**该区间内**读/写端口——内核在授权时校验端口落在区间内，
//! 超出即拒绝（受控 I/O 路径的核心约束）。

use crate::{Capability, Result, SyscallTransport, syscall, transport::Runtime};

impl<T: SyscallTransport> Runtime<T> {
    /// §15.5：铸造一个覆盖 `[base_port, base_port + port_count)` 的 `IOPortRange` 设备 Capability。
    pub fn dev_ioport_create(&self, base_port: u16, port_count: u16) -> Result<Capability> {
        let raw = self.call(
            syscall::DEV_IOPORT_CREATE,
            [base_port as u64, port_count as u64, 0, 0, 0, 0],
        )?;
        Capability::from_raw(raw)
    }

    /// §15.5：向 `cap` 授权区间内的 `port` 写一字节（内核代执行 `outb`）。区间外返回 `Denied`/`InvalidInput`。
    pub fn dev_ioport_out(&self, cap: Capability, port: u16, value: u8) -> Result<()> {
        self.call_unit(
            syscall::DEV_IOPORT_OUT,
            [cap.as_raw(), port as u64, value as u64, 0, 0, 0],
        )
    }

    /// §15.5：从 `cap` 授权区间内的 `port` 读一字节（内核代执行 `inb`）。区间外返回 `Denied`/`InvalidInput`。
    pub fn dev_ioport_in(&self, cap: Capability, port: u16) -> Result<u8> {
        let raw = self.call(
            syscall::DEV_IOPORT_IN,
            [cap.as_raw(), port as u64, 0, 0, 0, 0],
        )?;
        Ok(raw as u8)
    }
}
