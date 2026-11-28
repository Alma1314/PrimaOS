//! P5-3b：入口现场按 CPU 分区的受控探针（仅 `entry-percpu-probe` 镜像）。
//!
//! P5-3b-1 的 CPL0 部分：每个 CPU 在自己的入口执行一次 `int 0x80` 与一次未映射读 #PF，
//! 回调只写本 CPU 的见证槽。P5-3b-2 追加 CPL3 部分：每个 CPU 构造本 CPU 的最小用户根
//! （代码/栈/数据页），经每 CPU 恢复跳板进入 CPL3，用户代码依次触发写未映射页 #PF、
//! `int 0x80` 与 `syscall` 快路径，最后以一次 `int 0x80` 收尾。所有入口现场（保存块
//! 基址、CR2/error/现场帧、用户返回现场、快路径栈与 `USER_RSP`、恢复帧指针）都按本 CPU
//! 槽访问；任何「单槽 + 加锁」或「固定槽 0」实现都会让至少一个 CPU 读回零或被他 CPU
//! 覆盖，从而被本探针拒绝。**不是运行期启用**：普通镜像不含本模块。

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use prima_ostd::entry::{self, AbiFrame, SaveBlock, SretFrame, UserFrame};

use crate::gdt::{USER_CS, USER_DS};
use crate::mem::FRAME_SIZE;

const MAX: usize = crate::cpu::local::MAX_CPUS;
/// P5-3b-1：CPL0 探针的调用号与返回值编码。
const NUM_MAGIC: u64 = 0x5035_0100;
const RET_MAGIC: u64 = 0x5035_1a00;
/// P5-3b-2：CPL3 `int 0x80` / `syscall` / 收尾的调用号与返回值编码。
const USER_NUM_MAGIC: u64 = 0x5035_0200;
const FAST_NUM_MAGIC: u64 = 0x5035_0300;
const FINISH_MAGIC: u64 = 0x5035_0400;
const RET_USER: u64 = 0x5035_2a00;
const RET_FAST: u64 = 0x5035_3a00;

/// 每 CPU 快路径内核栈（探针显式映射，避免依赖启动器对 .bss 尾部的映射）。
const K_STACK_VA: u64 = 0x0000_0A00_0000_4000;
const K_STACK_PAGES: usize = 2;
const K_STACK_STRIDE: u64 = 0x4000;
const K_STACK_CANARY: u64 = 0x5035_57AC_4B57_A55A;

/// 每 CPU 用户区（同 VA，不同根/帧）：代码、栈、数据页；故障页保持未映射。
const U_CODE_VA: u64 = 0x0000_0A00_0000_0000;
const U_STACK_VA: u64 = U_CODE_VA + 0x1000;
const U_DATA_VA: u64 = U_CODE_VA + 0x2000;
const U_FAULT_VA: u64 = U_CODE_VA + 0x3000;

static NUM_OK: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static HANDLER_ADDR: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
static HANDLER_RIP: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
static SELF_SYSCALL: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static SELF_CTX: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static SELF_FAULT: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static SELF_FRAME: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
/// 本 CPU 进入过 CPL3 用户探针（收尾时置 1）。
static USER_DONE: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
/// 本 CPU 用户根构建失败（诊断用）与失败步骤码。
static ROOT_FAILED: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static ROOT_STEP: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static USER_INT80: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static USER_FAST: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static FAST_CTX: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static USER_FAULT: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
static USER_FRAME_OK: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];
/// 本 CPU 的用户数据页物理帧（回执读取用）。
static DATA_FRAME: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
/// 本 CPU 内核栈区间（CPL0 与 CPL3 入口保存块都必须落在其中）。
static KSTACK_LO: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
static KSTACK_HI: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
/// BSP 收尾时汇总的 CPU 数（由 BSP 写入；AP 写 0 不参与）。
static EXPECTED_CPUS: AtomicU32 = AtomicU32::new(0);
/// 物理直映射偏移（BSP/AP 写同一值，回执读取用）。
static PHYS_OFFSET: AtomicU64 = AtomicU64::new(0);
/// 每 CPU 快路径内核栈的显式映射区间与栈底哨兵。
static FAST_STACK_LO: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
static FAST_STACK_HI: [AtomicU64; MAX] = [const { AtomicU64::new(0) }; MAX];
/// 本 CPU 在自身根下校验自己的快路径栈哨兵（跨 CPU 根的 VA 不可互读）。
static FAST_CANARY_OK: [AtomicU32; MAX] = [const { AtomicU32::new(0) }; MAX];

