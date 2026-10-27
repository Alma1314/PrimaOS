//! Bounded, authenticated console reads. A committed reply stays tracked until
//! acknowledged, so cancellation cannot discard bytes already delivered.
use prima_runtime::Capability;

pub const READ_DEPTH: usize = 8;
pub const READ_SLOTS: usize = super::sessions::MAX_CLIENTS * READ_DEPTH;
pub const WAITING: u8 = 0;
pub const SERVED: u8 = 1;
pub const CANCEL_WAITING: u8 = 2;
pub const CANCEL_SERVED: u8 = 3;

#[derive(Clone, Copy, Debug)]
pub struct Read {
    pub cookie: u64,
    pub reply: Capability,
    pub sequence: u16,
    pub max: u8,
    pub state: u8,
    order: u64,
}
pub struct Reads {
    pub slots: [Option<Read>; READ_SLOTS],
    next: u64,
}
impl Default for Reads {
    fn default() -> Self {
        Self::new()
    }
}
impl Reads {
    pub const fn new() -> Self {
        Self {
            slots: [None; READ_SLOTS],
            next: 0,
        }
    }
    pub fn enqueue(&mut self, cookie: u64, reply: Capability, sequence: u16, max: u8) -> bool {
        if sequence == 0
            || self
                .slots
                .iter()
                .flatten()
                .any(|r| r.cookie == cookie && r.sequence == sequence)
            || self
                .slots
                .iter()
                .flatten()
                .filter(|r| r.cookie == cookie)
                .count()
                == READ_DEPTH
        {
            return false;
        }
        let Some(slot) = self.slots.iter_mut().find(|r| r.is_none()) else {
            return false;
        };
        let Some(next) = self.next.checked_add(1) else {
            return false;
        };
        *slot = Some(Read {
            cookie,
            reply,
            sequence,
            max,
            state: WAITING,
            order: self.next,
        });
        self.next = next;
        true
    }
    pub fn oldest(&self) -> Option<usize> {
        self.slots
            .iter()
            .enumerate()
            .filter_map(|(i, r)| r.filter(|r| r.state == WAITING).map(|r| (i, r.order)))
            .min_by_key(|(_, order)| *order)
            .map(|(i, _)| i)
    }
    pub fn cancel(&mut self, cookie: u64, sequence: u16) -> bool {
        if let Some(r) = self
            .slots
            .iter_mut()
            .flatten()
            .find(|r| r.cookie == cookie && r.sequence == sequence)
        {
            r.state = if r.state == WAITING || r.state == CANCEL_WAITING {
                CANCEL_WAITING
            } else {
                CANCEL_SERVED
            };
            return true;
        }
        false
    }
    pub fn acknowledge(&mut self, cookie: u64, sequence: u16) {
        if let Some(slot) = self.slots.iter_mut().find(|r| {
            r.is_some_and(|r| r.cookie == cookie && r.sequence == sequence && r.state != WAITING)
        }) {
            *slot = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_preserves_commit_and_fifo_and_authenticates_the_generation() {
        let mut reads = Reads::new();
        let cap = Capability::from_raw((1 << 32) | 1).unwrap();
        let a = (4 << 32) | 3;
        let b = (5 << 32) | 3;
        assert!(reads.enqueue(a, cap, 1, 3));
        assert!(reads.enqueue(b, cap, 1, 3));
        let first = reads.oldest().unwrap();
        reads.cancel(b, 1);
        assert_eq!(reads.slots[first].unwrap().state, WAITING);
        reads.slots[first].as_mut().unwrap().state = SERVED;
        reads.cancel(a, 1);
        assert_eq!(reads.slots[first].unwrap().state, CANCEL_SERVED);
        reads.acknowledge(b, 1);
        assert!(reads.slots[first].is_some());
        reads.acknowledge(a, 1);
        assert!(reads.oldest().is_none());
        for seq in 1..=READ_DEPTH as u16 {
            assert!(reads.enqueue(a, cap, seq, 1));
        }
        assert!(!reads.enqueue(a, cap, 9, 1));
    }
}
