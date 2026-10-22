/* M7：POSIX fd 表与进程标识的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - `dup`/`dup2`/`dup3` 走适配层的本地 fd 表（3..10），文件项共享服务端句柄与偏移；
 * - `getpid` 映射到 Prima 的 `SYS_TASK_ID`，要求非零且在一次文件 IPC 前后不变；
 * - 只输出通过位，不含真实 pid 或时间，故两端 stdout 可逐字节比较。
 *
 * 两次成功的 `open` 保持 NsWitness 的授权解析计数（与 M5/M6 的 harness 等价）。
 */
#define _GNU_SOURCE

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <unistd.h>

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

int main(void) {
    int pid_before = getpid();
    int parent_before = getppid();

    long fd = open("f", O_RDWR | O_CREAT | O_TRUNC, 0600);
    int open_ok = fd >= 3 && fd <= 10;

    int dup_ok = 0;
    int share_ok = 0;
    int dup2_ok = 0;
    int dup3_ok = 0;
    int close_ok = 0;
    int self_ok = 0;
    int reuse_ok = 0;
    int bad_ok = 0;
    char buffer[8];

    if (open_ok && write((int)fd, "abcdef", 6) == 6 &&
        lseek((int)fd, 0, SEEK_SET) == 0) {
        long d1 = dup((int)fd);
        if (d1 >= 3 && d1 <= 10 && d1 != fd) {
            dup_ok = 1;
            /* 共享打开描述：经原 fd 读 3 字节后，dup 项的偏移同步到 3。 */
            ssize_t got = read((int)fd, buffer, 3);
            share_ok = got == 3 && same_bytes(buffer, "abc", 3) &&
                       lseek((int)d1, 0, SEEK_CUR) == 3;

            long d2 = dup2((int)fd, 9);
            if (d2 == 9 && write((int)d2, "Z", 1) == 1)
                dup2_ok = lseek((int)d1, 0, SEEK_CUR) == 4;

            errno = 0;
            int same_fd = dup3((int)fd, (int)fd, 0) == -1 && errno == EINVAL;
            errno = 0;
            int replaced = dup3((int)fd, 9, 0) == 9;
            errno = 0;
            int bad_flags = dup3((int)fd, 10, 1) == -1 && errno == EINVAL;
            dup3_ok = same_fd && replaced && bad_flags;

            /* 关闭原 fd 后，dup 项仍持有同一句柄并读到最后写入的内容。 */
            if (close((int)fd) == 0) {
                (void)lseek((int)d1, 0, SEEK_SET);
                got = read((int)d1, buffer, 6);
                close_ok = got == 6 && same_bytes(buffer, "abcZef", 6);
            }
            (void)close((int)d1);
            (void)close(9);
        }
    }

    /* 第二次 open：fd 号复用、偏移从 0 开始、只读句柄拒绝写。 */
    long fd2 = open("f", O_RDONLY);
    if (fd2 >= 3 && fd2 <= 10) {
        errno = 0;
        int read_only = write((int)fd2, "x", 1) == -1 && errno == EBADF;
        ssize_t got = read((int)fd2, buffer, 6);
        reuse_ok = read_only && got == 6 && same_bytes(buffer, "abcZef", 6);

        self_ok = dup2((int)fd2, (int)fd2) == fd2;

        int closed = close((int)fd2) == 0;
        errno = 0;
        int bad_lseek = lseek((int)fd2, 0, SEEK_SET) == -1 && errno == EBADF;
        errno = 0;
        int bad_dup = dup((int)fd2) == -1 && errno == EBADF;
        errno = 0;
        int bad_old = dup2(99, 5) == -1 && errno == EBADF;
        errno = 0;
        int bad_target = dup(-1) == -1 && errno == EBADF;
        bad_ok = closed && bad_lseek && bad_dup && bad_old && bad_target;
    }

    int pid_ok = pid_before > 0 && getpid() == pid_before;
    int ppid_ok = parent_before > 0 && parent_before != pid_before;

    /* M12b：O_CLOEXEC 被接受且不影响读写；本阶段仍无 exec 语义。 */
    int cloexec_ok = 0;
    long ce = open("f", O_RDONLY | O_CLOEXEC);
    if (ce >= 3 && read((int)ce, buffer, 1) == 1 && close((int)ce) == 0)
        cloexec_ok = 1;

    /* M12b：占满 8 个服务端句柄后直接退出，由适配层自动 CLOSE；
     * 管理器在退出后查询存储服务，期望 open_handles == 0。 */
    int held = 0;
    for (int index = 0; index < 8; index++) {
        long handle = open("f", O_RDONLY);
        if (handle >= 3 && handle <= 10)
            held++;
    }
    int held_ok = held == 8;

    int all_ok = open_ok && pid_ok && ppid_ok && dup_ok && share_ok &&
                 dup2_ok && dup3_ok && close_ok && self_ok && reuse_ok &&
                 bad_ok && cloexec_ok && held_ok;
    failures = !all_ok;
    int first = 1;
    emit("FD");
    field("ok", all_ok, &first);
    field("pid", pid_ok, &first);
    field("ppid", ppid_ok, &first);
    field("open", open_ok, &first);
    field("dup", dup_ok, &first);
    field("share", share_ok, &first);
    field("dup2", dup2_ok, &first);
    field("dup3", dup3_ok, &first);
    field("close", close_ok, &first);
    field("self", self_ok, &first);
    field("reuse", reuse_ok, &first);
    field("bad", bad_ok, &first);
    field("cloexec", cloexec_ok, &first);
    field("held", held_ok, &first);
    emit("\n");
    return failures ? 1 : 0;
}
