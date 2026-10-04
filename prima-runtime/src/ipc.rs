//! 消息传递与显式 Capability 转移。

use crate::{Capability, Error, Result, SyscallTransport, syscall, transport::Runtime};

/// 当前内核原型允许单条消息携带的 Capability 数。
pub const MAX_MESSAGE_CAPABILITIES: usize = 2;

/// 从 `recv` 收到的消息。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message {
    pub payload: u64,
    pub capabilities: [Option<Capability>; MAX_MESSAGE_CAPABILITIES],
}

impl<T: SyscallTransport> Runtime<T> {
    /// 将消息发送给目标 Task。最多可随消息移动两个 Capability。
    ///
    /// 成功后这些 Capability 的内核位置转入消息/接收者；失败时沿用内核操作的原子失败语义。
    pub fn send(
        &self,
        receiver: crate::task::TaskId,
        send_authority: Capability,
        payload: u64,
        capabilities: &[Capability],
    ) -> Result<()> {
        if capabilities.len() > MAX_MESSAGE_CAPABILITIES {
            return Err(Error::InvalidInput);
        }
        let cap0 = capabilities.first().map_or(0, |cap| cap.as_raw());
        let cap1 = capabilities.get(1).map_or(0, |cap| cap.as_raw());
        self.call_unit(
            syscall::SEND,
            [
                receiver.as_raw() as u64,
                send_authority.as_raw(),
                payload,
                capabilities.len() as u64,
                cap0,
                cap1,
            ],
        )
    }

    /// 接收一条消息；队列为空时会阻塞当前 Task，直到被唤醒后返回。
    pub fn receive(&self) -> Result<Message> {
        let mut out = [0u64; 3];
        let raw_payload = self.call(
            syscall::RECV,
            [out.as_mut_ptr() as usize as u64, 0, 0, 0, 0, 0],
        )?;
        let count = out[2] as usize;
        if count > MAX_MESSAGE_CAPABILITIES {
            return Err(Error::InvalidResponse);
        }
        let mut capabilities = [None; MAX_MESSAGE_CAPABILITIES];
        for index in 0..count {
            capabilities[index] = Some(Capability::from_raw(out[index])?);
        }
        Ok(Message {
            payload: raw_payload,
            capabilities,
        })
    }
}
