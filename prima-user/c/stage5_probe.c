/* Freestanding C task: the assembler entry passes the initial stack in r9.
 *
 * 片 C / M2：同一份 `main` 在 Linux 宿主与 Prima QEMU 上逐字节同源。
 * - 两端都用**原始 Linux 调用号**走系统边界：Linux 用宿主 `syscall()`，Prima 用
 *   `prima_syscall.c` 的 `__syscallN` 翻译成原生号，失败保持原始负 errno。
 * - Prima 的 fd 1/2 由适配层逐字节发往服务管理器，再转发给 tty 服务写串口；
 *   `_exit` 先把状态发给管理器再自毁。
 * - `main` 在两端收到相同的 argv[1]="stack-ready" / PRIMA_ABI=1。
 *
 * 探针的代码段按 W^X 以只读映射，故**不使用可写全局变量**做控制流；`bss_probe` 与
 * `fs_probe` 只验证可写段/BSS 清零和 `%fs` 基址。Prima 侧的凭据由 `stage5_start.S`
 * 写入 `.bss` 的 `prima_ctx`。
 */
#include "prima_abi.h"

#ifdef PRIMA_HOST
/* 宿主参考：只声明所需 glibc 符号，避免依赖宿主头文件（Lean clang 资源目录缺
 * stddef.h）。struct iovec 由 prima_abi.h 提供，与 x86_64 SysV 布局一致。 */
extern long syscall(long number, ...);
extern int *__errno_location(void) __attribute__((const));

/* glibc 的 `syscall()` 在失败时返回 -1 并设置 errno；转回原始负值，才能与 Prima
 * 适配层做同源断言。musl 原生就返回负 errno，不受此影响。 */
static long probe_errno_result(long result) {
    return result == -1 ? -(long)*__errno_location() : result;
}

static long probe_raw1(long number, long a0) {
    return probe_errno_result(syscall(number, a0));
}

static long probe_raw3(long number, long a0, long a1, long a2) {
    return probe_errno_result(syscall(number, a0, a1, a2));
}

static long probe_raw4(long number, long a0, long a1, long a2, long a3) {
    return probe_errno_result(syscall(number, a0, a1, a2, a3));
}

static long probe_raw5(long number, long a0, long a1, long a2, long a3,
                       long a4) {
    return probe_errno_result(syscall(number, a0, a1, a2, a3, a4));
}

static long probe_raw6(long number, long a0, long a1, long a2, long a3,
                       long a4, long a5) {
    return probe_errno_result(syscall(number, a0, a1, a2, a3, a4, a5));
}
#else
#include "prima_syscall.h"

static long probe_raw1(long number, long a0) {
    return __syscall1(number, a0);
}

static long probe_raw3(long number, long a0, long a1, long a2) {
    return __syscall3(number, a0, a1, a2);
}

static long probe_raw4(long number, long a0, long a1, long a2, long a3) {
    return __syscall4(number, a0, a1, a2, a3);
}

static long probe_raw5(long number, long a0, long a1, long a2, long a3,
                       long a4) {
    return __syscall5(number, a0, a1, a2, a3, a4);
}

static long probe_raw6(long number, long a0, long a1, long a2, long a3,
                       long a4, long a5) {
    return __syscall6(number, a0, a1, a2, a3, a4, a5);
}
#endif

/* 放置于可写 PT_LOAD/.bss：装载器必须为它建立私有可写页，且 BSS 必须清零。 */
static volatile uint64_t bss_probe;
#if !defined(PRIMA_HOST) && !defined(PRIMA_WITH_LIBC)
/* M2/T-MUSL-02：FS base 指向本变量；yield 之后必须仍能读到同一值。
 * musl 构建跳过：不能覆盖 musl 的 TLS 基址；M3 的 TLS 由 musl 启动本身见证。 */
static volatile uint64_t fs_probe;
#endif
static int tid_sink;

#ifndef PRIMA_HOST
/* M3：不走 stdio 输出的 libc 断言——证明 musl 的格式化、环境变量与 errno 语义
 * 都经适配层工作，同时不改变既有 h/i/xyz 串口字节流。 */
extern int snprintf(char *buffer, unsigned long size, const char *format, ...);
extern char *getenv(const char *name);
extern int *__errno_location(void);
extern long write(int fd, const void *buffer, unsigned long count);
extern void *malloc(unsigned long size);
extern void free(void *pointer);
extern void *realloc(void *pointer, unsigned long size);

