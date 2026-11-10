/* Internal implementation, included after the authenticated session helpers.
 * One page per Task, one outstanding batch; other Tasks use separate pages. */
static uint64_t prima_bulk_address(const struct prima_session *session) {
    return PRIMA_BULK_VA + (uint32_t)session->cookie * UINT64_C(4096);
}

static long prima_bulk_open(void) {
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

/* Do not inherit buffer mappings through fork/exec or leave them at thread exit. */
static long prima_bulk_close_owned(void) {
    struct prima_session *session = prima_session_current();
    if (!session || !session->bulk_cap) return 0;
    long unmapped = prima_raw(PRIMA_SYS_MEMSEG_UNMAP, session->self_cap,
                              prima_bulk_address(session), 1, 0, 0, 0);
    if (unmapped < 0) return unmapped;
    (void)prima_raw(PRIMA_SYS_CAP_DROP, session->bulk_cap, 0, 0, 0, 0, 0);
    session->bulk_cap = 0;
    long sent = prima_session_send(PRIMA_BULK_CLOSE);
    if (sent < 0) return sent;
    long reply = prima_session_receive(PRIMA_BULK_COMPLETE);
    return reply < 0 ? reply : -(long)(((uint64_t)reply >> 32) & 0xff);
}

static long prima_bulk_close(void) {
    struct prima_session *session = prima_session_current();
    if (!session || !session->bulk_cap) return 0;
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

/* Mask signals during ownership: a handler may issue I/O on this same Task. */
static long prima_bulk_transfer(int handle, void *buffer, unsigned long length,
                                uint64_t offset, int writing, int positional) {
    uint64_t mask, blocked = ~PRIMA_UNMASKABLE;
    long result = prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &blocked, &mask, 8);
    if (result < 0) return result;
    result = prima_bulk_open();
    if (result < 0) goto restore;
    struct prima_session *session = prima_session_current();
    volatile unsigned char *page = (void *)prima_bulk_address(session);
    unsigned char *bytes = buffer;
    unsigned long done = 0;
    do {
        if (session->bulk_next > UINT32_MAX) { result = done ? (long)done : -PRIMA_ERRNO_EAGAIN; break; }
        unsigned long count = length - done;
        if (count > PRIMA_BULK_CAPACITY) count = PRIMA_BULK_CAPACITY;
        for (unsigned i = 0; i < 4096; i++) page[i] = 0;
        page[0]='P'; page[1]='B'; page[2]='I'; page[3]='O'; page[4]=1;
        prima_bulk_put(page + 8, session->bulk_next, 8);
        prima_bulk_put(page + 16, 1, 4);
        page[32] = positional ? (writing ? PRIMA_FILE_OP_WRITE_AT : PRIMA_FILE_OP_READ_AT)
                              : (writing ? PRIMA_FILE_OP_WRITE : PRIMA_FILE_OP_READ);
        page[33] = (unsigned char)handle;
        prima_bulk_put(page + 40, positional ? offset + done : 0, 8);
        prima_bulk_put(page + 48, count, 4);
        prima_bulk_put(page + 52, PRIMA_BULK_DATA, 4);
        if (writing) for (unsigned long i = 0; i < count; i++) page[PRIMA_BULK_DATA + i] = bytes[done + i];
        __asm__ volatile("" ::: "memory");
        long sent = prima_session_send(PRIMA_BULK_SUBMIT | session->bulk_next);
        if (sent < 0) { result = done ? (long)done : sent; break; }
        long reply = prima_session_receive(PRIMA_BULK_COMPLETE);
        if (reply < 0) { result = done ? (long)done : reply; break; }
        unsigned status = ((uint64_t)reply >> 32) & 0xff;
        if ((uint32_t)reply != session->bulk_next || status) {
            result = done ? (long)done : status ? -(long)status : -PRIMA_ERRNO_EINVAL;
            break;
        }
        session->bulk_next++;
        __asm__ volatile("" ::: "memory");
        status = prima_bulk_get(page + 56);
        unsigned received = prima_bulk_get(page + 60);
        if (status || received > count) {
            result = done ? (long)done : status ? -(long)status : -PRIMA_ERRNO_EINVAL;
            break;
        }
        if (!writing) for (unsigned i = 0; i < received; i++) bytes[done + i] = page[PRIMA_BULK_DATA + i];
        done += received;
        result = (long)done;
        if (received < count) break;
    } while (done < length);
restore:
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return result;
}
