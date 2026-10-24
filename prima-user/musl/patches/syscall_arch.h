/* Prima M3 port of musl 1.2.5 arch/x86_64/syscall_arch.h.
 *
 * The Linux syscall instruction is not the system boundary. Every musl call
 * site keeps the `__syscallN` name, but the implementation is the Prima
 * user-space adapter (`prima-user/c/prima_syscall.c`), which translates Linux
 * call numbers to Prima natives and returns the raw negative errno.
 */
#define __SYSCALL_LL_E(x) (x)
#define __SYSCALL_LL_O(x) (x)

long __syscall0(long n);
long __syscall1(long n, long a1);
long __syscall2(long n, long a1, long a2);
long __syscall3(long n, long a1, long a2, long a3);
long __syscall4(long n, long a1, long a2, long a3, long a4);
long __syscall5(long n, long a1, long a2, long a3, long a4, long a5);
long __syscall6(long n, long a1, long a2, long a3, long a4, long a5, long a6);

/* Prima does not publish a Linux vDSO. Always use the adapter. */

#define IPC_64 0
