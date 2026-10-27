use crate::time::Duration;
pub use super::unsupported::{SystemTime, UNIX_EPOCH};

unsafe extern "C" {
    fn __prima_std_ticks() -> u64;
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Instant(Duration);
impl Instant {
    pub fn now() -> Self {
        // SAFETY: hook has no pointers; native entry initializes the clock.
        let ticks = unsafe { __prima_std_ticks() };
        Self(Duration::new(ticks / 100, ((ticks % 100) * 10_000_000) as u32))
    }
    pub fn checked_sub_instant(&self, other: &Self) -> Option<Duration> {
        self.0.checked_sub(other.0)
    }
    pub fn checked_add_duration(&self, other: &Duration) -> Option<Self> {
        Some(Self(self.0.checked_add(*other)?))
    }
    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Self> {
        Some(Self(self.0.checked_sub(*other)?))
    }
}