fn current_cpu() -> usize {
    let cpu = crate::cpu::local::current_index();
    assert!(cpu < MAX, "entry probe cpu index out of range");
    cpu
}

/// 每 CPU 一个互不相同的未映射低地址；避开 AP 跳板页（< 0x9f000）。
fn expected_fault_va(cpu: usize) -> u64 {
    0x20_0000 + cpu as u64 * 0x1000
}

fn in_kstack(cpu: usize, block: SaveBlock) -> bool {
    let (lo, hi) = (
        KSTACK_LO[cpu].load(Ordering::SeqCst),
        KSTACK_HI[cpu].load(Ordering::SeqCst),
    );
    lo != 0 && (lo..hi).contains(&block.addr())
}

extern "C" fn probe_syscall(frame: AbiFrame) -> u64 {
    let cpu = current_cpu();
    let num = frame.num();
    if num == FINISH_MAGIC + cpu as u64 {
        finish(cpu);
    }
    if num == USER_NUM_MAGIC + cpu as u64 {
        // CPL3：调用号/参数含本 CPU 编码，保存块基址须落在本 CPU 的内核栈上。
        let ok = frame.arg(0) == cpu as u64
            && frame.cs() == USER_CS as u64
            && entry::percpu_context_base(cpu).is_some_and(|block| in_kstack(cpu, block));
        USER_INT80[cpu].store(u32::from(ok), Ordering::SeqCst);
        return RET_USER + cpu as u64;
    }
    let ok = num == NUM_MAGIC + cpu as u64 && frame.arg(0) == cpu as u64;
    NUM_OK[cpu].store(u32::from(ok), Ordering::SeqCst);
    RET_MAGIC + cpu as u64
}

extern "C" fn probe_fast(frame: AbiFrame) -> u64 {
    let cpu = current_cpu();
    let (lo, hi) = (
        FAST_STACK_LO[cpu].load(Ordering::SeqCst),
        FAST_STACK_HI[cpu].load(Ordering::SeqCst),
    );
    let ctx_ok = lo != 0
        && entry::percpu_context_base(cpu).is_some_and(|block| (lo..hi).contains(&block.addr()));
    let ok = frame.num() == FAST_NUM_MAGIC + cpu as u64
        && frame.arg(0) == cpu as u64
        && frame.cs() == USER_CS as u64
        && ctx_ok;
    USER_FAST[cpu].store(u32::from(ok), Ordering::SeqCst);
    FAST_CTX[cpu].store(u32::from(ctx_ok), Ordering::SeqCst);
    RET_FAST + cpu as u64
}

extern "C" fn probe_fault(frame: &SretFrame) -> u64 {
    let cpu = current_cpu();
    HANDLER_ADDR[cpu].store(entry::percpu_fault_addr(cpu), Ordering::SeqCst);
    HANDLER_RIP[cpu].store(frame.rip, Ordering::SeqCst);
    if frame.cs & 3 == 3 {
        // CPL3：写未映射页 → P=0,W=1,US=1（0x6）；返回现场须是用户选择子与用户栈。
        let uf: UserFrame = entry::percpu_user_frame(cpu);
        let fault_ok =
            entry::percpu_fault_addr(cpu) == U_FAULT_VA && entry::percpu_fault_error(cpu) == 0x6;
        let frame_ok = uf.cs == USER_CS as u64
            && uf.ss == USER_DS as u64
            && uf.rip == frame.rip
            && (U_STACK_VA..U_STACK_VA + FRAME_SIZE).contains(&uf.rsp);
        USER_FAULT[cpu].store(u32::from(fault_ok), Ordering::SeqCst);
        USER_FRAME_OK[cpu].store(u32::from(frame_ok), Ordering::SeqCst);
    }
    frame.rip + crate::idt::FAULT_SKIP_LEN
}

