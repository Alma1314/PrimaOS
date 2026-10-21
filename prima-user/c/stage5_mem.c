/* M10：匿名内存与保护（mmap/munmap/mprotect）的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - Prima 的匿名 mmap 区是 56 页固定 arena；Linux 侧是真实匿名映射。两者都只使用
 *   `MAP_PRIVATE|MAP_ANONYMOUS`、fd=-1、offset=0。
 * - 覆盖：首次映射清零、读写、`mprotect` 收缩/恢复、`MAP_FIXED` 替换、`munmap`
 *   后复用并清零、`PROT_NONE` 往返、错误码（`EINVAL`/`ENOMEM`），以及 musl 大块
 *   `malloc`（走 mmap）后 `free`（走 munmap）。
 * - `PROT_EXEC` 是两端唯一的语义差异：Linux 允许匿名可执行，Prima 的匿名段
 *   `allowed_access` 只有 RW（W^X），必须返回 `EACCES`。探针按平台断言各自期望，
 *   输出字段仍逐字节相同（`exec=1` 表示“本平台的冻结期望成立”）。
 * - Prima 侧另用原生号直调 `SYS_MEMSEG_PROTECT`，覆盖 `ZeroSize`/`InvalidAccess`/
 *   `NotMapped` 与收缩/扩权成功路径；宿主把 `native` 记为不适用（1）。
 *
 * 失败位写入 `MEM|ok=0;...` 并以 1 退出；QEMU 与 Linux 都要求退出码 0。
 */
#define _GNU_SOURCE

#include <errno.h>
#include <stdint.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <unistd.h>

#ifdef PRIMA_HOST
#define NATIVE_OK 1
#else
#define PRIMA_ABI_NO_IOVEC
#include "prima_syscall.h"
#endif

#define PAGE 4096UL

static int failures;

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

static void field(const char *name, int pass, int *first) {
    emit(*first ? "|" : ";");
    *first = 0;
    emit(name);
    emit("=");
    emit_dec(pass ? 1 : 0);
    if (!pass)
        failures = 1;
}

