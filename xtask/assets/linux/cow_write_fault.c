/*
 * 切片 P：Linux 侧的 `cow-write-fault` 场景契约实现。
 *
 * 契约（与 Prima 侧 `kernel/src/user.rs` 的 ring3 用户任务同构）：
 *   1. 一个 present、可写、用户可访问的页，初值 pattern(i) = 0xC0FFEE00_0000_0000 | i；
 *   2. fork 后父子两侧 PTE 被置为只读，但 VMA 仍可写；
 *   3. 子进程写 8 字节 VAL → COW 写保护故障（P|W|US → error_code=0x7）；
 *   4. 内核建立私有副本，重试写成功；
 *   5. 子进程读回 VAL、页内其余内容仍为 pattern(1)；父进程原副本仍为 pattern(0)。
 *
 * 输出以 `LINUX|event=<name>;value=<hex>;ok=<0|1>` 归一化行写到 stdout（串口），
 * 故障事实由内核 `exceptions:page_fault_user` tracepoint 独立记录。
 */
#define _GNU_SOURCE
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/mman.h>
#include <sys/wait.h>
#include <unistd.h>

#define PATTERN(i) (0xC0FFEE0000000000ULL | (uint64_t)(i))
#define VAL 0x5A5A1234DEADBEEFULL
#define PAGE_SIZE 4096
#define PAGE_WORDS 512

/* 避开 stdio：fork 后用 write(2) 直接落串口，防止缓冲继承导致的重复/丢失。 */
static void emit(const char *name, uint64_t value, int ok) {
    char buffer[128];
    int len = snprintf(buffer, sizeof(buffer), "LINUX|event=%s;value=0x%llx;ok=%d\n", name,
                       (unsigned long long)value, ok);
    if (len > 0) {
        ssize_t written = write(1, buffer, (size_t)len);
        (void)written;
    }
}

int main(void) {
    /* MAP_POPULATE 先把页建立为 present，使目标页只出现 fork 后的 COW 写保护故障；
     * 否则首次触碰会额外产生 not_present 的 0x6 故障。 */
    volatile uint64_t *page = mmap(NULL, PAGE_SIZE, PROT_READ | PROT_WRITE,
                                   MAP_PRIVATE | MAP_ANONYMOUS | MAP_POPULATE, -1, 0);
    if (page == MAP_FAILED) {
        perror("mmap");
        return 1;
    }
    for (int i = 0; i < PAGE_WORDS; i++) {
        page[i] = PATTERN(i);
    }
    printf("LINUX|event=page;value=0x%llx\n", (unsigned long long)(uintptr_t)page);
    fflush(stdout);

    pid_t pid = fork();
    if (pid == 0) {
        page[0] = VAL; /* COW 写保护故障发生点 */
        uint64_t readback = page[0];
        uint64_t rest = page[1];
        emit("write_retried_ok", readback, readback == VAL);
        emit("page_rest_intact", rest, rest == PATTERN(1));
        _exit(0);
    }
    if (pid < 0) {
        perror("fork");
        return 1;
    }

    int status = 0;
    waitpid(pid, &status, 0);
    uint64_t original = page[0];
    emit("original_unchanged", original, original == PATTERN(0));
    emit("child_status", (uint64_t)status, WIFEXITED(status) && WEXITSTATUS(status) == 0);
    return 0;
}
