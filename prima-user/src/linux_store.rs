//! Linux personality's file-store client. Every VFS syscall becomes one bounded
//! batch submitted through a registered proxy session; the frontend keeps no
//! file content or store handles of its own.
//!
//! `Store` is the transport seam. The manager-issued session transport lands
//! with the P2b-4 startup wiring; `Local` is the in-process store used until
//! then and by host tests.

use prima_runtime::{
    bulk::{self, MAX_PATH, NEUTRAL_STAT_BYTES, PAGE_BYTES, Request, WIRE_FD_BASE},
    protocol::SERVICE_FILE_HANDLES,
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
    /// child owner.
    fn publish(&mut self, key: u32, child: u64) -> Result<u8, i64>;
    /// Drop a reservation that will not be published.
    fn release(&mut self, key: u32) -> Result<(), i64>;
    /// Close every OFD held by `owner` (process exit).
    fn close_owner(&mut self, owner: u8) -> Result<(), i64>;
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

    pub fn reserve(&mut self, store: &mut dyn Store) -> Result<u32, i64> {
        store.reserve(self.owner)
    }

    pub fn publish(&mut self, store: &mut dyn Store, key: u32, child: u64) -> Result<u8, i64> {
        store.publish(key, child)
    }

    pub fn release(&mut self, store: &mut dyn Store, key: u32) -> Result<(), i64> {
        store.release(key)
    }

    pub fn close_owner(&mut self, store: &mut dyn Store) -> Result<(), i64> {
        store.close_owner(self.owner)
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

    fn publish(&mut self, key: u32, child: u64) -> Result<u8, i64> {
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

    fn release(&mut self, key: u32) -> Result<(), i64> {
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
        let child_owner = parent.publish(&mut store, key, (2 << 32) | 3).unwrap();
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
            parent.publish(&mut store, key, (3 << 32) | 4),
            Ok(child_owner)
        );
        // Republishing the same child is idempotent; other forms are rejected.
        assert_eq!(
            parent.publish(&mut store, key, (3 << 32) | 4),
            Ok(child_owner)
        );
        assert_eq!(parent.publish(&mut store, key, (4 << 32) | 5), Err(-13));
        assert_eq!(parent.release(&mut store, key), Err(-13));
    }

    #[test]
    fn released_and_stale_reservations_cannot_publish() {
        let mut store = Local::new().unwrap();
        let mut session = Session::new();
        let key = session.reserve(&mut store).unwrap();
        assert_eq!(session.publish(&mut store, key, 0), Err(-13));
        assert_eq!(session.release(&mut store, key), Ok(()));
        assert_eq!(session.release(&mut store, key), Ok(()));
        assert_eq!(session.publish(&mut store, key, (2 << 32) | 3), Err(-13));
        assert_eq!(session.close_owner(&mut store), Ok(()));
        assert!(session.reserve(&mut store).is_ok());
        assert_eq!(store.close_owner(31), Err(-13));
    }
}
