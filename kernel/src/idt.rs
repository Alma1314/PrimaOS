//! IDT 装载与最小 interrupt gate（vector 3 / breakpoint）trap 闭环的细化
//! (refine) 模块：承载 `lidt`/`int3` 与 `extern "x86-interrupt"` handler 的
//! `asm!`。`no_std` 由 crate 根 (`lib.rs`) 统一设置，故本模块无需（也不能）再
//! 声明 `#![no_std]` 内层属性。

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// IDT 条目（64-bit interrupt gate，16 字节）。
///
/// x86-64 中断门描述符共 16 字节：offset_low(2) + selector(2) + ist(1) +
/// type_attr(1) + offset_mid(2) + offset_high(4) + reserved(4)。`reserved` 必须
/// 存在，否则条目只有 12 字节，与 CPU 按 16 字节定步长的 IDT 不符。
#[repr(C, packed(2))]
#[derive(Clone, Copy, Default)]
pub struct Entry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

/// IDT 表（256 项）。
#[repr(C, packed(2))]
#[derive(Clone)]
pub struct Idt {
    entries: [Entry; 256],
}

/// CPU 压入的中断帧（同级 trap：int3 压 rip/cs/rflags）。
#[repr(C)]
#[derive(Clone, Copy)]
pub struct InterruptStackFrame {
    pub instruction_pointer: u64,
    pub code_segment: u64,
    pub cpu_flags: u64,
}

impl Idt {
    pub const fn new() -> Self {
        // 用 const 初始化器填零，避免 const 上下文中调用 Entry::default()（E0015）。
        Idt {
            entries: [const {
                Entry {
                    offset_low: 0,
                    selector: 0,
                    ist: 0,
                    type_attr: 0,
                    offset_mid: 0,
                    offset_high: 0,
                    reserved: 0,
                }
            }; 256],
        }
    }

    /// 把 `handler` 装入第 `index` 号向量，selector 为内核代码段 RPL0（0x8）。
    pub fn set_handler(&mut self, index: u8, handler: HandlerFunc) {
        let h = handler as *const () as u64;
        self.entries[index as usize] = Self::gate_entry(h);
    }

    /// 直接把 `handler_addr` 装入第 `index` 号向量。用于注册非 `HandlerFunc`
    /// 形态的裸函数入口（如 #PF 的 `page_fault_entry`），它们由 CPU 经 IDT 直接
    /// 调用、自带手动现场处理与 iretq。
    pub fn set_handler_addr(&mut self, index: u8, handler_addr: u64) {
        self.entries[index as usize] = Self::gate_entry(handler_addr);
    }

    /// 构造一个指向 `handler_addr` 的 64-bit interrupt gate 条目。
    fn gate_entry(handler_addr: u64) -> Entry {
        Entry {
            offset_low: (handler_addr & 0xffff) as u16,
            selector: 0x8,
            ist: 0,
            type_attr: 0x8e, // present | DPL0 | 64-bit interrupt gate
            offset_mid: ((handler_addr >> 16) & 0xffff) as u16,
            offset_high: (handler_addr >> 32) as u32,
            reserved: 0,
        }
    }
}

/// x86-interrupt handler 类型（编译器生成 iretq）。
pub type HandlerFunc = extern "x86-interrupt" fn(&mut InterruptStackFrame);

impl Default for Idt {
    fn default() -> Self {
        Self::new()
    }
}

/// 装载 IDT。
#[allow(clippy::result_unit_err)]
pub fn load_idt(idt: &Idt) -> Result<(), ()> {
    let limit = (size_of::<Idt>() - 1) as u16;
    let base = idt as *const Idt as u64;
    let mut idtr = [0u8; 10];
    idtr[0..2].copy_from_slice(&limit.to_le_bytes());
    idtr[2..10].copy_from_slice(&base.to_le_bytes());
    unsafe {
        asm!("lidt [{}]", in(reg) idtr.as_ptr(), options(nostack, readonly, preserves_flags));
    }
    Ok(())
}

static TRAP_VECTOR_SEEN: AtomicU8 = AtomicU8::new(0);

/// vector 3（breakpoint）handler —— 记录触发，返回后 iretq 回到 int3 之后。
extern "x86-interrupt" fn breakpoint_handler(_frame: &mut InterruptStackFrame) {
    TRAP_VECTOR_SEEN.store(3, Ordering::Relaxed);
}

/// 触发一次 int3（breakpoint）。由 IDT vector 3 处理，返回后继续。
pub fn trigger_int3() {
    unsafe {
        asm!("int3", options(nostack));
    }
}

