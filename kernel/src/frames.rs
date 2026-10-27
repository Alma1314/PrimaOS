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
//!
//! SPEC-068（ISSUE-FRAME-001 收尾）：image 帧集合原以 `(start, end)` **区间**表达，
//! 本模块改为**真实逐帧向量** [`FrameRegistry::image_frame_ids`]（§5.1 `frames: Vector<FrameId>`）。
//! 向量随注册表一起落在 `BootstrapState` **之外**（后者已贴近 16 KiB 结构体守卫），
//! 逐帧归属查询与不交性判定都覆盖它，不再有「区间判断」这条旁路。

use crate::mem::{FrameInventory, FreeFrameAllocator};
use prima_ostd::sync::StaticCell;

/// 平台 bring-up 的活动 Segment ID（S1-X3 的归属标签；切片 R 起这些标签**就是** Segment 槽位 id：
/// `frame_label` 的独立命名空间已取消，归属标签与 `bootstrap::SegmentObject::id` 恒等）。
///
/// id 1 是 bootstrap 的 image Segment（其帧由 `image_frames` 类单独计数，从不经
/// `allocate_for_segment`，故不在此列）；id 2..14 是平台 bring-up 的 13 个区域；
/// 用户经 `memseg_create` 创建的 Segment 从 `bootstrap::USER_SEGMENT_ID_FIRST`(=15) 起分配。
pub const SEG_VSPACE: u32 = 2;
pub const SEG_COW: u32 = 3;
pub const SEG_TASK1_CODE: u32 = 4;
pub const SEG_TASK1_STACK: u32 = 5;
pub const SEG_TASK1_X: u32 = 6;
pub const SEG_TASK1_Y: u32 = 7;
pub const SEG_TASK2_CODE: u32 = 8;
pub const SEG_TASK2_STACK: u32 = 9;
pub const SEG_TASK2_Y: u32 = 10;
pub const SEG_SPIN1_CODE: u32 = 11;
pub const SEG_SPIN1_DATA: u32 = 12;
pub const SEG_SPIN2_CODE: u32 = 13;
pub const SEG_SPIN2_DATA: u32 = 14;

/// 保留页表帧的容量上限（自建页表规模远小于此）。
///
/// SPEC-106：从 128 提到 **256**——S4-D3 的服务表扩到 5 项后，每个任务根各有一套页表结点，
/// `BETA` 收到转发的共享段做 `memseg_map` 时 `allocate_table_frame` 返回 `Err`，`memseg_map`
/// 以 `NoMemory` 失败，协商与 `done` 均不产出。该表是全局帧注册表的一块静态数组，不占
/// `BootstrapState` 空间。
pub const MAX_TABLE_FRAMES: usize = 256;
/// 活动 Segment 的容量上限（image + 平台 bring-up 各区域 + 用户创建 Segment）。
///
/// SPEC-106：从 32 提到 **48**，与 `bootstrap::MAX_SEGMENT_OBJECTS` 对齐——S4-D3 的三个基础
/// 服务端点各占一个代码段与一个栈段，段表在 32 时触顶使后续 `memseg_create` 失败。
pub const MAX_SEGMENTS: usize = 48;
/// 非 image 数据帧的逐帧归属条目上限（S2-D3 的 frame → owner 查询）。
///
/// **M3b 192 → 256**：C 任务的栈与匿名 arena 合并为一个 72 页 Segment。旧上限
/// 192 下 8 页栈段仍可创建，故创建前的已用条目 ≤ 184；栈 + arena 的 72 页使
/// 256 成为可证明的上界。存储已移至 [`OWNER_ARENA`]，不占结构体预算。
///
/// **M9 256 → 320**：C 任务的栈 + arena 段从 72 页扩到 104 页（sbase `ls` 的
/// 64 KiB visited 表需要更大的 brk 区）。按同一上界算式 184 + 104 = 288，320 留出
/// 32 项余量；`memseg_create` 仍以 `owner_count == MAX_FRAME_OWNERS` 确定失败。
/// The exec handoff can retain the old arena, prepare up to 149 image/stack
/// frames, and privatize all 96 retained heap pages: 320+149+96+3=568 entries.
/// With the 192-page boot vector the bound is 320+192+96+3=611. Keep 640
/// external owner entries; the matching recycle budget must also fit the
/// FrameRegistry's unchanged 32 KiB value-size guard.
pub const MAX_FRAME_OWNERS: usize = 640;
/// SPEC-068：image 帧向量容量（§5.1 的 `frames: Vector<FrameId>`）。
///
/// image 帧数随内核镜像体积增长（实测 663→778 帧），远超任何内联进 `BootstrapState`
/// 的定长数组所能承受的规模，故该向量物化在**注册表内部的独立数组**（注册表本身是
/// boot 早期安装的静态，不占结构体守卫额度）。超出即 `CapacityExceeded`——内核镜像
/// 超过 8 MiB 时启动失败，显式报错而非静默截断。
pub const MAX_IMAGE_FRAMES: usize = 2048;

