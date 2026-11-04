.section .text
.global _start
_start:
    cmpq $2, (%rsp)
    jne fail
    mov 16(%rsp), %rsi
    movabs $0x6c656e69746e6573, %rax /* sentinel */
    cmp %rax, (%rsi)
    jne fail
    cmpb $0, 8(%rsi)
    jne fail
    cmpq $0, 24(%rsp)
    jne fail
    mov 32(%rsp), %rsi
    movabs $0x756c61763d59454b, %rax /* KEY=valu */
    cmp %rax, (%rsi)
    jne fail
    cmpw $0x65, 8(%rsi)
    jne fail
    cmpq $0, 40(%rsp)
    jne fail
    mov $72, %eax
    mov $3, %edi
    mov $1, %esi
    syscall
    cmp $-9, %rax
    jne fail
    mov $1, %eax
    mov $1, %edi
    lea message(%rip), %rsi
    mov $14, %edx
    syscall
    mov $23, %edi
    mov $231, %eax
    syscall
fail:
    mov $1, %edi
    mov $231, %eax
    syscall
    ud2
.section .rodata
message: .ascii "M4 exec child\n"
.section .note.GNU-stack,"",@progbits
