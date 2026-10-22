/* M2：Prima C 系统边界适配层。
 *
 * 边界规则：
 * - Linux 调用号只在本文件出现，翻译后只向内核发 Prima 原生号；
 * - 所有失败返回原始负 errno（不设置 errno，musl 的 syscall 约定）；
 * - fd 0 是 null（读 EOF/写丢弃）、fd 1/2 是管理器的 tty 字节流；fd 3..18 是通用
 *   本地表，文件/目录项经用户态 VFS 存储服务；
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

long prima_native_syscall4(long number, long a0, long a1, long a2, long a3) {
    return prima_raw(number, a0, a1, a2, a3, 0, 0);
}

/* M3b/M10：musl 的匿名内存与保护。
 *
 * 服务管理器在创建 C 任务时把 104 页 arena 与栈作为**同一 Segment** 的两段映射进本
 * 任务根：前 8 页是 C 栈，随后 96 页堆（heap page 0..95）。适配层把堆切成两个区：
 * 首 40 页作 `brk` 区（mallocng 的元数据区），其余 56 页作匿名 `mmap` 区。
 *
 * M10 起 `mmap/munmap/mprotect` 不再只是指针推进：
 * - `mmap` 在 56 页区里做按页 first-fit 分配；新页清零；`MAP_FIXED` 只接受堆区
 *   （mallocng 的 brk guard 页）或 mmap 区内的地址。
 * - 首次 mmap 会向管理器申请 arena Segment 的派生映射授权（`Derive|SegMapR|SegMapW`），
 *   之后 `munmap` 用 `SYS_MEMSEG_UNMAP` 真正拆 PTE，`mmap` 复用地址时用
 *   `SYS_MEMSEG_MAP` 重新映射。
 * - `mprotect` 走内核 `SYS_MEMSEG_PROTECT=41`；RW→R 的收缩不需要额外授权，R→RW
 *   需要上面的 Segment cap。W^X：匿名段 allowed_access 只有 RW，故任何 `PROT_EXEC`
 *   请求返回 `EACCES`。
 *
 * 已知边界（M10 报告登记）：
 * - `PROT_NONE` 近似为只读（内核 `memseg_protect` 的访问集不可为空），页仍可读；
 * - 无文件映射、无 `MAP_SHARED`、无 `mremap`；`MAP_FIXED` 的地址必须在本 arena 内；
 * - 单次匿名映射上限 56 页；brk 区不在本 mmap 分配器内，`brk` 仍是 40 页上限；
 * - 管理器未授予 Segment cap 时退化为“只 free 不 unmap”，R→RW 的 mprotect 返回 `EACCES`。
 */
#define PRIMA_HEAP_VA UINT64_C(0x00000c0000000000)
#define PRIMA_HEAP_PAGES 96UL
#define PRIMA_BRK_PAGES 40UL
#define PRIMA_MMAP_PAGES (PRIMA_HEAP_PAGES - PRIMA_BRK_PAGES)
#define PRIMA_PAGE_SIZE 4096UL
#define PRIMA_MMAP_BASE (PRIMA_HEAP_VA + PRIMA_BRK_PAGES * PRIMA_PAGE_SIZE)
#define PRIMA_MMAP_LIMIT (PRIMA_HEAP_VA + PRIMA_HEAP_PAGES * PRIMA_PAGE_SIZE)
/* heap page i 对应 arena Segment 偏移 i + 8；8 是 servicemgr 的 C_STACK_PAGES。 */
#define PRIMA_SEG_HEAP_OFFSET 8UL
#define PRIMA_MMAP_BYTES ((PRIMA_MMAP_PAGES + 7) / 8)

#define PRIMA_MAP_SHARED 0x01
#define PRIMA_MAP_PRIVATE 0x02
#define PRIMA_MAP_FIXED 0x10
#define PRIMA_MAP_ANONYMOUS 0x20
#define PRIMA_PROT_READ 0x1
#define PRIMA_PROT_WRITE 0x2
#define PRIMA_PROT_EXEC 0x4

static uint64_t prima_brk_cur;
static unsigned char prima_mmap_used[PRIMA_MMAP_BYTES];
static unsigned char prima_mmap_present[PRIMA_MMAP_BYTES];
static uint64_t prima_arena_cap;
static int prima_arena_state; /* 0 = 未申请，1 = 已获得，-1 = 不可用 */

static int prima_bit_get(const unsigned char *map, unsigned page) {
    return (map[page / 8] >> (page % 8)) & 1;
}

static void prima_bit_set(unsigned char *map, unsigned page, int value) {
    if (value)
        map[page / 8] |= (unsigned char)(1u << (page % 8));
    else
        map[page / 8] &= (unsigned char)~(1u << (page % 8));
}

