//! Linux personality's file-store client. Every VFS syscall becomes one bounded
//! batch submitted through a registered proxy session; the frontend keeps no
//! file content or store handles of its own.
//!
//! `Store` is the transport seam. `Remote` is the manager-issued proxy session
//! (P2b-4/P2b-5); `Local` is the in-process store used by host tests and the
//! legacy differential. `Direct` keeps the plain direct-session client, which
//! the P2b-5 scenario no longer selects in favor of a policy view.

use prima_runtime::{
    Capability, Rights, Runtime, SyscallTransport, TaskId,
    bulk::{self, MAX_PATH, NEUTRAL_STAT_BYTES, PAGE_BYTES, Request, WIRE_FD_BASE},
    protocol::{
        SERVICE_FILE_HANDLES, SERVICE_SESSION_ACK_TAG, SERVICE_SESSION_FORK_ACK_TAG,
        SERVICE_SESSION_PROXY_CLOSE_TAG, SERVICE_SESSION_PROXY_REGISTER_TAG,
        SERVICE_SESSION_REGISTER_TAG, SERVICE_SESSION_ROUTE_TAG,
    },
    vfs::{self, OpenFiles, Storage, VfsError, View},
};

/// One store session transport. `page` carries a validated batch whose header
/// sequence already equals `sequence`; on success the completions are written
/// back in place. Errors are negative errno values.
///
/// Lifecycle methods are proxy-scoped: the transport already knows which
/// registered application entry it submits for, so `owner` is only the acting
/// Process's store owner. A real session is owner-bound and validates it.
pub trait Store {
    fn exchange(
        &mut self,
        owner: u8,
        page: &mut [u8; PAGE_BYTES],
        sequence: u64,
    ) -> Result<(), i64>;
    /// Reserve a child owner inheriting the acting owner's OFDs.
    fn reserve(&mut self, owner: u8) -> Result<u32, i64>;
    /// Bind a reservation to the published child Process cookie and return the
    /// child owner. The reservation belongs to the acting owner's entry. A real
    /// store authenticates `child` by the presented TaskSend capability; the
    /// kernel records no process parent for a managed gate fork.
    fn publish(&mut self, owner: u8, key: u32, child: u64, task: Capability) -> Result<u8, i64>;
    /// Drop a reservation that will not be published.
    fn release(&mut self, owner: u8, key: u32) -> Result<(), i64>;
    /// Close every OFD held by `owner` (process exit).
    fn close_owner(&mut self, owner: u8) -> Result<(), i64>;
    /// Bind a freshly published child Process to its own proxy entry. In-process
    /// storage keys batches by owner, so the default is a no-op.
    fn register_child(&mut self, owner: u8, task: Capability) -> Result<(), i64> {
        let _ = (owner, task);
        Ok(())
    }
}

/// Next accepted session sequence and the acting Process's store owner. A
/// store session is terminal after a lost completion, so the sequence only
/// advances on a successful exchange.
pub struct Session {
    owner: u8,
    next: u64,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    pub const fn new() -> Self {
        Self { owner: 0, next: 1 }
    }

    pub fn set_owner(&mut self, owner: u8) {
        self.owner = owner;
    }

    /// Next accepted sequence; the last accepted one is `next() - 1`.
    pub fn next(&self) -> u64 {
        self.next
    }

    pub fn reserve(&mut self, store: &mut dyn Store) -> Result<u32, i64> {
        store.reserve(self.owner)
    }

    pub fn publish(
        &mut self,
        store: &mut dyn Store,
        key: u32,
        child: u64,
        task: Capability,
    ) -> Result<u8, i64> {
        store.publish(self.owner, key, child, task)
    }

    pub fn release(&mut self, store: &mut dyn Store, key: u32) -> Result<(), i64> {
        store.release(self.owner, key)
    }

    pub fn close_owner(&mut self, store: &mut dyn Store) -> Result<(), i64> {
        store.close_owner(self.owner)
    }

    pub fn register_child(
        &mut self,
        store: &mut dyn Store,
        owner: u8,
        task: Capability,
    ) -> Result<(), i64> {
        store.register_child(owner, task)
    }

    fn submit(
        &mut self,
        store: &mut dyn Store,
        page: &mut [u8; PAGE_BYTES],
    ) -> Result<(u32, u32), i64> {
        if self.next > bulk::MAX_SEQUENCE {
            return Err(-5);
        }
        let sequence = self.next;
        store.exchange(self.owner, page, sequence)?;
        self.next += 1;
        let at = bulk::HEADER_BYTES;
        Ok((
            u32::from_le_bytes(page[at + 24..at + 28].try_into().unwrap()),
            u32::from_le_bytes(page[at + 28..at + 32].try_into().unwrap()),
        ))
    }

