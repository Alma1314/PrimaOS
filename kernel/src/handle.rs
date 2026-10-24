//! 不可伪造且不复用的句柄表示（COSIR §3 / §18 第 1 条）。
//!
//! §3 要求所有 ID 是内核分配的 opaque 自然数、每类单调递增且永久不复用；§18 第 1 条要求
//! 细化层用「内核表索引 + generation 或受保护句柄」表示 opaque ID。本模块给出该表示的
//! 唯一编码：
//!
//! ```text
//! raw u64 = ((generation as u64) << 32) | (index as u64)
//! ```
//!
//! - `index` 是**物理槽索引**（会随对象/位置释放而复用）；
//! - `generation` 从 1 起，释放后递增，达到 `u32::MAX` 后永久退役槽位而不回绕，
//!   故「同一个 index 的旧句柄」在复用后不再匹配，
//!   这是 S2-X4（stale handle 不因 ID 复用重获 Authority）的编码基础；
//! - `index == 0` 与 `generation == 0` 一律非法（[`HandleError::Malformed`]），故 `0u64` 与
//!   任何高位为 0 的裸索引都不是合法句柄。
//!
//! §4.2 的单调逻辑 `CapId` 另存于 [`crate::bootstrap::CapabilityRecord`]；
//! `Handle` 的数值和槽索引不用于判定派生树先后。父可被 drop，历史记录仍保留。

/// 句柄解码错误。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandleError {
    /// 编码非法（index 或 generation 为 0）。
    Malformed,
}

/// 内核分配的 opaque 句柄（物理槽索引 + generation）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handle {
    index: u32,
    generation: u32,
}

impl Handle {
    /// 「无句柄」哨兵（如 Task 未设置结构性根时的 `root_ns_cap`）。
    pub const ZERO: Self = Self {
        index: 0,
        generation: 0,
    };

    /// 由槽索引与 generation 构造；任一为 0 返回 [`HandleError::Malformed`]。
    pub const fn new(index: u32, generation: u32) -> Result<Self, HandleError> {
        if index == 0 || generation == 0 {
            Err(HandleError::Malformed)
        } else {
            Ok(Self { index, generation })
        }
    }

    /// 槽位首次投入使用时的 generation（§3：ID 从 1 起）。
    pub const FIRST_GENERATION: u32 = 1;

    /// 编码为 syscall ABI / 协议使用的 `u64`。
    pub const fn encode(self) -> u64 {
        ((self.generation as u64) << 32) | self.index as u64
    }

    /// 从 `u64` 解码；非法编码返回 [`HandleError::Malformed`]。
    pub const fn decode(raw: u64) -> Result<Self, HandleError> {
        let index = raw as u32;
        let generation = (raw >> 32) as u32;
        if index == 0 || generation == 0 {
            Err(HandleError::Malformed)
        } else {
            Ok(Self { index, generation })
        }
    }

    pub const fn index(self) -> u32 {
        self.index
    }

    pub const fn generation(self) -> u32 {
        self.generation
    }

    pub const fn is_zero(self) -> bool {
        self.index == 0 && self.generation == 0
    }

    /// 槽位释放后递增 generation；耗尽返回 None，调用方须永久退役该槽。
    pub const fn next_generation(self) -> Option<u32> {
        self.generation.checked_add(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_index_in_low_bits_and_generation_in_high_bits() {
        let handle = Handle::new(0x40, 2).expect("valid handle");
        assert_eq!(handle.encode(), 0x0000_0002_0000_0040);
        assert_eq!(handle.index(), 0x40);
        assert_eq!(handle.generation(), 2);
    }

    #[test]
    fn generation_one_is_not_the_bare_index() {
        // 严格编码：generation 从 1 起，故 gen1 句柄的高 32 位非 0，裸索引不是合法句柄。
        let handle = Handle::new(3, 1).expect("valid handle");
        assert_eq!(handle.encode(), 0x0000_0001_0000_0003);
        assert_ne!(handle.encode(), 3);
    }

    #[test]
    fn decode_round_trips_and_rejects_malformed() {
        let handle = Handle::new(7, 5).expect("valid handle");
        assert_eq!(Handle::decode(handle.encode()), Ok(handle));
        assert_eq!(Handle::decode(0), Err(HandleError::Malformed));
        // 高位为 0 的裸索引（generation = 0）非法。
        assert_eq!(Handle::decode(3), Err(HandleError::Malformed));
        // index = 0 非法。
        assert_eq!(Handle::decode(1 << 32), Err(HandleError::Malformed));
    }

    #[test]
    fn zero_handle_is_the_absent_sentinel_and_does_not_decode() {
        assert!(Handle::ZERO.is_zero());
        assert_eq!(
            Handle::decode(Handle::ZERO.encode()),
            Err(HandleError::Malformed)
        );
    }

    #[test]
    fn next_generation_stops_at_exhaustion() {
        let handle = Handle::new(1, 1).expect("valid handle");
        assert_eq!(handle.next_generation(), Some(2));
        assert_eq!(
            Handle::new(1, u32::MAX - 1).unwrap().next_generation(),
            Some(u32::MAX)
        );
        let saturated = Handle {
            index: 1,
            generation: u32::MAX,
        };
        assert_eq!(saturated.next_generation(), None);
    }

    #[test]
    fn zero_index_or_generation_is_rejected() {
        assert_eq!(Handle::new(0, 1), Err(HandleError::Malformed));
        assert_eq!(Handle::new(1, 0), Err(HandleError::Malformed));
        assert_eq!(Handle::new(0, 0), Err(HandleError::Malformed));
    }
}
