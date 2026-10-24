/* M17c-4：VFS 读取镜像（父任务，已授权）→ fork → 子任务 SYS_EXEC_REPLACE(55)。
 *
 * 说明：fork 子任务没有 rootfs 的按任务授权，不能在子任务内做 VFS IPC；
 * 因此文件读取放在父任务（授权主体）完成，子任务只做 exec。
 */
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
static unsigned char image[4096];
int main(void) {
    emit("EXECVE");
    long fd = syscall(2L, (long)"/tiny.elf", 0L, 0L);
    long len = fd >= 0 ? syscall(0L, fd, (long)image, (long)sizeof image, 0L) : -1;
    if (fd >= 0) {
        (void)syscall(3L, fd, 0L, 0L);
    }
    field("read", len > 100, len);

    long pid = fork();
    if (pid == 0) {
        char *argv[2] = {"tiny", "x"};
        long r = prima_native(55, (long)image, (unsigned long)len, (long)argv, 2, 0, 0);
        (void)prima_native(54, (unsigned long)(r < 0 ? 1 : 2), 0, 0, 0, 0, 0);
        for (;;) {
        }
    }
    int status = 0;
    long waited = pid > 0 ? syscall(61L, pid, (long)&status, 0L, 0L) : -1;
    field("exec", ((status >> 8) & 0xff) == 2, status);
    field("wait", waited == pid, waited);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
