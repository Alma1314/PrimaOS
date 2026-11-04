//! Bounded Linux x86_64 personality. Only this user-space module interprets
//! Linux numbers, errno, flags, stat and dirent layouts. Native Gate controls
//! remain independent of the personality. Profile publication needs runtime evidence.
use prima_runtime::{
    Capability, Error, Rights, Runtime, X86_64Transport,
    gate::GateRequest,
    vfs::{self, FileInfo, HandleId, NodeKind, OpenFiles, Storage, VfsError},
};

#[path = "linux_process.rs"]
pub mod process;
#[path = "linux_signal.rs"]
pub mod signal;
#[path = "linux_thread.rs"]
pub mod thread;

type Native = Runtime<X86_64Transport>;
type Result<T> = core::result::Result<T, i64>;
const RW: Rights = Rights::from_bits(6);
const PAGE: u64 = 4096;
const HEAP: u64 = 0x1000_0000;
const MMAP: u64 = 0x2000_0000;
pub const INPUT: &[u8] = b"z\nb\na\nb\n";
pub const FILE_CONTENT: &[u8] = b"hello from prima\n";
pub const CASES: [&str; 11] = [
    "true", "echo", "cat", "ls", "uname", "sort", "P1", "P2", "M4", "M4-fork", "M3",
];
#[cfg(feature = "linux-probe")]
pub const P1: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stage6-p1.elf"));
#[cfg(not(feature = "linux-probe"))]
pub const P1: &[u8] = &[];
#[cfg(feature = "linux-probe")]
pub const P2: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stage6-p2.elf"));
#[cfg(not(feature = "linux-probe"))]
pub const P2: &[u8] = &[];

#[cfg(feature = "linux-probe")]
pub const PTHREAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stage6-pthread.elf"));
#[cfg(not(feature = "linux-probe"))]
pub const PTHREAD: &[u8] = &[];

#[cfg(feature = "linux-probe")]
pub const PROCESS_CHILD: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/stage6-process-child.elf"));
#[cfg(not(feature = "linux-probe"))]
pub const PROCESS_CHILD: &[u8] = &[];
#[cfg(feature = "linux-probe")]
pub const PROCESS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stage6-process.elf"));
#[cfg(not(feature = "linux-probe"))]
pub const PROCESS: &[u8] = &[];

#[cfg(feature = "linux-probe")]
pub const MATRIX: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stage6-matrix.elf"));
#[cfg(not(feature = "linux-probe"))]
pub const MATRIX: &[u8] = &[];

fn native_error(error: Error) -> i64 {
    match error {
        Error::Fault => -14,
        Error::Denied => -13,
        Error::InvalidInput => -22,
        Error::Again => -11,
        Error::Unsupported => -38,
        Error::NotFound => -2,
        Error::ResourceExhausted => -12,
        _ => -5,
    }
}
fn fs_error(error: VfsError) -> i64 {
    match error {
        VfsError::NoEnt => -2,
        VfsError::NotDir => -20,
        VfsError::Access => -13,
        VfsError::Exist => -17,
        VfsError::Invalid => -22,
        VfsError::NoSpace => -28,
        VfsError::Loop => -40,
        VfsError::BadHandle => -9,
        VfsError::TooManyOpen => -24,
        VfsError::Unformatted | VfsError::Io => -5,
    }
}

pub struct Ram([u8; 4096]);
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

