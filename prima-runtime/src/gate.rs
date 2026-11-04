//! Native, OS-neutral synchronous request delivery. A copied Capability is only
//! a token: the kernel still checks its current holder and one-shot lifetime.
use crate::{Capability, Error, Result, Runtime, SyscallTransport};
use prima_abi::syscall::*;

/// One delivered request. `task_cookie` identifies a Task incarnation, not an ID.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateRequest {
    pub capability: Capability,
    pub task_cookie: u64,
    pub number: u64,
    pub args: [u64; 6],
    /// Bit zero is set for int80; zero denotes the x86_64 syscall instruction.
    pub flags: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateAudit {
    pub cursor: u64,
    pub kind: u64,
    pub request: u64,
    pub cookie: u64,
    pub number: u64,
    pub value: u64,
    pub flags: u64,
    pub args: [u64; 6],
}

impl GateRequest {
    fn decode(words: [u64; 12]) -> Result<Self> {
        if words[0] != 1 || words[1] != 96 || words[3] == 0 || words[11] & !1 != 0 {
            return Err(Error::InvalidResponse);
        }
        Ok(Self {
            capability: Capability::from_raw(words[2])?,
            task_cookie: words[3],
            number: words[4],
            args: words[5..11].try_into().unwrap(),
            flags: words[11],
        })
    }
}

/// OS-neutral saved user registers. This is the 160-byte Gate wire layout,
/// not a signal frame. Kernel authorization and live mapping checks still apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GateFrame {
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub rbp: u64,
    pub rsp: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rflags: u64,
    pub rip: u64,
    pub cs: u64,
    pub ss: u64,
}

impl GateFrame {
    fn from_words(words: [u64; 20]) -> Self {
        Self {
            rax: words[0],
            rbx: words[1],
            rcx: words[2],
            rdx: words[3],
            rsi: words[4],
            rdi: words[5],
            rbp: words[6],
            rsp: words[7],
            r8: words[8],
            r9: words[9],
            r10: words[10],
            r11: words[11],
            r12: words[12],
            r13: words[13],
            r14: words[14],
            r15: words[15],
            rflags: words[16],
            rip: words[17],
            cs: words[18],
            ss: words[19],
        }
    }

    fn words(self) -> [u64; 20] {
        [
            self.rax,
            self.rbx,
            self.rcx,
            self.rdx,
            self.rsi,
            self.rdi,
            self.rbp,
            self.rsp,
            self.r8,
            self.r9,
            self.r10,
            self.r11,
            self.r12,
            self.r13,
            self.r14,
            self.r15,
            self.rflags,
            self.rip,
            self.cs,
            self.ss,
        ]
    }
}

/// GPRs and the current platform's x87/SSE FXSAVE image. XSAVE/AVX are outside
/// this format. Kernel MXCSR validation precedes the atomic reply commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GateContext {
    pub frame: GateFrame,
    pub fpu: [u8; 512],
}

impl<T: SyscallTransport> Runtime<T> {
    /// Delegate a clean, Ready Task's traps to this native service. Passing no
    /// Gate creates one; otherwise the Task joins this service's existing Gate.
    /// `orphan_value` is opaque to the kernel and survives service authority loss.
    pub fn gate_attach(
        &self,
        task: Capability,
        gate: Option<Capability>,
        orphan_value: u64,
    ) -> Result<Capability> {
        Capability::from_raw(self.call(
            SYS_GATE_ATTACH,
            [
                task.as_raw(),
                gate.map_or(0, Capability::as_raw),
                orphan_value,
                0,
                0,
                0,
            ],
        )?)
    }

    /// Wait without a native signal interruption or deadline. The kernel
    /// revalidates this local output after wake, before delivering a request.
    pub fn gate_wait(&self, gate: Capability) -> Result<GateRequest> {
        let mut words = [0u64; 12];
        self.call_unit(
            SYS_GATE_WAIT,
            [gate.as_raw(), words.as_mut_ptr() as u64, 96, 0, 0, 0],
        )?;
        GateRequest::decode(words)
    }

    pub fn gate_copy_in(&self, request: Capability, address: u64, bytes: &mut [u8]) -> Result<()> {
        self.gate_copy(
            SYS_GATE_COPY_IN,
            request,
            address,
            bytes.as_mut_ptr() as u64,
            bytes.len(),
        )
    }

