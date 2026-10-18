/* M3 Prima crt：把内核初始栈交给 musl 的 `__libc_start_main`。
 *
 * 不使用 musl 的 `rcrt1.o`：内核 ELF 装载器已经消费了全部 `R_X86_64_RELATIVE`
 * 重定位，`rcrt1` 的 `_dlstart_c` 会再次应用并破坏 GOT/数据段。
 */
int main();
__attribute__((weak)) void _init();
__attribute__((weak)) void _fini();
int __libc_start_main(int (*)(), int, char **, void (*)(), void (*)(), void (*)());

void prima_start_c(long *stack) {
    __libc_start_main(main, (int)stack[0], (void *)(stack + 1), _init, _fini, 0);
}
