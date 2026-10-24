//! SPEC-081：固定容量、按实例分段的静态 arena。
//!
//! `BootstrapState` 已贴 13 KiB 结构体守卫（见 `bootstrap.rs` 的
//! `bootstrap_state_stays_within_the_stack_budget`），阶段 3 的 Namespace 条目存储**不得内联**。
//! 本模块提供与 [`AUDIT_ARENA`](crate::bootstrap) 同一模式的通用版本：容量与段数都放在静态
//! 存储里，结构体内只留一个 8 字节的 [`ArenaSlice`] 句柄。
//!
//! **段的意义**：内核只构造一个 `BootstrapState`（1 段足矣），而 host 单测**并行**构造多个
//! state——每个实例领一段、段之间不重叠，故单测互不干扰。段由原子游标分配，耗尽即确定性
//! panic（不静默复用他人区段）。
//!
//! **口径**：本模块不引入 `unsafe`（内核 crate 是 `#![deny(unsafe_code)]`）；可变性由
//! [`StaticCell`] 的 IRQ-safe 锁承担；不同 host 实例也经原子互斥，不能仅凭区段不同
//! 就构造对整个二维数组的并发可变引用。

use core::sync::atomic::{AtomicUsize, Ordering};

use prima_ostd::sync::StaticCell;

/// 一次 [`Arena::claim`] 得到的区段句柄。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArenaSlice {
    region: usize,
}

impl ArenaSlice {
    /// 区段序号（供证据/调试；语义上只需原样回传给 [`Arena`] 的读写接口）。
    pub const fn region(self) -> usize {
        self.region
    }
}

/// `REGIONS` 个区段、每段 `SLOTS` 槽的静态 arena。
pub struct Arena<T: Copy, const SLOTS: usize, const REGIONS: usize> {
    cells: StaticCell<[[Option<T>; SLOTS]; REGIONS]>,
    cursor: AtomicUsize,
}

impl<T: Copy, const SLOTS: usize, const REGIONS: usize> Arena<T, SLOTS, REGIONS> {
    /// 构造一个全空的 arena（`const`，可用于 `static`）。
    pub const fn new() -> Self {
        Self {
            cells: StaticCell::new([[None; SLOTS]; REGIONS]),
            cursor: AtomicUsize::new(0),
        }
    }

    /// 每段的槽数。
    pub const fn slot_count(&self) -> usize {
        SLOTS
    }

    /// 领一个区段。区段耗尽即确定性 panic——单测里段数足够，内核只用 1 段。
    pub fn claim(&self) -> ArenaSlice {
        let region = self.cursor.fetch_add(1, Ordering::Relaxed);
        assert!(
            region < REGIONS,
            "arena exhausted: 提高 REGIONS（它不占 BootstrapState 空间）"
        );
        ArenaSlice { region }
    }

    /// 读第 `index` 槽的副本；越界返回 `None`。
    pub fn get(&self, slice: ArenaSlice, index: usize) -> Option<T> {
        if index >= SLOTS {
            return None;
        }
        self.cells.with(|cells| cells[slice.region][index])
    }

    /// 写第 `index` 槽；越界为 no-op（调用方以 `slot_count` 自限）。
    pub fn set(&self, slice: ArenaSlice, index: usize, value: Option<T>) {
        if index >= SLOTS {
            return;
        }
        self.cells
            .with_mut(|cells| cells[slice.region][index] = value);
    }
}

impl<T: Copy, const SLOTS: usize, const REGIONS: usize> Default for Arena<T, SLOTS, REGIONS> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_returns_distinct_regions() {
        let arena: Arena<u32, 4, 2> = Arena::new();
        let first = arena.claim();
        let second = arena.claim();
        assert_ne!(first, second);
        assert_eq!(first.region(), 0);
        assert_eq!(second.region(), 1);
    }

    #[test]
    fn regions_do_not_share_slots() {
        let arena: Arena<u32, 4, 2> = Arena::new();
        let a = arena.claim();
        let b = arena.claim();
        arena.set(a, 0, Some(11));
        arena.set(b, 0, Some(22));
        assert_eq!(arena.get(a, 0), Some(11));
        assert_eq!(arena.get(b, 0), Some(22));
    }

    #[test]
    fn out_of_range_slot_is_none_and_write_is_ignored() {
        let arena: Arena<u32, 2, 1> = Arena::new();
        let slice = arena.claim();
        assert_eq!(arena.slot_count(), 2);
        assert_eq!(arena.get(slice, 2), None);
        arena.set(slice, 5, Some(1));
        assert_eq!(arena.get(slice, 5), None);
        assert_eq!(arena.get(slice, 1), None);
    }

    #[test]
    fn clearing_a_slot_is_observable() {
        let arena: Arena<u32, 2, 1> = Arena::new();
        let slice = arena.claim();
        arena.set(slice, 0, Some(7));
        arena.set(slice, 0, None);
        assert_eq!(arena.get(slice, 0), None);
    }

    #[test]
    #[should_panic(expected = "arena exhausted")]
    fn claiming_past_the_last_region_panics() {
        let arena: Arena<u32, 1, 1> = Arena::new();
        let _ = arena.claim();
        let _ = arena.claim();
    }
}
