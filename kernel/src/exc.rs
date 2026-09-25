//! 异常恢复帧（`SretFrame`）的内核侧编解码与纯校验。
//!
//! 本模块是切片 D「真实 #PF → 完整现场捕获 → 验证式 `exc_resume` 桥接」的
//! 语义宿主：它定义与抽象模型（`cosir/src/state.rs` 的 `SretFrame`）逐字节一致的
//! 0xA0 帧，并提供**无 asm、可 host 单测**的校验器。真实 trap 现场捕获在
//! `idt.rs`、真实 `iretq` 跳板在本模块（`exc_resume_entry`）。
//!
//! **范围边界：** 本内核为 ring0-only（`gdt.rs` 无 DPL3 选择子），故校验器只接受
//! 内核代码段/数据段选择子，不做用户态投递、不检查 capability/subscription、
//! 不实现抽象模型的 `Faulted → Ready`/`TaskResumed` audit 语义。

/// `SretFrame` 的字节长度（20 × u64）。
///
/// 帧类型与编解码已下沉框架 [`prima_ostd::entry`]；此别名保留内核侧既有引用。
pub const SRET_FRAME_LEN: usize = prima_ostd::entry::SRET_FRAME_LEN;

/// `SretFrame` 的对齐要求。
pub const SRET_FRAME_ALIGN: usize = prima_ostd::entry::SRET_FRAME_ALIGN;

/// 内核代码段选择子（`gdt.rs` 槽位 1）。
pub const KERNEL_CS: u64 = 0x8;

/// 内核数据段选择子（`gdt.rs` 槽位 2）。
pub const KERNEL_SS: u64 = 0x10;

/// 恢复帧哨兵 A：跳板落点用来证明寄存器确实被 `iretq` 恢复（而非盲跳）。
pub const RESUME_SENTINEL_A: u64 = 0x1234_5678_9abc_def0;

/// 恢复帧哨兵 B：与 A 互异，防单条车道误判。
pub const RESUME_SENTINEL_B: u64 = 0x0fed_cba9_8765_4321;

/// x86_64 异常返回帧（布局定义与编解码见 [`prima_ostd::entry::SretFrame`]）。
pub use prima_ostd::entry::SretFrame;

/// x86_64 canonical 地址判定（bits 63:47 全等于 bit 47）。
///
/// 与 `cosir::ops::is_user_canonical` 不同：本内核 ring0-only，landing 与 resume
/// 栈都在内核高半区，故这里接受低半区与高半区两种 canonical 形态。
pub const fn is_canonical(addr: u64) -> bool {
    let top = addr >> 47;
    top == 0 || top == 0x1_ffff
}

/// `iretq` 恢复内核帧时的合法 RFLAGS 位。
///
/// 必须置位：bit 1（`iretq` 要求，否则 #GP）。
/// 允许：CF/PF/AF/ZF/SF/OF/AC/ID 等算术与对齐位。
/// **刻意禁止**：TF(8)、IF(9)、IOPL(12-13)、NT(14)、RF(16)、VM(17)、VIF(19)、
/// VIP(20)，以及保留位 3/5/15。当前 IDT 只装载 vector 3 与 14，任何开中断或
/// 单步都会因缺 handler 而 #GP → double fault → triple fault。
pub const KERNEL_RFLAGS_FIXED: u64 = 1 << 1;

/// 允许出现的 RFLAGS 位掩码（见 [`KERNEL_RFLAGS_FIXED`] 的说明）。
pub const KERNEL_RFLAGS_ALLOWED: u64 = (1 << 0)   // CF
    | KERNEL_RFLAGS_FIXED
    | (1 << 2)   // PF
    | (1 << 4)   // AF
    | (1 << 6)   // ZF
    | (1 << 7)   // SF
    | (1 << 11)  // OF
    | (1 << 18)  // AC
    | (1 << 21); // ID

/// 校验 RFLAGS 是否可安全用于内核 `iretq` 恢复。
pub const fn valid_kernel_rflags(flags: u64) -> bool {
    flags & KERNEL_RFLAGS_FIXED != 0 && flags & !KERNEL_RFLAGS_ALLOWED == 0
}

/// 内核恢复帧校验失败的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelResumeError {
    /// `rip` 不是 canonical 地址。
    NonCanonicalRip,
    /// `rip` 不指向恢复跳板的 landing 地址。
    RipNotLanding,
    /// `rsp` 不是 canonical 地址。
    NonCanonicalRsp,
    /// `rsp` 未按 `SRET_FRAME_ALIGN` 对齐。
    MisalignedRsp,
    /// `rsp` 低于恢复栈下界（含 `iretq` + `call` 的 32 字节余量）。
    RspBelowStack,
    /// `rsp` 高于恢复栈上界（栈顶为 one-past-end，允许相等）。
    RspAboveStack,
    /// `cs` 不是内核代码段选择子。
    BadCs,
    /// `ss` 不是内核数据段选择子（策略字段；`iretq` 同 CPL 不加载 SS）。
    BadSs,
    /// `rflags` 含禁止位或缺 bit 1。
    BadRflags,
}

