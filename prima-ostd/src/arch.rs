//! x86-64 特权/底层原语（唯一允许 `unsafe` 的硬件触面之一）。
//!
//! 每个函数是**安全** `fn`：前置条件写在其文档里，由调用方保证。这些原语要么无 UB 风险
//! （`outb`/`inb`/`invlpg`/`hlt`），要么其危险性来自"错误使用会改变硬件状态"而非 Rust
//! 别名/内存模型（`wrmsr`/`write_cr3`）——本层的契约是：**不因 safe 调用序列触发 UB**。

use core::arch::asm;

/// 向 I/O 端口写一个字节。
#[inline]
pub fn outb(port: u16, value: u8) {
    // SAFETY: 端口 I/O 无内存副作用；`nomem`/`nostack` 声明如实。
    unsafe {
        asm!(
            "out dx, al",
            in("dx") port,
            in("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
}

/// 从 I/O 端口读一个字节。
#[inline]
pub fn inb(port: u16) -> u8 {
    let value: u8;
    // SAFETY: 端口 I/O 无内存副作用；`nomem`/`nostack` 声明如实。
    unsafe {
        asm!(
            "in al, dx",
            in("dx") port,
            out("al") value,
            options(nomem, nostack, preserves_flags)
        );
    }
    value
}

/// 读一个 MSR。
///
/// 契约：`rdmsr` 属特权指令，**只能在 CPL0 执行**。以 CPL3 调用会触发 #GP。
#[inline]
pub fn rdmsr(msr: u32) -> u64 {
    let (low, high): (u32, u32);
    // SAFETY: 见函数契约——调用方保证处于 CPL0。
    unsafe {
        asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") low,
            out("edx") high,
            options(nostack, preserves_flags)
        );
    }
    ((high as u64) << 32) | low as u64
}

/// 写一个 MSR。
///
/// 契约：特权指令，**只能在 CPL0 执行**；写入未定义的值会使依赖该 MSR 的后续行为未定义
/// （例如错误的 `IA32_LSTAR` 会让 `syscall` 跳向非法地址）。调用方必须自己保证值的合法性。
#[inline]
pub fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    // SAFETY: 见函数契约——调用方保证 CPL0 与值的合法性。
    unsafe {
        asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") low,
            in("edx") high,
            options(nostack, preserves_flags)
        );
    }
}