/* M3b：mallocng 的匿名内存。依次覆盖 brk 元数据区、小对象的 mmap 分组、
 * >= MMAP_THRESHOLD（131052）的独立 mmap + free 的 munmap，以及 realloc 的
 * 复制回退（mremap 明确返回 ENOSYS）。返回 0 表示通过；正数是失败阶段
 * （映射到退出码，便于 QEMU 定位）。 */
static int musl_heap_probe(void) {
    unsigned char *small = malloc(64);
    if (!small || ((uintptr_t)small & 15) != 0)
        return 1;
    for (int i = 0; i < 64; i++)
        small[i] = (unsigned char)(i + 1);
    for (int i = 0; i < 64; i++)
        if (small[i] != (unsigned char)(i + 1)) {
            free(small);
            return 2;
        }
    free(small);

    unsigned char *page = malloc(4096);
    if (!page)
        return 3;
    page[0] = 0x5a;
    page[4095] = 0xa5;
    if (page[0] != 0x5a || page[4095] != 0xa5) {
        free(page);
        return 4;
    }
    free(page);

    const unsigned long big_len = 140000;
    unsigned char *big = malloc(big_len);
    if (!big)
        return 5;
    big[0] = 1;
    big[big_len - 1] = 2;
    if (big[0] != 1 || big[big_len - 1] != 2) {
        free(big);
        return 6;
    }
    free(big);

    unsigned char *grown = malloc(32);
    if (!grown)
        return 7;
    grown[0] = 7;
    unsigned char *grown_more = realloc(grown, 128);
    if (!grown_more)
        return 8;
    if (grown_more[0] != 7) {
        free(grown_more);
        return 9;
    }
    free(grown_more);
    return 0;
}

static int musl_runtime_probe(void) {
    char buffer[64];
    int written = snprintf(buffer, sizeof buffer, "%d-%s-%x", 42, "musl", 0xabc);
    if (written != 11)
        return 11;
    const char expect[] = "42-musl-abc";
    for (int i = 0; i < written; i++)
        if (buffer[i] != expect[i])
            return 11;
    if (buffer[written] != 0)
        return 11;
    char *value = getenv("PRIMA_ABI");
    if (!value || value[0] != '1' || value[1] != 0)
        return 11;
    *__errno_location() = 0;
    if (write(99, "x", 1) != -1 || *__errno_location() != PRIMA_ERRNO_EBADF)
        return 11;
    {
        int heap = musl_heap_probe();
        return heap ? 30 + heap : 0;
    }
}
#endif

/* M4：stdio 输出路径（M3 遗留）。`printf` 走 musl/glibc 的缓冲与 `writev`，
 * `fflush` 强制下发；两端都执行，串口/stdout 出现同一行。 */
extern int printf(const char *format, ...);
extern int fflush(void *stream);
extern void *stdout;

/* M4：文件 API 的 Linux/Prima 同源差分。Prima 侧路径 "f" 由 Namespace 授权入口解析；
 * Linux 侧 xtask 把 cwd 指向一次性目录。返回 0 通过，正数为失败阶段。 */
static int same_bytes(const void *left, const void *right, unsigned long count) {
    const unsigned char *a = left;
    const unsigned char *b = right;
    for (unsigned long i = 0; i < count; i++)
        if (a[i] != b[i])
            return 0;
    return 1;
}

