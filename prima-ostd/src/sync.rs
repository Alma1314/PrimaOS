//! 单核内核的可变全局状态原语。
//!
//! 内核里大量 `static mut` 其实由"单核 + 顺序访问"这一运行环境保证安全，但 Rust 强制
//! 每一次触碰都写 `unsafe`。本模块把这份 unsafe 收敛到两个封装里，暴露**安全**的闭包式
//! API，使上层无需 `unsafe`。
//!
//! **口径**：这些类型的安全前提是调用方遵守单核顺序访问契约（无并发、不跨中断重入持有
//! 引用）。违反会产生错误行为（数据竞争），本层**不宣称已消除该风险**——与 `arch` 的
//! "信任调用方"取向一致。

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU8, Ordering},
};

/// 单核可变全局单元。
///
/// 契约：`with`/`with_mut` 的闭包不得重入同一 `StaticCell`（否则产生别名 `&mut`），
/// 且不得跨中断/其他核共享。除此外，从 safe 上下文读写无需 `unsafe`。
pub struct StaticCell<T> {
    value: UnsafeCell<T>,
}

// SAFETY: 见类型契约——单核顺序访问；`T: Send` 允许其跨执行上下文转移。
unsafe impl<T: Send> Sync for StaticCell<T> {}

impl<T> StaticCell<T> {
    /// 构造（`const`，可用于 `static`）。
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
        }
    }

    /// 以共享引用调用 `f`。
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        // SAFETY: 见类型契约——调用方保证无并发写与重入。
        f(unsafe { &*self.value.get() })
    }

    /// 以可变引用调用 `f`。
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        // SAFETY: 见类型契约——调用方保证无并发访问与重入。
        f(unsafe { &mut *self.value.get() })
    }

    /// 取内部值的地址（供"只关心地址、不按元素读写"的场合，如内核栈缓冲区）。
    ///
    /// 仅给出地址、不解引用，故本身安全；用该地址读写属调用方按契约负责。
    pub fn as_ptr(&self) -> *mut T {
        self.value.get()
    }
}

const ONCE_EMPTY: u8 = 0;
const ONCE_BUSY: u8 = 1;
const ONCE_READY: u8 = 2;

/// 只初始化一次的全局单元（boot-once 语义，`no_std` 下无 `std::sync::Once`）。
///
/// 初始化后只读；`get_or_init` 返回的 `&T` 在 `Once` 存活期内有效且不变。
pub struct Once<T> {
    state: AtomicU8,
    value: UnsafeCell<MaybeUninit<T>>,
}

// SAFETY: 初始化后 `value` 不再被写；`T: Send + Sync` 允许共享引用跨上下文。
unsafe impl<T: Send + Sync> Sync for Once<T> {}

impl<T> Once<T> {
    /// 构造未初始化的单元（`const`）。
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(ONCE_EMPTY),
            value: UnsafeCell::new(MaybeUninit::uninit()),
        }
    }

    /// 已初始化则返回其值，否则 `None`。
    pub fn get(&self) -> Option<&T> {
        if self.state.load(Ordering::Acquire) == ONCE_READY {
            // SAFETY: 状态为 READY 表示 `value` 已初始化且此后不再被写。
            Some(unsafe { (*self.value.get()).assume_init_ref() })
        } else {
            None
        }
    }

    /// 取已初始化值，否则用 `f` 初始化后返回。
    pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        if let Some(value) = self.get() {
            return value;
        }
        match self.state.compare_exchange(
            ONCE_EMPTY,
            ONCE_BUSY,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // SAFETY: 本调用者抢到 EMPTY→BUSY 的所有权，独占写 `value`。
                unsafe {
                    (*self.value.get()).write(f());
                }
                self.state.store(ONCE_READY, Ordering::Release);
            }
            Err(_) => {
                // 单核下不应发生；为不使语义依赖"不会发生"，自旋等待另一个初始化者完成。
                while self.state.load(Ordering::Acquire) != ONCE_READY {
                    core::hint::spin_loop();
                }
            }
        }
        self.get().expect("Once 在 get_or_init 返回前必为 READY")
    }

    /// 尝试写入初值；已初始化则返回原值。
    pub fn set(&self, value: T) -> Result<(), T> {
        match self.state.compare_exchange(
            ONCE_EMPTY,
            ONCE_BUSY,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // SAFETY: 抢到 EMPTY→BUSY 所有权，独占写 `value`。
                unsafe {
                    (*self.value.get()).write(value);
                }
                self.state.store(ONCE_READY, Ordering::Release);
                Ok(())
            }
            Err(_) => Err(value),
        }
    }
}

impl<T> Default for Once<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_cell_reads_and_mutates() {
        let cell = StaticCell::new(7u32);
        assert_eq!(cell.with(|v| *v), 7);
        cell.with_mut(|v| *v += 1);
        assert_eq!(cell.with(|v| *v), 8);
    }

    #[test]
    fn static_cell_returns_closure_result() {
        let cell = StaticCell::new(String::from("abc"));
        let len = cell.with(|v| v.len());
        assert_eq!(len, 3);
    }

    #[test]
    fn once_is_empty_then_initialized_once() {
        let once: Once<String> = Once::new();
        assert!(once.get().is_none());
        let value = once.get_or_init(|| String::from("first"));
        assert_eq!(value, "first");
        // 再次 init 不覆盖。
        let again = once.get_or_init(|| String::from("second"));
        assert_eq!(again, "first");
        assert_eq!(once.get().map(String::as_str), Some("first"));
    }

    #[test]
    fn once_set_rejects_second_write() {
        let once: Once<u8> = Once::new();
        assert_eq!(once.set(1), Ok(()));
        assert_eq!(once.set(2), Err(2));
        assert_eq!(once.get(), Some(&1));
    }

    #[test]
    fn static_cell_as_ptr_addresses_the_value() {
        let cell = StaticCell::new(0x2au64);
        // SAFETY: 单核测试、无并发写；仅解引用读取。
        assert_eq!(unsafe { *cell.as_ptr() }, 0x2a);
    }
}
