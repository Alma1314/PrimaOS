/* M16b-3：跨线程共享内存可见性最小探针（Prima 原生）。 */
#include <stdint.h>
#include <unistd.h>
extern long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);
#define SYS_TASK_DESTROY 20
#define SYS_WAIT_CREATE 42
#define SYS_WAIT_OBJECTS 44
#define SYS_WAIT_SIGNAL 45
#define SYS_WAIT_DESTROY 46
#define SYS_THREAD_CREATE 47
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
static volatile int g_word;
static long g_cap;
static unsigned char g_stack[16384] __attribute__((aligned(16)));
static void worker(unsigned long self) {
    g_word = 0x5a;
    if (g_cap > 0)
        (void)prima_native(SYS_WAIT_SIGNAL, g_cap, 0, 0, 0, 0, 0);
    (void)prima_native(SYS_TASK_DESTROY, (long)self, 0, 0, 0, 0, 0);
    for (;;) { }
}
int main(void) {
    emit("SHARE");
    long cap = prima_native(SYS_WAIT_CREATE, 0, 0, 0, 0, 0, 0);
    field("create", cap > 0, cap);
    g_cap = cap;
    g_word = 0;
    long tid = cap > 0 ? prima_native(SYS_THREAD_CREATE, (long)&worker,
                                      (long)(g_stack + sizeof g_stack), 0, 0, 0, 0)
                       : -1;
    field("thread", tid > 0, tid);
    long waited = cap > 0 ? prima_native(SYS_WAIT_OBJECTS, cap, 0, 0, 0, 0,
                                         (long)0x7fffffffffffffffL) : -1;
    field("wake", waited == 0, waited);
    field("word", g_word == 0x5a, g_word);
    long destroyed = cap > 0 ? prima_native(SYS_WAIT_DESTROY, cap, 0, 0, 0, 0, 0) : -1;
    field("destroy", destroyed == 0, destroyed);
    emit("|ok="); emit_dec(failures ? 0 : 1); emit(";err="); emit_dec(first_error); emit("\n");
    return failures ? 1 : 0;
}