/// 在 BSP 启动 AP 之前调用：把 syscall/快路径/#PF 回调换成探针版本。
///
/// 镜像在见证后立即退出，不恢复普通回调；这保证探针不会污染后续启动。
pub fn install_handlers() {
    entry::set_syscall_handler(probe_syscall);
    entry::set_fast_syscall_handler(probe_fast);
    entry::set_page_fault_handler(probe_fault);
    entry::set_cpu_exception_handler(probe_cpu_exception);
}

/// 诊断：CPU 异常（#UD/#GP 等）直接把向量写串口后停机，避免落入完整调度路径。
extern "C" fn probe_cpu_exception(_tag: u64, _block: SaveBlock, _error: u64) -> ! {
    loop {
        prima_ostd::arch::hlt();
    }
}

/// 在当前 CPU 上执行 CPL0 `int 0x80` 与一次 #PF，把四项自检写入本 CPU 槽。
///
/// `stack_lo..stack_hi` 是本 CPU 当前内核栈范围：入口保存的保存块基址必须落在本 CPU
/// 自己的栈上。共享单槽会让后运行的 CPU 覆盖前者的值，使前者的槽读回零或落在别处。
pub fn run_local(cpu: usize, stack_lo: u64, stack_hi: u64) {
    assert_eq!(cpu, current_cpu(), "run_local must run on its own CPU");
    KSTACK_LO[cpu].store(stack_lo, Ordering::SeqCst);
    KSTACK_HI[cpu].store(stack_hi, Ordering::SeqCst);

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

/// 每 CPU CPL0 探针见证；四项计数都等于 `cpus` 才算通过。
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

/// 每 CPU CPL3 探针见证；全部计数都等于 `cpus` 才算通过。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UserWitness {
    pub cpus: u32,
    pub int80_ok: u32,
    pub fast_ok: u32,
    pub fault_ok: u32,
    pub frame_ok: u32,
    pub fast_ctx_ok: u32,
    pub fast_rsp_ok: u32,
}

impl UserWitness {
    pub fn evidence_ok(&self) -> bool {
        self.int80_ok == self.cpus
            && self.fast_ok == self.cpus
            && self.fault_ok == self.cpus
            && self.frame_ok == self.cpus
            && self.fast_ctx_ok == self.cpus
            && self.fast_rsp_ok == self.cpus
    }
}

/// 汇总 CPL0 各 CPU 槽；同时核对两面槽数一致。
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

/// CPL3 回执：用户代码把 `int 0x80`/`syscall` 返回值写回本 CPU 数据页。
fn receipt(cpu: usize, offset: u64) -> Option<(u64, u64)> {
    let phys = DATA_FRAME[cpu].load(Ordering::SeqCst);
    if phys == 0 {
        return None;
    }
    let va = crate::mem::phys_to_virt(phys, offset);
    Some((
        prima_ostd::mm::read_u64(va),
        prima_ostd::mm::read_u64(va + 8),
    ))
}

