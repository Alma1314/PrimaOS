use crate::memseg_refine::{
    MemSegError, RefineState, memseg_alpha_verbose, memseg_create, memseg_destroy, memseg_map,
    memseg_unmap, spawn_task, SEG_DESTROY, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X,
};
use crate::state::{ACCESS_R, ACCESS_W, ACCESS_X, SegId};

// 8 页物理帧
fn universe() -> Vec<u64> {
    (0u64..8).map(|i| 0x1000 + i * 0x1000).collect()
}

fn seg_map_rights() -> u8 {
    SEG_MAP_R | SEG_MAP_W | SEG_MAP_X
}

fn state() -> RefineState {
    RefineState::new(&universe())
}

#[test]
fn create_allocates_frames_and_segment() {
    let mut st = RefineState::new(&universe());
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .expect("create ok");
    let s = st.segments.get(&seg).expect("segment present");
    assert_eq!(s.frames.len(), 2);
    assert_eq!(s.allowed_access, ACCESS_R | ACCESS_W);
    assert_eq!(st.alloc.free.len(), 6, "8-2=6 帧空闲");
}

#[test]
fn create_exhausts_memory_returns_no_memory() {
    let mut st = RefineState::new(&universe());
    let err = memseg_create(&mut st, seg_map_rights(), 9, ACCESS_R).unwrap_err();
    assert_eq!(err, MemSegError::NoMemory);
    assert_eq!(st.alloc.free.len(), 8, "缺帧不改空闲池");
}

#[test]
fn spawn_task_registers_aspace_with_entry() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).expect("spawn ok");
    assert!(st.tasks.contains_key(&7));
    assert_eq!(st.tasks[&7].entry, 0x4000_0000);
    assert!(st.tasks[&7].vspace.is_empty());
}

#[test]
fn alpha_validates_empty_state() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "空树空视图应满足 W6/W7");
}

#[test]
fn alpha_detects_vspace_view_divergence() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    // 人为在 vspace 视图加一个树中不存在的映射 → α 应判 False
    let task = st.tasks.get_mut(&7).unwrap();
    task.vspace.insert(0x6000_0000, crate::pagewalk::SegmentRef {
        segment: 1,
        offset: 0,
        access: ACCESS_R,
    });
    let mut diag = Vec::new();
    assert!(!memseg_alpha_verbose(&st, &mut diag), "视图与树不一致应被 α 捕获");
}

#[test]
fn map_inserts_leaf_and_vspace() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .unwrap();
    memseg_map(
        &mut st,
        seg_map_rights(),
        true,
        seg,
        7,
        0,
        0x5000_0000,
        1,
        ACCESS_R | ACCESS_W,
    )
    .expect("map ok");
    let task = &st.tasks[&7];
    assert!(task.vspace.contains_key(&0x5000_0000));
    assert_eq!(
        task.vspace[&0x5000_0000].segment,
        seg
    );
    let pte = crate::pagewalk::translate(&task.table, 0x5000_0000)
        .unwrap()
        .expect("leaf present");
    assert!(pte.mapping.access & ACCESS_W != 0);
}

#[test]
fn map_overlap_rejected_state_unchanged() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .unwrap();
    memseg_map(
        &mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 2, ACCESS_R | ACCESS_W,
    )
    .unwrap();
    // 精确重叠：再 map 覆盖已映射块
    let err = memseg_map(
        &mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 1, ACCESS_R | ACCESS_W,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::Overlap);
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "失败后状态仍 W6/W7 有效");
}

#[test]
fn map_rights_exceeded_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    // 段只允许 R|W，却请求含 X → RightsExceeded
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R | ACCESS_W)
        .unwrap();
    let err = memseg_map(
        &mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 1,
        ACCESS_R | ACCESS_W | ACCESS_X,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::RightsExceeded);
}

#[test]
fn map_missing_seg_map_right_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R | ACCESS_W)
        .unwrap();
    // 缺 SEG_MAP 授权 → MissingMapRight
    let err = memseg_map(
        &mut st, 0, true, seg, 7, 0, 0x5000_0000, 1, ACCESS_R | ACCESS_W,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::MissingMapRight);
}

#[test]
fn map_out_of_bounds_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    let err = memseg_map(
        &mut st, seg_map_rights(), true, seg, 7, 1, 0x5000_0000, 1, ACCESS_R,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::OutOfBounds);
}

#[test]
fn unmap_clears_leaf_and_vspace() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W)
        .unwrap();
    memseg_map(&mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 2, ACCESS_R | ACCESS_W).unwrap();
    memseg_unmap(&mut st, true, 7, 0x5000_0000, 2).expect("unmap ok");
    let task = &st.tasks[&7];
    assert!(!task.vspace.contains_key(&0x5000_0000));
    assert!(crate::pagewalk::translate(&task.table, 0x5000_0000).unwrap().is_none());
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "unmap 后仍 W6/W7 有效");
}

