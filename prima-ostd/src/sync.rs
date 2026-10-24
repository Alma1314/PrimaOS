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
    sync::atomic::{AtomicBool, Ordering},
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

/// 一次发布的只读单元；设计参考 Asterinas 的 boot/ACPI/logger Once。
///
/// 未就绪的初始化由 IRQ-safe 锁串行化；Err/unwind 不发布，后续调用可重试。
/// 初始化闭包不得递归初始化同一单元、阻塞、切换任务或不返回。
/// bare-metal panic=abort 不保证恢复；不会回滚闭包外部副作用。
/// 发布后引用绑定到单元生命周期，快路径不取锁、不改变 IF。
///
/// ```compile_fail
/// use prima_ostd::sync::Once;
/// let value = {
///     let cell = Once::new();
///     cell.get_or_init(|| 7u8)
/// };
/// assert_eq!(*value, 7);
/// ```
pub struct Once<T> {
    value: UnsafeCell<MaybeUninit<T>>,
    ready: AtomicBool,
    lock: lock::Lock,
}

// SAFETY: 发布前唯一写者持锁；Release/Acquire 发布完整 T 后不再写入。
// T: Send + Sync 允许初始化所有权和只读引用跨线程。
unsafe impl<T: Send + Sync> Sync for Once<T> {}

impl<T> Once<T> {
    pub const fn new() -> Self {
        Self {
            value: UnsafeCell::new(MaybeUninit::uninit()),
            ready: AtomicBool::new(false),
            lock: lock::Lock::new(),
        }
    }

    /// 未发布（含正在初始化）返回 None，否则借用完整值。
    pub fn get(&self) -> Option<&T> {
        if self.ready.load(Ordering::Acquire) {
            // SAFETY: Release 发布前已写入完整值，之后没有写者。
            Some(unsafe { (*self.value.get()).assume_init_ref() })
        } else {
            None
        }
    }

    pub fn get_or_init(&self, f: impl FnOnce() -> T) -> &T {
        match self.get_or_try_init(|| Ok::<T, core::convert::Infallible>(f())) {
            Ok(value) => value,
            Err(never) => match never {},
        }
    }

    /// 只执行胜者的闭包；Err/unwind 保持未发布并释放锁，可由后续调用重试。
    pub fn get_or_try_init<E>(&self, f: impl FnOnce() -> Result<T, E>) -> Result<&T, E> {
        if let Some(value) = self.get() {
            return Ok(value);
        }
        let _guard = self.lock.lock();
        if let Some(value) = self.get() {
            return Ok(value);
        }
        let value = f()?;
        // SAFETY: 持锁且尚未发布，没有其他写者或指向内部 T 的引用。
        unsafe { (*self.value.get()).write(value) };
        self.ready.store(true, Ordering::Release);
        Ok(self.get().expect("Once published before returning"))
    }

    /// 无等待地尝试发布；就绪或锁繁忙时原样返回 value。
    pub fn set(&self, value: T) -> Result<(), T> {
        let Some(_guard) = self.lock.try_lock() else {
            return Err(value);
        };
        if self.get().is_some() {
            return Err(value);
        }
        // SAFETY: 持锁且未发布，独占未初始化的内部存储。
        unsafe { (*self.value.get()).write(value) };
        self.ready.store(true, Ordering::Release);
        Ok(())
    }
}

impl<T> Drop for Once<T> {
    fn drop(&mut self) {
        if *self.ready.get_mut() {
            // SAFETY: &mut self 排除读者/初始化者；ready 只在完整写入后置位。
            unsafe { self.value.get_mut().assume_init_drop() };
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

    #[test]
    fn once_publishes_one_complete_value_to_contending_readers() {
        use std::sync::{Barrier, atomic::AtomicUsize};
        let once = Once::new();
        let starts = Barrier::new(8);
        let calls = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    starts.wait();
                    let value = once.get_or_init(|| {
                        calls.fetch_add(1, Ordering::Relaxed);
                        (0..128u64).map(|i| i * 17 + 3).collect::<Vec<_>>()
                    });
                    assert_eq!(value.len(), 128);
                    for (i, &item) in value.iter().enumerate() {
                        assert_eq!(item, i as u64 * 17 + 3);
                    }
                    assert!(core::ptr::eq(value, once.get().unwrap()));
                });
            }
        });
        assert_eq!(calls.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn once_waiter_can_initialize_after_error_or_unwind() {
        use std::sync::{Arc, mpsc};
        for unwind in [false, true] {
            let once = Arc::new(Once::new());
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let first_cell = once.clone();
            let first = std::thread::spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    first_cell
                        .get_or_try_init(|| {
                            assert!(first_cell.get().is_none());
                            assert_eq!(
                                first_cell.set(String::from("busy")),
                                Err(String::from("busy"))
                            );
                            entered_tx.send(()).unwrap();
                            release_rx.recv().unwrap();
                            if unwind {
                                panic!("test init unwind");
                            }
                            Err::<String, _>("retry")
                        })
                        .map(|_| ())
                }));
                if unwind {
                    assert!(result.is_err());
                } else {
                    assert_eq!(result.unwrap(), Err("retry"));
                }
            });
            entered_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            assert!(once.get().is_none());
            assert_eq!(
                once.set(String::from("rejected")),
                Err(String::from("rejected"))
            );
            let (waiting_tx, waiting_rx) = mpsc::channel();
            let next_cell = once.clone();
            let next = std::thread::spawn(move || {
                waiting_tx.send(()).unwrap();
                assert_eq!(next_cell.get_or_init(|| String::from("winner")), "winner");
            });
            waiting_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            release_tx.send(()).unwrap();
            first.join().unwrap();
            next.join().unwrap();
            assert_eq!(once.get().map(String::as_str), Some("winner"));
        }
    }

    #[test]
    fn once_failed_initialization_does_not_publish_and_success_skips_later_closures() {
        let once = Once::new();
        assert_eq!(once.get_or_try_init(|| Err::<u64, _>(7)), Err(7));
        assert!(once.get().is_none());
        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                once.get_or_init(|| panic!("test init unwind"));
            }))
            .is_err()
        );
        assert!(once.get().is_none());
        assert_eq!(once.set(42), Ok(()));
        assert_eq!(
            once.get_or_try_init(|| -> Result<_, ()> { panic!("must skip") }),
            Ok(&42)
        );
        assert_eq!(once.get_or_init(|| panic!("must skip")), &42);
        assert_eq!(once.set(9), Err(9));
    }

    #[test]
    fn once_drops_only_published_value_and_returns_rejected_ownership() {
        use core::sync::atomic::AtomicUsize;
        struct Counted<'a>(&'a AtomicUsize);
        impl Drop for Counted<'_> {
            fn drop(&mut self) {
                self.0.fetch_add(1, Ordering::Relaxed);
            }
        }
        let drops = AtomicUsize::new(0);
        drop(Once::<Counted<'_>>::new());
        assert_eq!(drops.load(Ordering::Relaxed), 0);
        let once = Once::new();
        assert!(
            once.get_or_try_init(|| {
                let _unpublished = Counted(&drops);
                Err::<Counted<'_>, _>(())
            })
            .is_err()
        );
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        assert!(once.set(Counted(&drops)).is_ok());
        let rejected = match once.set(Counted(&drops)) {
            Err(value) => value,
            Ok(()) => panic!("second set must fail"),
        };
        assert_eq!(drops.load(Ordering::Relaxed), 1);
        drop(rejected);
        assert_eq!(drops.load(Ordering::Relaxed), 2);
        drop(once);
        assert_eq!(drops.load(Ordering::Relaxed), 3);
    }
}