    fn one(
        &mut self,
        store: &mut dyn Store,
        request: Request,
    ) -> Result<([u8; PAGE_BYTES], u32), i64> {
        let sequence = self.next;
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, sequence, 1);
        bulk::encode_request(&mut page, 0, request);
        let (status, count) = self.submit(store, &mut page)?;
        if status != 0 {
            return Err(-i64::from(status));
        }
        Ok((page, count))
    }

    /// Path-carrying requests need their bytes before submission.
    fn path(
        &mut self,
        store: &mut dyn Store,
        mut request: Request,
        bytes: &[u8],
    ) -> Result<([u8; PAGE_BYTES], u32), i64> {
        if bytes.is_empty() {
            return Err(-22);
        }
        if bytes.len() > MAX_PATH {
            return Err(-36);
        }
        request.length = bytes.len();
        request.data = bulk::DATA_OFFSET;
        let sequence = self.next;
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, sequence, 1);
        bulk::encode_request(&mut page, 0, request);
        page[bulk::DATA_OFFSET..bulk::DATA_OFFSET + bytes.len()].copy_from_slice(bytes);
        let (status, count) = self.submit(store, &mut page)?;
        if status != 0 {
            return Err(-i64::from(status));
        }
        Ok((page, count))
    }

    pub fn open(
        &mut self,
        store: &mut dyn Store,
        dirfd: u8,
        path: &[u8],
        flags: u64,
    ) -> Result<u8, i64> {
        let (_, fd) = self.path(
            store,
            Request {
                op: Request::OPEN,
                fd: dirfd,
                offset: flags,
                ..Request::default()
            },
            path,
        )?;
        let wire = u8::try_from(fd).map_err(|_| -5)?;
        if wire < WIRE_FD_BASE
            || usize::from(wire) >= usize::from(WIRE_FD_BASE) + SERVICE_FILE_HANDLES
        {
            return Err(-5);
        }
        Ok(wire)
    }

    pub fn read(&mut self, store: &mut dyn Store, fd: u8, out: &mut [u8]) -> Result<usize, i64> {
        if out.is_empty() {
            return Ok(0);
        }
        if out.len() > bulk::DATA_BYTES {
            return Err(-22);
        }
        let (page, count) = self.one(
            store,
            Request {
                op: Request::READ,
                fd,
                length: out.len(),
                data: bulk::DATA_OFFSET,
                ..Request::default()
            },
        )?;
        let count = (count as usize).min(out.len());
        out[..count].copy_from_slice(&page[bulk::DATA_OFFSET..bulk::DATA_OFFSET + count]);
        Ok(count)
    }

    pub fn write(&mut self, store: &mut dyn Store, fd: u8, bytes: &[u8]) -> Result<usize, i64> {
        if bytes.is_empty() {
            return Ok(0);
        }
        if bytes.len() > bulk::DATA_BYTES {
            return Err(-22);
        }
        let request = Request {
            op: Request::WRITE,
            fd,
            length: bytes.len(),
            data: bulk::DATA_OFFSET,
            ..Request::default()
        };
        let sequence = self.next;
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, sequence, 1);
        bulk::encode_request(&mut page, 0, request);
        page[bulk::DATA_OFFSET..bulk::DATA_OFFSET + bytes.len()].copy_from_slice(bytes);
        let (status, count) = self.submit(store, &mut page)?;
        if status != 0 {
            return Err(-i64::from(status));
        }
        Ok((count as usize).min(bytes.len()))
    }

    pub fn stat(&mut self, store: &mut dyn Store, fd: u8) -> Result<[u8; NEUTRAL_STAT_BYTES], i64> {
        let (page, count) = self.one(
            store,
            Request {
                op: Request::STAT,
                fd,
                length: NEUTRAL_STAT_BYTES,
                data: bulk::DATA_OFFSET,
                ..Request::default()
            },
        )?;
        if count as usize != NEUTRAL_STAT_BYTES {
            return Err(-5);
        }
        Ok(
            page[bulk::DATA_OFFSET..bulk::DATA_OFFSET + NEUTRAL_STAT_BYTES]
                .try_into()
                .unwrap(),
        )
    }

    pub fn stat_path(
        &mut self,
        store: &mut dyn Store,
        dirfd: u8,
        path: &[u8],
        nofollow: bool,
    ) -> Result<[u8; NEUTRAL_STAT_BYTES], i64> {
        let (page, count) = self.path(
            store,
            Request {
                op: Request::STAT_PATH,
                fd: dirfd,
                offset: if nofollow { bulk::STAT_NOFOLLOW } else { 0 },
                ..Request::default()
            },
            path,
        )?;
        if count as usize != NEUTRAL_STAT_BYTES {
            return Err(-5);
        }
        let at = bulk::DATA_OFFSET + MAX_PATH;
        Ok(page[at..at + NEUTRAL_STAT_BYTES].try_into().unwrap())
    }

    /// Returns neutral directory records; whole-record framing is the store's
    /// contract, so the caller only re-encodes and possibly rewinds.
    pub fn getdents(
        &mut self,
        store: &mut dyn Store,
        fd: u8,
        out: &mut [u8],
    ) -> Result<usize, i64> {
        if out.is_empty() || out.len() > bulk::DATA_BYTES {
            return Err(-22);
        }
        let (page, count) = self.one(
            store,
            Request {
                op: Request::GETDENTS,
                fd,
                length: out.len(),
                data: bulk::DATA_OFFSET,
                ..Request::default()
            },
        )?;
        let count = (count as usize).min(out.len());
        out[..count].copy_from_slice(&page[bulk::DATA_OFFSET..bulk::DATA_OFFSET + count]);
        Ok(count)
    }

    pub fn lseek(
        &mut self,
        store: &mut dyn Store,
        fd: u8,
        offset: i64,
        whence: u64,
    ) -> Result<u64, i64> {
        if whence > 2 {
            return Err(-22);
        }
        let (_, next) = self.one(
            store,
            Request {
                op: Request::LSEEK,
                fd,
                offset: offset as u64,
                length: whence as usize,
                data: bulk::DATA_OFFSET,
            },
        )?;
        Ok(u64::from(next))
    }

    pub fn close(&mut self, store: &mut dyn Store, fd: u8) -> Result<(), i64> {
        self.one(
            store,
            Request {
                op: Request::CLOSE,
                fd,
                data: bulk::DATA_OFFSET,
                ..Request::default()
            },
        )
        .map(|_| ())
    }
}

