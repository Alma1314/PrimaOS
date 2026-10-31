//! Bounded user upcall state. Slots coalesce; this is not a realtime queue.
//! Dispositions belong to a Process, masks/pending/return stacks to each Task.
use crate::sched::MAX_TASKS;
use prima_ostd::{entry::SretFrame, sync::StaticCell};

pub const MAX_SIGNALS: usize = 64;
pub const MAX_NESTING: usize = 8;
pub const NO_DEFER: u64 = 1;
pub const RESET_ACTION: u64 = 2;
pub const RESTART: u64 = 4;
pub const IGNORE_DEFAULT: u64 = 8;
pub const WITH_INFO: u64 = 16;
/// The runtime resolves interrupted mailbox transactions before restarting.
pub const IPC_RESTART_IN_USER: u64 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub handler: u64,
    pub mask: u64,
    pub flags: u64,
    pub context: u64,
}
impl Action {
    pub const DEFAULT: Self = Self {
        handler: 0,
        mask: 0,
        flags: 0,
        context: 0,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    pub slot: u64,
    pub sender: u64,
    pub kind: u64,
    pub value: u64,
}
impl Info {
    pub const ZERO: Self = Self {
        slot: 0,
        sender: 0,
        kind: 0,
        value: 0,
    };
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Saved {
    pub frame: SretFrame,
    pub mask: u64,
    pub info: Info,
    pub token: u64,
}
#[derive(Clone, Copy)]
struct Thread {
    mask: u64,
    pending: u64,
    info: [Info; MAX_SIGNALS],
    saved: [Option<Saved>; MAX_NESTING],
    depth: usize,
    next_token: u64,
    suspended_mask: Option<u64>,
}
impl Thread {
    const EMPTY: Self = Self {
        mask: 0,
        pending: 0,
        info: [Info::ZERO; MAX_SIGNALS],
        saved: [None; MAX_NESTING],
        depth: 0,
        next_token: 1,
        suspended_mask: None,
    };
}
pub struct SignalState {
    owners: [usize; MAX_TASKS],
    actions: [[Action; MAX_SIGNALS]; MAX_TASKS],
    threads: [Thread; MAX_TASKS],
    routes: [[u32; 5]; MAX_TASKS],
}
impl Default for SignalState {
    fn default() -> Self {
        Self::new()
    }
}
impl SignalState {
    pub const fn new() -> Self {
        let mut owners = [0; MAX_TASKS];
        let mut index = 0;
        while index < MAX_TASKS {
            owners[index] = index;
            index += 1;
        }
        Self {
            owners,
            actions: [[Action::DEFAULT; MAX_SIGNALS]; MAX_TASKS],
            threads: [Thread::EMPTY; MAX_TASKS],
            routes: [[0; 5]; MAX_TASKS],
        }
    }
    fn slot(slot: u32) -> Option<usize> {
        (1..=MAX_SIGNALS as u32)
            .contains(&slot)
            .then_some(slot.wrapping_sub(1) as usize)
    }
    pub fn action(&self, task: u32, slot: u32) -> Option<Action> {
        Some(
            *self
                .actions
                .get(*self.owners.get(task as usize)?)?
                .get(Self::slot(slot)?)?,
        )
    }
    pub fn set_action(&mut self, task: u32, slot: u32, action: Action) -> Option<Action> {
        let owner = *self.owners.get(task as usize)?;
        let index = Self::slot(slot)?;
        let old = self.actions[owner][index];
        self.actions[owner][index] = action;
        if action.handler == 1 || action.handler == 0 && action.flags & IGNORE_DEFAULT != 0 {
            for id in 0..MAX_TASKS {
                if self.owners[id] == owner {
                    self.threads[id].pending &= !(1 << index);
                }
            }
        }
        Some(old)
    }
    pub fn set_mask(&mut self, task: u32, mask: u64) -> Option<u64> {
        let state = self.threads.get_mut(task as usize)?;
        let old = state.mask;
        state.mask = mask;
        Some(old)
    }
    pub fn mask(&self, task: u32) -> u64 {
        self.threads.get(task as usize).map_or(0, |t| t.mask)
    }
    pub fn pending(&self, task: u32) -> u64 {
        self.threads.get(task as usize).map_or(0, |t| t.pending)
    }
    fn has_return_context(&self, task: u32) -> bool {
        self.threads
            .get(task as usize)
            .is_some_and(|thread| thread.depth != 0 || thread.suspended_mask.is_some())
    }
    pub fn deliverable(&self, task: u32) -> u64 {
        self.threads
            .get(task as usize)
            .map_or(0, |t| t.pending & !t.mask)
    }
    pub fn queue(&mut self, task: u32, slot: u32, mut info: Info) -> bool {
        let Some(index) = Self::slot(slot) else {
            return false;
        };
        if self
            .action(task, slot)
            .is_none_or(|a| a.handler == 1 || a.handler == 0 && a.flags & IGNORE_DEFAULT != 0)
        {
            return false;
        }
        let Some(state) = self.threads.get_mut(task as usize) else {
            return false;
        };
        // Preserve the first coalesced notification's provenance.
        if state.pending & (1 << index) == 0 {
            info.slot = slot as u64;
            state.info[index] = info;
        }
        state.pending |= 1 << index;
        true
    }
    pub fn clear_pending(&mut self, task: u32, slot: u32) {
        if let (Some(state), Some(index)) = (self.threads.get_mut(task as usize), Self::slot(slot))
        {
            state.pending &= !(1 << index);
        }
    }
    pub fn enter(&mut self, task: u32, slot: u32, frame: SretFrame) -> Option<SretFrame> {
        let action = self.action(task, slot)?;
        let index = Self::slot(slot)?;
        let state = self.threads.get_mut(task as usize)?;
        if action.handler <= 1 || state.depth == MAX_NESTING || state.mask & (1 << index) != 0 {
            return None;
        }
        let top = frame.rsp.checked_sub(128)? & !15;
        let rsp = top.checked_sub(8)?;
        if rsp == 0 || rsp >> 47 != 0 {
            return None;
        }
        let token = state.next_token;
        state.next_token = token.checked_add(1)?;
        state.saved[state.depth] = Some(Saved {
            frame,
            mask: state.suspended_mask.take().unwrap_or(state.mask),
            info: state.info[index],
            token,
        });
        state.depth += 1;
        state.pending &= !(1 << index);
        state.mask |= action.mask
            | if action.flags & NO_DEFER == 0 {
                1 << index
            } else {
                0
            };
        if action.flags & RESET_ACTION != 0 {
            self.actions[self.owners[task as usize]][index] = Action {
                flags: action.flags & IGNORE_DEFAULT,
                ..Action::DEFAULT
            };
        }
        let mut handler = frame;
        handler.rax = 0;
        handler.rflags &= !((1 << 10) | (1 << 8)); // C ABI: DF/TF clear on handler entry.
        handler.rdi = slot as u64;
        handler.rsi = token;
        handler.rdx = action.context;
        handler.rcx = action.flags;
        handler.rip = action.handler;
        handler.rsp = rsp;
        Some(handler)
    }
    pub fn saved(&self, task: u32) -> Option<Saved> {
        let state = self.threads.get(task as usize)?;
        *state.saved.get(state.depth.checked_sub(1)?)?
    }
    pub fn leave(&mut self, task: u32, token: Option<u64>) -> Option<Saved> {
        let state = self.threads.get_mut(task as usize)?;
        let index = state.depth.checked_sub(1)?;
        let saved = state.saved[index]?;
        if token.is_some_and(|token| token != saved.token) {
            return None;
        }
        state.saved[index] = None;
        state.depth = index;
        state.mask = saved.mask;
        Some(saved)
    }
    pub fn bind_thread(&mut self, parent: u32, child: u32) -> bool {
        if parent == child || parent as usize >= MAX_TASKS || child as usize >= MAX_TASKS {
            return false;
        }
        self.clear(child);
        self.owners[child as usize] = self.owners[parent as usize];
        self.threads[child as usize].mask = self.threads[parent as usize].mask;
        true
    }
    pub fn inherit_process(&mut self, parent: u32, child: u32) -> bool {
        if parent == child || parent as usize >= MAX_TASKS || child as usize >= MAX_TASKS {
            return false;
        }
        self.clear(child);
        self.actions[child as usize] = self.actions[self.owners[parent as usize]];
        self.routes[child as usize] = self.routes[self.owners[parent as usize]];
        self.threads[child as usize] = self.threads[parent as usize];
        self.threads[child as usize].pending = 0;
        self.threads[child as usize].info = [Info::ZERO; MAX_SIGNALS];
        true
    }
    pub fn exec_reset(&mut self, task: u32) {
        if task as usize >= MAX_TASKS {
            return;
        }
        let owner = self.owners[task as usize];
        for action in &mut self.actions[owner] {
            if action.handler != 1 {
                *action = Action {
                    flags: action.flags & IGNORE_DEFAULT,
                    ..Action::DEFAULT
                };
            }
        }
        self.threads[task as usize].saved = [None; MAX_NESTING];
        self.threads[task as usize].depth = 0;
        self.threads[task as usize].suspended_mask = None;
    }
    pub fn clear(&mut self, task: u32) {
        let id = task as usize;
        if id >= MAX_TASKS {
            return;
        }
        self.actions[id] = [Action::DEFAULT; MAX_SIGNALS];
        self.threads[id] = Thread::EMPTY;
        self.owners[id] = id;
        self.routes[id] = [0; 5];
    }
}
static SAVED_FPUS: StaticCell<[[Option<crate::sched::FpuArea>; MAX_NESTING]; MAX_TASKS]> =
    StaticCell::new([[None; MAX_NESTING]; MAX_TASKS]);
static STATE: StaticCell<SignalState> = StaticCell::new(SignalState::new());
pub fn clear_task(task: u32) {
    STATE.with_mut(|s| s.clear(task));
    if (task as usize) < MAX_TASKS {
        SAVED_FPUS.with_mut(|rows| rows[task as usize] = [None; MAX_NESTING]);
    }
}
pub fn bind_thread(parent: u32, child: u32) {
    if STATE.with_mut(|s| s.bind_thread(parent, child)) {
        SAVED_FPUS.with_mut(|rows| rows[child as usize] = [None; MAX_NESTING]);
    }
}
pub fn inherit_process(parent: u32, child: u32) {
    if STATE.with_mut(|s| s.inherit_process(parent, child)) {
        SAVED_FPUS.with_mut(|rows| rows[child as usize] = rows[parent as usize]);
    }
}
pub fn exec_reset(task: u32) {
    STATE.with_mut(|s| s.exec_reset(task));
    if (task as usize) < MAX_TASKS {
        SAVED_FPUS.with_mut(|rows| rows[task as usize] = [None; MAX_NESTING]);
    }
}
pub fn set_handler(task: u32, slot: u32, handler: u64) {
    STATE.with_mut(|s| {
        s.set_action(
            task,
            slot,
            Action {
                handler,
                ..Action::DEFAULT
            },
        )
    });
}
pub fn handler_of(task: u32, slot: u32) -> u64 {
    STATE.with(|s| s.action(task, slot).map_or(0, |a| a.handler))
}
pub fn set_pending(task: u32, slot: u32) {
    STATE.with_mut(|s| s.queue(task, slot, Info::ZERO));
}
pub fn suspend_mask(task: u32, old: u64) {
    STATE.with_mut(|s| s.threads[task as usize].suspended_mask = Some(old));
}

/// Queued notifications are allowed; active native return contexts are not.
pub(crate) fn has_return_context(task: u32) -> bool {
    STATE.with(|s| s.has_return_context(task))
}
pub fn pending(task: u32) -> u64 {
    STATE.with(|s| s.deliverable(task))
}
pub fn clear_pending(task: u32, slot: u32) {
    STATE.with_mut(|s| s.clear_pending(task, slot));
}
pub const CONTEXT_BYTES: usize = 720;
pub fn set_route(task: u32, event: usize, slot: u32) -> bool {
    STATE.with_mut(|s| {
        if event >= 5 || slot > MAX_SIGNALS as u32 || task as usize >= MAX_TASKS {
            return false;
        }
        s.routes[s.owners[task as usize]][event] = slot;
        true
    })
}
pub fn route(task: u32, event: usize) -> u32 {
    STATE.with(|s| {
        if task as usize >= MAX_TASKS || event >= 5 {
            0
        } else {
            s.routes[s.owners[task as usize]][event]
        }
    })
}
pub fn action(task: u32, slot: u32) -> Option<Action> {
    STATE.with(|s| s.action(task, slot))
}
pub fn set_action(task: u32, slot: u32, action: Action) -> Option<Action> {
    STATE.with_mut(|s| s.set_action(task, slot, action))
}
pub fn mask(task: u32) -> u64 {
    STATE.with(|s| s.mask(task))
}
pub fn set_mask(task: u32, mask: u64) -> Option<u64> {
    STATE.with_mut(|s| s.set_mask(task, mask))
}
pub fn all_pending(task: u32) -> u64 {
    STATE.with(|s| s.pending(task))
}
pub fn queue_info(task: u32, slot: u32, info: Info) -> bool {
    STATE.with_mut(|s| s.queue(task, slot, info))
}

pub fn export_context(task: u32, token: u64) -> Option<[u8; CONTEXT_BYTES]> {
    let (saved, depth) =
        STATE.with(|s| Some((s.saved(task)?, s.threads[task as usize].depth - 1)))?;
    if saved.token != token {
        return None;
    }
    let fpu = SAVED_FPUS.with(|rows| rows[task as usize][depth])?;
    let mut bytes = [0; CONTEXT_BYTES];
    bytes[..160].copy_from_slice(&saved.frame.encode());
    let words = [
        saved.mask,
        saved.info.slot,
        saved.info.sender,
        saved.info.kind,
        saved.info.value,
        saved.token,
    ];
    for (index, word) in words.iter().enumerate() {
        bytes[160 + 8 * index..168 + 8 * index].copy_from_slice(&word.to_le_bytes());
    }
    bytes[208..].copy_from_slice(&fpu.bytes());
    Some(bytes)
}
/// An upcall may change its own user GPRs; selectors and privileged flags never
/// come from user input. In particular IOPL/VM/NT/IF cannot escape this boundary.
pub fn validate_user_return(mut frame: SretFrame) -> Option<SretFrame> {
    if frame.rip == 0 || frame.rsp == 0 || frame.rip >> 47 != 0 || frame.rsp >> 47 != 0 {
        return None;
    }
    frame.cs = u64::from(crate::gdt::USER_CS);
    frame.ss = u64::from(crate::gdt::USER_DS);
    frame.rflags = (frame.rflags & 0x24_0cd5) | 0x202;
    Some(frame)
}
pub fn restore_context(task: u32, token: u64, bytes: &[u8; CONTEXT_BYTES]) -> Option<SretFrame> {
    let word = |offset: usize| {
        u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("context word"))
    };
    if word(200) != token {
        return None;
    }
    let frame = validate_user_return(SretFrame::decode(bytes[..160].try_into().ok()?))?;
    let (top, depth) =
        STATE.with(|s| Some((s.saved(task)?, s.threads[task as usize].depth - 1)))?;
    if top.token != token {
        return None;
    }
    let original = SAVED_FPUS.with(|rows| rows[task as usize][depth])?;
    let fpu = crate::sched::FpuArea::from_user(bytes[208..].try_into().ok()?, original)?;
    STATE.with_mut(|s| {
        s.leave(task, Some(token))?;
        s.set_mask(task, word(160))
    })?;
    SAVED_FPUS.with_mut(|rows| rows[task as usize][depth] = None);
    crate::sched::set_saved_fpu(task, fpu);
    Some(frame)
}
pub fn deliver(task: u32, slot: u32, saved: SretFrame) -> Option<SretFrame> {
    let frame = STATE.with_mut(|s| s.enter(task, slot, saved))?;
    let depth = STATE.with(|s| s.threads[task as usize].depth - 1);
    let fpu = crate::sched::snapshot_current_fpu();
    SAVED_FPUS.with_mut(|rows| rows[task as usize][depth] = Some(fpu));
    Some(frame)
}
pub fn take_saved(task: u32) -> Option<SretFrame> {
    let (saved, depth) = STATE.with_mut(|s| {
        let saved = s.leave(task, None)?;
        Some((saved, s.threads[task as usize].depth))
    })?;
    if let Some(fpu) = SAVED_FPUS.with_mut(|rows| rows[task as usize][depth].take()) {
        crate::sched::set_saved_fpu(task, fpu);
    }
    Some(saved.frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn frame() -> SretFrame {
        SretFrame {
            rsp: 0x8000,
            rip: 0x1000,
            ..SretFrame::ZERO
        }
    }
    #[test]
    fn gate_admission_distinguishes_queued_signals_from_native_return_contexts() {
        let mut state = SignalState::new();
        state.set_action(
            1,
            10,
            Action {
                handler: 0x2000,
                ..Action::DEFAULT
            },
        );
        state.queue(1, 10, Info::ZERO);
        assert!(!state.has_return_context(1));
        state.threads[1].suspended_mask = Some(0);
        assert!(state.has_return_context(1));
        state.enter(1, 10, frame()).unwrap();
        assert!(state.has_return_context(1));
        assert!(state.threads[1].suspended_mask.is_none());
        let saved = state.saved(1).unwrap();
        assert!(state.has_return_context(1));
        assert_eq!(state.saved(1), Some(saved));
        state.leave(1, None).unwrap();
        assert!(!state.has_return_context(1));
    }
    #[test]
    fn user_return_cannot_install_kernel_selectors_privileged_flags_or_noncanonical_pc() {
        let mut input = frame();
        input.cs = 8;
        input.ss = 16;
        input.rflags = u64::MAX;
        let out = validate_user_return(input).unwrap();
        assert_eq!(out.cs, u64::from(crate::gdt::USER_CS));
        assert_eq!(out.ss, u64::from(crate::gdt::USER_DS));
        assert_eq!(out.rflags & ((3 << 12) | (1 << 14) | (1 << 17)), 0);
        assert_ne!(out.rflags & (1 << 9), 0);
        input.rip = 1 << 47;
        assert!(validate_user_return(input).is_none());
    }
    #[test]
    fn threads_share_actions_but_not_masks_and_fork_has_its_own_dispositions() {
        let mut s = SignalState::new();
        s.set_action(
            1,
            10,
            Action {
                handler: 0x2000,
                ..Action::DEFAULT
            },
        );
        s.set_mask(1, 1 << 9);
        assert!(s.bind_thread(1, 2));
        assert!(s.inherit_process(1, 3));
        s.set_mask(2, 0);
        s.set_action(
            2,
            10,
            Action {
                handler: 0x3000,
                ..Action::DEFAULT
            },
        );
        assert_eq!(s.action(1, 10).unwrap().handler, 0x3000);
        assert_eq!(s.action(3, 10).unwrap().handler, 0x2000);
        s.queue(1, 10, Info::ZERO);
        s.queue(2, 10, Info::ZERO);
        assert_eq!(s.deliverable(1), 0);
        assert_ne!(s.deliverable(2), 0);
        s.clear(2);
        assert_eq!(s.action(1, 10).unwrap().handler, 0x3000);
    }
    #[test]
    fn nested_returns_restore_masks_frames_and_red_zone_without_overwriting_saved_context() {
        let mut s = SignalState::new();
        for slot in [10, 12] {
            s.set_action(
                1,
                slot,
                Action {
                    handler: 0x2000,
                    ..Action::DEFAULT
                },
            );
        }
        s.queue(
            1,
            10,
            Info {
                sender: 7,
                ..Info::ZERO
            },
        );
        let first = s.enter(1, 10, frame()).unwrap();
        assert!(first.rsp + 128 < frame().rsp);
        assert_eq!(first.rsp & 15, 8);
        let original = s.saved(1).unwrap();
        s.queue(1, 10, Info::ZERO);
        assert!(s.enter(1, 10, first).is_none());
        s.queue(1, 12, Info::ZERO);
        let second = s.enter(1, 12, first).unwrap();
        let top = s.saved(1).unwrap();
        assert_ne!(top.token, original.token);
        assert!(s.leave(1, Some(original.token)).is_none());
        assert_eq!(s.saved(1), Some(top));
        assert_eq!(s.leave(1, Some(second.rsi)).unwrap().frame, first);
        assert_eq!(s.leave(1, Some(first.rsi)).unwrap().frame, frame());
        assert_ne!(s.deliverable(1), 0);
    }
    #[test]
    fn ignore_coalescing_reset_and_exec_have_explicit_lifecycle() {
        let mut s = SignalState::new();
        s.set_action(
            1,
            10,
            Action {
                handler: 0x2000,
                flags: RESET_ACTION,
                mask: 0,
                context: 0,
            },
        );
        s.queue(
            1,
            10,
            Info {
                sender: 3,
                ..Info::ZERO
            },
        );
        s.queue(
            1,
            10,
            Info {
                sender: 4,
                ..Info::ZERO
            },
        );
        s.enter(1, 10, frame()).unwrap();
        assert_eq!(s.saved(1).unwrap().info.sender, 3);
        assert_eq!(s.action(1, 10), Some(Action::DEFAULT));
        s.set_action(
            1,
            12,
            Action {
                handler: 1,
                ..Action::DEFAULT
            },
        );
        assert!(!s.queue(1, 12, Info::ZERO));
        s.exec_reset(1);
        assert!(s.saved(1).is_none());
        assert_eq!(s.action(1, 12).unwrap().handler, 1);
        s.clear(1);
        assert_eq!(s.pending(1), 0);
        assert_eq!(s.mask(1), 0);
    }
    #[test]
    fn suspend_delivery_restores_original_mask_instead_of_temporary_mask() {
        let mut state = SignalState::new();
        state.set_action(
            1,
            10,
            Action {
                handler: 0x4000,
                ..Action::DEFAULT
            },
        );
        state.set_mask(1, 1 << 9);
        let original = state.mask(1);
        state.set_mask(1, 0);
        state.threads[1].suspended_mask = Some(original);
        state.queue(1, 10, Info::ZERO);
        let frame = SretFrame {
            rip: 0x3000,
            rsp: 0x8000,
            ..SretFrame::ZERO
        };
        assert!(state.enter(1, 10, frame).is_some());
        assert_eq!(state.saved(1).unwrap().mask, original);
        state.leave(1, None).unwrap();
        assert_eq!(state.mask(1), original);
    }
}
