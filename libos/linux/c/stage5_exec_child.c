/* Separate static C ELF: exercised after a real root replacement, through the
 * platform crt and libc startup. No state is read at private adapter offsets. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <signal.h>
#include <stdlib.h>
#include <string.h>
#include <sys/stat.h>
#include <termios.h>
#include <unistd.h>
#ifdef PRIMA_HOST
#include "stage5_exec_fixture.h"
#endif

static int initialized;
static _Thread_local volatile int tls_value = 17;
__attribute__((constructor)) static void constructor(void) { initialized = 42; }

int main(int argc, char **argv) {
    if (argc != 2 || strcmp(argv[0], "exec-crt") || strcmp(argv[1], "verify")) return 71;
    const char *env = getenv("EXEC_CRT");
    if (!env || strcmp(env, "1") || initialized != 42) return 72;
    if (tls_value != 17 || !pthread_self()) return 73;
    tls_value = 23;
    errno = 0;
    if (fcntl(4, F_GETFD) != -1 || errno != EBADF || tls_value != 23) return 74;
    if (fcntl(3, F_GETFD) != 0 || (fcntl(3, F_GETFL) & O_ACCMODE) != O_RDONLY) return 75;
    char byte = 0;
    if (read(3, &byte, 1) != 1 || byte != 'a') return 76;
    char cwd[64];
    if (!getcwd(cwd, sizeof cwd) || strcmp(cwd, "/tmp")) return 77;
    mode_t previous = umask(0);
    umask(previous);
    if (previous != 027) return 78;
    sigset_t mask;
    struct sigaction action;
    if (sigprocmask(SIG_SETMASK, 0, &mask) || sigismember(&mask, SIGUSR2) != 1 ||
        sigaction(SIGUSR1, 0, &action) || action.sa_handler != SIG_IGN ||
        sigaction(SIGTERM, 0, &action) || action.sa_handler != SIG_DFL) return 79;
#ifndef PRIMA_HOST
    struct termios term;
    if (tcgetattr(0, &term) || (term.c_lflag & ECHO)) return 80;
#endif
    unsigned char *memory = calloc(37, 1);
    if (!memory) return 81;
    for (unsigned i = 0; i < 37; i++) if (memory[i]) return 82;
    memory[36] = 23;
    free(memory);
    /* A second replacement must be able to reuse the opaque handoff range.
     * Prima now exercises libc execve(path) from a freshly initialized libc. */
    char *args[] = {"again", "x", 0};
#ifdef PRIMA_HOST
    (void)exec_fixture(tiny_elf, sizeof tiny_elf, args, 2);
#else
    (void)execve("/tiny.elf", args, 0);
#endif
    return 83;
}
