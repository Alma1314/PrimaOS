#![allow(dead_code)]

use crate::memseg_refine::{
    RefineState, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_create, memseg_map, spawn_task,
};
use crate::ops;
use crate::pagewalk::{PAGE_SIZE, translate};
use crate::state::{
    ACCESS_R, ACCESS_W, ACCESS_X, CAP_EXC_SUBSCRIBE, CAP_SEG_WRITE, CapObject, Capability,
    ExceptionKind, ExcSourceId, PageMapping, SRET_FRAME_LEN, SretFrame, State, Task, TaskId,
    TaskStatus, USER_CS, USER_SS,
};

pub const PARENT: TaskId = 1;
pub const CHILD: TaskId = 2;
pub const COW_VA: u64 = 0x4000_0000;
pub const CODE_VA: u64 = 0x5000_0000;
pub const STACK_VA: u64 = 0x6000_0000; // 独立可写用户页：恢复帧 RSP 落点（与 COW 只读共享页解耦）
pub const CODE_SEG: u32 = 2; // frame 载体段：C 可写、内核可读
pub const FRAME_CAP: u32 = 12; // C 对 CODE_SEG 的 SegWrite capability

pub fn page_fault_source_id() -> ExcSourceId {
    1
}

/// 帧池 4 帧；P(1) 与 C(2) 已 spawn；数据段 seg=1（RW）、可执行段 seg=2（X）、栈段 seg=3（RW）。
pub fn new_refine() -> RefineState {
    let mut pf = RefineState::new(&[0x1000, 0x2000, 0x3000, 0x4000]).expect("valid frame universe");
    spawn_task(&mut pf, PARENT, CODE_VA).expect("spawn parent");
    spawn_task(&mut pf, CHILD, CODE_VA).expect("spawn child");
    let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
    // 数据段 seg=1（RW）；代码段 seg=2（X，兼作 frame 载体）；栈段 seg=3（RW，RSP 落点）
    let data_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("create data seg");
    assert_eq!(data_seg, 1);
    let code_seg = memseg_create(&mut pf, 1, ACCESS_X).expect("create code seg");
    assert_eq!(code_seg, 2);
    let stack_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("create stack seg");
    assert_eq!(stack_seg, 3);
    // 数据：父私写、子只读
    memseg_map(&mut pf, rights, true, data_seg, PARENT, 0, COW_VA, 1, ACCESS_R | ACCESS_W)
        .expect("map parent data RW");
    memseg_map(&mut pf, rights, true, data_seg, CHILD, 0, COW_VA, 1, ACCESS_R)
        .expect("map child data R");
    // 代码：父子都可执行（frame RIP 落点）
    memseg_map(&mut pf, rights, true, code_seg, PARENT, 0, CODE_VA, 1, ACCESS_X)
        .expect("map parent code X");
    memseg_map(&mut pf, rights, true, code_seg, CHILD, 0, CODE_VA, 1, ACCESS_X)
        .expect("map child code X");
    // 栈：父子都可写（frame RSP 落点，与 COW 只读共享页解耦）
    memseg_map(&mut pf, rights, true, stack_seg, PARENT, 0, STACK_VA, 1, ACCESS_R | ACCESS_W)
        .expect("map parent stack RW");
    memseg_map(&mut pf, rights, true, stack_seg, CHILD, 0, STACK_VA, 1, ACCESS_R | ACCESS_W)
        .expect("map child stack RW");
    pf
}

