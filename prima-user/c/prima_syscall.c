/* M2：Prima C 系统边界适配层。
 *
 * 边界规则：
 * - Linux 调用号只在本文件出现，翻译后只向内核发 Prima 原生号；
 * - 所有失败返回原始负 errno（不设置 errno，musl 的 syscall 约定）；
 * - fd 0 是 tty 输入、fd 1/2 是管理器的 tty 输出；fd 3..18 是通用
 *   本地表，文件/目录项经用户态 VFS 存储服务；
 * - tty SEND 后 YIELD；退出上报后停驻 RECV，由管理器消费状态并销毁任务。
 */
#include "prima_syscall.h"
#include <stddef.h>
#include "prima_contract.h"

/* retain survives --gc-sections; const volatile also forces crt's version read. */
__attribute__((used, retain, section(".note.prima.abi"), aligned(4)))
const volatile uint32_t prima_libc_contract_v1_0[11] = PRIMA_CONTRACT_WORDS;

struct prima_entry_ctx prima_ctx;
/* Private COW state: a fork child exits via its Process, never the inherited
 * manager/self capability. Descriptor close isolation is still a separate issue. */
static int prima_fork_child;

static void prima_reap_detached(void);
static void prima_futex_cleanup(void);
static long prima_futex_current_cap(void);
static int prima_memory_lock;
static long prima_futex(int *uaddr, int op, int value, const void *timeout);

static long prima_clone(unsigned long flags, void *stack, int *ptid, int *ctid, unsigned long tls);
__attribute__((noreturn)) static void prima_thread_exit(void);

/* 六参数陷入：号在 rax，参数依次 rdi/rsi/rdx/r10/r8/r9（Prima ABI）。 */
static long prima_raw(long number, long a0, long a1, long a2, long a3, long a4,
                      long a5) {
    register long r10 __asm__("r10") = a3;
    register long r8 __asm__("r8") = a4;
    register long r9 __asm__("r9") = a5;
    register long rax __asm__("rax") = number;
    __asm__ volatile("syscall"
                     : "+a"(rax)
                     : "D"(a0), "S"(a1), "d"(a2), "r"(r10), "r"(r8), "r"(r9)
                     : "rcx", "r11", "memory");
    return rax;
}

/* Linux layouts live at this boundary; native upcalls carry only a neutral
 * register image, mask, provenance and FXSAVE bytes. */
#define PRIMA_SYS_UPCALL_CONTROL 65
#define PRIMA_SIG_BLOCK 0
#define PRIMA_SIG_UNBLOCK 1
#define PRIMA_SIG_SETMASK 2
#define PRIMA_SA_SIGINFO UINT64_C(4)
#define PRIMA_SA_RESTORER UINT64_C(0x04000000)
#define PRIMA_SA_ONSTACK UINT64_C(0x08000000)
#define PRIMA_SA_RESTART UINT64_C(0x10000000)
#define PRIMA_SA_NODEFER UINT64_C(0x40000000)
#define PRIMA_SA_RESETHAND UINT64_C(0x80000000)
#define PRIMA_UNMASKABLE ((UINT64_C(1) << 8) | (UINT64_C(1) << 18))
struct prima_k_sigaction { uint64_t handler, flags, restorer, mask; };
struct prima_signal_context {
    uint64_t regs[20], mask, slot, sender, kind, value, token, fp[64];
} __attribute__((aligned(16)));
struct prima_linux_siginfo { int number, error, code, pad; unsigned char fields[112]; } __attribute__((aligned(8)));
struct prima_linux_ucontext {
    uint64_t flags, link;
    struct { uint64_t pointer; int flags, pad; uint64_t length; } stack;
    struct { uint64_t regs[23], fpregs, reserved[8]; } machine;
    uint64_t mask[16], fp_mem[64];
};
_Static_assert(sizeof(struct prima_signal_context) == 720, "native upcall context");
_Static_assert(sizeof(struct prima_linux_siginfo) == 128, "Linux x86_64 siginfo");
_Static_assert(sizeof(struct prima_linux_ucontext) == 936, "musl x86_64 ucontext");
extern void __restore_rt(void) __attribute__((weak));
extern void __restore(void) __attribute__((weak));
static struct prima_k_sigaction prima_actions[65];
static unsigned char prima_action_valid[65];
static int prima_signal_ready, prima_signal_lock;
/* One receive scope per nested callback. Mailbox restart is resolved here so a
 * callback that receives an outer read's reply cannot strand the outer syscall. */
struct prima_restart_scope { struct prima_restart_scope *previous; unsigned seen, stop; };
static struct prima_restart_scope *prima_restart_scopes[18];
static long prima_receive_meta(uint64_t out[5], uint16_t sequence, int cancel_ack);
static long prima_receive_meta_for(uint64_t out[5], uint16_t sequence, int cancel_ack, unsigned net_sequence);
static int prima_tty_active(void);
static int prima_net_active(void);
static long prima_tty_send(uint64_t payload);
static long prima_net_status(uint64_t reply);
static int prima_fd_lock;


__attribute__((noreturn)) static void prima_signal_trampoline(long slot, uint64_t token,
                                                            uint64_t callback, uint64_t flags) {
    struct prima_signal_context native;
    long result = prima_raw(PRIMA_SYS_UPCALL_CONTROL, 4, token, (long)&native, 0, 0, 0);
    if (result < 0) {
        (void)prima_raw(PRIMA_SYS_SIGRETURN, 0, 0, 0, 0, 0, 0);
        __builtin_trap();
    }
    struct prima_linux_siginfo info = {0};
    struct prima_linux_ucontext context = {0};
    static const unsigned char greg[18] = {13,11,14,12,9,8,10,15,0,1,2,3,4,5,6,7,17,16};
    for (unsigned i=0; i<18; i++) context.machine.regs[greg[i]] = native.regs[i];
    context.machine.regs[18] = native.regs[18];
    context.machine.regs[21] = native.mask;
    context.machine.fpregs = (uint64_t)native.fp;
    context.mask[0] = native.mask;
    context.stack.flags = 2; /* SS_DISABLE: this profile has no alternate stack. */
    for (unsigned i=0; i<64; i++) context.fp_mem[i] = native.fp[i];
    info.number = (int)slot;
    info.code = native.kind == 1 ? -6 : 0; /* SI_TKILL / SI_USER */
    uint32_t pid = (uint32_t)native.sender;
    for (unsigned i=0; i<4; i++) info.fields[i] = (unsigned char)(pid >> (8*i));
    if (native.kind == 2) {
        info.code = native.value >> 63 ? 2 : 1; /* CLD_KILLED / CLD_EXITED */
        uint32_t status = (uint32_t)(native.value & 0xff);
        for (unsigned i=0; i<4; i++) info.fields[8+i] = (unsigned char)(status >> (8*i));
    } else if (native.kind >= 3 && native.kind <= 8) {
        info.code = native.kind == 4 ? 2 : native.kind == 7 ? 7 : 1;
        context.machine.regs[22] = native.value;
        for (unsigned i=0; i<8; i++) info.fields[i] = (unsigned char)(native.value >> (8*i));
    }
    long task = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    if (task > 0 && task < 18 && prima_restart_scopes[task]) {
        prima_restart_scopes[task]->seen = 1;
        if (!(flags & 4)) prima_restart_scopes[task]->stop = 1;
    }
    if (callback > 1) {
        if (flags & 16) ((void (*)(int, void *, void *))callback)((int)slot, &info, &context);
        else ((void (*)(int))callback)((int)slot);
    }
    for (unsigned i=0; i<18; i++) native.regs[i] = context.machine.regs[greg[i]];
    native.mask = context.mask[0] & ~PRIMA_UNMASKABLE;
    const unsigned char *fp = (const unsigned char *)context.machine.fpregs;
    if (fp) for (unsigned i=0; i<512; i++) ((unsigned char *)native.fp)[i] = fp[i];
    result = prima_raw(PRIMA_SYS_UPCALL_CONTROL, 5, token, (long)&native, 0, 0, 0);
    (void)result;
    __builtin_trap();
}
static int prima_default_ignored(int slot) { return slot == 17 || slot == 23 || slot == 28; }
static uint64_t prima_upcall_flags(int slot, uint64_t flags) {
    return 32 | ((flags & PRIMA_SA_NODEFER) ? 1 : 0) |
           ((flags & PRIMA_SA_RESETHAND) ? 2 : 0) |
           /* musl 1.2.5: SIGTIMER=32, SIGCANCEL=33, SIGSYNCCALL=34. */
           ((flags & PRIMA_SA_RESTART) && slot != 33 ? 4 : 0) |
           (prima_default_ignored(slot) ? 8 : 0) |
           ((flags & PRIMA_SA_SIGINFO) ? 16 : 0);
}
static void prima_signals_ready(void) {
    if (prima_signal_ready) return;
    for (int slot=1; slot<=64; slot++) if (prima_default_ignored(slot)) {
        uint64_t action[4] = {0};
        (void)prima_raw(PRIMA_SYS_UPCALL_CONTROL, 7, slot, (long)action, 0, 0, 0);
        if (!action[0]) {
            (void)prima_raw(PRIMA_SYS_UPCALL_CONTROL, 0, slot, 0, 0, 8, 0);
            prima_action_valid[slot] = 1;
        }
    }
    (void)prima_raw(PRIMA_SYS_UPCALL_CONTROL, 8, 0, 17, 0, 0, 0);
    (void)prima_raw(PRIMA_SYS_UPCALL_CONTROL, 8, 1, 11, 0, 0, 0);
    (void)prima_raw(PRIMA_SYS_UPCALL_CONTROL, 8, 2, 4, 0, 0, 0);
    (void)prima_raw(PRIMA_SYS_UPCALL_CONTROL, 8, 3, 8, 0, 0, 0);
    (void)prima_raw(PRIMA_SYS_UPCALL_CONTROL, 8, 4, 7, 0, 0, 0);
    prima_signal_ready = 1;
}
static long prima_rt_sigprocmask(long how, const uint64_t *set, uint64_t *old, long size) {
    if (size != 8) return -PRIMA_ERRNO_EINVAL;
    uint64_t mask = set ? *set & ~PRIMA_UNMASKABLE : 0;
    return prima_raw(PRIMA_SYS_UPCALL_CONTROL, 1, how, set ? (long)&mask : 0, (long)old, 0, 0);
}
static uint64_t prima_memory_enter(void) {
    uint64_t old = 0, full = ~PRIMA_UNMASKABLE;
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &old, 8);
    while (__sync_lock_test_and_set(&prima_memory_lock, 1))
        (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    prima_reap_detached();
    return old;
}
static long prima_memory_leave(uint64_t old, long result) {
    __sync_lock_release(&prima_memory_lock);
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &old, 0, 8);
    return result;
}
static long prima_rt_sigaction(int slot, const struct prima_k_sigaction *action,
                              struct prima_k_sigaction *old, long size) {
    if (size != 8 || slot < 1 || slot > 34 || slot == 9 || slot == 19) return -PRIMA_ERRNO_EINVAL;
    const uint64_t allowed = PRIMA_SA_SIGINFO | PRIMA_SA_RESTORER | PRIMA_SA_ONSTACK |
                             PRIMA_SA_RESTART | PRIMA_SA_NODEFER | PRIMA_SA_RESETHAND | 1;
    if (action && (action->flags & ~allowed)) return -PRIMA_ERRNO_ENOSYS;
    if (action && (action->flags & PRIMA_SA_RESTORER) && action->restorer &&
        action->restorer != (uint64_t)&__restore_rt && action->restorer != (uint64_t)&__restore)
        return -PRIMA_ERRNO_ENOSYS;
    uint64_t full = ~PRIMA_UNMASKABLE, previous;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &previous, 8);
    if (result < 0) return result;
    while (__sync_lock_test_and_set(&prima_signal_lock, 1))
        (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    uint64_t wire[4] = {0};
    result = prima_raw(PRIMA_SYS_UPCALL_CONTROL, 7, slot, (long)wire, 0, 0, 0);
    if (!result && old) {
        if (prima_action_valid[slot] && wire[3] == prima_actions[slot].handler &&
            wire[1] == prima_actions[slot].mask && wire[2] == prima_upcall_flags(slot, prima_actions[slot].flags))
            *old = prima_actions[slot];
        else {
            old->handler = wire[0] == (uint64_t)&prima_signal_trampoline ? wire[3] : wire[0];
            old->mask = wire[1]; old->restorer = 0;
            old->flags = ((wire[2] & 1) ? PRIMA_SA_NODEFER : 0) |
                         ((wire[2] & 2) ? PRIMA_SA_RESETHAND : 0) |
                         ((wire[2] & 4) ? PRIMA_SA_RESTART : 0) |
                         ((wire[2] & 16) ? PRIMA_SA_SIGINFO : 0);
        }
    }
    if (!result && action) {
        struct prima_k_sigaction copy = *action;
        copy.mask &= ~PRIMA_UNMASKABLE;
        uint64_t handler = copy.handler > 1 ? (uint64_t)&prima_signal_trampoline : copy.handler;
        result = prima_raw(PRIMA_SYS_UPCALL_CONTROL, 0, slot, handler, copy.mask,
                           prima_upcall_flags(slot, copy.flags), copy.handler);
        if (!result) { prima_actions[slot] = copy; prima_action_valid[slot] = 1; }
    }
    __sync_lock_release(&prima_signal_lock);
    long restored = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &previous, 0, 8);
    return result < 0 ? result : restored;
}
static long prima_kill(long pid, long slot, int thread_directed, long group) {
    if (pid <= 0 || slot < 0 || slot > 34) return -PRIMA_ERRNO_EINVAL;
    /* Job-control stops/continues remain outside this profile. */
    if (slot == 18 || (slot >= 19 && slot <= 22)) return -PRIMA_ERRNO_ENOSYS;
    if (group > 0) {
        long cookie = prima_raw(PRIMA_SYS_TASK_INFO, 2, pid, 0, 0, 0, 0);
        if (cookie < 0 || (uint32_t)cookie != (uint32_t)group) return -PRIMA_ERRNO_ENOENT;
    }
    if (slot == 9) return prima_raw(PRIMA_SYS_SIGNAL, pid, slot, 0, 0, 0, 0);
    return prima_raw(PRIMA_SYS_UPCALL_CONTROL, 3, pid, slot, thread_directed, 0, 0);
}

long prima_native_syscall0(long number) {
    return prima_raw(number, 0, 0, 0, 0, 0, 0);
}

long prima_native_syscall3(long number, long a0, long a1, long a2) {
    return prima_raw(number, a0, a1, a2, 0, 0, 0);
}

long prima_native_syscall4(long number, long a0, long a1, long a2, long a3) {
    return prima_raw(number, a0, a1, a2, a3, 0, 0);
}

/* M3b/M10：musl 的匿名内存与保护。
 *
 * 服务管理器在创建 C 任务时把 104 页 arena 与栈作为**同一 Segment** 的两段映射进本
 * 任务根：前 8 页是 C 栈，随后 96 页堆（heap page 0..95）。适配层把堆切成两个区：
 * 首 40 页作 `brk` 区（mallocng 的元数据区），其余 56 页作匿名 `mmap` 区。
 *
 * M10 起 `mmap/munmap/mprotect` 不再只是指针推进：
 * - `mmap` 在 56 页区里做按页 first-fit 分配；新页清零；`MAP_FIXED` 只接受堆区
 *   （mallocng 的 brk guard 页）或 mmap 区内的地址。
 * - 首次 mmap 会向管理器申请 arena Segment 的派生映射授权（`Derive|SegMapR|SegMapW`），
 *   之后 `munmap` 用 `SYS_MEMSEG_UNMAP` 真正拆 PTE，`mmap` 复用地址时用
 *   `SYS_MEMSEG_MAP` 重新映射。
 * - `mprotect` 走内核 `SYS_MEMSEG_PROTECT=41`；RW→R 的收缩不需要额外授权，R→RW
 *   需要上面的 Segment cap。W^X：匿名段 allowed_access 只有 RW，故任何 `PROT_EXEC`
 *   请求返回 `EACCES`。
 *
 * 已知边界（M10 报告登记）：
 * - `PROT_NONE` 由原生68禁用用户访问，保留物理帧、逻辑权限与COW状态；
 * - 无文件映射、无 `MAP_SHARED`、无 `mremap`；`MAP_FIXED` 的地址必须在本 arena 内；
 * - 单次匿名映射上限 56 页；brk 区不在本 mmap 分配器内，`brk` 仍是 40 页上限；
 * - 管理器未授予 Segment cap 时退化为“只 free 不 unmap”，R→RW 的 mprotect 返回 `EACCES`。
 */
#define PRIMA_HEAP_VA UINT64_C(0x00000c0000000000)
#define PRIMA_HEAP_PAGES 96UL
#define PRIMA_BRK_PAGES 40UL
#define PRIMA_MMAP_PAGES (PRIMA_HEAP_PAGES - PRIMA_BRK_PAGES)
#define PRIMA_PAGE_SIZE 4096UL
#define PRIMA_MMAP_BASE (PRIMA_HEAP_VA + PRIMA_BRK_PAGES * PRIMA_PAGE_SIZE)
#define PRIMA_MMAP_LIMIT (PRIMA_HEAP_VA + PRIMA_HEAP_PAGES * PRIMA_PAGE_SIZE)
/* heap page i 对应 arena Segment 偏移 i + 8；8 是 servicemgr 的 C_STACK_PAGES。 */
#define PRIMA_SEG_HEAP_OFFSET 8UL
#define PRIMA_MMAP_BYTES ((PRIMA_MMAP_PAGES + 7) / 8)

#define PRIMA_MAP_SHARED 0x01
#define PRIMA_MAP_PRIVATE 0x02
#define PRIMA_MAP_FIXED 0x10
#define PRIMA_MAP_ANONYMOUS 0x20
#define PRIMA_PROT_READ 0x1
#define PRIMA_PROT_WRITE 0x2
#define PRIMA_PROT_EXEC 0x4

static uint64_t prima_brk_cur;
static unsigned char prima_mmap_used[PRIMA_MMAP_BYTES];
static unsigned char prima_mmap_present[PRIMA_MMAP_BYTES];
static uint64_t prima_arena_cap;
static int prima_arena_state; /* 0 = 未申请，1 = 已获得，-1 = 不可用 */

static int prima_bit_get(const unsigned char *map, unsigned page) {
    return (map[page / 8] >> (page % 8)) & 1;
}

static void prima_bit_set(unsigned char *map, unsigned page, int value) {
    if (value)
        map[page / 8] |= (unsigned char)(1u << (page % 8));
    else
        map[page / 8] &= (unsigned char)~(1u << (page % 8));
}

/* arena 由服务管理器预先映射；这里只初始化分配位图。 */
static void prima_heap_ready(void) {
    if (prima_brk_cur != 0)
        return;
    prima_brk_cur = PRIMA_HEAP_VA;
    for (unsigned i = 0; i < PRIMA_MMAP_BYTES; i++) {
        prima_mmap_used[i] = 0;
        prima_mmap_present[i] = 0xff;
    }
}

struct prima_detached_range { uint64_t cookie, base, length; };
static struct prima_detached_range prima_detached[18];
static void prima_reap_detached(void) {
    for (unsigned i=0; i<18; i++) {
        struct prima_detached_range *range = &prima_detached[i];
        if (!range->cookie) continue;
        long live = prima_raw(PRIMA_SYS_TASK_INFO, 1, (uint32_t)range->cookie, 0, 0, 0, 0);
        if (live >= 0 && (uint64_t)live == range->cookie) continue;
        unsigned start = (unsigned)((range->base - PRIMA_MMAP_BASE) / PRIMA_PAGE_SIZE);
        unsigned count = (unsigned)(range->length / PRIMA_PAGE_SIZE);
        for (unsigned page=start; page<start+count; page++) {
            prima_bit_set(prima_mmap_used, page, 0);
            prima_bit_set(prima_mmap_present, page, 0);
        }
        range->cookie = 0;
    }
}
static long prima_brk(uint64_t requested) {
    prima_heap_ready();
    if (requested == 0)
        return (long)prima_brk_cur;
    if (requested < PRIMA_HEAP_VA ||
        requested > PRIMA_HEAP_VA + PRIMA_BRK_PAGES * PRIMA_PAGE_SIZE)
        return (long)prima_brk_cur;
    prima_brk_cur = requested;
    return (long)requested;
}

/* 首次 mmap 向管理器申请 arena 的派生 cap；失败后本任务退化为“只 free 不 unmap”。 */
static int prima_arena_acquire(void) {
    if (prima_arena_state != 0)
        return prima_arena_state > 0;
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload =
        PRIMA_STAGE5_VMA_CAP_REQUEST_TAG |
        ((uint64_t)(id & 0xff) << PRIMA_VMA_CLIENT_SHIFT);
    long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                          (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    if (sent != 0) {
        prima_arena_state = -1;
        return 0;
    }
    uint64_t caps[5];
    uint64_t manager = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 4, prima_ctx.send_cap, 0, 0, 0, 0);
    for (;;) {
        long raw = prima_receive_meta(caps, 0, 0);
        if (raw < 0) {
            prima_arena_state = -1;
            return 0;
        }
        uint64_t reply = (uint64_t)raw;
        if (caps[3] == manager && (reply >> 48) == (PRIMA_STAGE5_VMA_CAP_REPLY_TAG >> 48)) {
            if (((reply >> PRIMA_VMA_STATUS_SHIFT) & 0xff) == 0 &&
                caps[2] >= 1 && caps[0] != 0) {
                prima_arena_cap = caps[0];
                prima_arena_state = 1;
                return 1;
            }
            prima_arena_state = -1;
            return 0;
        }
        /* 申请期间到达的非 VMA 消息不属于本通道；当前协议无并发请求，直接忽略。 */
    }
}

/* PROT 位 → COSIR 访问集；PROT_NONE 取最小非空访问集 R，PROT_EXEC 在匿名段不可满足。 */
static long prima_prot_access(long prot) {
    if ((prot & ~(long)(PRIMA_PROT_READ | PRIMA_PROT_WRITE | PRIMA_PROT_EXEC)) != 0)
        return -PRIMA_ERRNO_EINVAL;
    if ((prot & PRIMA_PROT_EXEC) != 0)
        return -PRIMA_ERRNO_EACCES;
    long access = 0;
    if ((prot & PRIMA_PROT_READ) != 0)
        access |= PRIMA_ACCESS_R;
    if ((prot & PRIMA_PROT_WRITE) != 0)
        access |= PRIMA_ACCESS_W;
    if (access == 0)
        access = PRIMA_ACCESS_R;
    return access;
}

