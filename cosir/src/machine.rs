#![allow(dead_code)]

use crate::state::VirtAddr;

// 本模块为 BLK-002 细化域起点：这些机器态类型在 Task 4 的 abstract.rs 中被构造/读取。
// 在此之前它们保持未在探针中实体化，故整体 allow(dead_code) 以保留规范保真度
// （与 state.rs 中对 TaskStatus::BlockedRecv、ExceptionKind 等前向模型字段的处理一致）。

/// 根页表物理地址（交接点已建分页）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cr3(pub u64);

/// 简化页表视图条目：(vaddr, size, access)；access 复用 state 的 R/W/X 位域常量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub vaddr: VirtAddr,
    pub size: u64,
    pub access: u8,
}

/// GDT 表：简化只建模基址/限长。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GdtTable {
    pub base: u64,
    pub limit: u16,
}

/// 处理器状态段：简化仅 RSP0 中断栈。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tss {
    pub rsp0: VirtAddr,
}

/// 交接的 BootInfo 抽象：物理内存范围。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BootInfoHandle {
    pub phys_mem_start: u64,
    pub phys_mem_len: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InterruptState {
    Masked,
    Unmasked,
}

/// 交接点的 x86_64 机器状态（细化域起点）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MachineState {
    pub cr3: Cr3,
    pub address_space: Vec<MapEntry>,
    pub gdt: GdtTable,
    pub tss: Tss,
    pub rip: VirtAddr,
    pub rsp: VirtAddr,
    pub boot_info: BootInfoHandle,
    pub idt_ready: bool,
    pub interrupt_state: InterruptState,
    /// 细化层可见：bootstrap 是否已执行（W14 唯一性）。
    pub booted: bool,
}