/// One family shares VFS open descriptions and inherited stdin offsets.
pub struct Shared {
    ram: Ram,
    files: OpenFiles<16>,
    input_position: usize,
    dir_position: [usize; 16],
    open_flags: [u64; 16],
    segments: [Option<(Capability, u32)>; 128],
}
impl Shared {
    pub fn new() -> Result<Self> {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).map_err(fs_error)?;
        // The common VFS starts with a read-only root; populate our fixture
        // before restoring that policy for Linux callers.
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).map_err(fs_error)?;
        vfs::mkdir(&mut ram, vfs::ROOT, b"/fixturedir").map_err(fs_error)?;
        for (path, bytes) in [
            (b"/fixture.txt".as_slice(), FILE_CONTENT),
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
            .map_err(fs_error)?;
            let mut node = opened.node;
            vfs::write(&mut ram, opened.id, &mut node, 0, bytes).map_err(fs_error)?;
        }
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o555).map_err(fs_error)?;
        Ok(Self {
            ram,
            files: OpenFiles::new(),
            input_position: 0,
            dir_position: [0; 16],
            open_flags: [0; 16],
            segments: [None; 128],
        })
    }
    fn select(&mut self, owner: u8) -> Result<()> {
        self.files.set_owner(owner).map_err(fs_error)
    }
    fn release_segment(&mut self, rt: &Native, cap: Capability, owner: u8) -> Result<()> {
        let slot = self
            .segments
            .iter()
            .position(|s| s.is_some_and(|(c, _)| c == cap))
            .ok_or(-5)?;
        let (_, owners) = self.segments[slot].unwrap();
        let remaining = owners & !(1 << owner);
        if remaining == 0 {
            rt.memseg_destroy(cap).map_err(native_error)?;
            rt.cap_drop(cap).map_err(native_error)?;
            self.segments[slot] = None;
        } else {
            self.segments[slot] = Some((cap, remaining));
        }
        Ok(())
    }
}

/// All state belongs to this personality, not to HexArc/COSIR or a syscall adapter.
pub struct Process {
    pub manager: Capability,
    pub cookie: u64,
    pub tls: u64,
    pub threads: thread::ThreadGroup,
    clear_tid: u64,
    owner: u8,
    fds: [Option<HandleId>; 16],
    fd_flags: [u64; 16],
    standard_open: [bool; 3],
    brk: u64,
    brk_mapped: u64,
    next_map: u64,
    allocations: [Option<(Capability, u64, u64)>; 32],
}

impl Process {
    pub fn new(manager: Capability, cookie: u64) -> Result<Self> {
        if cookie as u32 == 0 || cookie >> 32 == 0 {
            return Err(-22);
        }
        Ok(Self {
            manager,
            cookie,
            tls: 0,
            threads: thread::ThreadGroup::default(),
            clear_tid: 0,
            owner: 0,
            fds: [None; 16],
            fd_flags: [0; 16],
            standard_open: [true; 3],
            brk: HEAP,
            brk_mapped: HEAP,
            next_map: MMAP,
            allocations: [None; 32],
        })
    }

