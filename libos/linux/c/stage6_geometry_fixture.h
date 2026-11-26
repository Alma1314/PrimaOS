/* S6-M2: frozen busybox PT_LOAD geometry, synthetic bytes and native entry.
 * No busybox/libc/TLS execution is claimed. Keep the four tuples synchronized
 * with compat/stage6-static-whitelist.md; file padding is deliberately nonzero.
 */
#define GEOMETRY_LENGTH 0x205000UL
extern const unsigned char geometry_entry[], geometry_end[];
__asm__(
    ".pushsection .rodata\n"
    ".global geometry_entry, geometry_end\n"
    "geometry_entry:\n"
    "cld\n"
    /* All bytes of the third segment and initialized fourth segment. */
    "mov $0x5a9000, %esi\n mov $0x521e5, %ecx\n"
    "1: cmpb $0xa5, (%rsi)\n jne 9f\n inc %rsi\n loop 1b\n"
    "mov $0x5fc000, %esi\n mov $0x9d88, %ecx\n"
    "2: cmpb $0xa5, (%rsi)\n jne 9f\n inc %rsi\n loop 2b\n"
    /* BSS begins in the last file page; nonzero file padding must be ignored. */
    "mov $0x605d88, %esi\n mov $0x7380, %ecx\n"
    "3: cmpb $0, (%rsi)\n jne 9f\n inc %rsi\n loop 3b\n"
    "movq $42, 0x60d100\n cmpq $42, 0x60d100\n jne 9f\n"
    /* Eight stack pages, aligned entry, argc/argv/env layout. Verify all bytes
     * below initial rsp are zero, then write/read the bottom of each page. */
    "test $15, %rsp\n jnz 8f\n cmpq $1, (%rsp)\n jne 8f\n"
    "mov 8(%rsp), %rsi\n cmpb $103, (%rsi)\n jne 8f\n"
    "cmpq $0, 16(%rsp)\n jne 8f\n cmpq $0, 24(%rsp)\n jne 8f\n"
    "mov %rsp, %rsi\n and $-4096, %rsi\n sub $28672, %rsi\n"
#ifndef PRIMA_HOST
    "movabs $0x7f0000000000, %rax\n cmp %rax, %rsi\n jne 8f\n"
#endif
    "mov %rsi, %rbx\n"
    "4: cmpb $0, (%rsi)\n jne 8f\n inc %rsi\n cmp %rsp, %rsi\n jb 4b\n"
    "mov $8, %ecx\n"
    "5: movq $42, (%rbx)\n cmpq $42, (%rbx)\n jne 8f\n add $4096, %rbx\n loop 5b\n"
#ifndef PRIMA_HOST
    /* The original file source is now unmapped; release its four segments. */
    "mov $0x400300, %r12d\n mov $4, %ebx\n"
    "6: mov (%r12), %rdi\n mov $16, %eax\n syscall\n test %rax, %rax\n jnz 9f\n"
    "mov (%r12), %rdi\n mov $12, %eax\n syscall\n test %rax, %rax\n jnz 9f\n"
    "add $8, %r12\n dec %ebx\n jnz 6b\n"
#endif
    "mov $42, %edi\n jmp 7f\n"
    "8: mov $132, %edi\n jmp 7f\n"
    "9: mov $131, %edi\n"
    "7:\n"
#ifdef PRIMA_HOST
    "mov $60, %eax\n"
#else
    "mov $54, %eax\n"
#endif
    "syscall\n jmp .\n"
    "geometry_end:\n .popsection\n"
);

static void geometry_child(void) {
    static const uint64_t segments[4][5] = {
        {0, 0x400000, 0x6c8, 0x6c8, 4},
        {0x1000, 0x401000, 0x1a7865, 0x1a7865, 5},
        {0x1a9000, 0x5a9000, 0x521e5, 0x521e5, 4},
        {0x1fb820, 0x5fc820, 0x9568, 0x108e8, 6},
    };
    unsigned char *base;
#ifndef PRIMA_HOST
    long self = prima_native(63, 5, 0, 0, 0, 0, 0);
    long caps[4];
    base = (unsigned char *)UINT64_C(0x00000e0000000000);
    if (self < 0) _exit(133);
    for (unsigned i = 0; i < 4; i++) {
        unsigned pages = (GEOMETRY_LENGTH + 4095) / 4096 - i * 149;
        if (pages > 149) pages = 149;
        caps[i] = prima_native(13, pages, 6, 0, 0, 0, 0);
        if (caps[i] < 0 || prima_native(14, caps[i], self, 0,
            (long)(base + i * 149UL * 4096), pages, 6) != 0) _exit(134);
    }
#else
    base = mmap(0, GEOMETRY_LENGTH, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (base == MAP_FAILED) _exit(133);
#endif
    memset(base, 0xa5, GEOMETRY_LENGTH);
    memcpy(base, tiny_elf, 64);
    put64(base + 24, 0x401000);
    base[56] = 4;
    for (unsigned i = 0; i < 4; i++) {
        unsigned char *header = base + 64 + i * 56;
        memset(header, 0, 56);
        header[0] = 1; header[4] = segments[i][4];
        put64(header + 8, segments[i][0]);
        put64(header + 16, segments[i][1]);
        put64(header + 32, segments[i][2]);
        put64(header + 40, segments[i][3]);
        put64(header + 48, 4096);
    }
#ifndef PRIMA_HOST
    for (unsigned i = 0; i < 4; i++) put64(base + 0x300 + i * 8, caps[i]);
#endif
    memcpy(base + 0x1000, geometry_entry, (unsigned long)(geometry_end - geometry_entry));
    char *args[1] = {"geometry"};
    (void)exec_fixture(base, GEOMETRY_LENGTH, args, 1);
    _exit(135);
}
