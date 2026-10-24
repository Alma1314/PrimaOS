/* Host-only checks of the actual adapter. NULL devices require no native
 * syscall, so these cases must be safe to execute on a Linux host. */
#include "../c/prima_syscall.c"
#include <assert.h>

int main(void) {
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
