//! P5-3b-1：CPL0 入口现场按 CPU 分区的受控探针（仅 `entry-percpu-probe` 镜像）。
//!
//! 每个 CPU 在自己的入口执行一次 `int 0x80` 与一次未映射读 #PF，回调只写本 CPU 的见证
//! 槽；BSP 在 AP 全部停放后汇总。任何「单槽 + 加锁」或「固定槽 0」实现都会让某个 CPU
//! 读回零或被他 CPU 覆盖，从而被本探针拒绝。**不是运行期启用**：普通镜像不含本模块，
//! `syscall` 快路径、`resume`、CPL3 用户入口与 CPL3 #PF 现场仍未分区。

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use prima_ostd::entry::{self, AbiFrame, SretFrame};

const MAX: usize = crate::cpu::local::MAX_CPUS;
/// 调用号与返回值的每 CPU 编码；两者都含 CPU 索引，用于核对回调确实看到本 CPU。
const NUM_MAGIC: u64 = 0x5035_0100;
const RET_MAGIC: u64 = 0x5035_1a00;

static NUM_OK: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static HANDLER_ADDR: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
static HANDLER_RIP: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
static SELF_SYSCALL: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static SELF_CTX: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static SELF_FAULT: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static SELF_FRAME: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];

fn current_cpu() -> usize {
    let cpu = crate::cpu::local::current_index();
    assert!(cpu < MAX, "entry probe cpu index out of range");
    cpu
}

/// 每 CPU 一个互不相同的未映射低地址；避开 AP 跳板页（< 0x9f000）。
fn expected_fault_va(cpu: usize) -> u64 {
    0x20_0000 + cpu as u64 * 0x1000
}

extern "C" fn probe_syscall(frame: AbiFrame) -> u64 {
    let cpu = current_cpu();
    let ok = frame.num() == NUM_MAGIC + cpu as u64 && frame.arg(0) == cpu as u64;
    NUM_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    RET_MAGIC + cpu as u64
}

extern "C" fn probe_fault(frame: &SretFrame) -> u64 {
    let cpu = current_cpu();
    HANDLER_ADDR[cpu].store(entry::percpu_fault_addr(cpu), Ordering::SeqCst);
    HANDLER_RIP[cpu].store(frame.rip, Ordering::SeqCst);
    frame.rip + crate::idt::FAULT_SKIP_LEN
}

/// 在 BSP 启动 AP 之前调用：把 syscall/#PF 回调换成探针版本。
///
/// 镜像在见证后立即退出，不恢复普通回调；这保证探针不会污染后续启动。
pub fn install_handlers() {
    entry::set_syscall_handler(probe_syscall);
    entry::set_page_fault_handler(probe_fault);
}

/// 在当前 CPU 上执行 `int 0x80` 与一次 #PF，把四项自检写入本 CPU 槽。
///
/// `stack_lo..stack_hi` 是本 CPU 当前内核栈范围：入口保存的保存块基址必须落在本 CPU
/// 自己的栈上。共享单槽会让后运行的 CPU 覆盖前者的值，使前者的槽读回零或落在别处。
pub fn run_local(cpu: usize, stack_lo: u64, stack_hi: u64) {
    assert_eq!(cpu, current_cpu(), "run_local must run on its own CPU");

    let ret = entry::int80_probe(NUM_MAGIC + cpu as u64, [cpu as u64, 0, 0, 0, 0, 0]);
    let syscall_ok = ret == RET_MAGIC + cpu as u64 && NUM_OK[cpu].load(Ordering::SeqCst) == 1;
    let ctx_ok = entry::percpu_context_base(cpu)
        .map(|block| (stack_lo..stack_hi).contains(&block.addr()))
        .unwrap_or(false);
    SELF_SYSCALL[cpu].store(u32::from(syscall_ok), Ordering::SeqCst);
    SELF_CTX[cpu].store(u32::from(ctx_ok), Ordering::SeqCst);

    let expected = expected_fault_va(cpu);
    entry::trigger_page_fault(expected);
    let handler_rip = HANDLER_RIP[cpu].load(Ordering::SeqCst);
    let fault_ok = entry::percpu_fault_addr(cpu) == expected
        && entry::percpu_fault_error(cpu) == 0
        && HANDLER_ADDR[cpu].load(Ordering::SeqCst) == expected
        && handler_rip != 0;
    let frame = entry::percpu_fault_frame(cpu);
    let frame_ok = (frame.cs & 3) == 0 && frame.rip == handler_rip;
    SELF_FAULT[cpu].store(u32::from(fault_ok), Ordering::SeqCst);
    SELF_FRAME[cpu].store(u32::from(frame_ok), Ordering::SeqCst);
}

/// 每 CPU 探针见证；四项计数都等于 `cpus` 才算通过。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryWitness {
    pub cpus: u32,
    pub syscall_ok: u32,
    pub ctx_ok: u32,
    pub fault_ok: u32,
    pub frame_ok: u32,
}

impl EntryWitness {
    pub fn evidence_ok(&self) -> bool {
        self.syscall_ok == self.cpus
            && self.ctx_ok == self.cpus
            && self.fault_ok == self.cpus
            && self.frame_ok == self.cpus
    }
}

/// 汇总各 CPU 槽；同时核对两面槽数一致。
pub fn witness(cpus: u32) -> EntryWitness {
    assert_eq!(entry::PERCPU_MAX, MAX, "percpu slot count mismatch");
    let mut witness = EntryWitness {
        cpus,
        syscall_ok: 0,
        ctx_ok: 0,
        fault_ok: 0,
        frame_ok: 0,
    };
    for cpu in 0..cpus as usize {
        witness.syscall_ok += SELF_SYSCALL[cpu].load(Ordering::SeqCst);
        witness.ctx_ok += SELF_CTX[cpu].load(Ordering::SeqCst);
        witness.fault_ok += SELF_FAULT[cpu].load(Ordering::SeqCst);
        witness.frame_ok += SELF_FRAME[cpu].load(Ordering::SeqCst);
    }
    witness
}

/// 失败诊断：各 CPU 的四位自检标志（`[syscall, ctx, fault, frame]` 逐 CPU）。
pub fn progress() -> [[u32; 4]; MAX] {
    core::array::from_fn(|cpu| {
        [
            SELF_SYSCALL[cpu].load(Ordering::SeqCst),
            SELF_CTX[cpu].load(Ordering::SeqCst),
            SELF_FAULT[cpu].load(Ordering::SeqCst),
            SELF_FRAME[cpu].load(Ordering::SeqCst),
        ]
    })
}
