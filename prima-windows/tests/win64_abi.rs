#![cfg(target_arch = "x86_64")]

//! WIN-M1 的 Win64 调用约定宿主探针。
//!
//! 只验证 Linux x86_64 宿主上 rustc 与手写汇编共同遵守的冻结子集：
//! 四寄存器参数、栈参数、32 字节 shadow space 槽位、浮点位置分配和非易失寄存器保存。
//! 不验证 Windows 内核陷入、SEH 或异常上下文。

use core::arch::global_asm;

extern "win64" fn sum8(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64) -> u64 {
    a + b + c + d + e + f + g + h
}

extern "win64" fn floats4(a: f64, b: f64, c: f64, d: f64) -> f64 {
    a + b * 2.0 + c * 3.0 + d * 4.0
}

extern "win64" fn mixed4(a: u64, b: f64, c: u64, d: f64) -> f64 {
    a as f64 + b * 2.0 + c as f64 + d * 4.0
}

global_asm!(
    ".text",
    ".globl win64_shadow_probe",
    ".type win64_shadow_probe,@function",
    "win64_shadow_probe:",
    "lea rax, [rsp + 8]",
    "test rax, 15",
    "jnz 1f",
    "mov [rsp + 8], rcx",
    "mov [rsp + 16], rdx",
    "mov [rsp + 24], r8",
    "mov [rsp + 32], r9",
    "mov rax, [rsp + 40]",
    "ret",
    "1:",
    "ud2",
    ".size win64_shadow_probe, . - win64_shadow_probe",
);

unsafe extern "win64" {
    fn win64_shadow_probe(a: u64, b: u64, c: u64, d: u64, e: u64) -> u64;
}

#[test]
fn passes_register_and_stack_arguments() {
    let received = sum8(1, 2, 3, 4, 5, 6, 7, 8);
    assert_eq!(received, 36);
}

#[test]
fn passes_float_arguments_by_position() {
    assert_eq!(floats4(1.0, 2.0, 3.0, 4.0), 30.0);
    assert_eq!(mixed4(1, 2.0, 3, 4.0), 24.0);
}

#[test]
fn uses_shadow_space_and_fifth_stack_slot() {
    let fifth = unsafe { win64_shadow_probe(1, 2, 3, 4, 0x55aa_55aa_55aa_55aa) };
    assert_eq!(fifth, 0x55aa_55aa_55aa_55aa);
}

global_asm!(
    ".text",
    ".globl win64_nonvolatile_probe",
    ".type win64_nonvolatile_probe,@function",
    "win64_nonvolatile_probe:",
    "push rbx",
    "push rbp",
    "push rdi",
    "push rsi",
    "push r12",
    "push r13",
    "push r14",
    "push r15",
    "sub rsp, 0x28",
    "mov rbx, 0x1111111111111111",
    "mov rbp, 0x2222222222222222",
    "mov rdi, 0x3333333333333333",
    "mov rsi, 0x4444444444444444",
    "mov r12, 0x5555555555555555",
    "mov r13, 0x6666666666666666",
    "mov r14, 0x7777777777777777",
    "mov r15, 0x8888888888888888",
    "mov rcx, 7",
    "call win64_nonvolatile_callee",
    "xor eax, eax",
    "mov r10, 0x1111111111111111",
    "cmp rbx, r10",
    "jne 9f",
    "mov r10, 0x2222222222222222",
    "cmp rbp, r10",
    "jne 9f",
    "mov r10, 0x3333333333333333",
    "cmp rdi, r10",
    "jne 9f",
    "mov r10, 0x4444444444444444",
    "cmp rsi, r10",
    "jne 9f",
    "mov r10, 0x5555555555555555",
    "cmp r12, r10",
    "jne 9f",
    "mov r10, 0x6666666666666666",
    "cmp r13, r10",
    "jne 9f",
    "mov r10, 0x7777777777777777",
    "cmp r14, r10",
    "jne 9f",
    "mov r10, 0x8888888888888888",
    "cmp r15, r10",
    "jne 9f",
    "jmp 10f",
    "9:",
    "mov eax, 1",
    "10:",
    "add rsp, 0x28",
    "pop r15",
    "pop r14",
    "pop r13",
    "pop r12",
    "pop rsi",
    "pop rdi",
    "pop rbp",
    "pop rbx",
    "ret",
    ".size win64_nonvolatile_probe, . - win64_nonvolatile_probe",
);

unsafe extern "win64" {
    fn win64_nonvolatile_probe() -> u64;
}

#[unsafe(no_mangle)]
extern "win64" fn win64_nonvolatile_callee(seed: u64) -> u64 {
    let mut value = seed;
    for step in 0..128u64 {
        value = value
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(step);
    }
    value
}

#[test]
fn preserves_nonvolatile_registers_across_win64_call() {
    let clobbered = unsafe { win64_nonvolatile_probe() };
    assert_eq!(
        clobbered, 0,
        "win64 callee clobbered a nonvolatile register"
    );
}
