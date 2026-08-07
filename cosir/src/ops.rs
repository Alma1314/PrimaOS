use crate::state::{
    ACCESS_X, ExcSourceId, ExceptionKind, PendingException, SRET_FRAME_LEN, SegId, SretFrame,
    State, TaskId, TaskStatus, VirtAddr,
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

/// 恢复异常帧：Faulted -> Ready。
///
/// 语义（§12.10）：仅当 caller 有未恢复的异常帧、且源匹配、且 kind 可恢复时成功。
/// 帧边界（前置 #5）与 resume_vaddr 可执行性（前置 #6）均从模型实数据推导，而非
/// 调用者供应 bool：帧段不存在 / 越界 / 未对齐 → `InvalidFrameAddr`；
/// resume_vaddr 不在 caller vspace 可执行映射上 → `NotExecutable`。
/// 不可恢复异常（如 DoubleFault）→ `UnrecoverableFault` 且**不销毁 Task**（保持 Faulted）。
pub fn exc_resume(
    st: &mut State,
    caller: TaskId,
    source_id: ExcSourceId,
    frame_seg_id: SegId,
    frame_offset: usize,
    resume_vaddr: VirtAddr,
) -> Result<(), ResumeError> {
    // (1) 源存在性校验
    if !st.exc_sources.contains_key(&source_id) {
        return Err(ResumeError::InvalidExcSource);
    }
    // (2) 帧段边界（前置 #5）：段存在、frame_offset 帧长(0x88)整数倍对齐、帧完全落在段内
    let seg = st
        .segments
        .get(&frame_seg_id)
        .ok_or(ResumeError::InvalidFrameAddr)?;
    if !frame_offset.is_multiple_of(SRET_FRAME_LEN) {
        return Err(ResumeError::InvalidFrameAddr);
    }
    let frame_end = frame_offset.checked_add(SRET_FRAME_LEN);
    if frame_end.is_none_or(|end| end > seg.len) {
        return Err(ResumeError::InvalidFrameAddr);
    }
    // (3) resume_vaddr 可执行（前置 #6）：caller vspace 中有 base==resume_vaddr 的可执行映射
    let executable = st.tasks.get(&caller).is_some_and(|task| {
        task.vspace
            .iter()
            .any(|(base, access)| *base == resume_vaddr && access & ACCESS_X != 0)
    });
    if !executable {
        return Err(ResumeError::NotExecutable);
    }
    // (4) 待恢复帧必须存在且匹配源
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
    // (5) 转换：Faulted -> Ready，并原子清除待恢复帧
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
