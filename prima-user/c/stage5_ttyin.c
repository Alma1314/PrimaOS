/* M14：fd 0 tty 输入（canonical/raw/EOF/termios）的同源探针。
 *
 * Prima 侧：读取 COM1 输入，经管理器行规程；markers 供 xtask 注入串口输入。
 * Linux 参考：xtask 用管道的 stdin 提供同一读路径，验证 canonical 行与 EOF；
 *           termios/回显/raw 只能在真实 tty 上验证，差异在报告中登记。
 *
 * 失败位写入 `TTYIN|ok=0;...` 并以 1 退出；Linux 与 QEMU 都要求退出码 0。
 */
#define _GNU_SOURCE

#include <errno.h>
#include <stdint.h>
#include <termios.h>
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

int main(void) {
    emit("TTYRD");
    char buffer[16];

#ifdef PRIMA_HOST
    /* 管道 stdin：一行 + EOF。 */
    long line = read(0, buffer, sizeof buffer);
    int line_ok = line == 3 && same_bytes(buffer, "hi\n", 3);
    field("line", line_ok, line);
    long eof = read(0, buffer, sizeof buffer);
    field("eof", eof == 0, eof);
#else
    struct termios term;
    int term_ok = tcgetattr(0, &term) == 0 &&
                  (term.c_lflag & ICANON) != 0 && (term.c_lflag & ECHO) != 0;
    field("termios", term_ok, errno);

    emit("\nTTYREADY\n");
    long line = read(0, buffer, sizeof buffer);
    int line_ok = line == 3 && same_bytes(buffer, "hi\n", 3);
    field("line", line_ok, line);

    struct termios raw = term;
    raw.c_lflag &= (tcflag_t)~(ICANON | ECHO);
    int raw_set = tcsetattr(0, TCSANOW, &raw) == 0;
    emit("\nTTYRAW\n");
    long raw_len = 0;
    while (raw_len < 2) {
        long got = read(0, buffer + raw_len, (unsigned long)(2 - raw_len));
        if (got <= 0)
            break;
        raw_len += got;
    }
    int raw_ok = raw_set && raw_len == 2 && same_bytes(buffer, "xy", 2);
    field("raw", raw_ok, raw_len);

    int restore = tcsetattr(0, TCSANOW, &term) == 0;
    emit("\nTTYEOF\n");
    long eof = read(0, buffer, sizeof buffer);
    field("eof", restore && eof == 0, eof);
#endif

    emit("|ok=");
    emit_dec(failures ? 0 : 1);
    emit(";err=");
    emit_dec(first_error);
    emit("\n");
    return failures ? 1 : 0;
}