static int file_api_probe(void) {
    char buffer[8];
    long fd = probe_raw4(PRIMA_LINUX_SYS_openat, PRIMA_AT_FDCWD, (long)"f",
                         PRIMA_O_RDWR | PRIMA_O_CREAT | PRIMA_O_TRUNC, 0600);
    if (fd < 3)
        return 1;
    if (probe_raw3(PRIMA_LINUX_SYS_write, fd, (long)"abcdef", 6) != 6)
        return 2;
    if (probe_raw3(PRIMA_LINUX_SYS_lseek, fd, 0, PRIMA_SEEK_SET) != 0)
        return 3;
    if (probe_raw3(PRIMA_LINUX_SYS_read, fd, (long)buffer, 4) != 4)
        return 4;
    if (!same_bytes(buffer, "abcd", 4))
        return 5;
    if (probe_raw3(PRIMA_LINUX_SYS_lseek, fd, -2, PRIMA_SEEK_END) != 4)
        return 6;
    if (probe_raw3(PRIMA_LINUX_SYS_read, fd, (long)buffer, 8) != 2)
        return 7;
    if (!same_bytes(buffer, "ef", 2))
        return 8;
    if (probe_raw3(PRIMA_LINUX_SYS_read, fd, (long)buffer, 1) != 0)
        return 9; /* EOF */
    if (probe_raw3(PRIMA_LINUX_SYS_write, fd, (long)"g", 1) != 1)
        return 10;
    if (probe_raw3(PRIMA_LINUX_SYS_lseek, fd, 0, PRIMA_SEEK_SET) != 0)
        return 11;
    if (probe_raw3(PRIMA_LINUX_SYS_read, fd, (long)buffer, 8) != 7)
        return 12;
    if (!same_bytes(buffer, "abcdefg", 7))
        return 13;
    if (probe_raw3(PRIMA_LINUX_SYS_close, fd, 0, 0) != 0)
        return 14;
    /* 句柄失效后 read/write/close 都必须确定失败，而不是静默成功。 */
    if (probe_raw3(PRIMA_LINUX_SYS_read, fd, (long)buffer, 1) !=
        -PRIMA_ERRNO_EBADF)
        return 15;
    if (probe_raw3(PRIMA_LINUX_SYS_write, fd, (long)"x", 1) !=
        -PRIMA_ERRNO_EBADF)
        return 16;
    if (probe_raw3(PRIMA_LINUX_SYS_close, fd, 0, 0) != -PRIMA_ERRNO_EBADF)
        return 17;
    if (probe_raw4(PRIMA_LINUX_SYS_openat, PRIMA_AT_FDCWD, (long)"nope",
                   PRIMA_O_RDONLY, 0) != -PRIMA_ERRNO_ENOENT)
        return 18;
    /* 非法 flag 不做同源差分：Linux 接受 `O_ACCMODE==3` 且对 `O_DIRECTORY` 返回
     * ENOTDIR，与 Prima 的冻结子集错误码不同；该项只在 Prima 侧登记边界。 */
    /* 句柄关闭后立即复用；新句柄 offset 从 0 开始（每次 open 独立偏移）。 */
    long read_only = probe_raw4(PRIMA_LINUX_SYS_openat, PRIMA_AT_FDCWD,
                                (long)"f", PRIMA_O_RDONLY, 0);
    if (read_only < 3)
        return 20;
    if (probe_raw3(PRIMA_LINUX_SYS_write, read_only, (long)"x", 1) !=
        -PRIMA_ERRNO_EBADF)
        return 21;
    if (probe_raw3(PRIMA_LINUX_SYS_lseek, read_only, -1, PRIMA_SEEK_SET) !=
        -PRIMA_ERRNO_EINVAL)
        return 22;
    if (probe_raw3(PRIMA_LINUX_SYS_read, read_only, 0, 1) !=
        -PRIMA_ERRNO_EFAULT)
        return 23;
    if (probe_raw3(PRIMA_LINUX_SYS_read, read_only, (long)buffer, 3) != 3)
        return 24;
    if (!same_bytes(buffer, "abc", 3))
        return 25;
    if (probe_raw3(PRIMA_LINUX_SYS_close, read_only, 0, 0) != 0)
        return 26;
    return 0;
}

static int same(const char *actual, const char *expected) {
    for (; *expected; actual++, expected++)
        if (*actual != *expected)
            return 0;
    return *actual == 0;
}

static unsigned long text_len(const char *s) {
    unsigned long n = 0;
    while (s[n] != 0)
        n++;
    return n;
}

static long probe_write_fd(int fd, const char *s) {
    return probe_raw3(PRIMA_LINUX_SYS_write, fd, (long)s, (long)text_len(s));
}

#if !defined(PRIMA_HOST) && !defined(PRIMA_WITH_LIBC)
static uint64_t read_fs_zero(void) {
    uint64_t value;
    __asm__ volatile("mov %%fs:0, %0" : "=r"(value));
    return value;
}
#endif

#ifndef PRIMA_HOST
/* M3：本任务先把可辨识值放进 XMM0，经过 write 内部的协作切换后读回。
 * 内核必须在切换时 FXSAVE/FXRSTOR；否则其他任务（或残留状态）会串改 XMM。
 * write 走适配层的 `__syscallN`（不经过 libc），因此不会引入 libc 自身的 XMM 使用。
 * Linux 参考侧不跑：glibc 自己在调用间使用 XMM，不能作为保留性基线。 */
