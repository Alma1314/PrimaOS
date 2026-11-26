/* P3-3 acceptance probe for direct v2 sessions (V5 image only).
 *
 * Compiled under `PRIMA_ASYNC_PROBE` together with the adapter TU, so it can
 * drive the same session functions production clients use. It never runs in a
 * normal image. The probe prints no trace itself; `stage5_async.c` prints one
 * `S9ASYNC|case=...;status=...` line per call and a final `S9ASYNCDONE`.
 *
 * Each function returns 0 on success or a small positive case failure code.
 * The sequencing is deliberate: some cases reset the Store session, so they
 * abandon and re-open their own mapping before returning. */
#ifdef PRIMA_ASYNC_PROBE

static volatile unsigned char *prima_async_probe_control(void) {
    struct prima_session *session = prima_session_current();
    return (volatile unsigned char *)prima_async_address(session);
}

static volatile unsigned char *prima_async_probe_page(uint64_t ordinal) {
    struct prima_session *session = prima_session_current();
    return (volatile unsigned char *)(prima_async_address(session) +
        (1 + ordinal % PRIMA_ASYNC_CREDITS) * UINT64_C(4096));
}

/* One single-op batch page; the caller fills the payload or reads it back. */
static void prima_async_probe_batch(uint64_t ordinal, unsigned char op,
                                    unsigned char handle, uint64_t offset,
                                    unsigned long count) {
    volatile unsigned char *page = prima_async_probe_page(ordinal);
    for (unsigned i = 0; i < 4096; i++) page[i] = 0;
    page[0] = 'P'; page[1] = 'B'; page[2] = 'I'; page[3] = 'O'; page[4] = 1;
    prima_bulk_put(page + 8, ordinal + 1, 8);
    prima_bulk_put(page + 16, 1, 4);
    page[32] = op;
    page[33] = handle;
    prima_bulk_put(page + 40, offset, 8);
    prima_bulk_put(page + 48, count, 4);
    prima_bulk_put(page + 52, PRIMA_BULK_DATA, 4);
}

static void prima_async_probe_put(uint64_t ordinal, const char *bytes,
                                  unsigned long count) {
    volatile unsigned char *page = prima_async_probe_page(ordinal);
    for (unsigned long i = 0; i < count; i++)
        page[PRIMA_BULK_DATA + i] = (unsigned char)bytes[i];
    __asm__ volatile("" ::: "memory");
}

static void prima_async_probe_set_tail(uint64_t tail) {
    prima_bulk_put(prima_async_probe_control() + PRIMA_ASYNC_TAIL_OFFSET, tail, 8);
    __asm__ volatile("" ::: "memory");
}

static long prima_async_probe_doorbell(uint64_t tail) {
    struct prima_session *session = prima_session_current();
    uint64_t payload = PRIMA_ASYNC_DOORBELL |
        ((session->async_generation & PRIMA_ASYNC_GENERATION_MASK)
         << PRIMA_ASYNC_GENERATION_SHIFT) |
        (prima_async_slot(session) << PRIMA_ASYNC_SLOT_SHIFT) |
        (uint64_t)(uint32_t)tail;
    return prima_session_send(payload);
}

static long prima_async_probe_cancel_at(uint64_t ordinal) {
    struct prima_session *session = prima_session_current();
    uint64_t payload = PRIMA_ASYNC_CANCEL |
        ((session->async_generation & PRIMA_ASYNC_GENERATION_MASK)
         << PRIMA_ASYNC_GENERATION_SHIFT) |
        (prima_async_slot(session) << PRIMA_ASYNC_SLOT_SHIFT) |
        (uint64_t)(uint32_t)ordinal;
    return prima_session_send(payload);
}

/* 1 = ACK arrived (status in *ack), 2 = COMPLETE moved head past `ordinal`,
 * negative = transport error. */
static int prima_async_probe_wait(uint64_t ordinal, int *ack) {
    struct prima_session *session = prima_session_current();
    volatile unsigned char *control = prima_async_probe_control();
    uint64_t slot = prima_async_slot(session);
    for (;;) {
        uint64_t out[5] = {0};
        long raw = prima_receive_meta(out, 0, 0);
        if (raw < 0) return (int)raw;
        for (unsigned i = 0; i < out[2] && i < 2; i++)
            (void)prima_raw(PRIMA_SYS_CAP_DROP, out[i], 0, 0, 0, 0, 0);
        if (out[3] != session->store_cookie) continue;
        uint64_t tag = (uint64_t)raw >> 48;
        if (tag == (PRIMA_ASYNC_ACK >> 48)) {
            *ack = (int)((uint64_t)raw & 0xff);
            return 1;
        }
        if (tag != (PRIMA_ASYNC_COMPLETE >> 48)) continue;
        if ((((uint64_t)raw >> PRIMA_ASYNC_SLOT_SHIFT) &
             PRIMA_ASYNC_SLOT_MASK) != slot) continue;
        if (prima_bulk_get64(control + PRIMA_ASYNC_HEAD_OFFSET) > ordinal)
            return 2;
    }
}

