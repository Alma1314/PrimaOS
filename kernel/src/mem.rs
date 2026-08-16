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
    ImageNotUsable,
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

    let mut image_in_usable = 0usize;
    for region in regions {
        if region.kind != MemoryRegionKind::Usable {
            continue;
        }
        let lo = align_up(region.start, FRAME_SIZE).max(kernel_phys_start);
        let hi = align_down(region.end, FRAME_SIZE).min(kernel_phys_end);
        if hi > lo {
            image_in_usable += ((hi - lo) / FRAME_SIZE) as usize;
        }
    }
    if image_in_usable < image_frames {
        return Err(MemError::ImageNotUsable);
    }

    Ok(FrameInventory {
        usable_frames,
        image_frames,
        free_frames: usable_frames - image_frames,
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
        let regions = [usable(0x0, 0x1_0000)];
        let inv = build_inventory(&regions, 0x0, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 16);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 15);
        assert_eq!(inv.usable_regions, 1);
        assert_eq!(inv.kernel_phys_start, 0x0);
        assert_eq!(inv.kernel_phys_end, 0x1000);
    }

    #[test]
    fn drops_unaligned_region_boundary_pages() {
        let regions = [usable(0x0800, 0x5200)];
        let inv = build_inventory(&regions, 0x1000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.free_frames, 3);
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
        ];
        let inv = build_inventory(&regions, 0x1000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 4);
        assert_eq!(inv.usable_regions, 1);
    }

    #[test]
    fn reserves_kernel_image_frames_from_free() {
        let regions = [usable(0x0, 0x1_0000)];
        let inv = build_inventory(&regions, 0x2000, 0x2000).unwrap();
        assert_eq!(inv.image_frames, 2);
        assert_eq!(inv.free_frames, 14);
    }

    #[test]
    fn rounds_unaligned_kernel_image_up_to_full_frames() {
        let regions = [usable(0x0, 0x1_0000)];
        let inv = build_inventory(&regions, 0x1800, 0x800).unwrap();
        assert_eq!(inv.kernel_phys_start, 0x1000);
        assert_eq!(inv.kernel_phys_end, 0x2000);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 15);
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
    fn rejects_kernel_image_outside_usable() {
        let regions = [bootloader(0x0, 0x2000), usable(0x2000, 0x5000)];
        assert_eq!(
            build_inventory(&regions, 0x0, 0x1000),
            Err(MemError::ImageNotUsable)
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
        ];
        let inv = build_inventory(&regions, 0x1000, 0x1000).unwrap();
        assert_eq!(inv.usable_frames, 6);
        assert_eq!(inv.usable_regions, 2);
        assert_eq!(inv.image_frames, 1);
        assert_eq!(inv.free_frames, 5);
    }

    #[test]
    fn accepts_kernel_image_spanning_adjacent_usable_regions() {
        let regions = [usable(0x0, 0x3000), usable(0x3000, 0x6000)];
        let inv = build_inventory(&regions, 0x2000, 0x2000).unwrap();
        assert_eq!(inv.image_frames, 2);
        assert_eq!(inv.free_frames, 4);
    }
}
