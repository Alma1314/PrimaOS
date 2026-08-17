//! GDT 装载与最小 TSS 的细化（refine）模块：承载 `lgdt`/`ltr` 与数据段 reload 的
//! `asm!`；纯布局/字节编码可 host 单测。`no_std` 由 crate 根 (`lib.rs`) 统一设置。
//!
//! 布局说明：x86-64 下 GDT 描述符按选择子 index 寻址，字节偏移 = `index * 8`；
//! 16 字节系统描述符（TSS）占据连续两个 8 字节槽。故本模块把 `Gdt` 存为按
//! `index * 8` 对齐的字节表：槽 0=null、槽 1=内核 code、槽 2=内核 data、槽 3=TSS
//! （占偏移 24..39）。`lgdt` 时 limit 覆盖到 TSS 描述符末尾。

use core::arch::asm;

/// 16 字节段/系统描述符（x86-64）。
#[repr(C, packed(2))]
#[derive(Clone, Copy, Default)]
pub struct Descriptor([u8; 16]);

impl Descriptor {
    const fn null() -> Self {
        Descriptor([0; 16])
    }
    const fn kernel_code() -> Self {
        Descriptor([
            0xff, 0xff, // limit low = 0xffff
            0x00, 0x00, 0x00, // base low..mid = 0
            0x9a, // access: present | DPL0 | non-system | exec-read
            0x20, // flags: L=1 (64-bit code)
            0x00, // base hi = 0
            0, 0, 0, 0, 0, 0, 0, 0, // high 8 bytes = 0
        ])
    }
    const fn kernel_data() -> Self {
        Descriptor([
            0xff, 0xff, // limit low = 0xffff
            0x00, 0x00, 0x00, // base = 0
            0x92, // access: present | DPL0 | non-system | data-rw
            0x00, // flags = 0
            0x00, // base hi = 0
            0, 0, 0, 0, 0, 0, 0, 0,
        ])
    }
    #[cfg(test)]
    fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
    #[cfg(test)]
    fn is_zero(&self) -> bool {
        self.0.iter().all(|&b| b == 0)
    }
}

/// 最小 TSS：为 trap 提供内核栈切换（rsp0）。`#[repr(C, packed)]` 使其保持标准
/// x86-64 104 字节 byte-packed 布局（rsp0 位于偏移 4）。
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iopb: u16,
}

impl Tss {
    const fn new(rsp0: u64) -> Self {
        Tss {
            reserved0: 0,
            rsp0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist: [0; 7],
            reserved2: 0,
            reserved3: 0,
            iopb: 104,
        }
    }
}

/// GDT 的 x86 字节偏移常量（index * 8）。
const CODE_OFF: usize = 8; // 槽 1: index*8 = 内核 code
const DATA_OFF: usize = 16; // 槽 2: index*8 = 内核 data
const TSS_OFF: usize = 24; // 槽 3: index*8 = TSS 系统描述符（占 16 字节，覆盖槽 3..4）

/// 装配 GDT 字节表（null/code/data/TSS），不含任何特权指令；供 host 单测。
#[allow(clippy::result_unit_err)]
fn build_gdt_bytes(gdt: &mut Gdt, tss: &Tss) {
    // 槽 0 = null（全 0）；槽 1 = code 低 8 字节；槽 2 = data 低 8 字节。
    gdt.bytes[0..16].copy_from_slice(&Descriptor::null().0);
    gdt.bytes[CODE_OFF..CODE_OFF + 8].copy_from_slice(&Descriptor::kernel_code().0[..8]);
    gdt.bytes[DATA_OFF..DATA_OFF + 8].copy_from_slice(&Descriptor::kernel_data().0[..8]);
    // install_tss_desc 无 asm，可（也应）在这里调用；其 Result 实际从不 Err，故吞掉。
    let _ = install_tss_desc(gdt, tss);
}

