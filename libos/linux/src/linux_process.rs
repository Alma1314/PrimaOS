//! Bounded personality-owned ancestry, shared OFDs and wait/exec policy.
use super::{HEAP, MMAP, Native, Process, Result, Shared, native_error};
use crate::linux_store::{Session, Store};
use prima_runtime::{Capability, gate::GateRequest};

pub const MAX_PROCESSES: usize = 4;

struct Entry {
    cookie: u64,
    parent: Option<u64>,
    process: Option<Process>,
    status: Option<u32>,
    wait: Option<GateRequest>,
}
pub struct Family {
    entries: [Option<Entry>; MAX_PROCESSES],
    pub shared: Shared,
    /// One store session per Process slot: each has its own sequence and owner.
    sessions: [Session; MAX_PROCESSES],
}

impl Family {
    pub fn new(process: Process) -> Result<Self> {
        let cookie = process.cookie;
        let mut entries = [const { None }; MAX_PROCESSES];
        entries[0] = Some(Entry {
            cookie,
            parent: None,
            process: Some(process),
            status: None,
            wait: None,
        });
        Ok(Self {
            entries,
            shared: Shared::new(),
            sessions: core::array::from_fn(|_| Session::new()),
        })
    }
    pub fn owner(&self, cookie: u64) -> Option<usize> {
        self.entries.iter().position(|entry| {
            entry
                .as_ref()
                .and_then(|e| e.process.as_ref())
                .is_some_and(|p| p.threads.member(cookie).is_some())
        })
    }
    pub fn process(&mut self, owner: usize) -> &mut Process {
        self.entries[owner]
            .as_mut()
            .unwrap()
            .process
            .as_mut()
            .unwrap()
    }
    pub fn dispatch(
        &mut self,
        owner: usize,
        store: &mut dyn Store,
        rt: &Native,
        req: &GateRequest,
        write: impl FnMut(u64, &[u8]) -> Result<()>,
    ) -> i64 {
        let process = self.entries[owner]
            .as_mut()
            .unwrap()
            .process
            .as_mut()
            .unwrap();
        self.sessions[owner].set_owner(process.owner);
        process.dispatch(
            &mut self.shared,
            &mut self.sessions[owner],
            store,
            rt,
            req,
            write,
        )
    }
    pub fn parent(&self, owner: usize) -> i64 {
        let parent = self.entries[owner].as_ref().unwrap().parent;
        parent.map_or(1, |cookie| {
            self.entries
                .iter()
                .flatten()
                .find(|e| e.cookie == cookie && e.process.is_some())
                .map_or(1, |e| e.cookie as u32 as i64)
        })
    }
    pub fn fork(
        &mut self,
        rt: &Native,
        store: &mut dyn Store,
        req: &GateRequest,
        gate: Capability,
    ) -> i64 {
        self.fork_inner(rt, store, req, gate)
            .unwrap_or_else(|error| error)
    }
    fn fork_inner(
        &mut self,
        rt: &Native,
        store: &mut dyn Store,
        req: &GateRequest,
        gate: Capability,
    ) -> Result<i64> {
        let owner = self.owner(req.task_cookie).ok_or(-3)?;
        let slot = self.entries.iter().position(Option::is_none).ok_or(-11)?;
        let (parent_owner, context, thread_manager, tls) = {
            let parent = self.entries[owner]
                .as_ref()
                .unwrap()
                .process
                .as_ref()
                .unwrap();
            let thread = parent.threads.member(req.task_cookie).ok_or(-3)?;
            (parent.owner, thread.context, thread.manager, thread.tls)
        };
        // Reserve the child owner before kernel publication. A failed fork
        // releases it, so the window is either a complete reservation or none.
        self.sessions[owner].set_owner(parent_owner);
        let key = self.sessions[owner].reserve(store)?;
        let manager = match rt.gate_fork(req.capability, context, thread_manager, gate) {
            Ok(manager) => manager,
            Err(error) => {
                self.sessions[owner].release(store, key)?;
                return Err(native_error(error));
            }
        };
        let cookie = rt
            .send_target_cookie(manager)
            .expect("published child cookie");
        let child_owner = match self.sessions[owner].publish(store, key, cookie, manager) {
            Ok(child_owner) => child_owner,
            Err(error) => {
                // The kernel child already exists; Local cannot fail here, and
                // the real transport must keep publish infallible after reserve.
                let _ = self.sessions[owner].release(store, key);
                return Err(error);
            }
        };
        // A real proxy session needs its own entry for the child Process. The
        // in-process store already keys batches by owner and treats this as a
        // no-op. The kernel child exists, so a failure here is terminal; the
        // window is recorded in the P2b-3/P2b-4 reports.
        store.register_child(child_owner, manager)?;
        let parent = self.entries[owner]
            .as_ref()
            .unwrap()
            .process
            .as_ref()
            .unwrap();
        let child = Process {
            manager,
            cookie,
            tls,
            threads: parent.threads.fork_child(req.task_cookie, manager, cookie),
            clear_tid: 0,
            owner: child_owner,
            fds: parent.fds,
            dirs: parent.dirs,
            fd_flags: parent.fd_flags,
            standard_open: parent.standard_open,
            brk: parent.brk,
            brk_mapped: parent.brk_mapped,
            next_map: parent.next_map,
            allocations: parent.allocations,
        };
        for segment in self.shared.segments.iter_mut().flatten() {
            if segment.1 & (1 << parent_owner) != 0 {
                segment.1 |= 1 << child_owner;
            }
        }
        self.sessions[slot] = Session::new();
        self.sessions[slot].set_owner(child_owner);
        self.entries[slot] = Some(Entry {
            cookie,
            parent: Some(parent.cookie),
            process: Some(child),
            status: None,
            wait: None,
        });
        Ok(cookie as u32 as i64)
    }