/// 归属表 arena 的区段数：内核 1 段，host 单测并行构造多个注册表。
#[cfg(not(test))]
const OWNER_REGIONS: usize = 1;
#[cfg(test)]
const OWNER_REGIONS: usize = 1024;

/// 非 image 帧归属表的静态存储（每注册表实例一段，见 [`FrameRegistry`]）。
static OWNER_ARENA: crate::arena::Arena<(u64, u32), MAX_FRAME_OWNERS, OWNER_REGIONS> =
    crate::arena::Arena::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pool {
    Cpu,
    Gpu,
}

/// 一帧的归属类别（W6 的三类划分 + image 类）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameOwner {
    /// image 区间内的帧（由 image Segment 持有）。
    Image,
    /// 某个活动 Segment 的数据帧（值为归属标签）。
    Segment(u32),
    /// 保留页表帧（未映射进任何活动 Segment）。
    Table,
    /// 空闲帧。
    Free,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    NotInstalled,
    NoFrame,
    UnknownSegment,
    CapacityExceeded,
    UniverseMismatch,
    ConservationViolation,
    /// 帧不在该归属名下（释放失败，状态不变）。
    NotOwned,
    /// W6 不交性被破坏（同一帧被计入两类归属）。
    Overlap,
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
    /// W6 逐帧不交性（`check_disjoint`）。
    pub disjoint: bool,
}

/// 帧归属注册表（纯逻辑，无 asm；全局安装见 [`install`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameRegistry {
    alloc: FreeFrameAllocator,
    universe: usize,
    /// SPEC-068：image Segment 的**真实逐帧向量**（§5.1 `Vector<FrameId>`）。
    ///
    /// 第 `i` 项即 image 的第 `i` 帧物理地址，`image_count` 是其长度。该向量是 image
    /// 帧归属的**唯一真值源**（逐帧查询与不交性判定都读它），物化在注册表内部而非
    /// `BootstrapState`，不占后者的结构体守卫额度。
    image_frame_ids: [u64; MAX_IMAGE_FRAMES],
    image_count: usize,
    segment_frames: usize,
    table_frames: [u64; MAX_TABLE_FRAMES],
    table_count: usize,
    /// 非 image 数据帧的 `(frame, segment_label)` 归属表（逐帧 owner 查询与不交性判定）。
    ///
    /// 存储移出结构体（与 vspace/audit 同一 arena 模式）：256 项内联会把
    /// `FrameRegistry` 推过 32 KiB 按值预算；arena 的段数只在 host 单测放大。
    owner_slice: Option<crate::arena::ArenaSlice>,
    owner_count: usize,
    segments: [Option<(u32, Pool)>; MAX_SEGMENTS],
    segment_count: usize,
}

impl FrameRegistry {
    /// 第 `index` 项归属（未初始化或越界返回 `None`）。
    fn owner(&self, index: usize) -> Option<(u64, u32)> {
        self.owner_slice
            .and_then(|slice| OWNER_ARENA.get(slice, index))
    }

