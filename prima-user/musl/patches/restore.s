/* Native signal restoration, not Linux rt_sigreturn (15 = Prima unmap). */
.text
.global __restore_rt
.hidden __restore_rt
.type __restore_rt,@function
__restore_rt:
	mov $58,%eax
	syscall
	ud2
.size __restore_rt,.-__restore_rt
.section .note.GNU-stack,"",@progbits
