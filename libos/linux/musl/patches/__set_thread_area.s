/* Prima M3 port of musl 1.2.5 src/thread/x86_64/__set_thread_area.s.
 *
 * Route ARCH_SET_FS through the user-space adapter instead of issuing Linux
 * syscall 158 directly. The adapter maps it to SYS_TASK_SET_FS_BASE (38),
 * which the kernel validates and applies to the current task's IA32_FS_BASE.
 */
.text
.global __set_thread_area
.hidden __set_thread_area
.type __set_thread_area,@function
__set_thread_area:
	mov %rdi,%rdx           /* addr -> a2 */
	movl $0x1002,%esi       /* ARCH_SET_FS -> a1 */
	movl $158,%edi          /* Linux arch_prctl -> n */
	jmp __syscall2
