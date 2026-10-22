/* M17b：fork/wait/COW 隔离/退出状态的最小同源探针。
 *
 * Prima：raw_fork() 用内联 asm 直接发 SYS_FORK=51（子进程从最小 asm 续执行，
 * 避开 musl/适配层包装的返回路径），子进程用 SYS_PROCESS_EXIT(54) 以 7 退出。
 * Linux 参考用真实 fork(57)/wait4(61)/_exit(7)。
 */
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
static volatile int g_child;
int main(void) {
    emit("PROC");
#ifdef PRIMA_HOST
    long pid = syscall(57L);
#else
    long pid = raw_fork();
#endif
    if (pid == 0) {
        g_child = 1;
#ifdef PRIMA_HOST
        _exit(7);
#else
        (void)prima_native(54, 7, 0, 0, 0, 0, 0);
        for (;;) {
        }
#endif
    }
    int status = 0;
    errno = 0;
    long waited = pid > 0 ? syscall(61L, pid, (long)&status, 0L, 0L) : -1;
    field("fork", pid > 0, pid);
    field("wait", waited == pid, waited);
    field("status", ((status >> 8) & 0xff) == 7, status);
    field("cow", g_child == 0, g_child);
    errno = 0;
    long again = syscall(61L, -1L, (long)&status, 0L, 0L);
    field("echild", again == -1 && errno == ECHILD, errno);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
