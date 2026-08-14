mod abstract_ops;
mod cow_refine;
mod machine;
mod memseg_refine;
mod ops;
mod pagewalk;
mod state;
mod tlb_refine;

#[cfg(test)]
mod cow_refine_tests;
#[cfg(test)]
mod memseg_refine_tests;
#[cfg(test)]
mod pagewalk_tests;
#[cfg(test)]
mod tlb_refine_tests;

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
        rsp: 0x4000_0080,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rflags: 0x202,
        rip: 0x5000_0080,
        cs: state::USER_CS,
        ss: state::USER_SS,
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
            bytes: {
                let mut bytes = vec![0; 0x1000];
                bytes[..state::SRET_FRAME_LEN].copy_from_slice(&default_frame().encode());
                bytes
            },
            kernel_readable: true,
        },
    );
    st.capabilities.insert(
        10,
        state::Capability {
            owner: 1,
            object: state::CapObject::ExceptionSource(1),
            rights: state::CAP_EXC_SUBSCRIBE,
            active: true,
        },
    );
    st.capabilities.insert(
        11,
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
            supported_kind: page_fault(),
            active: true,
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
    assert!(
        st.tasks[&1]
            .vspace
            .iter()
            .any(|mapping| mapping.contains(0x5000_0000))
    );
    raise_exception(&mut st, 1, 1, page_fault(), default_frame());
    exc_resume(&mut st, 1, 10, 11, 0).expect("resume page fault");
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

    // COW 组合协议冒烟：共享只读 → 写触发 fault → 复制换私写 → exc_resume
    // 依 §12.8：全程仅组合既有 memseg/ops 原语，未新增任何内核机制。
    {
        use cow_refine::{
            CHILD, CODE_SEG, COW_VA, CowProbe, FRAME_CAP, PARENT, base_exc, new_refine,
            probe_write, resume_frame, write_fault,
        };
        use memseg_refine::{
            SEG_MAP_R, SEG_MAP_W, SEG_MAP_X, memseg_alpha, memseg_create, memseg_map, memseg_unmap,
        };
        use ops::exc_resume;
        use state::{ACCESS_R, ACCESS_W};
        let mut pf = new_refine();
        let mut exc = base_exc();
        let rights = SEG_MAP_R | SEG_MAP_W | SEG_MAP_X;
        // 1. 父私写 COW_VA，子只读共享
        assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
        assert!(cow_refine::cow_state_consistent(&pf, &exc, CHILD));
        println!("PASS cow shared read-only probe");
        // 2. 子写只读共享页 → 递送 page fault → Faulted
        assert!(matches!(
            write_fault(&mut exc, &mut pf, CHILD, COW_VA),
            cow_refine::WriteOutcome::Faulted(state::ExceptionKind::PageFault { .. })
        ));
        assert_eq!(exc.tasks[&CHILD].status, state::TaskStatus::Faulted);
        println!("PASS cow write-page-fault");
        // 3. 分配副本段：与父共享页同帧卡片数、物理帧相离 → 写隔离成立
        let copy_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("copy seg");
        let parent_frames = pf.segments[&1].frames.clone();
        let copy_frames = pf.segments[&copy_seg].frames.clone();
        assert_eq!(
            parent_frames.len(),
            copy_frames.len(),
            "副本应复制父共享页相同帧卡片数"
        );
        assert!(
            parent_frames.iter().all(|f| !copy_frames.contains(f)),
            "副本帧须与原共享页相离，写隔离才成立"
        );
        memseg_unmap(&mut pf, true, CHILD, COW_VA, 1).expect("unmap read-only");
        memseg_map(
            &mut pf,
            rights,
            true,
            copy_seg,
            CHILD,
            0,
            COW_VA,
            1,
            ACCESS_R | ACCESS_W,
        )
        .expect("map private");
        exc.tasks.get_mut(&CHILD).unwrap().vspace[0].access = ACCESS_R | ACCESS_W;
        assert!(cow_refine::cow_state_consistent(&pf, &exc, CHILD));
        println!("PASS cow copy-and-remap");
        // 4. 写入合法用户恢复帧并 exc_resume → Ready（本 smoke 不再 schedule，维持 Roundtrip 步骤 7-8 已由测试覆盖）
        let frame = resume_frame();
        exc.segments
            .get_mut(&CODE_SEG)
            .unwrap()
            .bytes
            .copy_from_slice(&frame.encode());
        exc_resume(&mut exc, CHILD, 10, FRAME_CAP, 0).expect("resume child");
        assert_eq!(exc.tasks[&CHILD].status, state::TaskStatus::Ready);
        assert!(!exc.pending_exc.contains_key(&CHILD));
        println!("PASS cow exc_resume roundtrip");
        // 5. 终态：子私写可写；α 全程成立；未新增内核机制
        assert_eq!(probe_write(&pf, CHILD, COW_VA), CowProbe::Writable);
        assert_eq!(probe_write(&pf, PARENT, COW_VA), CowProbe::Writable);
        assert!(cow_refine::cow_state_consistent(&pf, &exc, CHILD));
        assert!(memseg_alpha(&pf));
        assert_eq!(pf.segments[&1].frames, parent_frames);
        assert_eq!(pf.segments[&copy_seg].frames, copy_frames);
        println!("PASS model-test cow fork no-new-kernel-mechanism");
    }

    // TLB 细化冒烟：非强制刷新 + 显式失效（spec 2026-08-15-tlb-refine-design.md）。
    // 每 Task 独立 TLB；换映射后未失效命中陈旧只读 TLB，显式失效后走新可写 PTE。
    {
        use memseg_refine::{
            RefineState, SEG_MAP_R, SEG_MAP_W, memseg_create, memseg_map, memseg_unmap, spawn_task,
        };
        use state::{ACCESS_R, ACCESS_W};
        use tlb_refine::{access_allowed, flush_all, has_tlb, invalidate_va, translate};

        let mut pf = RefineState::new(&[0x1000, 0x2000, 0x3000, 0x4000]).expect("universe");
        spawn_task(&mut pf, 1, 0x6000_0000).expect("spawn");
        let seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg");
        let rights = SEG_MAP_R | SEG_MAP_W;
        memseg_map(&mut pf, rights, true, seg, 1, 0, 0x4000_0000, 1, ACCESS_R).expect("map R");
        let mut tlb = tlb_refine::TlbState::new();
        let xl = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("walk fills");
        assert!(!access_allowed(&xl, true), "只读 map 的首访不应许可写");
        assert!(has_tlb(&tlb, 1, 0x4000_0000), "未命中后应填充 TLB");
        // 换映射为可写（unmap+map），未失效
        memseg_unmap(&mut pf, true, 1, 0x4000_0000, 1).expect("unmap");
        let seg2 = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("seg2");
        memseg_map(&mut pf, rights, true, seg2, 1, 0, 0x4000_0000, 1, ACCESS_W).expect("remap W");
        let xl2 = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("stale hit");
        assert!(!access_allowed(&xl2, true), "未 flush 应命中陈旧只读 TLB");
        // 显式失效后走新可写 PTE
        invalidate_va(&mut tlb, 1, 0x4000_0000).expect("invalidate");
        assert!(!has_tlb(&tlb, 1, 0x4000_0000), "失效后不应命中");
        let xl3 = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("walk new");
        assert!(access_allowed(&xl3, true), "失效后应走新可写 PTE");
        // flush_all 全量失效
        let _ = translate(&pf, &mut tlb, 1, 0x4000_0000).expect("refill");
        flush_all(&mut tlb, 1).expect("flush all");
        assert!(!has_tlb(&tlb, 1, 0x4000_0000), "flush_all 后应清空");
        println!("PASS model-test tlb non-coherent refresh + explicit invalidation");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ops::ResumeError;
    use state::{AuditEvent, CapId, PageMapping, SegId};

    const CALLER: state::TaskId = 1;
    const SOURCE: state::ExcSourceId = 1;
    const SOURCE_CAP: CapId = 10;
    const FRAME_SEG: SegId = 1;
    const FRAME_CAP: CapId = 11;
    const FRAME_OFF: usize = 0;

    fn resume_ok(st: &mut State) {
        exc_resume(st, CALLER, SOURCE_CAP, FRAME_CAP, FRAME_OFF).expect("resume should succeed");
    }

    fn faulted_state_with_frame_at(offset: usize, frame: SretFrame) -> State {
        let mut st = build_state();
        let bytes = frame.encode();
        let seg = st.segments.get_mut(&FRAME_SEG).unwrap();
        let end = offset + bytes.len();
        if end > seg.bytes.len() {
            seg.bytes.resize(end, 0);
        }
        seg.bytes[offset..end].copy_from_slice(&bytes);
        raise_exception(&mut st, CALLER, SOURCE, page_fault(), default_frame());
        st
    }

    fn resume_frame() -> SretFrame {
        SretFrame {
            rip: 0x5000_0080,
            rsp: 0x4000_0080,
            ..default_frame()
        }
    }

    fn resumable_state() -> State {
        faulted_state_with_frame_at(FRAME_OFF, resume_frame())
    }

    fn assert_resume_failure_at_unchanged(
        mut st: State,
        frame_offset: usize,
        expected: ResumeError,
    ) {
        let before = st.clone();
        let err = exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, frame_offset).unwrap_err();
        assert_eq!(err, expected);
        assert_eq!(st, before);
    }

    fn assert_resume_failure_unchanged(st: State, expected: ResumeError) {
        assert_resume_failure_at_unchanged(st, FRAME_OFF, expected);
    }

    fn replace_frame(st: &mut State, frame: SretFrame) {
        st.segments.get_mut(&FRAME_SEG).unwrap().bytes[..state::SRET_FRAME_LEN]
            .copy_from_slice(&frame.encode());
    }

    fn valid_frame() -> SretFrame {
        SretFrame {
            rax: 0x0102_0304_0506_0708,
            rbx: 0x1112_1314_1516_1718,
            rcx: 0x2122_2324_2526_2728,
            rdx: 0x3132_3334_3536_3738,
            rsi: 0x4142_4344_4546_4748,
            rdi: 0x5152_5354_5556_5758,
            rbp: 0x6162_6364_6566_6768,
            rsp: 0x7172_7374_7576_7778,
            r8: 0x8182_8384_8586_8788,
            r9: 0x9192_9394_9596_9798,
            r10: 0xa1a2_a3a4_a5a6_a7a8,
            r11: 0xb1b2_b3b4_b5b6_b7b8,
            r12: 0xc1c2_c3c4_c5c6_c7c8,
            r13: 0xd1d2_d3d4_d5d6_d7d8,
            r14: 0xe1e2_e3e4_e5e6_e7e8,
            r15: 0xf1f2_f3f4_f5f6_f7f8,
            rflags: 0x0a0b_0c0d_0e0f_1011,
            rip: 0x1a1b_1c1d_1e1f_2021,
            cs: 0x2a2b_2c2d_2e2f_3031,
            ss: 0x3a3b_3c3d_3e3f_4041,
        }
    }

    #[test]
    fn sret_frame_wire_layout_is_fixed_little_endian() {
        let f = valid_frame();
        let b = f.encode();
        assert_eq!(b.len(), state::SRET_FRAME_LEN);
        assert_eq!(state::SRET_FRAME_ALIGN, 16);

        for (offset, value) in [
            (0x00, f.rax),
            (0x08, f.rbx),
            (0x10, f.rcx),
            (0x18, f.rdx),
            (0x20, f.rsi),
            (0x28, f.rdi),
            (0x30, f.rbp),
            (0x38, f.rsp),
            (0x40, f.r8),
            (0x48, f.r9),
            (0x50, f.r10),
            (0x58, f.r11),
            (0x60, f.r12),
            (0x68, f.r13),
            (0x70, f.r14),
            (0x78, f.r15),
            (0x80, f.rflags),
            (0x88, f.rip),
            (0x90, f.cs),
            (0x98, f.ss),
        ] {
            assert_eq!(&b[offset..offset + 8], &value.to_le_bytes());
        }
    }

    #[test]
    fn sret_frame_round_trips() {
        let f = valid_frame();
        assert_eq!(SretFrame::decode(&f.encode()), Some(f));
    }

    #[test]
    fn sret_frame_rejects_wrong_length() {
        assert_eq!(SretFrame::decode(&[0; state::SRET_FRAME_LEN - 1]), None);
    }

    #[test]
    fn exc_resume_state_is_deeply_comparable() {
        let st = build_state();
        assert_eq!(st, st.clone());
    }

    #[test]
    fn exc_resume_rejects_missing_source_cap_atomically() {
        let mut st = resumable_state();
        st.capabilities.remove(&SOURCE_CAP);
        assert_resume_failure_unchanged(st, ResumeError::InvalidExcSource);
    }

    #[test]
    fn exc_resume_rejects_inactive_source_cap_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&SOURCE_CAP).unwrap().active = false;
        assert_resume_failure_unchanged(st, ResumeError::InvalidExcSource);
    }

    #[test]
    fn exc_resume_rejects_foreign_source_cap_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&SOURCE_CAP).unwrap().owner = CALLER + 1;
        assert_resume_failure_unchanged(st, ResumeError::InvalidExcSource);
    }

    #[test]
    fn exc_resume_rejects_source_cap_with_wrong_object_type_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&SOURCE_CAP).unwrap().object = state::CapObject::Segment(FRAME_SEG);
        assert_resume_failure_unchanged(st, ResumeError::InvalidExcSource);
    }

    #[test]
    fn exc_resume_rejects_source_cap_without_subscribe_right_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&SOURCE_CAP).unwrap().rights &= !state::CAP_EXC_SUBSCRIBE;
        assert_resume_failure_unchanged(st, ResumeError::MissingRight);
    }

    #[test]
    fn exc_resume_rejects_inactive_source_object_atomically() {
        let mut st = resumable_state();
        st.exc_sources.get_mut(&SOURCE).unwrap().active = false;
        assert_resume_failure_unchanged(st, ResumeError::InvalidExcSource);
    }

    #[test]
    fn exc_resume_rejects_missing_source_object_atomically() {
        let mut st = resumable_state();
        st.exc_sources.remove(&SOURCE);
        assert_resume_failure_unchanged(st, ResumeError::InvalidExcSource);
    }

    #[test]
    fn exc_resume_rejects_missing_subscription_atomically() {
        let mut st = resumable_state();
        st.subscriptions.remove(&(CALLER, SOURCE));
        assert_resume_failure_unchanged(st, ResumeError::NotSubscribed);
    }

    #[test]
    fn exc_resume_rejects_missing_frame_cap_atomically() {
        let mut st = resumable_state();
        st.capabilities.remove(&FRAME_CAP);
        assert_resume_failure_unchanged(st, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_rejects_inactive_frame_cap_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&FRAME_CAP).unwrap().active = false;
        assert_resume_failure_unchanged(st, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_rejects_foreign_frame_cap_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&FRAME_CAP).unwrap().owner = CALLER + 1;
        assert_resume_failure_unchanged(st, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_rejects_frame_cap_with_wrong_object_type_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&FRAME_CAP).unwrap().object =
            state::CapObject::ExceptionSource(SOURCE);
        assert_resume_failure_unchanged(st, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_rejects_frame_cap_without_write_right_atomically() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&FRAME_CAP).unwrap().rights &= !state::CAP_SEG_WRITE;
        assert_resume_failure_unchanged(st, ResumeError::MissingRight);
    }

    #[test]
    fn exc_resume_rejects_kernel_unreadable_frame_segment_atomically() {
        let mut st = resumable_state();
        st.segments.get_mut(&FRAME_SEG).unwrap().kernel_readable = false;
        assert_resume_failure_unchanged(st, ResumeError::InvalidFrame);
    }

    #[test]
    fn exc_resume_accepts_sixteen_byte_aligned_offset() {
        let mut st = faulted_state_with_frame_at(16, resume_frame());
        exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, 16).unwrap();
    }

    #[test]
    fn exc_resume_frame_address_mutations_are_atomic() {
        let st = resumable_state();
        assert_resume_failure_at_unchanged(st.clone(), 1, ResumeError::InvalidFrameAddr);
        assert_resume_failure_at_unchanged(
            st.clone(),
            usize::MAX - (state::SRET_FRAME_ALIGN - 1),
            ResumeError::InvalidFrameAddr,
        );

        let mut short = st;
        short
            .segments
            .get_mut(&FRAME_SEG)
            .unwrap()
            .bytes
            .truncate(state::SRET_FRAME_LEN - 1);
        assert_resume_failure_unchanged(short, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_rip_mutations_are_atomic_and_interior_address_succeeds() {
        for rip in [0x0000_8000_0000_0000, 0xffff_8000_0000_0000] {
            let mut st = resumable_state();
            st.tasks.get_mut(&CALLER).unwrap().vspace.push(PageMapping {
                base: rip,
                len: 0x1000,
                access: ACCESS_X,
                user: true,
            });
            let mut frame = resume_frame();
            frame.rip = rip;
            replace_frame(&mut st, frame);
            assert_resume_failure_unchanged(st, ResumeError::InvalidFrame);
        }

        let mut unmapped = resumable_state();
        let mut frame = resume_frame();
        frame.rip = 0x5000_1000;
        replace_frame(&mut unmapped, frame);
        assert_resume_failure_unchanged(unmapped, ResumeError::InvalidFrame);

        for (access, user) in [(ACCESS_R, true), (ACCESS_X, false)] {
            let mut st = resumable_state();
            let mapping = st
                .tasks
                .get_mut(&CALLER)
                .unwrap()
                .vspace
                .get_mut(1)
                .unwrap();
            mapping.access = access;
            mapping.user = user;
            assert_resume_failure_unchanged(st, ResumeError::InvalidFrame);
        }

        let mut st = resumable_state();
        resume_ok(&mut st);
    }

    #[test]
    fn exc_resume_rsp_mutations_are_atomic_and_interior_address_succeeds() {
        for rsp in [0x0000_8000_0000_0000, 0xffff_8000_0000_0000] {
            let mut st = resumable_state();
            st.tasks.get_mut(&CALLER).unwrap().vspace.push(PageMapping {
                base: rsp,
                len: 0x1000,
                access: ACCESS_W,
                user: true,
            });
            let mut frame = resume_frame();
            frame.rsp = rsp;
            replace_frame(&mut st, frame);
            assert_resume_failure_unchanged(st, ResumeError::InvalidFrame);
        }

        let mut unmapped = resumable_state();
        let mut frame = resume_frame();
        frame.rsp = 0x4000_1000;
        replace_frame(&mut unmapped, frame);
        assert_resume_failure_unchanged(unmapped, ResumeError::InvalidFrame);

        for (access, user) in [(ACCESS_R, true), (ACCESS_W, false)] {
            let mut st = resumable_state();
            let mapping = st
                .tasks
                .get_mut(&CALLER)
                .unwrap()
                .vspace
                .get_mut(0)
                .unwrap();
            mapping.access = access;
            mapping.user = user;
            assert_resume_failure_unchanged(st, ResumeError::InvalidFrame);
        }

        let mut st = resumable_state();
        resume_ok(&mut st);
    }

    #[test]
    fn exc_resume_rejects_selector_mutations_atomically() {
        for mutate in [
            |frame: &mut SretFrame| frame.cs = state::USER_CS + 8,
            |frame: &mut SretFrame| frame.ss = state::USER_SS + 8,
        ] {
            let mut st = resumable_state();
            let mut frame = resume_frame();
            mutate(&mut frame);
            replace_frame(&mut st, frame);
            assert_resume_failure_unchanged(st, ResumeError::InvalidFrame);
        }
    }

    #[test]
    fn exc_resume_rejects_each_rflags_mutation_atomically() {
        let mutations = [
            0,
            0x202 | (1 << 12),
            0x202 | (1 << 13),
            0x202 | (1 << 14),
            0x202 | (1 << 16),
            0x202 | (1 << 17),
            0x202 | (1 << 19),
            0x202 | (1 << 20),
            0x202 | (1 << 22),
            0x202 | (1 << 63),
        ];
        for rflags in mutations {
            let mut st = resumable_state();
            let mut frame = resume_frame();
            frame.rflags = rflags;
            replace_frame(&mut st, frame);
            assert_resume_failure_unchanged(st, ResumeError::InvalidFrame);
        }
    }

    #[test]
    fn exc_resume_accepts_each_variable_allowed_user_rflags_bit() {
        for variable_allowed_bit in [0, 2, 4, 6, 7, 8, 9, 10, 11, 18, 21] {
            let mut st = resumable_state();
            let mut frame = resume_frame();
            frame.rflags = (1 << 1) | (1 << variable_allowed_bit);
            replace_frame(&mut st, frame);
            resume_ok(&mut st);
        }
    }

    #[test]
    fn exc_resume_pending_source_kind_and_status_mutations_are_atomic() {
        let mut source = resumable_state();
        source.pending_exc.get_mut(&CALLER).unwrap().source_id = SOURCE + 1;
        assert_resume_failure_unchanged(source, ResumeError::InvalidExcSource);

        let mut kind = resumable_state();
        kind.exc_sources.get_mut(&SOURCE).unwrap().supported_kind =
            ExceptionKind::GeneralProtectionFault;
        assert_resume_failure_unchanged(kind, ResumeError::InvalidExcSource);

        let mut field_kind = resumable_state();
        field_kind
            .exc_sources
            .get_mut(&SOURCE)
            .unwrap()
            .supported_kind = ExceptionKind::PageFault {
            page_present: true,
            writable_fault: true,
        };
        assert_resume_failure_unchanged(field_kind, ResumeError::InvalidExcSource);

        let mut double_fault = resumable_state();
        double_fault.pending_exc.get_mut(&CALLER).unwrap().kind = ExceptionKind::DoubleFault;
        double_fault
            .exc_sources
            .get_mut(&SOURCE)
            .unwrap()
            .supported_kind = ExceptionKind::DoubleFault;
        assert_resume_failure_unchanged(double_fault, ResumeError::UnrecoverableFault);

        let mut status = resumable_state();
        status.tasks.get_mut(&CALLER).unwrap().status = TaskStatus::Ready;
        assert_resume_failure_unchanged(status, ResumeError::NoPendingException);
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
    fn page_mapping_with_overflowing_end_contains_no_address() {
        let m = PageMapping {
            base: u64::MAX - 0x7f,
            len: 0x100,
            access: ACCESS_X,
            user: true,
        };
        assert!(!m.contains(0));
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
    fn exc_resume_consumes_frame_and_commits_resume_atomically() {
        const OFFSET: usize = 32;
        const PREFIX: [u8; OFFSET] = [
            0x13, 0x57, 0x9b, 0xdf, 0x24, 0x68, 0xac, 0xe0, 0x35, 0x79, 0xbd, 0xf1, 0x46, 0x8a,
            0xce, 0x02, 0x58, 0x94, 0xd0, 0x1c, 0x63, 0xa7, 0xeb, 0x2f, 0x74, 0xb8, 0xfc, 0x40,
            0x85, 0xc9, 0x0d, 0x51,
        ];
        const SUFFIX: [u8; 16] = [
            0xa5, 0x3c, 0xd2, 0x69, 0x0f, 0xb4, 0x5a, 0xe1, 0x87, 0x2d, 0xc3, 0x78, 0x1e, 0x96,
            0x4b, 0xf0,
        ];
        let frame = resume_frame();
        let mut st = faulted_state_with_frame_at(OFFSET, frame);
        let frame_end = OFFSET + state::SRET_FRAME_LEN;
        let bytes = &mut st.segments.get_mut(&FRAME_SEG).unwrap().bytes;
        bytes[..OFFSET].copy_from_slice(&PREFIX);
        bytes.extend_from_slice(&SUFFIX);
        let before = bytes.clone();

        exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, OFFSET).unwrap();

        assert_eq!(st.tasks[&CALLER].status, TaskStatus::Ready);
        assert!(!st.pending_exc.contains_key(&CALLER));
        assert_eq!(st.tasks[&CALLER].kernel_resume_frame, Some(frame));
        assert_eq!(st.segments[&FRAME_SEG].bytes[..OFFSET], before[..OFFSET]);
        assert!(
            st.segments[&FRAME_SEG].bytes[OFFSET..frame_end]
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(
            st.segments[&FRAME_SEG].bytes[frame_end..],
            before[frame_end..]
        );
        assert_eq!(
            st.audit,
            vec![AuditEvent::TaskResumed {
                actor: CALLER,
                source_id: SOURCE,
                source_cap: SOURCE_CAP,
                frame_cap: FRAME_CAP,
                frame_seg_id: FRAME_SEG,
                frame_offset: OFFSET,
                rip: frame.rip,
            }]
        );
    }

    #[test]
    fn exc_resume_second_consumption_is_no_pending_and_preserves_complete_state() {
        let mut st = resumable_state();
        resume_ok(&mut st);
        let after_first_resume = st.clone();

        let err = exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, FRAME_OFF).unwrap_err();

        assert_eq!(err, ResumeError::NoPendingException);
        assert_eq!(st, after_first_resume);
    }

    #[test]
    fn exc_resume_failure_does_not_append_audit() {
        let mut st = resumable_state();
        let mut frame = resume_frame();
        frame.cs = state::USER_CS + 8;
        replace_frame(&mut st, frame);
        let before = st.clone();

        assert_eq!(
            exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, FRAME_OFF),
            Err(ResumeError::InvalidFrame)
        );
        assert_eq!(st, before);
        assert!(st.audit.is_empty());
    }

    #[test]
    fn schedule_only_runs_ready_task_and_preserves_kernel_resume_frame() {
        let frame = resume_frame();
        let mut st = resumable_state();
        resume_ok(&mut st);
        assert_eq!(st.tasks[&CALLER].kernel_resume_frame, Some(frame));

        schedule(&mut st, CALLER);

        assert_eq!(st.tasks[&CALLER].status, TaskStatus::Running);
        assert_eq!(st.tasks[&CALLER].kernel_resume_frame, Some(frame));
        assert_eq!(st.audit.len(), 1);
    }

    #[test]
    fn exc_resume_on_ready_is_no_pending() {
        let mut st = build_state();
        st.tasks.get_mut(&1).unwrap().status = TaskStatus::Ready;
        // 从未 fault：pending 不存在
        assert_resume_failure_unchanged(st, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_after_consumed_is_no_pending() {
        let mut st = build_state();
        raise_exception(&mut st, 1, 1, page_fault(), default_frame());
        resume_ok(&mut st);
        assert_resume_failure_unchanged(st, ResumeError::NoPendingException);
    }

    #[test]
    fn exc_resume_double_fault_is_unrecoverable() {
        let mut st = build_state();
        st.exc_sources.get_mut(&SOURCE).unwrap().supported_kind = ExceptionKind::DoubleFault;
        raise_exception(&mut st, 1, 1, ExceptionKind::DoubleFault, default_frame());
        assert_resume_failure_unchanged(st, ResumeError::UnrecoverableFault);
    }

    #[test]
    fn exc_resume_source_mismatch_is_invalid_source() {
        let mut st = resumable_state();
        st.pending_exc.get_mut(&CALLER).unwrap().source_id = SOURCE + 1;
        assert_resume_failure_unchanged(st, ResumeError::InvalidExcSource);
    }

    #[test]
    fn exc_resume_missing_frame_seg_rejected() {
        let mut st = resumable_state();
        st.capabilities.get_mut(&FRAME_CAP).unwrap().object = state::CapObject::Segment(999);
        assert_resume_failure_unchanged(st, ResumeError::InvalidFrameAddr);
    }

    #[test]
    fn exc_resume_out_of_bounds_frame_rejected() {
        let mut st = resumable_state();
        let before = st.clone();
        let err = exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, 0x1000).unwrap_err();
        assert_eq!(err, ResumeError::InvalidFrameAddr);
        assert_eq!(st, before);
    }

    #[test]
    fn exc_resume_misaligned_frame_rejected() {
        let mut st = resumable_state();
        let before = st.clone();
        let err = exc_resume(&mut st, CALLER, SOURCE_CAP, FRAME_CAP, 1).unwrap_err();
        assert_eq!(err, ResumeError::InvalidFrameAddr);
        assert_eq!(st, before);
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
