/* Internal implementation, included after the authenticated session helpers.
 * One page per Task, one outstanding batch; other Tasks use separate pages. */
static uint64_t prima_bulk_address(const struct prima_session *session) {
    return PRIMA_BULK_VA + (uint32_t)session->cookie * UINT64_C(4096);
}

static long prima_async_close_owned(void);
static __attribute__((unused)) long prima_bulk_open(void) {
    long ready = prima_session_register(0);
    if (ready < 0) return ready;
    struct prima_session *session = prima_session_current();
    if (session->bulk_cap) return 0;
    long sent = prima_session_send(PRIMA_BULK_OPEN);
    if (sent < 0) return sent;
    for (;;) {
        uint64_t out[5] = {0};
        long raw = prima_receive_meta(out, 0, 0);
        if (raw < 0) return raw;
        if (out[3] != session->store_cookie ||
            ((uint64_t)raw >> 48) != (PRIMA_BULK_OPEN_REPLY >> 48)) {
            for (unsigned i = 0; i < out[2] && i < 2; i++)
                (void)prima_raw(PRIMA_SYS_CAP_DROP, out[i], 0, 0, 0, 0, 0);
            continue;
        }
        unsigned status = ((uint64_t)raw >> 32) & 0xff;
        if (status || out[2] != 1 || !(uint32_t)raw) {
            for (unsigned i = 0; i < out[2] && i < 2; i++)
                (void)prima_raw(PRIMA_SYS_CAP_DROP, out[i], 0, 0, 0, 0, 0);
            return status ? -(long)status : -PRIMA_ERRNO_EINVAL;
        }
        long mapped = prima_raw(PRIMA_SYS_MEMSEG_MAP, out[0], session->self_cap, 0,
                                prima_bulk_address(session), 1, PRIMA_ACCESS_R | PRIMA_ACCESS_W);
        if (mapped < 0) {
            (void)prima_raw(PRIMA_SYS_CAP_DROP, out[0], 0, 0, 0, 0, 0);
            (void)prima_session_send(PRIMA_BULK_CLOSE);
            (void)prima_session_receive(PRIMA_BULK_COMPLETE);
            return mapped;
        }
        session->bulk_cap = out[0]; session->bulk_next = (uint32_t)raw;
        return 0;
    }
}

/* Do not inherit buffer mappings through fork/exec or leave them at thread exit.
 * Both the v1 probe page and the v2 session must go: the child would otherwise
 * inherit a mapping to the parent's control page. */
static long prima_bulk_close_owned(void) {
    struct prima_session *session = prima_session_current();
    if (!session) return 0;
    if (session->bulk_cap) {
        long unmapped = prima_raw(PRIMA_SYS_MEMSEG_UNMAP, session->self_cap,
                                  prima_bulk_address(session), 1, 0, 0, 0);
        if (unmapped < 0) return unmapped;
        (void)prima_raw(PRIMA_SYS_CAP_DROP, session->bulk_cap, 0, 0, 0, 0, 0);
        session->bulk_cap = 0;
        long sent = prima_session_send(PRIMA_BULK_CLOSE);
        if (sent < 0) return sent;
        long reply = prima_session_receive(PRIMA_BULK_COMPLETE);
        if (reply < 0) return reply;
        long status = ((uint64_t)reply >> 32) & 0xff;
        if (status) return -status;
    }
    return prima_async_close_owned();
}

static long prima_bulk_close(void) {
    struct prima_session *session = prima_session_current();
    /* A v2-only client has no v1 page (bulk_cap == 0) but still owns a
     * five-page session the child must not inherit. */
    if (!session || (!session->bulk_cap && !session->async_cap)) return 0;
    uint64_t mask, blocked = ~PRIMA_UNMASKABLE;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &blocked, &mask, 8);
    if (result < 0) return result;
    result = prima_bulk_close_owned();
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return result;
}

static void prima_bulk_put(volatile unsigned char *at, uint64_t value, unsigned bytes) {
    for (unsigned i = 0; i < bytes; i++) at[i] = (unsigned char)(value >> (8 * i));
}
static uint32_t prima_bulk_get(const volatile unsigned char *at) {
    uint32_t value = 0;
    for (unsigned i = 0; i < 4; i++) value |= (uint32_t)at[i] << (8 * i);
    return value;
}
static uint64_t prima_bulk_get64(const volatile unsigned char *at) {
    uint64_t value = 0;
    for (unsigned i = 0; i < 8; i++) value |= (uint64_t)at[i] << (8 * i);
    return value;
}

/* ---------- P3 v2 async session (direct client) ---------- */

/* The slot in bits 32..39 of every v2 tag is this Task's registered slot. */
static uint64_t prima_async_slot(const struct prima_session *session) {
    return (uint64_t)(uint32_t)session->cookie & PRIMA_ASYNC_SLOT_MASK;
}
static uint64_t prima_async_address(const struct prima_session *session) {
    return PRIMA_ASYNC_VA + prima_async_slot(session) *
           PRIMA_ASYNC_PAGES * UINT64_C(4096);
}