fn status(error: VfsError) -> u32 {
    match error {
        VfsError::NoEnt => 2,
        VfsError::NotDir => 20,
        VfsError::Access => 13,
        VfsError::Exist => 17,
        VfsError::Invalid => 22,
        VfsError::NoSpace => 28,
        VfsError::Unformatted | VfsError::Io => 5,
        VfsError::Loop => 40,
        VfsError::BadHandle => 9,
        VfsError::TooManyOpen => 24,
    }
}

const LOCAL_OWNERS: usize = 32;

#[derive(Clone, Copy)]
struct Slot {
    key: u32,
    published: bool,
    next: u64,
    child: u64,
}

/// In-process store used until the manager-issued session transport is wired.
/// It executes the same codec and OFD semantics as the store service, including
/// owner inheritance, reservation keys and exit close.
pub struct Local {
    ram: Ram,
    files: OpenFiles<SERVICE_FILE_HANDLES>,
    slots: [Option<Slot>; LOCAL_OWNERS],
    next_key: u32,
}

impl Local {
    pub fn new() -> Result<Self, i64> {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).map_err(|_| -5)?;
        // The common VFS starts read-only; seed the fixture, then restore the
        // Linux personality's read-only root policy.
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).map_err(|_| -5)?;
        vfs::mkdir(&mut ram, vfs::ROOT, b"/fixturedir").map_err(|_| -5)?;
        for (path, bytes) in [
            (b"/fixture.txt".as_slice(), b"hello from prima\n".as_slice()),
            (b"/fixturedir/alpha".as_slice(), b"a\n".as_slice()),
            (b"/fixturedir/beta".as_slice(), b"b\n".as_slice()),
        ] {
            let opened = vfs::open(
                &mut ram,
                vfs::ROOT,
                path,
                vfs::OpenFlags {
                    create: true,
                    access: vfs::AccessMode::ReadWrite,
                    ..Default::default()
                },
            )
            .map_err(|_| -5)?;
            let mut node = opened.node;
            vfs::write(&mut ram, opened.id, &mut node, 0, bytes).map_err(|_| -5)?;
        }
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o555).map_err(|_| -5)?;
        let mut slots = [None; LOCAL_OWNERS];
        slots[0] = Some(Slot {
            key: 0,
            published: true,
            next: 1,
            child: 0,
        });
        Ok(Self {
            ram,
            files: OpenFiles::new(),
            slots,
            next_key: 1,
        })
    }

    fn slot(&self, owner: u8) -> Result<Slot, i64> {
        self.slots.get(owner as usize).copied().flatten().ok_or(-13)
    }
}

impl Store for Local {
    fn exchange(
        &mut self,
        owner: u8,
        page: &mut [u8; PAGE_BYTES],
        sequence: u64,
    ) -> Result<(), i64> {
        let slot = self.slot(owner)?;
        if !slot.published {
            return Err(-13);
        }
        if sequence == 0 || sequence != slot.next {
            return Err(-22);
        }
        self.files.set_owner(owner).map_err(|_| -22)?;
        match prima_user_store_execute(&mut self.files, &mut self.ram, page, sequence) {
            Err(_) => Err(-22),
            Ok(()) => {
                self.slots[owner as usize].as_mut().unwrap().next += 1;
                Ok(())
            }
        }
    }

    fn reserve(&mut self, owner: u8) -> Result<u32, i64> {
        self.slot(owner)?;
        let child = (1..LOCAL_OWNERS)
            .find(|slot| self.slots[*slot].is_none())
            .ok_or(-11)? as u8;
        let key = self.next_key;
        let next_key = key.checked_add(1).ok_or(-11)?;
        self.files.fork_owner(owner, child).map_err(|_| -22)?;
        self.slots[child as usize] = Some(Slot {
            key,
            published: false,
            next: 1,
            child: 0,
        });
        self.next_key = next_key;
        Ok(key)
    }

    fn publish(&mut self, owner: u8, key: u32, child: u64, task: Capability) -> Result<u8, i64> {
        let _ = task;
        self.slot(owner)?;
        if child >> 32 == 0 {
            return Err(-13);
        }
        let index = self
            .slots
            .iter()
            .position(|slot| slot.is_some_and(|slot| slot.key == key))
            .ok_or(-13)?;
        let slot = self.slots[index].as_mut().unwrap();
        if slot.published {
            // Republishing the same child is idempotent; another child is not.
            return if slot.child == child {
                Ok(index as u8)
            } else {
                Err(-13)
            };
        }
        slot.published = true;
        slot.child = child;
        Ok(index as u8)
    }

    fn release(&mut self, owner: u8, key: u32) -> Result<(), i64> {
        self.slot(owner)?;
        let Some(index) = self
            .slots
            .iter()
            .position(|slot| slot.is_some_and(|slot| slot.key == key))
        else {
            return Ok(()); // already reclaimed
        };
        if self.slots[index].unwrap().published {
            return Err(-13);
        }
        self.files
            .close_owner(&mut self.ram, index as u8)
            .map_err(|_| -5)?;
        self.slots[index] = None;
        Ok(())
    }

    fn close_owner(&mut self, owner: u8) -> Result<(), i64> {
        self.slot(owner)?;
        self.files
            .close_owner(&mut self.ram, owner)
            .map_err(|_| -5)?;
        if owner != 0 {
            self.slots[owner as usize] = None;
        }
        Ok(())
    }
}

