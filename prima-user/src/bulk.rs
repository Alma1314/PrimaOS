//! The same native OpenFiles operations behind a checked, private batch snapshot.
use prima_runtime::{
    bulk::{self, PAGE_BYTES, Request},
    vfs::{HandleId, NodeKind, OpenFiles, Storage, VfsError},
};

/// Protocol validation is batch-atomic; file effects are ordered per operation.
/// An error in one operation does not roll back or replay earlier operations.
pub fn execute<S: Storage, const N: usize>(
    files: &mut OpenFiles<N>,
    storage: &mut S,
    page: &mut [u8; PAGE_BYTES],
    sequence: u64,
    error_status: impl Fn(VfsError) -> u32,
) -> Result<(), bulk::Invalid> {
    let (requests, count) = bulk::validate(page, sequence)?;
    for (i, request) in requests[..count].iter().enumerate() {
        let data = &mut page[request.data..request.data + request.length];
        if matches!(request.op, Request::READ | Request::READ_AT) {
            data.fill(0);
        }
        let result = if let Some(index) = request.fd.checked_sub(3) {
            let handle = HandleId(index as usize);
            match request.op {
                Request::READ => match files.info(storage, handle) {
                    Ok(info) if info.kind == NodeKind::Directory => {
                        files.read_directory_bytes(storage, handle, data)
                    }
                    Ok(_) => files.read(storage, handle, data),
                    Err(error) => Err(error),
                },
                Request::WRITE => {
                    write_partial(data, 3, |_, bytes| files.write(storage, handle, bytes))
                }
                Request::READ_AT => files.read_at(storage, handle, request.offset, data),
                Request::WRITE_AT => write_partial(data, 2, |at, bytes| {
                    files.write_at(storage, handle, request.offset + at as u64, bytes)
                }),
                _ => unreachable!(),
            }
        } else {
            Err(VfsError::BadHandle)
        };
        let (status, count) = match result {
            Ok(count) => (0, count as u32),
            Err(error) => (error_status(error), 0),
        };
        bulk::complete(page, i, status, count);
    }
    Ok(())
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
    use prima_runtime::{
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
                    execute(&mut files, &mut ram, &mut page, 1, |_| 28).unwrap();
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
        assert!(execute(&mut files, &mut ram, &mut page, 1, |_| 9).is_err());
        assert_eq!(files.info(&mut ram, id).unwrap().size, 0);
        page = invalid;
        execute(&mut files, &mut ram, &mut page, 1, |_| 9).unwrap();
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
        execute(&mut files, &mut ram, &mut page, 2, |_| 9).unwrap();
        assert_eq!(result(&page, 0), (9, 0));
        assert_eq!(&page[DATA_OFFSET..DATA_OFFSET + 8], &[0; 8]);
    }
}
