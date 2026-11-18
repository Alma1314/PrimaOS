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

/// P3 version-2 async session tags. The slot is in bits 32..39; the doorbell
/// also carries the low 8 bits of the generation in 40..47 and `tail` in the
/// low 32 bits. `ACK` carries the slot in bits 8..15 and the status below it.
pub const ASYNC_OPEN: u64 = 0x5f30_0000_0000_0000;
pub const ASYNC_ACK: u64 = 0x5f31_0000_0000_0000;
pub const ASYNC_DOORBELL: u64 = 0x5f32_0000_0000_0000;
pub const ASYNC_COMPLETE: u64 = 0x5f33_0000_0000_0000;
pub const ASYNC_CANCEL: u64 = 0x5f34_0000_0000_0000;
pub const ASYNC_CLOSE: u64 = 0x5f35_0000_0000_0000;
/// Proxy-scoped v2 sessions use the next six tags, so a direct client's
/// Task-slot encoding can never be mistaken for a proxy index. The offset is
/// applied to the 16-bit tag field, not the payload bits.
pub const ASYNC_PROXY_OFFSET: u64 = 0x0006_0000_0000_0000;
pub const ASYNC_CREDITS: usize = crate::async_batch::CREDITS as usize;
/// The control page followed by one page per credit.
pub const ASYNC_PAGES: usize = ASYNC_CREDITS + 1;
/// Control-page header the Store validates after every producer message. Only
/// this prefix is read on the data path; the rest of the page stays zero.
pub const ASYNC_HEADER_BYTES: usize = 64;
pub const ASYNC_MAGIC: [u8; 4] = *b"PBQ2";
pub const ASYNC_VERSION: u8 = 2;
pub const ASYNC_TAIL_OFFSET: usize = 8;
pub const ASYNC_HEAD_OFFSET: usize = 16;
pub const ASYNC_GENERATION_OFFSET: usize = 24;
pub const ASYNC_GENERATION_SHIFT: u64 = 40;
pub const ASYNC_GENERATION_MASK: u64 = 0xff;

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
    validate_inner(page, expected, true)
}

/// P3 v2 data pages carry the full `ordinal + 1`, so the frozen v1 32-bit
/// sequence bound does not apply. Magic and reserved-field checks still do.
pub fn validate_version_2(
    page: &[u8; PAGE_BYTES],
    expected: u64,
) -> Result<([Request; MAX_OPS], usize), Invalid> {
    validate_inner(page, expected, false)
}

fn validate_inner(
    page: &[u8; PAGE_BYTES],
    expected: u64,
    bounded: bool,
) -> Result<([Request; MAX_OPS], usize), Invalid> {
    if page[..8] != *b"PBIO\x01\0\0\0" || page[20..32] != [0; 12] {
        return Err(Invalid::Header);
    }
    let sequence = u64::from_le_bytes(page[8..16].try_into().unwrap());
    if (bounded && (expected == 0 || expected > MAX_SEQUENCE)) || sequence != expected {
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

/// Producer-written control-page fields. `head` and `generation` are read back
/// for diagnostics only; the Store's session state remains authoritative.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AsyncHeader {
    pub tail: u64,
    pub head: u64,
    pub generation: u64,
}

pub fn initialize_async(header: &mut [u8; ASYNC_HEADER_BYTES], generation: u64) {
    header.fill(0);
    header[..4].copy_from_slice(&ASYNC_MAGIC);
    header[4] = ASYNC_VERSION;
    header[5] = ASYNC_CREDITS as u8;
    header[ASYNC_GENERATION_OFFSET..ASYNC_GENERATION_OFFSET + 8]
        .copy_from_slice(&generation.to_le_bytes());
}

/// Header shape and reserved bits only. The bounded window is checked against
/// the Store's own `head`, never against a producer-writable copy of it.
pub fn validate_async(header: &[u8; ASYNC_HEADER_BYTES]) -> Result<AsyncHeader, Invalid> {
    if header[..4] != ASYNC_MAGIC
        || header[4] != ASYNC_VERSION
        || header[5] != ASYNC_CREDITS as u8
        || header[6..8] != [0; 2]
        || header[32..ASYNC_HEADER_BYTES] != [0; ASYNC_HEADER_BYTES - 32]
    {
        return Err(Invalid::Header);
    }
    Ok(AsyncHeader {
        tail: u64::from_le_bytes(
            header[ASYNC_TAIL_OFFSET..ASYNC_TAIL_OFFSET + 8]
                .try_into()
                .unwrap(),
        ),
        head: u64::from_le_bytes(
            header[ASYNC_HEAD_OFFSET..ASYNC_HEAD_OFFSET + 8]
                .try_into()
                .unwrap(),
        ),
        generation: u64::from_le_bytes(
            header[ASYNC_GENERATION_OFFSET..ASYNC_GENERATION_OFFSET + 8]
                .try_into()
                .unwrap(),
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_2_pages_accept_sequences_past_the_v1_bound() {
        let mut page = [0; PAGE_BYTES];
        initialize(&mut page, MAX_SEQUENCE + 1, 1);
        encode_request(
            &mut page,
            0,
            Request {
                op: Request::CLOSE,
                fd: 3,
                data: DATA_OFFSET,
                ..Request::default()
            },
        );
        assert_eq!(validate(&page, MAX_SEQUENCE + 1), Err(Invalid::Sequence));
        assert!(validate_version_2(&page, MAX_SEQUENCE + 1).is_ok());
        assert_eq!(
            validate_version_2(&page, MAX_SEQUENCE),
            Err(Invalid::Sequence)
        );
    }

    #[test]
    fn async_control_header_round_trips_and_rejects_every_reserved_field() {
        let mut header = [0u8; ASYNC_HEADER_BYTES];
        initialize_async(&mut header, 9);
        assert_eq!(
            validate_async(&header),
            Ok(AsyncHeader {
                tail: 0,
                head: 0,
                generation: 9
            })
        );
        // The producer owns only `tail`; the Store writes `head` afterwards.
        header[ASYNC_TAIL_OFFSET..ASYNC_TAIL_OFFSET + 8]
            .copy_from_slice(&(ASYNC_CREDITS as u64).to_le_bytes());
        header[ASYNC_HEAD_OFFSET..ASYNC_HEAD_OFFSET + 8].copy_from_slice(&1u64.to_le_bytes());
        assert_eq!(validate_async(&header).unwrap().tail, ASYNC_CREDITS as u64);

        for (at, bad) in [(4, 1), (5, 0), (6, 1), (32, 1), (63, 1)] {
            let mut corrupt = header;
            corrupt[at] = bad;
            assert_eq!(
                validate_async(&corrupt),
                Err(Invalid::Header),
                "offset {at}"
            );
        }
        let mut magic = header;
        magic[0] = b'X';
        assert_eq!(validate_async(&magic), Err(Invalid::Header));
        let mut short = [0u8; ASYNC_HEADER_BYTES];
        short[..4].copy_from_slice(&ASYNC_MAGIC);
        short[4] = ASYNC_VERSION;
        short[5] = ASYNC_CREDITS as u8;
        assert!(validate_async(&short).is_ok());
    }

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
