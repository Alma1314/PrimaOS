#![forbid(unsafe_code)]
#![allow(dead_code)]

//! EXO-2C/2D host-side init machine-code generator.
//!
//! `generate` produces the two-page x86-64 code image that used to be built
//! inside `kernel/src/user.rs`. The kernel treats it as an opaque ramdisk ELF;
//! capability handles and manifest layout are read from the boot-info page
//! (`r15`), so this crate no longer mirrors kernel capability handles.

mod code;
pub mod rights;
mod syscall;

pub use code::{USER_CODE_LEN, USER_CODE_VA, USER_CODE2_VA, generate};

/// Manifest indices consumed by the generated init image.
///
/// `alpha_index` and `servicemgr_index` are indices into the generic boot
/// manifest/`BootInfo` entry array. The kernel does not know these service roles;
/// the build tool selects the entries by their user-space `kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub alpha_index: usize,
    pub servicemgr_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::*;
    use prima_abi::boot_abi;

    fn manifest() -> Manifest {
        Manifest {
            alpha_index: 1,
            servicemgr_index: 3,
        }
    }

    #[test]
    fn generated_image_reads_boot_info_and_is_stable() {
        let code = generate(&manifest());
        assert_eq!(code.len(), USER_CODE_LEN);
        // COW prologue is still the original immediate form.
        assert_eq!(&code[0..2], &[0x48, 0xBA]);
        assert_eq!(&code[2..10], &USER_X_VA.to_le_bytes());
        // Capability loads now come from `[r15 + boot-info offset]`.
        assert_eq!(&code[222..226], &[0x49, 0x8B, 0xBC, 0x27]);
        assert_eq!(
            u32::from_le_bytes(code[226..230].try_into().unwrap()),
            boot_abi::OFF_IMAGE_CAP
        );
        for (at, receipt) in [
            (699, 168u32),
            (728, 168),
            (796, 168),
            (835, 200),
            (874, 200),
            (972, 240),
            (1153, 240),
            (1182, 240),
            (1300, 296),
            (1448, 296),
        ] {
            assert_eq!(&code[at..at + 3], &[0x48, 0x8B, 0xB9]);
            assert_eq!(&code[at + 3..at + 7], &receipt.to_le_bytes());
            assert_eq!(&code[at + 7..at + 10], &[0x90; 3]);
        }
        let mut hash = 0xcbf2_9ce4_8422_2325u64;
        for byte in code {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        assert_eq!(hash, 0x1420_d9dc_85ca_1c5e);
    }
}
