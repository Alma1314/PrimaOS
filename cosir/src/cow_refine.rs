#![allow(dead_code)]

use crate::memseg_refine::{
    MemSegError, RefineState, destroy_task_memory, destroy_verified, map_verified, memseg_create,
    spawn_task, unmap_verified,
};
use crate::ops::{self, ResumeError};
use crate::pagewalk::{PAGE_SIZE, translate, validate_page_vaddr};
use crate::state::{
    ACCESS_R, ACCESS_W, ACCESS_X, CAP_EXC_SUBSCRIBE, CAP_SEG_DESTROY, CAP_SEG_MAP_R, CAP_SEG_MAP_W,
    CAP_SEG_MAP_X, CAP_TASK_MANAGE_VSPACE, CapId, CapObject, Capability, ExcSourceId,
    ExceptionKind, PageMapping, PageMappingBacking, SRET_FRAME_LEN, SegId, SretFrame, State, Task,
    TaskId, TaskStatus, USER_CS, USER_SS, VirtAddr,
};
use crate::tlb_refine::{
    AccessKind, CachedXlat, access_allowed, invalidate_va, remove_task as tlb_remove_task,
    translate as tlb_translate,
};
use crate::tlb_refine::{TlbError, TlbState};

pub const PARENT: TaskId = 1;
pub const CHILD: TaskId = 2;
pub const COW_VA: u64 = 0x4000_0000;
pub const CODE_VA: u64 = 0x5000_0000;
pub const STACK_VA: u64 = 0x6000_0000; // 独立可写用户页：恢复帧 RSP 落点（与 COW 只读共享页解耦）
pub const CODE_SEG: u32 = 2; // frame 载体段：C 可写、内核可读
pub const FRAME_CAP: u32 = 12; // C 对 CODE_SEG 的 SegMapW authority
pub const DATA_SEG_CAP: CapId = 20;
pub const CHILD_TASK_CAP: CapId = 21;
pub const CHILD_SELF_TASK_CAP: CapId = 22;
pub const PARENT_SELF_TASK_CAP: CapId = 23;
pub const PARENT_EXC_CAP: CapId = 11;
pub const PARENT_FRAME_CAP: CapId = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CowMapping {
    parent: TaskId,
    child: TaskId,
    va: VirtAddr,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CowSliceState {
    memory: RefineState,
    ops: State,
    tlb: TlbState,
    current: Option<TaskId>,
    cow_mappings: Vec<CowMapping>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CowError {
    NoSuchTask,
    NoSuchSegment,
    NoSuchCapability,
    InactiveOrStaleCapability,
    CapabilityHolderMismatch,
    CapabilityObjectMismatch,
    CapabilityRightDenied,
    InvalidTaskState,
    CpuBusy,
    InvalidAddress,
    MappingConflict,
    TlbLifecycleInconsistent,
    CorruptCompositeState,
    MemSeg(MemSegError),
    Resume(ResumeError),
    Tlb(TlbError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessOutcome {
    Allowed(CachedXlat),
    Faulted(ExceptionKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MappingAuthority {
    seg_id: SegId,
    task_id: TaskId,
}

pub(crate) fn authorize_mapping(
    ops: &State,
    memory: &RefineState,
    actor: TaskId,
    seg_cap: CapId,
    task_cap: CapId,
    requested: u8,
) -> Result<MappingAuthority, CowError> {
    if requested == 0 || requested & !(ACCESS_R | ACCESS_W | ACCESS_X) != 0 {
        return Err(CowError::InvalidAddress);
    }

    let segment_cap = ops
        .capabilities
        .get(&seg_cap)
        .ok_or(CowError::NoSuchCapability)?;
    if !segment_cap.active {
        return Err(CowError::InactiveOrStaleCapability);
    }
    if !ops.held(actor, seg_cap) {
        return Err(CowError::CapabilityHolderMismatch);
    }
    let seg_id = match segment_cap.object {
        CapObject::Segment(seg_id) => seg_id,
        CapObject::ExceptionSource(_)
        | CapObject::Task(_)
        | CapObject::Namespace(_)
        | CapObject::Timer(_)
        | CapObject::Node(_)
        | CapObject::WaitObject(_) => {
            return Err(CowError::CapabilityObjectMismatch);
        }
    };
    let segment = memory
        .segments
        .get(&seg_id)
        .ok_or(CowError::InactiveOrStaleCapability)?;
    let mut required_segment_rights = 0;
    for (access, right) in [
        (ACCESS_R, CAP_SEG_MAP_R),
        (ACCESS_W, CAP_SEG_MAP_W),
        (ACCESS_X, CAP_SEG_MAP_X),
    ] {
        if requested & access != 0 {
            required_segment_rights |= right;
        }
    }
    if segment_cap.rights & required_segment_rights != required_segment_rights {
        return Err(CowError::CapabilityRightDenied);
    }

    let task_capability = ops
        .capabilities
        .get(&task_cap)
        .ok_or(CowError::NoSuchCapability)?;
    if !task_capability.active {
        return Err(CowError::InactiveOrStaleCapability);
    }
    if !ops.held(actor, task_cap) {
        return Err(CowError::CapabilityHolderMismatch);
    }
    let task_id = match task_capability.object {
        CapObject::Task(task_id) => task_id,
        CapObject::ExceptionSource(_)
        | CapObject::Segment(_)
        | CapObject::Namespace(_)
        | CapObject::Timer(_)
        | CapObject::Node(_)
        | CapObject::WaitObject(_) => {
            return Err(CowError::CapabilityObjectMismatch);
        }
    };
    if !ops.tasks.contains_key(&task_id) || !memory.tasks.contains_key(&task_id) {
        return Err(CowError::InactiveOrStaleCapability);
    }
    if task_capability.rights & CAP_TASK_MANAGE_VSPACE == 0 {
        return Err(CowError::CapabilityRightDenied);
    }
    if segment.allowed_access & requested != requested {
        return Err(CowError::CapabilityRightDenied);
    }

    Ok(MappingAuthority { seg_id, task_id })
}

fn authorize_task_management(
    ops: &State,
    actor: TaskId,
    cap_id: CapId,
) -> Result<TaskId, CowError> {
    let cap = ops
        .capabilities
        .get(&cap_id)
        .ok_or(CowError::NoSuchCapability)?;
    if !cap.active {
        return Err(CowError::InactiveOrStaleCapability);
    }
    if !ops.held(actor, cap_id) {
        return Err(CowError::CapabilityHolderMismatch);
    }
    let CapObject::Task(task) = cap.object else {
        return Err(CowError::CapabilityObjectMismatch);
    };
    if !ops.tasks.contains_key(&task) {
        return Err(CowError::InactiveOrStaleCapability);
    }
    if cap.rights & CAP_TASK_MANAGE_VSPACE == 0 {
        return Err(CowError::CapabilityRightDenied);
    }
    Ok(task)
}

fn authorize_segment_destroy(
    ops: &State,
    memory: &RefineState,
    actor: TaskId,
    cap_id: CapId,
) -> Result<SegId, CowError> {
    let cap = ops
        .capabilities
        .get(&cap_id)
        .ok_or(CowError::NoSuchCapability)?;
    if !cap.active {
        return Err(CowError::InactiveOrStaleCapability);
    }
    if !ops.held(actor, cap_id) {
        return Err(CowError::CapabilityHolderMismatch);
    }
    let CapObject::Segment(seg_id) = cap.object else {
        return Err(CowError::CapabilityObjectMismatch);
    };
    if !memory.segments.contains_key(&seg_id) {
        return Err(CowError::InactiveOrStaleCapability);
    }
    if cap.rights & CAP_SEG_DESTROY == 0 {
        return Err(CowError::CapabilityRightDenied);
    }
    Ok(seg_id)
}

impl CowSliceState {
    pub fn new_cow_fixture() -> Self {
        let mut memory = new_refine();
        unmap_verified(&mut memory, CHILD, COW_VA, 1).expect("unmap initial child COW page");

        let mut ops = base_exc();
        ops.tasks.get_mut(&CHILD).unwrap().status = TaskStatus::Ready;
        ops.tasks
            .get_mut(&CHILD)
            .unwrap()
            .vspace
            .retain(|mapping| !mapping.contains(COW_VA));
        ops.insert_capability(
            DATA_SEG_CAP,
            Capability {
                owner: PARENT,
                object: CapObject::Segment(1),
                rights: CAP_SEG_MAP_R | CAP_SEG_MAP_W | CAP_SEG_DESTROY,
                active: true,
                parent: None,
            },
        );
        ops.insert_capability(
            CHILD_TASK_CAP,
            Capability {
                owner: PARENT,
                object: CapObject::Task(CHILD),
                rights: CAP_TASK_MANAGE_VSPACE,
                active: true,
                parent: None,
            },
        );
        ops.insert_capability(
            CHILD_SELF_TASK_CAP,
            Capability {
                owner: CHILD,
                object: CapObject::Task(CHILD),
                rights: CAP_TASK_MANAGE_VSPACE,
                active: true,
                parent: None,
            },
        );
        ops.insert_capability(
            PARENT_SELF_TASK_CAP,
            Capability {
                owner: PARENT,
                object: CapObject::Task(PARENT),
                rights: CAP_TASK_MANAGE_VSPACE,
                active: true,
                parent: None,
            },
        );
        ops.insert_capability(
            PARENT_EXC_CAP,
            Capability {
                owner: PARENT,
                object: CapObject::ExceptionSource(page_fault_source_id()),
                rights: CAP_EXC_SUBSCRIBE,
                active: true,
                parent: None,
            },
        );
        ops.insert_capability(
            PARENT_FRAME_CAP,
            Capability {
                owner: PARENT,
                object: CapObject::Segment(CODE_SEG),
                rights: CAP_SEG_MAP_W,
                active: true,
                parent: None,
            },
        );
        for (task_id, task) in &mut ops.tasks {
            let memory_task = &memory.tasks[task_id];
            for mapping in &mut task.vspace {
                let segment_ref = memory_task.vspace[&mapping.base];
                let segment = &memory.segments[&segment_ref.segment];
                mapping.backing = Some(PageMappingBacking {
                    segment: segment_ref.segment,
                    offset: segment_ref.offset,
                    frame_phys: segment.frames[segment_ref.offset as usize],
                });
            }
        }

        let slice = Self {
            memory,
            ops,
            tlb: TlbState::new(),
            current: Some(PARENT),
            cow_mappings: Vec::new(),
        };
        slice
            .check_invariants()
            .expect("new COW fixture must satisfy composite invariants");
        slice
    }

    pub fn check_invariants(&self) -> Result<(), CowError> {
        if self
            .tlb
            .task_ids()
            .any(|task_id| !self.ops.tasks.contains_key(task_id))
        {
            return Err(CowError::TlbLifecycleInconsistent);
        }
        let memory_tasks: std::collections::BTreeSet<_> =
            self.memory.tasks.keys().copied().collect();
        let ops_tasks: std::collections::BTreeSet<_> = self.ops.tasks.keys().copied().collect();
        if memory_tasks != ops_tasks || !crate::memseg_refine::memseg_alpha(&self.memory) {
            return Err(CowError::CorruptCompositeState);
        }
        let running: Vec<_> = self
            .ops
            .tasks
            .iter()
            .filter_map(|(id, task)| (task.status == TaskStatus::Running).then_some(*id))
            .collect();
        if running.len() > 1
            || self.current != running.first().copied()
            || self
                .current
                .is_some_and(|id| !self.ops.tasks.contains_key(&id))
        {
            return Err(CowError::CorruptCompositeState);
        }

        for (task_id, task) in &self.ops.tasks {
            if task.id != *task_id {
                return Err(CowError::CorruptCompositeState);
            }
            let has_pending = self.ops.pending_exc.contains_key(task_id);
            if (task.status == TaskStatus::Faulted) != has_pending
                || (task.status == TaskStatus::Faulted && self.current == Some(*task_id))
            {
                return Err(CowError::CorruptCompositeState);
            }

            let memory_task = &self.memory.tasks[task_id];
            if task.vspace.len() != memory_task.vspace.len() {
                return Err(CowError::CorruptCompositeState);
            }
            let mut bases = std::collections::BTreeSet::new();
            for mapping in &task.vspace {
                if mapping.len != PAGE_SIZE || !mapping.user || !bases.insert(mapping.base) {
                    return Err(CowError::CorruptCompositeState);
                }
                let Some(backing) = mapping.backing else {
                    return Err(CowError::CorruptCompositeState);
                };
                let Some(segment_ref) = memory_task.vspace.get(&mapping.base) else {
                    return Err(CowError::CorruptCompositeState);
                };
                if segment_ref.segment != backing.segment
                    || segment_ref.offset != backing.offset
                    || segment_ref.access != mapping.access
                {
                    return Err(CowError::CorruptCompositeState);
                }
                let frame = usize::try_from(backing.offset).ok().and_then(|offset| {
                    self.memory
                        .segments
                        .get(&backing.segment)
                        .and_then(|segment| segment.frames.get(offset))
                });
                if frame != Some(&backing.frame_phys) {
                    return Err(CowError::CorruptCompositeState);
                }
                let pte = translate(&memory_task.table, mapping.base)
                    .ok()
                    .flatten()
                    .filter(|pte| pte.present && pte.user);
                if !pte.is_some_and(|pte| {
                    pte.frame_phys == backing.frame_phys
                        && pte.mapping.segment == backing.segment
                        && pte.mapping.offset == backing.offset
                        && pte.mapping.access == mapping.access
                }) {
                    return Err(CowError::CorruptCompositeState);
                }
            }
        }
        if self
            .ops
            .pending_exc
            .keys()
            .any(|task_id| !self.ops.tasks.contains_key(task_id))
            || self.ops.locations.values().any(|location| matches!(location, crate::state::CapLocation::HeldBy(t) if !self.ops.tasks.contains_key(t)))
            // Historical records and external stale Task capabilities survive
            // target death; operation authorization checks live targets.
            || self.ops.subscriptions.iter().any(|(task, source)| {
                !self.ops.tasks.contains_key(task)
                    || !self
                        .ops
                        .exc_sources
                        .get(source)
                        .is_some_and(|source| source.active)
            })
        {
            return Err(CowError::CorruptCompositeState);
        }

        for cow in &self.cow_mappings {
            let Some(parent) = self
                .memory
                .tasks
                .get(&cow.parent)
                .and_then(|task| task.vspace.get(&cow.va))
            else {
                return Err(CowError::CorruptCompositeState);
            };
            let Some(child) = self
                .memory
                .tasks
                .get(&cow.child)
                .and_then(|task| task.vspace.get(&cow.va))
            else {
                return Err(CowError::CorruptCompositeState);
            };
            if parent.segment == child.segment && parent.offset == child.offset {
                if parent.access & ACCESS_W != 0 || child.access & ACCESS_W != 0 {
                    return Err(CowError::CorruptCompositeState);
                }
            } else {
                let parent_segment = &self.memory.segments[&parent.segment];
                let child_segment = &self.memory.segments[&child.segment];
                if parent_segment.frames.len() != child_segment.frames.len()
                    || parent_segment
                        .frames
                        .iter()
                        .any(|frame| child_segment.frames.contains(frame))
                {
                    return Err(CowError::CorruptCompositeState);
                }
            }
        }
        Ok(())
    }

    pub fn establish_cow(
        &mut self,
        actor: TaskId,
        parent: TaskId,
        child: TaskId,
        seg_cap: CapId,
        child_task_cap: CapId,
        va: VirtAddr,
    ) -> Result<(), CowError> {
        self.check_invariants()?;
        validate_page_vaddr(va).map_err(|_| CowError::InvalidAddress)?;
        if !self.ops.tasks.contains_key(&actor)
            || !self.ops.tasks.contains_key(&parent)
            || !self.ops.tasks.contains_key(&child)
            || !self.memory.tasks.contains_key(&actor)
            || !self.memory.tasks.contains_key(&parent)
            || !self.memory.tasks.contains_key(&child)
        {
            return Err(CowError::NoSuchTask);
        }
        if actor != parent || parent == child {
            return Err(CowError::InvalidTaskState);
        }
        let authority = authorize_mapping(
            &self.ops,
            &self.memory,
            actor,
            seg_cap,
            child_task_cap,
            ACCESS_R,
        )?;
        if authority.task_id != child {
            return Err(CowError::CapabilityObjectMismatch);
        }
        let parent_ref = self.memory.tasks[&parent]
            .vspace
            .get(&va)
            .copied()
            .ok_or(CowError::InvalidAddress)?;
        if parent_ref.segment != authority.seg_id || parent_ref.access != (ACCESS_R | ACCESS_W) {
            return Err(CowError::MappingConflict);
        }
        let parent_frame =
            self.memory.segments[&parent_ref.segment].frames[parent_ref.offset as usize];
        if self.memory.tasks.iter().any(|(task_id, task)| {
            task.vspace.iter().any(|(mapped_va, mapped_ref)| {
                (*task_id != parent || *mapped_va != va)
                    && mapped_ref.segment == parent_ref.segment
                    && mapped_ref.offset == parent_ref.offset
                    && self.memory.segments[&mapped_ref.segment].frames[mapped_ref.offset as usize]
                        == parent_frame
            })
        }) {
            return Err(CowError::MappingConflict);
        }
        if self.memory.tasks[&child].vspace.contains_key(&va) {
            return Err(CowError::MappingConflict);
        }

        let mut pending = self.clone();
        map_verified(
            &mut pending.memory,
            parent_ref.segment,
            child,
            parent_ref.offset,
            va,
            1,
            ACCESS_R,
        )
        .map_err(CowError::MemSeg)?;
        unmap_verified(&mut pending.memory, parent, va, 1).map_err(CowError::MemSeg)?;
        map_verified(
            &mut pending.memory,
            parent_ref.segment,
            parent,
            parent_ref.offset,
            va,
            1,
            ACCESS_R,
        )
        .map_err(CowError::MemSeg)?;

        let frame_phys =
            pending.memory.segments[&parent_ref.segment].frames[parent_ref.offset as usize];
        let backing = PageMappingBacking {
            segment: parent_ref.segment,
            offset: parent_ref.offset,
            frame_phys,
        };
        let parent_mapping = pending
            .ops
            .tasks
            .get_mut(&parent)
            .unwrap()
            .vspace
            .iter_mut()
            .find(|mapping| mapping.contains(va))
            .ok_or(CowError::CorruptCompositeState)?;
        parent_mapping.access = ACCESS_R;
        parent_mapping.backing = Some(backing);
        pending
            .ops
            .tasks
            .get_mut(&child)
            .unwrap()
            .vspace
            .push(PageMapping {
                base: va,
                len: PAGE_SIZE,
                access: ACCESS_R,
                user: true,
                backing: Some(backing),
            });
        invalidate_va(&pending.ops.tasks, &mut pending.tlb, parent, va).map_err(CowError::Tlb)?;
        invalidate_va(&pending.ops.tasks, &mut pending.tlb, child, va).map_err(CowError::Tlb)?;
        pending.cow_mappings.push(CowMapping { parent, child, va });
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    pub fn access(
        &mut self,
        task: TaskId,
        va: VirtAddr,
        kind: AccessKind,
    ) -> Result<AccessOutcome, CowError> {
        self.check_invariants()?;
        let current = self.ops.tasks.get(&task).ok_or(CowError::NoSuchTask)?;
        if self.current != Some(task) || current.status != TaskStatus::Running {
            return Err(CowError::InvalidTaskState);
        }

        let mut pending = self.clone();
        let translation = match tlb_translate(
            &pending.memory,
            &pending.ops.tasks,
            &mut pending.tlb,
            task,
            va,
        ) {
            Ok(translation) => translation,
            Err(TlbError::NotMapped) => {
                let kind = ExceptionKind::PageFault {
                    page_present: false,
                    writable_fault: matches!(kind, AccessKind::Write),
                };
                ops::raise_exception(
                    &mut pending.ops,
                    task,
                    page_fault_source_id(),
                    kind,
                    Some(va),
                    resume_frame(),
                );
                pending.current = None;
                pending.check_invariants()?;
                *self = pending;
                return Ok(AccessOutcome::Faulted(kind));
            }
            Err(error) => return Err(CowError::Tlb(error)),
        };
        if access_allowed(&translation, kind) {
            *self = pending;
            return Ok(AccessOutcome::Allowed(translation));
        }

        let kind = ExceptionKind::PageFault {
            page_present: true,
            writable_fault: matches!(kind, AccessKind::Write),
        };
        ops::raise_exception(
            &mut pending.ops,
            task,
            page_fault_source_id(),
            kind,
            Some(va),
            resume_frame(),
        );
        pending.current = None;
        pending.check_invariants()?;
        *self = pending;
        Ok(AccessOutcome::Faulted(kind))
    }

    pub fn yield_current(&mut self, task: TaskId) -> Result<(), CowError> {
        self.check_invariants()?;
        let current = self.ops.tasks.get(&task).ok_or(CowError::NoSuchTask)?;
        if self.current != Some(task) || current.status != TaskStatus::Running {
            return Err(CowError::InvalidTaskState);
        }
        let mut pending = self.clone();
        pending.ops.tasks.get_mut(&task).unwrap().status = TaskStatus::Ready;
        pending.current = None;
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    pub fn schedule(&mut self, task: TaskId) -> Result<(), CowError> {
        self.check_invariants()?;
        if self.current.is_some() {
            return Err(CowError::CpuBusy);
        }
        let next = self.ops.tasks.get(&task).ok_or(CowError::NoSuchTask)?;
        if next.status != TaskStatus::Ready {
            return Err(CowError::InvalidTaskState);
        }
        let mut pending = self.clone();
        pending.ops.tasks.get_mut(&task).unwrap().status = TaskStatus::Running;
        pending.current = Some(task);
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    pub fn create_segment(
        &mut self,
        actor: TaskId,
        page_count: u64,
        allowed_access: u8,
    ) -> Result<CapId, CowError> {
        self.check_invariants()?;
        if !self.ops.tasks.contains_key(&actor) || !self.memory.tasks.contains_key(&actor) {
            return Err(CowError::NoSuchTask);
        }

        let mut pending = self.clone();
        let seg_id = memseg_create(&mut pending.memory, page_count, allowed_access)
            .map_err(CowError::MemSeg)?;
        let cap_id = pending
            .ops
            .capabilities
            .keys()
            .copied()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(CowError::CorruptCompositeState)?;
        let mut rights = CAP_SEG_DESTROY;
        for (access, right) in [
            (ACCESS_R, CAP_SEG_MAP_R),
            (ACCESS_W, CAP_SEG_MAP_W),
            (ACCESS_X, CAP_SEG_MAP_X),
        ] {
            if allowed_access & access != 0 {
                rights |= right;
            }
        }
        pending.ops.insert_capability(
            cap_id,
            Capability {
                owner: actor,
                object: CapObject::Segment(seg_id),
                rights,
                active: true,
                parent: None,
            },
        );
        pending.check_invariants()?;
        *self = pending;
        Ok(cap_id)
    }

    pub fn privatize_cow(
        &mut self,
        actor: TaskId,
        faulted_task: TaskId,
        private_seg_cap: CapId,
        task_cap: CapId,
        va: VirtAddr,
    ) -> Result<(), CowError> {
        self.check_invariants()?;
        validate_page_vaddr(va).map_err(|_| CowError::InvalidAddress)?;
        let faulted = self
            .ops
            .tasks
            .get(&faulted_task)
            .ok_or(CowError::NoSuchTask)?;
        if !self.memory.tasks.contains_key(&faulted_task) || actor != faulted_task {
            return Err(CowError::InvalidTaskState);
        }
        if faulted.status != TaskStatus::Faulted || self.current.is_some() {
            return Err(CowError::InvalidTaskState);
        }
        if !matches!(
            self.ops.pending_exc.get(&faulted_task),
            Some(crate::state::PendingException {
                source_id,
                kind: ExceptionKind::PageFault {
                    page_present: true,
                    writable_fault: true,
                },
                fault_va: Some(fault_va),
                ..
            }) if *source_id == page_fault_source_id() && *fault_va == va
        ) {
            return Err(CowError::InvalidTaskState);
        }
        let authority = authorize_mapping(
            &self.ops,
            &self.memory,
            actor,
            private_seg_cap,
            task_cap,
            ACCESS_R | ACCESS_W,
        )?;
        if authority.task_id != faulted_task {
            return Err(CowError::CapabilityObjectMismatch);
        }
        let old = self.memory.tasks[&faulted_task]
            .vspace
            .get(&va)
            .copied()
            .ok_or(CowError::InvalidAddress)?;
        if old.access != ACCESS_R
            || self.memory.segments[&authority.seg_id].frames.len()
                != self.memory.segments[&old.segment].frames.len()
        {
            return Err(CowError::MappingConflict);
        }

        let mut pending = self.clone();
        unmap_verified(&mut pending.memory, faulted_task, va, 1).map_err(CowError::MemSeg)?;
        map_verified(
            &mut pending.memory,
            authority.seg_id,
            faulted_task,
            0,
            va,
            1,
            ACCESS_R | ACCESS_W,
        )
        .map_err(CowError::MemSeg)?;
        let frame_phys = pending.memory.segments[&authority.seg_id].frames[0];
        let mapping = pending
            .ops
            .tasks
            .get_mut(&faulted_task)
            .unwrap()
            .vspace
            .iter_mut()
            .find(|mapping| mapping.contains(va))
            .ok_or(CowError::CorruptCompositeState)?;
        mapping.access = ACCESS_R | ACCESS_W;
        mapping.backing = Some(PageMappingBacking {
            segment: authority.seg_id,
            offset: 0,
            frame_phys,
        });
        invalidate_va(&pending.ops.tasks, &mut pending.tlb, faulted_task, va)
            .map_err(CowError::Tlb)?;
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    /// Write the fixture's valid user return frame through the caller-owned frame capability.
    /// The fixture always writes at offset zero, which must hold a complete SRET frame.
    pub fn write_valid_resume_frame_for_test(&mut self, caller: TaskId) -> Result<(), CowError> {
        self.check_invariants()?;
        let (_, frame_cap) = self
            .ops
            .capabilities
            .iter()
            .find(|(id, cap)| {
                self.ops.held(caller, **id)
                    && cap.object == CapObject::Segment(CODE_SEG)
                    && cap.rights & CAP_SEG_MAP_W != 0
            })
            .ok_or(CowError::NoSuchCapability)?;
        if !frame_cap.active {
            return Err(CowError::InactiveOrStaleCapability);
        }
        if frame_cap.object != CapObject::Segment(CODE_SEG) {
            return Err(CowError::CapabilityObjectMismatch);
        }
        if frame_cap.rights & CAP_SEG_MAP_W == 0 {
            return Err(CowError::CapabilityRightDenied);
        }

        let mut pending = self.clone();
        let bytes = &mut pending
            .ops
            .segments
            .get_mut(&CODE_SEG)
            .ok_or(CowError::NoSuchSegment)?
            .bytes;
        const FRAME_OFFSET: usize = 0;
        let end = FRAME_OFFSET
            .checked_add(SRET_FRAME_LEN)
            .filter(|end| *end <= bytes.len())
            .ok_or(CowError::InvalidAddress)?;
        bytes[FRAME_OFFSET..end].copy_from_slice(&resume_frame().encode());
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    pub fn resume(
        &mut self,
        caller: TaskId,
        exc_source_cap: CapId,
        frame_seg_cap: CapId,
        frame_offset: usize,
    ) -> Result<(), CowError> {
        self.check_invariants()?;
        let mut pending = self.clone();
        ops::exc_resume(
            &mut pending.ops,
            caller,
            exc_source_cap,
            frame_seg_cap,
            frame_offset,
        )
        .map_err(CowError::Resume)?;
        if pending.current.is_some() || pending.ops.tasks[&caller].status != TaskStatus::Ready {
            return Err(CowError::CorruptCompositeState);
        }
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    pub fn destroy_task(
        &mut self,
        actor: TaskId,
        task: TaskId,
        task_cap: CapId,
    ) -> Result<(), CowError> {
        self.check_invariants()?;
        let authority = authorize_task_management(&self.ops, actor, task_cap)?;
        if authority != task {
            return Err(CowError::CapabilityObjectMismatch);
        }
        let task_state = self.ops.tasks.get(&task).ok_or(CowError::NoSuchTask)?;
        if self.current == Some(task) || task_state.status == TaskStatus::Faulted {
            return Err(CowError::InvalidTaskState);
        }

        let mut pending = self.clone();
        destroy_task_memory(&mut pending.memory, task).map_err(CowError::MemSeg)?;
        crate::authority::retire_task(&mut pending.ops, task, crate::state::Actor::Task(actor));
        pending
            .ops
            .subscriptions
            .retain(|(subscriber, _)| *subscriber != task);
        pending
            .cow_mappings
            .retain(|cow| cow.parent != task && cow.child != task);
        tlb_remove_task(&mut pending.tlb, task);
        for capability in pending.ops.capabilities.values_mut() {
            if capability.object == CapObject::Task(task) {
                capability.active = false;
            }
        }
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    pub fn destroy_segment(&mut self, actor: TaskId, seg_cap: CapId) -> Result<(), CowError> {
        self.check_invariants()?;
        let seg_id = authorize_segment_destroy(&self.ops, &self.memory, actor, seg_cap)?;
        let mut pending = self.clone();
        destroy_verified(&mut pending.memory, seg_id).map_err(CowError::MemSeg)?;
        for capability in pending.ops.capabilities.values_mut() {
            if capability.object == CapObject::Segment(seg_id) {
                capability.active = false;
            }
        }
        pending.check_invariants()?;
        *self = pending;
        Ok(())
    }

    pub fn task_exists(&self, task: TaskId) -> bool {
        self.ops.tasks.contains_key(&task)
    }

    pub fn has_active_capability_owner(&self, task: TaskId) -> bool {
        self.ops
            .capabilities
            .iter()
            .any(|(&id, cap)| cap.active && self.ops.held(task, id))
    }

    pub fn has_subscription(&self, task: TaskId, source: ExcSourceId) -> bool {
        self.ops.subscriptions.contains(&(task, source))
    }

    pub fn cow_frames(&self, task: TaskId, va: VirtAddr) -> Result<Vec<u64>, CowError> {
        let mapping = self
            .memory
            .tasks
            .get(&task)
            .and_then(|task| task.vspace.get(&va))
            .ok_or(CowError::InvalidAddress)?;
        self.memory
            .segments
            .get(&mapping.segment)
            .map(|segment| segment.frames.clone())
            .ok_or(CowError::NoSuchSegment)
    }

    pub fn mapping(&self, task: TaskId, va: VirtAddr) -> Result<&PageMapping, CowError> {
        self.ops
            .tasks
            .get(&task)
            .and_then(|task| task.vspace.iter().find(|mapping| mapping.contains(va)))
            .ok_or(CowError::InvalidAddress)
    }

    pub fn tlb_has_task(&self, task: TaskId) -> bool {
        self.tlb.task_ids().any(|task_id| *task_id == task)
    }

    pub fn task_capabilities(&self, task: TaskId) -> impl Iterator<Item = &Capability> {
        self.ops
            .capabilities
            .values()
            .filter(move |cap| cap.object == CapObject::Task(task))
    }
}

pub fn page_fault_source_id() -> ExcSourceId {
    1
}

/// 帧池 24 帧；P(1) 与 C(2) 已 spawn；数据段 seg=1（RW）、可执行段 seg=2（X）、栈段 seg=3（RW）。
pub fn new_refine() -> RefineState {
    let mut pf = RefineState::new(&(0u64..24).map(|i| 0x1000 + i * 0x1000).collect::<Vec<_>>())
        .expect("valid frame universe");
    spawn_task(&mut pf, PARENT, CODE_VA).expect("spawn parent");
    spawn_task(&mut pf, CHILD, CODE_VA).expect("spawn child");
    // 数据段 seg=1（RW）；代码段 seg=2（X，兼作 frame 载体）；栈段 seg=3（RW，RSP 落点）
    let data_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("create data seg");
    assert_eq!(data_seg, 1);
    let code_seg = memseg_create(&mut pf, 1, ACCESS_X).expect("create code seg");
    assert_eq!(code_seg, 2);
    let stack_seg = memseg_create(&mut pf, 1, ACCESS_R | ACCESS_W).expect("create stack seg");
    assert_eq!(stack_seg, 3);
    // 数据：父私写、子只读
    map_verified(&mut pf, data_seg, PARENT, 0, COW_VA, 1, ACCESS_R | ACCESS_W)
        .expect("map parent data RW");
    map_verified(&mut pf, data_seg, CHILD, 0, COW_VA, 1, ACCESS_R).expect("map child data R");
    // 代码：父子都可执行（frame RIP 落点）
    map_verified(&mut pf, code_seg, PARENT, 0, CODE_VA, 1, ACCESS_X).expect("map parent code X");
    map_verified(&mut pf, code_seg, CHILD, 0, CODE_VA, 1, ACCESS_X).expect("map child code X");
    // 栈：父子都可写（frame RSP 落点，与 COW 只读共享页解耦）
    map_verified(
        &mut pf,
        stack_seg,
        PARENT,
        0,
        STACK_VA,
        1,
        ACCESS_R | ACCESS_W,
    )
    .expect("map parent stack RW");
    map_verified(
        &mut pf,
        stack_seg,
        CHILD,
        0,
        STACK_VA,
        1,
        ACCESS_R | ACCESS_W,
    )
    .expect("map child stack RW");
    pf
}

/// 构造一个合法恢复帧：RIP 在可执行 CODE_VA、RSP 在可写 STACK_VA（user canonical）。
pub fn resume_frame() -> SretFrame {
    SretFrame {
        rax: 0,
        rbx: 0,
        rcx: 0,
        rdx: 0,
        rsi: 0,
        rdi: 0,
        rbp: 0,
        rsp: STACK_VA + 0x80,
        r8: 0,
        r9: 0,
        r10: 0,
        r11: 0,
        r12: 0,
        r13: 0,
        r14: 0,
        r15: 0,
        rflags: 0x202,
        rip: CODE_VA + 0x80,
        cs: USER_CS,
        ss: USER_SS,
    }
}

/// 异常源、capability、订阅、P/C task（Running）与 frame Segment(seg=2) 就绪的 ops 态。
#[cfg(test)]
#[path = "cow_refine_tests.rs"]
mod tests;

pub fn base_exc() -> State {
    let mut exc = State {
        booted: true,
        ..State::default()
    };
    // COW 写故障：子只读共享页物理存在（page_present=true），写触发可写访问故障。
    // supported_kind 必须与组合访问层对此类页实际递送的故障类型一致。
    exc.exc_sources.insert(
        page_fault_source_id(),
        crate::state::ExceptionSource {
            supported_kind: ExceptionKind::PageFault {
                page_present: true,
                writable_fault: true,
            },
            additional_kinds: vec![],
            active: true,
        },
    );
    exc.insert_capability(
        10,
        Capability {
            owner: CHILD,
            object: CapObject::ExceptionSource(page_fault_source_id()),
            rights: CAP_EXC_SUBSCRIBE,
            active: true,
            parent: None,
        },
    );
    // frame 载体段 = 代码段 CODE_SEG；C 有写权
    exc.insert_capability(
        FRAME_CAP,
        Capability {
            owner: CHILD,
            object: CapObject::Segment(CODE_SEG),
            rights: CAP_SEG_MAP_W,
            active: true,
            parent: None,
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
                PageMapping {
                    base: COW_VA,
                    len: PAGE_SIZE,
                    access: ACCESS_R | ACCESS_W,
                    user: true,
                    backing: None,
                },
                PageMapping {
                    base: CODE_VA,
                    len: PAGE_SIZE,
                    access: ACCESS_X,
                    user: true,
                    backing: None,
                },
                PageMapping {
                    base: STACK_VA,
                    len: PAGE_SIZE,
                    access: ACCESS_R | ACCESS_W,
                    user: true,
                    backing: None,
                },
            ],
            root_ns_cap: 0,
            kernel_resume_frame: None,
        },
    );
    exc.tasks.insert(
        CHILD,
        Task {
            id: CHILD,
            status: TaskStatus::Ready,
            vspace: vec![
                PageMapping {
                    base: COW_VA,
                    len: PAGE_SIZE,
                    access: ACCESS_R,
                    user: true,
                    backing: None,
                },
                PageMapping {
                    base: CODE_VA,
                    len: PAGE_SIZE,
                    access: ACCESS_X,
                    user: true,
                    backing: None,
                },
                PageMapping {
                    base: STACK_VA,
                    len: PAGE_SIZE,
                    access: ACCESS_R | ACCESS_W,
                    user: true,
                    backing: None,
                },
            ],
            root_ns_cap: 0,
            kernel_resume_frame: None,
        },
    );
    exc
}