/// 汇总 CPL3 各 CPU 槽。
pub fn user_witness(cpus: u32) -> UserWitness {
    let offset = PHYS_OFFSET.load(Ordering::SeqCst);
    let mut witness = UserWitness {
        cpus,
        int80_ok: 0,
        fast_ok: 0,
        fault_ok: 0,
        frame_ok: 0,
        fast_ctx_ok: 0,
        fast_rsp_ok: 0,
    };
    for cpu in 0..cpus as usize {
        let (rcpt_int80, rcpt_fast) = receipt(cpu, offset).unwrap_or((0, 0));
        let int80 =
            USER_INT80[cpu].load(Ordering::SeqCst) == 1 && rcpt_int80 == RET_USER + cpu as u64;
        let fast = USER_FAST[cpu].load(Ordering::SeqCst) == 1 && rcpt_fast == RET_FAST + cpu as u64;
        let rsp = crate::cpu::local::user_rsp_on(cpu);
        let rsp_ok = (U_STACK_VA..U_STACK_VA + FRAME_SIZE).contains(&rsp);
        witness.int80_ok += u32::from(int80);
        witness.fast_ok += u32::from(fast);
        witness.fault_ok += USER_FAULT[cpu].load(Ordering::SeqCst);
        witness.frame_ok += USER_FRAME_OK[cpu].load(Ordering::SeqCst);
        witness.fast_ctx_ok += FAST_CTX[cpu].load(Ordering::SeqCst);
        let canary = FAST_CANARY_OK[cpu].load(Ordering::SeqCst) != 0;
        witness.fast_rsp_ok += u32::from(rsp_ok && canary);
    }
    witness
}

/// 失败诊断：CPL0 各 CPU 四位自检（`[syscall, ctx, fault, frame]`）。
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

/// 失败诊断：CPL3 各 CPU 六位自检。
pub fn user_progress() -> [[u32; 8]; MAX] {
    core::array::from_fn(|cpu| {
        [
            USER_DONE[cpu].load(Ordering::SeqCst),
            USER_INT80[cpu].load(Ordering::SeqCst),
            USER_FAST[cpu].load(Ordering::SeqCst),
            FAST_CTX[cpu].load(Ordering::SeqCst),
            USER_FAULT[cpu].load(Ordering::SeqCst),
            USER_FRAME_OK[cpu].load(Ordering::SeqCst),
            FAST_CANARY_OK[cpu].load(Ordering::SeqCst),
            u32::from(
                (U_STACK_VA..U_STACK_VA + FRAME_SIZE)
                    .contains(&crate::cpu::local::user_rsp_on(cpu)),
            ),
        ]
    })
}

/// 用户代码（每 CPU 独立代码页）：写未映射页 → `int 0x80` → `syscall` → 收尾。
fn write_user_code(cpu: usize, frame: u64, offset: u64) -> bool {
    crate::mem::zero_frame(frame, offset);
    let mut buf = [0u8; 160];
    let mut at = 0usize;
    emit_movabs_rax(&mut buf, &mut at, U_FAULT_VA);
    emit_fixed(&mut buf, &mut at, &[0x48, 0x89, 0x00]); // mov [rax], rax → 写未映射页 #PF
    emit_movabs_rax(&mut buf, &mut at, USER_NUM_MAGIC + cpu as u64);
    emit_mov_rdi_imm(&mut buf, &mut at, cpu as u32);
    emit_fixed(&mut buf, &mut at, &[0xCD, 0x80]); // int 0x80
    emit_movabs_rdx(&mut buf, &mut at, U_DATA_VA);
    emit_fixed(&mut buf, &mut at, &[0x48, 0x89, 0x02]); // mov [rdx], rax
    emit_movabs_rax(&mut buf, &mut at, FAST_NUM_MAGIC + cpu as u64);
    emit_mov_rdi_imm(&mut buf, &mut at, cpu as u32);
    emit_fixed(&mut buf, &mut at, &[0x0F, 0x05]); // syscall
    emit_movabs_rdx(&mut buf, &mut at, U_DATA_VA + 8);
    emit_fixed(&mut buf, &mut at, &[0x48, 0x89, 0x02]); // mov [rdx], rax
    emit_movabs_rax(&mut buf, &mut at, FINISH_MAGIC + cpu as u64);
    emit_mov_rdi_imm(&mut buf, &mut at, cpu as u32);
    emit_fixed(&mut buf, &mut at, &[0xCD, 0x80]); // int 0x80（收尾，不返回）
    emit_fixed(&mut buf, &mut at, &[0x0F, 0x0B]); // ud2
    prima_ostd::mm::write_bytes(crate::mem::phys_to_virt(frame, offset), &buf[..at]);
    true
}

