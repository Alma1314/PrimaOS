/* Real same-root Tasks, independent FS, compare-and-park, wake and clear_tid. */
.section .text
.global _start
_start:
    mov $39, %eax
    syscall
    mov %rax, %r12
    mov $158, %eax
    mov $0x1002, %edi
    lea parent_tls(%rip), %rsi
    syscall
    test %rax, %rax
    jne fail
    mov %fs:0, %rax
    cmp $0x12345678, %rax
    jne fail
    mov $158, %eax
    mov $0x1003, %edi
    lea observed_fs(%rip), %rsi
    syscall
    test %rax, %rax
    jne fail
    lea parent_tls(%rip), %rax
    cmp %rax, observed_fs(%rip)
    jne fail
    /* Wrong value and early wake do not create a wake token. */
    lea mismatch(%rip), %rdi
    mov $129, %esi
    mov $1, %edx
    mov $202, %eax
    syscall
    test %rax, %rax
    jne fail
    lea mismatch(%rip), %rdi
    mov $128, %esi
    xor %edx, %edx
    xor %r10d, %r10d
    mov $202, %eax
    syscall
    cmp $-11, %rax
    jne fail
    lea mismatch+1(%rip), %rdi
    mov $202, %eax
    syscall
    cmp $-22, %rax
    jne fail
    xor %edi, %edi
    mov $202, %eax
    syscall
    cmp $-14, %rax
    jne fail
    lea gate_word(%rip), %rdi
    mov $137, %esi
    xor %edx, %edx
    xor %r10d, %r10d
    xor %r9d, %r9d
    mov $202, %eax
    syscall
    cmp $-22, %rax
    jne fail
    mov $128, %esi
    lea zero_time(%rip), %r10
    mov $202, %eax
    syscall
    cmp $-110, %rax
    jne fail
    /* VM/FS/FILES/SIGHAND/THREAD/SYSVSEM + TLS + both tid writes + clear. */
    mov $0x13d0f00, %edi
    lea child_stack_end(%rip), %rsi
    lea parent_tid(%rip), %rdx
    lea child_tid(%rip), %r10
    lea child_tls(%rip), %r8
    mov $56, %eax
    syscall
    test %rax, %rax
    js fail
    je child
    cmp %eax, parent_tid(%rip)
    jne fail
    mov %eax, %r13d
    /* Child repeatedly wakes this unchanged word until it wakes a real waiter. */
    lea gate_word(%rip), %rdi
    mov $128, %esi
    xor %edx, %edx
    xor %r10d, %r10d
    mov $202, %eax
    syscall
    test %rax, %rax
    jne fail
wait_done:
    cmpl $1, done_word(%rip)
    je join
    lea done_word(%rip), %rdi
    mov $128, %esi
    xor %edx, %edx
    xor %r10d, %r10d
    mov $202, %eax
    syscall
    test %rax, %rax
    je wait_done
    cmp $-11, %rax
    je wait_done
    jmp fail
join:
    cmpl $1, wake_count(%rip)
    jne fail
    mov child_tid(%rip), %edx
    test %edx, %edx
    je joined
    cmp %r13d, %edx
    jne fail
    lea child_tid(%rip), %rdi
    xor %esi, %esi /* kernel clear_child_tid uses the non-PRIVATE futex key */
    xor %r10d, %r10d
    mov $202, %eax
    syscall
    test %rax, %rax
    je join
    cmp $-11, %rax
    je join
    jmp fail
joined:
    mov %fs:0, %rax
    cmp $0x12345678, %rax
    jne fail
    mov $1, %eax
    mov $1, %edi
    lea message(%rip), %rsi
    mov $6, %edx
    syscall
    cmp $6, %rax
    jne fail
    xor %edi, %edi
    mov $231, %eax
    syscall
    ud2
child:
    mov $39, %eax
    syscall
    cmp %r12, %rax
    jne fail
    mov $186, %eax
    syscall
    cmp %r12, %rax
    je fail
    cmp %eax, child_tid(%rip)
    jne fail
    mov %fs:0, %rax
    cmp $0x23456789, %rax
    jne fail
    mov $158, %eax
    mov $0x1003, %edi
    lea child_observed_fs(%rip), %rsi
    syscall
    test %rax, %rax
    jne fail
    lea child_tls(%rip), %rax
    cmp %rax, child_observed_fs(%rip)
    jne fail
wake_parent:
    lea gate_word(%rip), %rdi
    mov $129, %esi
    mov $1, %edx
    mov $202, %eax
    syscall
    test %rax, %rax
    je wake_parent
    cmp $1, %rax
    jne fail
    movl $1, wake_count(%rip)
    movl $1, done_word(%rip)
    lea done_word(%rip), %rdi
    mov $129, %esi
    mov $1, %edx
    mov $202, %eax
    syscall
    test %rax, %rax
    js fail
    xor %edi, %edi
    mov $60, %eax
    syscall
    ud2
fail:
    mov $1, %edi
    mov $231, %eax
    syscall
    ud2
.section .rodata
message: .ascii "P2 ok\n"
.balign 8
parent_tls: .quad 0x12345678
child_tls: .quad 0x23456789
zero_time: .quad 0, 0
.section .data
.balign 4
mismatch: .long 7
.section .bss
.balign 8
observed_fs: .skip 8
child_observed_fs: .skip 8
parent_tid: .skip 4
child_tid: .skip 4
gate_word: .skip 4
done_word: .skip 4
wake_count: .skip 4
.balign 16
child_stack: .skip 8192
child_stack_end:
.section .note.GNU-stack,"",@progbits
