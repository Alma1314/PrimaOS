//! 帧唯一归属注册表（COSIR §16 W6 / §14 步骤 1）。
//!
//! W6 要求 `frame_universe` 划分为三类**两两不交**的帧：`free_frames`、所有活动 CPU Segment
//! 的帧集合、保留帧（其中包含细化层自 `frame_universe` 分配、当前未映射进任何活动 Segment
//! 的**页表帧**）。§14 步骤 1 进一步要求 `free_frames = frame_universe \ image_frames`，即
//! image 帧**属于** universe 且由 image Segment 持有。
//!
//! 本模块是该不变量的单一归属点：所有取帧都必须经 [`allocate_for_segment`]（记入某活动
//! Segment）或 [`allocate_table_frame`]（记入保留页表帧），不存在不带归属的取帧 API。
//! 归属**不交性由构造保证**（单一分配器 + 带标签的取帧入口），**守恒性由计数校验**
//! （`free + segment + table + image == universe`）——任何绕过标签入口的分配都会让守恒式失衡。
//!
//! 先前实现有两处违反：`cow.rs` 把 `FreeFrameAllocator` 的**副本**存入上下文（两侧 `next`
//! 可各自推进），以及页表帧取走后完全无归属记账。本模块同时修掉这两点。

use crate::mem::{FrameInventory, FreeFrameAllocator};

/// 平台 bring-up 的活动 Segment ID（S1-X3 的归属标签；stage 2 的 `memseg_create` 接管后
/// 这些标签会被真实 Segment 对象取代）。ID 0 保留给 bootstrap 的 image Segment（其帧由
/// `image_frames` 类单独计数，不经 `allocate_for_segment`）。
pub const SEG_VSPACE: u32 = 1;
pub const SEG_COW: u32 = 2;
pub const SEG_TASK1_CODE: u32 = 3;
pub const SEG_TASK1_STACK: u32 = 4;
pub const SEG_TASK1_X: u32 = 5;
pub const SEG_TASK1_Y: u32 = 6;
pub const SEG_TASK2_CODE: u32 = 7;
pub const SEG_TASK2_STACK: u32 = 8;
pub const SEG_TASK2_Y: u32 = 9;
pub const SEG_SPIN1_CODE: u32 = 10;
pub const SEG_SPIN1_DATA: u32 = 11;
pub const SEG_SPIN2_CODE: u32 = 12;
pub const SEG_SPIN2_DATA: u32 = 13;

/// 保留页表帧的容量上限（自建页表规模远小于此）。
pub const MAX_TABLE_FRAMES: usize = 64;
/// 活动 Segment 的容量上限（image + 平台 bring-up 各区域）。
pub const MAX_SEGMENTS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    Cpu,
    Gpu,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    NotInstalled,
    NoFrame,
    UnknownSegment,
    CapacityExceeded,
    UniverseMismatch,
    ConservationViolation,
}

/// W6 守恒见证（供协议事件与 host 单测使用）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameWitness {
    pub universe: u64,
    pub free: u64,
    pub segment_frames: u64,
    pub table_frames: u64,
    pub image_frames: u64,
    pub conserved: bool,
}

/// 帧归属注册表（纯逻辑，无 asm；全局安装见 [`install`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRegistry {
    alloc: FreeFrameAllocator,
    universe: usize,
    image_frames: usize,
    segment_frames: usize,
    table_frames: [u64; MAX_TABLE_FRAMES],
    table_count: usize,
    segments: [Option<(u32, Pool)>; MAX_SEGMENTS],
    segment_count: usize,
}

impl FrameRegistry {
    /// 按 §14 步骤 1 建立注册表：`free_frames = frame_universe \ image_frames`。
    pub fn new(inventory: &FrameInventory, alloc: FreeFrameAllocator) -> Result<Self, FrameError> {
        if inventory.kernel_phys_start >= inventory.kernel_phys_end
            || inventory.image_frames == 0
            || inventory.free_frames != inventory.usable_frames
            || alloc.remaining() != inventory.free_frames
        {
            return Err(FrameError::UniverseMismatch);
        }
        Ok(Self {
            alloc,
            universe: inventory.usable_frames + inventory.image_frames,
            image_frames: inventory.image_frames,
            segment_frames: 0,
            table_frames: [0; MAX_TABLE_FRAMES],
            table_count: 0,
            segments: [None; MAX_SEGMENTS],
            segment_count: 0,
        })
    }

    /// 登记一个活动 Segment。其帧由后续 [`allocate_for_segment`] 逐个归入。
    ///
    /// image Segment 是例外：它的帧就是 §14 的 `image_frames` 类，已在构造时计入
    /// [`FrameRegistry::image_frames`]，不经本函数计数（避免同一批帧被计两次）。
    pub fn open_segment(&mut self, id: u32, pool: Pool) -> Result<(), FrameError> {
        if self.segments[..self.segment_count]
            .iter()
            .flatten()
            .any(|(existing, _)| *existing == id)
        {
            return Err(FrameError::UnknownSegment);
        }
        if self.segment_count == MAX_SEGMENTS {
            return Err(FrameError::CapacityExceeded);
        }
        self.segments[self.segment_count] = Some((id, pool));
        self.segment_count += 1;
        Ok(())
    }

