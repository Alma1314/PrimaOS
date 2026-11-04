//! Bounded Linux signal policy. The kernel sees only opaque Gate contexts.
use super::{Native, Result, native_error};
use prima_runtime::{
    Capability,
    gate::{GateContext, GateFrame, GateRequest},
};

const RESTORER: u64 = 0x0400_0000;
const SIGINFO: u64 = 4;
const NODEFER: u64 = 0x4000_0000;
const RESETHAND: u64 = 0x8000_0000;
const RESTART: u64 = 0x1000_0000;
const FLAGS: u64 = RESTORER | SIGINFO | NODEFER | RESETHAND | RESTART | 0x0800_0000; // ONSTACK, altstack disabled
const UNBLOCKABLE: u64 = (1 << 8) | (1 << 18);
const SC: usize = 48;
const MASK: usize = 304;
const INFO: usize = 312;
const FP: usize = 456;
const FRAME_LEN: usize = FP + 512;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Action {
    handler: u64,
    flags: u64,
    restorer: u64,
    mask: u64,
}

#[derive(Clone, Copy)]
pub struct Actions([Action; 64]);
impl Default for Actions {
    fn default() -> Self {
        Self([Action::default(); 64])
    }
}

#[derive(Clone, Copy, Default)]
pub struct Signals {
    mask: u64,
    pending: u64,
    codes: [i32; 32],
    senders: [u32; 32],
    frames: [u64; 4],
    depth: usize,
    suspended: Option<(GateRequest, u64)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Replied(i64),
    Suspended,
    Terminated(u8),
}

fn put(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn word(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().unwrap())
}

// Linux's sigcontext order differs from the native SretFrame order.
fn encode_context(bytes: &mut [u8; FRAME_LEN], state: &GateContext, mask: u64, base: u64) {
    let f = state.frame;
    put(bytes, 8, 6); // UC_SIGCONTEXT_SS | UC_STRICT_RESTORE_SS, legacy FXSAVE
    put(bytes, 32, 2); // uc_stack.ss_flags = SS_DISABLE
    for (i, value) in [
        f.r8, f.r9, f.r10, f.r11, f.r12, f.r13, f.r14, f.r15, f.rdi, f.rsi, f.rbp, f.rbx, f.rdx,
        f.rax, f.rcx, f.rsp, f.rip, f.rflags,
    ]
    .into_iter()
    .enumerate()
    {
        put(bytes, SC + i * 8, value);
    }
    put(bytes, SC + 144, f.cs | (f.ss << 48));
    put(bytes, SC + 168, mask);
    put(bytes, SC + 184, base + FP as u64);
    put(bytes, MASK, mask);
    bytes[FP..].copy_from_slice(&state.fpu);
    bytes[FP + 464..].fill(0); // legacy fpstate: no XSAVE magic or extensions
}

fn decode_context(bytes: &[u8; FRAME_LEN]) -> GateContext {
    let g = |i: usize| word(bytes, SC + i * 8);
    let selectors = g(18);
    GateContext {
        frame: GateFrame {
            r8: g(0),
            r9: g(1),
            r10: g(2),
            r11: g(3),
            r12: g(4),
            r13: g(5),
            r14: g(6),
            r15: g(7),
            rdi: g(8),
            rsi: g(9),
            rbp: g(10),
            rbx: g(11),
            rdx: g(12),
            rax: g(13),
            rcx: g(14),
            rsp: g(15),
            rip: g(16),
            rflags: g(17),
            cs: selectors & 0xffff,
            ss: selectors >> 48,
        },
        fpu: bytes[FP..].try_into().unwrap(),
    }
}

impl Signals {
    pub fn exec_reset(&mut self) {
        self.depth = 0;
        self.frames.fill(0);
        self.suspended = None;
    }
    pub fn is_call(number: u64) -> bool {
        matches!(number, 13 | 14 | 15 | 62 | 130 | 234)
    }

