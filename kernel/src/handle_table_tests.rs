//! Bounded traces of the real slot table, paired with KernelHandleTable.lean.
use super::{CapabilitySlotTable, MAX_CAPABILITY_SLOTS, SlotEntry, SlotState};
use crate::handle::Handle;

#[test]
fn table_traces_preserve_other_slots_and_never_revive_freed_handles() {
    fn visit(table: CapabilitySlotTable, issued: Vec<Handle>, freed: Vec<Handle>, depth: usize) {
        for old in &freed {
            assert_ne!(table.check(*old), SlotState::Live);
        }
        for entry in table.entries {
            if entry.generation == 0 {
                assert!(!entry.in_use);
            }
        }
        if depth == 0 {
            return;
        }
        // Include normal lowest-slot allocation and explicit reservations.
        for slot in 0..=3 {
            let mut next = table;
            let allocated = if slot == 0 {
                next.alloc()
            } else if next.slot_free(slot) {
                Some(next.reserve(slot))
            } else {
                continue;
            };
            if let Some(handle) = allocated {
                assert_eq!(next.check(handle), SlotState::Live);
                for index in 0..MAX_CAPABILITY_SLOTS {
                    if index != handle.index() as usize {
                        assert_eq!(next.entries[index], table.entries[index]);
                    }
                }
                let mut handles = issued.clone();
                handles.push(handle);
                visit(next, handles, freed.clone(), depth - 1);
            } else {
                assert_eq!(next, table);
            }
        }
        // Stale, repeated and out-of-range frees must leave the entire table unchanged.
        for handle in issued.iter().copied().chain([
            Handle::ZERO,
            Handle::new(MAX_CAPABILITY_SLOTS as u32, 1).unwrap(),
        ]) {
            let mut next = table;
            if next.free(handle) {
                for index in 0..MAX_CAPABILITY_SLOTS {
                    if index != handle.index() as usize {
                        assert_eq!(next.entries[index], table.entries[index]);
                    }
                }
                let mut old = freed.clone();
                old.push(handle);
                visit(next, issued.clone(), old, depth - 1);
            } else {
                assert_eq!(next, table);
            }
        }
    }

    let mut table = CapabilitySlotTable::empty();
    // Inject end-of-counter states instead of attempting 2^32 iterations.
    table.entries.fill(SlotEntry {
        generation: 0,
        in_use: false,
    });
    for (slot, generation) in [(1, u32::MAX - 1), (2, u32::MAX), (3, 1)] {
        table.entries[slot].generation = generation;
    }
    visit(table, Vec::new(), Vec::new(), 6);
}