/// 装载 GDT 的槽 0/1/2（null/code/data），并把 TSS 描述符装进槽 3、`ltr`。
///
/// # Safety
///
/// 仅可在单核 boot 早期、内核尚未允许并发时调用一次。`gdt`/`tss` 必须存活到内核结束。
#[allow(clippy::result_unit_err)]
pub unsafe fn init(gdt: &mut Gdt, tss: &Tss) -> Result<(), ()> {
    build_gdt_bytes(gdt, tss);

    // lgdt：limit 覆盖到 TSS 描述符末尾（16 字节系统描述符结束处，再 -1）。
    let len = (TSS_OFF + 16) as u16 - 1;
    let base = gdt.bytes.as_ptr() as u64;
    let mut gdtr = [0u8; 10];
    gdtr[0..2].copy_from_slice(&len.to_le_bytes());
    gdtr[2..10].copy_from_slice(&base.to_le_bytes());
    unsafe { asm!("lgdt [{}]", in(reg) gdtr.as_ptr(), options(nostack, preserves_flags)) };

    // ltr: 装载 TSS 段选择子（index 3, RPL 0）为忙态。
    let selector: u16 = 3 * 8;
    unsafe { asm!("ltr cx", in("cx") selector, options(nostack, preserves_flags)) };

    // reload 数据段选择子（内核数据段 index 2）。
    // 注意：不复位 `ss`——保留 bootloader 已装载的 ss（其 shadow 描述符仍有效），
    // 避免在 QEMU 上 `mov ss` 触发 #GP/三故障。仅 reload ds/es。
    unsafe {
        asm!(
            "mov ds, cx",
            "mov es, cx",
            in("cx") DATA_OFF as u16,
            options(nostack, preserves_flags)
        );
    }
    Ok(())
}

#[allow(clippy::result_unit_err)]
fn install_tss_desc(gdt: &mut Gdt, tss: &Tss) -> Result<(), ()> {
    let base = tss as *const Tss as u64;
    let limit = (core::mem::size_of::<Tss>() - 1) as u16;
    let lo = limit; // limit 为 u16，低 16 位即 limit 本身
    let hi_flags = (((limit as u32) >> 16) & 0x0f) as u8; // TSS 用 byte granularity（无 L），flags=0

    let mut d = [0u8; 16];
    d[0] = (lo & 0xff) as u8;
    d[1] = (lo >> 8) as u8;
    d[2] = (base & 0xff) as u8;
    d[3] = ((base >> 8) & 0xff) as u8;
    d[4] = ((base >> 16) & 0xff) as u8;
    d[5] = 0x89; // present | DPL0 | sys | 64-bit available TSS (type=0x9)
    d[6] = hi_flags;
    d[7] = ((base >> 24) & 0xff) as u8;
    // 高 8 字节：base [31:24]..[63:32] + reserved
    d[8] = ((base >> 24) & 0xff) as u8;
    d[9] = ((base >> 32) & 0xff) as u8;
    d[10] = ((base >> 40) & 0xff) as u8;
    d[11] = ((base >> 48) & 0xff) as u8;
    d[12] = ((base >> 56) & 0xff) as u8;
    d[13..16].copy_from_slice(&[0, 0, 0]);
    gdt.bytes[TSS_OFF..TSS_OFF + 16].copy_from_slice(&d);
    Ok(())
}

/// GDT 容器：按 x86 字节偏移（index*8）排布的原始字节表。
/// 容量 64 字节，足以容纳到槽 3 的 16 字节 TSS 描述符（偏移 24..39）。
#[repr(align(8))]
#[derive(Clone)]
pub struct Gdt {
    bytes: [u8; 64],
}

impl Gdt {
    const fn new() -> Self {
        Gdt { bytes: [0; 64] }
    }
}

// 静态持有：单核 boot 早期，仅一次 init 时用 unsafe 写一次。
static mut GDT_ITEM: Gdt = Gdt::new();
static mut TSS_ITEM: Tss = Tss::new(0);

