/* M2：Prima C 系统边界适配层。
 *
 * 边界规则：
 * - Linux 调用号只在本文件出现，翻译后只向内核发 Prima 原生号；
 * - 所有失败返回原始负 errno（不设置 errno，musl 的 syscall 约定）；
 * - fd 1/2 固定映射到管理器的 tty 字节流；fd 3..10 是本地表，文件项经存储服务；
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

/* ---------- M4/M7：本地 fd 表 ----------
 *
 * 本地 fd 3..10 由适配层拥有；文件项指向服务端句柄（3..6，最多 4 个并发 open），
 * `dup*` 出的多项共享同一句柄与 offset，最后一个引用关闭时才向服务端发 CLOSE。
 * 单条 IPC 最多携带 `PRIMA_FILE_CHUNK` 字节，故 read/write 在这里分片——调用者
 * 仍看到完整的 `read`/`write` 语义（短读只在 EOF/服务端容量边界出现）。
 *
 * 已知边界（P5-FD-01 partial）：无 fd 0；`dup2`/`dup3` 的目标只接受 3..10；
 * 无 exec，故 `O_CLOEXEC` 被接受但无语义效果；本地表满返回 EAGAIN。
 */
#define PRIMA_FD_FREE 0
#define PRIMA_FD_TTY 1
#define PRIMA_FD_FILE 2

static long prima_file_call(long op, long fd, uint64_t arg);
static long prima_reply_status(uint64_t reply);

struct prima_fd_entry {
    unsigned char kind;
    unsigned char object; /* tty fd（1/2）或服务端句柄（3..6） */
};

static struct prima_fd_entry prima_fds[PRIMA_LOCAL_FD_COUNT];
static unsigned char prima_file_refs[PRIMA_FILE_HANDLES];

static struct prima_fd_entry *prima_fd_entry(int fd) {
    if (fd < PRIMA_LOCAL_FD_FIRST || fd > PRIMA_LOCAL_FD_LAST)
        return 0;
    struct prima_fd_entry *entry = &prima_fds[fd - PRIMA_LOCAL_FD_FIRST];
    return entry->kind == PRIMA_FD_FREE ? 0 : entry;
}

static int prima_fd_has_room(void) {
    for (int i = 0; i < PRIMA_LOCAL_FD_COUNT; i++)
        if (prima_fds[i].kind == PRIMA_FD_FREE)
            return 1;
    return 0;
}

/* 返回最低空闲本地 fd；表满返回 -1。 */
static int prima_fd_install(unsigned char kind, unsigned char object) {
    for (int i = 0; i < PRIMA_LOCAL_FD_COUNT; i++) {
        if (prima_fds[i].kind == PRIMA_FD_FREE) {
            prima_fds[i].kind = kind;
            prima_fds[i].object = object;
            return i + PRIMA_LOCAL_FD_FIRST;
        }
    }
    return -1;
}

/* 释放一项；文件句柄的最后一个引用负责通知服务端。返回服务端状态（tty 为 0）。 */
static long prima_fd_drop(int fd) {
    struct prima_fd_entry *entry = &prima_fds[fd - PRIMA_LOCAL_FD_FIRST];
    long status = 0;
    if (entry->kind == PRIMA_FD_FILE) {
        unsigned char handle = entry->object;
        if (--prima_file_refs[handle - PRIMA_FILE_FIRST_FD] == 0) {
            long reply = prima_file_call(PRIMA_FILE_OP_CLOSE, handle, 0);
            if (reply < 0)
                status = reply;
            else if (prima_reply_status((uint64_t)reply) != 0)
                status = -prima_reply_status((uint64_t)reply);
        }
    }
    entry->kind = PRIMA_FD_FREE;
    return status;
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
    if (!prima_fd_has_room())
        return -PRIMA_ERRNO_EAGAIN;
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
    long handle = (long)(((uint64_t)reply >> PRIMA_FILE_FD_SHIFT) & 0xf);
    if (handle < PRIMA_FILE_FIRST_FD || handle > PRIMA_FILE_LAST_FD)
        return -PRIMA_ERRNO_EINVAL;
    prima_file_refs[handle - PRIMA_FILE_FIRST_FD] = 1;
    return prima_fd_install(PRIMA_FD_FILE, (unsigned char)handle);
}

