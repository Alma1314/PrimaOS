//! The same native OpenFiles operations behind a checked, private batch snapshot.
use crate::{
    bulk::{
        self, DIRFD_ROOT, MAX_PATH, NEUTRAL_STAT_BYTES, OPEN_ACCESS_MASK, OPEN_APPEND, OPEN_CREATE,
        OPEN_DIRECTORY, OPEN_EXCLUSIVE, OPEN_TRUNCATE, PAGE_BYTES, Request, STAT_NOFOLLOW,
        WIRE_FD_BASE,
    },
    vfs::{
        self, AccessMode, FileInfo, HandleId, NodeKind, OpenFiles, OpenFlags, SeekFrom, Storage,
        VfsError, View,
    },
};

/// Protocol validation is batch-atomic; file effects are ordered per operation.
/// An error in one operation does not roll back or replay earlier operations.
/// `OPEN` returns the wire fd, so a client that needs its result must submit it
/// in a separate batch; later descriptors in the same batch cannot use it.
pub fn execute<S: Storage, const N: usize>(
    files: &mut OpenFiles<N>,
    storage: &mut S,
    view: View,
    page: &mut [u8; PAGE_BYTES],
    sequence: u64,
    error_status: impl Fn(VfsError) -> u32,
) -> Result<(), bulk::Invalid> {
    execute_with(
        files,
        storage,
        view,
        page,
        sequence,
        error_status,
        bulk::validate,
    )
}

/// P3 v2 variant: the page header is the full `ordinal + 1`, not a v1
/// `MAX_SEQUENCE`-bounded replay counter.
pub fn execute_version_2<S: Storage, const N: usize>(
    files: &mut OpenFiles<N>,
    storage: &mut S,
    view: View,
    page: &mut [u8; PAGE_BYTES],
    sequence: u64,
    error_status: impl Fn(VfsError) -> u32,
) -> Result<(), bulk::Invalid> {
    execute_with(
        files,
        storage,
        view,
        page,
        sequence,
        error_status,
        bulk::validate_version_2,
    )
}

fn execute_with<S: Storage, const N: usize>(
    files: &mut OpenFiles<N>,
    storage: &mut S,
    view: View,
    page: &mut [u8; PAGE_BYTES],
    sequence: u64,
    error_status: impl Fn(VfsError) -> u32,
    validate: impl Fn(
        &[u8; PAGE_BYTES],
        u64,
    ) -> Result<([Request; bulk::MAX_OPS], usize), bulk::Invalid>,
) -> Result<(), bulk::Invalid> {
    let (requests, count) = validate(page, sequence)?;
    for (i, request) in requests[..count].iter().enumerate() {
        let result = apply(files, storage, view, page, *request);
        let (status, count) = match result {
            Ok(count) => (0, count as u32),
            Err(error) => (error_status(error), 0),
        };
        bulk::complete(page, i, status, count);
    }
    Ok(())
}

