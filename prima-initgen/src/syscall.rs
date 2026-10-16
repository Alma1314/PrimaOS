//! Host-side copies of the kernel ABI constants used by the extracted init
//! machine-code generator. These are deliberately frozen mirrors; the kernel is
//! still the source of truth for enforcement, and QEMU/xtask gates detect drift.

use crate::rights::{Right, Rights};

pub const FRAME_SIZE: u64 = 4096;

pub const ACCESS_R_BITS: u64 = 1 << (Right::SegMapR as u32);
pub const ACCESS_W_BITS: u64 = 1 << (Right::SegMapW as u32);

pub const AUDIT_RECEIPT_LIVE: u64 = 720;

pub const AUDIT_RECEIPT_STALE: u64 = 728;

pub const BAD_PTR: u64 = crate::code::USER_BASE_VA + 0x20_0000;

pub const CAP_CHILD1_HANDLE: u64 = 0x0000_0001_0000_0006;

pub const CAP_CHILD2_HANDLE: u64 = 0x0000_0002_0000_0006;

pub const COPY_LEN: u64 = 8;

pub const COPY_PTR: u64 = crate::code::USER_X_VA;

pub const EXC_KIND_PAGEFAULT: u64 = 1;

pub const EXC_KIND_SYSCALLTRAP: u64 = 2;

pub const FAST_ARG: u64 = 0x0FED_CBA9_8765_4321;

pub const IPC_DIRECT_PAYLOAD: u64 = 0xD1EC_7000_0000_0002;

pub const IPC_SHARED_MAGIC: u64 = 0x51A5_0000_0000_0001;

pub const MEMSEG_CAP1_HANDLE: u64 = 0x0000_0003_0000_0006;

pub const MEMSEG_CAP2_HANDLE: u64 = 0x0000_0001_0000_0007;

pub const MEMSEG_PATTERN: u64 = 0xDEC0_DE00_1234_5678;

pub const MEMSEG_RW_BITS: u64 = ACCESS_R_BITS | ACCESS_W_BITS;

pub const NS_BIND_RIGHTS_BITS: u64 =
    Rights::from_rights(&[Right::Derive, Right::SegMapR]).bits() as u64;

pub const NS_PATH_DATA_OFFSET: usize = 1024;

pub const NS_PATH_DATA_VA: u64 = crate::code::USER_Y_VA + NS_PATH_DATA_OFFSET as u64;

pub const NS_PATH_ILLEGAL_OFF: u64 = 20;

pub const NS_PATH_LIB_OFF: u64 = 0;

pub const NS_PATH_MNT_OFF: u64 = 8;

pub const NS_PATH_MNTSUB_OFF: u64 = 12;

pub const NS_PATH_SUB_OFF: u64 = 4;

pub const NS_PATH_X_OFF: u64 = 25;

pub const NS_PATH_Y_OFF: u64 = 27;

pub const NS_RECEIPT_BAD_PATH: u64 = 632;

pub const NS_RECEIPT_BIND: u64 = 584;

pub const NS_RECEIPT_BIND_OVERLAP: u64 = 592;

pub const NS_RECEIPT_BIND_SUB: u64 = 600;

pub const NS_RECEIPT_COPY_B: u64 = 552;

pub const NS_RECEIPT_COPY_C: u64 = 560;

pub const NS_RECEIPT_CREATE_A: u64 = 512;

pub const NS_RECEIPT_CREATE_B: u64 = 520;

pub const NS_RECEIPT_CREATE_C: u64 = 528;

pub const NS_RECEIPT_DERIVE1: u64 = 536;

pub const NS_RECEIPT_DERIVE2: u64 = 544;

pub const NS_RECEIPT_DESTROY: u64 = 656;

pub const NS_RECEIPT_MOUNT_A_B: u64 = 568;

pub const NS_RECEIPT_MOUNT_CYCLE: u64 = 576;

pub const NS_RECEIPT_MOUNT_INIT_C: u64 = 616;

pub const NS_RECEIPT_RESOLVE_LIB: u64 = 608;

pub const NS_RECEIPT_RESOLVE_SUB: u64 = 624;

pub const NS_RECEIPT_UNBIND: u64 = 640;

pub const NS_RECEIPT_UNMOUNT: u64 = 648;

pub const NS_ROOT_RIGHTS_BITS: u64 = 0x3F01;

