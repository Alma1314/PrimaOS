/* M2 Prima crt 骨架：保存入口五寄存器到 `.bss` 的 prima_ctx，再进入 C 入口。
 *
 * task_create 的入口约定：rdi=self_cap、rsi=send_cap、rdx=code_cap、rcx=name、
 * r8=manager_id；rsp 指向初始栈。`.bss` 由装载器清零，适配层据此取退出凭据。
 */
.text
.global _start
.type _start,@function
_start:
    mov %rsp, %r9
    lea prima_ctx(%rip), %rax
    mov %rdi, 0(%rax)
    mov %rsi, 8(%rax)
    mov %rdx, 16(%rax)
    mov %rcx, 24(%rax)
    mov %r8, 32(%rax)
    call stage5_entry
    ud2
.size _start, .-_start
.section .note.GNU-stack,"",@progbits