    fn handle(&self, fd: u64) -> Result<HandleId> {
        self.fds.get(fd as usize).copied().flatten().ok_or(-9)
    }
    fn path(&self, rt: &Native, req: Capability, pointer: u64) -> Result<([u8; 64], usize)> {
        let mut path = [0; 64];
        for i in 0..64 {
            rt.gate_copy_in(
                req,
                pointer.checked_add(i as u64).ok_or(-14)?,
                &mut path[i..i + 1],
            )
            .map_err(native_error)?;
            if path[i] == 0 {
                return Ok((path, i));
            }
        }
        Err(-36)
    }
    fn start_dir(&mut self, shared: &mut Shared, dirfd: u64, path: &[u8]) -> Result<u8> {
        if path.starts_with(b"/") || dirfd as i32 == -100 {
            return Ok(vfs::ROOT);
        }
        let handle = self.handle(dirfd)?;
        let info = shared
            .files
            .info(&mut shared.ram, handle)
            .map_err(fs_error)?;
        if info.kind != NodeKind::Directory {
            return Err(-20);
        }
        Ok(info.id)
    }
    fn open(
        &mut self,
        shared: &mut Shared,
        rt: &Native,
        req: Capability,
        dirfd: u64,
        pointer: u64,
        flags: u64,
    ) -> Result<u64> {
        if self.standard_open.iter().any(|open| !open) {
            return Err(-38);
        }
        // Read-only files/directories, NONBLOCK, CLOEXEC and LARGEFILE. These
        // finite RAM nodes never block; retain NONBLOCK on the shared OFD.
        // Reject every other
        // flag rather than silently claiming creation, append or symlink semantics.
        if flags & !(0x10000 | 0x80000 | 0x8000 | 0x800) != 0 {
            return Err(-38);
        }
        let (path, len) = self.path(rt, req, pointer)?;
        let start = self.start_dir(shared, dirfd, &path[..len])?;
        let fd = (3..16).find(|i| self.fds[*i].is_none()).ok_or(-24)?;
        let (handle, _) = shared
            .files
            .open(
                &mut shared.ram,
                start,
                &path[..len],
                vfs::OpenFlags {
                    directory: flags & 0x10000 != 0,
                    ..Default::default()
                },
            )
            .map_err(fs_error)?;
        self.fds[fd] = Some(handle);
        self.fd_flags[fd] = u64::from(flags & 0x80000 != 0);
        shared.dir_position[handle.0] = 0;
        shared.open_flags[handle.0] = (flags & !0x80000) | 0x8000;
        Ok(fd as u64)
    }
    fn read(
        &mut self,
        shared: &mut Shared,
        rt: &Native,
        req: Capability,
        fd: u64,
        pointer: u64,
        count: u64,
    ) -> Result<u64> {
        let mut bytes = [0; 4096];
        let limit = count.min(bytes.len() as u64) as usize;
        if fd == 0 {
            if !self.standard_open[0] {
                return Err(-9);
            }
            let size = limit.min(INPUT.len() - shared.input_position);
            rt.gate_copy_out(
                req,
                pointer,
                &INPUT[shared.input_position..shared.input_position + size],
            )
            .map_err(native_error)?;
            shared.input_position += size;
            return Ok(size as u64);
        }
        let handle = self.handle(fd)?;
        let size = shared
            .files
            .read(&mut shared.ram, handle, &mut bytes[..limit])
            .map_err(fs_error)?;
        if let Err(error) = rt.gate_copy_out(req, pointer, &bytes[..size]) {
            shared
                .files
                .seek(
                    &mut shared.ram,
                    handle,
                    vfs::SeekFrom::Current(-(size as i64)),
                )
                .map_err(fs_error)?;
            return Err(native_error(error));
        }
        Ok(size as u64)
    }
    fn stat(
        &mut self,
        shared: &mut Shared,
        rt: &Native,
        req: Capability,
        fd: u64,
        pointer: u64,
    ) -> Result<u64> {
        let bytes = if fd <= 2 {
            if !self.standard_open[fd as usize] {
                return Err(-9);
            }
            standard_stat()
        } else {
            let handle = self.handle(fd)?;
            stat_bytes(
                shared
                    .files
                    .info(&mut shared.ram, handle)
                    .map_err(fs_error)?,
            )
        };
        rt.gate_copy_out(req, pointer, &bytes)
            .map_err(native_error)?;
        Ok(0)
    }
    fn getdents(
        &mut self,
        shared: &mut Shared,
        rt: &Native,
        req: Capability,
        fd: u64,
        pointer: u64,
        count: u64,
    ) -> Result<u64> {
        let handle = self.handle(fd)?;
        let info = shared
            .files
            .info(&mut shared.ram, handle)
            .map_err(fs_error)?;
        let mut output = [0; 4096];
        let mut size = 0;
        let mut next = shared.dir_position[handle.0];
        while let Some(entry) = vfs::dir_entry(&mut shared.ram, info.id, next).map_err(fs_error)? {
            let length = (19 + entry.name_len as usize + 1 + 7) & !7;
            if size + length > (count.min(4096) as usize) {
                if size == 0 {
                    return Err(-22);
                }
                break;
            }
            output[size..size + 8].copy_from_slice(&(u64::from(entry.node) + 1).to_le_bytes());
            output[size + 8..size + 16].copy_from_slice(&((next + 1) as u64).to_le_bytes());
            output[size + 16..size + 18].copy_from_slice(&(length as u16).to_le_bytes());
            output[size + 18] = if entry.kind == NodeKind::Directory {
                4
            } else {
                8
            };
            output[size + 19..size + 19 + entry.name_len as usize]
                .copy_from_slice(entry.name_bytes());
            size += length;
            next += 1;
        }
        rt.gate_copy_out(req, pointer, &output[..size])
            .map_err(native_error)?;
        shared.dir_position[handle.0] = next;
        Ok(size as u64)
    }
    fn allocate(&mut self, shared: &mut Shared, rt: &Native, base: u64, pages: u64) -> Result<()> {
        if pages == 0 || pages > 149 {
            return Err(-12);
        }
        let slot = self
            .allocations
            .iter()
            .position(Option::is_none)
            .ok_or(-12)?;
        let shared_slot = shared
            .segments
            .iter()
            .position(Option::is_none)
            .ok_or(-12)?;
        let cap = rt.memseg_create(pages, RW).map_err(native_error)?;
        if let Err(error) = rt.memseg_map(cap, self.manager, 0, base, pages, RW) {
            let _ = rt.memseg_destroy(cap);
            let _ = rt.cap_drop(cap);
            return Err(native_error(error));
        }
        shared.segments[shared_slot] = Some((cap, 1 << self.owner));
        self.allocations[slot] = Some((cap, base, pages));
        Ok(())
    }
    fn protect(&self, rt: &Native, address: u64, pages: u64, prot: u64) -> Result<()> {
        if prot & !7 != 0 {
            return Err(-22);
        }
        if prot & 4 != 0 {
            return Err(-13);
        } // anonymous Segment authority is RW only
        if prot == 0 {
            rt.vspace_access_enable(self.manager, address, pages, false)
                .map_err(native_error)
        } else {
            rt.memseg_protect(self.manager, address, pages, Rights::from_bits(prot << 1))
                .map_err(native_error)?;
            rt.vspace_access_enable(self.manager, address, pages, true)
                .map_err(native_error)
        }
    }
    pub fn clear_tid(&self, rt: &Native, req: Capability) {
        if self.clear_tid != 0 {
            let _ = rt.gate_copy_out(req, self.clear_tid, &[0; 4]);
        }
    }
    pub fn cleanup(&mut self, shared: &mut Shared, rt: &Native) -> Result<()> {
        shared.select(self.owner)?;
        shared.files.close_all(&mut shared.ram).map_err(fs_error)?;
        for allocation in &mut self.allocations {
            if let Some((cap, _, _)) = allocation.take() {
                shared.release_segment(rt, cap, self.owner)?;
            }
        }
        Ok(())
    }

