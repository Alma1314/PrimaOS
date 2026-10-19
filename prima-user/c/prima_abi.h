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
#define PRIMA_SYS_TIME_ENABLE 39
#define PRIMA_SYS_TIME_TICKS 40

/* Linux x86_64 syscall numbers：只在 prima_syscall.c 内翻译，绝不进入内核。 */
#define PRIMA_LINUX_SYS_read 0
#define PRIMA_LINUX_SYS_write 1
#define PRIMA_LINUX_SYS_close 3
#define PRIMA_LINUX_SYS_lseek 8
#define PRIMA_LINUX_SYS_mmap 9
#define PRIMA_LINUX_SYS_mprotect 10
#define PRIMA_LINUX_SYS_munmap 11
#define PRIMA_LINUX_SYS_brk 12
#define PRIMA_LINUX_SYS_ioctl 16
#define PRIMA_LINUX_SYS_writev 20
#define PRIMA_LINUX_SYS_nanosleep 35
#define PRIMA_LINUX_SYS_exit 60
#define PRIMA_LINUX_SYS_clock_gettime 228
#define PRIMA_LINUX_SYS_clock_nanosleep 230
#define PRIMA_LINUX_SYS_arch_prctl 158
#define PRIMA_LINUX_SYS_set_tid_address 218
#define PRIMA_LINUX_SYS_exit_group 231
#define PRIMA_LINUX_SYS_openat 257

/* 原始负 errno。 */
#define PRIMA_ERRNO_ENOENT 2
#define PRIMA_ERRNO_EBADF 9
#define PRIMA_ERRNO_EAGAIN 11
#define PRIMA_ERRNO_ENOMEM 12
#define PRIMA_ERRNO_EACCES 13
#define PRIMA_ERRNO_EFAULT 14
#define PRIMA_ERRNO_EINVAL 22
#define PRIMA_ERRNO_ENOSPC 28
#define PRIMA_ERRNO_ENOSYS 38

/* M4：openat 的冻结 flag 子集。adapter 归一化后只把 access/CREAT/TRUNC 传给服务端；
 * O_LARGEFILE/O_CLOEXEC 被接受但无语义效果（单线程、无 exec）。 */
#define PRIMA_O_ACCMODE 3
#define PRIMA_O_RDONLY 0
#define PRIMA_O_WRONLY 1
#define PRIMA_O_RDWR 2
#define PRIMA_O_CREAT 0100
#define PRIMA_O_TRUNC 01000
#define PRIMA_O_LARGEFILE 0100000
#define PRIMA_O_CLOEXEC 02000000
#define PRIMA_AT_FDCWD (-100)

/* M4：lseek 的 whence 子集。 */
#define PRIMA_SEEK_SET 0
#define PRIMA_SEEK_CUR 1
#define PRIMA_SEEK_END 2

/* arch_prctl 子命令（Linux 冻结值）。 */
#define PRIMA_ARCH_SET_FS 0x1002
#define PRIMA_ARCH_GET_FS 0x1003

/* 阶段 5 线上标签；必须与 `prima-runtime/src/protocol.rs` 一致。 */
#define PRIMA_STAGE5_TTY_WRITE_TAG UINT64_C(0xc602000000000000)
#define PRIMA_STAGE5_C_PROBE_TAG UINT64_C(0xc500000000005a00)

/* M4：文件服务请求/应答；布局见 `SERVICE_FILE_REQUEST_TAG`。
 * 请求 = tag | op<<44 | fd<<40 | client<<32 | arg。
 * 应答 = tag | op<<44 | fd<<40 | status<<32 | data。 */
#define PRIMA_STAGE5_FILE_REQUEST_TAG UINT64_C(0x5ed4000000000000)
#define PRIMA_STAGE5_FILE_REPLY_TAG UINT64_C(0x5ed5000000000000)
#define PRIMA_FILE_OP_SHIFT 44
#define PRIMA_FILE_FD_SHIFT 40
#define PRIMA_FILE_CLIENT_SHIFT 32
#define PRIMA_FILE_STATUS_SHIFT 32
#define PRIMA_FILE_ARG_MASK UINT64_C(0xffffffff)
#define PRIMA_FILE_OP_OPEN 0
#define PRIMA_FILE_OP_READ 1
#define PRIMA_FILE_OP_WRITE 2
#define PRIMA_FILE_OP_LSEEK 3
#define PRIMA_FILE_OP_CLOSE 4
#define PRIMA_FILE_OPEN_FLAGS_SHIFT 28
#define PRIMA_FILE_OPEN_PATH_LEN_SHIFT 26
#define PRIMA_FILE_OPEN_PATH_MASK UINT64_C(0xffffff)
#define PRIMA_FILE_FLAG_CREAT 1
#define PRIMA_FILE_FLAG_TRUNC 2
#define PRIMA_FILE_ACCESS_SHIFT 2
#define PRIMA_FILE_WRITE_COUNT_SHIFT 24
#define PRIMA_FILE_SEEK_WHENCE_SHIFT 30
#define PRIMA_FILE_SEEK_OFFSET_MASK UINT64_C(0x3fffffff)
#define PRIMA_FILE_CHUNK 3
#define PRIMA_FILE_HANDLES 4
#define PRIMA_FILE_FIRST_FD 3
#define PRIMA_FILE_LAST_FD (PRIMA_FILE_FIRST_FD + PRIMA_FILE_HANDLES - 1)

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
