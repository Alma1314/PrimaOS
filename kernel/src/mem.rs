use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

pub const FRAME_SIZE: u64 = 4096;
const MAX_FREE_RANGES: usize = 128;
// FrameRegistry 最多同时持有 192 个 Segment 帧和 128 个页表帧；全部归还也必须有位置。
const MAX_RECYCLED_FRAMES: usize = 320;

/// 物理地址经 bootloader 的物理内存直映射换算为虚拟地址（offset + phys）。
pub fn phys_to_virt(phys: u64, phys_map_offset: u64) -> u64 {
    phys_map_offset + phys
}

/// S2-X3：把一帧清零（§7.1 要求帧在**对新 Segment 可见之前**清零）。
///
/// 归还帧仍在空闲池中保留上次归属的字节，故每次 `memseg_create` 取帧后必须调用本函数。
pub fn zero_frame(phys: u64, phys_map_offset: u64) {
    let base = phys_to_virt(phys, phys_map_offset);
    prima_ostd::mm::fill_zero(base, FRAME_SIZE as usize);
}

/// 读取一个物理地址处的字节（经直映射），返回 (虚拟地址, 读到字节)。
///
/// 纯逻辑部分（换算 va）在本模块，真实的裸读取收敛到框架层 [`prima_ostd::mm`]。
///
/// # 前置约束
///
/// `probed_phys` 必须处于 bootloader 报告为 `Usable` 的物理内存中，且该地址必须
/// 落在物理内存直映射区内（调用方应先确认 `physical_memory_offset` 为 `Some`）。
/// 调用方负责保证 `probed_phys` 是可安全读取的内核可用物理帧。
pub fn probe_physical_memory(probed_phys: u64, phys_map_offset: u64) -> (u64, u8) {
    let va = phys_to_virt(probed_phys, phys_map_offset);
    let byte = prima_ostd::mm::read_u8(va);
    (va, byte)
}