static long prima_handle_read(int handle, void *buf, unsigned long len) {
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
        long reply = prima_file_call(PRIMA_FILE_OP_READ, handle, chunk);
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

static long prima_handle_write(int handle, const void *buf, unsigned long len) {
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
        long reply = prima_file_call(PRIMA_FILE_OP_WRITE, handle, arg);
        if (reply < 0)
            return done != 0 ? (long)done : reply;
        long status = prima_reply_status((uint64_t)reply);
        if (status != 0)
            return done != 0 ? (long)done : -status;
        done += chunk;
    }
    return (long)len;
}

static long prima_handle_lseek(int handle, long offset, int whence) {
    if (whence < PRIMA_SEEK_SET || whence > PRIMA_SEEK_END)
        return -PRIMA_ERRNO_EINVAL;
    if (offset < -0x20000000L || offset > 0x1fffffffL)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t arg = ((uint64_t)(whence & 3) << PRIMA_FILE_SEEK_WHENCE_SHIFT) |
                   ((uint64_t)offset & PRIMA_FILE_SEEK_OFFSET_MASK);
    long reply = prima_file_call(PRIMA_FILE_OP_LSEEK, handle, arg);
    if (reply < 0)
        return reply;
    uint64_t raw = (uint64_t)reply;
    long status = prima_reply_status(raw);
    if (status != 0)
        return -status;
    return (long)(raw & PRIMA_FILE_ARG_MASK);
}

static long prima_fd_read(int fd, void *buf, unsigned long len) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry || entry->kind != PRIMA_FD_FILE)
        return -PRIMA_ERRNO_EBADF;
    return prima_handle_read(entry->object, buf, len);
}

static long prima_fd_write(int fd, const void *buf, unsigned long len) {
    if (fd == 1 || fd == 2)
        return prima_write_bytes(fd, buf, len);
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind == PRIMA_FD_TTY)
        return prima_write_bytes(entry->object, buf, len);
    return prima_handle_write(entry->object, buf, len);
}

static long prima_fd_lseek(int fd, long offset, int whence) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry || entry->kind != PRIMA_FD_FILE)
        return -PRIMA_ERRNO_EBADF;
    return prima_handle_lseek(entry->object, offset, whence);
}

static long prima_fd_close(int fd) {
    if (fd == 1 || fd == 2)
        return 0;
    if (!prima_fd_entry(fd))
        return -PRIMA_ERRNO_EBADF;
    return prima_fd_drop(fd);
}

static long prima_fd_dup(int oldfd) {
    struct prima_fd_entry *entry = prima_fd_entry(oldfd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    int newfd = prima_fd_install(entry->kind, entry->object);
    if (newfd < 0)
        return -PRIMA_ERRNO_EAGAIN;
    if (entry->kind == PRIMA_FD_FILE)
        prima_file_refs[entry->object - PRIMA_FILE_FIRST_FD]++;
    return newfd;
}

/* dup2/dup3 共用：oldfd 已校验；newfd 必须落在本地表内。先为新引用加计数，
 * 再关闭目标项——即使两者指向同一服务端句柄，也不会在中途误发 CLOSE。 */
static long prima_fd_dup_to(int oldfd, int newfd) {
    struct prima_fd_entry *source = prima_fd_entry(oldfd);
    unsigned char kind = source->kind;
    unsigned char object = source->object;
    if (kind == PRIMA_FD_FILE)
        prima_file_refs[object - PRIMA_FILE_FIRST_FD]++;
    if (prima_fd_entry(newfd))
        (void)prima_fd_drop(newfd);
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].kind = kind;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].object = object;
    return newfd;
}