fn emit_movabs_rax(buf: &mut [u8; 160], at: &mut usize, value: u64) {
    let mut bytes = [0u8; 10];
    bytes[0] = 0x48;
    bytes[1] = 0xB8;
    bytes[2..].copy_from_slice(&value.to_le_bytes());
    buf[*at..*at + 10].copy_from_slice(&bytes);
    *at += 10;
}

fn emit_movabs_rdx(buf: &mut [u8; 160], at: &mut usize, value: u64) {
    let mut bytes = [0u8; 10];
    bytes[0] = 0x48;
    bytes[1] = 0xBA;
    bytes[2..].copy_from_slice(&value.to_le_bytes());
    buf[*at..*at + 10].copy_from_slice(&bytes);
    *at += 10;
}

fn emit_mov_rdi_imm(buf: &mut [u8; 160], at: &mut usize, value: u32) {
    let mut bytes = [0u8; 7];
    bytes[..3].copy_from_slice(&[0x48, 0xC7, 0xC7]);
    bytes[3..].copy_from_slice(&value.to_le_bytes());
    buf[*at..*at + 7].copy_from_slice(&bytes);
    *at += 7;
}

fn emit_fixed(buf: &mut [u8; 160], at: &mut usize, bytes: &[u8]) {
    buf[*at..*at + bytes.len()].copy_from_slice(bytes);
    *at += bytes.len();
}

/// 在当前根上建立本 CPU 的用户代码/栈/数据页（同一组 VA，不同物理帧）。
fn build_user_pages(cpu: usize, offset: u64) -> Result<(), u32> {
    let root = crate::cpu::cr3_phys();
    let map = |phys: u64| crate::mem::phys_to_virt(phys, offset);
    // BSP 使用启动器根：拒绝覆盖启动器已建的映射。AP 共享跳板根，节点复用探针
    // 自己在上一次 AP 进入时建立的用户页（覆盖叶 PTE 即换成本 CPU 的帧）。
    if cpu == crate::cpu::local::BSP {
        for va in [U_CODE_VA, U_STACK_VA, U_DATA_VA] {
            if !crate::vspace::slot_free(root, va, map) {
                return Err(1);
            }
        }
    }
    let code = crate::frames::allocate_table_frame().map_err(|_| 2u32)?;
    if !write_user_code(cpu, code, offset) {
        return Err(3);
    }
    crate::vspace::map_4k(root, U_CODE_VA, map, alloc_frame, code, false, true, |_| {})
        .map_err(|_| 4u32)?;
    // 数据页同时映射为用户栈（探针用户代码不压栈，共用一帧即可）。
    let data = crate::frames::allocate_table_frame().map_err(|_| 5u32)?;
    crate::mem::zero_frame(data, offset);
    for va in [U_STACK_VA, U_DATA_VA] {
        crate::vspace::map_4k(root, va, map, alloc_frame, data, true, true, |_| {})
            .map_err(|_| 6u32)?;
        crate::vspace::set_executable(root, va, false, map).map_err(|_| 7u32)?;
    }
    DATA_FRAME[cpu].store(data, Ordering::SeqCst);
    // 每 CPU 快路径内核栈：显式映射到当前根，避免依赖启动器只映射到镜像尾部。
    let base = K_STACK_VA + cpu as u64 * K_STACK_STRIDE;
    for page in 0..K_STACK_PAGES {
        let frame = crate::frames::allocate_table_frame().map_err(|_| 8u32)?;
        crate::mem::zero_frame(frame, offset);
        crate::vspace::map_4k(
            root,
            base + page as u64 * FRAME_SIZE,
            map,
            alloc_frame,
            frame,
            true,
            false,
            |_| {},
        )
        .map_err(|_| 9u32)?;
        crate::vspace::set_executable(root, base + page as u64 * FRAME_SIZE, false, map)
            .map_err(|_| 10u32)?;
    }
    prima_ostd::mm::write_u64(base, K_STACK_CANARY);
    let top = base + K_STACK_PAGES as u64 * FRAME_SIZE;
    FAST_STACK_LO[cpu].store(base, Ordering::SeqCst);
    FAST_STACK_HI[cpu].store(top, Ordering::SeqCst);
    crate::cpu::local::set_syscall_stack_top(cpu, top);
    Ok(())
}