pub const RIGHT_DERIVE: u64 = 0;

pub const RIGHT_SEGMAPR: u64 = 1;

pub const RIGHT_SEGMAPW: u64 = 2;

pub const RIGHTS_DERIVE_BITS: u64 = 1;

pub const RIGHTS_SEGMAPW_BITS: u64 = 1 << (Right::SegMapW as u32);

pub const SUM_ARGS: [u64; 4] = [
    0x1111_1111_1111_1111,
    0x2222_2222_2222_2222,
    0x0404_0404_0404_0404,
    0x0808_0808_0808_0808,
];

pub const SYS_BAD: u64 = 0xDEAD;

pub const SYS_ECHO_ARG: u64 = 0x0123_4567_89AB_CDEF;

pub const TASK_RECV_BUF: u64 = crate::code::USER_Y_VA + 0x300;
pub const TASK_RECEIPT_STACK_SEG: u64 = 368;

pub const TASK_REQ_VA: u64 = crate::code::USER_STACK_VA + 0x800;

pub const TASK2_ID: u32 = 2;

pub const TASK2_WAKE_PAYLOAD: u64 = 0x5A3C_0000_0000_0002;

pub const TIMER_RECEIPT_CANCEL: u64 = 696;

pub const TIMER_RECEIPT_CANCEL_AGAIN: u64 = 704;

pub const TIMER_RECEIPT_DESTROY: u64 = 712;

pub const TIMER_RECEIPT_NOW: u64 = 664;

pub const TIMER_RECEIPT_T1: u64 = 672;

pub const TIMER_RECEIPT_T2: u64 = 680;

pub const TIMER_RECEIPT_T3: u64 = 688;

pub const UNSET_CAP: u64 = (1u64 << 32) | 60;

pub const USER_MAP_WITNESS_VA: u64 = crate::code::USER_BASE_VA + 0x4000;

pub const USER_MAP_WITNESS_VA2: u64 = crate::code::USER_BASE_VA + 0x6000;

pub const USER_SEG_SHARED_VA: u64 = crate::code::USER_BASE_VA + 0x6000;

pub const USER_SEG_VA: u64 = crate::code::USER_BASE_VA + 0x5000;

pub use prima_abi::syscall::SYS_AUDIT;
pub use prima_abi::syscall::SYS_AUTHORIZE;
pub use prima_abi::syscall::SYS_CAP_DERIVE;
pub use prima_abi::syscall::SYS_CAP_DROP;
pub use prima_abi::syscall::SYS_COPY_IN;
pub use prima_abi::syscall::SYS_ECHO;
pub use prima_abi::syscall::SYS_EXC_SUBSCRIBE;
pub use prima_abi::syscall::SYS_EXC_UNSUBSCRIBE;
pub use prima_abi::syscall::SYS_EXC_WAIT;
pub use prima_abi::syscall::SYS_MEMSEG_CREATE;
pub use prima_abi::syscall::SYS_MEMSEG_DESTROY;
pub use prima_abi::syscall::SYS_MEMSEG_MAP;
pub use prima_abi::syscall::SYS_MEMSEG_UNMAP;
pub use prima_abi::syscall::SYS_NS_BIND;
pub use prima_abi::syscall::SYS_NS_CREATE;
pub use prima_abi::syscall::SYS_NS_DESTROY;
pub use prima_abi::syscall::SYS_NS_MOUNT;
pub use prima_abi::syscall::SYS_NS_RESOLVE;
pub use prima_abi::syscall::SYS_NS_UNBIND;
pub use prima_abi::syscall::SYS_NS_UNMOUNT;
pub use prima_abi::syscall::SYS_RECV;
pub use prima_abi::syscall::SYS_SEND;
pub use prima_abi::syscall::SYS_SUM4;
pub use prima_abi::syscall::SYS_TASK_CREATE;
pub use prima_abi::syscall::SYS_TASK_DESTROY;
pub use prima_abi::syscall::SYS_TASK_ID;
pub use prima_abi::syscall::SYS_TIME_NOW;
pub use prima_abi::syscall::SYS_TIMER_CANCEL;
pub use prima_abi::syscall::SYS_TIMER_DESTROY;
pub use prima_abi::syscall::SYS_TIMER_SET;
pub use prima_abi::syscall::SYS_YIELD;
