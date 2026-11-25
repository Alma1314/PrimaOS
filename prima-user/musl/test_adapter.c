/* Host-only checks of the actual adapter. NULL devices require no native
 * syscall, so these cases must be safe to execute on a Linux host. */
#include "../c/prima_syscall.c"
#include <assert.h>

int main(void) {
    /* All these reject locally or use the null device, without a native trap. */
    char positioned[2] = {'X', 'Y'};
    unsigned char wire[10] = {0xa5};
    wire[9] = 0x5a;
    prima_bulk_put(wire + 1, UINT64_C(0x0102030405060708), 8);
    for (unsigned i = 1; i <= 8; i++) assert(wire[i] == 9 - i);
    assert(prima_bulk_get(wire + 1) == UINT32_C(0x05060708));
    assert(wire[0] == 0xa5 && wire[9] == 0x5a);
    assert(PRIMA_BULK_DATA == 32 + 8 * 32);
    assert(PRIMA_BULK_DATA + PRIMA_BULK_CAPACITY == 4096);

    /* P3 v2 control-page validation and slot addressing are pure arithmetic. */
    unsigned char control[PRIMA_ASYNC_HEADER_BYTES] = {0};
    control[0] = 'P'; control[1] = 'B'; control[2] = 'Q'; control[3] = '2';
    control[4] = 2; control[5] = PRIMA_ASYNC_CREDITS;
    prima_bulk_put(control + PRIMA_ASYNC_GENERATION_OFFSET, 9, 8);
    uint64_t generation = 0;
    assert(prima_async_header_ok(control, &generation) && generation == 9);
    control[4] = 1; assert(!prima_async_header_ok(control, 0)); control[4] = 2;
    control[6] = 1; assert(!prima_async_header_ok(control, 0)); control[6] = 0;
    prima_bulk_put(control + PRIMA_ASYNC_TAIL_OFFSET, 1, 8);
    assert(!prima_async_header_ok(control, 0));
    struct prima_session session = {0};
    session.cookie = UINT64_C(17) | (UINT64_C(1) << 32);
    assert(prima_async_address(&session) ==
           PRIMA_ASYNC_VA + UINT64_C(17) * PRIMA_ASYNC_PAGES * 4096);
    assert(prima_fd_at(-1, positioned, 1, 0, 0) == -PRIMA_ERRNO_EBADF);
    assert(prima_fd_resize(-1, 0) == -PRIMA_ERRNO_EBADF);
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_FILE, 3, 0, 0};
    assert(prima_fd_resize(3, -1) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_at(3, positioned, 1, -1, 0) == -PRIMA_ERRNO_EINVAL);
    assert(prima_file_inflight[0] == 0); /* validation error releases the ref */
    assert(prima_fd_at(3, positioned, ~0UL, 0, 1) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_at(3, 0, 1, 0, 0) == -PRIMA_ERRNO_EFAULT);
    prima_fds[3].kind = PRIMA_FD_DIR;
    assert(prima_fd_resize(3, 0) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_at(3, positioned, 1, 0, 0) == -PRIMA_ERRNO_EISDIR);
    prima_fds[3].kind = PRIMA_FD_TTY;
    assert(prima_fd_at(3, positioned, 1, 0, 1) == -PRIMA_ERRNO_ESPIPE);
    prima_fds[3].kind = PRIMA_FD_NULL;
    assert(prima_fd_at(3, positioned, 2, 0, 1) == 2);
    assert(prima_fd_at(3, positioned, 2, 0, 0) == 0);
    assert(prima_fd_resize(3, 0) == -PRIMA_ERRNO_EINVAL);
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_SOCKET, 0, 0, 0};
    assert(prima_net_sendto(3, 0, 0, 1, 0, 0) == -PRIMA_ERRNO_EINVAL);
    assert(prima_net_sendto(3, 0, 0, PRIMA_MSG_NOSIGNAL | 1, 0, 0) == -PRIMA_ERRNO_EINVAL);
    /* Accepted flag advances to datagram-size validation, without a native call. */
    assert(prima_net_sendto(3, 0, 65, PRIMA_MSG_NOSIGNAL, 0, 0) == -PRIMA_ERRNO_MSGSIZE);
    assert(!(prima_upcall_flags(33, PRIMA_SA_RESTART) & 4));
    assert(prima_upcall_flags(32, PRIMA_SA_RESTART) & 4);
    assert(prima_upcall_flags(10, PRIMA_SA_RESTART) & 4);
    unsigned char elf[208] = {127,'E','L','F',2,1,1};
    elf[18]=62; elf[32]=64; elf[54]=56; elf[56]=1;
    elf[64]=4; elf[72]=120; elf[96]=44;
    const uint32_t note[11] = PRIMA_CONTRACT_WORDS;
    prima_copy(elf+120,note,sizeof note);
    assert(prima_contract_accepts(elf,sizeof elf));
    assert(!prima_contract_accepts(elf,163));
    for (unsigned at=140; at<=160; at+=4) {
        unsigned char saved=elf[at]; elf[at]=9;
        assert(!prima_contract_accepts(elf,sizeof elf)); elf[at]=saved;
    }
    elf[64]=1; assert(!prima_contract_accepts(elf,sizeof elf)); elf[64]=4;
    prima_copy(elf+164,elf+120,44); elf[96]=88;
    assert(!prima_contract_accepts(elf,sizeof elf)); elf[96]=44;
    for (unsigned i=0; i<4; i++) elf[120+i]=255;
    assert(!prima_contract_accepts(elf,sizeof elf));

    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_NULL, 0, 0, 1};
    assert(prima_fd_dup(3) == 4);
    assert(prima_fd_fcntl(4, PRIMA_F_GETFD, 0) == 0);
    assert(prima_fd_fcntl(3, PRIMA_F_GETFD, 0) == 1);
    assert(prima_fd_dup2(3, 3) == 3 && prima_fds[3].cloexec == 1);
    assert(prima_fd_dup2(3, 4) == 4 && prima_fds[4].cloexec == 0);
    assert(prima_fd_dup3(3, 4, PRIMA_O_CLOEXEC) == 4 && prima_fds[4].cloexec == 1);
    assert(prima_fd_fcntl(3, PRIMA_F_SETFD, 0) == 0 && prima_fds[3].cloexec == 0);
    assert(prima_fd_fcntl(3, PRIMA_F_DUPFD, 8) == 8);
    assert(prima_fd_fcntl(3, PRIMA_F_DUPFD_CLOEXEC, 8) == 9 && prima_fds[9].cloexec);
    assert(prima_fd_fcntl(3, PRIMA_F_DUPFD, -1) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_fcntl(3, PRIMA_F_DUPFD, PRIMA_LOCAL_FD_COUNT) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_fcntl(-1, PRIMA_F_GETFD, 0) == -PRIMA_ERRNO_EBADF);
    assert(prima_fd_fcntl(3, 123456, 0) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_fcntl(3, PRIMA_F_GETFL, 0) == PRIMA_O_RDWR);
    assert(prima_fd_fcntl(3, PRIMA_F_SETFL, PRIMA_O_RDWR) == 0);
    assert(prima_fd_fcntl(3, PRIMA_F_SETFL, PRIMA_O_APPEND) == -PRIMA_ERRNO_EINVAL);
    while (prima_fd_fcntl(3, PRIMA_F_DUPFD, 0) >= 0) {}
    assert(prima_fd_fcntl(3, PRIMA_F_DUPFD, 0) == -PRIMA_ERRNO_EMFILE);

    char byte = 'a';
    struct iovec iov[3] = {{0, 0}, {&byte, 1}, {0, 1}};
    assert(prima_fd_iov(3, 0, 0, 0) == 0);
    assert(prima_fd_iov(-1, 0, 0, 0) == -PRIMA_ERRNO_EBADF);
    assert(prima_fd_iov(3, 0, 1, 0) == -PRIMA_ERRNO_EFAULT);
    assert(prima_fd_iov(3, iov, -1, 0) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_iov(3, iov, 1025, 0) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_iov(3, iov, 3, 1) == 1); /* progress before EFAULT */
    assert(prima_fd_iov(3, iov, 3, 0) == 0); /* EOF before next vector */
    iov[0].iov_len = (~0UL) >> 1;
    iov[1].iov_len = 1;
    assert(prima_fd_iov(3, iov, 2, 1) == -PRIMA_ERRNO_EINVAL);

    /* P3: closing a descriptor with an in-flight batch defers the wire CLOSE
     * to the last unref (no IPC here; only the accounting is exercised). */
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_FILE, PRIMA_FILE_FIRST_FD, 0, 0};
    prima_file_refs[0] = 1;
    prima_file_inflight[0] = 0;
    prima_file_retire[0] = 0;
    unsigned char kind = 0, object = 0;
    assert(prima_fd_snapshot(3, &kind, &object));
    assert(kind == PRIMA_FD_FILE && object == PRIMA_FILE_FIRST_FD);
    assert(prima_file_inflight[0] == 1);
    struct prima_wire_close close = {0};
    prima_fd_lock_take();
    prima_fd_drop_locked(3, &close);
    prima_fd_lock_give();
    assert(close.kind == PRIMA_WIRE_NONE); /* deferred: batch still in flight */
    assert(prima_file_refs[0] == 0 && prima_file_retire[0] == 1);
    prima_file_inflight[0] = 0; /* the deferred send needs IPC; not on host */
    prima_file_retire[0] = 0;

    /* Directories now take the same in-flight reference as files, so getdents
     * and lseek cannot race a wire-handle reuse. */
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_DIR, PRIMA_FILE_FIRST_FD, 0, 0};
    prima_file_refs[0] = 1;
    prima_file_inflight[0] = 0;
    prima_file_retire[0] = 0;
    assert(prima_fd_snapshot(3, &kind, &object) && kind == PRIMA_FD_DIR);
    assert(prima_file_inflight[0] == 1);
    prima_file_unref(object);
    assert(prima_file_inflight[0] == 0 && prima_file_retire[0] == 0);

    /* Socket operations use their own in-flight counter and defer CLOSE more. */
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_SOCKET, 0, 0, 0};
    prima_net_refs[0] = 1;
    prima_net_inflight[0] = 0;
    prima_net_retire[0] = 0;
    unsigned char net_handle = 0;
    assert(prima_net_snapshot(3, &net_handle) && net_handle == 0);
    assert(prima_net_inflight[0] == 1);
    struct prima_wire_close net_close = {0};
    prima_fd_lock_take();
    prima_fd_drop_locked(3, &net_close);
    prima_fd_lock_give();
    assert(net_close.kind == PRIMA_WIRE_NONE);
    assert(prima_net_refs[0] == 0 && prima_net_retire[0] == 1);
    prima_net_inflight[0] = 0; /* the deferred send needs IPC; not on host */
    prima_net_retire[0] = 0;

    /* dup2/dup3 report the replaced target's CLOSE for an unlocked send. */
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_FILE, PRIMA_FILE_FIRST_FD, 0, 0};
    prima_fds[4] = (struct prima_fd_entry){PRIMA_FD_FILE, PRIMA_FILE_FIRST_FD + 1, 0, 0};
    prima_file_refs[0] = 1;
    prima_file_refs[1] = 1;
    struct prima_wire_close dup_close = {0};
    prima_fd_lock_take();
    assert(prima_fd_dup_to_locked(3, 4, &dup_close) == 4);
    prima_fd_lock_give();
    assert(dup_close.kind == PRIMA_WIRE_FILE &&
           dup_close.handle == PRIMA_FILE_FIRST_FD + 1);
    assert(prima_file_refs[0] == 2 && prima_file_refs[1] == 0);

    /* Path resolution copies the path so IPC runs without the table lock. */
    char send[PRIMA_VFS_PATH_MAX + 1];
    unsigned char start = 0;
    unsigned long send_len = 0;
    assert(prima_path_target(PRIMA_AT_FDCWD, "/abs", &start, send, &send_len) == 0);
    assert(start == 0 && send_len == 4 && send[0] == '/' && send[4] == 0);
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_DIR, 0, 7, 0};
    assert(prima_path_target(3, "rel", &start, send, &send_len) == 0);
    assert(start == 7 && send_len == 3 && send[2] == 'l' && send[3] == 0);
    return 0;
}
