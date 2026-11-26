/* Same raw ELF runs under Prima and the pinned Linux guest. No libc hooks. */
.section .text
.global _start
_start:
    mov $0x7fffffff, %eax
    mov $-1, %rdi
    mov $-1, %rsi
    mov $-1, %rdx
    mov $-1, %r10
    mov $-1, %r8
    mov $-1, %r9
    syscall
    cmp $-38, %rax
    jne fail
    mov $1, %eax
    mov $-1, %rdi
    xor %esi, %esi
    mov $1, %edx
    syscall
    cmp $-9, %rax
    jne fail
    mov $1, %eax
    mov $1, %edi
    mov $-1, %rsi
    mov $1, %edx
    syscall
    cmp $-14, %rax
    jne fail
    mov $13, %eax
    xor %edi, %edi
    xor %esi, %esi
    xor %edx, %edx
    mov $8, %r10d
    syscall
    cmp $-22, %rax
    jne fail
    mov $39, %eax
    syscall
    mov %rax, %r12
    mov %eax, sender(%rip)
    mov $186, %eax
    syscall
    mov %rax, %r13
    mov $13, %eax
    mov $10, %edi
    lea action(%rip), %rsi
    xor %edx, %edx
    mov $8, %r10d
    syscall
    test %rax, %rax
    jne fail
    mov $14, %eax
    xor %edi, %edi
    lea blocked(%rip), %rsi
    lea old_mask(%rip), %rdx
    mov $8, %r10d
    syscall
    test %rax, %rax
    jne fail
    mov $234, %eax
    mov %r12, %rdi
    mov %r13, %rsi
    mov $10, %edx
    syscall
    test %rax, %rax
    jne fail
    cmpq $0, handled(%rip)
    jne fail
    movabs $0x1122334455667788, %rax
    movq %rax, %xmm0
    mov $0x1234, %ebx
    mov $0x5678, %ebp
    mov $0x1357, %r14d
    mov $0x2468, %r15d
    mov $130, %eax
    lea empty_mask(%rip), %rdi
    mov $8, %esi
    syscall
    cmp $-4, %rax
    jne fail
    cmpq $1, handled(%rip)
    jne fail
    cmp $0x1234, %ebx
    jne fail
    cmp $0x5678, %ebp
    jne fail
    cmp $0x1357, %r14d
    jne fail
    cmp $0x2468, %r15d
    jne fail
    movq %xmm0, %rax
    movabs $0x1122334455667788, %rdx
    cmp %rdx, %rax
    jne fail
    pushfq
    pop %rax
    test $0x400, %eax
    jne fail
    mov $14, %eax
    xor %edi, %edi
    xor %esi, %esi
    lea observed_mask(%rip), %rdx
    mov $8, %r10d
    syscall
    test %rax, %rax
    jne fail
    testq $512, observed_mask(%rip)
    je fail
    mov $14, %eax
    mov $2, %edi
    lea old_mask(%rip), %rsi
    xor %edx, %edx
    mov $8, %r10d
    syscall
    test %rax, %rax
    jne fail
    mov $1, %eax
    mov $1, %edi
    lea message(%rip), %rsi
    mov $6, %edx
    syscall
    cmp $6, %rax
    jne fail
    xor %edi, %edi
    jmp finish
handler:
    cmp $10, %edi
    jne fail
    cmpl $10, (%rsi)
    jne fail
    cmpl $-6, 8(%rsi)
    jne fail
    mov sender(%rip), %eax
    cmp %eax, 16(%rsi)
    jne fail
    testq $512, 296(%rdx)
    je fail
    incq handled(%rip)
    pxor %xmm0, %xmm0
    xor %ebx, %ebx
    xor %ebp, %ebp
    xor %r14d, %r14d
    xor %r15d, %r15d
    std
    ret
restorer:
    mov $15, %eax
    syscall
    ud2
fail:
    mov $1, %edi
finish:
    mov $231, %eax
    syscall
    ud2
.section .rodata
message: .ascii "P1 ok\n"
.balign 8
blocked: .quad 512
empty_mask: .quad 0
action: .quad handler, 0x04000004, restorer, 0
.section .bss
.balign 8
old_mask: .skip 8
observed_mask: .skip 8
handled: .skip 8
sender: .skip 8
.section .note.GNU-stack,"",@progbits