fn apply<S: Storage, const N: usize>(
    files: &mut OpenFiles<N>,
    storage: &mut S,
    view: View,
    page: &mut [u8; PAGE_BYTES],
    request: Request,
) -> Result<usize, VfsError> {
    let handle = || {
        request
            .fd
            .checked_sub(WIRE_FD_BASE)
            .map(|index| HandleId(index as usize))
            .ok_or(VfsError::BadHandle)
    };
    match request.op {
        Request::READ => {
            let data = &mut page[request.data..request.data + request.length];
            data.fill(0);
            let handle = handle()?;
            match files.info(storage, handle) {
                Ok(info) if info.kind == NodeKind::Directory => {
                    files.read_directory_bytes(storage, handle, data)
                }
                Ok(_) => files.read(storage, handle, data),
                Err(error) => Err(error),
            }
        }
        Request::WRITE => {
            let data = &page[request.data..request.data + request.length];
            let handle = handle()?;
            write_partial(data, 3, |_, bytes| files.write(storage, handle, bytes))
        }
        Request::READ_AT => {
            let data = &mut page[request.data..request.data + request.length];
            data.fill(0);
            files.read_at(storage, handle()?, request.offset, data)
        }
        Request::WRITE_AT => {
            let data = &page[request.data..request.data + request.length];
            let handle = handle()?;
            write_partial(data, 2, |at, bytes| {
                files.write_at(storage, handle, request.offset + at as u64, bytes)
            })
        }
        Request::OPEN => {
            let start = start_dir(files, storage, request.fd)?;
            let path = &page[request.data..request.data + request.length];
            let (id, _) = files.open_in(storage, view, start, path, open_flags(request.offset)?)?;
            Ok(id.0 + WIRE_FD_BASE as usize)
        }
        Request::STAT => {
            let info = files.info(storage, handle()?)?;
            let out = &mut page[request.data..request.data + NEUTRAL_STAT_BYTES];
            encode_stat(&info, out);
            Ok(NEUTRAL_STAT_BYTES)
        }
        Request::CLOSE => {
            files.close(storage, handle()?)?;
            Ok(0)
        }
        Request::GETDENTS => {
            let out = &mut page[request.data..request.data + request.length];
            getdents(files, storage, handle()?, out)
        }
        Request::LSEEK => {
            let from = match request.length {
                0 => SeekFrom::Start(request.offset as i64),
                1 => SeekFrom::Current(request.offset as i64),
                2 => SeekFrom::End(request.offset as i64),
                _ => return Err(VfsError::Invalid),
            };
            let next = files.seek(storage, handle()?, from)?;
            u32::try_from(next)
                .map(|next| next as usize)
                .map_err(|_| VfsError::Invalid)
        }
        Request::STAT_PATH => {
            let start = start_dir(files, storage, request.fd)?;
            let path = &page[request.data..request.data + request.length];
            let node = if request.offset & STAT_NOFOLLOW != 0 {
                vfs::resolve_link_in(storage, view, start, path)?
            } else {
                vfs::resolve_in(storage, view, start, path)?
            };
            let info = vfs::metadata(storage, node)?;
            let at = request.data + MAX_PATH;
            encode_stat(&info, &mut page[at..at + NEUTRAL_STAT_BYTES]);
            Ok(NEUTRAL_STAT_BYTES)
        }
        _ => Err(VfsError::Invalid),
    }
}

fn start_dir<S: Storage, const N: usize>(
    files: &OpenFiles<N>,
    storage: &mut S,
    fd: u8,
) -> Result<vfs::NodeId, VfsError> {
    if fd == DIRFD_ROOT {
        return Ok(vfs::ROOT);
    }
    let index = fd.checked_sub(WIRE_FD_BASE).ok_or(VfsError::BadHandle)?;
    let info = files.info(storage, HandleId(index as usize))?;
    if info.kind != NodeKind::Directory {
        return Err(VfsError::NotDir);
    }
    Ok(info.id)
}

fn open_flags(bits: u64) -> Result<OpenFlags, VfsError> {
    let access = match bits & OPEN_ACCESS_MASK {
        0 => AccessMode::ReadOnly,
        1 => AccessMode::WriteOnly,
        2 => AccessMode::ReadWrite,
        _ => return Err(VfsError::Invalid),
    };
    Ok(OpenFlags {
        access,
        create: bits & OPEN_CREATE != 0,
        exclusive: bits & OPEN_EXCLUSIVE != 0,
        truncate: bits & OPEN_TRUNCATE != 0,
        append: bits & OPEN_APPEND != 0,
        directory: bits & OPEN_DIRECTORY != 0,
    })
}

/// Return only whole directory records; rewind the store cursor past any
/// partial record so the next request sees it again.
fn getdents<S: Storage, const N: usize>(
    files: &mut OpenFiles<N>,
    storage: &mut S,
    handle: HandleId,
    out: &mut [u8],
) -> Result<usize, VfsError> {
    if out.len() > bulk::DATA_BYTES {
        return Err(VfsError::Invalid);
    }
    let mut chunk = [0u8; bulk::DATA_BYTES];
    let read = files.read_directory_bytes(storage, handle, &mut chunk[..out.len()])?;
    let mut whole = 0;
    while whole < read {
        if whole + 4 > read {
            break;
        }
        let name_len = chunk[whole + 3] as usize;
        let record = 4 + name_len;
        if whole + record > read {
            break;
        }
        whole += record;
    }
    if whole == 0 {
        if read != 0 {
            files.seek(storage, handle, SeekFrom::Current(-(read as i64)))?;
        }
        return Ok(0);
    }
    if whole < read {
        files.seek(storage, handle, SeekFrom::Current(-((read - whole) as i64)))?;
    }
    out[..whole].copy_from_slice(&chunk[..whole]);
    Ok(whole)
}

