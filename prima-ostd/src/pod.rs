// SPDX-License-Identifier: MPL-2.0
// Design adapted from Asterinas ostd-pod; see docs/architecture/asterinas-provenance.md.

//! Audited, padding-free values that can cross a byte boundary.
//!
//! Unlike Asterinas' zerocopy-derived implementation, the small set of PrimaOS
//! hardware types is audited here without a derive dependency. Byte views borrow
//! their input; decoding copies from a length-checked, possibly unaligned slice.
//! Native representation is not a portable wire format: keep explicit endian
//! encoding at ABI boundaries.

/// Plain old data: every byte is initialized and every bit pattern is valid.
///
/// # Safety
///
/// Implementors must be `Copy`, have no padding or uninitialized bytes, accept
/// every bit pattern of their size, and contain no references, pointers, or
/// interior mutability. These properties must hold recursively for all fields.
/// `repr(C)` alone is insufficient; layout changes require another audit.
///
/// Invalid representations and padded types are rejected unless somebody adds
/// an incorrect unsafe implementation:
///
/// ```compile_fail
/// prima_ostd::mm::bytes_of(&true);
/// ```
/// ```compile_fail
/// #[repr(C)]
/// #[derive(Clone, Copy)]
/// struct Padded { tag: u8, value: u64 }
/// prima_ostd::mm::bytes_of(&Padded { tag: 0, value: 0 });
/// ```
/// ```compile_fail
/// prima_ostd::mm::bytes_of(&&42u64);
/// ```
/// ```compile_fail
/// let bytes;
/// {
///     let value = 42u64;
///     bytes = prima_ostd::mm::bytes_of(&value);
/// }
/// assert_eq!(bytes.len(), 8);
/// ```
pub unsafe trait Pod: Copy + 'static {
    /// Copies exactly one native-layout value; wrong lengths return `None`.
    /// No alignment is required and no reference into the byte slice escapes.
    fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != size_of::<Self>() {
            return None;
        }
        // SAFETY: length is checked, read_unaligned accepts byte alignment,
        // and Pod guarantees validity of every fully initialized bit pattern.
        Some(unsafe { core::ptr::read_unaligned(bytes.as_ptr().cast::<Self>()) })
    }
}

macro_rules! integer_pod {
    ($($ty:ty),* $(,)?) => {$(
        // SAFETY: integer types have no padding and accept every bit pattern.
        unsafe impl Pod for $ty {}
    )*};
}
integer_pod!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

// SAFETY: arrays have no inter-element padding; each element satisfies Pod.
unsafe impl<T: Pod, const N: usize> Pod for [T; N] {}

/// Borrows the native bytes of an audited value for exactly its lifetime.
#[inline]
pub fn bytes_of<T: Pod>(value: &T) -> &[u8] {
    // SAFETY: Pod excludes padding/uninitialized bytes and interior mutation.
    // The returned slice borrows value and never extends its lifetime.
    unsafe { core::slice::from_raw_parts(core::ptr::from_ref(value).cast(), size_of::<T>()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoding_checks_length_and_accepts_unaligned_input() {
        let mut storage = [0u64; 2];
        // Use the audited integer array's byte view to choose an unaligned input.
        storage[0] = 0x1234_5678_90ab_cdef;
        storage[1] = 0xfedc_ba09_8765_4321;
        let raw = bytes_of(&storage);
        let input = &raw[1..9];
        assert_ne!(input.as_ptr() as usize % align_of::<u64>(), 0);
        assert_eq!(
            u64::from_bytes(input),
            Some(u64::from_ne_bytes(input.try_into().unwrap()))
        );
        for len in 0..=16 {
            assert_eq!(u64::from_bytes(&raw[..len]).is_some(), len == 8);
        }
    }

    #[test]
    fn integer_arrays_and_zero_sized_arrays_round_trip() {
        for value in 0..=u8::MAX {
            assert_eq!(u8::from_bytes(&[value]), Some(value));
            assert_eq!(i8::from_bytes(&[value]), Some(value as i8));
        }
        let words = [0, 1, u64::MAX];
        assert_eq!(<[u64; 3]>::from_bytes(bytes_of(&words)), Some(words));
        assert_eq!(<[u64; 0]>::from_bytes(&[]), Some([]));
        assert!(bytes_of(&[0u64; 0]).is_empty());
    }
}