/// Fixed VA for proxy slot `index`'s shared page. The personality maps one page
/// per registered proxy entry for the lifetime of that entry.
pub const REMOTE_PAGE_VA: u64 = 0x0000_0d00_0000_0000;

/// One entry per Linux Process (`MAX_PROCESSES`); a forked child registers its
/// own proxy slot after the owner is published.
const REMOTE_OWNERS: usize = 4;
const TASK_SEND: Rights = Rights::from_bits(1 << 5);
const TASK_SEND_DERIVE: Rights = Rights::from_bits(1 | (1 << 5));
const SEGMENT_RW: Rights = Rights::from_bits((1 << 1) | (1 << 2));

#[derive(Clone, Copy)]
struct RemoteEntry {
    owner: u8,
    slot: u8,
}

/// Manager-issued proxy transport. Every batch goes through the Store service:
/// the proxy slot, its shared page and the acting owner all come from the
/// manager- and store-validated registration, never from the batch payload.
pub struct Remote<'a, T: SyscallTransport> {
    rt: &'a Runtime<T>,
    self_cap: Capability,
    manager: (u64, Capability),
    store: Option<(u64, Capability)>,
    policy: u8,
    pages: [Option<Capability>; crate::proxy::MAX_PROXIES],
    entries: [Option<RemoteEntry>; REMOTE_OWNERS],
}

fn sender_cookie(message: &prima_runtime::Message) -> u64 {
    message.sender.map_or(0, |task| {
        u64::from(task.as_raw()) | (u64::from(message.sender_generation) << 32)
    })
}

fn drop_caps<T: SyscallTransport>(rt: &Runtime<T>, message: &prima_runtime::Message) {
    for cap in message.capabilities.iter().flatten() {
        let _ = rt.cap_drop(*cap);
    }
}

/// Register a direct file session with the manager and wait for the store's
/// announcement. Shared by the proxy transport and the native direct client.
fn open_session<T: SyscallTransport>(
    rt: &Runtime<T>,
    manager: Capability,
) -> Result<(Capability, u64, Capability), i64> {
    let self_cap = rt.self_capability().map_err(|_| -13)?;
    let manager_cookie = rt.send_target_cookie(manager).map_err(|_| -13)?;
    let reply = rt.cap_derive(self_cap, TASK_SEND_DERIVE).map_err(|_| -13)?;
    let control = rt.cap_derive(self_cap, TASK_SEND).map_err(|_| -13)?;
    rt.send(
        TaskId::from_raw(manager_cookie as u32),
        manager,
        SERVICE_SESSION_REGISTER_TAG,
        &[reply, control],
    )
    .map_err(|_| -13)?;
    let store = accept_store(rt, manager_cookie)?;
    Ok((self_cap, store.0, store.1))
}

/// The manager announces the store route, then the store ACKs with the one
/// send capability whose target is its own live incarnation.
fn accept_store<T: SyscallTransport>(
    rt: &Runtime<T>,
    manager_cookie: u64,
) -> Result<(u64, Capability), i64> {
    let mut expected = 0u64;
    loop {
        let message = rt.receive().map_err(|_| -5)?;
        let cookie = sender_cookie(&message);
        let tag = message.payload & 0xffff_0000_0000_0000;
        if tag == SERVICE_SESSION_ROUTE_TAG && cookie == manager_cookie {
            expected = (message.payload & 0xff) | (((message.payload >> 8) & 0xffff_ffff) << 32);
            continue;
        }
        if tag == SERVICE_SESSION_ACK_TAG
            && let Some(cap) = message.capabilities[0]
        {
            let target = rt.send_target_cookie(cap).ok();
            let status = message.payload & 0xff;
            if status == 0
                && expected != 0
                && target == Some(cookie)
                && message.capabilities[1].is_none()
            {
                return Ok((cookie, cap));
            }
            for cap in message.capabilities.iter().flatten() {
                let _ = rt.cap_drop(*cap);
            }
            continue;
        }
        drop_caps(rt, &message);
    }
}

impl<'a, T: SyscallTransport> Remote<'a, T> {
    /// Register a direct file session with the manager, then keep the store's
    /// send authority. Proxy registration for each application follows.
    pub fn connect(rt: &'a Runtime<T>, manager: Capability, policy: u8) -> Result<Self, i64> {
        let (self_cap, store_cookie, store_cap) = open_session(rt, manager)?;
        let manager_cookie = rt.send_target_cookie(manager).map_err(|_| -13)?;
        Ok(Self {
            rt,
            self_cap,
            manager: (manager_cookie, manager),
            store: Some((store_cookie, store_cap)),
            policy,
            pages: [None; crate::proxy::MAX_PROXIES],
            entries: [None; REMOTE_OWNERS],
        })
    }

    /// Ask the manager to bind a live application Task to this proxy. The
    /// application's own TaskSend is the only subject authority in the message.
    fn register_proxy(&mut self, app: Capability) -> Result<u8, i64> {
        let (store_cookie, _) = self.store.ok_or(-13)?;
        let subject = self.rt.cap_derive(app, TASK_SEND).map_err(|_| -13)?;
        let reply = self
            .rt
            .cap_derive(self.self_cap, TASK_SEND)
            .map_err(|_| -13)?;
        let (manager_cookie, manager) = self.manager;
        self.rt
            .send(
                TaskId::from_raw(manager_cookie as u32),
                manager,
                SERVICE_SESSION_PROXY_REGISTER_TAG | u64::from(self.policy),
                &[subject, reply],
            )
            .map_err(|_| -13)?;
        loop {
            let message = self.rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != SERVICE_SESSION_ACK_TAG
            {
                drop_caps(self.rt, &message);
                continue;
            }
            let status = message.payload & 0xff;
            let index = ((message.payload >> 8) & 0xff) as usize;
            drop_caps(self.rt, &message);
            if status != 0 || index >= crate::proxy::MAX_PROXIES {
                return Err(-13);
            }
            return Ok(index as u8);
        }
    }