/* 映射 index 起的连续 pages 页；返回 0 或原始负 errno。 */
static long prima_arena_map_pages(unsigned index, unsigned pages, long access) {
    if (pages == 0)
        return 0;
    if (prima_arena_cap == 0)
        return -PRIMA_ERRNO_ENOMEM;
    uint64_t va = PRIMA_MMAP_BASE + (uint64_t)index * PRIMA_PAGE_SIZE;
    uint64_t seg_offset =
        PRIMA_SEG_HEAP_OFFSET + PRIMA_BRK_PAGES + (uint64_t)index;
    return prima_raw(PRIMA_SYS_MEMSEG_MAP, (long)prima_arena_cap,
                     (long)prima_ctx.self_cap, (long)seg_offset, (long)va,
                     (long)pages, access);
}

/* 真正拆除 index 起的连续 pages 页；无 cap 时保留映射（退化模式）。 */
static long prima_arena_unmap_pages(unsigned index, unsigned pages) {
    if (pages == 0 || prima_arena_cap == 0)
        return 0;
    uint64_t va = PRIMA_MMAP_BASE + (uint64_t)index * PRIMA_PAGE_SIZE;
    return prima_raw(PRIMA_SYS_MEMSEG_UNMAP, (long)prima_ctx.self_cap, (long)va,
                     (long)pages, 0, 0, 0);
}

/* 释放 mmap 区 [index, index+pages) 的占用；present 页整段拆映射。 */
static long prima_arena_release(unsigned index, unsigned pages) {
    for (unsigned page = index; page < index + pages; page++)
        prima_bit_set(prima_mmap_used, page, 0);
    unsigned cursor = index;
    while (cursor < index + pages) {
        if (!prima_bit_get(prima_mmap_present, cursor)) {
            cursor++;
            continue;
        }
        unsigned start = cursor;
        while (cursor < index + pages &&
               prima_bit_get(prima_mmap_present, cursor))
            cursor++;
        long result = prima_arena_unmap_pages(start, cursor - start);
        if (result != 0)
            return result;
        /* 退化模式（无 cap）没有真正拆映射，present 必须保持 1。 */
        if (prima_arena_cap != 0) {
            for (unsigned page = start; page < cursor; page++)
                prima_bit_set(prima_mmap_present, page, 0);
        }
    }
    return 0;
}

static int prima_find_free_run(unsigned pages) {
    unsigned run = 0;
    for (unsigned page = 0; page < PRIMA_MMAP_PAGES; page++) {
        if (!prima_bit_get(prima_mmap_used, page)) {
            run++;
            if (run == pages)
                return (int)(page + 1 - pages);
        } else {
            run = 0;
        }
    }
    return -1;
}

static void prima_zero(uint64_t address, uint64_t length) {
    for (uint64_t offset = 0; offset < length; offset += 8)
        *(volatile uint64_t *)(address + offset) = 0;
}

static long prima_mmap_anon(long requested_address, uint64_t length, long prot,
                            long flags, long fd, long offset) {
    if ((flags & PRIMA_MAP_ANONYMOUS) == 0 || (flags & PRIMA_MAP_SHARED) != 0 ||
        (flags & PRIMA_MAP_PRIVATE) == 0 || fd != -1 || offset != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (length == 0)
        return -PRIMA_ERRNO_EINVAL;
    if (length > UINT64_MAX - (PRIMA_PAGE_SIZE - 1))
        return -PRIMA_ERRNO_ENOMEM;
    uint64_t pages64 = (length + PRIMA_PAGE_SIZE - 1) / PRIMA_PAGE_SIZE;
    if (pages64 == 0 || pages64 > PRIMA_MMAP_PAGES)
        return -PRIMA_ERRNO_ENOMEM;
    unsigned pages = (unsigned)pages64;
    long access = prima_prot_access(prot);
    if (access < 0)
        return access;
    prima_heap_ready();
    (void)prima_arena_acquire();
    if (!prima_arena_cap) return -PRIMA_ERRNO_EACCES;

    unsigned index;
    uint64_t base;
    if ((flags & PRIMA_MAP_FIXED) != 0) {
        if (((uint64_t)requested_address & (PRIMA_PAGE_SIZE - 1)) != 0)
            return -PRIMA_ERRNO_EINVAL;
        base = (uint64_t)requested_address;
        /* mallocng 在 brk 区首页安装 guard：该区已映射，按原地址成功返回。 */
        if (base >= PRIMA_HEAP_VA && base < PRIMA_MMAP_BASE &&
            pages64 * PRIMA_PAGE_SIZE <= PRIMA_MMAP_BASE - base) {
            if (prot != 0) return -PRIMA_ERRNO_EINVAL;
            long result = prima_raw(PRIMA_SYS_VSPACE_ACCESS_ENABLE, prima_ctx.self_cap,
                                    base, pages, 0, 0, 0);
            return result < 0 ? result : requested_address;
        }
        if (base < PRIMA_MMAP_BASE || base >= PRIMA_MMAP_LIMIT ||
            base + pages64 * PRIMA_PAGE_SIZE > PRIMA_MMAP_LIMIT ||
            base + pages64 * PRIMA_PAGE_SIZE < base)
            return -PRIMA_ERRNO_ENOMEM;
        index = (unsigned)((base - PRIMA_MMAP_BASE) / PRIMA_PAGE_SIZE);
        long released = prima_arena_release(index, pages);
        if (released != 0)
            return released;
    } else {
        int found = prima_find_free_run(pages);
        if (found < 0)
            return -PRIMA_ERRNO_ENOMEM;
        index = (unsigned)found;
        base = PRIMA_MMAP_BASE + (uint64_t)index * PRIMA_PAGE_SIZE;
    }

    unsigned char was_present[PRIMA_MMAP_BYTES];
    for (unsigned i = 0; i < PRIMA_MMAP_BYTES; i++)
        was_present[i] = prima_mmap_present[i];
    unsigned cursor = index;
    while (cursor < index + pages) {
        if (prima_bit_get(prima_mmap_present, cursor)) {
            cursor++;
            continue;
        }
        unsigned start = cursor;
        while (cursor < index + pages &&
               !prima_bit_get(prima_mmap_present, cursor))
            cursor++;
        long mapped = prima_arena_map_pages(start, cursor - start, PRIMA_ACCESS_R | PRIMA_ACCESS_W);
        if (mapped != 0) {
            /* 失败原子：把本次新映射的页拆回映射前状态。 */
            for (unsigned page = index; page < index + pages; page++) {
                if (!prima_bit_get(was_present, page) &&
                    prima_bit_get(prima_mmap_present, page)) {
                    (void)prima_arena_unmap_pages(page, 1);
                    prima_bit_set(prima_mmap_present, page, 0);
                }
            }
            return mapped;
        }
        for (unsigned page = start; page < cursor; page++)
            prima_bit_set(prima_mmap_present, page, 1);
    }
    for (unsigned page = index; page < index + pages; page++)
        prima_bit_set(prima_mmap_used, page, 1);
    prima_zero(base, pages64 * PRIMA_PAGE_SIZE);
    long protected = prot == 0 ?
        prima_raw(PRIMA_SYS_VSPACE_ACCESS_ENABLE, prima_ctx.self_cap, base, pages, 0, 0, 0) :
        prima_raw(PRIMA_SYS_MEMSEG_PROTECT, prima_ctx.self_cap, base, pages, access, 0, 0);
    if (protected != 0) {
        (void)prima_arena_release(index, pages);
        return protected;
    }
    return (long)base;
}

static long prima_munmap_anon(uint64_t address, uint64_t length) {
    if ((address & (PRIMA_PAGE_SIZE - 1)) != 0 || length == 0 ||
        length > UINT64_MAX - (PRIMA_PAGE_SIZE - 1))
        return -PRIMA_ERRNO_EINVAL;
    uint64_t pages64 = (length + PRIMA_PAGE_SIZE - 1) / PRIMA_PAGE_SIZE;
    if (pages64 > PRIMA_MMAP_PAGES)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t span = pages64 * PRIMA_PAGE_SIZE;
    if (address < PRIMA_MMAP_BASE || address + span > PRIMA_MMAP_LIMIT ||
        address + span < address)
        return -PRIMA_ERRNO_EINVAL;
    unsigned index = (unsigned)((address - PRIMA_MMAP_BASE) / PRIMA_PAGE_SIZE);
    unsigned pages = (unsigned)pages64;
    prima_heap_ready();
    if (prima_arena_state == 0)
        (void)prima_arena_acquire();
    /* Linux 允许 munmap 未映射页；这里同样只释放“占用”位。 */
    for (unsigned page = index; page < index + pages; page++)
        prima_bit_set(prima_mmap_used, page, 0);
    return prima_arena_release(index, pages);
}

static long prima_mprotect_anon(uint64_t address, uint64_t length, long prot) {
    if ((address & (PRIMA_PAGE_SIZE - 1)) != 0 || length == 0 ||
        length > UINT64_MAX - (PRIMA_PAGE_SIZE - 1))
        return -PRIMA_ERRNO_EINVAL;
    uint64_t pages64 = (length + PRIMA_PAGE_SIZE - 1) / PRIMA_PAGE_SIZE;
    if (pages64 == 0 || pages64 > PRIMA_MMAP_PAGES)
        return -PRIMA_ERRNO_ENOMEM;
    uint64_t span = pages64 * PRIMA_PAGE_SIZE;
    if (address < PRIMA_MMAP_BASE || address + span > PRIMA_MMAP_LIMIT ||
        address + span < address)
        return -PRIMA_ERRNO_ENOMEM;
    unsigned index = (unsigned)((address - PRIMA_MMAP_BASE) / PRIMA_PAGE_SIZE);
    unsigned pages = (unsigned)pages64;
    prima_heap_ready();
    if (prima_arena_state == 0)
        (void)prima_arena_acquire();
    long access = prima_prot_access(prot);
    if (access < 0)
        return access;
    /* 只有仍被占用的已映射页可以改保护；缩权不要求 Segment cap。 */
    for (unsigned page = index; page < index + pages; page++) {
        if (!prima_bit_get(prima_mmap_used, page) ||
            !prima_bit_get(prima_mmap_present, page))
            return -PRIMA_ERRNO_ENOMEM;
    }
    if (prot == 0)
        return prima_raw(PRIMA_SYS_VSPACE_ACCESS_ENABLE, prima_ctx.self_cap, address, pages, 0, 0, 0);
    long result = prima_raw(PRIMA_SYS_MEMSEG_PROTECT, prima_ctx.self_cap, address, pages, access, 0, 0);
    if (result < 0) return result;
    return prima_raw(PRIMA_SYS_VSPACE_ACCESS_ENABLE, prima_ctx.self_cap, address, pages, 1, 0, 0);
}

/* fd 1/2 的逐个字节转发；返回字节数或原始负 errno。 */
static long prima_write_bytes(int fd, const void *buf, unsigned long len) {
    const unsigned char *bytes = buf;
    if (fd != 1 && fd != 2)
        return -PRIMA_ERRNO_EBADF;
    if (len == 0)
        return 0;
    if (!bytes)
        return -PRIMA_ERRNO_EFAULT;
    for (unsigned long i = 0; i < len; i++) {
        long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
        uint64_t payload = PRIMA_STAGE5_TTY_WRITE_TAG |
                           ((uint64_t)(unsigned char)fd << 16) |
                           ((uint64_t)bytes[i] << 24) |
                           ((uint64_t)id & 0xff) << 32;
        long sent = prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                              (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
        if (sent != 0)
            return sent < 0 ? sent : -PRIMA_ERRNO_EFAULT;
        (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    }
    return (long)len;
}

/* ---------- M8：通用本地 fd 表与用户态 VFS ----------
 *
 * fd 0..18 由适配层拥有：fd 0 预置 null（读 EOF、写丢弃），fd 1/2 预置 tty；
 * 文件/目录项指向服务端句柄（3..10，最多 8 个并发 open）。`dup*` 出的多项共享
 * 同一句柄、offset 与目录游标，最后一个引用关闭时才向服务端发 CLOSE。
 *
 * 路径在存储服务里解析（支持绝对/相对、`.`/`..`、多级目录与 dirfd 起始目录）；
 * 单条 IPC 最多携带 `PRIMA_FILE_CHUNK` 字节，故 read/write 在这里分片，open 的
 * 路径用 OPEN/PATH/COMMIT 三段式发送。`getdents64` 把服务端的目录字节流翻译成
 * Linux `linux_dirent64` 布局。
 *
 * 已知边界（P5-FS-01/P5-FD-01 partial）：无符号链接、无 mkdir/unlink/rename、
 * cwd 为每任务独立的适配层状态（M9 起支持 chdir/getcwd）、公共 `read`/`write`
 * 不把目录当普通文件。
 */
#define PRIMA_FD_FREE 0
#define PRIMA_FD_TTY 1
#define PRIMA_FD_NULL 2
#define PRIMA_FD_FILE 3
#define PRIMA_FD_DIR 4
#define PRIMA_FD_SOCKET 5

#define PRIMA_SESSION_SLOTS 18
#define PRIMA_SYS_RECV_META 60
struct prima_session { uint64_t cookie, store_cookie, store_cap, self_cap; };
static struct prima_session prima_sessions[PRIMA_SESSION_SLOTS];
struct prima_fork_record { long pid; uint32_t key; };
static struct prima_fork_record prima_forks[PRIMA_SESSION_SLOTS];

static struct prima_session *prima_session_current(void) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    return id > 0 && id < PRIMA_SESSION_SLOTS ? &prima_sessions[id] : 0;
}
static long prima_session_accept(void) {
    struct prima_session *session = prima_session_current();
    if (!session) return -PRIMA_ERRNO_EAGAIN;
    long cookie = prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    if (cookie < 0) return cookie;
    uint64_t manager_cookie = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 4, prima_ctx.send_cap, 0, 0, 0, 0);
    uint64_t expected_store = 0;
    for (;;) {
        uint64_t out[5] = {0};
        long raw = prima_receive_meta(out, 0, 0);
        if (raw < 0) return raw;
        if (((uint64_t)raw >> 48) == (PRIMA_SESSION_ROUTE >> 48) && out[3] == manager_cookie) {
            expected_store = ((uint64_t)raw & 0xff) | ((((uint64_t)raw >> 8) & UINT64_C(0xffffffff)) << 32);
            continue;
        }
        if (((uint64_t)raw >> 48) != (PRIMA_SESSION_ACK >> 48)) {
            for (unsigned i=0; i<out[2] && i<2; i++) (void)prima_raw(PRIMA_SYS_CAP_DROP, out[i], 0, 0, 0, 0, 0);
            continue;
        }
        if (((uint64_t)raw & 0xff) != 0 && out[3] == manager_cookie)
            return -(long)((uint64_t)raw & 0xff);
        if (out[2] != 1) continue;
        long target = prima_raw(PRIMA_SYS_TASK_INFO, 4, out[0], 0, 0, 0, 0);
        if (target < 0 || (uint64_t)target != out[3] || !expected_store || out[3] != expected_store) {
            (void)prima_raw(PRIMA_SYS_CAP_DROP, out[0], 0, 0, 0, 0, 0); continue;
        }
        if (session->cookie == (uint64_t)cookie && session->store_cap)
            (void)prima_raw(PRIMA_SYS_CAP_DROP, session->store_cap, 0, 0, 0, 0, 0);
        session->cookie = cookie; session->store_cookie = out[3]; session->store_cap = out[0];
        return 0;
    }
}
static long prima_session_register(uint32_t reservation) {
    struct prima_session *session = prima_session_current();
    if (!session) return -PRIMA_ERRNO_EAGAIN;
    long cookie = prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    if (cookie < 0) return cookie;
    if (!reservation && session->cookie == (uint64_t)cookie && session->store_cap) return 0;
    long self = prima_raw(PRIMA_SYS_TASK_INFO, 5, 0, 0, 0, 0, 0);
    if (self < 0) return self;
    session->self_cap = (uint64_t)self;
    long reply = prima_raw(PRIMA_SYS_CAP_DERIVE, self, (1 << 0) | (1 << 5), 0, 0, 0, 0);
    if (reply < 0) return reply;
    long control = prima_raw(PRIMA_SYS_CAP_DERIVE, self, (1 << 5) | (1 << 7), 0, 0, 0, 0);
    if (control < 0) { (void)prima_raw(PRIMA_SYS_CAP_DROP, reply, 0, 0, 0, 0, 0); return control; }
    long sent = prima_raw(PRIMA_SYS_SEND, prima_ctx.manager_id, prima_ctx.send_cap,
                          (long)(PRIMA_SESSION_REGISTER | reservation), 2, reply, control);
    if (sent < 0) {
        (void)prima_raw(PRIMA_SYS_CAP_DROP, reply, 0, 0, 0, 0, 0);
        (void)prima_raw(PRIMA_SYS_CAP_DROP, control, 0, 0, 0, 0, 0);
        return sent;
    }
    return prima_session_accept();
}

static long prima_session_send(uint64_t payload) {
    long ready = prima_session_register(0);
    if (ready < 0) return ready;
    struct prima_session *session = prima_session_current();
    return prima_raw(PRIMA_SYS_SEND, (uint32_t)session->store_cookie,
                      session->store_cap, payload, 0, 0, 0);
}
static long prima_session_receive(uint64_t tag) {
    struct prima_session *session = prima_session_current();
    if (!session) return -PRIMA_ERRNO_EFAULT;
    for (;;) {
        uint64_t out[5] = {0};
        long raw = prima_receive_meta(out, 0, 0);
        if (raw < 0) return raw;
        if (out[3] == session->store_cookie && ((uint64_t)raw >> 48) == (tag >> 48)) return raw;
    }
}
static long prima_session_control(uint64_t payload) {
    long result = prima_session_send(payload);
    return result < 0 ? result : prima_session_receive(PRIMA_SESSION_FORK_ACK);
}
static long prima_fork_handoff_receive(void) {
    struct prima_session *session = prima_session_current();
    uint64_t manager = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 4, prima_ctx.send_cap, 0, 0, 0, 0);
    for (;;) {
        uint64_t out[5] = {0};
        long raw = prima_receive_meta(out, 0, 0);
        if (raw < 0) return raw;
        if ((out[3] == manager || (session && out[3] == session->store_cookie)) &&
            ((uint64_t)raw >> 48) == (PRIMA_SESSION_FORK_ACK >> 48)) return raw;
    }
}
static void prima_session_thread_cleanup(void) {
    long task = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    if (task > 0 && task < 18) prima_restart_scopes[task] = 0;
    struct prima_session *session = prima_session_current();
    if (session && session->store_cap) {
        (void)prima_raw(PRIMA_SYS_CAP_DROP, session->store_cap, 0, 0, 0, 0, 0);
        session->store_cap = 0; session->cookie = 0;
    }
    if (session && session->self_cap) {
        (void)prima_raw(PRIMA_SYS_CAP_DROP, session->self_cap, 0, 0, 0, 0, 0);
        session->self_cap = 0;
    }
}

static long prima_file_call(long op, long fd, uint64_t arg);
static long prima_vfs_call(uint64_t payload);
static long prima_absolute_path(long dirfd, const char *path, char *out,
                                unsigned long *out_len);
static long prima_reply_status(uint64_t reply);
static long prima_chmod_path(long dirfd, const char *path, long mode);
static long prima_net_request(unsigned char op, unsigned char handle, uint64_t arg,
                              uint64_t *reply);
static long prima_tty_read(void *buf, unsigned long len);
static long prima_tty_ioctl(int fd, unsigned long request, unsigned long arg);
static long prima_vfs_chmod_request(unsigned char start, const char *path,
                                    unsigned long len, unsigned short mode);
static void prima_dir_reset(unsigned char handle);

/* M13b：单用户权限模型的进程级 umask（默认 022）。 */
static long prima_umask_value = 022;

struct prima_fd_entry {
    unsigned char kind;
    unsigned char object; /* tty fd（1/2）、服务端句柄（3..10）或 0 */
    unsigned char node;   /* 服务端节点编号（目录相对 open 用） */
    unsigned char cloexec; /* Per-fd flag; exec-time close is not wired yet. */
};

static struct prima_fd_entry prima_fds[PRIMA_LOCAL_FD_COUNT] = {
    {PRIMA_FD_TTY, 0, 0, 0},
    {PRIMA_FD_TTY, 1, 0, 0},
    {PRIMA_FD_TTY, 2, 0, 0},
};
static unsigned char prima_file_refs[PRIMA_FILE_HANDLES];
/* Open status is shared by fd aliases, unlike the descriptor CLOEXEC bit. */
static unsigned int prima_file_flags[PRIMA_FILE_HANDLES];
// M19：socket 服务端句柄的本地引用计数（dup 共享）。
static unsigned char prima_net_refs[PRIMA_NET_SOCKETS];

/* M13a：目录 fd 的规范化绝对路径（与 cwd 一样按文本保存），供 fchdir 使用。
 * dup/dup2/dup3 复制路径；关闭 fd 时清除。 */
static char prima_fd_paths[PRIMA_LOCAL_FD_COUNT][PRIMA_VFS_PATH_MAX + 1];

static void prima_fd_path_clear(int fd) {
    if (fd < PRIMA_LOCAL_FD_FIRST || fd > PRIMA_LOCAL_FD_LAST)
        return;
    prima_fd_paths[fd - PRIMA_LOCAL_FD_FIRST][0] = 0;
}

static void prima_fd_path_set(int fd, const char *path) {
    if (fd < PRIMA_LOCAL_FD_FIRST || fd > PRIMA_LOCAL_FD_LAST)
        return;
    char *out = prima_fd_paths[fd - PRIMA_LOCAL_FD_FIRST];
    unsigned long len = 0;
    while (path[len] != 0 && len < PRIMA_VFS_PATH_MAX) {
        out[len] = path[len];
        len++;
    }
    out[len] = 0;
}