/* Pure control-page check, also exercised by the host adapter test. */
static int prima_async_header_ok(const volatile unsigned char *page,
                                 uint64_t *generation) {
    if (page[0] != 'P' || page[1] != 'B' || page[2] != 'Q' || page[3] != '2' ||
        page[4] != 2 || page[5] != PRIMA_ASYNC_CREDITS || page[6] || page[7])
        return 0;
    for (unsigned i = 32; i < PRIMA_ASYNC_HEADER_BYTES; i++)
        if (page[i]) return 0;
    if (prima_bulk_get64(page + PRIMA_ASYNC_TAIL_OFFSET) != 0) return 0;
    if (prima_bulk_get64(page + PRIMA_ASYNC_HEAD_OFFSET) != 0) return 0;
    if (generation) *generation = prima_bulk_get64(page + PRIMA_ASYNC_GENERATION_OFFSET);
    return 1;
}

static long prima_async_open(void) {
    long ready = prima_session_register(0);
    if (ready < 0) return ready;
    struct prima_session *session = prima_session_current();
    if (!session) return -PRIMA_ERRNO_EAGAIN;
    if (session->async_cap) return 0;
    uint64_t slot = prima_async_slot(session);
    long sent = prima_session_send(PRIMA_ASYNC_OPEN | (slot << PRIMA_ASYNC_SLOT_SHIFT));
    if (sent < 0) return sent;
    for (;;) {
        uint64_t out[5] = {0};
        long raw = prima_receive_meta(out, 0, 0);
        if (raw < 0) return raw;
        if (out[3] != session->store_cookie ||
            ((uint64_t)raw >> 48) != (PRIMA_ASYNC_ACK >> 48)) {
            for (unsigned i = 0; i < out[2] && i < 2; i++)
                (void)prima_raw(PRIMA_SYS_CAP_DROP, out[i], 0, 0, 0, 0, 0);
            continue;
        }
        unsigned status = (uint64_t)raw & 0xff;
        unsigned ack_slot = ((uint64_t)raw >> 8) & 0xff;
        if (status || ack_slot != (unsigned)slot || out[2] != 1) {
            for (unsigned i = 0; i < out[2] && i < 2; i++)
                (void)prima_raw(PRIMA_SYS_CAP_DROP, out[i], 0, 0, 0, 0, 0);
            return status ? -(long)status : -PRIMA_ERRNO_EINVAL;
        }
        uint64_t address = prima_async_address(session);
        long mapped = prima_raw(PRIMA_SYS_MEMSEG_MAP, out[0], session->self_cap, 0,
                                address, PRIMA_ASYNC_PAGES,
                                PRIMA_ACCESS_R | PRIMA_ACCESS_W);
        uint64_t generation = 0;
        if (mapped < 0 ||
            !prima_async_header_ok((const volatile unsigned char *)address, &generation)) {
            if (mapped >= 0)
                (void)prima_raw(PRIMA_SYS_MEMSEG_UNMAP, session->self_cap, address,
                                PRIMA_ASYNC_PAGES, 0, 0, 0);
            (void)prima_raw(PRIMA_SYS_CAP_DROP, out[0], 0, 0, 0, 0, 0);
            return mapped < 0 ? mapped : -PRIMA_ERRNO_EINVAL;
        }
        session->async_cap = out[0];
        session->async_generation = generation;
        session->async_tail = 0;
        return 0;
    }
}

static long prima_async_close_owned(void) {
    struct prima_session *session = prima_session_current();
    if (!session || !session->async_cap) return 0;
    uint64_t slot = prima_async_slot(session);
    uint64_t address = prima_async_address(session);
    long unmapped = prima_raw(PRIMA_SYS_MEMSEG_UNMAP, session->self_cap, address,
                              PRIMA_ASYNC_PAGES, 0, 0, 0);
    if (unmapped < 0) return unmapped;
    (void)prima_raw(PRIMA_SYS_CAP_DROP, session->async_cap, 0, 0, 0, 0, 0);
    session->async_cap = 0;
    session->async_generation = 0;
    session->async_tail = 0;
    long sent = prima_session_send(PRIMA_ASYNC_CLOSE | (slot << PRIMA_ASYNC_SLOT_SHIFT));
    if (sent < 0) return sent;
    long reply = prima_session_receive(PRIMA_ASYNC_ACK);
    if (reply < 0) return reply;
    long status = (uint64_t)reply & 0xff;
    return status ? -status : 0;
}

/* One v2 batch: publish into the next credit page, raise the control `tail`,
 * doorbell with the generation fence, then wait for the merged COMPLETE whose
 * control `head` passes this ordinal. The Store owns `head`; re-reading it
 * after any message is what the contract requires. */