    /// 为一个活动 Segment 取一帧并归入其名下。
    pub fn allocate_for_segment(&mut self, id: u32) -> Result<u64, FrameError> {
        if !self.segments[..self.segment_count]
            .iter()
            .flatten()
            .any(|(existing, _)| *existing == id)
        {
            return Err(FrameError::UnknownSegment);
        }
        let frame = self.alloc.allocate().map_err(|_| FrameError::NoFrame)?;
        self.segment_frames += 1;
        Ok(frame)
    }

    /// 取一帧作为**保留页表帧**（W6：未映射进活动 Segment 的页表帧）。
    pub fn allocate_table_frame(&mut self) -> Result<u64, FrameError> {
        if self.table_count == MAX_TABLE_FRAMES {
            return Err(FrameError::CapacityExceeded);
        }
        let frame = self.alloc.allocate().map_err(|_| FrameError::NoFrame)?;
        self.table_frames[self.table_count] = frame;
        self.table_count += 1;
        Ok(frame)
    }

    /// W6 守恒：`free + segment + table + image == universe`。
    ///
    /// 不交性由构造保证：每个帧只可能被 [`allocate_for_segment`] 或
    /// [`allocate_table_frame`] 取走一次，image 帧与 Usable 帧在 `mem::build_inventory`
    /// 与 `FreeFrameAllocator::from_regions` 中已校验不相交。
    pub fn check_conservation(&self) -> Result<(), FrameError> {
        let accounted =
            self.alloc.remaining() + self.segment_frames + self.table_count + self.image_frames;
        if accounted == self.universe {
            Ok(())
        } else {
            Err(FrameError::ConservationViolation)
        }
    }

    pub fn witness(&self) -> FrameWitness {
        FrameWitness {
            universe: self.universe as u64,
            free: self.alloc.remaining() as u64,
            segment_frames: self.segment_frames as u64,
            table_frames: self.table_count as u64,
            image_frames: self.image_frames as u64,
            conserved: self.check_conservation().is_ok(),
        }
    }

    pub fn free_frames(&self) -> usize {
        self.alloc.remaining()
    }
}

static mut REGISTRY: Option<FrameRegistry> = None;

/// 安装全局帧注册表（boot 早期一次；进入任何取帧路径之前）。
pub fn install(inventory: &FrameInventory, alloc: FreeFrameAllocator) -> Result<(), FrameError> {
    let registry = FrameRegistry::new(inventory, alloc)?;
    registry.check_conservation()?;
    unsafe {
        let slot = core::ptr::addr_of_mut!(REGISTRY);
        *slot = Some(registry);
    }
    Ok(())
}

/// 在已安装的全局注册表上执行操作；未安装时返回 [`FrameError::NotInstalled`]。
fn global() -> Result<*mut FrameRegistry, FrameError> {
    unsafe {
        let slot = core::ptr::addr_of_mut!(REGISTRY);
        match (*slot).as_mut() {
            Some(registry) => Ok(registry as *mut FrameRegistry),
            None => Err(FrameError::NotInstalled),
        }
    }
}

/// 登记一个活动 Segment（其帧随后经 [`allocate_for_segment`] 归入）。
pub fn open_segment(id: u32, pool: Pool) -> Result<(), FrameError> {
    unsafe { (*global()?).open_segment(id, pool) }
}

/// 为一个活动 Segment 分配帧（唯一的数据帧取帧入口）。
pub fn allocate_for_segment(id: u32) -> Result<u64, FrameError> {
    unsafe { (*global()?).allocate_for_segment(id) }
}

/// 分配一帧保留页表帧（唯一的页表帧取帧入口）。
pub fn allocate_table_frame() -> Result<u64, FrameError> {
    unsafe { (*global()?).allocate_table_frame() }
}