static int prima_async_probe_file(int fd, unsigned char *handle) {
    int file = 0;
    prima_fd_lock_take();
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (entry && entry->kind == PRIMA_FD_FILE) {
        file = 1;
        *handle = entry->object;
    }
    prima_fd_lock_give();
    return file;
}

/* The data path already opened a session for the initial fixture write; this
 * confirms the mapping and the Store-written header (tail/head have advanced,
 * so only the fixed fields are checked). */
int prima_async_probe_open(void) {
    if (prima_async_open() < 0) return 1;
    struct prima_session *session = prima_session_current();
    if (!session || !session->async_cap) return 2;
    const volatile unsigned char *page =
        (const volatile unsigned char *)prima_async_address(session);
    if (page[0] != 'P' || page[1] != 'B' || page[2] != 'Q' || page[3] != '2' ||
        page[4] != 2 || page[5] != PRIMA_ASYNC_CREDITS || page[6] || page[7])
        return 3;
    for (unsigned i = 32; i < PRIMA_ASYNC_HEADER_BYTES; i++)
        if (page[i]) return 3;
    return 0;
}

/* Two published batches with one doorbell: the Store reports both through a
 * single merged COMPLETE and executes them in FIFO order. */
int prima_async_probe_merged(int fd) {
    unsigned char handle = 0;
    if (!prima_async_probe_file(fd, &handle)) return 1;
    struct prima_session *session = prima_session_current();
    uint64_t first = session->async_tail;
    prima_async_probe_batch(first, PRIMA_FILE_OP_WRITE_AT, handle, 0, 5);
    prima_async_probe_put(first, "async", 5);
    prima_async_probe_batch(first + 1, PRIMA_FILE_OP_READ_AT, handle, 0, 5);
    prima_async_probe_set_tail(first + 2);
    if (prima_async_probe_doorbell(first + 2) < 0) return 2;
    int ack = -1;
    if (prima_async_probe_wait(first + 1, &ack) != 2) return 3;
    volatile unsigned char *write_page = prima_async_probe_page(first);
    volatile unsigned char *read_page = prima_async_probe_page(first + 1);
    if (prima_bulk_get(write_page + 56) || prima_bulk_get(write_page + 60) != 5)
        return 4;
    if (prima_bulk_get(read_page + 56) || prima_bulk_get(read_page + 60) != 5)
        return 5;
    for (unsigned i = 0; i < 5; i++)
        if (read_page[PRIMA_BULK_DATA + i] != (unsigned char)"async"[i])
            return 6;
    session->async_tail = first + 2;
    return 0;
}

/* A tail beyond the credit window is refused with EAGAIN and leaves the
 * session usable; the control tail is restored for the next publish. */
int prima_async_probe_full(void) {
    struct prima_session *session = prima_session_current();
    uint64_t tail = session->async_tail;
    prima_async_probe_set_tail(tail + PRIMA_ASYNC_CREDITS + 1);
    if (prima_async_probe_doorbell(tail + PRIMA_ASYNC_CREDITS + 1) < 0)
        return 1;
    int ack = -1;
    if (prima_async_probe_wait(tail, &ack) != 1 || ack != PRIMA_ERRNO_EAGAIN)
        return 2;
    prima_async_probe_set_tail(tail);
    return 0;
}

/* A replayed doorbell publishes nothing; the follow-up CANCEL of the same
 * ordinal must report Missing (EINVAL) instead of completing work twice. */
int prima_async_probe_replay(void) {
    struct prima_session *session = prima_session_current();
    uint64_t tail = session->async_tail;
    if (prima_async_probe_doorbell(tail) < 0) return 1;
    if (prima_async_probe_cancel_at(tail) < 0) return 2;
    int ack = -1;
    if (prima_async_probe_wait(tail, &ack) != 1 || ack != PRIMA_ERRNO_EINVAL)
        return 3;
    return 0;
}

/* A backwards tail is a protocol violation: the Store resets the session with
 * EACCES, and the probe abandons and re-opens so later cases start clean. */
int prima_async_probe_rewind(void) {
    struct prima_session *session = prima_session_current();
    uint64_t tail = session->async_tail;
    if (tail == 0) return 1;
    prima_async_probe_set_tail(tail - 1);
    if (prima_async_probe_doorbell(tail - 1) < 0) return 2;
    int ack = -1;
    if (prima_async_probe_wait(tail, &ack) != 1 || ack != PRIMA_ERRNO_EACCES)
        return 3;
    prima_async_abandon(session);
    session->async_tail = 0;
    if (prima_async_open() < 0) return 4;
    return 0;
}