static void prima_fd_path_copy(int dst, int src) {
    if (dst < PRIMA_LOCAL_FD_FIRST || dst > PRIMA_LOCAL_FD_LAST ||
        src < PRIMA_LOCAL_FD_FIRST || src > PRIMA_LOCAL_FD_LAST)
        return;
    prima_fd_path_set(dst, prima_fd_paths[src - PRIMA_LOCAL_FD_FIRST]);
}

struct prima_dir_cache {
    unsigned char len;
    unsigned char pos;
    unsigned char buf[PRIMA_DIR_CACHE_BYTES];
};
static struct prima_dir_cache prima_dirs[PRIMA_FILE_HANDLES];

struct prima_dir_record {
    unsigned short node;
    unsigned char kind;
    unsigned char name_len;
    unsigned char name[PRIMA_DIR_CACHE_BYTES - 4];
};

struct prima_dirent64 {
    uint64_t d_ino;
    int64_t d_off;
    unsigned short d_reclen;
    unsigned char d_type;
    char d_name[];
};

static struct prima_fd_entry *prima_fd_entry(int fd) {
    if (fd < PRIMA_LOCAL_FD_FIRST || fd > PRIMA_LOCAL_FD_LAST)
        return 0;
    struct prima_fd_entry *entry = &prima_fds[fd - PRIMA_LOCAL_FD_FIRST];
    return entry->kind == PRIMA_FD_FREE ? 0 : entry;
}

static int prima_fd_has_room(void) {
    for (int i = 0; i < PRIMA_LOCAL_FD_COUNT; i++)
        if (prima_fds[i].kind == PRIMA_FD_FREE)
            return 1;
    return 0;
}

/* 返回最低空闲本地 fd；表满返回 -1。 */
static int prima_fd_install(unsigned char kind, unsigned char object,
                            unsigned char node, unsigned char cloexec) {
    for (int i = 0; i < PRIMA_LOCAL_FD_COUNT; i++) {
        if (prima_fds[i].kind == PRIMA_FD_FREE) {
            prima_fds[i].kind = kind;
            prima_fds[i].object = object;
            prima_fds[i].node = node;
            prima_fds[i].cloexec = cloexec;
            return i + PRIMA_LOCAL_FD_FIRST;
        }
    }
    return -1;
}

static void prima_dir_reset(unsigned char handle) {
    if (handle < PRIMA_FILE_FIRST_FD || handle > PRIMA_FILE_LAST_FD)
        return;
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    cache->len = 0;
    cache->pos = 0;
}

/* 释放一项；文件/目录句柄的最后一个引用负责通知服务端。返回服务端状态。 */
static long prima_fd_drop(int fd) {
    struct prima_fd_entry *entry = &prima_fds[fd - PRIMA_LOCAL_FD_FIRST];
    long status = 0;
    if (entry->kind == PRIMA_FD_SOCKET) {
        unsigned char handle = entry->object;
        if (handle < PRIMA_NET_SOCKETS && --prima_net_refs[handle] == 0) {
            uint64_t reply = 0;
            (void)prima_net_request(PRIMA_NET_OP_CLOSE, handle, 0, &reply);
        }
    } else if (entry->kind == PRIMA_FD_FILE || entry->kind == PRIMA_FD_DIR) {
        unsigned char handle = entry->object;
        if (--prima_file_refs[handle - PRIMA_FILE_FIRST_FD] == 0) {
            long reply = prima_file_call(PRIMA_FILE_OP_CLOSE, handle, 0);
            if (reply < 0)
                status = reply;
            else if (prima_reply_status((uint64_t)reply) != 0)
                status = -prima_reply_status((uint64_t)reply);
            prima_dir_reset(handle);
        }
    }
    entry->kind = PRIMA_FD_FREE;
    entry->object = 0;
    entry->node = 0;
    entry->cloexec = 0;
    prima_fd_path_clear(fd);
    return status;
}

/* 同步请求：发送一帧文件请求、阻塞接收文件应答；返回应答 payload 或原始负 errno。 */
static long prima_file_call(long op, long fd, uint64_t arg) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload = PRIMA_STAGE5_FILE_REQUEST_TAG |
                       ((uint64_t)(op & 0xf) << PRIMA_FILE_OP_SHIFT) |
                       ((uint64_t)(fd & 0xf) << PRIMA_FILE_FD_SHIFT) |
                       ((uint64_t)(id & 0xff) << PRIMA_FILE_CLIENT_SHIFT) |
                       (arg & PRIMA_FILE_ARG_MASK);
    long sent = prima_session_send(payload);
    return sent < 0 ? sent : prima_session_receive(PRIMA_STAGE5_FILE_REPLY_TAG);
}
static long prima_vfs_send(uint64_t payload) { return prima_session_send(payload); }
static long prima_vfs_call(uint64_t payload) {
    long sent = prima_session_send(payload);
    return sent < 0 ? sent : prima_session_receive(PRIMA_STAGE5_FILE_REPLY_TAG);
}

static long prima_reply_status(uint64_t reply) {
    return (long)((reply >> PRIMA_FILE_STATUS_SHIFT) & 0xff);
}

/* ---------- M9：工作目录、路径合成与 statx ----------
 *
 * cwd 由适配层保存为**规范化绝对路径**（≤ PRIMA_VFS_PATH_MAX 字节）。阶段 5 的 rootfs
 * 无符号链接，故 `..` 只按字面分量回退；`chdir` 先用 STAT 确认目标为目录，成功后才
 * 更新 cwd。`openat(AT_FDCWD, 相对路径)` 与 `statx(AT_FDCWD, ...)` 先把相对路径与 cwd
 * 合成为绝对路径再送服务端；`dirfd` 形式仍以该目录节点为起点，语义不变。
 *
 * 已知边界（P5-FS-01/P5-PROC-03 partial）：路径 ≤64 字节、名称 ≤16 字节、分量 ≤6；
 * 无符号链接、无 `fchdir`、无进程级 cwd 继承（每任务各自初始化为 `/`）。
 */
static char prima_cwd[PRIMA_VFS_PATH_MAX + 1] = "/";

struct prima_stat_meta {
    unsigned char node;
    unsigned char kind;
    unsigned short mode;
    unsigned short size;
};

static unsigned long prima_strlen_bounded(const char *text, unsigned long max) {
    unsigned long len = 0;
    while (text[len] != 0) {
        if (len >= max)
            return max + 1;
        len++;
    }
    return len;
}

/* 把 path 加上当前 cwd 前缀并消除 `.`/`..`/重复 `/`；失败返回负 errno。 */
static long prima_normalize_path(const char *path, char *out,
                                 unsigned long *out_len) {
    unsigned long len = 0;
    if (path[0] != '/') {
        unsigned long base = prima_strlen_bounded(prima_cwd, PRIMA_VFS_PATH_MAX);
        if (base > PRIMA_VFS_PATH_MAX)
            return -PRIMA_ERRNO_EINVAL;
        for (unsigned long i = 0; i < base; i++)
            out[len++] = prima_cwd[i];
    }
    const char *cursor = path;
    while (*cursor != 0) {
        while (*cursor == '/')
            cursor++;
        if (*cursor == 0)
            break;
        const char *part = cursor;
        unsigned long part_len = 0;
        while (*cursor != 0 && *cursor != '/') {
            cursor++;
            part_len++;
        }
        if (part_len == 1 && part[0] == '.')
            continue;
        if (part_len == 2 && part[0] == '.' && part[1] == '.') {
            while (len > 0 && out[len - 1] != '/')
                len--;
            if (len > 0)
                len--;
            continue;
        }
        if (len + 1 + part_len > PRIMA_VFS_PATH_MAX)
            return -PRIMA_ERRNO_EINVAL;
        if (len == 0 || out[len - 1] != '/')
            out[len++] = '/';
        for (unsigned long i = 0; i < part_len; i++)
            out[len++] = part[i];
    }
    if (len == 0)
        out[len++] = '/';
    out[len] = 0;
    *out_len = len;
    return 0;
}

/* M13a：把 dirfd + path 合成为规范化绝对路径（供 fchdir/rename/readlink 使用）。
 * 目录 fd 的基准路径来自打开时记录的文本；未知路径返回 EINVAL。 */
static long prima_absolute_path(long dirfd, const char *path, char *out,
                                unsigned long *out_len) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (dirfd == PRIMA_AT_FDCWD)
        return prima_normalize_path(path, out, out_len);
    struct prima_fd_entry *dir = prima_fd_entry((int)dirfd);
    if (!dir)
        return -PRIMA_ERRNO_EBADF;
    if (dir->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    const char *base = prima_fd_paths[dirfd - PRIMA_LOCAL_FD_FIRST];
    if (base[0] == 0)
        return -PRIMA_ERRNO_EINVAL;
    unsigned long base_len = prima_strlen_bounded(base, PRIMA_VFS_PATH_MAX);
    unsigned long path_len = prima_strlen_bounded(path, PRIMA_VFS_PATH_MAX);
    if (base_len > PRIMA_VFS_PATH_MAX || path_len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    char joined[PRIMA_VFS_PATH_MAX + 2];
    unsigned long len = 0;
    for (unsigned long i = 0; i < base_len; i++)
        joined[len++] = base[i];
    if (len == 0 || joined[len - 1] != '/')
        joined[len++] = '/';
    if (len + path_len > PRIMA_VFS_PATH_MAX + 1)
        return -PRIMA_ERRNO_EINVAL;
    for (unsigned long i = 0; i < path_len; i++)
        joined[len++] = path[i];
    joined[len] = 0;
    return prima_normalize_path(joined, out, out_len);
}

/* 解析 dirfd + path → (起始节点, 实际发送的路径)。cwd 合成缓冲由调用方提供。 */
static long prima_path_target(long dirfd, const char *path, unsigned char *start,
                              char *buffer, const char **send_path) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (dirfd != PRIMA_AT_FDCWD) {
        struct prima_fd_entry *dir = prima_fd_entry((int)dirfd);
        if (!dir)
            return -PRIMA_ERRNO_EBADF;
        if (dir->kind != PRIMA_FD_DIR)
            return -PRIMA_ERRNO_ENOTDIR;
        *start = dir->node;
        *send_path = path;
        return 0;
    }
    *start = 0;
    if (path[0] == '/') {
        *send_path = path;
        return 0;
    }
    unsigned long len = 0;
    long normalized = prima_normalize_path(path, buffer, &len);
    if (normalized < 0)
        return normalized;
    *send_path = buffer;
    return 0;
}

/* 发送一次 OPEN/PATH/COMMIT；成功把应答 payload 写入 reply。 */
static long prima_vfs_path_request(uint64_t internal, unsigned char start,
                                   const char *path, unsigned long len,
                                   uint64_t *reply) {
    uint64_t meta = PRIMA_STAGE5_VFS_OPEN_TAG |
                    ((internal & 0xff) << PRIMA_VFS_FLAGS_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT) |
                    (((uint64_t)start & 0xff) << PRIMA_VFS_START_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    long raw = prima_vfs_call(PRIMA_STAGE5_VFS_COMMIT_TAG |
                              (((uint64_t)id & 0xff)
                               << PRIMA_VFS_COMMIT_CLIENT_SHIFT));
    if (raw < 0)
        return raw;
    *reply = (uint64_t)raw;
    return 0;
}

/* M13a：双路径操作的单个槽：OPEN（前 3 字节）+ PATH（每段 5 字节），不等待应答。 */
static long prima_pair_stage(unsigned char op, unsigned char slot,
                             const char *path, unsigned long len) {
    if (len == 0 || len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t meta = PRIMA_STAGE5_VFS_PAIR_TAG |
                    ((uint64_t)(slot & 1) << PRIMA_VFS_PAIR_SLOT_SHIFT) |
                    ((uint64_t)(op & 7) << PRIMA_VFS_PAIR_OP_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    return 0;
}

/* 同步 COMMIT 双路径操作；成功把 PAIR_REPLY 写入 reply。 */
static long prima_pair_commit(unsigned char op, unsigned char slot,
                              uint64_t *reply) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload = PRIMA_STAGE5_VFS_PAIR_COMMIT_TAG |
                       ((uint64_t)(slot & 1) << PRIMA_VFS_PAIR_SLOT_SHIFT) |
                       ((uint64_t)(op & 7) << PRIMA_VFS_PAIR_OP_SHIFT) |
                       (((uint64_t)id & 0xff) << PRIMA_VFS_COMMIT_CLIENT_SHIFT);
    long sent = prima_session_send(payload);
    if (sent < 0) return sent;
    long raw = prima_session_receive(PRIMA_STAGE5_VFS_PAIR_REPLY_TAG);
    if (raw < 0) return raw;
    *reply = (uint64_t)raw;
    return 0;
}

static long prima_pair_status(uint64_t reply) {
    return (long)((reply >> PRIMA_VFS_PAIR_STATUS_SHIFT) & 0xff);
}

/* 只解析路径取元数据；不分配服务端句柄，也不做读写权限检查。 */
static long prima_stat_fetch(unsigned char start, const char *path,
                             struct prima_stat_meta *out) {
    unsigned long len = prima_strlen_bounded(path, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t reply = 0;
    long request = prima_vfs_path_request(PRIMA_VFS_FLAG_STAT, start, path, len,
                                          &reply);
    if (request < 0)
        return request;
    long status = prima_reply_status(reply);
    if (status != 0)
        return -status;
    out->node = (unsigned char)(reply & PRIMA_VFS_REPLY_NODE_MASK);
    out->kind = (unsigned char)((reply >> PRIMA_VFS_REPLY_KIND_SHIFT) &
                                PRIMA_VFS_REPLY_KIND_MASK);
    out->mode =
        (unsigned short)((reply >> PRIMA_VFS_REPLY_MODE_SHIFT) & PRIMA_VFS_REPLY_MODE_MASK);
    out->size =
        (unsigned short)((reply >> PRIMA_VFS_REPLY_SIZE_SHIFT) & PRIMA_VFS_REPLY_SIZE_MASK);
    return 0;
}

/* M8/M9：打开/创建路径。cwd 默认为根；`dirfd` 形式从该目录节点起。 */
static long prima_openat(long dirfd, const char *path, long flags, long mode) {
    (void)mode;
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    /* 空路径不能经 cwd 合成为 "."：Linux `open("")` 是 ENOENT。 */
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    const char *send = path;
    unsigned char start = 0;
    long target = prima_path_target(dirfd, path, &start, resolved, &send);
    if (target < 0)
        return target;
    unsigned long len = prima_strlen_bounded(send, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    if (len == 0)
        return -PRIMA_ERRNO_ENOENT;
    long access = flags & PRIMA_O_ACCMODE;
    if (access == 3)
        return -PRIMA_ERRNO_EINVAL;
    long allowed = PRIMA_O_ACCMODE | PRIMA_O_CREAT | PRIMA_O_EXCL |
                   PRIMA_O_TRUNC | PRIMA_O_APPEND | PRIMA_O_DIRECTORY |
                   PRIMA_O_LARGEFILE | PRIMA_O_CLOEXEC;
    if ((flags & ~allowed) != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (!prima_fd_has_room())
        return -PRIMA_ERRNO_EAGAIN;

    uint64_t internal = (uint64_t)access << PRIMA_VFS_ACCESS_SHIFT;
    if (flags & PRIMA_O_CREAT)
        internal |= PRIMA_VFS_FLAG_CREATE;
    if (flags & PRIMA_O_EXCL)
        internal |= PRIMA_VFS_FLAG_EXCLUSIVE;
    if (flags & PRIMA_O_TRUNC)
        internal |= PRIMA_VFS_FLAG_TRUNC;
    if (flags & PRIMA_O_APPEND)
        internal |= PRIMA_VFS_FLAG_APPEND;
    if (flags & PRIMA_O_DIRECTORY)
        internal |= PRIMA_VFS_FLAG_DIRECTORY;

    uint64_t reply = 0;
    long request = prima_vfs_path_request(internal, start, send, len, &reply);
    if (request < 0)
        return request;
    long status = prima_reply_status(reply);
    if (status != 0)
        return -status;
    long handle = (long)((reply >> PRIMA_FILE_FD_SHIFT) & 0xf);
    if (handle < PRIMA_FILE_FIRST_FD || handle > PRIMA_FILE_LAST_FD)
        return -PRIMA_ERRNO_EINVAL;
    unsigned char node = (unsigned char)(reply & PRIMA_VFS_REPLY_NODE_MASK);
    unsigned char kind =
        (unsigned char)((reply >> PRIMA_VFS_REPLY_KIND_SHIFT) &
                        PRIMA_VFS_REPLY_KIND_MASK);
    if (kind != PRIMA_VFS_KIND_DIR && kind != PRIMA_VFS_KIND_FILE &&
        kind != PRIMA_VFS_KIND_NULL) {
        (void)prima_file_call(PRIMA_FILE_OP_CLOSE, handle, 0);
        return -PRIMA_ERRNO_EINVAL;
    }
    prima_file_refs[handle - PRIMA_FILE_FIRST_FD] = 1;
    prima_file_flags[handle - PRIMA_FILE_FIRST_FD] = (unsigned int)flags &
        (PRIMA_O_ACCMODE | PRIMA_O_APPEND | PRIMA_O_DIRECTORY | PRIMA_O_LARGEFILE);
    prima_dir_reset((unsigned char)handle);
    int local = prima_fd_install(
        kind == PRIMA_VFS_KIND_DIR ? PRIMA_FD_DIR : PRIMA_FD_FILE,
        (unsigned char)handle, node, (unsigned char)((flags & PRIMA_O_CLOEXEC) != 0));
    if (local >= 0 && kind == PRIMA_VFS_KIND_DIR) {
        char absolute[PRIMA_VFS_PATH_MAX + 1];
        unsigned long absolute_len = 0;
        if (prima_absolute_path(dirfd, path, absolute, &absolute_len) == 0)
            prima_fd_path_set(local, absolute);
    }
    if (local >= 0 && (flags & PRIMA_O_CREAT) != 0) {
        // M13b：VFS 新建文件默认 0666；适配层按 umask 收紧（无 uid/gid 模型）。
        char absolute[PRIMA_VFS_PATH_MAX + 1];
        unsigned long absolute_len = 0;
        if (prima_absolute_path(dirfd, path, absolute, &absolute_len) == 0)
            (void)prima_vfs_chmod_request(0, absolute, absolute_len,
                                          (unsigned short)(mode & ~prima_umask_value));
    }
    if (local < 0) {
        (void)prima_file_call(PRIMA_FILE_OP_CLOSE, handle, 0);
        prima_file_refs[handle - PRIMA_FILE_FIRST_FD] = 0;
        return -PRIMA_ERRNO_EAGAIN;
    }
    return local;
}

/* M11：发送一次单路径变更（MUTATE + PATH + COMMIT），成功返回 0 或原始负 errno。 */
static long prima_vfs_mutate_request(unsigned char op, unsigned char start,
                                     const char *path, unsigned long len,
                                     uint64_t *reply) {
    uint64_t meta = PRIMA_STAGE5_VFS_MUTATE_TAG |
                    ((uint64_t)(op & 0xf) << PRIMA_VFS_MUTATE_OP_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT) |
                    (((uint64_t)start & 0xff) << PRIMA_VFS_START_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    long raw = prima_vfs_call(PRIMA_STAGE5_VFS_COMMIT_TAG |
                              (((uint64_t)id & 0xff)
                               << PRIMA_VFS_COMMIT_CLIENT_SHIFT));
    if (raw < 0)
        return raw;
    *reply = (uint64_t)raw;
    return 0;
}

/* M11：解析 dirfd + path，发变更请求并翻译服务端状态。 */
static long prima_mutate_path(long dirfd, const char *path, unsigned char op) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    const char *send = path;
    unsigned char start = 0;
    long target = prima_path_target(dirfd, path, &start, resolved, &send);
    if (target < 0)
        return target;
    unsigned long len = prima_strlen_bounded(send, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    if (len == 0)
        return -PRIMA_ERRNO_ENOENT;
    uint64_t reply = 0;
    long request = prima_vfs_mutate_request(op, start, send, len, &reply);
    if (request < 0)
        return request;
    long status = prima_reply_status(reply);
    return status == 0 ? 0 : -status;
}

static long prima_mkdirat(long dirfd, const char *path, long mode) {
    long status = prima_mutate_path(dirfd, path, PRIMA_VFS_OP_MKDIR);
    if (status != 0)
        return status;
    // M13b：VFS 新建目录默认 0777；适配层按 umask 收紧。
    return prima_chmod_path(dirfd, path, mode & ~prima_umask_value);
}

static long prima_unlinkat(long dirfd, const char *path, long flags) {
    if ((flags & ~(long)PRIMA_AT_REMOVEDIR) != 0)
        return -PRIMA_ERRNO_EINVAL;
    unsigned char op = (flags & PRIMA_AT_REMOVEDIR) != 0 ? PRIMA_VFS_OP_RMDIR
                                                        : PRIMA_VFS_OP_UNLINK;
    return prima_mutate_path(dirfd, path, op);
}

/* Linux x86_64 `struct statx`（256 字节）。x86_64 musl 的 stat 家族实际走下方
 * kstat 号 4/5/6/262，只有直接调用 `statx(332)` 的程序用本布局；适配层只填
 * profile 冻结的字段，其余清零。 */
struct prima_statx {
    uint32_t stx_mask;
    uint32_t stx_blksize;
    uint64_t stx_attributes;
    uint32_t stx_nlink;
    uint32_t stx_uid;
    uint32_t stx_gid;
    uint16_t stx_mode;
    uint16_t spare0;
    uint64_t stx_ino;
    uint64_t stx_size;
    uint64_t stx_blocks;
    uint64_t stx_attributes_mask;
    struct {
        int64_t tv_sec;
        uint32_t tv_nsec;
        int32_t pad;
    } stx_atime, stx_btime, stx_ctime, stx_mtime;
    uint32_t stx_rdev_major;
    uint32_t stx_rdev_minor;
    uint32_t stx_dev_major;
    uint32_t stx_dev_minor;
    uint64_t spare2[14];
};
_Static_assert(sizeof(struct prima_statx) == 256, "statx ABI size");

#define PRIMA_S_IFMT 0170000
#define PRIMA_S_IFDIR 0040000
#define PRIMA_S_IFREG 0100000
#define PRIMA_S_IFCHR 0020000
#define PRIMA_S_IFLNK 0120000
#define PRIMA_STATX_BASIC_STATS 0x7ff

static void prima_fill_statx(struct prima_statx *stx,
                             const struct prima_stat_meta *meta) {
    uint16_t ifmt;
    if (meta->kind == PRIMA_VFS_KIND_DIR)
        ifmt = PRIMA_S_IFDIR;
    else if (meta->kind == PRIMA_VFS_KIND_NULL)
        ifmt = PRIMA_S_IFCHR;
    else if (meta->kind == PRIMA_VFS_KIND_SYMLINK)
        ifmt = PRIMA_S_IFLNK;
    else
        ifmt = PRIMA_S_IFREG;
    uint16_t mode = (uint16_t)(ifmt | (meta->mode & 0777));
    *stx = (struct prima_statx){0};
    stx->stx_mask = PRIMA_STATX_BASIC_STATS;
    stx->stx_blksize = 4096;
    stx->stx_nlink = meta->kind == PRIMA_VFS_KIND_DIR ? 2 : 1;
    stx->stx_mode = mode;
    stx->stx_ino = (uint64_t)meta->node + 1;
    stx->stx_size = meta->size;
    stx->stx_blocks = ((uint64_t)meta->size + 511) / 512;
}

/* 取 stat 元数据：AT_EMPTY_PATH 按 fd（本地 tty/null 合成），否则解析路径。 */
static long prima_meta_for(long dirfd, const char *path, long flags,
                           struct prima_stat_meta *meta) {
    if ((flags & PRIMA_AT_EMPTY_PATH) != 0 && path != 0 && path[0] == 0) {
        struct prima_fd_entry *entry = prima_fd_entry((int)dirfd);
        if (!entry)
            return -PRIMA_ERRNO_EBADF;
        if (entry->kind == PRIMA_FD_TTY || entry->kind == PRIMA_FD_NULL) {
            meta->node = entry->node;
            meta->kind = PRIMA_VFS_KIND_NULL;
            meta->mode = 0666;
            meta->size = 0;
            return 0;
        }
        /* 节点自身：STAT 请求以空路径表示，不走 VFS 的相对路径解析（文件节点上的
         * 相对 "." 会被 POSIX/`walk` 判为 ENOTDIR）。 */
        return prima_stat_fetch(entry->node, "", meta);
    }
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    const char *send = path;
    unsigned char start = 0;
    long target = prima_path_target(dirfd, path, &start, resolved, &send);
    if (target < 0)
        return target;
    return prima_stat_fetch(start, send, meta);
}

static long prima_statx(long dirfd, const char *path, long flags, long mask,
                        void *buffer) {
    (void)mask;
    if (!buffer)
        return -PRIMA_ERRNO_EFAULT;
    long allowed = PRIMA_AT_SYMLINK_NOFOLLOW | PRIMA_AT_NO_AUTOMOUNT |
                   PRIMA_AT_EMPTY_PATH;
    if ((flags & ~allowed) != 0)
        return -PRIMA_ERRNO_EINVAL;
    struct prima_stat_meta meta;
    long fetched = prima_meta_for(dirfd, path, flags, &meta);
    if (fetched < 0)
        return fetched;
    prima_fill_statx((struct prima_statx *)buffer, &meta);
    return 0;
}

/* x86_64 musl 的 stat/lstat/fstat/fstatat 走 legacy kstat 系统调用（4/5/6/262），
 * 填充 144 字节的 `struct kstat`；statx(332) 只覆盖直接调用 statx 的程序。
 * 时间戳、dev/rdev 与 uid/gid 在本 profile 中恒 0。 */
struct prima_kstat {
    uint64_t st_dev;
    uint64_t st_ino;
    uint64_t st_nlink;
    uint32_t st_mode;
    uint32_t st_uid;
    uint32_t st_gid;
    uint32_t pad0;
    uint64_t st_rdev;
    int64_t st_size;
    int64_t st_blksize;
    int64_t st_blocks;
    int64_t st_atime_sec;
    int64_t st_atime_nsec;
    int64_t st_mtime_sec;
    int64_t st_mtime_nsec;
    int64_t st_ctime_sec;
    int64_t st_ctime_nsec;
    int64_t unused[3];
};
_Static_assert(sizeof(struct prima_kstat) == 144, "kstat ABI size");

static void prima_fill_kstat(struct prima_kstat *st,
                             const struct prima_stat_meta *meta) {
    *st = (struct prima_kstat){0};
    st->st_ino = (uint64_t)meta->node + 1;
    st->st_nlink = meta->kind == PRIMA_VFS_KIND_DIR ? 2 : 1;
    if (meta->kind == PRIMA_VFS_KIND_DIR)
        st->st_mode = (uint32_t)(PRIMA_S_IFDIR | (meta->mode & 0777));
    else if (meta->kind == PRIMA_VFS_KIND_NULL)
        st->st_mode = (uint32_t)(PRIMA_S_IFCHR | (meta->mode & 0777));
    else
        st->st_mode = (uint32_t)(PRIMA_S_IFREG | (meta->mode & 0777));
    st->st_size = meta->size;
    st->st_blksize = 4096;
    st->st_blocks = ((int64_t)meta->size + 511) / 512;
}

static long prima_stat_kstat(long dirfd, const char *path, long flags,
                             void *buffer) {
    if (!buffer)
        return -PRIMA_ERRNO_EFAULT;
    long allowed = PRIMA_AT_SYMLINK_NOFOLLOW | PRIMA_AT_NO_AUTOMOUNT |
                   PRIMA_AT_EMPTY_PATH;
    if ((flags & ~allowed) != 0)
        return -PRIMA_ERRNO_EINVAL;
    struct prima_stat_meta meta;
    long fetched = prima_meta_for(dirfd, path, flags, &meta);
    if (fetched < 0)
        return fetched;
    prima_fill_kstat((struct prima_kstat *)buffer, &meta);
    return 0;
}

/* M9：chdir 先用 stat 确认目标是目录；成功后保存**规范化绝对路径**。 */
static long prima_chdir(const char *path) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    char normalized[PRIMA_VFS_PATH_MAX + 1];
    unsigned long len = 0;
    long normalized_status = prima_normalize_path(path, normalized, &len);
    if (normalized_status < 0)
        return normalized_status;
    struct prima_stat_meta meta;
    long fetched = prima_stat_fetch(0, normalized, &meta);
    if (fetched < 0)
        return fetched;
    if (meta.kind != PRIMA_VFS_KIND_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    for (unsigned long i = 0; i <= len; i++)
        prima_cwd[i] = normalized[i];
    return 0;
}

/* M9：返回包含结尾 NUL 的长度；缓冲不足用 ERANGE（profile v1 新冻结）。 */
static long prima_getcwd(char *buffer, unsigned long size) {
    if (!buffer)
        return -PRIMA_ERRNO_EFAULT;
    unsigned long len = prima_strlen_bounded(prima_cwd, PRIMA_VFS_PATH_MAX);
    if (len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    if (size < len + 1)
        return -PRIMA_ERRNO_ERANGE;
    for (unsigned long i = 0; i <= len; i++)
        buffer[i] = prima_cwd[i];
    return (long)(len + 1);
}

/* M13a：fchdir 取目录 fd 打开时记录的目标路径；路径不可得时确定失败。 */
static long prima_fchdir(int fd) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    const char *path = prima_fd_paths[fd - PRIMA_LOCAL_FD_FIRST];
    unsigned long len = prima_strlen_bounded(path, PRIMA_VFS_PATH_MAX);
    if (len == 0 || len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    for (unsigned long i = 0; i <= len; i++)
        prima_cwd[i] = path[i];
    return 0;
}

/* M13a：rename/renameat。两条路径都先规范化为绝对路径，再按槽 0/1 分段发送。 */
static long prima_renameat(long olddirfd, const char *oldpath, long newdirfd,
                           const char *newpath) {
    if (!oldpath || !newpath)
        return -PRIMA_ERRNO_EFAULT;
    char from[PRIMA_VFS_PATH_MAX + 1];
    char to[PRIMA_VFS_PATH_MAX + 1];
    unsigned long from_len = 0;
    unsigned long to_len = 0;
    long status = prima_absolute_path(olddirfd, oldpath, from, &from_len);
    if (status < 0)
        return status;
    status = prima_absolute_path(newdirfd, newpath, to, &to_len);
    if (status < 0)
        return status;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_RENAME, 0, from, from_len);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_RENAME, 0, &reply);
    if (status < 0)
        return status;
    long result = prima_pair_status(reply);
    if (result != 0)
        return -result;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_RENAME, 1, to, to_len);
    if (status < 0)
        return status;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_RENAME, 1, &reply);
    if (status < 0)
        return status;
    result = prima_pair_status(reply);
    return result == 0 ? 0 : -result;
}

/* M13a：symlink/symlinkat。链接路径规范化；目标字符串按原样保存，不跟随/不合成。 */
static long prima_symlinkat(const char *target, long dirfd, const char *linkpath) {
    if (!target || !linkpath)
        return -PRIMA_ERRNO_EFAULT;
    unsigned long target_len = prima_strlen_bounded(target, PRIMA_VFS_PATH_MAX);
    if (target_len == 0 || target_len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    char link[PRIMA_VFS_PATH_MAX + 1];
    unsigned long link_len = 0;
    long status = prima_absolute_path(dirfd, linkpath, link, &link_len);
    if (status < 0)
        return status;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_SYMLINK, 0, link, link_len);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_SYMLINK, 0, &reply);
    if (status < 0)
        return status;
    long result = prima_pair_status(reply);
    if (result != 0)
        return -result;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_SYMLINK, 1, target, target_len);
    if (status < 0)
        return status;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_SYMLINK, 1, &reply);
    if (status < 0)
        return status;
    result = prima_pair_status(reply);
    return result == 0 ? 0 : -result;
}

