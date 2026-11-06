//! Linux policy over neutral Gate Tasks; no native clone/futex adapter is used.
use super::{
    Native, Result, native_error,
    signal::{Actions, Completion, Signals},
};
use prima_runtime::{Capability, gate::GateRequest};

pub const MAX_THREADS: usize = 8;

#[derive(Clone, Copy)]
pub struct Thread {
    pub cookie: u64,
    pub manager: Capability,
    pub context: Capability,
    pub tls: u64,
    pub clear_tid: u64,
    signals: Signals,
}
#[derive(Clone, Copy)]
struct Waiter {
    request: GateRequest,
    address: u64,
    bits: u32,
    private: bool,
}
#[derive(Default)]
pub struct ThreadGroup {
    tasks: [Option<Thread>; MAX_THREADS],
    waiters: [Option<Waiter>; MAX_THREADS],
    leader_exit: Option<GateRequest>,
    actions: Actions,
    process_signals: Signals,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Call {
    Value(i64),
    Pending,
    Terminated(u8),
}

fn clone_options(flags: u64, stack: u64) -> Result<()> {
    const REQUIRED: u64 = 0x100 | 0x200 | 0x400 | 0x800 | 0x10000 | 0x40000;
    const OPTIONAL: u64 = 0x80000 | 0x100000 | 0x200000 | 0x400000 | 0x1000000;
    if flags & (0x100 | 0x10000) != (0x100 | 0x10000) {
        return Err(-38);
    }
    if flags & !(REQUIRED | OPTIONAL) != 0 {
        return Err(-38);
    }
    if flags & REQUIRED != REQUIRED || stack == 0 || stack & 7 != 0 || stack >= 1 << 47 {
        return Err(-22);
    }
    Ok(())
}

impl ThreadGroup {
    pub fn fork_child(&self, parent: u64, manager: Capability, cookie: u64) -> Self {
        let source = self.member(parent).expect("authenticated fork source");
        let mut child = Self {
            actions: self.actions,
            ..Self::default()
        };
        child.register_leader(manager, manager, cookie);
        child.tasks[0].as_mut().unwrap().tls = source.tls;
        child.tasks[0].as_mut().unwrap().signals = source.signals.forked();
        child
    }
    pub fn exec_reset(&mut self) {
        let leader = self.tasks[0].as_mut().expect("single-thread exec");
        leader.tls = 0;
        leader.clear_tid = 0;
        leader.signals.exec_reset();
        self.actions.exec_reset();
    }
    pub fn register_leader(&mut self, manager: Capability, context: Capability, cookie: u64) {
        self.tasks[0] = Some(Thread {
            manager,
            context,
            cookie,
            tls: 0,
            clear_tid: 0,
            signals: Signals::default(),
        });
    }
    pub fn member(&self, cookie: u64) -> Option<Thread> {
        self.tasks
            .iter()
            .flatten()
            .find(|t| t.cookie == cookie)
            .copied()
    }
    pub fn member_mut(&mut self, cookie: u64) -> Option<&mut Thread> {
        self.tasks.iter_mut().flatten().find(|t| t.cookie == cookie)
    }
    pub fn is_leader(&self, cookie: u64) -> bool {
        self.tasks[0].is_some_and(|t| t.cookie == cookie)
    }
    pub fn is_call(number: u64) -> bool {
        matches!(number, 24 | 56 | 202 | 218)
    }

