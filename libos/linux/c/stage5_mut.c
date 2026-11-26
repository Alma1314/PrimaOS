/* M11：文件树变更（mkdir/rmdir/unlink）的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - Prima 在 `/tmp` 下建/删目录与文件，只读父目录用 `/etc`；宿主在一次性 cwd
 *   下做同样操作，只读父目录用 xtask 预置的 `ro/`（0555）。
 * - 覆盖 `mkdir`/`rmdir`/`unlink`、`mkdirat`/`unlinkat(AT_REMOVEDIR)`、
 *   已存在/父不存在/只读父目录/非空目录/类型不匹配等错误。
 * - `ENOTEMPTY` 与 `EISDIR` 不在 Prima 冻结错误集里：Prima 分别返回 `EACCES`，
 *   探针按平台断言各自期望，输出字段仍逐字节相同（`nonempty`/`unlink_dir`）。
 *
 * 失败位写入 `MUT|ok=0;...` 并以 1 退出；QEMU 与 Linux 都要求退出码 0。
 */
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <sys/stat.h>
#include <unistd.h>

#ifdef PRIMA_HOST
#define WD ""
#define WDIR "."
#define RO "ro/"
#else
#define PRIMA_ABI_NO_IOVEC
#include "prima_syscall.h"
#define WD "/tmp/"
#define WDIR "/tmp"
#define RO "/etc/"
#endif

static int failures;

static unsigned long text_len(const char *text) {
    unsigned long len = 0;
    while (text[len] != 0)
        len++;
    return len;
}

static void emit(const char *text) {
    ssize_t ignored = write(1, text, text_len(text));
    (void)ignored;
}

static void emit_dec(long value) {
    char digits[20];
    int index = 20;
    unsigned long magnitude =
        value < 0 ? (unsigned long)(-value) : (unsigned long)value;
    do {
        digits[--index] = (char)('0' + magnitude % 10);
        magnitude /= 10;
    } while (magnitude != 0);
    if (value < 0)
        digits[--index] = '-';
    ssize_t ignored = write(1, &digits[index], (unsigned long)(20 - index));
    (void)ignored;
}

static void field(const char *name, int pass, int *first) {
    emit(*first ? "|" : ";");
    *first = 0;
    emit(name);
    emit("=");
    emit_dec(pass ? 1 : 0);
    if (!pass)
        failures = 1;
}

static int is_dir(const char *path) {
    struct stat st;
    return stat(path, &st) == 0 && S_ISDIR(st.st_mode);
}

static int create_file(const char *path) {
    int fd = open(path, O_CREAT | O_WRONLY | O_TRUNC, 0666);
    if (fd < 0)
        return 0;
    int ok = write(fd, "x", 1) == 1 && close(fd) == 0;
    return ok;
}

int main(void) {
    int mkdir_ok = mkdir(WD "m11a", 0777) == 0;
    errno = 0;
    int exists = mkdir(WD "m11a", 0777) == -1 && errno == EEXIST;
    int stat_dir = is_dir(WD "m11a");
    int nested = mkdir(WD "m11a/sub", 0777) == 0;
    errno = 0;
#ifdef PRIMA_HOST
    int nonempty = rmdir(WD "m11a") == -1 && errno == ENOTEMPTY;
#else
    int nonempty = rmdir(WD "m11a") == -1 && errno == EACCES;
#endif
    int rmdir_ok = rmdir(WD "m11a/sub") == 0 && rmdir(WD "m11a") == 0;

    int unlink_file = create_file(WD "m11f") && unlink(WD "m11f") == 0;
    errno = 0;
    unlink_file = unlink_file && stat(WD "m11f", &(struct stat){0}) == -1 &&
                  errno == ENOENT;

    int unlink_dir = mkdir(WD "m11b", 0777) == 0;
    errno = 0;
#ifdef PRIMA_HOST
    unlink_dir = unlink_dir && unlink(WD "m11b") == -1 && errno == EISDIR;
#else
    unlink_dir = unlink_dir && unlink(WD "m11b") == -1 && errno == EACCES;
#endif
    int rmdir_file = create_file(WD "m11b/f");
    errno = 0;
    rmdir_file = rmdir_file && rmdir(WD "m11b/f") == -1 && errno == ENOTDIR;
    rmdir_file = rmdir_file && unlink(WD "m11b/f") == 0 &&
                 rmdir(WD "m11b") == 0;

    errno = 0;
    int readonly = mkdir(RO "m11x", 0777) == -1 && errno == EACCES;
    errno = 0;
    int missing = mkdir(WD "no/such", 0777) == -1 && errno == ENOENT;
    errno = 0;
    int enoent = unlink(WD "no-such") == -1 && errno == ENOENT;

    int at = 0;
    int dirfd = open(WDIR, O_RDONLY | O_DIRECTORY);
    if (dirfd >= 0) {
        int at_mkdir = mkdirat(dirfd, "m11c", 0777);
        int fd = openat(dirfd, "m11d", O_CREAT | O_WRONLY | O_TRUNC, 0666);
        int at_write = fd >= 0 && write(fd, "y", 1) == 1;
        int at_closefd = fd >= 0 && close(fd) == 0;
        int at_unlink = at_write && unlinkat(dirfd, "m11d", 0) == 0;
        int at_rmdir = unlinkat(dirfd, "m11c", AT_REMOVEDIR) == 0;
        int at_close = close(dirfd) == 0;
        at = at_mkdir == 0 && at_write && at_closefd && at_unlink &&
             at_rmdir && at_close;
    }

    int ok = mkdir_ok && exists && stat_dir && nested && nonempty && rmdir_ok &&
             unlink_file && unlink_dir && rmdir_file && readonly && missing &&
             enoent && at;
    emit(ok ? "MUT|ok=1" : "MUT|ok=0");
    int first = 0;
    field("mkdir", mkdir_ok, &first);
    field("exists", exists, &first);
    field("stat_dir", stat_dir, &first);
    field("nested", nested, &first);
    field("nonempty", nonempty, &first);
    field("rmdir", rmdir_ok, &first);
    field("unlink_file", unlink_file, &first);
    field("unlink_dir", unlink_dir, &first);
    field("rmdir_file", rmdir_file, &first);
    field("readonly", readonly, &first);
    field("missing", missing, &first);
    field("enoent", enoent, &first);
    field("at", at, &first);
    emit("\n");
    return ok ? 0 : 1;
}
