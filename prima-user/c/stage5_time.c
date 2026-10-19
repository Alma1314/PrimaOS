/* M6：POSIX 时间的最小同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - Prima 的 `clock_gettime`/`nanosleep`/`clock_nanosleep` 经 `__syscallN` 进入适配层，
 *   由 PIT tick（~100 Hz）提供单调时间；
 * - 输出只含通过/失败位，不含实际耗时，故两端 stdout 可逐字节比较；
 * - 任一检查失败 → 退出码 1，并把对应位写成 0。
 *
 * 冻结边界（profile P5-TIME-01 partial）：只查询 CLOCK_MONOTONIC；相对/绝对睡眠以
 * 10 ms tick 粒度向上取整；无信号，`rem` 不写回。
 */
#define _POSIX_C_SOURCE 200809L

#include <errno.h>
#include <stdint.h>
#include <time.h>

extern long write(int fd, const void *buffer, unsigned long count);

static int failures;

static unsigned long text_len(const char *text) {
    unsigned long len = 0;
    while (text[len] != 0)
        len++;
    return len;
}

static void emit(const char *text) {
    (void)write(1, text, text_len(text));
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
    (void)write(1, &digits[index], (unsigned long)(20 - index));
}

static void field(const char *name, int pass, int *first) {
    emit(*first ? "|" : ";");
    *first = 0;
    emit(name);
    emit("=");
    emit_dec(pass ? 1 : 0);
    if (!pass)
        failures = 1;
}

static int64_t ts_ns(const struct timespec *ts) {
    return (int64_t)ts->tv_sec * 1000000000LL + ts->tv_nsec;
}

int main(void) {
    struct timespec t0;
    struct timespec t1;

    int clock_ok = clock_gettime(CLOCK_MONOTONIC, &t0) == 0;
    int nsec_ok = clock_ok && t0.tv_sec >= 0 && t0.tv_nsec >= 0 &&
                  t0.tv_nsec < 1000000000L;
    int mono_ok = clock_ok && clock_gettime(CLOCK_MONOTONIC, &t1) == 0 &&
                  ts_ns(&t1) >= ts_ns(&t0);

    errno = 0;
    int badclk_ok =
        clock_gettime((clockid_t)1234, &t1) == -1 && errno == EINVAL;
    /* 走原始调用号测 NULL 指针：glibc 的 clock_gettime 走 vDSO 会直接解引用
     * NULL（SIGSEGV），而内核/Prima 适配层都必须确定性地返回 EFAULT。
     * 228 = Linux x86_64 clock_gettime；在 Prima 上由适配层翻译。 */
    extern long syscall(long number, ...);
    errno = 0;
    long raw_clock = syscall(228L, (long)CLOCK_MONOTONIC, 0L);
    int badptr_ok = raw_clock == -1 && errno == EFAULT;

    struct timespec req = {0, 1000000000L};
    errno = 0;
    int badnsec_ok = nanosleep(&req, 0) == -1 && errno == EINVAL;

    req.tv_sec = 0;
    req.tv_nsec = 0;
    int zero_ok = nanosleep(&req, 0) == 0;

    (void)clock_gettime(CLOCK_MONOTONIC, &t0);
    req.tv_sec = 0;
    req.tv_nsec = 30000000L;
    int rel_ok = nanosleep(&req, 0) == 0;
    (void)clock_gettime(CLOCK_MONOTONIC, &t1);
    rel_ok = rel_ok && ts_ns(&t1) - ts_ns(&t0) >= 30000000LL;

    (void)clock_gettime(CLOCK_MONOTONIC, &t0);
    struct timespec deadline = t0;
    deadline.tv_nsec += 40000000L;
    if (deadline.tv_nsec >= 1000000000L) {
        deadline.tv_nsec -= 1000000000L;
        deadline.tv_sec += 1;
    }
    int abs_ok =
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &deadline, 0) == 0;
    (void)clock_gettime(CLOCK_MONOTONIC, &t1);
    abs_ok = abs_ok && ts_ns(&t1) >= ts_ns(&deadline);

    struct timespec past = {0, 0};
    int past_ok =
        clock_nanosleep(CLOCK_MONOTONIC, TIMER_ABSTIME, &past, 0) == 0;

    int all_ok = clock_ok && nsec_ok && mono_ok && badclk_ok && badptr_ok &&
                 badnsec_ok && zero_ok && rel_ok && abs_ok && past_ok;
    failures = !all_ok;
    int first = 1;
    emit("TIME");
    field("ok", all_ok, &first);
    field("clock", clock_ok, &first);
    field("nsec", nsec_ok, &first);
    field("mono", mono_ok, &first);
    field("badclk", badclk_ok, &first);
    field("badptr", badptr_ok, &first);
    field("badnsec", badnsec_ok, &first);
    field("zero", zero_ok, &first);
    field("rel", rel_ok, &first);
    field("abs", abs_ok, &first);
    field("past", past_ok, &first);
    emit("\n");
    return failures ? 1 : 0;
}
