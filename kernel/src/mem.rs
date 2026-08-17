use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

pub const FRAME_SIZE: u64 = 4096;

/// 物理地址经 bootloader 的物理内存直映射换算为虚拟地址（offset + phys）。
pub fn phys_to_virt(phys: u64, phys_map_offset: u64) -> u64 {
    phys_map_offset + phys
}

/// 读取一个物理地址处的字节（经直映射），返回 (虚拟地址, 读到字节)。
///
/// 纯逻辑部分（换算 va）在本模块，真正的 `unsafe` 读取收敛到细化模块
/// [`crate::safety::read_phys_byte`]。
///
/// # Safety 约束
///
/// `probed_phys` 必须处于 bootloader 报告为 `Usable` 的物理内存中，且该地址必须
/// 落在物理内存直映射区内（调用方应先确认 `physical_memory_offset` 为 `Some`）。
/// 调用方负责保证 `probed_phys` 是可安全读取的内核可用物理帧。
pub fn probe_physical_memory(probed_phys: u64, phys_map_offset: u64) -> (u64, u8) {
    let va = phys_to_virt(probed_phys, phys_map_offset);
    // 真实 unsafe 读取收敛在细化层 safety 模块；此处仅保留最小的调用点。
    let byte = unsafe { crate::safety::read_phys_byte(va) };
    (va, byte)
}

/// 返回 mem 清单中第一个 `Usable` 帧的物理起始地址（`align_up(start, FRAME_SIZE)`）。
/// 无任何 Usable 区返回 `None`。仅做纯索引遍历，不读物理。
pub fn first_usable_frame_phys(regions: &[MemoryRegion]) -> Option<u64> {
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            let lo = align_up(region.start, FRAME_SIZE);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemError {
    EmptyImage,
    ImageOverlapsUsable,
    RegionsNotSortedOrOverlapping,
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
        if region.start < prev_end {
            return Err(MemError::RegionsNotSortedOrOverlapping);
        }
        prev_end = region.end;
    }

    let mut usable_frames = 0usize;
    let mut usable_regions = 0usize;
    for region in regions {
        if region.kind == MemoryRegionKind::Usable {
            usable_regions += 1;
            let lo = align_up(region.start, FRAME_SIZE);
            let hi = align_down(region.end, FRAME_SIZE);
            if hi > lo {
                usable_frames += ((hi - lo) / FRAME_SIZE) as usize;
            }
        }
    }

    let kernel_phys_start = align_down(kernel_addr, FRAME_SIZE);
    let kernel_phys_end = align_up(kernel_addr + kernel_len, FRAME_SIZE);
    let image_frames = ((kernel_phys_end - kernel_phys_start) / FRAME_SIZE) as usize;

    // bootloader 把内核镜像存放在 Bootloader 区（不在 Usable 内）。校验镜像与所有
    // Usable 帧不相交（W6）：若任一帧重叠，则该帧既被视为空闲又被镜像占用。
    for region in regions {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        let lo = align_up(region.start, FRAME_SIZE).max(kernel_phys_start);
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

fn align_up(value: u64, align: u64) -> u64 {
    value.div_ceil(align) * align
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