/* Non-zero reserved header bytes fail validation with EACCES and reset the
 * session; no batch is executed. Re-open for the cancel case. */
int prima_async_probe_reserved(int fd) {
    unsigned char handle = 0;
    if (!prima_async_probe_file(fd, &handle)) return 1;
    struct prima_session *session = prima_session_current();
    uint64_t ordinal = session->async_tail;
    prima_async_probe_control()[32] = 1;
    prima_async_probe_batch(ordinal, PRIMA_FILE_OP_WRITE_AT, handle, 0, 4);
    prima_async_probe_put(ordinal, "BAD!", 4);
    prima_async_probe_set_tail(ordinal + 1);
    if (prima_async_probe_doorbell(ordinal + 1) < 0) return 2;
    int ack = -1;
    if (prima_async_probe_wait(ordinal, &ack) != 1 || ack != PRIMA_ERRNO_EACCES)
        return 3;
    prima_async_probe_control()[32] = 0;
    prima_async_abandon(session);
    session->async_tail = 0;
    if (prima_async_open() < 0) return 4;
    return 0;
}

/* Cancel a published but un-doorbelled batch: the Store drains it as
 * ECANCELED without executing the write, and a second cancel is Missing. */
int prima_async_probe_cancel(int fd) {
    unsigned char handle = 0;
    if (!prima_async_probe_file(fd, &handle)) return 1;
    struct prima_session *session = prima_session_current();
    uint64_t ordinal = session->async_tail;
    prima_async_probe_batch(ordinal, PRIMA_FILE_OP_WRITE_AT, handle, 0, 4);
    prima_async_probe_put(ordinal, "ZZZZ", 4);
    prima_async_probe_set_tail(ordinal + 1);
    if (prima_async_probe_cancel_at(ordinal) < 0) return 2;
    int ack = -1;
    if (prima_async_probe_wait(ordinal, &ack) != 1 || ack != 0) return 3;
    if (prima_async_probe_wait(ordinal, &ack) != 2) return 4;
    if (prima_bulk_get(prima_async_probe_page(ordinal) + 56) != PRIMA_ERRNO_ECANCELED)
        return 5;
    /* The cancelled write must not be visible: merged() left "async". */
    uint64_t read_ordinal = ordinal + 1;
    prima_async_probe_batch(read_ordinal, PRIMA_FILE_OP_READ_AT, handle, 0, 4);
    prima_async_probe_set_tail(read_ordinal + 1);
    if (prima_async_probe_doorbell(read_ordinal + 1) < 0) return 6;
    if (prima_async_probe_wait(read_ordinal, &ack) != 2) return 7;
    volatile unsigned char *page = prima_async_probe_page(read_ordinal);
    if (prima_bulk_get(page + 56) || prima_bulk_get(page + 60) != 4) return 8;
    for (unsigned i = 0; i < 4; i++)
        if (page[PRIMA_BULK_DATA + i] != (unsigned char)"asyn"[i]) return 9;
    session->async_tail = read_ordinal + 1;
    if (prima_async_probe_cancel_at(session->async_tail) < 0) return 10;
    if (prima_async_probe_wait(read_ordinal, &ack) != 1 ||
        ack != PRIMA_ERRNO_EINVAL)
        return 11;
    return 0;
}

int prima_async_probe_close(void) {
    long result = prima_async_close_owned();
    return result < 0 ? 1 : 0;
}

/* ---------- concurrent sessions: fairness and one-sided death ---------- */

#include <pthread.h>

static volatile int prima_async_probe_barrier;
static volatile int prima_async_probe_peer_result;
static volatile int prima_async_probe_death_ready;
static volatile int prima_async_probe_peer_slot;

static void prima_async_probe_spin(volatile int *value, int want) {
    while (__atomic_load_n(value, __ATOMIC_ACQUIRE) != want)
        (void)prima_raw(PRIMA_SYS_YIELD, 0, 0, 0, 0, 0, 0);
}

/* Each Task owns its own direct session; publish two batches without a
 * doorbell so the caller can order the doorbells at a barrier. */
static int prima_async_probe_publish_two(int fd, uint64_t offset,
                                         const char *tag, uint64_t *first) {
    unsigned char handle = 0;
    if (!prima_async_probe_file(fd, &handle)) return 1;
    if (prima_async_open() < 0) return 2;
    struct prima_session *session = prima_session_current();
    *first = session->async_tail;
    for (unsigned i = 0; i < 2; i++) {
        prima_async_probe_batch(*first + i, PRIMA_FILE_OP_WRITE_AT, handle,
                                offset + i, 1);
        prima_async_probe_put(*first + i, tag, 1);
    }
    prima_async_probe_set_tail(*first + 2);
    return 0;
}