/* arena 由服务管理器预先映射；这里只初始化分配位图。 */
static void prima_heap_ready(void) {
    if (prima_brk_cur != 0)
        return;
    prima_brk_cur = PRIMA_HEAP_VA;
    for (unsigned i = 0; i < PRIMA_MMAP_BYTES; i++) {
        prima_mmap_used[i] = 0;
        prima_mmap_present[i] = 0xff;
    }
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

/* 首次 mmap 向管理器申请 arena 的派生 cap；失败后本任务退化为“只 free 不 unmap”。 */
static int prima_arena_acquire(void) {
    if (prima_arena_state != 0)
        return prima_arena_state > 0;
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload =
        PRIMA_STAGE5_VMA_CAP_REQUEST_TAG |
        ((uint64_t)(id & 0xff) << PRIMA_VMA_CLIENT_SHIFT);
    long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                          (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    if (sent != 0) {
        prima_arena_state = -1;
        return 0;
    }
    uint64_t caps[3];
    for (;;) {
        long raw = prima_raw(PRIMA_SYS_RECV, (long)caps, 0, 0, 0, 0, 0);
        if (raw < 0) {
            prima_arena_state = -1;
            return 0;
        }
        uint64_t reply = (uint64_t)raw;
        if ((reply >> 48) == (PRIMA_STAGE5_VMA_CAP_REPLY_TAG >> 48)) {
            if (((reply >> PRIMA_VMA_STATUS_SHIFT) & 0xff) == 0 &&
                caps[2] >= 1 && caps[0] != 0) {
                prima_arena_cap = caps[0];
                prima_arena_state = 1;
                return 1;
            }
            prima_arena_state = -1;
            return 0;
        }
        /* 申请期间到达的非 VMA 消息不属于本通道；当前协议无并发请求，直接忽略。 */
    }
}

/* PROT 位 → COSIR 访问集；PROT_NONE 取最小非空访问集 R，PROT_EXEC 在匿名段不可满足。 */
static long prima_prot_access(long prot) {
    if ((prot & ~(long)(PRIMA_PROT_READ | PRIMA_PROT_WRITE | PRIMA_PROT_EXEC)) != 0)
        return -PRIMA_ERRNO_EINVAL;
    if ((prot & PRIMA_PROT_EXEC) != 0)
        return -PRIMA_ERRNO_EACCES;
    long access = 0;
    if ((prot & PRIMA_PROT_READ) != 0)
        access |= PRIMA_ACCESS_R;
    if ((prot & PRIMA_PROT_WRITE) != 0)
        access |= PRIMA_ACCESS_W;
    if (access == 0)
        access = PRIMA_ACCESS_R;
    return access;
}

/* 映射 index 起的连续 pages 页；返回 0 或原始负 errno。 */
static long prima_arena_map_pages(unsigned index, unsigned pages, long access) {
    if (pages == 0)
        return 0;
    if (prima_arena_cap == 0)
        return -PRIMA_ERRNO_ENOMEM;
    uint64_t va = PRIMA_MMAP_BASE + (uint64_t)index * PRIMA_PAGE_SIZE;
    uint64_t seg_offset =
        PRIMA_SEG_HEAP_OFFSET + PRIMA_BRK_PAGES + (uint64_t)index;
    return prima_raw(PRIMA_SYS_MEMSEG_MAP, (long)prima_arena_cap,
                     (long)prima_ctx.self_cap, (long)seg_offset, (long)va,
                     (long)pages, access);
}

/* 真正拆除 index 起的连续 pages 页；无 cap 时保留映射（退化模式）。 */
static long prima_arena_unmap_pages(unsigned index, unsigned pages) {
    if (pages == 0 || prima_arena_cap == 0)
        return 0;
    uint64_t va = PRIMA_MMAP_BASE + (uint64_t)index * PRIMA_PAGE_SIZE;
    return prima_raw(PRIMA_SYS_MEMSEG_UNMAP, (long)prima_ctx.self_cap, (long)va,
                     (long)pages, 0, 0, 0);
}

/* 释放 mmap 区 [index, index+pages) 的占用；present 页整段拆映射。 */
static long prima_arena_release(unsigned index, unsigned pages) {
    for (unsigned page = index; page < index + pages; page++)
        prima_bit_set(prima_mmap_used, page, 0);
    unsigned cursor = index;
    while (cursor < index + pages) {
        if (!prima_bit_get(prima_mmap_present, cursor)) {
            cursor++;
            continue;
        }
        unsigned start = cursor;
        while (cursor < index + pages &&
               prima_bit_get(prima_mmap_present, cursor))
            cursor++;
        long result = prima_arena_unmap_pages(start, cursor - start);
        if (result != 0)
            return result;
        /* 退化模式（无 cap）没有真正拆映射，present 必须保持 1。 */
        if (prima_arena_cap != 0) {
            for (unsigned page = start; page < cursor; page++)
                prima_bit_set(prima_mmap_present, page, 0);
        }
    }
    return 0;
}

static int prima_find_free_run(unsigned pages) {
    unsigned run = 0;
    for (unsigned page = 0; page < PRIMA_MMAP_PAGES; page++) {
        if (!prima_bit_get(prima_mmap_used, page)) {
            run++;
            if (run == pages)
                return (int)(page + 1 - pages);
        } else {
            run = 0;
        }
    }
    return -1;
}

static void prima_zero(uint64_t address, uint64_t length) {
    for (uint64_t offset = 0; offset < length; offset += 8)
        *(volatile uint64_t *)(address + offset) = 0;
}

static long prima_mmap_anon(long requested_address, uint64_t length, long prot,
                            long flags, long fd, long offset) {
    if ((flags & PRIMA_MAP_ANONYMOUS) == 0 || (flags & PRIMA_MAP_SHARED) != 0 ||
        (flags & PRIMA_MAP_PRIVATE) == 0 || fd != -1 || offset != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (length == 0)
        return -PRIMA_ERRNO_EINVAL;
    if (length > UINT64_MAX - (PRIMA_PAGE_SIZE - 1))
        return -PRIMA_ERRNO_ENOMEM;
    uint64_t pages64 = (length + PRIMA_PAGE_SIZE - 1) / PRIMA_PAGE_SIZE;
    if (pages64 == 0 || pages64 > PRIMA_MMAP_PAGES)
        return -PRIMA_ERRNO_ENOMEM;
    unsigned pages = (unsigned)pages64;
    long access = prima_prot_access(prot);
    if (access < 0)
        return access;
    prima_heap_ready();
    (void)prima_arena_acquire();
    /* 没有 Segment cap 时无法先 R 再补 W；PROT_NONE 直接按 RW 返回（近似）。 */
    if (prima_arena_cap == 0 && (access & (PRIMA_ACCESS_R | PRIMA_ACCESS_W)) !=
                                     (PRIMA_ACCESS_R | PRIMA_ACCESS_W))
        access = PRIMA_ACCESS_R | PRIMA_ACCESS_W;

    unsigned index;
    uint64_t base;
    if ((flags & PRIMA_MAP_FIXED) != 0) {
        if (((uint64_t)requested_address & (PRIMA_PAGE_SIZE - 1)) != 0)
            return -PRIMA_ERRNO_EINVAL;
        base = (uint64_t)requested_address;
        /* mallocng 在 brk 区首页安装 guard：该区已映射，按原地址成功返回。 */
        if (base >= PRIMA_HEAP_VA && base < PRIMA_MMAP_BASE &&
            base + pages64 * PRIMA_PAGE_SIZE <= PRIMA_MMAP_BASE)
            return requested_address;
        if (base < PRIMA_MMAP_BASE || base >= PRIMA_MMAP_LIMIT ||
            base + pages64 * PRIMA_PAGE_SIZE > PRIMA_MMAP_LIMIT ||
            base + pages64 * PRIMA_PAGE_SIZE < base)
            return -PRIMA_ERRNO_ENOMEM;
        index = (unsigned)((base - PRIMA_MMAP_BASE) / PRIMA_PAGE_SIZE);
        long released = prima_arena_release(index, pages);
        if (released != 0)
            return released;
    } else {
        int found = prima_find_free_run(pages);
        if (found < 0)
            return -PRIMA_ERRNO_ENOMEM;
        index = (unsigned)found;
        base = PRIMA_MMAP_BASE + (uint64_t)index * PRIMA_PAGE_SIZE;
    }

    unsigned char was_present[PRIMA_MMAP_BYTES];
    for (unsigned i = 0; i < PRIMA_MMAP_BYTES; i++)
        was_present[i] = prima_mmap_present[i];
    unsigned cursor = index;
    while (cursor < index + pages) {
        if (prima_bit_get(prima_mmap_present, cursor)) {
            cursor++;
            continue;
        }
        unsigned start = cursor;
        while (cursor < index + pages &&
               !prima_bit_get(prima_mmap_present, cursor))
            cursor++;
        long mapped = prima_arena_map_pages(start, cursor - start, access);
        if (mapped != 0) {
            /* 失败原子：把本次新映射的页拆回映射前状态。 */
            for (unsigned page = index; page < index + pages; page++) {
                if (!prima_bit_get(was_present, page) &&
                    prima_bit_get(prima_mmap_present, page)) {
                    (void)prima_arena_unmap_pages(page, 1);
                    prima_bit_set(prima_mmap_present, page, 0);
                }
            }
            return mapped;
        }
        for (unsigned page = start; page < cursor; page++)
            prima_bit_set(prima_mmap_present, page, 1);
    }
    for (unsigned page = index; page < index + pages; page++)
        prima_bit_set(prima_mmap_used, page, 1);
    prima_zero(base, pages64 * PRIMA_PAGE_SIZE);
    /* mmap 的初始 prot 也必须生效（新页由服务管理器预映射为 RW）。 */
    if (access != (PRIMA_ACCESS_R | PRIMA_ACCESS_W)) {
        long protected = prima_raw(PRIMA_SYS_MEMSEG_PROTECT,
                                   (long)prima_ctx.self_cap, (long)base,
                                   (long)pages, access, 0, 0);
        if (protected != 0) {
            (void)prima_arena_release(index, pages);
            return protected;
        }
    }
    return (long)base;
}

static long prima_munmap_anon(uint64_t address, uint64_t length) {
    if ((address & (PRIMA_PAGE_SIZE - 1)) != 0 || length == 0 ||
        length > UINT64_MAX - (PRIMA_PAGE_SIZE - 1))
        return -PRIMA_ERRNO_EINVAL;
    uint64_t pages64 = (length + PRIMA_PAGE_SIZE - 1) / PRIMA_PAGE_SIZE;
    if (pages64 > PRIMA_MMAP_PAGES)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t span = pages64 * PRIMA_PAGE_SIZE;
    if (address < PRIMA_MMAP_BASE || address + span > PRIMA_MMAP_LIMIT ||
        address + span < address)
        return -PRIMA_ERRNO_EINVAL;
    unsigned index = (unsigned)((address - PRIMA_MMAP_BASE) / PRIMA_PAGE_SIZE);
    unsigned pages = (unsigned)pages64;
    prima_heap_ready();
    if (prima_arena_state == 0)
        (void)prima_arena_acquire();
    /* Linux 允许 munmap 未映射页；这里同样只释放“占用”位。 */
    for (unsigned page = index; page < index + pages; page++)
        prima_bit_set(prima_mmap_used, page, 0);
    return prima_arena_release(index, pages);
}

static long prima_mprotect_anon(uint64_t address, uint64_t length, long prot) {
    if ((address & (PRIMA_PAGE_SIZE - 1)) != 0 || length == 0 ||
        length > UINT64_MAX - (PRIMA_PAGE_SIZE - 1))
        return -PRIMA_ERRNO_EINVAL;
    uint64_t pages64 = (length + PRIMA_PAGE_SIZE - 1) / PRIMA_PAGE_SIZE;
    if (pages64 == 0 || pages64 > PRIMA_MMAP_PAGES)
        return -PRIMA_ERRNO_ENOMEM;
    uint64_t span = pages64 * PRIMA_PAGE_SIZE;
    if (address < PRIMA_MMAP_BASE || address + span > PRIMA_MMAP_LIMIT ||
        address + span < address)
        return -PRIMA_ERRNO_ENOMEM;
    unsigned index = (unsigned)((address - PRIMA_MMAP_BASE) / PRIMA_PAGE_SIZE);
    unsigned pages = (unsigned)pages64;
    prima_heap_ready();
    if (prima_arena_state == 0)
        (void)prima_arena_acquire();
    long access = prima_prot_access(prot);
    if (access < 0)
        return access;
    /* 只有仍被占用的已映射页可以改保护；缩权不要求 Segment cap。 */
    for (unsigned page = index; page < index + pages; page++) {
        if (!prima_bit_get(prima_mmap_used, page) ||
            !prima_bit_get(prima_mmap_present, page))
            return -PRIMA_ERRNO_ENOMEM;
    }
    long result = prima_raw(PRIMA_SYS_MEMSEG_PROTECT,
                            (long)prima_ctx.self_cap, (long)address, (long)pages,
                            access, 0, 0);
    if (result == -PRIMA_ERRNO_ENOENT)
        return -PRIMA_ERRNO_ENOMEM;
    return result;
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

/* ---------- M8：通用本地 fd 表与用户态 VFS ----------
 *
 * fd 0..18 由适配层拥有：fd 0 预置 null（读 EOF、写丢弃），fd 1/2 预置 tty；
 * 文件/目录项指向服务端句柄（3..10，最多 8 个并发 open）。`dup*` 出的多项共享
 * 同一句柄、offset 与目录游标，最后一个引用关闭时才向服务端发 CLOSE。
 *
 * 路径在存储服务里解析（支持绝对/相对、`.`/`..`、多级目录与 dirfd 起始目录）；
 * 单条 IPC 最多携带 `PRIMA_FILE_CHUNK` 字节，故 read/write 在这里分片，open 的
 * 路径用 OPEN/PATH/COMMIT 三段式发送。`getdents64` 把服务端的目录字节流翻译成
 * Linux `linux_dirent64` 布局。
 *
 * 已知边界（P5-FS-01/P5-FD-01 partial）：无符号链接、无 mkdir/unlink/rename、
 * cwd 为每任务独立的适配层状态（M9 起支持 chdir/getcwd）、公共 `read`/`write`
 * 不把目录当普通文件。
 */
#define PRIMA_FD_FREE 0
#define PRIMA_FD_TTY 1
#define PRIMA_FD_NULL 2
#define PRIMA_FD_FILE 3
#define PRIMA_FD_DIR 4
#define PRIMA_FD_SOCKET 5

static long prima_file_call(long op, long fd, uint64_t arg);
static long prima_vfs_call(uint64_t payload);
static long prima_absolute_path(long dirfd, const char *path, char *out,
                                unsigned long *out_len);
static long prima_reply_status(uint64_t reply);
static long prima_chmod_path(long dirfd, const char *path, long mode);
static long prima_net_request(unsigned char op, unsigned char handle, uint64_t arg,
                              uint64_t *reply);
static long prima_vfs_chmod_request(unsigned char start, const char *path,
                                    unsigned long len, unsigned short mode);
static void prima_dir_reset(unsigned char handle);

/* M13b：单用户权限模型的进程级 umask（默认 022）。 */
static long prima_umask_value = 022;

struct prima_fd_entry {
    unsigned char kind;
    unsigned char object; /* tty fd（1/2）、服务端句柄（3..10）或 0 */
    unsigned char node;   /* 服务端节点编号（目录相对 open 用） */
    unsigned char cloexec; /* M12b：记录 O_CLOEXEC；本阶段无 exec 语义 */
};

static struct prima_fd_entry prima_fds[PRIMA_LOCAL_FD_COUNT] = {
    {PRIMA_FD_NULL, 0, 0, 0},
    {PRIMA_FD_TTY, 1, 0, 0},
    {PRIMA_FD_TTY, 2, 0, 0},
};
static unsigned char prima_file_refs[PRIMA_FILE_HANDLES];
// M19：socket 服务端句柄的本地引用计数（dup 共享）。
static unsigned char prima_net_refs[PRIMA_NET_SOCKETS];

/* M13a：目录 fd 的规范化绝对路径（与 cwd 一样按文本保存），供 fchdir 使用。
 * dup/dup2/dup3 复制路径；关闭 fd 时清除。 */
static char prima_fd_paths[PRIMA_LOCAL_FD_COUNT][PRIMA_VFS_PATH_MAX + 1];

static void prima_fd_path_clear(int fd) {
    if (fd < PRIMA_LOCAL_FD_FIRST || fd > PRIMA_LOCAL_FD_LAST)
        return;
    prima_fd_paths[fd - PRIMA_LOCAL_FD_FIRST][0] = 0;
}

static void prima_fd_path_set(int fd, const char *path) {
    if (fd < PRIMA_LOCAL_FD_FIRST || fd > PRIMA_LOCAL_FD_LAST)
        return;
    char *out = prima_fd_paths[fd - PRIMA_LOCAL_FD_FIRST];
    unsigned long len = 0;
    while (path[len] != 0 && len < PRIMA_VFS_PATH_MAX) {
        out[len] = path[len];
        len++;
    }
    out[len] = 0;
}

static void prima_fd_path_copy(int dst, int src) {
    if (dst < PRIMA_LOCAL_FD_FIRST || dst > PRIMA_LOCAL_FD_LAST ||
        src < PRIMA_LOCAL_FD_FIRST || src > PRIMA_LOCAL_FD_LAST)
        return;
    prima_fd_path_set(dst, prima_fd_paths[src - PRIMA_LOCAL_FD_FIRST]);
}

struct prima_dir_cache {
    unsigned char len;
    unsigned char pos;
    unsigned char buf[PRIMA_DIR_CACHE_BYTES];
};
static struct prima_dir_cache prima_dirs[PRIMA_FILE_HANDLES];

struct prima_dir_record {
    unsigned short node;
    unsigned char kind;
    unsigned char name_len;
    unsigned char name[PRIMA_DIR_CACHE_BYTES - 4];
};

struct prima_dirent64 {
    uint64_t d_ino;
    int64_t d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[];
};

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
static int prima_fd_install(unsigned char kind, unsigned char object,
                            unsigned char node, unsigned char cloexec) {
    for (int i = 0; i < PRIMA_LOCAL_FD_COUNT; i++) {
        if (prima_fds[i].kind == PRIMA_FD_FREE) {
            prima_fds[i].kind = kind;
            prima_fds[i].object = object;
            prima_fds[i].node = node;
            prima_fds[i].cloexec = cloexec;
            return i + PRIMA_LOCAL_FD_FIRST;
        }
    }
    return -1;
}

static void prima_dir_reset(unsigned char handle) {
    if (handle < PRIMA_FILE_FIRST_FD || handle > PRIMA_FILE_LAST_FD)
        return;
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    cache->len = 0;
    cache->pos = 0;
}

/* 释放一项；文件/目录句柄的最后一个引用负责通知服务端。返回服务端状态。 */
static long prima_fd_drop(int fd) {
    struct prima_fd_entry *entry = &prima_fds[fd - PRIMA_LOCAL_FD_FIRST];
    long status = 0;
    if (entry->kind == PRIMA_FD_SOCKET) {
        unsigned char handle = entry->object;
        if (handle < PRIMA_NET_SOCKETS && --prima_net_refs[handle] == 0) {
            uint64_t reply = 0;
            (void)prima_net_request(PRIMA_NET_OP_CLOSE, handle, 0, &reply);
        }
    } else if (entry->kind == PRIMA_FD_FILE || entry->kind == PRIMA_FD_DIR) {
        unsigned char handle = entry->object;
        if (--prima_file_refs[handle - PRIMA_FILE_FIRST_FD] == 0) {
            long reply = prima_file_call(PRIMA_FILE_OP_CLOSE, handle, 0);
            if (reply < 0)
                status = reply;
            else if (prima_reply_status((uint64_t)reply) != 0)
                status = -prima_reply_status((uint64_t)reply);
            prima_dir_reset(handle);
        }
    }
    entry->kind = PRIMA_FD_FREE;
    entry->object = 0;
    entry->node = 0;
    entry->cloexec = 0;
    prima_fd_path_clear(fd);
    return status;
}

/* 同步请求：发送一帧文件请求、阻塞接收文件应答；返回应答 payload 或原始负 errno。 */
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

/* M8：OPEN/PATH 只需要入队；COMMIT 才同步等待一条应答。 */
static long prima_vfs_send(uint64_t payload) {
    long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                          (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    if (sent != 0)
        return sent < 0 ? sent : -PRIMA_ERRNO_EFAULT;
    return 0;
}

/* M8：COMMIT 同步等待 VFS 应答（OPEN/PATH 的应答由服务端省略）。 */
static long prima_vfs_call(uint64_t payload) {
    long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                          (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    if (sent != 0)
        return sent < 0 ? sent : -PRIMA_ERRNO_EFAULT;
    uint64_t caps[3];
    for (;;) {
        long raw = prima_raw(PRIMA_SYS_RECV, (long)caps, 0, 0, 0, 0, 0);
        if (raw < 0)
            return raw;
        if (((uint64_t)raw >> 48) == (PRIMA_STAGE5_FILE_REPLY_TAG >> 48))
            return raw;
    }
}

static long prima_reply_status(uint64_t reply) {
    return (long)((reply >> PRIMA_FILE_STATUS_SHIFT) & 0xff);
}

/* ---------- M9：工作目录、路径合成与 statx ----------
 *
 * cwd 由适配层保存为**规范化绝对路径**（≤ PRIMA_VFS_PATH_MAX 字节）。阶段 5 的 rootfs
 * 无符号链接，故 `..` 只按字面分量回退；`chdir` 先用 STAT 确认目标为目录，成功后才
 * 更新 cwd。`openat(AT_FDCWD, 相对路径)` 与 `statx(AT_FDCWD, ...)` 先把相对路径与 cwd
 * 合成为绝对路径再送服务端；`dirfd` 形式仍以该目录节点为起点，语义不变。
 *
 * 已知边界（P5-FS-01/P5-PROC-03 partial）：路径 ≤64 字节、名称 ≤16 字节、分量 ≤6；
 * 无符号链接、无 `fchdir`、无进程级 cwd 继承（每任务各自初始化为 `/`）。
 */
static char prima_cwd[PRIMA_VFS_PATH_MAX + 1] = "/";

struct prima_stat_meta {
    unsigned char node;
    unsigned char kind;
    unsigned short mode;
    unsigned short size;
};

static unsigned long prima_strlen_bounded(const char *text, unsigned long max) {
    unsigned long len = 0;
    while (text[len] != 0) {
        if (len >= max)
            return max + 1;
        len++;
    }
    return len;
}

/* 把 path 加上当前 cwd 前缀并消除 `.`/`..`/重复 `/`；失败返回负 errno。 */
static long prima_normalize_path(const char *path, char *out,
                                 unsigned long *out_len) {
    unsigned long len = 0;
    if (path[0] != '/') {
        unsigned long base = prima_strlen_bounded(prima_cwd, PRIMA_VFS_PATH_MAX);
        if (base > PRIMA_VFS_PATH_MAX)
            return -PRIMA_ERRNO_EINVAL;
        for (unsigned long i = 0; i < base; i++)
            out[len++] = prima_cwd[i];
    }
    const char *cursor = path;
    while (*cursor != 0) {
        while (*cursor == '/')
            cursor++;
        if (*cursor == 0)
            break;
        const char *part = cursor;
        unsigned long part_len = 0;
        while (*cursor != 0 && *cursor != '/') {
            cursor++;
            part_len++;
        }
        if (part_len == 1 && part[0] == '.')
            continue;
        if (part_len == 2 && part[0] == '.' && part[1] == '.') {
            while (len > 0 && out[len - 1] != '/')
                len--;
            if (len > 0)
                len--;
            continue;
        }
        if (len + 1 + part_len > PRIMA_VFS_PATH_MAX)
            return -PRIMA_ERRNO_EINVAL;
        if (len == 0 || out[len - 1] != '/')
            out[len++] = '/';
        for (unsigned long i = 0; i < part_len; i++)
            out[len++] = part[i];
    }
    if (len == 0)
        out[len++] = '/';
    out[len] = 0;
    *out_len = len;
    return 0;
}

/* M13a：把 dirfd + path 合成为规范化绝对路径（供 fchdir/rename/readlink 使用）。
 * 目录 fd 的基准路径来自打开时记录的文本；未知路径返回 EINVAL。 */
static long prima_absolute_path(long dirfd, const char *path, char *out,
                                unsigned long *out_len) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (dirfd == PRIMA_AT_FDCWD)
        return prima_normalize_path(path, out, out_len);
    struct prima_fd_entry *dir = prima_fd_entry((int)dirfd);
    if (!dir)
        return -PRIMA_ERRNO_EBADF;
    if (dir->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    const char *base = prima_fd_paths[dirfd - PRIMA_LOCAL_FD_FIRST];
    if (base[0] == 0)
        return -PRIMA_ERRNO_EINVAL;
    unsigned long base_len = prima_strlen_bounded(base, PRIMA_VFS_PATH_MAX);
    unsigned long path_len = prima_strlen_bounded(path, PRIMA_VFS_PATH_MAX);
    if (base_len > PRIMA_VFS_PATH_MAX || path_len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    char joined[PRIMA_VFS_PATH_MAX + 2];
    unsigned long len = 0;
    for (unsigned long i = 0; i < base_len; i++)
        joined[len++] = base[i];
    if (len == 0 || joined[len - 1] != '/')
        joined[len++] = '/';
    if (len + path_len > PRIMA_VFS_PATH_MAX + 1)
        return -PRIMA_ERRNO_EINVAL;
    for (unsigned long i = 0; i < path_len; i++)
        joined[len++] = path[i];
    joined[len] = 0;
    return prima_normalize_path(joined, out, out_len);
}

/* 解析 dirfd + path → (起始节点, 实际发送的路径)。cwd 合成缓冲由调用方提供。 */
static long prima_path_target(long dirfd, const char *path, unsigned char *start,
                              char *buffer, const char **send_path) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (dirfd != PRIMA_AT_FDCWD) {
        struct prima_fd_entry *dir = prima_fd_entry((int)dirfd);
        if (!dir)
            return -PRIMA_ERRNO_EBADF;
        if (dir->kind != PRIMA_FD_DIR)
            return -PRIMA_ERRNO_ENOTDIR;
        *start = dir->node;
        *send_path = path;
        return 0;
    }
    *start = 0;
    if (path[0] == '/') {
        *send_path = path;
        return 0;
    }
    unsigned long len = 0;
    long normalized = prima_normalize_path(path, buffer, &len);
    if (normalized < 0)
        return normalized;
    *send_path = buffer;
    return 0;
}

/* 发送一次 OPEN/PATH/COMMIT；成功把应答 payload 写入 reply。 */
static long prima_vfs_path_request(uint64_t internal, unsigned char start,
                                   const char *path, unsigned long len,
                                   uint64_t *reply) {
    uint64_t meta = PRIMA_STAGE5_VFS_OPEN_TAG |
                    ((internal & 0xff) << PRIMA_VFS_FLAGS_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT) |
                    (((uint64_t)start & 0xff) << PRIMA_VFS_START_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    long raw = prima_vfs_call(PRIMA_STAGE5_VFS_COMMIT_TAG |
                              (((uint64_t)id & 0xff)
                               << PRIMA_VFS_COMMIT_CLIENT_SHIFT));
    if (raw < 0)
        return raw;
    *reply = (uint64_t)raw;
    return 0;
}

/* M13a：双路径操作的单个槽：OPEN（前 3 字节）+ PATH（每段 5 字节），不等待应答。 */
static long prima_pair_stage(unsigned char op, unsigned char slot,
                             const char *path, unsigned long len) {
    if (len == 0 || len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t meta = PRIMA_STAGE5_VFS_PAIR_TAG |
                    ((uint64_t)(slot & 1) << PRIMA_VFS_PAIR_SLOT_SHIFT) |
                    ((uint64_t)(op & 7) << PRIMA_VFS_PAIR_OP_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    return 0;
}

/* 同步 COMMIT 双路径操作；成功把 PAIR_REPLY 写入 reply。 */
static long prima_pair_commit(unsigned char op, unsigned char slot,
                              uint64_t *reply) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload = PRIMA_STAGE5_VFS_PAIR_COMMIT_TAG |
                       ((uint64_t)(slot & 1) << PRIMA_VFS_PAIR_SLOT_SHIFT) |
                       ((uint64_t)(op & 7) << PRIMA_VFS_PAIR_OP_SHIFT) |
                       (((uint64_t)id & 0xff) << PRIMA_VFS_COMMIT_CLIENT_SHIFT);
    long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                          (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    if (sent != 0)
        return sent < 0 ? sent : -PRIMA_ERRNO_EFAULT;
    uint64_t caps[3];
    for (;;) {
        long raw = prima_raw(PRIMA_SYS_RECV, (long)caps, 0, 0, 0, 0, 0);
        if (raw < 0)
            return raw;
        if (((uint64_t)raw >> 48) == (PRIMA_STAGE5_VFS_PAIR_REPLY_TAG >> 48)) {
            *reply = (uint64_t)raw;
            return 0;
        }
    }
}

static long prima_pair_status(uint64_t reply) {
    return (long)((reply >> PRIMA_VFS_PAIR_STATUS_SHIFT) & 0xff);
}

/* 只解析路径取元数据；不分配服务端句柄，也不做读写权限检查。 */
static long prima_stat_fetch(unsigned char start, const char *path,
                             struct prima_stat_meta *out) {
    unsigned long len = prima_strlen_bounded(path, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t reply = 0;
    long request = prima_vfs_path_request(PRIMA_VFS_FLAG_STAT, start, path, len,
                                          &reply);
    if (request < 0)
        return request;
    long status = prima_reply_status(reply);
    if (status != 0)
        return -status;
    out->node = (unsigned char)(reply & PRIMA_VFS_REPLY_NODE_MASK);
    out->kind = (unsigned char)((reply >> PRIMA_VFS_REPLY_KIND_SHIFT) &
                                PRIMA_VFS_REPLY_KIND_MASK);
    out->mode =
        (unsigned short)((reply >> PRIMA_VFS_REPLY_MODE_SHIFT) & PRIMA_VFS_REPLY_MODE_MASK);
    out->size =
        (unsigned short)((reply >> PRIMA_VFS_REPLY_SIZE_SHIFT) & PRIMA_VFS_REPLY_SIZE_MASK);
    return 0;
}

/* M8/M9：打开/创建路径。cwd 默认为根；`dirfd` 形式从该目录节点起。 */
static long prima_openat(long dirfd, const char *path, long flags, long mode) {
    (void)mode;
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    /* 空路径不能经 cwd 合成为 "."：Linux `open("")` 是 ENOENT。 */
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    const char *send = path;
    unsigned char start = 0;
    long target = prima_path_target(dirfd, path, &start, resolved, &send);
    if (target < 0)
        return target;
    unsigned long len = prima_strlen_bounded(send, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    if (len == 0)
        return -PRIMA_ERRNO_ENOENT;
    long access = flags & PRIMA_O_ACCMODE;
    if (access == 3)
        return -PRIMA_ERRNO_EINVAL;
    long allowed = PRIMA_O_ACCMODE | PRIMA_O_CREAT | PRIMA_O_EXCL |
                   PRIMA_O_TRUNC | PRIMA_O_APPEND | PRIMA_O_DIRECTORY |
                   PRIMA_O_LARGEFILE | PRIMA_O_CLOEXEC;
    if ((flags & ~allowed) != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (!prima_fd_has_room())
        return -PRIMA_ERRNO_EAGAIN;

    uint64_t internal = (uint64_t)access << PRIMA_VFS_ACCESS_SHIFT;
    if (flags & PRIMA_O_CREAT)
        internal |= PRIMA_VFS_FLAG_CREATE;
    if (flags & PRIMA_O_EXCL)
        internal |= PRIMA_VFS_FLAG_EXCLUSIVE;
    if (flags & PRIMA_O_TRUNC)
        internal |= PRIMA_VFS_FLAG_TRUNC;
    if (flags & PRIMA_O_APPEND)
        internal |= PRIMA_VFS_FLAG_APPEND;
    if (flags & PRIMA_O_DIRECTORY)
        internal |= PRIMA_VFS_FLAG_DIRECTORY;

    uint64_t reply = 0;
    long request = prima_vfs_path_request(internal, start, send, len, &reply);
    if (request < 0)
        return request;
    long status = prima_reply_status(reply);
    if (status != 0)
        return -status;
    long handle = (long)((reply >> PRIMA_FILE_FD_SHIFT) & 0xf);
    if (handle < PRIMA_FILE_FIRST_FD || handle > PRIMA_FILE_LAST_FD)
        return -PRIMA_ERRNO_EINVAL;
    unsigned char node = (unsigned char)(reply & PRIMA_VFS_REPLY_NODE_MASK);
    unsigned char kind =
        (unsigned char)((reply >> PRIMA_VFS_REPLY_KIND_SHIFT) &
                        PRIMA_VFS_REPLY_KIND_MASK);
    if (kind != PRIMA_VFS_KIND_DIR && kind != PRIMA_VFS_KIND_FILE &&
        kind != PRIMA_VFS_KIND_NULL) {
        (void)prima_file_call(PRIMA_FILE_OP_CLOSE, handle, 0);
        return -PRIMA_ERRNO_EINVAL;
    }
    prima_file_refs[handle - PRIMA_FILE_FIRST_FD] = 1;
    prima_dir_reset((unsigned char)handle);
    int local = prima_fd_install(
        kind == PRIMA_VFS_KIND_DIR ? PRIMA_FD_DIR : PRIMA_FD_FILE,
        (unsigned char)handle, node, (unsigned char)((flags & PRIMA_O_CLOEXEC) != 0));
    if (local >= 0 && kind == PRIMA_VFS_KIND_DIR) {
        char absolute[PRIMA_VFS_PATH_MAX + 1];
        unsigned long absolute_len = 0;
        if (prima_absolute_path(dirfd, path, absolute, &absolute_len) == 0)
            prima_fd_path_set(local, absolute);
    }
    if (local >= 0 && (flags & PRIMA_O_CREAT) != 0) {
        // M13b：VFS 新建文件默认 0666；适配层按 umask 收紧（无 uid/gid 模型）。
        char absolute[PRIMA_VFS_PATH_MAX + 1];
        unsigned long absolute_len = 0;
        if (prima_absolute_path(dirfd, path, absolute, &absolute_len) == 0)
            (void)prima_vfs_chmod_request(0, absolute, absolute_len,
                                          (unsigned short)(mode & ~prima_umask_value));
    }
    if (local < 0) {
        (void)prima_file_call(PRIMA_FILE_OP_CLOSE, handle, 0);
        prima_file_refs[handle - PRIMA_FILE_FIRST_FD] = 0;
        return -PRIMA_ERRNO_EAGAIN;
    }
    return local;
}

/* M11：发送一次单路径变更（MUTATE + PATH + COMMIT），成功返回 0 或原始负 errno。 */
static long prima_vfs_mutate_request(unsigned char op, unsigned char start,
                                     const char *path, unsigned long len,
                                     uint64_t *reply) {
    uint64_t meta = PRIMA_STAGE5_VFS_MUTATE_TAG |
                    ((uint64_t)(op & 0xf) << PRIMA_VFS_MUTATE_OP_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT) |
                    (((uint64_t)start & 0xff) << PRIMA_VFS_START_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    long raw = prima_vfs_call(PRIMA_STAGE5_VFS_COMMIT_TAG |
                              (((uint64_t)id & 0xff)
                               << PRIMA_VFS_COMMIT_CLIENT_SHIFT));
    if (raw < 0)
        return raw;
    *reply = (uint64_t)raw;
    return 0;
}

/* M11：解析 dirfd + path，发变更请求并翻译服务端状态。 */
static long prima_mutate_path(long dirfd, const char *path, unsigned char op) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    const char *send = path;
    unsigned char start = 0;
    long target = prima_path_target(dirfd, path, &start, resolved, &send);
    if (target < 0)
        return target;
    unsigned long len = prima_strlen_bounded(send, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    if (len == 0)
        return -PRIMA_ERRNO_ENOENT;
    uint64_t reply = 0;
    long request = prima_vfs_mutate_request(op, start, send, len, &reply);
    if (request < 0)
        return request;
    long status = prima_reply_status(reply);
    return status == 0 ? 0 : -status;
}

static long prima_mkdirat(long dirfd, const char *path, long mode) {
    long status = prima_mutate_path(dirfd, path, PRIMA_VFS_OP_MKDIR);
    if (status != 0)
        return status;
    // M13b：VFS 新建目录默认 0777；适配层按 umask 收紧。
    return prima_chmod_path(dirfd, path, mode & ~prima_umask_value);
}

static long prima_unlinkat(long dirfd, const char *path, long flags) {
    if ((flags & ~(long)PRIMA_AT_REMOVEDIR) != 0)
        return -PRIMA_ERRNO_EINVAL;
    unsigned char op = (flags & PRIMA_AT_REMOVEDIR) != 0 ? PRIMA_VFS_OP_RMDIR
                                                        : PRIMA_VFS_OP_UNLINK;
    return prima_mutate_path(dirfd, path, op);
}

/* Linux x86_64 `struct statx`（256 字节）。x86_64 musl 的 stat 家族实际走下方
 * kstat 号 4/5/6/262，只有直接调用 `statx(332)` 的程序用本布局；适配层只填
 * profile 冻结的字段，其余清零。 */
struct prima_statx {
    uint32_t stx_mask;
    uint32_t stx_blksize;
    uint64_t stx_attributes;
    uint32_t stx_nlink;
    uint32_t stx_uid;
    uint32_t stx_gid;
    uint16_t stx_mode;
    uint16_t spare0;
    uint64_t stx_ino;
    uint64_t stx_size;
    uint64_t stx_blocks;
    uint64_t stx_attributes_mask;
    struct {
        int64_t tv_sec;
        uint32_t tv_nsec;
        int32_t pad;
    } stx_atime, stx_btime, stx_ctime, stx_mtime;
    uint32_t stx_rdev_major;
    uint32_t stx_rdev_minor;
    uint32_t stx_dev_major;
    uint32_t stx_dev_minor;
    uint64_t spare2[14];
};
_Static_assert(sizeof(struct prima_statx) == 256, "statx ABI size");

#define PRIMA_S_IFMT 0170000
#define PRIMA_S_IFDIR 0040000
#define PRIMA_S_IFREG 0100000
#define PRIMA_S_IFCHR 0020000
#define PRIMA_S_IFLNK 0120000
#define PRIMA_STATX_BASIC_STATS 0x7ff

static void prima_fill_statx(struct prima_statx *stx,
                             const struct prima_stat_meta *meta) {
    uint16_t ifmt;
    if (meta->kind == PRIMA_VFS_KIND_DIR)
        ifmt = PRIMA_S_IFDIR;
    else if (meta->kind == PRIMA_VFS_KIND_NULL)
        ifmt = PRIMA_S_IFCHR;
    else if (meta->kind == PRIMA_VFS_KIND_SYMLINK)
        ifmt = PRIMA_S_IFLNK;
    else
        ifmt = PRIMA_S_IFREG;
    uint16_t mode = (uint16_t)(ifmt | (meta->mode & 0777));
    *stx = (struct prima_statx){0};
    stx->stx_mask = PRIMA_STATX_BASIC_STATS;
    stx->stx_blksize = 4096;
    stx->stx_nlink = meta->kind == PRIMA_VFS_KIND_DIR ? 2 : 1;
    stx->stx_mode = mode;
    stx->stx_ino = (uint64_t)meta->node + 1;
    stx->stx_size = meta->size;
    stx->stx_blocks = ((uint64_t)meta->size + 511) / 512;
}

/* 取 stat 元数据：AT_EMPTY_PATH 按 fd（本地 tty/null 合成），否则解析路径。 */
static long prima_meta_for(long dirfd, const char *path, long flags,
                           struct prima_stat_meta *meta) {
    if ((flags & PRIMA_AT_EMPTY_PATH) != 0 && path != 0 && path[0] == 0) {
        struct prima_fd_entry *entry = prima_fd_entry((int)dirfd);
        if (!entry)
            return -PRIMA_ERRNO_EBADF;
        if (entry->kind == PRIMA_FD_TTY || entry->kind == PRIMA_FD_NULL) {
            meta->node = entry->node;
            meta->kind = PRIMA_VFS_KIND_NULL;
            meta->mode = 0666;
            meta->size = 0;
            return 0;
        }
        /* 节点自身：STAT 请求以空路径表示，不走 VFS 的相对路径解析（文件节点上的
         * 相对 "." 会被 POSIX/`walk` 判为 ENOTDIR）。 */
        return prima_stat_fetch(entry->node, "", meta);
    }
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    const char *send = path;
    unsigned char start = 0;
    long target = prima_path_target(dirfd, path, &start, resolved, &send);
    if (target < 0)
        return target;
    return prima_stat_fetch(start, send, meta);
}

static long prima_statx(long dirfd, const char *path, long flags, long mask,
                        void *buffer) {
    (void)mask;
    if (!buffer)
        return -PRIMA_ERRNO_EFAULT;
    long allowed = PRIMA_AT_SYMLINK_NOFOLLOW | PRIMA_AT_NO_AUTOMOUNT |
                   PRIMA_AT_EMPTY_PATH;
    if ((flags & ~allowed) != 0)
        return -PRIMA_ERRNO_EINVAL;
    struct prima_stat_meta meta;
    long fetched = prima_meta_for(dirfd, path, flags, &meta);
    if (fetched < 0)
        return fetched;
    prima_fill_statx((struct prima_statx *)buffer, &meta);
    return 0;
}

/* x86_64 musl 的 stat/lstat/fstat/fstatat 走 legacy kstat 系统调用（4/5/6/262），
 * 填充 144 字节的 `struct kstat`；statx(332) 只覆盖直接调用 statx 的程序。
 * 时间戳、dev/rdev 与 uid/gid 在本 profile 中恒 0。 */
struct prima_kstat {
    uint64_t st_dev;
    uint64_t st_ino;
    uint64_t st_nlink;
    uint32_t st_mode;
    uint32_t st_uid;
    uint32_t st_gid;
    uint32_t pad0;
    uint64_t st_rdev;
    int64_t st_size;
    int64_t st_blksize;
    int64_t st_blocks;
    int64_t st_atime_sec;
    int64_t st_atime_nsec;
    int64_t st_mtime_sec;
    int64_t st_mtime_nsec;
    int64_t st_ctime_sec;
    int64_t st_ctime_nsec;
    int64_t unused[3];
};
_Static_assert(sizeof(struct prima_kstat) == 144, "kstat ABI size");

static void prima_fill_kstat(struct prima_kstat *st,
                             const struct prima_stat_meta *meta) {
    *st = (struct prima_kstat){0};
    st->st_ino = (uint64_t)meta->node + 1;
    st->st_nlink = meta->kind == PRIMA_VFS_KIND_DIR ? 2 : 1;
    if (meta->kind == PRIMA_VFS_KIND_DIR)
        st->st_mode = (uint32_t)(PRIMA_S_IFDIR | (meta->mode & 0777));
    else if (meta->kind == PRIMA_VFS_KIND_NULL)
        st->st_mode = (uint32_t)(PRIMA_S_IFCHR | (meta->mode & 0777));
    else
        st->st_mode = (uint32_t)(PRIMA_S_IFREG | (meta->mode & 0777));
    st->st_size = meta->size;
    st->st_blksize = 4096;
    st->st_blocks = ((int64_t)meta->size + 511) / 512;
}

static long prima_stat_kstat(long dirfd, const char *path, long flags,
                             void *buffer) {
    if (!buffer)
        return -PRIMA_ERRNO_EFAULT;
    long allowed = PRIMA_AT_SYMLINK_NOFOLLOW | PRIMA_AT_NO_AUTOMOUNT |
                   PRIMA_AT_EMPTY_PATH;
    if ((flags & ~allowed) != 0)
        return -PRIMA_ERRNO_EINVAL;
    struct prima_stat_meta meta;
    long fetched = prima_meta_for(dirfd, path, flags, &meta);
    if (fetched < 0)
        return fetched;
    prima_fill_kstat((struct prima_kstat *)buffer, &meta);
    return 0;
}

/* M9：chdir 先用 stat 确认目标是目录；成功后保存**规范化绝对路径**。 */
static long prima_chdir(const char *path) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char normalized[PRIMA_VFS_PATH_MAX + 1];
    unsigned long len = 0;
    long normalized_status = prima_normalize_path(path, normalized, &len);
    if (normalized_status < 0)
        return normalized_status;
    struct prima_stat_meta meta;
    long fetched = prima_stat_fetch(0, normalized, &meta);
    if (fetched < 0)
        return fetched;
    if (meta.kind != PRIMA_VFS_KIND_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    for (unsigned long i = 0; i <= len; i++)
        prima_cwd[i] = normalized[i];
    return 0;
}

/* M9：返回包含结尾 NUL 的长度；缓冲不足用 ERANGE（profile v1 新冻结）。 */
static long prima_getcwd(char *buffer, unsigned long size) {
    if (!buffer)
        return -PRIMA_ERRNO_EFAULT;
    unsigned long len = prima_strlen_bounded(prima_cwd, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    if (size < len + 1)
        return -PRIMA_ERRNO_ERANGE;
    for (unsigned long i = 0; i <= len; i++)
        buffer[i] = prima_cwd[i];
    return (long)(len + 1);
}

/* M13a：fchdir 取目录 fd 打开时记录的目标路径；路径不可得时确定失败。 */
static long prima_fchdir(int fd) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    const char *path = prima_fd_paths[fd - PRIMA_LOCAL_FD_FIRST];
    unsigned long len = prima_strlen_bounded(path, PRIMA_VFS_PATH_MAX);
    if (len == 0 || len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    for (unsigned long i = 0; i <= len; i++)
        prima_cwd[i] = path[i];
    return 0;
}

/* M13a：rename/renameat。两条路径都先规范化为绝对路径，再按槽 0/1 分段发送。 */
static long prima_renameat(long olddirfd, const char *oldpath, long newdirfd,
                           const char *newpath) {
    if (!oldpath || !newpath)
        return -PRIMA_ERRNO_EFAULT;
    char from[PRIMA_VFS_PATH_MAX + 1];
    char to[PRIMA_VFS_PATH_MAX + 1];
    unsigned long from_len = 0;
    unsigned long to_len = 0;
    long status = prima_absolute_path(olddirfd, oldpath, from, &from_len);
    if (status < 0)
        return status;
    status = prima_absolute_path(newdirfd, newpath, to, &to_len);
    if (status < 0)
        return status;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_RENAME, 0, from, from_len);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_RENAME, 0, &reply);
    if (status < 0)
        return status;
    long result = prima_pair_status(reply);
    if (result != 0)
        return -result;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_RENAME, 1, to, to_len);
    if (status < 0)
        return status;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_RENAME, 1, &reply);
    if (status < 0)
        return status;
    result = prima_pair_status(reply);
    return result == 0 ? 0 : -result;
}

/* M13a：symlink/symlinkat。链接路径规范化；目标字符串按原样保存，不跟随/不合成。 */
static long prima_symlinkat(const char *target, long dirfd, const char *linkpath) {
    if (!target || !linkpath)
        return -PRIMA_ERRNO_EFAULT;
    unsigned long target_len = prima_strlen_bounded(target, PRIMA_VFS_PATH_MAX);
    if (target_len == 0 || target_len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    char link[PRIMA_VFS_PATH_MAX + 1];
    unsigned long link_len = 0;
    long status = prima_absolute_path(dirfd, linkpath, link, &link_len);
    if (status < 0)
        return status;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_SYMLINK, 0, link, link_len);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_SYMLINK, 0, &reply);
    if (status < 0)
        return status;
    long result = prima_pair_status(reply);
    if (result != 0)
        return -result;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_SYMLINK, 1, target, target_len);
    if (status < 0)
        return status;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_SYMLINK, 1, &reply);
    if (status < 0)
        return status;
    result = prima_pair_status(reply);
    return result == 0 ? 0 : -result;
}

/* M13a：readlink/readlinkat。先取目标长度，再按 3 字节分段取回；缓冲不足按 Linux
 * 语义截断并返回实际拷贝数（不补 NUL）。 */
static long prima_readlinkat(long dirfd, const char *path, char *buffer,
                             unsigned long size) {
    if (!path || !buffer)
        return -PRIMA_ERRNO_EFAULT;
    if (size == 0)
        return -PRIMA_ERRNO_EINVAL;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    unsigned long len = 0;
    long status = prima_absolute_path(dirfd, path, resolved, &len);
    if (status < 0)
        return status;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_READLINK, 0, resolved, len);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_READLINK, 0, &reply);
    if (status < 0)
        return status;
    long result = prima_pair_status(reply);
    if (result != 0)
        return -result;
    unsigned long target_len = (unsigned long)(reply & PRIMA_FILE_ARG_MASK);
    if (target_len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    unsigned long copied = 0;
    while (copied < target_len) {
        long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
        uint64_t request = PRIMA_STAGE5_VFS_LINK_READ_TAG |
                           (((uint64_t)copied & 0xff)
                            << PRIMA_VFS_LINK_OFFSET_SHIFT) |
                           (((uint64_t)id & 0xff)
                            << PRIMA_VFS_LINK_CLIENT_SHIFT);
        long sent = prima_vfs_send(request);
        if (sent < 0)
            return sent;
        uint64_t caps[3];
        uint64_t chunk = 0;
        for (;;) {
            long raw = prima_raw(PRIMA_SYS_RECV, (long)caps, 0, 0, 0, 0, 0);
            if (raw < 0)
                return raw;
            if (((uint64_t)raw >> 48) == (PRIMA_STAGE5_VFS_LINK_REPLY_TAG >> 48)) {
                chunk = (uint64_t)raw;
                break;
            }
        }
        long chunk_status =
            (long)((chunk >> PRIMA_VFS_LINK_OFFSET_SHIFT) & 0xff);
        if (chunk_status != 0)
            return -chunk_status;
        unsigned long take = target_len - copied;
        if (take > PRIMA_VFS_LINK_CHUNK)
            take = PRIMA_VFS_LINK_CHUNK;
        for (unsigned long i = 0; i < take && copied < size; i++)
            buffer[copied++] = (char)(unsigned char)((chunk >> (8 * i)) & 0xff);
        if (copied >= size)
            break;
    }
    return (long)copied;
}

/* M13b：chmod 的单路径请求；COMMIT 的 bits 8..16 携带 mode。 */
static long prima_vfs_chmod_request(unsigned char start, const char *path,
                                    unsigned long len, unsigned short mode) {
    uint64_t meta = PRIMA_STAGE5_VFS_CHMOD_TAG |
                    (((uint64_t)start & 0xff) << PRIMA_VFS_START_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t commit = PRIMA_STAGE5_VFS_COMMIT_TAG |
                      (((uint64_t)id & 0xff) << PRIMA_VFS_COMMIT_CLIENT_SHIFT) |
                      (((uint64_t)mode & PRIMA_VFS_CHMOD_MODE_MASK)
                       << PRIMA_VFS_CHMOD_MODE_SHIFT);
    long raw = prima_vfs_call(commit);
    if (raw < 0)
        return raw;
    long status = prima_reply_status((uint64_t)raw);
    return status == 0 ? 0 : -status;
}

/* chmod/chmodat：路径规范化后交给服务端；跟随符号链接。 */
static long prima_chmod_path(long dirfd, const char *path, long mode) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    if (mode < 0 || (mode & ~(long)0777) != 0)
        return -PRIMA_ERRNO_EINVAL;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    unsigned long len = 0;
    long status = prima_absolute_path(dirfd, path, resolved, &len);
    if (status < 0)
        return status;
    return prima_vfs_chmod_request(0, resolved, len, (unsigned short)mode);
}

static long prima_fchmod(int fd, long mode) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    // tty/null 没有 VFS 节点；node 字段为 0 时不能落到根节点。
    if (entry->kind != PRIMA_FD_FILE && entry->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_EBADF;
    if (mode < 0 || (mode & ~(long)0777) != 0)
        return -PRIMA_ERRNO_EINVAL;
    return prima_vfs_chmod_request(entry->node, "", 0, (unsigned short)mode);
}

static long prima_kernel_umask(long mask) {
    long previous = prima_umask_value;
    prima_umask_value = mask & 0777;
    return previous;
}

/* M19：同步网络请求（OPEN/BIND/SENDTO 系列/RECVFROM/RECV_CHUNK/GETSOCKNAME/CLOSE）。 */
static long prima_net_request(unsigned char op, unsigned char handle, uint64_t arg,
                              uint64_t *reply) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload = PRIMA_STAGE5_NET_REQUEST_TAG |
                       ((uint64_t)(op & 0xf) << PRIMA_NET_OP_SHIFT) |
                       ((uint64_t)(handle & 0xf) << PRIMA_NET_HANDLE_SHIFT) |
                       (((uint64_t)id & 0xff) << PRIMA_NET_CLIENT_SHIFT) |
                       (arg & PRIMA_NET_ARG_MASK);
    long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                          (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    if (sent != 0)
        return sent < 0 ? sent : -PRIMA_ERRNO_EFAULT;
    uint64_t caps[3];
    for (;;) {
        long raw = prima_raw(PRIMA_SYS_RECV, (long)caps, 0, 0, 0, 0, 0);
        if (raw < 0)
            return raw;
        uint64_t value = (uint64_t)raw;
        if ((value >> 48) != (PRIMA_STAGE5_NET_REPLY_TAG >> 48))
            continue;
        // 应答必须匹配本次 op（OPEN 的 handle 是新句柄，其余按原 handle）。
        if (((value >> PRIMA_NET_OP_SHIFT) & 0xf) != (uint64_t)op)
            continue;
        if (op != PRIMA_NET_OP_OPEN &&
            ((value >> PRIMA_NET_HANDLE_SHIFT) & 0xf) != (uint64_t)handle)
            continue;
        *reply = value;
        return 0;
    }
}

static long prima_net_status(uint64_t reply) {
    return (long)((reply >> PRIMA_NET_STATUS_SHIFT) & 0xff);
}

/* sockaddr_in 只接受 AF_INET + 127.0.0.1/0.0.0.0。返回主机序端口。 */
static long prima_sockaddr_port(const void *addr, int len, int allow_zero,
                                unsigned short *port) {
    if (!addr || len < 16)
        return -PRIMA_ERRNO_EINVAL;
    const unsigned char *bytes = addr;
    unsigned short family = (unsigned short)(bytes[0] | (bytes[1] << 8));
    if (family != PRIMA_AF_INET)
        return -PRIMA_ERRNO_AFNOSUPPORT;
    unsigned int host = (unsigned int)bytes[4] | ((unsigned int)bytes[5] << 8) |
                        ((unsigned int)bytes[6] << 16) |
                        ((unsigned int)bytes[7] << 24);
    if (host != 0x0100007Fu && host != 0)
        return -PRIMA_ERRNO_AFNOSUPPORT;
    unsigned short value = (unsigned short)((bytes[2] << 8) | bytes[3]);
    if (value == 0 && !allow_zero)
        return -PRIMA_ERRNO_EINVAL;
    *port = value;
    return 0;
}

static void prima_sockaddr_fill(void *addr, int *len, unsigned short port) {
    if (!addr || !len || *len < 16) {
        if (len)
            *len = 0;
        return;
    }
    unsigned char *bytes = addr;
    for (int i = 0; i < 16; i++)
        bytes[i] = 0;
    bytes[0] = PRIMA_AF_INET;
    bytes[1] = 0;
    bytes[2] = (unsigned char)(port >> 8);
    bytes[3] = (unsigned char)(port & 0xff);
    bytes[4] = 127;
    bytes[7] = 1;
    *len = 16;
}

static long prima_net_socket(long domain, long type, long protocol) {
    if (domain != PRIMA_AF_INET)
        return -PRIMA_ERRNO_AFNOSUPPORT;
    if (type != PRIMA_SOCK_DGRAM || protocol != 0)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t reply = 0;
    long status = prima_net_request(PRIMA_NET_OP_OPEN, 0, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    unsigned char handle = (unsigned char)(reply & 0xff);
    if (handle >= PRIMA_NET_SOCKETS)
        return -PRIMA_ERRNO_EINVAL;
    int local = prima_fd_install(PRIMA_FD_SOCKET, handle, 0, 0);
    if (local < 0) {
        uint64_t drop_reply = 0;
        (void)prima_net_request(PRIMA_NET_OP_CLOSE, handle, 0, &drop_reply);
        return -PRIMA_ERRNO_EAGAIN;
    }
    prima_net_refs[handle] = 1;
    return local;
}

static struct prima_fd_entry *prima_socket_entry(int fd) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return 0;
    if (entry->kind != PRIMA_FD_SOCKET)
        return 0;
    return entry;
}

static long prima_net_bind(int fd, const void *addr, int len) {
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    unsigned short port = 0;
    long status = prima_sockaddr_port(addr, len, 1, &port);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_net_request(PRIMA_NET_OP_BIND, entry->object, port, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    return status == 0 ? 0 : -status;
}

static long prima_net_getsockname(int fd, void *addr, int *len) {
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    uint64_t reply = 0;
    long status = prima_net_request(PRIMA_NET_OP_GETSOCKNAME, entry->object, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    prima_sockaddr_fill(addr, len, (unsigned short)(reply & 0xffff));
    return 0;
}

static long prima_net_sendto(int fd, const void *buf, unsigned long len, int flags,
                             const void *addr, int addrlen) {
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (flags != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (len > PRIMA_NET_DATAGRAM_MAX)
        return -PRIMA_ERRNO_MSGSIZE;
    if (!buf && len != 0)
        return -PRIMA_ERRNO_EFAULT;
    unsigned short port = 0;
    long status = prima_sockaddr_port(addr, addrlen, 0, &port);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    uint64_t arg = (uint64_t)port | ((uint64_t)(len & 0xff) << PRIMA_NET_LEN_SHIFT);
    status = prima_net_request(PRIMA_NET_OP_SENDTO_BEGIN, entry->object, arg, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    const unsigned char *bytes = buf;
    for (unsigned long offset = 0; offset < len; offset += PRIMA_NET_CHUNK) {
        uint64_t chunk = 0;
        for (unsigned long i = 0; i < PRIMA_NET_CHUNK && offset + i < len; i++)
            chunk |= (uint64_t)bytes[offset + i] << (8 * i);
        status = prima_net_request(PRIMA_NET_OP_SENDTO_DATA, entry->object, chunk, &reply);
        if (status < 0)
            return status;
        status = prima_net_status(reply);
        if (status != 0)
            return -status;
    }
    status = prima_net_request(PRIMA_NET_OP_SENDTO_END, entry->object, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    return status == 0 ? (long)len : -status;
}

static long prima_net_recvfrom(int fd, void *buf, unsigned long len, int flags,
                               void *addr, int *addrlen) {
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (flags != 0)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t reply = 0;
    long status = prima_net_request(PRIMA_NET_OP_RECVFROM, entry->object, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    unsigned short source = (unsigned short)(reply & 0xffff);
    unsigned long total = (reply >> PRIMA_NET_LEN_SHIFT) & 0xff;
    unsigned long copy = total < len ? total : len;
    unsigned char *bytes = buf;
    for (unsigned long offset = 0; offset < copy; offset += PRIMA_NET_CHUNK) {
        status = prima_net_request(PRIMA_NET_OP_RECV_CHUNK, entry->object, offset, &reply);
        if (status < 0)
            return status;
        status = prima_net_status(reply);
        if (status != 0)
            return -status;
        uint64_t data = reply & PRIMA_NET_ARG_MASK;
        for (unsigned long i = 0; i < PRIMA_NET_CHUNK && offset + i < copy; i++)
            bytes[offset + i] = (unsigned char)((data >> (8 * i)) & 0xff);
    }
    prima_sockaddr_fill(addr, addrlen, source);
    return (long)copy;
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

/* ---------- M8：目录字节流 → linux_dirent64 ---------- */

static long prima_dir_read(unsigned char handle, unsigned char *out,
                           unsigned long want) {
    if (want > PRIMA_FILE_CHUNK)
        want = PRIMA_FILE_CHUNK;
    long reply = prima_file_call(PRIMA_FILE_OP_READ, handle, want);
    if (reply < 0)
        return reply;
    long status = prima_reply_status((uint64_t)reply);
    if (status != 0)
        return -status;
    unsigned long count = ((uint64_t)reply >> PRIMA_FILE_WRITE_COUNT_SHIFT) & 0xff;
    uint64_t data = (uint64_t)reply & PRIMA_FILE_ARG_MASK;
    if (count > want)
        count = want;
    for (unsigned long i = 0; i < count; i++)
        out[i] = (unsigned char)(data >> (8 * i));
    return (long)count;
}

/* 返回 >0 成功、0 EOF、<0 原始负 errno。 */
static long prima_dir_fill(unsigned char handle, unsigned need) {
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    if (cache->pos > 0) {
        unsigned i = 0;
        while ((unsigned)cache->pos + i < cache->len) {
            cache->buf[i] = cache->buf[cache->pos + i];
            i++;
        }
        cache->len = (unsigned char)i;
        cache->pos = 0;
    }
    while (cache->len < need) {
        unsigned room = PRIMA_DIR_CACHE_BYTES - cache->len;
        if (room > PRIMA_FILE_CHUNK)
            room = PRIMA_FILE_CHUNK;
        long got = prima_dir_read(handle, cache->buf + cache->len, room);
        if (got < 0)
            return got;
        if (got == 0)
            return cache->len == 0 ? 0
                                   : (cache->len >= need ? 1
                                                         : -PRIMA_ERRNO_EINVAL);
        cache->len = (unsigned char)(cache->len + (unsigned char)got);
    }
    return 1;
}

/* 取下一条完整记录；返回 1 成功、0 EOF、负 errno。 */
static long prima_dir_take(unsigned char handle, struct prima_dir_record *out) {
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    long filled = prima_dir_fill(handle, 4);
    if (filled <= 0)
        return filled;
    unsigned name_len = cache->buf[3];
    if (name_len > PRIMA_DIR_CACHE_BYTES - 4)
        return -PRIMA_ERRNO_EINVAL;
    filled = prima_dir_fill(handle, 4 + name_len);
    if (filled <= 0)
        return filled < 0 ? filled : -PRIMA_ERRNO_EINVAL;
    out->node = (unsigned short)(cache->buf[0] | ((unsigned short)cache->buf[1] << 8));
    out->kind = cache->buf[2];
    out->name_len = (unsigned char)name_len;
    for (unsigned i = 0; i < name_len; i++)
        out->name[i] = cache->buf[4 + i];
    cache->pos = (unsigned char)(4 + name_len);
    return 1;
}

static void prima_dir_unread(unsigned char handle, unsigned char total) {
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    cache->pos = (unsigned char)(cache->pos - total);
}

static long prima_getdents64(int fd, void *dirp, unsigned long count) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    unsigned char *out = dirp;
    if (!out && count != 0)
        return -PRIMA_ERRNO_EFAULT;
    unsigned char handle = entry->object;
    unsigned long written = 0;
    for (;;) {
        struct prima_dir_record record;
        long taken = prima_dir_take(handle, &record);
        if (taken < 0)
            return written != 0 ? (long)written : taken;
        if (taken == 0)
            break;
        unsigned long name_len = record.name_len;
        unsigned long reclen = (19 + name_len + 1 + 7) & ~7UL;
        if (written + reclen > count) {
            prima_dir_unread(handle, (unsigned char)(4 + name_len));
            if (written == 0)
                return -PRIMA_ERRNO_EINVAL;
            break;
        }
        struct prima_dirent64 *dirent = (struct prima_dirent64 *)(out + written);
        dirent->d_ino = (uint64_t)record.node + 1;
        dirent->d_off = (int64_t)(written + reclen);
        dirent->d_reclen = (unsigned short)reclen;
        dirent->d_type = record.kind == PRIMA_VFS_KIND_DIR
                             ? 4
                             : (record.kind == PRIMA_VFS_KIND_FILE
                                    ? 8
                                    : (record.kind == PRIMA_VFS_KIND_SYMLINK ? 10
                                                                             : 2));
        for (unsigned long i = 0; i < name_len; i++)
            dirent->d_name[i] = (char)record.name[i];
        dirent->d_name[name_len] = 0;
        for (unsigned long i = 19 + name_len + 1; i < reclen; i++)
            ((unsigned char *)dirent)[i] = 0;
        written += reclen;
    }
    return (long)written;
}

/* ---------- M8：按 fd 类型分派 ---------- */

static long prima_fd_read(int fd, void *buf, unsigned long len) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind == PRIMA_FD_SOCKET)
        return -PRIMA_ERRNO_NOTSOCK;
    if (entry->kind == PRIMA_FD_NULL) {
        if (!buf && len != 0)
            return -PRIMA_ERRNO_EFAULT;
        return 0;
    }
    if (entry->kind != PRIMA_FD_FILE)
        return -PRIMA_ERRNO_EBADF;
    return prima_handle_read(entry->object, buf, len);
}

static long prima_fd_write(int fd, const void *buf, unsigned long len) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind == PRIMA_FD_NULL) {
        if (!buf && len != 0)
            return -PRIMA_ERRNO_EFAULT;
        return (long)len;
    }
    if (entry->kind == PRIMA_FD_SOCKET)
        return -PRIMA_ERRNO_NOTSOCK;
    if (entry->kind == PRIMA_FD_TTY)
        return prima_write_bytes(entry->object, buf, len);
    if (entry->kind == PRIMA_FD_FILE)
        return prima_handle_write(entry->object, buf, len);
    return -PRIMA_ERRNO_EBADF;
}

static long prima_fd_lseek(int fd, long offset, int whence) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind == PRIMA_FD_NULL)
        return 0;
    if (entry->kind == PRIMA_FD_DIR) {
        prima_dir_reset(entry->object);
        return prima_handle_lseek(entry->object, offset, whence);
    }
    if (entry->kind != PRIMA_FD_FILE)
        return -PRIMA_ERRNO_EBADF;
    return prima_handle_lseek(entry->object, offset, whence);
}

static long prima_fd_close(int fd) {
    if (!prima_fd_entry(fd))
        return -PRIMA_ERRNO_EBADF;
    return prima_fd_drop(fd);
}

static long prima_fd_dup(int oldfd) {
    struct prima_fd_entry *entry = prima_fd_entry(oldfd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    int newfd =
        prima_fd_install(entry->kind, entry->object, entry->node, entry->cloexec);
    if (newfd < 0)
        return -PRIMA_ERRNO_EAGAIN;
    if (entry->kind == PRIMA_FD_FILE || entry->kind == PRIMA_FD_DIR)
        prima_file_refs[entry->object - PRIMA_FILE_FIRST_FD]++;
    else if (entry->kind == PRIMA_FD_SOCKET && entry->object < PRIMA_NET_SOCKETS)
        prima_net_refs[entry->object]++;
    prima_fd_path_copy(newfd, oldfd);
    return newfd;
}

/* dup2/dup3 共用：oldfd 已校验；newfd 必须落在本地表内。先为新引用加计数，
 * 再关闭目标项——即使两者指向同一服务端句柄，也不会在中途误发 CLOSE。 */
static long prima_fd_dup_to(int oldfd, int newfd) {
    struct prima_fd_entry *source = prima_fd_entry(oldfd);
    unsigned char kind = source->kind;
    unsigned char object = source->object;
    unsigned char node = source->node;
    unsigned char cloexec = source->cloexec;
    if (kind == PRIMA_FD_FILE || kind == PRIMA_FD_DIR)
        prima_file_refs[object - PRIMA_FILE_FIRST_FD]++;
    else if (kind == PRIMA_FD_SOCKET && object < PRIMA_NET_SOCKETS)
        prima_net_refs[object]++;
    if (prima_fd_entry(newfd))
        (void)prima_fd_drop(newfd);
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].kind = kind;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].object = object;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].node = node;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].cloexec = cloexec;
    if (kind == PRIMA_FD_DIR)
        prima_fd_path_copy(newfd, oldfd);
    else
        prima_fd_path_clear(newfd);
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
    long result = prima_fd_dup_to(oldfd, newfd);
    if (result >= 0 && (flags & PRIMA_O_CLOEXEC) != 0)
        prima_fds[result - PRIMA_LOCAL_FD_FIRST].cloexec = 1;
    return result;
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

/* M12b：退出前释放本地 fd 与服务端句柄。fd 1/2 只清表项；文件/目录的最后一个引用
 * 发同步 CLOSE，保证存储服务的 8 个句柄不随进程退出泄漏。 */
static void prima_close_all_fds(void) {
    for (int fd = PRIMA_LOCAL_FD_FIRST; fd <= PRIMA_LOCAL_FD_LAST; fd++) {
        if (prima_fd_entry(fd))
            (void)prima_fd_drop(fd);
    }
}

__attribute__((noreturn)) static void prima_exit_group(int status) {
    prima_close_all_fds();
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
        /* Linux 顺序：addr, len, prot, flags, fd, offset。 */
        return prima_mmap_anon(a0, (uint64_t)a1, a2, a3, a4, a5);
    case PRIMA_LINUX_SYS_munmap:
        return prima_munmap_anon((uint64_t)a0, (uint64_t)a1);
    case PRIMA_LINUX_SYS_mprotect:
        return prima_mprotect_anon((uint64_t)a0, (uint64_t)a1, a2);
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
    case PRIMA_LINUX_SYS_readv: {
        // M13b：musl 的 stdio 读走 readv；按 iovec 顺序调用同一 fd 读路径。
        const struct iovec *iov = (const struct iovec *)a1;
        long count = a2;
        if (count <= 0)
            return -PRIMA_ERRNO_EINVAL;
        if (!iov)
            return -PRIMA_ERRNO_EFAULT;
        unsigned long total = 0;
        for (long i = 0; i < count; i++) {
            long got = prima_fd_read((int)a0, iov[i].iov_base, iov[i].iov_len);
            if (got < 0)
                return total > 0 ? (long)total : got;
            total += (unsigned long)got;
            if ((unsigned long)got < iov[i].iov_len)
                break;
        }
        return (long)total;
    }
    case PRIMA_LINUX_SYS_close:
        return prima_fd_close((int)a0);
    case PRIMA_LINUX_SYS_read:
        return prima_fd_read((int)a0, (void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_socket:
        return prima_net_socket(a0, a1, a2);
    case PRIMA_LINUX_SYS_bind:
        return prima_net_bind((int)a0, (const void *)a1, (int)a2);
    case PRIMA_LINUX_SYS_getsockname:
        return prima_net_getsockname((int)a0, (void *)a1, (int *)a2);
    case PRIMA_LINUX_SYS_sendto:
        return prima_net_sendto((int)a0, (const void *)a1, (unsigned long)a2,
                                (int)a3, (const void *)a4, (int)a5);
    case PRIMA_LINUX_SYS_recvfrom:
        return prima_net_recvfrom((int)a0, (void *)a1, (unsigned long)a2,
                                  (int)a3, (void *)a4, (int *)a5);
    case PRIMA_LINUX_SYS_getdents64:
        return prima_getdents64((int)a0, (void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_lseek:
        return prima_fd_lseek((int)a0, (long)a1, (int)a2);
    case PRIMA_LINUX_SYS_mkdir:
        return prima_mkdirat(PRIMA_AT_FDCWD, (const char *)a0, a1);
    case PRIMA_LINUX_SYS_mkdirat:
        return prima_mkdirat(a0, (const char *)a1, a2);
    case PRIMA_LINUX_SYS_rmdir:
        return prima_unlinkat(PRIMA_AT_FDCWD, (const char *)a0,
                              PRIMA_AT_REMOVEDIR);
    case PRIMA_LINUX_SYS_unlink:
        return prima_unlinkat(PRIMA_AT_FDCWD, (const char *)a0, 0);
    case PRIMA_LINUX_SYS_unlinkat:
        return prima_unlinkat(a0, (const char *)a1, a2);
    case PRIMA_LINUX_SYS_dup:
        return prima_fd_dup((int)a0);
    case PRIMA_LINUX_SYS_dup2:
        return prima_fd_dup2((int)a0, (int)a1);
    case PRIMA_LINUX_SYS_dup3:
        return prima_fd_dup3((int)a0, (int)a1, a2);
    case PRIMA_LINUX_SYS_getppid:
        return (long)prima_ctx.manager_id;
    case PRIMA_LINUX_SYS_getpid:
        return prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    /* musl/x86_64 的 `open()` 走 SYS_open(2)，不是 openat(257)；两者落到同一子集。 */
    case PRIMA_LINUX_SYS_open:
        return prima_openat(PRIMA_AT_FDCWD, (const char *)a0, a1, a2);
    case PRIMA_LINUX_SYS_openat:
        return prima_openat(a0, (const char *)a1, a2, a3);
    /* M9：x86_64 musl 的 stat/lstat/fstat/fstatat 走 legacy kstat 号 4/5/6/262；
     * 直接调用 statx 的程序走 332。cwd 相关调用走 chdir(80)/getcwd(79)。 */
    case PRIMA_LINUX_SYS_stat:
        return prima_stat_kstat(PRIMA_AT_FDCWD, (const char *)a0, 0,
                                (void *)a1);
    case PRIMA_LINUX_SYS_lstat:
        return prima_stat_kstat(PRIMA_AT_FDCWD, (const char *)a0,
                                PRIMA_AT_SYMLINK_NOFOLLOW, (void *)a1);
    case PRIMA_LINUX_SYS_fstat:
        return prima_stat_kstat((int)a0, "", PRIMA_AT_EMPTY_PATH, (void *)a1);
    case PRIMA_LINUX_SYS_newfstatat:
        return prima_stat_kstat((int)a0, (const char *)a1, a3, (void *)a2);
    case PRIMA_LINUX_SYS_statx:
        return prima_statx((int)a0, (const char *)a1, a2, a3, (void *)a4);
    case PRIMA_LINUX_SYS_chdir:
        return prima_chdir((const char *)a0);
    case PRIMA_LINUX_SYS_fchdir:
        return prima_fchdir((int)a0);
    case PRIMA_LINUX_SYS_chmod:
        return prima_chmod_path(PRIMA_AT_FDCWD, (const char *)a0, a1);
    case PRIMA_LINUX_SYS_fchmod:
        return prima_fchmod((int)a0, a1);
    case PRIMA_LINUX_SYS_fchmodat:
        if (a3 != 0)
            return -PRIMA_ERRNO_EINVAL;
        return prima_chmod_path((long)a0, (const char *)a1, a2);
    case PRIMA_LINUX_SYS_umask:
        return prima_kernel_umask(a0);
    case PRIMA_LINUX_SYS_getcwd:
        return prima_getcwd((char *)a0, (unsigned long)a1);
    case PRIMA_LINUX_SYS_rename:
        return prima_renameat(PRIMA_AT_FDCWD, (const char *)a0,
                              PRIMA_AT_FDCWD, (const char *)a1);
    case PRIMA_LINUX_SYS_renameat:
        return prima_renameat((long)a0, (const char *)a1, (long)a2,
                              (const char *)a3);
    case PRIMA_LINUX_SYS_symlink:
        return prima_symlinkat((const char *)a0, PRIMA_AT_FDCWD,
                               (const char *)a1);
    case PRIMA_LINUX_SYS_symlinkat:
        return prima_symlinkat((const char *)a0, (long)a1,
                               (const char *)a2);
    case PRIMA_LINUX_SYS_readlink:
        return prima_readlinkat(PRIMA_AT_FDCWD, (const char *)a0,
                                (char *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_readlinkat:
        return prima_readlinkat((long)a0, (const char *)a1, (char *)a2,
                                (unsigned long)a3);
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