    pub fn gate_copy_out(&self, request: Capability, address: u64, bytes: &[u8]) -> Result<()> {
        self.gate_copy(
            SYS_GATE_COPY_OUT,
            request,
            address,
            bytes.as_ptr() as u64,
            bytes.len(),
        )
    }

    fn gate_copy(
        &self,
        number: u64,
        request: Capability,
        address: u64,
        buffer: u64,
        len: usize,
    ) -> Result<()> {
        if len > 4096 {
            return Err(Error::InvalidInput);
        }
        if address.checked_add(len as u64).is_none() {
            return Err(Error::Fault);
        }
        if self.call(
            number,
            [request.as_raw(), address, len as u64, buffer, 0, 0],
        )? != len as u64
        {
            return Err(Error::InvalidResponse);
        }
        Ok(())
    }

    /// Complete exactly once, writing only the client's rax. All aliases become
    /// stale; this method makes no promise that the client runs immediately.
    pub fn gate_reply(&self, request: Capability, value: u64) -> Result<()> {
        self.call_unit(SYS_GATE_REPLY, [request.as_raw(), value, 0, 0, 0, 0])
    }

    /// Explicitly grant a separate TaskContext root from held Task management
    /// authority. Revoking the manager does not revoke this independent root.
    pub fn task_context_authority(&self, manager: Capability) -> Result<Capability> {
        Capability::from_raw(self.call(
            SYS_TASK_CONTEXT_AUTHORITY,
            [manager.as_raw(), 0, 0, 0, 0, 0],
        )?)
    }

    pub fn gate_audit_read(&self, gate: Capability, cursor: u64) -> Result<Option<GateAudit>> {
        let mut words = [0u64; 16];
        let found = self.call(
            SYS_GATE_AUDIT_READ,
            [gate.as_raw(), cursor, words.as_mut_ptr() as u64, 128, 0, 0],
        )?;
        if found == 0 {
            return Ok(None);
        }
        if found != 1
            || words[0] != 1
            || words[1] != 128
            || words[2] <= cursor
            || !(1..=3).contains(&words[3])
        {
            return Err(Error::InvalidResponse);
        }
        Ok(Some(GateAudit {
            cursor: words[2],
            kind: words[3],
            request: words[5],
            cookie: words[6],
            number: words[7],
            value: words[8],
            flags: words[9],
            args: words[10..].try_into().unwrap(),
        }))
    }

    pub fn gate_fork(
        &self,
        request: Capability,
        context: Capability,
        manager: Capability,
        gate: Capability,
    ) -> Result<Capability> {
        Capability::from_raw(self.call(
            SYS_GATE_FORK,
            [
                request.as_raw(),
                context.as_raw(),
                manager.as_raw(),
                gate.as_raw(),
                0,
                0,
            ],
        )?)
    }

    pub fn gate_exec_image(
        &self,
        request: Capability,
        context: Capability,
        manager: Capability,
        image: &[u8],
        argv: &[u64],
        env: &[u64],
    ) -> Result<()> {
        let descriptor = [
            image.as_ptr() as u64,
            image.len() as u64,
            argv.as_ptr() as u64,
            argv.len() as u64,
            env.as_ptr() as u64,
            env.len() as u64,
        ];
        self.call_unit(
            SYS_GATE_EXEC_IMAGE,
            [
                request.as_raw(),
                context.as_raw(),
                manager.as_raw(),
                descriptor.as_ptr() as u64,
                0,
                0,
            ],
        )
    }

    pub fn gate_read_frame(&self, request: Capability, context: Capability) -> Result<GateFrame> {
        let mut words = [0u64; 20];
        self.call_unit(
            SYS_GATE_READ_FRAME,
            [
                request.as_raw(),
                context.as_raw(),
                words.as_mut_ptr() as u64,
                160,
                0,
                0,
            ],
        )?;
        Ok(GateFrame::from_words(words))
    }

    /// Complete the request once with a new user context. Invalid frames leave
    /// it Delivered; no FS/FPU state or signal policy is implied by this call.
    pub fn gate_reply_frame(
        &self,
        request: Capability,
        context: Capability,
        frame: GateFrame,
    ) -> Result<()> {
        let words = frame.words();
        self.call_unit(
            SYS_GATE_REPLY_FRAME,
            [
                request.as_raw(),
                context.as_raw(),
                words.as_ptr() as u64,
                160,
                0,
                0,
            ],
        )
    }

