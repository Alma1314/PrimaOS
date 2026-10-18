/* Freestanding C task: the assembler entry passes the initial stack in r9. */
#include <stdint.h>

#define SYS_TASK_ID 7
#define SYS_SEND 17
#define SYS_RECV 18
#define SYS_TASK_DESTROY 20
#define SYS_ABI_CHECK 37
#define ABI_VERSION UINT64_C(0x0000000100000000)
#define ERR_INVALID ((uint64_t)-22)
#define PROBE_TAG UINT64_C(0xc500000000005a00)

static inline uint64_t prima_call(uint64_t number, uint64_t a0, uint64_t a1,
                                  uint64_t a2) {
    register uint64_t a3 __asm__("r10") = 0;
    register uint64_t a4 __asm__("r8") = 0;
    register uint64_t a5 __asm__("r9") = 0;
    __asm__ volatile("syscall"
                     : "+a"(number)
                     : "D"(a0), "S"(a1), "d"(a2), "r"(a3), "r"(a4), "r"(a5)
                     : "rcx", "r11", "memory");
    return number;
}

static int same(const char *actual, const char *expected) {
    for (; *expected; actual++, expected++)
        if (*actual != *expected)
            return 0;
    return *actual == 0;
}

__attribute__((noreturn)) void stage5_main(uint64_t self, uint64_t send_cap,
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
    if (prima_call(SYS_ABI_CHECK, 1, 0, 0) != ABI_VERSION ||
        prima_call(SYS_ABI_CHECK, 2, 0, 0) != ERR_INVALID ||
        prima_call(SYS_ABI_CHECK, 1, 1, 0) != ERR_INVALID ||
        prima_call(SYS_ABI_CHECK, 1, 0, 1) != ERR_INVALID)
        goto failed;
    uint64_t id = prima_call(SYS_TASK_ID, 0, 0, 0);
    if (prima_call(SYS_SEND, manager_id, send_cap, PROBE_TAG | (id & 0xff)) == 0)
        prima_call(SYS_TASK_DESTROY, self, 0, 0);
    /* A failed exit remains blocked so the manager's QEMU witness fails. */
failed:
    for (;;)
        prima_call(SYS_RECV, (uint64_t)received, 0, 0);
}
