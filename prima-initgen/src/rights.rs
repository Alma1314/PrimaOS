//! Minimal rights model for the host-side init code generator.
//!
//! The generator only needs the numeric bit positions; the kernel remains the
//! authority that enforces these rights.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Right {
    Derive,
    SegMapR,
    SegMapW,
    SegMapX,
    SegDestroy,
    TaskSend,
    TaskManageVSpace,
    TaskDestroy,
    NsResolve,
    NsBind,
    NsUnbind,
    NsMount,
    NsUnmount,
    NsDestroy,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rights(u32);

impl Rights {
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    pub const fn from_rights(rights: &[Right]) -> Self {
        let mut bits = 0u32;
        let mut index = 0;
        while index < rights.len() {
            bits |= 1 << (rights[index] as u32);
            index += 1;
        }
        Self(bits)
    }

    pub const fn bits(self) -> u32 {
        self.0
    }
}