    pub fn gate_read_context(
        &self,
        request: Capability,
        context: Capability,
    ) -> Result<GateContext> {
        let mut words = [0u64; 84];
        self.call_unit(
            SYS_GATE_READ_FRAME,
            [
                request.as_raw(),
                context.as_raw(),
                words.as_mut_ptr() as u64,
                672,
                0,
                0,
            ],
        )?;
        let mut fpu = [0; 512];
        for (chunk, word) in fpu.as_chunks_mut::<8>().0.iter_mut().zip(&words[20..]) {
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        Ok(GateContext {
            frame: GateFrame::from_words(words[..20].try_into().unwrap()),
            fpu,
        })
    }

    pub fn gate_reply_context(
        &self,
        request: Capability,
        context: Capability,
        state: &GateContext,
    ) -> Result<()> {
        let mut words = [0u64; 84];
        words[..20].copy_from_slice(&state.frame.words());
        for (word, chunk) in words[20..]
            .iter_mut()
            .zip(state.fpu.as_chunks::<8>().0.iter())
        {
            *word = u64::from_le_bytes(*chunk);
        }
        self.call_unit(
            SYS_GATE_REPLY_FRAME,
            [
                request.as_raw(),
                context.as_raw(),
                words.as_ptr() as u64,
                672,
                0,
                0,
            ],
        )
    }

    /// Spawn into the request's Process/root and Gate before publishing Ready.
    /// No Linux clone flags, tid writes or parent reply are implied.
    pub fn gate_spawn_thread(
        &self,
        request: Capability,
        context: Capability,
        gate: Capability,
        frame: GateFrame,
        fs_base: u64,
        id_outputs: [u64; 2],
    ) -> Result<Capability> {
        let mut words = [0; 22];
        words[..20].copy_from_slice(&frame.words());
        words[20..].copy_from_slice(&id_outputs);
        Capability::from_raw(self.call(
            SYS_GATE_SPAWN_THREAD,
            [
                request.as_raw(),
                context.as_raw(),
                words.as_ptr() as u64,
                176,
                fs_base,
                gate.as_raw(),
            ],
        )?)
    }

    pub fn gate_abort(&self, request: Capability, value: u64) -> Result<()> {
        self.call_unit(SYS_GATE_ABORT, [request.as_raw(), value, 0, 0, 0, 0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TransportError;
    use core::cell::Cell;

    #[test]
    fn request_layout_and_copy_bounds_preserve_opaque_results() {
        let raw = 0x0000_0001_0000_0002;
        let words = [1, 96, raw, 0x200000003, 123, 1, 2, 3, 4, 5, 6, 1];
        let request = GateRequest::decode(words).unwrap();
        assert_eq!(request.number, 123);
        assert_eq!(request.args, [1, 2, 3, 4, 5, 6]);
        for (at, value) in [(0, 2), (1, 95), (2, 0), (3, 0), (11, 2)] {
            let mut bad = words;
            bad[at] = value;
            assert!(GateRequest::decode(bad).is_err());
        }
        struct Transport(Cell<usize>);
        impl SyscallTransport for Transport {
            fn invoke(
                &self,
                number: u64,
                args: [u64; 6],
            ) -> core::result::Result<u64, TransportError> {
                self.0.set(self.0.get() + 1);
                assert_eq!(args[0], 0x0000_0001_0000_0002);
                assert_eq!(args[4..], [0, 0]);
                Ok(match number {
                    SYS_GATE_REPLY | SYS_GATE_ABORT => {
                        assert_eq!(args[1], (-38i64) as u64);
                        0
                    }
                    SYS_GATE_COPY_IN | SYS_GATE_COPY_OUT => args[2],
                    _ => panic!("unexpected operation"),
                })
            }
        }
        let runtime = Runtime::new(Transport(Cell::new(0)));
        runtime
            .gate_reply(request.capability, (-38i64) as u64)
            .unwrap();
        runtime
            .gate_abort(request.capability, (-38i64) as u64)
            .unwrap();
        runtime
            .gate_copy_out(request.capability, u64::MAX, &[])
            .unwrap();
        assert_eq!(
            runtime.gate_copy_in(request.capability, u64::MAX, &mut [0; 1]),
            Err(Error::Fault)
        );
        assert_eq!(
            runtime.gate_copy_out(request.capability, 0, &[0; 4097]),
            Err(Error::InvalidInput)
        );
        assert_eq!(runtime.into_transport().0.get(), 3);
    }
}
