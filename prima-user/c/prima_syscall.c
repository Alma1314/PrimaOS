/* M2：Prima C 系统边界适配层。
 *
 * 边界规则：
 * - Linux 调用号只在本文件出现，翻译后只向内核发 Prima 原生号；
 * - 所有失败返回原始负 errno（不设置 errno，musl 的 syscall 约定）；
 * - fd 1/2 固定映射到管理器的 tty 字节流；通用 fd 表属 P5-FD-01，未实现；
 * - 每次 SEND 后显式 YIELD：自毁会清掉仍在接收队列里的消息，退出状态必须先被取走。
 */
#include "prima_syscall.h"

struct prima_entry_ctx prima_ctx;

/* 六参数陷入：号在 rax，参数依次 rdi/rsi/rdx/r10/r8/r9（Prima ABI）。 */
static long prima_raw(long number, long a0, long a1, long a2, long a3, long a4,
                      long a5) {
    register long r10 __asm__("r10") = a3;
    register long r8 __asm__("r8") = a4;
    register long r9 __asm__("r9") = a5;
    register long rax __asm__("rax") = number;
    __asm__ volatile("syscall"
                     : "+a"(rax)
                     : "D"(a0), "S"(a1), "d"(a2), "r"(r10), "r"(r8), "r"(r9)
                     : "rcx", "r11", "memory");
    return rax;
}

long prima_native_syscall0(long number) {
    return prima_raw(number, 0, 0, 0, 0, 0, 0);
}

long prima_native_syscall3(long number, long a0, long a1, long a2) {
    return prima_raw(number, a0, a1, a2, 0, 0, 0);
}

/* fd 1/2 的逐个字节转发；返回字节数或原始负 errno。 */
static long prima_write_bytes(int fd, const void *buf, unsigned long len) {
    const unsigned char *bytes = buf;
    if (fd != 1 && fd != 2)
        return -PRIMA_ERRNO_EBADF;
    if (len == 0)
        return 0;
    if (!bytes)
        return -PRIMA_ERRNO_EFAULT;
    for (unsigned long i = 0; i < len; i++) {
        long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
        uint64_t payload = PRIMA_STAGE5_TTY_WRITE_TAG |
                           ((uint64_t)(unsigned char)fd << 16) |
                           ((uint64_t)bytes[i] << 24) |
                           ((uint64_t)id & 0xff) << 32;
        long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                              (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
        if (sent != 0)
            return sent < 0 ? sent : -PRIMA_ERRNO_EFAULT;
        (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    }
    return (long)len;
}

__attribute__((noreturn)) static void prima_exit_group(int status) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload = PRIMA_STAGE5_C_PROBE_TAG |
                       ((uint64_t)(uint8_t)status << 16) |
                       ((uint64_t)id & 0xff);
    if (prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                  (long)prima_ctx.send_cap, (long)payload, 0, 0, 0) == 0) {
        (void)prima_raw(PRIMA_SYS_TASK_DESTROY, (long)prima_ctx.self_cap, 0, 0,
                        0, 0, 0);
    }
    for (;;) {
        long sink[3];
        (void)prima_raw(PRIMA_SYS_RECV, (long)sink, 0, 0, 0, 0, 0);
    }
}

/* Linux 号 → Prima 动作。六参数完整透传：musl 的取消点路径经 `__syscall6` 调用，
 * 少传参数会静默丢参。未列出的号一律 -ENOSYS（不在本阶段承诺）。 */