/* M13a：readlink/readlinkat。先取目标长度，再按 3 字节分段取回；缓冲不足按 Linux
 * 语义截断并返回实际拷贝数（不补 NUL）。 */
static long prima_readlinkat(long dirfd, const char *path, char *buffer,
                             unsigned long size) {
    if (!path || !buffer)
        return -PRIMA_ERRNO_EFAULT;
    if (size == 0)
        return -PRIMA_ERRNO_EINVAL;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    unsigned long len = 0;
    long status = prima_absolute_path(dirfd, path, resolved, &len);
    if (status < 0)
        return status;
    status = prima_pair_stage(PRIMA_VFS_PAIR_OP_READLINK, 0, resolved, len);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_pair_commit(PRIMA_VFS_PAIR_OP_READLINK, 0, &reply);
    if (status < 0)
        return status;
    long result = prima_pair_status(reply);
    if (result != 0)
        return -result;
    unsigned long target_len = (unsigned long)(reply & PRIMA_FILE_ARG_MASK);
    if (target_len > PRIMA_VFS_PATH_MAX)
        return -PRIMA_ERRNO_EINVAL;
    unsigned long copied = 0;
    while (copied < target_len) {
        long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
        uint64_t request = PRIMA_STAGE5_VFS_LINK_READ_TAG |
                           (((uint64_t)copied & 0xff)
                            << PRIMA_VFS_LINK_OFFSET_SHIFT) |
                           (((uint64_t)id & 0xff)
                            << PRIMA_VFS_LINK_CLIENT_SHIFT);
        long sent = prima_vfs_send(request);
        if (sent < 0)
            return sent;
        long raw = prima_session_receive(PRIMA_STAGE5_VFS_LINK_REPLY_TAG);
        if (raw < 0) return raw;
        uint64_t chunk = (uint64_t)raw;
        long chunk_status =
            (long)((chunk >> PRIMA_VFS_LINK_OFFSET_SHIFT) & 0xff);
        if (chunk_status != 0)
            return -chunk_status;
        unsigned long take = target_len - copied;
        if (take > PRIMA_VFS_LINK_CHUNK)
            take = PRIMA_VFS_LINK_CHUNK;
        for (unsigned long i = 0; i < take && copied < size; i++)
            buffer[copied++] = (char)(unsigned char)((chunk >> (8 * i)) & 0xff);
        if (copied >= size)
            break;
    }
    return (long)copied;
}

/* M13b：chmod 的单路径请求；COMMIT 的 bits 8..16 携带 mode。 */
static long prima_vfs_chmod_request(unsigned char start, const char *path,
                                    unsigned long len, unsigned short mode) {
    uint64_t meta = PRIMA_STAGE5_VFS_CHMOD_TAG |
                    (((uint64_t)start & 0xff) << PRIMA_VFS_START_SHIFT) |
                    (((uint64_t)len & 0xff) << PRIMA_VFS_PATH_LEN_SHIFT);
    for (unsigned long i = 0; i < len && i < 3; i++)
        meta |= (uint64_t)(unsigned char)path[i] << (8 * i);
    long sent = prima_vfs_send(meta);
    if (sent < 0)
        return sent;
    for (unsigned long offset = 3; offset < len; offset += 5) {
        uint64_t chunk = PRIMA_STAGE5_VFS_PATH_TAG |
                         (((uint64_t)offset & 0xff) << PRIMA_VFS_PATH_OFFSET_SHIFT);
        for (unsigned long i = 0; i < 5 && offset + i < len; i++)
            chunk |= (uint64_t)(unsigned char)path[offset + i] << (8 * i);
        sent = prima_vfs_send(chunk);
        if (sent < 0)
            return sent;
    }
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t commit = PRIMA_STAGE5_VFS_COMMIT_TAG |
                      (((uint64_t)id & 0xff) << PRIMA_VFS_COMMIT_CLIENT_SHIFT) |
                      (((uint64_t)mode & PRIMA_VFS_CHMOD_MODE_MASK)
                       << PRIMA_VFS_CHMOD_MODE_SHIFT);
    long raw = prima_vfs_call(commit);
    if (raw < 0)
        return raw;
    long status = prima_reply_status((uint64_t)raw);
    return status == 0 ? 0 : -status;
}

/* chmod/chmodat：路径规范化后交给服务端；跟随符号链接。 */
static long prima_chmod_path(long dirfd, const char *path, long mode) {
    if (!path)
        return -PRIMA_ERRNO_EFAULT;
    if (path[0] == 0)
        return -PRIMA_ERRNO_ENOENT;
    if (mode < 0 || (mode & ~(long)0777) != 0)
        return -PRIMA_ERRNO_EINVAL;
    char resolved[PRIMA_VFS_PATH_MAX + 1];
    unsigned long len = 0;
    long status = prima_absolute_path(dirfd, path, resolved, &len);
    if (status < 0)
        return status;
    return prima_vfs_chmod_request(0, resolved, len, (unsigned short)mode);
}

static long prima_fchmod(int fd, long mode) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    // tty/null 没有 VFS 节点；node 字段为 0 时不能落到根节点。
    if (entry->kind != PRIMA_FD_FILE && entry->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_EBADF;
    if (mode < 0 || (mode & ~(long)0777) != 0)
        return -PRIMA_ERRNO_EINVAL;
    return prima_vfs_chmod_request(entry->node, "", 0, (unsigned short)mode);
}

static long prima_kernel_umask(long mask) {
    long previous = prima_umask_value;
    prima_umask_value = mask & 0777;
    return previous;
}

/* One blocking UDP read per Task; fast nested callbacks share its mailbox but
 * cannot steal its header. Snapshots remain owned until FINISH or cancellation. */
static struct prima_net_task {
    uint64_t cookie, reply;
    unsigned next, sequence, handle, ready;
} prima_net_tasks[18];
static int prima_net_active(void) {
    long task = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    return task > 0 && task < 18 && prima_net_tasks[task].sequence;
}
/* All short RPCs finish under a callback mask. A receive restores the caller's
 * mask after sending, then cancels under a mask on non-restarting interruption. */
static long prima_net_request(unsigned char op, unsigned char handle, uint64_t arg,
                              uint64_t *reply) {
    uint64_t full = ~PRIMA_UNMASKABLE, mask;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &mask, 8);
    if (result < 0) return result;
    result = prima_session_register(0);
    if (result < 0) goto done;
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    if (id <= 0 || id >= 18) { result = -PRIMA_ERRNO_EAGAIN; goto done; }
    struct prima_net_task *state = &prima_net_tasks[id];
    uint64_t cookie = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    if (state->cookie != cookie) *state = (struct prima_net_task){.cookie = cookie};
    if (op == PRIMA_NET_OP_RECVFROM && state->sequence) {
        result = -PRIMA_ERRNO_EAGAIN; goto done;
    }
    unsigned sequence;
    do { sequence = ++state->next & 255; } while (!sequence || sequence == state->sequence);
    if (op == PRIMA_NET_OP_RECVFROM) {
        state->sequence = sequence; state->handle = handle; state->ready = 0;
    }
    uint64_t payload = PRIMA_STAGE5_NET_REQUEST_TAG |
                       ((uint64_t)(op & 0xf) << PRIMA_NET_OP_SHIFT) |
                       ((uint64_t)(handle & 3) << PRIMA_NET_HANDLE_SHIFT) |
                       ((uint64_t)sequence << PRIMA_NET_SEQUENCE_SHIFT) |
                       (arg & PRIMA_NET_ARG_MASK);
    result = prima_tty_send(payload);
    if (result < 0) {
        if (op == PRIMA_NET_OP_RECVFROM) state->sequence = 0;
        goto done;
    }
    if (op == PRIMA_NET_OP_RECVFROM)
        (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    uint64_t caps[5];
    uint64_t manager = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 4, prima_ctx.send_cap, 0, 0, 0, 0);
    for (;;) {
        long raw = prima_receive_meta_for(caps, 0, 0, op == PRIMA_NET_OP_RECVFROM ? sequence : 0);
        if (raw < 0) {
            result = raw;
            if (op == PRIMA_NET_OP_RECVFROM) {
                (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, 0, 8);
                uint64_t cancelled = 0;
                long cancel = prima_net_request(PRIMA_NET_OP_CANCEL, handle, sequence, &cancelled);
                if (cancel < 0 || prima_net_status(cancelled)) {
                    result = cancel < 0 ? cancel : -prima_net_status(cancelled);
                } else if (!(cancelled & 1) && state->ready) {
                    *reply = state->reply; result = 0;
                }
                if (result < 0) state->sequence = 0;
            }
            break;
        }
        uint64_t value = (uint64_t)raw;
        if (caps[3] != manager || caps[2] || (value >> 48) != (PRIMA_STAGE5_NET_REPLY_TAG >> 48))
            continue;
        if (((value >> PRIMA_NET_OP_SHIFT) & 0xf) != (uint64_t)op ||
            ((value >> PRIMA_NET_SEQUENCE_SHIFT) & 255) != sequence)
            continue;
        if (op != PRIMA_NET_OP_OPEN &&
            ((value >> PRIMA_NET_HANDLE_SHIFT) & 3) != (uint64_t)handle)
            continue;
        *reply = value; result = 0;
        break;
    }
    if (op == PRIMA_NET_OP_RECVFROM && !result && prima_net_status(*reply)) state->sequence = 0;
done:
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return result;
}

static long prima_net_status(uint64_t reply) {
    return (long)((reply >> PRIMA_NET_STATUS_SHIFT) & 0xff);
}

/* sockaddr_in 只接受 AF_INET + 127.0.0.1/0.0.0.0。返回主机序端口。 */
static long prima_sockaddr_port(const void *addr, int len, int allow_zero,
                                unsigned short *port) {
    if (!addr || len < 16)
        return -PRIMA_ERRNO_EINVAL;
    const unsigned char *bytes = addr;
    unsigned short family = (unsigned short)(bytes[0] | (bytes[1] << 8));
    if (family != PRIMA_AF_INET)
        return -PRIMA_ERRNO_AFNOSUPPORT;
    unsigned int host = (unsigned int)bytes[4] | ((unsigned int)bytes[5] << 8) |
                        ((unsigned int)bytes[6] << 16) |
                        ((unsigned int)bytes[7] << 24);
    if (host != 0x0100007Fu && host != 0)
        return -PRIMA_ERRNO_AFNOSUPPORT;
    unsigned short value = (unsigned short)((bytes[2] << 8) | bytes[3]);
    if (value == 0 && !allow_zero)
        return -PRIMA_ERRNO_EINVAL;
    *port = value;
    return 0;
}

static void prima_sockaddr_fill(void *addr, int *len, unsigned short port) {
    if (!addr || !len || *len < 16) {
        if (len)
            *len = 0;
        return;
    }
    unsigned char *bytes = addr;
    for (int i = 0; i < 16; i++)
        bytes[i] = 0;
    bytes[0] = PRIMA_AF_INET;
    bytes[1] = 0;
    bytes[2] = (unsigned char)(port >> 8);
    bytes[3] = (unsigned char)(port & 0xff);
    bytes[4] = 127;
    bytes[7] = 1;
    *len = 16;
}

static long prima_net_socket(long domain, long type, long protocol) {
    if (domain != PRIMA_AF_INET)
        return -PRIMA_ERRNO_AFNOSUPPORT;
    if (type != PRIMA_SOCK_DGRAM || protocol != 0)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t reply = 0;
    long status = prima_net_request(PRIMA_NET_OP_OPEN, 0, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    unsigned char handle = (unsigned char)(reply & 0xff);
    if (handle >= PRIMA_NET_SOCKETS)
        return -PRIMA_ERRNO_EINVAL;
    int local = prima_fd_install(PRIMA_FD_SOCKET, handle, 0, 0);
    if (local < 0) {
        uint64_t drop_reply = 0;
        (void)prima_net_request(PRIMA_NET_OP_CLOSE, handle, 0, &drop_reply);
        return -PRIMA_ERRNO_EAGAIN;
    }
    prima_net_refs[handle] = 1;
    return local;
}

static struct prima_fd_entry *prima_socket_entry(int fd) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return 0;
    if (entry->kind != PRIMA_FD_SOCKET)
        return 0;
    return entry;
}

