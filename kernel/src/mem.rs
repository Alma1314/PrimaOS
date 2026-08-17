use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

pub const FRAME_SIZE: u64 = 4096;

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
}
