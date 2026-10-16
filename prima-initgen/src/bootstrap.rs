//! Frozen mirrors of the boot-time capability handles referenced by the generated
//! init machine code. The real authority remains the kernel capability table.

use crate::rights::Rights;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Handle(u64);

impl Handle {
    pub const fn new(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << 32) | index as u64)
    }

    pub const fn encode(self) -> u64 {
        self.0
    }
}

pub const INITIAL_CAP_GENERATION: u32 = 1;

pub const IMAGE_CAP_HANDLE: Handle = Handle::new(1, INITIAL_CAP_GENERATION);
pub const NAMESPACE_CAP_HANDLE: Handle = Handle::new(2, INITIAL_CAP_GENERATION);
pub const TASK_CAP_HANDLE: Handle = Handle::new(3, INITIAL_CAP_GENERATION);
pub const EXC_PAGE_FAULT_HANDLE: Handle = Handle::new(4, INITIAL_CAP_GENERATION);
pub const EXC_SYSCALL_TRAP_HANDLE: Handle = Handle::new(5, INITIAL_CAP_GENERATION);

pub const PLATFORM_SEGMENT_CAP_SLOT_BASE: u32 = 32;
pub const PLATFORM_SEGMENT_ID_FIRST: u32 = 2;
pub const PLATFORM_TASK_CAP_SLOT_BASE: u32 = 45;
pub const PLATFORM_TASK1_CAP_HANDLE: Handle =
    Handle::new(PLATFORM_TASK_CAP_SLOT_BASE, INITIAL_CAP_GENERATION);

pub const USER_IPC_CAP_SLOT_BASE: u32 = 47;
pub const TASK2_SEND_CAP_HANDLE: Handle =
    Handle::new(USER_IPC_CAP_SLOT_BASE, INITIAL_CAP_GENERATION);
pub const TASK2_SELF_CAP_HANDLE: Handle =
    Handle::new(USER_IPC_CAP_SLOT_BASE + 1, INITIAL_CAP_GENERATION);
pub const TASK_CREATE_SEND_CAP_HANDLE: Handle =
    Handle::new(USER_IPC_CAP_SLOT_BASE + 3, INITIAL_CAP_GENERATION);
pub const TASK4_SEND_CAP_HANDLE: Handle =
    Handle::new(USER_IPC_CAP_SLOT_BASE + 4, INITIAL_CAP_GENERATION);
pub const SERVICE3_STACK_CAP_HANDLE: Handle =
    Handle::new(USER_IPC_CAP_SLOT_BASE + 11, INITIAL_CAP_GENERATION);
pub const SERVICEMGR_SEND_CAP_HANDLE: Handle =
    Handle::new(USER_IPC_CAP_SLOT_BASE + 12, INITIAL_CAP_GENERATION);
pub const SERVICE_IMAGE_CAP_HANDLE: Handle = Handle::new(62, INITIAL_CAP_GENERATION);
pub const SERVICE_IMAGE_MGR_CAP_HANDLE: Handle = Handle::new(63, INITIAL_CAP_GENERATION);

pub const fn platform_segment_cap_handle(id: u32) -> Handle {
    Handle::new(
        PLATFORM_SEGMENT_CAP_SLOT_BASE + (id - PLATFORM_SEGMENT_ID_FIRST),
        INITIAL_CAP_GENERATION,
    )
}

/// `Derive | NsResolve | NsBind | NsUnbind | NsMount | NsUnmount | NsDestroy`.
pub const NAMESPACE_RIGHTS: Rights = Rights::from_bits(0x3F01);
/// Structural child root rights: only `NsResolve` (bit 8).
pub const TASK_CREATE_ROOT_NS_RIGHTS: Rights = Rights::from_bits(1 << 8);