static long prima_fd_dup2(int oldfd, int newfd) {
    if (oldfd == newfd)
        return prima_fd_entry(oldfd) ? oldfd : -PRIMA_ERRNO_EBADF;
    if (!prima_fd_entry(oldfd))
        return -PRIMA_ERRNO_EBADF;
    if (newfd < PRIMA_LOCAL_FD_FIRST || newfd > PRIMA_LOCAL_FD_LAST)
        return -PRIMA_ERRNO_EBADF;
    return prima_fd_dup_to(oldfd, newfd);
}

static long prima_fd_dup3(int oldfd, int newfd, long flags) {
    if (oldfd == newfd || (flags & ~(long)PRIMA_O_CLOEXEC) != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (!prima_fd_entry(oldfd))
        return -PRIMA_ERRNO_EBADF;
    if (newfd < PRIMA_LOCAL_FD_FIRST || newfd > PRIMA_LOCAL_FD_LAST)
        return -PRIMA_ERRNO_EBADF;
    return prima_fd_dup_to(oldfd, newfd);
}

/* ---------- M6：POSIX 时间（PIT tick，10 ms 粒度） ----------
 *
 * `SYS_TIME_ENABLE` 启动 PIT 通道 0 的自由运行计数（不打开中断）；`SYS_TIME_TICKS`
 * 轮询硬件计数，与抽象 `SYS_TIME_NOW`（Timer 时钟）分离，不会提前触发既有 Timer 见证。
 *
 * 已知边界（P5-TIME-01 partial）：
 * - 仅 `CLOCK_MONOTONIC` 可查询；`CLOCK_REALTIME` 无墙上时钟，返回 `EINVAL`。
 * - `nanosleep` 是 CPU 轮询，不阻塞接收；无信号，故不会返回 `EINTR`，`rem` 不写回。
 * - 未知时钟 id/flag 返回 `EINVAL`；非 NULL 坏指针由用户态缺页终止，适配层只保证
 *   NULL → `EFAULT`。
 */
#define PRIMA_CLOCK_REALTIME 0
#define PRIMA_CLOCK_MONOTONIC 1
#define PRIMA_TIMER_ABSTIME 1
#define PRIMA_TICK_NS UINT64_C(10000000)
#define PRIMA_TICKS_PER_SEC UINT64_C(100)

static int prima_time_enabled;

static long prima_time_ready(void) {
    if (prima_time_enabled)
        return 0;
    long result = prima_raw(PRIMA_SYS_TIME_ENABLE, 0, 0, 0, 0, 0, 0);
    if (result != 0)
        return result < 0 ? result : -PRIMA_ERRNO_EINVAL;
    prima_time_enabled = 1;
    return 0;
}

static long prima_ticks(void) {
    return prima_raw(PRIMA_SYS_TIME_TICKS, 0, 0, 0, 0, 0, 0);
}

static long prima_clock_gettime(long clock_id, void *ts) {
    if (clock_id != PRIMA_CLOCK_MONOTONIC)
        return -PRIMA_ERRNO_EINVAL;
    if (!ts)
        return -PRIMA_ERRNO_EFAULT;
    long ready = prima_time_ready();
    if (ready < 0)
        return ready;
    long ticks = prima_ticks();
    if (ticks < 0)
        return ticks;
    unsigned long *out = ts;
    out[0] = (unsigned long)ticks / PRIMA_TICKS_PER_SEC;
    out[1] = ((unsigned long)ticks % PRIMA_TICKS_PER_SEC) * PRIMA_TICK_NS;
    return 0;
}

static long prima_sleep_ns(long clock_id, long flags, const void *req,
                           void *rem) {
    (void)rem;
    if (!req)
        return -PRIMA_ERRNO_EFAULT;
    const long *times = req;
    long sec = times[0];
    long nsec = times[1];
    if (sec < 0 || nsec < 0 || nsec >= 1000000000L)
        return -PRIMA_ERRNO_EINVAL;
    if ((flags & ~PRIMA_TIMER_ABSTIME) != 0)
        return -PRIMA_ERRNO_EINVAL;
    /* 相对睡眠接受 REALTIME/MONOTONIC（musl 的 nanosleep 传入 REALTIME）；
     * 绝对睡眠只对 MONOTONIC 有意义。 */
    if (clock_id != PRIMA_CLOCK_MONOTONIC &&
        !(clock_id == PRIMA_CLOCK_REALTIME && flags == 0))
        return -PRIMA_ERRNO_EINVAL;
    if (sec > (0x7fffffffffffffffL - nsec) / 1000000000L)
        return -PRIMA_ERRNO_EINVAL;
    unsigned long long ns =
        (unsigned long long)sec * 1000000000ULL + (unsigned long long)nsec;
    unsigned long long ticks = (ns + PRIMA_TICK_NS - 1) / PRIMA_TICK_NS;
    long ready = prima_time_ready();
    if (ready < 0)
        return ready;
    long now = prima_ticks();
    if (now < 0)
        return now;
    unsigned long long deadline =
        (flags & PRIMA_TIMER_ABSTIME) ? ticks : (unsigned long long)now + ticks;
    for (;;) {
        long current = prima_ticks();
        if (current < 0)
            return current;
        if ((unsigned long long)current >= deadline)
            break;
        /* PIT 计数独立于中断推进。这里不 `yield`：协作调度器在无其他 Ready
         * 任务时会让 `SYS_YIELD` 直接 panic；轮询间隔远小于 PIT 周期。 */
    }
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
        return prima_fd_write((int)a0, (const void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_writev: {
        const struct iovec *iov = (const struct iovec *)a1;
        long count = a2;
        if (count <= 0)
            return -PRIMA_ERRNO_EINVAL;
        if (!iov)
            return -PRIMA_ERRNO_EFAULT;
        unsigned long total = 0;
        for (long i = 0; i < count; i++) {
            unsigned long len = iov[i].iov_len;
            if (len > ~(unsigned long)0 - total)
                return -PRIMA_ERRNO_EINVAL;
            long written = prima_fd_write((int)a0, iov[i].iov_base, len);
            if (written < 0)
                return written;
            total += (unsigned long)written;
        }
        return (long)total;
    }
    case PRIMA_LINUX_SYS_close:
        return prima_fd_close((int)a0);
    case PRIMA_LINUX_SYS_read:
        return prima_fd_read((int)a0, (void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_lseek:
        return prima_fd_lseek((int)a0, (long)a1, (int)a2);
    case PRIMA_LINUX_SYS_dup:
        return prima_fd_dup((int)a0);
    case PRIMA_LINUX_SYS_dup2:
        return prima_fd_dup2((int)a0, (int)a1);
    case PRIMA_LINUX_SYS_dup3:
        return prima_fd_dup3((int)a0, (int)a1, a2);
    case PRIMA_LINUX_SYS_getpid:
        return prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    /* musl/x86_64 的 `open()` 走 SYS_open(2)，不是 openat(257)；两者落到同一子集。 */
    case PRIMA_LINUX_SYS_open:
        return prima_openat(PRIMA_AT_FDCWD, (const char *)a0, a1, a2);
    case PRIMA_LINUX_SYS_openat:
        return prima_openat(a0, (const char *)a1, a2, a3);
    case PRIMA_LINUX_SYS_ioctl:
        return -PRIMA_ERRNO_ENOSYS;
    case PRIMA_LINUX_SYS_clock_gettime:
        return prima_clock_gettime(a0, (void *)a1);
    case PRIMA_LINUX_SYS_clock_nanosleep:
        return prima_sleep_ns(a0, a1, (const void *)a2, (void *)a3);
    case PRIMA_LINUX_SYS_nanosleep:
        return prima_sleep_ns(PRIMA_CLOCK_REALTIME, 0, (const void *)a0,
                              (void *)a1);
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
