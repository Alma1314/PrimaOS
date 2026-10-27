//! Per-Process local handle slots for controlled fork. Threads share their
//! Process view; fork copies its generations and replaces owned tokens. Moving a
//! token preserves its local identity; retiring it advances every local alias.
use crate::{
    arena::{Arena, ArenaSlice},
    bootstrap::{MAX_CAPABILITY_SLOTS, MAX_TASKS},
    handle::Handle,
};

#[cfg(test)]
const REGIONS: usize = 512;
#[cfg(not(test))]
const REGIONS: usize = 1;

#[derive(Clone, Copy)]
struct LocalSlot {
    generation: u32,
    token: Handle,
}
static ENTRIES: Arena<LocalSlot, { MAX_CAPABILITY_SLOTS * MAX_TASKS }, REGIONS> = Arena::new();

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForkViews {
    slice: ArenaSlice,
    enabled: u32,
    revision: u64,
}

impl ForkViews {
    pub fn claim() -> Self {
        Self {
            slice: ENTRIES.claim(),
            enabled: 0,
            revision: 0,
        }
    }

    fn space_index(space: u32) -> Option<usize> {
        (space as usize).checked_sub(1).filter(|i| *i < MAX_TASKS)
    }

    fn entry_index(space: u32, slot: usize) -> Option<usize> {
        (slot > 0 && slot < MAX_CAPABILITY_SLOTS)
            .then(|| Self::space_index(space).map(|i| i * MAX_CAPABILITY_SLOTS + slot))
            .flatten()
    }

    fn get(&self, space: u32, slot: usize) -> Option<LocalSlot> {
        ENTRIES.get(self.slice, Self::entry_index(space, slot)?)
    }

    fn set(&mut self, space: u32, slot: usize, value: LocalSlot) {
        ENTRIES.set(
            self.slice,
            Self::entry_index(space, slot).expect("local slot"),
            Some(value),
        );
        self.revision += 1;
    }

    pub fn enabled(&self, space: u32) -> bool {
        Self::space_index(space).is_some_and(|index| self.enabled & (1 << index) != 0)
    }

    pub fn clear(&mut self, space: u32) {
        if let Some(index) = Self::space_index(space) {
            self.enabled &= !(1 << index);
            self.revision += 1;
        }
    }

    /// Copy all generations, including vacant/retired slots. A non-fork parent
    /// supplies its global slot generations and tokens as the initial view.
    pub fn initialize(
        &mut self,
        child: u32,
        parent: u32,
        mut global: impl FnMut(usize) -> (u32, Handle),
    ) -> bool {
        let Some(index) = Self::space_index(child) else {
            return false;
        };
        if child == parent {
            return false;
        }
        for slot in 1..MAX_CAPABILITY_SLOTS {
            let entry = if self.enabled(parent) {
                self.get(parent, slot).expect("complete parent view")
            } else {
                let (generation, token) = global(slot);
                LocalSlot { generation, token }
            };
            self.set(child, slot, entry);
        }
        self.enabled |= 1 << index;
        true
    }

    /// Unknown/stale local handles resolve to ZERO, never to a guessed global
    /// token. Existing native error mapping reports that token as unavailable.
    pub fn resolve(&self, space: u32, local: Handle) -> Handle {
        if !self.enabled(space) {
            return local;
        }
        self.get(space, local.index() as usize)
            .filter(|entry| entry.generation == local.generation())
            .map_or(Handle::ZERO, |entry| entry.token)
    }

    pub fn rebind(&mut self, space: u32, source: Handle, cloned: Handle) -> bool {
        for slot in 1..MAX_CAPABILITY_SLOTS {
            if let Some(mut entry) = self.get(space, slot)
                && entry.token == source
            {
                entry.token = cloned;
                self.set(space, slot, entry);
                return true;
            }
        }
        false
    }

    pub fn retire(&mut self, token: Handle) {
        for space in 1..=MAX_TASKS as u32 {
            if !self.enabled(space) {
                continue;
            }
            for slot in 1..MAX_CAPABILITY_SLOTS {
                if let Some(mut entry) = self.get(space, slot)
                    && entry.token == token
                {
                    entry.token = Handle::ZERO;
                    entry.generation = entry.generation.checked_add(1).unwrap_or(0);
                    self.set(space, slot, entry);
                }
            }
        }
    }

    pub fn expose(
        &mut self,
        space: u32,
        token: Handle,
        mut live: impl FnMut(Handle) -> bool,
    ) -> Handle {
        if !self.enabled(space) {
            return token;
        }
        let mut free = None;
        for slot in 1..MAX_CAPABILITY_SLOTS {
            let mut entry = self.get(space, slot).expect("complete local view");
            if entry.token == token {
                return Handle::new(slot as u32, entry.generation).expect("live local generation");
            }
            if !entry.token.is_zero() && !live(entry.token) {
                entry.token = Handle::ZERO;
                entry.generation = entry.generation.checked_add(1).unwrap_or(0);
                self.set(space, slot, entry);
            }
            if entry.token.is_zero() && entry.generation != 0 && free.is_none() {
                free = Some((slot, entry));
            }
        }
        // Entries reference distinct live global tokens. A newly returned token
        // is itself globally allocated, so one local slot must be available,
        // unless every remaining slot has exhausted its local generation.
        let Some((slot, mut entry)) = free else {
            return Handle::ZERO;
        };
        entry.token = token;
        self.set(space, slot, entry);
        Handle::new(slot as u32, entry.generation).expect("free local generation")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(slot: usize) -> (u32, Handle) {
        (
            1,
            if slot == 4 {
                Handle::new(4, 1).unwrap()
            } else {
                Handle::ZERO
            },
        )
    }

    #[test]
    fn retiring_a_clone_does_not_reactivate_its_local_handle_when_original_is_received() {
        let mut views = ForkViews::claim();
        let original = Handle::new(4, 1).unwrap();
        let clone = Handle::new(5, 1).unwrap();
        assert!(views.initialize(2, 1, seed));
        assert!(views.rebind(2, original, clone));
        assert_eq!(views.resolve(2, original), clone);
        views.retire(clone);
        let received = views.expose(2, original, |_| true);
        assert_ne!(received, original);
        assert_eq!(views.resolve(2, received), original);
        assert_eq!(views.resolve(2, original), Handle::ZERO);
        assert_eq!(views.resolve(1, original), original);
    }

    #[test]
    fn grandchild_inherits_vacant_generations_and_never_falls_back_to_global_handles() {
        let mut views = ForkViews::claim();
        let original = Handle::new(4, 1).unwrap();
        let clone = Handle::new(5, 1).unwrap();
        assert!(views.initialize(2, 1, seed));
        assert!(views.rebind(2, original, clone));
        views.retire(clone);
        assert!(views.initialize(3, 2, |_| panic!("parent view exists")));
        let received = views.expose(3, original, |_| true);
        assert_ne!(received, original);
        assert_eq!(views.resolve(3, original), Handle::ZERO);
        assert_eq!(views.resolve(3, clone), Handle::ZERO);
        assert_eq!(views.resolve(3, received), original);
    }

    #[test]
    fn exhausted_local_generations_retire_instead_of_wrapping() {
        let mut views = ForkViews::claim();
        let token = Handle::new(4, 1).unwrap();
        assert!(views.initialize(2, 1, |slot| (
            u32::MAX,
            if slot == 4 { token } else { Handle::ZERO }
        )));
        views.retire(token);
        let old = Handle::new(4, u32::MAX).unwrap();
        assert_eq!(views.resolve(2, old), Handle::ZERO);
        assert!(views.get(2, 4).unwrap().generation == 0);
    }
}
