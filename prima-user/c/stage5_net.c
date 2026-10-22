/* M19：用户态 loopback IPv4/UDP 子集的同源探针。
 *
 * 同一份源码在 Linux 宿主（glibc）与 Prima（静态 musl + 适配层）上编译运行：
 * - 只使用 AF_INET/SOCK_DGRAM、127.0.0.1、bind(0)+getsockname 取端口；
 * - 覆盖双向数据报、源端口、零长度数据报、64 B 内的较大报文、未绑定目标（丢弃但成功）、
 *   端口占用、非法地址族与关闭后 EBADF；
 * - 不做 TCP、NIC、DNS、poll/select。
 *
 * 失败位写入 `NET|ok=0;...` 并以 1 退出；Linux 与 QEMU 都要求退出码 0。
 */
#define _GNU_SOURCE

#include <arpa/inet.h>
#include <errno.h>
#include <netinet/in.h>
#include <stdint.h>
#include <string.h>
#include <sys/socket.h>
#include <unistd.h>

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

static void fill_addr(struct sockaddr_in *addr, unsigned short port) {
    memset(addr, 0, sizeof *addr);
    addr->sin_family = AF_INET;
    addr->sin_port = htons(port);
    addr->sin_addr.s_addr = htonl(INADDR_LOOPBACK);
}

static int bind_ephemeral(int fd, unsigned short *port) {
    struct sockaddr_in addr;
    fill_addr(&addr, 0);
    if (bind(fd, (struct sockaddr *)&addr, sizeof addr) != 0)
        return 0;
    socklen_t len = sizeof addr;
    if (getsockname(fd, (struct sockaddr *)&addr, &len) != 0)
        return 0;
    *port = ntohs(addr.sin_port);
    return *port != 0;
}

static long send_to(int fd, unsigned short port, const void *buffer,
                    unsigned long len) {
    struct sockaddr_in addr;
    fill_addr(&addr, port);
    return sendto(fd, buffer, len, 0, (struct sockaddr *)&addr, sizeof addr);
}

static long recv_from(int fd, void *buffer, unsigned long len,
                      unsigned short *source) {
    struct sockaddr_in addr;
    socklen_t addrlen = sizeof addr;
    long got = recvfrom(fd, buffer, len, 0, (struct sockaddr *)&addr, &addrlen);
    if (got < 0)
        return got;
    if (addrlen >= sizeof addr && addr.sin_family == AF_INET)
        *source = ntohs(addr.sin_port);
    else
        *source = 0;
    return got;
}

int main(void) {
    emit("NET");

    int a = socket(AF_INET, SOCK_DGRAM, 0);
    int b = socket(AF_INET, SOCK_DGRAM, 0);
    unsigned short port_a = 0, port_b = 0;
    int bound = a >= 0 && b >= 0 && bind_ephemeral(a, &port_a) &&
                bind_ephemeral(b, &port_b) && port_a != port_b;
    char buffer[64];
    unsigned short source = 0;

    /* 1. 双向数据报与源端口。 */
    int pair = bound && send_to(a, port_b, "hello", 5) == 5 &&
               recv_from(b, buffer, sizeof buffer, &source) == 5 &&
               same_bytes(buffer, "hello", 5) && source == port_a &&
               send_to(b, port_a, "world", 5) == 5 &&
               recv_from(a, buffer, sizeof buffer, &source) == 5 &&
               same_bytes(buffer, "world", 5) && source == port_b;
    field("pair", pair, 0);

    /* 2. 零长度数据报保留边界与源端口。 */
    int zero = bound && send_to(a, port_b, buffer, 0) == 0 &&
               recv_from(b, buffer, sizeof buffer, &source) == 0 &&
               source == port_a;
    field("zero", zero, 0);

    /* 3. 20 B 报文逐字节完整。 */
    char large[20];
    for (int i = 0; i < 20; i++)
        large[i] = (char)('A' + i);
    int large_ok = bound && send_to(a, port_b, large, 20) == 20 &&
                   recv_from(b, buffer, sizeof buffer, &source) == 20 &&
                   same_bytes(buffer, large, 20);
    field("large", large_ok, 0);

    /* 4. 目标端口未绑定：UDP 丢弃报文但 sendto 成功。 */
    unsigned short unbound = (unsigned short)(port_b + 1);
    if (unbound == port_a || unbound == port_b)
        unbound = (unsigned short)(port_b + 2);
    int drop = bound && send_to(a, unbound, "x", 1) == 1;
    field("drop", drop, 0);

    /* 5. 端口占用：bind 到 b 的端口返回 EADDRINUSE。 */
    int c = socket(AF_INET, SOCK_DGRAM, 0);
    struct sockaddr_in reuse_addr;
    fill_addr(&reuse_addr, port_b);
    int reuse = c >= 0 &&
                bind(c, (struct sockaddr *)&reuse_addr, sizeof reuse_addr) == -1 &&
                errno == EADDRINUSE;
    field("reuse", reuse, errno);
    if (c >= 0)
        close(c);

    /* 6. 非法地址族确定失败。 */
    int badfam = socket(12345, SOCK_DGRAM, 0) == -1 && errno == EAFNOSUPPORT;
    field("badfam", badfam, errno);

    /* 7. close 后 sendto 返回 EBADF。 */
    int closed = 0;
    if (a >= 0) {
        close(a);
        a = -1;
        closed = send_to(3, port_b, "x", 1) == -1 && errno == EBADF;
    }
    field("closed", closed, errno);

    if (b >= 0)
        close(b);
    emit("|ok=");
    emit_dec(failures ? 0 : 1);
    emit(";err=");
    emit_dec(first_error);
    emit("\n");
    return failures ? 1 : 0;
}
