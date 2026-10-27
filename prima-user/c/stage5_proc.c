/* Fork continuation through musl syscall(), fork(), and the raw C adapter.
 * Parent writes before waiting; child must inherit the pre-fork stack/data/TLS.
 * No inline trap or native-fork bypass is permitted in this probe. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/wait.h>
#ifdef PRIMA_HOST
#include <stdlib.h>
#endif
#ifndef PRIMA_HOST
#define PRIMA_ABI_NO_IOVEC
#include "prima_syscall.h"
extern long prima_native(long, long, long, long, long, long, long);
#endif
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
static volatile unsigned long g_value;
static unsigned mxcsr(void) { unsigned value; __asm__ volatile("stmxcsr %0" : "=m"(value)); return value; }
static void set_mxcsr(unsigned value) { __asm__ volatile("ldmxcsr %0" :: "m"(value)); }
static unsigned short fcw(void) { unsigned short value; __asm__ volatile("fnstcw %0" : "=m"(value)); return value; }
static void set_fcw(unsigned short value) { __asm__ volatile("fldcw %0" :: "m"(value)); }
__attribute__((noinline)) static long fork_path(int mode) {
    if (mode == 1)
        return fork();
#ifndef PRIMA_HOST
    if (mode == 2)
        return __syscall0(57L);
#endif
    return syscall(57L);
}
int main(void) {
    int forks=1, waits=1, statuses=1, snapshots=1, guards=1, errors=1, fpu=1, isolate=1, vfs=1;
    int parent_pid = getpid();
    int nullfd = open("/dev/null", O_RDONLY);
    isolate = nullfd >= 0;
#ifdef PRIMA_HOST
    char file_path[] = "/tmp/prima-fork-XXXXXX";
    int shared_fd = mkstemp(file_path);
#else
    const char *file_path = "/tmp/fork-data";
    int shared_fd = open(file_path, O_CREAT | O_TRUNC | O_RDWR, 0666);
#endif
    vfs = shared_fd >= 0 && write(shared_fd, "abc", 3) == 3 && lseek(shared_fd, 0, SEEK_SET) == 0;
    emit("PROC");
    for (int mode=0; mode<3; mode++) {
        volatile unsigned long stack[64];
        for (unsigned i=0; i<64; i++) stack[i]=0x12340000UL+i+(unsigned)mode;
        g_value=0xabcdef00UL+(unsigned)mode;
        errno=77;
        unsigned old_mxcsr=mxcsr(), inherited_mxcsr=(old_mxcsr & ~0x6000u) | 0x2000u;
        unsigned short old_fcw=fcw(), inherited_fcw=(unsigned short)((old_fcw & ~0x0c00u) | 0x0400u);
        set_mxcsr(inherited_mxcsr); set_fcw(inherited_fcw);
        long pid=fork_path(mode);
        if (pid==0) {
            int ok=g_value==0xabcdef00UL+(unsigned)mode && errno==77 &&
                mxcsr()==inherited_mxcsr && fcw()==inherited_fcw;
            for (unsigned i=0; i<64; i++) ok=ok && stack[i]==0x12340000UL+i+(unsigned)mode;
            ok = ok && getppid() == parent_pid && nullfd >= 0 && close(nullfd) == 0;
#ifndef PRIMA_HOST
            long copied = prima_native(PRIMA_SYS_CAP_DERIVE, (long)prima_ctx.self_cap, 1L << 5, 0, 0, 0, 0);
            ok = ok && copied > 0 && prima_native(PRIMA_SYS_CAP_DROP, copied, 0, 0, 0, 0, 0) == 0;
            ok = ok && prima_native(PRIMA_SYS_CAP_DROP, (long)prima_ctx.send_cap, 0, 0, 0, 0, 0) == 0;
#endif
            char inherited = 0, fresh = 0;
            ok = ok && shared_fd >= 0 && read(shared_fd, &inherited, 1) == 1 && inherited == 'a' + mode;
            int own_fd = open(file_path, O_RDONLY);
            ok = ok && own_fd >= 0 && read(own_fd, &fresh, 1) == 1 && fresh == 'a';
            if (own_fd >= 0) ok = ok && close(own_fd) == 0;
            ok = ok && close(shared_fd) == 0;
            stack[0]=0;
            g_value=0;
            errno=88;
            _exit(ok ? 7+mode : 99);
        }
        forks=forks && pid>0;
        if (mode == 2 && shared_fd >= 0) {
            vfs = vfs && close(shared_fd) == 0;
            shared_fd = -1;
        }
        set_mxcsr((old_mxcsr & ~0x6000u) | 0x4000u);
        set_fcw((unsigned short)((old_fcw & ~0x0c00u) | 0x0800u));
        g_value=0x98760000UL+(unsigned)mode;
        for (unsigned i=0; i<64; i++) stack[i]=0x56780000UL+i+(unsigned)mode;
        struct { int status; uint32_t guard; } result={-1,UINT32_C(0xfacefeed)};
        long waited=pid>0 ? (mode==1 ? waitpid((pid_t)pid,&result.status,0) : syscall(61L,pid,(long)&result.status,0L,0L)) : -1;
        waits=waits && pid>0 && waited==pid;
        statuses=statuses && ((result.status>>8)&0xff)==7+mode;
        vfs = vfs && ((result.status>>8)&0xff)==7+mode;
        if (shared_fd >= 0) vfs = vfs && lseek(shared_fd, 0, SEEK_CUR) == mode + 1;
        char byte;
        isolate=isolate && ((result.status>>8)&0xff)==7+mode && read(nullfd,&byte,1)==0;
        guards=guards && result.guard==UINT32_C(0xfacefeed);
        fpu=fpu && mxcsr()==((old_mxcsr & ~0x6000u) | 0x4000u) &&
            fcw()==(unsigned short)((old_fcw & ~0x0c00u) | 0x0800u);
        set_mxcsr(old_mxcsr); set_fcw(old_fcw);
        snapshots=snapshots && g_value==0x98760000UL+(unsigned)mode && errno==77;
        for (unsigned i=0; i<64; i++) snapshots=snapshots && stack[i]==0x56780000UL+i+(unsigned)mode;
        errno=0;
        long again=syscall(61L,-1L,0L,0L,0L);
        errors=errors && again==-1 && errno==ECHILD;
    }
    if (shared_fd >= 0) (void)close(shared_fd);
    (void)unlink(file_path);
    if (nullfd >= 0) (void)close(nullfd);
    field("fork",forks,1); field("wait",waits,2); field("status",statuses,3);
    field("cow",snapshots,4); field("echild",errors,5); field("guard",guards,6);
    field("fpu",fpu,7); field("isolate",isolate,8); field("vfs",vfs,9);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
