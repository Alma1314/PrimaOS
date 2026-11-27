//! x86-64 特权/底层原语（唯一允许 `unsafe` 的硬件触面之一）。
//!
//! 每个函数是**安全** `fn`：前置条件写在其文档里，由调用方保证。这些原语要么无 UB 风险
//! （`outb`/`inb`/`invlpg`/`hlt`），要么其危险性来自"错误使用会改变硬件状态"而非 Rust
//! 别名/内存模型（`wrmsr`/`write_cr3`）——本层的契约是：**不因 safe 调用序列触发 UB**。

use core::arch::asm;

/// Serialized x86_64 timestamp sample. The single-CPU kernel calibrates this
/// counter against PIT channel 0; no cross-CPU or invariant-frequency claim.
pub fn timestamp_counter() -> u64 {
    let low: u32;
    let high: u32;
    // SAFETY: LFENCE/RDTSC access no memory and are available on the x86_64 target.
    unsafe {
        asm!("lfence", "rdtsc", out("eax") low, out("edx") high,
             options(nomem, nostack, preserves_flags));
    }
    (u64::from(high) << 32) | u64::from(low)
}

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

/// 执行 `cpuid`，返回 `[eax, ebx, ecx, edx]`。
///
/// 契约：无内存副作用；`leaf`/`subleaf` 由调用方选择。结果随 CPU 与固件变化，
/// 调用方不得把某个叶子的返回值当成跨平台常量。
#[inline]
pub fn cpuid(leaf: u32, subleaf: u32) -> [u32; 4] {
    let (eax, ebx, ecx, edx): (u32, u32, u32, u32);
    // SAFETY: CPUID 读取 CPU 标识，不改内存或特权状态。rbx 由 LLVM 保留，
    // 不能作为操作数：先存入通用寄存器，读回后再恢复。
    unsafe {
        asm!(
            "mov {saved:r}, rbx",
            "cpuid",
            "mov {out:e}, ebx",
            "mov rbx, {saved:r}",
            saved = out(reg) _,
            out = out(reg) ebx,
            inlateout("eax") leaf => eax,
            inlateout("ecx") subleaf => ecx,
            out("edx") edx,
            options(nostack, preserves_flags)
        );
    }
    [eax, ebx, ecx, edx]
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

/// 交换 `IA32_GS_BASE` 与 `IA32_KERNEL_GS_BASE`（`swapgs`）。
///
/// 契约：`swapgs` 在 CPL3 执行会触发 #GP；在 CPL0 每次调用翻转两根 GS 基址。调用方必须
/// 保证进入/离开配对，且 GS-3 之前内核不依赖 `%gs`（交换后的值不得被当作内核指针）。
#[inline]
pub fn swapgs() {
    // SAFETY: `swapgs` 只翻转两个 MSR；CPL0 下合法，无内存别名风险。
    unsafe {
        asm!("swapgs", options(nomem, nostack, preserves_flags));
    }
}

/// 读 `%gs` 段内偏移 `OFF` 处的 u32（GS-3 每 CPU 区）。
///
/// 契约：调用方必须处于内核态且 `IA32_GS_BASE` 指向有效、可读的每 CPU 区
/// （GP/缺页由调用方保证不发生）。`OFF` 必须是 4 字节对齐的编译期常量。
#[inline]
pub fn read_gs_u32<const OFF: usize>() -> u32 {
    let value: u32;
    // SAFETY: 见函数契约——段基址有效、偏移常量对齐；只读，不改内存或特权状态。
    unsafe {
        asm!(
            "mov {out:e}, dword ptr gs:[{off}]",
            off = const OFF,
            out = out(reg) value,
            options(nostack, readonly, preserves_flags)
        );
    }
    value
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

/// 写 CR0。
///
/// 契约：特权指令，仅 CPL0；调用方负责保留必须置位的位（如 PE/PG/WP）。
#[inline]
pub fn write_cr0(value: u64) {
    // SAFETY: 见函数契约。
    unsafe {
        asm!("mov cr0, {}", in(reg) value, options(nostack, preserves_flags));
    }
}

/// 读 CR4。
#[inline]
pub fn read_cr4() -> u64 {
    let mut value: u64;
    // SAFETY: 读控制寄存器无副作用；`mov reg, cr4` 在 CPL0 下合法。
    unsafe {
        asm!("mov {}, cr4", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
}

/// 写 CR4。
///
/// 契约：特权指令，仅 CPL0；调用方负责保留必须置位的位（如 PAE）。
#[inline]
pub fn write_cr4(value: u64) {
    // SAFETY: 见函数契约。
    unsafe {
        asm!("mov cr4, {}", in(reg) value, options(nostack, preserves_flags));
    }
}

/// 用 `FXSAVE` 保存 x87/SSE 状态到 512 字节区域（必须 16 字节对齐）。
///
/// 契约：CR4.OSFXSR 已置位；`area` 指向可写、16 字节对齐的 512 字节缓冲区。
#[inline]
pub fn fxsave(area: *mut u8) {
    // SAFETY: 见函数契约。
    unsafe {
        asm!("fxsave [{}]", in(reg) area, options(nostack));
    }
}

/// 用 `FXRSTOR` 恢复 512 字节区域里的 x87/SSE 状态（必须 16 字节对齐）。
///
/// 契约：CR4.OSFXSR 已置位；`area` 指向可读、16 字节对齐的 512 字节缓冲区。
#[inline]
pub fn fxrstor(area: *const u8) {
    // SAFETY: 见函数契约。
    unsafe {
        asm!("fxrstor [{}]", in(reg) area, options(nostack));
    }
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

/// 读当前 GDTR 的 `(base, limit)`。
#[inline]
pub fn sgdt() -> (u64, u16) {
    let mut gdtr = [0u8; 10];
    // SAFETY: sgdt 只写 10 字节局部缓冲，不读内存。
    unsafe {
        asm!("sgdt [{}]", in(reg) gdtr.as_mut_ptr(), options(nostack, preserves_flags));
    }
    let limit = u16::from_le_bytes(gdtr[0..2].try_into().expect("2 bytes"));
    let base = u64::from_le_bytes(gdtr[2..10].try_into().expect("8 bytes"));
    (base, limit)
}

/// 读任务寄存器（`str`）低 16 位：返回本 CPU 当前装载的 TSS 选择子。
///
/// 契约：仅 CPL0；只读，无内存副作用。用于证明每个 CPU 已装载自己的 TSS。
#[inline]
pub fn task_register() -> u16 {
    let value: u16;
    // SAFETY: `str` 读取任务寄存器，不改内存或特权状态。
    unsafe {
        asm!("str {0:x}", out(reg) value, options(nomem, nostack, preserves_flags));
    }
    value
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

/// 装载用户数据段到 `fs`（M2/T-MUSL-02）。
///
/// 契约：`selector` 必须是当前 GDT 中 DPL3 的数据段选择子（本内核为 `USER_DS = 0x2b`）。
/// 在长模式下该指令把 FS.base 置为描述符基址（本内核为 0）；随后必须用
/// `wrmsr(IA32_FS_BASE, base)` 设置实际基址——两者顺序不可颠倒。
#[inline]
pub fn load_fs_selector(selector: u16) {
    // SAFETY: 见函数契约——非法选择子只会 #GP，不构成 UB；该指令读取 GDT。
    unsafe {
        asm!("mov fs, cx", in("cx") selector, options(nostack, preserves_flags));
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

/// CPL0 idle wait. The STI shadow closes the enable/HLT race; return with IF=0.
/// No lock or borrowed shared state may be held across this operation.
pub fn wait_for_interrupt() {
    // SAFETY: called only by the kernel idle loop with a live interrupt stack/IDT.
    unsafe {
        asm!("sti", "hlt", "cli", options(nomem, nostack));
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
        let _: fn() = super::swapgs;
        let _: fn() -> u32 = super::read_gs_u32::<0>;
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
