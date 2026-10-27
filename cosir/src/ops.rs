use crate::state::{
    ACCESS_W, ACCESS_X, Actor, AuditKind, CAP_EXC_SUBSCRIBE, CAP_SEG_MAP_W, CapId, CapObject,
    ExcSourceId, ExceptionKind, PendingException, SRET_FRAME_ALIGN, SRET_FRAME_LEN, SretFrame,
    State, Task, TaskId, TaskStatus, USER_CS, USER_SS,
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
///
/// 订阅者通知复用 authority 的本地 FIFO/直投。硬件执行异常另提交 Faulted；
/// SyscallTrap/IRQ/TaskExit 仅递送通知，不改变目标运行状态。非机器细化证明。
pub fn raise_exception(
    st: &mut State,
    faulting_task: TaskId,
    source_id: ExcSourceId,
    kind: ExceptionKind,
    fault_va: Option<u64>,
    save_frame: SretFrame,
) {
    assert!(
        st.exc_sources
            .get(&source_id)
            .is_some_and(|source| source.active && source.supports_kind(kind))
    );
    assert_eq!(
        matches!(kind, ExceptionKind::PageFault { .. }),
        fault_va.is_some(),
        "raise_exception: PageFault must carry a fault VA and other exceptions must not"
    );
    if kind.is_hardware_fault() {
        let task = st
            .tasks
            .get(&faulting_task)
            .expect("raise_exception: task must exist");
        assert_eq!(
            task.status,
            TaskStatus::Running,
            "raise_exception: task must be Running"
        );
    }
    // §12.5 成功动作 1：先追加 `ExceptionRaised`（`KernelActor`，早于状态迁移）。
    st.push_audit(
        Actor::Kernel,
        AuditKind::ExceptionRaised {
            source_id,
            kind,
            fault_va,
        },
    );
    crate::authority::notify_exception(st, source_id, kind, fault_va);
    if !kind.is_hardware_fault() {
        return;
    }
    // §12.5 成功动作 3：置 Faulted、写唯一 `pending_exc`，再追加 `TaskStateChanged`。
    st.tasks
        .get_mut(&faulting_task)
        .expect("raise_exception: task must exist")
        .status = TaskStatus::Faulted;
    st.pending_exc.insert(
        faulting_task,
        PendingException {
            source_id,
            kind,
            fault_va,
            save_frame,
        },
    );
    st.push_audit(
        Actor::Kernel,
        AuditKind::TaskStateChanged {
            task: faulting_task,
            from: TaskStatus::Running,
            to: TaskStatus::Faulted,
        },
    );
}

/// 用户态 canonical 检验：地址必须落入用户低半区 (`≤ 0x0000_7fff_ffff_ffff`)，
/// 对应规范 §12.10 "canonical 用户地址"。与 `pagewalk::is_canonical_vaddr`
/// (接受内核高半区 canonical) 不同；此处刻意拒绝高半区，防止 exc_resume
/// 恢复到内核地址，是安全边界而非定义矛盾。
///
/// 边界假设：硬编码 4 级分页的 48 位 canonical（bit 63..48 符号扩展）；LA57/5 级
/// 分页下用户地址上界可达 `0x00ff_ffff_ffff_ffff`，此处会把合法用户地址误判为非 canonical。
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
/// 所有授权、pending 与恢复帧验证均在 mutation 前完成；成功路径按固定顺序
/// 原子消费用户帧、保存 kernel frame、记录 Audit，并将 Task 置为 Ready。
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
        .filter(|cap| cap.active && st.held(caller, exc_source_cap))
        .ok_or(ResumeError::InvalidExcSource)?;
    let source_id = match source_cap.object {
        CapObject::ExceptionSource(source_id) => source_id,
        _ => {
            return Err(ResumeError::InvalidExcSource);
        }
    };
    if source_cap.rights & CAP_EXC_SUBSCRIBE == 0 {
        return Err(ResumeError::MissingRight);
    }
    let source = st
        .exc_sources
        .get(&source_id)
        .filter(|source| source.active)
        .ok_or(ResumeError::InvalidExcSource)?;
    if !st.subscriptions.contains(&(caller, source_id)) {
        return Err(ResumeError::NotSubscribed);
    }

    let frame_cap = st
        .capabilities
        .get(&sret_frame_seg_cap)
        .filter(|cap| cap.active && st.held(caller, sret_frame_seg_cap))
        .ok_or(ResumeError::InvalidFrameAddr)?;
    let frame_seg_id = match frame_cap.object {
        CapObject::Segment(seg_id) => seg_id,
        _ => {
            return Err(ResumeError::InvalidFrameAddr);
        }
    };
    if frame_cap.rights & CAP_SEG_MAP_W == 0 {
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
    let task = st.tasks.get(&caller).ok_or(ResumeError::InvalidExcSource)?;
    if task.status != TaskStatus::Faulted {
        return Err(ResumeError::NoPendingException);
    }
    let pending = st
        .pending_exc
        .get(&caller)
        .ok_or(ResumeError::NoPendingException)?;
    // §12.3：source 的 `kind` 是「类型签名」（约束此源可产生的异常**类别**，非具体实例），
    // 故按变体比较而非整值——异因同类（如 `page_present`/`writable_fault` 位不同）的
    // `PageFault` 属同一类型签名，应当匹配。
    if pending.source_id != source_id || !source.supports_kind(pending.kind) {
        return Err(ResumeError::InvalidExcSource);
    }
    if !pending.kind.is_recoverable() {
        return Err(ResumeError::UnrecoverableFault);
    }

    let frame_bytes = seg
        .bytes
        .get(frame_offset..frame_end)
        .ok_or(ResumeError::InvalidFrameAddr)?;
    let frame = SretFrame::decode(frame_bytes).ok_or(ResumeError::InvalidFrame)?;
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

    st.segments
        .get_mut(&frame_seg_id)
        .expect("validated frame segment must still exist")
        .bytes
        .get_mut(frame_offset..frame_end)
        .expect("validated frame range must still exist")
        .fill(0);
    st.pending_exc
        .remove(&caller)
        .expect("validated pending exception must still exist");
    st.tasks
        .get_mut(&caller)
        .expect("validated task must still exist")
        .kernel_resume_frame = Some(frame);
    st.push_audit(
        Actor::Task(caller),
        AuditKind::TaskResumed {
            source_id,
            source_cap: exc_source_cap,
            frame_cap: sret_frame_seg_cap,
            frame_seg_id,
            frame_offset,
            rip: frame.rip,
        },
    );
    st.tasks
        .get_mut(&caller)
        .expect("validated task must still exist")
        .status = TaskStatus::Ready;
    Ok(())
}
