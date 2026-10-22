/* M15a-1：WaitObject 的 Prima 原生探针（无 Linux 对应物）。
 *
 * 覆盖 create/observe/signal、立即满足、立即超时、超前观测（EINVAL）、
 * destroy 后目标死亡（ENOENT）与权限位。需要阻塞的路径由 host 状态机覆盖
 * （W1 没有第二个用户任务可 signal，阻塞会挂起）。
 */
#include <errno.h>
#include <stdint.h>
#include <unistd.h>

extern long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);

#define SYS_WAIT_CREATE 42
#define SYS_WAIT_OBSERVE 43
#define SYS_WAIT_OBJECTS 44
#define SYS_WAIT_SIGNAL 45
#define SYS_WAIT_DESTROY 46
#define EINVAL_ 22
#define ENOENT_ 2

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

static long wait_call(long cap0, long gen0, long mode, long timeout) {
    return prima_native(SYS_WAIT_OBJECTS, cap0, gen0, 0, 0, mode, timeout);
}

int main(void) {
    emit("WAIT");
    long cap = prima_native(SYS_WAIT_CREATE, 0, 0, 0, 0, 0, 0);
    field("create", cap > 0, cap);

    long gen0 = cap > 0 ? prima_native(SYS_WAIT_OBSERVE, cap, 0, 0, 0, 0, 0) : -1;
    field("gen0", gen0 == 0, gen0);

    long gen1 = cap > 0 ? prima_native(SYS_WAIT_SIGNAL, cap, 0, 0, 0, 0, 0) : -1;
    field("signal", gen1 == 1, gen1);

    long satisfied = cap > 0 ? wait_call(cap, 0, 0, (long)0x7fffffffffffffffL) : -1;
    field("satisfied", satisfied == 0, satisfied);

    long timeout = cap > 0 ? wait_call(cap, 1, 0, 0) : -1;
    field("timeout", timeout == 1, timeout);

    long invalid = cap > 0 ? wait_call(cap, 9, 0, (long)0x7fffffffffffffffL) : -1;
    field("invalid", invalid == -EINVAL_, invalid);

    long destroyed = cap > 0 ? prima_native(SYS_WAIT_DESTROY, cap, 0, 0, 0, 0, 0) : -1;
    field("destroy", destroyed == 0, destroyed);

    long dead = cap > 0 ? prima_native(SYS_WAIT_OBSERVE, cap, 0, 0, 0, 0, 0) : -1;
    field("dead", dead == -ENOENT_, dead);

    emit("|ok=");
    emit_dec(failures ? 0 : 1);
    emit(";err=");
    emit_dec(first_error);
    emit("\n");
    return failures ? 1 : 0;
}
