/* P2b-5: fork-limit rollback. Three children fill MAX_PROCESSES; the fourth
 * fork fails with EAGAIN and must leave no reservation behind, so the parent's
 * inherited file descriptor and shared cursor still work and every exit closes
 * its own Store owner. */
.section .text
.global _start
_start:
    mov $257, %eax
    mov $-100, %rdi
    lea fixture(%rip), %rsi
    xor %edx, %edx
    xor %r10d, %r10d
    syscall
    cmp $3, %rax
    jne fail
    xor %eax, %eax
    mov $3, %edi
    lea byte(%rip), %rsi
    mov $1, %edx
    syscall
    cmp $1, %rax
    jne fail
    cmpb $'h', byte(%rip)
    jne fail

    xor %r12d, %r12d
fork_loop:
    cmp $3, %r12
    jae expect_eagain
    mov $57, %eax
    syscall
    test %rax, %rax
    js fail
    je child
    inc %r12
    jmp fork_loop

/* MAX_PROCESSES is four including the root: the fourth fork has no slot. */
expect_eagain:
    mov $57, %eax
    syscall
    cmp $-11, %rax
    jne fail

    mov $3, %r13d
reap:
    mov $61, %eax
    mov $-1, %rdi
    lea status(%rip), %rsi
    xor %edx, %edx
    xor %r10d, %r10d
    syscall
    test %rax, %rax
    js fail
    cmpl $0, status(%rip)
    jne fail
    dec %r13d
    jnz reap

/* No children remain; WNOHANG must report ECHILD, not a stale zombie. */
    mov $61, %eax
    mov $-1, %rdi
    lea status(%rip), %rsi
    mov $1, %edx
    xor %r10d, %r10d
    syscall
    cmp $-10, %rax
    jne fail

/* The failed fork reserved nothing: the parent cursor continues at byte 2. */
    xor %eax, %eax
    mov $3, %edi
    lea byte(%rip), %rsi
    mov $1, %edx
    syscall
    cmp $1, %rax
    jne fail
    cmpb $'e', byte(%rip)
    jne fail
    mov $1, %eax
    mov $1, %edi
    lea message(%rip), %rsi
    mov $12, %edx
    syscall
    xor %edi, %edi
    mov $231, %eax
    syscall

child:
    xor %edi, %edi
    mov $231, %eax
    syscall

fail:
    mov $1, %edi
    mov $231, %eax
    syscall
    ud2

.section .rodata
fixture: .asciz "/fixture.txt"
message: .ascii "M4 limit ok\n"

.section .bss
.balign 8
status: .skip 4
byte: .skip 1

.section .note.GNU-stack,"",@progbits
