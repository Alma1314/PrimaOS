//! IDT 门编码与装载的细化（refine）模块。
//!
//! 切片 AH 后，**裸入口、`extern "x86-interrupt"` handler、`lidt`/`int3` 与 asm 直接触碰
//! 的静态**全部下沉 [`prima_ostd::entry`]；本模块只保留可在 safe Rust 中完成的**门描述符
//! 字节编码**与装载调用点，以及对外仍以 `crate::idt::*` 暴露的薄转发（保持既有调用方与
//! host 单测不变）。`no_std` 由 crate 根 (`lib.rs`) 统一设置。

use core::mem::size_of;

use crate::exc::SretFrame;
use prima_ostd::sync::StaticCell;

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

    /// 把 `handler_addr` 装入第 `index` 号向量，selector 为内核代码段 RPL0（0x8）、DPL0。
    pub fn set_handler_addr(&mut self, index: u8, handler_addr: u64) {
        self.entries[index as usize] = Self::gate_entry(handler_addr, 0);
    }

    /// 同 [`Idt::set_handler_addr`]，但门 DPL 设为 3。
    ///
    /// 软件中断（如 `int3`/`int 0x80`）在 CPL3 调用时要求门 DPL ≥ CPL，否则 #GP；故用户态
    /// 可触发的向量必须用 DPL3 门。CPL0 调用不受影响（DPL ≥ CPL 仍成立）。
    pub fn set_handler_addr_dpl3(&mut self, index: u8, handler_addr: u64) {
        self.entries[index as usize] = Self::gate_entry(handler_addr, 3);
    }

    /// 构造一个指向 `handler_addr` 的 64-bit interrupt gate 条目（DPL = `dpl`）。
    fn gate_entry(handler_addr: u64, dpl: u8) -> Entry {
        Entry {
            offset_low: (handler_addr & 0xffff) as u16,
            selector: 0x8,
            ist: 0,
            // present | DPL | 64-bit interrupt gate
            type_attr: 0x8e | ((dpl & 3) << 5),
            offset_mid: ((handler_addr >> 16) & 0xffff) as u16,
            offset_high: (handler_addr >> 32) as u32,
            reserved: 0,
        }
    }
}

impl Default for Idt {
    fn default() -> Self {
        Self::new()
    }
}

extern "C" fn cpu_exception(tag: u64, block: prima_ostd::entry::SaveBlock, error: u64) -> ! {
    let with_error = tag & 0x100 != 0;
    let cs = if with_error {
        block.cs_with_error()
    } else {
        block.cs()
    };
    let vector = tag & 0xff;
    assert_eq!(
        cs & 3,
        3,
        "kernel CPU exception vector={vector} error={error:#x}"
    );
    let frame = if with_error {
        block.to_sret_frame_with_error()
    } else {
        block.to_sret_frame()
    };
    crate::syscall::deliver_cpu_fault(frame, vector, error);
    let task = crate::sched::current();
    let kind = match vector {
        0 => crate::bootstrap::ExcKind::DivideError,
        6 => crate::bootstrap::ExcKind::InvalidOpcode,
        _ => crate::bootstrap::ExcKind::GeneralProtectionFault,
    };
    crate::bootstrap::fault_hardware_global(task, kind)
        .expect("hardware fault source/audit capacity");
    let next = crate::sched::pick_next(task).expect("unhandled fault requires a Ready successor");
    crate::sched::fault_to(next)
}

/// 装载 IDT（委派 [`prima_ostd::entry::load_idt`] 执行 `lidt`）。
#[allow(clippy::result_unit_err)]
pub fn load_idt(idt: &Idt) -> Result<(), ()> {
    let limit = (size_of::<Idt>() - 1) as u16;
    prima_ostd::entry::load_idt(idt as *const Idt as u64, limit);
    Ok(())
}

/// 切片 G：注册用户 `int3` 收尾回调（委派框架）。
pub fn set_user_trap_handler(handler: extern "C" fn() -> !) {
    prima_ostd::entry::set_user_trap_handler(handler);
}