/// 校验一帧是否可安全用于内核 `iretq` 恢复。
///
/// 纯验证：任何失败都返回错误且不修改输入。成功不保证运行时一定能恢复
/// （例如目标地址未映射），只保证机器语义层面的帧自洽。
pub fn validate_for_kernel_resume(
    frame: &SretFrame,
    landing: u64,
    stack_bottom: u64,
    stack_top: u64,
) -> Result<(), KernelResumeError> {
    if !is_canonical(frame.rip) {
        return Err(KernelResumeError::NonCanonicalRip);
    }
    if frame.rip != landing {
        return Err(KernelResumeError::RipNotLanding);
    }
    if !is_canonical(frame.rsp) {
        return Err(KernelResumeError::NonCanonicalRsp);
    }
    if !frame.rsp.is_multiple_of(SRET_FRAME_ALIGN as u64) {
        return Err(KernelResumeError::MisalignedRsp);
    }
    // iretq 弹出 3 个 qword（24 字节），landing 的 `call` 再压 8 字节；
    // 要求下界之上留出 32 字节余量，避免恢复栈越界。
    if frame.rsp < stack_bottom.saturating_add(32) {
        return Err(KernelResumeError::RspBelowStack);
    }
    if frame.rsp > stack_top {
        return Err(KernelResumeError::RspAboveStack);
    }
    if frame.cs != KERNEL_CS {
        return Err(KernelResumeError::BadCs);
    }
    if frame.ss != KERNEL_SS {
        return Err(KernelResumeError::BadSs);
    }
    if !valid_kernel_rflags(frame.rflags) {
        return Err(KernelResumeError::BadRflags);
    }
    Ok(())
}