    /// 写第 `index` 项归属（越界为 no-op）。
    fn set_owner(&mut self, index: usize, value: Option<(u64, u32)>) {
        if let Some(slice) = self.owner_slice {
            OWNER_ARENA.set(slice, index, value);
        }
    }

    /// 空注册表（`const`；供 [`install`] 在静态上**就地**构造）。
    ///
    /// SPEC-068：image 向量使注册表达 ~27 KiB。`FrameRegistry` 若是"构造后按值返回"，
    /// boot 早期会在 256 KiB 启动栈上叠出多份副本，实测直接三重故障；故内核路径改为
    /// [`Self::init_at`] 就地写入，`empty()` 只作 `const` 初值、不产生运行时临时量。
    pub const fn empty() -> Self {
        Self {
            alloc: FreeFrameAllocator::empty(),
            universe: 0,
            image_frame_ids: [0; MAX_IMAGE_FRAMES],
            image_count: 0,
            segment_frames: 0,
            table_frames: [0; MAX_TABLE_FRAMES],
            table_count: 0,
            owner_slice: None,
            owner_count: 0,
            segments: [None; MAX_SEGMENTS],
            segment_count: 0,
        }
    }

    /// 按 §14 步骤 1 就地建立注册表：`free_frames = frame_universe \ image_frames`。
    ///
    /// 全部前置校验先于任何字段写入，故失败时 `self` 保持调用前状态。
    pub fn init_at(
        &mut self,
        inventory: &FrameInventory,
        alloc: FreeFrameAllocator,
    ) -> Result<(), FrameError> {
        if inventory.kernel_phys_start >= inventory.kernel_phys_end
            || inventory.image_frames == 0
            || inventory.free_frames != inventory.usable_frames
            || alloc.remaining() != inventory.free_frames
        {
            return Err(FrameError::UniverseMismatch);
        }
        if inventory.image_frames > MAX_IMAGE_FRAMES {
            return Err(FrameError::CapacityExceeded);
        }
        self.alloc = alloc;
        self.universe = inventory.usable_frames + inventory.image_frames;
        self.image_count = inventory.image_frames;
        // 物化真实帧向量：第 i 项即 image 的第 i 帧物理地址。
        // `mem::build_inventory` 由**帧对齐**的 `[kernel_phys_start, kernel_phys_end)` 导出
        // `image_frames`，故逐帧物化与旧的区间判断是同一帧集合（host 单测钉住这一点）。
        for (index, slot) in self.image_frame_ids[..self.image_count]
            .iter_mut()
            .enumerate()
        {
            *slot = inventory.kernel_phys_start + (index as u64) * crate::mem::FRAME_SIZE;
        }
        if self.owner_slice.is_none() {
            self.owner_slice = Some(OWNER_ARENA.claim());
        }
        self.segment_frames = 0;
        self.table_count = 0;
        self.owner_count = 0;
        self.segment_count = 0;
        Ok(())
    }