/// 读 CR3（页表根物理地址，含低 12 位标志/PCID）。
#[inline]
pub fn read_cr3() -> u64 {
    let mut value: u64;
    // SAFETY: 读控制寄存器无副作用；`mov reg, cr3` 在 CPL0 下合法。
    unsafe {
        asm!("mov {}, cr3", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// 写 CR3，切换到另一张 PML4。
///
/// 契约：特权指令，仅 CPL0；**调用瞬间的指令流与栈必须在新根下仍被映射**，否则写 CR3
/// 后立即取指/取栈失败。调用方（如基于已复刻内核高半区的自建根）负责保证这一点。
#[inline]
pub fn write_cr3(phys: u64) {
    // SAFETY: 见函数契约——调用方保证新根映射当前指令流与栈。
    unsafe {
        asm!("mov cr3, {}", in(reg) phys, options(nostack, preserves_flags));
    }
}

/// 读 CR0。
#[inline]
pub fn read_cr0() -> u64 {
    let mut value: u64;
    // SAFETY: `mov reg, cr0` 在 CPL0 下合法且无副作用。
    unsafe {
        asm!("mov {}, cr0", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// 使 `va` 的单条 TLB 项失效。
///
/// 契约：特权指令，仅 CPL0。`va` 无需已映射——`invlpg` 对未映射地址是空操作，不产生 UB。
#[inline]
pub fn invlpg(va: u64) {
    // SAFETY: 见函数契约——CPL0 调用；非法 va 不构成 UB。
    unsafe {
        asm!("invlpg [{}]", in(reg) va, options(nostack, preserves_flags));
    }
}

/// 执行一次 `hlt`，等待中断。
///
/// 契约：仅 CPL0；若中断被永久屏蔽则不再返回（逻辑问题，非 UB）。
#[inline]
pub fn hlt() {
    // SAFETY: `hlt` 无内存副作用；CPL0 下合法。
    unsafe {
        asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

/// 执行一次 `int3`（软件断点）。
///
/// 契约：仅 CPL0 且 IDT 的 vector 3 已装载；否则 #GP/宿主异常。不改内存。
#[inline]
pub fn int3() {
    // SAFETY: 见函数契约——指令本身无内存副作用。
    unsafe {
        asm!("int3", options(nomem, nostack));
    }
}

/// 装载 GDT（`lgdt`）。`base`/`limit` 拼成 10 字节 GDTR 伪描述符。
///
/// 契约：特权指令，仅 CPL0；`base` 必须指向在 GDT 有效期内存活的内存。
#[inline]
pub fn lgdt(base: u64, limit: u16) {
    let mut gdtr = [0u8; 10];
    gdtr[0..2].copy_from_slice(&limit.to_le_bytes());
    gdtr[2..10].copy_from_slice(&base.to_le_bytes());
    // SAFETY: 见函数契约——`gdtr` 为局部量，在指令执行期间存活。
    unsafe {
        asm!("lgdt [{}]", in(reg) gdtr.as_ptr(), options(nostack, preserves_flags));
    }
}

/// 装载 TSS 段选择子（`ltr`），使其进入忙态。
///
/// 契约：特权指令，仅 CPL0；`selector` 必须指向 GDT 中一个已装好的可用 TSS 描述符。
#[inline]
pub fn ltr(selector: u16) {
    // SAFETY: 见函数契约——非法选择子只会 #GP，不构成 UB。
    unsafe {
        asm!("ltr cx", in("cx") selector, options(nostack, preserves_flags));
    }
}

/// reload `ds`/`es` 到 `selector`。
///
/// 契约：`selector` 必须是当前 GDT 中合法的数据段选择子。**刻意不复位 `ss`**——保留
/// bootloader 已装载的 `ss`（其 shadow 描述符仍有效），避免 `mov ss` 在部分平台上触发
/// #GP/三故障。
#[inline]
pub fn reload_data_segments(selector: u16) {
    // SAFETY: 见函数契约——非法选择子只会 #GP，不构成 UB。
    unsafe {
        asm!(
            "mov ds, cx",
            "mov es, cx",
            in("cx") selector,
            options(nostack, preserves_flags)
        );
    }
}

/// 关中断（`cli`）。
#[inline]
pub fn cli() {
    // SAFETY: 仅修改 RFLAGS.IF；CPL0 下合法。
    unsafe {
        asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

/// 开中断（`sti`）。
#[inline]
pub fn sti() {
    // SAFETY: 仅修改 RFLAGS.IF；CPL0 下合法。
    unsafe {
        asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

#[cfg(test)]
mod tests {
    // 特权指令在 host 用户态执行会触发 SIGILL/STATUS_PRIVILEGED_INSTRUCTION，故此处只做
    // 编译期/类型检查：确认这些 safe API 可从 safe 上下文调用（真实语义由 QEMU 验证）。
    #[test]
    fn safe_apis_are_callable_without_unsafe() {
        let _: fn(u16, u8) = super::outb;
        let _: fn(u16) -> u8 = super::inb;
        let _: fn(u32) -> u64 = super::rdmsr;
        let _: fn(u32, u64) = super::wrmsr;
        let _: fn() -> u64 = super::read_cr3;
        let _: fn(u64) = super::write_cr3;
        let _: fn() -> u64 = super::read_cr0;
        let _: fn(u64) = super::invlpg;
        let _: fn() = super::hlt;
        let _: fn() = super::cli;
        let _: fn() = super::sti;
        let _: fn() = super::int3;
        let _: fn(u64, u16) = super::lgdt;
        let _: fn(u16) = super::ltr;
        let _: fn(u16) = super::reload_data_segments;
    }
}
