/* Copied into an explicitly retained RX page. All side effects happen after
 * exec commit; no old text, TLS or stack is accessed. The original ELF entry
 * is supplied in r12, the opaque state page in r15. */
.section .text.prima_exec_bridge,"ax",@progbits
.global __prima_exec_bridge_start
.global __prima_exec_bridge_end
.hidden __prima_exec_bridge_start
.hidden __prima_exec_bridge_end
__prima_exec_bridge_start:
    mov %rsp, %r13
    sub $48, %rsp
    mov 40(%r15), %rbx
    test %rbx, %rbx
    jz 1f
    mov %rbx, %rdi
    mov $46, %eax
    syscall
    test %rax, %rax
    js 9f
    mov %rbx, %rdi
    mov $12, %eax
    syscall
    test %rax, %rax
    js 9f
    movq $0, 40(%r15)
1:
    lea 4096(%r15), %rbp
    mov (%rbp), %r14
    lea 8(%rbp), %rbx
2:
    test %r14, %r14
    jz 6f
    mov 0(%rbx), %rdi
    mov 8(%rbx), %rsi
    mov 16(%rbx), %rdx
    xor %r10d, %r10d
    xor %r8d, %r8d
    xor %r9d, %r9d
    mov $17, %eax
    syscall
    cmp $-11, %rax
    jne 7f
    mov $8, %eax
    syscall
    jmp 2b
7:
    test %rax, %rax
    js 9f
3:
    mov %rsp, %rdi
    mov $60, %eax
    syscall
    test %rax, %rax
    js 9f
    cmpq $0, 16(%rsp)
    jne 9f
    mov 24(%rsp), %rdx
    cmp 32(%rbx), %rdx
    jne 3b
    mov %rax, %rdx
    shr $48, %rdx
    mov 24(%rbx), %rcx
    shr $48, %rcx
    cmp %rcx, %rdx
    jne 3b
    test 40(%rbx), %rax
    jnz 9f
    add $48, %rbx
    dec %r14
    jmp 2b
6:
    mov 48(%r15), %rdi
    mov 56(%r15), %rsi
    mov 64(%r15), %rdx
    mov 72(%r15), %rcx
    mov 80(%r15), %r8
    mov %r13, %rsp
    jmp *%r12
9:
    mov $127, %edi
    mov $54, %eax
    syscall
    ud2
__prima_exec_bridge_end:
.section .note.GNU-stack,"",@progbits
