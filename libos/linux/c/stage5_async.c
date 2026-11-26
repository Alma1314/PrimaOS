/* P3-3 acceptance program for the direct v2 session matrix.
 *
 * Runs only in the V5 image (`PRIMA_ASYNC_PROBE`); each probe call prints one
 * `S9ASYNC|case=...;status=...` line so xtask can assert machine evidence for
 * queueing, rejection, cancellation and close without a Linux reference.
 * The first `write` opens the v2 session through the production data path. */
#define _GNU_SOURCE
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

#ifdef PRIMA_ASYNC_PROBE
extern int prima_async_probe_open(void);
extern int prima_async_probe_merged(int fd);
extern int prima_async_probe_full(void);
extern int prima_async_probe_replay(void);
extern int prima_async_probe_rewind(void);
extern int prima_async_probe_reserved(int fd);
extern int prima_async_probe_cancel(int fd);
extern int prima_async_probe_fair(int fd);
extern int prima_async_probe_death(int fd);
extern int prima_async_probe_close(void);
#endif

#define ASYNC_PATH "/tmp/async-probe"

#define CASE(name, call) do { \
    int status = (call); \
    printf("S9ASYNC|case=%s;status=%d\n", name, status); \
    if (status) failed = 1; \
} while (0)

int main(void) {
    int failed = 0;
    int fd = open(ASYNC_PATH, O_CREAT | O_TRUNC | O_RDWR, 0600);
    if (fd < 0) {
        puts("S9ASYNC|case=fixture;status=1");
        return 1;
    }
    if (write(fd, "abcdef", 6) != 6)
        failed = 1;
#ifdef PRIMA_ASYNC_PROBE
    CASE("open", prima_async_probe_open());
    CASE("merged", prima_async_probe_merged(fd));
    CASE("full", prima_async_probe_full());
    CASE("replay", prima_async_probe_replay());
    CASE("rewind", prima_async_probe_rewind());
    CASE("reserved", prima_async_probe_reserved(fd));
    CASE("cancel", prima_async_probe_cancel(fd));
    CASE("fair", prima_async_probe_fair(fd));
    CASE("death", prima_async_probe_death(fd));
    CASE("close", prima_async_probe_close());
#else
    puts("S9ASYNC|case=build;status=1");
    failed = 1;
#endif
    printf("S9ASYNCDONE|fail=%d\n", failed);
    if (close(fd) != 0)
        failed = 1;
    if (unlink(ASYNC_PATH) != 0)
        failed = 1;
    return failed;
}
