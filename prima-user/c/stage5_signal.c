/* M18a：最小信号（默认终止）——父进程 kill 子进程，wait4 观察信号状态。 */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <unistd.h>
extern long syscall(long number, ...);
#ifndef PRIMA_HOST
extern long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);
static long raw_fork(void) {
    long r;
    __asm__ volatile("syscall" : "=a"(r) : "a"(51L) : "rcx", "r11", "memory");
    return r;
}
#endif
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
int main(void) {
    emit("SIG");
#ifdef PRIMA_HOST
    long pid = syscall(57L);
#else
    long pid = raw_fork();
#endif
    if (pid == 0) {
        for (;;) {
        }
    }
    errno = 0;
    long killed = pid > 0 ? syscall(62L, pid, 15L) : -1;
    field("kill", killed == 0, killed);
    int status = 0;
    errno = 0;
    long waited = pid > 0 ? syscall(61L, pid, (long)&status, 0L, 0L) : -1;
    field("wait", waited == pid, waited);
    field("signal", (status & 0x7f) == 15 || ((status >> 8) & 0xff) == 143, status);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
