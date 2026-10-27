/* Same-source Linux/Prima probe for bounded ftruncate/pread/pwrite.
 * Host runner supplies an empty cwd containing tmp/; guest uses /tmp.
 * No pre-existing file is removed. WRITE_AT follows POSIX and ignores append;
 * the Linux reference explicitly checks Linux's different append behavior.
 * The program is ready for xtask integration; it does not implement fsync. */
#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <unistd.h>

#ifdef PRIMA_HOST
#define EXT_PATH "tmp/file-ext"
#else
#define EXT_PATH "/tmp/file-ext"
#endif

#define CHECK(condition) do { if (!(condition)) { \
    fprintf(stderr, "FILEEXT failure line=%d errno=%d\n", __LINE__, errno); \
    failed = 1; goto cleanup; \
} } while (0)

int main(void) {
    int failed = 0, linked = 0;
    int fd = -1, alias = -1, ro = -1, wo = -1, append = -1;
    char out[16] = {0};
    struct stat info;
    fd = open(EXT_PATH, O_CREAT | O_EXCL | O_RDWR, 0600);
    CHECK(fd >= 0);
    linked = 1;
    CHECK(write(fd, "abcdef", 6) == 6);
    CHECK(lseek(fd, 3, SEEK_SET) == 3);
    alias = dup(fd);
    CHECK(alias >= 0);
    CHECK(pwrite(fd, "XYZ", 3, 1) == 3);
    CHECK(pread(alias, out, sizeof out, 0) == 6 && memcmp(out, "aXYZef", 6) == 0);
    CHECK(lseek(alias, 0, SEEK_CUR) == 3);
    CHECK(ftruncate(fd, 2) == 0 && ftruncate(alias, 8) == 0);
    CHECK(fstat(fd, &info) == 0 && info.st_size == 8);
    CHECK(pread(fd, out, 8, 0) == 8 && memcmp(out, "aX\0\0\0\0\0\0", 8) == 0);
    CHECK(lseek(fd, 0, SEEK_CUR) == 3);
    CHECK(fchmod(fd, 0) == 0);
    CHECK(ftruncate(fd, 4) == 0 && pwrite(fd, "Q", 1, 3) == 1);
    CHECK(fchmod(fd, 0600) == 0);
    ro = open(EXT_PATH, O_RDONLY);
    wo = open(EXT_PATH, O_WRONLY);
    CHECK(ro >= 0 && wo >= 0);
    errno = 0;
    CHECK(pwrite(ro, "Z", 1, 0) == -1 && errno == EBADF);
    errno = 0;
    CHECK(pread(wo, out, 1, 0) == -1 && errno == EBADF);
    errno = 0;
    CHECK(ftruncate(ro, 0) == -1);
    errno = 0;
    CHECK(pread(fd, out, 1, -1) == -1 && errno == EINVAL);
    errno = 0;
    CHECK(pwrite(fd, "Z", 1, -1) == -1 && errno == EINVAL);
    errno = 0;
    CHECK(ftruncate(fd, -1) == -1 && errno == EINVAL);
    CHECK(pread(fd, out, 1, INT64_C(1) << 40) == 0);
    CHECK(pwrite(fd, "", 0, INT64_MAX) == 0);
    append = open(EXT_PATH, O_WRONLY | O_APPEND);
    CHECK(append >= 0 && pwrite(append, "P", 1, 0) == 1);
#ifdef PRIMA_HOST
    CHECK(pread(fd, out, 5, 0) == 5 && memcmp(out, "aX\0QP", 5) == 0);
#else
    CHECK(pread(fd, out, 4, 0) == 4 && memcmp(out, "PX\0Q", 4) == 0);
    CHECK(pread(fd, out, 1, INT64_MAX) == 0);
    errno = 0;
    CHECK(ftruncate(fd, INT64_MAX) == -1 && errno == ENOSPC);
    errno = 0;
    CHECK(pwrite(fd, "Z", 1, INT64_MAX) == -1 && errno == ENOSPC);
#endif
    CHECK(lseek(fd, 0, SEEK_CUR) == 3);
    CHECK(unlink(EXT_PATH) == 0);
    linked = 0;
    CHECK(ftruncate(fd, 6) == 0 && pwrite(alias, "K", 1, 5) == 1);
    CHECK(pread(fd, out, 1, 5) == 1 && out[0] == 'K');
    CHECK(lseek(alias, 0, SEEK_CUR) == 3);
cleanup:
    if (append >= 0 && close(append) != 0) failed = 1;
    if (wo >= 0 && close(wo) != 0) failed = 1;
    if (ro >= 0 && close(ro) != 0) failed = 1;
    if (alias >= 0 && close(alias) != 0) failed = 1;
    if (fd >= 0 && close(fd) != 0) failed = 1;
    if (linked && unlink(EXT_PATH) != 0) failed = 1;
    if (!failed) puts("FILEEXT|resize=1;pread=1;pwrite=1;cursor=1;access=1;orphan=1|ok=1");
    return failed;
}