static long prima_net_bind(int fd, const void *addr, int len) {
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    unsigned short port = 0;
    long status = prima_sockaddr_port(addr, len, 1, &port);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    status = prima_net_request(PRIMA_NET_OP_BIND, entry->object, port, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    return status == 0 ? 0 : -status;
}

static long prima_net_getsockname(int fd, void *addr, int *len) {
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    uint64_t reply = 0;
    long status = prima_net_request(PRIMA_NET_OP_GETSOCKNAME, entry->object, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    prima_sockaddr_fill(addr, len, (unsigned short)(reply & 0xffff));
    return 0;
}

static long prima_net_sendto(int fd, const void *buf, unsigned long len, int flags,
                             const void *addr, int addrlen) {
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    /* UDP sends never raise SIGPIPE; accept the standard per-send suppression. */
    if ((flags & ~PRIMA_MSG_NOSIGNAL) != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (len > PRIMA_NET_DATAGRAM_MAX)
        return -PRIMA_ERRNO_MSGSIZE;
    if (!buf && len != 0)
        return -PRIMA_ERRNO_EFAULT;
    unsigned short port = 0;
    long status = prima_sockaddr_port(addr, addrlen, 0, &port);
    if (status < 0)
        return status;
    uint64_t reply = 0;
    uint64_t arg = (uint64_t)port | ((uint64_t)(len & 0xff) << PRIMA_NET_LEN_SHIFT);
    status = prima_net_request(PRIMA_NET_OP_SENDTO_BEGIN, entry->object, arg, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    const unsigned char *bytes = buf;
    for (unsigned long offset = 0; offset < len; offset += PRIMA_NET_CHUNK) {
        uint64_t chunk = 0;
        for (unsigned long i = 0; i < PRIMA_NET_CHUNK && offset + i < len; i++)
            chunk |= (uint64_t)bytes[offset + i] << (8 * i);
        status = prima_net_request(PRIMA_NET_OP_SENDTO_DATA, entry->object, chunk, &reply);
        if (status < 0)
            return status;
        status = prima_net_status(reply);
        if (status != 0)
            return -status;
    }
    status = prima_net_request(PRIMA_NET_OP_SENDTO_END, entry->object, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    return status == 0 ? (long)len : -status;
}

static long prima_net_recvfrom(int fd, void *buf, unsigned long len, int flags,
                               void *addr, int *addrlen) {
    if (flags != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (!buf && len) return -PRIMA_ERRNO_EFAULT;
    uint64_t full = ~PRIMA_UNMASKABLE, mask;
    long status = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &mask, 8);
    if (status < 0) return status;
    while (__sync_lock_test_and_set(&prima_fd_lock, 1)) (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    struct prima_fd_entry *entry = prima_socket_entry(fd);
    unsigned char handle = entry ? entry->object : PRIMA_NET_SOCKETS;
    __sync_lock_release(&prima_fd_lock);
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    if (handle >= PRIMA_NET_SOCKETS) return -PRIMA_ERRNO_EBADF;
    uint64_t reply = 0;
    status = prima_net_request(PRIMA_NET_OP_RECVFROM, handle, 0, &reply);
    if (status < 0)
        return status;
    status = prima_net_status(reply);
    if (status != 0)
        return -status;
    unsigned short source = (unsigned short)(reply & 0xffff);
    unsigned long total = (reply >> PRIMA_NET_LEN_SHIFT) & 0xff;
    unsigned long copy = total < len ? total : len;
    unsigned char *bytes = buf;
    long task = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    struct prima_net_task *state = &prima_net_tasks[task];
    unsigned sequence = state->sequence;
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &mask, 8);
    for (unsigned long offset = 0; offset < copy; offset += PRIMA_NET_CHUNK) {
        status = prima_net_request(PRIMA_NET_OP_RECV_CHUNK, handle, offset | ((uint64_t)sequence << 16), &reply);
        if (status < 0)
            goto finish;
        status = prima_net_status(reply);
        if (status != 0) { status = -status; goto finish; }
        uint64_t data = reply & PRIMA_NET_ARG_MASK;
        for (unsigned long i = 0; i < PRIMA_NET_CHUNK && offset + i < copy; i++)
            bytes[offset + i] = (unsigned char)((data >> (8 * i)) & 0xff);
    }
    prima_sockaddr_fill(addr, addrlen, source);
    status = (long)copy;
finish:;
    long finished = prima_net_request(PRIMA_NET_OP_FINISH, handle, sequence, &reply);
    if (status >= 0) status = finished < 0 ? finished : prima_net_status(reply) ? -prima_net_status(reply) : status;
    state->sequence = 0;
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return status;
}

#define PRIMA_TTY_DEPTH 8
struct prima_tty_request { uint16_t sequence; unsigned ready, ack; uint64_t reply, cancel_reply; };
static struct {
    uint64_t cookie;
    uint32_t next;
    struct prima_tty_request requests[PRIMA_TTY_DEPTH];
} prima_tty_tasks[18];

static long prima_receive_meta(uint64_t out[5], uint16_t sequence, int cancel_ack) {
    return prima_receive_meta_for(out, sequence, cancel_ack, 0);
}
static long prima_receive_meta_for(uint64_t out[5], uint16_t sequence, int cancel_ack, unsigned net_sequence) {
    long task = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    if (task <= 0 || task >= 18) return -PRIMA_ERRNO_EAGAIN;
    uint64_t manager = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 4, prima_ctx.send_cap, 0, 0, 0, 0);
    for (;;) {
        struct prima_net_task *net = &prima_net_tasks[task];
        if (net_sequence && net->sequence == net_sequence && net->ready) {
            for (unsigned n=0; n<5; n++) out[n] = 0;
            out[3] = manager;
            return (long)net->reply;
        }
        for (unsigned i=0; i<PRIMA_TTY_DEPTH; i++) {
            struct prima_tty_request *r = &prima_tty_tasks[task].requests[i];
            if (sequence && r->sequence == sequence && (cancel_ack ? r->ack : r->ready)) {
                for (unsigned n=0; n<5; n++) out[n] = 0;
                out[3] = manager;
                return (long)(cancel_ack ? r->cancel_reply : r->reply);
            }
        }
        struct prima_restart_scope scope = {prima_restart_scopes[task], 0, 0};
        prima_restart_scopes[task] = &scope;
        long raw = prima_raw(PRIMA_SYS_RECV_META, (long)out, 0, 0, 0, 0, 0);
        prima_restart_scopes[task] = scope.previous;
        if (raw == -4 && scope.seen && !scope.stop) continue;
        if (raw < 0) return raw;
        uint64_t tag = (uint64_t)raw >> 48;
        if (out[3] == manager && !out[2] && tag == (PRIMA_STAGE5_NET_REPLY_TAG >> 48) &&
            (((uint64_t)raw >> PRIMA_NET_OP_SHIFT) & 15) == PRIMA_NET_OP_RECVFROM && net->sequence &&
            (((uint64_t)raw >> PRIMA_NET_HANDLE_SHIFT) & 3) == net->handle &&
            (((uint64_t)raw >> PRIMA_NET_SEQUENCE_SHIFT) & 255) == net->sequence) {
            net->reply = (uint64_t)raw; net->ready = 1;
            continue;
        }
        if (out[3] == manager && !out[2] &&
            (tag == (PRIMA_STAGE5_TTY_READ_REPLY_TAG >> 48) || tag == (PRIMA_STAGE5_TTY_CANCEL_ACK_TAG >> 48))) {
            uint16_t seq = (uint16_t)((uint64_t)raw >> 32);
            for (unsigned i=0; i<PRIMA_TTY_DEPTH; i++) {
                struct prima_tty_request *r = &prima_tty_tasks[task].requests[i];
                if (r->sequence != seq) continue;
                if (tag == (PRIMA_STAGE5_TTY_READ_REPLY_TAG >> 48)) { r->reply = (uint64_t)raw; r->ready = 1; }
                else { r->cancel_reply = (uint64_t)raw; r->ack = 1; }
            }
            continue;
        }
        return raw;
    }
}
static long prima_tty_send(uint64_t payload) {
    for (;;) {
        long result = prima_raw(PRIMA_SYS_SEND, prima_ctx.manager_id, prima_ctx.send_cap, (long)payload, 0, 0, 0);
        if (result != -PRIMA_ERRNO_EAGAIN) return result;
        (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    }
}
static long prima_tty_cancel(struct prima_tty_request *request) {
    uint64_t full = ~PRIMA_UNMASKABLE, mask;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &mask, 8);
    if (result < 0) return result;
    result = prima_tty_send(PRIMA_STAGE5_TTY_CANCEL_TAG | request->sequence);
    while (result >= 0 && !request->ack) {
        uint64_t out[5];
        result = prima_receive_meta(out, request->sequence, 1);
    }
    if (result >= 0) result = (request->cancel_reply & 1) ? -4 : 0;
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return result;
}
static int prima_tty_active(void) {
    long task = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    if (task <= 0 || task >= 18) return 0;
    for (unsigned i=0; i<PRIMA_TTY_DEPTH; i++)
        if (prima_tty_tasks[task].requests[i].sequence) return 1;
    return 0;
}
/* A read owns its sequence through delivery/cancellation and final acknowledgement.
 * Nested callbacks can receive and cache outer replies without consuming their data. */
static long prima_tty_read(void *buf, unsigned long len) {
    if (!buf && len) return -PRIMA_ERRNO_EFAULT;
    if (!len) return 0;
    long result = prima_session_register(0);
    if (result < 0) return result;
    long task = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t cookie = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    if (task <= 0 || task >= 18) return -PRIMA_ERRNO_EAGAIN;
    if (prima_tty_tasks[task].cookie != cookie) {
        for (unsigned i=0; i<PRIMA_TTY_DEPTH; i++) prima_tty_tasks[task].requests[i] = (struct prima_tty_request){0};
        prima_tty_tasks[task].cookie = cookie;
        prima_tty_tasks[task].next = 0;
    }
    struct prima_tty_request *r = 0;
    for (unsigned i=0; i<PRIMA_TTY_DEPTH; i++) if (!prima_tty_tasks[task].requests[i].sequence) { r = &prima_tty_tasks[task].requests[i]; break; }
    if (!r) return -PRIMA_ERRNO_EAGAIN;
    uint16_t seq;
    for (;;) {
        seq = (uint16_t)++prima_tty_tasks[task].next;
        if (!seq) continue;
        unsigned used = 0;
        for (unsigned i=0; i<PRIMA_TTY_DEPTH; i++) used |= prima_tty_tasks[task].requests[i].sequence == seq;
        if (!used) break;
    }
    *r = (struct prima_tty_request){.sequence = seq};
    unsigned long chunk = len < PRIMA_TTY_READ_CHUNK ? len : PRIMA_TTY_READ_CHUNK;
    uint64_t payload = PRIMA_STAGE5_TTY_READ_TAG | ((uint64_t)task << PRIMA_TTY_READ_CLIENT_SHIFT)
        | ((uint64_t)chunk << PRIMA_TTY_READ_MAX_SHIFT) | r->sequence;
    result = prima_tty_send(payload);
    if (result < 0) { r->sequence = 0; return result; }
    while (!r->ready) {
        uint64_t out[5];
        result = prima_receive_meta(out, r->sequence, 0);
        if (result < 0) {
            if (result == -4) result = prima_tty_cancel(r);
            break;
        }
    }
    if (r->ready) {
        unsigned count = (unsigned)(r->reply >> PRIMA_TTY_READ_COUNT_SHIFT) & 255;
        result = count == 255 ? -(long)(r->reply & 0xffffff) : (long)(count < chunk ? count : chunk);
        if (result >= 0) for (long i=0; i<result; i++) ((unsigned char *)buf)[i] = (unsigned char)(r->reply >> (8*i));
    }
    (void)prima_tty_send(PRIMA_STAGE5_TTY_READ_ACK_TAG | r->sequence);
    r->sequence = 0;
    return result;
}

/* M14：termios 只保存 ICANON/ECHO 等标志；行规程在管理器侧。 */
struct prima_termios {
    uint32_t c_iflag;
    uint32_t c_oflag;
    uint32_t c_cflag;
    uint32_t c_lflag;
    unsigned char c_line;
    unsigned char c_cc[32];
    uint32_t c_ispeed;
    uint32_t c_ospeed;
};
_Static_assert(sizeof(struct prima_termios) == 60, "termios ABI size");

static struct prima_termios prima_tty_termios;
static int prima_tty_termios_ready;

static struct prima_termios *prima_termios(void) {
    if (!prima_tty_termios_ready) {
        prima_tty_termios.c_lflag = PRIMA_ICANON | PRIMA_ECHO | 0x1; /* ISIG */
        prima_tty_termios.c_cc[0] = 3;   /* VINTR */
        prima_tty_termios.c_cc[1] = 28;  /* VQUIT */
        prima_tty_termios.c_cc[2] = 127; /* VERASE */
        prima_tty_termios.c_cc[3] = 21;  /* VKILL */
        prima_tty_termios.c_cc[4] = 4;   /* VEOF */
        prima_tty_termios.c_cc[6] = 1;   /* VMIN */
        prima_tty_termios_ready = 1;
    }
    return &prima_tty_termios;
}

static void prima_tty_sync_mode(void) {
    if (prima_session_register(0) < 0) return;
    struct prima_termios *term = prima_termios();
    uint64_t mode = PRIMA_STAGE5_TTY_MODE_TAG;
    if (term->c_lflag & PRIMA_ICANON)
        mode |= 1;
    if (term->c_lflag & PRIMA_ECHO)
        mode |= 2;
    if (term->c_lflag & 1) mode |= 4;
    (void)prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                    (long)prima_ctx.send_cap, (long)mode, 0, 0, 0);
}

static long prima_tty_ioctl(int fd, unsigned long request, unsigned long arg) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind != PRIMA_FD_TTY)
        return -PRIMA_ERRNO_ENOTTY;
    struct prima_termios *term = prima_termios();
    if (request == PRIMA_TCGETS) {
        if (!arg)
            return -PRIMA_ERRNO_EFAULT;
        unsigned char *out = (unsigned char *)arg;
        const unsigned char *in = (const unsigned char *)term;
        for (int i = 0; i < 60; i++)
            out[i] = in[i];
        return 0;
    }
    if (request == PRIMA_TCSETS || request == PRIMA_TCSETSW ||
        request == PRIMA_TCSETSF) {
        if (!arg)
            return -PRIMA_ERRNO_EFAULT;
        const unsigned char *in = (const unsigned char *)arg;
        unsigned char *out = (unsigned char *)term;
        for (int i = 0; i < 60; i++)
            out[i] = in[i];
        prima_tty_sync_mode();
        return 0;
    }
    if (request == PRIMA_TIOCGWINSZ) {
        if (!arg)
            return -PRIMA_ERRNO_EFAULT;
        unsigned short *out = (unsigned short *)arg;
        out[0] = 24; /* rows */
        out[1] = 80; /* cols */
        out[2] = 0;
        out[3] = 0;
        return 0;
    }
    return -PRIMA_ERRNO_ENOTTY;
}

static long prima_handle_read(int handle, void *buf, unsigned long len) {
    if (len == 0)
        return 0;
    if (!buf)
        return -PRIMA_ERRNO_EFAULT;
    unsigned char *out = buf;
    unsigned long done = 0;
    while (done < len) {
        unsigned long chunk = len - done;
        if (chunk > PRIMA_FILE_CHUNK)
            chunk = PRIMA_FILE_CHUNK;
        long reply = prima_file_call(PRIMA_FILE_OP_READ, handle, chunk);
        if (reply < 0)
            return done != 0 ? (long)done : reply;
        uint64_t raw = (uint64_t)reply;
        long status = prima_reply_status(raw);
        if (status != 0)
            return done != 0 ? (long)done : -status;
        unsigned long count = (raw >> PRIMA_FILE_WRITE_COUNT_SHIFT) & 0xff;
        uint64_t data = raw & PRIMA_FILE_ARG_MASK;
        if (count > chunk)
            count = chunk;
        for (unsigned long i = 0; i < count; i++)
            out[done + i] = (unsigned char)(data >> (8 * i));
        done += count;
        if (count < chunk)
            break; /* EOF 或服务端可用字节不足 */
    }
    return (long)done;
}

static long prima_handle_write(int handle, const void *buf, unsigned long len) {
    if (len == 0)
        return 0;
    if (!buf)
        return -PRIMA_ERRNO_EFAULT;
    const unsigned char *in = buf;
    unsigned long done = 0;
    while (done < len) {
        unsigned long chunk = len - done;
        if (chunk > PRIMA_FILE_CHUNK)
            chunk = PRIMA_FILE_CHUNK;
        uint64_t arg = (uint64_t)chunk << PRIMA_FILE_WRITE_COUNT_SHIFT;
        for (unsigned long i = 0; i < chunk; i++)
            arg |= (uint64_t)in[done + i] << (8 * i);
        long reply = prima_file_call(PRIMA_FILE_OP_WRITE, handle, arg);
        if (reply < 0)
            return done != 0 ? (long)done : reply;
        long status = prima_reply_status((uint64_t)reply);
        if (status != 0)
            return done != 0 ? (long)done : -status;
        done += chunk;
    }
    return (long)len;
}

static long prima_handle_lseek(int handle, long offset, int whence) {
    if (whence < PRIMA_SEEK_SET || whence > PRIMA_SEEK_END)
        return -PRIMA_ERRNO_EINVAL;
    if (offset < -0x20000000L || offset > 0x1fffffffL)
        return -PRIMA_ERRNO_EINVAL;
    uint64_t arg = ((uint64_t)(whence & 3) << PRIMA_FILE_SEEK_WHENCE_SHIFT) |
                   ((uint64_t)offset & PRIMA_FILE_SEEK_OFFSET_MASK);
    long reply = prima_file_call(PRIMA_FILE_OP_LSEEK, handle, arg);
    if (reply < 0)
        return reply;
    uint64_t raw = (uint64_t)reply;
    long status = prima_reply_status(raw);
    if (status != 0)
        return -status;
    return (long)(raw & PRIMA_FILE_ARG_MASK);
}

/* ---------- M8：目录字节流 → linux_dirent64 ---------- */

static long prima_dir_read(unsigned char handle, unsigned char *out,
                           unsigned long want) {
    if (want > PRIMA_FILE_CHUNK)
        want = PRIMA_FILE_CHUNK;
    long reply = prima_file_call(PRIMA_FILE_OP_READ, handle, want);
    if (reply < 0)
        return reply;
    long status = prima_reply_status((uint64_t)reply);
    if (status != 0)
        return -status;
    unsigned long count = ((uint64_t)reply >> PRIMA_FILE_WRITE_COUNT_SHIFT) & 0xff;
    uint64_t data = (uint64_t)reply & PRIMA_FILE_ARG_MASK;
    if (count > want)
        count = want;
    for (unsigned long i = 0; i < count; i++)
        out[i] = (unsigned char)(data >> (8 * i));
    return (long)count;
}

/* 返回 >0 成功、0 EOF、<0 原始负 errno。 */
static long prima_dir_fill(unsigned char handle, unsigned need) {
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    if (cache->pos > 0) {
        unsigned i = 0;
        while ((unsigned)cache->pos + i < cache->len) {
            cache->buf[i] = cache->buf[cache->pos + i];
            i++;
        }
        cache->len = (unsigned char)i;
        cache->pos = 0;
    }
    while (cache->len < need) {
        unsigned room = PRIMA_DIR_CACHE_BYTES - cache->len;
        if (room > PRIMA_FILE_CHUNK)
            room = PRIMA_FILE_CHUNK;
        long got = prima_dir_read(handle, cache->buf + cache->len, room);
        if (got < 0)
            return got;
        if (got == 0)
            return cache->len == 0 ? 0
                                   : (cache->len >= need ? 1
                                                         : -PRIMA_ERRNO_EINVAL);
        cache->len = (unsigned char)(cache->len + (unsigned char)got);
    }
    return 1;
}

/* 取下一条完整记录；返回 1 成功、0 EOF、负 errno。 */
static long prima_dir_take(unsigned char handle, struct prima_dir_record *out) {
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    long filled = prima_dir_fill(handle, 4);
    if (filled <= 0)
        return filled;
    unsigned name_len = cache->buf[3];
    if (name_len > PRIMA_DIR_CACHE_BYTES - 4)
        return -PRIMA_ERRNO_EINVAL;
    filled = prima_dir_fill(handle, 4 + name_len);
    if (filled <= 0)
        return filled < 0 ? filled : -PRIMA_ERRNO_EINVAL;
    out->node = (unsigned short)(cache->buf[0] | ((unsigned short)cache->buf[1] << 8));
    out->kind = cache->buf[2];
    out->name_len = (unsigned char)name_len;
    for (unsigned i = 0; i < name_len; i++)
        out->name[i] = cache->buf[4 + i];
    cache->pos = (unsigned char)(4 + name_len);
    return 1;
}

static void prima_dir_unread(unsigned char handle, unsigned char total) {
    struct prima_dir_cache *cache = &prima_dirs[handle - PRIMA_FILE_FIRST_FD];
    cache->pos = (unsigned char)(cache->pos - total);
}

static long prima_getdents64(int fd, void *dirp, unsigned long count) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind != PRIMA_FD_DIR)
        return -PRIMA_ERRNO_ENOTDIR;
    unsigned char *out = dirp;
    if (!out && count != 0)
        return -PRIMA_ERRNO_EFAULT;
    unsigned char handle = entry->object;
    unsigned long written = 0;
    for (;;) {
        struct prima_dir_record record;
        long taken = prima_dir_take(handle, &record);
        if (taken < 0)
            return written != 0 ? (long)written : taken;
        if (taken == 0)
            break;
        unsigned long name_len = record.name_len;
        unsigned long reclen = (19 + name_len + 1 + 7) & ~7UL;
        if (written + reclen > count) {
            prima_dir_unread(handle, (unsigned char)(4 + name_len));
            if (written == 0)
                return -PRIMA_ERRNO_EINVAL;
            break;
        }
        struct prima_dirent64 *dirent = (struct prima_dirent64 *)(out + written);
        dirent->d_ino = (uint64_t)record.node + 1;
        dirent->d_off = (int64_t)(written + reclen);
        dirent->d_reclen = (unsigned short)reclen;
        dirent->d_type = record.kind == PRIMA_VFS_KIND_DIR
                             ? 4
                             : (record.kind == PRIMA_VFS_KIND_FILE
                                    ? 8
                                    : (record.kind == PRIMA_VFS_KIND_SYMLINK ? 10
                                                                             : 2));
        for (unsigned long i = 0; i < name_len; i++)
            dirent->d_name[i] = (char)record.name[i];
        dirent->d_name[name_len] = 0;
        for (unsigned long i = 19 + name_len + 1; i < reclen; i++)
            ((unsigned char *)dirent)[i] = 0;
        written += reclen;
    }
    return (long)written;
}

