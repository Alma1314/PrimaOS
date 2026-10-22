/* M15b-2：共享地址空间线程唤醒 WaitObject 的 Prima 原生探针。
 *
 * 主线程创建 WaitObject 与一个共享 Process 的线程；主线程阻塞等待，线程
 * 用同一能力空间 signal，然后以自毁（SYS_TASK_DESTROY + rdi 自 cap）退出。
 * 覆盖：thread_create(47)、跨线程 cap 授权、BlockedWait 端到端唤醒、
 * Dead 槽位回收后的线程创建。
 */
#include <stdint.h>
#include <unistd.h>

extern long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);

#define SYS_TASK_DESTROY 20
#define SYS_WAIT_CREATE 42
#define SYS_WAIT_OBSERVE 43
#define SYS_WAIT_OBJECTS 44
#define SYS_WAIT_SIGNAL 45
#define SYS_WAIT_DESTROY 46
#define SYS_THREAD_CREATE 47

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

static long g_cap;
static unsigned char g_stack[16384] __attribute__((aligned(16)));

/* rdi 由内核写入：新线程的自毁 cap（SysV 第一个参数）。 */
static void thread_fn(unsigned long self) {
    if (g_cap > 0)
        (void)prima_native(SYS_WAIT_SIGNAL, g_cap, 0, 0, 0, 0, 0);
    (void)prima_native(SYS_TASK_DESTROY, (long)self, 0, 0, 0, 0, 0);
    for (;;) {
    }
}

int main(void) {
    emit("WAIT2");
    long cap = prima_native(SYS_WAIT_CREATE, 0, 0, 0, 0, 0, 0);
    field("create", cap > 0, cap);
    g_cap = cap;

    long stack_top = (long)(g_stack + sizeof g_stack);
    long thread = cap > 0 ? prima_native(SYS_THREAD_CREATE, (long)&thread_fn, stack_top,
                                         0, 0, 0, 0)
                          : -1;
    field("thread", thread > 0, thread);

    long satisfied = cap > 0
                         ? prima_native(SYS_WAIT_OBJECTS, cap, 0, 0, 0, 0,
                                        (long)0x7fffffffffffffffL)
                         : -1;
    field("satisfied", satisfied == 0, satisfied);

    long gen = cap > 0 ? prima_native(SYS_WAIT_OBSERVE, cap, 0, 0, 0, 0, 0) : -1;
    field("gen", gen == 1, gen);

    long destroyed = cap > 0 ? prima_native(SYS_WAIT_DESTROY, cap, 0, 0, 0, 0, 0) : -1;
    field("destroy", destroyed == 0, destroyed);

    emit("|ok=");
    emit_dec(failures ? 0 : 1);
    emit(";err=");
    emit_dec(first_error);
    emit("\n");
    return failures ? 1 : 0;
}
