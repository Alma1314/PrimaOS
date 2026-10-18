/* M2：C 适配层与探针共享的调用号 / errno / 线上标签。
 *
 * Prima 号必须与 `prima-abi/src/lib.rs` 同步；Linux 号只允许出现在适配层内部。
 * 本文件按 `-ffreestanding` 编译，不依赖宿主 libc。
 */
#ifndef PRIMA_ABI_H
#define PRIMA_ABI_H

#include <stdint.h>

/* Prima native syscall numbers. */
#define PRIMA_SYS_TASK_ID 7
#define PRIMA_SYS_YIELD 8
#define PRIMA_SYS_SEND 17
#define PRIMA_SYS_RECV 18
#define PRIMA_SYS_TASK_DESTROY 20
#define PRIMA_SYS_ABI_CHECK 37
#define PRIMA_SYS_TASK_SET_FS_BASE 38

/* Linux x86_64 syscall numbers：只在 prima_syscall.c 内翻译，绝不进入内核。 */
#define PRIMA_LINUX_SYS_read 0
#define PRIMA_LINUX_SYS_write 1
#define PRIMA_LINUX_SYS_close 3
#define PRIMA_LINUX_SYS_exit 60
#define PRIMA_LINUX_SYS_ioctl 16
#define PRIMA_LINUX_SYS_writev 20
#define PRIMA_LINUX_SYS_arch_prctl 158
#define PRIMA_LINUX_SYS_set_tid_address 218
#define PRIMA_LINUX_SYS_exit_group 231

/* 原始负 errno。 */
#define PRIMA_ERRNO_EBADF 9
#define PRIMA_ERRNO_EACCES 13
#define PRIMA_ERRNO_EFAULT 14
#define PRIMA_ERRNO_EINVAL 22
#define PRIMA_ERRNO_ENOSYS 38

/* arch_prctl 子命令（Linux 冻结值）。 */
#define PRIMA_ARCH_SET_FS 0x1002
#define PRIMA_ARCH_GET_FS 0x1003

/* 阶段 5 线上标签；必须与 `prima-runtime/src/protocol.rs` 一致。 */
#define PRIMA_STAGE5_TTY_WRITE_TAG UINT64_C(0xc602000000000000)
#define PRIMA_STAGE5_C_PROBE_TAG UINT64_C(0xc500000000005a00)

/* 入口寄存器 → prima_ctx 的布局（由 stage5_start.S 写入）。 */
struct prima_entry_ctx {
    uint64_t self_cap;
    uint64_t send_cap;
    uint64_t code_cap;
    uint64_t name;
    uint64_t manager_id;
    uint64_t fs_base;
};

/* 与 struct iovec 同布局；musl 也使用这一 x86_64 布局。 */
struct iovec {
    void *iov_base;
    unsigned long iov_len;
};

#endif