    /// None means parked or already replied. Like Linux kernel_wait4, reaping
    /// precedes status copy: a bad output returns EFAULT but consumes the child.
    pub fn wait(&mut self, rt: &Native, req: &GateRequest) -> Result<Option<i64>> {
        let owner = self.owner(req.task_cookie).ok_or(-3)?;
        let pid = req.args[0] as i64;
        if req.args[2] & !1 != 0 || (pid != -1 && pid <= 0) {
            return Ok(Some(-22));
        }
        if req.args[3] != 0 {
            return Ok(Some(-38));
        }
        let cookie = self.entries[owner].as_ref().unwrap().cookie;
        let matches = |entry: &Entry| {
            entry.parent == Some(cookie) && (pid == -1 || entry.cookie as u32 as i64 == pid)
        };
        if let Some(child) = self
            .entries
            .iter()
            .position(|e| e.as_ref().is_some_and(|e| matches(e) && e.status.is_some()))
        {
            let entry = self.entries[child].as_ref().unwrap();
            if req.args[1] != 0
                && let Err(error) = rt.gate_copy_out(
                    req.capability,
                    req.args[1],
                    &entry.status.unwrap().to_le_bytes(),
                )
            {
                self.entries[child] = None;
                return Ok(Some(native_error(error)));
            }
            let pid = entry.cookie as u32 as i64;
            // Reply here so an unsuccessful reply cannot consume the status.
            rt.gate_reply(req.capability, pid as u64)
                .map_err(native_error)?;
            self.entries[child] = None;
            return Ok(None);
        }
        if !self.entries.iter().flatten().any(matches) {
            return Ok(Some(-10));
        }
        if req.args[2] & 1 != 0 {
            return Ok(Some(0));
        }
        if self.entries[owner].as_ref().unwrap().wait.is_some() {
            return Ok(Some(-11));
        }
        self.entries[owner].as_mut().unwrap().wait = Some(*req);
        Ok(None)
    }

