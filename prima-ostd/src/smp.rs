//! P5-2：AP 实模式启动跳板模板。
//!
//! 模板是**位置相关的一段字节**：由内核复制到 1 MiB 以下的可执行页，再由 LAPIC
//! INIT-SIPI-SIPI 让 AP 从页首开始执行。模板只做三件事：装载调用方给的 CR3、
//! 打开 PAE/LME/分页、跳到 64 位入口；不初始化 GDT/TSS（P5-2 的 AP 只在内核态
//! idle，per-CPU TSS 留到 P5-3）。
//!
//! 控制流：
//! 1. 16 位实模式：`DS=ES=CS(段=页基址)`，从模板偏移读取 CR3/临时 GDTR；
//! 2. 远跳 `0x08` 进 32 位保护模式（临时 GDT，基址 0），置 CR4.PAE、CR3、EFER.LME、
//!    CR0.PG；
//! 3. 远跳 `0x18` 进 64 位长模式。目标必须在 AP 页表里标识映射：内核在克隆的
//!    AP 根上把模板物理页映射到同址虚拟页；
//! 4. 64 位桩从模板数据区取栈顶/入口/CPU 索引，跳到入口（内核 `smp` 模块）。
//!
//! 所有远跳目标在复制后由内核改写为绝对低地址；模板内的 RIP 相对访问随整页
//! 复制保持有效。
//!
//! 边界：不处理 APIC 初始化（调用方）、不支持 x2APIC MMIO 差异、不建立每 CPU
//! GDT/TSS、不宣称 SMP 调度。

use core::arch::global_asm;

global_asm!(
    r#"
    .section .prima_ap_tramp,"a"
    .global prima_ap_trampoline
    .global prima_ap_trampoline_end
    .global prima_ap_far_pm32
    .global prima_ap_far_lm64
    .global prima_ap_pm32
    .global prima_ap_lm64

prima_ap_trampoline:
    .code16
    cli
    mov ax, cs
    mov ds, ax
    mov es, ax
    mov eax, dword ptr [0x100]
    mov cr3, eax
    mov eax, cr4
    or eax, 0x20
    mov cr4, eax
    lgdt [0x118]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    .byte 0x66, 0xea
prima_ap_far_pm32:
    .long 0
    .word 0x08

prima_ap_pm32:
    .code32
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov ecx, 0xc0000080
    rdmsr
    or eax, 0x900
    wrmsr
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    .byte 0xea
prima_ap_far_lm64:
    .long 0
    .word 0x18

prima_ap_lm64:
    .code64
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, qword ptr [rip + prima_ap_data_stack]
    mov edi, dword ptr [rip + prima_ap_data_index]
    mov rax, qword ptr [rip + prima_ap_data_entry]
    jmp rax

    .org 0x100
prima_ap_data_cr3:
    .long 0
prima_ap_data_index:
    .long 0
prima_ap_data_stack:
    .quad 0
prima_ap_data_entry:
    .quad 0
prima_ap_gdtr:
    .short 0x1f
    .long 0
    .balign 8
prima_ap_gdt:
    .quad 0
    .byte 0xff, 0xff, 0x00, 0x00, 0x00, 0x9a, 0xcf, 0x00
    .byte 0xff, 0xff, 0x00, 0x00, 0x00, 0x92, 0xcf, 0x00
    .byte 0xff, 0xff, 0x00, 0x00, 0x00, 0x9a, 0x20, 0x00
prima_ap_trampoline_end:
    "#
);

unsafe extern "C" {
    static prima_ap_trampoline: u8;
    static prima_ap_trampoline_end: u8;
    static prima_ap_far_pm32: u8;
    static prima_ap_far_lm64: u8;
    static prima_ap_pm32: u8;
    static prima_ap_lm64: u8;
}

/// 模板复制区长度上限（含 0x100 起的数据区与临时 GDT）。
pub const TEMPLATE_LEN: usize = 0x140;

/// 数据区/临时 GDT 内各字段的页内偏移，必须与上方汇编的 `.org` 布局一致。
pub const CR3_OFF: usize = 0x100;
pub const INDEX_OFF: usize = 0x104;
pub const STACK_TOP_OFF: usize = 0x108;
pub const ENTRY_OFF: usize = 0x110;
pub const GDTR_OFF: usize = 0x118;
pub const GDT_OFF: usize = 0x120;

const _: () = assert!(GDT_OFF >= GDTR_OFF + 6, "临时 GDT 必须在 GDTR 之后");
const _: () = assert!(GDT_OFF.is_multiple_of(8), "GDT 需 8 字节对齐");
const _: () = assert!(GDT_OFF + 32 <= TEMPLATE_LEN);

/// 模板内的绝对地址字段（复制到目标页后需要按页基址改写）。
#[derive(Clone, Copy, Debug)]
pub struct FarJumps {
    pub pm32: usize,
    pub lm64: usize,
    /// 32 位保护模式入口的页内偏移（远跳目标 = 页基址 + 此值）。
    pub pm32_target: usize,
    /// 64 位桩的页内偏移（远跳目标 = 页基址 + 此值）。
    pub lm64_target: usize,
}

/// 模板起始虚拟地址与长度；调用方按 [`TEMPLATE_LEN`] 复制。
pub fn template() -> &'static [u8] {
    // SAFETY: 模板是只读静态区，长度由汇编布局固定，读取不跨越模板末尾。
    unsafe {
        let start = core::ptr::addr_of!(prima_ap_trampoline) as usize;
        let end = core::ptr::addr_of!(prima_ap_trampoline_end) as usize;
        core::slice::from_raw_parts(core::ptr::addr_of!(prima_ap_trampoline), end - start)
    }
}

/// 模板内需要运行时改写的字段偏移。
pub fn far_jumps() -> FarJumps {
    let start = core::ptr::addr_of!(prima_ap_trampoline) as usize;
    FarJumps {
        pm32: core::ptr::addr_of!(prima_ap_far_pm32) as usize - start,
        lm64: core::ptr::addr_of!(prima_ap_far_lm64) as usize - start,
        pm32_target: core::ptr::addr_of!(prima_ap_pm32) as usize - start,
        lm64_target: core::ptr::addr_of!(prima_ap_lm64) as usize - start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_layout_matches_the_rust_offsets() {
        assert_eq!(template().len(), TEMPLATE_LEN);
        let jumps = far_jumps();
        assert!(jumps.pm32 < CR3_OFF);
        assert!(jumps.lm64 < CR3_OFF);
        assert!(jumps.pm32_target < CR3_OFF);
        assert!(jumps.lm64_target < CR3_OFF);
        assert!(GDT_OFF < template().len());
    }
}
