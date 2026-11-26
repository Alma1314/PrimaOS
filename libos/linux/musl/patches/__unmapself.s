/* The C adapter cleans sessions/clear_tid, then native 66 unmaps and exits
 * atomically on the kernel stack. No user-stack access follows the unmap. */
.text
.global __unmapself
.hidden __unmapself
.type __unmapself,@function
__unmapself:
    jmp __prima_unmapself
.size __unmapself,.-__unmapself
.section .note.GNU-stack,"",@progbits