    pub fn exec(
        &mut self,
        rt: &Native,
        store: &mut dyn Store,
        req: &GateRequest,
        busybox: &[u8],
        child: &[u8],
    ) -> Result<()> {
        let owner = self.owner(req.task_cookie).ok_or(-3)?;
        let process = self.entries[owner]
            .as_mut()
            .unwrap()
            .process
            .as_mut()
            .unwrap();
        let (path, len) = process.path(rt, req.capability, req.args[0])?;
        let image = match &path[..len] {
            b"/busybox" | b"/bin/busybox" => busybox,
            b"/bin/M4-child" => child,
            _ => return Err(-2),
        };
        let mut args = Strings::copy(rt, req, req.args[1])?;
        let mut env = Strings::copy(rt, req, req.args[2])?;
        let argv = args.pointers();
        let envp = env.pointers();
        let thread = process.threads.member(req.task_cookie).ok_or(-3)?;
        rt.gate_exec_image(
            req.capability,
            thread.context,
            thread.manager,
            image,
            &argv[..args.len],
            &envp[..env.len],
        )
        .map_err(native_error)?;
        self.sessions[owner].set_owner(process.owner);
        for fd in 0..16 {
            if process.fd_flags[fd] == 0 {
                continue;
            }
            if fd < 3 {
                process.standard_open[fd] = false;
            } else {
                if let Some(wire) = process.fds[fd] {
                    self.sessions[owner].close(store, wire)?;
                }
                process.fds[fd] = None;
                process.dirs[fd] = false;
            }
            process.fd_flags[fd] = 0;
        }
        // P3 exec fence: the old image closed every fd, so a v2 session is
        // re-opened under a new generation; v1 keeps its single session.
        self.sessions[owner].reopen(store)?;
        for allocation in &mut process.allocations {
            if let Some((cap, _, _)) = allocation.take() {
                self.shared.release_segment(rt, cap, process.owner)?;
            }
        }
        process.brk = HEAP;
        process.brk_mapped = HEAP;
        process.next_map = MMAP;
        process.tls = 0;
        process.clear_tid = 0;
        process.threads.exec_reset();
        Ok(())
    }

    pub fn exit(
        &mut self,
        rt: &Native,
        store: &mut dyn Store,
        owner: usize,
        req: &GateRequest,
        status: u32,
    ) -> Result<Option<u64>> {
        let entry = self.entries[owner].as_mut().unwrap();
        let process = entry.process.as_mut().unwrap();
        process.threads.destroy_all(rt, req)?;
        // Close the Process's Store owner before personality cleanup. A failed
        // close keeps the entry; the caller currently treats it as fatal.
        self.sessions[owner].set_owner(process.owner);
        self.sessions[owner].close_owner(store)?;
        process.cleanup(&mut self.shared, rt)?;
        entry.wait = None;
        entry.process = None;
        entry.status = Some(status);
        // Native Task destruction has no personality zombie; the user table
        // owns both this status and the parent's deferred request.
        let waits: [Option<GateRequest>; MAX_PROCESSES] =
            core::array::from_fn(|i| self.entries[i].as_mut().and_then(|e| e.wait.take()));
        for wait in waits.into_iter().flatten() {
            if let Some(value) = self.wait(rt, &wait)? {
                rt.gate_reply(wait.capability, value as u64)
                    .map_err(native_error)?;
            }
        }
        if self.entries.iter().flatten().all(|e| e.process.is_none()) {
            let status = self.entries[0].as_ref().unwrap().status.unwrap();
            return Ok(Some(if status & 127 == 0 {
                u64::from(status >> 8)
            } else {
                128 + u64::from(status & 127)
            }));
        }
        Ok(None)
    }
}

struct Strings {
    bytes: [[u8; 128]; 16],
    len: usize,
}
impl Strings {
    fn copy(rt: &Native, req: &GateRequest, vector: u64) -> Result<Self> {
        let mut result = Self {
            bytes: [[0; 128]; 16],
            len: 0,
        };
        if vector == 0 {
            return Ok(result);
        }
        for index in 0..=16 {
            let mut bytes = [0; 8];
            rt.gate_copy_in(
                req.capability,
                vector.checked_add(index * 8).ok_or(-14)?,
                &mut bytes,
            )
            .map_err(native_error)?;
            let pointer = u64::from_le_bytes(bytes);
            if pointer == 0 {
                return Ok(result);
            }
            if index == 16 {
                return Err(-7);
            }
            let string = &mut result.bytes[index as usize];
            for (offset, byte) in string.iter_mut().enumerate() {
                rt.gate_copy_in(
                    req.capability,
                    pointer.checked_add(offset as u64).ok_or(-14)?,
                    core::slice::from_mut(byte),
                )
                .map_err(native_error)?;
                if *byte == 0 {
                    break;
                }
                if offset == 127 {
                    return Err(-7);
                }
            }
            result.len += 1;
        }
        unreachable!()
    }
    fn pointers(&mut self) -> [u64; 16] {
        core::array::from_fn(|i| self.bytes[i].as_ptr() as u64)
    }
}
