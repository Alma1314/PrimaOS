/* Same-source libc signal actions, masks, siginfo and nested restoration. */
#define _GNU_SOURCE
#include <errno.h>
#include <stdint.h>
#include <unistd.h>
#include <signal.h>
#include <sys/wait.h>
#include <sys/mman.h>
#include <ucontext.h>
#include <time.h>
static int failures, first_error, first_field = 1;
static unsigned long text_len(const char *t){unsigned long n=0;while(t[n])n++;return n;}
static void emit(const char *t){ssize_t i=write(1,t,text_len(t));(void)i;}
static void emit_dec(long v){char d[20];int i=20;unsigned long m=v<0?(unsigned long)(-v):(unsigned long)v;do{d[--i]=(char)('0'+m%10);m/=10;}while(m);if(v<0)d[--i]='-';ssize_t g=write(1,&d[i],(unsigned long)(20-i));(void)g;}
static void field(const char *n,int p,long e){emit(first_field?"|":";");first_field=0;emit(n);emit("=");emit_dec(p?1:0);if(!p){failures=1;if(!first_error)first_error=(int)e;}}
static volatile sig_atomic_t got, nested, info_count;
static void handler(int slot) { got = slot; }
static void info_handler(int slot, siginfo_t *info, void *context) {
    if (slot == SIGUSR2 && info && info->si_signo == slot && info->si_pid == getpid() && context)
        info_count++;
    nested++;
    if (nested == 1) (void)kill(getpid(), SIGUSR2);
}
static volatile sig_atomic_t child_count, fault_count;
static volatile pid_t child_pid;
static void child_handler(int slot, siginfo_t *info, void *context) {
    (void)context;
    if (slot == SIGCHLD && info && info->si_code == CLD_EXITED && info->si_status == 7) {
        child_pid = info->si_pid;
        child_count++;
    }
}
static void fault_handler(int slot, siginfo_t *info, void *context) {
    ucontext_t *saved = context;
    if (!saved || !info || info->si_signo != slot) _exit(82);
    if (slot == SIGILL) { saved->uc_mcontext.gregs[REG_RIP] += 2; fault_count |= 1; }
    else if (slot == SIGSEGV) { saved->uc_mcontext.gregs[REG_RIP] += 3; fault_count |= 2; }
    else _exit(83);
}
static int check_wait_interrupt(int restart) {
    struct sigaction action = {0};
    action.sa_handler = handler;
    action.sa_flags = restart ? SA_RESTART : 0;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGUSR1, &action, 0)) return 0;
    got = 0;
    pid_t child = fork();
    if (!child) { (void)kill(getppid(), SIGUSR1); _exit(7); }
    if (child < 0) return 0;
    int status = 0;
    errno = 0;
    pid_t result = waitpid(child, &status, 0);
    int interrupted = result == -1 && errno == EINTR;
    if (interrupted) result = waitpid(child, &status, 0);
    return result == child && WIFEXITED(status) && WEXITSTATUS(status) == 7 &&
           got == SIGUSR1 && (restart ? !interrupted : interrupted);
}
static int check_sleep_interrupt(void) {
    struct sigaction action = {0};
    action.sa_handler = handler; action.sa_flags = SA_RESTART;
    sigemptyset(&action.sa_mask);
    if (sigaction(SIGUSR1, &action, 0)) return 0;
    got = 0;
    pid_t child = fork();
    if (!child) {
        struct timespec delay = {0, 20000000};
        (void)nanosleep(&delay, 0);
        (void)kill(getppid(), SIGUSR1);
        _exit(0);
    }
    if (child < 0) return 0;
    struct timespec request = {2, 0}, remaining = {0, 0};
    errno = 0;
    int interrupted = nanosleep(&request, &remaining) == -1 && errno == EINTR;
    int status = 0;
    pid_t waited;
    do { waited = waitpid(child, &status, 0); } while (waited < 0 && errno == EINTR);
    return interrupted && got == SIGUSR1 && (remaining.tv_sec || remaining.tv_nsec) &&
        remaining.tv_sec <= 2 && waited == child && WIFEXITED(status) && WEXITSTATUS(status) == 0;
}
int main(void) {
    emit("SIGACT");
    struct sigaction sa = {0}, old = {0};
    sa.sa_handler = handler;
    sigemptyset(&sa.sa_mask);
    long r = sigaction(SIGUSR1, &sa, &old);
    field("sigaction", r == 0, r);
    long k = kill(getpid(), SIGUSR1);
    field("kill", k == 0, k);
    field("ran", got == SIGUSR1, got);
    sigset_t set, pending;
    sigemptyset(&set); sigaddset(&set, SIGUSR1);
    got = 0;
    int masked = sigprocmask(SIG_BLOCK, &set, 0) == 0 && kill(getpid(), SIGUSR1) == 0 && got == 0;
    masked = masked && sigpending(&pending) == 0 && sigismember(&pending, SIGUSR1) == 1;
    masked = masked && sigprocmask(SIG_UNBLOCK, &set, 0) == 0 && got == SIGUSR1;
    field("mask", masked, got);
    sa.sa_sigaction = info_handler;
    sa.sa_flags = SA_SIGINFO | SA_NODEFER;
    int installed = sigaction(SIGUSR2, &sa, 0) == 0;
    int sent = installed && kill(getpid(), SIGUSR2) == 0;
    field("info", sent && info_count == 2, info_count);
    field("nested", sent && nested == 2, nested);
    sigset_t original, empty;
    sigemptyset(&empty);
    got = 0;
    int suspended = sigprocmask(SIG_BLOCK, &set, &original) == 0 && kill(getpid(), SIGUSR1) == 0;
    errno = 0;
    suspended = suspended && sigsuspend(&empty) == -1 && errno == EINTR && got == SIGUSR1;
    sigset_t restored;
    suspended = suspended && sigprocmask(SIG_SETMASK, 0, &restored) == 0 && sigismember(&restored, SIGUSR1) == 1;
    (void)sigprocmask(SIG_SETMASK, &original, 0);
    field("suspend", suspended, errno);
    struct sigaction child_action = {0};
    child_action.sa_sigaction = child_handler; child_action.sa_flags = SA_SIGINFO;
    sigemptyset(&child_action.sa_mask);
    int child_installed = sigaction(SIGCHLD, &child_action, 0) == 0;
    int interrupted = child_installed && check_wait_interrupt(0);
    field("eintr", interrupted, errno);
    field("restart", check_wait_interrupt(1), errno);
    field("child", child_count == 2 && child_pid > 0, child_count);
    field("sleep", check_sleep_interrupt(), errno);
    struct sigaction fault_action = {0};
    fault_action.sa_sigaction = fault_handler; fault_action.sa_flags = SA_SIGINFO;
    sigemptyset(&fault_action.sa_mask);
    int faults = sigaction(SIGILL, &fault_action, 0) == 0 && sigaction(SIGSEGV, &fault_action, 0) == 0;
    if (faults) {
        __asm__ volatile("ud2");
        uintptr_t address = UINT64_C(0x0000000000100000);
        __asm__ volatile("mov (%0), %0" : "+a"(address) : : "memory");
    }
    field("fault", faults && fault_count == 3, fault_count);
    unsigned char *page = mmap(0, 4096, PROT_READ | PROT_WRITE, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    int guard = page != MAP_FAILED;
    if (guard) {
        page[0] = 91;
        guard = mprotect(page, 4096, PROT_NONE) == 0;
        if (guard) {
            uintptr_t address = (uintptr_t)page;
            fault_count = 0;
            __asm__ volatile("mov (%0), %0" : "+a"(address) : : "memory");
            guard = fault_count == 2 && mprotect(page, 4096, PROT_READ | PROT_WRITE) == 0 && page[0] == 91;
        }
        guard = munmap(page, 4096) == 0 && guard;
    }
    field("guard", guard, errno);
    sa.sa_handler = SIG_IGN; sa.sa_flags = 0;
    got = 0;
    int ignored = sigaction(SIGUSR1, &sa, 0) == 0 && kill(getpid(), SIGUSR1) == 0 && got == 0;
    field("ignore", ignored, got);
    (void)sigaction(SIGUSR1, &old, 0);
    emit("|ok=");emit_dec(failures?0:1);emit(";err=");emit_dec(first_error);emit("\n");
    return failures?1:0;
}
