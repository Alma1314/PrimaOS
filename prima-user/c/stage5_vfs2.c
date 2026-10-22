/* M13a：符号链接、readlink、rename 与 fchdir 的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - `ROOT` 在 Prima 是 `/`，在宿主是空串（xtask 把 cwd 指向一次性目录）；
 *   `tmp` 在两端都可写，故共享探针只用 `ROOT "tmp/..."` 作为操作路径。
 * - 符号链接目标使用相对路径（`../etc/motd`），两端参考树布局一致，可逐字节比较。
 * - 失败消息只输出通过位与首次失败的负数，不输出真实路径，stdout 可逐字节比较。
 *
 * 失败位写入 `VFS2|ok=0;...` 并以 1 退出；Linux 与 QEMU 都要求退出码 0。
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

static int read_file_is(const char *path, const char *expected,
                        unsigned long len) {
    int fd = open(path, O_RDONLY);
    if (fd < 0)
        return 0;
    char buffer[32];
    unsigned long done = 0;
    while (done < len) {
        ssize_t got = read(fd, buffer + done, len - done);
        if (got <= 0)
            break;
        done += (unsigned long)got;
    }
    close(fd);
    return done == len && same_bytes(buffer, expected, len);
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

static int rejected(long result) { return result < 0; }

static void cleanup(void) {
    unlink(PATH("v2_rel"));
    unlink(PATH("v2_rel2"));
    unlink(PATH("v2_abs"));
    unlink(PATH("v2_a"));
    unlink(PATH("v2_b"));
    unlink(PATH("v2_loop1"));
    unlink(PATH("v2_loop2"));
    unlink(PATH("v2_e/x"));
    unlink(PATH("v2_f/k"));
    unlink(PATH("v2_g/k"));
    rmdir(PATH("v2_e/sub"));
    rmdir(PATH("v2_e"));
    rmdir(PATH("v2_d"));
    rmdir(PATH("v2_f"));
    rmdir(PATH("v2_g"));
}

int main(void) {
    cleanup();
    emit("VFS2");

    /* 1. 相对符号链接：创建、readlink 原文、经链接读取目标文件。 */
    char target[64];
    int link_ok = symlink("../etc/motd", PATH("v2_rel")) == 0;
    long link_len = link_ok ? readlink(PATH("v2_rel"), target, sizeof target) : -1;
    int rel_ok = link_ok && link_len == 11 &&
                 same_bytes(target, "../etc/motd", 11) &&
                 read_file_is(PATH("v2_rel"), "welcome\n", 8);
    field("sym", rel_ok, link_len);

    /* 2. 链接到链接：相对目标 "v2_rel" 再从 /tmp 解析并继续跟随。 */
    int chain_ok = symlink("v2_rel", PATH("v2_abs")) == 0 &&
                   read_file_is(PATH("v2_abs"), "welcome\n", 8) &&
                   readlink(PATH("v2_abs"), target, sizeof target) == 6 &&
                   same_bytes(target, "v2_rel", 6);
    field("chain", chain_ok, 0);

    /* 3. rename 文件：内容保留、旧名消失。 */
    int ren_ok = write_file(PATH("v2_a"), "abc", 3) &&
                 rename(PATH("v2_a"), PATH("v2_b")) == 0 &&
                 read_file_is(PATH("v2_b"), "abc", 3) &&
                 open(PATH("v2_a"), O_RDONLY) < 0;
    field("ren", ren_ok, 0);

    /* 4. rename 符号链接：链接本身被移动，目标不变。 */
    int renlink_ok = link_ok && rename(PATH("v2_rel"), PATH("v2_rel2")) == 0 &&
                     read_file_is(PATH("v2_rel2"), "welcome\n", 8) &&
                     open(PATH("v2_rel"), O_RDONLY) < 0;
    field("renlink", renlink_ok, 0);

    /* 5. rename 目录连同子树。 */
    int dir_ok = mkdir(PATH("v2_d"), 0777) == 0 &&
                 write_file(PATH("v2_d/x"), "x", 1) &&
                 rename(PATH("v2_d"), PATH("v2_e")) == 0 &&
                 read_file_is(PATH("v2_e/x"), "x", 1);
    field("dir", dir_ok, 0);

    /* 6. 目录不能移进自己的子树。 */
    int subtree_ok = dir_ok && mkdir(PATH("v2_e/sub"), 0777) == 0 &&
                     rejected(rename(PATH("v2_e"), PATH("v2_e/sub")));
    field("subtree", subtree_ok, 0);

    /* 7. 目录不能覆盖非空目录（冻结映射为 EACCES）。 */
    int nonempty_ok = mkdir(PATH("v2_f"), 0777) == 0 &&
                      write_file(PATH("v2_f/k"), "k", 1) &&
                      mkdir(PATH("v2_g"), 0777) == 0 &&
                      write_file(PATH("v2_g/k"), "k", 1) &&
                      rejected(rename(PATH("v2_f"), PATH("v2_g")));
    field("nonempty", nonempty_ok, 0);

    /* 8. fchdir：打开 `tmp` 目录 fd 后恢复工作目录；getcwd 以 /tmp 结尾
     *（宿主前缀不同），随后 `chdir("..")` 回到参考根。 */
    int dir_fd = open(ROOT "tmp", O_RDONLY | O_DIRECTORY);
    char cwd[128];
    int fchdir_ok = dir_fd >= 0 && fchdir(dir_fd) == 0 &&
                    getcwd(cwd, sizeof cwd) != 0;
    if (fchdir_ok) {
        unsigned long len = text_len(cwd);
        fchdir_ok = len >= 4 && same_bytes(cwd + len - 4, "/tmp", 4);
    }
    if (dir_fd >= 0)
        close(dir_fd);
    field("fchdir", fchdir_ok, 0);
    /* 回到参考根：宿主是 run_dir，Prima 是 `/`。 */
    if (chdir("..") != 0)
        failures = 1;

    /* 9. 只读父目录拒绝 rename。 */
    int ro_ok = rejected(rename(PATH("v2_e"), ROOT "usr/v2_ro"));
    field("ro", ro_ok, 0);

    /* 10. 符号链接环确定失败。 */
    int loop_ok = symlink("v2_loop2", PATH("v2_loop1")) == 0 &&
                  symlink("v2_loop1", PATH("v2_loop2")) == 0 &&
                  open(PATH("v2_loop1"), O_RDONLY) < 0;
    field("loop", loop_ok, 0);

    /* 11. 对普通文件 readlink 拒绝。 */
    int type_ok = rejected(readlink(PATH("v2_b"), target, sizeof target));
    field("type", type_ok, 0);

    /* 12. 小缓冲按 Linux 语义截断并返回拷贝数。 */
    long trunc_len = link_ok ? readlink(PATH("v2_rel2"), target, 5) : -1;
    int trunc_ok = trunc_len == 5 && same_bytes(target, "../et", 5);
    field("trunc", trunc_ok, trunc_len);

    /* 13. 缺失路径的 rename/readlink 确定失败。 */
    int missing_ok = rejected(rename(PATH("v2_none"), PATH("v2_none2"))) &&
                     rejected(readlink(PATH("v2_none"), target, sizeof target));
    field("missing", missing_ok, 0);

    emit("|ok=");
    emit_dec(failures ? 0 : 1);
    emit(";err=");
    emit_dec(first_error);
    emit("\n");

    cleanup();
    return failures ? 1 : 0;
}
