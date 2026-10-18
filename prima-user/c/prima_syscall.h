/* M2：C 程序使用的 Prima 系统边界适配层。
 *
 * `__syscall0..6` 接受 **Linux x86_64 调用号**（musl 的 `syscall_arch.h` 约定），
 * 在 `prima_syscall.c` 内翻译为 Prima 原生号；返回值保持原始负 errno。
 */
#ifndef PRIMA_SYSCALL_H
#define PRIMA_SYSCALL_H

#include "prima_abi.h"

extern struct prima_entry_ctx prima_ctx;

long __syscall0(long number);
long __syscall1(long number, long a0);
long __syscall2(long number, long a0, long a1);
long __syscall3(long number, long a0, long a1, long a2);
long __syscall4(long number, long a0, long a1, long a2, long a3);
long __syscall5(long number, long a0, long a1, long a2, long a3, long a4);
long __syscall6(long number, long a0, long a1, long a2, long a3, long a4, long a5);

/* 原生号直通：只用于探测/诊断，不参与 Linux 号翻译。 */
long prima_native_syscall0(long number);
long prima_native_syscall3(long number, long a0, long a1, long a2);

/* `write`/`read`/`writev`/`close`/`ioctl`/`_exit`/`exit` 由 libc（musl）提供，
 * 它们最终经上面的 `__syscallN` 进入本适配层；这里不再定义同名符号，避免覆盖
 * musl 的 stdio/exit 语义。`arch_prctl` 与 `set_tid_address` 不是 musl 的公开
 * 函数符号，故保留给探针使用。 */
int arch_prctl(int code, unsigned long addr);
long set_tid_address(int *tidptr);
/* libc 提供实现；探针的 Prima 入口需要声明。 */
void _exit(int status) __attribute__((noreturn));

#endif