/// 供 `main.rs` 调用的公开入口：装载 GDT/TSS。
#[allow(clippy::result_unit_err)]
pub fn init_kernel_gdt(rsp0: u64) -> Result<(), ()> {
    unsafe {
        GDT_ITEM = Gdt::new();
        TSS_ITEM = Tss::new(rsp0);
        let gdt = core::ptr::addr_of_mut!(GDT_ITEM);
        let tss = core::ptr::addr_of!(TSS_ITEM);
        init(&mut *gdt, &*tss)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    #[test]
    fn descriptor_is_16_bytes() {
        assert_eq!(size_of::<Descriptor>(), 16);
    }

    #[test]
    fn tss_is_104_bytes() {
        assert_eq!(size_of::<Tss>(), 104);
    }

    #[test]
    fn code_descriptor_layout() {
        let d = Descriptor::kernel_code();
        let b = d.as_bytes();
        assert_eq!(b[5], 0x9a);
        assert_eq!(b[6], 0x20);
        assert!(b[7..16].iter().all(|&x| x == 0));
    }

    #[test]
    fn data_descriptor_layout() {
        let d = Descriptor::kernel_data();
        let b = d.as_bytes();
        assert_eq!(b[5], 0x92);
        assert_eq!(b[6], 0x00);
    }

    #[test]
    fn null_descriptor_is_zero() {
        assert!(Descriptor::null().is_zero());
    }

    #[test]
    fn tss_rsp0_offset_is_4() {
        // rsp0 字段在 offset 4（TSS 标准布局）。
        let t = Tss::new(0x1234_5678);
        let bytes =
            unsafe { core::slice::from_raw_parts(&t as *const Tss as *const u8, size_of::<Tss>()) };
        assert_eq!(&bytes[4..12], &0x1234_5678u64.to_le_bytes());
    }

    // 以下测试钉住 build_gdt_bytes 的字节表布局——即本 task 最大的历史 bug：
    // brief 曾按 stride=16 排布 code/data/TSS，导致 ltr 命中描述符错误区而 #GP(0x18)。
    // 回归安全仅靠 QEMU 不够，故在此 pin 编码与槽位。

    #[test]
    fn gdt_layout_code_slot() {
        // 槽 1（index*8=8..16）：access=0x9a、flags 0x20（L=1）。
        let mut g = Gdt::new();
        build_gdt_bytes(&mut g, &Tss::new(0));
        assert_eq!(g.bytes[CODE_OFF + 5], 0x9a);
        assert_eq!(g.bytes[CODE_OFF + 6], 0x20);
    }

    #[test]
    fn gdt_layout_data_slot() {
        // 槽 2（index*8=16..24）：access=0x92。
        let mut g = Gdt::new();
        build_gdt_bytes(&mut g, &Tss::new(0));
        assert_eq!(g.bytes[DATA_OFF + 5], 0x92);
    }

    #[test]
    fn gdt_layout_tss_slot() {
        // 槽 3（index*8=24..39）：16 字节系统描述符。
        // access=0x89、limit 低字节 0x67（size 104 - 1）、高槽 13..15 为 0。
        let mut g = Gdt::new();
        build_gdt_bytes(&mut g, &Tss::new(0));
        assert_eq!(g.bytes[TSS_OFF + 5], 0x89);
        assert_eq!(g.bytes[TSS_OFF + 0], 0x67);
        assert_eq!(g.bytes[TSS_OFF + 13], 0);
        assert_eq!(g.bytes[TSS_OFF + 14], 0);
        assert_eq!(g.bytes[TSS_OFF + 15], 0);
    }

    #[test]
    fn gdt_gdtr_limit_arithmetic() {
        // GDTR limit 为字节表末尾再 -1：limit = (TSS_OFF + 16) - 1 = 39。
        let mut g = Gdt::new();
        build_gdt_bytes(&mut g, &Tss::new(0));
        assert_eq!((TSS_OFF + 16) as u16 - 1, 39);
    }
}
