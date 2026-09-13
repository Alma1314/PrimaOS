//! 切片 O：PIT 时钟抢占 —— 真实 8259 PIC + 8254 PIT 在 CPL3 抢占两个任务。
//!
//! 与既有协作式调度（`SYS_YIELD`）不同，本模块的切换**不由任务主动发起**：8254 通道 0
//! 周期性拉高 IRQ0，8259 主片把其中继到 IDT vector 32，裸入口 [`timer_entry`] 保存被中断
//! 任务的完整用户上下文（布局同 `syscall_entry` 的 15-GPR 保存块 + 5-qword 硬件帧，无
//! error code），由 [`preempt_tick`] 发 EOI、把当前任务置回 Ready、`sched::switch_to` 到
//! 下一任务。切换时 `sched::resume` 写目标任务 CR3 与 `TSS.rsp0`，故两任务各有独立地址
//! 空间与内核栈。
//!
//! **只在 CPL3 抢占：** 不在内核 `sti`，只有抢占阶段的用户帧把 RFLAGS.IF 置 1，故时钟只
//! 可能在被中断的用户态到达；入口 CS.RPL 必为 3。既有协作阶段用户帧 IF=0 且 PIT 未启动，
//! 完全不受扰动。
//!
//! **范围边界：** 单核、无 APIC timer、无 PCID/TLB shootdown、不宣称真实硬件或 `KERNEL-TRACE`。

use core::arch::asm;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use crate::bootstrap::INIT_TASK_ID;
use crate::mem::phys_to_virt;
use crate::sched;
use crate::syscall::TASK2_ID;

/// 8259 主片命令/数据口。
const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
/// 8259 从片命令/数据口。
const PIC2_CMD: u16 = 0xa0;
const PIC2_DATA: u16 = 0xa1;
/// 8254 PIT 通道 0 数据口与命令口。
const PIT_CH0: u16 = 0x40;
const PIT_CMD: u16 = 0x43;
/// 时钟中断向量（PIC 主片重映射到 0x20 起）。
pub const TIMER_VECTOR: u8 = 0x20;
/// PIT 分频：1193182 Hz / 11932 ≈ 100 Hz。
const PIT_DIVISOR: u16 = 11932;
/// 每个任务需累计的「被时钟抢占」次数，达到后收尾。
const PREEMPT_THRESHOLD: u64 = 2;

static PREEMPT_ENABLE: AtomicBool = AtomicBool::new(false);
static TICKS: AtomicU64 = AtomicU64::new(0);
/// 各任务被时钟抢占的次数（索引即 Task id）。
static PREEMPTS: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
/// 各任务在时钟入口观测到的 CR3 根物理地址。
static OBS_CR3: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
/// 各任务在时钟入口观测到的内核栈 rsp（证明 per-task 内核栈）。
static OBS_RSP: [AtomicU64; 3] = [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)];
/// 完成回调（由二进制 crate 注册；不返回）。
static FINISH: AtomicU64 = AtomicU64::new(0);

/// 注册完成回调。回调不得返回。
pub fn set_finish_handler(handler: fn() -> !) {
    FINISH.store(handler as usize as u64, Ordering::Relaxed);
}

unsafe fn outb(port: u16, value: u8) {
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nostack, preserves_flags));
    }
}

/// 发 EOI 给主片（IRQ0 只需主片）。
fn eoi() {
    unsafe { outb(PIC1_CMD, 0x20) };
}

/// 重映射 8259、只放行 IRQ0，并启动 8254 通道 0（~100 Hz）。
///
/// 在 IF=0 的内核态调用：PIC/PIT 就绪后由 `iretq` 恢复到 IF=1 的用户帧，时钟才开始到达。
pub fn init_timer() {
    unsafe {
        // ICW1：初始化 + 需要 ICW4。
        outb(PIC1_CMD, 0x11);
        outb(PIC2_CMD, 0x11);
        // ICW2：主片偏移 0x20、从片偏移 0x28。
        outb(PIC1_DATA, TIMER_VECTOR);
        outb(PIC2_DATA, 0x28);
        // ICW3：主片 IRQ2 接从片；从片接在 IRQ2。
        outb(PIC1_DATA, 0x04);
        outb(PIC2_DATA, 0x02);
        // ICW4：8086 模式。
        outb(PIC1_DATA, 0x01);
        outb(PIC2_DATA, 0x01);
        // OCW1：主片只放行 IRQ0（0xFE），从片全屏蔽。
        outb(PIC1_DATA, 0xFE);
        outb(PIC2_DATA, 0xFF);
        // 8254 通道 0：模式 3（方波）、先低后高写分频。
        outb(PIT_CMD, 0x36);
        outb(PIT_CH0, (PIT_DIVISOR & 0xff) as u8);
        outb(PIT_CH0, (PIT_DIVISOR >> 8) as u8);
    }
}

