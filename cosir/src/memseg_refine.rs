#![allow(dead_code)]

use crate::pagewalk::{
    PAGE_SIZE, FrameAllocator, PageTable, PhysAddr, Pte, SegmentRef, collect_leaves, map_vaddr,
    new_task_table, unmap_vaddr, validate_page_vaddr,
};
use crate::state::{SegId, TaskId, VirtAddr};
use std::collections::BTreeMap;

// Capability 权限位掩码（细化层用于前置授权检查；非 POSIR 一等原语）
// SEG_MAP_* 与 state::ACCESS_* 数值一致(R=0b001/W=0b010/X=0b100)。
pub const SEG_MAP_R: u8 = crate::state::ACCESS_R;
pub const SEG_MAP_W: u8 = crate::state::ACCESS_W;
pub const SEG_MAP_X: u8 = crate::state::ACCESS_X;
pub const SEG_DESTROY: u8 = 0x08;

/// 细化错误集：沿用规范错误名，不新增对 POSIR 可见的错误原语。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemSegError {
    ZeroSize,
    InvalidAccess,
    NoMemory,
    InvalidSegment,
    InvalidTask,
    MissingMapRight,
    RightsExceeded,
    OutOfBounds,
    Overlap,
    NotMapped,
    EntryMapping,
    StillMapped,
    MissingRight,
    InvalidAddress,
    ArithmeticOverflow,
    SegmentIdExhausted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub frames: Vec<PhysAddr>,
    pub allowed_access: u8,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskCtx {
    pub table: PageTable,
    pub vspace: BTreeMap<VirtAddr, SegmentRef>,
    pub entry: VirtAddr,
}

/// 细化域状态：活动 segment、物理帧池、每 Task 的 CR3 树 + vspace 双视图 + entry。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefineState {
    pub segments: BTreeMap<SegId, Segment>,
    pub alloc: FrameAllocator,
    pub tasks: BTreeMap<TaskId, TaskCtx>,
    pub next_seg: SegId,
}

impl RefineState {
    pub fn new(universe: &[PhysAddr]) -> Self {
        RefineState {
            segments: BTreeMap::new(),
            alloc: FrameAllocator::new(universe),
            tasks: BTreeMap::new(),
            next_seg: 1,
        }
    }

}

/// 为 Task 建立空的 CR3 树 + 空 vspace 双视图，记录入口（W8 依据）。
pub fn spawn_task(st: &mut RefineState, id: TaskId, entry: VirtAddr) -> Result<(), MemSegError> {
    if st.tasks.contains_key(&id) {
        return Err(MemSegError::InvalidTask);
    }
    st.tasks.insert(
        id,
        TaskCtx {
            table: new_task_table(),
            vspace: BTreeMap::new(),
            entry,
        },
    );
    Ok(())
}

/// memseg_create(page_count, allowed_access) 的细化迁移：
/// 帧分配(W6) + 建 Segment + 铸造根 SegCap（本层返回 seg id）。
pub fn memseg_create(
    st: &mut RefineState,
    seg_cap_rights: u8,
    page_count: u64,
    allowed_access: u8,
) -> Result<SegId, MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
    }
    if allowed_access == 0 || allowed_access & !0x07 != 0 {
        return Err(MemSegError::InvalidAccess);
    }
    // 授权：需含 Seg 映射权限（R/W/X 任一）才可创建用户可见 segment
    if seg_cap_rights & 0x07 == 0 {
        return Err(MemSegError::MissingMapRight);
    }
    let page_count = usize::try_from(page_count).map_err(|_| MemSegError::ArithmeticOverflow)?;
    let next_seg = st
        .next_seg
        .checked_add(1)
        .ok_or(MemSegError::SegmentIdExhausted)?;
    // 先检查容量，避免部分分配（原子）
    if st.alloc.free.len() < page_count {
        return Err(MemSegError::NoMemory);
    }
    let mut frames = Vec::with_capacity(page_count);
    for _ in 0..page_count {
        frames.push(st.alloc.alloc().expect("容量已保证"));
    }
    let id = st.next_seg;
    st.next_seg = next_seg;
    st.segments.insert(
        id,
        Segment {
            frames,
            allowed_access,
        },
    );
    Ok(id)
}

