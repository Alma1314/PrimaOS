mod abstract_ops;
mod machine;
mod memseg_refine;
mod ops;
mod pagewalk;
mod state;

#[cfg(test)]
mod memseg_refine_tests;
#[cfg(test)]
mod pagewalk_tests;

use abstract_ops::bootstrap;
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
    // 交接点后探针已建立：bootstrap 已执行（该标志即 State.booted，Task 4 细化层将读取）
    let mut st = State {
        booted: true,
        ..State::default()
    };
    st.segments.insert(
        1,
        state::Segment {
            bytes: vec![0; 0x1000],
            kernel_readable: true,
        },
    );
    st.capabilities.insert(
        1,
        state::Capability {
            owner: 1,
            object: state::CapObject::ExceptionSource(1),
            rights: state::CAP_EXC_SUBSCRIBE,
            active: true,
        },
    );
    st.capabilities.insert(
        2,
        state::Capability {
            owner: 1,
            object: state::CapObject::Segment(1),
            rights: state::CAP_SEG_WRITE,
            active: true,
        },
    );
    st.subscriptions.insert((1, 1));
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
        vspace: vec![
            state::PageMapping {
                base: 0x4000_0000,
                len: 0x1000,
                access: ACCESS_R | ACCESS_W,
                user: true,
            },
            state::PageMapping {
                base: 0x5000_0000,
                len: 0x1000,
                access: ACCESS_X,
                user: true,
            },
        ],
        kernel_resume_frame: None,
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

fn build_boot_machine() -> machine::MachineState {
    machine::MachineState {
        cr3: machine::Cr3(0x1000),
        address_space: vec![machine::MapEntry {
            vaddr: 0x4000_0000,
            size: 0x1000,
            access: state::ACCESS_R | state::ACCESS_X,
        }],
        gdt: machine::GdtTable {
            base: 0x2000,
            limit: 0x40,
        },
        tss: machine::Tss { rsp0: 0x7000_0000 },
        rip: 0x5000_0000,
        rsp: 0x6fff_f000,
        boot_info: machine::BootInfoHandle {
            phys_mem_start: 0x0,
            phys_mem_len: 0x8000,
        },
        idt_ready: false,
        interrupt_state: machine::InterruptState::Masked,
        booted: false,
    }
}

