/* Prima has no vfork contract. Linux 58 is Prima sigreturn: never issue it. */
.text
.global vfork
.type vfork,@function
vfork:
	mov $-38,%rdi
	jmp __syscall_ret
.size vfork,.-vfork
.section .note.GNU-stack,"",@progbits
