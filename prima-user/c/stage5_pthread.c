#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <sched.h>
#include <signal.h>
#include <unistd.h>
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
static volatile int g_value;
static pthread_t g_self;
static int g_pid;
static volatile int g_same_pid;
static void *worker(void *arg){g_self=pthread_self();g_same_pid=getpid()==g_pid;g_value=(int)(intptr_t)arg;return (void*)(intptr_t)7;}
static volatile int cancel_ready, cleanup_count, detached_done;
static pthread_mutex_t cancel_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t cancel_cond = PTHREAD_COND_INITIALIZER;
static void cancelled(void *arg) { if (arg) (void)pthread_mutex_unlock(arg); __atomic_add_fetch(&cleanup_count, 1, __ATOMIC_SEQ_CST); }
static void *cancel_worker(void *arg) {
    int async = (int)(intptr_t)arg;
    if (async) (void)pthread_setcanceltype(PTHREAD_CANCEL_ASYNCHRONOUS, 0);
    pthread_cleanup_push(cancelled, async ? 0 : &cancel_mutex);
    if (async) {
        __atomic_store_n(&cancel_ready, 1, __ATOMIC_RELEASE);
        for (;;) (void)sched_yield();
    } else {
        (void)pthread_mutex_lock(&cancel_mutex);
        __atomic_store_n(&cancel_ready, 1, __ATOMIC_RELEASE);
        (void)pthread_cond_wait(&cancel_cond, &cancel_mutex);
        (void)pthread_mutex_unlock(&cancel_mutex);
    }
    pthread_cleanup_pop(0);
    return 0;
}
static int check_cancel(int async) {
    pthread_t task;
    cancel_ready = 0;
    if (pthread_create(&task, 0, cancel_worker, (void *)(intptr_t)async)) return 0;
    while (!__atomic_load_n(&cancel_ready, __ATOMIC_ACQUIRE)) (void)sched_yield();
    if (pthread_cancel(task)) return 0;
    void *result = 0;
    int joined = pthread_join(task, &result) == 0 && result == PTHREAD_CANCELED;
    return joined;
}
static void *detached_worker(void *arg) {
    (void)arg;
    __atomic_add_fetch(&detached_done, 1, __ATOMIC_RELEASE);
    return 0;
}
static int check_detached(void) {
    pthread_attr_t attr;
    if (pthread_attr_init(&attr) || pthread_attr_setstacksize(&attr, 16384) ||
        pthread_attr_setguardsize(&attr, 4096) ||
        pthread_attr_setdetachstate(&attr, PTHREAD_CREATE_DETACHED)) return 0;
    /* Twelve guarded mappings exceed the arena if detached stacks leak. */
    for (int i=0; i<12; i++) {
        pthread_t task;
        if (pthread_create(&task, &attr, detached_worker, 0)) return 0;
        while (__atomic_load_n(&detached_done, __ATOMIC_ACQUIRE) <= i) (void)sched_yield();
        (void)sched_yield();
    }
    return pthread_attr_destroy(&attr) == 0;
}
int main(void){
    g_pid=getpid();
    emit("PTHREAD");
    pthread_t th; int c=pthread_create(&th,0,worker,(void*)(intptr_t)42);
    field("create",c==0,c);
    void *res=0; int j=c==0?pthread_join(th,&res):-1;
    field("join",j==0,j); field("ret",res==(void*)(intptr_t)7,(long)(intptr_t)res); field("value",g_value==42,g_value);
    field("self",g_self==th && g_same_pid,(long)(intptr_t)g_self);
    int deferred = check_cancel(0);
    field("cancel", deferred, cleanup_count);
    int async = check_cancel(1);
    field("async", async && cleanup_count == 2, cleanup_count);
    field("detach", check_detached(), detached_done);
    pthread_attr_t attr;
    int attributes = pthread_attr_init(&attr) == 0;
    attributes = attributes && pthread_attr_setstacksize(&attr, 1) == EINVAL &&
                 pthread_attr_setdetachstate(&attr, -1) == EINVAL;
    if (attributes) attributes = pthread_attr_destroy(&attr) == 0;
    field("attr", attributes, errno);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
