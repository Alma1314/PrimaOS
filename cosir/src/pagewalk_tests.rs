use crate::pagewalk::{
    FrameAllocator, PageWalkError, Pte, SegmentRef, collect_leaves, map_vaddr, new_task_table,
    translate, unmap_vaddr,
};
use crate::state::{ACCESS_R, ACCESS_W, ACCESS_X, SegId};
use std::collections::BTreeMap;

fn rwx() -> u8 {
    ACCESS_R | ACCESS_W | ACCESS_X
}

fn pte(seg: SegId, off: u64, frame: crate::pagewalk::PhysAddr) -> Pte {
    Pte {
        present: true,
        writable: true,
        user: true,
        nx: false,
        frame_phys: frame,
        mapping: SegmentRef {
            segment: seg,
            offset: off,
            access: rwx(),
        },
    }
}

fn base_tree() -> crate::pagewalk::PageTable {
    // 在 0x4000_0000 放一个叶，物理帧 0x1000，segment 1 offset 0
    let mut t = new_task_table();
    map_vaddr(&mut t, 0x4000_0000, pte(1, 0, 0x1000)).unwrap();
    t
}

#[test]
fn translate_returns_leaf_after_map() {
    let t = base_tree();
    let got = translate(&t, 0x4000_0000)
        .unwrap()
        .expect("mapped leaf present");
    assert_eq!(got.frame_phys, 0x1000);
    assert_eq!(got.mapping.segment, 1);
    assert!(got.mapping.access & ACCESS_R != 0);
}

#[test]
fn unmap_clears_leaf_and_reclaims_empty_tables() {
    let mut t = base_tree();
    assert!(translate(&t, 0x4000_0000).unwrap().is_some());
    let empty = unmap_vaddr(&mut t, 0x4000_0000).unwrap();
    assert!(empty, "叶被清后整棵子树应回收为空");
    assert!(translate(&t, 0x4000_0000).unwrap().is_none());
}

#[test]
fn collect_leaves_maps_vaddr_to_segment_ref() {
    let mut t = base_tree();
    map_vaddr(&mut t, 0x4001_0000, pte(1, 1, 0x2000)).unwrap();
    let mut leaves = BTreeMap::new();
    collect_leaves(&t, &mut leaves);
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[&0x4000_0000].offset, 0);
    assert_eq!(leaves[&0x4001_0000].offset, 1);
}

#[test]
fn map_rejects_page_offset_without_modifying_tree() {
    let mut table = new_task_table();
    let before = table.clone();
    assert_eq!(
        map_vaddr(&mut table, 0x4000_0001, pte(1, 0, 0x1000)),
        Err(PageWalkError::UnalignedAddress),
    );
    assert_eq!(table, before);
}

#[test]
fn pagewalk_rejects_noncanonical_alias() {
    let mut table = new_task_table();
    assert_eq!(
        map_vaddr(&mut table, 0x0000_8000_0000_0000, pte(1, 0, 0x1000)),
        Err(PageWalkError::NonCanonicalAddress),
    );
}

#[test]
fn high_half_leaf_roundtrips_with_sign_extension() {
    const VA: u64 = 0xffff_8000_0000_0000;
    let mut table = new_task_table();
    map_vaddr(&mut table, VA, pte(1, 0, 0x1000)).unwrap();
    assert!(translate(&table, VA).unwrap().is_some());
    let mut leaves = BTreeMap::new();
    collect_leaves(&table, &mut leaves);
    assert!(leaves.contains_key(&VA));
}

#[test]
fn frame_allocator_alloc_free_roundtrip() {
    let univ: Vec<crate::pagewalk::PhysAddr> = vec![0x1000, 0x2000, 0x3000];
    let mut a = FrameAllocator::new(&univ);
    let p = a.alloc().expect("frame available");
    assert_eq!(p, 0x3000, "pop 取栈顶");
    assert_eq!(a.free.len(), 2);
    a.free_frame(p);
    assert!(a.free.contains(&p));
    assert_eq!(a.free.len(), 3);
}

#[test]
fn allocate_exhausts_universe_returns_none() {
    let univ: Vec<crate::pagewalk::PhysAddr> = vec![0x1000];
    let mut a = FrameAllocator::new(&univ);
    assert!(a.alloc().is_some());
    assert!(a.alloc().is_none(), "耗尽后返回 None");
}