/// 构造一个合法恢复帧：RIP 在可执行 CODE_VA、RSP 在可写 STACK_VA（user canonical）。
pub fn resume_frame() -> SretFrame {
    SretFrame {
        rax: 0, rbx: 0, rcx: 0, rdx: 0, rsi: 0, rdi: 0, rbp: 0,
        rsp: STACK_VA + 0x80, r8: 0, r9: 0, r10: 0, r11: 0, r12: 0, r13: 0, r14: 0, r15: 0,
        rflags: 0x202, rip: CODE_VA + 0x80, cs: USER_CS, ss: USER_SS,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CowProbe { ReadOnly, Writable, NotMapped }

/// 纯查询该 vaddr 在 task 四级树中的可写观测（无副作用）。
pub fn probe_write(pf: &RefineState, task: TaskId, va: u64) -> CowProbe {
    let Some(tc) = pf.tasks.get(&task) else {
        return CowProbe::NotMapped;
    };
    let Ok(Some(pte)) = translate(&tc.table, va) else {
        return CowProbe::NotMapped;
    };
    if !pte.present || !pte.user {
        return CowProbe::NotMapped;
    }
    if pte.writable {
        CowProbe::Writable
    } else {
        CowProbe::ReadOnly
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WriteOutcome {
    Wrote,
    Faulted(ExceptionKind),
}

/// 模拟写指令：可写→Wrote；只读或未映射→暂停 task 并递送 page fault。
pub fn write_fault(exc: &mut State, pf: &mut RefineState, task: TaskId, va: u64) -> WriteOutcome {
    let kind = match probe_write(pf, task, va) {
        CowProbe::Writable => return WriteOutcome::Wrote,
        CowProbe::ReadOnly => ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        },
        CowProbe::NotMapped => ExceptionKind::PageFault {
            page_present: false,
            writable_fault: true,
        },
    };
    ops::schedule(exc, task); // 确保 task 处于可执行态（raise_exception 断言 Running）
    let save = resume_frame();
    ops::raise_exception(exc, task, page_fault_source_id(), kind, save);
    WriteOutcome::Faulted(kind)
}

/// 跨模型一致性断言：COW_VA 页在 RefineState 四级树中的可观测权限
/// 必须等于 ops 态中覆盖该页的 PageMapping 权限（且该 mapping 为 user）。
/// 仅检查 COW_VA 页；两模型任一缺失该 Task 即不一致。
pub fn cow_state_consistent(pf: &RefineState, exc: &State, task: TaskId) -> bool {
    let (Some(tc), Some(tk)) = (pf.tasks.get(&task), exc.tasks.get(&task)) else {
        return false;
    };
    let Some(pf_access) = tc.vspace.get(&COW_VA).map(|r| r.access) else {
        return false;
    };
    match tk.vspace.iter().find(|m| m.contains(COW_VA)) {
        Some(mapping) => mapping.user && mapping.access == pf_access,
        None => false,
    }
}

/// 异常源、capability、订阅、P/C task（Running）与 frame Segment(seg=2) 就绪的 ops 态。
pub fn base_exc() -> State {
    let mut exc = State {
        booted: true,
        ..State::default()
    };
    // COW 写故障：子只读共享页物理存在（page_present=true），写触发可写访问故障。
    // supported_kind 必须与 write_fault 对此类页实际递送的故障类型一致。
    exc.exc_sources.insert(
        page_fault_source_id(),
        crate::state::ExceptionSource {
            supported_kind: ExceptionKind::PageFault { page_present: true, writable_fault: true },
            active: true,
        },
    );
    exc.capabilities.insert(
        10,
        Capability {
            owner: CHILD,
            object: CapObject::ExceptionSource(page_fault_source_id()),
            rights: CAP_EXC_SUBSCRIBE,
            active: true,
        },
    );
    // frame 载体段 = 代码段 CODE_SEG；C 有写权
    exc.capabilities.insert(
        FRAME_CAP,
        Capability {
            owner: CHILD,
            object: CapObject::Segment(CODE_SEG),
            rights: CAP_SEG_WRITE,
            active: true,
        },
    );
    // 同时给 P 一个指向 / 无 frame 权（P 不参与 exc_resume，仅保持订阅）
    exc.segments.insert(
        CODE_SEG,
        crate::state::Segment {
            bytes: {
                let frame = resume_frame();
                let mut bytes = vec![0; SRET_FRAME_LEN];
                bytes.copy_from_slice(&frame.encode());
                bytes
            },
            kernel_readable: true,
        },
    );
    exc.subscriptions.insert((PARENT, page_fault_source_id()));
    exc.subscriptions.insert((CHILD, page_fault_source_id()));
    exc.tasks.insert(
        PARENT,
        Task {
            id: PARENT,
            status: TaskStatus::Running,
            vspace: vec![
                PageMapping { base: COW_VA, len: PAGE_SIZE, access: ACCESS_R | ACCESS_W, user: true },
                PageMapping { base: CODE_VA, len: PAGE_SIZE, access: ACCESS_X, user: true },
                PageMapping { base: STACK_VA, len: PAGE_SIZE, access: ACCESS_R | ACCESS_W, user: true },
            ],
            kernel_resume_frame: None,
        },
    );
    exc.tasks.insert(
        CHILD,
        Task {
            id: CHILD,
            status: TaskStatus::Running,
            vspace: vec![
                PageMapping { base: COW_VA, len: PAGE_SIZE, access: ACCESS_R, user: true },
                PageMapping { base: CODE_VA, len: PAGE_SIZE, access: ACCESS_X, user: true },
                PageMapping { base: STACK_VA, len: PAGE_SIZE, access: ACCESS_R | ACCESS_W, user: true },
            ],
            kernel_resume_frame: None,
        },
    );
    exc
}
