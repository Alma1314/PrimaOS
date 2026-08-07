use crate::state::{
    ExcSourceId, ExceptionKind, PendingException, SretFrame, State, TaskId, TaskStatus,
};

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // MissingWrite / NotSubscribed 属 §12.10 错误域的保留变体，本探针不触发
pub enum ResumeError {
    NoPendingException,
    UnrecoverableFault,
    InvalidFrameAddr,
    NotExecutable,
    MissingWrite,
    InvalidExcSource,
    NotSubscribed,
}

/// 异常发生：Running -> Faulted，并保存现场帧。
pub fn raise_exception(
    st: &mut State,
    faulting_task: TaskId,
    source_id: ExcSourceId,
    kind: ExceptionKind,
    save_frame: SretFrame,
) -> Result<(), ResumeError> {
    let task = st
        .tasks
        .get_mut(&faulting_task)
        .ok_or(ResumeError::InvalidExcSource)?;
    if task.status != TaskStatus::Running {
        return Err(ResumeError::NoPendingException);
    }
    task.status = TaskStatus::Faulted;
    st.pending_exc.insert(
        faulting_task,
        PendingException {
            source_id,
            kind,
            save_frame,
        },
    );
    Ok(())
}

/// 恢复异常帧：Faulted -> Ready。
///
/// 语义（§12.10）：仅当 caller 有未恢复的异常帧、且源匹配、且 kind 可恢复时成功；
/// 帧 Segment 非法（seg_ok=false）→ `InvalidFrameAddr`；resume 地址不可执行
/// （exec_ok=false）→ `NotExecutable`；不可恢复异常（如 DoubleFault）→
/// `UnrecoverableFault` 且**不销毁 Task**（保持 Faulted）。
pub fn exc_resume(
    st: &mut State,
    caller: TaskId,
    source_id: ExcSourceId,
    seg_ok: bool,  // true: 帧 Segment 存在、frame_offset 对齐、可写且内核可读
    exec_ok: bool, // true: resume_vaddr 已映射且可执行
) -> Result<(), ResumeError> {
    // (1) 源存在性校验
    if !st.exc_sources.contains_key(&source_id) {
        return Err(ResumeError::InvalidExcSource);
    }
    if !seg_ok {
        return Err(ResumeError::InvalidFrameAddr);
    }
    if !exec_ok {
        return Err(ResumeError::NotExecutable);
    }
    // (2) 待恢复帧必须存在且匹配源
    let pending = st
        .pending_exc
        .get(&caller)
        .ok_or(ResumeError::NoPendingException)?;
    if pending.source_id != source_id {
        return Err(ResumeError::InvalidExcSource);
    }
    if !pending.kind.is_recoverable() {
        return Err(ResumeError::UnrecoverableFault);
    }
    // (3) 转换：Faulted -> Ready，并原子清除待恢复帧
    let task = st
        .tasks
        .get_mut(&caller)
        .ok_or(ResumeError::InvalidExcSource)?;
    if task.status != TaskStatus::Faulted {
        return Err(ResumeError::NoPendingException);
    }
    task.status = TaskStatus::Ready;
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