fn alloc_frame() -> Option<u64> {
    crate::frames::allocate_table_frame().ok()
}

/// 在当前 CPU 上执行 CPL3 用户探针；返回前 `finish` 收尾（不返回）。
///
/// BSP 传入 `cpus`（在线总数）供收尾汇总；AP 传 0。
pub fn run_user_local(cpu: usize, offset: u64, cpus: u32) -> ! {
    assert_eq!(cpu, current_cpu(), "run_user_local must run on its own CPU");
    EXPECTED_CPUS.store(cpus, Ordering::SeqCst);
    PHYS_OFFSET.store(offset, Ordering::SeqCst);
    if let Err(step) = build_user_pages(cpu, offset) {
        ROOT_STEP[cpu].store(step, Ordering::SeqCst);
        ROOT_FAILED[cpu].store(1, Ordering::SeqCst);
        entry::finish_now(cpu);
    }
    // 每 CPU 快路径 MSR：LSTAR 指向每 CPU 入口（其余 MSR 也按 CPU 写）。
    crate::syscall::configure_fast_path();
    crate::msr::wrmsr(
        crate::msr::IA32_LSTAR,
        entry::syscall_fast_entry_percpu_addr(),
    );
    assert_eq!(
        crate::cpu::local::syscall_stack_top_on(cpu),
        FAST_STACK_HI[cpu].load(Ordering::SeqCst),
        "gs stack top"
    );
    entry::enable_gs_swap();
    let frame = SretFrame {
        rflags: 0x2,
        rip: U_CODE_VA,
        cs: USER_CS as u64,
        rsp: U_STACK_VA + FRAME_SIZE - 8,
        ss: USER_DS as u64,
        ..SretFrame::ZERO
    };
    entry::resume_percpu(cpu, &frame)
}

/// 汇总两面见证（BSP 收尾用；CPU 数取自探针入口）。
pub fn summarize() -> (EntryWitness, UserWitness) {
    let cpus = EXPECTED_CPUS.load(Ordering::SeqCst);
    (witness(cpus), user_witness(cpus))
}

/// 指定 CPU 的用户根构建是否失败（收尾诊断用）。
pub fn user_root_failed(cpu: usize) -> bool {
    ROOT_FAILED
        .get(cpu)
        .is_some_and(|flag| flag.load(Ordering::SeqCst) != 0)
}

/// 指定 CPU 的用户根构建失败步骤码（0 = 未失败）。
pub fn user_root_step(cpu: usize) -> u32 {
    ROOT_STEP
        .get(cpu)
        .map_or(0, |step| step.load(Ordering::SeqCst))
}

/// 用户代码的收尾：记录本 CPU 已完成，再交给注册的收尾回调（AP 停放、BSP 汇总退出）。
fn finish(cpu: usize) -> ! {
    // 在自身根下校验本 CPU 快路径栈哨兵（跨 CPU 根的 VA 不可互读）。
    let canary = FAST_STACK_LO[cpu].load(Ordering::SeqCst) != 0
        && prima_ostd::mm::read_u64(FAST_STACK_LO[cpu].load(Ordering::SeqCst)) == K_STACK_CANARY;
    FAST_CANARY_OK[cpu].store(u32::from(canary), Ordering::SeqCst);
    USER_DONE[cpu].store(1, Ordering::SeqCst);
    entry::finish_now(cpu)
}