    fn operation(&mut self, actions: &mut Actions, rt: &Native, req: &GateRequest) -> Result<i64> {
        let a = req.args;
        match req.number {
            13 => {
                if a[3] != 8 || a[0] == 0 || a[0] > 64 || [9, 19].contains(&a[0]) {
                    return Err(-22);
                }
                let index = a[0] as usize - 1;
                let mut next = None;
                if a[1] != 0 {
                    let mut bytes = [0; 32];
                    rt.gate_copy_in(req.capability, a[1], &mut bytes)
                        .map_err(native_error)?;
                    let action = Action {
                        handler: word(&bytes, 0),
                        flags: word(&bytes, 8),
                        restorer: word(&bytes, 16),
                        mask: word(&bytes, 24) & !UNBLOCKABLE,
                    };
                    if action.flags & !FLAGS != 0 {
                        return Err(-38);
                    }
                    next = Some(action);
                }
                if a[2] != 0 {
                    let old = actions.0[index];
                    let mut bytes = [0; 32];
                    for (i, value) in [old.handler, old.flags, old.restorer, old.mask]
                        .into_iter()
                        .enumerate()
                    {
                        put(&mut bytes, i * 8, value);
                    }
                    rt.gate_copy_out(req.capability, a[2], &bytes)
                        .map_err(native_error)?;
                }
                if let Some(action) = next {
                    actions.0[index] = action;
                    if action.handler == 1 {
                        self.pending &= !(1u64 << index);
                    }
                }
                Ok(0)
            }
            14 => {
                if a[3] != 8 {
                    return Err(-22);
                }
                let mut next = self.mask;
                if a[1] != 0 {
                    let mut bytes = [0; 8];
                    rt.gate_copy_in(req.capability, a[1], &mut bytes)
                        .map_err(native_error)?;
                    let set = u64::from_le_bytes(bytes) & !UNBLOCKABLE;
                    next = match a[0] {
                        0 => self.mask | set,
                        1 => self.mask & !set,
                        2 => set,
                        _ => return Err(-22),
                    };
                }
                if a[2] != 0 {
                    rt.gate_copy_out(req.capability, a[2], &self.mask.to_le_bytes())
                        .map_err(native_error)?;
                }
                self.mask = next;
                Ok(0)
            }
            _ => unreachable!(),
        }
    }

    pub fn inherited(&self) -> Self {
        Self {
            mask: self.mask,
            ..Self::default()
        }
    }
    pub fn forked(&self) -> Self {
        Self {
            mask: self.mask,
            frames: self.frames,
            depth: self.depth,
            ..Self::default()
        }
    }
    pub fn blocked(&self, signal: u8) -> bool {
        self.mask & (1 << (signal - 1)) != 0
    }
    pub fn enqueue(&mut self, actions: &Actions, signal: u8, code: i32, sender: u32) {
        let i = signal as usize - 1;
        if actions.0[i].handler == 1 {
            return;
        }
        if self.pending & (1 << i) == 0 {
            self.codes[i] = code;
            self.senders[i] = sender;
        }
        self.pending |= 1 << i;
    }
    pub fn queued(&self, signal: u8) -> Option<(i32, u32)> {
        let i = signal as usize - 1;
        (self.pending & (1 << i) != 0).then_some((self.codes[i], self.senders[i]))
    }
    pub fn dequeue(&mut self, signal: u8) {
        self.pending &= !(1 << (signal - 1));
    }
    pub fn deliverable(&self, actions: &Actions) -> bool {
        (1..=31).any(|signal| {
            self.queued(signal).is_some()
                && !self.blocked(signal)
                && actions.0[signal as usize - 1].handler != 1
                && !(actions.0[signal as usize - 1].handler == 0
                    && [17, 18, 23, 28].contains(&signal))
        })
    }
    pub fn suspended(&self) -> Option<GateRequest> {
        self.suspended.map(|(req, _)| req)
    }