fn encode_stat(info: &FileInfo, out: &mut [u8]) {
    out[0..4].copy_from_slice(&u32::from(info.id).to_le_bytes());
    out[4] = info.kind.as_raw();
    out[5..7].copy_from_slice(&info.mode.to_le_bytes());
    out[7] = 0;
    out[8..16].copy_from_slice(&info.size.to_le_bytes());
}

// A full write is normally one VFS operation. On ENOSPC preserve the previous
// adapter's partial-progress behavior using its old quanta, entirely inside
// the service: retrying a rejected capacity check has no file-data effect.
fn write_partial(
    data: &[u8],
    quantum: usize,
    mut write: impl FnMut(usize, &[u8]) -> Result<usize, VfsError>,
) -> Result<usize, VfsError> {
    match write(0, data) {
        Err(VfsError::NoSpace) if data.len() > quantum => {}
        result => return result,
    }
    let mut done = 0;
    while done < data.len() {
        let bytes = &data[done..data.len().min(done + quantum)];
        match write(done, bytes) {
            Ok(count) => {
                done += count;
                if count < bytes.len() {
                    break;
                }
            }
            Err(_) if done != 0 => return Ok(done),
            Err(error) => return Err(error),
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bulk::{DATA_OFFSET, DESCRIPTOR_BYTES, HEADER_BYTES},
        vfs::{self, AccessMode, OpenFlags, SeekFrom},
    };
    struct Ram([u8; 4096]);
    impl Storage for Ram {
        fn read(&mut self, at: usize, bytes: &mut [u8]) -> bool {
            if let Some(source) = self.0.get(at..at.saturating_add(bytes.len())) {
                bytes.copy_from_slice(source);
                true
            } else {
                false
            }
        }
        fn write(&mut self, at: usize, bytes: &[u8]) -> bool {
            if let Some(target) = self.0.get_mut(at..at.saturating_add(bytes.len())) {
                target.copy_from_slice(bytes);
                true
            } else {
                false
            }
        }
    }
    fn result(page: &[u8], i: usize) -> (u32, u32) {
        let at = HEADER_BYTES + i * DESCRIPTOR_BYTES + 24;
        (
            u32::from_le_bytes(page[at..at + 4].try_into().unwrap()),
            u32::from_le_bytes(page[at + 4..at + 8].try_into().unwrap()),
        )
    }
    #[test]
    fn full_buffer_enospc_preserves_old_stream_and_positional_partial_progress() {
        for op in [Request::WRITE, Request::WRITE_AT] {
            let run = |legacy: bool| {
                let mut ram = Ram([0; 4096]);
                vfs::format(&mut ram).unwrap();
                vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).unwrap();
                let mut files = OpenFiles::<8>::new();
                let (id, _) = files
                    .open(
                        &mut ram,
                        vfs::ROOT,
                        b"/full",
                        OpenFlags {
                            create: true,
                            access: AccessMode::ReadWrite,
                            ..OpenFlags::default()
                        },
                    )
                    .unwrap();
                files.seek(&mut ram, id, SeekFrom::Start(13)).unwrap();
                let mut page = [0; PAGE_BYTES];
                bulk::initialize(&mut page, 1, 1);
                bulk::encode_request(
                    &mut page,
                    0,
                    Request {
                        op,
                        fd: id.0 as u8 + 3,
                        offset: if op == Request::WRITE_AT { 13 } else { 0 },
                        length: bulk::DATA_BYTES,
                        data: DATA_OFFSET,
                    },
                );
                page[DATA_OFFSET..].fill(0x7b);
                let count = if legacy {
                    let quantum = if op == Request::WRITE { 3 } else { 2 };
                    let mut count = 0;
                    for bytes in page[DATA_OFFSET..].chunks(quantum) {
                        let result = if op == Request::WRITE {
                            files.write(&mut ram, id, bytes)
                        } else {
                            files.write_at(&mut ram, id, 13 + count as u64, bytes)
                        };
                        match result {
                            Ok(written) => count += written,
                            Err(VfsError::NoSpace) => break,
                            other => panic!("unexpected legacy result: {other:?}"),
                        }
                    }
                    count
                } else {
                    execute(&mut files, &mut ram, vfs::View::ROOT, &mut page, 1, |_| 28).unwrap();
                    assert_eq!(result(&page, 0).0, 0);
                    result(&page, 0).1 as usize
                };
                assert!(count > 512 && count < bulk::DATA_BYTES);
                let mut out = [0; bulk::DATA_BYTES];
                assert_eq!(files.read_at(&mut ram, id, 0, &mut out), Ok(13 + count));
                assert_eq!(&out[..13], &[0; 13]);
                assert!(out[13..13 + count].iter().all(|byte| *byte == 0x7b));
                (
                    count,
                    files.seek(&mut ram, id, SeekFrom::Current(0)).unwrap(),
                    files.info(&mut ram, id).unwrap(),
                    out,
                )
            };
            assert_eq!(run(false), run(true));
        }
    }
    #[test]
    fn batches_keep_owner_checks_positional_cursor_partial_results_and_zero_short_reads() {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).unwrap();
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).unwrap();
        let mut files = OpenFiles::<8>::new();
        let (id, _) = files
            .open(
                &mut ram,
                vfs::ROOT,
                b"/bulk",
                OpenFlags {
                    create: true,
                    access: AccessMode::ReadWrite,
                    ..OpenFlags::default()
                },
            )
            .unwrap();
        let fd = id.0 as u8 + 3;
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, 1, 3);
        let write = Request {
            op: Request::WRITE,
            fd,
            length: 512,
            data: DATA_OFFSET,
            ..Request::default()
        };
        bulk::encode_request(&mut page, 0, write);
        bulk::encode_request(
            &mut page,
            1,
            Request {
                op: Request::READ_AT,
                length: 520,
                data: DATA_OFFSET + 512,
                ..write
            },
        );
        bulk::encode_request(
            &mut page,
            2,
            Request {
                fd: 255,
                data: DATA_OFFSET + 1032,
                length: 0,
                ..write
            },
        );
        page[DATA_OFFSET..DATA_OFFSET + 512].fill(0x53);
        page[DATA_OFFSET + 512..DATA_OFFSET + 1032].fill(0xa5);
        let invalid = page;
        page[HEADER_BYTES + 2 * DESCRIPTOR_BYTES + 2] = 1;
        assert!(execute(&mut files, &mut ram, vfs::View::ROOT, &mut page, 1, |_| 9).is_err());
        assert_eq!(files.info(&mut ram, id).unwrap().size, 0);
        page = invalid;
        execute(&mut files, &mut ram, vfs::View::ROOT, &mut page, 1, |_| 9).unwrap();
        assert_eq!(result(&page, 0), (0, 512));
        assert_eq!(result(&page, 1), (0, 512));
        assert_eq!(result(&page, 2), (9, 0));
        assert_eq!(&page[DATA_OFFSET + 512..DATA_OFFSET + 1024], &[0x53; 512]);
        assert_eq!(&page[DATA_OFFSET + 1024..DATA_OFFSET + 1032], &[0; 8]);
        assert_eq!(files.seek(&mut ram, id, SeekFrom::Current(0)), Ok(512));
        files.set_owner(1).unwrap();
        bulk::initialize(&mut page, 2, 1);
        bulk::encode_request(
            &mut page,
            0,
            Request {
                op: Request::READ_AT,
                length: 8,
                ..write
            },
        );
        execute(&mut files, &mut ram, vfs::View::ROOT, &mut page, 2, |_| 9).unwrap();
        assert_eq!(result(&page, 0), (9, 0));
        assert_eq!(&page[DATA_OFFSET..DATA_OFFSET + 8], &[0; 8]);
    }

    fn run(
        files: &mut OpenFiles<8>,
        ram: &mut Ram,
        view: View,
        request: Request,
        fill: impl FnOnce(&mut [u8; PAGE_BYTES]),
        status: impl Fn(VfsError) -> u32,
    ) -> ([u8; PAGE_BYTES], (u32, u32)) {
        let mut page = [0; PAGE_BYTES];
        bulk::initialize(&mut page, 1, 1);
        bulk::encode_request(&mut page, 0, request);
        fill(&mut page);
        execute(files, ram, view, &mut page, 1, status).unwrap();
        let result = result(&page, 0);
        (page, result)
    }

    fn fixture_file(ram: &mut Ram, path: &[u8], bytes: &[u8]) {
        let opened = vfs::open(
            ram,
            vfs::ROOT,
            path,
            OpenFlags {
                create: true,
                access: AccessMode::ReadWrite,
                ..OpenFlags::default()
            },
        )
        .unwrap();
        let mut node = opened.node;
        vfs::write(ram, opened.id, &mut node, 0, bytes).unwrap();
    }

    #[test]
    fn p2b_open_stat_seek_read_close_roundtrip() {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).unwrap();
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).unwrap();
        fixture_file(&mut ram, b"/file", b"hello");
        let mut files = OpenFiles::<8>::new();

        let (_, (status, fd)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::OPEN,
                fd: DIRFD_ROOT,
                length: 5,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |page| page[DATA_OFFSET..DATA_OFFSET + 5].copy_from_slice(b"/file"),
            |_| 9,
        );
        assert_eq!((status, fd), (0, u32::from(bulk::WIRE_FD_BASE)));
        let fd = fd as u8;

        let (page, (status, count)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::STAT,
                fd,
                length: NEUTRAL_STAT_BYTES,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |_| {},
            |_| 9,
        );
        assert_eq!((status, count), (0, NEUTRAL_STAT_BYTES as u32));
        assert_eq!(page[DATA_OFFSET + 4], NodeKind::File.as_raw());
        assert_eq!(
            u64::from_le_bytes(page[DATA_OFFSET + 8..DATA_OFFSET + 16].try_into().unwrap()),
            5
        );

        let (_, (status, offset)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::LSEEK,
                fd,
                offset: 1,
                length: 0,
                data: DATA_OFFSET,
            },
            |_| {},
            |_| 9,
        );
        assert_eq!((status, offset), (0, 1));

        let (page, (status, count)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::READ,
                fd,
                length: 4,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |_| {},
            |_| 9,
        );
        assert_eq!((status, count), (0, 4));
        assert_eq!(&page[DATA_OFFSET..DATA_OFFSET + 4], b"ello");

        let (_, (status, _)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::CLOSE,
                fd,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |_| {},
            |_| 9,
        );
        assert_eq!(status, 0);

        let (_, (status, count)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::STAT,
                fd,
                length: NEUTRAL_STAT_BYTES,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |_| {},
            |_| 9,
        );
        assert_eq!((status, count), (9, 0));
    }

    #[test]
    fn p2b_getdents_returns_whole_records_and_rewinds() {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).unwrap();
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).unwrap();
        vfs::mkdir(&mut ram, vfs::ROOT, b"/d").unwrap();
        fixture_file(&mut ram, b"/d/a", b"a");
        fixture_file(&mut ram, b"/d/bb", b"b");
        let mut files = OpenFiles::<8>::new();

        let (_, (status, fd)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::OPEN,
                fd: DIRFD_ROOT,
                offset: OPEN_DIRECTORY,
                length: 2,
                data: DATA_OFFSET,
            },
            |page| page[DATA_OFFSET..DATA_OFFSET + 2].copy_from_slice(b"/d"),
            |_| 9,
        );
        assert_eq!((status, fd), (0, u32::from(WIRE_FD_BASE)));
        let fd = fd as u8;

        let expected: [&[u8]; 4] = [b".", b"..", b"a", b"bb"];
        let mut step = 0;
        loop {
            let (page, (status, count)) = run(
                &mut files,
                &mut ram,
                View::ROOT,
                Request {
                    op: Request::GETDENTS,
                    fd,
                    length: 8,
                    data: DATA_OFFSET,
                    ..Request::default()
                },
                |_| {},
                |_| 9,
            );
            assert_eq!(status, 0);
            if count == 0 {
                assert_eq!(step, expected.len());
                break;
            }
            let name_len = page[DATA_OFFSET + 3] as usize;
            assert_eq!(count as usize, 4 + name_len);
            assert_eq!(
                &page[DATA_OFFSET + 4..DATA_OFFSET + 4 + name_len],
                expected[step]
            );
            step += 1;
        }
    }

    #[test]
    fn p2b_open_path_cannot_escape_the_view_root() {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).unwrap();
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).unwrap();
        let view_root = vfs::mkdir(&mut ram, vfs::ROOT, b"/v").unwrap();
        fixture_file(&mut ram, b"/v/file", b"abc");
        fixture_file(&mut ram, b"/outside", b"secret");
        let view = View::new(view_root);
        let mut files = OpenFiles::<8>::new();

        let (_, (status, fd)) = run(
            &mut files,
            &mut ram,
            view,
            Request {
                op: Request::OPEN,
                fd: DIRFD_ROOT,
                length: 8,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |page| page[DATA_OFFSET..DATA_OFFSET + 8].copy_from_slice(b"/../file"),
            |_| 9,
        );
        assert_eq!((status, fd), (0, u32::from(WIRE_FD_BASE)));

        let (_, (status, count)) = run(
            &mut files,
            &mut ram,
            view,
            Request {
                op: Request::OPEN,
                fd: DIRFD_ROOT,
                length: 11,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |page| page[DATA_OFFSET..DATA_OFFSET + 11].copy_from_slice(b"/../outside"),
            |error| {
                if error == VfsError::NoEnt { 2 } else { 9 }
            },
        );
        assert_eq!((status, count), (2, 0));
    }

    #[test]
    fn p2b_stat_path_uses_the_view_and_fixed_output_window() {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).unwrap();
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).unwrap();
        let view_root = vfs::mkdir(&mut ram, vfs::ROOT, b"/v").unwrap();
        fixture_file(&mut ram, b"/v/file", b"abc");
        let view = View::new(view_root);
        let mut files = OpenFiles::<8>::new();

        for path in [b"/file".as_slice(), b"/../file".as_slice()] {
            let (page, (status, count)) = run(
                &mut files,
                &mut ram,
                view,
                Request {
                    op: Request::STAT_PATH,
                    fd: DIRFD_ROOT,
                    length: path.len(),
                    data: DATA_OFFSET,
                    ..Request::default()
                },
                |page| page[DATA_OFFSET..DATA_OFFSET + path.len()].copy_from_slice(path),
                |_| 9,
            );
            assert_eq!((status, count), (0, NEUTRAL_STAT_BYTES as u32));
            assert_eq!(page[DATA_OFFSET + MAX_PATH + 4], NodeKind::File.as_raw());
            assert_eq!(
                u64::from_le_bytes(
                    page[DATA_OFFSET + MAX_PATH + 8..DATA_OFFSET + MAX_PATH + 16]
                        .try_into()
                        .unwrap()
                ),
                3
            );
        }
    }

    #[test]
    fn p2b_open_reports_emfile_through_the_proxy_status_hook() {
        let mut ram = Ram([0; 4096]);
        vfs::format(&mut ram).unwrap();
        vfs::chmod(&mut ram, vfs::ROOT, b"/", 0o755).unwrap();
        fixture_file(&mut ram, b"/file", b"hello");
        let mut files = OpenFiles::<8>::new();
        for _ in 0..8 {
            files
                .open(
                    &mut ram,
                    vfs::ROOT,
                    b"/file",
                    OpenFlags {
                        access: AccessMode::ReadOnly,
                        ..OpenFlags::default()
                    },
                )
                .unwrap();
        }
        let (_, (status, count)) = run(
            &mut files,
            &mut ram,
            View::ROOT,
            Request {
                op: Request::OPEN,
                fd: DIRFD_ROOT,
                length: 5,
                data: DATA_OFFSET,
                ..Request::default()
            },
            |page| page[DATA_OFFSET..DATA_OFFSET + 5].copy_from_slice(b"/file"),
            |error| {
                if error == VfsError::TooManyOpen {
                    24
                } else {
                    9
                }
            },
        );
        assert_eq!((status, count), (24, 0));
    }
}