    /// 按值构造注册表（host 测试与 `new` 语义保留；内核 boot 路径用就地 [`install`]）。
    pub fn new(inventory: &FrameInventory, alloc: FreeFrameAllocator) -> Result<Self, FrameError> {
        let mut registry = Self::empty();
        registry.init_at(inventory, alloc)?;
        Ok(registry)
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

    /// 关闭一个活动 Segment 的归属标签（其帧必须已全部归还），使该标签可被复用。
    pub fn close_segment(&mut self, id: u32) -> Result<(), FrameError> {
        let Some(index) = self.segments[..self.segment_count]
            .iter()
            .position(|entry| entry.is_some_and(|(existing, _)| existing == id))
        else {
            return Err(FrameError::UnknownSegment);
        };
        for index in 0..self.owner_count {
            if self.owner(index).is_some_and(|(_, owner)| owner == id) {
                return Err(FrameError::NotOwned);
            }
        }
        for shift in index..self.segment_count - 1 {
            self.segments[shift] = self.segments[shift + 1];
        }
        self.segments[self.segment_count - 1] = None;
        self.segment_count -= 1;
        Ok(())
    }

    /// 为一个活动 Segment 取一帧并归入其名下（同时登记逐帧 owner）。
    pub fn allocate_for_segment(&mut self, id: u32) -> Result<u64, FrameError> {
        if !self.segments[..self.segment_count]
            .iter()
            .flatten()
            .any(|(existing, _)| *existing == id)
        {
            return Err(FrameError::UnknownSegment);
        }
        if self.owner_count == MAX_FRAME_OWNERS {
            return Err(FrameError::CapacityExceeded);
        }
        let frame = self.alloc.allocate().map_err(|_| FrameError::NoFrame)?;
        self.set_owner(self.owner_count, Some((frame, id)));
        self.owner_count += 1;
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

    /// `Segment` 销毁时归还一帧：撤销 owner 登记并放回空闲池（W6 守恒仍成立）。
    pub fn release_frame(&mut self, id: u32, frame: u64) -> Result<(), FrameError> {
        let Some(index) =
            (0..self.owner_count).find(|&index| self.owner(index) == Some((frame, id)))
        else {
            return Err(FrameError::NotOwned);
        };
        // 分配器可能拒绝归还；先完成可能失败的步骤，避免丢失归属记录。
        self.alloc
            .release(frame)
            .map_err(|_| FrameError::NotOwned)?;
        for shift in index..self.owner_count - 1 {
            self.set_owner(shift, self.owner(shift + 1));
        }
        self.set_owner(self.owner_count - 1, None);
        self.owner_count -= 1;
        self.segment_frames -= 1;
        Ok(())
    }

    /// `memseg_unmap` 回收变空的页表结点时归还一帧。
    pub fn release_table_frame(&mut self, frame: u64) -> Result<(), FrameError> {
        let Some(index) = self.table_frames[..self.table_count]
            .iter()
            .position(|owned| *owned == frame)
        else {
            return Err(FrameError::NotOwned);
        };
        self.alloc
            .release(frame)
            .map_err(|_| FrameError::NotOwned)?;
        for shift in index..self.table_count - 1 {
            self.table_frames[shift] = self.table_frames[shift + 1];
        }
        self.table_count -= 1;
        Ok(())
    }

    /// Reclaim the actual ownership ledger, including later COW allocations.
    /// The caller must first prove that no address space references the Segment.
    pub fn release_segment_frames(&mut self, id: u32) -> Result<(), FrameError> {
        loop {
            let frame = (0..self.owner_count).find_map(|index| {
                self.owner(index)
                    .filter(|(_, owner)| *owner == id)
                    .map(|(frame, _)| frame)
            });
            let Some(frame) = frame else {
                break;
            };
            self.release_frame(id, frame)?;
        }
        self.close_segment(id)
    }

    /// 逐帧归属查询（W6 三类划分 + image 类；空闲帧返回 [`FrameOwner::Free`]）。
    ///
    /// SPEC-068：image 归属经**物化向量**逐帧判定，不再用 `(start, end)` 区间。
    pub fn owner_of(&self, frame: u64) -> FrameOwner {
        if self.image_frame_ids[..self.image_count].contains(&frame) {
            return FrameOwner::Image;
        }
        for index in 0..self.owner_count {
            if let Some((owned, id)) = self.owner(index)
                && owned == frame
            {
                return FrameOwner::Segment(id);
            }
        }
        if self.table_frames[..self.table_count].contains(&frame) {
            return FrameOwner::Table;
        }
        FrameOwner::Free
    }

    /// W6 不交性判定：逐帧归属两两不交（可检测「同一帧计入两类归属」的构造外错误）。
    ///
    /// SPEC-068：image 类自身（向量内无重复）也纳入判定——区间表达下"向量自身有重复"
    /// 根本不可表达，逐帧向量则必须自证无重复。
    pub fn check_disjoint(&self) -> Result<(), FrameError> {
        let mut index = 0;
        while index < self.image_count {
            let frame = self.image_frame_ids[index];
            if self.image_frame_ids[..index].contains(&frame) {
                return Err(FrameError::Overlap);
            }
            index += 1;
        }
        index = 0;
        while index < self.owner_count {
            let frame = self.owner(index).map(|(frame, _)| frame).unwrap_or(0);
            if self.image_frame_ids[..self.image_count].contains(&frame) {
                return Err(FrameError::Overlap);
            }
            if self.table_frames[..self.table_count].contains(&frame) {
                return Err(FrameError::Overlap);
            }
            let mut previous = 0;
            while previous < index {
                if self
                    .owner(previous)
                    .is_some_and(|(owned, _)| owned == frame)
                {
                    return Err(FrameError::Overlap);
                }
                previous += 1;
            }
            index += 1;
        }
        index = 0;
        while index < self.table_count {
            let frame = self.table_frames[index];
            if self.image_frame_ids[..self.image_count].contains(&frame)
                || self.table_frames[..index].contains(&frame)
            {
                return Err(FrameError::Overlap);
            }
            index += 1;
        }
        Ok(())
    }

    /// W6 守恒：`free + segment + table + image == universe`。
    ///
    /// 不交性由构造保证：每个帧只可能被 [`allocate_for_segment`] 或
    /// [`allocate_table_frame`] 取走一次，image 帧与 Usable 帧在 `mem::build_inventory`
    /// 与 `FreeFrameAllocator::from_regions` 中已校验不相交。
    pub fn check_conservation(&self) -> Result<(), FrameError> {
        let accounted =
            self.alloc.remaining() + self.segment_frames + self.table_count + self.image_count;
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
            image_frames: self.image_count as u64,
            conserved: self.check_conservation().is_ok(),
            disjoint: self.check_disjoint().is_ok(),
        }
    }

    pub fn free_frames(&self) -> usize {
        self.alloc.remaining()
    }

    /// SPEC-068：image 帧向量长度（§5.1 image Segment 的 `frames.length`）。
    pub fn image_frame_count(&self) -> usize {
        self.image_count
    }

    /// SPEC-068：image 帧向量的第 `index` 项（越界返回 `None`）。
    pub fn image_frame_at(&self, index: usize) -> Option<u64> {
        if index < self.image_count {
            Some(self.image_frame_ids[index])
        } else {
            None
        }
    }
}

/// SPEC-068：全局注册表**就地**构造（`const` 初值 + [`FrameRegistry::init_at`]）。
///
/// 不用 `Option<FrameRegistry>` 的"构造后按值放入"形态：那会在 boot 栈上产生数份
/// ~27 KiB 的临时量，实测溢出 256 KiB 启动栈并三重故障。`INSTALLED` 单独表达安装状态。
static REGISTRY: StaticCell<FrameRegistry> = StaticCell::new(FrameRegistry::empty());
static INSTALLED: StaticCell<bool> = StaticCell::new(false);

/// 安装全局帧注册表（boot 早期一次；进入任何取帧路径之前）。
///
/// 校验失败时不写入 `INSTALLED`，故注册表保持"未安装"，调用方可按 panic 处理。
pub fn install(inventory: &FrameInventory, alloc: FreeFrameAllocator) -> Result<(), FrameError> {
    REGISTRY.with_mut(|registry| -> Result<(), FrameError> {
        registry.init_at(inventory, alloc)?;
        registry.check_conservation()?;
        Ok(())
    })?;
    INSTALLED.with_mut(|installed| *installed = true);
    Ok(())
}

/// 在已安装的全局注册表上以可变引用执行 `f`；未安装时返回 [`FrameError::NotInstalled`]。
fn with_registry<T>(
    f: impl FnOnce(&mut FrameRegistry) -> Result<T, FrameError>,
) -> Result<T, FrameError> {
    if !INSTALLED.with(|installed| *installed) {
        return Err(FrameError::NotInstalled);
    }
    REGISTRY.with_mut(f)
}

/// 登记一个活动 Segment（其帧随后经 [`allocate_for_segment`] 归入）。
pub fn open_segment(id: u32, pool: Pool) -> Result<(), FrameError> {
    with_registry(|registry| registry.open_segment(id, pool))
}

/// 为一个活动 Segment 分配帧（唯一的数据帧取帧入口）。
pub fn allocate_for_segment(id: u32) -> Result<u64, FrameError> {
    with_registry(|registry| registry.allocate_for_segment(id))
}

/// 分配一帧保留页表帧（唯一的页表帧取帧入口）。
pub fn allocate_table_frame() -> Result<u64, FrameError> {
    with_registry(|registry| registry.allocate_table_frame())
}

/// 关闭某 Segment 的归属标签（帧须已全部归还）。
pub fn close_segment(id: u32) -> Result<(), FrameError> {
    with_registry(|registry| registry.close_segment(id))
}

/// 归还某 Segment 的一帧（销毁或回收路径）。
pub fn release_frame(id: u32, frame: u64) -> Result<(), FrameError> {
    with_registry(|registry| registry.release_frame(id, frame))
}

pub fn release_segment_frames(id: u32) -> Result<(), FrameError> {
    with_registry(|registry| registry.release_segment_frames(id))
}

/// 归还一帧保留页表帧（空表结点回收路径）。
pub fn release_table_frame(frame: u64) -> Result<(), FrameError> {
    with_registry(|registry| registry.release_table_frame(frame))
}

/// 当前空闲帧数（`memseg_create` 的容量前置校验）。
pub fn free_frames() -> Result<usize, FrameError> {
    with_registry(|registry| Ok(registry.free_frames()))
}

/// 逐帧归属查询。
pub fn owner_of(frame: u64) -> Result<FrameOwner, FrameError> {
    with_registry(|registry| Ok(registry.owner_of(frame)))
}

/// W6 不交性判定（owner 列表与页表帧两两不交，且均不与 image 向量相交）。
pub fn check_disjoint() -> Result<(), FrameError> {
    with_registry(|registry| registry.check_disjoint())
}

/// SPEC-068：image 帧向量长度。
pub fn image_frame_count() -> Result<usize, FrameError> {
    with_registry(|registry| Ok(registry.image_frame_count()))
}

/// SPEC-068：image 帧向量的第 `index` 项（`bootstrap::SegmentFrames::Image` 的逐帧委托）。
pub fn image_frame_at(index: usize) -> Result<u64, FrameError> {
    with_registry(|registry| registry.image_frame_at(index).ok_or(FrameError::NoFrame))
}

/// 观察全局注册表的守恒状态（见证事件用）。
pub fn witness() -> Result<FrameWitness, FrameError> {
    with_registry(|registry| Ok(registry.witness()))
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
        // 构造一个帧数 ≥ MAX_TABLE_FRAMES 的 universe，验证容量上限先于取帧耗尽触发。
        let regions = [MemoryRegion {
            start: 0x10_0000,
            end: 0x10_0000 + 0x1000 * MAX_TABLE_FRAMES as u64,
            kind: MemoryRegionKind::Usable,
        }];
        let inventory = FrameInventory {
            usable_frames: MAX_TABLE_FRAMES,
            image_frames: 1,
            free_frames: MAX_TABLE_FRAMES,
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
    fn all_live_frames_can_be_returned_without_filling_the_recycle_pool() {
        let count = MAX_FRAME_OWNERS + MAX_TABLE_FRAMES;
        let regions = [MemoryRegion {
            start: 0x10_0000,
            end: 0x10_0000 + count as u64 * crate::mem::FRAME_SIZE,
            kind: MemoryRegionKind::Usable,
        }];
        let inventory = FrameInventory {
            usable_frames: count,
            image_frames: 1,
            free_frames: count,
            usable_regions: 1,
            kernel_phys_start: 0x1000_0000,
            kernel_phys_end: 0x1000_1000,
        };
        let alloc = FreeFrameAllocator::from_regions(&regions, 0x1000_0000, 0x1000_1000).unwrap();
        let mut registry = FrameRegistry::new(&inventory, alloc).unwrap();
        registry.open_segment(1, Pool::Cpu).unwrap();
        let mut data = [0; MAX_FRAME_OWNERS];
        let mut tables = [0; MAX_TABLE_FRAMES];
        for frame in &mut data {
            *frame = registry.allocate_for_segment(1).unwrap();
        }
        for frame in &mut tables {
            *frame = registry.allocate_table_frame().unwrap();
        }
        for frame in data {
            registry.release_frame(1, frame).unwrap();
        }
        for frame in tables {
            registry.release_table_frame(frame).unwrap();
        }
        assert_eq!(registry.alloc.remaining(), count);
        assert!(registry.check_conservation().is_ok());
    }

    #[test]
    fn rejected_release_preserves_frame_ownership() {
        let mut registry = registry();
        registry.open_segment(1, Pool::Cpu).unwrap();
        registry.allocate_for_segment(1).unwrap();
        registry.set_owner(0, Some((0xdead_0000, 1)));
        let before = registry;
        assert_eq!(
            registry.release_frame(1, 0xdead_0000),
            Err(FrameError::NotOwned)
        );
        assert!(registry == before);

        registry.allocate_table_frame().unwrap();
        registry.table_frames[0] = 0xdead_1000;
        let before = registry;
        assert_eq!(
            registry.release_table_frame(0xdead_1000),
            Err(FrameError::NotOwned)
        );
        assert!(registry == before);
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
    fn owner_query_release_and_disjointness_hold() {
        let mut registry = registry();
        registry.open_segment(1, Pool::Cpu).unwrap();
        registry.open_segment(2, Pool::Cpu).unwrap();
        let a = registry.allocate_for_segment(1).unwrap();
        let b = registry.allocate_for_segment(2).unwrap();
        let table = registry.allocate_table_frame().unwrap();
        assert_eq!(registry.owner_of(a), FrameOwner::Segment(1));
        assert_eq!(registry.owner_of(b), FrameOwner::Segment(2));
        assert_eq!(registry.owner_of(table), FrameOwner::Table);
        assert_eq!(registry.owner_of(0x40_0000), FrameOwner::Image);
        assert!(registry.check_disjoint().is_ok());

        // 归还后重算守恒与归属；同一帧不会被计两次。
        registry.release_frame(1, a).unwrap();
        assert_eq!(registry.owner_of(a), FrameOwner::Free);
        registry.release_table_frame(table).unwrap();
        assert_eq!(registry.owner_of(table), FrameOwner::Free);
        assert!(registry.check_disjoint().is_ok());
        assert!(registry.check_conservation().is_ok());
        assert!(registry.witness().disjoint);

        // 构造外错误：同一帧被两个 Segment 认领 → 不交性判定可检出。
        let index = registry.owner_count;
        registry.set_owner(index, Some((b, 1)));
        registry.owner_count += 1;
        assert_eq!(registry.check_disjoint(), Err(FrameError::Overlap));
        assert!(!registry.witness().disjoint);
    }

    #[test]
    fn segment_destruction_reclaims_initial_and_later_copy_frames_only() {
        let mut registry = registry();
        registry.open_segment(1, Pool::Cpu).unwrap();
        registry.open_segment(2, Pool::Cpu).unwrap();
        let initial = registry.allocate_for_segment(1).unwrap();
        let copied = registry.allocate_for_segment(1).unwrap();
        let other = registry.allocate_for_segment(2).unwrap();
        registry.release_segment_frames(1).unwrap();
        assert_eq!(registry.owner_of(initial), FrameOwner::Free);
        assert_eq!(registry.owner_of(copied), FrameOwner::Free);
        assert_eq!(registry.owner_of(other), FrameOwner::Segment(2));
        assert!(registry.check_conservation().is_ok());
    }

    #[test]
    fn releasing_a_frame_that_is_not_owned_is_rejected() {
        let mut registry = registry();
        registry.open_segment(1, Pool::Cpu).unwrap();
        let frame = registry.allocate_for_segment(1).unwrap();
        assert_eq!(
            registry.release_frame(1, frame + 0x1000),
            Err(FrameError::NotOwned)
        );
        assert_eq!(
            registry.release_table_frame(frame),
            Err(FrameError::NotOwned)
        );
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

    /// SPEC-068：image 帧是**真实逐帧向量**，逐帧归属查询与不交性判定都覆盖它。
    #[test]
    fn image_frames_are_materialized_as_a_real_vector() {
        let registry = registry();
        // 向量逐项等于 `[kernel_phys_start, kernel_phys_end)` 的逐帧枚举。
        assert_eq!(registry.image_frame_count(), 2);
        assert_eq!(registry.image_frame_at(0), Some(0x40_0000));
        assert_eq!(registry.image_frame_at(1), Some(0x40_1000));
        // 越界即 `None`（区间表达下无法区分"越界"与"区间外"）。
        assert_eq!(registry.image_frame_at(2), None);
        for index in 0..registry.image_frame_count() {
            let frame = registry.image_frame_at(index).unwrap();
            assert_eq!(registry.owner_of(frame), FrameOwner::Image);
        }
        // 区间端点（`kernel_phys_end`）不属于 image——逐帧向量与区间语义一致的边界证据。
        assert_eq!(registry.owner_of(0x40_2000), FrameOwner::Free);
        assert!(registry.check_disjoint().is_ok());
        assert!(registry.check_conservation().is_ok());
    }

    /// SPEC-068：向量自身重复必须被不交性判定检出（区间表达下这一反例不可表达）。
    #[test]
    fn image_vector_self_overlap_is_detected() {
        let mut registry = registry();
        assert!(registry.check_disjoint().is_ok());
        // 构造外错误：把第 1 项改写成第 0 项 → 同一帧在 image 向量内出现两次。
        registry.image_frame_ids[1] = registry.image_frame_ids[0];
        assert_eq!(registry.check_disjoint(), Err(FrameError::Overlap));
        assert!(!registry.witness().disjoint);
        // 守恒只看计数，故不受影响——不交性与守恒是两条独立判据。
        assert!(registry.check_conservation().is_ok());
    }

    /// SPEC-068：image 向量容量上限先于任何取帧触发，且失败不改动注册表。
    #[test]
    fn image_vector_capacity_is_enforced() {
        let regions = regions();
        let alloc = FreeFrameAllocator::from_regions(&regions, 0x40_0000, 0x40_2000).unwrap();
        let inventory = FrameInventory {
            usable_frames: 7,
            image_frames: MAX_IMAGE_FRAMES + 1,
            free_frames: 7,
            usable_regions: 3,
            kernel_phys_start: 0x40_0000,
            kernel_phys_end: 0x40_0000 + (MAX_IMAGE_FRAMES as u64 + 1) * crate::mem::FRAME_SIZE,
        };
        assert_eq!(
            FrameRegistry::new(&inventory, alloc),
            Err(FrameError::CapacityExceeded)
        );
    }

    /// SPEC-068：`init_at` 的失败前置须在**任何字段写入之前**——否则 `install` 会留下
    /// 半个注册表（`INSTALLED` 未置位但内容已被改写）。
    #[test]
    fn init_at_failure_leaves_the_registry_untouched() {
        let mut registry = registry();
        let before = registry.witness();
        let regions = regions();
        let alloc = FreeFrameAllocator::from_regions(&regions, 0x40_0000, 0x40_2000).unwrap();
        let bad = FrameInventory {
            usable_frames: 7,
            image_frames: 0,
            free_frames: 7,
            usable_regions: 3,
            kernel_phys_start: 0x40_0000,
            kernel_phys_end: 0x40_2000,
        };
        assert_eq!(
            registry.init_at(&bad, alloc),
            Err(FrameError::UniverseMismatch)
        );
        assert_eq!(registry.witness(), before);
    }

    /// SPEC-068：注册表按值大小上界。image 向量把注册表推到 ~27 KiB；再增长就必须
    /// 先评估 boot 早期栈帧（`install` 已改就地构造，但 `FrameRegistry::new` 仍按值返回，
    /// host 单测与未来调用点都受此约束）。
    #[test]
    fn registry_stays_within_the_value_size_budget() {
        assert!(
            core::mem::size_of::<FrameRegistry>() <= 32 * 1024,
            "FrameRegistry 按值大小 {} B 超出 32 KiB 预算",
            core::mem::size_of::<FrameRegistry>()
        );
    }
}