/// 触发一次 int3（breakpoint）。由 IDT vector 3 处理，返回后继续。
pub fn trigger_int3() {
    prima_ostd::arch::int3();
}

/// 读取已触发的向量号（初始 0；int3 后为 3）。
pub fn trap_seen() -> u8 {
    prima_ostd::entry::trap_seen()
}

/// 触发 #PF 的故障指令长度（字节）。
///
/// #PF 是 fault 类异常：CPU 把返回地址设为故障指令本身，iretq 后会用同一 rip
/// 重试故障指令，若不加处理将无限循环。裸入口把返回地址前移该常量，落到故障指令之后
/// 继续执行。与 #[`trigger_page_fault`] 的 `mov rax, [rax]` 指令（REX.W 8B + modrm，
/// 固定 3 字节）成对保证。
pub const FAULT_SKIP_LEN: u64 = 3;

/// 切片 H：syscall 陷入向量（`int 0x80`）。
///
/// 复用 IDT 中断门机制而非 `syscall`/`sysret` MSR 快路径：门 DPL=3 允许 CPL3 调用，
/// 特权级切换经 TSS.rsp0 换栈（与 `#PF` 路径相同），返回统一走 `iretq`。这是
/// "OS 中立的 syscall 递送"的最小见证，不宣称 x86-64 `syscall` 快速路径。
pub const SYSCALL_VECTOR: u8 = 0x80;

/// 读取最近一次 #PF 的 CR2 地址（初始 0）。
pub fn fault_addr() -> u64 {
    prima_ostd::entry::fault_addr()
}

/// 读取最近一次 #PF 的 error code（初始 0；CPU 压入为 64 位零扩展，低 32 位有效）。
pub fn last_fault_err() -> u64 {
    prima_ostd::entry::last_fault_err()
}

/// 读取最近一次 #PF 捕获的完整现场帧（初始全零）。
pub fn fault_frame() -> SretFrame {
    prima_ostd::entry::fault_frame()
}

/// 触发一次写只读页的 page fault：向 `addr` 写入，若该页 present 但只读则 #PF。
///
/// 故障指令固定为 `mov qword ptr [rax], rax`（`48 89 00`，3 字节），与 [`FAULT_SKIP_LEN`]
/// 成对。写故障的 error code 为 `P|W`（0x3），`fault::classify` 判为 `WriteProtection`。
pub fn trigger_page_fault_write(addr: u64) {
    prima_ostd::entry::trigger_page_fault_write(addr);
}

/// 触发一次读 `addr` 的 page fault：读该线性地址，若未映射则 #PF。
pub fn trigger_page_fault(addr: u64) {
    prima_ostd::entry::trigger_page_fault(addr);
}

// 静态持有 IDT：lidt 后需存活整个执行。
static IDT_ITEM: StaticCell<Idt> = StaticCell::new(Idt::new());

