use crate::pagewalk::{
    FrameAllocator, FrameAllocatorError, NonLeafLevel, PageTableNode, PageWalkError, Pte,
    SegmentRef, collect_leaves, map_vaddr, map_vaddr_owned, new_owned_task_table, new_task_table,
    translate, unmap_vaddr, unmap_vaddr_owned,
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
    let before = table.clone();
    assert_eq!(
        map_vaddr(&mut table, 0x0000_8000_0000_0000, pte(1, 0, 0x1000)),
        Err(PageWalkError::NonCanonicalAddress),
    );
    assert_eq!(table, before);
}

#[test]
fn translate_rejects_unaligned_and_noncanonical_addresses() {
    let table = base_tree();
    assert_eq!(
        translate(&table, 0x4000_0001),
        Err(PageWalkError::UnalignedAddress),
    );
    assert_eq!(
        translate(&table, 0x0000_8000_0000_0000),
        Err(PageWalkError::NonCanonicalAddress),
    );
}

#[test]
fn unmap_rejects_invalid_addresses_without_modifying_tree() {
    let mut table = base_tree();
    let before = table.clone();
    assert_eq!(
        unmap_vaddr(&mut table, 0x4000_0001),
        Err(PageWalkError::UnalignedAddress),
    );
    assert_eq!(table, before);
    assert_eq!(
        unmap_vaddr(&mut table, 0x0000_8000_0000_0000),
        Err(PageWalkError::NonCanonicalAddress),
    );
    assert_eq!(table, before);
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
    let mut a = FrameAllocator::new(&univ).unwrap();
    let p = a.alloc().expect("frame available");
    assert_eq!(p, 0x3000, "pop 取栈顶");
    assert_eq!(a.free.len(), 2);
    a.free_frame(p).unwrap();
    assert!(a.free.contains(&p));
    assert_eq!(a.free.len(), 3);
    assert_eq!(a.check_invariants(), Ok(()));
}

#[test]
fn allocate_exhausts_universe_returns_none() {
    let univ: Vec<crate::pagewalk::PhysAddr> = vec![0x1000];
    let mut a = FrameAllocator::new(&univ).unwrap();
    assert!(a.alloc().is_some());
    assert!(a.alloc().is_none(), "耗尽后返回 None");
}

#[test]
fn allocator_rejects_duplicate_universe() {
    assert_eq!(
        FrameAllocator::new(&[0x1000, 0x1000]),
        Err(FrameAllocatorError::DuplicateFrame)
    );
}

#[test]
fn allocator_rejects_unknown_return_atomically() {
    let mut a = FrameAllocator::new(&[0x1000]).unwrap();
    let before = a.clone();
    assert_eq!(a.free_frame(0x2000), Err(FrameAllocatorError::UnknownFrame));
    assert_eq!(a, before);
}

#[test]
fn allocator_rejects_duplicate_return_atomically() {
    let mut a = FrameAllocator::new(&[0x1000]).unwrap();
    let f = a.alloc().unwrap();
    a.free_frame(f).unwrap();
    let before = a.clone();
    assert_eq!(a.free_frame(f), Err(FrameAllocatorError::DuplicateFree));
    assert_eq!(a, before);
}

#[test]
fn page_table_structure_is_valid_after_map_and_unmap() {
    let mut table = new_task_table();
    assert_eq!(table.check_invariants(), Ok(()));
    map_vaddr(&mut table, 0x4000_0000, pte(1, 0, 0x1000)).unwrap();
    assert_eq!(table.check_invariants(), Ok(()));
    unmap_vaddr(&mut table, 0x4000_0000).unwrap();
    assert_eq!(table.check_invariants(), Ok(()));
}

#[test]
fn page_table_structure_rejects_wrong_level_and_orphan_node() {
    let mut wrong_root = new_task_table();
    wrong_root.root = PageTableNode::NonLeaf {
        level: NonLeafLevel::Pdpt,
        slots: Box::new(std::array::from_fn(|_| None)),
    };
    assert_eq!(
        wrong_root.check_invariants(),
        Err(PageWalkError::InvalidStructure)
    );

    let mut orphan = new_task_table();
    let child = PageTableNode::NonLeaf {
        level: NonLeafLevel::Pdpt,
        slots: Box::new(std::array::from_fn(|_| None)),
    };
    if let PageTableNode::NonLeaf { slots, .. } = &mut orphan.root {
        slots[0] = Some(child);
    }
    assert_eq!(
        orphan.check_invariants(),
        Err(PageWalkError::InvalidStructure)
    );
}

#[test]
fn owned_page_table_allocates_and_reclaims_backing_frames() {
    let mut allocator = FrameAllocator::new(&[0x1000, 0x2000, 0x3000, 0x4000, 0x5000]).unwrap();
    let mut table = new_owned_task_table(&mut allocator).unwrap();
    assert_eq!(table.table_frames.len(), 1);
    assert_eq!(allocator.free.len(), 4);

    map_vaddr_owned(&mut table, &mut allocator, 0x4000_0000, pte(1, 0, 0x1000)).unwrap();
    assert_eq!(table.table_frames.len(), 4);
    assert_eq!(allocator.free.len(), 1);
    unmap_vaddr_owned(&mut table, &mut allocator, 0x4000_0000).unwrap();
    assert_eq!(table.table_frames.len(), 1);
    assert_eq!(allocator.free.len(), 4);
    table.table_frames[0] = 0x2000;
    assert_eq!(
        table.check_frame_ownership(&allocator),
        Err(PageWalkError::FrameOwnership)
    );
}

#[test]
fn allocator_invariants_reject_duplicate_free_state() {
    let mut allocator = FrameAllocator::new(&[0x1000, 0x2000]).unwrap();
    allocator.alloc();
    allocator.free.push(0x1000);
    assert_eq!(
        allocator.check_invariants(),
        Err(FrameAllocatorError::InvariantViolation)
    );
}

#[test]
fn allocator_invariants_reject_free_frame_outside_universe() {
    let mut allocator = FrameAllocator::new(&[0x1000]).unwrap();
    allocator.free[0] = 0x2000;
    assert_eq!(
        allocator.check_invariants(),
        Err(FrameAllocatorError::InvariantViolation)
    );
}