/// 读取已触发的向量号（初始 0；int3 后为 3）。
pub fn trap_seen() -> u8 {
    TRAP_VECTOR_SEEN.load(Ordering::Relaxed)
}

/// 记录最近一次 #PF 的 CR2 地址（初始 0；触发后为缺页地址）。
///
/// 由 `page_fault_entry` 裸入口写入、`fault_addr` 读取。static 被子线程无关地以
/// 原子语义读、以裸 mov 写在单核 boot 阶段写——同一处理器上无竞争。
static FAULT_ADDR: AtomicU64 = AtomicU64::new(0);

/// 记录最近一次 #PF 的 error code（初始 0）。由裸入口在弹出前写入、`last_fault_err` 读取。
static FAULT_ERROR: AtomicU64 = AtomicU64::new(0);

/// 触发 #PF 的故障指令长度（字节）。
///
/// #PF 是 fault 类异常：CPU 把返回地址设为故障指令本身，iretq 后会用同一 rip
/// 重试故障指令，若不加处理将无限循环。`page_fault_entry` 把返回地址前移该
/// 常量，落到故障指令之后继续执行。与 `trigger_page_fault` 的 `mov rax, [rax]`
/// 指令（REX.W 8B + modrm，固定 3 字节）成对保证。
const FAULT_SKIP_LEN: u64 = 3;

/// vector 14（page fault）入口（裸函数）。
///
/// #PF 是带 error code 的 fault 类异常。naked 入口手动完成最小现场处理：
/// 读 CR2 记录到 `FAULT_ADDR`、把返回地址前移 `FAULT_SKIP_LEN`（跳过触发指令，
/// 避免 iretq 后重试同一故障指令而死循环）、弹出硬件压栈的 error code，再 iretq。
/// 之所以不用 Rust 的 `extern "x86-interrupt"` 第二参（error code）ABI，是因为当前
/// nightly 工具链（2026-07-30）对带 error code 的 x86-interrupt 函数在
/// `x86_64-unknown-none` 上会误生成 16 字节对齐错误；裸入口自控寄存与栈更直接。
///
/// # Safety
///
/// 该函数是 naked 的异常入口，只能由 CPU 在 #PF（vector 14）经 IDT 调用，不能
/// 当作普通函数从 Rust 侧调用。其 naked 汇编硬编码触发了入口栈布局假设：进入时
/// `rsp` 指向硬件压栈的 error code，且返回地址（rip/cs/rflags）在其上方 8/16/24
/// 字节处；它会在 iretq 前弹出 error code 并把 rip 前移 `FAULT_SKIP_LEN`。任何
/// 违反该进入约定的调用都是未定义行为。
#[unsafe(naked)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn page_fault_entry() -> ! {
    core::arch::naked_asm!(
        // 保存将被使用的 callee-saved 寄存器（触发路径的现场不允许破坏）。
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        // 读 CR2 并写入 FAULT_ADDR（既有）。
        "mov rax, cr2",
        "lea rbx, [rip + {fault}]",
        "mov qword ptr [rbx], rax",
        // 保存硬件压栈的 error code：6 个 push 后位于 [rsp+48]，pop/弹栈前读取。
        "lea rdi, [rip + {err}]",
        "mov rsi, qword ptr [rsp + 48]",
        "mov qword ptr [rdi], rsi",
        // 返回地址前移 FAULT_SKIP_LEN，跳过故障指令。
        // 栈（6 个 push 之后）：[0..47] 保存的寄存器；[48] error_code；
        // [56] rip（故障指令）；[64] cs；[72] rflags。
        "mov rax, qword ptr [rsp + 56]",
        "add rax, {skip}",
        "mov qword ptr [rsp + 56], rax",
        // 恢复被保存的寄存器。
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        // 弹出硬件压栈的 error code，使 rsp 指向 rip。
        "add rsp, 8",
        "iretq",
        fault = sym FAULT_ADDR,
        err = sym FAULT_ERROR,
        skip = const FAULT_SKIP_LEN,
    )
}

/// 读取最近一次 #PF 的 CR2 地址（初始 0）。
pub fn fault_addr() -> u64 {
    FAULT_ADDR.load(Ordering::Relaxed)
}

/// 读取最近一次 #PF 的 error code（初始 0；CPU 压入为 64 位零扩展，低 32 位有效）。
pub fn last_fault_err() -> u64 {
    FAULT_ERROR.load(Ordering::Relaxed)
}