    fn page_va(index: u8) -> u64 {
        REMOTE_PAGE_VA + u64::from(index) * PAGE_BYTES as u64
    }

    /// Map the store-owned page. Its root capability never leaves the store.
    fn open_slot(&mut self, index: u8) -> Result<(), i64> {
        let (store_cookie, store) = self.store.ok_or(-13)?;
        self.rt
            .send(
                TaskId::from_raw(store_cookie as u32),
                store,
                bulk::PROXY_OPEN | u64::from(index),
                &[],
            )
            .map_err(|_| -13)?;
        loop {
            let message = self.rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != bulk::OPEN_REPLY
            {
                drop_caps(self.rt, &message);
                continue;
            }
            let status = (message.payload >> 32) & 0xffff;
            let Some(cap) = message.capabilities[0] else {
                drop_caps(self.rt, &message);
                if status != 0 {
                    return Err(-(status as i64));
                }
                continue;
            };
            for extra in message.capabilities[1..].iter().flatten() {
                let _ = self.rt.cap_drop(*extra);
            }
            if status != 0 {
                let _ = self.rt.cap_drop(cap);
                return Err(-(status as i64));
            }
            if self
                .rt
                .memseg_map(cap, self.self_cap, 0, Self::page_va(index), 1, SEGMENT_RW)
                .is_err()
            {
                let _ = self.rt.cap_drop(cap);
                return Err(-12);
            }
            self.pages[index as usize] = Some(cap);
            return Ok(());
        }
    }

    fn entry(&self, owner: u8) -> Result<RemoteEntry, i64> {
        self.entries
            .iter()
            .flatten()
            .find(|entry| entry.owner == owner)
            .copied()
            .ok_or(-9)
    }

    fn send_control(&mut self, payload: u64) -> Result<u64, i64> {
        self.send_control_caps(payload, &[])
    }

    fn send_control_caps(&mut self, payload: u64, caps: &[Capability]) -> Result<u64, i64> {
        let (store_cookie, store) = self.store.ok_or(-13)?;
        self.rt
            .send(TaskId::from_raw(store_cookie as u32), store, payload, caps)
            .map_err(|_| -13)?;
        loop {
            let message = self.rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != SERVICE_SESSION_FORK_ACK_TAG
            {
                drop_caps(self.rt, &message);
                continue;
            }
            drop_caps(self.rt, &message);
            return Ok(message.payload & 0xffff_ffff);
        }
    }

    /// Proxy slot bound to a local owner tag.
    pub fn slot(&self, owner: u8) -> Result<u8, i64> {
        self.entry(owner).map(|entry| entry.slot)
    }

    /// Send one raw proxy control frame and return the store status word.
    /// Only the negative matrix uses this: `PROXY_SUBMIT` replays and
    /// unregistered slots must be rejected by the store, not by the client.
    pub fn probe(&mut self, owner: u8, payload: u64) -> Result<u64, i64> {
        self.entry(owner)?;
        let (store_cookie, store) = self.store.ok_or(-13)?;
        self.rt
            .send(TaskId::from_raw(store_cookie as u32), store, payload, &[])
            .map_err(|_| -13)?;
        loop {
            let message = self.rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != bulk::COMPLETE
            {
                drop_caps(self.rt, &message);
                continue;
            }
            let status = (message.payload >> 32) & 0xffff;
            drop_caps(self.rt, &message);
            return Ok(status);
        }
    }

    /// Register a forked child against its published owner. The reservation was
    /// published on the parent's entry, so the store reuses the inherited OFDs.
    pub fn register_owner(&mut self, owner: u8, task: Capability) -> Result<(), i64> {
        let index = self.register_proxy(task)?;
        if let Err(error) = self.open_slot(index) {
            let (store_cookie, store) = self.store.ok_or(-13)?;
            let _ = self.rt.send(
                TaskId::from_raw(store_cookie as u32),
                store,
                SERVICE_SESSION_PROXY_CLOSE_TAG | u64::from(index),
                &[],
            );
            return Err(error);
        }
        let slot = self
            .entries
            .iter_mut()
            .find(|entry| entry.is_none())
            .ok_or(-11)?;
        *slot = Some(RemoteEntry { owner, slot: index });
        Ok(())
    }
}

