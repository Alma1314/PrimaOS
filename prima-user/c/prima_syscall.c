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

/* M3b：musl mallocng 的匿名内存。
 *
 * 服务管理器在创建 C 任务时把 64 页 arena 与栈作为**同一 Segment** 的两段映射
 * 进本任务根：首 16 页作 `brk` 区（mallocng 的元数据区），其余作匿名 `mmap`
 * 的 bump 区。适配层只做指针推进，不再发 `memseg_create`——C 探针运行期容量
 * 快照已是 segment 槽位 41/48、audit 757/768，任务内建段会立即触顶。
 *
 * 已知边界（M3 报告登记，不得据此宣称通用 mmap）：
 * - 只支持 `MAP_PRIVATE|MAP_ANONYMOUS`、fd=-1、offset=0；
 * - `MAP_FIXED` 只为 mallocng 的 brk guard 页返回原地址，不改变权限；
 * - `mprotect` 返回成功但不改变权限（匿名页一律 RW）；
 * - `munmap` 是空操作，bump 区不回收；无 `mremap`；
 * - 单次匿名映射上限 48 页，arena 用尽后 `mmap` 返回 `ENOMEM`。
 */
#define PRIMA_HEAP_VA UINT64_C(0x00000c0000000000)
#define PRIMA_HEAP_PAGES 64UL
#define PRIMA_BRK_PAGES 16UL
#define PRIMA_MMAP_PAGES (PRIMA_HEAP_PAGES - PRIMA_BRK_PAGES)
#define PRIMA_PAGE_SIZE 4096UL

#define PRIMA_MAP_SHARED 0x01
#define PRIMA_MAP_PRIVATE 0x02
#define PRIMA_MAP_FIXED 0x10
#define PRIMA_MAP_ANONYMOUS 0x20

static uint64_t prima_heap_pages;
static uint64_t prima_brk_cur;
static uint64_t prima_mmap_next;

/* arena 由服务管理器预先映射；这里只初始化 bump 指针。 */
static void prima_heap_ready(void) {
    if (prima_heap_pages != 0)
        return;
    prima_heap_pages = PRIMA_HEAP_PAGES;
    prima_brk_cur = PRIMA_HEAP_VA;
    prima_mmap_next = PRIMA_HEAP_VA + PRIMA_BRK_PAGES * PRIMA_PAGE_SIZE;
}

static long prima_brk(uint64_t requested) {
    prima_heap_ready();
    if (requested == 0)
        return (long)prima_brk_cur;
    if (requested < PRIMA_HEAP_VA ||
        requested > PRIMA_HEAP_VA + PRIMA_BRK_PAGES * PRIMA_PAGE_SIZE)
        return (long)prima_brk_cur;
    prima_brk_cur = requested;
    return (long)requested;
}

