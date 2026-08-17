//! IDT 装载与最小 interrupt gate（vector 3 / breakpoint）trap 闭环的细化
//! (refine) 模块：承载 `lidt`/`int3` 与 `extern "x86-interrupt"` handler 的
//! `asm!`。`no_std` 由 crate 根 (`lib.rs`) 统一设置，故本模块无需（也不能）再
//! 声明 `#![no_std]` 内层属性。

use core::arch::asm;
use core::mem::size_of;
use core::sync::atomic::{AtomicU8, Ordering};

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
        // 经 `*const ()` 中转把 fn 指针转地址，规避 `clippy::fn_to_numeric_cast`。
        let h = handler as *const () as u64;
        self.entries[index as usize] = Entry {
            offset_low: (h & 0xffff) as u16,
            selector: 0x8,
            ist: 0,
            type_attr: 0x8e, // present | DPL0 | 64-bit interrupt gate
            offset_mid: ((h >> 16) & 0xffff) as u16,
            offset_high: (h >> 32) as u32,
            reserved: 0,
        };
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
