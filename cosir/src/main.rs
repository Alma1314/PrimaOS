mod ops;
mod state;

use ops::{exc_resume, raise_exception, schedule};
use state::{ACCESS_R, ACCESS_W, ACCESS_X, ExceptionKind, SretFrame, State, Task, TaskStatus};

fn default_frame() -> SretFrame {
    let mut f = SretFrame {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        rsp: 0x7fff_f000,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        flags: 0x202,
    };
    f.rax = 0x1234;
    f
}

fn build_state() -> State {
    let mut st = State::default();
    st.segments.insert(1, state::SegRange { len: 0x1000 });
    st.exc_sources.insert(
        1,
        state::ExceptionSource {
            supported_kind: ExceptionKind::PageFault {
                page_present: false,
                writable_fault: false,
            },
        },
    );
    let t = Task {
        id: 1,
        status: TaskStatus::Running,
        vspace: vec![(0x4000_0000, ACCESS_R | ACCESS_W), (0x5000_0000, ACCESS_X)],
    };
    st.tasks.insert(1, t);
    st
}

fn page_fault() -> ExceptionKind {
    ExceptionKind::PageFault {
        page_present: false,
        writable_fault: true,
    }
}

fn main() {
    let mut st = build_state();
    raise_exception(&mut st, 1, 1, page_fault(), default_frame()).expect("raise page fault");
    exc_resume(&mut st, 1, 1, true, true).expect("resume page fault");
    schedule(&mut st, 1);
    assert_eq!(st.tasks[&1].status, TaskStatus::Running);
    println!("PASS cosir raise_exception");
    println!("PASS cosir exc_resume");
    println!("PASS cosir schedule");
    println!("PASS cosir recoverable-fault roundtrip");
}

#[cfg(test)]
mod tests {
    use super::*;
    use ops::ResumeError;

    #[test]
    fn raise_exception_moves_running_to_faulted() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame()).unwrap();
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
        assert!(st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_recovers_faulted_to_ready() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame()).unwrap();
        exc_resume(&mut st, 1, 1, true, true).unwrap();
        assert_eq!(st.tasks[&1].status, TaskStatus::Ready);
        assert!(!st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_on_ready_is_no_pending() {
        let mut st = build_state();
        st.tasks.get_mut(&1).unwrap().status = TaskStatus::Ready;
        // 从未 fault：pending 不存在
        let err = exc_resume(&mut st, 1, 1, true, true).unwrap_err();
        assert_eq!(err, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_after_consumed_is_no_pending() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame()).unwrap();
        exc_resume(&mut st, 1, 1, true, true).unwrap();
        let err = exc_resume(&mut st, 1, 1, true, true).unwrap_err();
        assert_eq!(err, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_double_fault_is_unrecoverable() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, ExceptionKind::DoubleFault, default_frame()).unwrap();
        let err = exc_resume(&mut st, 1, 1, true, true).unwrap_err();
        assert_eq!(err, ResumeError::UnrecoverableFault);
        // 不销毁 Task，仍处于 Faulted
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
    }

    #[test]
    fn exc_resume_source_mismatch_is_invalid_source() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame()).unwrap();
        // 源不匹配：pending.source_id=1，调用方传 999
        let err = exc_resume(&mut st, 1, 999, true, true).unwrap_err();
        assert_eq!(err, ResumeError::InvalidExcSource);
        // 源不匹配不消费帧、不改状态：仍 Faulted 且 pending 保留
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
        assert!(st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_bad_frame_addr_rejected() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame()).unwrap();
        let err = exc_resume(&mut st, 1, 1, false, true).unwrap_err();
        assert_eq!(err, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_non_executable_target_rejected() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame()).unwrap();
        let err = exc_resume(&mut st, 1, 1, true, false).unwrap_err();
        assert_eq!(err, ResumeError::NotExecutable);
    }

    #[test]
    fn schedule_faulted_without_resume_is_safe() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, ExceptionKind::DoubleFault, default_frame()).unwrap();
        // Faulted 状态 schedule 不会回到 Running（原指令流不重播）
        schedule(&mut st, 1);
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
    }

    #[test]
    fn schedule_ready_becomes_running() {
        let mut st = build_state();
        st.tasks.get_mut(&1).unwrap().status = TaskStatus::Ready;
        schedule(&mut st, 1);
        assert_eq!(st.tasks[&1].status, TaskStatus::Running);
    }
}