    /// write output is supplied by the native service; all pointer reads still
    /// pass through request-authorized proxy copies. Returns a raw Linux result.
    pub fn dispatch(
        &mut self,
        shared: &mut Shared,
        rt: &Native,
        request: &GateRequest,
        mut write: impl FnMut(u64, &[u8]) -> Result<()>,
    ) -> i64 {
        if request.task_cookie != self.cookie && self.threads.member(request.task_cookie).is_none()
        {
            return -3;
        }
        if let Err(error) = shared.select(self.owner) {
            return error;
        }
        let req = request.capability;
        let a = request.args;
        let result: Result<u64> = (|| {
            match request.number {
                0 => self.read(shared, rt, req, a[0], a[1], a[2]),
                1 => {
                    if (a[0] != 1 && a[0] != 2) || !self.standard_open[a[0] as usize] {
                        return Err(-9);
                    }
                    let mut bytes = [0; 4096];
                    let size = a[2].min(4096) as usize;
                    rt.gate_copy_in(req, a[1], &mut bytes[..size])
                        .map_err(native_error)?;
                    write(a[0], &bytes[..size])?;
                    Ok(size as u64)
                }
                2 => self.open(shared, rt, req, (-100i64) as u64, a[0], a[1]),
                3 => {
                    if a[0] <= 2 {
                        let open = &mut self.standard_open[a[0] as usize];
                        if !*open {
                            return Err(-9);
                        }
                        *open = false;
                        return Ok(0);
                    }
                    let handle = self.handle(a[0])?;
                    shared
                        .files
                        .close(&mut shared.ram, handle)
                        .map_err(fs_error)?;
                    self.fds[a[0] as usize] = None;
                    Ok(0)
                }
                5 => self.stat(shared, rt, req, a[0], a[1]),
                8 => {
                    let handle = self.handle(a[0])?;
                    shared
                        .files
                        .seek(
                            &mut shared.ram,
                            handle,
                            match a[2] {
                                0 => vfs::SeekFrom::Start(a[1] as i64),
                                1 => vfs::SeekFrom::Current(a[1] as i64),
                                2 => vfs::SeekFrom::End(a[1] as i64),
                                _ => return Err(-22),
                            },
                        )
                        .map_err(fs_error)
                }
                9 => {
                    if a[0] != 0 || a[3] & !0x20000 != 0x22 || a[5] != 0 {
                        return Err(-38);
                    }
                    if a[1] == 0 || a[2] & !7 != 0 {
                        return Err(-22);
                    }
                    if a[2] & 4 != 0 {
                        return Err(-13);
                    }
                    let pages = a[1].checked_add(PAGE - 1).ok_or(-12)? / PAGE;
                    let base = self.next_map;
                    let end = base
                        .checked_add(pages.checked_mul(PAGE).ok_or(-12)?)
                        .ok_or(-12)?;
                    if end > 0x4000_0000 {
                        return Err(-12);
                    }
                    self.allocate(shared, rt, base, pages)?;
                    if a[2] != 3
                        && let Err(error) = self.protect(rt, base, pages, a[2])
                    {
                        let slot = self
                            .allocations
                            .iter()
                            .position(|v| v.is_some_and(|(_, b, _)| b == base))
                            .unwrap();
                        let (cap, _, _) = self.allocations[slot].unwrap();
                        rt.memseg_unmap(self.manager, base, pages)
                            .map_err(native_error)?;
                        shared.release_segment(rt, cap, self.owner)?;
                        self.allocations[slot] = None;
                        return Err(error);
                    }
                    self.next_map = end;
                    Ok(base)
                }
                10 => {
                    if a[0] & (PAGE - 1) != 0 || a[1] == 0 || a[2] & !7 != 0 {
                        return Err(-22);
                    }
                    self.protect(
                        rt,
                        a[0],
                        a[1].checked_add(PAGE - 1).ok_or(-22)? / PAGE,
                        a[2],
                    )?;
                    Ok(0)
                }
                11 => {
                    if a[0] & (PAGE - 1) != 0 || a[1] == 0 {
                        return Err(-22);
                    }
                    let index = self
                        .allocations
                        .iter()
                        .position(|v| {
                            v.is_some_and(|(_, base, pages)| {
                                base == a[0]
                                    && a[1].checked_add(PAGE - 1).map(|n| n / PAGE) == Some(pages)
                            })
                        })
                        .ok_or(-38)?;
                    let (cap, base, pages) = self.allocations[index].unwrap();
                    rt.memseg_unmap(self.manager, base, pages)
                        .map_err(native_error)?;
                    shared.release_segment(rt, cap, self.owner)?;
                    self.allocations[index] = None;
                    Ok(0)
                }
                12 => {
                    if a[0] == 0 || a[0] < self.brk || a[0] > HEAP + 149 * PAGE {
                        return Ok(self.brk);
                    }
                    let end = a[0].div_ceil(PAGE) * PAGE;
                    if end > self.brk_mapped {
                        if self
                            .allocate(shared, rt, self.brk_mapped, (end - self.brk_mapped) / PAGE)
                            .is_err()
                        {
                            return Ok(self.brk);
                        }
                        self.brk_mapped = end;
                    }
                    self.brk = a[0];
                    Ok(self.brk)
                }
                16 => Err(-25), // nonterminal input/output; no successful tty ioctl claim
                20 => {
                    if (a[0] != 1 && a[0] != 2) || !self.standard_open[a[0] as usize] {
                        return Err(-9);
                    }
                    if a[2] > 16 {
                        return Err(-22);
                    }
                    let mut total = 0u64;
                    for index in 0..a[2] {
                        let mut vector = [0; 16];
                        rt.gate_copy_in(req, a[1].checked_add(index * 16).ok_or(-14)?, &mut vector)
                            .map_err(native_error)?;
                        let base = u64::from_le_bytes(vector[..8].try_into().unwrap());
                        let count = u64::from_le_bytes(vector[8..].try_into().unwrap());
                        if count > 4096 {
                            return if total > 0 { Ok(total) } else { Err(-38) };
                        }
                        let mut bytes = [0; 4096];
                        if let Err(e) = rt.gate_copy_in(req, base, &mut bytes[..count as usize]) {
                            return if total > 0 {
                                Ok(total)
                            } else {
                                Err(native_error(e))
                            };
                        }
                        write(a[0], &bytes[..count as usize])?;
                        total += count;
                    }
                    Ok(total)
                }
                39 => Ok(self.cookie as u32 as u64),
                186 => Ok(request.task_cookie as u32 as u64),
                63 => {
                    let mut bytes = [0; 390];
                    for (index, value) in [
                        b"Linux".as_slice(),
                        b"prima",
                        b"6.8.0-prima",
                        b"#1 Prima",
                        b"x86_64",
                        b"(none)",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        bytes[index * 65..index * 65 + value.len()].copy_from_slice(value);
                    }
                    rt.gate_copy_out(req, a[0], &bytes).map_err(native_error)?;
                    Ok(0)
                }
                72 => {
                    if a[0] > 2 {
                        self.handle(a[0])?;
                    } else if !self.standard_open[a[0] as usize] {
                        return Err(-9);
                    }
                    match a[1] {
                        1 => self.fd_flags.get(a[0] as usize).copied().ok_or(-9),
                        2 if a[2] <= 1 => {
                            *self.fd_flags.get_mut(a[0] as usize).ok_or(-9)? = a[2];
                            Ok(0)
                        }
                        3 => Ok(if a[0] <= 2 {
                            u64::from(a[0] != 0)
                        } else {
                            shared.open_flags[self.handle(a[0])?.0]
                        }),
                        _ => Err(-38),
                    }
                }
                79 => {
                    if a[1] < 2 {
                        return Err(-34);
                    }
                    rt.gate_copy_out(req, a[0], b"/\0").map_err(native_error)?;
                    Ok(2)
                }
                89 => {
                    let (path, len) = self.path(rt, req, a[0])?;
                    if &path[..len] != b"/proc/self/exe" {
                        return Err(-2);
                    }
                    let size = a[2].min(8) as usize;
                    rt.gate_copy_out(req, a[1], &b"/busybox"[..size])
                        .map_err(native_error)?;
                    Ok(size as u64)
                }
                102 | 104 | 107 | 108 => Ok(0),
                158 => match a[0] {
                    0x1002 => {
                        if a[1] >= 1 << 47 {
                            return Err(-1);
                        }
                        let manager = self
                            .threads
                            .member(request.task_cookie)
                            .map_or(self.manager, |t| t.manager);
                        rt.task_set_tls(manager, a[1]).map_err(native_error)?;
                        if let Some(thread) = self.threads.member_mut(request.task_cookie) {
                            thread.tls = a[1];
                        }
                        if request.task_cookie == self.cookie {
                            self.tls = a[1];
                        }
                        Ok(0)
                    }
                    0x1003 => {
                        let tls = self
                            .threads
                            .member(request.task_cookie)
                            .map_or(self.tls, |t| t.tls);
                        rt.gate_copy_out(req, a[1], &tls.to_le_bytes())
                            .map_err(native_error)?;
                        Ok(0)
                    }
                    _ => Err(-22),
                },
                217 => self.getdents(shared, rt, req, a[0], a[1], a[2]),
                218 => {
                    self.clear_tid = a[0];
                    Ok(self.cookie as u32 as u64)
                }
                257 => self.open(shared, rt, req, a[0], a[1], a[2]),
                262 => {
                    if a[3] & !(0x100 | 0x1000) != 0 {
                        return Err(-22);
                    }
                    let (path, len) = self.path(rt, req, a[1])?;
                    if len == 0 && a[3] & 0x1000 != 0 {
                        return self.stat(shared, rt, req, a[0], a[2]);
                    }
                    let start = self.start_dir(shared, a[0], &path[..len])?;
                    let last = path[..len].rsplit(|b| *b == b'/').find(|p| !p.is_empty());
                    let node = if a[3] & 0x100 != 0 && !matches!(last, None | Some(b"." | b"..")) {
                        vfs::resolve_link(&mut shared.ram, start, &path[..len])
                    } else {
                        vfs::resolve(&mut shared.ram, start, &path[..len])
                    }
                    .map_err(fs_error)?;
                    let bytes = stat_bytes(vfs::metadata(&mut shared.ram, node).map_err(fs_error)?);
                    rt.gate_copy_out(req, a[2], &bytes).map_err(native_error)?;
                    Ok(0)
                }
                302 => {
                    if a[0] != 0 && a[0] != self.cookie as u32 as u64 {
                        return Err(-3);
                    }
                    if a[1] != 3 || a[2] != 0 {
                        return Err(-38);
                    }
                    if a[3] != 0 {
                        let mut limits = [0; 16];
                        limits[..8].copy_from_slice(&32768u64.to_le_bytes());
                        limits[8..].copy_from_slice(&32768u64.to_le_bytes());
                        rt.gate_copy_out(req, a[3], &limits).map_err(native_error)?;
                    }
                    Ok(0)
                }
                _ => Err(-38),
            }
        })();
        result
            .map(|value| value as i64)
            .unwrap_or_else(|error| error)
    }
}

