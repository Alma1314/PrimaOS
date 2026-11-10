/* Compiled only into the V4 acceptance image. No extra production entry point. */
static void prima_bulk_probe_header(volatile unsigned char *page, uint64_t sequence) {
    for (unsigned i = 0; i < 4096; i++) page[i] = 0;
    page[0]='P'; page[1]='B'; page[2]='I'; page[3]='O'; page[4]=1;
    prima_bulk_put(page + 8, sequence, 8);
    prima_bulk_put(page + 16, 2, 4);
}

static long prima_bulk_probe_submit(uint64_t sequence) {
    long sent = prima_session_send(PRIMA_BULK_SUBMIT | sequence);
    return sent < 0 ? sent : prima_session_receive(PRIMA_BULK_COMPLETE);
}

int prima_bulk_probe(int fd) {
    struct prima_fd_entry *entry = prima_fd_entry(fd);
    if (!entry || entry->kind != PRIMA_FD_FILE) return 1;
    uint64_t blocked = ~PRIMA_UNMASKABLE, mask;
    if (prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &blocked, &mask, 8) < 0) return 2;
    int failure = 0;
    if (prima_bulk_open() < 0) { failure = 3; goto done; }
    struct prima_session *session = prima_session_current();
    volatile unsigned char *page = (void *)prima_bulk_address(session);
    if (prima_raw(PRIMA_SYS_MEMSEG_DESTROY, session->bulk_cap, 0, 0, 0, 0, 0) != -PRIMA_ERRNO_EACCES) {
        failure = 4; goto done;
    }
    uint64_t sequence = session->bulk_next;
    prima_bulk_probe_header(page, sequence);
    /* A valid first WRITE_AT followed by an invalid second descriptor must
     * reject the entire batch before the first byte can change. */
    page[32]=PRIMA_FILE_OP_WRITE_AT; page[33]=entry->object;
    prima_bulk_put(page+48, 1, 4); prima_bulk_put(page+52, PRIMA_BULK_DATA, 4);
    page[PRIMA_BULK_DATA]='Z';
    page[64]=PRIMA_FILE_OP_READ_AT; page[65]=entry->object; page[66]=1;
    prima_bulk_put(page+80, 1, 4); prima_bulk_put(page+84, PRIMA_BULK_DATA+1, 4);
    long reply = prima_bulk_probe_submit(sequence);
    if (reply < 0 || (uint32_t)reply != sequence ||
        (((uint64_t)reply >> 32) & 0xff) != PRIMA_ERRNO_EINVAL) { failure = 5; goto done; }
    char byte = 0;
    if (prima_fd_at(fd, &byte, 1, 0, 0) != 1 || byte != 'a') { failure = 6; goto done; }

    sequence = session->bulk_next;
    prima_bulk_probe_header(page, sequence);
    page[32]=PRIMA_FILE_OP_READ_AT; page[33]=entry->object;
    prima_bulk_put(page+48, 6, 4); prima_bulk_put(page+52, PRIMA_BULK_DATA, 4);
    page[64]=PRIMA_FILE_OP_WRITE_AT; page[65]=entry->object;
    prima_bulk_put(page+72, 5, 8);
    prima_bulk_put(page+80, 1, 4); prima_bulk_put(page+84, PRIMA_BULK_DATA+6, 4);
    page[PRIMA_BULK_DATA+6]='f';
    reply = prima_bulk_probe_submit(sequence);
    if (reply < 0 || (uint32_t)reply != sequence || ((uint64_t)reply >> 32 & 0xff) ||
        prima_bulk_get(page+56) || prima_bulk_get(page+60) != 6 ||
        prima_bulk_get(page+88) || prima_bulk_get(page+92) != 1) { failure = 7; goto done; }
    for (unsigned i=0; i<6; i++) if (page[PRIMA_BULK_DATA+i] != (unsigned char)('a'+i)) { failure=8; goto done; }
    session->bulk_next++;
    /* Reset result slots and alter the payload: accepting a replay would now
     * have a visible side effect, even though the caller reuses its old ID. */
    prima_bulk_put(page+56, 0, 8); prima_bulk_put(page+88, 0, 8);
    page[PRIMA_BULK_DATA+6]='X';
    reply = prima_bulk_probe_submit(sequence);
    if (reply < 0 || (uint32_t)reply != sequence ||
        (((uint64_t)reply >> 32) & 0xff) != PRIMA_ERRNO_EINVAL) { failure = 9; goto done; }
    if (prima_fd_at(fd, &byte, 1, 5, 0) != 1 || byte != 'f') { failure=10; goto done; }
    sequence = session->bulk_next;
    if (prima_bulk_close() < 0 || prima_bulk_open() < 0 || session->bulk_next != sequence) { failure=11; goto done; }
    unsigned char input[512], output[512];
    for (unsigned i=0; i<sizeof input; i++) input[i]=(unsigned char)(i ^ (i >> 4));
    if (prima_fd_at(fd, input, sizeof input, 6, 1) != sizeof input ||
        prima_fd_at(fd, output, sizeof output, 6, 0) != sizeof output) { failure=12; goto done; }
    for (unsigned i=0; i<sizeof input; i++) if (input[i] != output[i]) { failure=13; goto done; }
    if (prima_fd_resize(fd, 6) < 0) { failure=14; goto done; }
done:
    (void)prima_rt_sigprocmask(PRIMA_SIG_SETMASK, &mask, 0, 8);
    return failure;
}