/// 返回 mem 清单中第一个 `Usable` 帧的物理起始地址（`align_up(start, FRAME_SIZE)`）。
/// 无任何 Usable 区返回 `None`。仅做纯索引遍历，不读物理。
pub fn first_usable_frame_phys(regions: &[MemoryRegion]) -> Option<u64> {
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            let Some(lo) = align_up(region.start, FRAME_SIZE) else {
                continue;
            };
            let hi = align_down(region.end, FRAME_SIZE);
            if hi > lo {
                return Some(lo);
            }
        }
    }
    None
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameInventory {
    pub usable_frames: usize,
    pub image_frames: usize,
    pub free_frames: usize,
    pub usable_regions: usize,
    pub kernel_phys_start: u64,
    pub kernel_phys_end: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeFrameRange {
    pub start: u64,
    pub end: u64,
    pub next: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FreeFrameAllocator {
    ranges: [Option<FreeFrameRange>; MAX_FREE_RANGES],
    range_count: usize,
    /// 已耗尽的区间永不重新扫描；归还的帧从 recycled 栈优先取用。
    next_range: usize,
    remaining: usize,
    recycled: [u64; MAX_RECYCLED_FRAMES],
    recycled_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AllocatorError {
    TooManyRanges,
    InvalidRange,
    ImageOverlap,
    InvariantViolation,
    NoFrame,
}

impl FreeFrameAllocator {
    /// 无任何空闲区间的空分配器（`const`；供 `FrameRegistry` 的静态就地构造使用）。
    pub const fn empty() -> Self {
        Self {
            ranges: [None; MAX_FREE_RANGES],
            range_count: 0,
            next_range: 0,
            remaining: 0,
            recycled: [0; MAX_RECYCLED_FRAMES],
            recycled_count: 0,
        }
    }

    pub fn from_regions(
        regions: &[MemoryRegion],
        image_start: u64,
        image_end: u64,
    ) -> Result<Self, AllocatorError> {
        if image_start >= image_end
            || !image_start.is_multiple_of(FRAME_SIZE)
            || !image_end.is_multiple_of(FRAME_SIZE)
        {
            return Err(AllocatorError::InvalidRange);
        }
        let mut allocator = Self::empty();
        let mut previous_end = 0;
        for region in regions {
            if region.start > region.end || region.start < previous_end {
                return Err(AllocatorError::InvalidRange);
            }
            previous_end = region.end;
            if region.kind != MemoryRegionKind::Usable {
                continue;
            }
            let Some(start) = align_up(region.start, FRAME_SIZE) else {
                continue;
            };
            let end = align_down(region.end, FRAME_SIZE);
            if end <= start {
                continue;
            }
            if start < image_end && image_start < end {
                return Err(AllocatorError::ImageOverlap);
            }
            if allocator.range_count == MAX_FREE_RANGES {
                return Err(AllocatorError::TooManyRanges);
            }
            allocator.ranges[allocator.range_count] = Some(FreeFrameRange {
                start,
                end,
                next: start,
            });
            allocator.range_count += 1;
            allocator.remaining += ((end - start) / FRAME_SIZE) as usize;
        }
        allocator.check_invariants(image_start, image_end)?;
        Ok(allocator)
    }

    pub fn remaining(&self) -> usize {
        self.remaining
    }

    pub fn allocate(&mut self) -> Result<u64, AllocatorError> {
        if self.recycled_count > 0 {
            self.recycled_count -= 1;
            self.remaining -= 1;
            return Ok(self.recycled[self.recycled_count]);
        }
        if self.next_range < self.range_count {
            let range = self.ranges[self.next_range].as_mut().unwrap();
            let frame = range.next;
            range.next += FRAME_SIZE;
            if range.next == range.end {
                self.next_range += 1;
            }
            self.remaining -= 1;
            return Ok(frame);
        }
        Err(AllocatorError::NoFrame)
    }

    pub fn release(&mut self, frame: u64) -> Result<(), AllocatorError> {
        if !frame.is_multiple_of(FRAME_SIZE)
            || !self.ranges[..self.range_count]
                .iter()
                .flatten()
                .any(|range| range.start <= frame && frame < range.next)
        {
            return Err(AllocatorError::InvalidRange);
        }
        if self.recycled[..self.recycled_count].contains(&frame) {
            return Err(AllocatorError::InvalidRange);
        }
        if self.recycled_count == MAX_RECYCLED_FRAMES {
            return Err(AllocatorError::TooManyRanges);
        }
        self.recycled[self.recycled_count] = frame;
        self.recycled_count += 1;
        self.remaining += 1;
        Ok(())
    }

    pub fn check_invariants(&self, image_start: u64, image_end: u64) -> Result<(), AllocatorError> {
        if image_start >= image_end
            || !image_start.is_multiple_of(FRAME_SIZE)
            || !image_end.is_multiple_of(FRAME_SIZE)
            || self.range_count > MAX_FREE_RANGES
            || self.next_range > self.range_count
            || self.recycled_count > MAX_RECYCLED_FRAMES
        {
            return Err(AllocatorError::InvariantViolation);
        }

        let mut previous_end = 0;
        let mut expected_remaining = self.recycled_count;
        for (index, range) in self.ranges.iter().enumerate() {
            if index >= self.range_count {
                if range.is_some() {
                    return Err(AllocatorError::InvariantViolation);
                }
                continue;
            }
            let Some(range) = range else {
                return Err(AllocatorError::InvariantViolation);
            };
            if range.start >= range.end
                || range.next < range.start
                || range.next > range.end
                || !range.start.is_multiple_of(FRAME_SIZE)
                || !range.end.is_multiple_of(FRAME_SIZE)
                || !range.next.is_multiple_of(FRAME_SIZE)
                || range.start < previous_end
                || (index < self.next_range && range.next != range.end)
                || (index >= self.next_range && range.next == range.end)
            {
                return Err(AllocatorError::InvariantViolation);
            }
            if range.start < image_end && image_start < range.end {
                return Err(AllocatorError::ImageOverlap);
            }
            expected_remaining += ((range.end - range.next) / FRAME_SIZE) as usize;
            previous_end = range.end;
        }

        for index in 0..self.recycled_count {
            let frame = self.recycled[index];
            if !frame.is_multiple_of(FRAME_SIZE)
                || self.recycled[..index].contains(&frame)
                || !self.ranges[..self.range_count]
                    .iter()
                    .flatten()
                    .any(|range| range.start <= frame && frame < range.next)
            {
                return Err(AllocatorError::InvariantViolation);
            }
        }
        if expected_remaining != self.remaining {
            return Err(AllocatorError::InvariantViolation);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemError {
    EmptyImage,
    InvalidImageRange,
    ImageOverlapsUsable,
    RegionsNotSortedOrOverlapping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTableOwnershipError {
    Unaligned,
    OutsideMemoryMap,
    ClassifiedUsable,
}

pub fn validate_external_page_table_root(
    regions: &[MemoryRegion],
    root_phys: u64,
) -> Result<(), PageTableOwnershipError> {
    if !root_phys.is_multiple_of(FRAME_SIZE) {
        return Err(PageTableOwnershipError::Unaligned);
    }
    let root_end = root_phys
        .checked_add(FRAME_SIZE)
        .ok_or(PageTableOwnershipError::OutsideMemoryMap)?;
    for region in regions {
        if region.start <= root_phys && root_end <= region.end {
            return if region.kind == MemoryRegionKind::Usable {
                Err(PageTableOwnershipError::ClassifiedUsable)
            } else {
                Ok(())
            };
        }
    }
    Err(PageTableOwnershipError::OutsideMemoryMap)
}

pub fn validate_reachable_page_table_frame(
    regions: &[MemoryRegion],
    frame_phys: u64,
) -> Result<(), PageTableOwnershipError> {
    if !frame_phys.is_multiple_of(FRAME_SIZE) {
        return Err(PageTableOwnershipError::Unaligned);
    }
    let frame_end = frame_phys
        .checked_add(FRAME_SIZE)
        .ok_or(PageTableOwnershipError::OutsideMemoryMap)?;
    if regions
        .iter()
        .any(|region| region.start <= frame_phys && frame_end <= region.end)
    {
        Ok(())
    } else {
        Err(PageTableOwnershipError::OutsideMemoryMap)
    }
}

pub fn build_inventory(
    regions: &[MemoryRegion],
    kernel_addr: u64,
    kernel_len: u64,
) -> Result<FrameInventory, MemError> {
    if kernel_len == 0 {
        return Err(MemError::EmptyImage);
    }

    let mut prev_end = 0u64;
    for region in regions {
        if region.start > region.end || region.start < prev_end {
            return Err(MemError::RegionsNotSortedOrOverlapping);
        }
        prev_end = region.end;
    }

    let mut usable_frames = 0usize;
    let mut usable_regions = 0usize;
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            usable_regions += 1;
            let Some(lo) = align_up(region.start, FRAME_SIZE) else {
                continue;
            };
            let hi = align_down(region.end, FRAME_SIZE);
            if hi > lo {
                usable_frames += ((hi - lo) / FRAME_SIZE) as usize;
            }
        }
    }

    let kernel_phys_start = align_down(kernel_addr, FRAME_SIZE);
    let kernel_phys_end = kernel_addr
        .checked_add(kernel_len)
        .and_then(|end| align_up(end, FRAME_SIZE))
        .ok_or(MemError::InvalidImageRange)?;
    let image_frames = ((kernel_phys_end - kernel_phys_start) / FRAME_SIZE) as usize;

    // bootloader 把内核镜像存放在 Bootloader 区（不在 Usable 内）。校验镜像与所有
    // Usable 帧不相交（W6）：若任一帧重叠，则该帧既被视为空闲又被镜像占用。
    for region in regions {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        let Some(lo) = align_up(region.start, FRAME_SIZE) else {
            continue;
        };
        let lo = lo.max(kernel_phys_start);
        let hi = align_down(region.end, FRAME_SIZE).min(kernel_phys_end);
        if hi > lo {
            return Err(MemError::ImageOverlapsUsable);
        }
    }

    Ok(FrameInventory {
        usable_frames,
        image_frames,
        // 镜像已从 Usable 排除，Usable 帧即自由帧，不扣减镜像。
        free_frames: usable_frames,
        usable_regions,
        kernel_phys_start,
        kernel_phys_end,
    })
}

fn align_up(value: u64, align: u64) -> Option<u64> {
    value.checked_add((align - value % align) % align)
}

fn align_down(value: u64, align: u64) -> u64 {
    value / align * align
}

#[cfg(test)]
mod tests {
    use super::*;
    use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

    fn usable(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion {
            start,
            end,
            kind: MemoryRegionKind::Usable,
        }
    }

    fn bootloader(start: u64, end: u64) -> MemoryRegion {
        MemoryRegion {
            start,
            end,
            kind: MemoryRegionKind::Bootloader,
        }
    }

    #[test]
    fn external_page_table_root_must_not_be_usable() {
        let regions = [bootloader(0x0, 0x2000), usable(0x2000, 0x4000)];
        assert_eq!(validate_external_page_table_root(&regions, 0x1000), Ok(()));
        assert_eq!(
            validate_external_page_table_root(&regions, 0x2000),
            Err(PageTableOwnershipError::ClassifiedUsable)
        );
    }

    #[test]
    fn external_page_table_root_rejects_bad_address() {
        let regions = [bootloader(0x0, 0x2000)];
        assert_eq!(
            validate_external_page_table_root(&regions, 0x0800),
            Err(PageTableOwnershipError::Unaligned)
        );
        assert_eq!(
            validate_external_page_table_root(&regions, 0x3000),
            Err(PageTableOwnershipError::OutsideMemoryMap)
        );
    }

    #[test]
    fn allocator_returns_only_unique_usable_frames() {
        let regions = [
            usable(0x0, 0x3000),
            bootloader(0x3000, 0x4000),
            usable(0x5000, 0x7000),
        ];
        let mut allocator = FreeFrameAllocator::from_regions(&regions, 0x3000, 0x4000).unwrap();
        assert_eq!(allocator.remaining(), 5);
        let frames = (0..5)
            .map(|_| allocator.allocate().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(frames, vec![0, 0x1000, 0x2000, 0x5000, 0x6000]);
        assert_eq!(allocator.allocate(), Err(AllocatorError::NoFrame));
    }

    #[test]
    fn allocator_rejects_image_overlap_and_unsorted_regions() {
        let overlap = [usable(0x2000, 0x6000)];
        assert_eq!(
            FreeFrameAllocator::from_regions(&overlap, 0x3000, 0x4000),
            Err(AllocatorError::ImageOverlap)
        );
        let unsorted = [usable(0x4000, 0x5000), usable(0x1000, 0x2000)];
        assert_eq!(
            FreeFrameAllocator::from_regions(&unsorted, 0x8000, 0x9000),
            Err(AllocatorError::InvalidRange)
        );
    }

    #[test]
    fn rejects_reversed_regions_and_skips_unrepresentable_tail() {
        let reversed = [usable(0x3000, 0x2000)];
        assert_eq!(
            build_inventory(&reversed, 0x4000, 0x1000),
            Err(MemError::RegionsNotSortedOrOverlapping)
        );
        assert_eq!(
            FreeFrameAllocator::from_regions(&reversed, 0x4000, 0x5000),
            Err(AllocatorError::InvalidRange)
        );

        let tail = [usable(u64::MAX - 10, u64::MAX)];
        assert_eq!(first_usable_frame_phys(&tail), None);
        assert_eq!(
            build_inventory(&tail, 0x4000, 0x1000)
                .unwrap()
                .usable_frames,
            0
        );
        assert_eq!(
            FreeFrameAllocator::from_regions(&tail, 0x4000, 0x5000)
                .unwrap()
                .remaining(),
            0
        );
    }

    #[test]
    fn rejects_image_address_overflow() {
        assert_eq!(
            build_inventory(&[], u64::MAX - 10, 20),
            Err(MemError::InvalidImageRange)
        );
        assert_eq!(
            build_inventory(&[], u64::MAX - 10, 1),
            Err(MemError::InvalidImageRange)
        );
    }

    #[test]
    fn allocator_releases_and_reuses_a_frame_once() {
        let regions = [usable(0x0, 0x3000)];
        let mut allocator = FreeFrameAllocator::from_regions(&regions, 0x4000, 0x5000).unwrap();
        let frame = allocator.allocate().unwrap();
        assert_eq!(allocator.remaining(), 2);
        allocator.release(frame).unwrap();
        assert_eq!(allocator.remaining(), 3);
        assert_eq!(allocator.release(frame), Err(AllocatorError::InvalidRange));
        assert_eq!(allocator.allocate(), Ok(frame));
        allocator.release(frame).unwrap();
        allocator.check_invariants(0x4000, 0x5000).unwrap();
    }

    #[test]
    fn allocator_skips_exhausted_ranges_and_reuses_recycled_frames() {
        let regions = [usable(0x0, 0x1000), usable(0x3000, 0x5000)];
        let mut allocator = FreeFrameAllocator::from_regions(&regions, 0x6000, 0x7000).unwrap();
        assert_eq!(allocator.allocate(), Ok(0));
        assert_eq!(allocator.next_range, 1);
        allocator.release(0).unwrap();
        assert_eq!(allocator.allocate(), Ok(0));
        assert_eq!(allocator.next_range, 1);
        assert_eq!(allocator.allocate(), Ok(0x3000));
        assert_eq!(allocator.allocate(), Ok(0x4000));
        assert_eq!(allocator.next_range, allocator.range_count);
        assert_eq!(allocator.allocate(), Err(AllocatorError::NoFrame));
        allocator.check_invariants(0x6000, 0x7000).unwrap();
    }

    #[test]
    fn allocator_detects_recycled_frame_in_free_tail() {
        let regions = [usable(0x0, 0x4000)];
        let mut allocator = FreeFrameAllocator::from_regions(&regions, 0x5000, 0x6000).unwrap();
        allocator.allocate().unwrap();
        allocator.recycled[0] = 0x2000;
        allocator.recycled_count = 1;
        assert_eq!(
            allocator.check_invariants(0x5000, 0x6000),
            Err(AllocatorError::InvariantViolation)
        );
    }

    #[test]
    fn allocator_detects_duplicate_recycled_frame() {
        let regions = [usable(0x0, 0x4000)];
        let mut allocator = FreeFrameAllocator::from_regions(&regions, 0x5000, 0x6000).unwrap();
        let first = allocator.allocate().unwrap();
        let second = allocator.allocate().unwrap();
        allocator.release(first).unwrap();
        allocator.release(second).unwrap();
        allocator.recycled[1] = first;
        assert_eq!(
            allocator.check_invariants(0x5000, 0x6000),
            Err(AllocatorError::InvariantViolation)
        );
    }

    #[test]
    fn counts_frames_in_a_single_usable_region() {
        let regions = [usable(0x0, 0x1_0000), bootloader(0x100_0000, 0x100_1000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 16);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 16); // == usable，镜像不扣减
        assert_eq!(inv.usable_regions, 1);
        assert_eq!(inv.kernel_phys_start, 0x100_0000);
        assert_eq!(inv.kernel_phys_end, 0x100_1000);
    }

    #[test]
    fn drops_unaligned_region_boundary_pages() {
        let regions = [usable(0x0800, 0x5200), bootloader(0x100_0000, 0x100_1000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.free_frames, 4);
    }

    #[test]
    fn ignores_non_usable_regions() {
        let regions = [
            bootloader(0x0, 0x1000),
            usable(0x1000, 0x5000),
            MemoryRegion {
                start: 0x5000,
                end: 0x6000,
                kind: MemoryRegionKind::UnknownBios(2),
            },
            bootloader(0x100_0000, 0x100_1000),
        ];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.usable_regions, 1);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 4);
    }

    #[test]
    fn image_frames_reported_but_not_subtracted() {
        let regions = [usable(0x0, 0x1_0000), bootloader(0x100_0000, 0x100_2000)];
        let inv = build_inventory(&regions, 0x100_0000, 0x2000).unwrap();
        assert_eq!(inv.image_frames, 2);
        assert_eq!(inv.free_frames, 16); // 镜像已从 Usable 排除，free == usable
    }

    #[test]
    fn rounds_unaligned_kernel_image_up_to_full_frames() {
        let regions = [bootloader(0x0, 0x2000), usable(0x2000, 0x1_2000)];
        let inv = build_inventory(&regions, 0x1800, 0x800).unwrap();
        assert_eq!(inv.kernel_phys_start, 0x1000);
        assert_eq!(inv.kernel_phys_end, 0x2000);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.usable_frames, 16);
        assert_eq!(inv.free_frames, 16);
    }

    #[test]
    fn rejects_empty_kernel_image() {
        let regions = [usable(0x0, 0x1_0000)];
        assert_eq!(
            build_inventory(&regions, 0x1000, 0),
            Err(MemError::EmptyImage)
        );
    }

    #[test]
    fn rejects_image_overlapping_usable() {
        let regions = [usable(0x0, 0x5000)];
        assert_eq!(
            build_inventory(&regions, 0x1000, 0x1000),
            Err(MemError::ImageOverlapsUsable)
        );
    }

    #[test]
    fn rejects_overlapping_regions() {
        let regions = [usable(0x0, 0x3000), usable(0x2000, 0x5000)];
        assert_eq!(
            build_inventory(&regions, 0x1000, 0x1000),
            Err(MemError::RegionsNotSortedOrOverlapping)
        );
    }

    #[test]
    fn sums_multiple_usable_regions() {
        let regions = [
            usable(0x0, 0x2000),
            bootloader(0x2000, 0x3000),
            usable(0x3000, 0x7000),
            bootloader(0x100_0000, 0x100_1000),
        ];
        let inv = build_inventory(&regions, 0x100_0000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 6);
        assert_eq!(inv.usable_regions, 2);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 6);
    }

    #[test]
    fn image_in_bootloader_does_not_reduce_free() {
        let regions = [
            usable(0x0, 0x3000),
            bootloader(0x4000, 0x5000), // 镜像落在该 Bootloader 区，不计入 usable
            usable(0x5000, 0x8000),
        ];
        let inv = build_inventory(&regions, 0x4000, 0x1000).unwrap();
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.usable_frames, 6);
        assert_eq!(inv.free_frames, 6);
    }

    #[test]
    fn phys_to_virt_adds_offset() {
        assert_eq!(
            phys_to_virt(0x1000, 0xffff_8000_0000_0000),
            0xffff_8000_0000_1000
        );
    }

    #[test]
    fn phys_to_virt_zero_offset_keeps_phys() {
        assert_eq!(phys_to_virt(0xc000_0000, 0), 0xc000_0000);
    }

    #[test]
    fn phys_to_virt_large_phys_near_limit() {
        // 64 GiB 物理上限 + 高半区偏移，不越界（纯算术，无 panic）
        assert_eq!(
            phys_to_virt(0x10_0000_0000, 0xffff_8000_0000_0000),
            0xffff_8010_0000_0000
        );
    }

    #[test]
    fn probe_returns_va_and_byte_shape() {
        // 用无害的堆分配器测试：probe 一个 0 字节不可靠，只验证返回值类型与 va=offset+phys。
        // 这里不真正读物理，改用 phys_to_virt 单测覆盖算术；probe 的真实性由 QEMU 验证。
        let va = phys_to_virt(0x1000, 0xffff_8000_0000_0000);
        assert_eq!(va, 0xffff_8000_0000_1000);
    }

    #[test]
    fn first_usable_frame_returns_aligned_start() {
        let regions = [bootloader(0x0, 0x1000), usable(0x0800, 0x5200)];
        assert_eq!(first_usable_frame_phys(&regions), Some(0x1000)); // align_up(0x800, 4096)
    }

    #[test]
    fn first_usable_frame_none_when_no_usable() {
        let regions = [bootloader(0x0, 0x1000)];
        assert_eq!(first_usable_frame_phys(&regions), None);
    }
}
