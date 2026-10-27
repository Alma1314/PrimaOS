/* M3 Prima crt：把内核初始栈交给 musl 的 `__libc_start_main`。
 *
 * 不使用 musl 的 `rcrt1.o`：内核 ELF 装载器已经消费了全部 `R_X86_64_RELATIVE`
 * 重定位，`rcrt1` 的 `_dlstart_c` 会再次应用并破坏 GOT/数据段。
 */
#include "prima_contract.h"
#ifdef PRIMA_ECHO_HARNESS
#include "prima_abi.h"
int prima_echo_harness(void);
long __syscall1(long number, long a0);
#endif

int main();
__attribute__((weak)) void _init();
__attribute__((weak)) void _fini();
int __libc_start_main(int (*)(), int, char **, void (*)(), void (*)(), void (*)());

void prima_exec_restore(void);
void prima_start_c(long *stack) {
    const uint32_t expected[11] = PRIMA_CONTRACT_WORDS;
    for (unsigned i=0; i<11; i++)
        if (prima_libc_contract_v1_0[i] != expected[i]) __builtin_trap();
    prima_exec_restore();
#ifdef PRIMA_ECHO_HARNESS
    int harness = prima_echo_harness();
    if (harness != 0)
        (void)__syscall1(PRIMA_LINUX_SYS_exit, 70 + harness);
#endif
    __libc_start_main(main, (int)stack[0], (void *)(stack + 1), _init, _fini, 0);
}
