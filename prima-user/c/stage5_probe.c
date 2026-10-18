/* First freestanding C task: Prima's current five-register entry, no libc. */
#include <stdint.h>

#define SYS_TASK_ID 7
#define SYS_SEND 17
#define SYS_RECV 18
#define SYS_TASK_DESTROY 20
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

__attribute__((noreturn)) void _start(uint64_t self, uint64_t send_cap,
                                      uint64_t code_cap, uint64_t name,
                                      uint64_t manager_id) {
    (void)code_cap;
    (void)name;
    uint64_t id = prima_call(SYS_TASK_ID, 0, 0, 0);
    if (prima_call(SYS_SEND, manager_id, send_cap, PROBE_TAG | (id & 0xff)) == 0)
        prima_call(SYS_TASK_DESTROY, self, 0, 0);
    /* A failed exit remains blocked so the manager's QEMU witness fails. */
    uint64_t received[3];
    for (;;)
        prima_call(SYS_RECV, (uint64_t)received, 0, 0);
}