    /// Complete a Linux call, or retain a real Delivered sigsuspend request.
    /// State commits follow the successful native reply; no native signal ABI
    /// is used for the client's Linux signal semantics.
    pub fn service(
        &mut self,
        rt: &Native,
        req: &GateRequest,
        context: Capability,
        actions: &mut Actions,
        group_pending: &mut Signals,
        ordinary: Option<i64>,
    ) -> Result<Completion> {
        let mut next = *self;
        let mut next_actions = *actions;
        let mut pending = *group_pending;
        let mut restore_mask = self
            .suspended
            .filter(|(r, _)| r.capability == req.capability)
            .map(|(_, old)| old);
        let value = if let Some(value) = ordinary {
            value
        } else if req.number == 15 {
            return self.sigreturn(rt, req, context);
        } else if req.number == 130 {
            if req.args[1] != 8 {
                -22
            } else {
                let mut bytes = [0; 8];
                if let Err(error) = rt.gate_copy_in(req.capability, req.args[0], &mut bytes) {
                    native_error(error)
                } else {
                    restore_mask = Some(self.mask);
                    next.mask = u64::from_le_bytes(bytes) & !UNBLOCKABLE;
                    -4 // only returned after a real pending signal handler
                }
            }
        } else {
            next.operation(&mut next_actions, rt, req)
                .unwrap_or_else(|error| error)
        };
        for signal in 1..=31 {
            if !next.blocked(signal)
                && let Some((code, sender)) = pending.queued(signal)
            {
                next.enqueue(&next_actions, signal, code, sender);
                pending.dequeue(signal);
            }
        }
        loop {
            let available = next.pending & !next.mask;
            if available == 0 {
                break;
            }
            let index = available.trailing_zeros() as usize;
            let signal = index as u8 + 1;
            let action = next_actions.0[index];
            next.pending &= !(1 << index);
            if action.handler == 1 || (action.handler == 0 && [17, 18, 23, 28].contains(&signal)) {
                continue;
            }
            if action.handler == 0 {
                return Ok(Completion::Terminated(signal));
            }
            if next.depth == next.frames.len() || action.flags & RESTORER == 0 {
                return Ok(Completion::Terminated(11));
            }
            let mut state = rt
                .gate_read_context(req.capability, context)
                .map_err(native_error)?;
            state.frame.rax = value as u64;
            if req.number == 202 && value == -4 && action.flags & RESTART != 0 {
                let Some(pc) = state.frame.rip.checked_sub(2) else {
                    return Ok(Completion::Terminated(11));
                };
                let mut opcode = [0; 2];
                if rt.gate_copy_in(req.capability, pc, &mut opcode).is_err()
                    || !matches!(opcode, [0x0f, 0x05] | [0xcd, 0x80])
                {
                    return Ok(Completion::Terminated(11));
                }
                state.frame.rip = pc;
                state.frame.rax = req.number;
            }
            let Some(base) = state
                .frame
                .rsp
                .checked_sub(128 + FRAME_LEN as u64 + 64)
                .and_then(|v| (v & !63).checked_sub(8))
            else {
                return Ok(Completion::Terminated(11));
            };
            let mut bytes = [0; FRAME_LEN];
            encode_context(&mut bytes, &state, restore_mask.unwrap_or(next.mask), base);
            put(&mut bytes, 0, action.restorer);
            bytes[INFO..INFO + 4].copy_from_slice(&u32::from(signal).to_le_bytes());
            bytes[INFO + 8..INFO + 12].copy_from_slice(&next.codes[index].to_le_bytes());
            bytes[INFO + 16..INFO + 20].copy_from_slice(&next.senders[index].to_le_bytes());
            if rt.gate_copy_out(req.capability, base, &bytes).is_err() {
                return Ok(Completion::Terminated(11));
            }
            state.frame.rip = action.handler;
            state.frame.rsp = base;
            state.frame.rax = 0;
            state.frame.rdi = u64::from(signal);
            state.frame.rsi = base + INFO as u64;
            state.frame.rdx = base + 8;
            state.frame.rflags &= !(1 << 10); // DF is clear in the handler, preserved in ucontext.
            if rt
                .gate_reply_context(req.capability, context, &state)
                .is_err()
            {
                return Ok(Completion::Terminated(11));
            }
            next.frames[next.depth] = base;
            next.depth += 1;
            next.mask |= action.mask;
            if action.flags & NODEFER == 0 {
                next.mask |= 1 << index;
            }
            next.mask &= !UNBLOCKABLE;
            if action.flags & RESETHAND != 0 {
                next_actions.0[index] = Action::default();
            }
            next.suspended = None;
            *self = next;
            *actions = next_actions;
            *group_pending = pending;
            return Ok(Completion::Replied(value));
        }
        if let Some(old) = restore_mask {
            next.suspended = Some((*req, old));
            *self = next;
            *group_pending = pending;
            return Ok(Completion::Suspended);
        }
        if value == -38 {
            rt.gate_abort(req.capability, value as u64)
        } else {
            rt.gate_reply(req.capability, value as u64)
        }
        .map_err(native_error)?;
        *self = next;
        *actions = next_actions;
        *group_pending = pending;
        Ok(Completion::Replied(value))
    }

