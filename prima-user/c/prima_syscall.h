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

long write(int fd, const void *buf, unsigned long count);
long read(int fd, void *buf, unsigned long count);
long writev(int fd, const struct iovec *iov, int iovcnt);
int close(int fd);
int ioctl(int fd, unsigned long request, ...);
int arch_prctl(int code, unsigned long addr);
long set_tid_address(int *tidptr);
void _exit(int status) __attribute__((noreturn));
void exit(int status) __attribute__((noreturn));

#endif
