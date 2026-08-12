#![allow(dead_code)]

use crate::state::SegId;
use std::collections::{BTreeMap, BTreeSet};

pub const PAGE_SIZE: u64 = 0x1000;
pub type PhysAddr = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PageWalkError {
    UnalignedAddress,
    NonCanonicalAddress,
}

pub fn is_canonical_vaddr(vaddr: u64) -> bool {
    let upper = vaddr >> 48;
    if vaddr & (1 << 47) == 0 {
        upper == 0
    } else {
        upper == 0xffff
    }
}

pub fn validate_page_vaddr(vaddr: u64) -> Result<(), PageWalkError> {
    if vaddr & (PAGE_SIZE - 1) != 0 {
        return Err(PageWalkError::UnalignedAddress);
    }
    if !is_canonical_vaddr(vaddr) {
        return Err(PageWalkError::NonCanonicalAddress);
    }
    Ok(())
}

fn canonicalize_48bit(vaddr: u64) -> u64 {
    let low = vaddr & 0x0000_ffff_ffff_ffff;
    if low & (1 << 47) != 0 {
        low | 0xffff_0000_0000_0000
    } else {
        low
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SegmentRef {
    pub segment: SegId,
    pub offset: u64,
    pub access: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pte {
    pub present: bool,
    pub writable: bool,
    pub user: bool,
    pub nx: bool,
    pub frame_phys: PhysAddr,
    pub mapping: SegmentRef,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonLeafLevel {
    Pml4,
    Pdpt,
    Pd,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageTableNode {
    NonLeaf {
        level: NonLeafLevel,
        slots: Box<[Option<PageTableNode>; 512]>,
    },
    Leaf(Box<[Option<Pte>; 512]>),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PageTable {
    pub root: PageTableNode,
}

fn pt_index(v: u64) -> usize {
    ((v >> 12) & 0x1ff) as usize
}
#[allow(clippy::redundant_closure_call)]
fn empty_slots<T>() -> Box<[Option<T>; 512]> {
    Box::new(std::array::from_fn(|_| None))
}
fn level_shift(level: NonLeafLevel) -> u32 {
    match level {
        NonLeafLevel::Pml4 => 39,
        NonLeafLevel::Pdpt => 30,
        NonLeafLevel::Pd => 21,
    }
}
fn child_level_for(parent: NonLeafLevel) -> Option<NonLeafLevel> {
    match parent {
        NonLeafLevel::Pml4 => Some(NonLeafLevel::Pdpt),
        NonLeafLevel::Pdpt => Some(NonLeafLevel::Pd),
        NonLeafLevel::Pd => None,
    }
}

pub fn new_task_table() -> PageTable {
    PageTable::default()
}

pub fn translate(table: &PageTable, vaddr: u64) -> Result<Option<&Pte>, PageWalkError> {
    validate_page_vaddr(vaddr)?;
    fn go(node: &PageTableNode, vaddr: u64) -> Option<&Pte> {
        match node {
            PageTableNode::Leaf(slots) => slots[pt_index(vaddr)].as_ref(),
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                let child = slots[idx].as_ref()?;
                go(child, vaddr)
            }
        }
    }
    Ok(go(&table.root, vaddr))
}

pub fn map_vaddr(table: &mut PageTable, vaddr: u64, pte: Pte) -> Result<(), PageWalkError> {
    validate_page_vaddr(vaddr)?;
    fn go(node: &mut PageTableNode, vaddr: u64, pte: &Pte) {
        match node {
            PageTableNode::Leaf(slots) => {
                slots[pt_index(vaddr)] = Some(*pte);
            }
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                if slots[idx].is_none() {
                    slots[idx] = Some(match child_level_for(*level) {
                        Some(cl) => PageTableNode::NonLeaf {
                            level: cl,
                            slots: empty_slots(),
                        },
                        None => PageTableNode::Leaf(empty_slots()),
                    });
                }
                go(slots[idx].as_mut().unwrap(), vaddr, pte);
            }
        }
    }
    go(&mut table.root, vaddr, &pte);
    Ok(())
}

pub fn unmap_vaddr(table: &mut PageTable, vaddr: u64) -> Result<bool, PageWalkError> {
    validate_page_vaddr(vaddr)?;
    fn go(node: &mut PageTableNode, vaddr: u64) -> bool {
        match node {
            PageTableNode::Leaf(slots) => {
                let _ = slots[pt_index(vaddr)].take();
                slots.iter().all(|o| o.is_none())
            }
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                let child_empty = match slots[idx].as_mut() {
                    Some(child) => go(child, vaddr),
                    None => false,
                };
                if child_empty {
                    slots[idx] = None;
                }
                slots.iter().all(|o| o.is_none())
            }
        }
    }
    Ok(go(&mut table.root, vaddr))
}

pub fn visit_leaves(table: &PageTable, visitor: &mut impl FnMut(u64, &Pte)) {
    fn go(node: &PageTableNode, base: u64, visitor: &mut impl FnMut(u64, &Pte)) {
        match node {
            PageTableNode::Leaf(slots) => {
                for (i, pte) in slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, pte)| pte.as_ref().map(|pte| (i, pte)))
                {
                    visitor(canonicalize_48bit(base + ((i as u64) << 12)), pte);
                }
            }
            PageTableNode::NonLeaf { level, slots } => {
                let shift = level_shift(*level);
                for (i, child) in slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, child)| child.as_ref().map(|child| (i, child)))
                {
                    go(child, base | ((i as u64) << shift), visitor);
                }
            }
        }
    }

    go(&table.root, 0, visitor);
}

pub fn collect_leaves(table: &PageTable, out: &mut BTreeMap<u64, SegmentRef>) {
    out.clear();
    visit_leaves(table, &mut |va, pte| {
        out.insert(va, pte.mapping);
    });
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameAllocatorError {
    DuplicateFrame,
    UnknownFrame,
    DuplicateFree,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameAllocator {
    pub universe: Vec<PhysAddr>,
    pub free: Vec<PhysAddr>,
}

impl FrameAllocator {
    pub fn new(universe: &[PhysAddr]) -> Result<Self, FrameAllocatorError> {
        let unique: BTreeSet<_> = universe.iter().copied().collect();
        if unique.len() != universe.len() {
            return Err(FrameAllocatorError::DuplicateFrame);
        }
        Ok(FrameAllocator {
            universe: universe.to_vec(),
            free: universe.to_vec(),
        })
    }
    pub fn alloc(&mut self) -> Option<PhysAddr> {
        self.free.pop()
    }
    pub fn free_frame(&mut self, p: PhysAddr) -> Result<(), FrameAllocatorError> {
        if !self.universe.contains(&p) {
            return Err(FrameAllocatorError::UnknownFrame);
        }
        if self.free.contains(&p) {
            return Err(FrameAllocatorError::DuplicateFree);
        }
        self.free.push(p);
        Ok(())
    }
}

impl Default for PageTableNode {
    fn default() -> Self {
        PageTableNode::NonLeaf {
            level: NonLeafLevel::Pml4,
            slots: empty_slots(),
        }
    }
}