impl<T: SyscallTransport> Store for Remote<'_, T> {
    fn exchange(
        &mut self,
        owner: u8,
        page: &mut [u8; PAGE_BYTES],
        sequence: u64,
    ) -> Result<(), i64> {
        let entry = self.entry(owner)?;
        let va = Self::page_va(entry.slot);
        self.rt.write_user_bytes(va, page).map_err(|_| -14)?;
        let (store_cookie, store) = self.store.ok_or(-13)?;
        let payload =
            bulk::PROXY_SUBMIT | (u64::from(entry.slot) << bulk::PROXY_SLOT_SHIFT) | sequence;
        self.rt
            .send(TaskId::from_raw(store_cookie as u32), store, payload, &[])
            .map_err(|_| -13)?;
        loop {
            let message = self.rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != bulk::COMPLETE
            {
                drop_caps(self.rt, &message);
                continue;
            }
            let status = (message.payload >> 32) & 0xffff;
            let echo = message.payload & 0xffff_ffff;
            drop_caps(self.rt, &message);
            if echo != sequence {
                return Err(-22);
            }
            if status != 0 {
                return Err(-(status as i64));
            }
            self.rt.read_user_bytes(va, page).map_err(|_| -14)?;
            return Ok(());
        }
    }

    fn reserve(&mut self, owner: u8) -> Result<u32, i64> {
        let entry = self.entry(owner)?;
        let payload = bulk::PROXY_RESERVE | (u64::from(entry.slot) << bulk::PROXY_SLOT_SHIFT);
        let key = self.send_control(payload)?;
        if key == 0 {
            return Err(-11);
        }
        Ok(key as u32)
    }

    fn publish(&mut self, owner: u8, key: u32, child: u64, task: Capability) -> Result<u8, i64> {
        let entry = self.entry(owner)?;
        let payload = bulk::PROXY_PUBLISH
            | (u64::from(entry.slot) << bulk::PROXY_SLOT_SHIFT)
            | ((child as u32 & 0xff) as u64) << bulk::PROXY_CHILD_SHIFT
            | u64::from(key);
        // The child TaskSend capability is the authenticated binding; the
        // reservation key alone never names an arbitrary task.
        let subject = self.rt.cap_derive(task, TASK_SEND).map_err(|_| -13)?;
        let reply = match self.send_control_caps(payload, &[subject]) {
            Ok(reply) => reply,
            Err(error) => {
                let _ = self.rt.cap_drop(subject);
                return Err(error);
            }
        };
        if reply & 0xff != 0 {
            return Err(-13);
        }
        Ok(((reply >> 8) & 0xff) as u8)
    }

    fn release(&mut self, owner: u8, key: u32) -> Result<(), i64> {
        let entry = self.entry(owner)?;
        let payload = bulk::PROXY_RELEASE
            | (u64::from(entry.slot) << bulk::PROXY_SLOT_SHIFT)
            | u64::from(key);
        if self.send_control(payload)? != 0 {
            return Err(-13);
        }
        Ok(())
    }

    fn close_owner(&mut self, owner: u8) -> Result<(), i64> {
        let entry = self.entry(owner)?;
        let (store_cookie, store) = self.store.ok_or(-13)?;
        let _ = self.rt.send(
            TaskId::from_raw(store_cookie as u32),
            store,
            SERVICE_SESSION_PROXY_CLOSE_TAG | u64::from(entry.slot),
            &[],
        );
        loop {
            let message = self.rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != SERVICE_SESSION_FORK_ACK_TAG
            {
                drop_caps(self.rt, &message);
                continue;
            }
            let status = message.payload & 0xff;
            drop_caps(self.rt, &message);
            if status != 0 {
                return Err(-(status as i64));
            }
            if let Some(cap) = self.pages[entry.slot as usize].take() {
                let _ = self
                    .rt
                    .memseg_unmap(self.self_cap, Self::page_va(entry.slot), 1);
                let _ = self.rt.cap_drop(cap);
            }
            if let Some(index) = self
                .entries
                .iter()
                .position(|candidate| candidate.is_some_and(|e| e.owner == owner))
            {
                self.entries[index] = None;
            }
            return Ok(());
        }
    }

    fn register_child(&mut self, owner: u8, task: Capability) -> Result<(), i64> {
        self.register_owner(owner, task)
    }
}

/// Fixed VA for the direct client's single batch page.
pub const DIRECT_PAGE_VA: u64 = 0x0000_0e00_0000_0000;

/// Plain direct-session client. Its batches name no proxy slot; the store binds
/// the owner from the registering Process. Kept as the legacy transport: P2b-5
/// moved the native scenario onto a policy view so private roots stay isolated.
pub struct Direct<'a, T: SyscallTransport> {
    rt: &'a Runtime<T>,
    self_cap: Capability,
    store: (u64, Capability),
    page: Capability,
    next: u64,
}

impl<'a, T: SyscallTransport> Direct<'a, T> {
    pub fn connect(rt: &'a Runtime<T>, manager: Capability) -> Result<Self, i64> {
        let (self_cap, store_cookie, store_cap) = open_session(rt, manager)?;
        rt.send(
            TaskId::from_raw(store_cookie as u32),
            store_cap,
            bulk::OPEN,
            &[],
        )
        .map_err(|_| -13)?;
        loop {
            let message = rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != bulk::OPEN_REPLY
            {
                drop_caps(rt, &message);
                continue;
            }
            let status = (message.payload >> 32) & 0xffff;
            let Some(page) = message.capabilities[0] else {
                drop_caps(rt, &message);
                continue;
            };
            let extra = message.capabilities[1];
            for cap in extra.iter() {
                let _ = rt.cap_drop(*cap);
            }
            if status != 0 {
                let _ = rt.cap_drop(page);
                return Err(-(status as i64));
            }
            if extra.is_some() {
                let _ = rt.cap_drop(page);
                return Err(-5);
            }
            if rt
                .memseg_map(page, self_cap, 0, DIRECT_PAGE_VA, 1, SEGMENT_RW)
                .is_err()
            {
                let _ = rt.cap_drop(page);
                return Err(-12);
            }
            return Ok(Self {
                rt,
                self_cap,
                store: (store_cookie, store_cap),
                page,
                next: message.payload as u32 as u64,
            });
        }
    }

