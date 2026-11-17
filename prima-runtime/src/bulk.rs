//! Versioned, bounded file batches. Callers snapshot shared memory before
//! validation; this codec never treats a descriptor or a local fd as authority.

pub const OPEN: u64 = 0x5f10_0000_0000_0000;
pub const OPEN_REPLY: u64 = 0x5f11_0000_0000_0000;
pub const SUBMIT: u64 = 0x5f12_0000_0000_0000;
pub const COMPLETE: u64 = 0x5f13_0000_0000_0000;
pub const CLOSE: u64 = 0x5f14_0000_0000_0000;

/// Proxy channels carry a store-validated slot: OPEN/CLOSE use the low 8 bits,
/// SUBMIT packs the slot in bits 32..39 and the sequence in the low 32 bits.
pub const PROXY_OPEN: u64 = 0x5f20_0000_0000_0000;
pub const PROXY_SUBMIT: u64 = 0x5f21_0000_0000_0000;
pub const PROXY_CLOSE: u64 = 0x5f22_0000_0000_0000;
/// Proxy fork lifecycle: the slot is in bits 32..39. RESERVE/RELEASE also
/// carry the reservation key in the low 32 bits; PUBLISH adds the published
/// child Task id in bits 40..47.
pub const PROXY_RESERVE: u64 = 0x5f23_0000_0000_0000;
pub const PROXY_PUBLISH: u64 = 0x5f24_0000_0000_0000;
pub const PROXY_RELEASE: u64 = 0x5f25_0000_0000_0000;
pub const PROXY_SLOT_MASK: u64 = 0xff;
pub const PROXY_SLOT_SHIFT: u64 = 32;
pub const PROXY_CHILD_SHIFT: u64 = 40;
pub const PAGE_BYTES: usize = 4096;
pub const MAX_OPS: usize = 8;
pub const HEADER_BYTES: usize = 32;
pub const DESCRIPTOR_BYTES: usize = 32;
pub const DATA_OFFSET: usize = HEADER_BYTES + MAX_OPS * DESCRIPTOR_BYTES;
pub const DATA_BYTES: usize = PAGE_BYTES - DATA_OFFSET;
pub const SERVER_VA: u64 = 0x0000_0b00_0040_0000;
pub const CLIENT_VA: u64 = 0x0000_0c00_0200_0000;
pub const MAX_SEQUENCE: u64 = u32::MAX as u64;

/// Path operations: the wire fd is a store handle index plus this base; an
/// application fd number never crosses the wire.
pub const WIRE_FD_BASE: u8 = 3;
/// `openat`/`newfstatat` root sentinel (AT_FDCWD is resolved by the frontend).
pub const DIRFD_ROOT: u8 = 0xff;
/// Frozen path limit for P2b; the VFS name limit is smaller.
pub const MAX_PATH: usize = 64;
/// Neutral stat record: node:u32 | kind:u8 | mode:u16 | pad:u8 | size:u64.
pub const NEUTRAL_STAT_BYTES: usize = 16;
/// A STAT_PATH descriptor reserves the path plus the fixed output window so
/// overlap validation sees both.
pub const STAT_PATH_BYTES: usize = MAX_PATH + NEUTRAL_STAT_BYTES;
pub const OPEN_ACCESS_MASK: u64 = 0b11;
pub const OPEN_CREATE: u64 = 1 << 2;
pub const OPEN_EXCLUSIVE: u64 = 1 << 3;
pub const OPEN_TRUNCATE: u64 = 1 << 4;
pub const OPEN_APPEND: u64 = 1 << 5;
pub const OPEN_DIRECTORY: u64 = 1 << 6;
pub const OPEN_FLAGS_MASK: u64 =
    OPEN_ACCESS_MASK | OPEN_CREATE | OPEN_EXCLUSIVE | OPEN_TRUNCATE | OPEN_APPEND | OPEN_DIRECTORY;
