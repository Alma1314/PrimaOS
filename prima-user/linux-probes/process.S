/* Raw Linux fork/exec/wait, COW isolation and inherited shared OFD/FD_CLOEXEC. */
.section .text
.global _start
_start:
    mov $39, %eax
    syscall
    mov %rax, %r12
    mov $61, %eax
    mov $-1, %rdi
    xor %esi, %esi
    mov $1, %edx
    xor %r10d, %r10d
    syscall
    cmp $-10, %rax
    jne fail
    mov $257, %eax
    mov $-100, %rdi
    lea fixture(%rip), %rsi
    mov $0x80000, %edx
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
    mov $57, %eax
    syscall
    test %rax, %rax
    js fail
    je child
    mov %rax, %r13
    mov %r13, %rdi
    lea status(%rip), %rsi
    xor %edx, %edx
    xor %r10d, %r10d
    mov $61, %eax
    syscall
    cmp %r13, %rax
    jne fail
    cmpl $0x1700, status(%rip)
    jne fail
    cmpl $0x1234, private_word(%rip)
    jne fail
    xor %eax, %eax
    mov $3, %edi
    lea byte(%rip), %rsi
    mov $1, %edx
    syscall
    cmp $1, %rax
    jne fail
    cmpb $'l', byte(%rip)
    jne fail
    mov $3, %eax
    mov $3, %edi
    syscall
    test %rax, %rax
    jne fail
    /* Linux reaps before copying status, including an EFAULT output. */
    mov $57, %eax
    syscall
    test %rax, %rax
    js fail
    je fault_child
    mov %rax, %r13
    mov %r13, %rdi
    mov $1, %esi
    xor %edx, %edx
    xor %r10d, %r10d
    mov $61, %eax
    syscall
    cmp $-14, %rax
    jne fail
    mov $61, %eax
    mov $-1, %rdi
    xor %esi, %esi
    xor %edx, %edx
    xor %r10d, %r10d
    syscall
    cmp $-10, %rax
    jne fail
    mov $1, %eax
    mov $1, %edi
    lea message(%rip), %rsi
    mov $11, %edx
    syscall
    xor %edi, %edi
    mov $231, %eax
    syscall
fault_child:
    mov $7, %edi
    mov $231, %eax
    syscall
child:
    movl $0x5678, private_word(%rip)
    mov $110, %eax
    syscall
    cmp %r12, %rax
    jne fail
    xor %eax, %eax
    mov $3, %edi
    lea byte(%rip), %rsi
    mov $1, %edx
    syscall
    cmp $1, %rax
    jne fail
    cmpb $'e', byte(%rip)
    jne fail
    mov $59, %eax
    xor %edi, %edi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $-14, %rax
    jne fail
    mov $59, %eax
    lea missing(%rip), %rdi
    xor %esi, %esi
    xor %edx, %edx
    syscall
    cmp $-2, %rax
    jne fail
    mov $72, %eax
    mov $3, %edi
    mov $1, %esi
    syscall
    cmp $1, %rax
    jne fail
    mov $59, %eax
    lea path(%rip), %rdi
    lea argv(%rip), %rsi
    lea env(%rip), %rdx
    syscall
fail:
    mov $1, %edi
    mov $231, %eax
    syscall
    ud2
.section .rodata
fixture: .asciz "/fixture.txt"
missing: .asciz "/missing"
path: .asciz "/bin/M4-child"
arg: .asciz "sentinel"
environment: .asciz "KEY=value"
message: .ascii "M4 fork ok\n"
.section .data
.balign 8
argv: .quad path, arg, 0
env: .quad environment, 0
private_word: .long 0x1234
.section .bss
.balign 8
status: .skip 4
byte: .skip 1
.section .note.GNU-stack,"",@progbits
