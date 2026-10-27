//! Private writable-leaf snapshots for an uncommitted fork. Each parent has at
//! most one transaction. Parent VM mutation/thread creation/exec is blocked;
//! ordinary COW writes may privatize pages and are preserved on cancellation.
use crate::{bootstrap::MAX_VSPACE_ENTRIES, sched::MAX_TASKS, vspace};
use prima_ostd::sync::StaticCell;

#[derive(Clone, Copy)]
struct Leaf {
    va: u64,
    pte: u64,
}
static LEAVES: StaticCell<[[Leaf; MAX_VSPACE_ENTRIES]; MAX_TASKS]> =
    StaticCell::new([[Leaf { va: 0, pte: 0 }; MAX_VSPACE_ENTRIES]; MAX_TASKS]);
static COUNTS: StaticCell<[usize; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);
static ROOTS: StaticCell<[u64; MAX_TASKS]> = StaticCell::new([0; MAX_TASKS]);

pub fn capture(parent: u32, root: u64, offset: u64) -> Result<(), vspace::VspaceError> {
    let slot = parent as usize;
    if slot >= MAX_TASKS {
        return Err(vspace::VspaceError::BadRoot);
    }
    forget(parent);
    let mut count = 0;
    let result = vspace::visit_user_leaves(
        root,
        |phys| crate::mem::phys_to_virt(phys, offset),
        |va, pte| {
            if count == MAX_VSPACE_ENTRIES {
                return Err(vspace::VspaceError::NoFrame);
            }
            LEAVES.with_mut(|rows| rows[slot][count] = Leaf { va, pte });
            count += 1;
            Ok(())
        },
    );
    if result.is_ok() {
        COUNTS.with_mut(|counts| counts[slot] = count);
        ROOTS.with_mut(|roots| roots[slot] = root);
    }
    result
}

pub fn forget(parent: u32) {
    if (parent as usize) < MAX_TASKS {
        COUNTS.with_mut(|counts| counts[parent as usize] = 0);
        ROOTS.with_mut(|roots| roots[parent as usize] = 0);
    }
}

pub fn restore(parent: u32, offset: u64) {
    let slot = parent as usize;
    if slot >= MAX_TASKS {
        return;
    }
    let root = ROOTS.with(|roots| roots[slot]);
    if root == 0 || crate::sched::root(parent) != Some(root) {
        forget(parent);
        return;
    }
    let count = COUNTS.with(|counts| counts[slot]);
    let map = |phys| crate::mem::phys_to_virt(phys, offset);
    for index in 0..count {
        let leaf = LEAVES.with(|rows| rows[slot][index]);
        if let Ok(current) = vspace::read_leaf(root, leaf.va, map)
            && let Some(restored) = cancelled_leaf(leaf.pte, current)
        {
            vspace::write_leaf_raw(root, leaf.va, restored, map)
                .expect("snapshot leaf path exists");
        }
    }
    forget(parent);
    if crate::sched::current_root() == Some(root) {
        crate::cpu::write_cr3(crate::cpu::cr3().0);
    }
}

/// Do not overwrite a page the parent already privatized during handoff.
pub fn cancelled_leaf(original: u64, current: u64) -> Option<u64> {
    const ACCESSED_DIRTY: u64 = (1 << 5) | (1 << 6);
    let protected = vspace::cow_leaf(original);
    (current & !ACCESSED_DIRTY == protected & !ACCESSED_DIRTY)
        .then_some(original | (current & ACCESSED_DIRTY))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancel_restores_only_unchanged_snapshot_pages() {
        let original = 0x1000 | vspace::PT_PRESENT | vspace::PT_USER | vspace::PT_WRITABLE;
        let protected = vspace::cow_leaf(original);
        assert_eq!(
            cancelled_leaf(original, protected | (1 << 5)),
            Some(original | (1 << 5))
        );
        assert_eq!(cancelled_leaf(original, protected ^ 0x3000), None);
        assert_eq!(cancelled_leaf(original, original), None);
        assert_eq!(
            cancelled_leaf(original & !vspace::PT_WRITABLE, protected),
            None
        );
    }
}
