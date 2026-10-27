//! Static program declarations, checked by host tools and user-side loaders.
//! These self-declarations confer no authority and create no kernel persona.

pub const NOTE_TYPE: u32 = 0x5052_494d;
pub const NATIVE: u32 = 0;
pub const POSIX_STATIC: u32 = 1;
pub const LAYOUT: u32 = 1;
pub const ADAPTER_MAJOR: u32 = 1;
pub const ADAPTER_MINOR: u32 = 0;

/// ELF note words for x86_64 little-endian output, aligned to four bytes.
pub const fn note(persona: u32, adapter_major: u32) -> [u32; 11] {
    [
        6,
        24,
        NOTE_TYPE,
        0x6d69_7250,
        0x61,
        LAYOUT,
        super::native_abi::MAJOR as u32,
        super::native_abi::MINOR as u32,
        persona,
        adapter_major,
        ADAPTER_MINOR,
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContractError {
    Malformed,
    Missing,
    Duplicate,
    Incompatible,
}

fn word(bytes: &[u8], offset: usize) -> Result<u32, ContractError> {
    let end = offset.checked_add(4).ok_or(ContractError::Malformed)?;
    let bytes = bytes.get(offset..end).ok_or(ContractError::Malformed)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn qword(bytes: &[u8], offset: usize) -> Result<usize, ContractError> {
    let lo = u64::from(word(bytes, offset)?);
    let hi = u64::from(word(
        bytes,
        offset.checked_add(4).ok_or(ContractError::Malformed)?,
    )?);
    usize::try_from(lo | hi << 32).map_err(|_| ContractError::Malformed)
}

/// Reject absent, duplicate and malformed declarations, including unrelated
/// malformed notes. Offsets are bounded before arithmetic or indexing.
pub fn validate(elf: &[u8], persona: u32, adapter_major: u32) -> Result<(), ContractError> {
    if elf.len() < 64
        || elf.get(..7) != Some(b"\x7fELF\x02\x01\x01")
        || u16::from_le_bytes([elf[18], elf[19]]) != 62
        || u16::from_le_bytes([elf[54], elf[55]]) != 56
    {
        return Err(ContractError::Malformed);
    }
    let phoff = qword(elf, 32)?;
    let count = usize::from(u16::from_le_bytes([elf[56], elf[57]]));
    if phoff > elf.len() || count > (elf.len() - phoff) / 56 {
        return Err(ContractError::Malformed);
    }
    let mut found = false;
    for index in 0..count {
        let ph = phoff + index * 56;
        if word(elf, ph)? != 4 {
            continue;
        } // PT_NOTE
        let start = qword(elf, ph + 8)?;
        let size = qword(elf, ph + 32)?;
        if start > elf.len() || size > elf.len() - start {
            return Err(ContractError::Malformed);
        }
        let notes = &elf[start..start + size];
        let mut at = 0;
        while at < notes.len() {
            if notes.len() - at < 12 {
                return Err(ContractError::Malformed);
            }
            let namesz = word(notes, at)? as usize;
            let descsz = word(notes, at + 4)? as usize;
            let kind = word(notes, at + 8)?;
            at += 12;
            let name_pad = namesz.checked_add(3).ok_or(ContractError::Malformed)? & !3;
            let desc_pad = descsz.checked_add(3).ok_or(ContractError::Malformed)? & !3;
            if name_pad > notes.len() - at {
                return Err(ContractError::Malformed);
            }
            let is_prima =
                kind == NOTE_TYPE && namesz == 6 && &notes[at..at + namesz] == b"Prima\0";
            at += name_pad;
            if desc_pad > notes.len() - at {
                return Err(ContractError::Malformed);
            }
            if is_prima {
                if found {
                    return Err(ContractError::Duplicate);
                }
                found = true;
                if descsz != 24
                    || word(notes, at)? != LAYOUT
                    || u64::from(word(notes, at + 4)?) != super::native_abi::MAJOR
                    || u64::from(word(notes, at + 8)?) > super::native_abi::MINOR
                    || persona > POSIX_STATIC
                    || word(notes, at + 12)? != persona
                    || word(notes, at + 16)? != adapter_major
                    || word(notes, at + 20)? > ADAPTER_MINOR
                {
                    return Err(ContractError::Incompatible);
                }
            }
            at += desc_pad;
        }
    }
    if found {
        Ok(())
    } else {
        Err(ContractError::Missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> [u8; 208] {
        let mut bytes = [0; 208];
        bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
        bytes[18] = 62;
        bytes[32] = 64;
        bytes[54] = 56;
        bytes[56] = 1;
        bytes[64] = 4;
        bytes[72] = 120;
        bytes[96] = 44;
        for (index, word) in note(POSIX_STATIC, ADAPTER_MAJOR).iter().enumerate() {
            bytes[120 + index * 4..124 + index * 4].copy_from_slice(&word.to_le_bytes());
        }
        bytes
    }
    #[test]
    fn program_declarations_reject_wrong_versions_persona_and_note_bounds() {
        let bytes = image();
        assert_eq!(validate(&bytes, POSIX_STATIC, 1), Ok(()));
        assert_eq!(
            validate(&bytes, NATIVE, 0),
            Err(ContractError::Incompatible)
        );
        for at in [140, 144, 148, 152, 156, 160] {
            let mut wrong = bytes;
            wrong[at] = 9;
            assert_eq!(
                validate(&wrong, POSIX_STATIC, 1),
                Err(ContractError::Incompatible)
            );
        }
        let mut missing = bytes;
        missing[64] = 1;
        assert_eq!(
            validate(&missing, POSIX_STATIC, 1),
            Err(ContractError::Missing)
        );
        let mut duplicate = bytes;
        duplicate[164..208].copy_from_slice(&bytes[120..164]);
        duplicate[96] = 88;
        assert_eq!(
            validate(&duplicate, POSIX_STATIC, 1),
            Err(ContractError::Duplicate)
        );
        for length in [0, 63, 119, 163] {
            assert!(validate(&bytes[..length], POSIX_STATIC, 1).is_err());
        }
        for at in [32, 72, 96, 120, 124] {
            let mut overflow = bytes;
            overflow[at..at + 4].fill(0xff);
            assert!(validate(&overflow, POSIX_STATIC, 1).is_err());
        }
    }
}
