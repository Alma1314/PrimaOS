use crate::state::{
    ACCESS_W, ACCESS_X, CAP_EXC_SUBSCRIBE, CAP_SEG_WRITE, CapId, CapObject, ExcSourceId,
    ExceptionKind, PendingException, SRET_FRAME_ALIGN, SRET_FRAME_LEN, SretFrame, State, Task,
    TaskId, TaskStatus, USER_CS, USER_SS,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResumeError {
    InvalidExcSource,
    MissingRight,
    NotSubscribed,
    InvalidFrameAddr,
    InvalidFrame,
    NoPendingException,
    UnrecoverableFault,
}

/// 异常发生：Running -> Faulted，并保存现场帧。
///
/// 探针构造级不变量（§12.10 raise 侧）：Task 必须存在且处于 Running。无合法失败路径，
/// 以 assert 表达而非错误码，避免复用 exc_resume 的 `ResumeError` 域污染错误域。
pub fn raise_exception(
    st: &mut State,
    faulting_task: TaskId,
    source_id: ExcSourceId,
    kind: ExceptionKind,
    save_frame: SretFrame,
) {
    let task = st
        .tasks
        .get_mut(&faulting_task)
        .expect("raise_exception: task must exist");
    assert_eq!(
        task.status,
        TaskStatus::Running,
        "raise_exception: task must be Running"
    );
    task.status = TaskStatus::Faulted;
    st.pending_exc.insert(
        faulting_task,
        PendingException {
            source_id,
            kind,
            save_frame,
        },
    );
}

fn is_user_canonical(addr: u64) -> bool {
    addr <= 0x0000_7fff_ffff_ffff
}

fn has_user_access(task: &Task, addr: u64, access: u8) -> bool {
    task.vspace
        .iter()
        .any(|mapping| mapping.user && mapping.access & access != 0 && mapping.contains(addr))
}

fn valid_user_rflags(flags: u64) -> bool {
    const FIXED: u64 = 1 << 1;
    const FORBIDDEN: u64 = (0b11 << 12) | (1 << 14) | (1 << 16) | (1 << 17) | (1 << 19) | (1 << 20);
    const ALLOWED: u64 = (1 << 0)
        | FIXED
        | (1 << 2)
        | (1 << 4)
        | (1 << 6)
        | (1 << 7)
        | (1 << 8)
        | (1 << 9)
        | (1 << 10)
        | (1 << 11)
        | (1 << 18)
        | (1 << 21);
    flags & FIXED != 0 && flags & FORBIDDEN == 0 && flags & !ALLOWED == 0
}

/// 通过异常源与帧段 Capability 授权恢复：Faulted -> Ready。
///
/// 本阶段解码并纯验证恢复帧，同时维持现有成功 transition；清零、
/// kernel_resume_frame 与 Audit 的最终提交语义由后续任务细化。
pub fn exc_resume(
    st: &mut State,
    caller: TaskId,
    exc_source_cap: CapId,
    sret_frame_seg_cap: CapId,
    frame_offset: usize,
) -> Result<(), ResumeError> {
    let source_cap = st
        .capabilities
        .get(&exc_source_cap)
        .filter(|cap| cap.active && cap.owner == caller)
        .ok_or(ResumeError::InvalidExcSource)?;
    let source_id = match source_cap.object {
        CapObject::ExceptionSource(source_id) => source_id,
        CapObject::Segment(_) => return Err(ResumeError::InvalidExcSource),
    };
    if source_cap.rights & CAP_EXC_SUBSCRIBE == 0 {
        return Err(ResumeError::MissingRight);
    }
    let source = st
        .exc_sources
        .get(&source_id)
        .filter(|source| source.active)
        .ok_or(ResumeError::InvalidExcSource)?;
    let supported_kind = source.supported_kind;
    if !st.subscriptions.contains(&(caller, source_id)) {
        return Err(ResumeError::NotSubscribed);
    }

    let frame_cap = st
        .capabilities
        .get(&sret_frame_seg_cap)
        .filter(|cap| cap.active && cap.owner == caller)
        .ok_or(ResumeError::InvalidFrameAddr)?;
    let frame_seg_id = match frame_cap.object {
        CapObject::Segment(seg_id) => seg_id,
        CapObject::ExceptionSource(_) => return Err(ResumeError::InvalidFrameAddr),
    };
    if frame_cap.rights & CAP_SEG_WRITE == 0 {
        return Err(ResumeError::MissingRight);
    }
    if !frame_offset.is_multiple_of(SRET_FRAME_ALIGN) {
        return Err(ResumeError::InvalidFrameAddr);
    }
    let frame_end = frame_offset
        .checked_add(SRET_FRAME_LEN)
        .ok_or(ResumeError::InvalidFrameAddr)?;
    let seg = st
        .segments
        .get(&frame_seg_id)
        .ok_or(ResumeError::InvalidFrameAddr)?;
    if !seg.kernel_readable {
        return Err(ResumeError::InvalidFrame);
    }
    let frame_bytes = seg
        .bytes
        .get(frame_offset..frame_end)
        .ok_or(ResumeError::InvalidFrameAddr)?;
    let frame = SretFrame::decode(frame_bytes).ok_or(ResumeError::InvalidFrame)?;

    let task = st.tasks.get(&caller).ok_or(ResumeError::InvalidExcSource)?;
    if task.status != TaskStatus::Faulted {
        return Err(ResumeError::NoPendingException);
    }
    let pending = st
        .pending_exc
        .get(&caller)
        .ok_or(ResumeError::NoPendingException)?;
    if pending.source_id != source_id || pending.kind != supported_kind {
        return Err(ResumeError::InvalidExcSource);
    }
    if !pending.kind.is_recoverable() {
        return Err(ResumeError::UnrecoverableFault);
    }
    if !is_user_canonical(frame.rip)
        || !has_user_access(task, frame.rip, ACCESS_X)
        || !is_user_canonical(frame.rsp)
        || !has_user_access(task, frame.rsp, ACCESS_W)
        || frame.cs != USER_CS
        || frame.ss != USER_SS
        || !valid_user_rflags(frame.rflags)
    {
        return Err(ResumeError::InvalidFrame);
    }

    st.tasks
        .get_mut(&caller)
        .expect("validated task must still exist")
        .status = TaskStatus::Ready;
    st.pending_exc.remove(&caller);
    Ok(())
}

/// 调度选中 Task 进入 Running。仅 Ready -> Running；Faulted 状态不变
/// （指令流重播由细化层在 resume 后交付，不在此处隐式发生）。
pub fn schedule(st: &mut State, chosen: TaskId) {
    if let Some(task) = st.tasks.get_mut(&chosen)
        && task.status == TaskStatus::Ready
    {
        task.status = TaskStatus::Running;
    }
}
