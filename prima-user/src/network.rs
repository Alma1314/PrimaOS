//! Bounded UDP objects with Process holders and authenticated Task transactions.
//! A blocked/committed receive owns a snapshot until completion or cancellation.
use crate::sessions::{MAX_CLIENTS, MAX_OWNERS};
use prima_runtime::{Capability, protocol::*};

const EAGAIN: u64 = 11;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Datagram {
    port: u16,
    len: u8,
    bytes: [u8; SERVICE_NET_DATAGRAM_MAX],
}
impl Datagram {
    const EMPTY: Self = Self {
        port: 0,
        len: 0,
        bytes: [0; SERVICE_NET_DATAGRAM_MAX],
    };
}
#[derive(Clone, Copy)]
struct Socket {
    holders: u32,
    port: u16,
    head: u8,
    count: u8,
    queue: [Datagram; SERVICE_NET_QUEUE],
}
impl Socket {
    const EMPTY: Self = Self {
        holders: 0,
        port: 0,
        head: 0,
        count: 0,
        queue: [Datagram::EMPTY; SERVICE_NET_QUEUE],
    };
}
#[derive(Clone, Copy)]
struct Sending {
    cookie: u64,
    handle: u8,
    destination: u16,
    data: Datagram,
    got: u8,
}
#[derive(Clone, Copy)]
struct Receiving {
    cookie: u64,
    reply: Capability,
    handle: u8,
    sequence: u8,
    order: u64,
    data: Option<Datagram>,
}
#[derive(Clone, Copy)]
pub struct Delivery {
    pub cookie: u64,
    pub reply: Capability,
    pub payload: u64,
    receiving: bool,
}
pub struct Network {
    owners: [u64; MAX_OWNERS],
    sockets: [Socket; SERVICE_NET_SOCKETS],
    sending: [Option<Sending>; MAX_CLIENTS],
    receiving: [Option<Receiving>; MAX_CLIENTS],
    replies: [Option<Delivery>; MAX_CLIENTS],
    next_port: u16,
    next_read: u64,
}
impl Default for Network {
    fn default() -> Self {
        Self::new()
    }
}
impl Network {
    pub const fn new() -> Self {
        Self {
            owners: [0; MAX_OWNERS],
            sockets: [Socket::EMPTY; SERVICE_NET_SOCKETS],
            sending: [None; MAX_CLIENTS],
            receiving: [None; MAX_CLIENTS],
            replies: [None; MAX_CLIENTS],
            next_port: SERVICE_NET_EPHEMERAL_PORT,
            next_read: 0,
        }
    }
    pub fn owner_cookies(&self) -> [u64; MAX_OWNERS] {
        self.owners
    }
    pub fn client_cookies(&self) -> [u64; MAX_CLIENTS] {
        core::array::from_fn(|i| {
            self.sending[i]
                .map(|s| s.cookie)
                .or_else(|| self.receiving[i].map(|r| r.cookie))
                .or_else(|| self.replies[i].map(|r| r.cookie))
                .unwrap_or(0)
        })
    }
    fn owner(&self, cookie: u64) -> Option<usize> {
        self.owners.iter().position(|c| *c != 0 && *c == cookie)
    }
    fn ensure_owner(&mut self, cookie: u64) -> Option<usize> {
        if cookie >> 32 == 0 {
            return None;
        }
        if let Some(i) = self.owner(cookie) {
            return Some(i);
        }
        let index = self.owners.iter().position(|c| *c == 0)?;
        self.owners[index] = cookie;
        Some(index)
    }
    pub fn inherit(&mut self, parent: u64, child: u64) -> bool {
        let Some(parent) = self.owner(parent) else {
            return true;
        };
        let Some(child) = self.ensure_owner(child) else {
            return false;
        };
        for socket in &mut self.sockets {
            if socket.holders & (1 << parent) != 0 {
                socket.holders |= 1 << child;
            }
        }
        true
    }
    pub fn release_owner(&mut self, cookie: u64) {
        if let Some(owner) = self.owner(cookie) {
            for socket in &mut self.sockets {
                socket.holders &= !(1 << owner);
            }
            self.owners[owner] = 0;
        }
        self.reclaim();
    }
    pub fn drop_client(&mut self, cookie: u64) {
        let i = cookie as u32 as usize;
        if i >= MAX_CLIENTS {
            return;
        }
        if self.sending[i].is_some_and(|s| s.cookie == cookie) {
            self.sending[i] = None;
        }
        if self.receiving[i].is_some_and(|r| r.cookie == cookie) {
            self.receiving[i] = None;
        }
        if self.replies[i].is_some_and(|r| r.cookie == cookie) {
            self.replies[i] = None;
        }
        self.reclaim();
    }
    fn retained(&self, handle: usize) -> bool {
        self.sockets[handle].holders != 0
            || self
                .sending
                .iter()
                .flatten()
                .any(|s| s.handle as usize == handle)
            || self
                .receiving
                .iter()
                .flatten()
                .any(|r| r.handle as usize == handle)
    }
    fn reclaim(&mut self) {
        for handle in 0..SERVICE_NET_SOCKETS {
            if !self.retained(handle) {
                self.sockets[handle] = Socket::EMPTY;
            }
        }
    }
    fn owns(&self, owner: usize, handle: u8) -> bool {
        self.sockets
            .get(handle as usize)
            .is_some_and(|s| s.holders & (1 << owner) != 0)
    }
    fn bind(&mut self, handle: u8, port: u16) -> Result<u16, u64> {
        if self.sockets[handle as usize].port != 0 {
            return Err(SERVICE_FILE_EINVAL);
        }
        let mut assigned = if port == 0 { self.next_port } else { port };
        for _ in 0..=SERVICE_NET_SOCKETS {
            let busy = self
                .sockets
                .iter()
                .enumerate()
                .any(|(i, s)| i != handle as usize && self.retained(i) && s.port == assigned);
            if !busy {
                self.sockets[handle as usize].port = assigned;
                if port == 0 {
                    self.next_port = if assigned == u16::MAX {
                        SERVICE_NET_EPHEMERAL_PORT
                    } else {
                        assigned + 1
                    };
                }
                return Ok(assigned);
            }
            if port != 0 {
                return Err(SERVICE_NET_EADDRINUSE);
            }
            assigned = if assigned == u16::MAX {
                SERVICE_NET_EPHEMERAL_PORT
            } else {
                assigned + 1
            };
        }
        Err(SERVICE_FILE_ENOSPC)
    }
    fn operation(
        &mut self,
        cookie: u64,
        owner: usize,
        op: u64,
        handle: u8,
        arg: u64,
    ) -> Result<u64, u64> {
        let i = cookie as u32 as usize;
        if op == SERVICE_NET_OP_CANCEL
            || op == SERVICE_NET_OP_FINISH
            || op == SERVICE_NET_OP_RECV_CHUNK
        {
            let sequence = if op == SERVICE_NET_OP_RECV_CHUNK {
                (arg >> 16) as u8
            } else {
                arg as u8
            };
            let Some(read) = self.receiving[i]
                .filter(|r| r.cookie == cookie && r.handle == handle && r.sequence == sequence)
            else {
                return if op == SERVICE_NET_OP_CANCEL {
                    Ok(1)
                } else {
                    Err(SERVICE_FILE_EBADF)
                };
            };
            if op == SERVICE_NET_OP_CANCEL {
                if read.data.is_some() {
                    return Ok(0);
                }
                self.receiving[i] = None;
                self.reclaim();
                return Ok(1);
            }
            if op == SERVICE_NET_OP_FINISH {
                self.receiving[i] = None;
                self.reclaim();
                return Ok(0);
            }
            let data = read.data.ok_or(SERVICE_FILE_EINVAL)?;
            let offset = (arg & 0xffff) as usize;
            if offset > usize::from(data.len) {
                return Err(SERVICE_FILE_EINVAL);
            }
            let mut value = 0;
            for n in 0..SERVICE_NET_CHUNK.min(usize::from(data.len) - offset) {
                value |= u64::from(data.bytes[offset + n]) << (8 * n);
            }
            return Ok(value);
        }
        if op == SERVICE_NET_OP_OPEN {
            let handle = (0..SERVICE_NET_SOCKETS)
                .find(|h| !self.retained(*h))
                .ok_or(EAGAIN)?;
            self.sockets[handle] = Socket {
                holders: 1 << owner,
                ..Socket::EMPTY
            };
            return Ok(handle as u64);
        }
        if op != SERVICE_NET_OP_SENDTO_DATA
            && op != SERVICE_NET_OP_SENDTO_END
            && !self.owns(owner, handle)
        {
            return Err(SERVICE_FILE_EBADF);
        }
        match op {
            SERVICE_NET_OP_BIND => self.bind(handle, arg as u16).map(u64::from),
            SERVICE_NET_OP_GETSOCKNAME => Ok(u64::from(self.sockets[handle as usize].port)),
            SERVICE_NET_OP_CLOSE => {
                self.sockets[handle as usize].holders &= !(1 << owner);
                self.reclaim();
                Ok(0)
            }
            SERVICE_NET_OP_SENDTO_BEGIN => {
                let len = ((arg >> SERVICE_NET_LEN_SHIFT) & 0xff) as u8;
                let port = arg as u16;
                if self.sending[i].is_some() {
                    return Err(EAGAIN);
                }
                if len as usize > SERVICE_NET_DATAGRAM_MAX || port == 0 {
                    return Err(SERVICE_FILE_EINVAL);
                }
                if self.sockets[handle as usize].port == 0 {
                    self.bind(handle, 0)?;
                }
                self.sending[i] = Some(Sending {
                    cookie,
                    handle,
                    destination: port,
                    data: Datagram {
                        port: self.sockets[handle as usize].port,
                        len,
                        ..Datagram::EMPTY
                    },
                    got: 0,
                });
                Ok(0)
            }
            SERVICE_NET_OP_SENDTO_DATA => {
                let send = self.sending[i]
                    .as_mut()
                    .filter(|s| s.cookie == cookie && s.handle == handle)
                    .ok_or(SERVICE_FILE_EBADF)?;
                if send.got >= send.data.len {
                    return Err(SERVICE_FILE_EINVAL);
                }
                for n in 0..SERVICE_NET_CHUNK {
                    if send.got == send.data.len {
                        break;
                    }
                    send.data.bytes[send.got as usize] = (arg >> (8 * n)) as u8;
                    send.got += 1;
                }
                Ok(0)
            }
            SERVICE_NET_OP_SENDTO_END => {
                let send = self.sending[i]
                    .filter(|s| s.cookie == cookie && s.handle == handle)
                    .ok_or(SERVICE_FILE_EBADF)?;
                self.sending[i] = None;
                if send.got != send.data.len {
                    self.reclaim();
                    return Err(SERVICE_FILE_EINVAL);
                }
                let destination = self
                    .sockets
                    .iter()
                    .enumerate()
                    .position(|(h, s)| self.retained(h) && s.port == send.destination);
                if let Some(destination) = destination {
                    let socket = &mut self.sockets[destination];
                    if socket.count as usize == SERVICE_NET_QUEUE {
                        self.reclaim();
                        return Err(SERVICE_FILE_ENOSPC);
                    }
                    let tail = (socket.head as usize + socket.count as usize) % SERVICE_NET_QUEUE;
                    socket.queue[tail] = send.data;
                    socket.count += 1;
                }
                self.reclaim();
                Ok(0)
            }
            _ => Err(SERVICE_FILE_EINVAL),
        }
    }
    /// Manager supplies an authenticated Task cookie, Process cookie and reply
    /// cap. A valid C client has one short RPC plus one blocked receive per Task.
    pub fn request(&mut self, cookie: u64, process: u64, reply: Capability, payload: u64) {
        let i = cookie as u32 as usize;
        if i >= MAX_CLIENTS || cookie >> 32 == 0 {
            return;
        }
        if self.replies[i].is_some() {
            return;
        }
        let op = (payload >> SERVICE_NET_OP_SHIFT) & 15;
        let handle = ((payload >> SERVICE_NET_HANDLE_SHIFT) & 3) as u8;
        let sequence = (payload >> SERVICE_NET_SEQUENCE_SHIFT) & 255;
        let arg = payload & SERVICE_NET_ARG_MASK;
        let owner = self.ensure_owner(process);
        let result = if sequence == 0 {
            Err(SERVICE_FILE_EINVAL)
        } else if let Some(owner) = owner {
            if op == SERVICE_NET_OP_RECVFROM && self.owns(owner, handle) {
                if self.receiving[i].is_some() || self.next_read == u64::MAX {
                    Err(EAGAIN)
                } else {
                    self.receiving[i] = Some(Receiving {
                        cookie,
                        reply,
                        handle,
                        sequence: sequence as u8,
                        order: self.next_read,
                        data: None,
                    });
                    self.next_read += 1;
                    return;
                }
            } else {
                self.operation(cookie, owner, op, handle, arg)
            }
        } else {
            Err(EAGAIN)
        };
        let (status, data) = match result {
            Ok(data) => (0, data),
            Err(status) => (status, 0),
        };
        let reply_handle = if op == SERVICE_NET_OP_OPEN && status == 0 {
            data
        } else {
            u64::from(handle)
        };
        self.replies[i] = Some(Delivery {
            cookie,
            reply,
            receiving: false,
            payload: pack_net_reply(op, reply_handle, status, data)
                | (sequence << SERVICE_NET_SEQUENCE_SHIFT),
        });
    }
    pub fn next_delivery(&self) -> Option<Delivery> {
        self.next_delivery_excluding(0)
    }
    /// Skip Tasks whose mailbox is full for this pump pass. A full mailbox for
    /// one waiter must not prevent another Task's short RPC from completing.
    pub fn next_delivery_excluding(&self, skipped: u32) -> Option<Delivery> {
        if let Some(reply) = self
            .replies
            .iter()
            .flatten()
            .find(|r| skipped & (1 << (r.cookie as u32)) == 0)
        {
            return Some(*reply);
        }
        let read = self
            .receiving
            .iter()
            .flatten()
            .filter(|r| {
                r.data.is_none()
                    && self.sockets[r.handle as usize].count != 0
                    && skipped & (1 << (r.cookie as u32)) == 0
            })
            .min_by_key(|r| r.order)?;
        let socket = &self.sockets[read.handle as usize];
        let data = socket.queue[socket.head as usize];
        Some(Delivery {
            cookie: read.cookie,
            reply: read.reply,
            receiving: true,
            payload: pack_net_reply(
                SERVICE_NET_OP_RECVFROM,
                u64::from(read.handle),
                0,
                u64::from(data.port) | (u64::from(data.len) << SERVICE_NET_LEN_SHIFT),
            ) | (u64::from(read.sequence) << SERVICE_NET_SEQUENCE_SHIFT),
        })
    }
    pub fn delivered(&mut self, delivery: Delivery) {
        let i = delivery.cookie as u32 as usize;
        if !delivery.receiving {
            self.replies[i] = None;
            return;
        }
        let read = self.receiving[i].as_mut().expect("selected receive");
        let socket = &mut self.sockets[read.handle as usize];
        read.data = Some(socket.queue[socket.head as usize]);
        socket.head = ((socket.head as usize + 1) % SERVICE_NET_QUEUE) as u8;
        socket.count -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cap() -> Capability {
        Capability::from_raw((1 << 32) | 1).unwrap()
    }
    fn request(net: &mut Network, cookie: u64, process: u64, op: u64, handle: u8, arg: u64) -> u64 {
        net.request(
            cookie,
            process,
            cap(),
            SERVICE_NET_REQUEST_TAG
                | (op << SERVICE_NET_OP_SHIFT)
                | (u64::from(handle) << SERVICE_NET_HANDLE_SHIFT)
                | (1 << SERVICE_NET_SEQUENCE_SHIFT)
                | arg,
        );
        let reply = net.next_delivery().unwrap();
        let payload = reply.payload;
        net.delivered(reply);
        payload
    }
    fn status(payload: u64) -> u64 {
        (payload >> SERVICE_NET_STATUS_SHIFT) & 255
    }
    #[test]
    fn fork_holders_authenticate_generation_and_last_close_reclaims_endpoint() {
        let mut net = Network::new();
        let parent = (4 << 32) | 2;
        let child = (5 << 32) | 3;
        assert_eq!(
            status(request(&mut net, parent, parent, SERVICE_NET_OP_OPEN, 0, 0)),
            0
        );
        assert_eq!(
            status(request(&mut net, child, child, SERVICE_NET_OP_CLOSE, 0, 0)),
            SERVICE_FILE_EBADF
        );
        assert!(net.inherit(parent, child));
        assert_eq!(
            status(request(
                &mut net,
                parent,
                parent,
                SERVICE_NET_OP_CLOSE,
                0,
                0
            )),
            0
        );
        assert_eq!(
            status(request(
                &mut net,
                child,
                child,
                SERVICE_NET_OP_GETSOCKNAME,
                0,
                0
            )),
            0
        );
        net.release_owner(child);
        assert!(!net.retained(0));
        assert_eq!(
            status(request(
                &mut net,
                child + (1 << 32),
                child + (1 << 32),
                SERVICE_NET_OP_GETSOCKNAME,
                0,
                0
            )),
            SERVICE_FILE_EBADF
        );
    }
    #[test]
    fn separate_tasks_keep_send_staging_and_receive_snapshots_and_cancel_without_consuming() {
        let mut net = Network::new();
        let a = (4 << 32) | 2;
        let b = (5 << 32) | 3;
        request(&mut net, a, a, SERVICE_NET_OP_OPEN, 0, 0);
        request(&mut net, b, b, SERVICE_NET_OP_OPEN, 0, 0);
        request(&mut net, b, b, SERVICE_NET_OP_BIND, 1, 1234);
        net.request(
            b,
            b,
            cap(),
            SERVICE_NET_REQUEST_TAG
                | (SERVICE_NET_OP_RECVFROM << SERVICE_NET_OP_SHIFT)
                | (1 << SERVICE_NET_HANDLE_SHIFT)
                | (7 << SERVICE_NET_SEQUENCE_SHIFT),
        );
        assert!(net.next_delivery().is_none());
        assert_eq!(
            request(&mut net, b, b, SERVICE_NET_OP_CANCEL, 1, 7) & SERVICE_NET_ARG_MASK,
            1
        );
        request(
            &mut net,
            a,
            a,
            SERVICE_NET_OP_SENDTO_BEGIN,
            0,
            1234 | (3 << SERVICE_NET_LEN_SHIFT),
        );
        request(&mut net, a, a, SERVICE_NET_OP_SENDTO_DATA, 0, 0x636261);
        request(&mut net, a, a, SERVICE_NET_OP_SENDTO_END, 0, 0);
        assert_eq!(net.sockets[1].count, 1);
        net.request(
            b,
            b,
            cap(),
            SERVICE_NET_REQUEST_TAG
                | (SERVICE_NET_OP_RECVFROM << SERVICE_NET_OP_SHIFT)
                | (1 << SERVICE_NET_HANDLE_SHIFT)
                | (8 << SERVICE_NET_SEQUENCE_SHIFT),
        );
        let reply = net.next_delivery().unwrap();
        assert_eq!(net.sockets[1].count, 1); // A failed send hasn't consumed it.
        net.delivered(reply);
        assert_eq!(net.sockets[1].count, 0);
        assert_eq!(
            request(&mut net, b, b, SERVICE_NET_OP_CANCEL, 1, 8) & SERVICE_NET_ARG_MASK,
            0
        );
        assert_eq!(
            request(&mut net, b, b, SERVICE_NET_OP_RECV_CHUNK, 1, 8 << 16) & SERVICE_NET_ARG_MASK,
            0x636261
        );
        request(&mut net, b, b, SERVICE_NET_OP_FINISH, 1, 8);
        assert!(net.receiving[3].is_none());
    }
    #[test]
    fn interleaved_workers_keep_private_staging_reply_routes_and_snapshots_after_close() {
        let mut net = Network::new();
        let a = (4 << 32) | 2;
        let b = (5 << 32) | 3;
        let worker_a = (6 << 32) | 4;
        let worker_b = (7 << 32) | 5;
        request(&mut net, a, a, SERVICE_NET_OP_OPEN, 0, 0);
        request(&mut net, b, b, SERVICE_NET_OP_OPEN, 0, 0);
        request(&mut net, b, b, SERVICE_NET_OP_BIND, 1, 1234);
        for sender in [a, worker_a] {
            request(
                &mut net,
                sender,
                a,
                SERVICE_NET_OP_SENDTO_BEGIN,
                0,
                1234 | (3 << SERVICE_NET_LEN_SHIFT),
            );
        }
        request(&mut net, a, a, SERVICE_NET_OP_SENDTO_DATA, 0, 0x636261);
        request(
            &mut net,
            worker_a,
            a,
            SERVICE_NET_OP_SENDTO_DATA,
            0,
            0x666564,
        );
        request(&mut net, a, a, SERVICE_NET_OP_SENDTO_END, 0, 0);
        request(&mut net, worker_a, a, SERVICE_NET_OP_SENDTO_END, 0, 0);
        let worker_reply = Capability::from_raw((2 << 32) | 8).unwrap();
        net.request(
            b,
            b,
            cap(),
            pack_net_request(SERVICE_NET_OP_RECVFROM, 1, 7, 0),
        );
        net.request(
            worker_b,
            b,
            worker_reply,
            pack_net_request(SERVICE_NET_OP_RECVFROM, 1, 8, 0),
        );
        let first = net.next_delivery().unwrap();
        assert_eq!(first.cookie, b);
        assert_eq!(first.reply, cap());
        net.delivered(first);
        let second = net.next_delivery().unwrap();
        assert_eq!(second.cookie, worker_b);
        assert_eq!(second.reply, worker_reply);
        net.delivered(second);
        request(&mut net, b, b, SERVICE_NET_OP_CLOSE, 1, 0);
        assert!(net.retained(1)); // The outstanding receives own the object.
        assert_eq!(
            request(&mut net, b, b, SERVICE_NET_OP_RECV_CHUNK, 1, 7 << 16) & SERVICE_NET_ARG_MASK,
            0x636261
        );
        assert_eq!(
            status(request(
                &mut net,
                worker_b,
                b,
                SERVICE_NET_OP_RECV_CHUNK,
                1,
                7 << 16
            )),
            SERVICE_FILE_EBADF
        );
        assert_eq!(
            request(&mut net, worker_b, b, SERVICE_NET_OP_RECV_CHUNK, 1, 8 << 16)
                & SERVICE_NET_ARG_MASK,
            0x666564
        );
        net.drop_client(b);
        assert!(net.retained(1));
        request(&mut net, worker_b, b, SERVICE_NET_OP_FINISH, 1, 8);
        assert!(!net.retained(1));
    }
}
