/* Freestanding C task: the assembler entry passes the initial stack in r9.
 *
 * 片 C：同一份源码在 Linux 宿主与 Prima QEMU 上编译运行。
 * - Linux：`write(1/2)` 走宿主 syscall。
 * - Prima：无 libc；fd 1/2 由 STAGE5_TTY_WRITE_TAG 直投服务管理器，管理器转发给
 *   用户态 tty 服务写串口。`_exit` 先把 main 返回码发给管理器，再自毁 Task。
 * `main` 在两端收到相同的 argv[1]="stack-ready" / PRIMA_ABI=1，控制流逐字节同源。
 *
 * 探针的代码段按 W^X 以只读映射，故**不使用可写全局变量**：Prima 侧的对端/凭据通过
 * 栈上的 `probe_ctx` 传递，宏在 Linux 侧展开为空。
 */
#include <stdint.h>

#define SYS_TASK_ID 7
#define SYS_YIELD 8
#define SYS_SEND 17
#define SYS_RECV 18
#define SYS_TASK_DESTROY 20
#define SYS_ABI_CHECK 37
#define ABI_VERSION UINT64_C(0x0000000100000000)
#define ERR_INVALID ((uint64_t)-22)
#define PROBE_TAG UINT64_C(0xc500000000005a00)
#define TTY_WRITE_TAG UINT64_C(0xc602000000000000)

#ifdef PRIMA_HOST
/* Linux 参考路径使用的宿主 syscall（write=1）。 */
static uint64_t host_call(uint64_t number, uint64_t a0, uint64_t a1,
                          uint64_t a2) {
    register uint64_t r_number __asm__("rax") = number;
    __asm__ volatile("syscall"
                     : "+r"(r_number)
                     : "D"(a0), "S"(a1), "d"(a2)
                     : "rcx", "r11", "memory");
    return r_number;
}

static long host_write(int fd, const void *buf, unsigned long len) {
    return (long)host_call(1, (uint64_t)fd, (uint64_t)buf, len);
}

#define PROBE_CTX_PARAM
#define PROBE_CTX_ARG

static unsigned long write_fd(int fd, const char *s) {
    unsigned long n = 0;
    while (s[n] != 0)
        n++;
    return (unsigned long)host_write(fd, s, n);
}
#else
#define PRIMA_CALL3(nr, a0, a1, a2)                                            \
    ({                                                                         \
        register uint64_t _n __asm__("rax") = (nr);                            \
        register uint64_t _d __asm__("r10") = 0;                               \
        register uint64_t _e __asm__("r8") = 0;                                \
        register uint64_t _f __asm__("r9") = 0;                                \
        __asm__ volatile("syscall"                                             \
                         : "+a"(_n)                                            \
                         : "D"(a0), "S"(a1), "d"(a2), "r"(_d), "r"(_e),        \
                           "r"(_f)                                             \
                         : "rcx", "r11", "memory");                            \
        _n;                                                                    \
    })

typedef struct {
    uint64_t peer;
    uint64_t send_cap;
} probe_ctx;

#define PROBE_CTX_PARAM const probe_ctx *ctx,
#define PROBE_CTX_ARG ctx,

/* Prima 路径：fd 1/2 的每个字节直投服务管理器（tag + fd + byte + sender id）。 */
static long prima_write(const probe_ctx *ctx, int fd, const void *buf,
                        unsigned long len) {
    const unsigned char *p = buf;
    for (unsigned long i = 0; i < len; i++) {
        uint64_t id = PRIMA_CALL3(SYS_TASK_ID, 0, 0, 0);
        uint64_t payload = TTY_WRITE_TAG | ((uint64_t)(uint8_t)fd << 16) |
                           ((uint64_t)p[i] << 24) | ((id & 0xff) << 32);
        if (PRIMA_CALL3(SYS_SEND, ctx->peer, ctx->send_cap, payload) != 0)
            return -1;
        /* 让管理器先取走这条消息：自毁会清掉本任务仍在队列里未取的消息。 */
        (void)PRIMA_CALL3(SYS_YIELD, 0, 0, 0);
    }
    return (long)len;
}

static unsigned long write_fd(const probe_ctx *ctx, int fd, const char *s) {
    unsigned long n = 0;
    while (s[n] != 0)
        n++;
    return (unsigned long)prima_write(ctx, fd, s, n);
}
#endif

static int same(const char *actual, const char *expected) {
    for (; *expected; actual++, expected++)
        if (*actual != *expected)
            return 0;
    return *actual == 0;
}

int main(PROBE_CTX_PARAM int argc, char **argv, char **envp) {
    if (argc != 2 || !same(argv[1], "stack-ready"))
        return 2;
    if (!envp || !envp[0] || !same(envp[0], "PRIMA_ABI=1") || envp[1] != 0)
        return 3;
    /* 同源差分：fd 1 写 'h'、fd 2 写 'i'，两端都要求各写 1 字节。 */
    if (write_fd(PROBE_CTX_ARG 1, "h") != 1 ||
        write_fd(PROBE_CTX_ARG 2, "i") != 1)
        return 4;
    return 37;
}

#ifndef PRIMA_HOST
/* Prima 路径 `_exit(status)`：上报 main 返回码后自毁；失败则永阻塞，使 QEMU 见证失败。 */
__attribute__((noreturn)) static void prima_exit(const probe_ctx *ctx,
                                                 uint64_t self, int status) {
    uint64_t id = PRIMA_CALL3(SYS_TASK_ID, 0, 0, 0);
    uint64_t payload =
        PROBE_TAG | ((uint64_t)(uint8_t)status << 16) | (id & 0xff);
    /* 此刻管理器阻塞在 recv，退出状态走 pending 直投；随即自毁不会丢掉它。 */
    if (PRIMA_CALL3(SYS_SEND, ctx->peer, ctx->send_cap, payload) == 0)
        PRIMA_CALL3(SYS_TASK_DESTROY, self, 0, 0);
    uint64_t sink[3];
    for (;;)
        (void)PRIMA_CALL3(SYS_RECV, (uint64_t)sink, 0, 0);
}

/* Prima 的汇编入口只存在于裸机目标；Linux 参考路径由宿主 libc 调用 main 并退出。 */
__attribute__((noreturn)) void stage5_entry(uint64_t self, uint64_t send_cap,
                                            uint64_t code_cap, uint64_t name,
                                            uint64_t manager_id,
                                            const uint64_t *stack) {
    (void)code_cap;
    (void)name;
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
    {
        const probe_ctx ctx = {manager_id, send_cap};
        if (PRIMA_CALL3(SYS_ABI_CHECK, 1, 0, 0) != ABI_VERSION ||
            PRIMA_CALL3(SYS_ABI_CHECK, 2, 0, 0) != ERR_INVALID ||
            PRIMA_CALL3(SYS_ABI_CHECK, 1, 1, 0) != ERR_INVALID ||
            PRIMA_CALL3(SYS_ABI_CHECK, 1, 0, 1) != ERR_INVALID)
            goto failed;
        prima_exit(&ctx, self,
                   main(&ctx, (int)stack[0], (char **)&stack[1],
                        (char **)&stack[4]));
    }
failed:
    for (;;)
        (void)PRIMA_CALL3(SYS_RECV, (uint64_t)received, 0, 0);
}
#endif