static int xmm_round_trip_survives_yield(void) {
    const uint64_t lo = UINT64_C(0x0123456789abcdef);
    const uint64_t hi = UINT64_C(0xfedcba9876543210);
    uint64_t got_lo = 0, got_hi = 0;
    __asm__ volatile("movq %0, %%xmm0" ::"r"(lo) : "xmm0");
    __asm__ volatile("movq %0, %%xmm1" ::"r"(hi) : "xmm1");
    __asm__ volatile("punpcklqdq %%xmm1, %%xmm0" ::: "xmm0");
    if (probe_write_fd(1, "h") != 1 || probe_write_fd(2, "i") != 1)
        return 0;
    __asm__ volatile("movq %%xmm0, %0" : "=r"(got_lo));
    __asm__ volatile("punpckhqdq %%xmm0, %%xmm0" ::: "xmm0");
    __asm__ volatile("movq %%xmm0, %0" : "=r"(got_hi));
    return got_lo == lo && got_hi == hi;
}
#endif

int main(int argc, char **argv, char **envp) {
    if (argc != 2 || !same(argv[1], "stack-ready"))
        return 2;
    if (!envp || !envp[0] || !same(envp[0], "PRIMA_ABI=1") || envp[1] != 0)
        return 3;
    /* 写读私有 .bss：BSS 必须初始为零，且数据页可写。 */
    if (bss_probe != 0)
        return 5;
    bss_probe = UINT64_C(0x5a5a);
    if (bss_probe != UINT64_C(0x5a5a))
        return 5;

#if !defined(PRIMA_HOST) && !defined(PRIMA_WITH_LIBC)
    /* T-MUSL-02：设置本任务 FS base；GET_FS 与 `%fs:0` 必须立即读回同一值。 */
    fs_probe = UINT64_C(0x4d325f52505f5346);
    if (arch_prctl(PRIMA_ARCH_SET_FS, (unsigned long)&fs_probe) != 0)
        return 6;
    unsigned long got = 0;
    if (arch_prctl(PRIMA_ARCH_GET_FS, (unsigned long)&got) != 0 ||
        got != (unsigned long)&fs_probe)
        return 6;
    if (read_fs_zero() != fs_probe)
        return 6;
#endif

    /* 同源差分：fd 1 写 'h'、fd 2 写 'i'，再用两个 iovec 写 5 字节。
     * 后者证明 stdio 实际依赖的 writev 路径与 write 走同一条字节流。 */
#ifdef PRIMA_HOST
    if (probe_write_fd(1, "h") != 1 || probe_write_fd(2, "i") != 1)
        return 4;
#else
    /* M3：同两次写同时验证 x87/SSE 状态跨协作切换保留。 */
    if (!xmm_round_trip_survives_yield())
        return 9;
#ifndef PRIMA_WITH_LIBC
    /* write 内部逐字节 yield 过；恢复后 FS base 不得被其他任务串改。 */
    if (read_fs_zero() != fs_probe)
        return 6;
#endif
#endif

    {
        struct iovec iov[2] = {{(void *)"\nxy", 3}, {(void *)"z\n", 2}};
        if (probe_raw3(PRIMA_LINUX_SYS_writev, 1, (long)iov, 2) != 5)
            return 4;

        /* 非法 fd：write/writev 在适配层前置拒绝，不产生任何消息。 */
        if (probe_raw3(PRIMA_LINUX_SYS_write, 99, (long)"x", 1) !=
            -PRIMA_ERRNO_EBADF)
            return 7;
        if (probe_raw3(PRIMA_LINUX_SYS_writev, 99, (long)iov, 2) !=
            -PRIMA_ERRNO_EBADF)
            return 7;
        if (probe_raw3(PRIMA_LINUX_SYS_close, 99, 0, 0) != -PRIMA_ERRNO_EBADF)
            return 7;
#ifndef PRIMA_HOST
        /* 坏缓冲区：适配层在用户态读指针前先拒绝 NULL，返回原始 -EFAULT。
         * Linux 参考路径不跑这一项：当前宿主内核对 `write(1, ptr=1, 1)` 的返回
         * 与环境相关，不能作为稳定的差分基线。 */
        if (probe_raw3(PRIMA_LINUX_SYS_write, 1, 0, 1) != -PRIMA_ERRNO_EFAULT)
            return 7;
#endif
        /* M4：文件 API 与 stdio 输出必须在 close(1) **之前**——Linux 侧 close(1) 会把
         * 下一次 open 的 fd 变成 1，与 Prima 的 fd 1/2 固定语义分叉。文件失败编码为
         * 40..66；stdio 输出经 libc 的 writev 路径。 */
        {
            int file = file_api_probe();
            if (file)
                return 40 + file;
        }
        if (printf("M4STDIO\n") != 8 || fflush(stdout) != 0)
            return 60;
        /* fd 1/2 的 close 成功；其他 fd 已在上面拒绝。 */
        if (probe_raw3(PRIMA_LINUX_SYS_close, 1, 0, 0) != 0)
            return 7;
        /* ioctl 本阶段不实现：只断言失败（Linux 为 -ENOTTY，Prima 为 -ENOSYS）。 */
        if (probe_raw3(PRIMA_LINUX_SYS_ioctl, 1, 0, 0) >= 0)
            return 7;
        /* 未知号必须确定性地失败，且不得把 Linux 号泄漏给内核。4–6 参数路径
         * 另测：musl 的取消点经 `__syscall6` 进入适配层，不能少传参数。 */
        if (probe_raw3(999, 0, 0, 0) != -PRIMA_ERRNO_ENOSYS)
            return 7;
        if (probe_raw4(999, 0, 0, 0, 0) != -PRIMA_ERRNO_ENOSYS)
            return 7;
        if (probe_raw5(999, 0, 0, 0, 0, 0) != -PRIMA_ERRNO_ENOSYS)
            return 7;
        if (probe_raw6(999, 0, 0, 0, 0, 0, 0) != -PRIMA_ERRNO_ENOSYS)
            return 7;
        if (probe_raw1(PRIMA_LINUX_SYS_set_tid_address, (long)&tid_sink) <= 0)
            return 7;
    }

#ifndef PRIMA_HOST
    /* 无授权：代码段 cap 只有 R 权限，不能用于 TaskSend，内核必须先拒绝。 */
    if (prima_native_syscall3(PRIMA_SYS_SEND,
                              (long)prima_ctx.manager_id,
                              (long)prima_ctx.code_cap, 0) !=
        -PRIMA_ERRNO_EACCES)
        return 8;
#endif

#ifndef PRIMA_HOST
    /* M3：musl 的格式化、环境变量、errno 与 mallocng（不产生输出）。
     * 失败原因编码进退出码（11 = 既有断言；31..39 = 堆阶段），便于 QEMU 定位。 */
    {
        int reason = musl_runtime_probe();
        if (reason)
            return reason;
    }
#endif

    /* Linux `exit`(60) 与 `exit_group`(231) 都是终止路径；用 60 退出并要求它不返回。
     * Linux 参考侧同样走 syscall(60)，因此两端退出码仍是 37。 */
    (void)probe_raw1(PRIMA_LINUX_SYS_exit, 37);
    return 7;
}