/// 切片 O：IRQ0（vector 32）裸入口。
///
/// 布局与 `syscall_entry` 相同：入口先 `mov rsi, rsp` 记录进入时的内核栈 rsp（per-task
/// 内核栈见证），再压 15 个 GPR（rax 最低地址），`rbp` 作保存块基址。`preempt_tick` 返回
/// 则弹栈 + 丢弃 5-qword 硬件帧 + `iretq`；若发生抢占切换则永不返回。
///
/// # Safety
///
/// 只能由 CPU 在 IRQ0（vector 32）经 IDT 调用；不可从 Rust 侧调用。
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_entry() -> ! {
    core::arch::naked_asm!(
        "mov rsi, rsp",
        "push r15",
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rbp",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        "mov rbp, rsp",
        "mov rdi, rbp",
        "and rsp, -16",
        "call {tick}",
        "mov rsp, rbp",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop rbp",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "add rsp, 40",
        "iretq",
        tick = sym preempt_tick,
    )
}

/// 时钟 tick 的内核侧处理。
///
/// `frame` 指向裸入口的 15-GPR 保存块；`entry_rsp` 是进入时的内核栈 rsp。仅 CPL3 且抢占
/// 已启用时动作：发 EOI、计数、记录当前任务 CR3/rsp，把当前完整 `SretFrame` 存入调度器，
/// 若阈值达到则收尾，否则切到下一 Ready 任务（不返回）。
extern "C" fn preempt_tick(frame: *const u64, entry_rsp: u64) {
    // 保存块中 CS 位于第 16 个 qword（15 GPR + rip）。
    let cs = unsafe { core::ptr::read_volatile(frame.add(16)) };
    if cs & 3 != 3 {
        return;
    }
    if !PREEMPT_ENABLE.load(Ordering::Relaxed) {
        return;
    }
    eoi();
    TICKS.fetch_add(1, Ordering::Relaxed);

    let cur = sched::current();
    let idx = cur as usize;
    if idx < PREEMPTS.len() {
        OBS_CR3[idx].store(crate::cpu::cr3_phys(), Ordering::Relaxed);
        OBS_RSP[idx].store(entry_rsp, Ordering::Relaxed);
    }

    let Some(next) = sched::pick_next(cur) else {
        return;
    };

    let frame_value = unsafe { crate::syscall::frame_from_save_block(frame) };
    if idx < PREEMPTS.len() {
        PREEMPTS[idx].fetch_add(1, Ordering::Relaxed);
    }
    sched::set_frame(cur, frame_value);

    if PREEMPTS[INIT_TASK_ID as usize].load(Ordering::Relaxed) >= PREEMPT_THRESHOLD
        && PREEMPTS[TASK2_ID as usize].load(Ordering::Relaxed) >= PREEMPT_THRESHOLD
    {
        finish();
    }

    sched::switch_to(next)
}

/// 调用完成回调（不返回）；未注册时停机。
fn finish() -> ! {
    let raw = FINISH.load(Ordering::Relaxed);
    if raw != 0 {
        let handler: fn() -> ! = unsafe { core::mem::transmute(raw as usize) };
        handler();
    }
    loop {
        unsafe { asm!("hlt") };
    }
}

/// 一次时钟抢占见证的实测证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreemptWitness {
    pub ticks: u64,
    pub preempt1: u64,
    pub preempt2: u64,
    /// 两任务各自的 PML4 物理地址（应互异且非零）。
    pub root1: u64,
    pub root2: u64,
    /// 时钟入口观测到的 CR3（应分别等于 root1/root2）。
    pub cr31: u64,
    pub cr32: u64,
    /// 时钟入口观测到的内核栈 rsp 与两任务内核栈边界。
    pub rsp1: u64,
    pub rsp2: u64,
    pub kstack1_bottom: u64,
    pub kstack1_top: u64,
    pub kstack2_bottom: u64,
    pub kstack2_top: u64,
    /// 同 VA（[`crate::user::SPIN_DATA_VA`]）在两任务根下解析出的物理帧（应互异）。
    pub data1: u64,
    pub data2: u64,
    /// 两任务空转代码写入各自数据帧的标记（应 1 / 2）。
    pub mark1: u64,
    pub mark2: u64,
}

impl PreemptWitness {
    /// 纯判定：时钟抢占是否满足全部成功条件（host 可测）。
    pub fn evidence_ok(&self) -> bool {
        self.ticks > 0
            && self.preempt1 >= PREEMPT_THRESHOLD
            && self.preempt2 >= PREEMPT_THRESHOLD
            && self.root1 != 0
            && self.root2 != 0
            && self.root1 != self.root2
            && self.cr31 == self.root1
            && self.cr32 == self.root2
            && self.rsp1 != self.rsp2
            && self.rsp1 > self.kstack1_bottom
            && self.rsp1 <= self.kstack1_top
            && self.rsp2 > self.kstack2_bottom
            && self.rsp2 <= self.kstack2_top
            && self.data1 != 0
            && self.data2 != 0
            && self.data1 != self.data2
            && self.mark1 == INIT_TASK_ID as u64
            && self.mark2 == TASK2_ID as u64
    }
}