/* ---------- M8：按 fd 类型分派 ---------- */

static long prima_fd_read(int fd, void *buf, unsigned long len) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind == PRIMA_FD_SOCKET)
        return -PRIMA_ERRNO_NOTSOCK;
    if (entry->kind == PRIMA_FD_NULL) {
        if (!buf && len != 0)
            return -PRIMA_ERRNO_EFAULT;
        return 0;
    }
    if (entry->kind == PRIMA_FD_TTY) {
        if (entry->object != 0)
            return -PRIMA_ERRNO_EBADF;
        return prima_tty_read(buf, len);
    }
    if (entry->kind != PRIMA_FD_FILE)
        return -PRIMA_ERRNO_EBADF;
    return prima_handle_read(entry->object, buf, len);
}

static long prima_fd_write(int fd, const void *buf, unsigned long len) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind == PRIMA_FD_NULL) {
        if (!buf && len != 0)
            return -PRIMA_ERRNO_EFAULT;
        return (long)len;
    }
    if (entry->kind == PRIMA_FD_SOCKET)
        return -PRIMA_ERRNO_NOTSOCK;
    if (entry->kind == PRIMA_FD_TTY)
        return prima_write_bytes(entry->object == 0 ? 1 : entry->object, buf, len);
    if (entry->kind == PRIMA_FD_FILE)
        return prima_handle_write(entry->object, buf, len);
    return -PRIMA_ERRNO_EBADF;
}

static long prima_fd_lseek(int fd, long offset, int whence) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (entry->kind == PRIMA_FD_NULL)
        return 0;
    if (entry->kind == PRIMA_FD_DIR) {
        prima_dir_reset(entry->object);
        return prima_handle_lseek(entry->object, offset, whence);
    }
    if (entry->kind != PRIMA_FD_FILE)
        return -PRIMA_ERRNO_EBADF;
    return prima_handle_lseek(entry->object, offset, whence);
}

/* Native positional ops never mutate the shared OFD cursor. Like POSIX pwrite,
 * WRITE_AT ignores O_APPEND (Linux's pwrite-on-append behavior differs).
 * Do not emulate this with seek/read/restore: fork holders share the cursor. */
static uint64_t prima_file_at_arg(uint64_t offset, unsigned count, const unsigned char *bytes) {
    if (offset > PRIMA_FILE_AT_OFFSET_MASK) offset = PRIMA_FILE_AT_OFFSET_MASK;
    uint64_t arg = offset | ((uint64_t)count << PRIMA_FILE_AT_COUNT_SHIFT);
    if (bytes)
        for (unsigned i = 0; i < count; i++)
            arg |= (uint64_t)bytes[i] << (PRIMA_FILE_AT_DATA_SHIFT + 8 * i);
    return arg;
}

static long prima_fd_at(int fd, void *buf, unsigned long len, long offset, int writing) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry) return -PRIMA_ERRNO_EBADF;
    if (offset < 0 || len > (~0UL >> 1)) return -PRIMA_ERRNO_EINVAL;
    if (entry->kind == PRIMA_FD_DIR)
        return writing ? -PRIMA_ERRNO_EBADF : -PRIMA_ERRNO_EISDIR;
    if (entry->kind != PRIMA_FD_FILE && entry->kind != PRIMA_FD_NULL)
        return -PRIMA_ERRNO_ESPIPE;
    if (!buf && len) return -PRIMA_ERRNO_EFAULT;
    if (entry->kind == PRIMA_FD_NULL) return writing ? (long)len : 0;
    unsigned char *bytes = buf;
    unsigned long done = 0;
    /* Even a zero-length request reaches the OFD access check. */
    do {
        unsigned count = len - done > PRIMA_FILE_AT_CHUNK ? PRIMA_FILE_AT_CHUNK : (unsigned)(len - done);
        uint64_t arg = prima_file_at_arg((uint64_t)offset + done, count,
                                          writing && count ? bytes + done : 0);
        long reply = prima_file_call(writing ? PRIMA_FILE_OP_WRITE_AT : PRIMA_FILE_OP_READ_AT,
                                     entry->object, arg);
        if (reply < 0) return done ? (long)done : reply;
        long status = prima_reply_status((uint64_t)reply);
        if (status) return done ? (long)done : -status;
        unsigned received = writing ? (unsigned)((uint64_t)reply & PRIMA_FILE_ARG_MASK)
                                    : (unsigned)(((uint64_t)reply >> PRIMA_FILE_WRITE_COUNT_SHIFT) & 0xff);
        if (received > count) return done ? (long)done : -PRIMA_ERRNO_EINVAL;
        if (!writing)
            for (unsigned i = 0; i < received; i++)
                bytes[done + i] = (unsigned char)((uint64_t)reply >> (8 * i));
        done += received;
        if (received < count) break;
    } while (done < len);
    return (long)done;
}

static long prima_fd_resize(int fd, long size) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry) return -PRIMA_ERRNO_EBADF;
    if (size < 0 || entry->kind != PRIMA_FD_FILE) return -PRIMA_ERRNO_EINVAL;
    /* Preserve every oversize request as oversize; never narrow to zero. */
    uint64_t arg = (uint64_t)size > UINT32_MAX ? UINT32_MAX : (uint64_t)size;
    long reply = prima_file_call(PRIMA_FILE_OP_RESIZE, entry->object, arg);
    if (reply < 0) return reply;
    return -prima_reply_status((uint64_t)reply);
}

static long prima_fd_close(int fd) {
    if (!prima_fd_entry(fd))
        return -PRIMA_ERRNO_EBADF;
    return prima_fd_drop(fd);
}

static long prima_fd_dup_min(int oldfd, long minimum, unsigned char cloexec) {
    struct prima_fd_entry *entry = prima_fd_entry(oldfd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    if (minimum < PRIMA_LOCAL_FD_FIRST || minimum > PRIMA_LOCAL_FD_LAST)
        return -PRIMA_ERRNO_EINVAL;
    int newfd = (int)minimum;
    while (newfd <= PRIMA_LOCAL_FD_LAST && prima_fd_entry(newfd))
        newfd++;
    if (newfd > PRIMA_LOCAL_FD_LAST)
        return -PRIMA_ERRNO_EMFILE;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST] = *entry;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].cloexec = cloexec;
    if (entry->kind == PRIMA_FD_FILE || entry->kind == PRIMA_FD_DIR)
        prima_file_refs[entry->object - PRIMA_FILE_FIRST_FD]++;
    else if (entry->kind == PRIMA_FD_SOCKET && entry->object < PRIMA_NET_SOCKETS)
        prima_net_refs[entry->object]++;
    prima_fd_path_copy(newfd, oldfd);
    return newfd;
}

static long prima_fd_dup(int oldfd) {
    long result = prima_fd_dup_min(oldfd, PRIMA_LOCAL_FD_FIRST, 0);
    /* Preserve the existing profile's table-full errno for dup. */
    return result == -PRIMA_ERRNO_EMFILE ? -PRIMA_ERRNO_EAGAIN : result;
}

static long prima_fd_fcntl(int fd, long command, long arg) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry)
        return -PRIMA_ERRNO_EBADF;
    switch (command) {
    case PRIMA_F_GETFD:
        return entry->cloexec ? PRIMA_FD_CLOEXEC : 0;
    case PRIMA_F_SETFD:
        entry->cloexec = (arg & PRIMA_FD_CLOEXEC) != 0;
        return 0;
    case PRIMA_F_DUPFD:
    case PRIMA_F_DUPFD_CLOEXEC:
        return prima_fd_dup_min(fd, arg, command == PRIMA_F_DUPFD_CLOEXEC);
    case PRIMA_F_GETFL:
        if (entry->kind == PRIMA_FD_FILE || entry->kind == PRIMA_FD_DIR)
            return prima_file_flags[entry->object - PRIMA_FILE_FIRST_FD];
        if (entry->kind == PRIMA_FD_NULL)
            return PRIMA_O_RDWR;
        if (entry->kind == PRIMA_FD_TTY)
            return entry->object == 0 ? PRIMA_O_RDONLY : PRIMA_O_WRONLY;
        return -PRIMA_ERRNO_EINVAL;
    case PRIMA_F_SETFL: {
        /* The current service protocol cannot change an open handle's append
         * mode. Accept a no-op only; fdopen must propagate a failed change. */
        long current = prima_fd_fcntl(fd, PRIMA_F_GETFL, 0);
        long allowed = PRIMA_O_ACCMODE | PRIMA_O_APPEND | PRIMA_O_DIRECTORY | PRIMA_O_LARGEFILE;
        if (current < 0 || (arg & ~allowed) || ((arg ^ current) & PRIMA_O_APPEND))
            return -PRIMA_ERRNO_EINVAL;
        return 0;
    }
    default:
        return -PRIMA_ERRNO_EINVAL;
    }
}

/* dup2/dup3 共用：oldfd 已校验；newfd 必须落在本地表内。先为新引用加计数，
 * 再关闭目标项——即使两者指向同一服务端句柄，也不会在中途误发 CLOSE。 */
static long prima_fd_dup_to(int oldfd, int newfd) {
    struct prima_fd_entry *source = prima_fd_entry(oldfd);
    if (!source || newfd < PRIMA_LOCAL_FD_FIRST || newfd > PRIMA_LOCAL_FD_LAST)
        return -PRIMA_ERRNO_EBADF;
    unsigned char kind = source->kind;
    unsigned char object = source->object;
    unsigned char node = source->node;
    if (kind == PRIMA_FD_FILE || kind == PRIMA_FD_DIR)
        prima_file_refs[object - PRIMA_FILE_FIRST_FD]++;
    else if (kind == PRIMA_FD_SOCKET && object < PRIMA_NET_SOCKETS)
        prima_net_refs[object]++;
    if (prima_fd_entry(newfd))
        (void)prima_fd_drop(newfd);
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].kind = kind;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].object = object;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].node = node;
    prima_fds[newfd - PRIMA_LOCAL_FD_FIRST].cloexec = 0;
    if (kind == PRIMA_FD_DIR)
        prima_fd_path_copy(newfd, oldfd);
    else
        prima_fd_path_clear(newfd);
    return newfd;
}

static long prima_fd_dup2(int oldfd, int newfd) {
    if (oldfd == newfd)
        return prima_fd_entry(oldfd) ? oldfd : -PRIMA_ERRNO_EBADF;
    if (!prima_fd_entry(oldfd))
        return -PRIMA_ERRNO_EBADF;
    if (newfd < PRIMA_LOCAL_FD_FIRST || newfd > PRIMA_LOCAL_FD_LAST)
        return -PRIMA_ERRNO_EBADF;
    return prima_fd_dup_to(oldfd, newfd);
}

static long prima_fd_dup3(int oldfd, int newfd, long flags) {
    if (oldfd == newfd || (flags & ~(long)PRIMA_O_CLOEXEC) != 0)
        return -PRIMA_ERRNO_EINVAL;
    if (!prima_fd_entry(oldfd))
        return -PRIMA_ERRNO_EBADF;
    if (newfd < PRIMA_LOCAL_FD_FIRST || newfd > PRIMA_LOCAL_FD_LAST)
        return -PRIMA_ERRNO_EBADF;
    long result = prima_fd_dup_to(oldfd, newfd);
    if (result >= 0 && (flags & PRIMA_O_CLOEXEC) != 0)
        prima_fds[result - PRIMA_LOCAL_FD_FIRST].cloexec = 1;
    return result;
}

/* Validate all lengths before any I/O, then retain progress on a later error
 * or short transfer. Multi-IPC operations are not atomic across threads. */
static long prima_fd_iov(int fd, const struct iovec *iov, long count, int writing) {
    if (!prima_fd_entry(fd))
        return -PRIMA_ERRNO_EBADF;
    if (count < 0 || count > 1024)
        return -PRIMA_ERRNO_EINVAL;
    if (count == 0)
        return 0;
    if (!iov)
        return -PRIMA_ERRNO_EFAULT;
    const unsigned long limit = (~0UL) >> 1; /* SSIZE_MAX on x86_64 */
    unsigned long length = 0;
    for (long i = 0; i < count; i++) {
        if (iov[i].iov_len > limit - length)
            return -PRIMA_ERRNO_EINVAL;
        length += iov[i].iov_len;
    }
    unsigned long done = 0;
    for (long i = 0; i < count; i++) {
        if (iov[i].iov_len == 0)
            continue;
        long transferred = writing
            ? prima_fd_write(fd, iov[i].iov_base, iov[i].iov_len)
            : prima_fd_read(fd, iov[i].iov_base, iov[i].iov_len);
        if (transferred < 0)
            return done ? (long)done : transferred;
        done += (unsigned long)transferred;
        if ((unsigned long)transferred < iov[i].iov_len)
            break;
    }
    return (long)done;
}

/* ---------- M6：POSIX 时间（PIT tick，10 ms 粒度） ----------
 *
 * `SYS_TIME_ENABLE` 以 PIT 校准单 CPU 时间戳并启用 IRQ0；`SYS_TIME_TICKS`
 * 返回 10 ms 单调 tick，与抽象 `SYS_TIME_NOW`（Timer 时钟）分离。
 *
 * 已知边界（P5-TIME-01 partial）：
 * - 仅 `CLOCK_MONOTONIC` 可查询；`CLOCK_REALTIME` 无墙上时钟，返回 `EINVAL`。
 * - `nanosleep` 使用原生物理 deadline 阻塞；中断返回 EINTR 并写回相对 rem。
 * - 未知时钟 id/flag 返回 `EINVAL`；非 NULL 坏指针由用户态缺页终止，适配层只保证
 *   NULL → `EFAULT`。
 */
#define PRIMA_CLOCK_REALTIME 0
#define PRIMA_CLOCK_MONOTONIC 1
#define PRIMA_TIMER_ABSTIME 1
#define PRIMA_TICK_NS UINT64_C(10000000)
#define PRIMA_TICKS_PER_SEC UINT64_C(100)

static int prima_time_enabled;

static long prima_time_ready(void) {
    if (prima_time_enabled)
        return 0;
    long result = prima_raw(PRIMA_SYS_TIME_ENABLE, 0, 0, 0, 0, 0, 0);
    if (result != 0)
        return result < 0 ? result : -PRIMA_ERRNO_EINVAL;
    prima_time_enabled = 1;
    return 0;
}

static long prima_ticks(void) {
    return prima_raw(PRIMA_SYS_TIME_TICKS, 0, 0, 0, 0, 0, 0);
}

static long prima_clock_gettime(long clock_id, void *ts) {
    if (clock_id != PRIMA_CLOCK_MONOTONIC)
        return -PRIMA_ERRNO_EINVAL;
    if (!ts)
        return -PRIMA_ERRNO_EFAULT;
    long ready = prima_time_ready();
    if (ready < 0)
        return ready;
    long ticks = prima_ticks();
    if (ticks < 0)
        return ticks;
    unsigned long *out = ts;
    out[0] = (unsigned long)ticks / PRIMA_TICKS_PER_SEC;
    out[1] = ((unsigned long)ticks % PRIMA_TICKS_PER_SEC) * PRIMA_TICK_NS;
    return 0;
}

static long prima_sleep_ns(long clock_id, long flags, const void *req,
                           void *rem) {
    if (!req)
        return -PRIMA_ERRNO_EFAULT;
    const long *times = req;
    long sec = times[0];
    long nsec = times[1];
    if (sec < 0 || nsec < 0 || nsec >= 1000000000L)
        return -PRIMA_ERRNO_EINVAL;
    if ((flags & ~PRIMA_TIMER_ABSTIME) != 0)
        return -PRIMA_ERRNO_EINVAL;
    /* 相对睡眠接受 REALTIME/MONOTONIC（musl 的 nanosleep 传入 REALTIME）；
     * 绝对睡眠只对 MONOTONIC 有意义。 */
    if (clock_id != PRIMA_CLOCK_MONOTONIC &&
        !(clock_id == PRIMA_CLOCK_REALTIME && flags == 0))
        return -PRIMA_ERRNO_EINVAL;
    if (sec > (0x7fffffffffffffffL - nsec) / 1000000000L)
        return -PRIMA_ERRNO_EINVAL;
    unsigned long long ns =
        (unsigned long long)sec * 1000000000ULL + (unsigned long long)nsec;
    unsigned long long ticks = (ns + PRIMA_TICK_NS - 1) / PRIMA_TICK_NS;
    long ready = prima_time_ready();
    if (ready < 0)
        return ready;
    long now = prima_ticks();
    if (now < 0)
        return now;
    unsigned long long deadline =
        (flags & PRIMA_TIMER_ABSTIME) ? ticks : (unsigned long long)now + ticks;
    if (!ticks && !(flags & PRIMA_TIMER_ABSTIME)) return 0;
    long result = prima_raw(PRIMA_SYS_TASK_WAIT_DEADLINE, (long)deadline, 0, 0, 0, 0, 0);
    if (result == 1) return 0;
    if (result == -4 && rem && !(flags & PRIMA_TIMER_ABSTIME)) {
        long current = prima_ticks();
        uint64_t left = current >= 0 && (uint64_t)current < deadline ? deadline - (uint64_t)current : 0;
        ((long *)rem)[0] = (long)(left / PRIMA_TICKS_PER_SEC);
        ((long *)rem)[1] = (long)((left % PRIMA_TICKS_PER_SEC) * PRIMA_TICK_NS);
    }
    return result;
}

/* M12b：退出前释放本地 fd 与服务端句柄。fd 1/2 只清表项；文件/目录的最后一个引用
 * 发同步 CLOSE，保证存储服务的 8 个句柄不随进程退出泄漏。 */
static void prima_close_all_fds(void) {
    for (int fd = PRIMA_LOCAL_FD_FIRST; fd <= PRIMA_LOCAL_FD_LAST; fd++) {
        if (prima_fd_entry(fd))
            (void)prima_fd_drop(fd);
    }
}

/* Explicit opaque handoff for the Prima crt. The kernel sees retained page
 * ranges and registers; fd/cwd/CLOEXEC semantics stay here in the personality. */
#define PRIMA_EXEC_STATE_VA UINT64_C(0x00000d0000000000)
#define PRIMA_EXEC_STATE_PAGES 3
extern const unsigned char __prima_exec_bridge_start[] __attribute__((weak));
extern const unsigned char __prima_exec_bridge_end[] __attribute__((weak));
#define PRIMA_EXEC_STATE_MAGIC UINT64_C(0x5052494d45584531)
struct prima_exec_state {
    uint64_t magic, bytes, cookie, mask, arena, waiter;
    struct prima_entry_ctx context;
    struct prima_fd_entry fds[PRIMA_LOCAL_FD_COUNT];
    unsigned char file_refs[PRIMA_FILE_HANDLES], net_refs[PRIMA_NET_SOCKETS], present[PRIMA_MMAP_BYTES];
    unsigned int file_flags[PRIMA_FILE_HANDLES];
    char paths[PRIMA_LOCAL_FD_COUNT][PRIMA_VFS_PATH_MAX+1], cwd[PRIMA_VFS_PATH_MAX+1];
    struct prima_dir_cache dirs[PRIMA_FILE_HANDLES];
    struct prima_session session;
    struct prima_fork_record forks[PRIMA_SESSION_SLOTS];
    struct prima_termios termios;
    long umask_value;
    int fork_child, termios_ready;
};
_Static_assert(sizeof(struct prima_exec_state) <= 4096, "one-page exec state");
_Static_assert(offsetof(struct prima_exec_state, waiter) == 40, "exec bridge waiter offset");
_Static_assert(offsetof(struct prima_exec_state, context) == 48, "exec bridge register offset");
_Static_assert(offsetof(struct prima_exec_state, fds) == 96, "exec probe fd offset");
uint64_t prima_exec_state_pointer;
static void prima_copy(void *to, const void *from, unsigned long bytes) {
    unsigned char *out = to; const unsigned char *in = from;
    for (unsigned long i=0; i<bytes; i++) out[i] = in[i];
}
static long prima_exec_count(char *const *vector) {
    if (!vector) return 0;
    for (long i=0; i<=16; i++) if (!vector[i]) return i;
    return -PRIMA_ERRNO_E2BIG;
}
static long prima_exec_common(const char *path, const void *provided, unsigned long provided_length,
                              char *const *argv, char *const *envp) {
    /* Pending console transactions still refer to this image's callback stack. */
    if (prima_tty_active() || prima_net_active()) return -PRIMA_ERRNO_EAGAIN;
    unsigned long bridge_size = (uintptr_t)__prima_exec_bridge_end - (uintptr_t)__prima_exec_bridge_start;
    if (!__prima_exec_bridge_start || !bridge_size || bridge_size > 4096) return -PRIMA_ERRNO_ENOSYS;
    if (prima_raw(PRIMA_SYS_TASK_INFO, 7, 0, 0, 0, 0, 0) != 1) return -PRIMA_ERRNO_EAGAIN;
    long argc = prima_exec_count(argv), envc = prima_exec_count(envp);
    if (argc < 0 || envc < 0) return -PRIMA_ERRNO_E2BIG;
    uint64_t full = ~PRIMA_UNMASKABLE, mask;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &mask, 8);
    if (result < 0) return result;
    long fd = -1;
    uint64_t image = (uint64_t)provided, length = provided_length, capacity = 0;
    long state_cap = 0;
    int state_mapped = 0;
    if (provided) goto prepare_state;
    fd = prima_openat(PRIMA_AT_FDCWD, path, PRIMA_O_RDONLY, 0);
    if (fd < 0) { result = fd; goto done; }
    struct prima_stat_meta meta;
    result = prima_meta_for(fd, "", PRIMA_AT_EMPTY_PATH, &meta);
    if (result < 0) goto close_image;
    if (meta.kind != PRIMA_VFS_KIND_FILE || !(meta.mode & 0111)) { result = -PRIMA_ERRNO_EACCES; goto close_image; }
    if (!meta.size) { result = -PRIMA_ERRNO_ENOEXEC; goto close_image; }
    capacity = ((uint64_t)meta.size + 4095) & ~UINT64_C(4095);
    {
        uint64_t old = prima_memory_enter();
        result = prima_mmap_anon(0, capacity, PRIMA_PROT_READ | PRIMA_PROT_WRITE,
                                PRIMA_MAP_PRIVATE | PRIMA_MAP_ANONYMOUS, -1, 0);
        (void)prima_memory_leave(old, 0);
    }
    if (result < 0) goto close_image;
    image = (uint64_t)result;
    while (length < meta.size) {
        result = prima_fd_read((int)fd, (void *)(image+length), meta.size-length);
        if (result < 0) goto close_image;
        if (!result) break;
        length += (uint64_t)result;
    }
    result = 0;
