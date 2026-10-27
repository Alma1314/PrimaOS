/* Host-only checks of the actual adapter. NULL devices require no native
 * syscall, so these cases must be safe to execute on a Linux host. */
#include "../c/prima_syscall.c"
#include <assert.h>

int main(void) {
    /* All these reject locally or use the null device, without a native trap. */
    char positioned[2] = {'X', 'Y'};
    assert(prima_file_at_arg(1, 2, (const unsigned char *)positioned) == UINT64_C(0x59582001));
    assert(prima_file_at_arg(UINT64_MAX, 0, 0) == PRIMA_FILE_AT_OFFSET_MASK);
    assert(prima_fd_at(-1, positioned, 1, 0, 0) == -PRIMA_ERRNO_EBADF);
    assert(prima_fd_resize(-1, 0) == -PRIMA_ERRNO_EBADF);
    prima_fds[3] = (struct prima_fd_entry){PRIMA_FD_FILE, 3, 0, 0};
    assert(prima_fd_resize(3, -1) == -PRIMA_ERRNO_EINVAL);
    assert(prima_fd_at(3, positioned, 1, -1, 0) == -PRIMA_ERRNO_EINVAL);
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
    return 0;
}
