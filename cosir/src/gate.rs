//! S6-M1 executable design slice, not a frozen POSIR operation or kernel wiring.
//! Client words are opaque; only the service interprets their meaning. The host
//! owns State exclusively so private completion objects cannot escape via IPC.
use crate::authority::{signal_object, task_alert, waits_well_formed};
use crate::state::*;
use std::collections::{BTreeMap, VecDeque};

pub const MAX_BINDINGS: usize = 64;
pub const MAX_GATES: usize = 16;
pub const QUEUE_DEPTH: usize = 8;
pub const COPY_LIMIT: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Denied,
    Capacity,
    Busy,
    Stale,
    Fault,
    Overflow,
    AuditFull,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Gate(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Request(u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    WaitingCapacity,
    Queued,
    Delivered,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Attached(TaskId, Gate),
    Trapped(Request, TaskId, u64, [u64; 6]),
    Delivered(Request),
    Copied(Request, bool, u64, usize),
    Terminal(Request, TaskId, Option<u64>),
    Orphaned(Gate),
    OrphanTrap(TaskId, u64),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Call {
    id: Request,
    phase: Phase,
    number: u64,
    args: [u64; 6],
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    gate: Gate,
    orphan: u64,
    wait: WaitObjectId,
    call: Option<Call>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Endpoint {
    service: TaskId,
    live: bool,
    queue: VecDeque<TaskId>,
    capacity: VecDeque<TaskId>,
}

/// Single-owner host transaction model. `audit_limit` injects exhaustion: no
/// mutation is committed on failure. A kernel adapter must fail-stop scheduling
/// on AuditFull, never return this host error as an opaque client's syscall result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Model {
    state: State,
    endpoints: Vec<Endpoint>,
    bindings: BTreeMap<TaskId, Binding>,
    frames: BTreeMap<TaskId, SretFrame>,
    next: u64,
    events: Vec<Event>,
    audit_limit: usize,
    memory: crate::gate_memory::Memory,
}

impl Model {
    pub fn new(state: State, memory: crate::gate_memory::Memory, audit_limit: usize) -> Self {
        Self {
            state,
            endpoints: vec![],
            bindings: BTreeMap::new(),
            frames: BTreeMap::new(),
            next: 1,
            events: vec![],
            audit_limit,
            memory,
        }
    }

    pub fn memory(&self) -> &crate::gate_memory::Memory {
        &self.memory
    }

    pub fn copy(
        &mut self,
        service: TaskId,
        id: Request,
        address: u64,
        local: u64,
        len: usize,
        to_client: bool,
    ) -> Result<(), Error> {
        self.transaction(|m| {
            let task = m.request(service, id)?;
            let client_root = m.memory.0.get(&task).ok_or(Error::Fault)?.identity;
            let service_root = m.memory.0.get(&service).ok_or(Error::Fault)?.identity;
            if to_client {
                m.memory.copy(
                    service,
                    service_root,
                    local,
                    task,
                    client_root,
                    address,
                    len,
                )?;
            } else {
                m.memory.copy(
                    task,
                    client_root,
                    address,
                    service,
                    service_root,
                    local,
                    len,
                )?;
            }
            m.events.push(Event::Copied(id, to_client, address, len));
            Ok(())
        })
    }

    pub fn state(&self) -> &State {
        &self.state
    }
    pub fn events(&self) -> &[Event] {
        &self.events
    }
    pub fn frame(&self, task: TaskId) -> Option<SretFrame> {
        self.frames.get(&task).copied()
    }
    pub fn phase(&self, task: TaskId) -> Option<Phase> {
        self.bindings.get(&task)?.call.as_ref().map(|c| c.phase)
    }

    // ponytail: clone transactions are host-only; kernel uses preflight + commit.
    fn transaction<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let mut next = self.clone();
        let value = f(&mut next)?;
        if next.events.len() + next.state.audit.len() > next.audit_limit {
            return Err(Error::AuditFull);
        }
        assert!(next.well_formed());
        *self = next;
        Ok(value)
    }

    /// Caller becomes the sole service. The held TaskManageVSpace capability
    /// delegates the ENTIRE target user address space, not just argument ranges.
    pub fn attach(
        &mut self,
        service: TaskId,
        task_cap: CapId,
        gate: Option<Gate>,
        orphan: u64,
    ) -> Result<Gate, Error> {
        self.transaction(|m| {
            let cap = m.state.capabilities.get(&task_cap).ok_or(Error::Denied)?;
            if !cap.active
                || !m.state.held(service, task_cap)
                || cap.rights & CAP_TASK_MANAGE_VSPACE == 0
            {
                return Err(Error::Denied);
            }
            let CapObject::Task(task) = cap.object else {
                return Err(Error::Denied);
            };
            if task == service
                || m.bindings.contains_key(&service)
                || m.endpoints.iter().any(|e| e.live && e.service == task)
                || !m.state.tasks.contains_key(&service)
                || m.state
                    .tasks
                    .get(&task)
                    .is_none_or(|t| t.status != TaskStatus::Ready)
                || m.state.remote_tasks.contains_key(&task)
                || m.bindings.contains_key(&task)
            {
                return Err(Error::Busy);
            }
            if let Some(space) = m.memory.0.get(&task)
                && m.memory
                    .0
                    .iter()
                    .any(|(&other, s)| other != task && s.identity == space.identity)
            {
                return Err(Error::Busy);
            }
            if m.bindings.len() == MAX_BINDINGS {
                return Err(Error::Capacity);
            }
            let gate = match gate {
                Some(g) => {
                    m.service(service, g)?;
                    g
                }
                None => {
                    if m.endpoints.len() == MAX_GATES {
                        return Err(Error::Capacity);
                    }
                    let g = Gate(m.endpoints.len());
                    m.endpoints.push(Endpoint {
                        service,
                        live: true,
                        queue: VecDeque::new(),
                        capacity: VecDeque::new(),
                    });
                    g
                }
            };
            let wait =
                u32::try_from(m.state.next_wait_object.max(1)).map_err(|_| Error::Capacity)?;
            m.state.next_wait_object = u64::from(wait) + 1;
            m.state.wait_objects.insert(
                wait,
                WaitObject {
                    creator: service,
                    generation: 0,
                    waiters: Default::default(),
                },
            );
            m.bindings.insert(
                task,
                Binding {
                    gate,
                    orphan,
                    wait,
                    call: None,
                },
            );
            m.events.push(Event::Attached(task, gate));
            Ok(gate)
        })
    }

    fn service(&self, caller: TaskId, gate: Gate) -> Result<(), Error> {
        let e = self.endpoints.get(gate.0).ok_or(Error::Stale)?;
        if !e.live || e.service != caller || !self.state.tasks.contains_key(&caller) {
            return Err(Error::Denied);
        }
        Ok(())
    }

    pub fn trap(
        &mut self,
        task: TaskId,
        frame: SretFrame,
        number: u64,
        args: [u64; 6],
    ) -> Result<(), Error> {
        self.transaction(|m| {
            let b = m.bindings.get(&task).ok_or(Error::Denied)?;
            if b.call.is_some()
                || m.state
                    .tasks
                    .get(&task)
                    .is_none_or(|t| t.status != TaskStatus::Ready)
            {
                return Err(Error::Busy);
            }
            m.frames.insert(task, frame);
            if !m.endpoints[b.gate.0].live {
                m.frames.get_mut(&task).unwrap().rax = b.orphan;
                m.events.push(Event::OrphanTrap(task, b.orphan));
                return Ok(());
            }
            let id = Request(m.next);
            m.next = m.next.checked_add(1).ok_or(Error::Capacity)?;
            let endpoint = &mut m.endpoints[b.gate.0];
            let phase = if endpoint.queue.len() < QUEUE_DEPTH {
                endpoint.queue.push_back(task);
                Phase::Queued
            } else {
                endpoint.capacity.push_back(task);
                Phase::WaitingCapacity
            };
            let wait = b.wait;
            m.state
                .wait_objects
                .get_mut(&wait)
                .unwrap()
                .waiters
                .insert(task);
            m.state.wait_requests.insert(
                task,
                WaitRequest {
                    entries: vec![(wait, 0)],
                    mode: WaitMode::Any,
                    alertable: false,
                    deadline: None,
                },
            );
            m.state.tasks.get_mut(&task).unwrap().status = TaskStatus::BlockedWait;
            m.bindings.get_mut(&task).unwrap().call = Some(Call {
                id,
                phase,
                number,
                args,
            });
            m.events.push(Event::Trapped(id, task, number, args));
            Ok(())
        })
    }

    /// `output_ok` and `slot_ok` represent the adapter's copy-out/handle preflight.
    /// Failure leaves the head queued, with no request capability installed.
    pub fn wait(
        &mut self,
        service: TaskId,
        gate: Gate,
        output_ok: bool,
        slot_ok: bool,
    ) -> Result<Request, Error> {
        self.transaction(|m| {
            m.service(service, gate)?;
            if !output_ok {
                return Err(Error::Fault);
            }
            if !slot_ok {
                return Err(Error::Capacity);
            }
            let task = m.endpoints[gate.0].queue.pop_front().ok_or(Error::Busy)?;
            let c = m.bindings.get_mut(&task).unwrap().call.as_mut().unwrap();
            c.phase = Phase::Delivered;
            let id = c.id;
            m.promote(gate);
            m.events.push(Event::Delivered(id));
            Ok(id)
        })
    }

    fn promote(&mut self, gate: Gate) {
        let e = &mut self.endpoints[gate.0];
        if e.queue.len() < QUEUE_DEPTH
            && let Some(task) = e.capacity.pop_front()
        {
            e.queue.push_back(task);
            self.bindings
                .get_mut(&task)
                .unwrap()
                .call
                .as_mut()
                .unwrap()
                .phase = Phase::Queued;
        }
    }

    fn request(&self, service: TaskId, id: Request) -> Result<TaskId, Error> {
        let (&task, b) = self
            .bindings
            .iter()
            .find(|(_, b)| {
                b.call
                    .as_ref()
                    .is_some_and(|c| c.id == id && c.phase == Phase::Delivered)
            })
            .ok_or(Error::Stale)?;
        self.service(service, b.gate)?;
        Ok(task)
    }

    fn terminal(&mut self, task: TaskId, value: Option<u64>) {
        let b = self.bindings.get_mut(&task).unwrap();
        let c = b.call.take().unwrap();
        let gate = b.gate;
        self.state
            .wait_objects
            .get_mut(&b.wait)
            .unwrap()
            .waiters
            .remove(&task);
        self.state.wait_requests.remove(&task);
        if let Some(value) = value {
            self.frames.get_mut(&task).unwrap().rax = value;
            self.state.tasks.get_mut(&task).unwrap().status = TaskStatus::Ready;
        }
        self.endpoints[gate.0].queue.retain(|t| *t != task);
        self.endpoints[gate.0].capacity.retain(|t| *t != task);
        self.promote(gate);
        self.events.push(Event::Terminal(c.id, task, value));
    }

    /// abort is the same terminal mechanism with an opaque value; no errno table.
    pub fn reply(&mut self, service: TaskId, id: Request, value: u64) -> Result<(), Error> {
        self.transaction(|m| {
            let task = m.request(service, id)?;
            m.terminal(task, Some(value));
            Ok(())
        })
    }

    /// Gate waits are deliberately non-alertable in v1. No hidden native-number
    /// restart path and no signal cancellation handshake are needed before M4.
    pub fn alert(&mut self, from: TaskId, task_cap: CapId) -> Result<(), Error> {
        self.transaction(|m| {
            task_alert(&mut m.state, from, task_cap, &[]).map_err(|_| Error::Denied)
        })
    }

    /// Trusted task-lifecycle hook, also called when the last owner-held service
    /// capability disappears (moving it away counts as loss, not delegation).
    pub fn orphan(&mut self, gate: Gate) -> Result<(), Error> {
        self.transaction(|m| {
            if m.endpoints.get(gate.0).is_none_or(|e| !e.live) {
                return Err(Error::Stale);
            }
            m.endpoints[gate.0].live = false;
            let tasks: Vec<_> = m
                .bindings
                .iter()
                .filter_map(|(&t, b)| (b.gate == gate && b.call.is_some()).then_some(t))
                .collect();
            for task in tasks {
                m.terminal(task, Some(m.bindings[&task].orphan));
            }
            m.events.push(Event::Orphaned(gate));
            Ok(())
        })
    }

    pub fn destroy_client(&mut self, task: TaskId) -> Result<(), Error> {
        self.transaction(|m| {
            let b = m.bindings.get(&task).ok_or(Error::Stale)?.clone();
            if b.call.is_some() {
                m.terminal(task, None);
            }
            m.bindings.remove(&task);
            m.state.wait_objects.remove(&b.wait);
            m.frames.remove(&task);
            m.memory.0.remove(&task);
            m.state.tasks.remove(&task);
            m.state.pending_upcalls.remove(&task);
            Ok(())
        })
    }

    /// Negative check: no user authority is ever minted for completion waits.
    pub fn signal_private(&mut self, caller: TaskId, cap: CapId) -> Result<(), Error> {
        self.transaction(|m| {
            signal_object(&mut m.state, caller, cap)
                .map(|_| ())
                .map_err(|_| Error::Denied)
        })
    }

    pub fn well_formed(&self) -> bool {
        waits_well_formed(&self.state)
            && self.bindings.len() <= MAX_BINDINGS
            && self.endpoints.len() <= MAX_GATES
            && self.endpoints.iter().enumerate().all(|(i, e)| {
                e.queue.len() <= QUEUE_DEPTH
                    && e.capacity.len() <= MAX_BINDINGS
                    && e.queue.iter().chain(&e.capacity).all(|task| {
                        self.bindings
                            .get(task)
                            .is_some_and(|b| b.gate == Gate(i) && b.call.is_some())
                    })
            })
            && self.bindings.iter().all(|(&task, b)| {
                let e = &self.endpoints[b.gate.0];
                let queued = e.queue.iter().filter(|t| **t == task).count();
                let capacity = e.capacity.iter().filter(|t| **t == task).count();
                match &b.call {
                    None => queued + capacity == 0 && !self.state.wait_requests.contains_key(&task),
                    Some(c) => {
                        self.state.wait_requests.get(&task).is_some_and(|r| {
                            r.entries == [(b.wait, 0)] && !r.alertable && r.deadline.is_none()
                        }) && match c.phase {
                            Phase::Queued => queued == 1 && capacity == 0,
                            Phase::WaitingCapacity => queued == 0 && capacity == 1,
                            Phase::Delivered => queued + capacity == 0,
                        }
                    }
                }
            })
    }
}