/// α 投影：返回该细化态是否满足 W6 + W7。细节写入 diag 供测试断言。
pub fn memseg_alpha_verbose(st: &RefineState, diag: &mut Vec<String>) -> bool {
    let mut ok = true;

    // W7 双视图一致 + 指向活动 Segment + access≤allowed
    for (tid, tc) in &st.tasks {
        let mut leaves: BTreeMap<VirtAddr, SegmentRef> = BTreeMap::new();
        collect_leaves(&tc.table, &mut leaves);
        #[allow(clippy::op_ref)]
        if &leaves != &tc.vspace {
            ok = false;
            diag.push(format!("W7: task {tid} 树↔视图不一致"));
        }
        for (va, sref) in &leaves {
            match st.segments.get(&sref.segment) {
                None => {
                    ok = false;
                    diag.push(format!("W7: task {tid} {va:#x} 指向不活动 segment {}", sref.segment));
                }
                Some(seg) => {
                    if seg.allowed_access & sref.access != sref.access {
                        ok = false;
                        diag.push(format!("W7: {va:#x} access 超出段权限"));
                    }
                }
            }
        }
    }

    // W6 物理页唯一归属
    // 收集所有活动 segment 帧
    let mut used: BTreeMap<PhysAddr, SegId> = BTreeMap::new();
    for (sid, seg) in &st.segments {
        for f in &seg.frames {
            if let Some(prev) = used.insert(*f, *sid) {
                ok = false;
                diag.push(format!("W6: 帧 {f:#x} 被 segment {prev} 与 {sid} 共同引用"));
            }
        }
    }
    // 段帧与空闲帧不交
    for f in &st.alloc.free {
        if used.contains_key(f) {
            ok = false;
            diag.push(format!("W6: 空闲帧 {f:#x} 同时被 segment {} 引用", used[f]));
        }
    }
    // 并集 = universe
    let mut combined: Vec<PhysAddr> = used.keys().copied().collect();
    combined.extend(st.alloc.free.iter().copied());
    combined.sort_unstable();
    let mut univ = st.alloc.universe.clone();
    univ.sort_unstable();
    if combined != univ {
        ok = false;
        diag.push("W6: 段帧∪空闲帧 ≠ frame_universe".to_string());
    }

    ok
}

/// α 便捷封装（Bool 返回值）。
pub fn memseg_alpha(st: &RefineState) -> bool {
    let mut diag = Vec::new();
    memseg_alpha_verbose(st, &mut diag)
}

fn checked_page_vaddr(base: u64, index: u64) -> Result<u64, MemSegError> {
    let delta = index
        .checked_mul(PAGE_SIZE)
        .ok_or(MemSegError::ArithmeticOverflow)?;
    let va = base
        .checked_add(delta)
        .ok_or(MemSegError::ArithmeticOverflow)?;
    validate_page_vaddr(va).map_err(|_| MemSegError::InvalidAddress)?;
    Ok(va)
}