    fn sigreturn(
        &mut self,
        rt: &Native,
        req: &GateRequest,
        context: Capability,
    ) -> Result<Completion> {
        let current = rt
            .gate_read_frame(req.capability, context)
            .map_err(native_error)?;
        let Some(base) = current.rsp.checked_sub(8) else {
            return Ok(Completion::Terminated(11));
        };
        if self.depth == 0 || self.frames[self.depth - 1] != base {
            return Ok(Completion::Terminated(11));
        }
        let mut bytes = [0; FRAME_LEN];
        if rt.gate_copy_in(req.capability, base, &mut bytes).is_err() {
            return Ok(Completion::Terminated(11));
        }
        let mut state = decode_context(&bytes);
        let fp = word(&bytes, SC + 184);
        if fp != 0 {
            if rt.gate_copy_in(req.capability, fp, &mut state.fpu).is_err() {
                return Ok(Completion::Terminated(11));
            }
            if word(&state.fpu, 464) != 0 {
                return Ok(Completion::Terminated(11));
            }
        } else {
            // Linux's null fpstate clears the legacy floating-point state.
            state.fpu = [0; 512];
            state.fpu[..2].copy_from_slice(&0x37fu16.to_le_bytes());
            state.fpu[24..28].copy_from_slice(&0x1f80u32.to_le_bytes());
        }
        if rt
            .gate_reply_context(req.capability, context, &state)
            .is_err()
        {
            return Ok(Completion::Terminated(11));
        }
        self.mask = word(&bytes, MASK) & !UNBLOCKABLE;
        self.depth -= 1;
        Ok(Completion::Replied(state.frame.rax as i64))
    }
}

impl Actions {
    pub fn exec_reset(&mut self) {
        for action in &mut self.0 {
            if action.handler != 1 {
                *action = Action::default();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn thread_masks_pending_and_fork_frames_have_distinct_inheritance() {
        let mut actions = Actions::default();
        actions.0[9] = Action {
            handler: 0x401000,
            ..Action::default()
        };
        let mut parent = Signals {
            mask: 1 << 9,
            depth: 1,
            ..Signals::default()
        };
        parent.frames[0] = 0x70000000;
        parent.enqueue(&actions, 10, -6, 17);
        parent.enqueue(&actions, 10, 0, 18); // ordinary signals coalesce, first source wins
        assert_eq!(parent.queued(10), Some((-6, 17)));
        assert!(!parent.deliverable(&actions));
        let thread = parent.inherited();
        let fork = parent.forked();
        assert!(thread.blocked(10));
        assert_eq!((thread.pending, thread.depth), (0, 0));
        assert_eq!(
            (fork.pending, fork.depth, fork.frames[0]),
            (0, 1, 0x70000000)
        );
        parent.mask = 0;
        assert!(parent.deliverable(&actions));
        actions.0[9].handler = 1;
        assert!(!parent.deliverable(&actions));
        actions.exec_reset();
        assert_eq!(actions.0[9].handler, 1);
        parent.exec_reset();
        assert_eq!((parent.depth, parent.pending), (0, 1 << 9));
    }
    #[test]
    fn linux_signal_context_round_trip_preserves_registers_and_frame_offsets() {
        let frame = GateFrame {
            rax: (-4i64) as u64,
            rbx: 2,
            rcx: 3,
            rdx: 4,
            rsi: 5,
            rdi: 6,
            rbp: 7,
            rsp: 0x70000003,
            r8: 8,
            r9: 9,
            r10: 10,
            r11: 11,
            r12: 12,
            r13: 13,
            r14: 14,
            r15: 15,
            rflags: 0x602,
            rip: 0x401000,
            cs: 0x33,
            ss: 0x2b,
        };
        let mut state = GateContext {
            frame,
            fpu: [0; 512],
        };
        state.fpu[160..176].copy_from_slice(&[0xa5; 16]);
        let base = 0x6fff_fbf8;
        let mut bytes = [0; FRAME_LEN];
        encode_context(&mut bytes, &state, 0x200, base);
        assert_eq!(decode_context(&bytes), state);
        assert_eq!(word(&bytes, SC + 13 * 8), (-4i64) as u64);
        assert_eq!(word(&bytes, SC + 184), base + FP as u64);
        assert_eq!((base + FP as u64) % 64, 0);
        assert_eq!(word(&bytes, MASK), 0x200);
        assert_eq!(word(&bytes, 32), 2);
        put(&mut bytes, SC + 13 * 8, 123);
        assert_eq!(decode_context(&bytes).frame.rax, 123);
    }
}