fn standard_stat() -> [u8; 144] {
    let mut bytes = [0; 144];
    bytes[16..24].copy_from_slice(&1u64.to_le_bytes());
    bytes[24..28].copy_from_slice(&0o010600u32.to_le_bytes());
    bytes[56..64].copy_from_slice(&4096u64.to_le_bytes());
    bytes
}
pub fn stat_bytes(info: FileInfo) -> [u8; 144] {
    let mut bytes = standard_stat();
    bytes[8..16].copy_from_slice(&(u64::from(info.id) + 1).to_le_bytes());
    let kind = match info.kind {
        NodeKind::Directory => 0o040000,
        NodeKind::File => 0o100000,
        NodeKind::Null => 0o020000,
        NodeKind::Symlink => 0o120000,
    };
    bytes[24..28].copy_from_slice(&(kind | u32::from(info.mode)).to_le_bytes());
    bytes[48..56].copy_from_slice(&info.size.to_le_bytes());
    bytes[64..72].copy_from_slice(&info.size.div_ceil(512).to_le_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_raw_numbers_and_foreign_cookie_never_touch_parameters() {
        let cap = Capability::from_raw(0x100000002).unwrap();
        let mut process = Process::new(cap, 0x100000003).unwrap();
        let mut shared = Shared::new().unwrap();
        assert!(matches!(Process::new(cap, 0), Err(-22)));
        let mut request = GateRequest {
            capability: cap,
            task_cookie: 0x100000003,
            number: 0,
            args: [u64::MAX; 6],
            flags: 0,
        };
        for nr in [u64::MAX, 13, 56, 57, 59, 202, 273, 334] {
            request.number = nr;
            assert_eq!(
                process.dispatch(&mut shared, &Runtime::x86_64(), &request, |_, _| panic!(
                    "unexpected output"
                )),
                -38
            );
        }
        request.task_cookie += 1;
        request.number = 0; // would copy a pointer if dispatched to this Process
        assert_eq!(
            process.dispatch(&mut shared, &Runtime::x86_64(), &request, |_, _| panic!(
                "unexpected output"
            )),
            -3
        );
    }

    #[test]
    fn x86_64_stat_encodes_mode_size_blocks_and_padding_at_abi_offsets() {
        let bytes = stat_bytes(FileInfo {
            id: 7,
            kind: NodeKind::File,
            size: 513,
            mode: 0o644,
        });
        assert_eq!(u64::from_le_bytes(bytes[8..16].try_into().unwrap()), 8);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            0o100644
        );
        assert_eq!(u64::from_le_bytes(bytes[48..56].try_into().unwrap()), 513);
        assert_eq!(u64::from_le_bytes(bytes[64..72].try_into().unwrap()), 2);
        assert_eq!(&bytes[72..], &[0; 72]);
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).unwrap();
        assert_eq!(
            vfs::read_node(&mut ram, vfs::ROOT).unwrap().kind,
            NodeKind::Directory
        );
    }
}
