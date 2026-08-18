#![allow(dead_code)]

use crate::pagewalk::{
    FrameAllocator, PAGE_SIZE, PageTable, PhysAddr, Pte, SegmentRef, collect_leaves, map_vaddr,
    new_task_table, unmap_vaddr, validate_page_vaddr, visit_leaves,
};
use crate::state::{SegId, TaskId, VirtAddr};
use std::collections::{BTreeMap, BTreeSet};

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
    FrameAllocatorCorrupt,
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
    pub fn new(universe: &[PhysAddr]) -> Result<Self, MemSegError> {
        Ok(RefineState {
            segments: BTreeMap::new(),
            alloc: FrameAllocator::new(universe).map_err(|_| MemSegError::FrameAllocatorCorrupt)?,
            tasks: BTreeMap::new(),
            next_seg: 1,
        })
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
    page_count: u64,
    allowed_access: u8,
) -> Result<SegId, MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
    }
    if allowed_access == 0 || allowed_access & !0x07 != 0 {
        return Err(MemSegError::InvalidAccess);
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
        visit_leaves(
            &tc.table,
            &mut |va, pte| match st.segments.get(&pte.mapping.segment) {
                None => {
                    ok = false;
                    diag.push(format!(
                        "W7: task {tid} {va:#x} 指向不活动 segment {}",
                        pte.mapping.segment
                    ));
                }
                Some(seg) => {
                    let offset = usize::try_from(pte.mapping.offset).ok();
                    let expected = offset.and_then(|i| seg.frames.get(i));
                    if expected != Some(&pte.frame_phys) {
                        ok = false;
                        diag.push(format!(
                            "W7: task {tid} {va:#x} 物理帧与 segment offset 不匹配"
                        ));
                    }
                    if !pte.present {
                        ok = false;
                        diag.push(format!("W7: task {tid} {va:#x} 叶 PTE 不存在"));
                    }
                    if !pte.user {
                        ok = false;
                        diag.push(format!("W7: task {tid} {va:#x} 叶 PTE 非用户可访问"));
                    }
                    if pte.writable != (pte.mapping.access & crate::state::ACCESS_W != 0) {
                        ok = false;
                        diag.push(format!("W7: task {tid} {va:#x} writable 与映射权限不匹配"));
                    }
                    if pte.nx != (pte.mapping.access & crate::state::ACCESS_X == 0) {
                        ok = false;
                        diag.push(format!("W7: task {tid} {va:#x} NX 与映射权限不匹配"));
                    }
                    if pte.mapping.access == 0 || pte.mapping.access & !0x07 != 0 {
                        ok = false;
                        diag.push(format!("W7: task {tid} {va:#x} 映射权限编码无效"));
                    }
                    if seg.allowed_access & pte.mapping.access != pte.mapping.access {
                        ok = false;
                        diag.push(format!("W7: task {tid} {va:#x} access 超出段权限"));
                    }
                }
            },
        );
    }

    // W6 物理页有限集合划分：universe/free/used 各源无重复，且 free ⊎ used = universe。
    let universe: BTreeSet<_> = st.alloc.universe.iter().copied().collect();
    if universe.len() != st.alloc.universe.len() {
        ok = false;
        diag.push("W6: frame_universe 含重复帧".to_string());
    }

    let free: BTreeSet<_> = st.alloc.free.iter().copied().collect();
    if free.len() != st.alloc.free.len() {
        ok = false;
        diag.push("W6: 空闲帧集合含重复帧".to_string());
    }
    if !free.is_subset(&universe) {
        ok = false;
        diag.push("W6: 空闲帧不属于 frame_universe".to_string());
    }

    let mut used = BTreeSet::new();
    for (sid, seg) in &st.segments {
        let segment_frames: BTreeSet<_> = seg.frames.iter().copied().collect();
        if segment_frames.len() != seg.frames.len() {
            ok = false;
            diag.push(format!("W6: segment {sid} 含重复帧"));
        }
        for frame in &seg.frames {
            if !universe.contains(frame) {
                ok = false;
                diag.push(format!(
                    "W6: segment {sid} 的帧 {frame:#x} 不属于 frame_universe"
                ));
            }
            if !used.insert(*frame) {
                ok = false;
                diag.push(format!("W6: 帧 {frame:#x} 被多个 segment 引用"));
            }
        }
    }

    if !used.is_disjoint(&free) {
        ok = false;
        diag.push("W6: 段帧与空闲帧相交".to_string());
    }
    if used.union(&free).copied().collect::<BTreeSet<_>>() != universe {
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
pub fn map_verified(
    st: &mut RefineState,
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
    let seg = st
        .segments
        .get(&seg_id)
        .ok_or(MemSegError::InvalidSegment)?;
    let seg = seg.clone(); // 借出来后释放，便于下面可变迁用
    if !st.tasks.contains_key(&task_id) {
        return Err(MemSegError::InvalidTask);
    }
    // access ⊆ allowed_access（不会把段不允许的权限映射进地址空间）
    if seg.allowed_access & access != access {
        return Err(MemSegError::RightsExceeded);
    }
    checked_page_vaddr(target_base, 0)?;
    checked_page_vaddr(target_base, page_count - 1)?;
    let seg_end = seg_offset
        .checked_add(page_count)
        .ok_or(MemSegError::ArithmeticOverflow)?;
    if seg_end > seg.frames.len() as u64 {
        return Err(MemSegError::OutOfBounds);
    }

    let mut pending = Vec::with_capacity(
        usize::try_from(page_count).map_err(|_| MemSegError::ArithmeticOverflow)?,
    );
    for i in 0..page_count {
        let va = checked_page_vaddr(target_base, i)?;
        let frame_index = seg_offset
            .checked_add(i)
            .ok_or(MemSegError::ArithmeticOverflow)?;
        let frame_index =
            usize::try_from(frame_index).map_err(|_| MemSegError::ArithmeticOverflow)?;
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

pub fn unmap_verified(
    st: &mut RefineState,
    task_id: TaskId,
    target_base: VirtAddr,
    page_count: u64,
) -> Result<(), MemSegError> {
    if page_count == 0 {
        return Err(MemSegError::ZeroSize);
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

pub fn destroy_verified(st: &mut RefineState, seg_id: SegId) -> Result<(), MemSegError> {
    let seg = st
        .segments
        .get(&seg_id)
        .ok_or(MemSegError::InvalidSegment)?;
    let frames = seg.frames.clone();
    // StillMapped：任何活动 Task 的 vspace 都不能引用 target
    for tc in st.tasks.values() {
        for sref in tc.vspace.values() {
            if sref.segment == seg_id {
                return Err(MemSegError::StillMapped);
            }
        }
    }
    // 在副本上预检全部归还；任一帧非法时原状态保持不变。
    let mut alloc_after = st.alloc.clone();
    for frame in frames {
        alloc_after
            .free_frame(frame)
            .map_err(|_| MemSegError::FrameAllocatorCorrupt)?;
    }

    st.segments.remove(&seg_id);
    st.alloc = alloc_after;
    Ok(())
}
