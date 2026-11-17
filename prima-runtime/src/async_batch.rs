//! P3 version-2 async batch protocol state machine.
//!
//! The protocol is ordered by the kernel IPC edges, not by lock-free shared
//! memory: the producer fills a credit page before sending `ASYNC_DOORBELL`,
//! and the store reads that page only after receiving the doorbell; the store
//! writes results before sending `ASYNC_COMPLETE`. No party reads a page the
//! other side is still writing, so this crate keeps its `forbid(unsafe_code)`
//! and uses no atomics. P5 may revisit lock-free access with its own contract.
//!
//! `Session` is the store-side authority; `Producer` is the client-side view.
//! Both are pure and host-testable; the wire tags live in [`crate::bulk`].

/// Credits per session. Each credit owns one `bulk::PAGE_BYTES` data page.
pub const CREDITS: u64 = 4;

/// Stored batches a single drain pass may claim from one session before
/// rotating to the next live session.
pub const FAIR_QUOTA: u64 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishError {
    /// `tail - head > CREDITS`: the producer overran its bounded window.
    Full,
    /// The doorbell names an older session generation.
    Stale,
    /// Backwards or otherwise invalid `tail`; the session is reset.
    Protocol,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// Execute the batch in `ordinal % CREDITS`.
    Execute(u64),
    /// The batch was cancelled before claim; complete it with `ECANCELED`
    /// and no VFS effect.
    Cancelled(u64),
    /// No published batch is waiting.
    Idle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cancel {
    /// The batch had not been claimed and will not execute.
    Cancelled,
    /// The batch was already claimed; it runs to completion normally.
    Claimed,
    /// Not outstanding (already completed or never published).
    Missing,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseError {
    /// Batches are still outstanding; drain first.
    Busy,
    /// The session was reset; the client must re-open.
    Reset,
}

/// Store-side session state. All methods are driven by received messages.
#[derive(Clone, Copy, Debug)]
pub struct Session {
    generation: u64,
    published: u64,
    claimed: u64,
    head: u64,
    cancelled: [bool; CREDITS as usize],
    reset: bool,
}

impl Session {
    pub const fn new(generation: u64) -> Self {
        Self {
            generation,
            published: 0,
            claimed: 0,
            head: 0,
            cancelled: [false; CREDITS as usize],
            reset: false,
        }
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Data page index for an absolute ordinal.
    pub const fn page(ordinal: u64) -> usize {
        (ordinal % CREDITS) as usize
    }

    /// Batches received but not yet completed.
    pub fn outstanding(&self) -> u64 {
        self.published.wrapping_sub(self.head)
    }

    pub fn reset_flag(&self) -> bool {
        self.reset
    }

    /// Doorbell: the producer announces all batches below `tail` are filled.
    /// Repeated or coalesced doorbells are idempotent.
    pub fn publish(&mut self, generation: u64, tail: u64) -> Result<(), PublishError> {
        if self.reset {
            return Err(PublishError::Protocol);
        }
        if generation != self.generation {
            return Err(PublishError::Stale);
        }
        // A coalesced duplicate is a no-op; a backwards tail is a violation.
        // Signed modular distance keeps the u64 wrap unambiguous.
        if (tail.wrapping_sub(self.published) as i64) < 0 {
            self.reset = true;
            return Err(PublishError::Protocol);
        }
        if tail.wrapping_sub(self.head) > CREDITS {
            return Err(PublishError::Full);
        }
        self.published = tail;
        Ok(())
    }

    /// Claim the next published batch, FIFO. Cancelled batches are reported so
    /// the caller can complete them with `ECANCELED` instead of executing.
    pub fn claim(&mut self) -> Claim {
        if self.claimed == self.published {
            return Claim::Idle;
        }
        let ordinal = self.claimed;
        self.claimed = self.claimed.wrapping_add(1);
        if self.cancelled[Self::page(ordinal)] {
            self.cancelled[Self::page(ordinal)] = false;
            Claim::Cancelled(ordinal)
        } else {
            Claim::Execute(ordinal)
        }
    }

    /// Complete the oldest in-flight batch and return its ordinal.
    pub fn complete(&mut self) -> Option<u64> {
        if self.head == self.claimed {
            return None;
        }
        let ordinal = self.head;
        self.head = self.head.wrapping_add(1);
        Some(ordinal)
    }

    /// Cancel a published batch that has not been claimed. Cancellation after
    /// claim is not possible: the batch runs to completion.
    pub fn cancel(&mut self, ordinal: u64) -> Cancel {
        let outstanding = self.published.wrapping_sub(self.head);
        let offset = ordinal.wrapping_sub(self.head);
        if self.reset || offset >= outstanding {
            return Cancel::Missing;
        }
        if offset < self.claimed.wrapping_sub(self.head) {
            return Cancel::Claimed;
        }
        self.cancelled[Self::page(ordinal)] = true;
        Cancel::Cancelled
    }

    /// Abort unclaimed batches with `ECANCELED` completions and refuse further
    /// publishes. In-flight claims still complete; the generation advances so
    /// old doorbells are stale.
    pub fn reset(&mut self) {
        for ordinal in self.claimed..self.published {
            self.cancelled[Self::page(ordinal)] = true;
        }
        self.reset = true;
        self.generation = self.generation.wrapping_add(1);
    }

    /// Close is accepted only when every batch has completed.
    pub fn close(&self) -> Result<(), CloseError> {
        if self.reset {
            return Err(CloseError::Reset);
        }
        if self.published != self.head {
            return Err(CloseError::Busy);
        }
        Ok(())
    }
}

/// Client-side view. It never reads store-owned fields; it advances on its own
/// publishes and on `ASYNC_COMPLETE` observations.
#[derive(Clone, Copy, Debug)]
pub struct Producer {
    generation: u64,
    published: u64,
    completed: u64,
}

impl Producer {
    pub const fn new(generation: u64) -> Self {
        Self {
            generation,
            published: 0,
            completed: 0,
        }
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Publish the next credit if the bounded window has room, returning the
    /// ordinal whose page must be filled before the doorbell is sent.
    pub fn try_publish(&mut self) -> Option<u64> {
        if self.published.wrapping_sub(self.completed) >= CREDITS {
            return None;
        }
        let ordinal = self.published;
        self.published = self.published.wrapping_add(1);
        Some(ordinal)
    }

    /// Observe a store `COMPLETE` carrying `head`.
    pub fn observe(&mut self, head: u64) -> Result<(), PublishError> {
        if head.wrapping_sub(self.completed) > self.published.wrapping_sub(self.completed) {
            return Err(PublishError::Protocol);
        }
        self.completed = head;
        Ok(())
    }

    /// Whether an `ASYNC_CLOSE` may be sent.
    pub fn drained(&self) -> bool {
        self.completed == self.published
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use super::*;
    use std::collections::{BTreeSet, VecDeque};
    use std::vec::Vec;

    #[test]
    fn full_window_is_bounded_and_completes_in_order() {
        let mut producer = Producer::new(7);
        let mut session = Session::new(7);
        for expected in 0..CREDITS {
            let ordinal = producer.try_publish().unwrap();
            assert_eq!(ordinal, expected);
            assert_eq!(Session::page(ordinal), expected as usize);
            session.publish(7, ordinal + 1).unwrap();
        }
        assert_eq!(producer.try_publish(), None);
        assert_eq!(session.publish(7, CREDITS + 1), Err(PublishError::Full));
        for expected in 0..CREDITS {
            assert_eq!(session.claim(), Claim::Execute(expected));
            assert_eq!(session.complete(), Some(expected));
            producer.observe(expected + 1).unwrap();
        }
        assert_eq!(session.claim(), Claim::Idle);
        assert_eq!(session.close(), Ok(()));
        assert!(producer.drained());
        assert_eq!(producer.try_publish(), Some(CREDITS));
    }

    #[test]
    fn cancel_before_claim_skips_execution_after_claim_wins() {
        let mut session = Session::new(1);
        session.publish(1, 3).unwrap();
        assert_eq!(session.cancel(1), Cancel::Cancelled);
        assert_eq!(session.claim(), Claim::Execute(0));
        assert_eq!(session.cancel(0), Cancel::Claimed);
        assert_eq!(session.complete(), Some(0));
        assert_eq!(session.claim(), Claim::Cancelled(1));
        assert_eq!(session.complete(), Some(1));
        assert_eq!(session.claim(), Claim::Execute(2));
        assert_eq!(session.cancel(2), Cancel::Claimed);
        assert_eq!(session.cancel(9), Cancel::Missing);
    }

    #[test]
    fn backwards_tail_resets_and_stale_generation_is_rejected() {
        let mut session = Session::new(3);
        session.publish(3, 2).unwrap();
        assert_eq!(session.publish(3, 1), Err(PublishError::Protocol));
        assert!(session.reset_flag());
        assert_eq!(session.publish(3, 3), Err(PublishError::Protocol));
        assert_eq!(session.close(), Err(CloseError::Reset));

        let mut fresh = Session::new(4);
        assert_eq!(fresh.publish(3, 1), Err(PublishError::Stale));
        fresh.publish(4, 1).unwrap();
    }

    #[test]
    fn u64_counters_wrap_without_breaking_the_bounded_window() {
        let mut session = Session::new(1);
        session.published = u64::MAX - 1;
        session.claimed = u64::MAX - 1;
        session.head = u64::MAX - 1;
        session.publish(1, u64::MAX).unwrap();
        session.publish(1, 0).unwrap();
        assert_eq!(session.published, 0);
        for expected in [u64::MAX - 1, u64::MAX] {
            assert_eq!(session.claim(), Claim::Execute(expected));
            assert_eq!(session.complete(), Some(expected));
        }
        assert_eq!(session.head, 0);
        session.publish(1, 4).unwrap();
        assert_eq!(session.publish(1, 5), Err(PublishError::Full));
        for _ in 0..4 {
            assert!(matches!(session.claim(), Claim::Execute(_)));
            assert!(session.complete().is_some());
        }
        assert_eq!(session.claim(), Claim::Idle);
        assert_eq!(session.head, 4);
        assert_eq!(session.close(), Ok(()));
    }

    #[test]
    fn reset_aborts_unclaimed_but_lets_claimed_finish() {
        let mut session = Session::new(1);
        session.publish(1, 4).unwrap();
        assert_eq!(session.claim(), Claim::Execute(0));
        assert_eq!(session.claim(), Claim::Execute(1));
        session.reset();
        assert_eq!(session.generation(), 2);
        assert_eq!(session.complete(), Some(0));
        assert_eq!(session.complete(), Some(1));
        // Unclaimed batches 2 and 3 are aborted, not executed.
        assert_eq!(session.claim(), Claim::Cancelled(2));
        assert_eq!(session.complete(), Some(2));
        assert_eq!(session.claim(), Claim::Cancelled(3));
        assert_eq!(session.complete(), Some(3));
        assert_eq!(session.claim(), Claim::Idle);
        assert_eq!(session.complete(), None);
        assert_eq!(session.publish(2, 0), Err(PublishError::Protocol));
    }

    /// One exhaustive state of the message model: indices plus the in-flight
    /// doorbell and completion queues (IPC is FIFO per sender).
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct State {
        session: [u64; 4],  // generation, published, claimed, head
        producer: [u64; 2], // published, completed
        doorbells: Vec<u64>,
        completes: Vec<u64>,
    }

    impl State {
        fn invariants(&self) -> bool {
            let (generation, published, claimed, head) = (
                self.session[0],
                self.session[1],
                self.session[2],
                self.session[3],
            );
            let _ = generation;
            head <= claimed
                && claimed <= published
                && published - head <= CREDITS
                && self.producer[1] <= self.producer[0]
                && self.producer[0] - self.producer[1] <= CREDITS
                && self.completes.windows(2).all(|w| w[0] <= w[1])
        }
    }

    /// Explore all message interleavings up to `limit` steps. Every reachable
    /// state must satisfy the bounded-window and ordering invariants.
    #[test]
    fn bounded_interleavings_preserve_window_and_order_invariants() {
        let start = State {
            session: [1, 0, 0, 0],
            producer: [0, 0],
            doorbells: Vec::new(),
            completes: Vec::new(),
        };
        let mut frontier = VecDeque::from([(start, 0usize)]);
        let mut seen = BTreeSet::new();
        let mut states = 0usize;
        while let Some((state, depth)) = frontier.pop_front() {
            assert!(state.invariants(), "invariant violated: {state:?}");
            if depth == 8 || !seen.insert(state.clone()) {
                continue;
            }
            states += 1;
            let mut push = Vec::new();

            // Producer publishes another credit and queues its doorbell.
            if state.producer[0] - state.producer[1] < CREDITS {
                let mut s = state.clone();
                s.producer[0] += 1;
                s.doorbells.push(s.producer[0]);
                push.push(s);
            }
            // Store receives the oldest doorbell.
            if let Some(tail) = state.doorbells.first().copied() {
                let mut session = Session::new(state.session[0]);
                session.published = state.session[1];
                session.claimed = state.session[2];
                session.head = state.session[3];
                if session.publish(state.session[0], tail).is_ok() {
                    let mut s = state.clone();
                    s.doorbells.remove(0);
                    s.session[1] = session.published;
                    push.push(s);
                }
            }
            // Store claims one batch.
            if state.session[2] < state.session[1] {
                let mut s = state.clone();
                s.session[2] += 1;
                push.push(s);
            }
            // Store completes the oldest claimed batch and queues COMPLETE.
            if state.session[3] < state.session[2] {
                let mut s = state.clone();
                s.session[3] += 1;
                s.completes.push(s.session[3]);
                push.push(s);
            }
            // Client observes the oldest coalesced completion.
            if let Some(head) = state.completes.first().copied()
                && head >= state.producer[1]
            {
                let mut s = state.clone();
                s.completes.remove(0);
                s.producer[1] = head;
                push.push(s);
            }
            for s in push {
                frontier.push_back((s, depth + 1));
            }
        }
        assert!(states > 20, "interleaving model barely explored: {states}");
    }
}
