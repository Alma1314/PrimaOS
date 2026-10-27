//! Bounded file extensions shared by the live Store and host protocol tests.
//! Session authentication and HandleId selection remain in the service.

use prima_runtime::{
    protocol::{
        SERVICE_FILE_AT_CHUNK, SERVICE_FILE_AT_COUNT_SHIFT, SERVICE_FILE_AT_DATA_SHIFT,
        SERVICE_FILE_AT_OFFSET_MASK, SERVICE_FILE_OP_READ_AT, SERVICE_FILE_OP_RESIZE,
        SERVICE_FILE_OP_WRITE_AT, SERVICE_FILE_WRITE_COUNT_SHIFT,
    },
    vfs::{HandleId, OpenFiles, Storage, VfsError},
};

// The C adapter saturates larger offsets at this sentinel; it must stay EOF.
const _: () = assert!(prima_runtime::vfs::CONTENT_CAPACITY < SERVICE_FILE_AT_OFFSET_MASK as usize);

pub fn request<S: Storage, const N: usize>(
    files: &OpenFiles<N>,
    storage: &mut S,
    id: HandleId,
    op: u64,
    arg: u32,
) -> Result<u64, VfsError> {
    let arg = u64::from(arg);
    if op == SERVICE_FILE_OP_RESIZE {
        return files.resize(storage, id, arg).map(|()| 0);
    }
    if op != SERVICE_FILE_OP_READ_AT && op != SERVICE_FILE_OP_WRITE_AT {
        return Err(VfsError::Invalid);
    }
    let count = ((arg >> SERVICE_FILE_AT_COUNT_SHIFT) & 3) as usize;
    if count > SERVICE_FILE_AT_CHUNK || arg & 0xc000 != 0 {
        return Err(VfsError::Invalid);
    }
    let offset = arg & SERVICE_FILE_AT_OFFSET_MASK;
    let mut bytes = [0u8; SERVICE_FILE_AT_CHUNK];
    if op == SERVICE_FILE_OP_READ_AT {
        if arg >> SERVICE_FILE_AT_DATA_SHIFT != 0 {
            return Err(VfsError::Invalid);
        }
        let count = files.read_at(storage, id, offset, &mut bytes[..count])?;
        Ok(((count as u64) << SERVICE_FILE_WRITE_COUNT_SHIFT)
            | u64::from(u16::from_le_bytes(bytes)))
    } else {
        bytes.copy_from_slice(&((arg >> SERVICE_FILE_AT_DATA_SHIFT) as u16).to_le_bytes());
        files
            .write_at(storage, id, offset, &bytes[..count])
            .map(|count| count as u64)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use prima_runtime::vfs::{self, AccessMode, OpenFlags, ROOT, SeekFrom};
    use std::{vec, vec::Vec};

    struct Memory(Vec<u8>);
    impl Storage for Memory {
        fn read(&mut self, offset: usize, out: &mut [u8]) -> bool {
            let Some(bytes) = self.0.get(offset..offset + out.len()) else {
                return false;
            };
            out.copy_from_slice(bytes);
            true
        }
        fn write(&mut self, offset: usize, data: &[u8]) -> bool {
            let Some(bytes) = self.0.get_mut(offset..offset + data.len()) else {
                return false;
            };
            bytes.copy_from_slice(data);
            true
        }
    }

    #[test]
    fn live_extension_protocol_preserves_cursor_and_rejects_malformed_requests() {
        let mut storage = Memory(vec![0; 4088]);
        vfs::mount(&mut storage).unwrap();
        let mut files = OpenFiles::<2>::new();
        let (id, _) = files
            .open(
                &mut storage,
                ROOT,
                b"/tmp/ext",
                OpenFlags {
                    access: AccessMode::ReadWrite,
                    create: true,
                    ..OpenFlags::default()
                },
            )
            .unwrap();
        files.write(&mut storage, id, b"abcd").unwrap();
        let arg = 1
            | (2 << SERVICE_FILE_AT_COUNT_SHIFT)
            | (u32::from(u16::from_le_bytes(*b"XY")) << SERVICE_FILE_AT_DATA_SHIFT);
        assert_eq!(
            request(&files, &mut storage, id, SERVICE_FILE_OP_WRITE_AT, arg),
            Ok(2)
        );
        let read = 1 | (2 << SERVICE_FILE_AT_COUNT_SHIFT);
        assert_eq!(
            request(&files, &mut storage, id, SERVICE_FILE_OP_READ_AT, read),
            Ok((2 << SERVICE_FILE_WRITE_COUNT_SHIFT) | u64::from(u16::from_le_bytes(*b"XY")))
        );
        assert_eq!(
            request(&files, &mut storage, id, SERVICE_FILE_OP_RESIZE, 8),
            Ok(0)
        );
        assert_eq!(files.seek(&mut storage, id, SeekFrom::Current(0)), Ok(4));
        assert_eq!(
            request(
                &files,
                &mut storage,
                id,
                SERVICE_FILE_OP_READ_AT,
                4095 | (2 << SERVICE_FILE_AT_COUNT_SHIFT)
            ),
            Ok(0)
        );
        assert_eq!(
            request(&files, &mut storage, id, SERVICE_FILE_OP_WRITE_AT, 4095),
            Ok(0)
        );
        let before = storage.0.clone();
        for arg in [3 << SERVICE_FILE_AT_COUNT_SHIFT, 0x4000, 0x8000, 0xffff] {
            for op in [SERVICE_FILE_OP_READ_AT, SERVICE_FILE_OP_WRITE_AT] {
                assert_eq!(
                    request(&files, &mut storage, id, op, arg),
                    Err(VfsError::Invalid)
                );
            }
        }
        assert_eq!(
            request(
                &files,
                &mut storage,
                id,
                SERVICE_FILE_OP_READ_AT,
                1 << SERVICE_FILE_AT_DATA_SHIFT
            ),
            Err(VfsError::Invalid)
        );
        assert_eq!(
            request(&files, &mut storage, id, SERVICE_FILE_OP_RESIZE, u32::MAX),
            Err(VfsError::NoSpace)
        );
        assert_eq!(
            request(
                &files,
                &mut storage,
                id,
                SERVICE_FILE_OP_WRITE_AT,
                4095 | (1 << SERVICE_FILE_AT_COUNT_SHIFT)
            ),
            Err(VfsError::NoSpace)
        );
        assert_eq!(storage.0, before);
        files.set_owner(1).unwrap();
        for op in [
            SERVICE_FILE_OP_READ_AT,
            SERVICE_FILE_OP_WRITE_AT,
            SERVICE_FILE_OP_RESIZE,
        ] {
            assert_eq!(
                request(&files, &mut storage, id, op, 0),
                Err(VfsError::BadHandle)
            );
        }
        files.set_owner(0).unwrap();
        files.close_all(&mut storage).unwrap();
    }
}
