/* M13b：chmod/fchmod/umask 与 stdio 文件 API 的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - `ROOT` 在 Prima 是 `/`，在宿主是空串；`tmp` 在两端都可写。
 * - 只测冻结子集：umask 对创建 mode 的收紧、chmod/fchmod 的可观察效果、
 *   0444 文件的写拒绝，以及 fopen/fwrite/fseek/fread/fclose。
 * - 不测 setuid/setgid、uid/gid、组/other 权限与符号链接上的 chmod。
 *
 * 失败位写入 `VFS3|ok=0;...` 并以 1 退出；Linux 与 QEMU 都要求退出码 0。
 */
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <sys/stat.h>
#include <unistd.h>

#ifdef PRIMA_HOST
#define ROOT ""
#else
#define PRIMA_ABI_NO_IOVEC
#include "prima_syscall.h"
#define ROOT "/"
#endif

#define PATH(name) ROOT "tmp/" name

static int failures;
static int first_error;
static int first_field = 1;

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

static void field(const char *name, int pass, long error) {
    emit(first_field ? "|" : ";");
    first_field = 0;
    emit(name);
    emit("=");
    emit_dec(pass ? 1 : 0);
    if (!pass) {
        failures = 1;
        if (first_error == 0)
            first_error = (int)error;
    }
}

static int same_bytes(const void *left, const void *right, unsigned long count) {
    const unsigned char *a = left;
    const unsigned char *b = right;
    for (unsigned long i = 0; i < count; i++)
        if (a[i] != b[i])
            return 0;
    return 1;
}

static int mode_is(const char *path, mode_t type, unsigned long permission) {
    struct stat st;
    if (stat(path, &st) != 0)
        return 0;
    return (st.st_mode & S_IFMT) == type &&
           (unsigned long)(st.st_mode & 0777) == permission;
}

static int write_file(const char *path, const char *text, unsigned long len) {
    int fd = open(path, O_WRONLY | O_CREAT | O_TRUNC, 0666);
    if (fd < 0)
        return 0;
    unsigned long done = 0;
    while (done < len) {
        ssize_t wrote = write(fd, text + done, len - done);
        if (wrote <= 0)
            break;
        done += (unsigned long)wrote;
    }
    close(fd);
    return done == len;
}

int main(void) {
    unlink(PATH("v3_mode"));
    unlink(PATH("v3_stdio"));
    rmdir(PATH("v3_dir"));
    emit("VFS3");

    /* 1. umask(0)：创建 mode 原样保留。 */
    long previous = umask(0);
    int fd = open(PATH("v3_mode"), O_RDWR | O_CREAT | O_TRUNC, 0666);
    int zero_ok = fd >= 0 && mode_is(PATH("v3_mode"), S_IFREG, 0666) &&
                  mkdir(PATH("v3_dir"), 0777) == 0 &&
                  mode_is(PATH("v3_dir"), S_IFDIR, 0777);
    if (fd >= 0)
        close(fd);
    field("umask0", zero_ok, 0);

    /* 2. umask(077)：文件 0666→0600、目录 0777→0700。 */
    if (unlink(PATH("v3_mode")) != 0 || rmdir(PATH("v3_dir")) != 0)
        failures = 1;
    long before077 = umask(077);
    fd = open(PATH("v3_mode"), O_RDWR | O_CREAT | O_TRUNC, 0666);
    int mask_ok = fd >= 0 && mode_is(PATH("v3_mode"), S_IFREG, 0600) &&
                  mkdir(PATH("v3_dir"), 0777) == 0 &&
                  mode_is(PATH("v3_dir"), S_IFDIR, 0700);
    if (fd >= 0)
        close(fd);
    field("umask077", mask_ok && before077 >= 0, 0);

    /* 3. chmod/fchmod 的可观察效果。 */
    int chmod_ok = chmod(PATH("v3_mode"), 0644) == 0 &&
                   mode_is(PATH("v3_mode"), S_IFREG, 0644);
    fd = open(PATH("v3_mode"), O_RDONLY);
    if (fd >= 0) {
        chmod_ok = chmod_ok && fchmod(fd, 0600) == 0 &&
                   mode_is(PATH("v3_mode"), S_IFREG, 0600);
        close(fd);
    } else {
        chmod_ok = 0;
    }
    field("chmod", chmod_ok, 0);

    /* 4. 0444 文件拒绝写打开；恢复 0666 后可写。 */
    int ro_fd = chmod(PATH("v3_mode"), 0444) == 0
                    ? open(PATH("v3_mode"), O_WRONLY)
                    : -1;
    int write_fd = ro_fd < 0 && chmod(PATH("v3_mode"), 0666) == 0
                       ? open(PATH("v3_mode"), O_WRONLY)
                       : -1;
    int ro_ok = ro_fd < 0 && write_fd >= 0;
    if (ro_fd >= 0)
        close(ro_fd);
    if (write_fd >= 0)
        close(write_fd);
    field("readonly", ro_ok, 0);

    /* 5. stdio：fopen("w+")/fwrite/fseek/fread/fclose 往返。 */
    FILE *file = fopen(PATH("v3_stdio"), "w+");
    int stdio_ok = file != 0;
    if (file != 0) {
        stdio_ok = fwrite("hello", 1, 5, file) == 5 && fflush(file) == 0 &&
                   fseek(file, 0, SEEK_SET) == 0;
        char buffer[8] = {0};
        stdio_ok = stdio_ok && fread(buffer, 1, 5, file) == 5 &&
                   same_bytes(buffer, "hello", 5) && fclose(file) == 0;
    }
    field("stdio", stdio_ok, 0);

    /* 6. stdio 只读打开 0444 文件。 */
    if (unlink(PATH("v3_stdio")) != 0)
        failures = 1;
    int stdio_ro = write_file(PATH("v3_stdio"), "ro", 2) &&
                   chmod(PATH("v3_stdio"), 0444) == 0;
    if (stdio_ro) {
        FILE *reader = fopen(PATH("v3_stdio"), "r");
        stdio_ro = reader != 0;
        if (reader != 0) {
            char buffer[4] = {0};
            stdio_ro = fread(buffer, 1, 2, reader) == 2 &&
                       same_bytes(buffer, "ro", 2) && fclose(reader) == 0;
        }
        stdio_ro = stdio_ro && fopen(PATH("v3_stdio"), "a") == 0;
    }
    field("stdio_ro", stdio_ro, 0);

    /* 7. 恢复 umask；缺失路径的 chmod 确定失败。 */
    umask((mode_t)(previous < 0 ? 022 : previous));
    int missing_ok = chmod(PATH("v3_none"), 0666) < 0;
    field("missing", missing_ok, 0);

    emit("|ok=");
    emit_dec(failures ? 0 : 1);
    emit(";err=");
    emit_dec(first_error);
    emit("\n");

    unlink(PATH("v3_mode"));
    unlink(PATH("v3_stdio"));
    rmdir(PATH("v3_dir"));
    return failures ? 1 : 0;
}