static long prima_mmap_anon(long requested_address, uint64_t length,
                            long flags, long fd, long offset) {
    if ((flags & PRIMA_MAP_FIXED) != 0) {
        /* mallocng 在 brk 区首页安装 guard；该区已映射，返回请求地址即可。 */
        return requested_address;
    }
    if ((flags & PRIMA_MAP_ANONYMOUS) == 0 ||
        (flags & PRIMA_MAP_SHARED) != 0 || (flags & PRIMA_MAP_PRIVATE) == 0 ||
        fd != -1 || offset != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (length == 0)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t pages = (length + PRIMA_PAGE_SIZE - 1) / PRIMA_PAGE_SIZE;
    if (pages == 0 || pages > PRIMA_MMAP_PAGES)
        return -PRIMA_ERRNO_ENOMEM;
    prima_heap_ready();
    uint64_t base = prima_mmap_next;
    uint64_t limit = PRIMA_HEAP_VA + PRIMA_HEAP_PAGES * PRIMA_PAGE_SIZE;
    if (base + pages * PRIMA_PAGE_SIZE > limit)
        return -PRIMA_ERRNO_ENOMEM;
    prima_mmap_next = base + pages * PRIMA_PAGE_SIZE;
    return (long)base;
}

static long prima_munmap_anon(uint64_t address) {
    /* bump arena 不回收单个映射；`free` 仍能复用 mallocng 自己的分组。 */
    (void)address;
    return 0;
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

/* ---------- M4：文件 fd 生命周期 ----------
 *
 * fd 3..6 由本适配层拥有，服务端持有打开句柄与共享 offset。单条 IPC 最多携带
 * `PRIMA_FILE_CHUNK` 字节，故 read/write 在这里分片——调用者仍看到完整的
 * `read`/`write` 语义（短读只在 EOF/服务端容量边界出现）。
 */
static unsigned char prima_file_open[PRIMA_FILE_HANDLES];

static int prima_file_fd(int fd) {
    return fd >= PRIMA_FILE_FIRST_FD && fd <= PRIMA_FILE_LAST_FD &&
           prima_file_open[fd - PRIMA_FILE_FIRST_FD] != 0;
}

/* 同步请求：发送一帧、让出、阻塞接收文件应答；返回应答 payload 或原始负 errno。 */
static long prima_file_call(long op, long fd, uint64_t arg) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload = PRIMA_STAGE5_FILE_REQUEST_TAG |
                       ((uint64_t)(op & 0xf) << PRIMA_FILE_OP_SHIFT) |
                       ((uint64_t)(fd & 0xf) << PRIMA_FILE_FD_SHIFT) |
                       ((uint64_t)(id & 0xff) << PRIMA_FILE_CLIENT_SHIFT) |
                       (arg & PRIMA_FILE_ARG_MASK);
    long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                          (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    if (sent != 0)
        return sent < 0 ? sent : -PRIMA_ERRNO_EFAULT;
    /* 不显式 yield：紧随其后的 RECV 在队列为空时阻塞并切走；显式 yield 会在
     * 抢占窗口里与任务状态迁移竞争。 */
    uint64_t caps[3];
    for (;;) {
        long raw = prima_raw(PRIMA_SYS_RECV, (long)caps, 0, 0, 0, 0, 0);
        if (raw < 0)
            return raw;
        if (((uint64_t)raw >> 48) == (PRIMA_STAGE5_FILE_REPLY_TAG >> 48))
            return raw;
        /* 文件请求是同步的；非文件消息不属于本通道，继续等待。 */
    }
}

static long prima_reply_status(uint64_t reply) {
    return (long)((reply >> PRIMA_FILE_STATUS_SHIFT) & 0xff);
}

/* openat 的冻结子集：单层短名（≤3 字节、无 '/'）、AT_FDCWD、access/CREAT/TRUNC。 */
static long prima_openat(long dirfd, const char *path, long flags, long mode) {
    (void)mode;
    if (dirfd != PRIMA_AT_FDCWD)
        return -PRIMA_ERRNO_EINVAL;
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    unsigned long len = 0;
    while (path[len] != 0) {
        if (path[len] == '/' || len >= 3)
            return -PRIMA_ERRNO_ENOENT;
        len++;
    }
    if (len == 0)
        return -PRIMA_ERRNO_ENOENT;
    long access = flags & PRIMA_O_ACCMODE;
    if (access == 3)
        return -PRIMA_ERRNO_EINVAL;
    long allowed = PRIMA_O_ACCMODE | PRIMA_O_CREAT | PRIMA_O_TRUNC |
                   PRIMA_O_LARGEFILE | PRIMA_O_CLOEXEC;
    if ((flags & ~allowed) != 0)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t internal = (uint64_t)access << PRIMA_FILE_ACCESS_SHIFT;
    if (flags & PRIMA_O_CREAT)
        internal |= PRIMA_FILE_FLAG_CREAT;
    if (flags & PRIMA_O_TRUNC)
        internal |= PRIMA_FILE_FLAG_TRUNC;
    uint64_t arg = ((internal & 0xf) << PRIMA_FILE_OPEN_FLAGS_SHIFT) |
                   (((uint64_t)len & 3) << PRIMA_FILE_OPEN_PATH_LEN_SHIFT);
    for (unsigned long i = 0; i < len; i++)
        arg |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long reply = prima_file_call(PRIMA_FILE_OP_OPEN, 0, arg);
    if (reply < 0)
        return reply;
    long status = prima_reply_status((uint64_t)reply);
    if (status != 0)
        return -status;
    long fd = (long)(((uint64_t)reply >> PRIMA_FILE_FD_SHIFT) & 0xf);
    if (fd < PRIMA_FILE_FIRST_FD || fd > PRIMA_FILE_LAST_FD)
        return -PRIMA_ERRNO_EINVAL;
    prima_file_open[fd - PRIMA_FILE_FIRST_FD] = 1;
    return fd;
}

static long prima_file_read(int fd, void *buf, unsigned long len) {
    if (!prima_file_fd(fd))
        return -PRIMA_ERRNO_EBADF;
    if (len == 0)
        return 0;
    if (!buf)
        return -PRIMA_ERRNO_EFAULT;
    unsigned char *out = buf;
    unsigned long done = 0;
    while (done < len) {
        unsigned long chunk = len - done;
        if (chunk > PRIMA_FILE_CHUNK)
            chunk = PRIMA_FILE_CHUNK;
        long reply = prima_file_call(PRIMA_FILE_OP_READ, fd, chunk);
        if (reply < 0)
            return done != 0 ? (long)done : reply;
        uint64_t raw = (uint64_t)reply;
        long status = prima_reply_status(raw);
        if (status != 0)
            return done != 0 ? (long)done : -status;
        unsigned long count = (raw >> PRIMA_FILE_WRITE_COUNT_SHIFT) & 0xff;
        uint64_t data = raw & PRIMA_FILE_ARG_MASK;
        if (count > chunk)
            count = chunk;
        for (unsigned long i = 0; i < count; i++)
            out[done + i] = (unsigned char)(data >> (8 * i));
        done += count;
        if (count < chunk)
            break; /* EOF 或服务端可用字节不足 */
    }
    return (long)done;
}

static long prima_file_write(int fd, const void *buf, unsigned long len) {
    if (!prima_file_fd(fd))
        return -PRIMA_ERRNO_EBADF;
    if (len == 0)
        return 0;
    if (!buf)
        return -PRIMA_ERRNO_EFAULT;
    const unsigned char *in = buf;
    unsigned long done = 0;
    while (done < len) {
        unsigned long chunk = len - done;
        if (chunk > PRIMA_FILE_CHUNK)
            chunk = PRIMA_FILE_CHUNK;
        uint64_t arg = (uint64_t)chunk << PRIMA_FILE_WRITE_COUNT_SHIFT;
        for (unsigned long i = 0; i < chunk; i++)
            arg |= (uint64_t)in[done + i] << (8 * i);
        long reply = prima_file_call(PRIMA_FILE_OP_WRITE, fd, arg);
        if (reply < 0)
            return done != 0 ? (long)done : reply;
        long status = prima_reply_status((uint64_t)reply);
        if (status != 0)
            return done != 0 ? (long)done : -status;
        done += chunk;
    }
    return (long)len;
}

static long prima_file_lseek(int fd, long offset, int whence) {
    if (!prima_file_fd(fd))
        return -PRIMA_ERRNO_EBADF;
    if (whence < PRIMA_SEEK_SET || whence > PRIMA_SEEK_END)
        return -PRIMA_ERRNO_EINVAL;
    if (offset < -0x20000000L || offset > 0x1fffffffL)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t arg = ((uint64_t)(whence & 3) << PRIMA_FILE_SEEK_WHENCE_SHIFT) |
                   ((uint64_t)offset & PRIMA_FILE_SEEK_OFFSET_MASK);
    long reply = prima_file_call(PRIMA_FILE_OP_LSEEK, fd, arg);
    if (reply < 0)
        return reply;
    uint64_t raw = (uint64_t)reply;
    long status = prima_reply_status(raw);
    if (status != 0)
        return -status;
    return (long)(raw & PRIMA_FILE_ARG_MASK);
}

static long prima_file_close(int fd) {
    if (!prima_file_fd(fd))
        return -PRIMA_ERRNO_EBADF;
    long reply = prima_file_call(PRIMA_FILE_OP_CLOSE, fd, 0);
    if (reply < 0)
        return reply;
    long status = prima_reply_status((uint64_t)reply);
    if (status != 0)
        return -status;
    prima_file_open[fd - PRIMA_FILE_FIRST_FD] = 0;
    return 0;
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
    switch (number) {
    case PRIMA_LINUX_SYS_mmap:
        /* Linux 顺序：addr, len, prot, flags, fd, offset；prot 不参与判定。 */
        return prima_mmap_anon(a0, (uint64_t)a1, a3, a4, a5);
    case PRIMA_LINUX_SYS_munmap:
        return prima_munmap_anon((uint64_t)a0);
    case PRIMA_LINUX_SYS_mprotect:
        /* 匿名页一律 RW；mallocng 只对已 RW 的元数据页调用。 */
        return 0;
    case PRIMA_LINUX_SYS_brk:
        return prima_brk((uint64_t)a0);
    case PRIMA_LINUX_SYS_write:
        if (a0 == 1 || a0 == 2)
            return prima_write_bytes((int)a0, (const void *)a1,
                                     (unsigned long)a2);
        return prima_file_write((int)a0, (const void *)a1, (unsigned long)a2);
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
        if (a0 == 1 || a0 == 2)
            return 0;
        return prima_file_close((int)a0);
    case PRIMA_LINUX_SYS_read:
        return prima_file_read((int)a0, (void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_lseek:
        return prima_file_lseek((int)a0, (long)a1, (int)a2);
    case PRIMA_LINUX_SYS_openat:
        return prima_openat(a0, (const char *)a1, a2, a3);
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

int arch_prctl(int code, unsigned long addr) {
    return (int)__syscall2(PRIMA_LINUX_SYS_arch_prctl, code, (long)addr);
}

long set_tid_address(int *tidptr) {
    return __syscall1(PRIMA_LINUX_SYS_set_tid_address, (long)tidptr);
}
