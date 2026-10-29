//! Opt-in D14 ring3 experiment using existing native mechanisms. The two
//! personality stubs reuse ALPHA/BETA; no Linux gate or new kernel object.
use prima_runtime::{Capability, Error, Message, Rights, Runtime, SyscallTransport, TaskId};

pub const ENABLED: bool = option_env!("PRIMA_STAGE6_DOMAINS").is_some();
pub const INIT: u64 = 0xd610_0000_0000_0000;
pub const FOREIGN: u64 = 0xd611_0000_0000_0000;
pub const LOG: u64 = 0xd612_0000_0000_0000;
pub const TAG_MASK: u64 = 0xffff_0000_0000_0000;
pub const WORD_MASK: u64 = !TAG_MASK;
const VA: u64 = 0x0000_0b00_0030_0000;
const RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));
const SEND: Rights = Rights::from_bits(1 << 5);

#[derive(Default)]
pub struct Client {
    root: Option<Capability>,
    pattern: u64,
}
impl Client {
    pub fn handle(
        &mut self,
        runtime: &Runtime<impl SyscallTransport>,
        self_cap: Capability,
        peer: TaskId,
        reply: Capability,
        message: Message,
    ) -> bool {
        if !ENABLED || message.sender != Some(peer) {
            return false;
        }
        match message.payload & TAG_MASK {
            INIT => {
                self.root = message.capabilities[0];
                self.pattern = runtime.task_id().map_or(0, |id| u64::from(id.as_raw()));
                if let Ok(segment) = runtime.memseg_create(1, RW) {
                    let _ = runtime.memseg_map(segment, self_cap, 0, VA, 1, RW);
                    let _ = runtime.write_user_bytes(VA, &self.pattern.to_le_bytes());
                }
                let _ = runtime.send(peer, reply, INIT | self.pattern, &[]);
                true
            }
            FOREIGN => {
                let mut flags = 0;
                if let Ok(foreign) = Capability::from_raw(message.payload & WORD_MASK)
                    && runtime.namespace_resolve(foreign, "log", SEND) == Err(Error::Denied)
                {
                    flags |= 1;
                }
                if runtime.read_user_u64(VA) == Ok(self.pattern) && self.pattern != 0 {
                    flags |= 2;
                }
                if let Some(root) = self.root
                    && let Ok(log) = runtime.namespace_resolve(root, "log", SEND)
                {
                    flags |= 4;
                    let _ = runtime.send(peer, log, LOG | (self.pattern << 8) | flags, &[]);
                    let _ = runtime.cap_drop(log);
                } else {
                    let _ = runtime.send(peer, reply, LOG | (self.pattern << 8) | flags, &[]);
                }
                true
            }
            _ => false,
        }
    }
}