/// 观察全局注册表的守恒状态（见证事件用）。
pub fn witness() -> Result<FrameWitness, FrameError> {
    let registry = global()?;
    unsafe { Ok((*registry).witness()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bootloader_api::info::{MemoryRegion, MemoryRegionKind};

    /// 两块 Usable（共 6 帧）+ 镜像 2 帧：universe = 8。
    fn regions() -> [MemoryRegion; 3] {
        [
            MemoryRegion {
                start: 0x10_0000,
                end: 0x10_3000,
                kind: MemoryRegionKind::Usable,
            },
            MemoryRegion {
                start: 0x20_0000,
                end: 0x20_3000,
                kind: MemoryRegionKind::Usable,
            },
            MemoryRegion {
                start: 0x30_0000,
                end: 0x30_1000,
                kind: MemoryRegionKind::Usable,
            },
        ]
    }

    fn registry() -> FrameRegistry {
        let regions = regions();
        let inventory = FrameInventory {
            usable_frames: 7,
            image_frames: 2,
            free_frames: 7,
            usable_regions: 3,
            kernel_phys_start: 0x40_0000,
            kernel_phys_end: 0x40_2000,
        };
        let alloc = FreeFrameAllocator::from_regions(&regions, 0x40_0000, 0x40_2000).unwrap();
        FrameRegistry::new(&inventory, alloc).unwrap()
    }

    #[test]
    fn universe_includes_image_and_starts_conserved() {
        let registry = registry();
        assert_eq!(registry.witness().universe, 9);
        assert_eq!(registry.witness().free, 7);
        assert_eq!(registry.witness().image_frames, 2);
        assert!(registry.check_conservation().is_ok());
    }

    #[test]
    fn segment_and_table_allocations_stay_conserved() {
        let mut registry = registry();
        // image Segment 持有 2 帧。
        registry.open_segment(1, Pool::Cpu).unwrap();
        // bring-up Segment 取 3 帧。
        registry.open_segment(2, Pool::Cpu).unwrap();
        for _ in 0..3 {
            registry.allocate_for_segment(2).unwrap();
        }
        // 自建页表 2 帧。
        registry.allocate_table_frame().unwrap();
        registry.allocate_table_frame().unwrap();

        let witness = registry.witness();
        assert_eq!(witness.free, 2);
        // image Segment 的 2 帧计入 image_frames 类，不重复计入 segment_frames。
        assert_eq!(witness.segment_frames, 3);
        assert_eq!(witness.image_frames, 2);
        assert_eq!(witness.table_frames, 2);
        assert!(witness.conserved);
        assert!(registry.check_conservation().is_ok());
    }

    #[test]
    fn untagged_free_frame_loss_breaks_conservation() {
        let mut registry = registry();
        registry.open_segment(1, Pool::Cpu).unwrap();
        // 绕过标签入口直接取帧：free 减少但没有任何归属计数增加。
        registry.alloc.allocate().unwrap();
        assert_eq!(
            registry.check_conservation(),
            Err(FrameError::ConservationViolation)
        );
        assert!(!registry.witness().conserved);
    }

    #[test]
    fn unknown_segment_is_rejected() {
        let mut registry = registry();
        assert_eq!(
            registry.allocate_for_segment(99),
            Err(FrameError::UnknownSegment)
        );
    }

    #[test]
    fn duplicate_segment_id_is_rejected() {
        let mut registry = registry();
        registry.open_segment(1, Pool::Cpu).unwrap();
        assert_eq!(
            registry.open_segment(1, Pool::Cpu),
            Err(FrameError::UnknownSegment)
        );
    }

    #[test]
    fn table_frame_capacity_is_enforced() {
        // 构造一个帧数远超 MAX_TABLE_FRAMES 的 universe，验证容量上限先于取帧耗尽触发。
        let regions = [MemoryRegion {
            start: 0x10_0000,
            end: 0x10_0000 + 0x1000 * 200,
            kind: MemoryRegionKind::Usable,
        }];
        let inventory = FrameInventory {
            usable_frames: 200,
            image_frames: 1,
            free_frames: 200,
            usable_regions: 1,
            kernel_phys_start: 0x40_0000,
            kernel_phys_end: 0x40_1000,
        };
        let alloc = FreeFrameAllocator::from_regions(&regions, 0x40_0000, 0x40_1000).unwrap();
        let mut registry = FrameRegistry::new(&inventory, alloc).unwrap();
        for _ in 0..MAX_TABLE_FRAMES {
            registry.allocate_table_frame().unwrap();
        }
        assert_eq!(
            registry.allocate_table_frame(),
            Err(FrameError::CapacityExceeded)
        );
        assert!(registry.check_conservation().is_ok());
    }

    #[test]
    fn allocator_exhaustion_is_reported() {
        let mut registry = registry();
        registry.open_segment(1, Pool::Cpu).unwrap();
        for _ in 0..7 {
            registry.allocate_for_segment(1).unwrap();
        }
        assert_eq!(registry.allocate_for_segment(1), Err(FrameError::NoFrame));
        assert!(registry.check_conservation().is_ok());
    }

    #[test]
    fn inventory_mismatch_is_rejected() {
        let regions = regions();
        let inventory = FrameInventory {
            usable_frames: 7,
            image_frames: 2,
            free_frames: 6,
            usable_regions: 3,
            kernel_phys_start: 0x40_0000,
            kernel_phys_end: 0x40_2000,
        };
        let alloc = FreeFrameAllocator::from_regions(&regions, 0x40_0000, 0x40_2000).unwrap();
        assert_eq!(
            FrameRegistry::new(&inventory, alloc),
            Err(FrameError::UniverseMismatch)
        );
    }
}
