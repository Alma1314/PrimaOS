/* Fork continuation through musl syscall(), fork(), and the raw C adapter.
 * Parent writes before waiting; child must inherit the pre-fork stack/data/TLS.
 * No inline trap or native-fork bypass is permitted in this probe. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <unistd.h>
#include <sys/wait.h>
#ifndef PRIMA_HOST
extern long __syscall0(long);
#endif
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
static volatile unsigned long g_value;
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
    int forks=1, waits=1, statuses=1, snapshots=1, guards=1, errors=1;
    emit("PROC");
    for (int mode=0; mode<3; mode++) {
        volatile unsigned long stack[64];
        for (unsigned i=0; i<64; i++) stack[i]=0x12340000UL+i+(unsigned)mode;
        g_value=0xabcdef00UL+(unsigned)mode;
        errno=77;
        long pid=fork_path(mode);
        if (pid==0) {
            int ok=g_value==0xabcdef00UL+(unsigned)mode && errno==77;
            for (unsigned i=0; i<64; i++) ok=ok && stack[i]==0x12340000UL+i+(unsigned)mode;
            stack[0]=0;
            g_value=0;
            errno=88;
            _exit(ok ? 7+mode : 99);
        }
        forks=forks && pid>0;
        g_value=0x98760000UL+(unsigned)mode;
        for (unsigned i=0; i<64; i++) stack[i]=0x56780000UL+i+(unsigned)mode;
        struct { int status; uint32_t guard; } result={-1,UINT32_C(0xfacefeed)};
        long waited=pid>0 ? (mode==1 ? waitpid((pid_t)pid,&result.status,0) : syscall(61L,pid,(long)&result.status,0L,0L)) : -1;
        waits=waits && pid>0 && waited==pid;
        statuses=statuses && ((result.status>>8)&0xff)==7+mode;
        guards=guards && result.guard==UINT32_C(0xfacefeed);
        snapshots=snapshots && g_value==0x98760000UL+(unsigned)mode && errno==77;
        for (unsigned i=0; i<64; i++) snapshots=snapshots && stack[i]==0x56780000UL+i+(unsigned)mode;
        errno=0;
        long again=syscall(61L,-1L,0L,0L,0L);
        errors=errors && again==-1 && errno==ECHILD;
    }
    field("fork",forks,1); field("wait",waits,2); field("status",statuses,3);
    field("cow",snapshots,4); field("echild",errors,5); field("guard",guards,6);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
