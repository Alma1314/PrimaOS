#![allow(dead_code)]

use crate::memseg_refine::RefineState;
use crate::pagewalk::{PageWalkError, PhysAddr, translate as table_translate, validate_page_vaddr};
use crate::state::{ACCESS_R, ACCESS_W, ACCESS_X, Task, TaskId, VirtAddr};
use std::collections::{BTreeMap, HashMap};

/// 每条 TLB 缓存翻译：命中时返回的物理帧与权限，可对不应期 PTE（陈旧）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CachedXlat {
    pub frame_phys: PhysAddr,
    pub access: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Tlb {
    entries: BTreeMap<VirtAddr, CachedXlat>,
}

/// 每 Task 独立 TLB（每 CR3 一 TLB）。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TlbState {
    per_task: BTreeMap<TaskId, Tlb>,
}

impl TlbState {
    pub fn new() -> Self {
        TlbState::default()
    }

    pub(crate) fn task_ids(&self) -> impl Iterator<Item = &TaskId> {
        self.per_task.keys()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlbError {
    NoSuchTask,
    UnalignedAddress,
    NonCanonicalAddress,
    NotMapped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessKind {
    Read,
    Write,
    Execute,
}

pub fn access_allowed(xl: &CachedXlat, kind: AccessKind) -> bool {
    let required = match kind {
        AccessKind::Read => ACCESS_R,
        AccessKind::Write => ACCESS_W,
        AccessKind::Execute => ACCESS_X,
    };
    xl.access & required != 0
}

pub fn has_tlb(tlb: &TlbState, task: TaskId, va: u64) -> bool {
    tlb.per_task
        .get(&task)
        .is_some_and(|t| t.entries.contains_key(&va))
}

/// 命中走 TLB 缓存（不用户表）；未命中去页表并填充。
pub fn translate(
    memory: &RefineState,
    tasks: &HashMap<TaskId, Task>,
    tlb: &mut TlbState,
    task: TaskId,
    va: VirtAddr,
) -> Result<CachedXlat, TlbError> {
    match validate_page_vaddr(va) {
        Err(PageWalkError::UnalignedAddress) => return Err(TlbError::UnalignedAddress),
        Err(PageWalkError::NonCanonicalAddress) => return Err(TlbError::NonCanonicalAddress),
        Err(PageWalkError::InvalidStructure) => return Err(TlbError::NotMapped),
        Ok(()) => {}
    }
    tasks.get(&task).ok_or(TlbError::NoSuchTask)?;
    let tc = memory.tasks.get(&task).ok_or(TlbError::NoSuchTask)?;
    if let Some(xl) = tlb
        .per_task
        .get(&task)
        .and_then(|entry| entry.entries.get(&va))
    {
        return Ok(*xl); // 命中：陈旧与否都返回缓存
    }
    let Some(pte) = table_translate(&tc.table, va).map_err(|_| TlbError::NotMapped)? else {
        return Err(TlbError::NotMapped); // 未映射，不填充
    };
    let xl = CachedXlat {
        frame_phys: pte.frame_phys,
        access: pte.mapping.access,
    };
    tlb.per_task.entry(task).or_default().entries.insert(va, xl);
    Ok(xl)
}

/// 单地址失效（对应 invlpg）：合法 Task 的无缓存地址同样成功。
pub fn invalidate_va(
    tasks: &HashMap<TaskId, Task>,
    tlb: &mut TlbState,
    task: TaskId,
    va: VirtAddr,
) -> Result<(), TlbError> {
    match validate_page_vaddr(va) {
        Err(PageWalkError::UnalignedAddress) => return Err(TlbError::UnalignedAddress),
        Err(PageWalkError::NonCanonicalAddress) => return Err(TlbError::NonCanonicalAddress),
        Err(PageWalkError::InvalidStructure) => return Err(TlbError::NotMapped),
        Ok(()) => {}
    }
    tasks.get(&task).ok_or(TlbError::NoSuchTask)?;
    if let Some(entry) = tlb.per_task.get_mut(&task) {
        entry.entries.remove(&va);
    }
    Ok(())
}

/// 全量失效（对应 CR3 reload / INVPCID）：合法 Task 的无缓存状态同样成功。
pub fn flush_all(
    tasks: &HashMap<TaskId, Task>,
    tlb: &mut TlbState,
    task: TaskId,
) -> Result<(), TlbError> {
    tasks.get(&task).ok_or(TlbError::NoSuchTask)?;
    if let Some(entry) = tlb.per_task.get_mut(&task) {
        entry.entries.clear();
    }
    Ok(())
}

/// Task 销毁时清除其全部缓存翻译；不存在的缓存同样成功。
pub fn remove_task(tlb: &mut TlbState, task: TaskId) {
    tlb.per_task.remove(&task);
}