/// 供 `main.rs` 调用：注册四个入口回调、初始化 IDT 并装载。
///
/// 单核 boot 早期仅调用一次。**注册先于任何触发**：`init_idt` 在所有 #PF/`int 0x80`/
/// 时钟之前执行，故不存在"handler 未注册即被触发"的窗口。
#[allow(clippy::result_unit_err)]
pub fn init_idt() -> Result<(), ()> {
    prima_ostd::entry::set_page_fault_handler(crate::cow::dispatch);
    prima_ostd::entry::set_cpu_exception_handler(cpu_exception);
    prima_ostd::entry::set_external_irq_handler(crate::irq::dispatch);
    prima_ostd::entry::set_syscall_handler(crate::syscall::handle);
    prima_ostd::entry::set_fast_syscall_handler(crate::syscall::handle_fast);
    IDT_ITEM.with_mut(|idt| {
        *idt = Idt::new();
        // vector 3 用 DPL3 门：切片 G 的用户任务在 CPL3 执行 `int3` 陷入内核。
        idt.set_handler_addr_dpl3(3, prima_ostd::entry::breakpoint_handler_addr());
        for vector in [0, 6, 12, 13, 16, 17, 19] {
            idt.set_handler_addr(
                vector,
                prima_ostd::entry::cpu_exception_entry_addr(vector).expect("registered CPU vector"),
            );
        }
        for line in 1..16u8 {
            if let Some(address) = prima_ostd::entry::external_irq_entry_addr(line) {
                idt.set_handler_addr(0x20 + line, address);
            }
        }
        // #PF 用裸入口（自带现场处理与 iretq）。
        idt.set_handler_addr(14, prima_ostd::entry::page_fault_entry_addr());
        // syscall 用 DPL3 门 + 裸入口：允许 CPL3 执行 `int 0x80`。
        idt.set_handler_addr_dpl3(SYSCALL_VECTOR, prima_ostd::entry::syscall_entry_addr());
        // 切片 O：IRQ0（PIT 时钟）用裸入口；中断门 DPL0，IF 由门自动清除。
        idt.set_handler_addr(
            crate::preempt::TIMER_VECTOR,
            prima_ostd::entry::timer_entry_addr(),
        );
    });
    IDT_ITEM.with(load_idt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use prima_ostd::entry::InterruptStackFrame;

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
    fn breakpoint_gate_encodes_offsets_and_selector() {
        let mut idt = Idt::new();
        // 用真实 handler 地址验证编码位。这里不写死具体值，而是与 breakpoint handler
        // 自身取出的地址对账，从而既测试 gate 编码又验证 offset 分段。
        let handler_addr = prima_ostd::entry::breakpoint_handler_addr();
        idt.set_handler_addr(3, handler_addr);
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
        let entry_addr = prima_ostd::entry::page_fault_entry_addr();
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
    fn set_handler_addr_dpl3_encodes_naked_entry_with_dpl3() {
        let mut idt = Idt::new();
        let entry_addr = prima_ostd::entry::syscall_entry_addr();
        idt.set_handler_addr_dpl3(SYSCALL_VECTOR, entry_addr);
        let e = idt.entries[SYSCALL_VECTOR as usize];
        let encoded =
            (e.offset_low as u64) | ((e.offset_mid as u64) << 16) | ((e.offset_high as u64) << 32);
        assert_eq!(encoded, entry_addr);
        assert_eq!(e.selector, 0x8);
        assert_eq!(e.ist, 0);
        // present | DPL3 | 64-bit interrupt gate = 0xee
        assert_eq!(e.type_attr, 0xee);
    }

    #[test]
    fn syscall_vector_is_0x80() {
        assert_eq!(SYSCALL_VECTOR, 0x80);
    }

    #[test]
    fn set_handler_addr_dpl3_sets_gate_dpl() {
        let mut idt = Idt::new();
        idt.set_handler_addr_dpl3(3, prima_ostd::entry::breakpoint_handler_addr());
        let e = idt.entries[3];
        // present | DPL3 | 64-bit interrupt gate = 0xee
        assert_eq!(e.type_attr, 0xee);
        // 其余 DPL0 门不受影响。
        let mut idt2 = Idt::new();
        idt2.set_handler_addr(3, prima_ostd::entry::breakpoint_handler_addr());
        assert_eq!(idt2.entries[3].type_attr, 0x8e);
    }

    #[test]
    fn last_fault_err_defaults_to_zero() {
        assert_eq!(last_fault_err(), 0);
    }

    #[test]
    fn fault_frame_defaults_to_zero() {
        assert_eq!(fault_frame(), SretFrame::ZERO);
    }

    #[test]
    fn interrupt_stack_frame_field_layout() {
        // 三条 u64：rip, cs, rflags —— 顺序与 CPU 压入一致。
        let f = InterruptStackFrame {
            instruction_pointer: 0xdead_beef,
            code_segment: 0x8,
            cpu_flags: 0x202,
        };
        let bytes = prima_ostd::mm::bytes_of(&f);
        assert_eq!(&bytes[0..8], &0xdead_beefu64.to_le_bytes());
        assert_eq!(&bytes[8..16], &0x8u64.to_le_bytes());
        assert_eq!(&bytes[16..24], &0x202u64.to_le_bytes());
    }
}
