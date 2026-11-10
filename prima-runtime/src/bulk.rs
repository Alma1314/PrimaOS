//! Versioned, bounded file batches. Callers snapshot shared memory before
//! validation; this codec never treats a descriptor or a local fd as authority.

pub const OPEN: u64 = 0x5f10_0000_0000_0000;
pub const OPEN_REPLY: u64 = 0x5f11_0000_0000_0000;
pub const SUBMIT: u64 = 0x5f12_0000_0000_0000;
pub const COMPLETE: u64 = 0x5f13_0000_0000_0000;
pub const CLOSE: u64 = 0x5f14_0000_0000_0000;
pub const PAGE_BYTES: usize = 4096;
pub const MAX_OPS: usize = 8;
pub const HEADER_BYTES: usize = 32;
pub const DESCRIPTOR_BYTES: usize = 32;
pub const DATA_OFFSET: usize = HEADER_BYTES + MAX_OPS * DESCRIPTOR_BYTES;
pub const DATA_BYTES: usize = PAGE_BYTES - DATA_OFFSET;
pub const SERVER_VA: u64 = 0x0000_0b00_0040_0000;
pub const CLIENT_VA: u64 = 0x0000_0c00_0200_0000;
pub const MAX_SEQUENCE: u64 = u32::MAX as u64;

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
    pub const READ_AT: u8 = 7;
    pub const WRITE_AT: u8 = 8;
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
        if !matches!(request.op, 1 | 2 | 7 | 8)
            || page[at + 2..at + 8] != [0; 6]
            || page[at + 24..at + 32] != [0; 8]
            || (matches!(request.op, 1 | 2) && request.offset != 0)
        {
            return Err(Invalid::Descriptor);
        }
        let end = request
            .data
            .checked_add(request.length)
            .ok_or(Invalid::Range)?;
        if request.data < DATA_OFFSET || end > PAGE_BYTES {
            return Err(Invalid::Range);
        }
        for previous in &requests[..i] {
            if request.length != 0
                && previous.length != 0
                && request.data < previous.data + previous.length
                && previous.data < end
            {
                return Err(Invalid::Overlap);
            }
        }
        requests[i] = request;
    }
    Ok((requests, count))
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
}