static unsigned char *map_pages(unsigned long pages, int prot) {
    return mmap(0, pages * PAGE, prot, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
}

/* 一页从零填充、可读写、可改保护、可 MAP_FIXED、可复用。 */
static void anonymous_roundtrip(int *map_ok, int *zero_ok, int *write_ok,
                                int *protect_ok, int *protect_back_ok,
                                int *fixed_ok, int *reuse_ok, int *none_ok) {
    unsigned char *p = map_pages(3, PROT_READ | PROT_WRITE);
    *map_ok = p != MAP_FAILED && ((uintptr_t)p % PAGE) == 0;
    if (p == MAP_FAILED)
        return;
    *zero_ok = p[0] == 0 && p[PAGE] == 0 && p[2 * PAGE] == 0 &&
               p[3 * PAGE - 1] == 0;
    p[0] = 0x5a;
    p[PAGE + 1] = 0xa5;
    p[2 * PAGE + 2] = 0x3c;
    *write_ok = p[0] == 0x5a && p[PAGE + 1] == 0xa5 && p[2 * PAGE + 2] == 0x3c;

    *protect_ok = mprotect(p, PAGE, PROT_READ) == 0 && p[0] == 0x5a;
    *protect_back_ok =
        mprotect(p, PAGE, PROT_READ | PROT_WRITE) == 0 && (p[0] = 0x5b) == 0x5b;

    /* MAP_FIXED 替换：先释放中间页，再固定映射回同一地址。 */
    unsigned char *fixed = p + PAGE;
    *fixed_ok = munmap(fixed, PAGE) == 0;
    if (*fixed_ok) {
        unsigned char *again = mmap(fixed, PAGE, PROT_READ | PROT_WRITE,
                                    MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED, -1,
                                    0);
        *fixed_ok = again == fixed && again[0] == 0 &&
                    (again[0] = 0x7e) == 0x7e && munmap(again, PAGE) == 0;
    }

    /* 剩余两页整体释放后，新映射必须重新清零（栈内残留不得泄漏）。 */
    *reuse_ok = munmap(p, 2 * PAGE) == 0;
    if (*reuse_ok) {
        unsigned char *fresh = map_pages(2, PROT_READ | PROT_WRITE);
        *reuse_ok = fresh != MAP_FAILED && fresh[0] == 0 && fresh[PAGE] == 0 &&
                    (fresh[0] = 0x33) == 0x33 && munmap(fresh, 2 * PAGE) == 0;
    }

    /* PROT_NONE 往返：Linux 与 Prima（近似为只读）都成功。 */
    unsigned char *none = map_pages(1, PROT_READ | PROT_WRITE);
    *none_ok = none != MAP_FAILED && mprotect(none, PAGE, PROT_NONE) == 0 &&
               mprotect(none, PAGE, PROT_READ | PROT_WRITE) == 0 &&
               (none[0] = 0x11) == 0x11 && munmap(none, PAGE) == 0;
}

/* 错误码：缺页、非法长度/对齐与已释放区间的 mprotect。 */
static void error_paths(int *mmap_err, int *munmap_err, int *mprotect_err,
                        int *unmapped_err) {
    errno = 0;
    void *empty = map_pages(0, PROT_READ | PROT_WRITE);
    *mmap_err = empty == MAP_FAILED && errno == EINVAL;

    unsigned char *p = map_pages(1, PROT_READ | PROT_WRITE);
    if (p == MAP_FAILED) {
        *munmap_err = 0;
        *mprotect_err = 0;
        *unmapped_err = 0;
        return;
    }
    errno = 0;
    *munmap_err = munmap(p, 0) == -1 && errno == EINVAL;
    errno = 0;
    *munmap_err = *munmap_err && munmap(p + 1, PAGE) == -1 && errno == EINVAL;
    /* musl 的 mprotect 会把地址向下取整，故用非法 prot 位测 EINVAL（两端一致）。 */
    errno = 0;
    *mprotect_err = mprotect(p, PAGE, 0x80) == -1 && errno == EINVAL;
    *unmapped_err = munmap(p, PAGE) == 0;
    if (*unmapped_err) {
        errno = 0;
        *unmapped_err = mprotect(p, PAGE, PROT_READ) == -1 && errno == ENOMEM;
    }
}

/* 大块 malloc 走 mmap，free 走 munmap；随后小分配仍必须可用。 */
static int big_allocations(void) {
    unsigned long size = 140000;
    unsigned char *big = malloc(size);
    if (!big)
        return 0;
    for (unsigned long offset = 0; offset < size; offset += PAGE)
        big[offset] = (unsigned char)(offset / PAGE + 1);
    int ok = big[0] == 1;
    big[size - 1] = 0xee;
    ok = ok && big[size - 1] == 0xee;
    free(big);

    unsigned char *small = malloc(64);
    int small_ok = small != 0 && (small[0] = 0x21) == 0x21;
    free(small);
    unsigned char *grown = realloc(0, 8192);
    int grown_ok = grown != 0 && (grown[4095] = 0x42) == 0x42;
    free(grown);
    return ok && small_ok && grown_ok;
}

#ifndef PRIMA_HOST
/* 原生号直调：只有 Prima 侧有 memseg_protect。 */
static long native_protect(long address, long pages, long access) {
    return prima_native_syscall4(PRIMA_SYS_MEMSEG_PROTECT,
                                 (long)prima_ctx.self_cap, address, pages,
                                 access);
}

static int native_memseg_protect(void) {
    unsigned char *live = map_pages(1, PROT_READ | PROT_WRITE);
    unsigned char *gone = map_pages(1, PROT_READ | PROT_WRITE);
    if (live == MAP_FAILED || gone == MAP_FAILED)
        return 0;
    int ok = munmap(gone, PAGE) == 0;
    /* ZeroSize / 新增 X / 未映射页。 */
    ok = ok && native_protect((long)live, 0, PRIMA_ACCESS_R) == -PRIMA_ERRNO_EINVAL;
    /* X 超出匿名 Segment 的 allowed_access：内核 InvalidAccess → EINVAL。 */
    ok = ok && native_protect((long)live, 1, PRIMA_ACCESS_R | PRIMA_ACCESS_X) ==
                   -PRIMA_ERRNO_EINVAL;
    ok = ok && native_protect((long)gone, 1, PRIMA_ACCESS_R) ==
                   -PRIMA_ERRNO_ENOENT;
    /* 收缩（不需要 Segment cap）与扩权（需要 M10 申请的 cap）都成功。 */
    ok = ok && native_protect((long)live, 1, PRIMA_ACCESS_R) == 0;
    ok = ok && native_protect((long)live, 1, PRIMA_ACCESS_R | PRIMA_ACCESS_W) == 0;
    ok = ok && (live[0] = 0x66) == 0x66;
    ok = ok && munmap(live, PAGE) == 0;
    return ok;
}
#endif

int main(void) {
    int map_ok = 0, zero_ok = 0, write_ok = 0, protect_ok = 0;
    int protect_back_ok = 0, fixed_ok = 0, reuse_ok = 0, none_ok = 0;
    int mmap_err = 0, munmap_err = 0, mprotect_err = 0, unmapped_err = 0;
    anonymous_roundtrip(&map_ok, &zero_ok, &write_ok, &protect_ok,
                        &protect_back_ok, &fixed_ok, &reuse_ok, &none_ok);
    error_paths(&mmap_err, &munmap_err, &mprotect_err, &unmapped_err);

    /* 匿名可执行：Prima 的 W^X 必须拒绝；Linux 允许。 */
    int exec_ok = 0;
    unsigned char *exec = map_pages(1, PROT_READ | PROT_WRITE);
    if (exec != MAP_FAILED) {
        errno = 0;
        int result = mprotect(exec, PAGE, PROT_READ | PROT_EXEC);
#ifdef PRIMA_HOST
        exec_ok = result == 0;
        if (exec_ok)
            exec_ok = mprotect(exec, PAGE, PROT_READ | PROT_WRITE) == 0;
#else
        exec_ok = result == -1 && errno == EACCES;
#endif
        exec_ok = exec_ok && munmap(exec, PAGE) == 0;
    }

    int big_ok = big_allocations();
#ifdef PRIMA_HOST
    int native_ok = NATIVE_OK;
#else
    int native_ok = native_memseg_protect();
#endif

    int ok = map_ok && zero_ok && write_ok && protect_ok && protect_back_ok &&
             fixed_ok && reuse_ok && none_ok && exec_ok && mmap_err &&
             munmap_err && mprotect_err && unmapped_err && big_ok && native_ok;
    emit(ok ? "MEM|ok=1" : "MEM|ok=0");
    int first = 0;
    field("map", map_ok, &first);
    field("zero", zero_ok, &first);
    field("write", write_ok, &first);
    field("protect", protect_ok, &first);
    field("protect_back", protect_back_ok, &first);
    field("fixed", fixed_ok, &first);
    field("reuse", reuse_ok, &first);
    field("none", none_ok, &first);
    field("exec", exec_ok, &first);
    field("mmap_err", mmap_err, &first);
    field("munmap_err", munmap_err, &first);
    field("mprotect_err", mprotect_err, &first);
    field("unmapped", unmapped_err, &first);
    field("big", big_ok, &first);
    field("native", native_ok, &first);
    emit("\n");
    return ok ? 0 : 1;
}
