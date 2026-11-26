/* M8：用户态 VFS/rootfs、通用 fd 与多级路径的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - `ROOT` 在 Prima 是绝对 `/`，在宿主是空串（xtask 把 cwd 指向一次性目录），
 *   因此两端都从同一个语义根出发；绝对路径解析另在 Prima 侧由 `/` 覆盖。
 * - 覆盖多级路径、`.`/`..`、`openat(dirfd, ...)`、目录枚举、`dup` 目录 fd、
 *   `O_CREAT`/`O_EXCL`/`O_TRUNC`/`O_APPEND`、只读文件、fd 0 与 `/dev/null`。
 * - 只输出通过位，不含真实路径或 errno 数值以外的环境信息，故 stdout 可逐字节比较。
 *
 * 失败位写入 `VFS|ok=0;...` 并以 1 退出；QEMU 与 Linux 都要求退出码 0。
 */
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <unistd.h>

#ifdef PRIMA_HOST
extern long syscall(long number, ...);
#define PRIMA_LINUX_SYS_getdents64 217
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

static int same_bytes(const void *left, const void *right, unsigned long count) {
    const unsigned char *a = left;
    const unsigned char *b = right;
    for (unsigned long i = 0; i < count; i++)
        if (a[i] != b[i])
            return 0;
    return 1;
}

static int read_exact(int fd, const char *expected, unsigned long len) {
    char buffer[16];
    if (len > sizeof buffer)
        return 0;
    ssize_t got = read(fd, buffer, len);
    return got == (ssize_t)len && same_bytes(buffer, expected, len) &&
           read(fd, buffer, 1) == 0;
}

static int expect_error(const char *path, int flags, int wanted) {
    int fd = openat(AT_FDCWD, path, flags, 0);
    if (fd >= 0) {
        (void)close(fd);
        return 0;
    }
    return errno == wanted;
}

/* 直接从目录 fd 取一条记录；返回读到的字节数或 -1。宿主与 Prima 使用同一 Linux 号。 */
static long raw_getdents64(int fd, void *buffer, unsigned long count) {
#ifdef PRIMA_HOST
    long result = syscall(PRIMA_LINUX_SYS_getdents64, fd, buffer, count);
    return result == -1 ? -(long)errno : result;
#else
    return __syscall3(PRIMA_LINUX_SYS_getdents64, fd, (long)buffer,
                      (long)count);
#endif
}

struct probe_dirent {
    uint64_t d_ino;
    int64_t d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[];
};

static int collect_etc(int fd, int *saw_motd, int *saw_hostname, int *count) {
    char buffer[256];
    for (;;) {
        long got = raw_getdents64(fd, buffer, sizeof buffer);
        if (got < 0)
            return 0;
        if (got == 0)
            return 1;
        long offset = 0;
        while (offset < got) {
            struct probe_dirent *entry = (struct probe_dirent *)(buffer + offset);
            if (entry->d_reclen == 0 || offset + entry->d_reclen > got)
                return 0;
            if (entry->d_name[0] != '.') {
                (*count)++;
                if (same_bytes(entry->d_name, "motd", 5))
                    *saw_motd = 1;
                if (same_bytes(entry->d_name, "hostname", 9))
                    *saw_hostname = 1;
            }
            offset += entry->d_reclen;
        }
    }
}

