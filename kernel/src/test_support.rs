//! host 单测的页表页池（`#[cfg(test)]` 测试基建，**零 `unsafe`**）。
//!
//! 页表页用 `Box::new(Page::zeroed())` 在**堆上**按 4096 对齐分配（`Page` 自身
//! `#[repr(C, align(4096))]`，`Box` 的分配器按该对齐处理），读写一律走 [`prima_ostd::mm`]
//! 的安全访问器。相对此前 `std::alloc::{Layout, alloc_zeroed, dealloc}` 的裸 API：无 `unsafe`、
//! 无手工 `Drop`，且**每页地址稳定**（`Box` 的堆块不随 `Vec` 增长而搬移）——页表项里存的
//! 帧地址在被 `translate` 掩码后仍指向同一页。
//!
//! 本模块只在 `cfg(test)` 下编译，不进入内核镜像。

/// 4 KiB 对齐的一页（512 × u64），host 上以其宿主地址充当"物理地址"。
#[repr(C, align(4096))]
pub struct Page([u64; 512]);

impl Page {
    /// 全零页。
    pub const fn zeroed() -> Self {
        Page([0; 512])
    }

    /// 该页地址。
    ///
    /// host 上"物理地址"即宿主虚拟地址：`translate`/`audit_reachable_page_tables` 只对地址做
    /// 低位掩码与透传，故只需 4 KiB 对齐，不需与真实物理内存语义一致。
    pub fn addr(&self) -> u64 {
        self as *const Page as u64
    }
}

/// 按需分配页表页的池；释放由 `Vec<Box<Page>>` 自动完成。
#[derive(Default)]
pub struct PagePool {
    pages: Vec<Box<Page>>,
}

impl PagePool {
    /// 空池。
    pub fn new() -> Self {
        PagePool::default()
    }

    /// 取一个新的全零页并返回其地址。
    ///
    /// 返回 `Option` 是为对齐内核 `map_4k` 的分配器闭包签名 `|| Option<u64>`；host 上不会返回
    /// `None`（无帧预算概念）。
    pub fn alloc(&mut self) -> Option<u64> {
        self.pages.push(Box::new(Page::zeroed()));
        self.pages.last().map(|page| page.addr())
    }

    /// 已分配页数（供"路径复用时不新建页表页"一类断言）。
    pub fn len(&self) -> usize {
        self.pages.len()
    }

    /// 是否尚未分配任何页。
    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use prima_ostd::mm::{read_u64_at, write_u64_at};

    #[test]
    fn pages_are_aligned_zeroed_and_independent() {
        let mut pool = PagePool::new();
        assert!(pool.is_empty());
        let a = pool.alloc().unwrap();
        let b = pool.alloc().unwrap();
        assert_eq!(pool.len(), 2);
        assert_eq!(a % 4096, 0, "页表页必须 4 KiB 对齐");
        assert_eq!(b % 4096, 0);
        assert_ne!(a, b, "每次分配必须是不同的页");
        // 全零（首末槽）。
        assert_eq!(read_u64_at(a, 0), 0);
        assert_eq!(read_u64_at(a, 511), 0);
        // 写一页不影响另一页。
        write_u64_at(a, 0, 0xdead_beef);
        assert_eq!(read_u64_at(a, 0), 0xdead_beef);
        assert_eq!(read_u64_at(b, 0), 0);
    }
}