pub const ACCESS_READ: u64 = 0;
pub const ACCESS_WRITE: u64 = 1;
pub const ACCESS_READ_WRITE: u64 = 2;
pub const STAT_NOFOLLOW: u64 = 1;
pub const STAT_FLAGS_MASK: u64 = STAT_NOFOLLOW;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Request {
    pub op: u8,
    pub fd: u8,
    pub offset: u64,
    pub length: usize,
    pub data: usize,
}

impl Request {
    pub const READ: u8 = 1;
    pub const WRITE: u8 = 2;
    pub const OPEN: u8 = 3;
    pub const STAT: u8 = 4;
    pub const CLOSE: u8 = 5;
    pub const GETDENTS: u8 = 6;
    pub const READ_AT: u8 = 7;
    pub const WRITE_AT: u8 = 8;
    pub const LSEEK: u8 = 9;
    pub const STAT_PATH: u8 = 10;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Invalid {
    Header,
    Sequence,
    Descriptor,
    Range,
    Overlap,
}

fn u32_at(page: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(page[at..at + 4].try_into().unwrap())
}

/// Returns all checked descriptors before any operation may execute. Unused
/// descriptor slots must also be zero, making version/extension mistakes fail.
pub fn validate(
    page: &[u8; PAGE_BYTES],
    expected: u64,
) -> Result<([Request; MAX_OPS], usize), Invalid> {
    if page[..8] != *b"PBIO\x01\0\0\0" || page[20..32] != [0; 12] {
        return Err(Invalid::Header);
    }
    let sequence = u64::from_le_bytes(page[8..16].try_into().unwrap());
    if expected == 0 || expected > MAX_SEQUENCE || sequence != expected {
        return Err(Invalid::Sequence);
    }
    let count = u32_at(page, 16) as usize;
    if count == 0 || count > MAX_OPS {
        return Err(Invalid::Header);
    }
    let mut requests = [Request::default(); MAX_OPS];
    let mut reserved = [0usize; MAX_OPS];
    for i in 0..MAX_OPS {
        let at = HEADER_BYTES + i * DESCRIPTOR_BYTES;
        if i >= count {
            if page[at..at + DESCRIPTOR_BYTES] != [0; DESCRIPTOR_BYTES] {
                return Err(Invalid::Descriptor);
            }
            continue;
        }
        let request = Request {
            op: page[at],
            fd: page[at + 1],
            offset: u64::from_le_bytes(page[at + 8..at + 16].try_into().unwrap()),
            length: u32_at(page, at + 16) as usize,
            data: u32_at(page, at + 20) as usize,
        };
        if !fields_valid(request)
            || page[at + 2..at + 8] != [0; 6]
            || page[at + 24..at + 32] != [0; 8]
        {
            return Err(Invalid::Descriptor);
        }
        let size = reserved_len(request.op, request.length);
        let end = request.data.checked_add(size).ok_or(Invalid::Range)?;
        if request.data < DATA_OFFSET || end > PAGE_BYTES {
            return Err(Invalid::Range);
        }
        for (j, previous) in requests[..i].iter().enumerate() {
            if size != 0
                && reserved[j] != 0
                && request.data < previous.data + reserved[j]
                && previous.data < end
            {
                return Err(Invalid::Overlap);
            }
        }
        requests[i] = request;
        reserved[i] = size;
    }
    Ok((requests, count))
}

/// Per-op descriptor shape; the store still validates the fd and path.
fn fields_valid(request: Request) -> bool {
    match request.op {
        Request::READ | Request::WRITE => request.offset == 0,
        Request::READ_AT | Request::WRITE_AT => true,
        Request::OPEN => {
            request.offset & !OPEN_FLAGS_MASK == 0
                && request.offset & OPEN_ACCESS_MASK != OPEN_ACCESS_MASK
                && (1..=MAX_PATH).contains(&request.length)
        }
        Request::STAT => request.offset == 0 && request.length == NEUTRAL_STAT_BYTES,
        Request::CLOSE => request.offset == 0 && request.length == 0 && request.data == DATA_OFFSET,
        Request::GETDENTS => request.offset == 0 && (1..=DATA_BYTES).contains(&request.length),
        Request::LSEEK => request.length <= 2 && request.data == DATA_OFFSET,
        Request::STAT_PATH => {
            request.offset & !STAT_FLAGS_MASK == 0 && (1..=MAX_PATH).contains(&request.length)
        }
        _ => false,
    }
}

/// Bytes a descriptor occupies for range and overlap validation. STAT_PATH
/// reserves its fixed output window even when the path is shorter.
fn reserved_len(op: u8, length: usize) -> usize {
    match op {
        Request::STAT => NEUTRAL_STAT_BYTES,
        Request::CLOSE | Request::LSEEK => 0,
        Request::STAT_PATH => STAT_PATH_BYTES,
        _ => length,
    }
}

pub fn initialize(page: &mut [u8; PAGE_BYTES], sequence: u64, count: u32) {
    page.fill(0);
    page[..8].copy_from_slice(b"PBIO\x01\0\0\0");
    page[8..16].copy_from_slice(&sequence.to_le_bytes());
    page[16..20].copy_from_slice(&count.to_le_bytes());
}

pub fn encode_request(page: &mut [u8; PAGE_BYTES], index: usize, request: Request) {
    assert!(index < MAX_OPS && request.length <= DATA_BYTES && request.data <= PAGE_BYTES);
    let at = HEADER_BYTES + index * DESCRIPTOR_BYTES;
    page[at..at + DESCRIPTOR_BYTES].fill(0);
    page[at] = request.op;
    page[at + 1] = request.fd;
    page[at + 8..at + 16].copy_from_slice(&request.offset.to_le_bytes());
    page[at + 16..at + 20].copy_from_slice(&(request.length as u32).to_le_bytes());
    page[at + 20..at + 24].copy_from_slice(&(request.data as u32).to_le_bytes());
}

pub fn complete(page: &mut [u8; PAGE_BYTES], index: usize, status: u32, count: u32) {
    let at = HEADER_BYTES + index * DESCRIPTOR_BYTES;
    page[at + 24..at + 28].copy_from_slice(&status.to_le_bytes());
    page[at + 28..at + 32].copy_from_slice(&count.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_rejects_late_invalid_descriptors_overlap_and_replay_before_execution() {
        let mut page = [0; PAGE_BYTES];
        initialize(&mut page, 7, 2);
        let first = Request {
            op: Request::WRITE,
            fd: 3,
            length: 17,
            data: DATA_OFFSET,
            ..Request::default()
        };
        encode_request(&mut page, 0, first);
        encode_request(
            &mut page,
            1,
            Request {
                data: DATA_OFFSET + 17,
                ..first
            },
        );
        assert!(validate(&page, 7).is_ok());
        assert_eq!(validate(&page, 8), Err(Invalid::Sequence));
        encode_request(&mut page, 1, first);
        assert_eq!(validate(&page, 7), Err(Invalid::Overlap));
        encode_request(
            &mut page,
            1,
            Request {
                data: PAGE_BYTES,
                ..first
            },
        );
        assert_eq!(validate(&page, 7), Err(Invalid::Range));
        encode_request(
            &mut page,
            1,
            Request {
                data: DATA_OFFSET + 17,
                ..first
            },
        );
        page[HEADER_BYTES + DESCRIPTOR_BYTES + 2] = 1;
        assert_eq!(validate(&page, 7), Err(Invalid::Descriptor));
        initialize(&mut page, 0, 1);
        assert_eq!(validate(&page, 0), Err(Invalid::Sequence));
    }

    #[test]
    fn p2b_ops_validate_shapes_flags_ranges_and_reserved_overlap() {
        let one = |request: Request| {
            let mut page = [0; PAGE_BYTES];
            initialize(&mut page, 1, 1);
            encode_request(&mut page, 0, request);
            validate(&page, 1)
        };
        let open = Request {
            op: Request::OPEN,
            fd: DIRFD_ROOT,
            offset: OPEN_DIRECTORY,
            length: 5,
            data: DATA_OFFSET,
        };
        assert!(one(open).is_ok());
        assert_eq!(
            one(Request {
                offset: OPEN_ACCESS_MASK,
                ..open
            }),
            Err(Invalid::Descriptor)
        );
        assert_eq!(
            one(Request {
                offset: 1 << 7,
                ..open
            }),
            Err(Invalid::Descriptor)
        );
        assert_eq!(one(Request { length: 0, ..open }), Err(Invalid::Descriptor));
        assert_eq!(
            one(Request {
                length: MAX_PATH + 1,
                ..open
            }),
            Err(Invalid::Descriptor)
        );
        assert_eq!(
            one(Request {
                data: PAGE_BYTES - 4,
                ..open
            }),
            Err(Invalid::Range)
        );

        let stat = Request {
            op: Request::STAT,
            fd: WIRE_FD_BASE,
            length: NEUTRAL_STAT_BYTES,
            data: DATA_OFFSET,
            ..Request::default()
        };
        assert!(one(stat).is_ok());
        assert_eq!(
            one(Request {
                length: NEUTRAL_STAT_BYTES - 1,
                ..stat
            }),
            Err(Invalid::Descriptor)
        );

        let close = Request {
            op: Request::CLOSE,
            fd: WIRE_FD_BASE,
            data: DATA_OFFSET,
            ..Request::default()
        };
        assert!(one(close).is_ok());
        assert_eq!(
            one(Request {
                data: DATA_OFFSET + 1,
                ..close
            }),
            Err(Invalid::Descriptor)
        );

        let seek = Request {
            op: Request::LSEEK,
            fd: WIRE_FD_BASE,
            length: 2,
            data: DATA_OFFSET,
            ..Request::default()
        };
        assert!(one(seek).is_ok());
        assert_eq!(one(Request { length: 3, ..seek }), Err(Invalid::Descriptor));
        assert_eq!(
            one(Request {
                data: DATA_OFFSET + 1,
                ..seek
            }),
            Err(Invalid::Descriptor)
        );

        let getdents = Request {
            op: Request::GETDENTS,
            fd: WIRE_FD_BASE,
            length: DATA_BYTES,
            data: DATA_OFFSET,
            ..Request::default()
        };
        assert!(one(getdents).is_ok());
        assert_eq!(
            one(Request {
                length: 0,
                ..getdents
            }),
            Err(Invalid::Descriptor)
        );
        assert_eq!(
            one(Request {
                data: PAGE_BYTES - 1,
                ..getdents
            }),
            Err(Invalid::Range)
        );

        let stat_path = Request {
            op: Request::STAT_PATH,
            fd: DIRFD_ROOT,
            length: 1,
            data: DATA_OFFSET,
            ..Request::default()
        };
        assert!(one(stat_path).is_ok());
        assert_eq!(
            one(Request {
                offset: 2,
                ..stat_path
            }),
            Err(Invalid::Descriptor)
        );
        assert_eq!(
            one(Request {
                data: PAGE_BYTES - STAT_PATH_BYTES + 1,
                ..stat_path
            }),
            Err(Invalid::Range)
        );

        let mut page = [0; PAGE_BYTES];
        initialize(&mut page, 1, 2);
        encode_request(&mut page, 0, stat_path);
        encode_request(
            &mut page,
            1,
            Request {
                data: DATA_OFFSET + 70,
                ..stat
            },
        );
        assert_eq!(validate(&page, 1), Err(Invalid::Overlap));

        let mut page = [0; PAGE_BYTES];
        initialize(&mut page, 1, 1);
        encode_request(
            &mut page,
            0,
            Request {
                op: 11,
                ..Request::default()
            },
        );
        assert_eq!(validate(&page, 1), Err(Invalid::Descriptor));
    }
}