    pub fn call(&mut self, rt: &Native, req: &GateRequest, gate: Capability) -> Call {
        let result = self.operation(rt, req, gate).unwrap_or_else(Call::Value);
        // A signal queued while Ready must also interrupt the first actual park.
        if result == Call::Pending {
            match self.interrupt_signals(rt) {
                Ok(Some(signal)) => Call::Terminated(signal),
                Ok(None) => Call::Pending,
                Err(error) => panic!("signal interruption failed: {error}"),
            }
        } else {
            result
        }
    }
    fn operation(&mut self, rt: &Native, req: &GateRequest, gate: Capability) -> Result<Call> {
        let thread = self.member(req.task_cookie).ok_or(-3)?;
        let a = req.args;
        match req.number {
            24 => Ok(Call::Value(0)),
            218 => {
                self.member_mut(req.task_cookie).unwrap().clear_tid = a[0];
                Ok(Call::Value(req.task_cookie as u32 as i64))
            }
            56 => {
                clone_options(a[0], a[1])?;
                let slot = self.tasks.iter().position(Option::is_none).ok_or(-11)?;
                let fs = if a[0] & 0x80000 != 0 {
                    a[4]
                } else {
                    thread.tls
                };
                if fs >= 1 << 47 {
                    return Err(-1);
                }
                let mut frame = rt
                    .gate_read_frame(req.capability, thread.context)
                    .map_err(native_error)?;
                frame.rax = 0;
                frame.rsp = a[1];
                let outputs = [
                    if a[0] & 0x100000 != 0 { a[2] } else { 0 },
                    if a[0] & 0x1000000 != 0 { a[3] } else { 0 },
                ];
                let manager = rt
                    .gate_spawn_thread(req.capability, thread.context, gate, frame, fs, outputs)
                    .map_err(native_error)?;
                let cookie = rt.send_target_cookie(manager).expect("spawned Task cookie");
                let context = manager; // spawn explicitly grants independent TaskContext
                self.tasks[slot] = Some(Thread {
                    manager,
                    context,
                    cookie,
                    tls: fs,
                    signals: thread.signals.inherited(),
                    clear_tid: if a[0] & 0x200000 != 0 { a[3] } else { 0 },
                });
                Ok(Call::Value(cookie as u32 as i64))
            }
            202 => {
                if a[0] & 3 != 0 {
                    return Err(-22);
                }
                let op = a[1] & !(128 | 256);
                if a[1] & 256 != 0 && op != 9 {
                    return Err(-38);
                }
                if ![0, 1, 9, 10].contains(&op) {
                    return Err(-38);
                }
                let bits = if op >= 9 { a[5] as u32 } else { u32::MAX };
                if bits == 0 {
                    return Err(-22);
                }
                // Full request/root validation applies even to wake; no cross-Process key aliasing.
                let mut bytes = [0; 4];
                rt.gate_copy_in(req.capability, a[0], &mut bytes)
                    .map_err(native_error)?;
                if op == 1 || op == 10 {
                    if (a[2] as u32 as i32) < 0 {
                        return Err(-22);
                    }
                    return Ok(Call::Value(
                        self.wake(rt, a[0], bits, a[2] as u32, a[1] & 128 != 0)? as i64,
                    ));
                }
                if u32::from_le_bytes(bytes) != a[2] as u32 {
                    return Err(-11);
                }
                if a[3] != 0 {
                    let mut time = [0; 16];
                    rt.gate_copy_in(req.capability, a[3], &mut time)
                        .map_err(native_error)?;
                    let secs = i64::from_le_bytes(time[..8].try_into().unwrap());
                    let nsecs = i64::from_le_bytes(time[8..].try_into().unwrap());
                    if secs < 0 || !(0..1_000_000_000).contains(&nsecs) {
                        return Err(-22);
                    }
                    return Err(if secs == 0 && nsecs == 0 { -110 } else { -38 });
                }
                let slot = self.waiters.iter().position(Option::is_none).ok_or(-12)?;
                self.waiters[slot] = Some(Waiter {
                    request: *req,
                    address: a[0],
                    bits,
                    private: a[1] & 128 != 0,
                });
                Ok(Call::Pending)
            }
            _ => unreachable!(),
        }
    }

    /// Group signals remain queued until a live member unmasks them.
    fn route_signals(&mut self) {
        for signal in 1..=31 {
            if let Some((code, sender)) = self.process_signals.queued(signal)
                && let Some(thread) = self
                    .tasks
                    .iter_mut()
                    .flatten()
                    .find(|t| !t.signals.blocked(signal))
            {
                thread.signals.enqueue(&self.actions, signal, code, sender);
                self.process_signals.dequeue(signal);
            }
        }
    }

    pub fn send_signal(&mut self, req: &GateRequest) -> i64 {
        let leader = self.tasks[0].expect("registered leader").cookie as u32 as u64;
        let a = req.args;
        let (target, signal, code) = if req.number == 62 {
            if a[0] != leader && a[0] != 0 {
                return -3;
            }
            (None, a[1], 0)
        } else {
            if a[0] != leader {
                return -3;
            }
            let Some(index) = self
                .tasks
                .iter()
                .position(|t| t.is_some_and(|t| t.cookie as u32 as u64 == a[1]))
            else {
                return -3;
            };
            (Some(index), a[2], -6)
        };
        if signal > 64 {
            return -22;
        }
        if signal == 0 {
            return 0;
        }
        if signal > 31 || [19, 20, 21, 22].contains(&signal) {
            return -38;
        }
        if let Some(index) = target {
            self.tasks[index].as_mut().unwrap().signals.enqueue(
                &self.actions,
                signal as u8,
                code,
                leader as u32,
            );
        } else {
            self.process_signals
                .enqueue(&self.actions, signal as u8, code, leader as u32);
        }
        self.route_signals();
        0
    }

    pub fn signal_service(
        &mut self,
        rt: &Native,
        req: &GateRequest,
        ordinary: Option<i64>,
    ) -> Result<Completion> {
        self.route_signals();
        let index = self
            .tasks
            .iter()
            .position(|t| t.is_some_and(|t| t.cookie == req.task_cookie))
            .ok_or(-3)?;
        let thread = self.tasks[index].as_mut().unwrap();
        let result = thread.signals.service(
            rt,
            req,
            thread.context,
            &mut self.actions,
            &mut self.process_signals,
            ordinary,
        )?;
        // Dispositions are shared: SIG_IGN also removes signals already queued
        // for other members, so a later handler cannot resurrect old pending.
        self.process_signals.discard_ignored(&self.actions);
        for thread in self.tasks.iter_mut().flatten() {
            thread.signals.discard_ignored(&self.actions);
        }
        self.route_signals();
        Ok(result)
    }

