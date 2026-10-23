/* M18b：sigaction + handler 递送 + sigreturn 的最小探针。 */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <unistd.h>
extern long syscall(long number, ...);
#ifndef PRIMA_HOST
extern long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);
#else
#include <signal.h>
#endif
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
static volatile long g_got;
#ifdef PRIMA_HOST
static void handler(int sig) { g_got = sig; }
#else
static void handler(long sig) {
    g_got = sig;
    (void)prima_native(58, 0, 0, 0, 0, 0, 0);
}
#endif
int main(void) {
    emit("SIGACT");
#ifdef PRIMA_HOST
    struct sigaction sa;
    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    sa.sa_flags = 0;
    long r = sigaction(SIGUSR1, &sa, 0);
#else
    long r = prima_native(57, 10, (long)&handler, 0, 0, 0, 0);
#endif
    field("sigaction", r == 0, r);
#ifdef PRIMA_HOST
    long k = kill(getpid(), SIGUSR1);
#else
    long self = prima_native(7, 0, 0, 0, 0, 0, 0);
    long k = prima_native(56, self, 10, 0, 0, 0, 0);
#endif
    field("kill", k == 0, k);
    field("ran", g_got == 10, g_got);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
