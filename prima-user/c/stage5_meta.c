/* M9：路径元数据（statx）与工作目录（chdir/getcwd）的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - `ROOT` 在 Prima 是绝对 `/`，在宿主是空串（xtask 把 cwd 指向一次性目录并按同一
 *   树形/权限布置），两端都从同一语义根出发。
 * - 覆盖 `stat`/`lstat`/`fstat`/`fstatat`（经 musl 的 `statx(332)`）、普通文件/目录/
 *   字符设备类型与 0444/0555/0666/0777 权限位、ENOENT/ENOTDIR 错误，
 *   以及 `chdir`/`getcwd`/相对路径解析/`getcwd` 的 ERANGE。
 * - `/dev/null` 是 Prima 侧字符设备；宿主非 root 无法 mknod，故 `null` 位在宿主按
 *   “不适用”保留为 1，并在报告里登记。
 *
 * 失败位写入 `META|ok=0;...` 并以 1 退出；QEMU 与 Linux 都要求退出码 0。
 */
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <sys/stat.h>
#include <unistd.h>

#ifdef PRIMA_HOST
#define ROOT ""
#else
#define PRIMA_ABI_NO_IOVEC
#include "prima_syscall.h"
#define ROOT "/"
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

static int same_text(const char *left, const char *right) {
    unsigned long index = 0;
    for (;;) {
        if (left[index] != right[index])
            return 0;
        if (left[index] == 0)
            return 1;
        index++;
    }
}

static int ends_with(const char *text, const char *suffix) {
    unsigned long text_length = text_len(text);
    unsigned long suffix_length = text_len(suffix);
    if (suffix_length > text_length)
        return 0;
    return same_text(text + text_length - suffix_length, suffix);
}

static int mode_is(const struct stat *st, mode_t type, unsigned long permission) {
    return (st->st_mode & S_IFMT) == type &&
           (unsigned long)(st->st_mode & 07777) == permission;
}

static int read_exact(int fd, const char *expected, unsigned long len) {
    char buffer[16];
    if (len > sizeof buffer)
        return 0;
    ssize_t got = read(fd, buffer, len);
    if (got != (ssize_t)len)
        return 0;
    for (unsigned long i = 0; i < len; i++)
        if (buffer[i] != expected[i])
            return 0;
    return read(fd, buffer, 1) == 0;
}