/// 经真实 `iretq` 恢复进一帧（跳板与帧指针存储已下沉 [`prima_ostd::entry`]）。
///
/// 契约（信任调用方）：必须先经 [`validate_for_kernel_resume`] 校验该帧，并保证帧在整个
/// 跳转期间有效、`rsp` 指向可写 canonical 栈、`cs`/`ss` 为合法内核选择子。不返回。
pub fn resume(frame: &SretFrame) -> ! {
    prima_ostd::entry::resume(frame)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 便于构造测试帧：字段序与 `SretFrame` 一致。
    fn frame() -> SretFrame {
        SretFrame {
            rax: 1,
            rbx: 2,
            rcx: 3,
            rdx: 4,
            rsi: 5,
            rdi: 6,
            rbp: 7,
            rsp: 0x2000,
            r8: 9,
            r9: 10,
            r10: 11,
            r11: 12,
            r12: RESUME_SENTINEL_A,
            r13: RESUME_SENTINEL_B,
            r14: 15,
            r15: 16,
            rflags: KERNEL_RFLAGS_FIXED | (1 << 6),
            rip: 0x1000,
            cs: KERNEL_CS,
            ss: KERNEL_SS,
        }
    }

    const LANDING: u64 = 0x1000;
    const STACK_BOTTOM: u64 = 0x800;
    const STACK_TOP: u64 = 0x4000;

    fn validate(f: &SretFrame) -> Result<(), KernelResumeError> {
        validate_for_kernel_resume(f, LANDING, STACK_BOTTOM, STACK_TOP)
    }

    #[test]
    fn sret_frame_size_is_0xa0() {
        assert_eq!(core::mem::size_of::<SretFrame>(), SRET_FRAME_LEN);
    }

    #[test]
    fn encode_places_fields_at_cosir_wire_offsets() {
        let f = frame();
        let bytes = f.encode();
        let word = |offset: usize| {
            let mut raw = [0u8; 8];
            raw.copy_from_slice(&bytes[offset..offset + 8]);
            u64::from_le_bytes(raw)
        };
        assert_eq!(word(0x00), f.rax);
        assert_eq!(word(0x08), f.rbx);
        assert_eq!(word(0x10), f.rcx);
        assert_eq!(word(0x18), f.rdx);
        assert_eq!(word(0x20), f.rsi);
        assert_eq!(word(0x28), f.rdi);
        assert_eq!(word(0x30), f.rbp);
        assert_eq!(word(0x38), f.rsp);
        assert_eq!(word(0x40), f.r8);
        assert_eq!(word(0x48), f.r9);
        assert_eq!(word(0x50), f.r10);
        assert_eq!(word(0x58), f.r11);
        assert_eq!(word(0x60), f.r12);
        assert_eq!(word(0x68), f.r13);
        assert_eq!(word(0x70), f.r14);
        assert_eq!(word(0x78), f.r15);
        assert_eq!(word(0x80), f.rflags);
        assert_eq!(word(0x88), f.rip);
        assert_eq!(word(0x90), f.cs);
        assert_eq!(word(0x98), f.ss);
    }

    #[test]
    fn decode_encode_round_trip() {
        let f = frame();
        assert_eq!(SretFrame::decode(&f.encode()), f);
        assert_eq!(SretFrame::ZERO.encode(), [0u8; SRET_FRAME_LEN]);
    }

    #[test]
    fn accepts_valid_kernel_frame() {
        assert_eq!(validate(&frame()), Ok(()));
    }

    #[test]
    fn rejects_non_canonical_rip() {
        let mut f = frame();
        f.rip = 0x0000_8000_0000_0000; // top bits 0 but bit47 set: non-canonical
        assert_eq!(validate(&f), Err(KernelResumeError::NonCanonicalRip));
    }

    #[test]
    fn rejects_rip_not_landing() {
        let mut f = frame();
        f.rip = 0x2000; // canonical but not the landing address
        assert_eq!(validate(&f), Err(KernelResumeError::RipNotLanding));
    }

    #[test]
    fn rejects_non_canonical_rsp() {
        let mut f = frame();
        f.rsp = 0x0000_8000_0000_0000;
        assert_eq!(validate(&f), Err(KernelResumeError::NonCanonicalRsp));
    }

    #[test]
    fn rejects_misaligned_rsp() {
        let mut f = frame();
        f.rsp = 0x1008;
        assert_eq!(validate(&f), Err(KernelResumeError::MisalignedRsp));
    }

    #[test]
    fn rejects_rsp_below_stack() {
        let mut f = frame();
        f.rsp = STACK_BOTTOM; // needs >= bottom + 32
        assert_eq!(validate(&f), Err(KernelResumeError::RspBelowStack));
    }

    #[test]
    fn rejects_rsp_above_stack() {
        let mut f = frame();
        f.rsp = STACK_TOP + SRET_FRAME_ALIGN as u64;
        assert_eq!(validate(&f), Err(KernelResumeError::RspAboveStack));
    }

    #[test]
    fn accepts_rsp_at_top_one_past_end() {
        let mut f = frame();
        f.rsp = STACK_TOP;
        assert_eq!(validate(&f), Ok(()));
    }

    #[test]
    fn rejects_bad_cs() {
        let mut f = frame();
        f.cs = 0x1b; // user selector: out of scope for this ring0 trampoline
        assert_eq!(validate(&f), Err(KernelResumeError::BadCs));
    }

    #[test]
    fn rejects_bad_ss() {
        let mut f = frame();
        f.ss = 0x23;
        assert_eq!(validate(&f), Err(KernelResumeError::BadSs));
    }

    #[test]
    fn rejects_rflags_without_fixed_bit() {
        let mut f = frame();
        f.rflags = 1 << 6; // bit 1 clear
        assert_eq!(validate(&f), Err(KernelResumeError::BadRflags));
    }

    #[test]
    fn rejects_forbidden_rflags_bits() {
        for bit in [3u32, 5, 8, 9, 10, 12, 13, 14, 15, 16, 17, 19, 20] {
            let mut f = frame();
            f.rflags = KERNEL_RFLAGS_FIXED | (1 << bit);
            assert_eq!(
                validate(&f),
                Err(KernelResumeError::BadRflags),
                "bit {bit} must be rejected"
            );
        }
    }

    #[test]
    fn is_canonical_accepts_low_and_high_half() {
        assert!(is_canonical(0));
        assert!(is_canonical(0x0000_7fff_ffff_ffff));
        assert!(is_canonical(0xffff_8000_0000_0000));
        assert!(is_canonical(0xffff_ffff_ffff_ffff));
        assert!(!is_canonical(0x0000_8000_0000_0000));
        assert!(!is_canonical(0x0000_ffff_ffff_ffff));
        assert!(!is_canonical(0xffff_0000_0000_0000));
    }

    #[test]
    fn validator_does_not_mutate_input() {
        let f = frame();
        let snapshot = f;
        let _ = validate_for_kernel_resume(&f, LANDING, STACK_BOTTOM, STACK_TOP);
        assert_eq!(f, snapshot);
    }
}
