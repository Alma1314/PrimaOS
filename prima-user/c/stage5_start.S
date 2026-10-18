.text
.global _start
.type _start,@function
_start:
    mov %rsp, %r9
    call stage5_main
    ud2
.size _start, .-_start
.section .note.GNU-stack,"",@progbits
