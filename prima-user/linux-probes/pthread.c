/* A normal Linux/glibc program: no Prima headers, hooks or syscall rewriting. */
#define _GNU_SOURCE
#include <errno.h>
#include <linux/futex.h>
#include <pthread.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/syscall.h>
#include <ucontext.h>
#include <unistd.h>

static _Thread_local unsigned long tls_word;
static pthread_t parent;
static atomic_int stop;
static volatile sig_atomic_t handled;
static volatile sig_atomic_t restart_phase, restarted, wait_word;

static void handler(int signal, siginfo_t *info, void *context) {
    (void)info;
    if (signal != SIGUSR1) return;
    handled = 1;
    if (restart_phase) {
        const greg_t *g = ((ucontext_t *)context)->uc_mcontext.gregs;
        const unsigned char *pc = (const void *)(uintptr_t)g[REG_RIP];
        if (g[REG_RAX] == SYS_futex && (uintptr_t)g[REG_RDI] == (uintptr_t)&wait_word
            && pc[0] == 0x0f && pc[1] == 0x05) {
            restarted = 1;
            wait_word = 1;
        }
    }
}

static void *worker(void *unused) {
    (void)unused;
    tls_word = 0x87654321;
    while (!atomic_load_explicit(&stop, memory_order_acquire)) {
        if (pthread_kill(parent, SIGUSR1) != 0 || tls_word != 0x87654321)
            return (void *)(uintptr_t)1;
        sched_yield();
    }
    return (void *)(uintptr_t)0x1234;
}

int main(void) {
    struct sigaction action = { .sa_sigaction = handler, .sa_flags = SA_SIGINFO };
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGUSR1, &action, NULL) != 0) return 1;
    tls_word = 0x12345678;
    parent = pthread_self();
    pthread_attr_t attr;
    pthread_t child;
    if (pthread_attr_init(&attr) != 0 || pthread_attr_setstacksize(&attr, 65536) != 0
        || pthread_create(&child, &attr, worker, NULL) != 0) return 2;
    pthread_attr_destroy(&attr);
    /* Repeated sends ensure a signal reaches a real parked WAIT even when a
       previous send arrived before its trap. No timeout or fixed EINTR stub. */
    int rc = syscall(SYS_futex, &wait_word, FUTEX_WAIT_PRIVATE, 0, NULL, NULL, 0);
    if (rc != -1 || errno != EINTR || !handled || tls_word != 0x12345678) return 3;
    atomic_store_explicit(&stop, 1, memory_order_release);
    void *value;
    if (pthread_join(child, &value) != 0 || value != (void *)(uintptr_t)0x1234
        || tls_word != 0x12345678) return 4;
    action.sa_flags |= SA_RESTART;
    if (sigaction(SIGUSR1, &action, NULL) != 0) return 5;
    restart_phase = 1;
    handled = 0;
    atomic_store_explicit(&stop, 0, memory_order_release);
    if (pthread_create(&child, NULL, worker, NULL) != 0) return 6;
    rc = syscall(SYS_futex, &wait_word, FUTEX_WAIT_PRIVATE, 0, NULL, NULL, 0);
    if (rc != -1 || errno != EAGAIN || !restarted || wait_word != 1) return 7;
    atomic_store_explicit(&stop, 1, memory_order_release);
    if (pthread_join(child, &value) != 0 || value != (void *)(uintptr_t)0x1234) return 8;
    puts("M4 pthread ok");
    return 0;
}