static long prima_translate(long number, long a0, long a1, long a2, long a3,
                            long a4, long a5) {
    (void)a3;
    (void)a4;
    (void)a5;
    switch (number) {
    case PRIMA_LINUX_SYS_write:
        return prima_write_bytes((int)a0, (const void *)a1,
                                 (unsigned long)a2);
    case PRIMA_LINUX_SYS_writev: {
        const struct iovec *iov = (const struct iovec *)a1;
        long count = a2;
        if (a0 != 1 && a0 != 2)
            return -PRIMA_ERRNO_EBADF;
        if (count <= 0)
            return -PRIMA_ERRNO_EINVAL;
        if (!iov)
            return -PRIMA_ERRNO_EFAULT;
        unsigned long total = 0;
        for (long i = 0; i < count; i++) {
            unsigned long len = iov[i].iov_len;
            if (len > ~(unsigned long)0 - total)
                return -PRIMA_ERRNO_EINVAL;
            long written = prima_write_bytes((int)a0, iov[i].iov_base, len);
            if (written < 0)
                return written;
            total += (unsigned long)written;
        }
        return (long)total;
    }
    case PRIMA_LINUX_SYS_close:
        return (a0 == 1 || a0 == 2) ? 0 : -PRIMA_ERRNO_EBADF;
    case PRIMA_LINUX_SYS_read:
        return -PRIMA_ERRNO_ENOSYS;
    case PRIMA_LINUX_SYS_ioctl:
        return -PRIMA_ERRNO_ENOSYS;
    case PRIMA_LINUX_SYS_set_tid_address:
        (void)a0;
        return prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    case PRIMA_LINUX_SYS_arch_prctl:
        if (a0 == PRIMA_ARCH_SET_FS) {
            long result = prima_raw(PRIMA_SYS_TASK_SET_FS_BASE, a1, 0, 0, 0, 0, 0);
            if (result == 0)
                prima_ctx.fs_base = (uint64_t)a1;
            return result;
        }
        if (a0 == PRIMA_ARCH_GET_FS) {
            if (!a1)
                return -PRIMA_ERRNO_EFAULT;
            *(unsigned long *)a1 = prima_ctx.fs_base;
            return 0;
        }
        return -PRIMA_ERRNO_EINVAL;
    case PRIMA_LINUX_SYS_exit:
    case PRIMA_LINUX_SYS_exit_group:
        prima_exit_group((int)a0);
    default:
        return -PRIMA_ERRNO_ENOSYS;
    }
}

long __syscall0(long number) { return prima_translate(number, 0, 0, 0, 0, 0, 0); }

long __syscall1(long number, long a0) {
    return prima_translate(number, a0, 0, 0, 0, 0, 0);
}

long __syscall2(long number, long a0, long a1) {
    return prima_translate(number, a0, a1, 0, 0, 0, 0);
}

long __syscall3(long number, long a0, long a1, long a2) {
    return prima_translate(number, a0, a1, a2, 0, 0, 0);
}

long __syscall4(long number, long a0, long a1, long a2, long a3) {
    return prima_translate(number, a0, a1, a2, a3, 0, 0);
}

long __syscall5(long number, long a0, long a1, long a2, long a3, long a4) {
    return prima_translate(number, a0, a1, a2, a3, a4, 0);
}

long __syscall6(long number, long a0, long a1, long a2, long a3, long a4,
                long a5) {
    return prima_translate(number, a0, a1, a2, a3, a4, a5);
}

long write(int fd, const void *buf, unsigned long count) {
    return __syscall3(PRIMA_LINUX_SYS_write, fd, (long)buf, (long)count);
}

long read(int fd, void *buf, unsigned long count) {
    return __syscall3(PRIMA_LINUX_SYS_read, fd, (long)buf, (long)count);
}

long writev(int fd, const struct iovec *iov, int iovcnt) {
    return __syscall3(PRIMA_LINUX_SYS_writev, fd, (long)iov, iovcnt);
}

int close(int fd) { return (int)__syscall1(PRIMA_LINUX_SYS_close, fd); }

int ioctl(int fd, unsigned long request, ...) {
    return (int)__syscall3(PRIMA_LINUX_SYS_ioctl, fd, (long)request, 0);
}

int arch_prctl(int code, unsigned long addr) {
    return (int)__syscall2(PRIMA_LINUX_SYS_arch_prctl, code, (long)addr);
}

long set_tid_address(int *tidptr) {
    return __syscall1(PRIMA_LINUX_SYS_set_tid_address, (long)tidptr);
}

void _exit(int status) {
    (void)__syscall1(PRIMA_LINUX_SYS_exit_group, status);
    for (;;)
        ;
}

void exit(int status) { _exit(status); }
