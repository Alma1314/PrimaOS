//! 消息传递与显式 Capability 转移。

use crate::{Capability, Error, Result, SyscallTransport, syscall, transport::Runtime};

/// 当前内核原型允许单条消息携带的 Capability 数。
pub const MAX_MESSAGE_CAPABILITIES: usize = 2;

/// 从 `recv` 收到的消息。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Message {
    pub payload: u64,
    /// Authenticated local sender; None denotes a kernel notification.
    pub sender: Option<crate::TaskId>,
    pub sender_generation: u32,
    pub id: u32,
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
        let mut out = [0u64; 5];
        let raw_payload = self.call(
            syscall::RECV_META,
            [out.as_mut_ptr() as usize as u64, 0, 0, 0, 0, 0],
        )?;
        decode_message(raw_payload, out)
    }
}

fn decode_message(payload: u64, out: [u64; 5]) -> Result<Message> {
    let count = usize::try_from(out[2]).map_err(|_| Error::InvalidResponse)?;
    if count > MAX_MESSAGE_CAPABILITIES {
        return Err(Error::InvalidResponse);
    }
    let sender = out[3] as u32;
    let sender_generation = (out[3] >> 32) as u32;
    if (sender == 0) != (sender_generation == 0) {
        return Err(Error::InvalidResponse);
    }
    let id = u32::try_from(out[4]).map_err(|_| Error::InvalidResponse)?;
    if id == 0 {
        return Err(Error::InvalidResponse);
    }
    let mut capabilities = [None; MAX_MESSAGE_CAPABILITIES];
    for index in 0..count {
        capabilities[index] = Some(Capability::from_raw(out[index])?);
    }
    Ok(Message {
        payload,
        capabilities,
        sender: (sender != 0).then_some(crate::TaskId::from_raw(sender)),
        sender_generation,
        id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receive_metadata_distinguishes_kernel_origin_and_rejects_invalid_ids() {
        let user = decode_message(99, [0, 0, 0, (3u64 << 32) | 7, 42]).unwrap();
        assert_eq!(user.sender.map(crate::TaskId::as_raw), Some(7));
        assert_eq!(user.id, 42);
        assert_eq!(user.sender_generation, 3);
        assert_eq!(decode_message(99, [0, 0, 0, 0, 43]).unwrap().sender, None);
        for out in [
            [0, 0, 3, (3u64 << 32) | 7, 1],
            [0, 0, 0, (3u64 << 32) | 7, 0],
            [0, 0, 0, 1u64 << 32, 1],
            [0, 0, 0, 7, 1],
            [0, 0, 0, (3u64 << 32) | 7, u64::MAX],
        ] {
            assert_eq!(decode_message(99, out), Err(Error::InvalidResponse));
        }
    }
}