int main(void) {
    struct stat st;
    char initial[256];
    char current[256];
    char tiny[1];

    int motd_ok = stat(ROOT "etc/motd", &st) == 0 && mode_is(&st, S_IFREG, 0444) &&
                  st.st_size == 8 && st.st_nlink >= 1 && st.st_ino != 0;
    int hostname_ok = stat(ROOT "etc/hostname", &st) == 0 &&
                      mode_is(&st, S_IFREG, 0444) && st.st_size == 6;
    int readme_ok = stat(ROOT "README", &st) == 0 &&
                    mode_is(&st, S_IFREG, 0444) && st.st_size == 13;
    int version_ok = stat(ROOT "usr/lib/version", &st) == 0 &&
                     mode_is(&st, S_IFREG, 0444) && st.st_size == 4;
    int etc_ok = stat(ROOT "etc", &st) == 0 && mode_is(&st, S_IFDIR, 0555);
    int tmp_ok = stat(ROOT "tmp", &st) == 0 && mode_is(&st, S_IFDIR, 0777);
    int f_ok = stat(ROOT "f", &st) == 0 && mode_is(&st, S_IFREG, 0666) &&
               st.st_size == 0;
    int null_ok = 1;
#ifndef PRIMA_HOST
    null_ok = stat(ROOT "dev/null", &st) == 0 && mode_is(&st, S_IFCHR, 0666) &&
              st.st_size == 0;
#endif
    int lstat_ok = lstat(ROOT "etc/motd", &st) == 0 &&
                   mode_is(&st, S_IFREG, 0444) && st.st_size == 8;

    int fstat_ok = 0;
    int fd = open(ROOT "etc/motd", O_RDONLY);
    if (fd >= 0) {
        fstat_ok = fstat(fd, &st) == 0 && mode_is(&st, S_IFREG, 0444) &&
                   st.st_size == 8;
        (void)close(fd);
    }

    int fstatat_ok = 0;
    int etcfd = open(ROOT "etc", O_RDONLY | O_DIRECTORY);
    if (etcfd >= 0) {
        fstatat_ok = fstatat(etcfd, "motd", &st, 0) == 0 &&
                     mode_is(&st, S_IFREG, 0444) && st.st_size == 8;
        int child = openat(etcfd, "motd", O_RDONLY);
        fstatat_ok = fstatat_ok && child >= 0 &&
                     read_exact(child, "welcome\n", 8);
        if (child >= 0)
            (void)close(child);
        (void)close(etcfd);
    }

    errno = 0;
    int enoent_ok = stat(ROOT "nope", &st) == -1 && errno == ENOENT;
    errno = 0;
    int enotdir_ok = stat(ROOT "etc/motd/x", &st) == -1 && errno == ENOTDIR;
    errno = 0;
    int empty_ok = stat("", &st) == -1 && errno == ENOENT;

    int chdir_ok = getcwd(initial, sizeof initial) != 0;
    int cwd_abs_ok = 0;
    int cwd_etc_ok = 0;
    int cwd_parent_ok = 0;
    int openrel_ok = 0;
    if (chdir_ok) {
        cwd_abs_ok = initial[0] == '/';
        chdir_ok = chdir(ROOT "etc") == 0 &&
                   getcwd(current, sizeof current) != 0 &&
                   current[0] == '/' && ends_with(current, "/etc");
        cwd_etc_ok = chdir_ok && stat("motd", &st) == 0 &&
                     mode_is(&st, S_IFREG, 0444) && st.st_size == 8;
        int relative = open("motd", O_RDONLY);
        openrel_ok = relative >= 0 && read_exact(relative, "welcome\n", 8);
        if (relative >= 0)
            (void)close(relative);
        cwd_parent_ok = chdir("..") == 0 &&
                        getcwd(current, sizeof current) != 0 &&
                        same_text(current, initial);
        int back = open("etc/hostname", O_RDONLY);
        cwd_parent_ok = cwd_parent_ok && back >= 0 &&
                        read_exact(back, "prima\n", 6);
        if (back >= 0)
            (void)close(back);
    }
    int chdir_file_ok = chdir(ROOT "etc/motd") == -1 && errno == ENOTDIR &&
                        getcwd(current, sizeof current) != 0 &&
                        same_text(current, initial);
    errno = 0;
    int erange_ok = getcwd(tiny, sizeof tiny) == 0 && errno == ERANGE;

    int all_ok = motd_ok && hostname_ok && readme_ok && version_ok && etc_ok &&
                 tmp_ok && f_ok && null_ok && lstat_ok && fstat_ok &&
                 fstatat_ok && enoent_ok && enotdir_ok && empty_ok &&
                 chdir_ok && cwd_abs_ok && cwd_etc_ok && cwd_parent_ok &&
                 openrel_ok && chdir_file_ok && erange_ok;
    failures = !all_ok;
    int first = 1;
    emit("META");
    field("ok", all_ok, &first);
    field("motd", motd_ok, &first);
    field("hostname", hostname_ok, &first);
    field("readme", readme_ok, &first);
    field("version", version_ok, &first);
    field("etc", etc_ok, &first);
    field("tmp", tmp_ok, &first);
    field("f", f_ok, &first);
    field("null", null_ok, &first);
    field("lstat", lstat_ok, &first);
    field("fstat", fstat_ok, &first);
    field("fstatat", fstatat_ok, &first);
    field("enoent", enoent_ok, &first);
    field("enotdir", enotdir_ok, &first);
    field("empty", empty_ok, &first);
    field("chdir", chdir_ok, &first);
    field("cwd_abs", cwd_abs_ok, &first);
    field("cwd_etc", cwd_etc_ok, &first);
    field("cwd_parent", cwd_parent_ok, &first);
    field("openrel", openrel_ok, &first);
    field("chdir_file", chdir_file_ok, &first);
    field("erange", erange_ok, &first);
    emit("\n");
    return failures ? 1 : 0;
}
