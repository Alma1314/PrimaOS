#![allow(dead_code)]

use crate::state::SegId;
use std::collections::BTreeMap;

pub const PAGE_SIZE: u64 = 0x1000;
pub type PhysAddr = u64;

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
pub enum NonLeafLevel { Pml4, Pdpt, Pd }

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PageTableNode {
    NonLeaf { level: NonLeafLevel, slots: Box<[Option<PageTableNode>; 512]> },
    Leaf(Box<[Option<Pte>; 512]>),
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PageTable { pub root: PageTableNode }

fn pt_index(v: u64) -> usize { ((v >> 12) & 0x1ff) as usize }
#[allow(clippy::redundant_closure_call)]
fn empty_slots<T>() -> Box<[Option<T>; 512]> { Box::new(std::array::from_fn(|_| None)) }
fn level_shift(level: NonLeafLevel) -> u32 {
    match level { NonLeafLevel::Pml4 => 39, NonLeafLevel::Pdpt => 30, NonLeafLevel::Pd => 21 }
}
fn child_level_for(parent: NonLeafLevel) -> Option<NonLeafLevel> {
    match parent {
        NonLeafLevel::Pml4 => Some(NonLeafLevel::Pdpt),
        NonLeafLevel::Pdpt => Some(NonLeafLevel::Pd),
        NonLeafLevel::Pd => None,
    }
}

pub fn new_task_table() -> PageTable { PageTable::default() }

pub fn translate<'a>(table: &'a PageTable, vaddr: u64) -> Option<&'a Pte> {
    fn go<'a>(node: &'a PageTableNode, vaddr: u64) -> Option<&'a Pte> {
        match node {
            PageTableNode::Leaf(slots) => slots[pt_index(vaddr)].as_ref(),
            PageTableNode::NonLeaf { level, slots } => {
                let idx = ((vaddr >> level_shift(*level)) & 0x1ff) as usize;
                let child = slots[idx].as_ref()?;
                go(child, vaddr)
            }
        }
    }
    go(&table.root, vaddr)
}

pub fn map_vaddr(table: &mut PageTable, vaddr: u64, pte: Pte) {
    fn go(node: &mut PageTableNode, vaddr: u64, pte: &Pte) {
        match node {
            PageTableNode::Leaf(slots) => { slots[pt_index(vaddr)] = Some(*pte); }
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
}

pub fn unmap_vaddr(table: &mut PageTable, vaddr: u64) -> bool {
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
                if child_empty { slots[idx] = None; }
                slots.iter().all(|o| o.is_none())
            }
        }
    }
    go(&mut table.root, vaddr)
}

pub fn collect_leaves(table: &PageTable, out: &mut BTreeMap<u64, SegmentRef>) {
    fn go(node: &PageTableNode, base: u64, out: &mut BTreeMap<u64, SegmentRef>) {
        match node {
            PageTableNode::Leaf(slots) => {
                for (i, o) in slots.iter().enumerate() {
                    if let Some(pte) = o {
                        out.insert(base + ((i as u64) << 12), pte.mapping);
                    }
                }
            }
            PageTableNode::NonLeaf { level, slots } => {
                let shift = level_shift(*level);
                for (i, o) in slots.iter().enumerate() {
                    if let Some(child) = o {
                        go(child, base | ((i as u64) << shift), out);
                    }
                }
            }
        }
    }
    out.clear();
    go(&table.root, 0, out);
}

#[derive(Clone, Debug)]
pub struct FrameAllocator {
    pub universe: Vec<PhysAddr>,
    pub free: Vec<PhysAddr>,
}

impl FrameAllocator {
    pub fn new(universe: &[PhysAddr]) -> Self {
        let free = universe.to_vec();
        FrameAllocator { universe: universe.to_vec(), free }
    }
    pub fn alloc(&mut self) -> Option<PhysAddr> { self.free.pop() }
    pub fn free_frame(&mut self, p: PhysAddr) {
        assert!(!self.free.contains(&p), "W6: 帧重复归还");
        self.free.push(p);
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