#[allow(clippy::too_many_arguments)]
pub fn memseg_map(
    st: &mut RefineState,
    seg_cap_rights: u8,
    task_manage_rights: bool,
    seg_id: SegId,
    task_id: TaskId,
    seg_offset: u64,
    target_base: VirtAddr,
    page_count: u64,
    access: u8,
) -> Result<(), MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
    }
    if access == 0 || access & !0x07 != 0 {
        return Err(MemSegError::InvalidAccess);
    }
    let seg = st.segments.get(&seg_id).ok_or(MemSegError::InvalidSegment)?;
    let seg = seg.clone(); // 借出来后释放，便于下面可变迁用
    if !st.tasks.contains_key(&task_id) {
        return Err(MemSegError::InvalidTask);
    }
    // 授权：调用方持有所请求访问对应的全部 segment 映射权限（SEG_MAP_* 复用 state ACCESS 位码，值相同）
    if seg_cap_rights & access != access {
        return Err(MemSegError::MissingMapRight);
    }
    if !task_manage_rights {
        return Err(MemSegError::MissingRight);
    }
    // access ⊆ allowed_access（不会把段不允许的权限映射进地址空间）
    if seg.allowed_access & access != access {
        return Err(MemSegError::RightsExceeded);
    }
    let seg_end = seg_offset
        .checked_add(page_count)
        .ok_or(MemSegError::ArithmeticOverflow)?;
    if seg_end > seg.frames.len() as u64 {
        return Err(MemSegError::OutOfBounds);
    }
    checked_page_vaddr(target_base, 0)?;
    checked_page_vaddr(target_base, page_count - 1)?;

    let mut pending = Vec::with_capacity(
        usize::try_from(page_count).map_err(|_| MemSegError::ArithmeticOverflow)?,
    );
    for i in 0..page_count {
        let va = checked_page_vaddr(target_base, i)?;
        let frame_index = seg_offset
            .checked_add(i)
            .ok_or(MemSegError::ArithmeticOverflow)?;
        let frame_index = usize::try_from(frame_index)
            .map_err(|_| MemSegError::ArithmeticOverflow)?;
        if st.tasks[&task_id].vspace.contains_key(&va) {
            return Err(MemSegError::Overlap);
        }
        pending.push((va, frame_index));
    }

    let writable = access & crate::state::ACCESS_W != 0;
    let nx = access & crate::state::ACCESS_X == 0;
    for (va, frame_index) in pending {
        let offset = u64::try_from(frame_index).map_err(|_| MemSegError::ArithmeticOverflow)?;
        let pte = Pte {
            present: true,
            writable,
            user: true,
            nx,
            frame_phys: seg.frames[frame_index],
            mapping: SegmentRef {
                segment: seg_id,
                offset,
                access,
            },
        };
        let task = st.tasks.get_mut(&task_id).unwrap();
        map_vaddr(&mut task.table, va, pte).map_err(|_| MemSegError::InvalidAddress)?;
        task.vspace.insert(va, pte.mapping);
    }
    Ok(())
}

pub fn memseg_unmap(
    st: &mut RefineState,
    task_manage_rights: bool,
    task_id: TaskId,
    target_base: VirtAddr,
    page_count: u64,
) -> Result<(), MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
    }
    if !task_manage_rights {
        return Err(MemSegError::MissingRight);
    }
    if !st.tasks.contains_key(&task_id) {
        return Err(MemSegError::InvalidTask);
    }
    checked_page_vaddr(target_base, 0)?;
    checked_page_vaddr(target_base, page_count - 1)?;
    let mut pending = Vec::with_capacity(
        usize::try_from(page_count).map_err(|_| MemSegError::ArithmeticOverflow)?,
    );
    // entry 页保护（W8）：目标区间不包含 entry 所在页
    let entry_page = st.tasks[&task_id].entry & !(PAGE_SIZE - 1);
    for i in 0..page_count {
        let va = checked_page_vaddr(target_base, i)?;
        if va == entry_page {
            return Err(MemSegError::EntryMapping);
        }
        if !st.tasks[&task_id].vspace.contains_key(&va) {
            return Err(MemSegError::NotMapped);
        }
        pending.push(va);
    }
    // 完整预验证后再清叶 + 双视图
    for va in pending {
        let task = st.tasks.get_mut(&task_id).unwrap();
        unmap_vaddr(&mut task.table, va).map_err(|_| MemSegError::InvalidAddress)?;
        task.vspace.remove(&va);
    }
    Ok(())
}

pub fn memseg_destroy(
    st: &mut RefineState,
    seg_destroy_right: bool,
    seg_id: SegId,
) -> Result<(), MemSegError> {
    if !seg_destroy_right {
        return Err(MemSegError::MissingRight);
    }
    let seg = st.segments.get(&seg_id).ok_or(MemSegError::InvalidSegment)?;
    let frames = seg.frames.clone();
    // StillMapped：任何活动 Task 的 vspace 都不能引用 target
    for tc in st.tasks.values() {
        for sref in tc.vspace.values() {
            if sref.segment == seg_id {
                return Err(MemSegError::StillMapped);
            }
        }
    }
    st.segments.remove(&seg_id);
    for f in frames {
        st.alloc.free_frame(f);
    }
    Ok(())
}