static void *prima_async_probe_fair_thread(void *arg) {
    int fd = (int)(long)arg;
    uint64_t first = 0;
    int result = prima_async_probe_publish_two(fd, 0x100, "T", &first);
    if (!result) {
        prima_async_probe_peer_slot =
            (int)prima_async_slot(prima_session_current());
        __atomic_add_fetch(&prima_async_probe_barrier, 1, __ATOMIC_ACQ_REL);
        prima_async_probe_spin(&prima_async_probe_barrier, 2);
        struct prima_session *session = prima_session_current();
        int ack = -1;
        int waited;
        if (prima_async_probe_doorbell(first + 2) < 0) result = 3;
        else if ((waited = prima_async_probe_wait(first + 1, &ack)) != 2)
            result = waited == 1 ? 20 + (ack & 0x7f) : 100;
        else for (unsigned i = 0; i < 2; i++)
            if (prima_bulk_get(prima_async_probe_page(first + i) + 56))
                result = 5;
    }
    prima_async_probe_peer_result = result;
    return 0;
}

/* Both sessions publish two batches, then doorbell together. Each must see
 * its own merged completion; neither may starve the other. */
int prima_async_probe_fair(int fd) {
    __atomic_store_n(&prima_async_probe_barrier, 0, __ATOMIC_RELEASE);
    __atomic_store_n(&prima_async_probe_peer_result, 0, __ATOMIC_RELEASE);
    uint64_t first = 0;
    int result = prima_async_probe_publish_two(fd, 0x200, "M", &first);
    if (result) return 1;
    struct prima_session *session = prima_session_current();
    pthread_t thread;
    if (pthread_create(&thread, 0, prima_async_probe_fair_thread,
                       (void *)(long)fd) != 0)
        return 2;
    __atomic_add_fetch(&prima_async_probe_barrier, 1, __ATOMIC_ACQ_REL);
    prima_async_probe_spin(&prima_async_probe_barrier, 2);
    if (prima_async_probe_peer_slot == (int)prima_async_slot(session)) {
        pthread_join(thread, 0);
        return 60;
    }
    int ack = -1;
    int waited;
    if (prima_async_probe_doorbell(first + 2) < 0) result = 3;
    else if ((waited = prima_async_probe_wait(first + 1, &ack)) != 2)
        result = waited == 1 ? 20 + (ack & 0x7f) : 100;
    else for (unsigned i = 0; i < 2; i++)
        if (prima_bulk_get(prima_async_probe_page(first + i) + 56))
            result = 5;
    session->async_tail = first + 2;
    pthread_join(thread, 0);
    if (result) return result;
    return prima_async_probe_peer_result;
}

static void *prima_async_probe_death_thread(void *arg) {
    int fd = (int)(long)arg;
    uint64_t first = 0;
    int result = prima_async_probe_publish_two(fd, 0x300, "D", &first);
    if (!result) {
        if (prima_async_probe_doorbell(first + 2) < 0) result = 3;
    }
    prima_async_probe_peer_result = result;
    __atomic_store_n(&prima_async_probe_death_ready, 1, __ATOMIC_RELEASE);
    return 0;
}

/* One Task publishes and exits without waiting; the live session must still
 * make progress and complete its own batches. */
int prima_async_probe_death(int fd) {
    __atomic_store_n(&prima_async_probe_death_ready, 0, __ATOMIC_RELEASE);
    __atomic_store_n(&prima_async_probe_peer_result, 0, __ATOMIC_RELEASE);
    pthread_t thread;
    if (pthread_create(&thread, 0, prima_async_probe_death_thread,
                       (void *)(long)fd) != 0)
        return 1;
    uint64_t first = 0;
    int result = prima_async_probe_publish_two(fd, 0x400, "L", &first);
    if (result) {
        pthread_join(thread, 0);
        return 2;
    }
    struct prima_session *session = prima_session_current();
    int ack = -1;
    int waited;
    if (prima_async_probe_doorbell(first + 2) < 0) result = 3;
    else if ((waited = prima_async_probe_wait(first + 1, &ack)) != 2)
        result = waited == 1 ? 20 + (ack & 0x7f) : 100;
    else for (unsigned i = 0; i < 2; i++)
        if (prima_bulk_get(prima_async_probe_page(first + i) + 56))
            result = 5;
    pthread_join(thread, 0);
    if (result) return result;
    return prima_async_probe_peer_result;
}

#endif