    /// One batch, one COMPLETE. Output stays in the shared page for the caller.
    fn submit(&mut self, page: &mut [u8; PAGE_BYTES]) -> Result<(u32, u32), i64> {
        if self.next == 0 || self.next > bulk::MAX_SEQUENCE {
            return Err(-5);
        }
        let (store_cookie, store) = self.store;
        self.rt
            .write_user_bytes(DIRECT_PAGE_VA, page)
            .map_err(|_| -14)?;
        self.rt
            .send(
                TaskId::from_raw(store_cookie as u32),
                store,
                bulk::SUBMIT | self.next,
                &[],
            )
            .map_err(|_| -13)?;
        loop {
            let message = self.rt.receive().map_err(|_| -5)?;
            if sender_cookie(&message) != store_cookie
                || message.payload & 0xffff_0000_0000_0000 != bulk::COMPLETE
            {
                drop_caps(self.rt, &message);
                continue;
            }
            let status = (message.payload >> 32) & 0xffff;
            let echo = message.payload & 0xffff_ffff;
            drop_caps(self.rt, &message);
            if echo != self.next {
                return Err(-22);
            }
            if status != 0 {
                return Err(-(status as i64));
            }
            self.next += 1;
            self.rt
                .read_user_bytes(DIRECT_PAGE_VA, page)
                .map_err(|_| -14)?;
            let at = bulk::HEADER_BYTES;
            return Ok((
                u32::from_le_bytes(page[at + 24..at + 28].try_into().unwrap()),
                u32::from_le_bytes(page[at + 28..at + 32].try_into().unwrap()),
            ));
        }
    }

    pub fn open(&mut self, path: &[u8]) -> Result<u8, i64> {
        if path.is_empty() || path.len() > MAX_PATH {
            return Err(-36);
        }
        let sequence = self.next;
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, sequence, 1);
        bulk::encode_request(
            &mut page,
            0,
            Request {
                op: Request::OPEN,
                fd: bulk::DIRFD_ROOT,
                length: path.len(),
                data: bulk::DATA_OFFSET,
                ..Request::default()
            },
        );
        page[bulk::DATA_OFFSET..bulk::DATA_OFFSET + path.len()].copy_from_slice(path);
        let (status, fd) = self.submit(&mut page)?;
        if status != 0 {
            return Err(-i64::from(status));
        }
        u8::try_from(fd).map_err(|_| -5)
    }

    pub fn read(&mut self, fd: u8, out: &mut [u8]) -> Result<usize, i64> {
        if out.is_empty() || out.len() > bulk::DATA_BYTES {
            return Err(-22);
        }
        let sequence = self.next;
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, sequence, 1);
        bulk::encode_request(
            &mut page,
            0,
            Request {
                op: Request::READ,
                fd,
                length: out.len(),
                data: bulk::DATA_OFFSET,
                ..Request::default()
            },
        );
        let (status, count) = self.submit(&mut page)?;
        if status != 0 {
            return Err(-i64::from(status));
        }
        let count = (count as usize).min(out.len());
        out[..count].copy_from_slice(&page[bulk::DATA_OFFSET..bulk::DATA_OFFSET + count]);
        Ok(count)
    }

    pub fn shutdown(&mut self) {
        let (store_cookie, store) = self.store;
        let _ = self.rt.send(
            TaskId::from_raw(store_cookie as u32),
            store,
            bulk::CLOSE,
            &[],
        );
        let _ = self.rt.memseg_unmap(self.self_cap, DIRECT_PAGE_VA, 1);
        let _ = self.rt.cap_drop(self.page);
    }
}

fn prima_user_store_execute(
    files: &mut OpenFiles<SERVICE_FILE_HANDLES>,
    ram: &mut Ram,
    page: &mut [u8; PAGE_BYTES],
    sequence: u64,
) -> Result<(), bulk::Invalid> {
    crate::bulk::execute(files, ram, View::ROOT, page, sequence, status)
}