#ifndef PRIMA_HOST
/* Prima 路径 `_exit(status)`：上报 main 返回码后自毁；失败则永阻塞，使 QEMU 见证失败。 */
__attribute__((noreturn)) void stage5_entry(uint64_t self, uint64_t send_cap,
                                            uint64_t code_cap, uint64_t name,
                                            uint64_t manager_id,
                                            const uint64_t *stack) {
    (void)self;
    (void)send_cap;
    (void)code_cap;
    (void)name;
    (void)manager_id;
    uint64_t received[3];
    if (((uint64_t)stack & 15) != 0 || stack[0] != 2 ||
        stack[1] != (uint64_t)stack + 0x80 ||
        stack[2] != (uint64_t)stack + 0x90 || stack[3] != 0 ||
        stack[4] != (uint64_t)stack + 0xa0 || stack[5] != 0 ||
        stack[6] != 6 || stack[7] != 4096 || stack[8] != 0 || stack[9] != 0 ||
        !same((const char *)stack[1], "stage5-probe") ||
        !same((const char *)stack[2], "stack-ready") ||
        !same((const char *)stack[4], "PRIMA_ABI=1"))
        goto failed;
    if (prima_native_syscall3(PRIMA_SYS_ABI_CHECK, 1, 0, 0) !=
            UINT64_C(0x0000000100000000) ||
        prima_native_syscall3(PRIMA_SYS_ABI_CHECK, 2, 0, 0) !=
            -PRIMA_ERRNO_EINVAL ||
        prima_native_syscall3(PRIMA_SYS_ABI_CHECK, 1, 1, 0) !=
            -PRIMA_ERRNO_EINVAL ||
        prima_native_syscall3(PRIMA_SYS_ABI_CHECK, 1, 0, 1) !=
            -PRIMA_ERRNO_EINVAL)
        goto failed;
    _exit(main((int)stack[0], (char **)&stack[1], (char **)&stack[4]));
failed:
    for (;;)
        (void)prima_native_syscall3(PRIMA_SYS_RECV, (long)received, 0, 0);
}
#endif