int main(void) {
    int motd_ok = 0;
    int multi_ok = 0;
    int parent_ok = 0;
    int dirfd_ok = 0;
    int dupdir_ok = 0;
    int enoent_ok = 0;
    int enotdir_ok = 0;
    int eacces_ok = 0;
    int eexist_ok = 0;
    int create_ok = 0;
    int append_ok = 0;
    int null_ok = 0;
    int getdents_ok = 0;
    char buffer[16];

    int motd = openat(AT_FDCWD, ROOT "etc/motd", O_RDONLY);
    if (motd >= 0) {
        motd_ok = read_exact(motd, "welcome\n", 8);
        (void)close(motd);
    }

    int version = openat(AT_FDCWD, "usr/./lib/../lib/version", O_RDONLY);
    if (version >= 0) {
        multi_ok = read_exact(version, "1.0\n", 4);
        (void)close(version);
    }

    int hostname = openat(AT_FDCWD, "etc/../etc/hostname", O_RDONLY);
    if (hostname >= 0) {
        parent_ok = read_exact(hostname, "prima\n", 6);
        (void)close(hostname);
    }

    int etc = openat(AT_FDCWD, ROOT "etc", O_RDONLY | O_DIRECTORY);
    if (etc >= 0) {
        int child = openat(etc, "motd", O_RDONLY);
        dirfd_ok = child >= 0 && read_exact(child, "welcome\n", 8);
        if (child >= 0)
            (void)close(child);
        int parent = openat(etc, "../README", O_RDONLY);
        dirfd_ok = dirfd_ok && parent >= 0 &&
                   read_exact(parent, "prima rootfs\n", 13);
        if (parent >= 0)
            (void)close(parent);
        (void)close(etc);
    }

    /* 关闭原目录 fd 后，dup 出的 fd 仍持有同一打开描述。 */
    int lib = openat(AT_FDCWD, ROOT "usr/lib", O_RDONLY | O_DIRECTORY);
    if (lib >= 0) {
        int dup_lib = dup(lib);
        if (dup_lib >= 0) {
            (void)close(lib);
            int version2 = openat(dup_lib, "version", O_RDONLY);
            dupdir_ok = version2 >= 0 && read_exact(version2, "1.0\n", 4);
            if (version2 >= 0)
                (void)close(version2);
            (void)close(dup_lib);
        }
    }

    enoent_ok = expect_error(ROOT "nope", O_RDONLY, ENOENT);
    enotdir_ok = expect_error(ROOT "etc/motd/x", O_RDONLY, ENOTDIR) &&
                 expect_error(ROOT "etc/motd", O_RDONLY | O_DIRECTORY, ENOTDIR);
    int expect_access = 1;
#ifdef PRIMA_HOST
    /* root 用户不受 0444 限制；此时该位作为“不适用”保留为 1，报告里登记。 */
    expect_access = geteuid() != 0;
#endif
    eacces_ok = expect_access
                    ? expect_error(ROOT "etc/motd", O_WRONLY, EACCES)
                    : 1;
    eexist_ok = expect_error(ROOT "etc/motd", O_RDONLY | O_CREAT | O_EXCL,
                             EEXIST);

    /* 在唯一可写目录 /tmp 新建文件；O_EXCL 第二次必须 EEXIST。 */
    int scratch = openat(AT_FDCWD, ROOT "tmp/scratch",
                         O_RDWR | O_CREAT | O_EXCL, 0600);
    if (scratch >= 0) {
        int wrote = write(scratch, "xyz", 3) == 3;
        (void)close(scratch);
        int again = expect_error(ROOT "tmp/scratch",
                                 O_RDWR | O_CREAT | O_EXCL, EEXIST);
        int reopened = openat(AT_FDCWD, ROOT "tmp/scratch", O_RDONLY);
        create_ok = wrote && again && reopened >= 0 &&
                    read_exact(reopened, "xyz", 3);
        if (reopened >= 0)
            (void)close(reopened);
    }

    /* 根文件 f 的写/截断 + O_APPEND：写入位置在每次写前移到 EOF。 */
    int data = openat(AT_FDCWD, ROOT "f", O_WRONLY | O_CREAT | O_TRUNC, 0600);
    if (data >= 0) {
        append_ok = write(data, "abcdef", 6) == 6;
        (void)close(data);
        int tail = openat(AT_FDCWD, ROOT "f", O_WRONLY | O_APPEND);
        append_ok = append_ok && tail >= 0 && write(tail, "Z", 1) == 1;
        if (tail >= 0)
            (void)close(tail);
        int check = openat(AT_FDCWD, ROOT "f", O_RDONLY);
        append_ok = append_ok && check >= 0 &&
                    read(check, buffer, sizeof buffer) == 7 &&
                    same_bytes(buffer, "abcdefZ", 7);
        if (check >= 0)
            (void)close(check);
    }

    /* M14：fd 0 改为 tty 输入（读会阻塞），本探针不再声明 fd0 EOF；
     * /dev/null 仍读 EOF、写计数成功。 */
    int nullfd = openat(AT_FDCWD, "/dev/null", O_RDWR);
    if (nullfd >= 0) {
        null_ok = read(nullfd, buffer, sizeof buffer) == 0 &&
                  write(nullfd, "abc", 3) == 3;
        (void)close(nullfd);
    }

    /* 目录枚举：/etc 恰有 motd、hostname（忽略 . 与 ..）。 */
    etc = openat(AT_FDCWD, ROOT "etc", O_RDONLY | O_DIRECTORY);
    if (etc >= 0) {
        int saw_motd = 0;
        int saw_hostname = 0;
        int count = 0;
        getdents_ok = collect_etc(etc, &saw_motd, &saw_hostname, &count) &&
                      saw_motd && saw_hostname && count == 2;
        (void)close(etc);
    }

    int all_ok = motd_ok && multi_ok && parent_ok && dirfd_ok && dupdir_ok &&
                 enoent_ok && enotdir_ok && eacces_ok && eexist_ok &&
                 create_ok && append_ok && null_ok && getdents_ok;
    failures = !all_ok;
    int first = 1;
    emit("VFS");
    field("ok", all_ok, &first);
    field("motd", motd_ok, &first);
    field("multi", multi_ok, &first);
    field("parent", parent_ok, &first);
    field("dirfd", dirfd_ok, &first);
    field("dupdir", dupdir_ok, &first);
    field("enoent", enoent_ok, &first);
    field("enotdir", enotdir_ok, &first);
    field("eacces", eacces_ok, &first);
    field("eexist", eexist_ok, &first);
    field("create", create_ok, &first);
    field("append", append_ok, &first);
    field("null", null_ok, &first);
    field("getdents", getdents_ok, &first);
    emit("\n");
    return failures ? 1 : 0;
}