/// 读取一个 qword（经直映射）。
fn read_phys_qword(phys: u64, offset: u64) -> u64 {
    unsafe { core::ptr::read_volatile(phys_to_virt(phys, offset) as *const u64) }
}

/// 某任务根的 PML4 虚拟地址（直映射），用于 `cpu::translate`。
fn root_va(root_phys: u64, offset: u64) -> u64 {
    phys_to_virt(root_phys, offset)
}

/// 收集抢占见证（在完成回调中调用，单核无并发）。
pub fn witness() -> PreemptWitness {
    let offset = crate::cow::root_and_offset()
        .map(|(_, offset)| offset)
        .unwrap_or(0);
    let root1 = sched::root(INIT_TASK_ID).unwrap_or(0);
    let root2 = sched::root(TASK2_ID).unwrap_or(0);
    let map = |phys: u64| phys_to_virt(phys, offset);
    let data1 =
        crate::cpu::translate(root_va(root1, offset), crate::user::SPIN_DATA_VA, map).unwrap_or(0);
    let data2 =
        crate::cpu::translate(root_va(root2, offset), crate::user::SPIN_DATA_VA, map).unwrap_or(0);
    let top1 = sched::kstack_top(INIT_TASK_ID).unwrap_or(0);
    let top2 = sched::kstack_top(TASK2_ID).unwrap_or(0);
    PreemptWitness {
        ticks: TICKS.load(Ordering::Relaxed),
        preempt1: PREEMPTS[INIT_TASK_ID as usize].load(Ordering::Relaxed),
        preempt2: PREEMPTS[TASK2_ID as usize].load(Ordering::Relaxed),
        root1,
        root2,
        cr31: OBS_CR3[INIT_TASK_ID as usize].load(Ordering::Relaxed),
        cr32: OBS_CR3[TASK2_ID as usize].load(Ordering::Relaxed),
        rsp1: OBS_RSP[INIT_TASK_ID as usize].load(Ordering::Relaxed),
        rsp2: OBS_RSP[TASK2_ID as usize].load(Ordering::Relaxed),
        kstack1_bottom: top1.saturating_sub(crate::user::KSTACK_SIZE as u64),
        kstack1_top: top1,
        kstack2_bottom: top2.saturating_sub(crate::user::KSTACK_SIZE as u64),
        kstack2_top: top2,
        data1,
        data2,
        mark1: if data1 != 0 {
            read_phys_qword(data1, offset)
        } else {
            0
        },
        mark2: if data2 != 0 {
            read_phys_qword(data2, offset)
        } else {
            0
        },
    }
}

/// 进入时钟抢占阶段：装入两任务空转帧、启动 PIT/PIC，然后恢复 task 1（不返回）。
///
/// 由 `user_trap_exit` 在所有既有 witness 之后调用；此时 task 1 为 Running、task 2 为 Ready。
pub fn run() -> ! {
    let (f1, f2) = crate::user::spin_frames();
    sched::set_frame(INIT_TASK_ID, f1);
    sched::set_frame(TASK2_ID, f2);
    init_timer();
    PREEMPT_ENABLE.store(true, Ordering::Relaxed);
    sched::resume(INIT_TASK_ID)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> PreemptWitness {
        PreemptWitness {
            ticks: 4,
            preempt1: 2,
            preempt2: 2,
            root1: 0x1000,
            root2: 0x2000,
            cr31: 0x1000,
            cr32: 0x2000,
            rsp1: 0xFFFF_8000_0001_0400,
            rsp2: 0xFFFF_8000_0002_0400,
            kstack1_bottom: 0xFFFF_8000_0001_0000,
            kstack1_top: 0xFFFF_8000_0001_4000,
            kstack2_bottom: 0xFFFF_8000_0002_0000,
            kstack2_top: 0xFFFF_8000_0002_4000,
            data1: 0x3000,
            data2: 0x4000,
            mark1: 1,
            mark2: 2,
        }
    }

    #[test]
    fn accepts_complete_preempt_evidence() {
        assert!(valid().evidence_ok());
    }

    #[test]
    fn rejects_shared_address_space() {
        let mut w = valid();
        w.root2 = w.root1;
        w.cr32 = w.root1;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_unrotated_tasks() {
        let mut w = valid();
        w.preempt2 = 1; // task 2 只被抢占一次 ⇒ 未真正轮转
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_same_kernel_stack() {
        let mut w = valid();
        w.rsp2 = w.rsp1;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_shared_physical_frame() {
        let mut w = valid();
        w.data2 = w.data1;
        assert!(!w.evidence_ok());
    }

    #[test]
    fn rejects_wrong_marker() {
        let mut w = valid();
        w.mark2 = 1; // 两任务写了同一标记 ⇒ 地址空间未隔离
        assert!(!w.evidence_ok());
    }

    #[test]
    fn timer_vector_is_master_pic_base() {
        assert_eq!(TIMER_VECTOR, 0x20);
    }
}
