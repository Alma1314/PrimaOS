#![allow(dead_code)]

use crate::memseg_refine::RefineState;
use crate::pagewalk::{PageWalkError, PhysAddr, translate as table_translate, validate_page_vaddr};
use crate::state::{ACCESS_W, TaskId, VirtAddr};
use std::collections::BTreeMap;

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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TlbError {
    NoSuchTask,
    UnalignedAddress,
    NonCanonicalAddress,
    NotMapped,
}

pub fn access_allowed(xl: &CachedXlat, for_write: bool) -> bool {
    if for_write {
        xl.access & ACCESS_W != 0
    } else {
        xl.access != 0
    }
}

pub fn has_tlb(tlb: &TlbState, task: TaskId, va: u64) -> bool {
    tlb.per_task
        .get(&task)
        .is_some_and(|t| t.entries.contains_key(&va))
}

/// 命中走 TLB 缓存（不用户表）；未命中去页表并填充。
pub fn translate(
    pf: &RefineState,
    tlb: &mut TlbState,
    task: TaskId,
    va: u64,
) -> Result<CachedXlat, TlbError> {
    match validate_page_vaddr(va) {
        Err(PageWalkError::UnalignedAddress) => return Err(TlbError::UnalignedAddress),
        Err(PageWalkError::NonCanonicalAddress) => return Err(TlbError::NonCanonicalAddress),
        Ok(()) => {}
    }
    // Task 存在性以页表细化态为准（权威），TLB 的每 Task 表在首次访问时惰性建立。
    let tc = pf.tasks.get(&task).ok_or(TlbError::NoSuchTask)?;
    let entry = tlb.per_task.entry(task).or_default();
    if let Some(xl) = entry.entries.get(&va) {
        return Ok(*xl); // 命中：陈旧与否都返回缓存
    }
    let Some(pte) = table_translate(&tc.table, va).map_err(|_| TlbError::NotMapped)? else {
        return Err(TlbError::NotMapped); // 未映射，不填充
    };
    let xl = CachedXlat {
        frame_phys: pte.frame_phys,
        access: pte.mapping.access,
    };
    entry.entries.insert(va, xl);
    Ok(xl)
}

/// 单地址失效（对应 invlpg）：幂等。
pub fn invalidate_va(tlb: &mut TlbState, task: TaskId, va: u64) -> Result<(), TlbError> {
    let entry = tlb.per_task.get_mut(&task).ok_or(TlbError::NoSuchTask)?;
    entry.entries.remove(&va);
    Ok(())
}

/// 全量失效（对应 CR3 reload / INVPCID）。
pub fn flush_all(tlb: &mut TlbState, task: TaskId) -> Result<(), TlbError> {
    let entry = tlb.per_task.get_mut(&task).ok_or(TlbError::NoSuchTask)?;
    entry.entries.clear();
    Ok(())
}