#[test]
fn unmap_entry_page_rejected() {
    let mut st = RefineState::new(&universe());
    // entry 设在 0x5000_0000 所在页
    spawn_task(&mut st, 7, 0x5000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W | ACCESS_X)
        .unwrap();
    // 映射包含 entry 页
    memseg_map(&mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 1, ACCESS_R | ACCESS_X).unwrap();
    let err = memseg_unmap(&mut st, true, 7, 0x5000_0000, 1).unwrap_err();
    assert_eq!(err, MemSegError::EntryMapping);
    // 树与视图不应被清除
    let task = &st.tasks[&7];
    assert!(task.vspace.contains_key(&0x5000_0000));
    assert!(crate::pagewalk::translate(&task.table, 0x5000_0000).unwrap().is_some());
}

#[test]
fn unmap_not_mapped_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let err = memseg_unmap(&mut st, true, 7, 0x6000_0000, 1).unwrap_err();
    assert_eq!(err, MemSegError::NotMapped);
}

#[test]
fn unmap_missing_task_vspace_right_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    memseg_map(&mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 1, ACCESS_R).unwrap();
    let err = memseg_unmap(&mut st, false, 7, 0x5000_0000, 1).unwrap_err();
    assert_eq!(err, MemSegError::MissingRight);
}

#[test]
fn destroy_returns_frames_and_removes_segment() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R).unwrap();
    let before = st.alloc.free.len(); // = 6
    memseg_destroy(&mut st, true, seg).expect("destroy ok");
    assert!(!st.segments.contains_key(&seg));
    assert_eq!(st.alloc.free.len(), before + 2, "两帧归还");
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "destroy 后仍 W6 有效");
}

#[test]
fn destroy_still_mapped_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    // 映射进 task 7 → destroy 应拒绝
    memseg_map(&mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 1, ACCESS_R).unwrap();
    let err = memseg_destroy(&mut st, true, seg).unwrap_err();
    assert_eq!(err, MemSegError::StillMapped);
    assert!(st.segments.contains_key(&seg), "段不删除");
}

#[test]
fn destroy_missing_right_rejected() {
    let mut st = RefineState::new(&universe());
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    let err = memseg_destroy(&mut st, false, seg).unwrap_err();
    assert_eq!(err, MemSegError::MissingRight);
    assert!(st.segments.contains_key(&seg));
}

#[test]
fn destroy_invalid_segment_rejected() {
    let mut st = RefineState::new(&universe());
    let err = memseg_destroy(&mut st, true, 999).unwrap_err();
    assert_eq!(err, MemSegError::InvalidSegment);
}

#[test]
fn destroy_idempotent_double_frees_rejected() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 1, ACCESS_R).unwrap();
    memseg_destroy(&mut st, true, seg).unwrap();
    // 再次 destroy → InvalidSegment（段已不在）
    let err = memseg_destroy(&mut st, true, seg).unwrap_err();
    assert_eq!(err, MemSegError::InvalidSegment);
}

#[test]
fn map_rejects_range_crossing_canonical_boundary_atomically() {
    let mut st = state();
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, SEG_MAP_R, 2, ACCESS_R).unwrap();
    let before = st.clone();
    let err = memseg_map(
        &mut st,
        SEG_MAP_R,
        true,
        seg,
        7,
        0,
        0x0000_7fff_ffff_f000,
        2,
        ACCESS_R,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::InvalidAddress);
    assert_eq!(st, before);
}

#[test]
fn map_rejects_segment_range_overflow_atomically() {
    let mut st = state();
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, SEG_MAP_R, 1, ACCESS_R).unwrap();
    let before = st.clone();
    let err = memseg_map(
        &mut st,
        SEG_MAP_R,
        true,
        seg,
        7,
        u64::MAX,
        0x5000_0000,
        1,
        ACCESS_R,
    )
    .unwrap_err();
    assert_eq!(err, MemSegError::ArithmeticOverflow);
    assert_eq!(st, before);
}

#[test]
fn create_rejects_exhausted_segment_id_atomically() {
    let mut st = state();
    st.next_seg = SegId::MAX;
    let before = st.clone();
    assert_eq!(
        memseg_create(&mut st, SEG_MAP_R, 1, ACCESS_R),
        Err(MemSegError::SegmentIdExhausted)
    );
    assert_eq!(st, before);
}

#[test]
fn full_memseg_lifecycle_alpha_preserved() {
    let mut st = RefineState::new(&universe());
    spawn_task(&mut st, 7, 0x4000_0000).unwrap();
    let seg = memseg_create(&mut st, seg_map_rights() | SEG_DESTROY, 2, ACCESS_R | ACCESS_W).unwrap();
    memseg_map(&mut st, seg_map_rights(), true, seg, 7, 0, 0x5000_0000, 2, ACCESS_R | ACCESS_W).unwrap();
    memseg_unmap(&mut st, true, 7, 0x5000_0000, 1).unwrap(); // 释放第一页(0x5000_0000)
    // 剩余一页(0x5000_1000)映射仍引用段 → 不可 destroy
    let err = memseg_destroy(&mut st, true, seg).unwrap_err();
    assert_eq!(err, MemSegError::StillMapped);
    memseg_unmap(&mut st, true, 7, 0x5000_1000, 1).unwrap(); // 释放最后一页(0x5000_1000)
    memseg_destroy(&mut st, true, seg).expect("destroy ok after unmaps");
    assert!(!st.segments.contains_key(&seg));
    assert_eq!(st.alloc.free.len(), 8, "全部帧归还");
    let mut diag = Vec::new();
    assert!(memseg_alpha_verbose(&st, &mut diag), "完整生命周期后 α 恒真");
}