    /// Only recognized, unmasked signals can remove a genuine parked wait.
    pub fn interrupt_signals(&mut self, rt: &Native) -> Result<Option<u8>> {
        self.route_signals();
        for index in 0..self.tasks.len() {
            let Some(thread) = self.tasks[index] else {
                continue;
            };
            if !thread.signals.deliverable(&self.actions) {
                continue;
            }
            let wait = self
                .waiters
                .iter()
                .position(|w| w.is_some_and(|w| w.request.task_cookie == thread.cookie));
            let req = wait
                .and_then(|i| self.waiters[i].map(|w| w.request))
                .or(thread.signals.suspended());
            if let Some(req) = req {
                let result = self.signal_service(rt, &req, Some(-4))?;
                if let Completion::Terminated(signal) = result {
                    return Ok(Some(signal));
                }
                if let Some(i) = wait {
                    self.waiters[i] = None;
                }
            }
        }
        Ok(None)
    }

    fn wake(
        &mut self,
        rt: &Native,
        address: u64,
        bits: u32,
        count: u32,
        private: bool,
    ) -> Result<u32> {
        let mut woken = 0;
        for slot in &mut self.waiters {
            if woken == count {
                break;
            }
            if let Some(waiter) = *slot
                && waiter.address == address
                && waiter.bits & bits != 0
                && waiter.private == private
            {
                rt.gate_reply(waiter.request.capability, 0)
                    .map_err(native_error)?;
                *slot = None;
                woken += 1;
            }
        }
        Ok(woken)
    }

    fn clear_tid(&mut self, rt: &Native, req: &GateRequest, thread: Thread) -> Result<()> {
        if thread.clear_tid != 0
            && rt
                .gate_copy_out(req.capability, thread.clear_tid, &[0; 4])
                .is_ok()
        {
            self.wake(rt, thread.clear_tid, u32::MAX, 1, false)?;
        }
        Ok(())
    }

    /// Nonleader exit is a group-local teardown; a leader exiting first remains
    /// parked until the last sibling has exited. Returns a final group status.
    pub fn exit_thread(&mut self, rt: &Native, req: &GateRequest) -> Result<Option<u64>> {
        let thread = self.member(req.task_cookie).ok_or(-3)?;
        self.clear_tid(rt, req, thread)?;
        if self.is_leader(req.task_cookie) {
            if self.tasks[1..].iter().any(Option::is_some) {
                self.leader_exit = Some(*req);
                return Ok(None);
            }
            return Ok(Some(req.args[0] & 255));
        }
        self.remove_thread(rt, req.task_cookie)?;
        if self.tasks[1..].iter().all(Option::is_none) {
            return Ok(self.leader_exit.map(|leader| leader.args[0] & 255));
        }
        Ok(None)
    }

    fn remove_thread(&mut self, rt: &Native, cookie: u64) -> Result<()> {
        let index = self
            .tasks
            .iter()
            .position(|t| t.is_some_and(|t| t.cookie == cookie))
            .ok_or(-3)?;
        let thread = self.tasks[index].unwrap();
        if thread.context != thread.manager {
            rt.cap_drop(thread.context).map_err(native_error)?;
        }
        rt.task_destroy(thread.manager).map_err(native_error)?;
        rt.cap_drop(thread.manager).map_err(native_error)?;
        self.tasks[index] = None;
        for waiter in &mut self.waiters {
            if waiter.is_some_and(|w| w.request.task_cookie == cookie) {
                *waiter = None;
            }
        }
        Ok(())
    }

    pub fn destroy_all(&mut self, rt: &Native, req: &GateRequest) -> Result<()> {
        // Clear every user exit word before destroying the request's root/token.
        let authority = self.leader_exit.unwrap_or(*req);
        let threads = self.tasks;
        for thread in threads.into_iter().flatten() {
            self.clear_tid(rt, &authority, thread)?;
        }
        for index in (0..self.tasks.len()).rev() {
            if let Some(thread) = self.tasks[index] {
                self.remove_thread(rt, thread.cookie)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clone_profile_rejects_nonthread_unknown_flags_and_bad_stacks() {
        let flags = 0x50f00;
        assert_eq!(clone_options(flags, 0x70000000), Ok(()));
        assert_eq!(clone_options(flags | 0x380000, 0x70000000), Ok(()));
        assert_eq!(clone_options(17, 0), Err(-38));
        assert_eq!(clone_options(flags | (1 << 63), 0x70000000), Err(-38));
        assert_eq!(clone_options(flags & !0x800, 0x70000000), Err(-22));
        for stack in [0, 0x70000003, 1 << 47] {
            assert_eq!(clone_options(flags, stack), Err(-22));
        }
    }
}
