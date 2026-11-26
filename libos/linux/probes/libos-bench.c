/* Same raw Linux ELF in Prima's real LibOS and the pinned reference guest. */
#include <stdint.h>

static long raw(long n, long a, long b, long c) {
    register long r10 __asm__("r10") = 0;
    register long r8 __asm__("r8") = 0;
    register long r9 __asm__("r9") = 0;
    __asm__ volatile("syscall" : "+a"(n) : "D"(a), "S"(b), "d"(c), "r"(r10), "r"(r8), "r"(r9) : "rcx", "r11", "memory");
    return n;
}
static uint64_t stamp(void) {
    uint32_t lo, hi;
    __asm__ volatile("lfence; rdtsc" : "=a"(lo), "=d"(hi) : : "memory");
    return ((uint64_t)hi << 32) | lo;
}
static const unsigned char expected[] = "hello from prima\n";
static unsigned char buffer[19];
static struct { uint64_t begin, end, result; unsigned char bytes[19]; } rows[64];
static char output[12000];
static unsigned used;
static void text(const char *s) { while (*s) output[used++] = *s++; }
static void number(uint64_t n) {
    char digits[20]; unsigned at = 20;
    do { digits[--at] = '0' + n % 10; n /= 10; } while (n);
    while (at < 20) output[used++] = digits[at++];
}
long bench_main(void) {
    if (raw(2, (long)"/fixture.txt", 0, 0) != 3) return 1;
    for (unsigned i = 0; i < 64; ++i) {
        if (raw(8, 3, 0, 0) != 0) return 2;
        for (unsigned j = 0; j < 19; ++j) buffer[j] = 0xa5;
        rows[i].begin = stamp();
        long result = raw(0, 3, (long)(buffer + 1), 17);
        rows[i].end = stamp();
        rows[i].result = result;
        if (result != 17 || buffer[0] != 0xa5 || buffer[18] != 0xa5) return 3;
        for (unsigned j = 0; j < 17; ++j) if (buffer[j + 1] != expected[j]) return 4;
        for (unsigned j = 0; j < 19; ++j) rows[i].bytes[j] = buffer[j];
    }
    if (raw(3, 3, 0, 0) != 0) return 5;
    text("S7LIBOS|samples=64;operation=read;fd=3;bytes=17;boundary=user-wrapper;verified=1\n");
    for (unsigned i = 0; i < 64; ++i) {
        text("S7LTIME|"); number(i); text(";"); number(rows[i].begin);
        text(";"); number(rows[i].end); text(";"); number(rows[i].result); text(";");
        for (unsigned j = 1; j <= 17; ++j) {
            output[used++] = "0123456789abcdef"[rows[i].bytes[j] >> 4];
            output[used++] = "0123456789abcdef"[rows[i].bytes[j] & 15];
        }
        text(";"); number(rows[i].bytes[0]); text(";"); number(rows[i].bytes[18]); text("\n");
    }
    for (unsigned at = 0; at < used;) {
        unsigned size = used - at > 4096 ? 4096 : used - at;
        if (raw(1, 1, (long)(output + at), size) != size) return 6;
        at += size;
    }
    return 0;
}
__asm__(".global _start\n_start:\nxor %ebp,%ebp\nandq $-16,%rsp\ncall bench_main\nmov %rax,%rdi\nmov $231,%eax\nsyscall\nud2\n");
