// SPDX-License-Identifier: MPL-2.0

//! Adapted from Asterinas `ostd/libs/align_ext` at ceb65e3f850096d30283137845ac01ad51dfcdaf.
//! PrimaOS adds fallible alignment for untrusted boot memory ranges.
//! See `docs/architecture/asterinas-provenance.md` for provenance and modifications.

/// An extension trait for Rust integer types, including `u8`, `u16`, `u32`,
/// `u64`, and `usize`, to provide methods to make integers aligned to a
/// power of two.
pub trait AlignExt: Sized {
    /// Aligns upward, returning `None` for overflow or an alignment that is not
    /// a power of two greater than or equal to 2. This never wraps or panics.
    fn checked_align_up(self, power_of_two: Self) -> Option<Self>;

    /// Returns to the smallest number that is greater than or equal to
    /// `self` and is a multiple of the given power of two.
    ///
    /// The method panics if `power_of_two` is not a
    /// power of two or is smaller than 2 or the calculation overflows
    /// because `self` is too large.
    ///
    /// # Examples
    ///
    /// ```
    /// use prima_ostd::align::AlignExt;
    /// assert_eq!(12usize.align_up(2), 12);
    /// assert_eq!(12usize.align_up(4), 12);
    /// assert_eq!(12usize.align_up(8), 16);
    /// assert_eq!(12usize.align_up(16), 16);
    /// ```
    fn align_up(self, power_of_two: Self) -> Self;

    /// Returns to the greatest number that is smaller than or equal to
    /// `self` and is a multiple of the given power of two.
    ///
    /// The method panics if `power_of_two` is not a
    /// power of two or is smaller than 2.
    ///
    /// # Examples
    ///
    /// ```
    /// use prima_ostd::align::AlignExt;
    /// assert_eq!(12usize.align_down(2), 12);
    /// assert_eq!(12usize.align_down(4), 12);
    /// assert_eq!(12usize.align_down(8), 8);
    /// assert_eq!(12usize.align_down(16), 0);
    /// ```
    fn align_down(self, power_of_two: Self) -> Self;
}

macro_rules! impl_align_ext {
    ($( $uint_type:ty ),+,) => {
        $(
            impl AlignExt for $uint_type {
                #[inline]
                fn checked_align_up(self, align: Self) -> Option<Self> {
                    if !align.is_power_of_two() || align < 2 {
                        return None;
                    }
                    self.checked_add(align - 1).map(|value| value & !(align - 1))
                }

                #[inline]
                fn align_up(self, align: Self) -> Self {
                    self.checked_align_up(align).expect("invalid alignment or overflow")
                }

                #[inline]
                fn align_down(self, align: Self) -> Self {
                    assert!(align.is_power_of_two() && align >= 2);
                    self & !(align - 1)
                }
            }
        )*
    }
}

impl_align_ext! {
    u8,
    u16,
    u32,
    u64,
    usize,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn align_up() {
        let input_ns = [0usize, 1, 2, 9, 15, 21, 32, 47, 50];
        let input_as = [2usize, 2, 2, 2, 4, 4, 8, 8, 8];
        let output_ns = [0usize, 2, 2, 10, 16, 24, 32, 48, 56];

        for i in 0..input_ns.len() {
            let n = input_ns[i];
            let a = input_as[i];
            let n2 = output_ns[i];
            assert!(n.align_up(a) == n2);
        }
    }

    #[test]
    fn align_down() {
        let input_ns = [0usize, 1, 2, 9, 15, 21, 32, 47, 50];
        let input_as = [2usize, 2, 2, 2, 4, 4, 8, 8, 8];
        let output_ns = [0usize, 0, 2, 8, 12, 20, 32, 40, 48];

        for i in 0..input_ns.len() {
            let n = input_ns[i];
            let a = input_as[i];
            let n2 = output_ns[i];
            assert!(n.align_down(a) == n2);
        }
    }
}

#[cfg(test)]
mod checked_tests {
    use super::AlignExt;

    #[test]
    fn checked_alignment_matches_widened_arithmetic_for_every_u8_input() {
        for value in 0..=u8::MAX {
            for align in 0..=u8::MAX {
                let expected = if align >= 2 && align.is_power_of_two() {
                    let rounded =
                        (u16::from(value) + u16::from(align) - 1) & !(u16::from(align) - 1);
                    u8::try_from(rounded).ok()
                } else {
                    None
                };
                assert_eq!(value.checked_align_up(align), expected);
                if align >= 2 && align.is_power_of_two() {
                    assert_eq!(
                        u16::from(value.align_down(align)),
                        u16::from(value) / u16::from(align) * u16::from(align)
                    );
                }
            }
        }
    }

    #[test]
    fn every_integer_width_preserves_top_boundary_and_rejects_overflow() {
        macro_rules! check {
            ($($ty:ty),*) => {$(
                assert_eq!((<$ty>::MAX - 4095).checked_align_up(4096), Some(<$ty>::MAX - 4095));
                assert_eq!(<$ty>::MAX.checked_align_up(4096), None);
                assert_eq!(0 as $ty, (0 as $ty).align_up(4096));
            )*};
        }
        check!(u16, u32, u64, usize);
        assert_eq!(u8::MAX.checked_align_up(2), None);
    }

    #[test]
    fn infallible_helpers_reject_invalid_alignment_and_overflow() {
        for align in [0u64, 1, 3, u64::MAX] {
            assert!(std::panic::catch_unwind(|| 0u64.align_up(align)).is_err());
            assert!(std::panic::catch_unwind(|| 0u64.align_down(align)).is_err());
        }
        assert!(std::panic::catch_unwind(|| u64::MAX.align_up(4096)).is_err());
    }
}
