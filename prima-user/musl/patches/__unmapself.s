/* Detached-thread stack reclamation has no safe Prima contract yet.
 * Keep the mapping until process teardown and use the normal thread-exit
 * adapter (including clear_child_tid). Never issue Linux munmap/exit numbers.
 * This guard does not establish pthread_detach support. */
.text
.global __unmapself
.hidden __unmapself
.type __unmapself,@function
__unmapself:
	mov $60,%edi
	xor %esi,%esi
	jmp __syscall1
.size __unmapself,.-__unmapself
.section .note.GNU-stack,"",@progbits