/// Same byte storage the former frontend owned; now private to the transport.
pub struct Ram(pub [u8; 4096]);
impl Storage for Ram {
    fn read(&mut self, at: usize, bytes: &mut [u8]) -> bool {
        let Some(end) = at.checked_add(bytes.len()) else {
            return false;
        };
        let Some(source) = self.0.get(at..end) else {
            return false;
        };
        bytes.copy_from_slice(source);
        true
    }
    fn write(&mut self, at: usize, bytes: &[u8]) -> bool {
        let Some(end) = at.checked_add(bytes.len()) else {
            return false;
        };
        let Some(target) = self.0.get_mut(at..end) else {
            return false;
        };
        target.copy_from_slice(bytes);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prima_runtime::{bulk::DIRFD_ROOT, vfs};

    fn cap() -> Capability {
        Capability::from_raw((1 << 32) | 3).unwrap()
    }

    #[test]
    fn session_roundtrip_covers_open_write_read_stat_seek_dirents_and_close() {
        let mut store = Local::new().unwrap();
        let mut session = Session::new();
        let fd = session
            .open(&mut store, DIRFD_ROOT, b"/fixture.txt", 0)
            .unwrap();
        assert_eq!(fd, WIRE_FD_BASE);

        let stat = session.stat(&mut store, fd).unwrap();
        assert_eq!(
            u64::from_le_bytes(stat[8..16].try_into().unwrap()),
            b"hello from prima\n".len() as u64
        );
        assert_eq!(stat[4], vfs::NodeKind::File.as_raw());

        let mut out = [0; 32];
        let read = session.read(&mut store, fd, &mut out).unwrap();
        assert_eq!(&out[..read], b"hello from prima\n");

        assert_eq!(session.lseek(&mut store, fd, 0, 0), Ok(0));
        assert_eq!(session.read(&mut store, fd, &mut out[..5]), Ok(5));
        assert_eq!(&out[..5], b"hello");
        assert_eq!(session.close(&mut store, fd), Ok(()));
        assert_eq!(session.stat(&mut store, fd), Err(-9));

        // The fixture root is read-only; create one writable file to prove the
        // WRITE descriptor path and shared cursor.
        vfs::chmod(&mut store.ram, vfs::ROOT, b"/", 0o755).unwrap();
        let created = session
            .open(
                &mut store,
                DIRFD_ROOT,
                b"/new",
                bulk::OPEN_CREATE | bulk::ACCESS_READ_WRITE,
            )
            .unwrap();
        assert_eq!(session.write(&mut store, created, b"abc"), Ok(3));
        assert_eq!(session.lseek(&mut store, created, 0, 0), Ok(0));
        let mut written = [0; 3];
        assert_eq!(session.read(&mut store, created, &mut written), Ok(3));
        assert_eq!(&written, b"abc");
        assert_eq!(session.close(&mut store, created), Ok(()));
    }

    #[test]
    fn session_reports_store_errors_and_rewinds_partial_dirents() {
        let mut store = Local::new().unwrap();
        let mut session = Session::new();
        assert_eq!(
            session.open(&mut store, DIRFD_ROOT, b"/missing", 0),
            Err(-2)
        );
        let dir = session
            .open(&mut store, DIRFD_ROOT, b"/fixturedir", bulk::OPEN_DIRECTORY)
            .unwrap();
        let mut neutral = [0; bulk::DATA_BYTES];
        let read = session
            .getdents(&mut store, dir, &mut neutral[..24])
            .unwrap();
        assert!(read > 4 && read <= 24);
        assert_eq!(neutral[3], 1);
        assert_eq!(&neutral[4..5], b".");
        // A short budget rewinds the record the frontend did not emit.
        assert_eq!(session.lseek(&mut store, dir, -1, 1), Ok(read as u64 - 1));
        assert_eq!(session.close(&mut store, dir), Ok(()));
    }

    #[test]
    fn io_errors_keep_the_frozen_storage_errno() {
        assert_eq!(status(VfsError::Unformatted), 5);
        assert_eq!(status(VfsError::Io), 5);
    }

    #[test]
    fn transport_mismatched_sequence_is_rejected_before_file_effects() {
        let mut store = Local::new().unwrap();
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, 2, 1);
        bulk::encode_request(
            &mut page,
            0,
            Request {
                op: Request::OPEN,
                fd: DIRFD_ROOT,
                length: 1,
                data: bulk::DATA_OFFSET,
                ..Request::default()
            },
        );
        page[bulk::DATA_OFFSET] = b'/';
        assert_eq!(store.exchange(0, &mut page, 2), Err(-22));
        assert_eq!(store.files.len(), 0);
    }

    #[test]
    fn fork_inherits_the_shared_cursor_and_exit_closes_one_owner() {
        let mut store = Local::new().unwrap();
        let mut parent = Session::new();
        let fd = parent
            .open(&mut store, DIRFD_ROOT, b"/fixture.txt", 0)
            .unwrap();
        let mut out = [0; 5];
        assert_eq!(parent.read(&mut store, fd, &mut out), Ok(5));
        assert_eq!(&out, b"hello");

        let key = parent.reserve(&mut store).unwrap();
        let child_owner = parent
            .publish(&mut store, key, (2 << 32) | 3, cap())
            .unwrap();
        let mut child = Session::new();
        child.set_owner(child_owner);
        assert_eq!(child.read(&mut store, fd, &mut out), Ok(5));
        assert_eq!(&out, b" from");

        // The parent's exit releases its reference; the child keeps the OFD
        // and the shared cursor.
        parent.close_owner(&mut store).unwrap();
        assert_eq!(child.read(&mut store, fd, &mut out), Ok(5));
        assert_eq!(&out, b" prim");
        child.close_owner(&mut store).unwrap();

        // The freed owner slot is reusable for the next child.
        let key = parent.reserve(&mut store).unwrap();
        assert_eq!(
            parent.publish(&mut store, key, (3 << 32) | 4, cap()),
            Ok(child_owner)
        );
        // Republishing the same child is idempotent; other forms are rejected.
        assert_eq!(
            parent.publish(&mut store, key, (3 << 32) | 4, cap()),
            Ok(child_owner)
        );
        assert_eq!(
            parent.publish(&mut store, key, (4 << 32) | 5, cap()),
            Err(-13)
        );
        assert_eq!(parent.release(&mut store, key), Err(-13));
    }

    #[test]
    fn released_and_stale_reservations_cannot_publish() {
        let mut store = Local::new().unwrap();
        let mut session = Session::new();
        let key = session.reserve(&mut store).unwrap();
        assert_eq!(session.publish(&mut store, key, 0, cap()), Err(-13));
        assert_eq!(session.release(&mut store, key), Ok(()));
        assert_eq!(session.release(&mut store, key), Ok(()));
        assert_eq!(
            session.publish(&mut store, key, (2 << 32) | 3, cap()),
            Err(-13)
        );
        assert_eq!(session.close_owner(&mut store), Ok(()));
        assert!(session.reserve(&mut store).is_ok());
        assert_eq!(store.close_owner(31), Err(-13));
    }
}