/// 触发一次读 `addr` 的 page fault：读该线性地址，若未映射则 #PF。
///
/// 故障指令固定为 `mov tmp, [addr]`（REX.W 8B，基址寻址，3 字节），与 handler
/// 的 `FAULT_SKIP_LEN` 成对：handler 记录 CR2 并把返回地址前移 3 字节后 iretq，
/// CPU 落到该指令之后继续执行（不会重试同一故障指令而死循环）。
pub fn trigger_page_fault(addr: u64) {
    unsafe {
        core::arch::asm!(
            "mov rax, qword ptr [rax]",
            inout("rax") addr => _,
            options(nostack, preserves_flags)
        );
    }
}

// 静态持有 IDT：lidt 后需存活整个执行。
static mut IDT_ITEM: Idt = Idt::new();

/// 供 `main.rs` 调用：初始化 IDT（vector 3 指向 breakpoint handler）并装载。
///
/// 单核 boot 早期仅调用一次；通过 `addr_of_mut!`/`addr_of!` 访问静态，避免
/// 2024 edition 的 `static_mut_refs` 拒绝。
#[allow(clippy::result_unit_err)]
pub fn init_idt() -> Result<(), ()> {
    unsafe {
        IDT_ITEM = Idt::new();
    }
    let rt_mut = core::ptr::addr_of_mut!(IDT_ITEM);
    unsafe {
        (*rt_mut).set_handler(3, breakpoint_handler);
        // #PF 用裸入口（自带现场处理与 iretq）。
        let entry_addr = page_fault_entry as *const () as u64;
        (*rt_mut).set_handler_addr(14, entry_addr);
    }
    let rt = core::ptr::addr_of!(IDT_ITEM);
    load_idt(unsafe { &*rt })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idt_entry_size_is_16() {
        assert_eq!(size_of::<Entry>(), 16);
    }

    #[test]
    fn idt_total_size_is_4096() {
        assert_eq!(size_of::<Idt>(), 256 * 16);
    }

    #[test]
    fn default_entry_is_not_present() {
        let e = Entry::default();
        assert_eq!(e.type_attr, 0);
    }

    #[test]
    fn set_handler_encodes_offsets_and_selector() {
        let mut idt = Idt::new();
        // 用真实 handler 地址验证编码位。这里不写死具体值，而是与
        // breakpoint_handler 自身取出的地址对账，从而既测试 set_handler 又验证 offset 分段。
        let handler_addr = breakpoint_handler as *const () as u64;
        idt.set_handler(3, breakpoint_handler);
        let e = idt.entries[3];
        let encoded =
            (e.offset_low as u64) | ((e.offset_mid as u64) << 16) | ((e.offset_high as u64) << 32);
        assert_eq!(encoded, handler_addr);
        assert_eq!(e.selector, 0x8);
        assert_eq!(e.ist, 0);
        assert_eq!(e.type_attr, 0x8e);
    }

    #[test]
    fn set_handler_addr_encodes_naked_entry_offsets_and_selector() {
        let mut idt = Idt::new();
        // 裸函数入口同样可经 gate 编码装入 IDT：#PF entry 地址分段写入描述符。
        let entry_addr = page_fault_entry as *const () as u64;
        idt.set_handler_addr(14, entry_addr);
        let e = idt.entries[14];
        let encoded =
            (e.offset_low as u64) | ((e.offset_mid as u64) << 16) | ((e.offset_high as u64) << 32);
        assert_eq!(encoded, entry_addr);
        assert_eq!(e.selector, 0x8);
        assert_eq!(e.ist, 0);
        assert_eq!(e.type_attr, 0x8e);
    }

    #[test]
    fn last_fault_err_defaults_to_zero() {
        assert_eq!(FAULT_ERROR.load(Ordering::Relaxed), 0);
        assert_eq!(last_fault_err(), 0);
    }

    #[test]
    fn interrupt_stack_frame_field_layout() {
        // 三条 u64：rip, cs, rflags —— 顺序与 CPU 压入一致。
        let f = InterruptStackFrame {
            instruction_pointer: 0xdead_beef,
            code_segment: 0x8,
            cpu_flags: 0x202,
        };
        let bytes = unsafe {
            core::slice::from_raw_parts(
                &f as *const _ as *const u8,
                size_of::<InterruptStackFrame>(),
            )
        };
        assert_eq!(&bytes[0..8], &0xdead_beefu64.to_le_bytes());
        assert_eq!(&bytes[8..16], &0x8u64.to_le_bytes());
        assert_eq!(&bytes[16..24], &0x202u64.to_le_bytes());
    }
}
