//! 内核可变全局状态原语。
//!
//! StaticCell 的闭包访问由原子锁互斥；bare-metal 下先屏蔽本地 IRQ，释放锁后恢复原 IF。
//! 锁实现按 Asterinas OSTD 移植。host 下只做原子互斥，便于并行验证内核纯逻辑。
//! 同一单元不可递归加锁，闭包不得阻塞、切换任务或不返回；裸指针/汇编访问仍由调用方
//! 保证独占与生命周期，本模块没有把整个框架变为 sound。

mod lock;

use core::{
    cell::UnsafeCell,
    mem::MaybeUninit,
    sync::atomic::{AtomicU8, Ordering},
};

/// 闭包访问期间持有 IRQ-safe 互斥锁的全局单元。
///
/// 闭包不得递归访问同一单元（会死锁）、阻塞或转移控制权而不返回。
/// `repr(C)` 与首字段 value 保证裸入口的 `sym CELL` 仍指向 T 的第一个字节。
#[repr(C)]
pub struct StaticCell<T> {
    value: UnsafeCell<T>,
    lock: lock::Lock,
}

// SAFETY: 所有闭包访问由同一 Acquire/Release 锁串行化；T: Send 允许所有权跨上下文。
unsafe impl<T: Send> Sync for StaticCell<T> {}

impl<T> StaticCell<T> {
    /// 构造（`const`，可用于 `static`）。
    pub const fn new(value: T) -> Self {
        Self {
            value: UnsafeCell::new(value),
            lock: lock::Lock::new(),
        }
    }

    /// 以共享引用调用 `f`。
    #[inline(always)]
    pub fn with<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let _guard = self.lock.lock();
        // SAFETY: guard 保证该闭包期间没有其他闭包访问同一值。
        f(unsafe { &*self.value.get() })
    }

    /// 以可变引用调用 `f`。
    #[inline(always)]
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let _guard = self.lock.lock();
        // SAFETY: guard 保证该闭包期间独占访问。
        f(unsafe { &mut *self.value.get() })
    }

    /// 取内部值的地址（供"只关心地址、不按元素读写"的场合，如内核栈缓冲区）。
    ///
    /// 仅给出地址、不解引用，故本身安全。该指针不持锁；调用方必须另外保证裸访问不会
    /// 与闭包/汇编访问冲突。适用于既有栈缓冲与硬件入口，不是绕过锁的安全读写接口。
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
    fn static_cell_serializes_parallel_read_modify_write() {
        let cell = StaticCell::new(0usize);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..2000 {
                        cell.with_mut(|value| *value += 1);
                    }
                });
            }
        });
        assert_eq!(cell.with(|value| *value), 16000);
    }

    #[test]
    fn static_cell_unlocks_after_panicking_closure() {
        let cell = StaticCell::new(1u8);
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                cell.with_mut(|value| {
                    *value = 2;
                    panic!("test unwind");
                });
            }))
            .is_err()
        );
        assert_eq!(cell.with(|value| *value), 2);
        cell.with_mut(|value| *value = 3);
        assert_eq!(cell.with(|value| *value), 3);
    }

    #[test]
    fn assembly_visible_value_starts_at_the_cell_address() {
        #[repr(align(4096))]
        struct Page([u8; 4096]);
        let cell = StaticCell::new(Page([0; 4096]));
        assert_eq!(core::mem::offset_of!(StaticCell<Page>, value), 0);
        assert_eq!(&cell as *const _ as usize, cell.as_ptr() as usize);
        assert!(core::mem::offset_of!(StaticCell<Page>, lock) >= size_of::<Page>());
        assert_eq!(cell.with(|page| page.0[0]), 0);
    }

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
