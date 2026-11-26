/* M16a：futex 子集（WAIT/WAKE）的 Prima 探针。
 *
 * 共享线程在 &g_word 上 futex_wait(0)；主线程在 g_ready 后置 g_word=1 并
 * futex_wake(1)：线程被唤醒返回 0、自毁。另测 EAGAIN（值不匹配）与
 * 无等待者的 wake 返回 0。
 */
#include <errno.h>
#include <stdint.h>
#include <unistd.h>

extern long syscall(long number, ...);
extern long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);

#define SYS_TASK_DESTROY 20
#define SYS_YIELD 8
#define SYS_THREAD_CREATE 47
#define FUTEX_WAIT 0
#define FUTEX_WAKE 1

static int failures;
static int first_error;
static int first_field = 1;

static unsigned long text_len(const char *text) {
    unsigned long len = 0;
    while (text[len] != 0)
        len++;
    return len;
}

static void emit(const char *text) {
    ssize_t ignored = write(1, text, text_len(text));
    (void)ignored;
}

static void emit_dec(long value) {
    char digits[20];
    int index = 20;
    unsigned long magnitude =
        value < 0 ? (unsigned long)(-value) : (unsigned long)value;
    do {
        digits[--index] = (char)('0' + magnitude % 10);
        magnitude /= 10;
    } while (magnitude != 0);
    if (value < 0)
        digits[--index] = '-';
    ssize_t ignored = write(1, &digits[index], (unsigned long)(20 - index));
    (void)ignored;
}

static void field(const char *name, int pass, long error) {
    emit(first_field ? "|" : ";");
    first_field = 0;
    emit(name);
    emit("=");
    emit_dec(pass ? 1 : 0);
    if (!pass) {
        failures = 1;
        if (first_error == 0)
            first_error = (int)error;
    }
}

static int g_word;
static volatile int g_ready;
static volatile long g_wait_result = -999;
static unsigned char g_stack[16384] __attribute__((aligned(16)));

static void waiter(unsigned long self) {
    g_ready = 1;
    errno = 0;
    long result = syscall(202L, (long)&g_word, (long)FUTEX_WAIT, 0L, 0L);
    g_wait_result = result == 0 ? 0 : -errno;
    (void)prima_native(SYS_TASK_DESTROY, (long)self, 0, 0, 0, 0, 0);
    for (;;) {
    }
}

int main(void) {
    emit("FUTEX");
    long stack_top = (long)(g_stack + sizeof g_stack);
    long thread = prima_native(SYS_THREAD_CREATE, (long)&waiter, stack_top, 0, 0, 0, 0);
    field("thread", thread > 0, thread);

    /* 协作调度：yield 让 waiter 先进入 futex_wait 阻塞。 */
    while (!g_ready)
        (void)prima_native(SYS_YIELD, 0, 0, 0, 0, 0, 0);

    g_word = 1;
    errno = 0;
    long woken = syscall(202L, (long)&g_word, (long)FUTEX_WAKE, 1L, 0L);
    field("wake", woken == 1, woken);

    while (g_wait_result == -999)
        (void)prima_native(SYS_YIELD, 0, 0, 0, 0, 0, 0);
    field("wait", g_wait_result == 0, g_wait_result);

    errno = 0;
    long mismatch = syscall(202L, (long)&g_word, (long)FUTEX_WAIT, 0L, 0L);
    field("eagain", mismatch == -1 && errno == EAGAIN, errno);

    errno = 0;
    long nowake = syscall(202L, (long)&g_word, (long)FUTEX_WAKE, 1L, 0L);
    field("nowake", nowake == 0, nowake);

    emit("|ok=");
    emit_dec(failures ? 0 : 1);
    emit(";err=");
    emit_dec(first_error);
    emit("\n");
    return failures ? 1 : 0;
}
