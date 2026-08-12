use crate::memseg_refine::{
    MemSegError, RefineState, memseg_alpha_verbose, memseg_create, memseg_destroy, memseg_map,
    memseg_unmap, spawn_task, SEG_DESTROY, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X,
};
use crate::state::{ACCESS_R, ACCESS_W, ACCESS_X};

// 8 页物理帧
fn universe() -> Vec<u64> {
    (0u64..8).map(|i| 0x1000 + i * 0x1000).collect()
}

fn seg_map_rights() -> u8 {
    SEG_MAP_R | SEG_MAP_W | SEG_MAP_X
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
