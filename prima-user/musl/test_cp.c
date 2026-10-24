#include <assert.h>

extern long __syscall_cp_asm(volatile int *, long, long, long, long, long, long, long);
static int calls;
long __syscall6(long n, long a, long b, long c, long d, long e, long f) {
    calls++;
    assert(n == 123 && a == 11 && b == 22 && c == 33 && d == 44 && e == 55 && f == 66);
    return -321;
}
long __cancel(void) { return -456; }
int main(void) {
    volatile int cancel = 0;
    assert(__syscall_cp_asm(&cancel, 123, 11, 22, 33, 44, 55, 66) == -321);
    assert(calls == 1);
    cancel = 1;
    assert(__syscall_cp_asm(&cancel, 0, 0, 0, 0, 0, 0, 0) == -456);
    assert(calls == 1);
    return 0;
}
