/* M17c：exec_replace 的最小探针。 */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
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
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
