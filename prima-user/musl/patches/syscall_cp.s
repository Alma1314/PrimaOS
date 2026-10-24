/* Cancellation points must cross the same adapter as ordinary musl calls.
 * This preserves the pending-cancel check; asynchronous cancellation and
 * interruption of an in-flight Prima IPC remain unsupported. */
.text
.global __syscall_cp_asm
.hidden __syscall_cp_asm
.global __cp_begin
.hidden __cp_begin
.global __cp_end
.hidden __cp_end
.global __cp_cancel
.hidden __cp_cancel
.hidden __cancel
.type __syscall_cp_asm,@function
__syscall_cp_asm:
__cp_begin:
	cmpl $0,(%rdi)
	jne __cp_cancel
	mov %rsi,%rdi
	mov %rdx,%rsi
	mov %rcx,%rdx
	mov %r8,%rcx
	mov %r9,%r8
	mov 8(%rsp),%r9
	mov 16(%rsp),%rax
	mov %rax,8(%rsp)
	jmp __syscall6
__cp_end:
__cp_cancel:
	jmp __cancel
.size __syscall_cp_asm,.-__syscall_cp_asm
.section .note.GNU-stack,"",@progbits
