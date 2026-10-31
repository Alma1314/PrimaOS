/* M17c：exec_replace 的最小探针。 */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
extern long syscall(long number, ...);
#ifndef PRIMA_HOST
extern long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);
#endif
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}

#include "stage5_exec_fixture.h"

/* S6-M2: only five source pages exist on Prima. The program header straddles
 * pages beyond the old 4 KiB prefix; the two payload pages are beyond 2 MiB.
 * Linux uses a contiguous virtual buffer to write the same sparse file to memfd.
 * Its custom entry reads page one and executes page two, then exits with 42.
 */
#define STREAM_PHDR 0x2ff0UL
#define STREAM_DATA 0x201000UL
#define STREAM_LENGTH (STREAM_DATA + 8192UL)
static void put64(unsigned char *p, uint64_t value) {
    for (unsigned i = 0; i < 8; i++) p[i] = (unsigned char)(value >> (8 * i));
}
#ifndef PRIMA_HOST
static const unsigned long stream_offsets[5] = {0, 0x2000, 0x3000, STREAM_DATA, STREAM_DATA + 4096};
static long stream_map(long cap, long self, unsigned char *base, unsigned index) {
    /* SYS_MEMSEG_MAP(cap, task, segment-page, VA, count, R|W). */
    return prima_native(14, cap, self, index, (long)(base + stream_offsets[index]), 1, 6);
}
#endif
static void stream_child(void) {
    char *args[2] = {"stream", "x"};
    unsigned char *base;
#ifndef PRIMA_HOST
    long self = prima_native(63, 5, 0, 0, 0, 0, 0);
    long cap = prima_native(13, 5, 6, 0, 0, 0, 0);
    if (self < 0 || cap < 0) _exit(110);
    base = (unsigned char *)UINT64_C(0x00000e0000000000);
    for (unsigned i = 0; i < 5; i++) {
        long result = stream_map(cap, self, base, i);
        if (result != 0) {
            emit("STREAM map index="); emit_dec(i); emit(";error="); emit_dec(result); emit("\n");
            _exit(111);
        }
    }
#else
    base = mmap(0, STREAM_LENGTH, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (base == MAP_FAILED) _exit(110);
#endif
    memcpy(base, tiny_elf, 64);
    put64(base + 24, 0x401000); /* entry on the second payload page */
    put64(base + 32, STREAM_PHDR);
    base[56] = 1;
    unsigned char phdr[56] = {0};
    phdr[0] = 1; phdr[4] = 5; /* PT_LOAD, R|X */
    put64(phdr + 8, STREAM_DATA);
    put64(phdr + 16, 0x400000);
    put64(phdr + 32, 8192);
    put64(phdr + 40, 8192);
    put64(phdr + 48, 4096);
    memcpy(base + STREAM_PHDR, phdr, sizeof phdr);
    base[STREAM_DATA] = 42;
    unsigned char *code = base + STREAM_DATA + 4096;
#ifndef PRIMA_HOST
    /* Release the source Segment after exec unmapped it; avoid fixture leaks. */
    code[0] = 0x48; code[1] = 0xbf; put64(code + 2, (uint64_t)cap);
    const unsigned char destroy[] = {0xb8,16,0,0,0,0x0f,0x05};
    memcpy(code + 10, destroy, sizeof destroy);
    code += 17;
#endif
    const unsigned char exit_code[] = {
        0x0f,0xb6,0x3c,0x25,0,0,0x40,0, /* movzbl 0x400000, edi */
        0xb8,TINY_EXIT_SYSCALL,0,0,0,0x0f,0x05,0xeb,0xfe
    };
    memcpy(code, exit_code, sizeof exit_code);
#ifndef PRIMA_HOST
    volatile uint64_t guard = UINT64_C(0x123456789abcdef0);
    /* Declared address+length overflow is EFAULT without replacing this image. */
    if (exec_fixture(base, (unsigned long)-1, args, 2) != -EFAULT) _exit(112);
    /* An unreadable half of the program header remains a source fault. */
    if (prima_native(15, self, (long)(base + 0x3000), 1, 0, 0, 0) != 0 ||
        exec_fixture(base, STREAM_LENGTH, args, 2) != -EFAULT ||
        stream_map(cap, self, base, 2) != 0) _exit(113);
    /* Fault after the first payload page has been copied. Repeat to exercise
     * staging rollback; then restore the page and exec in the same Task. */
    if (prima_native(15, self, (long)(base + STREAM_DATA + 4096), 1, 0, 0, 0) != 0) _exit(114);
    for (int retry = 0; retry < 2; retry++) {
        if (exec_fixture(base, STREAM_LENGTH, args, 2) != -EFAULT ||
            guard != UINT64_C(0x123456789abcdef0) || base[STREAM_DATA] != 42) _exit(115);
    }
    if (stream_map(cap, self, base, 4) != 0) _exit(116);
#endif
    (void)exec_fixture(base, STREAM_LENGTH, args, 2);
    _exit(117);
}

/* Full-sized input and replacement: 526 load pages + eight stack pages.
 * The entry verifies a distinct u64 at the end of EVERY load page. On the first
 * run (argc=2) it execs the entire mapped image again; argc=1 then exits 42.
 * This keeps the old large image alive while the new one is built.
 */
#define LARGE_PAGES 526UL
#define LARGE_LENGTH (LARGE_PAGES * 4096UL)
#define LARGE_BASE 0x400000UL
#define LARGE_ENTRY (LARGE_BASE + (LARGE_PAGES - 1) * 4096UL + 16)
static void large_child(void) {
    char *args[2] = {"large", "again"};
    unsigned char *base;
#ifndef PRIMA_HOST
    long self = prima_native(63, 5, 0, 0, 0, 0, 0);
    long caps[4] = {0};
    base = (unsigned char *)UINT64_C(0x00000e0000000000);
    if (self < 0) _exit(120);
    for (unsigned i = 0; i < 4; i++) {
        unsigned pages = LARGE_PAGES - i * 149;
        if (pages > 149) pages = 149;
        caps[i] = prima_native(13, pages, 6, 0, 0, 0, 0);
        if (caps[i] < 0 || prima_native(14, caps[i], self, 0,
            (long)(base + i * 149UL * 4096), pages, 6) != 0) _exit(121);
    }
#else
    base = mmap(0, LARGE_LENGTH, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (base == MAP_FAILED) _exit(120);
#endif
    memcpy(base, tiny_elf, 120);
    put64(base + 24, LARGE_ENTRY);
    base[56] = 1;
    put64(base + 64 + 8, 0);
    put64(base + 64 + 16, LARGE_BASE);
    put64(base + 64 + 32, LARGE_LENGTH);
    put64(base + 64 + 40, LARGE_LENGTH);
    for (unsigned long page = 0; page < LARGE_PAGES; page++) put64(base + (page + 1) * 4096 - 8, page + 1);
    put64(base + 0x180, LARGE_BASE + 0x1a0);
    put64(base + 0x188, 0);
    memcpy(base + 0x1a0, "large", 6);
    unsigned char *code = base + (LARGE_PAGES - 1) * 4096 + 16;
    /* mov esi, first-page-tail; mov ecx, count; mov edx, expected;
     * cmp [rsi],rdx / jne fail; add rsi,4096; inc edx; dec ecx; jne loop. */
    const unsigned char verify[] = {
        0xbe,0xf8,0x0f,0x40,0, 0xb9,0x0e,2,0,0, 0xba,1,0,0,0,
        0x48,0x39,0x16, 0x75,0x1b, 0x48,0x81,0xc6,0,0x10,0,0,
        0xff,0xc2,0xff,0xc9,0x75,0xee,
        /* cmp qword [rsp],2; je first-run; mov edi,42; jmp exit */
        0x48,0x83,0x3c,0x24,2, 0x74,0x15, 0xbf,42,0,0,0,0xeb,5,
        /* fail: mov edi,118; exit: mov eax,exit; syscall; spin */
        0xbf,118,0,0,0, 0xb8,TINY_EXIT_SYSCALL,0,0,0,0x0f,0x05,0xeb,0xfe
    };
    memcpy(code, verify, sizeof verify);
    code += sizeof verify;
#ifndef PRIMA_HOST
    /* Only the first run owns the original, now unmapped input Segments. */
    for (unsigned i = 0; i < 4; i++) {
        code[0] = 0x48; code[1] = 0xbf; put64(code + 2, (uint64_t)caps[i]);
        const unsigned char destroy_drop[] = {0xb8,16,0,0,0,0x0f,0x05,0xb8,12,0,0,0,0x0f,0x05};
        memcpy(code + 10, destroy_drop, sizeof destroy_drop);
        code += 24;
    }
    /* Native exec uses the still mapped RX image as its complete file source. */
    const unsigned char again[] = {
        0xbf,0,0,0x40,0, /* rdi=image */
        0xbe,0,0xe0,0x20,0, /* rsi=526*4096 */
        0xba,0x80,1,0x40,0, /* rdx=argv */
        0x41,0xba,1,0,0,0, /* r10=argc */
        0x45,0x31,0xc0,0x45,0x31,0xc9, /* r8=r9=0 */
        0xb8,55,0,0,0,0x0f,0x05,
        0xbf,119,0,0,0,0xb8,54,0,0,0,0x0f,0x05,0xeb,0xfe
    };
    memcpy(code, again, sizeof again);
    volatile uint64_t guard = UINT64_C(0x7edcba9876543210);
    /* Public Segment size remains 149, and total exec size is capped at 640. */
    if (prima_native(13, 150, 6, 0, 0, 0, 0) != -ENOENT) _exit(122);
    put64(base + 64 + 40, 633UL * 4096);
    if (exec_fixture(base, LARGE_LENGTH, args, 2) != -ENOENT) _exit(123);
    put64(base + 64 + 40, LARGE_LENGTH);
    /* Force a partial data-frame allocation at the real global quota. */
    long pressure[6];
    for (unsigned i = 0; i < 6; i++) {
        pressure[i] = prima_native(13, 149, 6, 0, 0, 0, 0);
        if (pressure[i] < 0) _exit(124);
    }
    if (exec_fixture(base, LARGE_LENGTH, args, 2) != -ENOSPC) _exit(125);
    for (unsigned i = 0; i < 6; i++) {
        if (prima_native(16, pressure[i], 0, 0, 0, 0, 0) != 0 ||
            prima_native(12, pressure[i], 0, 0, 0, 0, 0) != 0) _exit(126);
    }
    /* Four failures exceed the metadata pool if one 534-page run leaks. */
    unsigned long missing = 149UL * 4096;
    if (prima_native(15, self, (long)(base + missing), 1, 0, 0, 0) != 0) _exit(127);
    for (unsigned i = 0; i < 4; i++) {
        if (exec_fixture(base, LARGE_LENGTH, args, 2) != -EFAULT ||
            guard != UINT64_C(0x7edcba9876543210) ||
            *(volatile uint64_t *)(base + 4096 - 8) != 1) _exit(128);
    }
    if (prima_native(14, caps[1], self, 0, (long)(base + missing), 1, 6) != 0) _exit(129);
#else
    /* Linux reference: same 526-page content check, then exit (native 55 is
     * not a Linux control interface). Large-to-large re-exec is Prima-only. */
    const unsigned char host_exit[] = {0xbf,42,0,0,0,0xb8,60,0,0,0,0x0f,0x05,0xeb,0xfe};
    memcpy(code, host_exit, sizeof host_exit);
#endif
    (void)exec_fixture(base, LARGE_LENGTH, args, 2);
    _exit(130);
}

int main(void) {
    emit("EXEC");
    long pid = fork();
    if (pid == 0) {
        char *argv[2] = {"tiny", "x"};
        long r = exec_fixture(tiny_elf, sizeof tiny_elf, argv, 2);
        _exit((int)(r < 0 ? 100 + (-r) : 99));
    }
    int status = 0;
    errno = 0;
    long waited = pid > 0 ? syscall(61L, pid, (long)&status, 0L, 0L) : -1;
    field("exec", ((status >> 8) & 0xff) == 2, status);
    field("wait", waited == pid, waited);
    long bad = fork();
    if (bad == 0) {
        char *argv[1] = {"tiny"};
        long r = exec_fixture(tiny_elf, 8, argv, 1);
        _exit(r == -8 ? 1 : 2);
    }
    int bad_status = 0;
    (void)syscall(61L, bad, (long)&bad_status, 0L, 0L);
    field("bad", ((bad_status >> 8) & 0xff) == 1, bad_status);
    long streamed = fork();
    if (streamed == 0) stream_child();
    int stream_status = 0;
    long stream_waited = streamed > 0 ? syscall(61L, streamed, (long)&stream_status, 0L, 0L) : -1;
    field("stream", stream_waited == streamed && streamed > 0 &&
          (stream_status & 0x7f) == 0 && ((stream_status >> 8) & 0xff) == 42, stream_status);
    long large = fork();
    if (large == 0) large_child();
    int large_status = 0;
    long large_waited = large > 0 ? syscall(61L, large, (long)&large_status, 0L, 0L) : -1;
    field("large", large > 0 && large_waited == large && (large_status & 0x7f) == 0 &&
          ((large_status >> 8) & 0xff) == 42, large_status);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