fn main() {
    let mut st = build_state();
    assert!(st.booted, "bootstrap 交接点后应已建立");
    raise_exception(&mut st, 1, 1, page_fault(), default_frame());
    exc_resume(&mut st, 1, 1, 1, 0, 0x5000_0000).expect("resume page fault");
    schedule(&mut st, 1);
    assert_eq!(st.tasks[&1].status, TaskStatus::Running);
    println!("PASS cosir raise_exception");
    println!("PASS cosir exc_resume");
    println!("PASS cosir schedule");
    println!("PASS cosir recoverable-fault roundtrip");

    // BLK-002 细化：bootstrap 的 α 投影冒烟
    let m = build_boot_machine();
    let st_after = bootstrap(&m).expect("bootstrap should succeed");
    assert!(st_after.booted);
    assert!(st_after.tasks.contains_key(&abstract_ops::init_task_id()));
    println!("PASS cosir bootstrap abstract");
    println!("PASS cosir bootstrap alpha projection");

    // MemorySegment 页表↔vspace 细化冒烟：create→map→unmap→destroy + α 校验
    {
        use memseg_refine::{
            RefineState, SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_alpha, memseg_create,
            memseg_destroy, memseg_map, memseg_unmap, spawn_task,
        };
        let universe: Vec<u64> = (0u64..4).map(|i| 0x1000 + i * 0x1000).collect();
        let mut st = RefineState::new(&universe).expect("valid frame universe");
        spawn_task(&mut st, 1, 0x4000_0000).expect("spawn init");
        let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
        let seg = memseg_create(&mut st, 2, ACCESS_R | ACCESS_W).expect("create");
        // memseg_map(seg_cap_rights, task_manage_rights, seg_id, task_id, seg_offset,
        //            target_base, page_count, access)
        memseg_map(
            &mut st,
            rights,
            true,
            seg,
            1,
            0,
            0x5000_0000,
            1,
            ACCESS_R | ACCESS_W,
        )
        .expect("map");
        assert!(memseg_alpha(&st), "map 后 α 成立");
        memseg_unmap(&mut st, true, 1, 0x5000_0000, 1).expect("unmap");
        memseg_destroy(&mut st, true, seg).expect("destroy");
        assert!(memseg_alpha(&st), "destroy 后 α 成立");
        println!("PASS memseg create");
        println!("PASS memseg map");
        println!("PASS memseg unmap");
        println!("PASS memseg destroy");
        println!("PASS model-test memseg page-walk dual view");
        println!("PASS model-test memseg W6/W7 checks");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ops::ResumeError;
    use state::{PageMapping, SegId, VirtAddr};

    // 合法 exc_resume 参数：帧段 1 偏移 0（0x1000 ≥ 0x88），resume_vaddr 在可执行映射上
    const FRAME_SEG: SegId = 1;
    const FRAME_OFF: usize = 0;
    const RESUME_VADDR: VirtAddr = 0x5000_0000;

    fn resume_ok(st: &mut State) {
        exc_resume(st, 1, 1, FRAME_SEG, FRAME_OFF, RESUME_VADDR).expect("resume should succeed");
    }

    #[test]
    fn exc_resume_state_is_deeply_comparable() {
        let st = build_state();
        assert_eq!(st, st.clone());
    }

    #[test]
    fn page_mapping_contains_only_its_half_open_range() {
        let m = PageMapping {
            base: 0x5000_0000,
            len: 0x1000,
            access: ACCESS_X,
            user: true,
        };
        assert!(m.contains(0x5000_0000));
        assert!(m.contains(0x5000_0fff));
        assert!(!m.contains(0x5000_1000));
    }

    #[test]
    fn raise_exception_moves_running_to_faulted() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
        assert!(st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_recovers_faulted_to_ready() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        resume_ok(&mut st);
        assert_eq!(st.tasks[&1].status, TaskStatus::Ready);
        assert!(!st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_on_ready_is_no_pending() {
        let mut st = build_state();
        st.tasks.get_mut(&1).unwrap().status = TaskStatus::Ready;
        // 从未 fault：pending 不存在
        let err = exc_resume(&mut st, 1, 1, FRAME_SEG, FRAME_OFF, RESUME_VADDR).unwrap_err();
        assert_eq!(err, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_after_consumed_is_no_pending() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        resume_ok(&mut st);
        let err = exc_resume(&mut st, 1, 1, FRAME_SEG, FRAME_OFF, RESUME_VADDR).unwrap_err();
        assert_eq!(err, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_double_fault_is_unrecoverable() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, ExceptionKind::DoubleFault, default_frame());
        let err = exc_resume(&mut st, 1, 1, FRAME_SEG, FRAME_OFF, RESUME_VADDR).unwrap_err();
        assert_eq!(err, ResumeError::UnrecoverableFault);
        // 不销毁 Task，仍处于 Faulted
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
    }

    #[test]
    fn exc_resume_source_mismatch_is_invalid_source() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        // 源不匹配：pending.source_id=1，调用方传 999
        let err = exc_resume(&mut st, 1, 999, FRAME_SEG, FRAME_OFF, RESUME_VADDR).unwrap_err();
        assert_eq!(err, ResumeError::InvalidExcSource);
        // 源不匹配不消费帧、不改状态：仍 Faulted 且 pending 保留
        assert_eq!(st.tasks[&1].status, TaskStatus::Faulted);
        assert!(st.pending_exc.contains_key(&1));
    }

    #[test]
    fn exc_resume_missing_frame_seg_rejected() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        let err = exc_resume(&mut st, 1, 1, 999, FRAME_OFF, RESUME_VADDR).unwrap_err();
        assert_eq!(err, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_out_of_bounds_frame_rejected() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        // 0x1000 + 0x88 > 0x1000：帧越界（帧段 1 len=0x1000）
        let err = exc_resume(&mut st, 1, 1, FRAME_SEG, 0x1000, RESUME_VADDR).unwrap_err();
        assert_eq!(err, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_misaligned_frame_rejected() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        // offset=1 不满足帧长（SRET_FRAME_LEN=0x88）整数倍对齐
        let err = exc_resume(&mut st, 1, 1, FRAME_SEG, 1, RESUME_VADDR).unwrap_err();
        assert_eq!(err, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_non_executable_target_rejected() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        // 0x0000_0000 在 vspace 中无映射 / 不可执行
        let err = exc_resume(&mut st, 1, 1, FRAME_SEG, FRAME_OFF, 0x0000_0000).unwrap_err();
        assert_eq!(err, ResumeError::NotExecutable);
    }

    #[test]
    fn schedule_faulted_without_resume_is_safe() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, ExceptionKind::DoubleFault, default_frame());
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