close_image:
    {
        long closed = prima_fd_close((int)fd);
        if (!result && closed < 0) result = closed;
    }
    if (result < 0) goto done;
prepare_state:
    if (!prima_contract_accepts((const void *)image, length)) {
        result = -PRIMA_ERRNO_ENOEXEC;
        goto done;
    }
    prima_heap_ready();
    if (!prima_arena_acquire()) { result = -PRIMA_ERRNO_EACCES; goto done; }
    state_cap = prima_raw(PRIMA_SYS_MEMSEG_CREATE, PRIMA_EXEC_STATE_PAGES, PRIMA_ACCESS_R | PRIMA_ACCESS_W | PRIMA_ACCESS_X, 0, 0, 0, 0);
    if (state_cap < 0) { result = state_cap; state_cap = 0; goto done; }
    result = prima_raw(PRIMA_SYS_MEMSEG_MAP, state_cap, prima_ctx.self_cap, 0,
                       PRIMA_EXEC_STATE_VA, PRIMA_EXEC_STATE_PAGES, PRIMA_ACCESS_R | PRIMA_ACCESS_W);
    if (result < 0) goto done;
    state_mapped = 1;
    struct prima_exec_state *state = (void *)PRIMA_EXEC_STATE_VA;
    prima_zero(PRIMA_EXEC_STATE_VA, PRIMA_EXEC_STATE_PAGES*4096);
    state->magic = PRIMA_EXEC_STATE_MAGIC; state->bytes = sizeof *state;
    state->cookie = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    state->mask = mask; state->arena = prima_arena_cap; state->waiter = prima_futex_current_cap();
    state->context = prima_ctx; state->context.fs_base = 0;
    state->fork_child = prima_fork_child; state->umask_value = prima_umask_value;
    state->termios = prima_tty_termios; state->termios_ready = prima_tty_termios_ready;
#define PRIMA_SAVE(field, variable) prima_copy(state->field, variable, sizeof state->field)
    PRIMA_SAVE(fds, prima_fds); PRIMA_SAVE(file_refs, prima_file_refs); PRIMA_SAVE(net_refs, prima_net_refs);
    PRIMA_SAVE(file_flags, prima_file_flags); PRIMA_SAVE(paths, prima_fd_paths); PRIMA_SAVE(cwd, prima_cwd);
    PRIMA_SAVE(dirs, prima_dirs); PRIMA_SAVE(forks, prima_forks); PRIMA_SAVE(present, prima_mmap_present);
#undef PRIMA_SAVE
    state->session = *prima_session_current();
    /* Compute the post-exec fd view without changing the old image. The RX
     * bridge commits the corresponding service closes before any ELF entry. */
    uint64_t *control = (void *)(PRIMA_EXEC_STATE_VA+4096);
    unsigned closes = 0;
    for (unsigned i=0; i<PRIMA_LOCAL_FD_COUNT; i++) {
        struct prima_fd_entry *entry = &state->fds[i];
        if (!entry->cloexec || entry->kind == PRIMA_FD_FREE) continue;
        uint64_t *request = control + 1 + closes*6;
        unsigned char handle = entry->object;
        if ((entry->kind == PRIMA_FD_FILE || entry->kind == PRIMA_FD_DIR) &&
            --state->file_refs[handle-PRIMA_FILE_FIRST_FD] == 0) {
            request[0] = (uint32_t)state->session.store_cookie;
            request[1] = state->session.store_cap;
            request[2] = PRIMA_STAGE5_FILE_REQUEST_TAG | ((uint64_t)PRIMA_FILE_OP_CLOSE << PRIMA_FILE_OP_SHIFT) |
                         ((uint64_t)handle << PRIMA_FILE_FD_SHIFT) | ((state->cookie & 0xff) << PRIMA_FILE_CLIENT_SHIFT);
            request[3] = PRIMA_STAGE5_FILE_REPLY_TAG; request[4] = state->session.store_cookie;
            request[5] = UINT64_C(0xff) << PRIMA_FILE_STATUS_SHIFT; closes++;
            state->dirs[handle-PRIMA_FILE_FIRST_FD].len = 0;
            state->dirs[handle-PRIMA_FILE_FIRST_FD].pos = 0;
        } else if (entry->kind == PRIMA_FD_SOCKET && --state->net_refs[handle] == 0) {
            request[0] = prima_ctx.manager_id; request[1] = prima_ctx.send_cap;
            request[2] = PRIMA_STAGE5_NET_REQUEST_TAG | ((uint64_t)PRIMA_NET_OP_CLOSE << PRIMA_NET_OP_SHIFT) |
                         ((uint64_t)handle << PRIMA_NET_HANDLE_SHIFT) | (UINT64_C(1) << PRIMA_NET_SEQUENCE_SHIFT);
            request[3] = PRIMA_STAGE5_NET_REPLY_TAG;
            request[4] = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 4, prima_ctx.send_cap, 0, 0, 0, 0);
            request[5] = UINT64_C(0xff) << PRIMA_NET_STATUS_SHIFT; closes++;
        }
        *entry = (struct prima_fd_entry){0}; state->paths[i][0] = 0;
    }
    control[0] = closes;
    prima_copy((void *)(PRIMA_EXEC_STATE_VA+8192), __prima_exec_bridge_start, bridge_size);
    result = prima_raw(PRIMA_SYS_MEMSEG_PROTECT, prima_ctx.self_cap, PRIMA_EXEC_STATE_VA+8192,
                       1, PRIMA_ACCESS_R | PRIMA_ACCESS_X, 0, 0);
    if (result < 0) goto done;
    uint64_t regions[2][2] = {{PRIMA_HEAP_VA, PRIMA_HEAP_PAGES}, {PRIMA_EXEC_STATE_VA, PRIMA_EXEC_STATE_PAGES}};
    uint64_t registers[6] = {prima_ctx.self_cap, prima_ctx.send_cap, prima_ctx.code_cap,
                            prima_ctx.name, prima_ctx.manager_id, PRIMA_EXEC_STATE_VA};
    uint64_t request[11] = {image, length, (uint64_t)argv, (uint64_t)argc, (uint64_t)envp, (uint64_t)envc,
                            (uint64_t)regions, 2, (uint64_t)registers, (uint64_t)state_cap, PRIMA_EXEC_STATE_VA+8192};
    result = prima_raw(PRIMA_SYS_EXEC_HANDOFF, (long)request, 0, 0, 0, 0, 0);
done:
    if (state_cap > 0) {
        if (state_mapped) (void)prima_raw(PRIMA_SYS_MEMSEG_UNMAP, prima_ctx.self_cap, PRIMA_EXEC_STATE_VA, PRIMA_EXEC_STATE_PAGES, 0, 0, 0);
        (void)prima_raw(PRIMA_SYS_MEMSEG_DESTROY, state_cap, 0, 0, 0, 0, 0);
        (void)prima_raw(PRIMA_SYS_CAP_DROP, state_cap, 0, 0, 0, 0, 0);
    }
    if (capacity && image) {
        uint64_t old = prima_memory_enter();
        (void)prima_memory_leave(old, prima_munmap_anon(image, capacity));
    }
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return result;
}
static long prima_execve(const char *path, char *const *argv, char *const *envp) {
    return prima_exec_common(path, 0, 0, argv, envp);
}
/* Native C memory-image entry, using the same opaque handoff as execve(path).
 * The bounded RAM rootfs cannot store a complete libc image (2920 B content). */
long prima_exec_image(const void *image, unsigned long length, char *const *argv, char *const *envp) {
    if (!image || !length) return -PRIMA_ERRNO_ENOEXEC;
    return prima_exec_common(0, image, length, argv, envp);
}
/* Called before musl/TLS initialization. Successful exec has already committed;
 * the bridge has already closed CLOEXEC descriptors. Restore the remaining fd
 * view and reset libc storage; an old-image failure never reaches this code. */
void prima_exec_restore(void) {
    if (prima_exec_state_pointer != PRIMA_EXEC_STATE_VA) return;
    const struct prima_exec_state *source = (const void *)PRIMA_EXEC_STATE_VA;
    if (source->magic != PRIMA_EXEC_STATE_MAGIC || source->bytes != sizeof *source ||
        source->cookie != (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0)) __builtin_trap();
    struct prima_exec_state state = *source;
    prima_ctx = state.context; prima_fork_child = state.fork_child;
    prima_arena_cap = state.arena; prima_arena_state = 1;
    prima_umask_value = state.umask_value;
    prima_tty_termios = state.termios; prima_tty_termios_ready = state.termios_ready;
#define PRIMA_LOAD(field, variable) prima_copy(variable, state.field, sizeof state.field)
    PRIMA_LOAD(fds, prima_fds); PRIMA_LOAD(file_refs, prima_file_refs); PRIMA_LOAD(net_refs, prima_net_refs);
    PRIMA_LOAD(file_flags, prima_file_flags); PRIMA_LOAD(paths, prima_fd_paths); PRIMA_LOAD(cwd, prima_cwd);
    PRIMA_LOAD(dirs, prima_dirs); PRIMA_LOAD(forks, prima_forks);
#undef PRIMA_LOAD
    *prima_session_current() = state.session;
    if (state.waiter) {
        (void)prima_raw(PRIMA_SYS_WAIT_DESTROY, state.waiter, 0, 0, 0, 0, 0);
        (void)prima_raw(PRIMA_SYS_CAP_DROP, state.waiter, 0, 0, 0, 0, 0);
    }
    for (unsigned page=0; page<PRIMA_MMAP_PAGES; page++) {
        if (!prima_bit_get(state.present, page) &&
            prima_arena_map_pages(page, 1, PRIMA_ACCESS_R | PRIMA_ACCESS_W) < 0) __builtin_trap();
    }
    if (prima_raw(PRIMA_SYS_MEMSEG_PROTECT, prima_ctx.self_cap, PRIMA_HEAP_VA, PRIMA_HEAP_PAGES,
                  PRIMA_ACCESS_R | PRIMA_ACCESS_W, 0, 0) < 0 ||
        prima_raw(PRIMA_SYS_VSPACE_ACCESS_ENABLE, prima_ctx.self_cap, PRIMA_HEAP_VA, PRIMA_HEAP_PAGES, 1, 0, 0) < 0) __builtin_trap();
    if (prima_raw(PRIMA_SYS_MEMSEG_UNMAP, prima_ctx.self_cap, PRIMA_EXEC_STATE_VA, PRIMA_EXEC_STATE_PAGES, 0, 0, 0) < 0) __builtin_trap();
    prima_exec_state_pointer = 0;
    /* The inherited arena supplies fresh malloc/mmap storage to the new libc. */
    prima_zero(PRIMA_HEAP_VA, PRIMA_HEAP_PAGES*4096);
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &state.mask, 0, 8);
}

/* ---------- M16b：clone / 线程退出 ---------- */

long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);

#define PRIMA_THREAD_SLOTS 8

struct prima_thread_record {
    long tid;
    int *clear_tid;
};

static struct prima_thread_record prima_threads[PRIMA_THREAD_SLOTS];
static unsigned long prima_linux_flags_to_prima(unsigned long flags) {
    /* 只保留内核 clone 支持的位；其余（含 CLONE_VFORK/PID 等）拒绝。 */
    const unsigned long allowed = PRIMA_CLONE_VM | PRIMA_CLONE_FS | PRIMA_CLONE_FILES |
                                  PRIMA_CLONE_SIGHAND | PRIMA_CLONE_THREAD |
                                  PRIMA_CLONE_SYSVSEM | PRIMA_CLONE_SETTLS |
                                  PRIMA_CLONE_PARENT_SETTID |
                                  PRIMA_CLONE_CHILD_CLEARTID |
                                  PRIMA_CLONE_CHILD_SETTID | PRIMA_CLONE_DETACHED;
    if (flags & ~allowed)
        return 0;
    return flags;
}

static void prima_thread_record(long tid, int *ctid) {
    for (int i = 0; i < PRIMA_THREAD_SLOTS; i++) {
        if (prima_threads[i].tid == 0) {
            prima_threads[i].tid = tid;
            prima_threads[i].clear_tid = ctid;
            return;
        }
    }
}

static long prima_clone(unsigned long flags, void *stack, int *ptid, int *ctid,
                        unsigned long tls) {
    unsigned long prima_flags = prima_linux_flags_to_prima(flags);
    if (!prima_flags || !stack || (prima_flags & (PRIMA_CLONE_VM | PRIMA_CLONE_THREAD)) !=
                                      (PRIMA_CLONE_VM | PRIMA_CLONE_THREAD))
        return -PRIMA_ERRNO_EINVAL;
    long tid = prima_native(PRIMA_SYS_CLONE, (long)prima_flags, (long)stack,
                            (long)ptid, (long)ctid, (long)tls, 0);
    if (tid <= 0)
        return tid; /* 0 = 子线程；负值 = errno */
    if ((flags & PRIMA_CLONE_PARENT_SETTID) && ptid)
        *ptid = (int)tid;
    if ((flags & (PRIMA_CLONE_CHILD_SETTID | PRIMA_CLONE_CHILD_CLEARTID)) && ctid) {
        *ctid = (int)tid;
        if (flags & PRIMA_CLONE_CHILD_CLEARTID)
            prima_thread_record(tid, ctid);
    }
    return tid;
}

static void prima_thread_cleanup(void) {
    prima_session_thread_cleanup();
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    for (int i = 0; i < PRIMA_THREAD_SLOTS; i++) {
        if (prima_threads[i].tid == id) {
            if (prima_threads[i].clear_tid) {
                *prima_threads[i].clear_tid = 0;
                (void)prima_futex(prima_threads[i].clear_tid, 1, 0x7fffffff, 0);
            }
            prima_threads[i].tid = 0;
            prima_threads[i].clear_tid = 0;
        }
    }
    /* M16b-3：清除 clone 的 child_tid 并 futex 唤醒（musl 线程退出）。 */
    long clear_tid = prima_native(PRIMA_SYS_THREAD_CLEAR_TID, 0, 0, 0, 0, 0, 0);
    if (clear_tid > 0) {
        *(volatile int *)clear_tid = 0;
        (void)prima_futex((int *)clear_tid, 1, 0x7fffffff, 0);
    }
    prima_futex_cleanup();
}
__attribute__((noreturn)) static void prima_thread_exit(void) {
    prima_thread_cleanup();
    (void)prima_native(PRIMA_SYS_THREAD_EXIT, 0, 0, 0, 0, 0, 0);
    for (;;) {
        long sink[3];
        (void)prima_raw(PRIMA_SYS_RECV, (long)sink, 0, 0, 0, 0, 0);
    }
}

__attribute__((noreturn, visibility("hidden"))) void __prima_unmapself(uint64_t base, uint64_t length) {
    if (!length || base < PRIMA_MMAP_BASE || base >= PRIMA_MMAP_LIMIT ||
        length > PRIMA_MMAP_LIMIT-base || (base & 4095) || (length & 4095)) __builtin_trap();
    uint64_t full = ~PRIMA_UNMASKABLE;
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, 0, 8);
    prima_thread_cleanup();
    uint64_t cookie = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    uint32_t id = (uint32_t)cookie;
    if (id >= 18) __builtin_trap();
    (void)prima_memory_enter();
    prima_detached[id] = (struct prima_detached_range){cookie, base, length};
    __sync_lock_release(&prima_memory_lock);
    /* The allocator keeps this range occupied until this incarnation is gone. */
    (void)prima_raw(PRIMA_SYS_THREAD_EXIT_UNMAP, prima_ctx.self_cap, base, length/4096, 0, 0, 0);
    __builtin_trap();
}

__attribute__((noreturn, visibility("hidden"))) void __prima_thread_exit(void) {
    prima_thread_exit();
}

__attribute__((noreturn)) static void prima_exit_group(int status) {
    if (prima_fork_child) {
        prima_close_all_fds();
        (void)prima_session_control(PRIMA_SESSION_CLOSE);
        (void)prima_raw(PRIMA_SYS_PROCESS_EXIT, status, 0, 0, 0, 0, 0);
        __builtin_trap();
    }
    prima_close_all_fds();
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    uint64_t payload = PRIMA_STAGE5_C_PROBE_TAG |
                       ((uint64_t)(uint8_t)status << 16) |
                       ((uint64_t)id & 0xff);
    /* Report then park: the manager owns final destruction and releases the
     * arena only after this task can no longer run. Queued outgoing messages
     * now survive sender destruction independently, as required by COSIR. */
    (void)prima_raw(PRIMA_SYS_SEND, (long)prima_ctx.manager_id,
                    (long)prima_ctx.send_cap, (long)payload, 0, 0, 0);
    for (;;) {
        long sink[3];
        (void)prima_raw(PRIMA_SYS_RECV, (long)sink, 0, 0, 0, 0, 0);
    }
}

/* Linux 号 → Prima 动作。六参数完整透传：musl 的取消点路径经 `__syscall6` 调用，
 * 少传参数会静默丢参。未列出的号一律 -ENOSYS（不在本阶段承诺）。 */
