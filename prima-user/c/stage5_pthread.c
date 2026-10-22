#define _GNU_SOURCE
#include <errno.h>
#include <pthread.h>
#include <stdint.h>
#include <unistd.h>
static int failures, first_error = 0, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
static volatile int g_value;
static pthread_t g_self;
static void *worker(void *arg){g_self=pthread_self();g_value=(int)(intptr_t)arg;return (void*)(intptr_t)7;}
int main(void){
    emit("PTHREAD");
    pthread_t th; int c=pthread_create(&th,0,worker,(void*)(intptr_t)42);
    field("create",c==0,c);
    void *res=0; int j=c==0?pthread_join(th,&res):-1;
    field("join",j==0,j); field("ret",res==(void*)(intptr_t)7,(long)(intptr_t)res); field("value",g_value==42,g_value);
    field("self",g_self==th,(long)(intptr_t)g_self);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