static long prima_async_transfer_batch(struct prima_session *session, int handle,
                                       unsigned char *bytes, unsigned long count,
                                       unsigned long done, uint64_t offset,
                                       int writing, int positional,
                                       long *result) {
    uint64_t address = prima_async_address(session);
    uint64_t slot = prima_async_slot(session);
    uint64_t ordinal = session->async_tail;
    volatile unsigned char *control = (volatile unsigned char *)address;
    volatile unsigned char *page =
        (volatile unsigned char *)(address +
            (1 + ordinal % PRIMA_ASYNC_CREDITS) * UINT64_C(4096));
    for (unsigned i = 0; i < 4096; i++) page[i] = 0;
    page[0]='P'; page[1]='B'; page[2]='I'; page[3]='O'; page[4]=1;
    prima_bulk_put(page + 8, ordinal + 1, 8);
    prima_bulk_put(page + 16, 1, 4);
    page[32] = positional ? (writing ? PRIMA_FILE_OP_WRITE_AT : PRIMA_FILE_OP_READ_AT)
                          : (writing ? PRIMA_FILE_OP_WRITE : PRIMA_FILE_OP_READ);
    page[33] = (unsigned char)handle;
    prima_bulk_put(page + 40, positional ? offset + done : 0, 8);
    prima_bulk_put(page + 48, count, 4);
    prima_bulk_put(page + 52, PRIMA_BULK_DATA, 4);
    if (writing)
        for (unsigned long i = 0; i < count; i++)
            page[PRIMA_BULK_DATA + i] = bytes[done + i];
    __asm__ volatile("" ::: "memory");
    prima_bulk_put(control + PRIMA_ASYNC_TAIL_OFFSET, ordinal + 1, 8);
    __asm__ volatile("" ::: "memory");
    uint64_t doorbell = PRIMA_ASYNC_DOORBELL
        | ((session->async_generation & PRIMA_ASYNC_GENERATION_MASK)
           << PRIMA_ASYNC_GENERATION_SHIFT)
        | (slot << PRIMA_ASYNC_SLOT_SHIFT)
        | (uint64_t)(uint32_t)(ordinal + 1);
    long sent = prima_session_send(doorbell);
    if (sent < 0) { *result = done ? (long)done : sent; return -1; }
    for (;;) {
        uint64_t out[5] = {0};
        long raw = prima_receive_meta(out, 0, 0);
        if (raw < 0) { *result = done ? (long)done : raw; return -1; }
        for (unsigned i = 0; i < out[2] && i < 2; i++)
            (void)prima_raw(PRIMA_SYS_CAP_DROP, out[i], 0, 0, 0, 0, 0);
        if (out[3] != session->store_cookie) continue;
        uint64_t tag = (uint64_t)raw >> 48;
        if (tag == (PRIMA_ASYNC_ACK >> 48)) {
            long status = (uint64_t)raw & 0xff;
            *result = done ? (long)done : status ? -status : -PRIMA_ERRNO_EINVAL;
            return -1;
        }
        if (tag != (PRIMA_ASYNC_COMPLETE >> 48)) continue;
        if ((((uint64_t)raw >> PRIMA_ASYNC_SLOT_SHIFT) & PRIMA_ASYNC_SLOT_MASK) != slot)
            continue;
        __asm__ volatile("" ::: "memory");
        if (prima_bulk_get64(control + PRIMA_ASYNC_HEAD_OFFSET) > ordinal) break;
    }
    uint32_t status = prima_bulk_get(page + 56);
    unsigned received = prima_bulk_get(page + 60);
    /* The Store consumed this ordinal even when the operation failed, so the
     * producer must advance before reporting the error. */
    session->async_tail = ordinal + 1;
    if (status || received > count) {
        *result = done ? (long)done : status ? -(long)status : -PRIMA_ERRNO_EINVAL;
        return -1;
    }
    if (!writing)
        for (unsigned i = 0; i < received; i++)
            bytes[done + i] = page[PRIMA_BULK_DATA + i];
    *result = (long)(done + received);
    /* A short transfer ends the loop; a full page continues. */
    return received == count ? 0 : -1;
}

/* Mask signals during ownership: a handler may issue I/O on this same Task. */
static long prima_bulk_transfer(int handle, void *buffer, unsigned long length,
                                uint64_t offset, int writing, int positional) {
    uint64_t mask, blocked = ~PRIMA_UNMASKABLE;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &blocked, &mask, 8);
    if (result < 0) return result;
    result = prima_async_open();
    if (result < 0) goto restore;
    struct prima_session *session = prima_session_current();
    unsigned char *bytes = buffer;
    unsigned long done = 0;
    do {
        unsigned long count = length - done;
        if (count > PRIMA_BULK_CAPACITY) count = PRIMA_BULK_CAPACITY;
        if (prima_async_transfer_batch(session, handle, bytes, count, done, offset,
                                       writing, positional, &result) < 0)
            break;
        done = (unsigned long)result;
    } while (done < length);
restore:
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return result;
}