static long prima_translate_body(long number, long a0, long a1, long a2, long a3,
                            long a4, long a5) {
    prima_signals_ready();
    uint64_t memory_mask;
    switch (number) {
    case PRIMA_LINUX_SYS_execve:
        return prima_execve((const char *)a0, (char *const *)a1, (char *const *)a2);
    case PRIMA_LINUX_SYS_mmap:
        /* Linux 顺序：addr, len, prot, flags, fd, offset。 */
        memory_mask = prima_memory_enter();
        return prima_memory_leave(memory_mask, prima_mmap_anon(a0, (uint64_t)a1, a2, a3, a4, a5));
    case PRIMA_LINUX_SYS_munmap:
        memory_mask = prima_memory_enter();
        return prima_memory_leave(memory_mask, prima_munmap_anon((uint64_t)a0, (uint64_t)a1));
    case PRIMA_LINUX_SYS_mprotect:
        memory_mask = prima_memory_enter();
        return prima_memory_leave(memory_mask, prima_mprotect_anon((uint64_t)a0, (uint64_t)a1, a2));
    case PRIMA_LINUX_SYS_brk:
        memory_mask = prima_memory_enter();
        return prima_memory_leave(memory_mask, prima_brk((uint64_t)a0));
    case PRIMA_LINUX_SYS_write:
        return prima_fd_write((int)a0, (const void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_writev:
        return prima_fd_iov((int)a0, (const struct iovec *)a1, a2, 1);
    case PRIMA_LINUX_SYS_readv:
        return prima_fd_iov((int)a0, (const struct iovec *)a1, a2, 0);
    case PRIMA_LINUX_SYS_close:
        return prima_fd_close((int)a0);
    case PRIMA_LINUX_SYS_read:
        return prima_fd_read((int)a0, (void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_socket:
        return prima_net_socket(a0, a1, a2);
    case PRIMA_LINUX_SYS_bind:
        return prima_net_bind((int)a0, (const void *)a1, (int)a2);
    case PRIMA_LINUX_SYS_getsockname:
        return prima_net_getsockname((int)a0, (void *)a1, (int *)a2);
    case PRIMA_LINUX_SYS_sendto:
        return prima_net_sendto((int)a0, (const void *)a1, (unsigned long)a2,
                                (int)a3, (const void *)a4, (int)a5);
    case PRIMA_LINUX_SYS_recvfrom:
        return prima_net_recvfrom((int)a0, (void *)a1, (unsigned long)a2,
                                  (int)a3, (void *)a4, (int *)a5);
    case PRIMA_LINUX_SYS_getdents64:
        return prima_getdents64((int)a0, (void *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_lseek:
        return prima_fd_lseek((int)a0, (long)a1, (int)a2);
    case PRIMA_LINUX_SYS_ftruncate:
        return prima_fd_resize((int)a0, a1);
    case PRIMA_LINUX_SYS_pread64:
        return prima_fd_at((int)a0, (void *)a1, (unsigned long)a2, a3, 0);
    case PRIMA_LINUX_SYS_pwrite64:
        return prima_fd_at((int)a0, (void *)a1, (unsigned long)a2, a3, 1);
    case PRIMA_LINUX_SYS_mkdir:
        return prima_mkdirat(PRIMA_AT_FDCWD, (const char *)a0, a1);
    case PRIMA_LINUX_SYS_mkdirat:
        return prima_mkdirat(a0, (const char *)a1, a2);
    case PRIMA_LINUX_SYS_rmdir:
        return prima_unlinkat(PRIMA_AT_FDCWD, (const char *)a0,
                              PRIMA_AT_REMOVEDIR);
    case PRIMA_LINUX_SYS_unlink:
        return prima_unlinkat(PRIMA_AT_FDCWD, (const char *)a0, 0);
    case PRIMA_LINUX_SYS_unlinkat:
        return prima_unlinkat(a0, (const char *)a1, a2);
    case PRIMA_LINUX_SYS_dup:
        return prima_fd_dup((int)a0);
    case PRIMA_LINUX_SYS_dup2:
        return prima_fd_dup2((int)a0, (int)a1);
    case PRIMA_LINUX_SYS_dup3:
        return prima_fd_dup3((int)a0, (int)a1, a2);
    case PRIMA_LINUX_SYS_fcntl:
        return prima_fd_fcntl((int)a0, a1, a2);
    case PRIMA_LINUX_SYS_getppid:
        return prima_raw(PRIMA_SYS_PROCESS_ID, 1, 0, 0, 0, 0, 0);
    case PRIMA_LINUX_SYS_getpid:
        return prima_raw(PRIMA_SYS_PROCESS_ID, 0, 0, 0, 0, 0, 0);
    /* musl/x86_64 的 `open()` 走 SYS_open(2)，不是 openat(257)；两者落到同一子集。 */
    case PRIMA_LINUX_SYS_open:
        return prima_openat(PRIMA_AT_FDCWD, (const char *)a0, a1, a2);
    case PRIMA_LINUX_SYS_openat:
        return prima_openat(a0, (const char *)a1, a2, a3);
    /* M9：x86_64 musl 的 stat/lstat/fstat/fstatat 走 legacy kstat 号 4/5/6/262；
     * 直接调用 statx 的程序走 332。cwd 相关调用走 chdir(80)/getcwd(79)。 */
    case PRIMA_LINUX_SYS_stat:
        return prima_stat_kstat(PRIMA_AT_FDCWD, (const char *)a0, 0,
                                (void *)a1);
    case PRIMA_LINUX_SYS_lstat:
        return prima_stat_kstat(PRIMA_AT_FDCWD, (const char *)a0,
                                PRIMA_AT_SYMLINK_NOFOLLOW, (void *)a1);
    case PRIMA_LINUX_SYS_fstat:
        return prima_stat_kstat((int)a0, "", PRIMA_AT_EMPTY_PATH, (void *)a1);
    case PRIMA_LINUX_SYS_newfstatat:
        return prima_stat_kstat((int)a0, (const char *)a1, a3, (void *)a2);
    case PRIMA_LINUX_SYS_statx:
        return prima_statx((int)a0, (const char *)a1, a2, a3, (void *)a4);
    case PRIMA_LINUX_SYS_chdir:
        return prima_chdir((const char *)a0);
    case PRIMA_LINUX_SYS_fchdir:
        return prima_fchdir((int)a0);
    case PRIMA_LINUX_SYS_chmod:
        return prima_chmod_path(PRIMA_AT_FDCWD, (const char *)a0, a1);
    case PRIMA_LINUX_SYS_fchmod:
        return prima_fchmod((int)a0, a1);
    case PRIMA_LINUX_SYS_fchmodat:
        if (a3 != 0)
            return -PRIMA_ERRNO_EINVAL;
        return prima_chmod_path((long)a0, (const char *)a1, a2);
    case PRIMA_LINUX_SYS_umask:
        return prima_kernel_umask(a0);
    case PRIMA_LINUX_SYS_getcwd:
        return prima_getcwd((char *)a0, (unsigned long)a1);
    case PRIMA_LINUX_SYS_rename:
        return prima_renameat(PRIMA_AT_FDCWD, (const char *)a0,
                              PRIMA_AT_FDCWD, (const char *)a1);
    case PRIMA_LINUX_SYS_renameat:
        return prima_renameat((long)a0, (const char *)a1, (long)a2,
                              (const char *)a3);
    case PRIMA_LINUX_SYS_symlink:
        return prima_symlinkat((const char *)a0, PRIMA_AT_FDCWD,
                               (const char *)a1);
    case PRIMA_LINUX_SYS_symlinkat:
        return prima_symlinkat((const char *)a0, (long)a1,
                               (const char *)a2);
    case PRIMA_LINUX_SYS_readlink:
        return prima_readlinkat(PRIMA_AT_FDCWD, (const char *)a0,
                                (char *)a1, (unsigned long)a2);
    case PRIMA_LINUX_SYS_readlinkat:
        return prima_readlinkat((long)a0, (const char *)a1, (char *)a2,
                                (unsigned long)a3);
    case PRIMA_LINUX_SYS_ioctl:
        return prima_tty_ioctl((int)a0, a1, a2);
    case PRIMA_LINUX_SYS_futex:
        return prima_futex((int *)a0, (int)a1, (int)a2, (void *)a3);
    case PRIMA_LINUX_SYS_fork: {
        if (prima_tty_active() || prima_net_active()) return -PRIMA_ERRNO_EAGAIN;
        long reserved = prima_session_control(PRIMA_SESSION_RESERVE);
        if (reserved < 0) return reserved;
        uint32_t key = (uint32_t)reserved;
        if (!key) return -PRIMA_ERRNO_EAGAIN;
        long ticket = prima_raw(PRIMA_SYS_FORK_TRANSACTION, 0, 0, 0, 0, 0, 0);
        if (ticket < 0) { (void)prima_session_control(PRIMA_SESSION_RELEASE | key); return ticket; }
        if (!ticket) {
            prima_fork_child = 1;
            if (prima_session_accept() < 0) {
                (void)prima_raw(PRIMA_SYS_PROCESS_EXIT, 127, 0, 0, 0, 0, 0);
                __builtin_trap();
            }
            return 0;
        }
        long cookie = prima_raw(PRIMA_SYS_TASK_INFO, 4, ticket, 0, 0, 0, 0);
        long reply = prima_raw(PRIMA_SYS_CAP_DERIVE, ticket, (1 << 0) | (1 << 5), 0, 0, 0, 0);
        long control = reply < 0 ? reply : prima_raw(PRIMA_SYS_CAP_DERIVE, ticket, (1 << 5) | (1 << 7), 0, 0, 0, 0);
        long failure = control < 0 ? control : 0;
        if (failure && reply >= 0) (void)prima_raw(PRIMA_SYS_CAP_DROP, reply, 0, 0, 0, 0, 0);
        if (!failure) {
            long sent = prima_raw(PRIMA_SYS_SEND, prima_ctx.manager_id, prima_ctx.send_cap,
                                  PRIMA_SESSION_FORK_REGISTER | key, 2, reply, control);
            if (sent < 0) {
                (void)prima_raw(PRIMA_SYS_CAP_DROP, reply, 0, 0, 0, 0, 0);
                (void)prima_raw(PRIMA_SYS_CAP_DROP, control, 0, 0, 0, 0, 0);
                failure = sent;
            }
            else {
                long ack = prima_fork_handoff_receive();
                if (ack < 0) failure = ack;
                else if ((uint32_t)ack) failure = -(long)(uint32_t)ack;
            }
        }
        long result = failure ? failure : prima_raw(PRIMA_SYS_FORK_TRANSACTION, 1, ticket, 0, 0, 0, 0);
        if (result < 0) {
            (void)prima_raw(PRIMA_SYS_FORK_TRANSACTION, 2, ticket, 0, 0, 0, 0);
            (void)prima_session_control(PRIMA_SESSION_RELEASE | key);
            return result;
        }
        (void)prima_raw(PRIMA_SYS_CAP_DROP, ticket, 0, 0, 0, 0, 0);
        if ((uint32_t)cookie != (uint32_t)result) __builtin_trap();
        for (int i=0; i<PRIMA_SESSION_SLOTS; i++) if (!prima_forks[i].pid) {
            prima_forks[i].pid = result; prima_forks[i].key = key; break;
        }
        return result;
    }
    case PRIMA_LINUX_SYS_rt_sigsuspend: {
        if (a1 != 8 || !a0) return -PRIMA_ERRNO_EINVAL;
        uint64_t mask = *(const uint64_t *)a0 & ~PRIMA_UNMASKABLE;
        return prima_raw(PRIMA_SYS_UPCALL_CONTROL, 6, mask, 0, 0, 0, 0);
    }
    case PRIMA_LINUX_SYS_sched_yield:
        return prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    case PRIMA_LINUX_SYS_pause: {
        uint64_t mask;
        long result = prima_rt_sigprocmask(0, 0, &mask, 8);
        return result < 0 ? result : prima_raw(PRIMA_SYS_UPCALL_CONTROL, 6, mask, 0, 0, 0, 0);
    }
    case PRIMA_LINUX_SYS_rt_sigaction:
        return prima_rt_sigaction((int)a0, (const void *)a1, (void *)a2, a3);
    case PRIMA_LINUX_SYS_rt_sigprocmask:
        return prima_rt_sigprocmask(a0, (const void *)a1, (void *)a2, a3);
    case PRIMA_LINUX_SYS_rt_sigpending:
        if (a1 != 8) return -PRIMA_ERRNO_EINVAL;
        return prima_raw(PRIMA_SYS_UPCALL_CONTROL, 2, a0, 0, 0, 0, 0);
    case PRIMA_LINUX_SYS_kill:
        return prima_kill(a0, a1, 0, 0);
    case PRIMA_LINUX_SYS_tkill:
        return prima_kill(a0, a1, 1, 0);
    case PRIMA_LINUX_SYS_tgkill:
        return prima_kill(a1, a2, 1, a0);
    case PRIMA_LINUX_SYS_wait4: {
        long result = prima_native(PRIMA_SYS_WAIT4, a0, a1, a2, 0, 0, 0);
        if (result > 0) for (int i=0; i<PRIMA_SESSION_SLOTS; i++) if (prima_forks[i].pid == result) {
            (void)prima_session_control(PRIMA_SESSION_RELEASE | prima_forks[i].key);
            prima_forks[i].pid = 0; prima_forks[i].key = 0; break;
        }
        return result;
    }
    case PRIMA_LINUX_SYS_clone:
        return prima_clone((unsigned long)a0, (void *)a1, (int *)a2, (int *)a3,
                           (unsigned long)a4);
    case PRIMA_LINUX_SYS_clock_gettime:
        return prima_clock_gettime(a0, (void *)a1);
    case PRIMA_LINUX_SYS_clock_nanosleep:
        return prima_sleep_ns(a0, a1, (const void *)a2, (void *)a3);
    case PRIMA_LINUX_SYS_nanosleep:
        return prima_sleep_ns(PRIMA_CLOCK_REALTIME, 0, (const void *)a0,
                              (void *)a1);
    case PRIMA_LINUX_SYS_set_tid_address:
        (void)a0;
        return prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    case PRIMA_LINUX_SYS_arch_prctl:
        if (a0 == PRIMA_ARCH_SET_FS) {
            long result = prima_raw(PRIMA_SYS_TASK_SET_FS_BASE, a1, 0, 0, 0, 0, 0);
            if (result == 0)
                prima_ctx.fs_base = (uint64_t)a1;
            return result;
        }
        if (a0 == PRIMA_ARCH_GET_FS) {
            if (!a1)
                return -PRIMA_ERRNO_EFAULT;
            *(unsigned long *)a1 = prima_ctx.fs_base;
            return 0;
        }
        return -PRIMA_ERRNO_EINVAL;
    case PRIMA_LINUX_SYS_exit: {
        long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
        for (int i = 0; i < PRIMA_THREAD_SLOTS; i++)
            if (prima_threads[i].tid == id)
                prima_thread_exit();
        /* musl 的 clone.s 直接发 Prima 48，适配层的线程表可能没有记录；
         * 先清 child_tid 并唤醒 joiner，再试 SYS_THREAD_EXIT；leader 回退到
         * 进程退出。 */
        if (prima_raw(PRIMA_SYS_PROCESS_ID, 0, 0, 0, 0, 0, 0) != id)
            prima_session_thread_cleanup();
        long clear_tid = prima_native(PRIMA_SYS_THREAD_CLEAR_TID, 0, 0, 0, 0, 0, 0);
        if (clear_tid > 0) {
            *(volatile int *)clear_tid = 0;
            (void)prima_futex((int *)clear_tid, 1, 0x7fffffff, 0);
        }
        (void)prima_native(PRIMA_SYS_THREAD_EXIT, 0, 0, 0, 0, 0, 0);
        prima_exit_group((int)a0);
    }
    case PRIMA_LINUX_SYS_exit_group:
        prima_exit_group((int)a0);
    default:
        return -PRIMA_ERRNO_ENOSYS;
    }
}

/* M15：Prima 原生 syscall 直通（供只存在于 Prima 的机制探针使用；
 * 避免与 Linux 42-46 的 connect/accept/sendto/recvfrom/sendmsg 号冲突）。 */
/* ---------- M16a：futex（用户态表 + WaitObject） ---------- */

long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5);

/* One waiter per Task avoids both a finite address cache and wake(count)
 * accidentally waking every thread waiting at that address. */
#define PRIMA_FUTEX_SLOTS 18
struct prima_futex_entry { uint64_t addr, cookie; long cap; };
static struct prima_futex_entry prima_futexes[PRIMA_FUTEX_SLOTS];
static struct prima_futex_entry *prima_futex_slot(uint64_t addr) {
    uint64_t cookie = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    unsigned id = (uint32_t)cookie;
    if (id >= PRIMA_FUTEX_SLOTS) return 0;
    struct prima_futex_entry *entry = &prima_futexes[id];
    if (entry->cookie != cookie && entry->cap > 0) {
        (void)prima_raw(PRIMA_SYS_WAIT_DESTROY, entry->cap, 0, 0, 0, 0, 0);
        (void)prima_raw(PRIMA_SYS_CAP_DROP, entry->cap, 0, 0, 0, 0, 0);
        entry->cap = 0;
    }
    if (entry->cap <= 0) {
        long cap = prima_raw(PRIMA_SYS_WAIT_CREATE, 0, 0, 0, 0, 0, 0);
        if (cap <= 0) return 0;
        entry->cap = cap;
    }
    entry->cookie = cookie;
    entry->addr = 0;
    (void)addr;
    return entry;
}
static void prima_futex_cleanup(void) {
    long id = prima_raw(PRIMA_SYS_TASK_ID, 0, 0, 0, 0, 0, 0);
    if (id < 0 || id >= PRIMA_FUTEX_SLOTS) return;
    struct prima_futex_entry *entry = &prima_futexes[id];
    if (entry->cap > 0) {
        (void)prima_raw(PRIMA_SYS_WAIT_DESTROY, entry->cap, 0, 0, 0, 0, 0);
        (void)prima_raw(PRIMA_SYS_CAP_DROP, entry->cap, 0, 0, 0, 0, 0);
    }
    *entry = (struct prima_futex_entry){0};
}

static long prima_futex_current_cap(void) {
    uint64_t cookie = (uint64_t)prima_raw(PRIMA_SYS_TASK_INFO, 0, 0, 0, 0, 0, 0);
    unsigned id = (uint32_t)cookie;
    return id < PRIMA_FUTEX_SLOTS && prima_futexes[id].cookie == cookie ? prima_futexes[id].cap : 0;
}
static long prima_futex_wait(int *uaddr, int value, const void *timeout) {
    if (!uaddr)
        return -PRIMA_ERRNO_EFAULT;
    if (*(volatile int *)uaddr != value)
        return -PRIMA_ERRNO_EAGAIN;
    struct prima_futex_entry *entry = prima_futex_slot((uint64_t)uaddr);
    if (!entry)
        return -PRIMA_ERRNO_EFAULT;
    long generation = prima_native(PRIMA_SYS_WAIT_OBSERVE, entry->cap, 0, 0, 0, 0, 0);
    if (generation < 0)
        return -PRIMA_ERRNO_EAGAIN;
    /* 观察后重查，避免 observe 与阻塞之间丢唤醒。 */
    if (*(volatile int *)uaddr != value)
        return -PRIMA_ERRNO_EAGAIN;
    uint64_t deadline = UINT64_MAX;
    if (timeout) {
        const long *parts = timeout;
        if (parts[0] < 0 || parts[1] < 0 || parts[1] >= 1000000000L)
            return -PRIMA_ERRNO_EINVAL;
        long ready = prima_time_ready();
        if (ready < 0) return ready;
        if ((uint64_t)parts[0] > (UINT64_MAX - (uint64_t)parts[1] - 9999999) / 1000000000)
            return -PRIMA_ERRNO_EINVAL;
        uint64_t nanoseconds = (uint64_t)parts[0] * 1000000000 + (uint64_t)parts[1];
        uint64_t ticks = (nanoseconds + 9999999) / 10000000;
        long current = prima_ticks();
        if (current < 0) return current;
        if (ticks >= UINT64_MAX - (uint64_t)current) return -PRIMA_ERRNO_EINVAL;
        deadline = (uint64_t)current + ticks;
    }
    entry->addr = (uint64_t)uaddr;
    long result = prima_native(PRIMA_SYS_WAIT_WORD, entry->cap, generation,
                               (long)uaddr, value, (long)deadline, 0);
    entry->addr = 0;
    if (result < 0) return result;
    if (result == 0)
        return 0;
    if (result == 1)
        return -PRIMA_ERRNO_ETIMEDOUT;
    return -PRIMA_ERRNO_EAGAIN;
}

static long prima_futex_wake(int *uaddr, int count) {
    if (!uaddr)
        return -PRIMA_ERRNO_EFAULT;
    if (count <= 0)
        return 0;
    long woken = 0;
    for (unsigned i=0; i<PRIMA_FUTEX_SLOTS && woken<count; i++) {
        struct prima_futex_entry *entry = &prima_futexes[i];
        if (entry->cap <= 0 || entry->addr != (uint64_t)uaddr) continue;
        long live = prima_raw(PRIMA_SYS_TASK_INFO, 1, (uint32_t)entry->cookie, 0, 0, 0, 0);
        if (live < 0 || (uint64_t)live != entry->cookie) continue;
        long result = prima_raw(PRIMA_SYS_WAIT_SIGNAL, entry->cap, 0, 0, 0, 0, 0);
        if (result < 0) return result;
        woken += (long)((uint64_t)result >> 32);
    }
    return woken;
}

/* Linux futex 号 202；只支持 WAIT/WAKE（PRIVATE 标志按同一进程内语义处理）。 */
static long prima_futex(int *uaddr, int op, int value, const void *timeout) {
    int command = op & PRIMA_FUTEX_CMD_MASK;
    if (command == PRIMA_FUTEX_WAIT)
        return prima_futex_wait(uaddr, value, timeout);
    if (command == PRIMA_FUTEX_WAKE)
        return prima_futex_wake(uaddr, value);
    return -PRIMA_ERRNO_ENOSYS;
}

long prima_native(long number, long a0, long a1, long a2, long a3, long a4, long a5) {
    return prima_raw(number, a0, a1, a2, a3, a4, a5);
}

/* Fast descriptor transactions finish before delivering a callback. Blocking
 * waits stay interruptible and own explicit cancellation/restart protocols. */
static long prima_translate(long number, long a0, long a1, long a2, long a3, long a4, long a5) {
    int fast = 0;
    switch (number) {
    case PRIMA_LINUX_SYS_open: case PRIMA_LINUX_SYS_openat:
    case PRIMA_LINUX_SYS_close: case PRIMA_LINUX_SYS_dup: case PRIMA_LINUX_SYS_dup2:
    case PRIMA_LINUX_SYS_dup3: case PRIMA_LINUX_SYS_fcntl:
    case PRIMA_LINUX_SYS_stat: case PRIMA_LINUX_SYS_lstat: case PRIMA_LINUX_SYS_fstat:
    case PRIMA_LINUX_SYS_newfstatat: case PRIMA_LINUX_SYS_statx:
    case PRIMA_LINUX_SYS_chdir: case PRIMA_LINUX_SYS_fchdir: case PRIMA_LINUX_SYS_getcwd:
    case PRIMA_LINUX_SYS_chmod: case PRIMA_LINUX_SYS_fchmod: case PRIMA_LINUX_SYS_fchmodat:
    case PRIMA_LINUX_SYS_umask: case PRIMA_LINUX_SYS_rename: case PRIMA_LINUX_SYS_renameat:
    case PRIMA_LINUX_SYS_mkdir: case PRIMA_LINUX_SYS_mkdirat: case PRIMA_LINUX_SYS_rmdir:
    case PRIMA_LINUX_SYS_unlink: case PRIMA_LINUX_SYS_unlinkat:
    case PRIMA_LINUX_SYS_symlink: case PRIMA_LINUX_SYS_symlinkat:
    case PRIMA_LINUX_SYS_readlink: case PRIMA_LINUX_SYS_readlinkat:
    case PRIMA_LINUX_SYS_getdents64: case PRIMA_LINUX_SYS_lseek:
    case PRIMA_LINUX_SYS_ftruncate: case PRIMA_LINUX_SYS_pread64:
    case PRIMA_LINUX_SYS_pwrite64:
    case PRIMA_LINUX_SYS_ioctl: case PRIMA_LINUX_SYS_fork:
    case PRIMA_LINUX_SYS_socket: case PRIMA_LINUX_SYS_bind:
    case PRIMA_LINUX_SYS_getsockname: case PRIMA_LINUX_SYS_sendto:
        fast = 1; break;
    case PRIMA_LINUX_SYS_read: case PRIMA_LINUX_SYS_readv:
    case PRIMA_LINUX_SYS_write: case PRIMA_LINUX_SYS_writev: {
        struct prima_fd_entry *entry = prima_fd_entry((int)a0);
        fast = entry && entry->kind != PRIMA_FD_TTY && entry->kind != PRIMA_FD_SOCKET;
        break;
    }
    default: break;
    }
    if (!fast) return prima_translate_body(number, a0, a1, a2, a3, a4, a5);
    uint64_t mask, full = ~PRIMA_UNMASKABLE;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &full, &mask, 8);
    if (result < 0) return result;
    // ponytail: bounded single-CPU Process lock; split only when concurrent VFS throughput matters.
    while (__sync_lock_test_and_set(&prima_fd_lock, 1)) (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
    result = prima_translate_body(number, a0, a1, a2, a3, a4, a5);
    __sync_lock_release(&prima_fd_lock);
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return result;
}

long __syscall0(long number) { return prima_translate(number, 0, 0, 0, 0, 0, 0); }

long __syscall1(long number, long a0) {
    return prima_translate(number, a0, 0, 0, 0, 0, 0);
}

long __syscall2(long number, long a0, long a1) {
    return prima_translate(number, a0, a1, 0, 0, 0, 0);
}

long __syscall3(long number, long a0, long a1, long a2) {
    return prima_translate(number, a0, a1, a2, 0, 0, 0);
}

long __syscall4(long number, long a0, long a1, long a2, long a3) {
    return prima_translate(number, a0, a1, a2, a3, 0, 0);
}

long __syscall5(long number, long a0, long a1, long a2, long a3, long a4) {
    return prima_translate(number, a0, a1, a2, a3, a4, 0);
}

long __syscall6(long number, long a0, long a1, long a2, long a3, long a4,
                long a5) {
    return prima_translate(number, a0, a1, a2, a3, a4, a5);
}

int arch_prctl(int code, unsigned long addr) {
    return (int)__syscall2(PRIMA_LINUX_SYS_arch_prctl, code, (long)addr);
}

long set_tid_address(int *tidptr) {
    return __syscall1(PRIMA_LINUX_SYS_set_tid_address, (long)tidptr);
}
